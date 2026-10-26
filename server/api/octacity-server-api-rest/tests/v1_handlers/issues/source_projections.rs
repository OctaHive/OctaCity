//! Wire contract uses the optional safe Reporter query, never a provider call.
use super::*;
pub(super) struct Projections;
#[async_trait]
impl ManagementQueryUseCase<ListIssueSourceProjectionsQuery> for Projections {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: ListIssueSourceProjectionsQuery,
  ) -> Result<IssueSourceProjectionPage, ApplicationError> {
    let identity = "0199a6f4-d56c-7440-9aa2-6a320f862796";
    let row = IssueSourceProjection {
      operation_id: FactoryDigest::from_bytes([0x11; 32]),
      record_id: FactoryDigest::from_bytes([0x22; 32]),
      issue_version: IssueVersion::INITIAL,
      mapping: profile().source.configuration,
      native_correlation: FactoryDigest::from_bytes([0x33; 32]),
      workflow: IssueWorkflowPin {
        id: identity.parse().unwrap(),
        version: IssueWorkflowVersion::INITIAL,
      },
      status_id: identity.parse().unwrap(),
      effects: Some(
        serde_json::from_value(serde_json::json!({"status":identity,"add":["custom-ready"],"remove":[],"state":null}))
          .unwrap(),
      ),
      pipeline: Some(
        serde_json::from_value(serde_json::json!({"issue_id":identity,"requirements_version":1,
        "work_id":identity,"run_id":identity,"flow_run_id":identity,"cycle_id":identity,"node_attempt_id":identity,
        "operation_id":"44".repeat(32),"candidate":null,"outcome":"eligible"}))
        .unwrap(),
      ),
      resolves: None,
      accepted_at: Timestamp::from_unix_millis(100).unwrap(),
      state: octacity_server_application::FactoryOutboxState::Failed,
      failure: Some(octacity_server_application::FactoryOutboxFailure::Conflict),
      attempt: 0,
      recorded_at: Timestamp::from_unix_millis(120).unwrap(),
      available_at: Timestamp::from_unix_millis(120).unwrap(),
    };
    Ok(IssueSourceProjectionPage {
      items: vec![row],
      next_cursor: None,
      observation: "live-reporter-v1",
      observed_at: Timestamp::from_unix_millis(123).unwrap(),
    })
  }
}
#[tokio::test]
async fn issue_source_http_projections_are_bounded_no_store_retained_reads() {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let router = source_router_with_projections(
    Arc::new(NoSource),
    vec![],
    policy.clone(),
    None,
    Some(Arc::new(AuthorizedQueryHandler::new(policy, Arc::new(Projections)))),
  )
  .await;
  let path = format!(
    "{}/0199a6f4-d56c-7440-9aa2-6a320f862796/synchronization/projections",
    root()
  );
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let value = document(response).await;
  assert_eq!(value["observed_at"], 123);
  assert_eq!(value["observation"], "live-reporter-v1");
  assert_eq!(value["items"][0]["failure"], "conflict");
  assert_eq!(value["items"][0]["pipeline"]["outcome"], "eligible");
  assert_json_matches_component(&openapi_document(), "IssueSourceProjectionPage", &value);
  for query in ["limit=0", "limit=201", "after=invalid", "force=true"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
}
