use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode},
};
use octacity_server_api_rest::integrations::{
  IntegrationManagementApplication, ProtectedIntegrationRequest, integration_management_routes,
};
use octacity_server_application::*;
use serde_json::json;
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt as _;

struct Policy;
#[async_trait]
impl ManagementAuthorizationPolicy for Policy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
#[derive(Default)]
struct Peer(AtomicUsize);
#[async_trait]
impl ManagementCommandUseCase<ManageIntegrationCredentialCommand> for Peer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: ManageIntegrationCredentialCommand,
  ) -> Result<IntegrationAdministrationReceipt, ApplicationError> {
    self.0.fetch_add(1, Ordering::SeqCst);
    assert_eq!(command.value.unwrap().expose_for_installation(), b"private-rest-token");
    Ok(IntegrationAdministrationReceipt {
      intent: command.request.intent,
      operation: [7; 32],
      revision: 1,
      requested_at: Timestamp::from_unix_millis(10).unwrap(),
      publication: None,
      status: IntegrationAdministrationStatus::Completed,
    })
  }
}
#[async_trait]
impl ManagementCommandUseCase<CleanupIntegrationPreparationsCommand> for Peer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: CleanupIntegrationPreparationsCommand,
  ) -> Result<usize, ApplicationError> {
    assert_eq!(command.limit, 2);
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(2)
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetIntegrationAdministrationQuery> for Peer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIntegrationAdministrationQuery,
  ) -> Result<IntegrationAdministrationState, ApplicationError> {
    // Retained references span metadata versions, exceeding one version's resource ceiling.
    let credentials: Vec<_> = (0..257)
      .map(|index| json!({"reference":{"provider":"installed","reference":format!("historical-{index}")},"configured":false}))
      .collect();
    Ok(
      serde_json::from_value(
        json!({"id":query.id,"revision":2,"enabled":false,"active_version":1,"credentials":credentials,"pending":null}),
      )
      .unwrap(),
    )
  }
}

fn exact(name: &str) -> serde_json::Value {
  json!({"identity":name,"version":"v1","digest":"07".repeat(32)})
}
fn metadata() -> serde_json::Value {
  json!({"id":"0199a6f4-d56c-7440-9aa2-6a320f862795","version":1,"digest":"07".repeat(32),"published_at":10,
    "definition":{"name":"Tracker","provider":"jira","installation":exact("installation"),"adapter":exact("adapter"),
      "endpoint":"https://tracker.invalid/","authentication":"token","resources":[{"reference":exact("tickets"),
        "native_id":"external-project","locator":"team/project","capabilities":[{"purpose":"task_read","capability":exact("read"),
          "configuration":exact("profile"),"credential":{"provider":"installed","reference":"reader"}}]}]}})
}
#[async_trait]
impl ManagementCommandUseCase<PublishIntegrationConnectionCommand> for Peer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishIntegrationConnectionCommand,
  ) -> Result<IntegrationConnectionMutationOutcome, ApplicationError> {
    assert_eq!(command.intent.draft.name.as_str(), "Tracker");
    assert!(command.intent.expected_current_version.is_none());
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(serde_json::from_value(json!({"disposition":"applied","connection":metadata()})).unwrap())
  }
}
#[async_trait]
impl ManagementQueryUseCase<GetIntegrationConnectionQuery> for Peer {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIntegrationConnectionQuery,
  ) -> Result<PublishedIntegrationConnection, ApplicationError> {
    assert_eq!(query.id.to_string(), "0199a6f4-d56c-7440-9aa2-6a320f862795");
    Ok(serde_json::from_value(metadata()).unwrap())
  }
}
#[async_trait]
impl ManagementCommandUseCase<SetIntegrationConnectionStateCommand> for Peer {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: SetIntegrationConnectionStateCommand,
  ) -> Result<IntegrationAdministrationReceipt, ApplicationError> {
    assert_eq!(command.request.intent.action, IntegrationAdministrationAction::Disable);
    assert_eq!(command.request.intent.expected_revision, 1);
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(IntegrationAdministrationReceipt {
      intent: command.request.intent,
      operation: [8; 32],
      revision: 2,
      requested_at: Timestamp::from_unix_millis(20).unwrap(),
      publication: None,
      status: IntegrationAdministrationStatus::Completed,
    })
  }
}

fn proof() -> ProtectedIntegrationRequest {
  ProtectedIntegrationRequest::new(
    ManagementRequestContext::new(
      ManagementActor::authenticated("admin").unwrap(),
      ManagementSecurityScope::new("rest-integration-admin").unwrap(),
      ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
      ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
    )
    .unwrap(),
  )
  .unwrap()
}

#[test]
fn integration_openapi_retains_typed_authorization_and_write_only_secret_submission() {
  let document = octacity_server_api_rest::v1::openapi_document();
  let operation = &document["paths"]["/api/v1/integrations/{integration_id}/credentials"]["post"];
  assert_eq!(operation["operationId"], "manageIntegrationCredential");
  assert_eq!(operation["security"], json!([{"ProtectedManagementIngress":[]}]));
  let schema = &document["components"]["schemas"]["IntegrationCredentialSubmission"];
  assert_eq!(schema["properties"]["value"]["writeOnly"], true);
  assert_eq!(schema["properties"]["value"]["maxLength"], 16384);
  let operations = octacity_server_api_rest::v1::management_authorization_operations();
  let credential = operations
    .iter()
    .find(|o| o.operation_id == "manageIntegrationCredential")
    .unwrap();
  assert_eq!(
    credential.authorization,
    ManageIntegrationCredentialCommand::AUTHORIZATION
  );
}

#[test]
fn integration_openapi_name_limit_matches_publication_validation() {
  let document = octacity_server_api_rest::v1::openapi_document();
  let schemas = &document["components"]["schemas"];
  let draft = &schemas["IntegrationConnectionPublication"]["properties"]["intent"]["properties"]["draft"];
  let definition = &schemas["IntegrationConnection"]["properties"]["definition"];
  for (name, accepted) in [("a".repeat(128), true), ("a".repeat(129), false)] {
    let intent = json!({"id":"0199a6f4-d56c-7440-9aa2-6a320f862795","expected_current_version":null,
      "draft":{"name":name,"installation":exact("installation"),"endpoint":"https://tracker.invalid/",
        "authentication":"token","resources":[{"reference":exact("tickets"),"purposes":["task_read"]}]}});
    assert_eq!(
      serde_json::from_value::<IntegrationConnectionIntent>(intent).is_ok(),
      accepted
    );
    for envelope in [draft, definition] {
      let maximum = envelope["properties"]["name"]["maxLength"].as_u64().unwrap();
      assert_eq!(name.chars().count() as u64 <= maximum, accepted);
    }
  }
}
fn body() -> serde_json::Value {
  json!({"intent":{"id":"0199a6f4-d56c-7440-9aa2-6a320f862795", "expected_metadata_version":1,"expected_revision":0,
    "action":{"operation":"credential","reference":{"provider":"installed","reference":"source-reader"},"kind":"provision"}},"value":"private-rest-token"})
}
#[tokio::test]
async fn credential_rest_requires_verified_protected_ingress_and_redacts_success_and_rejections() {
  let peer = Arc::new(Peer::default());
  let commands = Arc::new(AuthorizedCommandHandler::new(Arc::new(Policy), peer.clone()));
  let router = integration_management_routes(IntegrationManagementApplication::new(commands));
  let path = "/api/v1/integrations/0199a6f4-d56c-7440-9aa2-6a320f862795/credentials";
  for verified in [false, true] {
    let mut request = Request::builder()
      .method("POST")
      .uri(path)
      .header("content-type", "application/json")
      .header("idempotency-key", "credential-once")
      .header("if-none-match", "*")
      .header("authorization", "Bearer spoofed")
      .header("x-forwarded-proto", "https")
      .body(Body::from(body().to_string()))
      .unwrap();
    if verified {
      request.extensions_mut().insert(proof());
    }
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(
      response.status(),
      if verified {
        StatusCode::OK
      } else {
        StatusCode::FORBIDDEN
      }
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    let response_body = to_bytes(response.into_body(), 8192).await.unwrap();
    assert!(!String::from_utf8_lossy(&response_body).contains("private-rest-token"));
    if verified {
      let receipt: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
      assert_eq!(receipt["revision"], 1);
      assert_eq!(receipt["status"], "completed");
    }
  }
  let mut missing_precondition = Request::builder()
    .method("POST")
    .uri(path)
    .header("content-type", "application/json")
    .header("idempotency-key", "missing-precondition")
    .body(Body::from(body().to_string()))
    .unwrap();
  missing_precondition.extensions_mut().insert(proof());
  assert_eq!(
    router.clone().oneshot(missing_precondition).await.unwrap().status(),
    StatusCode::PRECONDITION_REQUIRED
  );
  for payload in [
    body().to_string().replace("private-rest-token", &"x".repeat(17 * 1024)),
    r#"{"intent":{},"value":"private-rest-token","path":"/private/credential"}"#.to_string(),
    "{\"value\":\"private-rest-token\",broken".into(),
  ] {
    let mut request = Request::builder()
      .method("POST")
      .uri(path)
      .header("content-type", "application/json")
      .header("idempotency-key", "invalid")
      .body(Body::from(payload))
      .unwrap();
    request.extensions_mut().insert(proof());
    let response = router.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_client_error());
    assert!(
      !String::from_utf8_lossy(&to_bytes(response.into_body(), 8192).await.unwrap()).contains("private-rest-token")
    );
  }
  assert_eq!(peer.0.load(Ordering::SeqCst), 1);
  assert!(
    ProtectedIntegrationRequest::new(ManagementRequestContext::trusted_network(
      ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()
    ))
    .is_err()
  );
}

#[tokio::test]
async fn connection_rest_publishes_and_inspects_provider_neutral_metadata_and_disables_with_preconditions() {
  let peer = Arc::new(Peer::default());
  let commands = Arc::new(AuthorizedCommandHandler::new(Arc::new(Policy), peer.clone()));
  let queries = Arc::new(AuthorizedQueryHandler::new(Arc::new(Policy), peer.clone()));
  let router = integration_management_routes(
    IntegrationManagementApplication::new(commands.clone())
      .with_metadata(commands.clone(), queries.clone())
      .with_administration(commands, queries),
  );
  let base = "/api/v1/integrations/0199a6f4-d56c-7440-9aa2-6a320f862795";
  let payload = json!({"intent":{"id":"0199a6f4-d56c-7440-9aa2-6a320f862795","expected_current_version":null,
    "draft":{"name":"Tracker","installation":exact("installation"),"endpoint":"https://tracker.invalid/","authentication":"token",
      "resources":[{"reference":exact("tickets"),"purposes":["task_read"]}]}}});
  let mut publish = Request::builder()
    .method("POST")
    .uri(format!("{base}/versions"))
    .header("content-type", "application/json")
    .header("idempotency-key", "publish")
    .header("if-none-match", "*")
    .body(Body::from(payload.to_string()))
    .unwrap();
  publish.extensions_mut().insert(proof());
  let published = router.clone().oneshot(publish).await.unwrap();
  assert_eq!(published.status(), StatusCode::OK);
  let document: serde_json::Value =
    serde_json::from_slice(&to_bytes(published.into_body(), 16384).await.unwrap()).unwrap();
  assert_eq!(document["connection"]["definition"]["provider"], "jira");
  assert_eq!(document["connection"]["version"], 1);
  for suffix in ["", "/versions/1"] {
    let mut request = Request::builder()
      .uri(format!("{base}{suffix}"))
      .body(Body::empty())
      .unwrap();
    request.extensions_mut().insert(proof());
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let document: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
    assert_eq!(document["definition"]["provider"], "jira");
  }
  let payload = json!({"intent":{"id":"0199a6f4-d56c-7440-9aa2-6a320f862795","expected_metadata_version":1,"expected_revision":1,"action":{"operation":"disable"}}});
  for (header, status) in [("\"2\"", StatusCode::PRECONDITION_FAILED), ("\"1\"", StatusCode::OK)] {
    let mut request = Request::builder()
      .method("POST")
      .uri(format!("{base}/state"))
      .header("content-type", "application/json")
      .header("idempotency-key", "disable")
      .header("if-match", header)
      .body(Body::from(payload.to_string()))
      .unwrap();
    request.extensions_mut().insert(proof());
    assert_eq!(router.clone().oneshot(request).await.unwrap().status(), status);
  }
  assert_eq!(peer.0.load(Ordering::SeqCst), 2);
  let mut state = Request::builder()
    .uri(format!("{base}/state"))
    .body(Body::empty())
    .unwrap();
  state.extensions_mut().insert(proof());
  let state = router.clone().oneshot(state).await.unwrap();
  assert_eq!(state.status(), StatusCode::OK);
  let state: serde_json::Value = serde_json::from_slice(&to_bytes(state.into_body(), 65536).await.unwrap()).unwrap();
  assert_eq!(state["enabled"], false);
  let retained = state["credentials"].as_array().unwrap();
  assert_eq!(retained.len(), 257);
  let schema = &octacity_server_api_rest::v1::openapi_document()["components"]["schemas"]["IntegrationAdministrationState"]
    ["properties"]["credentials"];
  assert!(
    schema["maxItems"]
      .as_u64()
      .is_none_or(|maximum| retained.len() as u64 <= maximum),
    "the response schema must accept retained credentials across metadata versions"
  );
  for (limit, expected) in [
    (0, StatusCode::BAD_REQUEST),
    (101, StatusCode::BAD_REQUEST),
    (2, StatusCode::OK),
  ] {
    let mut request = Request::builder()
      .method("POST")
      .uri(format!("{base}/preparations/cleanup"))
      .header("content-type", "application/json")
      .body(Body::from(json!({"limit":limit}).to_string()))
      .unwrap();
    request.extensions_mut().insert(proof());
    assert_eq!(router.clone().oneshot(request).await.unwrap().status(), expected);
  }
  assert_eq!(peer.0.load(Ordering::SeqCst), 3);
}
