#![cfg(unix)]
#[path = "support/issues.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::*;
use octacity_work_source_protocol::{Request, Response, preview::SelectionMismatch as Reason, source};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn strictly_unlabeled_scan_and_preview_share_group_selection_and_initial_window_fences() {
  use sha2::{Digest as _, Sha256};
  for (operation, group) in [("scan", false), ("preview", false), ("scan", true), ("preview", true)] {
    let forge = Forge::start().await;
    let mut labeled = issue(9002, 18);
    labeled["labels"] = json!([{"name":"approved"}]);
    let mut unrelated = issue(9003, 19);
    unrelated["labels"] = json!([{"name":"human-owned"}]);
    let mut pull = issue(9004, 20);
    pull["pull_request"] = json!({});
    let mut future = issue(9005, 21);
    future["updated_at"] = json!("2026-10-16T08:00:00.001Z");
    forge
      .replies
      .lock()
      .await
      .push_back(Reply::json(json!([issue(9001, 17), labeled, unrelated, pull, future])));
    let mut strict = selector();
    strict["label_presence"] = json!("none");
    let installation = Installation::new(&forge, strict);
    let mut alternative = selector();
    alternative["all_of"] = json!(["approved"]);
    let bytes = serde_json::to_vec(&alternative).unwrap();
    private_file(
      &installation
        .root
        .path()
        .join("selections")
        .join(hashed("alternative-selector")),
      &bytes,
    );
    let alternative = octacity_work_source_protocol::ConfigurationReference {
      identity: "alternative-selector".into(),
      sha256: hex::encode(Sha256::digest(bytes)),
    };
    let mut wire = serde_json::to_value(installation.request()).unwrap();
    wire["message"]["command"]["operation"] = json!(operation);
    let payload = &mut wire["message"]["command"]["payload"];
    payload["window"] = json!({"from_millis":0,"until_millis":1_792_137_600_000_u64});
    if group {
      payload["additional_selections"] = json!([alternative]);
    }
    let primary = payload["selection"].clone();
    let request: Request = octacity_work_source_protocol::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
    // Exercise the actual binary through the same verified host as managed Source.
    let registry_root = tempfile::tempdir().unwrap();
    let directory = registry_root.path().join("github");
    std::fs::rename(installation.root.path(), &directory).unwrap();
    let binary = std::fs::read(env!("CARGO_BIN_EXE_octacity-github")).unwrap();
    let binary_digest = hex::encode(Sha256::digest(&binary));
    std::fs::write(directory.join("plugin"), binary).unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(directory.join("plugin"), std::fs::Permissions::from_mode(0o700)).unwrap();
    let configuration = wire["message"]["command"]["payload"]["configuration_digest"]
      .as_str()
      .unwrap();
    std::fs::write(
      directory.join("adapter.toml"),
      format!(
        "manifest_version=1\nadapter_id='github'\nexecutable='plugin'\nexecutable_sha256='{binary_digest}'\n\
      [protocol.source]\nmin=1\nmax=1\n[capabilities.source]\npreview=true\nlabel_presence=true\n\
      cancel=false\ncredential_class='work_source_read'\nconfiguration_digest='{configuration}'\n"
      ),
    )
    .unwrap();
    let registry = octacity_server_work_source::WorkAdapterRegistry::discover(registry_root.path()).unwrap();
    let adapter = registry.resolve("github", &binary_digest).unwrap();
    assert_eq!(
      serde_json::to_value(&adapter.manifest.capabilities.source).unwrap()["label_presence"],
      json!(true)
    );
    let response = adapter
      .execute(
        request.clone(),
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(1),
        CancellationToken::new(),
      )
      .await
      .unwrap();
    response.validate_for(&request).unwrap();
    match response {
      Response::Source(source::Response {
        outcome: source::Outcome::Page(page),
        ..
      }) => {
        let expected = if group { vec!["9001", "9002"] } else { vec!["9001"] };
        assert_eq!(
          page.candidates.iter().map(|c| c.item_id.as_str()).collect::<Vec<_>>(),
          expected
        );
        if group {
          assert_eq!(
            serde_json::to_value(page.matching_selections).unwrap(),
            json!([[primary], [alternative]])
          );
        } else {
          assert!(page.matching_selections.is_empty());
        }
      }
      Response::Source(source::Response {
        outcome: source::Outcome::Previewed(page),
        ..
      }) => {
        assert_eq!(
          page.candidates.iter().map(|c| c.item_id.as_str()).collect::<Vec<_>>(),
          ["9001", "9002", "9003"]
        );
        assert!(page.candidates[0].selections[0].reasons.is_empty());
        for candidate in &page.candidates[1..] {
          assert_eq!(
            serde_json::to_value(&candidate.selections[0].reasons).unwrap(),
            json!(["labels_present"])
          );
        }
        if group {
          assert!(page.candidates[1].selections[1].reasons.is_empty());
        }
      }
      other => panic!("expected {operation} under strict label selection: {other:?}"),
    }
    let requests = forge.requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.starts_with("GET ")));
    let target = requests[1].split_whitespace().nth(1).unwrap();
    let url = url::Url::parse(&format!("https://localhost{target}")).unwrap();
    assert!(url.query_pairs().all(|(key, _)| key != "since"));
  }
  let any = octacity_github::issue_selector::IssueSelector::from_config(
    br#"{"state":"open","all_of":[],"any_of":[],"none_of":[],"label_presence":"any"}"#,
  )
  .unwrap();
  assert_eq!(
    serde_json::to_string(&any).unwrap(),
    r#"{"state":"open","all_of":[],"any_of":[],"none_of":[]}"#
  );
}

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
