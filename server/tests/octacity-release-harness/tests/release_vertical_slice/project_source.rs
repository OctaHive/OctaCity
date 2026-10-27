//! Source-only browser journey over the shared packaged integration environment.
use super::*;

pub(super) async fn run() {
  let mut environment = source_environment::SourceEnvironment::start().await;
  let project =
    source_setup::enroll_reader_with_history(&environment.client, &environment.origin, &environment.installation).await;
  let result = environment.browser("project-source.spec.ts", &project).await;
  // Failed qualification also shuts down both packaged processes before reporting.
  environment.stop().await;
  result.unwrap();
  let records: Vec<_> = fs::read_dir(environment.evidence.join("browser-results"))
    .unwrap()
    .filter_map(|entry| entry.ok().map(|entry| entry.path().join("canonical-issue.json")))
    .filter(|path| path.is_file())
    .collect();
  assert_eq!(
    records.len(),
    1,
    "retain one public canonical Issue observation with the browser evidence"
  );
  let observed: Value = serde_json::from_slice(&fs::read(&records[0]).unwrap()).unwrap();
  assert_eq!(observed["project_id"], project);
  assert_eq!(observed["native_item"], json!({"resource":"1234","item":"9001"}));
  {
    let requests = environment.provider.requests.lock().await;
    assert!(
      requests
        .iter()
        .any(|request| request.starts_with("GET /api/v3/repos/owner/tickets/issues?"))
    );
    assert!(
      requests.iter().all(|request| request.starts_with("GET ")),
      "import/preview must never mutate the provider"
    );
  }
  environment.record(&project, "packaged-source.json").await;
}
