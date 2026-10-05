use super::*;

const AGENT_ID: &str = "77777777-7777-4777-8777-777777777777";
const AFTER_AGENT_ID: &str = "55555555-5555-4555-8555-555555555555";
const POOL_ID: &str = "66666666-6666-4666-8666-666666666666";

#[tokio::test]
async fn agent_capacity_handlers_preserve_filters_current_versions_and_safe_execution() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let listed = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/agents?pool_id={POOL_ID}&after={AFTER_AGENT_ID}&limit=7"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(listed.status(), StatusCode::OK);
  let list_query = application.agent_list_queries.lock().unwrap()[0];
  assert_eq!(list_query.pool_id.map(|id| id.to_string()).as_deref(), Some(POOL_ID));
  assert_eq!(
    list_query.after.map(|id| id.to_string()).as_deref(),
    Some(AFTER_AGENT_ID)
  );
  assert_eq!(list_query.limit, 7);

  let current_pool = routes
    .clone()
    .oneshot(empty_request("GET", &format!("/api/v1/agent-pools/{POOL_ID}"), None))
    .await
    .unwrap();
  assert_eq!(current_pool.status(), StatusCode::OK);
  assert_eq!(application.agent_pool_queries.lock().unwrap()[0].version, None);

  let detail = routes
    .oneshot(empty_request("GET", &format!("/api/v1/agents/{AGENT_ID}"), None))
    .await
    .unwrap();
  assert_eq!(detail.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body["current_execution"]["build_id"],
    "99999999-9999-4999-8999-999999999999"
  );
  assert_eq!(body["current_execution"]["lease_state"], "active");
  assert!(body["current_execution"].get("fencing_token").is_none());
}
