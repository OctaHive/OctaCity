//! Shared action reads translate scope/visibility/cursors; existing controls own mutations.
use super::*;
use octacity_server_application::*;

#[derive(Default)]
struct Queue {
  reads: Mutex<Vec<ReadFactoryActionQueue>>,
}
#[async_trait]
impl FactoryActionQueueStore for Queue {
  async fn read_factory_action_queue(
    &self,
    request: ReadFactoryActionQueue,
  ) -> Result<FactoryActionQueuePage, StoreError> {
    self.reads.lock().unwrap().push(request.clone());
    let mut action = action();
    let next = if request.after.is_none() {
      Some(action.position())
    } else {
      action.action.node_attempt_id = "0199a6f4-d56c-7440-9aa2-6a320f862797".parse().unwrap();
      None
    };
    Ok(FactoryActionQueuePage {
      items: vec![action],
      next,
      total: 2,
      coverage: FactoryActionProjectionCoverage::Complete,
    })
  }
}
fn action() -> FactoryActionQueueItem {
  serde_json::from_value(serde_json::json!({"project_id":"0199a6f4-d56c-7440-9aa2-6a320f862795","association":null,"action":{
    "work_id":"0199a6f4-d56c-7440-9aa2-6a320f862790","run_id":"0199a6f4-d56c-7440-9aa2-6a320f862791","run_version":3,"flow_run_id":"0199a6f4-d56c-7440-9aa2-6a320f862792","cycle_id":"0199a6f4-d56c-7440-9aa2-6a320f862793","node_attempt_id":"0199a6f4-d56c-7440-9aa2-6a320f862794","contract":{"id":"0199a6f4-d56c-7440-9aa2-6a320f862798","version":1,"digest":"a".repeat(64)},"node_key":"operator_selected","input_digest":"b".repeat(64),"candidate":null,"actionable_at":11,"deadline":null,"kind":{"kind":"retry"}
  }})).unwrap()
}
#[tokio::test]
async fn factory_action_http_scopes_no_store_and_cursor_binding_use_the_common_reader() {
  let queue = Arc::new(Queue::default());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let service = Arc::new(FactoryActionHandlers::new(
    queue.clone(),
    policy.clone(),
    Arc::new(SystemFactoryExecutionClock),
  ));
  let peer = Arc::new(RecordingApplication::default());
  let router = management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_factory_actions(
      octacity_server_api_rest::v1::FactoryActionManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
        policy, service,
      ))),
    ),
  );
  let first = router
    .clone()
    .oneshot(empty_request("GET", "/api/v1/factory-actions?limit=1", None))
    .await
    .unwrap();
  let status = first.status();
  let cache = first.headers().get("cache-control").cloned();
  let page = document(first).await;
  assert_eq!(status, StatusCode::OK, "{page}");
  assert_eq!(cache.unwrap(), "no-store");
  assert_json_matches_component(&openapi_document(), "FactoryActionsPage", &page);
  assert_eq!(page["total"], 2);
  assert_eq!(page["coverage"], "complete");
  assert_eq!(page["items"][0]["action"]["run_version"], 3);
  let cursor = page["next_cursor"].as_str().unwrap();
  assert!(page["observed_at"].as_i64().unwrap() > 0);
  let project = "0199a6f4-d56c-7440-9aa2-6a320f862795";
  let issue = "0199a6f4-d56c-7440-9aa2-6a320f862796";
  let different = format!("/api/v1/projects/{project}/issues/{issue}/actions?after={cursor}");
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &different, None))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  for path in [
    format!("/api/v1/factory-actions?after={cursor}&limit=1"),
    format!("/api/v1/projects/{project}/factory-actions?limit=1"),
    format!("/api/v1/projects/{project}/issues/{issue}/actions?limit=1"),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &path, None))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
  }
  let reads = queue.reads.lock().unwrap();
  assert_eq!(reads.len(), 4, "cross-scope cursor is rejected before persistence");
  assert!(reads[0].project_id.is_none());
  assert!(reads[1].after.is_some());
  assert_eq!(reads[2].project_id, Some(project.parse().unwrap()));
  assert_eq!(reads[3].issue_id, Some(issue.parse().unwrap()));
}

#[test]
fn factory_action_openapi_publishes_closed_shared_contracts_and_keeps_original_control_authority() {
  let document = openapi_document();
  for path in [
    "/api/v1/factory-actions",
    "/api/v1/projects/{project_id}/factory-actions",
    "/api/v1/projects/{project_id}/issues/{issue_id}/actions",
  ] {
    let op = &document["paths"][path]["get"];
    assert_eq!(
      op["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
      "#/components/schemas/FactoryActionsPage"
    );
    assert_eq!(
      op["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],
      "no-store"
    );
    assert!(document["paths"][path].get("post").is_none());
  }
  let schemas = &document["components"]["schemas"];
  for name in [
    "FactoryActionsPage",
    "FactoryActionQueueItem",
    "FactoryNodeActionDescription",
  ] {
    assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
  }
  assert_eq!(schemas["FactoryActionsPage"]["properties"]["items"]["maxItems"], 100);
  assert_eq!(
    schemas["FactoryNodeActionDescription"]["properties"]["run_version"]["minimum"],
    1
  );
  assert_eq!(
    schemas["FactoryNodeActionKind"]["oneOf"][0]["properties"]["schemas"]["items"]["$ref"],
    "#/components/schemas/FactoryFlowDataSchema"
  );
  assert!(
    schemas["FactoryNodeActionDescription"]["properties"]
      .get("value")
      .is_none()
  );
  for path in [
    "/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/responses",
    "/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/requests",
    "/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/retries",
  ] {
    let parameters = document["paths"][path]["post"]["parameters"].as_array().unwrap();
    for name in ["If-Match", "Idempotency-Key"] {
      assert!(parameters.iter().any(|p| p["name"] == name && p["required"] == true));
    }
  }
}

#[async_trait]
impl ManagementQueryUseCase<GetFactoryNodeActionQuery> for Queue {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetFactoryNodeActionQuery,
  ) -> Result<FactoryNodeActionObservation, ApplicationError> {
    Ok(FactoryNodeActionObservation {
      run_id: query.run_id,
      node_attempt_id: query.node_attempt_id,
      run_version: serde_json::from_value(serde_json::json!(4)).unwrap(),
      state: FactoryNodeActionState::Recorded,
      description: None,
      accepted_candidate: false,
      recorded_version: Some(action().action.run_version),
      recorded_at: Some(SystemFactoryExecutionClock.now()?),
      observed_at: SystemFactoryExecutionClock.now()?,
    })
  }
}
#[tokio::test]
async fn factory_action_http_observes_exact_recorded_target_without_private_response_values() {
  let queue = Arc::new(Queue::default());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let service = Arc::new(FactoryActionHandlers::new(
    queue.clone(),
    policy.clone(),
    Arc::new(SystemFactoryExecutionClock),
  ));
  let peer = Arc::new(RecordingApplication::default());
  let router = management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_factory_actions(
      octacity_server_api_rest::v1::FactoryActionManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
        policy.clone(),
        service,
      )))
      .with_observations(Arc::new(AuthorizedQueryHandler::new(policy, queue.clone()))),
    ),
  );
  let target = action().action;
  let path = format!(
    "/api/v1/factory-runs/{}/nodes/{}/action",
    target.run_id, target.node_attempt_id
  );
  let response = router.oneshot(empty_request("GET", &path, None)).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let body = document(response).await;
  assert_eq!(body["run_id"], target.run_id.to_string());
  assert_eq!(body["node_attempt_id"], target.node_attempt_id.to_string());
  assert_json_matches_component(&openapi_document(), "FactoryNodeActionObservation", &body);
  assert_eq!(body["state"], "recorded");
  assert_eq!(body["run_version"], 4);
  assert_eq!(body["recorded_version"], 3);
  assert!(body["description"].is_null());
  for field in ["value", "reason", "payload"] {
    assert!(body.get(field).is_none());
  }
  assert!(
    queue.reads.lock().unwrap().is_empty(),
    "exact receipt observation is not queue repair"
  );
  let schema = &openapi_document()["components"]["schemas"]["FactoryNodeActionObservation"];
  assert_eq!(schema["additionalProperties"], false);
  assert_eq!(
    schema["properties"]["state"]["enum"],
    serde_json::json!(["waiting", "recorded", "consumed", "historical"])
  );
}

#[tokio::test]
async fn factory_action_http_rejects_invalid_or_revoked_selection_before_reader_dispatch() {
  let scope = Arc::new(Mutex::new(None));
  let policy = Arc::new(IssueScope(scope.clone()));
  let queue = Arc::new(Queue::default());
  let service = Arc::new(FactoryActionHandlers::new(
    queue.clone(),
    policy.clone(),
    Arc::new(SystemFactoryExecutionClock),
  ));
  let peer = Arc::new(RecordingApplication::default());
  let router = management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_factory_actions(
      octacity_server_api_rest::v1::FactoryActionManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
        policy.clone(),
        service,
      )))
      .with_observations(Arc::new(AuthorizedQueryHandler::new(policy, queue.clone()))),
    ),
  );
  let first = document(
    router
      .clone()
      .oneshot(empty_request("GET", "/api/v1/factory-actions?limit=1", None))
      .await
      .unwrap(),
  )
  .await;
  let cursor = first["next_cursor"].as_str().unwrap();
  *scope.lock().unwrap() = Some(vec![]);
  let project = "0199a6f4-d56c-7440-9aa2-6a320f862795";
  let issue = "0199a6f4-d56c-7440-9aa2-6a320f862796";
  for query in ["limit=0", "limit=101", "after=bad", "extra=1"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("/api/v1/factory-actions?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/factory-actions?after={cursor}"),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{project}/issues/{issue}/actions"),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  let action = action().action;
  for query in [
    format!("issue_id={issue}"),
    "extra=1".to_owned(),
    "project_id=bad".to_owned(),
  ] {
    let path = format!(
      "/api/v1/factory-runs/{}/nodes/{}/action?{query}",
      action.run_id, action.node_attempt_id
    );
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &path, None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  assert_eq!(
    queue.reads.lock().unwrap().len(),
    1,
    "invalid/revoked collection selections never reach the persistence port"
  );
}
