#![cfg(unix)]
#[path = "support/issues.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::*;
use octacity_work_source_protocol::{Request, Response, reporter};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

fn failure(response: Response, expected: octacity_work_source_protocol::FailureClass) {
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Failure(failure),
    ..
  }) = response
  else {
    panic!("expected a closed reporter failure, got {response:?}")
  };
  assert_eq!(failure.class, expected);
}
fn intent(request: &mut Request) -> &mut reporter::ProjectionIntent {
  let Request::Reporter(reporter::Request {
    command: reporter::Command::Project(intent) | reporter::Command::Observe(intent),
    ..
  }) = request
  else {
    unreachable!()
  };
  intent
}

fn mapping() -> Value {
  json!({"config_version":1,"managed_label_groups":{"editorial":["ready / review","needs context"]},
    "statuses":{"needs-human-eyes":{"add":["ready / review"],"remove":["needs context"]}}})
}
fn install(forge: &Forge, mapping: Value) -> (Installation, Request) {
  use std::os::unix::fs::PermissionsExt as _;
  let installation = Installation::new(forge, selector());
  let root = installation.root.path();
  let mut profile: toml::Value =
    toml::from_str(&std::fs::read_to_string(root.join("github-work.toml")).unwrap()).unwrap();
  profile
    .as_table_mut()
    .unwrap()
    .insert("mapping_directory".into(), toml::Value::String("mappings".into()));
  profile["capabilities"].as_table_mut().unwrap().insert(
    "reporter_repositories".into(),
    toml::Value::Array(vec!["installed-ticket-space".into()]),
  );
  private_file(
    &root.join("github-work.toml"),
    toml::to_string(&profile).unwrap().as_bytes(),
  );
  std::fs::create_dir(root.join("mappings")).unwrap();
  std::fs::set_permissions(root.join("mappings"), std::fs::Permissions::from_mode(0o700)).unwrap();
  let bytes = serde_json::to_vec(&mapping).unwrap();
  private_file(
    &root.join("mappings").join(hashed("arbitrary-installed-effects")),
    &bytes,
  );
  private_file(
    &root.join("credentials").join(hashed("private-reporter-grant")),
    br#"{"class":"work_reporter_write","token":"private-reporter-token","repositories":["installed-ticket-space"]}"#,
  );
  let mut wire: Value = serde_json::from_slice(include_bytes!(
    "../../../protocols/octacity-work-source-protocol/fixtures/project-v1.json"
  ))
  .unwrap();
  let intent = &mut wire["message"]["command"]["payload"];
  intent["configuration_digest"] = json!(hex::encode(Sha256::digest(
    std::fs::read(root.join("github-work.toml")).unwrap()
  )));
  intent["mapping"]["sha256"] = json!(hex::encode(Sha256::digest(bytes)));
  intent["resource"]["resource_id"] = json!("1234");
  intent["item_id"] = json!("9001");
  intent["display_id"] = json!("17");
  intent["network_hosts"] = json!(["localhost", "127.0.0.1"]);
  (
    installation,
    octacity_work_source_protocol::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap(),
  )
}
fn observe(request: Request) -> Request {
  let Request::Reporter(mut request) = request else {
    unreachable!()
  };
  let reporter::Command::Project(intent) = request.command else {
    unreachable!()
  };
  request.command = reporter::Command::Observe(intent);
  Request::Reporter(request)
}

#[path = "issue_reporting/recovery.rs"]
mod recovery;

#[tokio::test]
async fn controlled_mutation_barrier_pauses_after_apply_before_acknowledgement() {
  let forge = Forge::start().await;
  *forge.reporting.lock().await = Some(reporting::Issue::new(&["human-owned"]));
  let paused = forge.pause_next_mutation().await;
  let (installation, request) = install(&forge, mapping());
  let operation = tokio::spawn(async move {
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    response.validate_for(&request).unwrap();
    response
  });
  paused.wait().await;
  assert!(
    forge
      .reporting
      .lock()
      .await
      .as_ref()
      .unwrap()
      .labels
      .contains("ready / review")
  );
  assert!(
    !operation.is_finished(),
    "the actual provider response remains unacknowledged"
  );
  paused.resume();
  let response = operation.await.unwrap();
  assert!(matches!(
    response,
    Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Applied(_),
      ..
    })
  ));
  assert_eq!(
    forge
      .requests
      .lock()
      .await
      .iter()
      .filter(|request| !request.starts_with("GET "))
      .count(),
    1
  );
}

#[tokio::test]
async fn observation_checks_pinned_arbitrary_effects_without_writes() {
  for (labels, matches) in [
    (json!([{"name":"ready / review"},{"name":"human-owned"}]), true),
    (json!([{"name":"ready / review"},{"name":"needs context"}]), false),
  ] {
    let forge = Forge::start().await;
    forge.replies.lock().await.extend([
      Reply::json(json!({"name":"ready / review"})),
      Reply::json(json!({"name":"needs context"})),
      Reply::json(issue(9001, 17)),
      Reply::json(labels),
    ]);
    let (installation, request) = install(&forge, mapping());
    let request = observe(request);
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    response.validate_for(&request).unwrap();
    let Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Observed(receipt),
      ..
    }) = response
    else {
      panic!("expected observed configured effects, got {response:?}")
    };
    assert_eq!(receipt.matches, matches);
    let requests = forge.requests.lock().await;
    assert_eq!(requests.len(), 5);
    assert!(requests.iter().all(|request| request.starts_with("GET ")));
  }
}

#[tokio::test]
async fn projection_changes_only_explicit_labels_and_optional_state_then_observes_the_result() {
  for state in [None, Some("open"), Some("closed")] {
    let forge = Forge::start().await;
    let original_state = if state == Some("open") { "closed" } else { "open" };
    let mut original = issue(9001, 17);
    original["state"] = json!(original_state);
    let mut final_issue = original.clone();
    final_issue["state"] = json!(state.unwrap_or(original_state));
    let final_labels = json!([{"name":"ready / review"},{"name":"human-owned"}]);
    let mut replies = vec![
      Reply::json(json!({"name":"ready / review"})),
      Reply::json(json!({"name":"needs context"})),
      Reply::json(original),
      Reply::json(json!([{"name":"needs context"},{"name":"human-owned"}])),
      Reply::json(json!([])),
      Reply::json(final_labels.clone()),
    ];
    if state.is_some() {
      replies.push(Reply::json(final_issue.clone()));
    }
    replies.extend([Reply::json(final_issue), Reply::json(final_labels)]);
    forge.replies.lock().await.extend(replies);
    let mut mapping = mapping();
    if let Some(state) = state {
      mapping["statuses"]["needs-human-eyes"]["state"] = json!(state);
    }
    let (installation, request) = install(&forge, mapping);
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    response.validate_for(&request).unwrap();
    assert!(
      matches!(
        response,
        Response::Reporter(reporter::Response {
          outcome: reporter::Outcome::Applied(_),
          ..
        })
      ),
      "{response:?}"
    );
    let requests = forge.requests.lock().await;
    let writes: Vec<_> = requests.iter().filter(|r| !r.starts_with("GET ")).collect();
    assert_eq!(writes.len(), if state.is_some() { 3 } else { 2 });
    assert!(writes[0].starts_with("DELETE /api/v3/repos/owner/tickets/issues/17/labels/needs%20context HTTP/"));
    assert!(writes[1].starts_with("POST /api/v3/repos/owner/tickets/issues/17/labels HTTP/"));
    assert_eq!(
      serde_json::from_str::<Value>(writes[1].split_once("\r\n\r\n").unwrap().1).unwrap(),
      json!({"labels":["ready / review"]})
    );
    if let Some(state) = state {
      assert!(writes[2].starts_with("PATCH /api/v3/repos/owner/tickets/issues/17 HTTP/"));
      assert_eq!(
        serde_json::from_str::<Value>(writes[2].split_once("\r\n\r\n").unwrap().1).unwrap(),
        json!({"state":state})
      );
    }
  }
}

#[tokio::test]
async fn unconfigured_values_mapping_drift_and_wrong_grants_fail_before_any_provider_call() {
  use octacity_work_source_protocol::FailureClass;
  for case in ["status", "digest", "malformed", "grant", "cancelled"] {
    let forge = Forge::start().await;
    let (installation, mut request) = install(&forge, mapping());
    let mut expected = FailureClass::Permanent;
    let cancellation = CancellationToken::new();
    match case {
      "status" => {
        intent(&mut request).status = "approved".into();
        expected = FailureClass::Unsupported;
      }
      "digest" => {
        intent(&mut request).mapping.sha256 = "a".repeat(64);
        expected = FailureClass::Conflict;
      }
      "malformed" => {
        let bytes = br#"{"config_version":2}"#;
        private_file(
          &installation
            .root
            .path()
            .join("mappings")
            .join(hashed("arbitrary-installed-effects")),
          bytes,
        );
        intent(&mut request).mapping.sha256 = hex::encode(Sha256::digest(bytes));
      }
      "grant" => private_file(
        &installation
          .root
          .path()
          .join("credentials")
          .join(hashed("private-reporter-grant")),
        br#"{"class":"work_source_read","token":"private-secret","repositories":["installed-ticket-space"]}"#,
      ),
      "cancelled" => {
        cancellation.cancel();
        expected = FailureClass::Cancelled;
      }
      _ => unreachable!(),
    }
    let response = installation.provider().execute(&request, cancellation).await;
    response.validate_for(&request).unwrap();
    failure(response, expected);
    assert!(forge.requests.lock().await.is_empty(), "{case}");
  }
}

#[tokio::test]
async fn missing_repository_labels_never_create_labels_or_partially_update_the_issue() {
  use octacity_work_source_protocol::FailureClass;
  for missing_first in [true, false] {
    let forge = Forge::start().await;
    if !missing_first {
      forge
        .replies
        .lock()
        .await
        .push_back(Reply::json(json!({"name":"ready / review"})));
    }
    forge.replies.lock().await.push_back(Reply {
      status: 404,
      ..Reply::json(json!({"message":"private-provider-error"}))
    });
    let (installation, request) = install(&forge, mapping());
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    failure(response, FailureClass::Permanent);
    let requests = forge.requests.lock().await;
    assert_eq!(requests.len(), if missing_first { 2 } else { 3 });
    assert!(requests.iter().all(|request| request.starts_with("GET ")));
  }
}

#[tokio::test]
async fn foreign_issues_pull_requests_and_incomplete_label_observations_cannot_authorize_writes() {
  use octacity_work_source_protocol::FailureClass;
  for case in ["repository", "item", "number", "pull_request", "pagination"] {
    let forge = Forge::start().await;
    let mut observed = issue(9001, 17);
    let mut expected = FailureClass::Conflict;
    match case {
      "repository" => *forge.repository.lock().await = json!({"id":999}),
      "item" => observed["id"] = json!(999),
      "number" => observed["number"] = json!(99),
      "pull_request" => observed["pull_request"] = json!({}),
      "pagination" => expected = FailureClass::ProtocolFault,
      _ => unreachable!(),
    }
    forge.replies.lock().await.extend([
      Reply::json(json!({"name":"ready / review"})),
      Reply::json(json!({"name":"needs context"})),
      Reply::json(observed),
      Reply {
        headers: vec![("link".into(), "<https://forbidden.invalid/>; rel=\"next\"".into())],
        ..Reply::json(json!([]))
      },
    ]);
    let (installation, request) = install(&forge, mapping());
    failure(
      installation
        .provider()
        .execute(&request, CancellationToken::new())
        .await,
      expected,
    );
    assert!(
      forge
        .requests
        .lock()
        .await
        .iter()
        .all(|request| request.starts_with("GET "))
    );
  }
}

#[tokio::test]
async fn actual_reporter_process_confirms_already_matching_effects_without_writes() {
  use tokio::io::AsyncWriteExt as _;
  let forge = Forge::start().await;
  forge.replies.lock().await.extend([
    Reply::json(json!({"name":"ready / review"})),
    Reply::json(json!({"name":"needs context"})),
    Reply::json(issue(9001, 17)),
    Reply::json(json!([{"name":"READY / REVIEW"},{"name":"human-owned"}])),
    Reply::json(issue(9001, 17)),
    Reply::json(json!([{"name":"READY / REVIEW"},{"name":"human-owned"}])),
  ]);
  let (installation, request) = install(&forge, mapping());
  let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(installation.root.path())
    .env_clear()
    .kill_on_drop(true)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  let mut bytes = serde_json::to_vec(&request).unwrap();
  bytes.push(b'\n');
  child.stdin.take().unwrap().write_all(&bytes).await.unwrap();
  let output = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait_with_output())
    .await
    .unwrap()
    .unwrap();
  assert!(output.status.success());
  assert!(output.stderr.is_empty());
  let response = octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap();
  assert!(
    matches!(
      response,
      Response::Reporter(reporter::Response {
        outcome: reporter::Outcome::Applied(_),
        ..
      })
    ),
    "{response:?}"
  );
  assert!(
    forge
      .requests
      .lock()
      .await
      .iter()
      .all(|request| request.starts_with("GET "))
  );
}
