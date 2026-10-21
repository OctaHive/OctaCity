//! PR/MR HTTP contracts over real Issue/review application owners and retained memory storage.
use super::*;
use octacity_server_api_rest::v1::IssueCodeReviewManagementApplication;
use octacity_server_application::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct ReviewReader {
  reads: AtomicUsize,
  revoked: AtomicBool,
  failed: AtomicBool,
  hidden: AtomicBool,
}
fn selection(repository: &str, item: &str) -> IssueCodeReviewSelection {
  serde_json::from_value(serde_json::json!({"source":{
    "installation":{"identity":"code-forge","version":"v1","digest":"01".repeat(32)},
    "capability":{"identity":"review-read","version":"v1","digest":"02".repeat(32)},
    "configuration":{"identity":"review-profile","version":"v1","digest":"03".repeat(32)},
    "resource":{"identity":repository,"version":"v1","digest":"04".repeat(32)}},
    "item":{"resource":repository,"item":item}}))
  .unwrap()
}
#[async_trait]
impl IssueCodeReviewObservations for ReviewReader {
  async fn visibility(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
  ) -> Result<IssueCodeReviewRepositoryVisibility, ApplicationError> {
    use IssueCodeReviewRepositoryVisibility as Visibility;
    Ok(if self.hidden.load(Ordering::SeqCst) {
      Visibility::restricted([selection("repo-b", "MR-23").repository_identity()]).unwrap()
    } else {
      Visibility::all()
    })
  }
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    selected: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    if self.revoked.load(Ordering::SeqCst)
      || ![
        selection("repo-a", selected.item.item.as_str()),
        selection("repo-b", selected.item.item.as_str()),
      ]
      .contains(selected)
    {
      return Err(ApplicationError::capability_unavailable());
    }
    Ok(())
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    self.authorize(context, project, selection).await?;
    let read = self.reads.fetch_add(1, Ordering::SeqCst);
    if self.failed.load(Ordering::SeqCst) {
      return Err(ApplicationError::SourceDependency {
        class: WorkSourceFailure::Transient,
        retry_after_millis: None,
      });
    }
    Ok(serde_json::from_value(serde_json::json!({"selection":selection,"observed_at":20+read as u64,
      "title":"Review title","number":"MR-23","url":"https://code.example/review/23","author":"visible-author",
      "state":"open","draft":true,"head":format!("head-{read}"),"base":"original-base","changed_files":4,"additions":7,"deletions":null,"review":null,"checks":null,"check_details":{"source":"external_ci","head":format!("head-{read}"),"observed_at":20+read as u64,"has_more":true,"items":[{"id":"ci-17","name":"contract","producer":null,"state":"skipped","required":null}]}})).unwrap())
  }
}
struct ReviewClock;
impl FactoryExecutionClock for ReviewClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(100).unwrap())
  }
}
async fn review_router(reader: Arc<ReviewReader>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> axum::Router {
  let store = manual_issue_store(PROJECT.parse().unwrap()).await;
  let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
  let reviews = Arc::new(IssueCodeReviewHandlers::new(
    store,
    reader,
    policy.clone(),
    Arc::new(ReviewClock),
  ));
  let peer = Arc::new(RecordingApplication::default());
  management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer)
      .with_issues(IssueManagementApplication::new(
        Arc::new(AuthorizedCommandHandler::new(policy.clone(), issues.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy.clone(), issues)),
      ))
      .with_issue_code_reviews(
        IssueCodeReviewManagementApplication::new(
          Arc::new(AuthorizedCommandHandler::new(policy.clone(), reviews.clone())),
          Arc::new(AuthorizedQueryHandler::new(policy.clone(), reviews.clone())),
        )
        .with_checks(Arc::new(AuthorizedQueryHandler::new(policy, reviews))),
      ),
  )
}
async fn manual(router: &axum::Router, key: &str) -> serde_json::Value {
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), key, None, &draft().to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  document(response).await["resource"].clone()
}
fn collection(issue: &serde_json::Value) -> String {
  format!("{}/{}/code-reviews", root(), issue["id"].as_str().unwrap())
}

#[tokio::test]
async fn review_http_attachment_uses_the_independent_reader_and_preserves_manual_issue_state() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let issue = manual(&router, "draft-with-review").await;
  let selected = selection("repo-a", "native-mr-23");
  let body = serde_json::json!({"selection":selected}).to_string();
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "attach-review",
      Some("\"1\""),
      &body,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let attached = document(response).await;
  assert_json_matches_component(&openapi_document(), "IssueCodeReviewOutcome", &attached);
  assert_eq!(attached["disposition"], "applied");
  assert_eq!(
    attached["association"]["selection"],
    serde_json::to_value(selected).unwrap()
  );
  assert_eq!(attached["association"]["issue_id"], issue["id"]);
  assert_eq!(attached["association"]["provenance"]["kind"], "explicit");
  assert!(attached["association"]["binding"].is_null());
  assert_eq!(reader.reads.load(Ordering::SeqCst), 1);
  let current = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{}/{}", root(), issue["id"].as_str().unwrap()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(
    document(current).await,
    issue,
    "attaching a review creates no task revision, status move or execution"
  );
}

#[tokio::test]
async fn review_http_reads_retained_facts_and_blocked_availability_without_provider_dispatch() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let issue = manual(&router, "retained-review").await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "attach",
      Some("\"1\""),
      &serde_json::json!({"selection":selection("repo-a","native-review")}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  for revoked in [false, true] {
    reader.revoked.store(revoked, Ordering::SeqCst);
    let response = router
      .clone()
      .oneshot(empty_request("GET", &format!("{}?limit=1", collection(&issue)), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let page = document(response).await;
    assert_json_matches_component(&openapi_document(), "IssueCodeReviewPage", &page);
    assert_eq!(page["observation"], "issue-code-review-membership-v1");
    assert!(page["next_cursor"].is_null());
    assert_eq!(
      page["items"][0]["availability"],
      if revoked { "blocked" } else { "available" }
    );
    let facts = &page["items"][0]["observation"];
    assert_eq!(facts["title"], "Review title");
    assert_eq!(facts["number"], "MR-23");
    assert_eq!(facts["url"], "https://code.example/review/23");
    assert_eq!(facts["author"], "visible-author");
    assert_eq!(facts["state"], "open");
    assert_eq!(facts["draft"], true);
    assert_eq!(facts["head"], "head-0");
    assert_eq!(facts["base"], "original-base");
    assert_eq!(facts["changed_files"], 4);
    assert_eq!(facts["additions"], 7);
    assert!(facts["deletions"].is_null() && facts["review"].is_null() && facts["checks"].is_null());
    assert_eq!(facts["observed_at"], 20);
  }
  assert_eq!(reader.reads.load(Ordering::SeqCst), 1, "GET never reads the provider");
}

#[tokio::test]
async fn review_http_refresh_retains_original_head_history_replay_and_outage_data() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let issue = manual(&router, "review-history").await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "attach",
      Some("\"1\""),
      &serde_json::json!({"selection":selection("repo-a","review-1")}).to_string(),
    ))
    .await
    .unwrap();
  let attached = document(response).await;
  let path = format!(
    "{}/{}/observations",
    collection(&issue),
    attached["association"]["id"].as_str().unwrap()
  );
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "refresh", None, "{}"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let refreshed = document(response).await;
  assert_json_matches_component(&openapi_document(), "IssueCodeReviewObservationOutcome", &refreshed);
  assert_eq!(refreshed["observation"]["observation"]["head"], "head-1");
  assert_eq!(refreshed["observation"]["sequence"], 2);
  reader.failed.store(true, Ordering::SeqCst);
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "refresh", None, "{}"))
    .await
    .unwrap();
  let replay = document(response).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["observation"], refreshed["observation"]);
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    2,
    "replay precedes provider dispatch"
  );
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "outage", None, "{}"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let first = document(response).await;
  assert_eq!(first["items"][0]["observation"]["head"], "head-0");
  let cursor = first["next_cursor"].as_str().unwrap();
  let foreign = format!(
    "{}/{}/observations?limit=1&after={cursor}",
    collection(&issue),
    "ab".repeat(32)
  );
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &foreign, None))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  let second = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}?limit=1&after={cursor}"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(second["items"][0]["observation"]["head"], "head-1");
  assert_eq!(second["association"], attached["association"]);
  assert_eq!(second["items"][0]["observation"]["observed_at"], 21);
  assert!(second["next_cursor"].is_null());
  let list = document(
    router
      .clone()
      .oneshot(empty_request("GET", &collection(&issue), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(list["items"][0]["observation"]["head"], "head-1");
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    3,
    "outage neither removes nor refreshes retained facts; GET performs no read"
  );
  assert_json_matches_component(&openapi_document(), "IssueCodeReviewObservationPage", &second);
}

#[tokio::test]
async fn review_http_rejects_stale_requirements_before_read_but_preserves_original_replay() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let issue = manual(&router, "stale-review").await;
  let issue_path = format!("{}/{}", root(), issue["id"].as_str().unwrap());
  let mut edit = draft();
  edit["title"] = serde_json::json!("Updated requirements");
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &issue_path,
      "edit",
      Some("\"1\""),
      &edit.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = serde_json::json!({"selection":selection("repo-a","review-1")}).to_string();
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "stale-attach",
      Some("\"1\""),
      &body,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CONFLICT);
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    0,
    "stale reviewed requirements cannot spend an external read"
  );
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "attach-current",
      Some("\"2\""),
      &body,
    ))
    .await
    .unwrap();
  let original = document(response).await;
  edit["title"] = serde_json::json!("Later requirements");
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &issue_path,
        "later-edit",
        Some("\"2\""),
        &edit.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  reader.revoked.store(true, Ordering::SeqCst);
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "attach-current",
      Some("\"2\""),
      &body,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let replay = document(response).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["association"], original["association"]);
  assert_eq!(reader.reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn review_http_rejects_unreviewed_or_uninstalled_authority_and_closed_body_setters() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let issue = manual(&router, "review-boundaries").await;
  let body = serde_json::json!({"selection":selection("repo-a","review")});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &collection(&issue),
        "missing-review",
        None,
        &body.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::PRECONDITION_REQUIRED
  );
  for setter in [
    "credentials",
    "url",
    "provenance",
    "status_id",
    "execution_state",
    "accepted",
  ] {
    let mut invalid = body.clone();
    invalid[setter] = serde_json::json!("caller-owned");
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &collection(&issue),
          setter,
          Some("\"1\""),
          &invalid.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST,
      "{setter}"
    );
  }
  for repo in ["foreign-repository", "repo-a"] {
    reader.revoked.store(repo == "repo-a", Ordering::SeqCst);
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &collection(&issue),
        repo,
        Some("\"1\""),
        &serde_json::json!({"selection":selection(repo,"native-review")}).to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
  }
  assert_eq!(reader.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn review_http_pages_are_visibility_bound_and_cannot_cross_issues_or_associations() {
  let reader = Arc::new(ReviewReader::default());
  let scope = Arc::new(Mutex::new(None));
  let router = review_router(reader.clone(), Arc::new(IssueScope(scope.clone()))).await;
  let issue = manual(&router, "review-page").await;
  let other = manual(&router, "other-issue").await;
  let mut ids = Vec::new();
  for (repository, item) in [("repo-a", "review-a"), ("repo-b", "review-b"), ("repo-b", "review-c")] {
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &collection(&issue),
        item,
        Some("\"1\""),
        &serde_json::json!({"selection":selection(repository,item)}).to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    ids.push(
      document(response).await["association"]["id"]
        .as_str()
        .unwrap()
        .to_owned(),
    );
  }
  let first = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{}?limit=1", collection(&issue)), None))
      .await
      .unwrap(),
  )
  .await;
  let cursor = first["next_cursor"].as_str().unwrap();
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{}?limit=1&after={cursor}", collection(&other)),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  reader.hidden.store(true, Ordering::SeqCst);
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{}?limit=1&after={cursor}", collection(&issue)),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  let visible = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{}?limit=1", collection(&issue)), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(visible["items"].as_array().unwrap().len(), 1);
  assert_eq!(
    visible["items"][0]["association"]["selection"]["item"]["resource"],
    "repo-b"
  );
  assert!(!visible.to_string().contains("review-a"));
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{}/{}/observations", collection(&issue), ids[0]),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  for query in ["limit=0", "limit=201", "after=invalid", "force=true"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{}?{query}", collection(&issue)), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  *scope.lock().unwrap() = Some(vec![]);
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &collection(&issue), None))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  assert_eq!(reader.reads.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn review_http_writes_require_issue_update_permission_before_reader_use() {
  let reader = Arc::new(ReviewReader::default());
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let router = review_router(reader.clone(), Arc::new(IssueRights(denied.clone()))).await;
  let issue = manual(&router, "restricted-review").await;
  denied.lock().unwrap().insert(ManagementAction::Update);
  let body = serde_json::json!({"selection":selection("repo-a","review-1")}).to_string();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &collection(&issue),
        "forbidden",
        Some("\"1\""),
        &body
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  assert_eq!(reader.reads.load(Ordering::SeqCst), 0);
  let response = router
    .clone()
    .oneshot(empty_request("GET", &collection(&issue), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await["items"], serde_json::json!([]));
}

#[test]
fn review_http_generated_contracts_are_closed_bounded_and_separate_from_lifecycle_authority() {
  let doc = openapi_document();
  let path = "/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews";
  let history = "/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/observations";
  assert_required_header(&doc["paths"][path]["post"], "If-Match", true);
  assert_required_header(&doc["paths"][path]["post"], "Idempotency-Key", true);
  assert_required_header(&doc["paths"][history]["post"], "If-Match", false);
  assert_required_header(&doc["paths"][history]["post"], "Idempotency-Key", true);
  assert!(doc["paths"][path]["post"]["responses"]["200"].is_object());
  for path in [path, history] {
    let parameters = doc["paths"][path]["get"]["parameters"].as_array().unwrap();
    let limit = parameters.iter().find(|p| p["name"] == "limit").unwrap();
    assert_eq!(limit["schema"]["maximum"], 200);
    let cursor = parameters.iter().find(|p| p["name"] == "after").unwrap();
    assert_eq!(cursor["schema"]["maxLength"], MAX_ISSUE_OBSERVATION_CURSOR_BYTES);
  }
  let request = &doc["components"]["schemas"]["AttachIssueCodeReviewRequest"];
  assert_eq!(request["additionalProperties"], false);
  assert_eq!(request["required"], serde_json::json!(["selection"]));
  for forbidden in [
    "credentials",
    "url",
    "actor",
    "provenance",
    "accepted",
    "status_id",
    "workflow_id",
    "execution_state",
  ] {
    assert!(request["properties"].get(forbidden).is_none());
  }
  assert_eq!(
    doc["components"]["schemas"]["IssueCodeReviewAvailability"]["enum"],
    serde_json::json!(["available", "blocked"])
  );
  let parameter = doc["paths"][history]["get"]["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|p| p["name"] == "review_id")
    .unwrap();
  assert_eq!(parameter["schema"]["$ref"], "#/components/schemas/FactoryDigest");
}

#[tokio::test]
async fn review_http_scoped_replay_cannot_retarget_another_issue_or_selection() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let one = manual(&router, "one").await;
  let two = manual(&router, "two").await;
  let body = serde_json::json!({"selection":selection("repo-a","review")}).to_string();
  let mut associations = Vec::new();
  for (issue, key) in [(&one, "same-key"), (&two, "distinct-key")] {
    let response = router
      .clone()
      .oneshot(json_request("POST", &collection(issue), key, Some("\"1\""), &body))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let outcome = document(response).await;
    assert_eq!(outcome["association"]["issue_id"], issue["id"]);
    associations.push(outcome["association"]["id"].clone());
  }
  assert_ne!(associations[0], associations[1]);
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &collection(&two),
        "same-key",
        Some("\"1\""),
        &body
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );

  let changed = serde_json::json!({"selection":selection("repo-a","changed-review")}).to_string();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &collection(&one),
        "same-key",
        Some("\"1\""),
        &changed
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    2,
    "conflicting replay cannot dispatch another read"
  );
}

#[tokio::test]
async fn issue_external_checks_http_pages_retained_heads_without_provider_dispatch_or_acceptance() {
  let reader = Arc::new(ReviewReader::default());
  let router = review_router(reader.clone(), Arc::new(TrustedNetworkManagementPolicy)).await;
  let issue = manual(&router, "ci-task").await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &collection(&issue),
      "ci-review",
      Some("\"1\""),
      &serde_json::json!({"selection":selection("repo-a","ci-review")}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let attached = document(response).await;
  let base = format!(
    "{}/{}",
    collection(&issue),
    attached["association"]["id"].as_str().unwrap()
  );
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{base}/observations"),
      "ci-refresh",
      None,
      "{}",
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  reader.revoked.store(true, Ordering::SeqCst);
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{base}/checks"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let first = document(response).await;
  assert_json_matches_component(&openapi_document(), "IssueExternalChecksPage", &first);
  assert_eq!(first["source"], "external_ci");
  assert_eq!(first["availability"], "blocked");
  assert_eq!(first["coverage"], "partial");
  assert_eq!(first["items"][0]["head"], "head-0");
  assert_eq!(first["items"][0]["checks"]["items"][0]["state"], "skipped");
  assert!(first.get("accepted").is_none());
  let cursor = first["next_cursor"].as_str().unwrap();
  let next = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{base}/checks?after={cursor}"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(next["items"][0]["head"], "head-1");
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    2,
    "GET reads only the existing retained observation journal"
  );
  let summary = document(
    router
      .clone()
      .oneshot(empty_request("GET", &collection(&issue), None))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    summary["items"][0]["observation"].get("check_details").is_none(),
    "summary reads do not eagerly expand CI pages"
  );
}
