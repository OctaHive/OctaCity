//! The transport contract selects immutable profiles; it cannot accept secrets or Issue replay keys.
use super::*;
use serde_json::Value;
const PROJECT: &str = "0199a6f4-d56c-7440-9aa2-6a320f862790";
const INPUT: &str = "0199a6f4-d56c-7440-9aa2-6a320f862791";
const GROUP: &str = "0199a6f4-d56c-7440-9aa2-6a320f862792";
const CONNECTION: &str = "0199a6f4-d56c-7440-9aa2-6a320f862795";
fn definition() -> Value {
  json!({"connection":{"id":CONNECTION,"version":1,"access_version":2,"source_configuration":exact("connection-settings"),"reporter_configuration":null},
    "target":{"id":GROUP,"version":1,"project_id":PROJECT,"definition_digest":"07".repeat(32)},
    "repository":{"id":INPUT,"version":3,"project_id":PROJECT,"definition_digest":"08".repeat(32)},
    "source":{"installation":exact("installed"),"capability":exact("read"),"configuration":exact("triage-labels"),"resource":exact("tickets")},
    "reporter":null,"enabled":true})
}
fn intent(group: bool) -> Value {
  json!({"project_id":PROJECT,"inputs":[{"id":INPUT,"expected_version":null,"definition":definition()}],
    "group":if group { json!({"id":GROUP,"expected_version":null,"members":[{"id":INPUT,"version":1}]}) } else { Value::Null }})
}
fn input() -> Value {
  json!({"id":INPUT,"version":1,"project_id":PROJECT,
    "root_flow":{"id":GROUP,"version":2,"digest":"09".repeat(32)},"definition":definition()})
}
fn publication(group: bool) -> Value {
  json!({"project_id":PROJECT,"inputs":[{"input":input(),"expected_version":null}],
    "group":if group { json!({"group":{"id":GROUP,"version":1,"project_id":PROJECT,"members":[input()]},"expected_version":null}) } else { Value::Null }})
}
#[derive(Default)]
struct SourcePeer(AtomicUsize);
#[async_trait]
impl ManagementCommandUseCase<PublishWorkSourceConfigurationCommand> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishWorkSourceConfigurationCommand,
  ) -> Result<<PublishWorkSourceConfigurationCommand as Command>::Outcome, ApplicationError> {
    let grouped = command.intent.group.is_some();
    assert_eq!(serde_json::to_value(command.intent).unwrap(), intent(grouped));
    assert_eq!(command.idempotency_key.as_str(), "publish-input-once");
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(
      serde_json::from_value(json!({"disposition":"applied","publication":publication(grouped),"published_at":10}))
        .unwrap(),
    )
  }
}
#[async_trait]
impl ManagementQueryUseCase<ValidateWorkSourceConfigurationQuery> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ValidateWorkSourceConfigurationQuery,
  ) -> Result<<ValidateWorkSourceConfigurationQuery as Query>::Outcome, ApplicationError> {
    assert_eq!(
      serde_json::to_value(&query.intent).unwrap(),
      intent(query.intent.group.is_some())
    );
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(serde_json::from_value(publication(query.intent.group.is_some())).unwrap())
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetWorkSourceConfigurationQuery> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetWorkSourceConfigurationQuery,
  ) -> Result<<GetWorkSourceConfigurationQuery as Query>::Outcome, ApplicationError> {
    assert_eq!(query.project_id.to_string(), PROJECT);
    let target = serde_json::to_value(query.target).unwrap();
    assert_eq!(target["version"], 1);
    Ok(
      serde_json::from_value(if target["kind"] == "input" {
        json!({"kind":"input","definition":input()})
      } else {
        json!({"kind":"group","definition":{"id":GROUP,"version":1,"project_id":PROJECT,"members":[input()]}})
      })
      .unwrap(),
    )
  }
}
#[async_trait]
impl ManagementQueryUseCase<ListWorkSourceConfigurationsQuery> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListWorkSourceConfigurationsQuery,
  ) -> Result<<ListWorkSourceConfigurationsQuery as Query>::Outcome, ApplicationError> {
    assert_eq!(query.project_id.to_string(), PROJECT);
    assert_eq!(query.limit, 1);
    Ok(serde_json::from_value(json!({"items":[{"kind":"input","definition":input()}],"next_cursor":null})).unwrap())
  }
}
#[async_trait]
impl ManagementQueryUseCase<ListWorkSourceInputChoicesQuery> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListWorkSourceInputChoicesQuery,
  ) -> Result<WorkSourceInputChoices, ApplicationError> {
    assert_eq!(query.project_id.to_string(), PROJECT);
    assert_eq!(query.connection_id.to_string(), CONNECTION);
    assert_eq!(query.connection_version.get(), 1);
    Ok(WorkSourceInputChoices {
      connection_id: query.connection_id,
      connection_version: query.connection_version,
      access_version: serde_json::from_value(json!(2)).unwrap(),
      items: vec![],
    })
  }
}
fn router(peer: Arc<SourcePeer>) -> axum::Router {
  let policy = Arc::new(Policy);
  integration_management_routes(
    IntegrationManagementApplication::new(Arc::new(AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(Peer::default()),
    )))
    .with_source_configuration(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), peer.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy.clone(), peer.clone())),
    )
    .with_source_reads(Arc::new(AuthorizedQueryHandler::new(policy, peer))),
  )
}
// Readiness must inspect composition without invoking any application/provider operation.
struct ReadinessPeer;
#[async_trait]
impl<Q: Query + Send + Sync + 'static> ManagementQueryUseCase<Q> for ReadinessPeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: Q,
  ) -> Result<Q::Outcome, ApplicationError> {
    panic!("readiness must not dispatch queries")
  }
}
#[async_trait]
impl<C: Command + Send + Sync + 'static> ManagementCommandUseCase<C> for ReadinessPeer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    _: C,
  ) -> Result<C::Outcome, ApplicationError> {
    panic!("readiness must not dispatch commands")
  }
}
#[tokio::test]
async fn project_source_partial_installation_is_unavailable_without_factory_inputs() {
  let policy = Arc::new(Policy);
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), Arc::new(ReadinessPeer)));
  let application = IntegrationManagementApplication::new(Arc::new(AuthorizedCommandHandler::new(
    policy,
    Arc::new(Peer::default()),
  )))
  .with_project_source_choices(queries);
  let readiness = application.source_management_readiness();
  let router = octacity_server_api_rest::factory_readiness_routes(move || readiness);
  let response = router
    .oneshot(Request::builder().uri("/health/factory").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn complete_project_source_family_can_be_installed_without_factory_input_authoring() {
  let policy = Arc::new(Policy);
  let peer = Arc::new(ReadinessPeer);
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), peer.clone()));
  let commands = Arc::new(AuthorizedCommandHandler::new(policy, peer));
  let application = IntegrationManagementApplication::new(commands.clone())
    .with_project_source_choices(queries.clone())
    .with_project_source_preview(queries.clone())
    .with_project_source_imports(commands, queries);
  let readiness = application.source_management_readiness();
  let router = octacity_server_api_rest::factory_readiness_routes(move || readiness);
  let response = router
    .oneshot(Request::builder().uri("/health/factory").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
  assert_eq!(body, json!({"status":"ready"}));
}

#[tokio::test]
async fn advertised_source_readiness_requires_each_route_family_without_dispatching() {
  for omitted in [
    Some("configuration"),
    Some("reads"),
    Some("choices"),
    Some("preview"),
    Some("imports"),
    Some("dependencies"),
    None,
  ] {
    let policy = Arc::new(Policy);
    let peer = Arc::new(ReadinessPeer);
    let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), peer.clone()));
    let commands = Arc::new(AuthorizedCommandHandler::new(policy, peer));
    let mut application = IntegrationManagementApplication::new(commands.clone()).require_source_management();
    if omitted != Some("configuration") {
      application = application.with_source_configuration(commands.clone(), queries.clone());
    }
    if omitted != Some("reads") {
      application = application.with_source_reads(queries.clone());
    }
    if omitted != Some("choices") {
      application = application.with_project_source_choices(queries.clone());
    }
    if omitted != Some("preview") {
      application = application.with_project_source_preview(queries.clone());
    }
    if omitted != Some("imports") {
      application = application.with_project_source_imports(commands.clone(), queries.clone());
    }
    if omitted != Some("dependencies") {
      application = application.with_dependencies(queries);
    }
    let readiness = application.source_management_readiness();
    let router = octacity_server_api_rest::factory_readiness_routes(move || readiness);
    let response = router
      .oneshot(Request::builder().uri("/health/factory").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      if omitted.is_some() {
        StatusCode::SERVICE_UNAVAILABLE
      } else {
        StatusCode::OK
      },
      "omitted {omitted:?}"
    );
  }
}

#[tokio::test]
async fn advertised_source_management_requires_project_import_routes() {
  let policy = Arc::new(Policy);
  let peer = Arc::new(SourcePeer::default());
  let application = IntegrationManagementApplication::new(Arc::new(AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(Peer::default()),
  )))
  .require_source_management()
  .with_source_configuration(
    Arc::new(AuthorizedCommandHandler::new(policy.clone(), peer.clone())),
    Arc::new(AuthorizedQueryHandler::new(policy.clone(), peer.clone())),
  )
  .with_source_reads(Arc::new(AuthorizedQueryHandler::new(policy, peer)));
  let readiness = application.source_management_readiness();
  let router = integration_management_routes(application)
    .merge(octacity_server_api_rest::factory_readiness_routes(move || readiness));
  let response = router
    .clone()
    .oneshot(Request::builder().uri("/health/factory").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::SERVICE_UNAVAILABLE,
    "Factory input handlers alone cannot qualify advertised Project import support"
  );
  let path = format!("/api/v1/projects/{PROJECT}/issue-source-imports");
  let (status, body) = request(&router, "GET", &path, Value::Null, &[], true).await;
  assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(body["code"], "unavailable");
  let (status, _) = request(&router, "GET", &path, Value::Null, &[], false).await;
  assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn source_management_readiness_distinguishes_optional_absence_and_incomplete_composition() {
  for (configuration, reads, status, state) in [
    (false, false, StatusCode::OK, "disabled"),
    (true, false, StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    (false, true, StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    (true, true, StatusCode::OK, "ready"),
  ] {
    let policy = Arc::new(Policy);
    let peer = Arc::new(SourcePeer::default());
    let mut application = IntegrationManagementApplication::new(Arc::new(AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(Peer::default()),
    )));
    if configuration {
      application = application.with_source_configuration(
        Arc::new(AuthorizedCommandHandler::new(policy.clone(), peer.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy.clone(), peer.clone())),
      );
    }
    if reads {
      application = application.with_source_reads(Arc::new(AuthorizedQueryHandler::new(policy, peer)));
    }
    let readiness = application.source_management_readiness();
    if configuration != reads {
      let missing = if configuration {
        format!("/api/v1/projects/{PROJECT}/factory-source-inputs/{INPUT}/versions/1/diagnostics")
      } else {
        format!("/api/v1/projects/{PROJECT}/factory-sources")
      };
      let (status, body) = request(
        &integration_management_routes(application),
        "GET",
        &missing,
        Value::Null,
        &[],
        true,
      )
      .await;
      assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
      assert_eq!(body["code"], "unavailable");
    }
    let router = octacity_server_api_rest::management_router(|| true)
      .merge(octacity_server_api_rest::factory_readiness_routes(move || readiness));
    let response = router
      .clone()
      .oneshot(Request::builder().uri("/health/factory").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), status);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body, json!({"status":state}));
    let ordinary = router
      .oneshot(Request::builder().uri("/health/ready").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(ordinary.status(), StatusCode::OK);
  }
}
async fn request(
  router: &axum::Router,
  method: &str,
  path: &str,
  body: Value,
  headers: &[(&str, &str)],
  verified: bool,
) -> (StatusCode, Value) {
  let mut request = Request::builder()
    .method(method)
    .uri(path)
    .header("content-type", "application/json");
  for (key, value) in headers {
    request = request.header(*key, *value);
  }
  let mut request = request.body(Body::from(body.to_string())).unwrap();
  if verified {
    request.extensions_mut().insert(proof());
  }
  let response = router.clone().oneshot(request).await.unwrap();
  assert_eq!(response.headers()["cache-control"], "no-store");
  let status = response.status();
  let value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
  if status == StatusCode::OK {
    let component = if path.contains("/preview") {
      "WorkSourcePreview"
    } else if path.contains("/diagnostics") {
      "WorkSourceDiagnostics"
    } else if path.contains("/validate") {
      "WorkSourceConfigurationValidation"
    } else if path.contains("factory-source-choices") {
      "WorkSourceInputChoices"
    } else if method == "POST" {
      "WorkSourceConfigurationMutation"
    } else if path.contains("/versions/") {
      "WorkSourceConfigurationRecord"
    } else {
      "WorkSourceConfigurationPage"
    };
    openapi::assert_json_matches_component(&octacity_server_api_rest::v1::openapi_document(), component, &value);
  }
  (status, value)
}
#[tokio::test]
async fn source_configuration_rest_requires_proof_preconditions_and_closed_provider_neutral_intent() {
  let peer = Arc::new(SourcePeer::default());
  let router = router(peer.clone());
  let path = format!("/api/v1/projects/{PROJECT}/factory-source-inputs/{INPUT}/versions");
  let valid = json!({"intent":intent(false)});
  let headers = [("idempotency-key", "publish-input-once"), ("if-none-match", "*")];
  for (verified, selected_headers, expected) in [
    (false, headers.as_slice(), StatusCode::FORBIDDEN),
    (true, &headers[..1], StatusCode::PRECONDITION_REQUIRED),
    (true, &headers[1..], StatusCode::BAD_REQUEST),
  ] {
    assert_eq!(
      request(&router, "POST", &path, valid.clone(), selected_headers, verified)
        .await
        .0,
      expected
    );
  }
  for field in ["token", "credential", "issue_id"] {
    let mut invalid = valid.clone();
    invalid["intent"]["inputs"][0]["definition"][field] = json!("private-sentinel");
    let (status, body) = request(&router, "POST", &path, invalid, &headers, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains("private-sentinel"));
  }
  let mut stale = valid.clone();
  stale["intent"]["inputs"][0]["expected_version"] = json!(2);
  assert_eq!(
    request(
      &router,
      "POST",
      &path,
      stale,
      &[("idempotency-key", "publish-input-once"), ("if-match", "\"1\"")],
      true
    )
    .await
    .0,
    StatusCode::PRECONDITION_FAILED
  );
  assert_eq!(peer.0.load(Ordering::SeqCst), 0);
  let (status, body) = request(&router, "POST", &path, valid, &headers, true).await;
  assert_eq!(status, StatusCode::OK);
  assert_eq!(body["publication"]["inputs"][0]["input"]["root_flow"]["version"], 2);
  assert_eq!(body["published_at_unix_ms"], 10);
  assert_eq!(peer.0.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn source_configuration_rest_reads_choices_validates_and_publishes_exact_groups() {
  let peer = Arc::new(SourcePeer::default());
  let router = router(peer.clone());
  for suffix in [
    format!("factory-source-inputs/{INPUT}/versions/1"),
    format!("factory-source-groups/{GROUP}/versions/1"),
    "factory-sources?limit=1".into(),
    format!("factory-source-choices?connection_id={CONNECTION}&connection_version=1"),
  ] {
    let (status, body) = request(
      &router,
      "GET",
      &format!("/api/v1/projects/{PROJECT}/{suffix}"),
      Value::Null,
      &[],
      true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.to_string().contains("credential"));
  }
  let base = format!("/api/v1/projects/{PROJECT}");
  let valid = json!({"intent":intent(true)});
  let (status, body) = request(
    &router,
    "POST",
    &format!("{base}/factory-sources/validate"),
    valid.clone(),
    &[],
    true,
  )
  .await;
  assert_eq!(status, StatusCode::OK);
  assert_eq!(body["publication"]["group"]["group"]["members"][0]["id"], INPUT);
  let (status, body) = request(
    &router,
    "POST",
    &format!("{base}/factory-source-groups/{GROUP}/versions"),
    valid,
    &[("idempotency-key", "publish-input-once"), ("if-none-match", "*")],
    true,
  )
  .await;
  assert_eq!(status, StatusCode::OK, "{body}");
  assert_eq!(body["publication"]["group"]["group"]["version"], 1);
  assert_eq!(peer.0.load(Ordering::SeqCst), 2);
}
#[test]
fn source_configuration_openapi_has_closed_typed_contracts_and_authorization() {
  let document = octacity_server_api_rest::v1::openapi_document();
  let schemas = &document["components"]["schemas"];
  for name in [
    "WorkSourceConfigurationSubmission",
    "WorkSourceConfigurationIntent",
    "WorkSourceInputIntent",
    "WorkSourceInputDraft",
    "WorkSourceConnectionReference",
    "WorkSourceGroupIntent",
    "WorkSourceConfigurationPage",
  ] {
    assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
  }
  assert_eq!(
    schemas["WorkSourceConfigurationIntent"]["properties"]["inputs"]["maxItems"],
    64
  );
  assert!(schemas["WorkSourceInputDraft"]["properties"].get("issue_id").is_none());
  for (path, method, operation) in [
    (
      "/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions",
      "post",
      "publishWorkSourceInput",
    ),
    (
      "/api/v1/projects/{project_id}/factory-source-groups/{group_id}/versions",
      "post",
      "publishWorkSourceGroup",
    ),
    (
      "/api/v1/projects/{project_id}/factory-sources/validate",
      "post",
      "validateWorkSourceConfiguration",
    ),
    (
      "/api/v1/projects/{project_id}/factory-source-choices",
      "get",
      "listWorkSourceInputChoices",
    ),
    (
      "/api/v1/projects/{project_id}/factory-sources",
      "get",
      "listWorkSourceConfigurations",
    ),
    (
      "/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}",
      "get",
      "getWorkSourceInput",
    ),
    (
      "/api/v1/projects/{project_id}/factory-source-groups/{group_id}/versions/{version}",
      "get",
      "getWorkSourceGroup",
    ),
  ] {
    let entry = &document["paths"][path][method];
    assert_eq!(entry["operationId"], operation);
    assert!(
      entry["responses"]["422"].is_object(),
      "missing Source capability response for {operation}"
    );
    assert_eq!(entry["security"], json!([{"ProtectedManagementIngress":[]}]));
    assert!(
      octacity_server_api_rest::v1::management_authorization_operations()
        .iter()
        .any(|o| o.operation_id == operation)
    );
  }
}

#[tokio::test]
async fn source_configuration_rest_and_schema_allow_omitting_optional_reporter() {
  let router = router(Arc::new(SourcePeer::default()));
  let mut selected = intent(false);
  selected["inputs"][0]["definition"]
    .as_object_mut()
    .unwrap()
    .remove("reporter");
  let (status, body) = request(
    &router,
    "POST",
    &format!("/api/v1/projects/{PROJECT}/factory-sources/validate"),
    json!({"intent":selected}),
    &[],
    true,
  )
  .await;
  assert_eq!(status, StatusCode::OK, "{body}");
  let document = octacity_server_api_rest::v1::openapi_document();
  let required = document["components"]["schemas"]["WorkSourceInputDraft"]["required"]
    .as_array()
    .unwrap();
  assert!(
    !required.iter().any(|field| field == "reporter"),
    "the schema must preserve optional write integration"
  );
}

#[tokio::test]
async fn source_configuration_rest_bounds_pagination_before_query_dispatch() {
  let router = router(Arc::new(SourcePeer::default()));
  for limit in [0, 65] {
    let (status, body) = request(
      &router,
      "GET",
      &format!("/api/v1/projects/{PROJECT}/factory-sources?limit={limit}"),
      Value::Null,
      &[],
      true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
  }
}

#[async_trait]
impl ManagementQueryUseCase<PreviewWorkSourcesQuery> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: PreviewWorkSourcesQuery,
  ) -> Result<WorkSourcePreview, ApplicationError> {
    assert_eq!(serde_json::to_value(&query.intent).unwrap(), intent(false));
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(WorkSourcePreview {
      configuration: query.intent,
      selectors: vec![],
      observed_at: query.observed_at,
      failure: (query.limit == 1).then_some(WorkSourceFailure::RateLimited),
      retry_after_millis: (query.limit == 1).then_some(30000),
      candidates: vec![],
      has_more: false,
      admissions_truncated: false,
    })
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetWorkSourceDiagnosticsQuery> for SourcePeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetWorkSourceDiagnosticsQuery,
  ) -> Result<WorkSourceDiagnostics, ApplicationError> {
    assert_eq!(query.project_id.to_string(), PROJECT);
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(WorkSourceDiagnostics {
      configuration: serde_json::from_value(json!({"kind":"input","definition":input()})).unwrap(),
      selectors: vec![],
      scan: None,
      admissions: vec![],
      next_cursor: None,
    })
  }
}
#[tokio::test]
async fn source_preview_and_diagnostics_rest_are_protected_closed_bounded_and_read_only() {
  let peer = Arc::new(SourcePeer::default());
  let app = router(peer.clone());
  let path = format!("/api/v1/projects/{PROJECT}/factory-sources/preview");
  assert_eq!(
    request(
      &app,
      "POST",
      &path,
      json!({"intent":intent(false),"limit":2}),
      &[],
      false
    )
    .await
    .0,
    StatusCode::FORBIDDEN
  );
  for limit in [1, 2] {
    let (status, result) = request(
      &app,
      "POST",
      &path,
      json!({"intent":intent(false),"limit":limit}),
      &[],
      true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(
      result["failure"],
      if limit == 1 { json!("rate_limited") } else { Value::Null }
    );
    assert_eq!(result["candidates"], json!([]));
  }
  let calls = peer.0.load(Ordering::SeqCst);
  for body in [
    json!({"intent":intent(false),"limit":0}),
    json!({"intent":intent(false),"limit":101}),
    json!({"intent":intent(false),"limit":2,"token":"SECRET-NEVER-ECHO"}),
    json!({"intent":intent(false),"limit":2,"issue_id":INPUT}),
  ] {
    let (status, result) = request(&app, "POST", &path, body, &[], true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!result.to_string().contains("SECRET-NEVER-ECHO"));
  }
  assert_eq!(peer.0.load(Ordering::SeqCst), calls);
  for kind in ["inputs", "groups"] {
    let id = if kind == "inputs" { INPUT } else { GROUP };
    let path = format!("/api/v1/projects/{PROJECT}/factory-source-{kind}/{id}/versions/1/diagnostics?limit=2");
    assert_eq!(
      request(&app, "GET", &path, Value::Null, &[], false).await.0,
      StatusCode::FORBIDDEN
    );
    assert_eq!(
      request(&app, "GET", &path, Value::Null, &[], true).await.0,
      StatusCode::OK
    );
    assert_eq!(
      request(
        &app,
        "GET",
        &format!("{path}&body=SECRET-NEVER-ECHO"),
        Value::Null,
        &[],
        true
      )
      .await
      .0,
      StatusCode::BAD_REQUEST
    );
  }
}

#[test]
fn project_source_preview_contract_is_closed_source_only_and_exposes_stale_review_failure() {
  let document = octacity_server_api_rest::v1::openapi_document();
  let schema = &document["components"]["schemas"]["ProjectIssueSourcePreviewSubmission"];
  assert_eq!(schema["additionalProperties"], false);
  assert_eq!(schema["properties"].as_object().unwrap().len(), 5);
  assert_eq!(schema["properties"]["limit"]["minimum"], 1);
  assert_eq!(schema["properties"]["limit"]["maximum"], 100);
  for field in ["issue_id", "repository", "target", "token", "credential"] {
    assert!(schema["properties"].get(field).is_none(), "{field}");
  }
  for (path, method) in [
    ("/api/v1/projects/{project_id}/issue-sources/preview", "post"),
    ("/api/v1/projects/{project_id}/issue-source-choices", "get"),
  ] {
    let operation = &document["paths"][path][method];
    assert!(
      operation["responses"].get("412").is_some(),
      "stale metadata/grant review is a public precondition failure: {path}"
    );
  }
}

#[test]
fn project_import_routes_expose_the_required_import_identity_to_generated_clients() {
  let document = octacity_server_api_rest::v1::openapi_document();
  for (path, method) in [
    ("/api/v1/projects/{project_id}/issue-source-imports/{import_id}", "get"),
    (
      "/api/v1/projects/{project_id}/issue-source-imports/{import_id}/versions",
      "post",
    ),
    (
      "/api/v1/projects/{project_id}/issue-source-imports/{import_id}/versions/{version}",
      "get",
    ),
  ] {
    let parameters = document["paths"][path][method]["parameters"].as_array().unwrap();
    let identity = parameters
      .iter()
      .find(|parameter| parameter["in"] == "path" && parameter["name"] == "import_id")
      .expect("generated clients must bind the exact retained import identity");
    assert_eq!(identity["required"], true);
    assert_eq!(identity["schema"]["type"], "string");
    assert_eq!(identity["schema"]["format"], "uuid");
  }
}

#[test]
fn project_import_contract_requires_reviewed_activation_and_excludes_private_or_execution_inputs() {
  let document = octacity_server_api_rest::v1::openapi_document();
  let submission = &document["components"]["schemas"]["ProjectIssueSourceImportSubmission"];
  assert_eq!(submission["additionalProperties"], false);
  let intent = &submission["properties"]["intent"];
  assert_eq!(intent["additionalProperties"], false);
  let definition = &intent["properties"]["definition"];
  assert_eq!(definition["additionalProperties"], false);
  assert!(definition["required"].as_array().unwrap().contains(&json!("enabled")));
  for private in [
    "issue_id",
    "target",
    "repository",
    "body",
    "acceptance",
    "token",
    "credential",
    "plugin",
    "network_hosts",
  ] {
    assert!(definition["properties"].get(private).is_none(), "{private}");
  }
  let publish = &document["paths"]["/api/v1/projects/{project_id}/issue-source-imports/{import_id}/versions"]["post"];
  for status in ["409", "412", "428"] {
    assert!(
      publish["responses"].get(status).is_some(),
      "reviewed import publication must advertise {status}"
    );
  }
}
