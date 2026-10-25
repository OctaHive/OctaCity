//! Runtime HTTP journeys use the ordinary role/profile handlers and memory publication owner.
use super::*;
use octacity_server_api_rest::v1::FactoryRoleManagementApplication;
use octacity_server_application::*;

fn runtime_router(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> (
  axum::Router,
  Arc<testing::InMemoryFactoryConfigurationStore>,
  ProjectId,
  serde_json::Value,
) {
  let project = ProjectId::generate();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let (choices, definition) = testing::installed_role_choices(project);
  let alias = choices.role_profile_capabilities().keys().next().unwrap().clone();
  let choices = choices
    .with_runtime_settings_support(
      alias,
      serde_json::from_value(serde_json::json!({
        "mcp_transports":["stdio"], "mcp_executables":{"/runtime/mcp-reader":definition.toolchain.executable}
      }))
      .unwrap(),
    )
    .unwrap();
  store.enable_project(project, choices).unwrap();
  let handlers = Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy.clone()));
  let commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), handlers.clone()));
  let queries = Arc::new(AuthorizedQueryHandler::new(policy, handlers));
  let roles =
    FactoryRoleManagementApplication::new(commands.clone(), queries.clone()).with_runtime_settings(commands, queries);
  let peer = Arc::new(RecordingApplication::default());
  let app = recording_management_application(peer.clone(), peer).with_factory_roles(roles);
  let body = serde_json::json!({"expected_version":null,"definition":{
    "toolchain":definition.toolchain,"mcp_servers":{"reader":{"server":{
      "transport":"stdio","command":"/runtime/mcp-reader","args":["--approved-private-argument"],"env_vars":["LANGUAGE"]
    },"integration":null}},"integrations":{},"permissions":definition.permissions
  }});
  (management_router_with_application(|| true, app), store, project, body)
}
async fn json(response: axum::response::Response) -> serde_json::Value {
  serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn runtime_http_reads_and_publishes_reviewed_settings_through_the_existing_owner() {
  let policy = Arc::new(RuntimePolicy(std::sync::atomic::AtomicBool::new(true)));
  let (router, _, project, body) = runtime_router(policy.clone());
  assert_json_matches_component(&openapi_document(), "FactoryRuntimeSettingsRequest", &body);
  let path = format!("/api/v1/projects/{project}/factory-runtimes/dependency-1");
  let choices = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-runtime-choices?limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(choices.status(), StatusCode::OK);
  assert_eq!(choices.headers()["cache-control"], "no-store");
  let choices = json(choices).await;
  assert_json_matches_component(&openapi_document(), "FactoryRuntimeChoicePage", &choices);
  assert_eq!(choices["total"], 1);
  assert_eq!(choices["items"][0]["alias"], "dependency-1");
  assert_eq!(
    choices["items"][0]["support"]["mcp_transports"],
    serde_json::json!(["stdio"])
  );
  assert_eq!(
    choices["items"][0]["support"]["mcp_executables"]["/runtime/mcp-reader"],
    body["definition"]["toolchain"]["executable"]
  );
  assert!(
    choices["items"][0]["environment_names"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("LANGUAGE"))
  );
  assert!(
    !choices.to_string().contains("projects/model"),
    "setup publishes names, never logical secret locators"
  );
  let empty = router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap();
  assert_eq!(empty.status(), StatusCode::OK);
  assert_eq!(empty.headers()["cache-control"], "no-store");
  assert_eq!(json(empty).await, serde_json::json!({"settings":null}));
  let checked = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/validate"),
      "unused-validation-key",
      None,
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(checked.status(), StatusCode::OK);
  assert_eq!(checked.headers()["cache-control"], "no-store");
  let checked = json(checked).await;
  assert_eq!(checked["settings"]["version"], 1);
  assert_eq!(checked["settings"]["definition"], body["definition"]);
  let unchanged = router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap();
  assert_eq!(json(unchanged).await, serde_json::json!({"settings":null}));
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "runtime-save", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let saved = json(response).await;
  assert_json_matches_component(&openapi_document(), "FactoryRuntimeSettingsPublication", &saved);
  assert_eq!(saved["disposition"], "applied");
  assert_eq!(saved["settings"]["version"], 1);
  assert_eq!(saved["settings"]["definition"], body["definition"]);
  let current = router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap();
  assert_eq!(current.status(), StatusCode::OK);
  assert_eq!(json(current).await["settings"], saved["settings"]);
  let mut replacement = body.clone();
  replacement["expected_version"] = serde_json::json!(1);
  replacement["definition"]["mcp_servers"]["reader"]["server"]["enabled_tools"] = serde_json::json!(["read"]);
  let changed = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "runtime-replacement",
      None,
      &replacement.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(changed.status(), StatusCode::OK);
  let changed = json(changed).await;
  assert_eq!(changed["settings"]["version"], 2);
  let retried = router
    .clone()
    .oneshot(json_request("POST", &path, "runtime-save", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(retried.status(), StatusCode::OK);
  let retried = json(retried).await;
  assert_eq!(retried["disposition"], "replayed");
  assert_eq!(retried["settings"], saved["settings"]);
  assert_eq!(retried["published_at"], saved["published_at"]);
  let historical = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/versions/1"), None))
    .await
    .unwrap();
  assert_eq!(historical.status(), StatusCode::OK);
  assert_eq!(json(historical).await["settings"], saved["settings"]);
  let stale = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "stale-runtime",
      None,
      &replacement.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
  let conflict = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "runtime-save",
      None,
      &replacement.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(conflict.status(), StatusCode::CONFLICT);
  policy.0.store(false, Ordering::SeqCst);
  for request in [
    empty_request("GET", &path, None),
    empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-runtime-choices"),
      None,
    ),
    json_request("POST", &path, "runtime-save", None, &body.to_string()),
    json_request(
      "POST",
      &format!("{path}/validate"),
      "unused",
      None,
      &replacement.to_string(),
    ),
  ] {
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
  }
}

struct RuntimePolicy(std::sync::atomic::AtomicBool);
#[async_trait]
impl ManagementAuthorizationPolicy for RuntimePolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if !self.0.load(Ordering::SeqCst) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}

pub(super) fn management_requests() -> Vec<Request<Body>> {
  let project = "11111111-1111-4111-8111-111111111111";
  let (_, definition) = testing::installed_role_choices(project.parse().unwrap());
  let body = serde_json::json!({"expected_version":null,"definition":{
    "toolchain":definition.toolchain,"mcp_servers":{},"integrations":{},"permissions":definition.permissions,
  }})
  .to_string();
  let base = format!("/api/v1/projects/{project}");
  let path = format!("{base}/factory-runtimes/dependency-1");
  vec![
    empty_request("GET", &format!("{base}/factory-runtime-choices?limit=1"), None),
    empty_request("GET", &path, None),
    empty_request("GET", &format!("{path}/versions/1"), None),
    json_request("POST", &format!("{path}/validate"), "unused", None, &body),
    json_request("POST", &path, "publish-runtime", None, &body),
  ]
}

#[tokio::test]
async fn runtime_http_rejects_invalid_settings_without_publishing_or_reserving_the_retry_key() {
  let (router, _, project, body) = runtime_router(Arc::new(TrustedNetworkManagementPolicy));
  let path = format!("/api/v1/projects/{project}/factory-runtimes/dependency-1");
  let mut unknown = body.clone();
  unknown["definition"]["credential_value"] = serde_json::json!("must-not-be-accepted");
  let mut unsupported = body.clone();
  unsupported["definition"]["mcp_servers"]["reader"]["server"]["command"] = serde_json::json!("/not-installed/mcp");
  for (bad, status) in [
    (unknown, StatusCode::BAD_REQUEST),
    (unsupported, StatusCode::UNPROCESSABLE_ENTITY),
  ] {
    for target in [path.clone(), format!("{path}/validate")] {
      let rejected = router
        .clone()
        .oneshot(json_request(
          "POST",
          &target,
          "repaired-runtime",
          None,
          &bad.to_string(),
        ))
        .await
        .unwrap();
      assert_eq!(rejected.status(), status);
      let error = json(rejected).await;
      assert!(!error.to_string().contains("must-not-be-accepted"));
    }
    let current = router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap();
    assert_eq!(json(current).await, serde_json::json!({"settings":null}));
  }
  let saved = router
    .oneshot(json_request("POST", &path, "repaired-runtime", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(saved.status(), StatusCode::OK);
  assert_eq!(json(saved).await["settings"]["version"], 1);
}
