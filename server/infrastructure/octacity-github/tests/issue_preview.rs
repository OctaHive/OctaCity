#![cfg(unix)]
#[path = "support/issues.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::*;
use octacity_work_source_protocol::{Request, Response, preview::SelectionMismatch as Reason, source};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn native_preview_explains_filtered_and_matching_issues_with_only_bounded_gets() {
  let forge = Forge::start().await;
  let mut excluded = issue(9002, 18);
  excluded["state"] = json!("closed");
  excluded["labels"] = json!([{"name":"on hold"}]);
  let mut matching = issue(9003, 19);
  matching["title"] = json!("Я".repeat(300));
  matching["body"] = json!("private-body-never-in-preview");
  matching["labels"] = json!([{"name":"ready-for-review"},{"name":"documentation"}]);
  let mut pull = issue(9004, 20);
  pull["pull_request"] = json!({});
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([issue(9001, 17), excluded, matching, pull])));
  let installation = Installation::new(
    &forge,
    json!({"state":"open","all_of":["ready-for-review"],"any_of":["documentation"],"none_of":["on hold"]}),
  );
  let mut wire = serde_json::to_value(installation.request()).unwrap();
  wire["message"]["command"]["operation"] = json!("preview");
  let request: Request = octacity_work_source_protocol::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  let safe = serde_json::to_string(&response).unwrap();
  assert!(!safe.contains("private-body-never-in-preview"));
  assert!(!safe.contains("private-fixture-token"));
  let Response::Source(source::Response {
    outcome: source::Outcome::Previewed(page),
    ..
  }) = response
  else {
    panic!("expected native summaries: {response:?}")
  };
  assert_eq!(
    page.candidates.iter().map(|c| c.item_id.as_str()).collect::<Vec<_>>(),
    ["9001", "9002", "9003"]
  );
  assert_eq!(
    page.candidates[0].selections[0].reasons,
    [Reason::RequiredLabelMissing, Reason::AlternativeLabelMissing]
  );
  assert_eq!(
    page.candidates[1].selections[0].reasons,
    [
      Reason::StateMismatch,
      Reason::RequiredLabelMissing,
      Reason::AlternativeLabelMissing,
      Reason::ExcludedLabelPresent
    ]
  );
  assert!(page.candidates[2].selections[0].reasons.is_empty());
  assert_eq!(page.candidates[2].title, "Я".repeat(256));
  assert!(page.next_cursor.is_none());
  let requests = forge.requests.lock().await;
  assert_eq!(requests.len(), 2);
  assert!(requests.iter().all(|r| r.starts_with("GET ")));
}

// Characterize the existing native read failure contract on the Preview operation as well.
#[tokio::test]
async fn native_preview_never_confuses_empty_with_throttled_or_unavailable_and_never_writes() {
  for status in [200, 429, 503] {
    let forge = Forge::start().await;
    let reply = if status == 200 {
      Reply::json(json!([]))
    } else {
      Reply {
        status,
        headers: if status == 429 {
          vec![("retry-after".into(), "2".into())]
        } else {
          vec![]
        },
        ..Reply::json(json!({"message":"private-error-never-expose"}))
      }
    };
    forge.replies.lock().await.push_back(reply);
    let installation = Installation::new(&forge, selector());
    let mut wire = serde_json::to_value(installation.request()).unwrap();
    wire["message"]["command"]["operation"] = json!("preview");
    let request: Request = octacity_work_source_protocol::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
    let response = installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await;
    response.validate_for(&request).unwrap();
    assert!(
      !serde_json::to_string(&response)
        .unwrap()
        .contains("private-error-never-expose")
    );
    match response {
      Response::Source(source::Response {
        outcome: source::Outcome::Previewed(page),
        ..
      }) => {
        assert_eq!(status, 200);
        assert!(page.candidates.is_empty());
      }
      Response::Source(source::Response {
        outcome: source::Outcome::Failure(failure),
        ..
      }) => {
        assert_ne!(status, 200);
        assert_eq!(failure.class, octacity_work_source_protocol::FailureClass::Transient);
        assert_eq!(
          failure.retry_after_millis,
          if status == 429 { Some(2000) } else { None }
        );
      }
      other => panic!("unexpected preview response: {other:?}"),
    }
    assert!(forge.requests.lock().await.iter().all(|r| r.starts_with("GET ")));
  }
}
