//! Public plugin operations over an actual HTTP fixture; HTTPS validation remains production-only.
use super::*;
use octacity_delivery_protocol::{DeliveryIntent, ReviewState, decode_request};
use sha2::{Digest as _, Sha256};
use std::{fs, os::unix::fs::PermissionsExt as _};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

fn fixture() -> (tempfile::TempDir, GithubPlugin, DeliveryIntent) {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  for dir in ["credentials", "inputs"] {
    fs::create_dir(root.path().join(dir)).unwrap();
    fs::set_permissions(root.path().join(dir), fs::Permissions::from_mode(0o700)).unwrap();
  }
  let git = std::path::Path::new("/usr/bin/git");
  let config = format!(
    r#"config_version = 1
api_base = "https://forge.invalid/"
api_version = "2026-03-10"
git_path = {:?}
git_sha256 = {:?}
credential_directory = "credentials"
input_directory = "inputs"
[repositories."repository:accepted-project"]
repository_id = "repository-1"
owner = "owner"
name = "repository"
remote = "https://forge.invalid/owner/repository.git"
"#,
    git.to_str().unwrap(),
    hex::encode(Sha256::digest(fs::read(git).unwrap()))
  );
  write_private(&root.path().join("github-plugin.toml"), config.as_bytes());
  let path = root.path().join("credentials").join(format!(
    "{}.json",
    hex::encode(Sha256::digest("protected-delivery-grant"))
  ));
  write_private(
    &path,
    br#"{"class":"delivery_write","token":"private-fixture-token","repositories":["repository:accepted-project"]}"#,
  );
  let request = decode_request(include_bytes!(
    "../../../protocols/octacity-delivery-protocol/fixtures/publish-v1.json"
  ))
  .unwrap();
  let Command::Publish(mut intent) = request.command else {
    unreachable!()
  };
  let plugin = GithubPlugin::load(root.path()).unwrap();
  intent.configuration_digest = plugin.configuration_digest().to_owned();
  intent.network_hosts = vec!["forge.invalid".into(), "127.0.0.1".into()];
  (root, plugin, intent)
}

#[tokio::test]
async fn changed_provider_configuration_cannot_redirect_an_accepted_delivery() {
  let (root, plugin, intent) = fixture();
  let original_digest = plugin.configuration_digest().to_owned();
  let path = root.path().join("github-plugin.toml");
  let config = fs::read_to_string(&path)
    .unwrap()
    .replace("name = \"repository\"", "name = \"other-repository\"");
  write_private(&path, config.as_bytes());
  let replacement = GithubPlugin::load(root.path()).unwrap();
  assert_ne!(replacement.configuration_digest(), original_digest);
  let request = Request {
    protocol_version: 1,
    request_id: "pinned-provider".into(),
    command: Command::Publish(intent),
  };
  assert!(matches!(
    replacement.execute(&request, CancellationToken::new()).await,
    Outcome::Failure(Failure {
      class: FailureClass::Conflict,
      ..
    })
  ));
}
fn write_private(path: &std::path::Path, bytes: &[u8]) {
  fs::write(path, bytes).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
async fn read_server(replies: Vec<(&'static str, serde_json::Value)>) -> (url::Url, tokio::task::JoinHandle<()>) {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let endpoint = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
  let task = tokio::spawn(async move {
    for (path, value) in replies {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut request = Vec::new();
      while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0; 1];
        socket.read_exact(&mut byte).await.unwrap();
        request.push(byte[0]);
        assert!(request.len() < 16 * 1024);
      }
      let request = String::from_utf8(request).unwrap();
      assert!(
        request.starts_with(&format!("GET {path}")),
        "observation cannot write or select another repository"
      );
      assert!(
        request
          .to_ascii_lowercase()
          .contains("authorization: bearer private-fixture-token")
      );
      let body = serde_json::to_vec(&value).unwrap();
      socket
        .write_all(
          format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      socket.write_all(&body).await.unwrap();
    }
  });
  (endpoint, task)
}

#[tokio::test]
async fn observes_the_existing_exact_candidate_and_stable_review_without_writes() {
  let (_root, mut plugin, intent) = fixture();
  let marker = format!(
    "<!-- octacity-delivery:v1 {} {} {} -->",
    "a".repeat(64),
    "c".repeat(64),
    "b".repeat(64)
  );
  let (endpoint, served) = read_server(vec![
    (
      "/repos/owner/repository/git/ref/heads/main",
      serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":"1".repeat(40)}}),
    ),
    (
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      serde_json::json!({"ref":"refs/heads/review/accepted-candidate","object":{"type":"commit","sha":"2".repeat(40)}}),
    ),
    (
      "/repos/owner/repository/pulls?",
      serde_json::json!([{"number":23,"state":"open","merged_at":null,
      "head":{"sha":"2".repeat(40),"ref":"review/accepted-candidate","repo":{"full_name":"OWNER/Repository"}},
      "base":{"sha":"1".repeat(40),"ref":"main","repo":{"full_name":"OWNER/Repository"}},"title":intent.review.title,"body":format!("{}\n\n{}",intent.review.summary,marker)}]),
    ),
  ])
  .await;
  // Network-boundary fixture injection; installed configuration still rejects cleartext URLs.
  plugin.config.api_base = endpoint;
  let request = Request {
    protocol_version: 1,
    request_id: "observe-existing".into(),
    command: Command::Observe(intent),
  };
  let result = plugin.execute(&request, CancellationToken::new()).await;
  let Outcome::Observed(receipt) = result else {
    panic!("exact existing publication must be observed: {result:?}")
  };
  assert_eq!(receipt.remote_head, "2".repeat(40));
  assert_eq!(receipt.review.as_ref().unwrap().identity, "23");
  assert_eq!(receipt.review.as_ref().unwrap().state, ReviewState::Open);
  assert_eq!(receipt.delivery_id, "a".repeat(64));
  served.await.unwrap();
}

#[tokio::test]
async fn observation_requires_repair_when_owned_review_metadata_has_changed() {
  let (_root, mut plugin, intent) = fixture();
  let (endpoint, served) = read_server(vec![
    (
      "/repos/owner/repository/git/ref/heads/main",
      serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":"1".repeat(40)}}),
    ),
    (
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":"2".repeat(40)}}),
    ),
    (
      "/repos/owner/repository/pulls?",
      serde_json::json!([{"number":23,"state":"open","merged_at":null,"title":"changed externally",
      "head":{"sha":"2".repeat(40),"ref":"review/accepted-candidate","repo":{"full_name":"owner/repository"}},
      "base":{"sha":"1".repeat(40),"ref":"main","repo":{"full_name":"owner/repository"}},"body":api::marker(&intent)}]),
    ),
  ])
  .await;
  plugin.config.api_base = endpoint;
  let result = plugin
    .execute(
      &Request {
        protocol_version: 1,
        request_id: "observe-metadata".into(),
        command: Command::Observe(intent),
      },
      CancellationToken::new(),
    )
    .await;
  let response = serde_json::to_value(result).unwrap();
  assert_eq!(
    response["status"], "repair_required",
    "an owned PR with different metadata is not a completed publication"
  );
  assert_eq!(response["payload"]["review"]["identity"], "23");
  served.await.unwrap();
}

#[tokio::test]
async fn externally_merged_review_remains_observable_after_the_base_advances() {
  let (_root, mut plugin, intent) = fixture();
  let (endpoint, served) = read_server(vec![
    ("/repos/owner/repository/git/ref/heads/main", serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":"3".repeat(40)}})),
    ("/repos/owner/repository/git/ref/heads/review/accepted-candidate", serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":"2".repeat(40)}})),
    ("/repos/owner/repository/pulls?", serde_json::json!([{"number":23,"state":"closed","merged_at":"2026-10-12T12:00:00Z","title":intent.review.title,
      "head":{"sha":"2".repeat(40),"ref":"review/accepted-candidate","repo":{"full_name":"owner/repository"}},
      "base":{"sha":"3".repeat(40),"ref":"main","repo":{"full_name":"owner/repository"}},"body":format!("{}\n\n{}",intent.review.summary,api::marker(&intent))}])),
  ]).await;
  plugin.config.api_base = endpoint;
  let request = Request {
    protocol_version: 1,
    request_id: "observe-merged".into(),
    command: Command::Observe(intent.clone()),
  };
  let result = plugin.execute(&request, CancellationToken::new()).await;
  let Outcome::Observed(receipt) = result else {
    served.abort();
    panic!("an external merge is a fact, not a changed-base publication conflict: {result:?}")
  };
  assert_eq!(receipt.review.as_ref().unwrap().state, ReviewState::MergedExternally);
  assert_eq!(receipt.review.as_ref().unwrap().base, "3".repeat(40));
  assert_eq!(receipt.base_revision, intent.base_revision);
  octacity_delivery_protocol::Response {
    protocol_version: 1,
    request_id: request.request_id.clone(),
    outcome: Outcome::Observed(receipt),
  }
  .validate_for(&request)
  .unwrap();
  served.await.unwrap();
}

fn git(directory: &std::path::Path, args: &[&str]) -> String {
  let output = std::process::Command::new("/usr/bin/git")
    .current_dir(directory)
    .args(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid"])
    .args(args)
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "fixture git failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn captured_candidate(root: &std::path::Path, plugin: &mut GithubPlugin, intent: &mut DeliveryIntent) {
  let remote = root.join("remote.git");
  fs::create_dir(&remote).unwrap();
  git(&remote, &["init", "--bare", "--initial-branch=main"]);
  let source = root.join("source");
  fs::create_dir(&source).unwrap();
  git(&source, &["init", "--initial-branch=main"]);
  fs::write(source.join("README.md"), "base\n").unwrap();
  git(&source, &["add", "."]);
  git(&source, &["commit", "-m", "base"]);
  intent.base_revision = git(&source, &["rev-parse", "HEAD"]);
  git(&source, &["push", remote.to_str().unwrap(), "HEAD:refs/heads/main"]);
  fs::write(source.join("README.md"), "accepted candidate\n").unwrap();
  git(&source, &["add", "."]);
  git(&source, &["commit", "-m", "accepted candidate"]);
  intent.candidate_revision = git(&source, &["rev-parse", "HEAD"]);
  let bundle_path = root.join("candidate.bundle");
  git(
    &source,
    &[
      "bundle",
      "create",
      bundle_path.to_str().unwrap(),
      "HEAD",
      &format!("^{}", intent.base_revision),
    ],
  );
  let bundle = fs::read(bundle_path).unwrap();
  intent.bundle.sha256 = hex::encode(Sha256::digest(&bundle));
  intent.bundle.size_bytes = bundle.len() as u64;
  let manifest: octacity_protocol::CapturedChangeSetManifestV1 = serde_json::from_value(serde_json::json!({
    "format_version":1,"base_revision":intent.base_revision,"candidate_revision":intent.candidate_revision,
    "node_attempt_id":"accepted-attempt","capture_tool":{"identity":"capture","version":"1","sha256":"f".repeat(64)},
    "changed_paths":[{"status":"M","path":"README.md","old_mode":"100644","new_mode":"100644","content_sha256":hex::encode(Sha256::digest(b"accepted candidate\n"))}],
    "cumulative_changes":{"base_revision":intent.base_revision,"changed_paths":[{"status":"M","path":"README.md","old_mode":"100644","new_mode":"100644","content_sha256":hex::encode(Sha256::digest(b"accepted candidate\n"))}]},
    "bundle":{"name":"change-set.bundle","size_bytes":bundle.len(),"sha256":intent.bundle.sha256}
  })).unwrap();
  let mut bytes = serde_json::to_vec(&manifest).unwrap();
  bytes.push(b'\n');
  intent.manifest.sha256 = hex::encode(Sha256::digest(&bytes));
  intent.changeset_digest = intent.manifest.sha256.clone();
  intent.manifest.size_bytes = bytes.len() as u64;
  for (handle, bytes) in [(&intent.bundle.handle, bundle), (&intent.manifest.handle, bytes)] {
    write_private(
      &plugin
        .config
        .input_directory
        .join(format!("{}.bin", hex::encode(Sha256::digest(handle)))),
      &bytes,
    );
  }
  // A local forge boundary exercises real Git without permitting file remotes in installed configuration.
  plugin
    .config
    .repositories
    .get_mut(&intent.repository.repository_locator)
    .unwrap()
    .remote = url::Url::from_file_path(&remote).unwrap().to_string();
}

async fn forge_server(
  replies: Vec<(&'static str, &'static str, u16, serde_json::Value)>,
) -> (url::Url, tokio::task::JoinHandle<()>) {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let endpoint = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
  let task = tokio::spawn(async move {
    for (method, path, status, value) in replies {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut bytes = Vec::new();
      while !bytes.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        socket.read_exact(&mut b).await.unwrap();
        bytes.push(b[0]);
        assert!(bytes.len() < 16 * 1024);
      }
      let headers = String::from_utf8(bytes).unwrap();
      assert!(
        headers.starts_with(&format!("{method} {path}")),
        "unexpected forge operation: {headers}"
      );
      assert!(
        headers
          .to_ascii_lowercase()
          .contains("authorization: bearer private-fixture-token")
      );
      let size: usize = headers
        .lines()
        .find_map(|line| {
          line
            .to_ascii_lowercase()
            .strip_prefix("content-length: ")
            .and_then(|size| size.parse().ok())
        })
        .unwrap_or(0);
      let mut body = vec![0; size];
      socket.read_exact(&mut body).await.unwrap();
      if method == "POST" {
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["head"], "review/accepted-candidate");
        assert_eq!(json["base"], "main");
        assert_eq!(json["title"], "Accepted candidate");
        assert!(json.get("merge").is_none());
      }
      let body = serde_json::to_vec(&value).unwrap();
      socket.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
      socket.write_all(&body).await.unwrap();
    }
  });
  (endpoint, task)
}

#[tokio::test]
async fn publishes_only_the_captured_candidate_and_checkpoints_the_observed_review() {
  captured_publication(false).await;
}
#[tokio::test]
async fn a_lost_git_push_reply_is_transient_and_replay_repairs_the_same_branch() {
  captured_publication(true).await;
}
async fn captured_publication(lose_push_response: bool) {
  let (root, mut plugin, mut intent) = fixture();
  captured_candidate(root.path(), &mut plugin, &mut intent);
  // Trusted executable boundary: refuse to see candidate bytes in a publicly
  // traversable quarantine. Installed Git is still pinned by its exact digest.
  let guarded = root.path().join("guarded-git");
  let script=b"#!/bin/sh\ncase \"$(/usr/bin/uname -s)\" in Darwin) mode=$(/usr/bin/stat -f '%Lp' .);; *) mode=$(/usr/bin/stat -c '%a' .);; esac\n[ \"$mode\" = 700 ] || exit 74\nexec /usr/bin/git \"$@\"\n";
  let script = if lose_push_response {
    b"#!/bin/sh\nfor arg in \"$@\"; do if [ \"$arg\" = push ]; then /usr/bin/git \"$@\" || exit 128; exit 128; fi; done\nexec /usr/bin/git \"$@\"\n".as_slice()
  } else {
    script.as_slice()
  };
  fs::write(&guarded, script).unwrap();
  fs::set_permissions(&guarded, fs::Permissions::from_mode(0o700)).unwrap();
  plugin.config.git_path = guarded;
  plugin.config.git_sha256 = hex::encode(Sha256::digest(script));
  let base = serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":intent.base_revision}});
  let candidate = serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":intent.candidate_revision}});
  let review = serde_json::json!({"number":23,"state":"open","merged_at":null,
    "head":{"sha":intent.candidate_revision,"ref":"review/accepted-candidate","repo":{"full_name":"owner/repository"}},
    "base":{"sha":intent.base_revision,"ref":"main","repo":{"full_name":"owner/repository"}},
    "title":intent.review.title,"body":format!("{}\n\n{}",intent.review.summary,api::marker(&intent))});
  let mut replies = vec![
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base.clone()),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      404,
      serde_json::json!({}),
    ),
  ];
  if lose_push_response {
    replies.extend([
      ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base.clone()),
      (
        "GET",
        "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
        200,
        candidate.clone(),
      ),
      ("GET", "/repos/owner/repository/pulls?", 200, serde_json::json!([])),
    ]);
  }
  replies.extend([
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base.clone()),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate.clone(),
    ),
    ("GET", "/repos/owner/repository/pulls?", 200, serde_json::json!([])),
    ("POST", "/repos/owner/repository/pulls", 201, review.clone()),
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate,
    ),
    (
      "GET",
      "/repos/owner/repository/pulls?",
      200,
      serde_json::json!([review]),
    ),
  ]);
  let (endpoint, served) = forge_server(replies).await;
  plugin.config.api_base = endpoint;
  let request = Request {
    protocol_version: 1,
    request_id: "publish-accepted".into(),
    command: Command::Publish(intent.clone()),
  };
  if lose_push_response {
    let ambiguous = plugin.execute(&request, CancellationToken::new()).await;
    assert!(
      matches!(
        ambiguous,
        Outcome::Failure(Failure {
          class: FailureClass::Transient,
          ..
        })
      ),
      "a lost Git acknowledgment must stay recoverable: {ambiguous:?}"
    );
    assert_eq!(
      git(
        &root.path().join("remote.git"),
        &["rev-parse", "refs/heads/review/accepted-candidate"]
      ),
      intent.candidate_revision
    );
  }
  let result = plugin.execute(&request, CancellationToken::new()).await;
  let Outcome::Observed(receipt) = result else {
    panic!("accepted candidate must be published: {result:?}")
  };
  assert_eq!(receipt.remote_head, intent.candidate_revision);
  assert_eq!(receipt.review.unwrap().identity, "23");
  assert_eq!(
    git(
      &root.path().join("remote.git"),
      &["rev-parse", "refs/heads/review/accepted-candidate"]
    ),
    intent.candidate_revision
  );
  served.await.unwrap();
}

#[tokio::test]
async fn repairs_review_metadata_under_the_same_observed_identity_without_another_pr() {
  let (root, mut plugin, mut intent) = fixture();
  captured_candidate(root.path(), &mut plugin, &mut intent);
  git(
    &root.path().join("source"),
    &[
      "push",
      root.path().join("remote.git").to_str().unwrap(),
      "HEAD:refs/heads/review/accepted-candidate",
    ],
  );
  let base = serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":intent.base_revision}});
  let candidate = serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":intent.candidate_revision}});
  let old = serde_json::json!({"number":23,"state":"open","merged_at":null,"title":"edited outside the factory",
    "head":{"sha":intent.candidate_revision,"ref":"review/accepted-candidate","repo":{"full_name":"owner/repository"}},
    "base":{"sha":intent.base_revision,"ref":"main","repo":{"full_name":"owner/repository"}},"body":api::marker(&intent)});
  let mut repaired = old.clone();
  repaired["title"] = serde_json::json!(intent.review.title);
  repaired["body"] = serde_json::json!(format!("{}\n\n{}", intent.review.summary, api::marker(&intent)));
  let (endpoint, served) = forge_server(vec![
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base.clone()),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate.clone(),
    ),
    ("GET", "/repos/owner/repository/pulls?", 200, serde_json::json!([old])),
    ("PATCH", "/repos/owner/repository/pulls/23", 200, repaired.clone()),
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate,
    ),
    (
      "GET",
      "/repos/owner/repository/pulls?",
      200,
      serde_json::json!([repaired]),
    ),
  ])
  .await;
  plugin.config.api_base = endpoint;
  let result = plugin
    .execute(
      &Request {
        protocol_version: 1,
        request_id: "repair-metadata".into(),
        command: Command::Publish(intent),
      },
      CancellationToken::new(),
    )
    .await;
  assert!(matches!(result,Outcome::Observed(receipt) if receipt.review.as_ref().unwrap().identity=="23"));
  tokio::time::timeout(std::time::Duration::from_secs(2), served)
    .await
    .expect("publication must repair the same review before accepting it")
    .unwrap();
}

#[tokio::test]
async fn replay_observes_the_original_review_after_an_unknown_write_response() {
  let (root, mut plugin, mut intent) = fixture();
  captured_candidate(root.path(), &mut plugin, &mut intent);
  git(
    &root.path().join("source"),
    &[
      "push",
      root.path().join("remote.git").to_str().unwrap(),
      "HEAD:refs/heads/review/accepted-candidate",
    ],
  );
  let base = serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":intent.base_revision}});
  let candidate = serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":intent.candidate_revision}});
  let review = serde_json::json!({"number":23,"state":"open","merged_at":null,"title":intent.review.title,
    "head":{"sha":intent.candidate_revision,"ref":"review/accepted-candidate","repo":{"full_name":"owner/repository"}},
    "base":{"sha":intent.base_revision,"ref":"main","repo":{"full_name":"owner/repository"}},"body":format!("{}\n\n{}",intent.review.summary,api::marker(&intent))});
  let (endpoint, served) = forge_server(vec![
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base.clone()),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate.clone(),
    ),
    ("GET", "/repos/owner/repository/pulls?", 200, serde_json::json!([])),
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base.clone()),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate.clone(),
    ),
    ("GET", "/repos/owner/repository/pulls?", 200, serde_json::json!([])),
    // A gateway lost the response after accepting the write. Its next reads
    // expose the original PR; retrying POST here would fail this fixture.
    (
      "POST",
      "/repos/owner/repository/pulls",
      503,
      serde_json::json!({"message":"private upstream diagnostic"}),
    ),
    ("GET", "/repos/owner/repository/git/ref/heads/main", 200, base),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      candidate,
    ),
    (
      "GET",
      "/repos/owner/repository/pulls?",
      200,
      serde_json::json!([review]),
    ),
  ])
  .await;
  plugin.config.api_base = endpoint;
  let request = Request {
    protocol_version: 1,
    request_id: "unknown-response".into(),
    command: Command::Publish(intent),
  };
  let result = plugin.execute(&request, CancellationToken::new()).await;
  assert!(matches!(
    result,
    Outcome::Failure(Failure {
      class: FailureClass::Transient,
      ..
    })
  ));
  assert!(!format!("{result:?}").contains("private upstream"));
  let replay = plugin.execute(&request, CancellationToken::new()).await;
  assert!(matches!(replay,Outcome::Observed(receipt) if receipt.review.as_ref().unwrap().identity=="23"));
  tokio::time::timeout(std::time::Duration::from_secs(2), served)
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn altered_capture_bytes_and_a_conflicting_remote_head_cannot_be_published() {
  let (root, mut plugin, mut intent) = fixture();
  captured_candidate(root.path(), &mut plugin, &mut intent);
  let input = plugin
    .config
    .input_directory
    .join(format!("{}.bin", hex::encode(Sha256::digest(&intent.bundle.handle))));
  let original = fs::read(&input).unwrap();
  write_private(&input, b"substituted bundle");
  let request = Request {
    protocol_version: 1,
    request_id: "altered-capture".into(),
    command: Command::Publish(intent.clone()),
  };
  assert!(matches!(
    plugin.execute(&request, CancellationToken::new()).await,
    Outcome::Failure(Failure {
      class: FailureClass::Conflict,
      ..
    })
  ));
  write_private(&input, &original);
  let (endpoint, served) = forge_server(vec![
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/main",
      200,
      serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":intent.base_revision}}),
    ),
    (
      "GET",
      "/repos/owner/repository/git/ref/heads/review/accepted-candidate",
      200,
      serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":"3".repeat(40)}}),
    ),
  ])
  .await;
  plugin.config.api_base = endpoint;
  assert!(matches!(
    plugin.execute(&request, CancellationToken::new()).await,
    Outcome::Failure(Failure {
      class: FailureClass::Conflict,
      ..
    })
  ));
  assert!(
    !std::process::Command::new("/usr/bin/git")
      .current_dir(root.path().join("remote.git"))
      .args(["show-ref", "--verify", "refs/heads/review/accepted-candidate"])
      .output()
      .unwrap()
      .status
      .success()
  );
  served.await.unwrap();
}

#[tokio::test]
async fn git_transport_cannot_follow_a_redirect_outside_the_selected_remote() {
  let (root, mut plugin, mut intent) = fixture();
  captured_candidate(root.path(), &mut plugin, &mut intent);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let remote = format!("http://{}/repository.git", listener.local_addr().unwrap());
  plugin
    .config
    .repositories
    .get_mut(&intent.repository.repository_locator)
    .unwrap()
    .remote = remote.clone();
  let served = tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut bytes = vec![];
    while !bytes.ends_with(b"\r\n\r\n") {
      let mut b = [0];
      socket.read_exact(&mut b).await.unwrap();
      bytes.push(b[0]);
      assert!(bytes.len() < 16 * 1024);
    }
    socket
      .write_all(
        format!(
          "HTTP/1.1 302 Found\r\nLocation: {remote}/unexpected-route\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .as_bytes(),
      )
      .await
      .unwrap();
    drop(socket);
    match tokio::time::timeout(std::time::Duration::from_millis(500), listener.accept()).await {
      Err(_) => false,
      Ok(Ok((mut socket, _))) => {
        let _ = socket
          .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
          .await;
        true
      }
      Ok(Err(error)) => panic!("fixture listener failed: {error}"),
    }
  });
  let result = plugin
    .execute(
      &Request {
        protocol_version: 1,
        request_id: "remote-redirect".into(),
        command: Command::Publish(intent),
      },
      CancellationToken::new(),
    )
    .await;
  assert!(matches!(result, Outcome::Failure(_)));
  assert!(
    !served.await.unwrap(),
    "a redirect cannot receive the selected remote's credential or repository query"
  );
}

#[tokio::test]
async fn actual_provider_origins_must_fit_the_frozen_network_ceiling_before_activity() {
  for missing_api in [true, false] {
    let (_root, mut plugin, mut intent) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    plugin.config.api_base = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    intent.network_hosts = vec![if missing_api { "forge.invalid" } else { "127.0.0.1" }.into()];
    let result = plugin
      .execute(
        &Request {
          protocol_version: 1,
          request_id: "denied-origin".into(),
          command: Command::Observe(intent),
        },
        CancellationToken::new(),
      )
      .await;
    assert!(
      matches!(
        result,
        Outcome::Failure(Failure {
          class: FailureClass::InvalidRequest,
          ..
        })
      ),
      "every actual API/Git host must be authorized before credential or network activity: {result:?}"
    );
    assert!(
      tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
        .await
        .is_err()
    );
  }
}
