//! Public role/profile HTTP contracts reuse the audited application and shared store.
use super::*;
use octacity_server_api_rest::v1::FactoryRoleManagementApplication;
use octacity_server_application::*;
use std::sync::atomic::AtomicBool;

#[tokio::test]
async fn factory_roles_http_publishes_complete_definitions_and_replays_the_original_receipt() {
  let (router, project, mut intent) = role_router();
  intent["profiles"][0]["definition"]["thinking_parameter"] = serde_json::json!("reasoning_effort");
  let path = format!("/api/v1/projects/{project}/factory-role-publications");
  assert_json_matches_component(&openapi_document(), "PublishFactoryRoleProfilesRequest", &intent);
  let body = intent.to_string();
  let first = router
    .clone()
    .oneshot(json_request("POST", &path, "role-copy", None, &body))
    .await
    .unwrap();
  assert_eq!(first.status(), StatusCode::OK);
  assert_eq!(first.headers()["cache-control"], "no-store");
  let first: serde_json::Value =
    serde_json::from_slice(&to_bytes(first.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_json_matches_component(&openapi_document(), "FactoryRoleProfilesPublication", &first);
  assert_eq!(first["disposition"], "applied");
  assert_eq!(first["profiles"].as_array().unwrap().len(), 2);
  assert!(first["role"]["definition"]["instructions"].is_string());
  let detail_path = format!(
    "/api/v1/projects/{project}/factory-roles/{}",
    first["role"]["reference"]["id"].as_str().unwrap()
  );
  let detail = router
    .clone()
    .oneshot(empty_request("GET", &detail_path, None))
    .await
    .unwrap();
  assert_eq!(detail.status(), StatusCode::OK);
  assert_eq!(detail.headers()["cache-control"], "no-store");
  let detail: serde_json::Value =
    serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(detail, first["role"]);
  let replay = router
    .oneshot(json_request("POST", &path, "role-copy", None, &body))
    .await
    .unwrap();
  assert_eq!(replay.status(), StatusCode::OK);
  let replay: serde_json::Value =
    serde_json::from_slice(&to_bytes(replay.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["role"], first["role"]);
  assert_eq!(replay["profiles"], first["profiles"]);
  assert_eq!(replay["published_at"], first["published_at"]);
}

#[tokio::test]
async fn factory_role_shared_skills_publish_and_read_without_profile_copies() {
  let (router, project, mut intent) = role_router();
  let skills = intent["profiles"][0]["definition"]["skills"].clone();
  intent["role"]["definition"]["skills"] = skills.clone();
  intent["profiles"] = serde_json::json!([]);
  assert_json_matches_component(&openapi_document(), "PublishFactoryRoleProfilesRequest", &intent);
  let result = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-role-publications"),
      "shared-role-skills",
      None,
      &intent.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(result.status(), StatusCode::OK);
  let published = json_response(result).await;
  assert_eq!(published["role"]["definition"]["skills"], skills);
  assert_eq!(published["profiles"], serde_json::json!([]));
  let result = router
    .oneshot(empty_request(
      "GET",
      &format!(
        "/api/v1/projects/{project}/factory-roles/{}",
        published["role"]["reference"]["id"].as_str().unwrap()
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(json_response(result).await, published["role"]);
}

fn role_router() -> (axum::Router, ProjectId, serde_json::Value) {
  role_router_with_policy(Arc::new(TrustedNetworkManagementPolicy))
}

#[tokio::test]
async fn factory_agent_avatar_publication_decodes_the_complete_raster() {
  let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgaPj/HwAEggJ/59habAAAAABJRU5ErkJggg==";
  for (avatar, expected) in [
    (png.to_string(), StatusCode::OK),
    (
      format!("data:image/png;base64,{}", &png[22..66]),
      StatusCode::BAD_REQUEST,
    ),
  ] {
    let (router, project, mut intent) = role_router();
    intent["profiles"] = serde_json::json!([]);
    intent["role"]["definition"]["avatar"] = serde_json::json!(avatar);
    assert_json_matches_component(&openapi_document(), "PublishFactoryRoleProfilesRequest", &intent);
    let response = router
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/projects/{project}/factory-role-publications"),
        "uploaded-avatar",
        None,
        &intent.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), expected);
    if expected == StatusCode::OK {
      let result = json_response(response).await;
      assert_eq!(result["role"]["definition"]["avatar"], png);
    }
  }
}
fn role_router_with_policy(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> (axum::Router, ProjectId, serde_json::Value) {
  let project: ProjectId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let handlers = Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy.clone()));
  let peer = Arc::new(RecordingApplication::default());
  let app =
    recording_management_application(peer.clone(), peer).with_factory_roles(FactoryRoleManagementApplication::new(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), handlers.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, handlers)),
    ));
  let router = management_router_with_application(|| true, app);
  let mut intent = serde_json::to_value(testing::role_profile_contract_intent(project)).unwrap();
  intent.as_object_mut().unwrap().remove("project_id");
  (router, project, intent)
}
async fn json_response(response: axum::response::Response) -> serde_json::Value {
  serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}
#[tokio::test]
async fn factory_roles_http_pages_are_safe_and_exact_replacements_keep_old_versions() {
  let (router, project, intent) = role_router();
  let publication = format!("/api/v1/projects/{project}/factory-role-publications");
  let response = router
    .clone()
    .oneshot(json_request("POST", &publication, "initial", None, &intent.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let original = json_response(response).await;
  let role = original["role"]["reference"]["id"].as_str().unwrap();
  let role_page = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-roles?limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(role_page.status(), StatusCode::OK);
  let role_page = json_response(role_page).await;
  assert_eq!(role_page["total"], 1);
  assert!(!role_page.to_string().contains("instructions"));
  let profiles = format!("/api/v1/projects/{project}/factory-roles/{role}/profiles?limit=1");
  let page = router
    .clone()
    .oneshot(empty_request("GET", &profiles, None))
    .await
    .unwrap();
  assert_eq!(page.status(), StatusCode::OK);
  let page = json_response(page).await;
  assert_eq!(page["total"], 2);
  assert_eq!(page["items"].as_array().unwrap().len(), 1);
  for private in ["instructions", "environment", "cli_arguments", "projects/model"] {
    assert!(!page.to_string().contains(private));
  }
  let next = page["next_cursor"].as_str().unwrap();
  let next_page = router
    .clone()
    .oneshot(empty_request("GET", &format!("{profiles}&after={next}"), None))
    .await
    .unwrap();
  assert_eq!(next_page.status(), StatusCode::OK);
  let next_page = json_response(next_page).await;
  assert_eq!(next_page["items"].as_array().unwrap().len(), 1);
  assert_ne!(page["items"][0]["reference"], next_page["items"][0]["reference"]);
  let profile = original["profiles"][0]["reference"]["id"].as_str().unwrap();
  let mut replacement = intent["profiles"][0].clone();
  replacement["expected_current_version"] = serde_json::json!(1);
  replacement["definition"]["name"] = serde_json::json!("Independently replaced");
  let request = serde_json::json!({"role":null,"profiles":[replacement]}).to_string();
  let replaced = router
    .clone()
    .oneshot(json_request("POST", &publication, "replace", None, &request))
    .await
    .unwrap();
  assert_eq!(replaced.status(), StatusCode::OK);
  let replaced = json_response(replaced).await;
  assert_eq!(replaced["profiles"][0]["reference"]["version"], 2);
  let stale = router
    .clone()
    .oneshot(json_request("POST", &publication, "stale", None, &request))
    .await
    .unwrap();
  assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
  for (suffix, expected) in [
    ("/versions/1", &original["profiles"][0]),
    ("", &replaced["profiles"][0]),
  ] {
    let read = router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{project}/factory-role-profiles/{profile}{suffix}"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(read.headers()["cache-control"], "no-store");
    assert_eq!(&json_response(read).await, expected);
  }
}

#[tokio::test]
async fn factory_roles_http_copies_exact_templates_and_validates_without_publishing() {
  let (router, project, intent) = role_router();
  let base = format!("/api/v1/projects/{project}");
  let page = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{base}/factory-role-templates?limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(page.status(), StatusCode::OK);
  let page = json_response(page).await;
  assert_eq!(page["total"], 3);
  assert!(!page.to_string().contains("instructions"));
  let reference = &page["items"][0]["reference"];
  let template_path = format!(
    "{base}/factory-role-templates/{}/versions/{}?digest={}",
    reference["identity"].as_str().unwrap(),
    reference["version"].as_str().unwrap(),
    reference["digest"].as_str().unwrap()
  );
  let detail = router
    .clone()
    .oneshot(empty_request("GET", &template_path, None))
    .await
    .unwrap();
  assert_eq!(detail.status(), StatusCode::OK);
  assert_eq!(detail.headers()["cache-control"], "no-store");
  let detail = json_response(detail).await;
  let mut bindings = serde_json::Map::new();
  for (index, name) in detail["profiles"].as_object().unwrap().keys().enumerate() {
    let profile = &intent["profiles"][index];
    bindings.insert(
      name.clone(),
      serde_json::json!({"id":profile["id"],"definition":profile["definition"]}),
    );
  }
  let request = serde_json::json!({"template":reference,"role_id":intent["role"]["id"],"role":intent["role"]["definition"],"profiles":bindings}).to_string();
  let path = format!("{base}/factory-role-template-copies");
  let copied = router
    .clone()
    .oneshot(json_request("POST", &path, "copy-template", None, &request))
    .await
    .unwrap();
  assert_eq!(copied.status(), StatusCode::OK);
  let copied = json_response(copied).await;
  assert_eq!(
    copied["role"]["definition"]["name"],
    intent["role"]["definition"]["name"]
  );
  assert_eq!(copied["role"]["definition"]["template"], *reference);
  let replay = router
    .clone()
    .oneshot(json_request("POST", &path, "copy-template", None, &request))
    .await
    .unwrap();
  assert_eq!(replay.status(), StatusCode::OK);
  let replay = json_response(replay).await;
  assert_eq!(copied["role"], replay["role"]);
  let validation =
    serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"definition":intent["role"]["definition"]}).to_string();
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{base}/factory-roles/validate"),
      "unused",
      None,
      &validation,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let candidate = json_response(response).await;
  let read = router
    .oneshot(empty_request(
      "GET",
      &format!(
        "{base}/factory-roles/{}",
        candidate["reference"]["id"].as_str().unwrap()
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(
    read.status(),
    StatusCode::NOT_FOUND,
    "validation must not publish a role"
  );
}

#[tokio::test]
async fn factory_roles_http_imports_a_skill_once_without_private_bytes_in_catalogs() {
  let project: ProjectId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let roles = Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy.clone()));
  let skills = Arc::new(ProjectSkillImport::new(
    store,
    Arc::new(testing::InMemoryArtifactBytes::default()),
    policy.clone(),
    Arc::new(SystemFactoryExecutionClock),
  ));
  let peer = Arc::new(RecordingApplication::default());
  let application = recording_management_application(peer.clone(), peer).with_factory_roles(
    FactoryRoleManagementApplication::new(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), roles.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy.clone(), roles)),
    )
    .with_skills(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), skills.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, skills)),
    ),
  );
  let router = management_router_with_application(|| true, application);
  let path = format!("/api/v1/projects/{project}/factory-skills");
  let request = serde_json::json!({"identity":"my-skill","files":[{"path":"SKILL.md","sha256":"37e3178f620a84dd0c5b9b08df3d5093871f9c9424a0f3f648dec3d534df9546","bytes":b"# HTTP skill\n".to_vec()}]}).to_string();
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "skill-import", None, &request))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let original = json_response(response).await;
  assert_json_matches_component(&openapi_document(), "ProjectSkillImportReceipt", &original);
  assert_eq!(original["bundle"]["identity"], "my-skill");
  assert!(!original.to_string().contains("SKILL.md"));
  assert!(!original.to_string().contains("HTTP skill"));
  let read_path = format!(
    "{path}/my-skill/versions/{}?digest={}",
    original["bundle"]["version"].as_str().unwrap(),
    original["bundle"]["digest"].as_str().unwrap()
  );
  let response = router
    .clone()
    .oneshot(empty_request("GET", &read_path, None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let tree = json_response(response).await;
  assert_json_matches_component(&openapi_document(), "ProjectSkillDetails", &tree);
  assert!(tree["bundle"]["files"]["SKILL.md"].is_object());
  assert!(tree["file"].is_null());
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{read_path}&path=SKILL.md"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(json_response(response).await["file"]["text"], "# HTTP skill\n");
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "skill-import", None, &request))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let replay = json_response(response).await;
  assert_json_matches_component(&openapi_document(), "ProjectSkillImportReceipt", &replay);
  assert_eq!(replay, original);
  let page = router.oneshot(empty_request("GET", &path, None)).await.unwrap();
  assert_eq!(page.status(), StatusCode::OK);
  let page = json_response(page).await;
  assert_eq!(page["total"], 1);
  assert_eq!(page["items"][0], original["bundle"]);
  assert!(!page.to_string().contains("SKILL.md"));
  assert!(!page.to_string().contains("HTTP skill"));
}

#[tokio::test]
async fn factory_roles_http_qualifies_installed_settings_without_publishing_or_disclosing_them_in_pages() {
  let project: ProjectId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let (choices, definition) = testing::installed_role_choices(project);
  store.enable_project(project, choices).unwrap();
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let handlers = Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy.clone()));
  let peer = Arc::new(RecordingApplication::default());
  let app =
    recording_management_application(peer.clone(), peer).with_factory_roles(FactoryRoleManagementApplication::new(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), handlers.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, handlers)),
    ));
  let router = management_router_with_application(|| true, app);
  let base = format!("/api/v1/projects/{project}");
  let page = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{base}/factory-profile-capabilities?limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(page.status(), StatusCode::OK);
  let page = json_response(page).await;
  assert_eq!(page["total"], 1);
  assert_eq!(page["items"][0]["alias"], "dependency-1");
  for private in [
    "instructions",
    "projects/model",
    "--approved-private-argument",
    "cli_arguments",
  ] {
    assert!(!page.to_string().contains(private));
  }
  let detail = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{base}/factory-profile-capabilities/dependency-1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(detail.status(), StatusCode::OK);
  assert_eq!(detail.headers()["cache-control"], "no-store");
  let detail = json_response(detail).await;
  assert_eq!(detail["definition"], serde_json::to_value(&definition).unwrap());
  let mut intent = serde_json::to_value(testing::role_profile_contract_intent(project)).unwrap();
  intent.as_object_mut().unwrap().remove("project_id");
  let published = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{base}/factory-role-publications"),
      "role",
      None,
      &intent.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(published.status(), StatusCode::OK);
  let published = json_response(published).await;
  let candidate_id = uuid::Uuid::new_v4().to_string();
  let mut request = serde_json::json!({"id":candidate_id,"role":published["role"]["reference"],"capability":"dependency-1","definition":definition});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{base}/factory-role-profiles/validate"),
      "unused",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let normalized = json_response(response).await;
  assert_eq!(normalized["profile"]["reference"]["id"], candidate_id);
  assert!(normalized["incompatibility"].is_null());
  request["definition"]["model_parameters"]["reasoning_effort"] = serde_json::json!("unsupported");
  let incompatible = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{base}/factory-role-profiles/validate"),
      "unused",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(incompatible.status(), StatusCode::OK);
  let incompatible = json_response(incompatible).await;
  assert!(incompatible["profile"].is_null());
  assert_eq!(incompatible["incompatibility"], "model_parameters");
  let absent = router
    .oneshot(empty_request(
      "GET",
      &format!("{base}/factory-role-profiles/{candidate_id}"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(absent.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn factory_roles_http_applies_authority_before_content_and_requires_a_scoped_mutation_identity() {
  let policy = Arc::new(DenyAllManagementPolicy {
    decisions: AtomicUsize::new(0),
  });
  let (denied, project, intent) = role_router_with_policy(policy.clone());
  let base = format!("/api/v1/projects/{project}");
  for request in [
    empty_request("GET", &format!("{base}/factory-roles"), None),
    empty_request(
      "GET",
      &format!("{base}/factory-roles/{}", intent["role"]["id"].as_str().unwrap()),
      None,
    ),
    empty_request("GET", &format!("{base}/factory-profile-capabilities"), None),
    json_request(
      "POST",
      &format!("{base}/factory-role-publications"),
      "forbidden",
      None,
      &intent.to_string(),
    ),
  ] {
    let response = denied.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = json_response(response).await;
    for private in ["projects/model", "Custom reviewer", "Follow Project rules"] {
      assert!(!response.to_string().contains(private));
    }
  }
  assert_eq!(policy.decisions.load(Ordering::SeqCst), 4);
  let (router, project, intent) = role_router();
  let path = format!("/api/v1/projects/{project}/factory-role-publications");
  let mut request = json_request("POST", &path, "missing", None, &intent.to_string());
  request
    .headers_mut()
    .remove(octacity_server_api_rest::v1::IDEMPOTENCY_KEY_HEADER);
  let response = router.clone().oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let invalid = serde_json::json!({"role":intent["role"],"profiles":intent["profiles"],"unexpected":"private"});
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "unknown-field", None, &invalid.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let page = router
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-roles?limit=0"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(page.status(), StatusCode::BAD_REQUEST);
}

struct ControlPolicy(AtomicBool);
#[async_trait]
impl ManagementAuthorizationPolicy for ControlPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if !self.0.load(Ordering::SeqCst) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}
#[tokio::test]
async fn factory_profile_http_control_fences_competing_intents_and_replays_after_dispatch_with_current_authority() {
  let fixture = testing::pending_factory_profile_fixture().await;
  let policy = Arc::new(ControlPolicy(AtomicBool::new(true)));
  let service = Arc::new(FactoryOperatorHandlers::new(fixture.store.clone(), policy.clone()));
  let peer = Arc::new(RecordingApplication::default());
  let app = recording_management_application(peer.clone(), peer)
    .with_factory_profile_selection(Arc::new(AuthorizedCommandHandler::new(policy.clone(), service)));
  let router = management_router_with_application(|| true, app);
  let path = format!(
    "/api/v1/factory-runs/{}/profile-selections/{}",
    fixture.run_id, fixture.input_digest
  );
  let bodies = [0, 1].map(|i| serde_json::json!({"profile":fixture.profiles[i]}).to_string());
  let missing = router
    .clone()
    .oneshot(json_request("POST", &path, "missing", None, &bodies[0]))
    .await
    .unwrap();
  assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
  let expected = format!("\"{}\"", fixture.version);
  let (first, second) = tokio::join!(
    router
      .clone()
      .oneshot(json_request("POST", &path, "choose-a", Some(&expected), &bodies[0])),
    router
      .clone()
      .oneshot(json_request("POST", &path, "choose-b", Some(&expected), &bodies[1])),
  );
  let responses = [first.unwrap(), second.unwrap()];
  assert_eq!(
    responses.iter().filter(|r| r.status() == StatusCode::OK).count(),
    1,
    "{responses:?}"
  );
  assert_eq!(
    responses.iter().filter(|r| r.status() == StatusCode::CONFLICT).count(),
    1
  );
  let winner = responses.iter().position(|r| r.status() == StatusCode::OK).unwrap();
  let receipt = json_response(responses.into_iter().nth(winner).unwrap()).await;
  assert_eq!(receipt["disposition"], "applied");
  assert_eq!(receipt["version"], fixture.version + 1);
  assert_eq!(receipt["state"], "running");
  let version_after_dispatch = fixture.commit_build_intent().await;
  let key = ["choose-a", "choose-b"][winner];
  let replay = router
    .clone()
    .oneshot(json_request("POST", &path, key, Some(&expected), &bodies[winner]))
    .await
    .unwrap();
  assert_eq!(replay.status(), StatusCode::OK);
  let replay = json_response(replay).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["version"], receipt["version"]);
  let fresh = format!("\"{version_after_dispatch}\"");
  let late = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "after-dispatch",
      Some(&fresh),
      &bodies[1 - winner],
    ))
    .await
    .unwrap();
  assert_eq!(late.status(), StatusCode::CONFLICT);
  policy.0.store(false, Ordering::SeqCst);
  let revoked = router
    .oneshot(json_request("POST", &path, key, Some(&expected), &bodies[winner]))
    .await
    .unwrap();
  assert_eq!(revoked.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn profile_diagnostics_http_uses_the_shared_authorized_reader_and_safe_schema() {
  let fixture = testing::pending_factory_profile_fixture().await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(
      FactoryDiscoveryHandlers::with_policy(fixture.store.clone(), policy)
        .with_profile_diagnostics(fixture.store.clone()),
    ),
  ));
  let peer = Arc::new(RecordingApplication::default());
  let app = recording_management_application(peer.clone(), peer).with_factory_diagnostic_reader(queries);
  let router = management_router_with_application(|| true, app);
  let path = format!("/api/v1/factory-runs/{}/profile-selections?limit=1", fixture.run_id);
  let response = router.oneshot(empty_request("GET", &path, None)).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let page = json_response(response).await;
  assert_eq!(page["items"][0]["kind"], "profile_selection");
  assert_eq!(
    page["items"][0]["record"]["eligible_candidates"]
      .as_array()
      .unwrap()
      .len(),
    3
  );
  assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &page);
}

// Inventory probes exercise authorization and dispatch separately from real application behavior above.
macro_rules! unavailable_role_use_cases {
  (query: $($ty:ty => $label:literal),+ $(,)?) => {$(
    #[async_trait]
    impl ManagementQueryUseCase<$ty> for RecordingApplication {
      type Error=ApplicationError;
      async fn execute_management_query(&self,_context:&ManagementRequestContext,_grant:&ManagementAuthorizationGrant,_query:$ty)->Result<<$ty as Query>::Outcome,Self::Error> {
        self.record($label); Err(ApplicationError::unavailable())
      }
    }
  )+};
  (command: $($ty:ty => $label:literal),+ $(,)?) => {$(
    #[async_trait]
    impl ManagementCommandUseCase<$ty> for RecordingApplication {
      type Error=ApplicationError;
      async fn execute_management_command(&self,_context:&ManagementRequestContext,_grant:&ManagementAuthorizationGrant,_command:$ty)->Result<<$ty as Command>::Outcome,Self::Error> {
        self.record($label); Err(ApplicationError::unavailable())
      }
    }
  )+};
}
unavailable_role_use_cases!(query:
  GetFactoryRoleQuery=>"get_factory_role", GetFactoryRoleProfileQuery=>"get_factory_role_profile",
  ListFactoryRolesQuery=>"list_factory_roles", ListFactoryRoleProfilesQuery=>"list_factory_role_profiles",
  ValidateFactoryRoleQuery=>"validate_factory_role", ListFactoryRoleTemplatesQuery=>"list_factory_role_templates",
  GetFactoryRoleTemplateQuery=>"get_factory_role_template", ListFactoryRoleProfileCapabilitiesQuery=>"list_factory_profile_capabilities",
  RefreshFactoryRoleProfileCapabilityQuery=>"refresh_factory_profile_capability",
  GetFactoryRoleProfileCapabilityQuery=>"get_factory_profile_capability", ValidateFactoryRoleProfileQuery=>"validate_factory_role_profile",
  ListFactoryRuntimeChoicesQuery=>"list_factory_runtime_choices",
  GetFactoryRuntimeSettingsQuery=>"get_factory_runtime_settings",
  ValidateFactoryRuntimeSettingsQuery=>"validate_factory_runtime_settings",
  ListProjectSkillsQuery=>"list_project_skills",
  GetProjectSkillQuery=>"get_project_skill",
);
unavailable_role_use_cases!(command:
  DeleteFactoryRoleCommand=>"delete_factory_role", PublishFactoryRoleProfilesCommand=>"publish_factory_role_profiles", CopyFactoryRoleTemplateCommand=>"copy_factory_role_template",
  ImportProjectSkill=>"import_project_skill", SelectFactoryProfileCommand=>"select_factory_profile",
  PublishFactoryRuntimeSettingsCommand=>"publish_factory_runtime_settings",
);

pub(super) fn management_requests() -> Vec<Request<Body>> {
  let project = "11111111-1111-4111-8111-111111111111";
  let prefix = format!("/api/v1/projects/{project}");
  let mut intent = serde_json::to_value(testing::role_profile_contract_intent(project.parse().unwrap())).unwrap();
  intent.as_object_mut().unwrap().remove("project_id");
  let role = intent["role"]["id"].as_str().unwrap();
  let profile = intent["profiles"][0]["id"].as_str().unwrap();
  let reference = &intent["profiles"][0]["role"];
  let template = serde_json::json!({"identity":"starter","version":"v1","digest":"01".repeat(32)});
  vec![
    empty_request("POST", &format!("{prefix}/factory-profile-capabilities/runtime/refresh"), None),
    json_request("POST",&format!("{prefix}/factory-role-publications"),"publish",None,&intent.to_string()),
    empty_request("DELETE", &format!("{prefix}/factory-roles/{role}"), Some(("delete-role", "\"1\""))),
    empty_request("GET",&format!("{prefix}/factory-roles?limit=1"),None),
    empty_request("GET",&format!("{prefix}/factory-roles/{role}"),None),
    empty_request("GET",&format!("{prefix}/factory-roles/{role}/versions/1"),None),
    empty_request("GET",&format!("{prefix}/factory-roles/{role}/profiles?limit=1"),None),
    empty_request("GET",&format!("{prefix}/factory-role-profiles/{profile}"),None),
    empty_request("GET",&format!("{prefix}/factory-role-profiles/{profile}/versions/1"),None),
    empty_request("GET",&format!("{prefix}/factory-role-templates?limit=1"),None),
    empty_request("GET",&format!("{prefix}/factory-role-templates/starter/versions/v1?digest={}","01".repeat(32)),None),
    json_request("POST",&format!("{prefix}/factory-role-template-copies"),"copy",None,&serde_json::json!({"template":template,"role_id":role,"role":intent["role"]["definition"],"profiles":{"default":{"id":profile,"definition":intent["profiles"][0]["definition"]}}}).to_string()),
    json_request("POST",&format!("{prefix}/factory-roles/validate"),"unused",None,&serde_json::json!({"id":role,"definition":intent["role"]["definition"]}).to_string()),
    empty_request("GET",&format!("{prefix}/factory-profile-capabilities?limit=1"),None),
    empty_request("GET",&format!("{prefix}/factory-profile-capabilities/runtime"),None),
    json_request("POST",&format!("{prefix}/factory-role-profiles/validate"),"unused",None,&serde_json::json!({"id":profile,"role":reference,"capability":"runtime","definition":intent["profiles"][0]["definition"]}).to_string()),
    json_request("POST",&format!("{prefix}/factory-skills"),"import",None,&serde_json::json!({"identity":"test","files":[{"path":"SKILL.md","sha256":"01".repeat(32),"bytes":[1]}]}).to_string()),
    empty_request("GET",&format!("{prefix}/factory-skills?limit=1"),None),
    empty_request("GET",&format!("{prefix}/factory-skills/test/versions/v1?digest={}&path=SKILL.md","01".repeat(32)),None),
    json_request("POST",&format!("/api/v1/factory-runs/{role}/profile-selections/{}","01".repeat(32)),"select",Some("\"1\""),&serde_json::json!({"profile":reference}).to_string()),
  ]
}

#[tokio::test]
async fn flow_authoring_content_that_can_embed_profile_instructions_is_not_cached() {
  let peer = Arc::new(RecordingApplication::successful_workflow());
  let router = management_router_with_application(|| true, recording_management_application(peer.clone(), peer));
  let id = "33333333-3333-4333-8333-333333333333";
  let draft = factory::flow_draft_request();
  for request in [
    empty_request("GET", &format!("/api/v1/flow-definitions/{id}"), None),
    empty_request("GET", &format!("/api/v1/flow-definitions/{id}/versions/1"), None),
    empty_request(
      "GET",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/flow-definitions?limit=1",
      None,
    ),
    json_request(
      "POST",
      "/api/v1/flow-definitions/validate",
      "unused",
      None,
      &draft.to_string(),
    ),
    json_request("POST", "/api/v1/flow-definitions", "create", None, &draft.to_string()),
    json_request(
      "POST",
      &format!("/api/v1/flow-definitions/{id}/versions"),
      "replace",
      Some("\"1\""),
      &serde_json::json!({"definition":draft["definition"]}).to_string(),
    ),
  ] {
    let path = request.uri().clone();
    let response = router.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_success(), "{path}");
    assert_eq!(
      response.headers().get("cache-control").map(|h| h.to_str().unwrap()),
      Some("no-store"),
      "{path}"
    );
  }
}

#[tokio::test]
async fn factory_roles_http_deletion_fences_replays_and_keeps_exact_history() {
  let policy = Arc::new(ControlPolicy(AtomicBool::new(true)));
  let (router, project, intent) = role_router_with_policy(policy.clone());
  let published = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{project}/factory-role-publications"),
      "create-before-delete",
      None,
      &intent.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(published.status(), StatusCode::OK);
  let original = json_response(published).await;
  let id = original["role"]["reference"]["id"].as_str().unwrap();
  let path = format!("/api/v1/projects/{project}/factory-roles/{id}");
  let missing = router
    .clone()
    .oneshot(json_request("DELETE", &path, "delete-agent", None, ""))
    .await
    .unwrap();
  assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
  let stale = router
    .clone()
    .oneshot(json_request("DELETE", &path, "stale-delete", Some("\"2\""), ""))
    .await
    .unwrap();
  assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
  let deleted = router
    .clone()
    .oneshot(json_request("DELETE", &path, "delete-agent", Some("\"1\""), ""))
    .await
    .unwrap();
  assert_eq!(deleted.status(), StatusCode::OK);
  assert_eq!(deleted.headers()["cache-control"], "no-store");
  let receipt = json_response(deleted).await;
  assert_json_matches_component(&openapi_document(), "DeleteFactoryRoleResponse", &receipt);
  assert_eq!(receipt["role"], original["role"]["reference"]);
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &path, None))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  let page = json_response(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{project}/factory-roles"),
        None,
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(page["total"], 0);
  let exact = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/versions/1"), None))
    .await
    .unwrap();
  assert_eq!(json_response(exact).await, original["role"]);
  for profile in original["profiles"].as_array().unwrap() {
    let path = format!(
      "/api/v1/projects/{project}/factory-role-profiles/{}/versions/1",
      profile["reference"]["id"].as_str().unwrap()
    );
    assert_eq!(
      json_response(router.clone().oneshot(empty_request("GET", &path, None)).await.unwrap()).await,
      *profile
    );
  }
  let replay = json_response(
    router
      .clone()
      .oneshot(json_request("DELETE", &path, "delete-agent", Some("\"1\""), ""))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["deleted_at"], receipt["deleted_at"]);
  policy.0.store(false, Ordering::SeqCst);
  let denied = router
    .oneshot(json_request("DELETE", &path, "delete-agent", Some("\"1\""), ""))
    .await
    .unwrap();
  assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn plugin_model_catalog_http_uses_explicit_refresh_and_the_existing_capability_response() {
  let project = ProjectId::generate();
  let (choices, definition) = testing::installed_role_choices(project);
  let (alias, mut cap) = choices
    .role_profile_capabilities()
    .iter()
    .next()
    .map(|(a, c)| (a.clone(), c.clone()))
    .unwrap();
  cap.model_catalog = Some(serde_json::from_value(serde_json::json!({"available":true,"manual_model":true,"models":[{"identifier":"Vendor/future:preview","label":"Future preview","parameters":{"reference":{"identity":"operator.exact-integer","version":"v1","digest":"cf6037a1bca746f5423c6b01bf78ea84376ed75a5f0565596ca2a3ed15b9aba3"},"shape":{"type":"integer","minimum":i64::MIN,"maximum":i64::MAX}}}]})).unwrap());
  let choices = choices.with_role_profile_capability(alias.clone(), cap).unwrap();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  store.enable_project(project, choices).unwrap();
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let handlers = Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy.clone()));
  let peer = Arc::new(RecordingApplication::default());
  let app =
    recording_management_application(peer.clone(), peer).with_factory_roles(FactoryRoleManagementApplication::new(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), handlers.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, handlers)),
    ));
  let router = management_router_with_application(|| true, app);
  let path = format!("/api/v1/projects/{project}/factory-profile-capabilities/{alias}");
  for (method, path) in [("GET", path.clone()), ("POST", format!("{path}/refresh"))] {
    let response = router
      .clone()
      .oneshot(empty_request(method, &path, None))
      .await
      .unwrap();
    if method == "POST" {
      assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "an installed discovery source is required"
      );
      continue;
    }
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = json_response(response).await;
    assert_eq!(
      body["model_catalog"]["models"][0]["identifier"],
      "Vendor/future:preview"
    );
    assert_eq!(
      body["model_catalog"]["models"][0]["parameters"]["shape"]["maximum"],
      "9223372036854775807"
    );
    assert_eq!(
      body["model_catalog"]["models"][0]["parameters"]["shape"]["minimum"],
      "-9223372036854775808"
    );
    assert_json_matches_component(&openapi_document(), "FactoryRoleProfileCapabilityDetails", &body);
  }
  let mut manual = serde_json::to_value(definition).unwrap();
  manual["model_identifier"] = serde_json::json!("unlisted/model:next");
  assert_json_matches_component(&openapi_document(), "FactoryRoleProfileDefinition", &manual);
}
