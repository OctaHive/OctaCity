//! Recovery checkpoints observed through ordinary management routes.
use super::*;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

pub(super) async fn checkpoint(client: &Client, origin: &str, run: &str) -> Value {
  timeout(Duration::from_secs(180), async {
    loop {
      let calls = get_json(client, format!("{origin}/api/v1/factory-runs/{run}/calls?limit=100")).await;
      assert!(
        calls["items"].as_array().unwrap().len() <= 1,
        "the accepted predecessor must wait at an explicit recovery checkpoint before any successor invocation"
      );
      let attempts = get_json(client, format!("{origin}/api/v1/factory-runs/{run}/node-attempts?limit=100")).await;
      if let Some(gate) = attempts["items"].as_array().unwrap().iter().find(|row| row["record"]["node_key"] == "resume") {
        let completions = get_json(client, format!("{origin}/api/v1/factory-runs/{run}/node-completions?limit=100")).await;
        assert_eq!(completions["items"].as_array().unwrap().len(), 1);
        assert_eq!(completions["items"][0]["record"]["node_key"], "compose");
        let builds = get_json(client, format!("{origin}/api/v1/factory-runs/{run}/builds?limit=100")).await;
        assert_eq!(builds["items"].as_array().unwrap().len(), 1);
        return json!({"gate":gate["record"],"calls":calls["items"],"builds":builds["items"],"node-completions":completions["items"]});
      }
      let state = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      assert!(!["failed", "cancelled", "completed"].contains(&state["state"].as_str().unwrap()), "Run stopped before the checkpoint: {state}");
      sleep(Duration::from_millis(100)).await;
    }
  }).await.expect("accepted predecessor did not reach the recovery checkpoint")
}

pub(super) async fn edit_documents(client: &Client, origin: &str, project: &str, original: &isolation::Authored) {
  for (name, artifact) in [
    ("Glossary", &original.documents[1]),
    ("Checklist", &original.documents[2]),
    ("Bulletin completion", &original.criteria),
    ("Bulletin request", &original.task),
  ] {
    let replacement = post_management(
      client,
      origin,
      &format!("/api/v1/projects/{project}/input-documents"),
      &format!("edit-{name}"),
      json!({"name":name,"content":"later-document-canary: changed after acceptance"}),
    )
    .await;
    assert_ne!(replacement["artifact"]["artifact_id"], artifact["artifact_id"]);
    let retained = get_json(
      client,
      format!(
        "{origin}/api/v1/projects/{project}/input-documents/{}",
        string(artifact, "artifact_id")
      ),
    )
    .await;
    assert!(!retained["content"].as_str().unwrap().contains("later-document-canary"));
  }
}

pub(super) async fn resume(client: &Client, origin: &str, run: &str, checkpoint: &Value) {
  for collection in ["calls", "builds", "node-completions"] {
    let page = get_json(
      client,
      format!("{origin}/api/v1/factory-runs/{run}/{collection}?limit=100"),
    )
    .await;
    assert_eq!(
      page["items"], checkpoint[collection],
      "restart must retain the accepted predecessor exactly"
    );
  }
  let state = client
    .get(format!("{origin}/api/v1/factory-runs/{run}"))
    .send()
    .await
    .unwrap();
  let etag = state.headers()["etag"].to_str().unwrap().to_owned();
  let path = format!(
    "/api/v1/factory-runs/{run}/nodes/{}/responses",
    string(&checkpoint["gate"], "id")
  );
  replay_after_lost_reply(
    client,
    origin,
    &path,
    "resume-after-restart",
    Some(&etag),
    json!({"outcome":"continue","value":true}),
  )
  .await;
}

/// One transparent TCP hop forwards the actual request and consumes the committed
/// response, then closes without acknowledging it. The normal API owns all effects.
pub(super) async fn replay_after_lost_reply(
  client: &Client,
  origin: &str,
  path: &str,
  key: &str,
  etag: Option<&str>,
  body: Value,
) -> Value {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let proxy = listener.local_addr().unwrap();
  let upstream = origin.strip_prefix("http://").unwrap().to_owned();
  let relay = tokio::spawn(async move {
    let (mut downstream, _) = listener.accept().await.unwrap();
    let mut request = Vec::new();
    loop {
      assert!(request.len() < 2 * 1024 * 1024, "bounded fault-injection request");
      let mut bytes = [0; 8192];
      let read = downstream.read(&mut bytes).await.unwrap();
      assert_ne!(read, 0);
      request.extend_from_slice(&bytes[..read]);
      if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
        let header = std::str::from_utf8(&request[..end]).unwrap();
        let length: usize = header
          .lines()
          .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name
              .eq_ignore_ascii_case("content-length")
              .then(|| value.trim().parse().unwrap())
          })
          .unwrap();
        if request.len() == end + 4 + length {
          break;
        }
      }
    }
    let mut server = tokio::net::TcpStream::connect(upstream).await.unwrap();
    server.write_all(&request).await.unwrap();
    let mut response = Vec::new();
    server.take(2 * 1024 * 1024).read_to_end(&mut response).await.unwrap();
    let status: u16 = std::str::from_utf8(&response[..response.iter().position(|byte| *byte == b'\r').unwrap()])
      .unwrap()
      .split_whitespace()
      .nth(1)
      .unwrap()
      .parse()
      .unwrap();
    assert!(
      (200..300).contains(&status),
      "the real operation must commit before its response is lost: {status}"
    );
    drop(downstream);
  });
  let request = |url: String| {
    let mut request = client
      .post(url)
      .header("idempotency-key", key)
      .json(&body)
      .timeout(Duration::from_secs(15));
    if let Some(etag) = etag {
      request = request.header("if-match", etag);
    }
    request
  };
  let response = request(format!("http://{proxy}{path}"))
    .header("connection", "close")
    .send()
    .await;
  timeout(Duration::from_secs(15), relay)
    .await
    .expect("lost-reply relay must be bounded")
    .unwrap();
  assert!(response.is_err(), "the client must actually lose the committed reply");
  let replay = request(format!("{origin}{path}")).send().await.unwrap();
  assert!(
    replay.status().is_success(),
    "exact replay must recover the committed operation: {}",
    replay.status()
  );
  replay.json().await.unwrap()
}
