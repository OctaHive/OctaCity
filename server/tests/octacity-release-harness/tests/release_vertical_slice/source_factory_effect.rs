//! Lose native acknowledgement, then restart the normal Server before its retry observes the effect.
use super::*;

pub(super) async fn run(environment: &mut source_environment::SourceEnvironment, project: &str, configuration: &str) {
  let mut native = source_provider::reporting::Issue::new(&["human-owned"]);
  native.fault = Some(("PATCH", source_provider::reporting::LostResponse::UnavailableAfterApply));
  native.confirmation = Some(source_provider::Reply {
    status: 503,
    ..source_provider::Reply::json(json!({"message":"private-provider-error"}))
  });
  *environment.provider.reporting.lock().await = Some(native);
  let evidence = environment.evidence.join("factory-issue-execution");
  fs::create_dir_all(&evidence).unwrap();
  let (origin, project, configuration) = (environment.origin.clone(), project.to_owned(), configuration.to_owned());
  let browser = tokio::spawn(async move {
    browser::run_browser_scenario(
      &required_path("OCTACITY_RELEASE_UI_ROOT", true),
      &origin,
      "factory-issue-execution.spec.ts",
      &[
        ("OCTACITY_RELEASE_PROJECT_ID", &project),
        ("OCTACITY_RELEASE_CONFIGURATION_ID", &configuration),
      ],
      &evidence,
    )
    .await
  });
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    if environment.provider.reporting.lock().await.as_ref().unwrap().state == "closed" {
      break;
    }
    if browser.is_finished() {
      panic!(
        "browser failed before the native effect: {}",
        browser.await.unwrap().unwrap_err()
      );
    }
    assert!(
      Instant::now() < deadline,
      "the configured action must reach its exact native effect"
    );
    sleep(Duration::from_millis(5)).await;
  }
  environment.restart().await;
  browser.await.unwrap().unwrap();
  let requests = environment.provider.requests.lock().await;
  let writes: Vec<_> = requests.iter().filter(|r| !r.starts_with("GET ")).collect();
  assert_eq!(
    writes.len(),
    1,
    "restart must observe the committed effect, never apply it twice or add another producer"
  );
  assert!(writes[0].starts_with("PATCH /api/v3/repos/owner/tickets/issues/17 HTTP/"));
  let body: Value = serde_json::from_str(writes[0].split_once("\r\n\r\n").unwrap().1).unwrap();
  assert_eq!(body, json!({"state":"closed"}));
  assert_eq!(
    environment.provider.reporting.lock().await.as_ref().unwrap().labels,
    ["human-owned".to_owned()].into_iter().collect(),
    "unmanaged labels survive"
  );
}
