//! Real Issue/Factory owners behind HTTP; only persistence, bytes and VCS use existing memory ports.
use super::*;
use octacity_server_application::testing::{
  InMemoryFactoryConfigurationStore, MutationEvidenceProbe, issue_execution_factory,
};
use octacity_server_application::*;

struct Fixture {
  router: axum::Router,
  issues: Arc<InMemoryProjectStore>,
  factory: Arc<InMemoryFactoryConfigurationStore>,
  body: serde_json::Value,
  path: String,
}
const COMMENT: &str = "60000000-0000-4000-8000-000000000077";
const BODY_DIGEST: &str = "35bf6c2f4f5a2cdfa1f68b067d43c3a1f98dc1e3835c1667a0d2d33ace3fb39d";

#[test]
fn issue_snapshot_openapi_accepts_canonical_empty_dependencies() {
  let schema = openapi_document();
  let dependency = serde_json::json!({
    "work_id":"60000000-0000-4000-8000-000000000078",
    "run_id":"60000000-0000-4000-8000-000000000079",
    "work_digest":BODY_DIGEST,
    "terminal":"accepted"
  });
  // No dependency claims is a valid selected value, alongside nonempty
  // dependencies and material text. The alias contract supplies its meaning.
  for value in [
    serde_json::json!([]),
    serde_json::json!([dependency.clone()]),
    serde_json::json!(["notes"]),
  ] {
    assert_json_matches_component(&schema, "IssueSnapshotValue", &value);
  }
  assert!(
    std::panic::catch_unwind(|| {
      assert_json_matches_component(&schema, "IssueSnapshotValue", &serde_json::json!([dependency, "notes"]));
    })
    .is_err(),
    "mixed dependency references and material text have no valid value type"
  );
}

struct SupplementaryBody;
#[async_trait]
impl ManagementQueryUseCase<GetIssueCommentBodyQuery> for SupplementaryBody {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    q: GetIssueCommentBodyQuery,
  ) -> Result<IssueCommentContent, ApplicationError> {
    assert_eq!(q.comment_id.to_string(), COMMENT);
    Ok(IssueCommentContent { comment: serde_json::from_value(serde_json::json!({"id":COMMENT,"project_id":q.project_id,"issue_id":q.issue_id,
      "sequence":1,"parent_id":null,"actor":{"kind":"unauthenticated_management","identity":null},"created_at":1,"body_digest":BODY_DIGEST,"body_bytes":27})).unwrap(),
      body: IssueCommentBody::try_new("Approved supplementary text").unwrap() })
  }
}
#[derive(Default)]
struct SnapshotRights {
  deny_body: std::sync::atomic::AtomicBool,
  deny_execute: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl ManagementAuthorizationPolicy for SnapshotRights {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if (resource.kind() == ManagementResourceKind::IssueDiscussion && self.deny_body.load(Ordering::SeqCst))
      || (action == ManagementAction::Execute && self.deny_execute.load(Ordering::SeqCst))
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
fn input() -> serde_json::Value {
  serde_json::json!({"version":1,"definition_version":0,"fields":{
    "title":{"source":{"kind":"title"},"required":true},
    "size":{"source":{"kind":"size"},"required":true},
    "priority":{"source":{"kind":"priority"},"required":true},
    "component":{"source":{"kind":"component"},"required":false}},
    "admission":{"priority":{"field":"priority","values":{"high":80,"low":20}},
      "risk":{"field":"size","values":{"small":"low","large":"high"}}}})
}
fn shape() -> serde_json::Value {
  serde_json::json!({"type":"object","fields":{
    "title":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":256}},
    "size":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":256}},
    "priority":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":256}},
    "component":{"required":false,"value":{"type":"string","min_bytes":1,"max_bytes":256}}}})
}
async fn fixture(input: serde_json::Value, shape: serde_json::Value) -> Fixture {
  fixture_with_policy(input, shape, Arc::new(TrustedNetworkManagementPolicy)).await
}
async fn fixture_with_policy(
  input: serde_json::Value,
  shape: serde_json::Value,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> Fixture {
  let issues = manual_issue_store(PROJECT.parse().unwrap()).await;
  let (factory, target) = issue_execution_factory(PROJECT.parse().unwrap(), input, shape);
  let materials = Arc::new(IssueSnapshotMaterialReaders::new(Arc::new(
    AuthorizedQueryHandler::new(policy.clone(), Arc::new(SupplementaryBody)),
  )));
  let owner = Arc::new(
    IssueExecutionHandlers::new(issues.clone(), factory.clone(), policy.clone()).with_materials(materials.clone()),
  );
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), owner.clone()));
  let publisher = Arc::new(
    IssueExecutionPublisher::new(
      issues.clone(),
      factory.clone(),
      Arc::new(testing::InMemoryArtifactBytes::default()),
      Arc::new(ExactRevisionResolver),
      policy.clone(),
      Arc::new(SystemFactoryExecutionClock),
    )
    .with_materials(materials),
  );
  let manual = Arc::new(IssueHandlers::new(issues.clone(), policy.clone()));
  let peer = Arc::new(RecordingApplication::default());
  let router = management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer)
      .with_issues(IssueManagementApplication::new(
        Arc::new(AuthorizedCommandHandler::new(policy.clone(), manual.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy.clone(), manual)),
      ))
      .with_issue_executions(
        IssueExecutionManagementApplication::new(queries.clone())
          .with_snapshot_queries(queries)
          .with_publication(
            Arc::new(AuthorizedCommandHandler::new(policy.clone(), owner)),
            Arc::new(AuthorizedCommandHandler::new(policy, publisher)),
            std::time::Duration::from_secs(30),
          ),
      ),
  );
  let request = serde_json::json!({"title":"Manual feature","description":"Original requirements","acceptance":"Expected behavior","priority":"high",
    "planning":{"definition_version":0,"size":"small","assignee":null,"labels":[],"values":{},"due_date":null,"parent_id":null}});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &root(),
      "snapshot-issue",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let created = document(response).await;
  assert_eq!(status, StatusCode::CREATED, "{created}");
  Fixture {
    router,
    issues,
    factory,
    body: serde_json::json!({"target":target}),
    path: format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap()),
  }
}
async fn read(router: &axum::Router, uri: &str) -> serde_json::Value {
  let response = router.clone().oneshot(empty_request("GET", uri, None)).await.unwrap();
  let status = response.status();
  assert_eq!(response.headers()["cache-control"], "no-store");
  let value = document(response).await;
  assert_eq!(status, StatusCode::OK, "{value}");
  value
}
#[tokio::test]
async fn issue_snapshot_http_review_detail_and_retry_retain_original_selected_values_without_a_predecessor() {
  let f = fixture(input(), shape()).await;
  let before = (
    f.issues.mutation_evidence_counts().await,
    f.factory.mutation_evidence_counts().await,
  );
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/executions/review", f.path),
      "unused-read",
      Some("\"1\""),
      &f.body.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let review = document(response).await;
  assert_eq!(status, StatusCode::OK, "{review}");
  let schema = openapi_document();
  assert_component_exists(&schema, "IssueExecutionSnapshot");
  assert_json_matches_component(&schema, "IssueExecutionReview", &review);
  assert_eq!(
    review["snapshot"]["values"],
    serde_json::json!({"title":"Manual feature","size":"small","priority":"high"})
  );
  assert_eq!(review["snapshot"]["omissions"]["component"], "missing");
  assert_eq!(review["snapshot"]["admission_classification"]["priority"], 80);
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before
  );
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/executions", f.path),
      "snapshot-start",
      Some("\"1\""),
      &f.body.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let receipt = document(response).await;
  assert_eq!(status, StatusCode::CREATED, "{receipt}");
  assert_json_matches_component(&schema, "IssueExecutionMutationResponse", &receipt);
  assert_eq!(receipt["resource"]["snapshot_digest"], review["snapshot_digest"]);
  let id = receipt["resource"]["id"].as_str().unwrap();
  let detail = read(&f.router, &format!("{}/executions/{id}/snapshot", f.path)).await;
  assert_eq!(detail["availability"], "available");
  assert_json_matches_component(&schema, "IssueExecutionSnapshotObservation", &detail);
  assert_eq!(detail["snapshot"], review["snapshot"]);
  assert_eq!(detail["subject"]["value"]["base_revision"], "original-base");
  let current = read(&f.router, &f.path).await;
  let mut plan = current["planning"].clone();
  plan["size"] = serde_json::json!("large");
  assert_eq!(
    f.router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{}/planning", f.path),
        "later",
        Some("\"1\""),
        &plan.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let before = (
    f.issues.mutation_evidence_counts().await,
    f.factory.mutation_evidence_counts().await,
  );
  assert_eq!(
    read(&f.router, &format!("{}/executions/{id}/snapshot", f.path)).await,
    detail
  );
  let replay = document(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/executions", f.path),
        "snapshot-start",
        Some("\"1\""),
        &f.body.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["resource"], receipt["resource"]);
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before
  );
  let mut changed = f.body.clone();
  changed["classification"] = serde_json::json!({"priority":1,"risk":"high","metadata":{"accepted":"true"}});
  assert_eq!(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/executions", f.path),
        "snapshot-start",
        Some("\"1\""),
        &changed.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  for suffix in ["/executions/review", "/executions"] {
    assert_eq!(
      f.router
        .clone()
        .oneshot(json_request(
          "POST",
          &format!("{}{suffix}", f.path),
          "stale-new",
          Some("\"1\""),
          &f.body.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::CONFLICT
    );
    let mut forged = f.body.clone();
    forged["snapshot"] = review["snapshot"].clone();
    assert_eq!(
      f.router
        .clone()
        .oneshot(json_request(
          "POST",
          &format!("{}{suffix}", f.path),
          "forged",
          Some("\"2\""),
          &forged.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  let history = read(&f.router, &format!("{}/executions", f.path)).await;
  assert_eq!(history["items"][0]["snapshot_digest"], review["snapshot_digest"]);
  assert!(
    history["items"][0].get("snapshot").is_none(),
    "metadata history contains no selected text"
  );
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before
  );
}

#[tokio::test]
async fn issue_snapshot_http_blockers_name_missing_stale_incompatible_and_required_evidence_without_reservation() {
  for (case, field, code) in [
    (
      "missing_acceptance",
      "/issue_snapshot/fields/acceptance",
      "issue_snapshot_missing_value",
    ),
    ("missing", "/issue_snapshot/fields/size", "issue_snapshot_missing_value"),
    (
      "stale",
      "/issue_snapshot/fields/priority",
      "issue_snapshot_inapplicable",
    ),
    ("incompatible", "/issue_snapshot/fields/size", "invalid_contract"),
    (
      "evidence",
      "/issue_snapshot/evidence/proof",
      "issue_snapshot_evidence_unavailable",
    ),
  ] {
    let mut input = input();
    let mut shape = shape();
    if case == "missing_acceptance" {
      input["fields"]["acceptance"] = serde_json::json!({"source":{"kind":"acceptance"},"required":true});
      shape["fields"]["acceptance"] =
        serde_json::json!({"required":true,"value":{"type":"string","min_bytes":0,"max_bytes":1048576}});
    }
    if case == "evidence" {
      input["evidence"] = serde_json::json!({"proof":{"field":"size","required":true,"maximum":1}});
      shape["fields"]["proof"] = serde_json::json!({"required":true,"value":{"type":"array","min_items":1,"max_items":1,"item":{"type":"string","min_bytes":64,"max_bytes":64}}});
    }
    if case == "incompatible" {
      shape["fields"]["size"]["value"] = serde_json::json!({"type":"integer","minimum":0,"maximum":10});
    }
    let f = fixture(input, shape).await;
    let mut version = 1;
    if case == "missing" {
      let current = read(&f.router, &f.path).await;
      let mut plan = current["planning"].clone();
      plan.as_object_mut().unwrap().remove("size");
      assert_eq!(
        f.router
          .clone()
          .oneshot(json_request(
            "PATCH",
            &format!("{}/planning", f.path),
            "remove-size",
            Some("\"1\""),
            &plan.to_string()
          ))
          .await
          .unwrap()
          .status(),
        StatusCode::OK
      );
      version = 2;
    }
    if case == "stale" || case == "missing_acceptance" {
      let current = read(&f.router, &f.path).await;
      let mut content = current["content"].clone();
      content["acceptance"] = if case == "missing_acceptance" {
        serde_json::Value::Null
      } else {
        serde_json::json!("Changed behavior")
      };
      assert_eq!(
        f.router
          .clone()
          .oneshot(json_request(
            "PATCH",
            &f.path,
            "changed-acceptance",
            Some("\"1\""),
            &content.to_string()
          ))
          .await
          .unwrap()
          .status(),
        StatusCode::OK
      );
      version = 2;
    }
    let before = (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await,
    );
    for suffix in ["/executions/review", "/executions"] {
      let response = f
        .router
        .clone()
        .oneshot(json_request(
          "POST",
          &format!("{}{suffix}", f.path),
          case,
          Some(&format!("\"{version}\"")),
          &f.body.to_string(),
        ))
        .await
        .unwrap();
      let status = response.status();
      let body = document(response).await;
      assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {body}");
      assert_eq!(
        body["diagnostics"]["errors"][0],
        serde_json::json!({"field":field,"code":code}),
        "{case}: {body}"
      );
    }
    assert_eq!(
      (
        f.issues.mutation_evidence_counts().await,
        f.factory.mutation_evidence_counts().await
      ),
      before,
      "{case}"
    );
  }
}

#[tokio::test]
async fn issue_snapshot_http_detail_rechecks_selected_body_access_without_resuming_or_replacing_input() {
  let policy = Arc::new(SnapshotRights::default());
  let mut input = input();
  let mut shape = shape();
  input["materials"] =
    serde_json::json!({"notes":{"source":{"kind":"selected","max_entries":1},"required":true,"max_bytes":256}});
  shape["fields"]["notes"] = serde_json::json!({"required":true,"value":{"type":"array","min_items":1,"max_items":1,"item":{"type":"string","min_bytes":0,"max_bytes":256}}});
  let f = fixture_with_policy(input, shape, policy.clone()).await;
  let issue_id = f.path.rsplit('/').next().unwrap().parse().unwrap();
  // Trusted retained-selection fixture, not a new HTTP setter or automatic ingestion.
  let mut issue = f.issues.issue(PROJECT.parse().unwrap(), issue_id, None).await.unwrap();
  issue.version = issue.version.next().unwrap();
  issue.materials=serde_json::from_value(serde_json::json!([{"reference":{"kind":"comment","project_id":PROJECT,"issue_id":issue_id,
    "comment_id":COMMENT,"body_digest":BODY_DIGEST},"selected_by":{"kind":"unauthenticated_management","identity":null},"selected_at":1}])).unwrap();
  f.issues.seed_retained_issue_revision(issue).unwrap();
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/executions", f.path),
      "with-notes",
      Some("\"2\""),
      &f.body.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let receipt = document(response).await;
  assert_eq!(status, StatusCode::CREATED, "{receipt}");
  let id = receipt["resource"]["id"].as_str().unwrap();
  let uri = format!("{}/executions/{id}/snapshot", f.path);
  let original = read(&f.router, &uri).await;
  assert_eq!(
    original["snapshot"]["values"]["notes"],
    serde_json::json!(["Approved supplementary text"])
  );
  assert_json_matches_component(&openapi_document(), "IssueExecutionSnapshotObservation", &original);
  let before = (
    f.issues.mutation_evidence_counts().await,
    f.factory.mutation_evidence_counts().await,
  );
  policy.deny_body.store(true, Ordering::SeqCst);
  let unavailable = read(&f.router, &uri).await;
  assert_eq!(
    unavailable,
    serde_json::json!({"availability":"unavailable","digest":receipt["resource"]["snapshot_digest"]})
  );
  assert_json_matches_component(&openapi_document(), "IssueExecutionSnapshotObservation", &unavailable);
  policy.deny_body.store(false, Ordering::SeqCst);
  policy.deny_execute.store(true, Ordering::SeqCst);
  assert_eq!(
    read(&f.router, &uri).await,
    original,
    "read authority grants no execution"
  );
  assert_eq!(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/executions/review", f.path),
        "unused",
        Some("\"2\""),
        &f.body.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before
  );
}

#[tokio::test]
async fn issue_snapshot_http_legacy_target_still_requires_classification_before_any_reservation() {
  let f = fixture(serde_json::Value::Null, serde_json::Value::Null).await;
  let before = (
    f.issues.mutation_evidence_counts().await,
    f.factory.mutation_evidence_counts().await,
  );
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/executions", f.path),
      "legacy-missing",
      Some("\"1\""),
      &f.body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before,
    "legacy rejection must precede request/audit/replay mutation"
  );
  let mut body = f.body.clone();
  body["classification"] = serde_json::json!({"priority":20,"risk":"medium","metadata":{}});
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/executions", f.path),
      "legacy-explicit",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let receipt = document(response).await;
  assert_eq!(status, StatusCode::CREATED, "{receipt}");
  assert!(receipt["resource"].get("snapshot_digest").is_none());
  assert_eq!(receipt["resource"]["intent"]["classification"], body["classification"]);
  let id = receipt["resource"]["id"].as_str().unwrap();
  assert_eq!(
    read(&f.router, &format!("{}/executions/{id}/snapshot", f.path)).await,
    serde_json::json!({"availability":"not_selected"})
  );
  let replay = document(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/executions", f.path),
        "legacy-explicit",
        Some("\"1\""),
        &body.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["resource"], receipt["resource"]);
}

#[tokio::test]
async fn issue_snapshot_http_review_digest_fences_optional_availability_without_an_issue_edit() {
  let policy = Arc::new(SnapshotRights::default());
  let mut contract = input();
  let mut schema = shape();
  contract["materials"] =
    serde_json::json!({"notes":{"source":{"kind":"selected","max_entries":1},"required":false,"max_bytes":256}});
  schema["fields"]["notes"] = serde_json::json!({"required":false,"value":{"type":"array","min_items":0,"max_items":1,"item":{"type":"string","min_bytes":0,"max_bytes":256}}});
  let f = fixture_with_policy(contract, schema, policy.clone()).await;
  let issue_id = f.path.rsplit('/').next().unwrap().parse().unwrap();
  let mut issue = f.issues.issue(PROJECT.parse().unwrap(), issue_id, None).await.unwrap();
  issue.version = issue.version.next().unwrap();
  issue.materials = serde_json::from_value(serde_json::json!([{"reference":{"kind":"comment","project_id":PROJECT,"issue_id":issue_id,
    "comment_id":COMMENT,"body_digest":BODY_DIGEST},"selected_by":{"kind":"unauthenticated_management","identity":null},"selected_at":1}])).unwrap();
  f.issues.seed_retained_issue_revision(issue).unwrap();
  let review_path = format!("{}/executions/review", f.path);
  let start_path = format!("{}/executions", f.path);
  let reviewed = document(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &review_path,
        "unused-review",
        Some("\"2\""),
        &f.body.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(
    reviewed["snapshot"]["values"]["notes"],
    serde_json::json!(["Approved supplementary text"])
  );
  let mut body = f.body.clone();
  body["expected_snapshot_digest"] = reviewed["snapshot_digest"].clone();
  policy.deny_body.store(true, Ordering::SeqCst);
  let before = (
    f.issues.mutation_evidence_counts().await,
    f.factory.mutation_evidence_counts().await,
  );
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &start_path,
      "reviewed-start",
      Some("\"2\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let rejected = document(response).await;
  assert_eq!(
    status,
    StatusCode::CONFLICT,
    "reviewed text cannot become an unreviewed omission: {rejected}"
  );
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before,
    "a failed preview precondition allocates no execution, replay or audit mutation"
  );
  let fresh = document(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &review_path,
        "unused-review",
        Some("\"2\""),
        &f.body.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(fresh["snapshot"]["omissions"]["notes"], "unavailable");
  body["expected_snapshot_digest"] = fresh["snapshot_digest"].clone();
  let response = f
    .router
    .clone()
    .oneshot(json_request(
      "POST",
      &start_path,
      "reviewed-start",
      Some("\"2\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  let status = response.status();
  let receipt = document(response).await;
  assert_eq!(status, StatusCode::CREATED, "{receipt}");
  assert_eq!(receipt["resource"]["snapshot_digest"], fresh["snapshot_digest"]);
  assert_json_matches_component(&openapi_document(), "IssueExecutionMutationResponse", &receipt);
  policy.deny_body.store(false, Ordering::SeqCst);
  let replay = document(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &start_path,
        "reviewed-start",
        Some("\"2\""),
        &body.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(
    replay["resource"], receipt["resource"],
    "retry keeps the original reviewed omission after body access returns"
  );
  let before_new = (
    f.issues.mutation_evidence_counts().await,
    f.factory.mutation_evidence_counts().await,
  );
  assert_eq!(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &start_path,
        "stale-omission",
        Some("\"2\""),
        &body.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT,
    "newly available optional text also requires a new preview"
  );
  assert_eq!(
    (
      f.issues.mutation_evidence_counts().await,
      f.factory.mutation_evidence_counts().await
    ),
    before_new
  );
  body["expected_snapshot_digest"] = reviewed["snapshot_digest"].clone();
  assert_eq!(
    f.router
      .clone()
      .oneshot(json_request(
        "POST",
        &start_path,
        "reviewed-start",
        Some("\"2\""),
        &body.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT,
    "changing the precondition cannot reuse another confirmed request"
  );
}
