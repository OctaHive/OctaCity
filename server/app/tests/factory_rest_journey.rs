//! OpenAPI-only operator workflow against the released HTTP composition.
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
#[path = "support/factory_rest_fixture.rs"]
mod fixture;
#[path = "support/openapi_client.rs"]
mod openapi_client;
#[allow(dead_code)]
#[path = "../../api/octacity-server-api-rest/tests/support/openapi.rs"]
mod openapi_contract;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres_support;
#[allow(dead_code)]
#[path = "support/runtime.rs"]
mod runtime_fixture;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and object storage"]
async fn openapi_only_client_drives_a_nested_factory_run() {
  let database = postgres_support::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let directory = tempfile::tempdir().unwrap();
  let config = octacity_server::ServerConfig::parse_toml(&runtime_fixture::runtime_config_source(
    directory.path(),
    url.as_str(),
    &std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
    "[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=5000\nbatch_size=4\nconcurrency=2",
  ))
  .unwrap();
  let runtime = octacity_server::ServerRuntime::start_with_factory(config, Arc::new(fixture::Installation))
    .await
    .unwrap();
  let client = openapi_client::OpenApiClient::connect(format!("http://{}", runtime.management_addr())).await;
  let result = client
    .ok(
      "createProject",
      &[],
      Some(json!({"parent_id":null,"name":format!("headless-{}",Uuid::new_v4())})),
      Some("project"),
      None,
    )
    .await;
  let project = result.body["resource"]["id"].as_str().unwrap();
  let choices = client
    .ok(
      "listFactoryConfigurationChoices",
      &[("project_id", project)],
      None,
      None,
      None,
    )
    .await;
  let choice = |kind: &str| {
    choices.body["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|item| item["kind"] == kind)
      .unwrap()
  };
  let plugin = choices.body["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["alias"] == "observer")
    .unwrap()["reference"]["reference"]
    .clone();
  let (boolean, operation) = fixture::contracts();
  let (eligibility_contract, classification_contract) = fixture::triage_contracts();
  let selected = format!("step-{}", Uuid::new_v4().simple());
  let mut node = fixture::node(
    &selected,
    "trusted_action",
    &boolean["reference"],
    &eligibility_contract["reference"],
    "observed",
  );
  node["action"] = json!({"plugin":plugin,"action":"observe","parameters":{}});
  let eligibility_gate_key = format!("gate-{}", Uuid::new_v4().simple());
  let classification_key = format!("classify-{}", Uuid::new_v4().simple());
  let classification_gate_key = format!("route-{}", Uuid::new_v4().simple());
  let eligibility_gate = fixture::gate(
    &eligibility_gate_key,
    &eligibility_contract["reference"],
    &boolean["reference"],
    "eligible",
    vec![("/fit", json!("fits")), ("/duplicate", json!(false))],
  );
  let mut classification = fixture::node(
    &classification_key,
    "trusted_action",
    &boolean["reference"],
    &classification_contract["reference"],
    "classified",
  );
  let classifier = choices.body["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["alias"] == "classifier")
    .unwrap()["reference"]["reference"]
    .clone();
  classification["action"] = json!({"plugin":classifier,"action":"observe","parameters":{}});
  let classification_gate = fixture::gate(
    &classification_gate_key,
    &classification_contract["reference"],
    &boolean["reference"],
    "review",
    vec![
      ("/work_kind", json!("feature_request")),
      ("/size", json!("small")),
      ("/risk", json!("low")),
      ("/recommended_route", json!("review")),
    ],
  );
  let evidence_key = format!("validation-{}", Uuid::new_v4().simple());
  let mut evidence_node = fixture::node(
    &evidence_key,
    "trusted_action",
    &boolean["reference"],
    &boolean["reference"],
    "verified",
  );
  let evidence_plugin = choices.body["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["alias"] == "evidence")
    .unwrap()["reference"]["reference"]
    .clone();
  evidence_node["action"] = json!({"plugin":evidence_plugin,"action":"observe","parameters":{}});
  let mut draft = fixture::draft(
    &selected,
    vec![
      node,
      eligibility_gate,
      classification,
      classification_gate,
      evidence_node,
    ],
    json!([
      fixture::transition(&selected, "observed", "node", &eligibility_gate_key),
      fixture::transition(&eligibility_gate_key, "eligible", "node", &classification_key),
      fixture::transition(&eligibility_gate_key, "rejected", "terminal", "declined"),
      fixture::transition(&classification_key, "classified", "node", &classification_gate_key),
      fixture::transition(&classification_gate_key, "review", "node", &evidence_key),
      fixture::transition(&classification_gate_key, "rejected", "terminal", "declined"),
      fixture::transition(&evidence_key, "verified", "terminal", "returned"),
    ]),
    "returned",
    &boolean["reference"],
    vec![
      boolean.clone(),
      eligibility_contract.clone(),
      classification_contract.clone(),
    ],
  );
  draft["root"]["terminals"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"declined","schema":boolean["reference"]}));
  let id = Uuid::new_v4().to_string();
  let child = client
    .ok(
      "createFactoryFlowDefinition",
      &[],
      Some(json!({"id":id,"project_id":project,"definition":draft})),
      Some("child"),
      None,
    )
    .await;
  assert_eq!(child.body["resource"]["flow"]["closure"]["root"]["id"], id);
  let call_key = format!("call-{}", Uuid::new_v4().simple());
  let human_key = format!("approval-{}", Uuid::new_v4().simple());
  let request_key = format!("request-{}", Uuid::new_v4().simple());
  let mut call = fixture::node(
    &call_key,
    "subflow_call",
    &boolean["reference"],
    &boolean["reference"],
    "returned",
  );
  call["subflow"] = child.body["resource"]["flow"]["closure"]["root"].clone();
  call["outcomes"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"declined","kind":"failure","schema":boolean["reference"]}));
  let human = fixture::node(
    &human_key,
    "human_gate",
    &boolean["reference"],
    &boolean["reference"],
    "answered",
  );
  let mut request = fixture::node(
    &request_key,
    "human_gate",
    &boolean["reference"],
    &operation["reference"],
    "requested",
  );
  request["operator_requests"] = json!({"operation":"delivery","outcomes":{"confirm":"requested"}});
  let mut draft = fixture::draft(
    &call_key,
    vec![call, human, request],
    json!([
      fixture::transition(&call_key, "returned", "node", &human_key),
      fixture::transition(&call_key, "declined", "terminal", "declined"),
      fixture::transition(&human_key, "answered", "node", &request_key),
      fixture::transition(&request_key, "requested", "terminal", "finished"),
    ]),
    "finished",
    &operation["reference"],
    vec![
      boolean.clone(),
      operation.clone(),
      eligibility_contract.clone(),
      classification_contract.clone(),
    ],
  );
  draft["root"]["terminals"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"declined","schema":boolean["reference"]}));
  let parent = client
    .ok(
      "createFactoryFlowDefinition",
      &[],
      Some(json!({"id":Uuid::new_v4().to_string(),"project_id":project,"definition":draft})),
      Some("parent"),
      None,
    )
    .await;
  assert_eq!(
    parent.body["resource"]["flow"]["closure"]["definitions"]
      .as_array()
      .unwrap()
      .len(),
    2
  );
  let configuration = client.ok("createFactoryConfiguration",&[],Some(json!({"id":Uuid::new_v4().to_string(),"project_id":project,"definition":{
    "admission_policy":choice("admission_policy")["alias"],"permission_ceiling":choice("permission_ceiling")["alias"],
    "hard_budget":fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8},"enabled":true,"flow":parent.body["resource"]["flow"],
  }})),Some("configuration"),None).await;
  let repository = client
    .ok(
      "createRepository",
      &[],
      Some(json!({"project_id":project,"name":"source","definition":{
        "vcs_integration_id":Uuid::new_v4().to_string(),"repository_locator":"https://example.test/source.git",
        "selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true},
      }})),
      Some("repository"),
      None,
    )
    .await;
  let admitted = client.ok("admitManualFactoryWork",&[("project_id",project)],Some(json!({
    "configuration_id":configuration.body["resource"]["configuration"]["reference"]["id"],"configuration_version":1,
    "repository_id":repository.body["resource"]["id"],"repository_version":1,"external_identity":"headless/work",
    "source":{"kind":"exact_revision","value":"0123456789abcdef0123456789abcdef01234567"},
    "artifacts":{"task":{"artifact_id":Uuid::new_v4().to_string(),"content_digest":"ab".repeat(32),"encoded_size":1},
      "acceptance":{"artifact_id":Uuid::new_v4().to_string(),"content_digest":"cd".repeat(32),"encoded_size":1},"specifications":[]},
    "classification":{"priority":40,"risk":"low","metadata":{}},
  })),Some("work"),None).await;
  let run = admitted.body["factory_run_id"].as_str().unwrap();
  let human_attempt = client.wait_for_node(run, &human_key).await;
  let results = client
    .ok(
      "listFactoryDiagnostics",
      &[("factory_run_id", run), ("collection", "node-completions")],
      None,
      None,
      None,
    )
    .await;
  for contract in [&eligibility_contract, &classification_contract] {
    assert!(
      results.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["record"]["output_schema"] == contract["reference"]),
      "the nested journey must retain the exact typed eligibility and triage result contracts"
    );
  }
  let flows = client
    .ok(
      "listFactoryDiagnostics",
      &[("factory_run_id", run), ("collection", "flow-runs")],
      None,
      None,
      None,
    )
    .await;
  assert_eq!(flows.body["items"].as_array().unwrap().len(), 2);
  let cycles = client
    .ok(
      "listFactoryDiagnostics",
      &[("factory_run_id", run), ("collection", "workflow-cycles")],
      None,
      None,
      None,
    )
    .await;
  assert_eq!(cycles.body["items"].as_array().unwrap().len(), 2);
  let detail = client
    .ok("getFactoryRun", &[("factory_run_id", run)], None, None, None)
    .await;
  assert!(detail.headers.contains_key("etag"));
  let human_outcome = parent.body["resource"]["flow"]["closure"]["definitions"]
    .as_array()
    .unwrap()
    .iter()
    .flat_map(|definition| definition["nodes"].as_array().unwrap())
    .find(|node| node["key"] == human_attempt["node_key"])
    .unwrap()["outcomes"][0]["key"]
    .clone();
  client
    .ok(
      "submitFactoryNodeResponse",
      &[
        ("factory_run_id", run),
        ("node_attempt_id", human_attempt["id"].as_str().unwrap()),
      ],
      Some(json!({"outcome":human_outcome,"value":true})),
      Some("human-response"),
      Some(detail.headers["etag"].to_str().unwrap()),
    )
    .await;
  let request_attempt = client.wait_for_node(run, &request_key).await;
  let detail = client
    .ok("getFactoryRun", &[("factory_run_id", run)], None, None, None)
    .await;
  let etag = detail.headers["etag"].to_str().unwrap();
  let identities = [
    ("factory_run_id", run),
    ("node_attempt_id", request_attempt["id"].as_str().unwrap()),
  ];
  let intent = json!({"operation":"delivery","disposition":"confirm","reason":"request candidate review"});
  let accepted = client
    .ok(
      "confirmFactoryNodeOperation",
      &identities,
      Some(intent.clone()),
      Some("review-request"),
      Some(etag),
    )
    .await;
  let completed = tokio::time::timeout(std::time::Duration::from_secs(15), async {
    loop {
      let detail = client
        .ok("getFactoryRun", &[("factory_run_id", run)], None, None, None)
        .await;
      if detail.body["state"] == "completed" {
        break detail;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await
  .expect("the common owner completes the configured review request");
  let audits = client
    .ok(
      "listFactoryDiagnostics",
      &[("factory_run_id", run), ("collection", "audit")],
      None,
      None,
      None,
    )
    .await;
  let replay = client
    .ok(
      "confirmFactoryNodeOperation",
      &identities,
      Some(intent.clone()),
      Some("review-request"),
      Some(etag),
    )
    .await;
  assert_eq!(replay.body["disposition"], "replayed");
  assert_eq!(replay.body["version"], accepted.body["version"]);
  let after_replay = client
    .ok(
      "listFactoryDiagnostics",
      &[("factory_run_id", run), ("collection", "audit")],
      None,
      None,
      None,
    )
    .await;
  assert_eq!(after_replay.body, audits.body);
  let stale = client
    .call(
      "confirmFactoryNodeOperation",
      &identities,
      Some(intent),
      Some("stale-review"),
      Some(completed.headers["etag"].to_str().unwrap()),
    )
    .await;
  assert_eq!(stale.status, reqwest::StatusCode::CONFLICT);
  let evidence = client
    .ok(
      "listFactoryDiagnostics",
      &[("factory_run_id", run), ("collection", "evidence")],
      None,
      None,
      None,
    )
    .await;
  assert_eq!(
    evidence.body["items"].as_array().unwrap().len(),
    1,
    "verified evidence is visible without protected contents"
  );
  for collection in ["work", "calls", "budgets", "node-completions"] {
    let page = client
      .ok(
        "listFactoryDiagnostics",
        &[("factory_run_id", run), ("collection", collection)],
        None,
        None,
        None,
      )
      .await;
    assert!(!page.body["items"].as_array().unwrap().is_empty(), "{collection}");
  }
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}
