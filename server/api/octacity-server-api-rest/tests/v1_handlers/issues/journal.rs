use super::*;
use octacity_server_application::{
  FactoryDiagnosticCollection, GetIssueExecutionJournalEventQuery, IssueExecutionJobProjection,
  IssueExecutionJournalEvents, IssueExecutionJournalPage, IssueExecutionProjection, IssueJobLogReference,
  IssueJobProjection, IssueJournalEvent, IssueJournalEventFacts, IssueRequirementsRelation, IssueResultCoverage,
  IssueRunObservation, JobEventPageProjection, JobEventProjection, ListIssueExecutionJournalQuery,
  ManagementResourceKind, ManagementVisibility, ReadIssueExecutionJournalEventsQuery,
};
const JOB: &str = "60000000-0000-4000-8000-000000000009";

#[tokio::test]
async fn requestless_work_events_reuse_exact_authorized_journal_contracts() {
  let work = admitted_execution().association.unwrap().work_id;
  let base = format!("{}/{ISSUE}/work/{work}/journal/events", root());
  let selector = format!("build_execution={}&job_id={JOB}", "ab".repeat(32));
  let response = routes()
    .oneshot(empty_request("GET", &format!("{base}?{selector}"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page = document(response).await;
  let detail = format!("{base}/1?{selector}");
  assert_eq!(page["events"][0]["detail_url"], detail);
  let response = routes().oneshot(empty_request("GET", &detail, None)).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    document(response).await["items"][0]["payload"]["text"],
    "authorized detail"
  );
  for path in [format!("{base}?{selector}"), detail] {
    let response = routes_with_policy(Arc::new(DenyJournalContent))
      .oneshot(empty_request("GET", &path, None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
  }
}
struct JournalRead;
#[async_trait]
impl ManagementQueryUseCase<ListIssueExecutionJournalQuery> for JournalRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListIssueExecutionJournalQuery,
  ) -> Result<IssueExecutionJournalPage, ApplicationError> {
    assert_eq!(query.execution().request_id.to_string(), EXECUTION);
    assert_eq!(query.collection(), FactoryDiagnosticCollection::FlowRuns);
    Ok(IssueExecutionJournalPage {
      execution: IssueExecutionProjection {
        request: execution(),
        issue_version: execution().intent.expected_version,
        requirements: IssueRequirementsRelation::Current,
        run: IssueRunObservation::Preparing,
      },
      records: None,
    })
  }
}
fn job() -> IssueExecutionJobProjection {
  let build_id = "60000000-0000-4000-8000-000000000007".parse().unwrap();
  let attempt_id = "60000000-0000-4000-8000-000000000008".parse().unwrap();
  IssueExecutionJobProjection {
    association: admitted_execution().association.unwrap(),
    execution: serde_json::from_value(serde_json::json!({"id":"ab".repeat(32),"operation_id":"cd".repeat(32),
      "node_attempt_id":"60000000-0000-4000-8000-000000000010","build_id":build_id,"attempt_id":attempt_id,"jobs":[JOB]})).unwrap(),
    profile: None,
    job: IssueJobProjection { id: JOB.parse().unwrap(), state: serde_json::from_value(serde_json::json!("ready")).unwrap(),
      version: serde_json::from_value(serde_json::json!(1)).unwrap(), created_at: Timestamp::from_unix_millis(10).unwrap(),
      updated_at: Timestamp::from_unix_millis(11).unwrap(), assignment: None, terminal: None },
    log: IssueJobLogReference { build_id, attempt_id, job_id: JOB.parse().unwrap(), event_cursor: 1 },
  }
}
#[async_trait]
impl ManagementQueryUseCase<ReadIssueExecutionJournalEventsQuery> for JournalRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ReadIssueExecutionJournalEventsQuery,
  ) -> Result<IssueExecutionJournalEvents, ApplicationError> {
    assert_eq!(query.job().job_id.to_string(), JOB);
    assert_eq!(query.job().build_execution.to_string(), "ab".repeat(32));
    assert_eq!(
      query.job().execution.work_id,
      admitted_execution().association.unwrap().work_id
    );
    Ok(IssueExecutionJournalEvents {
      job: job(),
      observation: "issue-job-event-sequence-v1",
      coverage: IssueResultCoverage::Partial,
      cursor: "retained-event-boundary".into(),
      job_stale: false,
      gaps: vec![],
      events: vec![IssueJournalEvent {
        sequence: 1,
        occurred_at_unix_ms: 101,
        facts: IssueJournalEventFacts::Operation {
          observation: serde_json::from_value(serde_json::json!({
            "operation_id":"ab".repeat(32),"phase":"completed","observed_at_unix_ms":77,
            "operation":{"kind":"command","exit_code":17,"failed":true}
          }))
          .unwrap(),
          runner_sequence: 11,
          run_id: Some(2),
          step_id: Some(9),
          command_id: Some("command-1".parse().unwrap()),
        },
      }],
    })
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetIssueExecutionJournalEventQuery> for JournalRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIssueExecutionJournalEventQuery,
  ) -> Result<JobEventPageProjection, ApplicationError> {
    assert_eq!(query.job().job_id.to_string(), JOB);
    assert_eq!(query.sequence(), 1);
    Ok(JobEventPageProjection {
      cursor: 1,
      events: vec![JobEventProjection {
        sequence: 1,
        kind: "progress".into(),
        occurred_at_unix_ms: 101,
        payload: serde_json::json!({"text":"authorized detail"}),
      }],
    })
  }
}
fn routes() -> axum::Router {
  routes_with_policy(Arc::new(TrustedNetworkManagementPolicy))
}
fn routes_with_policy(policy: Arc<dyn ManagementAuthorizationPolicy>) -> axum::Router {
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), Arc::new(JournalRead)));
  execution_routes(
    IssueExecutionManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
      policy,
      Arc::new(ExecutionRead(true)),
    )))
    .with_journal(queries.clone(), queries),
  )
}

struct DenyJournalContent;
#[async_trait]
impl ManagementAuthorizationPolicy for DenyJournalContent {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::JobEvent {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}

#[tokio::test]
async fn issue_journal_http_requires_current_event_rights_for_metadata_and_detail() {
  let routes = routes_with_policy(Arc::new(DenyJournalContent));
  let base = format!("{}/{ISSUE}/executions/{EXECUTION}/journal", root());
  let hierarchy = routes.clone().oneshot(empty_request("GET", &base, None)).await.unwrap();
  assert_eq!(hierarchy.status(), StatusCode::OK);
  let selector = format!("build_execution={}&job_id={JOB}", "ab".repeat(32));
  for path in [
    format!("{base}/events?{selector}"),
    format!("{base}/events/1?{selector}"),
  ] {
    let response = routes.clone().oneshot(empty_request("GET", &path, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!document(response).await.to_string().contains("authorized detail"));
  }
}

#[tokio::test]
async fn issue_journal_http_invalid_sequence_uses_the_common_error_contract() {
  let base = format!("{}/{ISSUE}/executions/{EXECUTION}/journal/events", root());
  let selector = format!("build_execution={}&job_id={JOB}", "ab".repeat(32));
  for sequence in ["-1", "unknown", "18446744073709551616"] {
    let response = routes()
      .oneshot(empty_request("GET", &format!("{base}/{sequence}?{selector}"), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(document(response).await["code"], "invalid_request");
  }
}
#[tokio::test]
async fn issue_journal_http_publishes_exact_safe_pages_and_separate_no_store_detail() {
  let routes = routes();
  let base = format!("{}/{ISSUE}/executions/{EXECUTION}/journal", root());
  let response = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{base}?collection=flow-runs&limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let value = document(response).await;
  assert_eq!(value["execution"]["run"]["availability"], "preparing");
  assert!(value["records"].is_null());
  assert_json_matches_component(&openapi_document(), "IssueExecutionJournalPage", &value);
  let selector = format!("build_execution={}&job_id={JOB}", "ab".repeat(32));
  let response = routes
    .clone()
    .oneshot(empty_request("GET", &format!("{base}/events?{selector}&limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let value = document(response).await;
  assert_eq!(value["events"][0]["facts"]["observation"]["operation"]["exit_code"], 17);
  assert!(!value.to_string().contains("authorized detail"));
  assert_eq!(value["job"]["log"]["events_url"], format!("/api/v1/jobs/{JOB}/events"));
  let detail_url = format!("{base}/events/1?{selector}");
  assert_eq!(value["events"][0]["detail_url"], detail_url);
  assert_json_matches_component(&openapi_document(), "IssueExecutionJournalEvents", &value);
  let response = routes
    .clone()
    .oneshot(empty_request("GET", &detail_url, None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let value = document(response).await;
  assert_eq!(value["items"][0]["payload"]["text"], "authorized detail");
  assert_json_matches_component(&openapi_document(), "IssueExecutionJournalEventDetail", &value);
  for path in [
    format!("{base}?limit=0"),
    format!("{base}?collection=unknown"),
    format!("{base}?unexpected=1"),
    format!("{base}/events?{selector}&limit=101"),
    format!("{base}/events?{selector}&after=bad"),
    format!("{base}/events?job_id={JOB}"),
    format!("{base}/events/0?{selector}"),
    format!("{base}/events/1?{selector}&limit=2"),
  ] {
    assert_eq!(
      routes
        .clone()
        .oneshot(empty_request("GET", &path, None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST,
      "{path}"
    );
  }
}
