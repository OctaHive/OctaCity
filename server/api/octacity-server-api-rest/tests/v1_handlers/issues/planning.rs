use super::*;
fn settings() -> String {
  format!("/api/v1/projects/{PROJECT}/issue-property-definitions")
}
fn choices() -> String {
  format!("/api/v1/projects/{PROJECT}/issue-property-choices")
}
#[tokio::test]
async fn issue_http_planning_choices_are_unassigned_without_a_directory_or_provider() {
  let (router, _store) = issue_router_with_store(Arc::new(TrustedNetworkManagementPolicy)).await;
  let response = router
    .clone()
    .oneshot(empty_request("GET", &choices(), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let result = document(response).await;
  assert_eq!(result["definitions"]["version"], 0);
  assert_eq!(
    result["definitions"]["definition"],
    serde_json::json!({"labels":[],"properties":[]})
  );
  assert_eq!(result["assignees"], serde_json::json!({"available":false,"items":[]}));
  let response = router
    .oneshot(json_request(
      "POST",
      &root(),
      "manual-without-directory",
      None,
      &draft().to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
}

fn catalog() -> serde_json::Value {
  serde_json::json!({"labels":[{"id":"defect","name":"Дефект","retired":false}],"properties":[
    {"id":"size","name":"Размер","retired":false,"kind":{"type":"choice","choices":[{"id":"small","name":"S","retired":false}]}},
    {"id":"note","name":"Заметка","retired":false,"kind":{"type":"text","max_bytes":16}},
    {"id":"verified","name":"Проверено","retired":false,"kind":{"type":"boolean"}},
    {"id":"estimate","name":"Оценка","retired":false,"kind":{"type":"number","minimum":0,"maximum":10}},
    {"id":"review_date","name":"Дата","retired":false,"kind":{"type":"date"}}
  ]})
}
async fn publish(
  router: &axum::Router,
  key: &str,
  version: u64,
  definition: serde_json::Value,
) -> axum::response::Response {
  router
    .clone()
    .oneshot(json_request(
      "POST",
      &settings(),
      key,
      None,
      &serde_json::json!({"expected_version":version,"definition":definition}).to_string(),
    ))
    .await
    .unwrap()
}
#[tokio::test]
async fn issue_http_planning_catalog_is_versioned_bounded_and_replays_original_publication() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let response = router
    .clone()
    .oneshot(empty_request("GET", &settings(), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"0\"");
  let empty = document(response).await;
  let response = publish(&router, "definitions", 0, catalog()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"1\"");
  assert_eq!(response.headers()["cache-control"], "no-store");
  let original = document(response).await;
  assert_eq!(original["resource"]["version"], 1);
  assert_eq!(original["resource"]["definition"], catalog());
  let mut retired = catalog();
  retired["labels"][0]["retired"] = true.into();
  assert_eq!(publish(&router, "retired", 1, retired).await.status(), StatusCode::OK);
  let replay = document(publish(&router, "definitions", 0, catalog()).await).await;
  assert_eq!(replay["resource"], original["resource"]);
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(
    publish(&router, "stale", 0, catalog()).await.status(),
    StatusCode::CONFLICT
  );
  for (version, expected) in [(0, empty), (1, original["resource"].clone())] {
    let response = router
      .clone()
      .oneshot(empty_request("GET", &format!("{}/{version}", settings()), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(document(response).await, expected);
  }
  let mut invalid = catalog();
  invalid["properties"][0]["kind"] = serde_json::json!({"type":"formula","expression":"start()"});
  assert_eq!(
    publish(&router, "invalid", 2, invalid).await.status(),
    StatusCode::BAD_REQUEST
  );
  let mut oversized = catalog();
  oversized["properties"] = serde_json::json!(vec![catalog()["properties"][0].clone(); 33]);
  assert_eq!(
    publish(&router, "oversized", 2, oversized).await.status(),
    StatusCode::BAD_REQUEST
  );
  let response = router.oneshot(empty_request("GET", &settings(), None)).await.unwrap();
  assert_eq!(document(response).await["version"], 2);
}

fn planned() -> serde_json::Value {
  serde_json::json!({"definition_version":1,"assignee":null,"labels":["defect"],"due_date":"2028-02-29","parent_id":null,
    "values":{"size":{"type":"choice","value":"small"},"note":{"type":"text","value":"PRIVATE NOTE"},"verified":{"type":"boolean","value":true},"estimate":{"type":"number","value":3},"review_date":{"type":"date","value":"2029-01-01"}}})
}
#[tokio::test]
async fn issue_http_planning_create_edit_and_exact_replay_preserve_status_and_requirements() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  assert_eq!(publish(&router, "catalog", 0, catalog()).await.status(), StatusCode::OK);
  let mut request = draft();
  request["planning"] = planned();
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), "planned", None, &request.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let first = document(response).await;
  assert_eq!(first["resource"]["planning"], planned());
  let path = format!("{}/{}", root(), first["resource"]["id"].as_str().unwrap());
  let mut changed = planned();
  changed["due_date"] = serde_json::Value::Null;
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &format!("{path}/planning"),
      "plan",
      Some("\"1\""),
      &changed.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  assert_eq!(response.headers()["cache-control"], "no-store");
  let second = document(response).await;
  assert_eq!(second["resource"]["planning"], changed);
  for field in [
    "content",
    "status_id",
    "workflow_id",
    "workflow_version",
    "applicability_version",
  ] {
    assert_eq!(second["resource"][field], first["resource"][field], "{field}");
  }
  let mut later = changed.clone();
  later["labels"] = serde_json::json!([]);
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{path}/planning"),
        "later",
        Some("\"2\""),
        &later.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let replay = document(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{path}/planning"),
        "plan",
        Some("\"1\""),
        &changed.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["resource"], second["resource"]);
  assert_eq!(replay["disposition"], "replayed");
  let revision = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/revisions/1"), None))
    .await
    .unwrap();
  assert_eq!(revision.headers()["cache-control"], "no-store");
  assert_eq!(document(revision).await, first["resource"]);
  for (version, status) in [
    (None, StatusCode::PRECONDITION_REQUIRED),
    (Some("W/\"3\""), StatusCode::BAD_REQUEST),
    (Some("\"1\""), StatusCode::CONFLICT),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "PATCH",
          &format!("{path}/planning"),
          "stale",
          version,
          &later.to_string()
        ))
        .await
        .unwrap()
        .status(),
      status
    );
  }
  later["definition_version"] = 0.into();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{path}/planning"),
        "stale-catalog",
        Some("\"3\""),
        &later.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  let page = document(
    router
      .clone()
      .oneshot(empty_request("GET", &root(), None))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    page["items"][0].get("planning").is_none(),
    "lists expose no private property values"
  );
  let history = document(
    router
      .oneshot(empty_request("GET", &format!("{path}/status-history"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(history["items"].as_array().unwrap().len(), 1);
}

fn empty_plan(parent: serde_json::Value) -> serde_json::Value {
  serde_json::json!({"definition_version":0,"assignee":null,"labels":[],"values":{},"due_date":null,"parent_id":parent})
}
#[tokio::test]
async fn issue_http_planning_children_enforce_visible_parent_and_filter_before_paging() {
  let scope = Arc::new(Mutex::new(None));
  let router = issue_router(Arc::new(IssueScope(scope.clone()))).await;
  let parent = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "parent", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await["resource"]
    .clone();
  let mut children = Vec::new();
  for index in 0..3 {
    let mut request = draft();
    request["planning"] = empty_plan(parent["id"].clone());
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        &format!("child-{index}"),
        None,
        &request.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    children.push(document(response).await["resource"].clone());
  }
  children.sort_by_key(|v| v["id"].as_str().unwrap().to_owned());
  let visible = |v: &serde_json::Value| {
    ManagementResource::instance(
      octacity_server_application::ManagementResourceKind::Issue,
      octacity_server_application::ManagementResourceIdentity::new(v["id"].as_str().unwrap()).unwrap(),
    )
    .unwrap()
  };
  *scope.lock().unwrap() = Some(vec![visible(&parent), visible(&children[1]), visible(&children[2])]);
  let path = format!("{}/{}/children", root(), parent["id"].as_str().unwrap());
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page = document(response).await;
  assert_eq!(page["items"][0]["id"], children[1]["id"]);
  assert_eq!(page["next_cursor"], children[1]["id"]);
  assert!(page["items"][0].get("planning").is_none());
  let last = document(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{path}?limit=1&after={}", children[1]["id"].as_str().unwrap()),
        None,
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(last["items"][0]["id"], children[2]["id"]);
  assert!(last["next_cursor"].is_null());
  for query in [
    "limit=0".into(),
    "limit=201".into(),
    "provider=github".into(),
    format!("after={}", children[0]["id"].as_str().unwrap()),
    format!("after={}", parent["id"].as_str().unwrap()),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  let plan = empty_plan(children[1]["id"].clone());
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{}/{}/planning", root(), parent["id"].as_str().unwrap()),
        "cycle",
        Some("\"1\""),
        &plan.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  *scope.lock().unwrap() = Some(vec![visible(&children[2])]);
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &path, None))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  let mut request = draft();
  request["planning"] = empty_plan(parent["id"].clone());
  // Even original creation replay must recheck parent visibility before the receipt.
  assert_eq!(
    router
      .oneshot(json_request("POST", &root(), "child-0", None, &request.to_string()))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
}

#[tokio::test]
async fn issue_openapi_planning_is_closed_bounded_and_matches_actual_http_documents() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  for name in [
    "IssuePlanning",
    "IssuePropertyDefinitionSet",
    "IssuePropertyDefinition",
    "IssuePropertyChoice",
    "PublishedIssueProperties",
    "IssueAssigneeChoices",
    "IssueAssignee",
    "IssuePropertyChoices",
    "PublishIssuePropertiesRequest",
    "CreateIssueRequest",
  ] {
    assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
  }
  assert_eq!(
    schemas["IssuePropertyDefinitionSet"]["properties"]["properties"]["maxItems"],
    32
  );
  assert_eq!(schemas["IssuePlanning"]["properties"]["values"]["maxProperties"], 32);
  assert_eq!(schemas["IssueAssigneeChoices"]["properties"]["items"]["maxItems"], 200);
  assert_eq!(schemas["IssuePropertyValue"]["oneOf"].as_array().unwrap().len(), 5);
  assert_eq!(schemas["IssuePropertyKind"]["oneOf"].as_array().unwrap().len(), 5);
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let choices = document(
    router
      .clone()
      .oneshot(empty_request("GET", &choices(), None))
      .await
      .unwrap(),
  )
  .await;
  support::assert_json_matches_component(&doc, "IssuePropertyChoices", &choices);
  let publication = document(publish(&router, "catalog", 0, catalog()).await).await;
  support::assert_json_matches_component(&doc, "IssuePropertiesMutationResponse", &publication);
  support::assert_json_matches_component(
    &doc,
    "PublishIssuePropertiesRequest",
    &serde_json::json!({"expected_version":0,"definition":catalog()}),
  );
  let mut request = draft();
  request["planning"] = planned();
  support::assert_json_matches_component(&doc, "CreateIssueRequest", &request);
  let receipt = document(
    router
      .oneshot(json_request("POST", &root(), "planned", None, &request.to_string()))
      .await
      .unwrap(),
  )
  .await;
  support::assert_json_matches_component(&doc, "IssueMutationResponse", &receipt);
  let definitions = "/api/v1/projects/{project_id}/issue-property-definitions";
  assert_eq!(
    management_authorization_operations()
      .into_iter()
      .find(|o| o.operation_id == "publishIssueProperties")
      .unwrap()
      .authorization
      .action(),
    ManagementAction::Administer
  );
  for (path, method) in [
    (definitions.to_string(), "get"),
    (definitions.to_string(), "post"),
    ("/api/v1/projects/{project_id}/issue-property-choices".into(), "get"),
    (
      "/api/v1/projects/{project_id}/issues/{issue_id}/planning".into(),
      "patch",
    ),
    ("/api/v1/projects/{project_id}/issues/{issue_id}/children".into(), "get"),
  ] {
    assert_eq!(
      doc["paths"][path][method]["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],
      "no-store"
    );
  }
}

#[tokio::test]
async fn issue_http_planning_rejects_duplicate_labels_unsupported_values_and_reserved_setters() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  assert_eq!(publish(&router, "catalog", 0, catalog()).await.status(), StatusCode::OK);
  let mut invalid = Vec::new();
  for count in [33, 2] {
    let mut plan = planned();
    plan["labels"] = serde_json::json!(vec!["defect"; count]);
    invalid.push((plan, StatusCode::BAD_REQUEST));
  }
  for (field, value) in [
    ("status_id", serde_json::json!("approved")),
    ("execution_state", serde_json::json!("running")),
    ("credentials", serde_json::json!({"token":"not-a-token"})),
    ("assignee", serde_json::json!("gateway-operator")),
    ("due_date", serde_json::json!("2027-02-29")),
  ] {
    let mut plan = planned();
    plan[field] = value;
    invalid.push((
      plan,
      if field == "assignee" {
        StatusCode::CONFLICT
      } else {
        StatusCode::BAD_REQUEST
      },
    ));
  }
  for value in [
    serde_json::json!({"type":"number","value":1.5}),
    serde_json::json!({"type":"number","value":11}),
    serde_json::json!({"type":"formula","value":"run()"}),
    serde_json::json!({"type":"number","value":1,"execute":true}),
  ] {
    let mut plan = planned();
    plan["values"]["estimate"] = value;
    invalid.push((plan, StatusCode::BAD_REQUEST));
  }
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "manual", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}/planning", root(), created["resource"]["id"].as_str().unwrap());
  for (index, (plan, status)) in invalid.into_iter().enumerate() {
    let mut request = draft();
    request["planning"] = plan.clone();
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &root(),
          &format!("invalid-{index}"),
          None,
          &request.to_string()
        ))
        .await
        .unwrap()
        .status(),
      status,
      "create {index}"
    );
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "PATCH",
          &path,
          &format!("edit-{index}"),
          Some("\"1\""),
          &plan.to_string()
        ))
        .await
        .unwrap()
        .status(),
      status,
      "edit {index}"
    );
  }
}

struct People(Arc<Mutex<bool>>);
#[async_trait]
impl octacity_server_application::IssueAssigneeDirectory for People {
  async fn choices(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_application::ProjectId,
  ) -> Result<octacity_server_application::IssueAssigneeChoices, ApplicationError> {
    Ok(if *self.0.lock().unwrap() {
      octacity_server_application::IssueAssigneeChoices {
        available: true,
        items: vec![octacity_server_application::IssueAssignee {
          id: "person-1".into(),
          name: "Дежурный".into(),
        }],
      }
    } else {
      Default::default()
    })
  }
}
#[tokio::test]
async fn issue_http_planning_only_offers_authorized_people_and_retains_assignments_during_directory_loss() {
  let available = Arc::new(Mutex::new(true));
  let (router, _) = issue_router_with_directory(
    Arc::new(TrustedNetworkManagementPolicy),
    Some(Arc::new(People(available.clone()))),
  )
  .await;
  let result = document(
    router
      .clone()
      .oneshot(empty_request("GET", &choices(), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(
    result["assignees"],
    serde_json::json!({"available":true,"items":[{"id":"person-1","name":"Дежурный"}]})
  );
  let mut plan = empty_plan(serde_json::Value::Null);
  plan["assignee"] = "person-1".into();
  let mut request = draft();
  request["planning"] = plan.clone();
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), "assigned", None, &request.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let created = document(response).await;
  let path = format!("{}/{}/planning", root(), created["resource"]["id"].as_str().unwrap());
  *available.lock().unwrap() = false;
  let result = document(
    router
      .clone()
      .oneshot(empty_request("GET", &choices(), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(result["assignees"], serde_json::json!({"available":false,"items":[]}));
  plan["due_date"] = "2028-01-01".into();
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &path,
      "retain-person",
      Some("\"1\""),
      &plan.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await["resource"]["planning"]["assignee"], "person-1");
  plan["assignee"] = "hidden-person".into();
  assert_eq!(
    router
      .oneshot(json_request(
        "PATCH",
        &path,
        "unknown-person",
        Some("\"2\""),
        &plan.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
}

struct PlanningRights(Arc<Mutex<(bool, bool, bool)>>);
#[async_trait]
impl ManagementAuthorizationPolicy for PlanningRights {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let (admin, edit, visible) = *self.0.lock().unwrap();
    if (action == ManagementAction::Administer && !admin) || (action == ManagementAction::Update && !edit) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(
      if !visible && resource.kind() == octacity_server_application::ManagementResourceKind::Project {
        octacity_server_application::ManagementVisibility::none()
      } else {
        octacity_server_application::ManagementVisibility::all()
      },
    ))
  }
}
#[tokio::test]
async fn issue_http_planning_definition_administration_and_task_editing_have_separate_rights() {
  let rights = Arc::new(Mutex::new((false, true, true)));
  let router = issue_router(Arc::new(PlanningRights(rights.clone()))).await;
  assert_eq!(
    publish(&router, "denied-catalog", 0, catalog()).await.status(),
    StatusCode::FORBIDDEN
  );
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "manual", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}/planning", root(), created["resource"]["id"].as_str().unwrap());
  let mut plan = empty_plan(serde_json::Value::Null);
  plan["due_date"] = "2028-01-01".into();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "edit-allowed",
        Some("\"1\""),
        &plan.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  *rights.lock().unwrap() = (true, false, true);
  assert_eq!(
    publish(&router, "admin-only", 0, catalog()).await.status(),
    StatusCode::OK
  );
  plan["definition_version"] = 1.into();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "edit-denied",
        Some("\"2\""),
        &plan.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  *rights.lock().unwrap() = (true, true, false);
  for path in [choices(), settings(), format!("{}/1", settings())] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &path, None))
        .await
        .unwrap()
        .status(),
      StatusCode::NOT_FOUND
    );
  }
}
