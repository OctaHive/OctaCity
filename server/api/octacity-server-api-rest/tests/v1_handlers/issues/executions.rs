use super::*;
#[path = "journal.rs"]
mod journal;
use octacity_server_api_rest::v1::IssueExecutionManagementApplication;
use octacity_server_application::{
  GetIssueExecutionQuery, IssueExecutionHistoryPage, IssueExecutionOutcome, IssueExecutionRequest,
  ListIssueExecutionsQuery, PublishIssueExecutionCommand, RequestIssueExecutionCommand, StoreError,
  StoreMutationDisposition,
};

fn read_routes() -> axum::Router {
  execution_routes(IssueExecutionManagementApplication::new(Arc::new(
    AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(ExecutionRead(false))),
  )))
}
fn execution_routes(services: IssueExecutionManagementApplication) -> axum::Router {
  let peer = Arc::new(RecordingApplication::default());
  management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_issue_executions(services),
  )
}
fn publication_routes<P>(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  publisher: Arc<P>,
  admitted: bool,
) -> axum::Router
where
  P: ManagementCommandUseCase<PublishIssueExecutionCommand, Error = ApplicationError> + 'static,
{
  execution_routes(
    IssueExecutionManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(ExecutionRead(admitted)),
    )))
    .with_publication(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), Arc::new(ExecutionStart))),
      Arc::new(AuthorizedCommandHandler::new(policy, publisher)),
      std::time::Duration::from_millis(5),
    ),
  )
}

const ISSUE: &str = "60000000-0000-4000-8000-000000000001";
const EXECUTION: &str = "60000000-0000-4000-8000-000000000002";
fn execution() -> IssueExecutionRequest {
  serde_json::from_value(serde_json::json!({
    "id":EXECUTION,"intent":{"project_id":PROJECT,"issue_id":ISSUE,"expected_version":2,
      "target":{"configuration_id":"60000000-0000-4000-8000-000000000003","configuration_version":3,
        "repository_id":"60000000-0000-4000-8000-000000000004","repository_version":4,
        "source":{"kind":"default_reference"},"mapping":"title_description_acceptance_v1"}},
    "task_digest":"11".repeat(32),"task_size":32,"acceptance_digest":"22".repeat(32),"acceptance_size":16,
    "title_digest":"33".repeat(32),"description_digest":"44".repeat(32),"requested_at":10,"association":null
  }))
  .unwrap()
}
struct ExecutionRead(bool);
#[async_trait]
impl ManagementQueryUseCase<GetIssueExecutionQuery> for ExecutionRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIssueExecutionQuery,
  ) -> Result<IssueExecutionRequest, ApplicationError> {
    assert_eq!(query.project_id.to_string(), PROJECT);
    assert_eq!(query.issue_id.to_string(), ISSUE);
    assert_eq!(query.request_id.to_string(), EXECUTION);
    Ok(if self.0 { admitted_execution() } else { execution() })
  }
}
#[tokio::test]
async fn issue_execution_http_status_exposes_frozen_pending_metadata_without_publication() {
  let routes = read_routes();
  let response = routes
    .oneshot(empty_request(
      "GET",
      &format!("{}/{ISSUE}/executions/{EXECUTION}", root()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let body = document(response).await;
  assert_eq!(body["state"], "pending");
  assert_eq!(body["id"], EXECUTION);
  assert_eq!(body["intent"]["expected_version"], 2);
  assert_eq!(body["intent"]["target"]["configuration_version"], 3);
  assert_eq!(body["run_url"], serde_json::Value::Null);
  assert!(body.get("documents").is_none());
  let schema = openapi_document();
  assert_json_matches_component(&schema, "IssueExecutionResource", &body);
}

fn admitted_execution() -> IssueExecutionRequest {
  let mut value = serde_json::to_value(execution()).unwrap();
  value["association"] = serde_json::json!({"project_id":PROJECT,"issue_id":ISSUE,"request_id":EXECUTION,
    "requirements_version":2,"work_id":"60000000-0000-4000-8000-000000000005","run_id":"60000000-0000-4000-8000-000000000006"});
  serde_json::from_value(value).unwrap()
}
struct ExecutionStart;
#[async_trait]
impl ManagementCommandUseCase<RequestIssueExecutionCommand> for ExecutionStart {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: RequestIssueExecutionCommand,
  ) -> Result<IssueExecutionOutcome, ApplicationError> {
    assert_eq!(command.intent.expected_version.get(), 2);
    assert_eq!(command.idempotency_key.as_str(), "start-reviewed");
    assert!(command.intent.classification.is_some());
    Ok(IssueExecutionOutcome {
      disposition: StoreMutationDisposition::Applied,
      request: IssueExecutionRequest {
        intent: command.intent,
        ..execution()
      },
    })
  }
}
#[async_trait]
impl ManagementCommandUseCase<PublishIssueExecutionCommand> for ExecutionStart {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishIssueExecutionCommand,
  ) -> Result<IssueExecutionRequest, ApplicationError> {
    assert_eq!(command.request_id.to_string(), EXECUTION);
    Ok(admitted_execution())
  }
}
fn execution_body() -> String {
  issue_execution_body()
}
#[tokio::test]
async fn issue_execution_http_start_returns_durable_receipt_and_existing_run_url() {
  let routes = publication_routes(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(ExecutionStart),
    false,
  );
  let response = routes
    .oneshot(json_request(
      "POST",
      &format!("{}/{ISSUE}/executions", root()),
      "start-reviewed",
      Some("\"2\""),
      &execution_body(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(
    response.headers()["location"],
    format!("{}/{ISSUE}/executions/{EXECUTION}", root())
  );
  assert_eq!(response.headers()["cache-control"], "no-store");
  let value = document(response).await;
  assert_eq!(value["resource"]["state"], "admitted");
  assert_eq!(
    value["resource"]["run_url"],
    "/api/v1/factory-runs/60000000-0000-4000-8000-000000000006"
  );
  let schema = openapi_document();
  assert_json_matches_component(&schema, "IssueExecutionMutationResponse", &value);
  let operation = &schema["paths"]["/api/v1/projects/{project_id}/issues/{issue_id}/executions"]["post"];
  for code in ["200", "201", "202", "409", "428"] {
    assert!(operation["responses"].get(code).is_some(), "missing {code}");
  }
}

enum PendingPublication {
  Unavailable,
  Wait,
  Busy,
}
#[async_trait]
impl ManagementCommandUseCase<PublishIssueExecutionCommand> for PendingPublication {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: PublishIssueExecutionCommand,
  ) -> Result<IssueExecutionRequest, ApplicationError> {
    match self {
      Self::Wait => std::future::pending().await,
      Self::Unavailable => Err(ApplicationError::unavailable()),
      Self::Busy => Err(ApplicationError::Store(StoreError::Conflict {
        entity: serde_json::from_value(serde_json::json!("issue")).unwrap(),
      })),
    }
  }
}
#[tokio::test]
async fn issue_execution_http_retains_pending_request_when_shared_publication_is_unavailable() {
  pending_response(PendingPublication::Unavailable, false).await;
}
#[tokio::test]
async fn issue_execution_http_returns_pending_after_bounded_publication_wait() {
  pending_response(PendingPublication::Wait, false).await;
}
#[tokio::test]
async fn issue_execution_http_returns_pending_when_original_publication_is_owned_elsewhere() {
  pending_response(PendingPublication::Busy, false).await;
}
#[tokio::test]
async fn issue_execution_http_reads_durable_association_after_unknown_publication_outcome() {
  pending_response(PendingPublication::Unavailable, true).await;
}
async fn pending_response(failure: PendingPublication, admitted: bool) {
  let routes = publication_routes(Arc::new(TrustedNetworkManagementPolicy), Arc::new(failure), admitted);
  let response = routes
    .oneshot(json_request(
      "POST",
      &format!("{}/{ISSUE}/executions", root()),
      "start-reviewed",
      Some("\"2\""),
      &execution_body(),
    ))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    if admitted {
      StatusCode::CREATED
    } else {
      StatusCode::ACCEPTED
    }
  );
  assert_eq!(
    response.headers()["location"],
    format!("{}/{ISSUE}/executions/{EXECUTION}", root())
  );
  let value = document(response).await;
  assert_eq!(
    value["resource"]["state"],
    if admitted { "admitted" } else { "pending" }
  );
  assert_eq!(value["resource"]["association"].is_null(), !admitted);
  assert_json_matches_component(&openapi_document(), "IssueExecutionMutationResponse", &value);
}

#[async_trait]
impl ManagementQueryUseCase<ListIssueExecutionsQuery> for ExecutionRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: ListIssueExecutionsQuery,
  ) -> Result<IssueExecutionHistoryPage, ApplicationError> {
    Ok(IssueExecutionHistoryPage {
      items: vec![execution()],
      next_cursor: None,
      observation: "request-watermark-v1",
      watermark: 7,
      issue_version: execution().intent.expected_version,
    })
  }
}
#[tokio::test]
async fn issue_execution_http_history_is_bounded_metadata_with_declared_observation() {
  let routes = read_routes();
  let uri = format!("{}/{ISSUE}/executions", root());
  let response = routes
    .clone()
    .oneshot(empty_request("GET", &format!("{uri}?limit=2"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page = document(response).await;
  assert_eq!(page["observation"], "request-watermark-v1");
  assert_eq!(page["watermark"], 7);
  assert_eq!(page["items"][0]["state"], "pending");
  assert_json_matches_component(&openapi_document(), "IssueExecutionHistoryPage", &page);
  for query in ["limit=0", "limit=201", "after=garbage", "unsupported=1"] {
    assert_eq!(
      routes
        .clone()
        .oneshot(empty_request("GET", &format!("{uri}?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
}

#[test]
fn issue_execution_http_generated_contract_documents_review_headers_and_exact_status_location() {
  let schema = openapi_document();
  let base = "/api/v1/projects/{project_id}/issues/{issue_id}/executions";
  let operation = &schema["paths"][base]["post"];
  assert_required_header(operation, "If-Match", true);
  assert_required_header(operation, "Idempotency-Key", true);
  for status in ["200", "201", "202"] {
    assert_eq!(
      operation["responses"][status]["headers"]["Location"]["schema"]["format"],
      "uri-reference"
    );
  }
  for name in [
    "RequestIssueExecutionRequest",
    "IssueExecutionTarget",
    "IssueExecutionIntent",
    "IssueExecutionResource",
    "IssueExecutionHistoryPage",
  ] {
    assert_eq!(schema["components"]["schemas"][name]["additionalProperties"], false);
  }
  assert_eq!(
    schema["components"]["schemas"]["IssueExecutionHistoryPage"]["properties"]["items"]["maxItems"],
    200
  );
  let status = &schema["paths"][format!("{base}/{{request_id}}")]["get"];
  assert!(
    status["parameters"]
      .as_array()
      .unwrap()
      .iter()
      .any(|p| p["name"] == "request_id" && p["required"] == true)
  );
}

#[tokio::test]
async fn issue_execution_http_execute_permission_is_separate_from_manual_edit_and_reads() {
  let policy = Arc::new(IssueRights(Arc::new(Mutex::new([ManagementAction::Execute].into()))));
  let manual = issue_router(policy.clone()).await;
  let response = manual
    .clone()
    .oneshot(json_request("POST", &root(), "manual", None, &draft().to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let issue = document(response).await["resource"]["id"].as_str().unwrap().to_owned();
  assert_eq!(
    manual
      .oneshot(json_request(
        "PATCH",
        &format!("{}/{issue}", root()),
        "edit",
        Some("\"1\""),
        &draft().to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let routes = publication_routes(policy, Arc::new(ExecutionStart), false);
  let response = routes
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/{ISSUE}/executions", root()),
      "start-reviewed",
      Some("\"2\""),
      &execution_body(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::FORBIDDEN);
  assert_eq!(
    routes
      .oneshot(empty_request(
        "GET",
        &format!("{}/{ISSUE}/executions/{EXECUTION}", root()),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
}

#[tokio::test]
async fn issue_execution_http_rejects_unreviewed_or_open_ended_commands_before_dispatch() {
  let routes = publication_routes(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(ExecutionStart),
    false,
  );
  let path = format!("{}/{ISSUE}/executions", root());
  assert_eq!(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "start-reviewed", None, &execution_body()))
      .await
      .unwrap()
      .status(),
    StatusCode::PRECONDITION_REQUIRED
  );
  let body: serde_json::Value = serde_json::from_str(&execution_body()).unwrap();
  for invalid in [
    serde_json::json!({"target":body["target"]}),
    serde_json::json!({"target":body["target"],"classification":null}),
    serde_json::json!({"target":body["target"],"classification":body["classification"],"run_id":EXECUTION}),
    serde_json::json!({"target":body["target"],"classification":{"priority":101,"risk":"medium","metadata":{}}}),
  ] {
    assert_eq!(
      routes
        .clone()
        .oneshot(json_request(
          "POST",
          &path,
          "start-reviewed",
          Some("\"2\""),
          &invalid.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  assert_json_matches_component(&openapi_document(), "RequestIssueExecutionRequest", &body);
}
