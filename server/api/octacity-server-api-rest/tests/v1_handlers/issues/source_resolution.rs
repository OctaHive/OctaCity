//! HTTP resolution forwards exact reviewed versions and receipt, with no force-write surface.
use super::*;
type MappingOutcome = <ResolveIssueSourceConflictCommand as Command>::Outcome;
#[derive(Default)]
struct Resolver(Mutex<Vec<ResolveIssueSourceConflictCommand>>);
#[async_trait]
impl ManagementCommandUseCase<ResolveIssueSourceConflictCommand> for Resolver {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: ResolveIssueSourceConflictCommand,
  ) -> Result<MappingOutcome, ApplicationError> {
    self.0.lock().unwrap().push(command.clone());
    Ok(MappingOutcome {
      disposition: StoreMutationDisposition::Applied,
      projection: Some(FactoryDigest::from_bytes([0x33; 32])),
      mapping: PublishedIssueSourceMapping {
        project_id: command.intent.mapping.project_id,
        issue_id: command.intent.mapping.issue_id,
        version: IssueSourceMappingVersion::new(2).unwrap(),
        definition: command.intent.mapping.definition,
        published_at: command.resolved_at,
      },
    })
  }
}
#[tokio::test]
async fn issue_source_http_resolution_requires_reviewed_versions_and_returns_one_new_intent() {
  let peer = Arc::new(Resolver::default());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let router = source_router_with_resolution(
    Arc::new(NoSource),
    vec![],
    policy.clone(),
    Some(Arc::new(AuthorizedCommandHandler::new(policy, peer.clone()))),
  )
  .await;
  let issue = "0199a6f4-d56c-7440-9aa2-6a320f862796";
  let path = format!("{}/{issue}/synchronization/resolve", root());
  let pin = serde_json::to_value(profile().source.installation).unwrap();
  let body = serde_json::json!({"review":{"operation_id":"11".repeat(32),"expected_record":"22".repeat(32)},
    "mapping":{"expected_mapping":pin,"definition":{"source":profile().source,"reporter":profile().source,
      "push_enabled":true,"fields":{"status":"octacity_owned_push"},"status":null}}});
  let missing = router
    .clone()
    .oneshot(json_request("POST", &path, "resolve-missing", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
  assert!(peer.0.lock().unwrap().is_empty());
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "resolve-one",
      Some("\"2\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let result = document(response).await;
  assert_eq!(result["projection"], "33".repeat(32));
  assert_json_matches_component(&openapi_document(), "IssueSourceResolutionResponse", &result);
  {
    let captured = peer.0.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].intent.mapping.expected_issue_version.get(), 2);
    assert_eq!(captured[0].intent.mapping.issue_id.to_string(), issue);
    assert_eq!(captured[0].intent.review.expected_record.to_string(), "22".repeat(32));
  }
  let mut unsafe_body = body.clone();
  unsafe_body["force"] = serde_json::json!(true);
  let rejected = router
    .oneshot(json_request(
      "POST",
      &path,
      "resolve-force",
      Some("\"2\""),
      &unsafe_body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
  assert_eq!(peer.0.lock().unwrap().len(), 1);
}
