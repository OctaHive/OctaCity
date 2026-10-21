//! Released HTTP composition over disposable PostgreSQL and an optional existing review reader.
use super::*;
use octacity_server_application::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[derive(Default)]
struct InstalledReviews {
  reads: AtomicUsize,
  revoked: AtomicBool,
  failed: AtomicBool,
}
#[async_trait::async_trait]
impl IssueCodeReviewObservations for InstalledReviews {
  async fn visibility(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
  ) -> Result<IssueCodeReviewRepositoryVisibility, ApplicationError> {
    Ok(IssueCodeReviewRepositoryVisibility::all())
  }
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    _: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    if self.revoked.load(Ordering::SeqCst) {
      Err(ApplicationError::capability_unavailable())
    } else {
      Ok(())
    }
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    self.authorize(context, project, selection).await?;
    self.reads.fetch_add(1, Ordering::SeqCst);
    if self.failed.load(Ordering::SeqCst) {
      return Err(ApplicationError::SourceDependency {
        class: WorkSourceFailure::Transient,
        retry_after_millis: None,
      });
    }
    Ok(serde_json::from_value(json!({"selection":selection,"observed_at":1,"title":"Retained MR","number":"MR-42","url":null,"author":null,"state":"open","draft":null,"head":"original-head","base":null,"changed_files":null,"additions":null,"deletions":null,"review":null,"checks":null,"check_details":{"source":"external_ci","head":"original-head","observed_at":1,"has_more":false,"items":[{"id":"native-ci","name":"contract","producer":null,"state":"skipped","required":null}]}})).unwrap())
  }
}
#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL pointing to disposable PostgreSQL"]
async fn runtime_installs_review_http_independently_and_preserves_retained_data_after_outage_and_restart() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let settings = config::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9");
  let runtime = ServerRuntime::start(settings.clone()).await.unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  let project = accepted(
    client
      .post(format!("{origin}/api/v1/projects"))
      .header("idempotency-key", "project")
      .json(&json!({"name":"Manual review task"}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await;
  let project = project["resource"]["id"].as_str().unwrap();
  let issue = accepted(
    client
      .post(format!("{origin}/api/v1/projects/{project}/issues"))
      .header("idempotency-key", "manual")
      .json(&json!({"title":"Manual task","description":""}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await["resource"]
    .clone();
  for suffix in [
    format!(
      "checks?work_id=60000000-0000-4000-8000-000000000001&record={}",
      "ab".repeat(32)
    ),
    "artifacts?work_id=60000000-0000-4000-8000-000000000001".into(),
    "artifacts/60000000-0000-4000-8000-000000000002/content?work_id=60000000-0000-4000-8000-000000000001&mode=preview"
      .into(),
  ] {
    let response = client
      .get(format!(
        "{origin}/api/v1/projects/{project}/issues/{}/results/{suffix}",
        issue["id"].as_str().unwrap()
      ))
      .send()
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      reqwest::StatusCode::NOT_FOUND,
      "runtime installs read services even without Factory or provider installations"
    );
  }
  let path = format!(
    "/api/v1/projects/{project}/issues/{}/code-reviews",
    issue["id"].as_str().unwrap()
  );
  let empty = accepted(
    client.get(format!("{origin}{path}")).send().await.unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(empty["items"], json!([]));
  runtime.shutdown().await.unwrap();
  let reader = Arc::new(InstalledReviews::default());
  let integrations = super::super::IssueRuntimeIntegrations {
    code_reviews: Some(reader.clone()),
    ..Default::default()
  };
  let runtime = ServerRuntime::start_with_issue_integrations(settings.clone(), integrations)
    .await
    .unwrap();
  let origin = format!("http://{}", runtime.management_addr());
  let pin = |identity: &str| json!({"identity":identity,"version":"v1","digest":"01".repeat(32)});
  let selection = json!({"source":{"installation":pin("code-forge"),"capability":pin("review"),"configuration":pin("review-profile"),"resource":pin("code")},"item":{"resource":"native-code","item":"native-mr-42"}});
  let attached = accepted(
    client
      .post(format!("{origin}{path}"))
      .header("idempotency-key", "attach")
      .header("if-match", "\"1\"")
      .json(&json!({"selection":selection}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await;
  reader.failed.store(true, Ordering::SeqCst);
  assert_eq!(
    client
      .post(format!(
        "{origin}{path}/{}/observations",
        attached["association"]["id"].as_str().unwrap()
      ))
      .header("idempotency-key", "outage")
      .json(&json!({}))
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::SERVICE_UNAVAILABLE
  );
  runtime.shutdown().await.unwrap();
  reader.revoked.store(true, Ordering::SeqCst);
  let runtime = ServerRuntime::start_with_issue_integrations(
    settings,
    super::super::IssueRuntimeIntegrations {
      code_reviews: Some(reader.clone()),
      ..Default::default()
    },
  )
  .await
  .unwrap();
  let origin = format!("http://{}", runtime.management_addr());
  let retained = accepted(
    client.get(format!("{origin}{path}")).send().await.unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(retained["items"][0]["association"], attached["association"]);
  assert_eq!(retained["items"][0]["observation"]["head"], "original-head");
  assert_eq!(retained["items"][0]["availability"], "blocked");
  let checks = accepted(
    client
      .get(format!(
        "{origin}{path}/{}/checks",
        attached["association"]["id"].as_str().unwrap()
      ))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(checks["items"][0]["head"], "original-head");
  assert_eq!(checks["items"][0]["checks"]["items"][0]["state"], "skipped");
  assert_eq!(checks["availability"], "blocked");
  let current = accepted(
    client
      .get(format!(
        "{origin}/api/v1/projects/{project}/issues/{}",
        issue["id"].as_str().unwrap()
      ))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(current, issue);
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    2,
    "only explicit attach and refresh contact the installed reader"
  );
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}
