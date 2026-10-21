//! Wire behavior over authorized public result contracts; no provider or byte URL is accepted.
use super::*;
use octacity_server_api_rest::v1::IssueResultPanelManagementApplication;
use octacity_server_application::*;
const ISSUE: &str = "60000000-0000-4000-8000-000000000001";
const WORK: &str = "60000000-0000-4000-8000-000000000002";
const ARTIFACT: &str = "60000000-0000-4000-8000-000000000003";
struct PanelRead(AtomicUsize);
fn association() -> serde_json::Value {
  serde_json::json!({"project_id":PROJECT,"issue_id":ISSUE,"request_id":null,"requirements_version":null,"work_id":WORK,"run_id":"60000000-0000-4000-8000-000000000004"})
}
#[async_trait]
impl ManagementQueryUseCase<ListIssueChecksQuery> for PanelRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListIssueChecksQuery,
  ) -> Result<IssueChecksPage, ApplicationError> {
    assert_eq!(query.execution().work_id.to_string(), WORK);
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(IssueChecksPage {association:serde_json::from_value(association()).unwrap(),subject:serde_json::from_value(serde_json::json!({"exact":{"project_id":PROJECT,"repository_id":"60000000-0000-4000-8000-000000000005","base_revision":"base"},"candidate_revision":"original-head","changeset_digest":"cd".repeat(32)})).unwrap(),record:"ab".repeat(32).parse().unwrap(),run_version:serde_json::from_value(serde_json::json!(1)).unwrap(),coverage:IssueResultCoverage::Partial,items:vec![],next_cursor:None})
  }
}
#[async_trait]
impl ManagementQueryUseCase<ListIssueArtifactsQuery> for PanelRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListIssueArtifactsQuery,
  ) -> Result<IssueArtifactsPage, ApplicationError> {
    assert_eq!(query.execution().work_id.to_string(), WORK);
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(IssueArtifactsPage {
      association: serde_json::from_value(association()).unwrap(),
      run_version: serde_json::from_value(serde_json::json!(1)).unwrap(),
      coverage: IssueResultCoverage::Partial,
      items: vec![],
      next_cursor: None,
    })
  }
}
#[async_trait]
impl ManagementQueryUseCase<ReadIssueArtifactContentQuery> for PanelRead {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ReadIssueArtifactContentQuery,
  ) -> Result<IssueArtifactContent, ApplicationError> {
    assert_eq!(query.execution.work_id.to_string(), WORK);
    assert_eq!(query.artifact_id.to_string(), ARTIFACT);
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(IssueArtifactContent {
      media_type: if query.mode == IssueArtifactReadMode::Preview {
        "text/plain; charset=utf-8"
      } else {
        "application/octet-stream"
      }
      .into(),
      bytes: b"retained <script>text</script>".to_vec(),
    })
  }
}
#[tokio::test]
async fn issue_result_panels_http_translate_exact_bounded_reads_and_safe_content() {
  let peer = Arc::new(RecordingApplication::default());
  let reads = Arc::new(PanelRead(AtomicUsize::new(0)));
  let authorized = Arc::new(AuthorizedQueryHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    reads.clone(),
  ));
  let router = management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer).with_issue_result_panels(
      IssueResultPanelManagementApplication::new(authorized.clone(), authorized),
    ),
  );
  let path = format!("{}/{ISSUE}/results", root());
  for (suffix, component) in [
    (
      format!("checks?work_id={WORK}&record={}&limit=1", "ab".repeat(32)),
      "IssueChecksPage",
    ),
    (format!("artifacts?work_id={WORK}&limit=1"), "IssueArtifactsPage"),
  ] {
    let response = router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/{suffix}"), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let page = document(response).await;
    assert_eq!(page["association"]["work_id"], WORK);
    assert_eq!(page["coverage"], "partial");
    assert_json_matches_component(&openapi_document(), component, &page);
  }
  for (mode, disposition, media) in [
    ("preview", "inline", "text/plain; charset=utf-8"),
    ("download", "attachment", "application/octet-stream"),
  ] {
    let response = router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{path}/artifacts/{ARTIFACT}/content?work_id={WORK}&mode={mode}"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], media);
    assert_eq!(response.headers()["content-disposition"], disposition);
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
      response.headers()["content-security-policy"],
      "default-src 'none'; sandbox"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
      to_bytes(response.into_body(), 1024).await.unwrap(),
      b"retained <script>text</script>"[..]
    );
  }
  assert_eq!(reads.0.load(Ordering::SeqCst), 4);
  for suffix in [
    format!("checks?work_id={WORK}"),
    format!("checks?work_id={WORK}&record={}&limit=101", "ab".repeat(32)),
    format!("artifacts?work_id={WORK}&limit=0"),
    format!("artifacts?work_id={WORK}&url=https://provider.example"),
    format!("artifacts/{ARTIFACT}/content?work_id={WORK}&mode=html"),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}/{suffix}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  assert_eq!(
    reads.0.load(Ordering::SeqCst),
    4,
    "invalid selectors never reach a reader"
  );
}

#[test]
fn issue_artifact_content_openapi_declares_unsupported_preview_capability() {
  let doc = openapi_document();
  let operation =
    &doc["paths"]["/api/v1/projects/{project_id}/issues/{issue_id}/results/artifacts/{artifact_id}/content"]["get"];
  assert_eq!(
    operation["responses"]["422"]["$ref"],
    "#/components/responses/ManagementError"
  );
  let content = operation["responses"]["200"]["content"].as_object().unwrap();
  assert_eq!(
    content.keys().map(String::as_str).collect::<BTreeSet<_>>(),
    ["application/octet-stream", "image/png", "text/plain"]
      .into_iter()
      .collect()
  );
  assert!(!content.contains_key("text/html") && !content.contains_key("image/svg+xml"));
}
