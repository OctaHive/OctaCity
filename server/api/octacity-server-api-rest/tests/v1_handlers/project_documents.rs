//! Named Project input transport stays independent of Build and Work admission.
use super::*;
use octacity_server_application::*;

#[derive(Default)]
struct Documents {
  publications: Mutex<Vec<PublishProjectDocumentCommand>>,
  pages: Mutex<Vec<ListProjectInputDocumentsQuery>>,
  reads: Mutex<Vec<ReadProjectInputDocumentQuery>>,
}
#[async_trait]
impl ManagementQueryUseCase<ReadProjectInputDocumentQuery> for Documents {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ReadProjectInputDocumentQuery,
  ) -> Result<ProjectDocumentContent, ApplicationError> {
    let document = resource(query.project_id);
    assert_eq!(query.artifact_id, document.artifact.artifact_id());
    self.reads.lock().unwrap().push(query);
    Ok(ProjectDocumentContent {
      project_id: document.project_id,
      artifact: document.artifact,
      content: "Private goals\r\nexactly\n".into(),
    })
  }
}

#[tokio::test]
async fn project_document_content_is_explicitly_read_through_the_project_owner_without_caching() {
  let peer = Arc::new(RecordingApplication::default());
  let documents = Arc::new(Documents::default());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let app = recording_management_application(peer.clone(), peer.clone()).with_project_documents(
    Arc::new(AuthorizedCommandHandler::new(policy.clone(), documents.clone())),
    Arc::new(AuthorizedQueryHandler::new(policy, documents.clone())),
  );
  let router = management_router_with_application(|| true, app);
  let response = router
    .oneshot(empty_request(
      "GET",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/input-documents/22222222-2222-4222-8222-222222222222",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["content"], "Private goals\r\nexactly\n");
  assert_json_matches_component(&openapi_document(), "ProjectDocumentContent", &body);
  assert_eq!(documents.reads.lock().unwrap().len(), 1);
  assert!(documents.publications.lock().unwrap().is_empty());
  assert!(documents.pages.lock().unwrap().is_empty());
  assert!(peer.calls.lock().unwrap().is_empty());
}
fn resource(project: ProjectId) -> ProjectDocumentResource {
  ProjectDocumentResource {
    project_id: project,
    name: "Reviewed Project goals".parse().unwrap(),
    artifact: serde_json::from_value(serde_json::json!({
      "artifact_id":"22222222-2222-4222-8222-222222222222",
      "content_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "encoded_size":23
    }))
    .unwrap(),
    published_at: Timestamp::from_unix_millis(20).unwrap(),
    expires_at: Timestamp::from_unix_millis(2000).unwrap(),
  }
}
#[async_trait]
impl ManagementCommandUseCase<PublishProjectDocumentCommand> for Documents {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishProjectDocumentCommand,
  ) -> Result<ProjectDocumentResource, ApplicationError> {
    let result = resource(command.submission.project_id);
    self.publications.lock().unwrap().push(command);
    Ok(result)
  }
}
#[async_trait]
impl ManagementQueryUseCase<ListProjectInputDocumentsQuery> for Documents {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListProjectInputDocumentsQuery,
  ) -> Result<ProjectDocumentPage, ApplicationError> {
    let result = ProjectDocumentPage {
      items: vec![resource(query.page.project_id())],
      next_cursor: None,
    };
    self.pages.lock().unwrap().push(query);
    Ok(result)
  }
}
#[tokio::test]
async fn project_documents_http_publishes_exact_reviewed_text_and_lists_only_metadata() {
  let peer = Arc::new(RecordingApplication::default());
  let documents = Arc::new(Documents::default());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let app = recording_management_application(peer.clone(), peer.clone()).with_project_documents(
    Arc::new(AuthorizedCommandHandler::new(policy.clone(), documents.clone())),
    Arc::new(AuthorizedQueryHandler::new(policy, documents.clone())),
  );
  let router = management_router_with_application(|| true, app);
  let path = "/api/v1/projects/11111111-1111-4111-8111-111111111111/input-documents";
  for schema in [
    "PublishProjectDocumentRequest",
    "ProjectDocumentResource",
    "ProjectDocumentPage",
  ] {
    assert_component_exists(&openapi_document(), schema);
  }
  let body = serde_json::json!({"name":"Reviewed Project goals","content":"Private goals\r\nexactly\n"});
  assert_json_matches_component(&openapi_document(), "PublishProjectDocumentRequest", &body);
  let response = router
    .clone()
    .oneshot(json_request("POST", path, "reviewed-goals", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let published: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_json_matches_component(&openapi_document(), "ProjectDocumentResource", &published);
  let response = router
    .oneshot(empty_request("GET", &format!("{path}?limit=5"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_json_matches_component(&openapi_document(), "ProjectDocumentPage", &page);
  assert_eq!(page["items"], serde_json::json!([published]));
  assert!(!page.to_string().contains("Private goals"));
  let commands = documents.publications.lock().unwrap();
  assert_eq!(commands[0].submission.content, "Private goals\r\nexactly\n");
  assert_eq!(
    commands[0].submission.project_id.to_string(),
    "11111111-1111-4111-8111-111111111111"
  );
  assert_eq!(commands[0].submission.idempotency_key.as_str(), "reviewed-goals");
  assert_eq!(documents.pages.lock().unwrap()[0].page.limit(), 5);
  assert!(
    peer.calls.lock().unwrap().is_empty(),
    "document publication cannot dispatch admission or execution handlers"
  );
}
