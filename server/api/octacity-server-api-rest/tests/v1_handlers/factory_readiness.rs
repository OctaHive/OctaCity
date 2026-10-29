//! Exact readiness through the public management boundary.
use super::*;
#[tokio::test]
async fn exact_configuration_repository_readiness_is_read_only_and_cannot_infer_execution_from_metadata() {
  use testing::MutationEvidenceProbe;
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let request = serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":resolved["configuration"]});
  let publication = router
    .clone()
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations",
      "readiness-target",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(publication.status(), StatusCode::CREATED);
  let before = store.mutation_evidence_counts().await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!(
        "/api/v1/factory-configurations/{}/versions/1/readiness",
        parameters["configuration_id"].as_str().unwrap()
      ),
      "unused-readiness-key",
      None,
      &serde_json::json!({"repository":parameters["repository"]}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let result = json(response).await;
  assert_eq!(result["configuration"]["id"], parameters["configuration_id"]);
  assert_eq!(result["configuration"]["version"], 1);
  assert_eq!(result["repository"], parameters["repository"]);
  assert_eq!(result["flow"], resolved["configuration"]["flow"]["closure"]["root"]);
  assert_eq!(result["execution"], "blocked");
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "agent" && c["state"] != "ready")
  );
  assert!(result["expires_at"].as_i64().unwrap() > result["observed_at"].as_i64().unwrap());
  assert_eq!(store.mutation_evidence_counts().await, before);
}

struct ReadinessClock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for ReadinessClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Timestamp::from_unix_millis(self.0.load(Ordering::SeqCst)).map_err(|_| ApplicationError::invalid())
  }
}
struct RetainedReadinessDocuments;
#[async_trait]
impl IssueArtifactReader for RetainedReadinessDocuments {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    Ok(IssueArtifactMetadata {
      name: "document".into(),
      media_type: "text/plain".into(),
      origin: IssueArtifactOrigin::ProjectInput,
      availability: IssueArtifactAvailability::Available,
      published_at: Some(Timestamp::from_unix_millis(1).unwrap()),
      expires_at: Some(Timestamp::from_unix_millis(2000).unwrap()),
    })
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("readiness must not read object bytes")
  }
}
#[tokio::test]
async fn readiness_observes_exact_declared_context_retention_and_expires_without_object_io() {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let request = serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":resolved["configuration"]});
  assert_eq!(
    router
      .oneshot(json_request(
        "POST",
        "/api/v1/factory-configurations",
        "context-target",
        None,
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  let clock = Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000)));
  let routes = readiness_router(project, store, clock.clone());
  let path = format!(
    "/api/v1/factory-configurations/{}/versions/1/readiness",
    parameters["configuration_id"].as_str().unwrap()
  );
  let body = serde_json::json!({"repository":parameters["repository"]}).to_string();
  let first = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(first["expires_at"], 2000);
  let retained = first["checks"]
    .as_array()
    .unwrap()
    .iter()
    .find(|check| check["aspect"] == "context" && check["state"] == "ready")
    .unwrap();
  assert_eq!(retained["subject"]["definition"], first["flow"]);
  assert!(retained["subject"]["node"].is_string());
  assert!(retained["subject"]["dependency"].is_string());
  assert_eq!(retained["remediation"], "retain_context");
  assert!(
    first["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "context" && c["state"] == "ready")
  );
  clock.0.store(2100, Ordering::SeqCst);
  let expired = json(
    routes
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    expired["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "context" && c["state"] == "blocked" && c["reason"] == "expired")
  );
}
fn readiness_router(
  project: ProjectId,
  store: Arc<testing::InMemoryFactoryConfigurationStore>,
  clock: Arc<dyn FactoryExecutionClock>,
) -> axum::Router {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(store, policy.clone(), clock, std::time::Duration::from_secs(30))
    .unwrap()
    .with_repositories(metadata)
    .with_artifacts(Arc::new(RetainedReadinessDocuments));
  let peer = Arc::new(RecordingApplication::default());
  management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_factory_readiness(
      octacity_server_api_rest::v1::FactoryReadinessManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
        policy,
        Arc::new(handler),
      ))),
    ),
  )
}

type ReadinessPublishedTarget = (
  axum::Router,
  ProjectId,
  Arc<testing::InMemoryFactoryConfigurationStore>,
  serde_json::Value,
);
async fn published_target(enabled: bool) -> ReadinessPublishedTarget {
  published_target_with_runtime(enabled, PresetTestRuntime::Plain).await
}
async fn published_target_with_runtime(enabled: bool, runtime: PresetTestRuntime) -> ReadinessPublishedTarget {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters_with_runtime(&router, project, &store, runtime).await;
  testing::qualify_readiness_pipeline(
    &store,
    project,
    parameters["profiles"]["eligibility"]["build"].clone(),
    "root",
  )
  .await;
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let mut definition = resolved["configuration"].clone();
  definition["enabled"] = serde_json::json!(enabled);
  let request = serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":definition});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        "/api/v1/factory-configurations",
        "readiness-published-target",
        None,
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  (router, project, store, parameters)
}
#[tokio::test]
async fn readiness_rechecks_withdrawn_installation_instead_of_trusting_published_profiles() {
  let (router, project, store, parameters) = published_target(true).await;
  store.disable_project(project).unwrap();
  let response = router
    .oneshot(json_request(
      "POST",
      &format!(
        "/api/v1/factory-configurations/{}/versions/1/readiness",
        parameters["configuration_id"].as_str().unwrap()
      ),
      "read",
      None,
      &serde_json::json!({"repository":parameters["repository"]}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(result["execution"], "blocked");
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "installation" && c["state"] == "blocked")
  );
  let _ = project;
}

#[tokio::test]
async fn readiness_does_not_infer_scoped_mcp_authentication_or_capture_from_saved_settings() {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters_with_runtime(&router, project, &store, PresetTestRuntime::Mcp(1)).await;
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert!(resolved["configuration"].is_object(), "{resolved}");
  let mut definition = resolved["configuration"].clone();
  definition["enabled"] = serde_json::json!(true);
  let publication = router
    .clone()
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations",
      "mcp-readiness",
      None,
      &serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":definition})
        .to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(publication.status(), StatusCode::CREATED, "{}", json(publication).await);
  let result = json(
    router
      .oneshot(json_request(
        "POST",
        &format!(
          "/api/v1/factory-configurations/{}/versions/1/readiness",
          parameters["configuration_id"].as_str().unwrap()
        ),
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let checks = result["checks"].as_array().unwrap();
  for aspect in [
    "mcp_authentication",
    "mcp_capture",
    "octacity_mcp",
    "model_authentication",
  ] {
    assert!(
      checks
        .iter()
        .any(|check| check["aspect"] == aspect && check["state"] != "ready"),
      "{aspect}: {result}"
    );
  }
  assert_ne!(result["execution"], "ready");
  for aspect in ["mcp_transport", "mcp_secret_reference"] {
    let fact = checks
      .iter()
      .find(|check| check["aspect"] == aspect && check["state"] == "ready")
      .unwrap_or_else(|| panic!("qualified {aspect}: {result}"));
    assert_eq!(fact["subject"]["dependency"], "city");
    assert_eq!(fact["subject"]["definition"], result["flow"]);
    assert!(fact["subject"]["node"].is_string());
    assert_eq!(fact["remediation"], "review_mcp_connection");
  }
  assert!(!result.to_string().contains("mcp-reader"));
}

struct NoReadinessAgents;
#[async_trait]
impl ManagementQueryUseCase<ListAgentsQuery> for NoReadinessAgents {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: ListAgentsQuery,
  ) -> Result<AgentPageProjection, ApplicationError> {
    Ok(AgentPageProjection {
      agents: vec![],
      next_cursor: None,
      active_count: 0,
      total_count: 0,
    })
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetAgentQuery> for NoReadinessAgents {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: GetAgentQuery,
  ) -> Result<AgentProjection, ApplicationError> {
    panic!("empty inventory cannot request an Agent")
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetAgentPoolQuery> for NoReadinessAgents {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: GetAgentPoolQuery,
  ) -> Result<AgentPoolProjection, ApplicationError> {
    panic!("empty inventory cannot request a Pool")
  }
}
#[tokio::test]
async fn readiness_distinguishes_observed_absent_capacity_from_unobserved_agents() {
  let (_, project, store, parameters) = published_target(true).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let agents = Arc::new(AuthorizedQueryHandler::new(policy.clone(), Arc::new(NoReadinessAgents)));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_placement(FactoryReadinessPlacement::new(
    Arc::new(testing::UnavailableReadinessContexts),
    agents.clone(),
    agents,
  ));
  let routes = readiness_handler_router(handler, policy);
  let response = routes
    .oneshot(json_request(
      "POST",
      &format!(
        "/api/v1/factory-configurations/{}/versions/1/readiness",
        parameters["configuration_id"].as_str().unwrap()
      ),
      "read",
      None,
      &serde_json::json!({"repository":parameters["repository"]}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "blocked" && check["reason"] == "capacity_absent"),
    "{result}"
  );
}

#[tokio::test]
async fn readiness_uses_current_qualified_agent_inventory_and_original_observation_expiry() {
  let (_, project, store, parameters) = published_target(true).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let fixture = Arc::new(testing::ReadinessExecutionFixture::new(
    project,
    parameters["profiles"]["eligibility"]["build"].clone(),
    serde_json::from_value(parameters["repository"].clone()).unwrap(),
  ));
  let agents = Arc::new(AuthorizedQueryHandler::new(policy.clone(), fixture.clone()));
  let clock = Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000)));
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(store, policy.clone(), clock.clone(), std::time::Duration::from_secs(30))
    .unwrap()
    .with_repositories(metadata)
    .with_placement(FactoryReadinessPlacement::new(fixture.clone(), agents.clone(), agents));
  let routes = readiness_handler_router(handler, policy);
  let path = format!(
    "/api/v1/factory-configurations/{}/versions/1/readiness",
    parameters["configuration_id"].as_str().unwrap()
  );
  let body = serde_json::json!({"repository":parameters["repository"]}).to_string();
  let ready = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    ready["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "ready" && check["expires_at"] == 31000),
    "{ready}"
  );
  let original_resources = fixture.agent.lock().unwrap().resources.clone();
  {
    let mut agent = fixture.agent.lock().unwrap();
    let mut resources = serde_json::to_value(&agent.resources).unwrap();
    resources["registration_expires_at"] = serde_json::json!(2000);
    agent.resources = Some(serde_json::from_value(resources).unwrap());
  }
  let expiring = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    expiring["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "ready" && check["expires_at"] == 2000),
    "{expiring}"
  );
  clock.0.store(2000, Ordering::SeqCst);
  let expired_registration = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    expired_registration["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "unknown"),
    "{expired_registration}"
  );
  fixture.agent.lock().unwrap().resources = original_resources;
  clock.0.store(1000, Ordering::SeqCst);
  {
    let mut agent = fixture.agent.lock().unwrap();
    agent.current_execution=Some(serde_json::from_value(serde_json::json!({"lease_id":uuid::Uuid::new_v4().to_string(),"build_id":uuid::Uuid::new_v4().to_string(),"attempt_id":uuid::Uuid::new_v4().to_string(),"job_id":uuid::Uuid::new_v4().to_string(),"lease_state":"active"})).unwrap());
    let resources = agent.resources.as_mut().unwrap();
    resources.available_cpu_millis = 0;
    resources.available_memory_bytes = 0;
  }
  let busy = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    busy["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "ready" && check["reason"] == "queue_pending"),
    "{busy}"
  );
  {
    let mut agent = fixture.agent.lock().unwrap();
    agent.current_execution = None;
    let resources = agent.resources.as_mut().unwrap();
    resources.available_cpu_millis = 2000;
    resources.available_memory_bytes = 4096;
  }
  fixture.pool.lock().unwrap().drain_state = AgentPoolDrainStateProjection::GracefulDrain;
  let drained = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    drained["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "blocked"),
    "{drained}"
  );
  fixture.pool.lock().unwrap().drain_state = AgentPoolDrainStateProjection::Accepting;
  clock.0.store(31000, Ordering::SeqCst);
  let stale = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    stale["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "unknown" && check["reason"] == "stale"),
    "{stale}"
  );
  clock.0.store(1000, Ordering::SeqCst);
  fixture.agent.lock().unwrap().inventory.factory_executions.clear();
  let unsupported = json(
    routes
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    unsupported["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "blocked"),
    "{unsupported}"
  );
}

#[tokio::test]
async fn readiness_does_not_infer_native_writer_authority_from_execution_metadata() {
  let (router, _, store, parameters) = published_target(true).await;
  use testing::MutationEvidenceProbe;
  let before = store.mutation_evidence_counts().await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!(
        "/api/v1/factory-configurations/{}/versions/1/readiness",
        parameters["configuration_id"].as_str().unwrap()
      ),
      "read",
      None,
      &serde_json::json!({"repository":parameters["repository"]}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(result["native_write"], "unknown");
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "native_write" && check["reason"] == "unverified" && check["required"] == false),
    "{result}"
  );
  assert_eq!(store.mutation_evidence_counts().await, before);
}

struct ReadinessSourceMetadata {
  record: WorkSourceConfigurationRecord,
  metadata: PresetRepository,
  allowed: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl ManagementQueryUseCase<GetWorkSourceConfigurationQuery> for ReadinessSourceMetadata {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetWorkSourceConfigurationQuery,
  ) -> Result<WorkSourceConfigurationRecord, ApplicationError> {
    assert_eq!(self.record.target(), query.target);
    Ok(self.record.clone())
  }
}
#[async_trait]
impl ManagementQueryUseCase<ListWorkSourceInputChoicesQuery> for ReadinessSourceMetadata {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListWorkSourceInputChoicesQuery,
  ) -> Result<WorkSourceInputChoices, ApplicationError> {
    if !self.allowed.load(Ordering::SeqCst) {
      return Err(ApplicationError::capability_unavailable());
    }
    self.metadata.execute_management_query(context, grant, query).await
  }
}
#[tokio::test]
async fn readiness_reuses_independent_source_authority_without_promoting_native_writes() {
  let (router, project, store, parameters) = published_target(true).await;
  let path = format!(
    "/api/v1/factory-configurations/{}/versions/1/readiness",
    parameters["configuration_id"].as_str().unwrap()
  );
  let pins = json(
    router
      .oneshot(json_request(
        "POST",
        &path,
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let mut selected = input_selection();
  selected["connection"]["reporter_configuration"] = serde_json::Value::Null;
  let record:WorkSourceConfigurationRecord=serde_json::from_value(serde_json::json!({"kind":"input","definition":{"id":selected["id"],"version":1,"project_id":project,"root_flow":pins["flow"],"definition":{"target":pins["configuration"],"repository":parameters["repository"],"source":selected["source"],"reporter":null,"connection":selected["connection"],"enabled":true}}})).unwrap();
  let fixture = Arc::new(ReadinessSourceMetadata {
    record,
    metadata: PresetRepository(repository(project), Some(selected.clone())),
    allowed: std::sync::atomic::AtomicBool::new(true),
  });
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let sources = Arc::new(AuthorizedQueryHandler::new(policy.clone(), fixture.clone()));
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_sources(sources.clone());
  let routes = readiness_handler_router(handler, policy);
  let body =
    serde_json::json!({"repository":parameters["repository"],"input":{"id":selected["id"],"version":1}}).to_string();
  let response = routes
    .clone()
    .oneshot(json_request("POST", &path, "read", None, &body))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let allowed = json(response).await;
  assert!(
    allowed["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "source" && check["state"] == "ready"),
    "{allowed}"
  );
  assert_eq!(allowed["native_write"], "unknown");
  fixture.allowed.store(false, Ordering::SeqCst);
  let revoked = json(
    routes
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    revoked["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "source" && check["state"] == "blocked"),
    "{revoked}"
  );
  assert_eq!(revoked["native_write"], "unknown");
}

struct RevocableReadinessPolicy {
  visible: Arc<std::sync::atomic::AtomicBool>,
  kind: ManagementResourceKind,
}
#[async_trait]
impl ManagementAuthorizationPolicy for RevocableReadinessPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.kind && !self.visible.load(Ordering::SeqCst) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
struct RevokingReadinessDocuments(Arc<std::sync::atomic::AtomicBool>);
#[async_trait]
impl IssueArtifactReader for RevokingReadinessDocuments {
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    self.0.store(false, Ordering::SeqCst);
    RetainedReadinessDocuments.metadata(request).await
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("no object reads")
  }
}
#[tokio::test]
async fn readiness_rechecks_exact_subject_visibility_after_awaiting_dependencies() {
  for kind in [
    ManagementResourceKind::FactoryConfiguration,
    ManagementResourceKind::Repository,
  ] {
    let (_, project, store, parameters) = published_target(true).await;
    let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let policy = Arc::new(RevocableReadinessPolicy {
      visible: visible.clone(),
      kind,
    });
    let metadata = Arc::new(AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(PresetRepository(repository(project), None)),
    ));
    let handler = FactoryReadinessHandlers::new(
      store,
      policy.clone(),
      Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
      std::time::Duration::from_secs(30),
    )
    .unwrap()
    .with_repositories(metadata)
    .with_artifacts(Arc::new(RevokingReadinessDocuments(visible.clone())));
    let routes = readiness_handler_router(handler, policy);
    let response = routes
      .oneshot(json_request(
        "POST",
        &format!(
          "/api/v1/factory-configurations/{}/versions/1/readiness",
          parameters["configuration_id"].as_str().unwrap()
        ),
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap();
    assert!(!visible.load(Ordering::SeqCst));
    assert!(
      !response.status().is_success(),
      "revoked {kind:?} must prevent disclosure of its dependency report"
    );
  }
}

struct NoReadinessObjectReads;
#[async_trait]
impl IssueArtifactReader for NoReadinessObjectReads {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    panic!("oversized readiness must stop before dependency reads")
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("no object reads")
  }
}
#[tokio::test]
async fn readiness_bounds_required_observations_and_dependency_work_without_truncating_to_ready() {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters_with_runtime(&router, project, &store, PresetTestRuntime::Mcp(24)).await;
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert!(resolved["configuration"].is_object(), "{resolved}");
  let response=router.oneshot(json_request("POST","/api/v1/factory-configurations","bounded-readiness",None,&serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":resolved["configuration"]}).to_string())).await.unwrap();
  assert_eq!(response.status(), StatusCode::CREATED, "{}", json(response).await);
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_artifacts(Arc::new(NoReadinessObjectReads));
  let routes = readiness_handler_router(handler, policy);
  let response = routes
    .oneshot(json_request(
      "POST",
      &format!(
        "/api/v1/factory-configurations/{}/versions/1/readiness",
        parameters["configuration_id"].as_str().unwrap()
      ),
      "read",
      None,
      &serde_json::json!({"repository":parameters["repository"]}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_ne!(result["execution"], "ready");
  let checks = result["checks"].as_array().unwrap();
  assert!(checks.len() <= factory_configuration_document_bounds().choice_entries);
  assert!(
    checks
      .iter()
      .any(|check| check["aspect"] == "coverage" && check["state"] == "unknown" && check["required"] == true),
    "{result}"
  );
}

fn readiness_handler_router(
  handler: FactoryReadinessHandlers<testing::InMemoryFactoryConfigurationStore>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> axum::Router {
  let peer = Arc::new(RecordingApplication::default());
  management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_factory_readiness(
      octacity_server_api_rest::v1::FactoryReadinessManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
        policy,
        Arc::new(handler),
      ))),
    ),
  )
}
struct ExpiringReadinessDocuments(Arc<ReadinessClock>);
#[async_trait]
impl IssueArtifactReader for ExpiringReadinessDocuments {
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    self.0.0.store(2100, Ordering::SeqCst);
    RetainedReadinessDocuments.metadata(request).await
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("no object reads")
  }
}
#[tokio::test]
async fn readiness_does_not_return_ready_facts_that_expire_while_dependencies_are_read() {
  let (_, project, store, parameters) = published_target(true).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let clock = Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000)));
  let handler = FactoryReadinessHandlers::new(store, policy.clone(), clock.clone(), std::time::Duration::from_secs(30))
    .unwrap()
    .with_repositories(metadata)
    .with_artifacts(Arc::new(ExpiringReadinessDocuments(clock)));
  let routes = readiness_handler_router(handler, policy);
  let response = routes
    .oneshot(json_request(
      "POST",
      &format!(
        "/api/v1/factory-configurations/{}/versions/1/readiness",
        parameters["configuration_id"].as_str().unwrap()
      ),
      "read",
      None,
      &serde_json::json!({"repository":parameters["repository"]}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(result["observed_at"], 2100);
  assert!(
    !result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "context" && check["state"] == "ready"),
    "{result}"
  );
}

struct ReadinessActionChoices {
  metadata: PresetRepository,
  writer: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl ManagementQueryUseCase<ListWorkSourceInputChoicesQuery> for ReadinessActionChoices {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListWorkSourceInputChoicesQuery,
  ) -> Result<WorkSourceInputChoices, ApplicationError> {
    let mut choices = self.metadata.execute_management_query(context, grant, query).await?;
    if !self.writer.load(Ordering::SeqCst) {
      choices
        .items
        .retain(|choice| choice.purpose != IntegrationPurpose::TaskUpdate);
    }
    Ok(choices)
  }
}
#[tokio::test]
async fn readiness_checks_frozen_reporter_actions_without_an_optional_input() {
  let input = reviewed_input_selection();
  let (router, project, store) =
    preset_router_with_input(Arc::new(TrustedNetworkManagementPolicy), None, Some(input.clone()));
  let mut parameters = selected_parameters(&router, project, &store).await;
  let reporting = effect_parameters(project, &store, &mut parameters, &input).await;
  parameters["effects"] = serde_json::json!({"eligibility_gate":reporting});
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert!(resolved["configuration"].is_object(), "{resolved}");
  let publication=router.oneshot(json_request("POST","/api/v1/factory-configurations","action-readiness",None,&serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":resolved["configuration"]}).to_string())).await.unwrap();
  assert_eq!(publication.status(), StatusCode::CREATED, "{}", json(publication).await);
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let choices = Arc::new(ReadinessActionChoices {
    metadata: PresetRepository(repository(project), Some(input)),
    writer: std::sync::atomic::AtomicBool::new(true),
  });
  let actions = Arc::new(AuthorizedQueryHandler::new(policy.clone(), choices.clone()));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_source_actions(actions);
  let routes = readiness_handler_router(handler, policy);
  let path = format!(
    "/api/v1/factory-configurations/{}/versions/1/readiness",
    parameters["configuration_id"].as_str().unwrap()
  );
  let body = serde_json::json!({"repository":parameters["repository"]}).to_string();
  let allowed = json(
    routes
      .clone()
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    allowed["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "reporter_action" && check["state"] == "ready" && check["required"] == true),
    "{allowed}"
  );
  choices.writer.store(false, Ordering::SeqCst);
  let revoked = json(
    routes
      .oneshot(json_request("POST", &path, "read", None, &body))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    revoked["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "reporter_action" && check["state"] == "blocked" && check["required"] == true),
    "{revoked}"
  );
  assert_eq!(revoked["native_write"], "unknown");
}

struct AdvancingReadinessAuthorization {
  clock: Arc<ReadinessClock>,
  repositories: AtomicUsize,
}
#[async_trait]
impl ManagementAuthorizationPolicy for AdvancingReadinessAuthorization {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::Repository && self.repositories.fetch_add(1, Ordering::SeqCst) == 1 {
      self.clock.0.store(2100, Ordering::SeqCst);
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn readiness_review_expiry_includes_the_final_authorization_fence() {
  let (_, project, store, parameters) = published_target(true).await;
  let clock = Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000)));
  let policy = Arc::new(AdvancingReadinessAuthorization {
    clock: clock.clone(),
    repositories: AtomicUsize::new(0),
  });
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(store, policy.clone(), clock, std::time::Duration::from_secs(30))
    .unwrap()
    .with_repositories(metadata)
    .with_artifacts(Arc::new(RetainedReadinessDocuments));
  let routes = readiness_handler_router(handler, policy);
  let result = json(
    routes
      .oneshot(json_request(
        "POST",
        &format!(
          "/api/v1/factory-configurations/{}/versions/1/readiness",
          parameters["configuration_id"].as_str().unwrap()
        ),
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(result["observed_at"], 2100);
  assert!(
    !result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "context" && check["state"] == "ready"),
    "{result}"
  );
}
#[tokio::test]
async fn readiness_review_tool_using_agent_requires_the_exact_control_contract() {
  let (_, project, store, parameters) = published_target_with_runtime(true, PresetTestRuntime::ControlledTools).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let fixture = Arc::new(testing::ReadinessExecutionFixture::new(
    project,
    parameters["profiles"]["eligibility"]["build"].clone(),
    serde_json::from_value(parameters["repository"].clone()).unwrap(),
  ));
  fixture.agent.lock().unwrap().inventory.factory_executions[0].controlled_tools = false;
  let agents = Arc::new(AuthorizedQueryHandler::new(policy.clone(), fixture.clone()));
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_placement(FactoryReadinessPlacement::new(fixture, agents.clone(), agents));
  let routes = readiness_handler_router(handler, policy);
  let result = json(
    routes
      .oneshot(json_request(
        "POST",
        &format!(
          "/api/v1/factory-configurations/{}/versions/1/readiness",
          parameters["configuration_id"].as_str().unwrap()
        ),
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent"
        && check["state"] == "unknown"
        && check["reason"] == "tool_control_unobserved"),
    "{result}"
  );
}

#[tokio::test]
async fn readiness_rejects_pipeline_shapes_that_the_selected_materializer_cannot_build() {
  let (_, project, store, parameters) = published_target(true).await;
  testing::qualify_readiness_pipeline(
    &store,
    project,
    parameters["profiles"]["eligibility"]["build"].clone(),
    "root",
  )
  .await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let fixture = Arc::new(
    testing::ReadinessExecutionFixture::new(
      project,
      parameters["profiles"]["eligibility"]["build"].clone(),
      serde_json::from_value(parameters["repository"].clone()).unwrap(),
    )
    .with_pipeline_nodes(&["root", "child"]),
  );
  let agents = Arc::new(AuthorizedQueryHandler::new(policy.clone(), fixture.clone()));
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_placement(FactoryReadinessPlacement::new(fixture, agents.clone(), agents));
  let routes = readiness_handler_router(handler, policy);
  let result = json(
    routes
      .oneshot(json_request(
        "POST",
        &format!(
          "/api/v1/factory-configurations/{}/versions/1/readiness",
          parameters["configuration_id"].as_str().unwrap()
        ),
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent"
        && check["state"] == "blocked"
        && check["reason"] == "pipeline_incompatible"),
    "{result}"
  );
}

#[tokio::test]
async fn readiness_shared_pool_supports_many_required_profiles_within_the_observation_budget() {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  testing::qualify_readiness_pipeline(
    &store,
    project,
    parameters["profiles"]["eligibility"]["build"].clone(),
    "root",
  )
  .await;
  let resolved = json(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-presets/resolve"),
        "read",
        None,
        &parameters.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let mut definition = resolved["configuration"].clone();
  testing::expand_readiness_profiles(&mut definition, 43);
  let publication = router
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations",
      "shared-pool-budget",
      None,
      &serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":definition})
        .to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(publication.status(), StatusCode::CREATED, "{}", json(publication).await);
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let fixture = Arc::new(testing::ReadinessExecutionFixture::new(
    project,
    parameters["profiles"]["eligibility"]["build"].clone(),
    serde_json::from_value(parameters["repository"].clone()).unwrap(),
  ));
  let agents = Arc::new(AuthorizedQueryHandler::new(policy.clone(), fixture.clone()));
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), None)),
  ));
  let handler = FactoryReadinessHandlers::new(
    store,
    policy.clone(),
    Arc::new(ReadinessClock(std::sync::atomic::AtomicI64::new(1000))),
    std::time::Duration::from_secs(30),
  )
  .unwrap()
  .with_repositories(metadata)
  .with_placement(FactoryReadinessPlacement::new(fixture, agents.clone(), agents));
  let routes = readiness_handler_router(handler, policy);
  let result = json(
    routes
      .oneshot(json_request(
        "POST",
        &format!(
          "/api/v1/factory-configurations/{}/versions/1/readiness",
          parameters["configuration_id"].as_str().unwrap()
        ),
        "read",
        None,
        &serde_json::json!({"repository":parameters["repository"]}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    result["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|check| check["aspect"] == "agent" && check["state"] == "ready"),
    "{result}"
  );
}
