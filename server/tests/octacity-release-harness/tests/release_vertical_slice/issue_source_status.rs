//! Reviewed workflow/status synchronization over the shared packaged Source environment.
use super::*;

pub(super) async fn run() {
  let mut environment = source_environment::SourceEnvironment::start_with_reporter().await;
  let project = source_setup::enroll_writer(&environment.client, &environment.origin, &environment.installation).await;
  let mapping = environment
    .browser_phase("issue-source-mapping.spec.ts", &project)
    .await;
  if let Err(error) = mapping {
    environment.stop().await;
    panic!("{error}");
  }
  assert!(
    environment
      .provider
      .requests
      .lock()
      .await
      .iter()
      .all(|request| request.starts_with("GET ")),
    "workflow/mapping publication and import cannot authorize a provider write"
  );
  *environment.provider.reporting.lock().await = Some(source_provider::reporting::Issue::new(&["human-owned"]));
  let result = environment.browser_phase("issue-source-status.spec.ts", &project).await;
  if let Err(error) = result {
    environment.stop().await;
    panic!("{error}");
  }
  let pull = environment.browser_phase("issue-source-pull.spec.ts", &project).await;
  environment.stop().await;
  pull.unwrap();
  {
    let requests = environment.provider.requests.lock().await;
    let writes: Vec<_> = requests.iter().filter(|r| !r.starts_with("GET ")).collect();
    assert_eq!(
      writes.len(),
      1,
      "one exact native state effect, without a duplicate or unrelated label mutation"
    );
    assert!(writes[0].starts_with("PATCH /api/v3/repos/owner/tickets/issues/17 HTTP/"));
    let body: Value = serde_json::from_str(writes[0].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body, json!({"state":"closed"}));
    let state = environment.provider.reporting.lock().await;
    assert_eq!(state.as_ref().unwrap().state, "closed");
    assert_eq!(
      state.as_ref().unwrap().labels,
      ["human-owned".to_owned()].into_iter().collect()
    );
  }
  environment.record(&project, "packaged-status.json").await;
  let metadata: Value = serde_json::from_slice(
    &fs::read(environment.evidence.join("packaged-status.json"))
      .expect("retain status qualification separately from Source-only evidence"),
  )
  .unwrap();
  assert_eq!(metadata["reporter_installed"], true);
  assert_eq!(metadata["project_id"], project);
}

/// Installed writing support cannot substitute for a connection's reviewed read-only purposes.
pub(super) async fn read_only() {
  let mut environment = source_environment::SourceEnvironment::start_with_reporter().await;
  let project =
    source_setup::enroll_reader_with_history(&environment.client, &environment.origin, &environment.installation).await;
  let result = environment.browser_phase("project-source.spec.ts", &project).await;
  environment.stop().await;
  result.unwrap();
  assert!(
    environment
      .provider
      .requests
      .lock()
      .await
      .iter()
      .all(|request| request.starts_with("GET ")),
    "a read-only connection cannot publish an effect through an installed Reporter"
  );
  environment.record(&project, "packaged-read-only.json").await;
}
