#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/issues.rs"]
mod fixture;
use fixture::*;
use octacity_work_source_protocol::{FailureClass, Request, Response, decode_request, review};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn installed(forge: &Forge) -> Installation {
  let installation = Installation::new(forge, selector());
  let path = installation.root.path().join("github-work.toml");
  let config = std::fs::read_to_string(&path).unwrap().replace(
    "[capabilities]",
    "[capabilities]\nreview_repositories = [\"installed-ticket-space\"]",
  );
  private_file(&path, config.as_bytes());
  private_file(
    &installation
      .root
      .path()
      .join("credentials")
      .join(hashed("independent-code-reader")),
    br#"{"class":"vcs_read","token":"private-review-token","repositories":["installed-ticket-space"]}"#,
  );
  installation
}
fn request(installation: &Installation) -> Request {
  let mut wire: Value = serde_json::from_slice(include_bytes!(
    "../../../protocols/octacity-work-source-protocol/fixtures/review-v1.json"
  ))
  .unwrap();
  let intent = &mut wire["message"]["command"]["payload"];
  intent["configuration_digest"] = json!(installation.provider().configuration_digest());
  intent["resource"]["resource_id"] = json!("1234");
  intent["resource"]["resource_locator"] = json!("installed-ticket-space");
  intent["network_hosts"] = json!(["localhost"]);
  decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap()
}
fn native_review() -> Value {
  json!({"id":9001,"number":23,"title":"Review title","body":"PRIVATE-BODY-MUST-NOT-CROSS","html_url":"https://github.example/owner/tickets/pull/23", "user":{"login":"visible-author"},"state":"open","draft":true,"merged":false,
    "head":{"sha":"a".repeat(40)},"base":{"sha":"b".repeat(40),"repo":{"id":1234}},"changed_files":4,"additions":7,"deletions":0})
}
fn observed(response: Response) -> review::Observation {
  match response {
    Response::Review(review::Response {
      outcome: review::Outcome::Observed(observed),
      ..
    }) => observed,
    other => panic!("review observation: {other:?}"),
  }
}
fn failure(response: Response, class: FailureClass) {
  match response {
    Response::Review(review::Response {
      outcome: review::Outcome::Failure(failure),
      ..
    }) => assert_eq!(failure.class, class),
    other => panic!("closed failure: {other:?}"),
  }
}

#[tokio::test]
async fn installed_repository_reader_observes_safe_metadata_over_verified_https() {
  let forge = Forge::start().await;
  forge.replies.lock().await.push_back(Reply::json(native_review()));
  let installation = installed(&forge);
  let request = request(&installation);
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  assert!(!serde_json::to_string(&response).unwrap().contains("PRIVATE-BODY"));
  let observation = observed(response);
  assert_eq!(observation.subject.resource_id, "1234");
  assert_eq!(observation.subject.item_id, "23");
  assert_eq!(observation.facts.title.as_deref(), Some("Review title"));
  assert_eq!(observation.facts.number.as_deref(), Some("23"));
  assert_eq!(observation.facts.author.as_deref(), Some("visible-author"));
  assert_eq!(observation.facts.state, Some(review::State::Open));
  assert_eq!(observation.facts.draft, Some(true));
  assert_eq!(observation.facts.head, Some("a".repeat(40)));
  assert_eq!(observation.facts.base, Some("b".repeat(40)));
  assert_eq!(observation.facts.changed_files, Some(4));
  assert_eq!(observation.facts.additions, Some(7));
  assert_eq!(observation.facts.deletions, Some(0));
  assert!(observation.facts.review.is_none() && observation.facts.checks.is_none());
  let requests = forge.requests.lock().await;
  assert_eq!(requests.len(), 2);
  assert!(requests[1].starts_with("GET /api/v3/repos/owner/tickets/pulls/23 HTTP/"));
  for request in requests.iter() {
    assert!(
      request
        .to_ascii_lowercase()
        .contains("authorization: bearer private-review-token")
    );
    assert!(
      request
        .to_ascii_lowercase()
        .contains("x-github-api-version: fixture-v1")
    );
  }
}

#[tokio::test]
async fn grant_checks_use_only_the_installed_repository_credential_and_no_provider_calls() {
  let forge = Forge::start().await;
  let installation = installed(&forge);
  let mut request = request(&installation);
  let Request::Review(read) = &mut request else {
    unreachable!()
  };
  let intent = read.command.intent().unwrap().clone();
  read.command = review::Command::Authorize(Box::new(intent));
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  assert!(matches!(
    response,
    Response::Review(review::Response {
      outcome: review::Outcome::Authorized(_),
      ..
    })
  ));
  assert!(forge.requests.lock().await.is_empty());
  // An ambient credential can never substitute the missing independent installed grant.
  std::fs::remove_file(
    installation
      .root
      .path()
      .join("credentials")
      .join(hashed("independent-code-reader")),
  )
  .unwrap();
  failure(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
    FailureClass::Permanent,
  );
  let Request::Review(read) = &mut request else {
    unreachable!()
  };
  let review::Command::Authorize(intent) = &mut read.command else {
    unreachable!()
  };
  intent.resource.credential.handle = "private-source-grant".into();
  failure(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
    FailureClass::Permanent,
  );
  assert!(forge.requests.lock().await.is_empty());
}

fn check_request(installation: &Installation) -> Request {
  let mut request = request(installation);
  let Request::Review(read) = &mut request else {
    unreachable!()
  };
  let mut intent = read.command.intent().unwrap().clone();
  intent.head = Some("a".repeat(40));
  read.command = review::Command::Checks(Box::new(intent));
  request
}
fn check(id: u64, state: &str, conclusion: Option<&str>) -> Value {
  json!({"id":id,"name":"unit tests","head_sha":"a".repeat(40),"status":state,"conclusion":conclusion,
    "html_url":"https://github.example/owner/tickets/runs/1","app":{"slug":"ci"},
    "output":{"text":"PRIVATE-CHECK-LOG","annotations_url":"https://forbidden.invalid/annotations"}})
}
fn checks(response: Response) -> review::CheckPage {
  match response {
    Response::Review(review::Response {
      outcome: review::Outcome::Checks(page),
      ..
    }) => page,
    other => panic!("bounded checks: {other:?}"),
  }
}

#[tokio::test]
async fn check_pages_use_fixed_repository_and_original_head_without_following_provider_links() {
  let forge = Forge::start().await;
  let installation = installed(&forge);
  let mut reply = Reply::json(json!({"total_count":2,"check_runs":[check(1,"completed",Some("future-conclusion"))]}));
  reply.headers.push((
    "link".into(),
    format!(
      "<{}repos/owner/tickets/commits/{}/check-runs?per_page=2&page=2>; rel=\"next\"",
      forge.endpoint,
      "a".repeat(40)
    ),
  ));
  forge.replies.lock().await.push_back(reply);
  let mut request = check_request(&installation);
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  assert!(!serde_json::to_string(&response).unwrap().contains("PRIVATE-CHECK"));
  let first = checks(response);
  assert_eq!(first.head, "a".repeat(40));
  assert_eq!(first.items[0].state, review::CheckState::Completed);
  assert!(first.items[0].conclusion.is_none());
  assert!(first.next_cursor.is_some());
  assert_eq!(
    forge.requests.lock().await.len(),
    2,
    "one page only, no implicit pagination"
  );
  let Request::Review(read) = &mut request else {
    unreachable!()
  };
  let review::Command::Checks(intent) = &mut read.command else {
    unreachable!()
  };
  intent.cursor = first.next_cursor;
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!({"check_runs":[check(2,"queued",None)]})));
  let second = checks(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(second.items[0].state, review::CheckState::Pending);
  assert!(second.next_cursor.is_none());
  let requests = forge.requests.lock().await;
  assert!(requests[1].starts_with(&format!(
    "GET /api/v3/repos/owner/tickets/commits/{}/check-runs?per_page=2&page=1 HTTP/",
    "a".repeat(40)
  )));
  assert!(requests[3].contains("check-runs?per_page=2&page=2 HTTP/"));
}

fn spawn_reader(directory: &std::path::Path) -> tokio::process::Child {
  use std::process::Stdio;
  tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(directory)
    .env_clear()
    .env("GITHUB_TOKEN", "ambient-must-not-be-used")
    .env("GH_TOKEN", "ambient-must-not-be-used")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn()
    .unwrap()
}

#[tokio::test]
async fn native_process_uses_installed_authority_and_preserves_cancellation_and_failures() {
  use tokio::io::AsyncWriteExt as _;
  let forge = Forge::start().await;
  let installation = installed(&forge);
  let exchange = |request: Request| {
    let directory = installation.root.path().to_owned();
    async move {
      let mut child = spawn_reader(&directory);
      let mut frame = serde_json::to_vec(&request).unwrap();
      frame.push(b'\n');
      child.stdin.take().unwrap().write_all(&frame).await.unwrap();
      let output = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
      assert!(output.status.success());
      assert!(output.stderr.is_empty());
      octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap()
    }
  };
  forge.replies.lock().await.push_back(Reply::json(native_review()));
  let original = request(&installation);
  let retained = observed(exchange(original.clone()).await);
  assert_eq!(retained.facts.head, Some("a".repeat(40)));
  let cancel = decode_request(&serde_json::to_vec(&json!({"family":"review","message":{"protocol_version":1,"request_id":"cancel","command":{"operation":"cancel","payload":{"target_read_id":"a".repeat(64)}}}})).unwrap()).unwrap();
  assert!(matches!(
    exchange(cancel).await,
    Response::Review(review::Response {
      outcome: review::Outcome::Acknowledged { .. },
      ..
    })
  ));
  std::fs::remove_file(
    installation
      .root
      .path()
      .join("credentials")
      .join(hashed("independent-code-reader")),
  )
  .unwrap();
  failure(exchange(original).await, FailureClass::Permanent);
  assert_eq!(
    forge.requests.lock().await.len(),
    2,
    "missing independent grant makes no call despite ambient tokens"
  );
  assert_eq!(
    retained.facts.head,
    Some("a".repeat(40)),
    "outage or revoked reads cannot rewrite historical facts"
  );
}

#[tokio::test]
async fn caller_response_bound_is_enforced_before_returning_provider_metadata() {
  let forge = Forge::start().await;
  let installation = installed(&forge);
  forge.replies.lock().await.push_back(Reply::json(native_review()));
  let mut request = request(&installation);
  let Request::Review(read) = &mut request else {
    unreachable!()
  };
  let review::Command::Observe(intent) = &mut read.command else {
    unreachable!()
  };
  intent.max_page_bytes = 128;
  failure(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
    FailureClass::ProtocolFault,
  );
}

#[tokio::test]
async fn unsupported_fields_stay_unknown_and_remote_identity_cannot_retarget_a_review() {
  for defect in ["unsupported", "repository", "review", "base-repository"] {
    let forge = Forge::start().await;
    let installation = installed(&forge);
    let mut value = native_review();
    match defect {
      "unsupported" => {
        value
          .as_object_mut()
          .unwrap()
          .retain(|key, _| ["id", "number", "base"].contains(&key.as_str()));
        value["html_url"] = json!("https://github.example/pull/23?credential=must-not-appear");
      }
      "repository" => {
        *forge.repository.lock().await = json!({"id":9999});
      }
      "review" => {
        value["number"] = json!(24);
      }
      _ => {
        value["base"]["repo"]["id"] = json!(9999);
      }
    }
    forge.replies.lock().await.push_back(Reply::json(value));
    let response = installation
      .provider()
      .execute(&request(&installation), CancellationToken::new())
      .await;
    if defect == "unsupported" {
      let facts = observed(response).facts;
      assert!(
        facts.title.is_none()
          && facts.author.is_none()
          && facts.state.is_none()
          && facts.draft.is_none()
          && facts.head.is_none()
          && facts.changed_files.is_none()
          && facts.additions.is_none()
          && facts.deletions.is_none()
          && facts.url.is_none()
      );
    } else {
      failure(response, FailureClass::Conflict);
    }
    assert_eq!(
      forge.requests.lock().await.len(),
      if defect == "repository" { 1 } else { 2 }
    );
  }
}

#[tokio::test]
async fn checks_reject_untrusted_links_scope_drift_and_throttling_without_hidden_retry() {
  for defect in ["head", "duplicate", "link", "throttle", "cursor"] {
    let forge = Forge::start().await;
    let installation = installed(&forge);
    let mut request = check_request(&installation);
    let mut run = check(1, "completed", Some("success"));
    if defect == "head" {
      run["head_sha"] = json!("b".repeat(40));
    }
    let mut reply = Reply::json(json!({"check_runs":[run.clone()]}));
    if defect == "duplicate" {
      reply = Reply::json(json!({"check_runs":[run.clone(),run]}));
    }
    if defect == "link" {
      reply.headers.push((
        "link".into(),
        "<https://forbidden.invalid/private>; rel=\"next\"".into(),
      ));
    }
    if defect == "throttle" {
      reply.status = 429;
      reply.headers.push(("retry-after".into(), "2".into()));
    }
    if defect == "cursor" {
      let Request::Review(read) = &mut request else {
        unreachable!()
      };
      let review::Command::Checks(intent) = &mut read.command else {
        unreachable!()
      };
      intent.cursor = Some("https://forbidden.invalid/private".into());
    }
    forge.replies.lock().await.push_back(reply);
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    if defect == "throttle" {
      match response {
        Response::Review(review::Response {
          outcome: review::Outcome::Failure(failure),
          ..
        }) => {
          assert_eq!(failure.class, FailureClass::Transient);
          assert_eq!(failure.retry_after_millis, Some(2000));
        }
        other => panic!("closed rate limit: {other:?}"),
      }
    } else {
      failure(
        response,
        match defect {
          "head" => FailureClass::Conflict,
          "cursor" => FailureClass::InvalidRequest,
          _ => FailureClass::ProtocolFault,
        },
      );
    }
    assert_eq!(
      forge.requests.lock().await.len(),
      if defect == "cursor" { 0 } else { 2 },
      "no provider URL or implicit retry is followed"
    );
  }
}

#[tokio::test]
async fn native_process_cancels_the_original_read_while_https_is_stalled() {
  use tokio::io::AsyncWriteExt as _;
  let forge = Forge::start().await;
  let installation = installed(&forge);
  let mut stalled = Reply::json(native_review());
  stalled.stall = true;
  forge.replies.lock().await.push_back(stalled);
  let request = request(&installation);
  let mut child = spawn_reader(installation.root.path());
  let mut input = child.stdin.take().unwrap();
  let mut frame = serde_json::to_vec(&request).unwrap();
  frame.push(b'\n');
  input.write_all(&frame).await.unwrap();
  forge.wait_for_requests(2).await;
  let cancel = json!({"family":"review","message":{"protocol_version":1,"request_id":"stop-original-read","command":{"operation":"cancel","payload":{"target_read_id":"a".repeat(64)}}}});
  let mut frame = serde_json::to_vec(&cancel).unwrap();
  frame.push(b'\n');
  input.write_all(&frame).await.unwrap();
  drop(input);
  let output = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
    .await
    .unwrap()
    .unwrap();
  assert!(output.status.success() && output.stderr.is_empty());
  failure(
    octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap(),
    FailureClass::Cancelled,
  );
  assert_eq!(forge.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn github_checks_require_a_native_review_key_before_provider_calls() {
  let forge = Forge::start().await;
  let installation = installed(&forge);
  let mut request = check_request(&installation);
  let Request::Review(read) = &mut request else {
    unreachable!()
  };
  let review::Command::Checks(intent) = &mut read.command else {
    unreachable!()
  };
  intent.item_id = "https://forbidden.invalid/task-key".into();
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!({"check_runs":[]})));
  failure(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
    FailureClass::InvalidRequest,
  );
  assert!(forge.requests.lock().await.is_empty());
}
