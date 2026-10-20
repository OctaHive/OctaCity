//! Public task Source HTTP contracts over the real optional application owner.
use super::*;
use octacity_server_application::*;

#[path = "source_projections.rs"]
mod projections;
#[path = "source_resolution.rs"]
mod resolution;

struct NoSource;
#[async_trait]
impl WorkSourceObservations for NoSource {
  async fn read(
    &self,
    _: &ManagementRequestContext,
    _: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    panic!("retained task reads must not call a provider")
  }
}
async fn source_router() -> axum::Router {
  source_router_with(Arc::new(NoSource), vec![]).await
}
async fn source_router_with(
  reader: Arc<dyn WorkSourceObservations>,
  choices: Vec<InstalledIssueSourceMapping>,
) -> axum::Router {
  source_router_with_policy(reader, choices, Arc::new(TrustedNetworkManagementPolicy)).await
}
async fn source_router_with_policy(
  reader: Arc<dyn WorkSourceObservations>,
  choices: Vec<InstalledIssueSourceMapping>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> axum::Router {
  source_router_with_resolution(reader, choices, policy, None).await
}
async fn source_router_with_resolution(
  reader: Arc<dyn WorkSourceObservations>,
  choices: Vec<InstalledIssueSourceMapping>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  resolution: Option<
    Arc<dyn AuthorizedManagementCommandHandler<ResolveIssueSourceConflictCommand, Error = ApplicationError>>,
  >,
) -> axum::Router {
  source_router_with_projections(reader, choices, policy, resolution, None).await
}
async fn source_router_with_projections(
  reader: Arc<dyn WorkSourceObservations>,
  choices: Vec<InstalledIssueSourceMapping>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  resolution: Option<
    Arc<dyn AuthorizedManagementCommandHandler<ResolveIssueSourceConflictCommand, Error = ApplicationError>>,
  >,
  projections: Option<
    Arc<dyn AuthorizedManagementQueryHandler<ListIssueSourceProjectionsQuery, Error = ApplicationError>>,
  >,
) -> axum::Router {
  let store = manual_issue_store(PROJECT.parse().unwrap()).await;
  let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
  let workflows = Arc::new(IssueWorkflowHandlers::new(store.clone()));
  let sources = Arc::new(
    IssueSourceHandlers::new(store, reader, policy.clone(), Arc::new(SystemFactoryExecutionClock))
      .with_refresh_policy(IssueSourceRefreshPolicy {
        read: IssueSourceReadPolicy {
          polling: SourcePollingPolicy {
            page_items: 100,
            max_pages: 1,
            max_items: 100,
            max_bytes: 65536,
            max_page_bytes: 65536,
            overlap_millis: 1,
            reconciliation_interval_millis: 1000,
            retention_millis: 86400000,
          },
          retry: DurableRetryPolicy::new(3, 1, 100).unwrap(),
          poll_interval_millis: 1,
        },
        claim_lifetime: std::time::Duration::from_secs(30),
      })
      .unwrap()
      .with_source_mappings(choices)
      .unwrap(),
  );
  let peer = Arc::new(RecordingApplication::default());
  let mut source_application = octacity_server_api_rest::v1::IssueSourceManagementApplication::new(Arc::new(
    AuthorizedQueryHandler::new(policy.clone(), sources.clone()),
  ))
  .with_commands(Arc::new(AuthorizedCommandHandler::new(policy.clone(), sources)));
  if let Some(handler) = resolution {
    source_application = source_application.with_resolution(handler);
  }
  if let Some(handler) = projections {
    source_application = source_application.with_projection_queries(handler);
  }
  management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer)
      .with_issues(IssueManagementApplication::new(
        Arc::new(AuthorizedCommandHandler::new(policy.clone(), issues.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy.clone(), issues)),
      ))
      .with_issue_workflows(octacity_server_api_rest::v1::IssueWorkflowManagementApplication::new(
        Arc::new(AuthorizedCommandHandler::new(policy.clone(), workflows.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy.clone(), workflows)),
      ))
      .with_issue_sources(source_application),
  )
}

#[tokio::test]
async fn issue_source_http_mapping_publication_has_separate_immutable_versions_and_review_preconditions() {
  let router = source_router_with(Arc::new(NativeSource), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "mapping-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let issue = &created["resource"];
  let path = format!("{}/{}", root(), issue["id"].as_str().unwrap());
  let attach = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
    "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link"),
      "mapping-link",
      Some("\"1\""),
      &attach.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let workflow = document(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!(
          "/api/v1/projects/{PROJECT}/issue-workflows/{}/versions/{}",
          issue["workflow_id"].as_str().unwrap(),
          issue["workflow_version"]
        ),
        None,
      ))
      .await
      .unwrap(),
  )
  .await;
  let mut body = serde_json::json!({"expected_mapping":null,"definition":{
    "source":profile().source,"reporter":null,"push_enabled":false,
    "fields":{"description":"source_owned_pull","status":"source_owned_pull"},
    "status":{"workflow":{"id":issue["workflow_id"],"version":issue["workflow_version"]},
      "pull":[{"native":{"state":"Waiting for native review","all_of":[],"any_of":[],"none_of":[]},
        "status_id":workflow["definition"]["transitions"][0]["to"]}],"push":null}
  }});
  let request = body.clone();
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link/mappings"),
      "mapping-one",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let first = document(response).await;
  assert_eq!(first["resource"]["version"], 1);
  assert_eq!(first["resource"]["definition"], body["definition"]);
  assert_json_matches_component(&openapi_document(), "IssueSourceMappingMutationResponse", &first);
  let review = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(review["issue_version"], 1);
  assert_eq!(review["mapping_reference"], first["mapping_reference"]);
  assert_eq!(
    review["field_policy"]["source_owned"],
    serde_json::json!(["description", "status"])
  );
  body["expected_mapping"] = first["mapping_reference"].clone();
  body["definition"]["fields"]["description"] = serde_json::json!("unsynchronized");
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link/mappings"),
      "mapping-two",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(document(response).await["resource"]["version"], 2);
  let replay = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link/mappings"),
      "mapping-one",
      Some("\"1\""),
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(replay.status(), StatusCode::OK);
  assert_eq!(document(replay).await["resource"], first["resource"]);
  let historical = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/source-link/mappings/1"), None))
    .await
    .unwrap();
  assert_eq!(historical.status(), StatusCode::OK);
  assert_eq!(document(historical).await, first["resource"]);
  let stale = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link/mappings"),
      "stale-mapping",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(stale.status(), StatusCode::CONFLICT);
  let current = document(router.oneshot(empty_request("GET", &path, None)).await.unwrap()).await;
  assert_eq!(current, *issue);
}

#[tokio::test]
async fn issue_source_http_explicit_refresh_retains_freshness_without_editing_unsynchronized_fields() {
  let router = source_router_with(Arc::new(NativeSource), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "refresh-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let issue = &created["resource"];
  let path = format!("{}/{}", root(), issue["id"].as_str().unwrap());
  let attach = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
    "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link"),
      "refresh-link",
      Some("\"1\""),
      &attach.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  tokio::time::sleep(std::time::Duration::from_millis(2)).await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/refresh"),
      "read-native",
      None,
      &serde_json::json!({"source":profile().source,"continuation":null}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let result = document(response).await;
  assert_eq!(result["issue_id"], issue["id"]);
  assert_eq!(result["disposition"], "unchanged");
  assert_json_matches_component(&openapi_document(), "IssueSourceRefreshOutcome", &result);
  let current = document(router.oneshot(empty_request("GET", &path, None)).await.unwrap()).await;
  assert_eq!(current, *issue);
}

#[derive(Default)]
struct CanonicalVisibility(std::sync::Mutex<Option<String>>);
#[async_trait]
impl ManagementAuthorizationPolicy for CanonicalVisibility {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::View
      && resource.kind() == ManagementResourceKind::Issue
      && resource
        .identity()
        .is_some_and(|id| self.0.lock().unwrap().as_deref() == Some(id.as_str()))
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn issue_source_http_canonical_conflicts_disclose_only_visible_links_and_never_merge_tasks() {
  for hidden in [false, true] {
    let policy = Arc::new(CanonicalVisibility::default());
    let router = source_router_with_policy(Arc::new(NativeSource), vec![profile()], policy.clone()).await;
    let first = document(
      router
        .clone()
        .oneshot(json_request("POST", &root(), "canonical", None, &draft().to_string()))
        .await
        .unwrap(),
    )
    .await;
    let first_id = first["resource"]["id"].as_str().unwrap();
    let body = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
      "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
    let linked = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/{first_id}/source-link", root()),
        "canonical-link",
        Some("\"1\""),
        &body.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(linked.status(), StatusCode::CREATED);
    let original = document(linked).await;
    let second = document(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &root(),
          "separate-manual",
          None,
          &draft().to_string(),
        ))
        .await
        .unwrap(),
    )
    .await;
    let second_id = second["resource"]["id"].as_str().unwrap();
    if hidden {
      *policy.0.lock().unwrap() = Some(first_id.into());
    }
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/{second_id}/source-link", root()),
        "conflicting-link",
        Some("\"1\""),
        &body.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      StatusCode::OK,
      "canonical resolution is an explicit finite association outcome"
    );
    let result = document(response).await;
    assert_eq!(result["kind"], "conflict");
    assert_eq!(
      result["canonical"],
      if hidden {
        serde_json::Value::Null
      } else {
        original["link"].clone()
      }
    );
    assert_json_matches_component(&openapi_document(), "IssueSourceOutcome", &result);
    let current = document(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{}/{second_id}", root()), None))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(current, second["resource"]);
    let current_link = document(
      router
        .oneshot(empty_request(
          "GET",
          &format!("{}/{second_id}/source-link", root()),
          None,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(current_link, serde_json::Value::Null);
  }
}

fn profile() -> InstalledIssueSourceMapping {
  let pin = |name: &str| serde_json::json!({"identity":name,"version":"v1","digest":"01".repeat(32)});
  InstalledIssueSourceMapping {
    project_id: PROJECT.parse().unwrap(),
    source: serde_json::from_value(serde_json::json!({"installation":pin("installed-tracker"),"capability":pin("task-read"),"configuration":pin("read-profile"),"resource":pin("board")})).unwrap(),
    native_resource: serde_json::from_value(serde_json::json!("board-9")).unwrap(),
    readable: serde_json::from_value(serde_json::json!({"states":["Waiting for native review"],"labels":["native-review"]})).unwrap(),
    writer: None,
  }
}
struct ReadableSource;
#[async_trait]
impl WorkSourceObservations for ReadableSource {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    _: &ManagementRequestContext,
    _: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    panic!("choice discovery authorizes a profile without reading provider data")
  }
}
struct NativeSource;
struct AvailableSource(std::sync::atomic::AtomicBool);
#[async_trait]
impl WorkSourceObservations for AvailableSource {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    ReadableSource.authorize(context, project, source).await
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    if !self.0.load(std::sync::atomic::Ordering::SeqCst) {
      return Err(ApplicationError::unavailable());
    }
    NativeSource.read(context, request).await
  }
}
#[async_trait]
impl WorkSourceObservations for NativeSource {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    ReadableSource.authorize(context, project, source).await
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let snapshot: WorkSourceSnapshot = serde_json::from_value(serde_json::json!({
      "item":{"resource":"board-9","item":"task-7"},"display_id":"NATIVE-7","update_identity":"opaque-update",
      "title":"Native requirements","body":"Native body","state":"Waiting for native review","labels":[],"fields":{},"selection_matches":[]
    })).unwrap();
    let selection = IssueSourceSelection {
      project_id: request.project_id,
      source: request.source.clone(),
      item: snapshot.item.clone(),
    };
    let now = SystemFactoryExecutionClock.now()?;
    Ok(WorkSourceObservationPage {
      observations: vec![WorkSourceObservation {
        project_id: request.project_id,
        security_scope: context.security_scope().clone(),
        source: request.source,
        native_correlation: selection.native_correlation(&context.security_scope().as_str().parse().unwrap()),
        plugin: profile().source.capability,
        provider_configuration: serde_json::from_value(serde_json::json!("02".repeat(32))).unwrap(),
        selection_digest: request.scan_id,
        snapshot_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&snapshot).unwrap()),
        snapshot,
        observed_at: now,
        expires_at: Timestamp::from_unix_millis(now.unix_millis() + 60_000).unwrap(),
      }],
      next_cursor: None,
    })
  }
}
#[tokio::test]
async fn issue_source_http_native_candidates_use_an_explicit_bounded_read_without_attaching_or_importing() {
  let router = source_router_with(Arc::new(NativeSource), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "candidate-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let issue = created["resource"]["id"].as_str().unwrap();
  let path = format!("{}/{issue}", root());
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link/candidates"),
      "preview-candidates",
      None,
      &serde_json::json!({"source":profile().source,"continuation":null,"scan_id":null}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let result = document(response).await;
  assert_eq!(
    result["items"],
    serde_json::json!([{"resource":"board-9","item":"task-7"}])
  );
  assert_eq!(result["next_cursor"], serde_json::Value::Null);
  assert!(result["scan_id"].as_str().is_some_and(|v| v.len() == 64));
  assert_json_matches_component(&openapi_document(), "IssueSourceCandidates", &result);
  let current = document(router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap()).await;
  assert_eq!(current, created["resource"]);
  assert_eq!(
    document(
      router
        .oneshot(empty_request("GET", &format!("{path}/source-link"), None))
        .await
        .unwrap()
    )
    .await,
    serde_json::Value::Null
  );
}
#[tokio::test]
async fn issue_source_http_untrusted_continuations_are_bounded_before_any_provider_read() {
  for endpoint in ["source-link", "source-link/candidates"] {
    let router = source_router_with(Arc::new(ReadableSource), vec![profile()]).await;
    let created = document(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &root(),
          "cursor-manual",
          None,
          &draft().to_string(),
        ))
        .await
        .unwrap(),
    )
    .await;
    let issue = created["resource"]["id"].as_str().unwrap();
    let cursor = "я".repeat(MAX_SOURCE_POLL_CURSOR_BYTES / 2 + 1);
    let body = if endpoint == "source-link" {
      serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
        "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":cursor})
    } else {
      serde_json::json!({"source":profile().source,"continuation":cursor})
    };
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{}/{issue}/{endpoint}", root()),
        "bounded-cursor",
        Some("\"1\""),
        &body.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let link = document(
      router
        .oneshot(empty_request("GET", &format!("{}/{issue}/source-link", root()), None))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(link, serde_json::Value::Null);
  }
}
#[tokio::test]
async fn issue_source_http_explicit_attachment_preserves_manual_requirements_and_original_replay() {
  let router = source_router_with(Arc::new(NativeSource), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "attach-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let issue = created["resource"]["id"].as_str().unwrap();
  let path = format!("{}/{issue}", root());
  let body = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
    "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link"),
      "attach-reviewed",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let original = document(response).await;
  assert_eq!(original["kind"], "linked");
  assert_eq!(original["link"]["issue_id"], issue);
  assert_eq!(original["link"]["display_id"], "NATIVE-7");
  assert_json_matches_component(&openapi_document(), "IssueSourceOutcome", &original);
  let current = document(router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap()).await;
  assert_eq!(current, created["resource"]);
  let mut edit = draft();
  edit["description"] = serde_json::json!("Local changes remain local");
  let edited = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &path,
      "edit-after-attach",
      Some("\"1\""),
      &edit.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(edited.status(), StatusCode::OK);
  let replay = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link"),
      "attach-reviewed",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(replay.status(), StatusCode::OK);
  assert_eq!(document(replay).await["link"], original["link"]);
  let review = document(
    router
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(review["issue_version"], 2);
  assert_eq!(review["mapping"], serde_json::Value::Null);
  assert_eq!(review["field_policy"]["source_owned"], serde_json::json!([]));
  assert_json_matches_component(&openapi_document(), "IssueSynchronization", &review);
}
#[tokio::test]
async fn issue_source_http_choices_advertise_only_installed_authorized_fields_and_opaque_native_values() {
  let router = source_router_with(Arc::new(ReadableSource), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "manual-choice",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let issue = created["resource"]["id"].as_str().unwrap();
  let response = router
    .oneshot(empty_request(
      "GET",
      &format!("{}/{issue}/source-choices", root()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = document(response).await;
  assert_eq!(body["items"][0]["native_resource"], "board-9");
  assert_eq!(
    body["items"][0]["pull_fields"],
    serde_json::json!(["title", "description", "status"])
  );
  assert_eq!(body["items"][0]["push_fields"], serde_json::json!([]));
  assert_eq!(
    body["items"][0]["readable"]["states"],
    serde_json::json!(["Waiting for native review"])
  );
  assert_eq!(body["items"][0]["writer"], serde_json::Value::Null);
  assert_json_matches_component(&openapi_document(), "IssueSourceChoices", &body);
}
#[tokio::test]
async fn issue_source_http_manual_synchronization_is_read_only_and_needs_no_provider() {
  let router = source_router().await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "manual", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let issue = created["resource"]["id"].as_str().unwrap();
  let response = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{}/{issue}/synchronization", root()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let body = document(response).await;
  assert_eq!(body["issue_version"], 1);
  assert_eq!(body["link"], serde_json::Value::Null);
  assert_eq!(body["mapping"], serde_json::Value::Null);
  assert_eq!(body["observation"], serde_json::Value::Null);
  assert_eq!(body["field_policy"]["source_owned"], serde_json::json!([]));
  assert_json_matches_component(&openapi_document(), "IssueSynchronization", &body);
  let response = router
    .oneshot(empty_request("GET", &format!("{}/{issue}/source-link", root()), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await, serde_json::Value::Null);
}

#[tokio::test]
async fn issue_source_http_refresh_retry_returns_the_original_receipt_after_provider_outage() {
  let reader = Arc::new(AvailableSource(std::sync::atomic::AtomicBool::new(true)));
  let router = source_router_with(reader.clone(), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "replay-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap());
  let attach = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
    "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link"),
        "replay-link",
        Some("\"1\""),
        &attach.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  tokio::time::sleep(std::time::Duration::from_millis(2)).await;
  let body = serde_json::json!({"source":profile().source,"continuation":null});
  let original = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/refresh"),
      "frozen-read",
      None,
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(original.status(), StatusCode::OK);
  let original = document(original).await;
  let before = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap(),
  )
  .await;
  reader.0.store(false, std::sync::atomic::Ordering::SeqCst);
  let retry = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/refresh"),
      "frozen-read",
      None,
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(retry.status(), StatusCode::OK);
  assert_eq!(document(retry).await, original);
  let changed = serde_json::json!({"source":profile().source,"continuation":"another-page"});
  let conflict = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/refresh"),
      "frozen-read",
      None,
      &changed.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(conflict.status(), StatusCode::CONFLICT);
  let after = document(
    router
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(after, before);
}

#[tokio::test]
async fn issue_source_http_pull_control_reviews_its_own_generation_and_replays_without_provider_access() {
  let reader = Arc::new(AvailableSource(std::sync::atomic::AtomicBool::new(true)));
  let router = source_router_with(reader.clone(), vec![profile()]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "pull-manual", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap());
  let attach = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
    "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link"),
        "pull-link",
        Some("\"1\""),
        &attach.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  reader.0.store(false, std::sync::atomic::Ordering::SeqCst);
  let pause = serde_json::json!({"expected_generation":1,"enabled":false});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/pull"),
      "pause-pull",
      None,
      &pause.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let paused = document(response).await;
  assert_eq!(paused, serde_json::json!({"enabled":false,"generation":2}));
  let resume = serde_json::json!({"expected_generation":2,"enabled":true});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/pull"),
      "resume-pull",
      None,
      &resume.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    document(response).await,
    serde_json::json!({"enabled":true,"generation":3})
  );
  let replay = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/pull"),
      "pause-pull",
      None,
      &pause.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(document(replay).await, paused);
  let conflict = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/pull"),
      "stale-pull",
      None,
      &pause.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(conflict.status(), StatusCode::CONFLICT);
  let review = document(
    router
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(review["observation"]["generation"], 3);
  assert_eq!(review["observation"]["enabled"], true);
  assert_eq!(review["issue_version"], 1);
}

#[tokio::test]
async fn issue_source_http_linked_choices_exclude_profiles_that_would_replace_the_native_association() {
  let mut other = profile();
  other.native_resource = serde_json::from_value(serde_json::json!("another-board")).unwrap();
  other.source.installation = serde_json::from_value(
    serde_json::json!({"identity":"another-installation","version":"v1","digest":"03".repeat(32)}),
  )
  .unwrap();
  let router = source_router_with(Arc::new(NativeSource), vec![profile(), other]).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "scoped-choice-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap());
  let choices = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/source-choices"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(choices["items"].as_array().unwrap().len(), 2);
  let attach = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},
    "expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null},"continuation":null});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link"),
        "scoped-choice-link",
        Some("\"1\""),
        &attach.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  let choices = document(
    router
      .oneshot(empty_request("GET", &format!("{path}/source-choices"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(choices["items"].as_array().unwrap().len(), 1);
  assert_eq!(choices["items"][0]["native_resource"], "board-9");
}

#[test]
fn issue_source_http_schema_publishes_utf8_bounds_for_generated_mapping_forms() {
  let document = openapi_document();
  let schemas = &document["components"]["schemas"];
  assert_eq!(schemas["IssueSourceMappingDefinition"]["x-max-utf8-bytes"], 65_536);
  let state = schemas["WorkSourcePredicate"]["properties"]["state"]["anyOf"]
    .as_array()
    .unwrap()
    .iter()
    .find(|s| s["type"] == "string")
    .unwrap();
  assert_eq!(state["x-max-utf8-bytes"], 128);
  assert_eq!(
    schemas["IssueSourceItem"]["properties"]["item"]["x-max-utf8-bytes"],
    256
  );
  assert_eq!(
    schemas["IssueSourceLink"]["properties"]["display_id"]["x-max-utf8-bytes"],
    256
  );
  assert!(
    !schemas["IssueSourceLink"]["required"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("display_id"))
  );
}

struct LocalIssueEditor;
#[async_trait]
impl ManagementAuthorizationPolicy for LocalIssueEditor {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::Administer && resource.kind() == ManagementResourceKind::Issue {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn issue_source_http_local_editing_does_not_grant_source_administration_or_accept_credentials() {
  let router = source_router_with_policy(Arc::new(ReadableSource), vec![profile()], Arc::new(LocalIssueEditor)).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "ordinary-editor",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap());
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/source-choices"), None))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  let preview = serde_json::json!({"source":profile().source});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link/candidates"),
        "denied",
        None,
        &preview.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  for field in ["credential", "endpoint", "actor", "run_state"] {
    let mut body = preview.clone();
    body[field] = serde_json::json!("untrusted");
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &format!("{path}/source-link/candidates"),
          "closed",
          None,
          &body.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
}

#[tokio::test]
async fn issue_source_http_attachment_replay_survives_provider_outage_and_conflicts_before_io() {
  let reader = Arc::new(AvailableSource(std::sync::atomic::AtomicBool::new(true)));
  let router = source_router_with(reader.clone(), vec![profile()]).await;
  let issue = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "replay-attach-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), issue["resource"]["id"].as_str().unwrap());
  let body = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},"expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null}});
  let request = || {
    json_request(
      "POST",
      &format!("{path}/source-link"),
      "original-attachment",
      Some("\"1\""),
      &body.to_string(),
    )
  };
  let first = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(first.status(), StatusCode::CREATED);
  let first = document(first).await;
  reader.0.store(false, std::sync::atomic::Ordering::SeqCst);
  let replay = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(replay.status(), StatusCode::OK);
  let replay = document(replay).await;
  assert_eq!(replay["link"], first["link"]);
  assert_eq!(replay["disposition"], "replayed");
  let mut changed = body.clone();
  changed["item"]["item"] = serde_json::json!("other-task");
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link"),
      "original-attachment",
      Some("\"1\""),
      &changed.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[derive(Default)]
struct PinnedPageSource(std::sync::Mutex<Option<FactoryDigest>>);
#[async_trait]
impl WorkSourceObservations for PinnedPageSource {
  async fn authorize(
    &self,
    c: &ManagementRequestContext,
    p: ProjectId,
    s: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    ReadableSource.authorize(c, p, s).await
  }
  async fn read(
    &self,
    c: &ManagementRequestContext,
    r: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let second = r.continuation.is_some();
    {
      let mut scan = self.0.lock().unwrap();
      if second {
        if *scan != Some(r.scan_id) {
          return Err(ApplicationError::invalid());
        }
      } else {
        *scan = Some(r.scan_id);
      }
    }
    let mut page = NativeSource.read(c, r).await?;
    if !second {
      page.observations.clear();
      page.next_cursor = Some("opaque-second-page".into());
    }
    Ok(page)
  }
}
#[tokio::test]
async fn issue_source_http_attachment_preserves_the_preview_scan_for_an_opaque_second_page() {
  let router = source_router_with(Arc::new(PinnedPageSource::default()), vec![profile()]).await;
  let issue = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "paged-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), issue["resource"]["id"].as_str().unwrap());
  let first = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link/candidates"),
        "unused",
        None,
        &serde_json::json!({"source":profile().source}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let second = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link/candidates"),
        "unused",
        None,
        &serde_json::json!({"source":profile().source,"scan_id":first["scan_id"],"continuation":first["next_cursor"]})
          .to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(second["items"][0]["item"], "task-7");
  let body = serde_json::json!({"source":profile().source,"scan_id":second["scan_id"],"continuation":first["next_cursor"],"item":second["items"][0],"expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null}});
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("{path}/source-link"),
      "paged-attach",
      Some("\"1\""),
      &body.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn issue_source_http_refresh_resumes_the_original_bounded_pass_beyond_the_first_page() {
  let router = source_router_with(Arc::new(PinnedPageSource::default()), vec![profile()]).await;
  let issue = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "refresh-paged-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), issue["resource"]["id"].as_str().unwrap());
  let page = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link/candidates"),
        "unused",
        None,
        &serde_json::json!({"source":profile().source}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let attach = serde_json::json!({"source":profile().source,"scan_id":page["scan_id"],"continuation":page["next_cursor"],"item":{"resource":"board-9","item":"task-7"},"expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null}});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link"),
        "refresh-paged-link",
        Some("\"1\""),
        &attach.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  let request = || {
    json_request(
      "POST",
      &format!("{path}/synchronization/refresh"),
      "frozen-paged-refresh",
      None,
      &serde_json::json!({"source":profile().source}).to_string(),
    )
  };
  let first = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(first.status(), StatusCode::OK);
  assert_eq!(document(first).await["disposition"], "scanning");
  tokio::time::sleep(std::time::Duration::from_millis(2)).await;
  let second = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(second.status(), StatusCode::OK);
  let second = document(second).await;
  assert_eq!(second["disposition"], "unchanged");
  assert_eq!(document(router.oneshot(request()).await.unwrap()).await, second);
}

#[tokio::test]
async fn issue_source_http_paused_pull_rejects_a_new_refresh_before_provider_io() {
  let reader = Arc::new(AvailableSource(std::sync::atomic::AtomicBool::new(true)));
  let router = source_router_with(reader.clone(), vec![profile()]).await;
  let issue = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "paused-read-manual",
        None,
        &draft().to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), issue["resource"]["id"].as_str().unwrap());
  let attach = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},"expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null}});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/source-link"),
        "paused-read-attach",
        Some("\"1\""),
        &attach.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CREATED
  );
  let review = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/synchronization"), None))
      .await
      .unwrap(),
  )
  .await;
  let pause = serde_json::json!({"expected_generation":review["observation"]["generation"],"enabled":false});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/synchronization/pull"),
        "pause-read",
        None,
        &pause.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  reader.0.store(false, std::sync::atomic::Ordering::SeqCst);
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("{path}/synchronization/refresh"),
      "paused-new-read",
      None,
      &serde_json::json!({"source":profile().source}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

struct ConcurrentAttachmentSource(tokio::sync::Barrier);
#[async_trait]
impl WorkSourceObservations for ConcurrentAttachmentSource {
  async fn authorize(
    &self,
    c: &ManagementRequestContext,
    p: ProjectId,
    s: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    NativeSource.authorize(c, p, s).await
  }
  async fn read(
    &self,
    c: &ManagementRequestContext,
    r: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    self.0.wait().await;
    NativeSource.read(c, r).await
  }
}
#[tokio::test]
async fn issue_source_http_concurrent_attachment_rechecks_replay_inside_the_shared_publisher() {
  let router = source_router_with(
    Arc::new(ConcurrentAttachmentSource(tokio::sync::Barrier::new(2))),
    vec![profile()],
  )
  .await;
  let issue = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "race-manual", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), issue["resource"]["id"].as_str().unwrap());
  let body = serde_json::json!({"source":profile().source,"item":{"resource":"board-9","item":"task-7"},"expected_field_policy":{"source_owned":[],"status_mapping_workflow":null,"push":null}});
  let request = || {
    json_request(
      "POST",
      &format!("{path}/source-link"),
      "racing-attachment",
      Some("\"1\""),
      &body.to_string(),
    )
  };
  let (a, b) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
    tokio::join!(router.clone().oneshot(request()), router.clone().oneshot(request()))
  })
  .await
  .unwrap();
  let (a, b) = (a.unwrap(), b.unwrap());
  assert!(matches!(
    (a.status(), b.status()),
    (StatusCode::CREATED, StatusCode::OK) | (StatusCode::OK, StatusCode::CREATED)
  ));
  assert_eq!(document(a).await["link"], document(b).await["link"]);
}
