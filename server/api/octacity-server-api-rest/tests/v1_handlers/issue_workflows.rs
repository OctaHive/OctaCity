use super::*;
use octacity_server_api_rest::v1::IssueWorkflowManagementApplication;
use octacity_server_application::{
  AuthorizedCommandHandler, AuthorizedQueryHandler, GetDefaultIssueWorkflowQuery, IssueWorkflowDefault,
  TrustedNetworkManagementPolicy,
};

struct WorkflowPeer;
#[async_trait]
impl ManagementQueryUseCase<GetDefaultIssueWorkflowQuery> for WorkflowPeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetDefaultIssueWorkflowQuery,
  ) -> Result<IssueWorkflowDefault, ApplicationError> {
    Ok(
      serde_json::from_value(serde_json::json!({"version":1,"workflow":{
      "project_id": query.project_id, "id":query.project_id, "version":1,"published_at":1,
      "definition":support::issue_workflow_definition()}}))
      .unwrap(),
    )
  }
}

#[tokio::test]
async fn issue_workflow_default_http_needs_no_factory_or_provider_and_is_read_only() {
  let handlers = Arc::new(WorkflowPeer);
  let commands = Arc::new(AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    handlers.clone(),
  ));
  let queries = Arc::new(AuthorizedQueryHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    handlers,
  ));
  let peer = Arc::new(RecordingApplication::default());
  let app = recording_management_application(peer.clone(), peer)
    .with_issue_workflows(IssueWorkflowManagementApplication::new(commands, queries));
  let router = management_router_with_application(|| true, app);
  let response = router
    .oneshot(
      Request::builder()
        .uri("/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflow-default")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"1\"");
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  assert_eq!(body["version"], 1);
  assert_eq!(
    body["workflow"]["definition"]["initial_status_id"],
    "10000000-0000-4000-8000-000000000001"
  );
  assert_eq!(
    body["workflow"]["definition"]["terminal_status_ids"]
      .as_array()
      .unwrap()
      .len(),
    2
  );
}

#[test]
fn issue_workflow_openapi_exposes_closed_bounded_provider_neutral_policy() {
  let doc = openapi_document();
  let schema = &doc["components"]["schemas"];
  let definition = &schema["IssueWorkflowDefinition"];
  assert_eq!(definition["additionalProperties"], false);
  assert_eq!(definition["properties"]["statuses"]["minItems"], 1);
  assert_eq!(definition["properties"]["statuses"]["maxItems"], 64);
  assert_eq!(definition["properties"]["terminal_status_ids"]["uniqueItems"], true);
  assert_eq!(definition["properties"]["terminal_status_ids"]["maxItems"], 64);
  assert_eq!(definition["properties"]["transitions"]["maxItems"], 256);
  assert!(
    definition["required"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("initial_status_id"))
  );
  assert!(
    definition["required"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("terminal_status_ids"))
  );
  assert_eq!(
    schema["IssueWorkflowStatus"]["properties"]
      .as_object()
      .unwrap()
      .keys()
      .cloned()
      .collect::<Vec<_>>(),
    ["id", "name"]
  );
  assert_eq!(schema["IssueWorkflowTransition"]["additionalProperties"], false);
  assert_eq!(
    schema["IssueWorkflowTransition"]["properties"]["origins"]["items"]["enum"],
    serde_json::json!(["manual", "pipeline", "source"])
  );
  let path = &doc["paths"]["/api/v1/projects/{project_id}/issue-workflows/{workflow_id}"]["get"];
  let identity = path["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|p| p["name"] == "workflow_id");
  assert!(
    identity.is_some(),
    "generated clients need the stable workflow path parameter"
  );
  assert_eq!(identity.unwrap()["schema"]["format"], "uuid");
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ValidateIssueWorkflowQuery> for WorkflowPeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: octacity_server_application::ValidateIssueWorkflowQuery,
  ) -> Result<octacity_server_application::IssueWorkflowDefinition, ApplicationError> {
    query.definition.validate()?;
    Ok(query.definition)
  }
}
fn workflow_router() -> axum::Router {
  workflow_router_with_policy(Arc::new(TrustedNetworkManagementPolicy))
}
fn workflow_router_with_policy(policy: Arc<dyn ManagementAuthorizationPolicy>) -> axum::Router {
  let peer = Arc::new(RecordingApplication::default());
  let service = Arc::new(WorkflowPeer);
  management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_issue_workflows(IssueWorkflowManagementApplication::new(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), service.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, service)),
    )),
  )
}

#[tokio::test]
async fn issue_workflow_http_read_permission_does_not_grant_administration() {
  struct ViewOnly;
  #[async_trait]
  impl ManagementAuthorizationPolicy for ViewOnly {
    async fn authorize(
      &self,
      _: &ManagementRequestContext,
      action: ManagementAction,
      _: &ManagementResource,
    ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
      if action == ManagementAction::View {
        Ok(ManagementAuthorizationGrant::new(
          octacity_server_application::ManagementVisibility::all(),
        ))
      } else {
        Err(ManagementAuthorizationDenial::forbidden())
      }
    }
  }
  let router = workflow_router_with_policy(Arc::new(ViewOnly));
  let definition = support::issue_workflow_definition();
  let root = "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795";
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{root}/issue-workflow-default"), None))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{root}/issue-workflows/validate"),
        "unused",
        None,
        &serde_json::json!({"definition":definition}).to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  for (suffix, body, version) in [
    (
      "issue-workflows",
      serde_json::json!({"id":"20000000-0000-4000-8000-000000000001","definition":definition}),
      None,
    ),
    (
      "issue-workflows/20000000-0000-4000-8000-000000000001/versions",
      serde_json::json!({"definition":definition}),
      Some("\"1\""),
    ),
    (
      "issue-workflow-default",
      serde_json::json!({"workflow_id":"20000000-0000-4000-8000-000000000001","workflow_version":2}),
      Some("\"1\""),
    ),
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{root}/{suffix}"),
        "admin-attempt",
        version,
        &body.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{suffix}");
  }
}

#[tokio::test]
async fn issue_workflow_http_invalid_version_has_the_standard_safe_error() {
  let response = workflow_router().oneshot(empty_request("GET",
    "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflows/20000000-0000-4000-8000-000000000001/versions/not-a-version", None)).await.unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(response.headers()["content-type"], "application/json");
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  assert_eq!(body["code"], "invalid_request");
}
async fn definition() -> serde_json::Value {
  let response = workflow_router()
    .oneshot(empty_request(
      "GET",
      "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflow-default",
      None,
    ))
    .await
    .unwrap();
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  body["workflow"]["definition"].clone()
}
#[tokio::test]
async fn issue_workflow_http_validation_rejects_legacy_flags_scripts_and_execution_setters() {
  let router = workflow_router();
  let definition = definition().await;
  let path = "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflows/validate";
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      path,
      "unused",
      None,
      &serde_json::json!({"definition":definition}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let accepted: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  assert_eq!(accepted, definition);
  let mut invalid = Vec::new();
  for field in ["initial", "terminal"] {
    let mut doc = definition.clone();
    doc["statuses"][0][field] = serde_json::json!(true);
    invalid.push(doc);
  }
  for field in ["guard", "script", "run_state", "decision"] {
    let mut doc = definition.clone();
    doc[field] = serde_json::json!("accepted");
    invalid.push(doc);
  }
  for doc in invalid {
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        path,
        "unused",
        None,
        &serde_json::json!({"definition":doc}).to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  }
}

#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::PublishIssueWorkflowCommand> for WorkflowPeer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: octacity_server_application::PublishIssueWorkflowCommand,
  ) -> Result<octacity_server_application::IssueWorkflowPublicationOutcome, ApplicationError> {
    assert_eq!(command.intent.expected_current_version.unwrap().get(), 1);
    assert_eq!(command.idempotency_key.as_str(), "publish-policy");
    Ok(octacity_server_application::IssueWorkflowPublicationOutcome {
      disposition: octacity_server_application::StoreMutationDisposition::Applied,
      workflow: octacity_server_application::PublishedIssueWorkflow {
        project_id: command.intent.project_id,
        id: command.intent.id,
        version: serde_json::from_value(serde_json::json!(2)).unwrap(),
        definition: command.intent.definition,
        published_at: command.published_at,
      },
    })
  }
}
#[tokio::test]
async fn issue_workflow_publication_http_requires_strong_precondition_and_preserves_policy() {
  let router = workflow_router();
  let mut definition = definition().await;
  definition["name"] = serde_json::json!("Our policy");
  let body = serde_json::json!({"definition":definition}).to_string();
  let path = "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflows/0199a6f4-d56c-7440-9aa2-6a320f862795/versions";
  let response = router
    .clone()
    .oneshot(json_request("POST", path, "publish-policy", Some("\"1\""), &body))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let result: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  assert_eq!(result["resource"]["definition"], definition);
  assert_eq!(result["resource"]["version"], 2);
  assert_eq!(result["disposition"], "applied");
  for (version, status) in [
    (None, StatusCode::PRECONDITION_REQUIRED),
    (Some("W/\"1\""), StatusCode::BAD_REQUEST),
    (Some("*"), StatusCode::BAD_REQUEST),
  ] {
    let response = router
      .clone()
      .oneshot(json_request("POST", path, "publish-policy", version, &body))
      .await
      .unwrap();
    assert_eq!(response.status(), status);
  }
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::GetIssueWorkflowQuery> for WorkflowPeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::GetIssueWorkflowQuery,
  ) -> Result<octacity_server_application::PublishedIssueWorkflow, ApplicationError> {
    let mut workflow = self
      .execute_management_query(
        context,
        grant,
        GetDefaultIssueWorkflowQuery {
          project_id: query.project_id,
        },
      )
      .await?
      .workflow;
    workflow.id = query.id;
    if let Some(version) = query.version {
      workflow.version = version;
    }
    Ok(workflow)
  }
}

#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::SelectDefaultIssueWorkflowCommand> for WorkflowPeer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::SelectDefaultIssueWorkflowCommand,
  ) -> Result<octacity_server_application::IssueWorkflowSelectionOutcome, ApplicationError> {
    assert_eq!(command.intent.expected_version.get(), 1);
    assert_eq!(command.intent.workflow_version.get(), 2);
    assert_eq!(command.idempotency_key.as_str(), "select-policy");
    let mut default = self
      .execute_management_query(
        context,
        grant,
        GetDefaultIssueWorkflowQuery {
          project_id: command.intent.project_id,
        },
      )
      .await?;
    default.version = serde_json::from_value(serde_json::json!(2)).unwrap();
    default.workflow.id = command.intent.workflow_id;
    default.workflow.version = command.intent.workflow_version;
    Ok(octacity_server_application::IssueWorkflowSelectionOutcome {
      disposition: octacity_server_application::StoreMutationDisposition::Applied,
      default,
    })
  }
}
#[tokio::test]
async fn issue_workflow_default_http_selects_an_exact_pin_under_its_own_precondition() {
  let path = "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflow-default";
  let body = r#"{"workflow_id":"20000000-0000-4000-8000-000000000001","workflow_version":2}"#;
  let router = workflow_router();
  let response = router
    .clone()
    .oneshot(json_request("POST", path, "select-policy", Some("\"1\""), body))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let result: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  assert_eq!(result["resource"]["version"], 2);
  assert_eq!(
    result["resource"]["workflow"]["id"],
    "20000000-0000-4000-8000-000000000001"
  );
  assert_eq!(result["resource"]["workflow"]["version"], 2);
  for (version, status) in [
    (None, StatusCode::PRECONDITION_REQUIRED),
    (Some("W/\"1\""), StatusCode::BAD_REQUEST),
  ] {
    let response = router
      .clone()
      .oneshot(json_request("POST", path, "select-policy", version, body))
      .await
      .unwrap();
    assert_eq!(response.status(), status);
  }
}

#[tokio::test]
async fn issue_workflow_discovery_http_is_bounded_and_returns_summaries() {
  let router = workflow_router();
  let path = "/api/v1/projects/0199a6f4-d56c-7440-9aa2-6a320f862795/issue-workflows";
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let page: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
  assert_eq!(page["items"].as_array().unwrap().len(), 1);
  assert_eq!(page["items"][0]["name"], "Local workflow");
  assert!(page["items"][0].get("definition").is_none());
  assert!(page["next_cursor"].is_null());
  for query in ["limit=0", "limit=201", "after=not-a-workflow-id", "provider=github"] {
    let response = router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}?{query}"), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
  }
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ListProjectIssueWorkflowsQuery> for WorkflowPeer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    _: octacity_server_application::ListProjectIssueWorkflowsQuery,
  ) -> Result<octacity_server_application::IssueWorkflowPage, ApplicationError> {
    let workflow = self
      .execute_management_query(
        context,
        grant,
        GetDefaultIssueWorkflowQuery {
          project_id: "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap(),
        },
      )
      .await?
      .workflow;
    Ok(octacity_server_application::IssueWorkflowPage {
      items: vec![workflow.into()],
      next_cursor: None,
    })
  }
}
