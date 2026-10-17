#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/issues.rs"]
mod fixture;
use fixture::*;
use octacity_work_source_protocol::{Request, decode_request};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn check(installation: &Installation, endpoint: &str) -> Request {
  let scan = serde_json::to_value(installation.request()).unwrap();
  decode_request(
    &serde_json::to_vec(&json!({"family":"source","message":{"protocol_version":1,
    "request_id":"check-request","command":{"operation":"check","payload":{"check_id":"a".repeat(64),
      "configuration_digest":installation.provider().configuration_digest(),"endpoint":endpoint,
      "resource":scan["message"]["command"]["payload"]["resource"],"network_hosts":["localhost"]}}}}))
    .unwrap(),
  )
  .unwrap()
}
fn report(response: octacity_work_source_protocol::Response) -> Value {
  let value = serde_json::to_value(response).unwrap();
  assert_eq!(value["message"]["outcome"]["status"], "checked");
  value["message"]["outcome"]["payload"].clone()
}

fn writer_check(installation: &Installation, endpoint: &str) -> Request {
  let config = installation.root.path().join("github-work.toml");
  let text = std::fs::read_to_string(&config).unwrap().replace(
    "[capabilities]",
    "[capabilities]\nreporter_repositories = [\"installed-ticket-space\"]",
  );
  private_file(&config, text.as_bytes());
  private_file(
    &installation
      .root
      .path()
      .join("credentials")
      .join(hashed("private-writer")),
    br#"{"class":"work_reporter_write","token":"private-write-token","repositories":["installed-ticket-space"]}"#,
  );
  let mut wire = serde_json::to_value(check(installation, endpoint)).unwrap();
  wire["family"] = json!("reporter");
  wire["message"]["command"]["payload"]["resource"]["credential"] =
    json!({"class":"work_reporter_write","handle":"private-writer"});
  decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap()
}

#[tokio::test]
async fn connection_check_confirms_authenticated_native_issue_read_without_retaining_tasks_or_mutating() {
  let forge = Forge::start().await;
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([issue(9001, 17)])));
  let installation = Installation::new(&forge, selector());
  let request = check(&installation, &forge.endpoint);
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  let result = report(response);
  assert_eq!(result["configuration"], "ready");
  assert_eq!(result["authentication"], "ready");
  assert_eq!(result["resource_access"], "ready");
  assert_eq!(result["resource_id"], "1234");
  assert_eq!(result["capability"], "ready");
  let text = serde_json::to_string(&result).unwrap();
  for private in ["private-fixture-token", "Private task", "9001", "cursor", "candidate"] {
    assert!(!text.contains(private));
  }
  let requests = forge.requests.lock().await;
  assert_eq!(requests.len(), 2);
  assert!(requests[0].starts_with("GET /api/v3/repos/owner/tickets HTTP/"));
  assert!(requests[1].starts_with("GET /api/v3/repos/owner/tickets/issues?"));
  assert!(requests[1].contains("per_page=1"));
  assert!(requests.iter().all(|r| r.starts_with("GET ")));
}

#[tokio::test]
async fn write_check_uses_only_its_own_grant_and_never_claims_mutation_readiness_from_metadata() {
  let forge = Forge::start().await;
  *forge.repository.lock().await = json!({"id":1234,"permissions":{"push":true,"admin":true}});
  let installation = Installation::new(&forge, selector());
  let request = writer_check(&installation, &forge.endpoint);
  let result = report(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(result["authentication"], "ready");
  assert_eq!(result["resource_access"], "ready");
  assert_eq!(result["capability"], "unverified");
  let requests = forge.requests.lock().await;
  assert_eq!(
    requests.len(),
    1,
    "no test Issue/label/state/branch/review mutation or alternate read handle"
  );
  assert!(requests[0].starts_with("GET /api/v3/repos/owner/tickets HTTP/"));
  assert!(
    requests[0]
      .to_ascii_lowercase()
      .contains("authorization: bearer private-write-token")
  );
}

#[tokio::test]
async fn checks_reject_endpoint_network_and_installed_scope_substitution_before_provider_access() {
  for (field, value) in [
    ("endpoint", json!("https://unapproved.invalid/api/")),
    ("network_hosts", json!(["unapproved.invalid"])),
    ("configuration_digest", json!("f".repeat(64))),
  ] {
    let forge = Forge::start().await;
    let installation = Installation::new(&forge, selector());
    let mut wire = serde_json::to_value(check(&installation, &forge.endpoint)).unwrap();
    wire["message"]["command"]["payload"][field] = value;
    let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
    let result = report(
      installation
        .provider()
        .execute(&request, CancellationToken::new())
        .await,
    );
    assert_ne!(result["configuration"], "ready", "{field}");
    assert_eq!(result["authentication"], "unverified");
    assert!(
      forge.requests.lock().await.is_empty(),
      "{field} must fail before authenticated HTTP"
    );
  }
}

#[tokio::test]
async fn missing_role_credential_is_distinct_from_configuration_and_never_falls_back_to_an_existing_handle() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let mut wire = serde_json::to_value(check(&installation, &forge.endpoint)).unwrap();
  wire["message"]["command"]["payload"]["resource"]["credential"]["handle"] = json!("missing-private-handle");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let result = report(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(result["configuration"], "ready");
  assert_eq!(result["authentication"], "missing");
  assert_eq!(result["resource_access"], "unverified");
  assert_eq!(result["capability"], "unverified");
  assert!(forge.requests.lock().await.is_empty());
  // Endpoint validation must still precede resolving that absent credential.
  wire["message"]["command"]["payload"]["endpoint"] = json!("https://unapproved.invalid/");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let result = report(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(result["configuration"], "invalid");
  assert_eq!(result["authentication"], "unverified");
}

#[tokio::test]
async fn checks_distinguish_authentication_resource_denial_throttling_and_outage_without_private_diagnostics() {
  for (status, headers, aspect, expected) in [
    (401, vec![], "authentication", "denied"),
    (404, vec![], "resource_access", "denied"),
    (403, vec![], "resource_access", "denied"),
    (
      429,
      vec![("retry-after".into(), "3".into())],
      "resource_access",
      "throttled",
    ),
    (503, vec![], "resource_access", "unavailable"),
  ] {
    let forge = Forge::start().await;
    *forge.repository_reply.lock().await = Some(Reply {
      status,
      headers,
      ..Reply::json(json!({"message":"private-provider-error private-fixture-token"}))
    });
    let installation = Installation::new(&forge, selector());
    let request = check(&installation, &forge.endpoint);
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    response.validate_for(&request).unwrap();
    let result = report(response);
    assert_eq!(result["configuration"], "ready");
    assert_eq!(result[aspect], expected, "HTTP {status}");
    assert_eq!(result["capability"], "unverified");
    if status == 429 {
      assert_eq!(result["retry_after_millis"], 3000);
    }
    for private in ["private-provider-error", "private-fixture-token"] {
      assert!(!result.to_string().contains(private));
    }
    assert_eq!(
      forge.requests.lock().await.len(),
      1,
      "no automatic retries or follow-up reads after refusal"
    );
  }
}

#[tokio::test]
async fn unsupported_write_capability_is_not_a_missing_read_credential_or_a_reason_to_try_another_role() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let mut wire = serde_json::to_value(check(&installation, &forge.endpoint)).unwrap();
  wire["family"] = json!("reporter");
  wire["message"]["command"]["payload"]["resource"]["credential"]["class"] = json!("work_reporter_write");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let result = report(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(result["configuration"], "ready");
  assert_eq!(result["capability"], "unsupported");
  assert_eq!(result["authentication"], "unverified");
  assert!(forge.requests.lock().await.is_empty());
}

#[tokio::test]
async fn cancelled_checks_do_not_read_credentials_or_continue_an_in_flight_provider_request() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let request = check(&installation, &forge.endpoint);
  let token = CancellationToken::new();
  token.cancel();
  let result = report(installation.provider().execute(&request, token).await);
  assert_eq!(result["configuration"], "unverified");
  assert_eq!(result["capability"], "unavailable");
  assert!(forge.requests.lock().await.is_empty());
  *forge.repository_reply.lock().await = Some(Reply {
    stall: true,
    ..Reply::json(json!({"id":1234}))
  });
  let token = CancellationToken::new();
  let provider = installation.provider();
  let call = provider.execute(&request, token.clone());
  let stop = async {
    forge.wait_for_requests(1).await;
    token.cancel();
  };
  let (response, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async { tokio::join!(call, stop) })
    .await
    .unwrap();
  assert_eq!(report(response)["capability"], "unavailable");
  assert_eq!(forge.requests.lock().await.len(), 1);
}

#[tokio::test]
async fn provider_process_cancels_the_original_read_only_check_for_both_roles() {
  use std::process::Stdio;
  use tokio::io::AsyncWriteExt as _;
  for family in ["source", "reporter"] {
    let forge = Forge::start().await;
    let installation = Installation::new(&forge, selector());
    let request = if family == "source" {
      check(&installation, &forge.endpoint)
    } else {
      writer_check(&installation, &forge.endpoint)
    };
    *forge.repository_reply.lock().await = Some(Reply {
      stall: true,
      ..Reply::json(json!({"id":1234}))
    });
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
      .current_dir(installation.root.path())
      .env_clear()
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.unwrap();
    forge.wait_for_requests(1).await;
    let target = if family == "source" {
      "target_scan_id"
    } else {
      "target_projection_id"
    };
    let mut bytes = serde_json::to_vec(
      &json!({"family":family,"message":{"protocol_version":1,"request_id":"cancel-check",
      "command":{"operation":"cancel","payload":{target:"a".repeat(64)}}}}),
    )
    .unwrap();
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.unwrap();
    drop(stdin);
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait_with_output())
      .await
      .unwrap()
      .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let response = octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap();
    assert_eq!(report(response)["capability"], "unavailable");
    assert_eq!(forge.requests.lock().await.len(), 1);
  }
}

#[tokio::test]
async fn replaced_native_repository_stops_checks_before_reading_any_tasks_or_claiming_write_readiness() {
  for family in ["source", "reporter"] {
    let forge = Forge::start().await;
    *forge.repository.lock().await = json!({"id":5678});
    let installation = Installation::new(&forge, selector());
    let request = if family == "source" {
      check(&installation, &forge.endpoint)
    } else {
      writer_check(&installation, &forge.endpoint)
    };
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    response.validate_for(&request).unwrap();
    let result = report(response);
    assert_eq!(result["authentication"], "ready");
    assert_eq!(
      result["resource_id"], "1234",
      "the original native identity is retained"
    );
    assert_eq!(result["resource_access"], "conflict");
    assert_eq!(result["capability"], "unverified");
    let requests = forge.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /api/v3/repos/owner/tickets HTTP/"));
  }
}

#[tokio::test]
async fn native_provider_process_checks_over_https_for_each_separate_role() {
  use std::{process::Stdio, time::Duration};
  use tokio::io::AsyncWriteExt as _;
  for family in ["source", "reporter"] {
    let forge = Forge::start().await;
    forge.replies.lock().await.push_back(Reply::json(json!([])));
    let installation = Installation::new(&forge, selector());
    let request = if family == "source" {
      check(&installation, &forge.endpoint)
    } else {
      writer_check(&installation, &forge.endpoint)
    };
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
      .current_dir(installation.root.path())
      .env_clear()
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true)
      .spawn()
      .unwrap();
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    child.stdin.take().unwrap().write_all(&bytes).await.unwrap();
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
      .await
      .unwrap()
      .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let result = report(octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap());
    assert_eq!(result["configuration"], "ready");
    assert_eq!(result["authentication"], "ready");
    assert_eq!(result["resource_access"], "ready");
    assert_eq!(
      result["capability"],
      if family == "source" { "ready" } else { "unverified" }
    );
    let requests = forge.requests.lock().await;
    assert_eq!(requests.len(), if family == "source" { 2 } else { 1 });
    assert!(requests.iter().all(|r| r.starts_with("GET ")));
    let token = if family == "source" {
      "private-fixture-token"
    } else {
      "private-write-token"
    };
    assert!(requests.iter().all(|r| {
      r.to_ascii_lowercase()
        .contains(&format!("authorization: bearer {token}"))
    }));
  }
}
