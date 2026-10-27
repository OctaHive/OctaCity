#![cfg(unix)]
#[path = "support/issues.rs"]
mod fixture;
use fixture::*;
use octacity_work_source_protocol::Request;
use octacity_work_source_protocol::{Response, source};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn intent(request: &mut Request) -> &mut source::ScanIntent {
  match request {
    Request::Source(source::Request {
      command: source::Command::Scan(intent),
      ..
    }) => intent,
    _ => panic!("scan fixture"),
  }
}
fn failure(response: Response, expected: octacity_work_source_protocol::FailureClass) {
  match response {
    Response::Source(source::Response {
      outcome: source::Outcome::Failure(failure),
      ..
    }) => assert_eq!(failure.class, expected),
    response => panic!("expected a closed failure, got {response:?}"),
  }
}

fn page(response: Response) -> source::Page {
  match response {
    Response::Source(source::Response {
      outcome: source::Outcome::Page(page),
      ..
    }) => page,
    response => panic!("expected a normalized Issue page, got {response:?}"),
  }
}

#[tokio::test]
async fn initial_epoch_window_omits_since_and_still_filters_future_updates() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let mut request = installation.request();
  intent(&mut request).window = Some(source::ScanWindow {
    from_millis: 0,
    until_millis: 1_792_137_600_000, // 2026-10-16 08:00:00 UTC.
  });
  let mut future = issue(9002, 18);
  future["updated_at"] = json!("2026-10-16T08:00:00.001Z");
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([issue(9001, 17), future])));
  let result = page(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(
    result.candidates.iter().map(|c| c.item_id.as_str()).collect::<Vec<_>>(),
    ["9001"]
  );
  let requests = forge.requests.lock().await;
  let target = requests[1].split_whitespace().nth(1).unwrap();
  let url = url::Url::parse(&format!("https://localhost{target}")).unwrap();
  assert!(
    url.query_pairs().all(|(key, _)| key != "since"),
    "initial history must not use GitHub's epoch since sentinel"
  );
}

#[tokio::test]
async fn installed_read_scope_lists_private_issues_over_verified_https() {
  let forge = Forge::start().await;
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([issue(9001, 17)])));
  let installation = Installation::new(&forge, selector());
  let request = installation.request();
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  let page = page(response);
  assert_eq!(page.resource_id, "1234");
  assert!(page.next_cursor.is_none());
  assert_eq!(page.candidates.len(), 1);
  let candidate = &page.candidates[0];
  assert_eq!(candidate.item_id, "9001");
  assert_eq!(candidate.display_id, "17");
  assert_eq!(candidate.update_identity, "2026-10-16T08:00:00Z");
  assert_eq!(candidate.title, "Private task title");
  assert_eq!(candidate.body.as_deref(), Some("Task\nAcceptance: explicit criteria"));
  assert!(candidate.labels.is_empty());
  assert!(candidate.fields.is_empty());
  assert_eq!(candidate.state, "open");
  let requests = forge.requests.lock().await;
  assert_eq!(
    requests.len(),
    2,
    "only repository identity and Issue listing may be fetched"
  );
  assert!(requests[0].starts_with("GET /api/v3/repos/owner/tickets HTTP/"));
  assert!(requests[1].starts_with("GET /api/v3/repos/owner/tickets/issues?"));
  for request in requests.iter() {
    assert!(
      request
        .to_ascii_lowercase()
        .contains("authorization: bearer private-fixture-token")
    );
    let headers = request.to_ascii_lowercase();
    assert_eq!(
      headers
        .lines()
        .filter(|line| line.starts_with("x-github-api-version:"))
        .collect::<Vec<_>>(),
      ["x-github-api-version: fixture-v1"]
    );
  }
}

#[tokio::test]
async fn every_returned_issue_is_filtered_locally_and_pull_requests_are_excluded() {
  let forge = Forge::start().await;
  let configured = json!({"state":"closed","all_of":["ready-for-review"],"any_of":["documentation","enhancement"],"none_of":["on hold"]});
  let installation = Installation::new(&forge, configured);
  let with = |id, state, labels| {
    let mut item = issue(id, id - 9000);
    item["state"] = json!(state);
    item["labels"] = json!(labels);
    item
  };
  forge.replies.lock().await.push_back(Reply::json(json!([
    with(9001,"closed",vec![json!({"name":"READY-FOR-REVIEW"}),json!({"name":"documentation"}),json!({"name":"unrelated"})]),
    with(9002,"closed",vec![json!({"name":"documentation"})]),
    with(9003,"closed",vec![json!({"name":"ready-for-review"})]),
    with(9004,"closed",vec![json!({"name":"ready-for-review"}),json!({"name":"enhancement"}),json!({"name":"ON HOLD"})]),
    with(9005,"open",vec![json!({"name":"ready-for-review"}),json!({"name":"documentation"})]),
    {"pull_request":{"url":"https://forbidden.invalid/pull/1"}},
    {"pull_request":null}
  ])));
  let selected = page(
    installation
      .provider()
      .execute(&installation.request(), CancellationToken::new())
      .await,
  );
  assert_eq!(
    selected
      .candidates
      .iter()
      .map(|item| item.item_id.as_str())
      .collect::<Vec<_>>(),
    ["9001"]
  );
  assert_eq!(selected.candidates[0].state, "closed");
  assert_eq!(
    selected.candidates[0].labels,
    ["READY-FOR-REVIEW", "documentation", "unrelated"]
  );
  assert_eq!(forge.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn pagination_keeps_empty_filtered_pages_and_continues_the_exact_installed_query() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let mut request = installation.request();
  intent(&mut request).max_items = 2;
  let mut filtered = issue(9000, 1);
  filtered["state"] = json!("closed");
  let mut first = Reply::json(json!([filtered]));
  first.headers.push(("Link".into(),format!("<{}repos/owner/tickets/issues?state=all&sort=updated&direction=asc&per_page=2&page=2>; rel=\"next\", <{}repos/owner/tickets/issues?state=all&sort=updated&direction=asc&per_page=2&page=3>; rel=\"last\"",forge.endpoint,forge.endpoint)));
  forge
    .replies
    .lock()
    .await
    .extend([first, Reply::json(json!([issue(9001, 17)]))]);
  let provider = installation.provider();
  let first = page(provider.execute(&request, CancellationToken::new()).await);
  assert!(first.candidates.is_empty());
  let cursor = first.next_cursor.expect("filtered page is not the end of discovery");
  assert!(!cursor.contains("https://"));
  intent(&mut request).cursor = Some(cursor);
  let second = page(provider.execute(&request, CancellationToken::new()).await);
  assert_eq!(second.candidates[0].item_id, "9001");
  assert!(second.next_cursor.is_none());
  let requests = forge.requests.lock().await;
  assert_eq!(requests.len(), 4);
  let url = url::Url::parse(&format!(
    "https://localhost{}",
    requests[3].split_whitespace().nth(1).unwrap()
  ))
  .unwrap();
  let query: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
  assert_eq!(query.get("page").map(|s| s.as_ref()), Some("2"));
  assert_eq!(query.get("per_page").map(|s| s.as_ref()), Some("2"));
  assert_eq!(query.get("state").map(|s| s.as_ref()), Some("all"));
}

#[tokio::test]
async fn pagination_destinations_cannot_change_the_origin_repository_or_query() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  let valid = format!(
    "{}repos/owner/tickets/issues?state=all&sort=updated&direction=asc&per_page=100&page=2",
    forge.endpoint
  );
  for link in [
    valid.replace("localhost", "127.0.0.1"),
    valid.replace("https://", "http://"),
    valid.replace("/owner/tickets/", "/other/tickets/"),
    valid.replace("state=all", "state=closed"),
    valid.replace("page=2", "page=1"),
    format!("{valid}&page=3"),
    format!("{valid}#private-fragment"),
    valid.replace("https://", "https://private-token@"),
  ] {
    let mut reply = Reply::json(json!([]));
    reply.headers.push(("Link".into(), format!("<{link}>; rel=\"next\"")));
    forge.replies.lock().await.push_back(reply);
    failure(
      provider
        .execute(&installation.request(), CancellationToken::new())
        .await,
      FailureClass::ProtocolFault,
    );
  }
  assert_eq!(
    forge.requests.lock().await.len(),
    16,
    "no unapproved continuation is fetched"
  );
}

#[tokio::test]
async fn malformed_identity_and_unbounded_provider_pages_are_failures_without_partial_candidates() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  let mut zero_number = issue(9001, 0);
  zero_number["labels"] = json!([]);
  let mut oversize = issue(9001, 17);
  oversize["title"] = json!("x".repeat(4097));
  for response in [
    json!([issue(0, 17)]),
    json!([zero_number]),
    json!([oversize]),
    json!({"message":"private-provider-error"}),
    json!([issue(9001, 17), issue(9001, 18)]),
  ] {
    forge.replies.lock().await.push_back(Reply::json(response));
    failure(
      provider
        .execute(&installation.request(), CancellationToken::new())
        .await,
      FailureClass::ProtocolFault,
    );
  }
  let mut request = installation.request();
  intent(&mut request).max_items = 1;
  let mut filtered = issue(9001, 17);
  filtered["state"] = json!("closed");
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([filtered.clone(), filtered])));
  failure(
    provider.execute(&request, CancellationToken::new()).await,
    FailureClass::ProtocolFault,
  );
}

#[tokio::test]
async fn fixed_update_windows_include_equal_boundaries_and_stable_ids_survive_edits() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  let mut request = installation.request();
  intent(&mut request).window = Some(source::ScanWindow {
    from_millis: 1000,
    until_millis: 2000,
  });
  let at = |id, updated| {
    let mut issue = issue(id, 17);
    issue["updated_at"] = json!(updated);
    issue
  };
  forge.replies.lock().await.push_back(Reply::json(json!([
    at(9001, "1970-01-01T00:00:01Z"),
    at(9002, "1970-01-01T00:00:02Z"),
    at(9003, "1970-01-01T00:00:00.999Z"),
    at(9004, "1970-01-01T00:00:02.001Z")
  ])));
  let first = page(provider.execute(&request, CancellationToken::new()).await);
  assert_eq!(
    first.candidates.iter().map(|i| i.item_id.as_str()).collect::<Vec<_>>(),
    ["9001", "9002"]
  );
  let listing = forge.requests.lock().await[1].clone();
  let url = url::Url::parse(&format!(
    "https://localhost{}",
    listing.split_whitespace().nth(1).unwrap()
  ))
  .unwrap();
  assert_eq!(
    url
      .query_pairs()
      .find(|(name, _)| name == "since")
      .map(|(_, v)| v.into_owned()),
    Some("1970-01-01T00:00:00Z".into())
  );
  let mut edited = at(9001, "1970-01-01T00:00:02Z");
  edited["title"] = json!("Edited private title");
  edited["labels"] = json!([{"name":"unrelated"}]);
  forge.replies.lock().await.push_back(Reply::json(json!([edited])));
  let second = page(provider.execute(&request, CancellationToken::new()).await);
  assert_eq!(second.candidates[0].item_id, first.candidates[0].item_id);
  assert_ne!(
    second.candidates[0].update_identity,
    first.candidates[0].update_identity
  );
}

#[tokio::test]
async fn wire_byte_limits_apply_to_declared_lengths_and_chunked_streams() {
  use octacity_work_source_protocol::{FailureClass, MAX_RESPONSE_BYTES};
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  let mut declared = Reply::json(json!([]));
  declared
    .headers
    .push(("Content-Length".into(), (MAX_RESPONSE_BYTES + 1).to_string()));
  declared.body.clear();
  let mut chunked = Reply::json(json!([]));
  chunked.chunked = true;
  chunked.body = vec![b' '; MAX_RESPONSE_BYTES + 1];
  let mut malformed = Reply::json(json!([]));
  malformed.body = b"private response is not JSON".to_vec();
  for reply in [declared, chunked, malformed] {
    forge.replies.lock().await.push_back(reply);
    failure(
      provider
        .execute(&installation.request(), CancellationToken::new())
        .await,
      FailureClass::ProtocolFault,
    );
  }
}

#[tokio::test]
async fn provider_outages_and_denied_scopes_never_look_like_empty_discovery() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  for (status, headers, expected) in [
    (429, vec![], FailureClass::Transient),
    (503, vec![], FailureClass::Transient),
    (
      403,
      vec![("x-ratelimit-remaining".into(), "0".into())],
      FailureClass::Transient,
    ),
    (403, vec![("retry-after".into(), "60".into())], FailureClass::Transient),
    (401, vec![], FailureClass::Permanent),
    (403, vec![], FailureClass::Permanent),
    (404, vec![], FailureClass::Permanent),
    (422, vec![], FailureClass::Permanent),
    (409, vec![], FailureClass::Conflict),
  ] {
    forge.replies.lock().await.push_back(Reply {
      status,
      headers,
      ..Reply::json(json!({"message":"private-provider-error"}))
    });
    let result = installation
      .provider()
      .execute(&installation.request(), CancellationToken::new())
      .await;
    assert!(!format!("{result:?}").contains("private-provider-error"));
    failure(result, expected);
  }
  assert_eq!(
    forge.requests.lock().await.len(),
    18,
    "the SDK must not retry a page implicitly"
  );
}

fn edit_profile(installation: &Installation, edit: impl FnOnce(&mut toml::Value)) {
  let path = installation.root.path().join("github-work.toml");
  let mut config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  edit(&mut config);
  private_file(&path, toml::to_string(&config).unwrap().as_bytes());
}

#[tokio::test]
async fn issue_transport_requires_installed_trust_and_matching_hostname_without_redirects() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let untrusted = Installation::new(&forge, selector());
  edit_profile(&untrusted, |config| {
    config["api_root_certificates_pem"] = toml::Value::Array(vec![])
  });
  failure(
    untrusted
      .provider()
      .execute(&untrusted.request(), CancellationToken::new())
      .await,
    FailureClass::Transient,
  );
  let wrong_host = Installation::new(&forge, selector());
  edit_profile(&wrong_host, |config| {
    config["api_base"] = toml::Value::String(forge.endpoint.replace("localhost", "127.0.0.1"))
  });
  failure(
    wrong_host
      .provider()
      .execute(&wrong_host.request(), CancellationToken::new())
      .await,
    FailureClass::Transient,
  );
  assert!(
    forge.requests.lock().await.is_empty(),
    "TLS failures must precede HTTP and token transmission"
  );
  let installation = Installation::new(&forge, selector());
  for target in [
    format!("{}forbidden", forge.endpoint),
    "https://forbidden.invalid/stolen-token".into(),
  ] {
    let mut reply = Reply {
      status: 302,
      ..Reply::json(json!({"message":"private-redirect-body"}))
    };
    reply.headers.push(("Location".into(), target));
    forge.replies.lock().await.push_back(reply);
    failure(
      installation
        .provider()
        .execute(&installation.request(), CancellationToken::new())
        .await,
      FailureClass::ProtocolFault,
    );
  }
  assert_eq!(forge.requests.lock().await.len(), 4);
}

#[tokio::test]
async fn selector_integrity_private_files_and_native_repository_identity_precede_candidate_exposure() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  for defect in ["digest", "permissions", "malformed", "symlink"] {
    let installation = Installation::new(&forge, selector());
    let mut request = installation.request();
    let path = installation
      .root
      .path()
      .join("selections")
      .join(hashed("arbitrary-installed-selector"));
    match defect {
      "digest" => {
        intent(&mut request).selection.sha256 = "f".repeat(64);
      }
      "permissions" => {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
      }
      "malformed" => {
        let bytes = br#"{"state":"open","all_of":["private-label"],"any_of":[],"none_of":["private-label"]}"#;
        private_file(&path, bytes);
        use sha2::{Digest as _, Sha256};
        intent(&mut request).selection.sha256 = hex::encode(Sha256::digest(bytes));
      }
      "symlink" => {
        let moved = path.with_extension("private");
        std::fs::rename(&path, &moved).unwrap();
        std::os::unix::fs::symlink(moved, &path).unwrap();
      }
      _ => unreachable!(),
    }
    failure(
      installation
        .provider()
        .execute(&request, CancellationToken::new())
        .await,
      if defect == "digest" {
        FailureClass::Conflict
      } else {
        FailureClass::Permanent
      },
    );
  }
  assert!(forge.requests.lock().await.is_empty());
  let installation = Installation::new(&forge, selector());
  edit_profile(&installation, |config| {
    config["repositories"]["installed-ticket-space"]["repository_id"] = toml::Value::String("4321".into())
  });
  let mut request = installation.request();
  intent(&mut request).resource.resource_id = "4321".into();
  failure(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
    FailureClass::Conflict,
  );
  assert_eq!(
    forge.requests.lock().await.len(),
    1,
    "a recreated repository cannot lend its Issues to the old native identity"
  );
}

#[tokio::test]
async fn opaque_continuations_cannot_be_reused_for_another_query_or_choose_a_url() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  for cursor in [
    "https://forbidden.invalid/issues",
    "{\"page\":2,\"context\":\"forged\"}",
  ] {
    let mut request = installation.request();
    intent(&mut request).cursor = Some(cursor.into());
    failure(
      provider.execute(&request, CancellationToken::new()).await,
      FailureClass::InvalidRequest,
    );
  }
  assert!(forge.requests.lock().await.is_empty());
  let mut reply = Reply::json(json!([]));
  reply.headers.push((
    "Link".into(),
    format!(
      "<{}repos/owner/tickets/issues?state=all&sort=updated&direction=asc&per_page=100&page=2>; rel=\"next\"",
      forge.endpoint
    ),
  ));
  forge.replies.lock().await.push_back(reply);
  let first = page(
    provider
      .execute(&installation.request(), CancellationToken::new())
      .await,
  );
  let mut request = installation.request();
  intent(&mut request).cursor = first.next_cursor;
  intent(&mut request).max_items = 50;
  failure(
    provider.execute(&request, CancellationToken::new()).await,
    FailureClass::InvalidRequest,
  );
  assert_eq!(forge.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn cancellation_drops_an_inflight_read_and_precancellation_makes_no_provider_call() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  let request = installation.request();
  let cancelled = CancellationToken::new();
  cancelled.cancel();
  failure(provider.execute(&request, cancelled).await, FailureClass::Cancelled);
  assert!(forge.requests.lock().await.is_empty());
  forge.replies.lock().await.push_back(Reply {
    stall: true,
    ..Reply::json(json!([]))
  });
  let cancellation = CancellationToken::new();
  let operation = provider.execute(&request, cancellation.clone());
  tokio::pin!(operation);
  tokio::select! {result=&mut operation=>panic!("read ended before cancellation: {result:?}"), _=forge.wait_for_requests(2)=>{}}
  cancellation.cancel();
  failure(
    tokio::time::timeout(std::time::Duration::from_secs(2), operation)
      .await
      .unwrap(),
    FailureClass::Cancelled,
  );
}

async fn process(installation: &Installation, request: &Request) -> tokio::process::Child {
  use tokio::io::AsyncWriteExt as _;
  let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(installation.root.path())
    .env_clear()
    .kill_on_drop(true)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  let mut bytes = serde_json::to_vec(request).unwrap();
  bytes.push(b'\n');
  child.stdin.as_mut().unwrap().write_all(&bytes).await.unwrap();
  child
}

#[tokio::test]
async fn the_actual_source_process_returns_a_checked_issue_page() {
  let forge = Forge::start().await;
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([issue(9001, 17)])));
  let installation = Installation::new(&forge, selector());
  let request = installation.request();
  let output = tokio::time::timeout(
    std::time::Duration::from_secs(5),
    process(&installation, &request).await.wait_with_output(),
  )
  .await
  .unwrap()
  .unwrap();
  assert!(output.status.success());
  assert!(output.stderr.is_empty());
  let result = octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap();
  assert_eq!(page(result).candidates[0].item_id, "9001");
}

#[tokio::test]
async fn the_source_process_honors_only_its_original_scan_cancellation() {
  use octacity_work_source_protocol::FailureClass;
  use tokio::io::AsyncWriteExt as _;
  for (family, target, expected) in [
    ("source", "target_scan_id", FailureClass::Cancelled),
    ("reporter", "target_projection_id", FailureClass::ProtocolFault),
  ] {
    let forge = Forge::start().await;
    forge.replies.lock().await.push_back(Reply {
      stall: true,
      ..Reply::json(json!([]))
    });
    let installation = Installation::new(&forge, selector());
    let mut request = installation.request();
    let original = intent(&mut request).scan_id.clone();
    let mut child = process(&installation, &request).await;
    forge.wait_for_requests(2).await;
    let cancellation = json!({"family":family,"message":{"protocol_version":1,"request_id":"cancel-original","command":{"operation":"cancel","payload":{target:original}}}});
    let mut bytes = serde_json::to_vec(&cancellation).unwrap();
    bytes.push(b'\n');
    child.stdin.take().unwrap().write_all(&bytes).await.unwrap();
    let output = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait_with_output())
      .await
      .expect("the process must stop its in-flight HTTPS read")
      .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    failure(
      octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap(),
      expected,
    );
  }
}

#[tokio::test]
async fn malformed_repository_metadata_cannot_qualify_a_ticket_container() {
  use octacity_work_source_protocol::FailureClass;
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  edit_profile(&installation, |config| {
    config["repositories"]["installed-ticket-space"]["repository_id"] = toml::Value::String("0".into())
  });
  let mut request = installation.request();
  intent(&mut request).resource.resource_id = "0".into();
  let provider = installation.provider();
  for metadata in [
    json!({"id":0}),
    json!({"id":"1234"}),
    json!({"message":"private-provider-error"}),
  ] {
    *forge.repository.lock().await = metadata;
    forge.replies.lock().await.push_back(Reply::json(json!([])));
    failure(
      provider.execute(&request, CancellationToken::new()).await,
      FailureClass::ProtocolFault,
    );
  }
  assert_eq!(
    forge.requests.lock().await.len(),
    3,
    "malformed repository facts must stop before listing Issues"
  );
}

#[tokio::test]
async fn group_members_share_one_observation_and_return_every_matching_selector() {
  use sha2::{Digest as _, Sha256};
  let forge = Forge::start().await;
  let mut first = issue(1, 1);
  first["labels"] = json!([{"name":"arbitrary-one"}]);
  let mut second = issue(2, 2);
  second["labels"] = json!([{"name":"arbitrary-two"}]);
  let mut ambiguous = issue(3, 3);
  ambiguous["labels"] = json!([{"name":"arbitrary-one"},{"name":"arbitrary-two"}]);
  forge
    .replies
    .lock()
    .await
    .push_back(Reply::json(json!([first, second, ambiguous])));
  let mut primary = selector();
  primary["all_of"] = json!(["arbitrary-one"]);
  let installation = Installation::new(&forge, primary);
  let mut alternative = selector();
  alternative["all_of"] = json!(["arbitrary-two"]);
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
  let mut request = installation.request();
  let primary = intent(&mut request).selection.clone();
  intent(&mut request).additional_selections = vec![alternative.clone()];
  let result = page(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
  );
  assert_eq!(
    result.candidates.iter().map(|v| v.item_id.as_str()).collect::<Vec<_>>(),
    ["1", "2", "3"]
  );
  assert_eq!(
    result.matching_selections,
    vec![
      vec![primary.clone()],
      vec![alternative.clone()],
      vec![primary, alternative]
    ]
  );
  assert_eq!(
    forge.requests.lock().await.len(),
    2,
    "one repository read and one page, independent of group size"
  );
}

#[tokio::test]
async fn rate_limit_delays_are_preserved_and_excessive_waits_require_repair_instead_of_an_early_retry() {
  for (status, headers, expected) in [
    (429, vec![("retry-after".into(), "2".into())], Some(2000)),
    (403, vec![("x-ratelimit-remaining".into(), "0".into())], Some(60000)),
    (429, vec![], Some(60000)),
    (429, vec![("retry-after".into(), "3601".into())], None),
  ] {
    let forge = Forge::start().await;
    let mut reply = Reply::json(json!({"message":"private diagnostic"}));
    reply.status = status;
    reply.headers = headers;
    forge.replies.lock().await.push_back(reply);
    let installation = Installation::new(&forge, selector());
    let response = installation
      .provider()
      .execute(&installation.request(), CancellationToken::new())
      .await;
    let Response::Source(source::Response {
      outcome: source::Outcome::Failure(result),
      ..
    }) = response
    else {
      panic!("rate limit cannot become an empty page")
    };
    assert_eq!(result.retry_after_millis, expected);
    assert_eq!(
      result.class,
      if expected.is_some() {
        octacity_work_source_protocol::FailureClass::Transient
      } else {
        octacity_work_source_protocol::FailureClass::Permanent
      }
    );
    assert_eq!(forge.requests.lock().await.len(), 2);
  }
}

#[tokio::test]
async fn primary_rate_limit_reset_is_a_lower_bound_even_with_a_shorter_retry_after() {
  let forge = Forge::start().await;
  let reset = chrono::Utc::now().timestamp() + 120;
  let mut reply = Reply::json(json!({"message":"private diagnostic"}));
  reply.status = 403;
  reply.headers = vec![
    ("x-ratelimit-remaining".into(), "0".into()),
    ("x-ratelimit-reset".into(), reset.to_string()),
    ("retry-after".into(), "1".into()),
  ];
  forge.replies.lock().await.push_back(reply);
  let installation = Installation::new(&forge, selector());
  let response = installation
    .provider()
    .execute(&installation.request(), CancellationToken::new())
    .await;
  let Response::Source(source::Response {
    outcome: source::Outcome::Failure(result),
    ..
  }) = response
  else {
    panic!("rate limit cannot become an empty page")
  };
  assert_eq!(result.class, octacity_work_source_protocol::FailureClass::Transient);
  let delay = result.retry_after_millis.unwrap();
  assert!(
    (115000..=120000).contains(&delay),
    "the reset lower bound must survive: {delay}"
  );
}

#[tokio::test]
async fn source_authorization_rechecks_the_current_private_grant_without_listing_tickets() {
  let forge = Forge::start().await;
  let installation = Installation::new(&forge, selector());
  let provider = installation.provider();
  let Request::Source(mut request) = installation.request() else {
    unreachable!()
  };
  let source::Command::Scan(scan) = request.command else {
    unreachable!()
  };
  request.command = source::Command::Authorize(source::AccessIntent {
    scan_id: scan.scan_id,
    configuration_digest: scan.configuration_digest,
    resource: scan.resource,
    network_hosts: scan.network_hosts,
  });
  let request = Request::Source(request);
  let response = provider.execute(&request, CancellationToken::new()).await;
  response.validate_for(&request).unwrap();
  assert!(
    matches!(
      response,
      Response::Source(source::Response {
        outcome: source::Outcome::Authorized(_),
        ..
      })
    ),
    "installed read access must be checked without a new Issue page"
  );
  private_file(
    &installation
      .root
      .path()
      .join("credentials")
      .join(hashed("private-source-grant")),
    br#"{"class":"work_reporter_write","token":"private-revoked","repositories":["installed-ticket-space"]}"#,
  );
  failure(
    provider.execute(&request, CancellationToken::new()).await,
    octacity_work_source_protocol::FailureClass::Permanent,
  );
  assert!(
    forge.requests.lock().await.is_empty(),
    "grant checks do not list tickets, change a cursor or create effects"
  );
}

#[tokio::test]
async fn separate_installed_read_selector_observes_closed_nonmatching_tasks_without_execution_target() {
  let forge = Forge::start().await;
  let mut closed = issue(9001, 17);
  closed["state"] = json!("closed");
  let execution = Installation::new(
    &forge,
    json!({
      "state":"open", "all_of":["ready"], "any_of":[], "none_of":[],
    }),
  );
  let observer = Installation::new(
    &forge,
    json!({
      "state":"all", "all_of":[], "any_of":[], "none_of":[],
    }),
  );
  forge
    .replies
    .lock()
    .await
    .extend([Reply::json(json!([closed.clone()])), Reply::json(json!([closed]))]);
  let excluded = page(
    execution
      .provider()
      .execute(&execution.request(), CancellationToken::new())
      .await,
  );
  assert!(excluded.candidates.is_empty());
  let request = observer.request();
  let observed = observer.provider().execute(&request, CancellationToken::new()).await;
  observed.validate_for(&request).unwrap();
  let observed = page(observed);
  assert_eq!(observed.candidates.len(), 1);
  assert_eq!(observed.candidates[0].item_id, "9001");
  assert_eq!(observed.candidates[0].state, "closed");
  assert!(observed.candidates[0].labels.is_empty());
  assert_eq!(
    observed.candidates[0].body.as_deref(),
    Some("Task\nAcceptance: explicit criteria")
  );
  let calls = forge.requests.lock().await;
  assert_eq!(calls.len(), 4, "only existing bounded repository and page reads");
  assert!(
    calls
      .iter()
      .all(|call| call.starts_with("GET /api/v3/repos/owner/tickets"))
  );
}
