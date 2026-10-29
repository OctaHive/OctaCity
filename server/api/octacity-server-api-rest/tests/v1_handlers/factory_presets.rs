//! Preset setup uses authorized management HTTP and ordinary publication contracts.
use super::*;
use octacity_server_application::*;

pub(super) fn management_requests() -> Vec<Request<Body>> {
  let path = "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-presets";
  vec![
    empty_request("GET", path, None),
    json_request(
      "POST",
      &format!("{path}/resolve"),
      "unused-read",
      None,
      &serde_json::json!({"preset":{"identity":"triage","version":"v1","digest":"d".repeat(64)}}).to_string(),
    ),
  ]
}

fn preset_router() -> (axum::Router, ProjectId, Arc<testing::InMemoryFactoryConfigurationStore>) {
  preset_router_with_policy(Arc::new(TrustedNetworkManagementPolicy), None)
}
fn preset_router_with_policy(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  revoke: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> (axum::Router, ProjectId, Arc<testing::InMemoryFactoryConfigurationStore>) {
  preset_router_with_input(policy, revoke, None)
}
fn preset_router_with_input(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  revoke: Option<Arc<std::sync::atomic::AtomicBool>>,
  input: Option<serde_json::Value>,
) -> (axum::Router, ProjectId, Arc<testing::InMemoryFactoryConfigurationStore>) {
  let project = ProjectId::generate();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let peer = Arc::new(RecordingApplication::default());
  let metadata = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(PresetRepository(repository(project), input)),
  ));
  let handlers = Arc::new(
    FactoryPresetHandlers::new(store.clone(), policy.clone())
      .with_repositories(metadata.clone())
      .with_sources(metadata.clone())
      .with_source_actions(metadata.clone())
      .with_artifacts(Arc::new(PresetArtifacts(project, revoke))),
  );

  let configs =
    Arc::new(FactoryConfigurationHandlers::with_policy(store.clone(), policy.clone()).with_source_actions(metadata));
  let commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), peer.clone()));
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), peer.clone()));
  let factory = octacity_server_api_rest::v1::FactoryManagementApplication::new(
    Arc::new(AuthorizedCommandHandler::new(policy.clone(), configs.clone())),
    Arc::new(AuthorizedQueryHandler::new(policy.clone(), configs)),
    commands.clone(),
    queries.clone(),
    commands.clone(),
    queries,
    commands,
  );
  let app = support::recording_management_application_with_factory(peer, factory).with_factory_presets(
    octacity_server_api_rest::v1::FactoryPresetManagementApplication::new(Arc::new(AuthorizedQueryHandler::new(
      policy, handlers,
    ))),
  );
  (management_router_with_application(|| true, app), project, store)
}

async fn json(response: axum::response::Response) -> serde_json::Value {
  serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn presets_discovery_exposes_bounded_release_identity_without_factory_execution() {
  let (router, project, _) = preset_router();
  let response = router
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-presets?limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page = json(response).await;
  assert_eq!(page["total"], 1);
  assert_eq!(page["items"].as_array().unwrap().len(), 1);
  let preset = &page["items"][0];
  assert_eq!(preset["reference"]["identity"], "triage");
  assert_eq!(preset["reference"]["version"], "v1");
  assert_eq!(preset["reference"]["digest"].as_str().unwrap().len(), 64);
  let aliases = preset["profile_aliases"].as_array().unwrap();
  assert!(aliases.contains(&serde_json::json!("eligibility")));
  assert!(aliases.contains(&serde_json::json!("classification")));
  assert!(page["next_cursor"].is_null());
  assert_json_matches_component(&openapi_document(), "FactoryPresetPage", &page);
}

#[tokio::test]
async fn preset_resolution_returns_reviewable_process_criteria_and_missing_dependencies_without_publication() {
  use testing::MutationEvidenceProbe;
  let (router, project, store) = preset_router();
  let before = store.mutation_evidence_counts().await;
  let path = format!("/api/v1/projects/{project}/factory-presets");
  let page = json(router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap()).await;
  let body = serde_json::json!({"preset":page["items"][0]["reference"]});
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("{path}/resolve"),
      "unused-read-key",
      None,
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let result = json(response).await;
  assert_eq!(result["preset"], body["preset"]);
  assert_eq!(result["completion"]["kind"], "draft");
  assert_eq!(result["completion"]["project_id"], project.to_string());
  assert_eq!(result["completion"]["preset"]["identity"], "triage-completion");
  assert!(
    result["completion"]["content"]
      .as_str()
      .unwrap()
      .contains("not ticket-authored implementation acceptance")
  );
  for path in [
    "/profiles/eligibility",
    "/profiles/classification",
    "/repository",
    "/context",
  ] {
    assert!(
      result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["path"] == path),
      "{path}"
    );
  }
  assert!(result["flow"].is_null());
  assert!(result["configuration"].is_null());
  assert_eq!(store.mutation_evidence_counts().await, before);
  assert_json_matches_component(&openapi_document(), "FactoryPresetResolution", &result);
}

async fn selected_parameters(
  router: &axum::Router,
  project: ProjectId,
  store: &Arc<testing::InMemoryFactoryConfigurationStore>,
) -> serde_json::Value {
  let (choices, definition) = testing::installed_preset_choices(project);
  store.enable_project(project, choices).unwrap();
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let roles = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy)),
  );
  let mut profiles = serde_json::Map::new();
  for (index, alias) in ["eligibility", "classification"].into_iter().enumerate() {
    let mut intent = testing::role_profile_contract_intent(project);
    intent.profiles.truncate(1);
    intent.profiles[0].definition = definition.clone();
    let publication = roles
      .handle_command(
        &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
        PublishFactoryRoleProfilesCommand {
          intent,
          idempotency_key: serde_json::from_value(serde_json::json!(format!("preset-profile-{index}"))).unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    profiles.insert(alias.into(),serde_json::json!({"role":publication.role.unwrap().reference(),"profile":publication.profiles[0].reference(),
      "build":{"plugin":definition.toolchain.plugin,"tool":definition.toolchain.executable,"model_or_tool":definition.toolchain.model,
        "task_digest":"a".repeat(64),"build_configuration":definition.build_configuration,"result_output":"triage.result","result_output_kind":"report","result_outcome":"observed","evidence":[]}
    }));
  }
  let page = json(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{project}/factory-presets"),
        None,
      ))
      .await
      .unwrap(),
  )
  .await;
  let artifact = |byte: char| serde_json::json!({"artifact_id":uuid::Uuid::new_v4().to_string(),"content_digest":byte.to_string().repeat(64),"encoded_size":64});
  let context = serde_json::json!({"project_id":project,"goals":artifact('b'),"rules":artifact('c'),"documents":{}});
  let budget = serde_json::json!({"max_attempts":10,"max_elapsed_millis":300000,"max_tokens":40000,"max_cost_micro_units":4000000,"max_output_bytes":8388608});
  serde_json::json!({"preset":page["items"][0]["reference"],"flow_id":uuid::Uuid::new_v4().to_string(),"configuration_id":uuid::Uuid::new_v4().to_string(),
    "profiles":profiles,"repository":{"project_id":project,"id":repository(project).id,"version":1,"definition_digest":repository(project).definition_digest().unwrap()},
    "context":{"eligibility":context,"classification":context},
    "execution":{"budget":budget,"permissions":definition.permissions,"max_active_nodes":1},
    "configuration":{"admission_policy":"admission","permission_ceiling":"permissions","wip_limits":{"max_active_runs":1,"max_active_nodes":1},"hard_budget":budget}
  })
}

#[tokio::test]
async fn selected_preset_profiles_resolve_to_an_ordinary_immutable_flow_draft() {
  use testing::MutationEvidenceProbe;
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  assert_json_matches_component(&openapi_document(), "FactoryPresetResolutionRequest", &parameters);
  let before = store.mutation_evidence_counts().await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read-only-selection",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(result["diagnostics"], serde_json::json!([]), "{result}");
  let nodes = result["flow"]["root"]["nodes"].as_array().unwrap();
  assert_eq!(nodes.len(), 4);
  for alias in ["eligibility", "classification"] {
    let node = nodes.iter().find(|node| node["key"] == alias).unwrap();
    assert_eq!(node["kind"], "reasoning");
    assert_eq!(node["profile_selection"]["role"], parameters["profiles"][alias]["role"]);
    assert_eq!(
      node["profile_selection"]["candidates"]["selected"]["profile"]["reference"],
      parameters["profiles"][alias]["profile"]
    );
  }
  assert_eq!(result["configuration"]["enabled"], false);
  assert_eq!(
    result["configuration"]["flow"]["closure"]["root"]["id"],
    parameters["flow_id"]
  );
  assert_eq!(store.mutation_evidence_counts().await, before);
  assert_json_matches_component(
    &openapi_document(),
    "FactoryConfigurationDefinition",
    &result["configuration"],
  );
  assert_json_matches_component(&openapi_document(), "FactoryPresetResolution", &result);
}

#[tokio::test]
async fn preset_resolution_rejects_a_forged_repository_pin_without_publication() {
  use testing::MutationEvidenceProbe;
  let (router, project, store) = preset_router();
  let mut parameters = selected_parameters(&router, project, &store).await;
  parameters["repository"]["definition_digest"] = serde_json::json!("e".repeat(64));
  let before = store.mutation_evidence_counts().await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/repository","reason":"unavailable"}])
  );
  assert!(result["flow"].is_null());
  assert!(result["configuration"].is_null());
  assert_eq!(store.mutation_evidence_counts().await, before);
}

// External metadata authority fixture. It exposes no provider or credential operation.
struct PresetRepository(RepositoryProjection, Option<serde_json::Value>);
#[async_trait]
impl ManagementQueryUseCase<GetRepositoryQuery> for PresetRepository {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetRepositoryQuery,
  ) -> Result<RepositoryProjection, ApplicationError> {
    if query.repository_id == self.0.id && query.version == self.0.version {
      Ok(self.0.clone())
    } else {
      Err(ApplicationError::invalid())
    }
  }
}
fn repository(project: ProjectId) -> RepositoryProjection {
  serde_json::from_value(serde_json::json!({"id":project,"project_id":project,"name":"source","version":1,"vcs_integration_id":"55555555-5555-4555-8555-555555555555","repository_locator":"https://example.test/source.git","selection":{"allowed_references":["refs/heads/main"],"default_reference":"refs/heads/main","allow_exact_revision":true},"published_at":1})).unwrap()
}

#[tokio::test]
async fn preset_resolution_checks_exact_context_metadata_without_fetching_object_bytes() {
  let (router, project, store) = preset_router();
  let mut parameters = selected_parameters(&router, project, &store).await;
  parameters["context"]["eligibility"]["goals"]["content_digest"] = serde_json::json!("f".repeat(64));
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/context/eligibility/goals","reason":"unavailable"}])
  );
  assert!(result["flow"].is_null());
}

struct PresetArtifacts(ProjectId, Option<Arc<std::sync::atomic::AtomicBool>>);
#[async_trait]
impl IssueArtifactReader for PresetArtifacts {
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    if request.project_id != self.0
      || !["b".repeat(64), "c".repeat(64)].contains(&request.artifact.content_digest().to_string())
    {
      return Err(ApplicationError::invalid());
    }
    if let Some(visible) = &self.1 {
      visible.store(false, Ordering::SeqCst);
    }
    Ok(IssueArtifactMetadata {
      name: "project document".into(),
      media_type: "text/plain".into(),
      origin: IssueArtifactOrigin::ProjectInput,
      availability: IssueArtifactAvailability::Available,
      published_at: Some(Timestamp::from_unix_millis(1).unwrap()),
      expires_at: None,
    })
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("preset resolution must never fetch object bytes")
  }
}

struct RevocablePresetPolicy(Arc<std::sync::atomic::AtomicBool>);
#[async_trait]
impl ManagementAuthorizationPolicy for RevocablePresetPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::Project && !self.0.load(Ordering::SeqCst) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn preset_resolution_rechecks_project_visibility_after_metadata_reads() {
  let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
  let (router, project, store) =
    preset_router_with_policy(Arc::new(RevocablePresetPolicy(visible.clone())), Some(visible));
  let parameters = selected_parameters(&router, project, &store).await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert!(
    !response.status().is_success(),
    "a revoked Project must not disclose the resolved private profile definitions"
  );
}

/// Qualification of the existing publication owners: resolution creates no alternate mutation path.
#[tokio::test]
async fn resolved_preset_uses_ordinary_confirmed_versioned_publication_and_current_dependencies() {
  use testing::MutationEvidenceProbe;
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  for (path, identity, definition) in [(
    "factory-configurations",
    &parameters["configuration_id"],
    &result["configuration"],
  )] {
    let body = serde_json::json!({"id":identity,"project_id":project,"definition":definition}).to_string();
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/{path}/validate"),
        "read",
        None,
        &body,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{}", json(response).await);
    let response = router
      .clone()
      .oneshot(json_request("POST", &format!("/api/v1/{path}"), path, None, &body))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "{}", json(response).await);
    let receipt = json(response).await;
    assert_eq!(receipt["disposition"], "applied");
    let before = store.mutation_evidence_counts().await;
    let replay = router
      .clone()
      .oneshot(json_request("POST", &format!("/api/v1/{path}"), path, None, &body))
      .await
      .unwrap();
    assert!(replay.status().is_success(), "{}", json(replay).await);
    assert_eq!(json(replay).await["disposition"], "replayed");
    assert_eq!(store.mutation_evidence_counts().await, before);
    let version_path = format!("/api/v1/{path}/{}/versions", identity.as_str().unwrap());
    let replacement = serde_json::json!({"definition":definition}).to_string();
    assert_eq!(
      router
        .clone()
        .oneshot(json_request("POST", &version_path, "next", None, &replacement))
        .await
        .unwrap()
        .status(),
      StatusCode::PRECONDITION_REQUIRED
    );
    let next = router
      .clone()
      .oneshot(json_request("POST", &version_path, "next", Some("\"1\""), &replacement))
      .await
      .unwrap();
    assert_eq!(next.status(), StatusCode::CREATED, "{}", json(next).await);
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &version_path,
          "stale",
          Some("\"1\""),
          &replacement
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::PRECONDITION_FAILED
    );
  }
  store.disable_project(project).unwrap();
  let before = store.mutation_evidence_counts().await;
  for (path, identity, definition) in [(
    "factory-configurations",
    &parameters["configuration_id"],
    &result["configuration"],
  )] {
    let body = serde_json::json!({"id":identity,"project_id":project,"definition":definition}).to_string();
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/{path}/validate"),
        "read",
        None,
        &body,
      ))
      .await
      .unwrap();
    assert!(
      !response.status().is_success(),
      "a prior draft cannot authorize unavailable current parents"
    );
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/{path}/{}/versions", identity.as_str().unwrap()),
        "revoked",
        Some("\"2\""),
        &serde_json::json!({"definition":definition}).to_string(),
      ))
      .await
      .unwrap();
    assert!(!response.status().is_success());
  }
  assert_eq!(store.mutation_evidence_counts().await, before);
}

#[tokio::test]
async fn preset_discovery_advertises_typed_parameters_and_required_technical_node_kinds() {
  let (router, project, _) = preset_router();
  let response = router
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-presets"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let page = json(response).await;
  let summary = &page["items"][0];
  assert_eq!(
    summary["required_node_kinds"],
    serde_json::json!(["reasoning", "deterministic_gate"])
  );
  let parameters = summary["parameters"].as_array().expect("typed parameter requirements");
  for alias in ["eligibility", "classification"] {
    assert!(
      parameters
        .contains(&serde_json::json!({"path":format!("/profiles/{alias}"),"kind":"role_profile","required":true}))
    );
    assert!(
      parameters
        .contains(&serde_json::json!({"path":format!("/context/{alias}"),"kind":"project_context","required":true}))
    );
  }
  assert_json_matches_component(&openapi_document(), "FactoryPresetPage", &page);
}

fn input_selection() -> serde_json::Value {
  let reference = |identity: &str| serde_json::json!({"identity":identity,"version":"v1","digest":"9".repeat(64)});
  serde_json::json!({"id":uuid::Uuid::new_v4().to_string(), "connection":{"id":uuid::Uuid::new_v4().to_string(),"version":1,"access_version":1,"source_configuration":reference("read"),"reporter_configuration":reference("write")},"source":{"installation":reference("tracker"),"capability":reference("source"),"configuration":reference("read"),"resource":reference("tickets")},"reporter":null,"reporting":null,"predicate":null})
}
#[tokio::test]
async fn preset_resolution_reports_unavailable_source_selection_without_enabling_intake() {
  let (router, project, store) = preset_router();
  let mut parameters = selected_parameters(&router, project, &store).await;
  parameters["input"] = input_selection();
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/input","reason":"unavailable"}])
  );
  assert!(result["configuration"].is_null());
  assert!(result["input"].is_null());
}

#[tokio::test]
async fn preset_http_contract_bounds_discovery_and_rejects_authority_or_unsupported_fields() {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  let path = format!("/api/v1/projects/{project}/factory-presets");
  for key in ["enabled", "credentials", "grant", "flow", "model", "provider_url"] {
    let mut invalid = parameters.clone();
    invalid[key] = serde_json::json!(true);
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &format!("{path}/resolve"),
          "read",
          None,
          &invalid.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST,
      "unsupported {key}"
    );
  }
  for suffix in ["?limit=0", "?limit=65535", "?unsupported=true"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}{suffix}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  let page = json(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}?after=triage&limit=1"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(page["total"], 1);
  assert_eq!(page["items"], serde_json::json!([]));
  let response = router
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{}/factory-presets", ProjectId::generate()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[async_trait]
impl ManagementQueryUseCase<ListWorkSourceInputChoicesQuery> for PresetRepository {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListWorkSourceInputChoicesQuery,
  ) -> Result<WorkSourceInputChoices, ApplicationError> {
    let rows = self.1.clone().ok_or_else(ApplicationError::capability_unavailable)?;
    let rows = match rows {
      serde_json::Value::Array(rows) => rows,
      row => vec![row],
    };
    let selection: FactoryPresetInputSelection = rows
      .into_iter()
      .map(|row| serde_json::from_value::<FactoryPresetInputSelection>(row).unwrap())
      .find(|row| row.connection.id == query.connection_id)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if query.project_id != self.0.project_id
      || query.connection_id != selection.connection.id
      || query.connection_version != selection.connection.version
    {
      return Err(ApplicationError::invalid());
    }
    let choice = |binding, purpose, configuration, reporting| WorkSourceInputChoice {
      binding,
      purpose,
      connection_configuration: configuration,
      reporting,
      native_id: serde_json::from_value(serde_json::json!("tickets")).unwrap(),
      locator: serde_json::from_value(serde_json::json!("https://example.test/tickets")).unwrap(),
      selection: None,
      predicate_states: None,
      predicate_label_presence: None,
    };
    let mut items = vec![choice(
      selection.source,
      IntegrationPurpose::TaskRead,
      selection.connection.source_configuration.clone(),
      None,
    )];
    if let Some(writer) = selection.reporter {
      items.push(choice(
        writer,
        IntegrationPurpose::TaskUpdate,
        selection.connection.reporter_configuration.clone().unwrap(),
        Some(WorkSourceReportingChoice {
          mapping: selection.reporting.unwrap(),
          editable: Some(
            serde_json::from_value(serde_json::json!({"labels":["triage:reviewed"],"states":[],"ownership":null}))
              .unwrap(),
          ),
        }),
      ));
    }
    Ok(WorkSourceInputChoices {
      connection_id: selection.connection.id,
      connection_version: selection.connection.version,
      access_version: selection.connection.access_version,
      items,
    })
  }
}
#[tokio::test]
async fn selected_source_and_literal_effects_resolve_to_an_ordinary_disabled_input() {
  use testing::MutationEvidenceProbe;
  let input = reviewed_input_selection();
  let (router, project, store) =
    preset_router_with_input(Arc::new(TrustedNetworkManagementPolicy), None, Some(input.clone()));
  let mut parameters = selected_parameters(&router, project, &store).await;
  parameters["input"] = input.clone();
  let before = store.mutation_evidence_counts().await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(result["diagnostics"], serde_json::json!([]), "{result}");
  assert_eq!(result["input"]["id"], input["id"]);
  assert_eq!(result["input"]["definition"]["enabled"], false);
  assert_eq!(
    result["input"]["definition"]["target"]["id"],
    parameters["configuration_id"]
  );
  assert_eq!(result["input"]["definition"]["repository"], parameters["repository"]);
  assert_eq!(result["input"]["definition"]["reporting"], input["reporting"]);
  assert!(result["input"]["expected_version"].is_null());
  assert_eq!(store.mutation_evidence_counts().await, before);
  assert_json_matches_component(&openapi_document(), "FactoryPresetResolution", &result);
}

#[tokio::test]
async fn preset_resolution_reports_the_unavailable_exact_profile_parameter_without_private_details() {
  let (router, project, store) = preset_router();
  let mut parameters = selected_parameters(&router, project, &store).await;
  parameters["profiles"]["eligibility"]["profile"]["digest"] = serde_json::json!("e".repeat(64));
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/profiles/eligibility","reason":"unavailable"}])
  );
  assert!(result["flow"].is_null());
  assert!(!result.to_string().contains("local_permissions"));
}

#[tokio::test]
async fn preset_resolution_rejects_an_unpaired_connection_writer_pin_like_normal_publication() {
  let input = input_selection();
  let (router, project, store) =
    preset_router_with_input(Arc::new(TrustedNetworkManagementPolicy), None, Some(input.clone()));
  let mut parameters = selected_parameters(&router, project, &store).await;
  parameters["input"] = input;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/input","reason":"unavailable"}])
  );
  assert!(result["input"].is_null());
}

struct RevocableArtifactPolicy(Arc<std::sync::atomic::AtomicBool>);
#[async_trait]
impl ManagementAuthorizationPolicy for RevocableArtifactPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::Artifact && !self.0.load(Ordering::SeqCst) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn preset_resolution_rechecks_individual_artifact_visibility_after_metadata_reads() {
  let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
  let (router, project, store) =
    preset_router_with_policy(Arc::new(RevocableArtifactPolicy(visible.clone())), Some(visible));
  let parameters = selected_parameters(&router, project, &store).await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert!(
    result["diagnostics"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!({"path":"/context/classification/goals","reason":"unavailable"})),
    "{result}"
  );
  assert!(result["flow"].is_null());
}

fn reviewed_input_selection() -> serde_json::Value {
  let mut input = input_selection();
  input["reporter"] = input["source"].clone();
  input["reporter"]["capability"]["identity"] = serde_json::json!("reporter");
  input["reporter"]["configuration"]["identity"] = serde_json::json!("write");
  input["reporting"] =
    serde_json::json!({"statuses":[{"status":"reviewed","add":["triage:reviewed"],"remove":[],"state":null}]});
  input
}

async fn effect_parameters(
  project: ProjectId,
  store: &Arc<testing::InMemoryFactoryConfigurationStore>,
  parameters: &mut serde_json::Value,
  input: &serde_json::Value,
) -> serde_json::Value {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let schema = AuthorizedQueryHandler::new(policy.clone(), Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy))).handle_query(
    &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
    ValidateFactoryFlowDataSchemaQuery { project_id:project, identity:"report.receipt".parse().unwrap(), version:"v1".parse().unwrap(), shape:serde_json::from_value(serde_json::json!({"type":"object","fields":{"receipt":{"required":true,"value":{"type":"string","min_bytes":64,"max_bytes":64}}}})).unwrap() }).await.unwrap();
  let mut permissions = parameters["execution"]["permissions"].clone();
  permissions["plugins"] = serde_json::json!([input["reporter"]["capability"]]);
  parameters["execution"]["permissions"] = permissions.clone();
  parameters["execution"]["budget"]["max_attempts"] = serde_json::json!(512);
  parameters["configuration"]["hard_budget"]["max_attempts"] = serde_json::json!(512);
  permissions["executables"] = serde_json::json!([]);
  permissions["commands"] = serde_json::json!([]);
  let effect = serde_json::json!({"status":"reviewed","add":["triage:reviewed"],"remove":[],"state":null});
  serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"definition":{
    "target":{"source":input["source"],"reporter":input["reporter"],"connection":input["connection"],"policy":{"labels":["triage:reviewed"],"states":[],"ownership":null}},
    "outcomes":{"classify":null,"duplicate":effect,"review":effect,"out_of_scope":effect,"needs_input":effect},"dimensions":[],"receipt":schema,"action_outcome":"queued",
    "budget":{"max_attempts":1,"max_elapsed_millis":1000,"max_tokens":0,"max_cost_micro_units":0,"max_output_bytes":1024},
    "execution":{"budget":{"max_attempts":64,"max_elapsed_millis":60000,"max_tokens":0,"max_cost_micro_units":0,"max_output_bytes":65536},"permissions":permissions,"max_active_nodes":1},
    "retry":{"max_attempts":2,"retry_delay_millis":5}
  }})
}

#[tokio::test]
async fn selected_accepted_gate_effects_resolve_to_ordinary_reporting_flows_and_publish_normally() {
  use testing::MutationEvidenceProbe;
  let input = reviewed_input_selection();
  let (router, project, store) =
    preset_router_with_input(Arc::new(TrustedNetworkManagementPolicy), None, Some(input.clone()));
  let mut parameters = selected_parameters(&router, project, &store).await;
  let reporting = effect_parameters(project, &store, &mut parameters, &input).await;
  parameters["effects"] = serde_json::json!({"eligibility_gate":reporting});
  parameters["input"] = input;
  let before = store.mutation_evidence_counts().await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK, "{}", json(response).await);
  let result = json(response).await;
  assert_eq!(result["diagnostics"], serde_json::json!([]), "{result}");
  assert_eq!(result["dependencies"].as_array().unwrap().len(), 1);
  assert_eq!(result["dependencies"][0]["id"], reporting["id"]);
  let child = &result["dependencies"][0]["definition"];
  let actions: Vec<_> = child["root"]["nodes"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|n| n["kind"] == "trusted_action")
    .collect();
  assert_eq!(actions.len(), 4);
  assert!(actions.iter().all(|n| n["action"]["action"] == "set_status"));
  assert!(
    actions
      .iter()
      .all(|n| n["action"]["parameters"]["source_update"]["effects"]["add"] == serde_json::json!(["triage:reviewed"]))
  );
  assert_eq!(
    result["configuration"]["flow"]["closure"]["definitions"]
      .as_array()
      .unwrap()
      .len(),
    2
  );
  assert_eq!(store.mutation_evidence_counts().await, before);
  assert_json_matches_component(&openapi_document(), "FactoryPresetResolution", &result);
  let body =
    serde_json::json!({"id":parameters["configuration_id"],"project_id":project,"definition":result["configuration"]})
      .to_string();
  let response = router
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations",
      "reporting-config",
      None,
      &body,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED, "{}", json(response).await);
}

#[tokio::test]
async fn preset_resolution_names_an_unavailable_factory_installation_as_a_dependency() {
  let (router, project, store) = preset_router();
  let parameters = selected_parameters(&router, project, &store).await;
  store.disable_project(project).unwrap();
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/execution","reason":"unavailable"}])
  );
  assert!(result["flow"].is_null());
}

#[tokio::test]
async fn preset_effects_cannot_resolve_against_another_selected_input_connection() {
  let input = reviewed_input_selection();
  let other_input = reviewed_input_selection();
  let (router, project, store) = preset_router_with_input(
    Arc::new(TrustedNetworkManagementPolicy),
    None,
    Some(serde_json::json!([input, other_input])),
  );
  let mut parameters = selected_parameters(&router, project, &store).await;
  let reporting = effect_parameters(project, &store, &mut parameters, &other_input).await;
  parameters["effects"] = serde_json::json!({"eligibility_gate":reporting});
  parameters["input"] = input;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/effects","reason":"unavailable"}])
  );
  assert!(result["configuration"].is_null());
  assert!(result["input"].is_null());
}

struct HiddenPresetRepository;
#[async_trait]
impl ManagementAuthorizationPolicy for HiddenPresetRepository {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::Repository {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn preset_resolution_returns_a_safe_dependency_reason_for_a_hidden_repository() {
  let (router, project, store) = preset_router_with_policy(Arc::new(HiddenPresetRepository), None);
  let parameters = selected_parameters(&router, project, &store).await;
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = json(response).await;
  assert_eq!(
    result["diagnostics"],
    serde_json::json!([{"path":"/repository","reason":"unavailable"}])
  );
  assert!(result["configuration"].is_null());
}

#[tokio::test]
async fn preset_resolution_rejects_root_and_child_flow_identity_collisions() {
  let input = reviewed_input_selection();
  let (router, project, store) =
    preset_router_with_input(Arc::new(TrustedNetworkManagementPolicy), None, Some(input.clone()));
  let mut parameters = selected_parameters(&router, project, &store).await;
  let mut reporting = effect_parameters(project, &store, &mut parameters, &input).await;
  reporting["id"] = parameters["flow_id"].clone();
  parameters["effects"] = serde_json::json!({"eligibility_gate":reporting});
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let error = json(response).await;
  assert_eq!(error["diagnostics"]["errors"][0]["field"], "/effects");
}

#[tokio::test]
async fn preset_resolution_rejects_distinct_reporting_children_with_flow_identity_collisions() {
  let input = reviewed_input_selection();
  let (router, project, store) =
    preset_router_with_input(Arc::new(TrustedNetworkManagementPolicy), None, Some(input.clone()));
  let mut parameters = selected_parameters(&router, project, &store).await;
  let baseline = json(
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
  let reporting = effect_parameters(project, &store, &mut parameters, &input).await;
  let mut second = reporting.clone();
  let outcomes = baseline["flow"]["root"]["nodes"]
    .as_array()
    .unwrap()
    .iter()
    .find(|n| n["key"] == "classification_gate")
    .unwrap()["outcomes"]
    .as_array()
    .unwrap();
  second["definition"]["outcomes"] = serde_json::Value::Object(
    outcomes
      .iter()
      .map(|o| {
        (
          o["key"].as_str().unwrap().to_owned(),
          serde_json::json!({"status":"reviewed","add":["triage:reviewed"],"remove":[],"state":null}),
        )
      })
      .collect(),
  );
  parameters["effects"] = serde_json::json!({"eligibility_gate":reporting,"classification_gate":second});
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-presets/resolve"),
      "read",
      None,
      &parameters.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(json(response).await["diagnostics"]["errors"][0]["field"], "/effects");
}
