use super::*;

#[derive(Debug, Eq, PartialEq)]
struct AuthorizationInventoryError {
  duplicates: Vec<&'static str>,
  missing: Vec<&'static str>,
  unexpected: Vec<&'static str>,
}

fn validate_authorization_inventory(
  registered: impl IntoIterator<Item = &'static str>,
  mapped: impl IntoIterator<Item = &'static str>,
) -> Result<(), AuthorizationInventoryError> {
  let registered = registered.into_iter().collect::<BTreeSet<_>>();
  let mut mappings = BTreeSet::new();
  let mut duplicates = Vec::new();
  for operation_id in mapped {
    if !mappings.insert(operation_id) {
      duplicates.push(operation_id);
    }
  }

  let missing = registered.difference(&mappings).copied().collect::<Vec<_>>();
  let unexpected = mappings.difference(&registered).copied().collect::<Vec<_>>();
  if duplicates.is_empty() && missing.is_empty() && unexpected.is_empty() {
    Ok(())
  } else {
    Err(AuthorizationInventoryError {
      duplicates,
      missing,
      unexpected,
    })
  }
}

fn documented_operation_inventory(document: &serde_json::Value) -> BTreeSet<(String, String, String)> {
  document["paths"]
    .as_object()
    .unwrap()
    .iter()
    .flat_map(|(path, item)| {
      item
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, operation)| operation["operationId"] != "getOpenApiDocument")
        .map(move |(method, operation)| {
          (
            method.to_ascii_uppercase(),
            path.clone(),
            operation["operationId"].as_str().unwrap().to_owned(),
          )
        })
    })
    .collect()
}

fn registered_operation_inventory() -> BTreeSet<(String, String, String)> {
  management_authorization_operations()
    .into_iter()
    .map(|operation| {
      (
        operation.method.to_owned(),
        operation.path.to_owned(),
        operation.operation_id.to_owned(),
      )
    })
    .collect()
}

fn assert_forbidden_contract(document: &serde_json::Value) {
  for operation in MANAGEMENT_OPERATIONS {
    let method = operation.method.to_ascii_lowercase();
    assert_eq!(
      document["paths"][operation.path][method]["responses"]["403"]["$ref"],
      "#/components/responses/ManagementForbidden",
      "{} must document its forbidden response",
      operation.operation_id
    );
  }
  let forbidden_content = &document["components"]["responses"]["ManagementForbidden"]["content"]["application/json"];
  assert_eq!(
    forbidden_content["schema"]["$ref"],
    "#/components/schemas/ForbiddenErrorResponse"
  );
  let expected = serde_json::json!({
      "code": "forbidden",
      "message": "management operation is forbidden",
      "request_id": "33333333-3333-4333-8333-333333333333"
  });
  assert_eq!(forbidden_content["example"], expected);
  assert_json_matches_component(document, "ForbiddenErrorResponse", &expected);
}

fn collect_local_references<'a>(value: &'a serde_json::Value, references: &mut Vec<&'a str>) {
  match value {
    serde_json::Value::Array(values) => {
      for value in values {
        collect_local_references(value, references);
      }
    }
    serde_json::Value::Object(object) => {
      if let Some(reference) = object.get("$ref").and_then(serde_json::Value::as_str)
        && reference.starts_with("#/")
      {
        references.push(reference);
      }
      for value in object.values() {
        collect_local_references(value, references);
      }
    }
    _ => {}
  }
}

#[test]
fn authorization_inventory_rejects_missing_and_duplicate_mappings() {
  assert_eq!(
    validate_authorization_inventory(["first", "second"], ["first"]),
    Err(AuthorizationInventoryError {
      duplicates: Vec::new(),
      missing: vec!["second"],
      unexpected: Vec::new(),
    })
  );
  assert_eq!(
    validate_authorization_inventory(["first", "second"], ["first", "second", "second"]),
    Err(AuthorizationInventoryError {
      duplicates: vec!["second"],
      missing: Vec::new(),
      unexpected: Vec::new(),
    })
  );
}

#[tokio::test]
async fn openapi_document_cannot_drift_from_registered_routes_and_v1_dtos() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .clone()
    .oneshot(empty_request("GET", "/api/v1/openapi.json", None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let document: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(document["openapi"], "3.1.0");
  assert_eq!(document["security"], serde_json::json!([]));
  assert_eq!(
    document["x-octacity-management-security"],
    serde_json::json!({
      "mode": "trusted_network_unauthenticated",
      "operator_authentication": false,
      "network_isolation_required": true,
      "agent_and_webhook_authentication_unchanged": true
    })
  );

  let documented = documented_operation_inventory(&document);
  let registered = management_authorization_operations();
  let registered_contract = registered_operation_inventory();
  assert_eq!(documented, registered_contract);
  assert_forbidden_contract(&document);
  validate_authorization_inventory(
    MANAGEMENT_OPERATIONS.iter().map(|operation| operation.operation_id),
    registered.iter().map(|operation| operation.operation_id),
  )
  .expect("every registered management operation must have exactly one typed authorization mapping");
  assert_eq!(
    registered
      .iter()
      .map(|operation| (operation.method, operation.path, operation.operation_id))
      .collect::<BTreeSet<_>>(),
    MANAGEMENT_OPERATIONS
      .iter()
      .map(|operation| (operation.method, operation.path, operation.operation_id))
      .collect::<BTreeSet<_>>()
  );
  for operation in &registered {
    assert!(operation.authorization.is_supported());
  }
  let create_project = registered
    .iter()
    .find(|operation| operation.operation_id == "createProject")
    .unwrap();
  assert_eq!(
    create_project.authorization,
    <CreateProjectCommand as ManagementAuthorizationTarget>::AUTHORIZATION
  );
  let search = registered
    .iter()
    .find(|operation| operation.operation_id == "searchResources")
    .unwrap();
  assert_eq!(
    search.authorization,
    <SearchResourcesQuery as ManagementAuthorizationTarget>::AUTHORIZATION
  );
  assert_eq!(search.authorization.action(), ManagementAction::Search);
  let attention = registered
    .iter()
    .find(|operation| operation.operation_id == "listOperatorAttention")
    .unwrap();
  assert_eq!(
    attention.authorization,
    <ListOperatorAttentionQuery as ManagementAuthorizationTarget>::AUTHORIZATION
  );
  assert_eq!(attention.authorization.action(), ManagementAction::View);
  for (operation_id, expected, action) in [
    (
      "observeManagedWebhookIntegration",
      <ObserveManagedWebhookRegistrationCommand as ManagementAuthorizationTarget>::AUTHORIZATION,
      ManagementAction::View,
    ),
    (
      "rotateManagedWebhookIntegration",
      <RotateManagedWebhookRegistrationCommand as ManagementAuthorizationTarget>::AUTHORIZATION,
      ManagementAction::Update,
    ),
    (
      "deleteManagedWebhookIntegration",
      <DeleteManagedWebhookRegistrationCommand as ManagementAuthorizationTarget>::AUTHORIZATION,
      ManagementAction::Delete,
    ),
  ] {
    let authorization = registered
      .iter()
      .find(|operation| operation.operation_id == operation_id)
      .unwrap()
      .authorization;
    assert_eq!(authorization, expected);
    assert_eq!(authorization.action(), action);
  }

  for operation in MANAGEMENT_OPERATIONS {
    let method = operation.method.to_ascii_lowercase();
    let documented = &document["paths"][operation.path][&method];
    assert_eq!(documented["operationId"], operation.operation_id);
    assert_eq!(documented["security"], serde_json::json!([]));
    assert_eq!(
      documented["responses"][operation.success_status]["content"]["application/json"]["schema"]["$ref"],
      format!("#/components/schemas/{}", operation.response_schema)
    );
    assert_component_exists(&document, operation.response_schema);
    if let Some(request_schema) = operation.request_schema {
      assert_eq!(
        documented["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        format!("#/components/schemas/{request_schema}")
      );
      assert_component_exists(&document, request_schema);
      assert_eq!(
        documented["responses"]["413"]["$ref"],
        "#/components/responses/ManagementError"
      );
      assert_eq!(
        documented["responses"]["415"]["$ref"],
        "#/components/responses/ManagementError"
      );
    }
    for status in ["400", "404", "409", "500", "503"] {
      assert_eq!(
        documented["responses"][status]["$ref"],
        "#/components/responses/ManagementError"
      );
    }
    assert_eq!(
      documented["responses"]["429"]["$ref"],
      "#/components/responses/ManagementRateLimited"
    );
    assert_eq!(
      document["components"]["responses"]["ManagementRateLimited"]["headers"]["Retry-After"]["schema"]["minimum"],
      1
    );
    if operation.operation_id.contains("ManagedWebhook") {
      assert_eq!(
        documented["responses"]["422"]["$ref"],
        "#/components/responses/ManagementError"
      );
    }
    assert_required_header(documented, "Idempotency-Key", operation.idempotent_mutation);
    assert_required_header(documented, "If-Match", operation.optimistic_precondition);

    let response = routes
      .clone()
      .oneshot(
        Request::builder()
          .method(Method::from_bytes(operation.method.as_bytes()).unwrap())
          .uri(concrete_path(operation.path))
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_ne!(
      response.status(),
      StatusCode::NOT_FOUND,
      "{} {}",
      operation.method,
      operation.path
    );
    assert_ne!(
      response.status(),
      StatusCode::METHOD_NOT_ALLOWED,
      "{} {}",
      operation.method,
      operation.path
    );
  }

  let managed_response = &document["components"]["schemas"]["ManagedWebhookResource"]["properties"];
  assert!(managed_response.get("administration_credential_handle").is_none());
  assert!(managed_response.get("verification_material_handle").is_none());

  let artifact = document["components"]["schemas"]["ArtifactResource"]["properties"]
    .as_object()
    .unwrap();
  let download = document["components"]["schemas"]["ArtifactDownload"]["properties"]
    .as_object()
    .unwrap();
  for provider_field in [
    "bucket",
    "key",
    "object_key",
    "credential",
    "access_key",
    "etag",
    "generation",
    "url",
  ] {
    assert!(artifact.get(provider_field).is_none());
    assert!(download.get(provider_field).is_none());
  }
  let cache_session = document["components"]["schemas"]["CacheSessionResource"]["properties"]
    .as_object()
    .unwrap();
  for secret_field in [
    "credential",
    "credential_hash",
    "bearer",
    "bearer_token",
    "scope_id",
    "fencing_token",
  ] {
    assert!(cache_session.get(secret_field).is_none());
  }
  let error_codes = [
    ErrorCode::InvalidRequest,
    ErrorCode::UnsupportedMediaType,
    ErrorCode::UnsupportedApiVersion,
    ErrorCode::PayloadTooLarge,
    ErrorCode::InvalidIdempotencyKey,
    ErrorCode::IdempotencyConflict,
    ErrorCode::PreconditionRequired,
    ErrorCode::PreconditionFailed,
    ErrorCode::Forbidden,
    ErrorCode::NotFound,
    ErrorCode::Conflict,
    ErrorCode::CapabilityUnavailable,
    ErrorCode::Unavailable,
    ErrorCode::RateLimited,
    ErrorCode::Internal,
  ]
  .map(|code| serde_json::to_value(code).unwrap());
  assert_eq!(
    document["components"]["schemas"]["ErrorCode"]["enum"],
    serde_json::Value::Array(error_codes.into())
  );
  assert_eq!(
    document["components"]["responses"]["ManagementError"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/ErrorResponse"
  );
  let list_limit = document["paths"]["/api/v1/projects"]["get"]["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|parameter| parameter["name"] == "limit")
    .unwrap();
  assert_eq!(list_limit["schema"]["maximum"], 200);
  let cache_limit = document["paths"]["/api/v1/builds/{build_id}/cache-sessions"]["get"]["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|parameter| parameter["name"] == "limit")
    .unwrap();
  assert_eq!(cache_limit["schema"]["maximum"], 100);
  let log_search = &document["paths"]["/api/v1/projects/{project_id}/build-logs/search"]["get"];
  assert_eq!(log_search["operationId"], "searchBuildLogs");
  let search_parameters = log_search["parameters"].as_array().unwrap();
  let query = search_parameters
    .iter()
    .find(|parameter| parameter["name"] == "query")
    .unwrap();
  assert_eq!(query["required"], true);
  assert_eq!(query["schema"]["maxLength"], 1024);
  let mode = search_parameters
    .iter()
    .find(|parameter| parameter["name"] == "mode")
    .unwrap();
  assert_eq!(mode["schema"]["$ref"], "#/components/schemas/BuildLogSearchMode");
  assert_eq!(
    document["components"]["schemas"]["BuildLogSearchMode"]["enum"],
    serde_json::json!(["full_text", "literal"])
  );
  let log_page = document["components"]["schemas"]["BuildLogSearchPage"]["properties"]
    .as_object()
    .unwrap();
  assert!(log_page.contains_key("freshness"));
  let log_hit = document["components"]["schemas"]["BuildLogSearchHit"]["properties"]
    .as_object()
    .unwrap();
  for backend_field in ["bucket", "object_key", "tsvector", "postgres_rank"] {
    assert!(log_hit.get(backend_field).is_none());
  }
  assert_eq!(
    document["components"]["schemas"]["PlaceBuildResultHoldRequest"]["properties"]["reason"]["x-max-utf8-bytes"],
    512
  );
  assert!(
    document["components"]["schemas"]["PlaceBuildResultHoldRequest"]["properties"]["reason"]
      .get("maxLength")
      .is_none()
  );
  assert!(
    document["paths"]["/api/v1/builds/{build_id}/retention/hold/release"]["post"]["responses"]
      .get("412")
      .is_some()
  );
  assert!(
    document["paths"]["/api/v1/projects/{project_id}/rename"]["post"]["responses"]
      .get("412")
      .is_none(),
    "routes that classify stale versions as conflict must not advertise 412"
  );
  let retention = document["components"]["schemas"]["BuildResultRetentionResource"]["properties"]
    .as_object()
    .unwrap();
  for backend_field in [
    "bucket",
    "object_key",
    "credential",
    "storage_provider",
    "retention_work_id",
  ] {
    assert!(retention.get(backend_field).is_none());
  }

  let audit = &document["paths"]["/api/v1/audit-facts"]["get"];
  assert_eq!(audit["operationId"], "listAuditFacts");
  assert_eq!(
    document["components"]["schemas"]["AuditActorKind"]["enum"],
    serde_json::json!([
      "unauthenticated_management",
      "authenticated_management",
      "agent",
      "trigger",
      "orchestrator",
      "adapter",
      "worker"
    ])
  );
  let audit_limit = audit["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|parameter| parameter["name"] == "limit")
    .unwrap();
  assert_eq!(audit_limit["schema"]["maximum"], 200);
  let audit_fact = document["components"]["schemas"]["AuditFactResource"]["properties"]
    .as_object()
    .unwrap();
  for forbidden in ["request_body", "credential", "secret", "raw_payload"] {
    assert!(audit_fact.get(forbidden).is_none());
  }

  for (path, operation_id, page_schema, fixture) in [
    (
      "/api/v1/projects/{project_id}/pipelines",
      "listProjectPipelines",
      "PipelineSummaryPage",
      include_str!("../../fixtures/v1/pipeline-summary-page.json"),
    ),
    (
      "/api/v1/projects/{project_id}/repositories",
      "listProjectRepositories",
      "RepositorySummaryPage",
      include_str!("../../fixtures/v1/repository-summary-page.json"),
    ),
    (
      "/api/v1/projects/{project_id}/build-configurations",
      "listProjectBuildConfigurations",
      "BuildConfigurationSummaryPage",
      include_str!("../../fixtures/v1/build-configuration-summary-page.json"),
    ),
    (
      "/api/v1/projects/{project_id}/trigger-definitions",
      "listProjectTriggerDefinitions",
      "TriggerDefinitionSummaryPage",
      include_str!("../../fixtures/v1/trigger-definition-summary-page.json"),
    ),
  ] {
    let operation = &document["paths"][path]["get"];
    assert_eq!(operation["operationId"], operation_id);
    assert_eq!(
      operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
      format!("#/components/schemas/{page_schema}")
    );
    let parameters = operation["parameters"].as_array().unwrap();
    assert!(parameters.iter().any(|parameter| parameter["name"] == "project_id"));
    assert!(parameters.iter().any(|parameter| parameter["name"] == "after"));
    let limit = parameters
      .iter()
      .find(|parameter| parameter["name"] == "limit")
      .unwrap();
    assert_eq!(limit["schema"]["maximum"], 200);
    assert_json_matches_component(&document, page_schema, &serde_json::from_str(fixture).unwrap());
  }

  let resource_search = &document["paths"]["/api/v1/search"]["get"];
  assert_eq!(resource_search["operationId"], "searchResources");
  assert_eq!(
    resource_search["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/ResourceSearchPage"
  );
  let search_parameters = resource_search["parameters"].as_array().unwrap();
  let search_parameter = |name| {
    search_parameters
      .iter()
      .find(|parameter| parameter["name"] == name)
      .unwrap()
  };
  assert_eq!(search_parameter("query")["required"], true);
  assert_eq!(
    search_parameter("query")["schema"]["x-max-utf8-bytes"],
    octacity_server_application::MAX_RESOURCE_SEARCH_QUERY_BYTES
  );
  assert_eq!(search_parameter("kinds")["style"], "form");
  assert_eq!(search_parameter("kinds")["explode"], true);
  assert_eq!(search_parameter("kinds")["schema"]["maxItems"], 4);
  assert_eq!(search_parameter("kinds")["schema"]["uniqueItems"], true);
  assert_eq!(
    search_parameter("kinds")["schema"]["items"]["$ref"],
    "#/components/schemas/ResourceSearchKind"
  );
  assert_eq!(
    search_parameter("after")["schema"]["maxLength"],
    octacity_server_application::MAX_RESOURCE_SEARCH_CURSOR_BYTES
  );
  assert_eq!(
    search_parameter("limit")["schema"]["maximum"],
    octacity_server_application::MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE
  );
  assert_eq!(
    document["components"]["schemas"]["ResourceSearchKind"]["enum"],
    serde_json::json!(["project", "build", "agent", "agent_pool"])
  );
  assert_json_matches_component(
    &document,
    "ResourceSearchPage",
    &serde_json::from_str(include_str!("../../fixtures/v1/resource-search-page.json")).unwrap(),
  );
  let search_result = document["components"]["schemas"]["ResourceSearchResult"]["properties"]
    .as_object()
    .unwrap();
  assert_eq!(
    search_result.keys().map(String::as_str).collect::<BTreeSet<_>>(),
    BTreeSet::from(["context", "id", "kind", "label"]),
    "search results must expose only stable navigation and safe display fields"
  );
  for forbidden in [
    "credential",
    "secret",
    "token",
    "url",
    "parameters",
    "effective_policy",
    "inventory",
  ] {
    assert!(search_result.get(forbidden).is_none());
  }

  let attention = &document["paths"]["/api/v1/operator-attention"]["get"];
  assert_eq!(attention["operationId"], "listOperatorAttention");
  assert_eq!(
    attention["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/OperatorAttentionPage"
  );
  let attention_parameters = attention["parameters"].as_array().unwrap();
  let attention_parameter = |name| {
    attention_parameters
      .iter()
      .find(|parameter| parameter["name"] == name)
      .unwrap()
  };
  for name in ["build_ids", "agent_ids", "pool_ids"] {
    assert_eq!(attention_parameter(name)["style"], "form");
    assert_eq!(attention_parameter(name)["explode"], true);
    assert_eq!(
      attention_parameter(name)["schema"]["maxItems"],
      octacity_server_application::MAX_OPERATOR_ATTENTION_SCOPE_TARGETS
    );
    assert_eq!(attention_parameter(name)["schema"]["uniqueItems"], true);
  }
  assert_eq!(
    attention_parameter("after")["schema"]["maxLength"],
    octacity_server_application::MAX_OPERATOR_ATTENTION_CURSOR_BYTES
  );
  assert_eq!(
    attention_parameter("limit")["schema"]["maximum"],
    octacity_server_application::MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE
  );
  assert_json_matches_component(
    &document,
    "OperatorAttentionPage",
    &serde_json::from_str(include_str!("../../fixtures/v1/operator-attention-page.json")).unwrap(),
  );
  let attention_item = document["components"]["schemas"]["OperatorAttentionItem"]["properties"]
    .as_object()
    .unwrap();
  assert_eq!(
    attention_item.keys().map(String::as_str).collect::<BTreeSet<_>>(),
    BTreeSet::from([
      "category",
      "code",
      "id",
      "occurred_at_unix_ms",
      "resolved_at_unix_ms",
      "severity",
      "summary",
      "target",
    ])
  );
  for forbidden in ["recipient", "unread", "read_at", "request_identity", "raw_event"] {
    assert!(attention_item.get(forbidden).is_none());
  }

  for (path, response_schema) in [
    ("/api/v1/projects/{project_id}", "ProjectDetails"),
    ("/api/v1/builds/{build_id}", "BuildResource"),
    ("/api/v1/agents/{agent_id}", "AgentResource"),
    ("/api/v1/agent-pools/{pool_id}", "AgentPoolResource"),
    ("/api/v1/agent-pools/{pool_id}/versions/{version}", "AgentPoolResource"),
  ] {
    assert_eq!(
      document["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
      format!("#/components/schemas/{response_schema}"),
      "global search must remain additive to existing detail contracts"
    );
  }

  let build_list = &document["paths"]["/api/v1/projects/{project_id}/builds"]["get"];
  assert_eq!(build_list["operationId"], "listProjectBuilds");
  assert_eq!(
    build_list["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/BuildSummaryPage"
  );
  let build_parameters = build_list["parameters"].as_array().unwrap();
  let parameter = |name| {
    build_parameters
      .iter()
      .find(|parameter| parameter["name"] == name)
      .unwrap()
  };
  assert_eq!(parameter("state")["schema"]["$ref"], "#/components/schemas/BuildState");
  assert_eq!(parameter("after")["schema"]["maxLength"], 128);
  assert_eq!(parameter("limit")["schema"]["maximum"], 200);
  assert!(
    build_parameters
      .iter()
      .any(|parameter| parameter["name"] == "project_id")
  );
  assert!(
    build_parameters
      .iter()
      .any(|parameter| parameter["name"] == "configuration_id")
  );
  assert_json_matches_component(
    &document,
    "BuildSummaryPage",
    &serde_json::from_str(include_str!("../../fixtures/v1/build-summary-page.json")).unwrap(),
  );
  assert_eq!(
    document["components"]["schemas"]["BuildState"]["enum"],
    serde_json::json!(["queued", "running", "succeeded", "failed", "cancelled"])
  );
  assert_eq!(
    document["components"]["schemas"]["AttemptState"]["enum"],
    serde_json::json!(["created", "running", "succeeded", "failed", "cancelled"])
  );
  let build_summary = document["components"]["schemas"]["BuildSummaryResource"]["properties"]
    .as_object()
    .unwrap();
  for diagnostic_field in ["jobs", "edges", "events", "parameters", "effective_policy", "trigger"] {
    assert!(build_summary.get(diagnostic_field).is_none());
  }

  let build_detail = document["components"]["schemas"]["BuildResource"]["properties"]
    .as_object()
    .unwrap()
    .keys()
    .map(String::as_str)
    .collect::<BTreeSet<_>>();
  assert_eq!(
    build_detail,
    BTreeSet::from([
      "configuration_id",
      "configuration_version",
      "created_at_unix_ms",
      "current_attempt",
      "effective_policy",
      "id",
      "immutable_revision",
      "parameters",
      "pipeline_id",
      "pipeline_version",
      "priority",
      "project_id",
      "repository_id",
      "repository_version",
      "source",
      "state",
      "trigger",
      "updated_at_unix_ms",
      "version",
    ]),
    "the Build collection must not alter the existing Build detail contract"
  );

  for (path, operation_id, response_schema) in [
    (
      "/api/v1/pipelines/{pipeline_id}/versions/{version}",
      "getPipelineVersion",
      "PipelineResource",
    ),
    (
      "/api/v1/repositories/{repository_id}/versions/{version}",
      "getRepositoryVersion",
      "RepositoryResource",
    ),
    (
      "/api/v1/build-configurations/{configuration_id}/versions/{version}",
      "getBuildConfigurationVersion",
      "BuildConfigurationResource",
    ),
    (
      "/api/v1/trigger-definitions/internal/{trigger_id}/versions/{version}",
      "getInternalTriggerDefinitionVersion",
      "InternalTriggerResource",
    ),
    (
      "/api/v1/trigger-definitions/manual/{trigger_id}/versions/{version}",
      "getManualTriggerDefinitionVersion",
      "ManualTriggerDefinitionResource",
    ),
    (
      "/api/v1/schedules/{trigger_id}/versions/{version}",
      "getSchedule",
      "ScheduleResource",
    ),
  ] {
    let operation = &document["paths"][path]["get"];
    assert_eq!(operation["operationId"], operation_id);
    assert_eq!(
      operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
      format!("#/components/schemas/{response_schema}")
    );
  }

  let trigger_summary = document["components"]["schemas"]["TriggerDefinitionSummaryResource"]["properties"]
    .as_object()
    .unwrap();
  for forbidden in [
    "credential",
    "credential_handle",
    "secret",
    "verification_material_handle",
  ] {
    assert!(trigger_summary.get(forbidden).is_none());
  }

  for (schema, body) in request_examples() {
    assert_json_matches_component(&document, schema, &body);
  }
  assert_json_matches_component(
    &document,
    "ScheduleResource",
    &serde_json::json!({
      "trigger_id": "88888888-8888-4888-8888-888888888888",
      "trigger_version": 1,
      "configuration_id": "44444444-4444-4444-8444-444444444444",
      "configuration_version": 1,
      "enabled": true,
      "schedule": {
        "expression": "0 0 9 * * Mon-Fri *",
        "timezone": "Europe/Moscow",
        "missed_run_policy": {"kind": "run_once"}
      },
      "next_occurrence_at_unix_ms": 1_700_000_000_000_i64,
      "build": {
        "source": {"kind": "exact_revision", "value": "0123456789abcdef"},
        "parameters": {},
        "priority": 0
      }
    }),
  );
  assert_json_matches_component(
    &document,
    "BuildLogSearchPage",
    &serde_json::from_str(include_str!("../../fixtures/v1/build-log-search-page.json")).unwrap(),
  );
}

#[test]
fn generated_openapi_contract_is_line_ending_independent() {
  let generated = openapi_document();
  let pretty = serde_json::to_string_pretty(&generated).unwrap();

  for (platform, line_ending) in [("Linux", "\n"), ("macOS", "\r"), ("Windows", "\r\n")] {
    let encoded = pretty.lines().collect::<Vec<_>>().join(line_ending);
    let parsed: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
      parsed, generated,
      "generated document changed with {platform} line endings"
    );
    assert_eq!(
      documented_operation_inventory(&parsed),
      registered_operation_inventory(),
      "registered route inventory changed with {platform} line endings"
    );
    assert_forbidden_contract(&parsed);
  }
}

#[test]
fn generated_openapi_contract_has_no_dangling_local_references() {
  let document = openapi_document();
  let mut references = Vec::new();
  collect_local_references(&document, &mut references);

  let unresolved = references
    .into_iter()
    .filter(|reference| document.pointer(&reference[1..]).is_none())
    .collect::<BTreeSet<_>>();

  assert!(unresolved.is_empty(), "unresolved OpenAPI references: {unresolved:?}");
}
