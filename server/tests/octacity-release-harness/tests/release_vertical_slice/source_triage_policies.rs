//! Policy qualification around the ordinary packaged Source and Agent journey.
use super::*;
use octacity_server_store::{BuildQueryStore, FactoryRunStore};

pub(super) struct Matrix {
  pub(super) automatic: Value,
  manual: Value,
  legacy: Value,
  legacy_details: Value,
  launch: Value,
  access: Value,
  max_descendants: u64,
}

impl Matrix {
  pub(super) async fn prepare(execution: &ExecutionEnvironment, installed: &Value) -> Self {
    let operation = |purpose: &str| format!("{}-{purpose}", execution.run);
    let project = &execution.resources.project;
    let origin = &execution.environment.origin;
    let client = &execution.environment.client;
    let legacy = installed["target"].clone();
    let legacy_details = get_json(client, configuration_url(origin, &legacy)).await;
    let mut definition = legacy_details["configuration"].clone();
    definition.as_object_mut().unwrap().remove("reference");

    let policies = format!("/api/v1/projects/{project}/factory-policies");
    let standard = &installed["policy_setup"]["provisioning"]["standard_access"];
    let mut permissions = get_json(
      client,
      format!(
        "{origin}{policies}/access/{}/versions/{}?digest={}",
        string(standard, "id"),
        standard["version"],
        string(standard, "digest")
      ),
    )
    .await["policy"]["policy"]["definition"]["permissions"]
      .clone();
    // Leave the required model and MCP hosts in scope. Tighten a grant that
    // the selected Flow, Profiles and installed Agent otherwise share.
    let max_descendants = required_u64("OCTACITY_TEST_TRIAGE_PROCESS_LIMIT")
      .checked_sub(1)
      .unwrap();
    assert!(permissions["max_descendants"].as_u64().unwrap() > max_descendants);
    let eligibility = definition["flow"]["closure"]["definitions"][0]["nodes"]
      .as_array()
      .unwrap()
      .iter()
      .find(|node| node["key"] == "eligibility")
      .unwrap();
    assert!(
      eligibility["permissions"]["max_descendants"].as_u64().unwrap() > max_descendants,
      "the ordinary node must otherwise request the wider limit"
    );
    permissions["max_descendants"] = json!(max_descendants);
    let access = post_management(
      client,
      origin,
      &policies,
      &operation("qualified-narrow-access"),
      json!({
        "kind":"access","id":Uuid::new_v4(),"definition":{"name":"Restricted triage","permissions":permissions}
      }),
    )
    .await["policy"]["policy"]["reference"]
      .clone();

    let launch = post_management(client, origin, &policies, &operation("qualified-automatic"), json!({
      "kind":"launch","id":Uuid::new_v4(),"definition":{"name":"Automatic only","allow_manual":false,"allow_automatic_source":true}
    })).await["policy"]["policy"]["reference"].clone();
    let manual_launch = post_management(client, origin, &policies, &operation("qualified-manual"), json!({
      "kind":"launch","id":Uuid::new_v4(),"definition":{"name":"Manual only","allow_manual":true,"allow_automatic_source":false}
    })).await["policy"]["policy"]["reference"].clone();
    definition["permission_ceiling"] = json!({"project_policy":access});
    definition["admission_policy"] = json!({"project_policy":manual_launch});
    let manual = publish_configuration(execution, &operation("qualified-manual-target"), &definition).await;
    definition["admission_policy"] = json!({"project_policy":launch});
    let automatic = publish_configuration(execution, &operation("qualified-automatic-target"), &definition).await;

    Self {
      automatic,
      manual,
      legacy,
      legacy_details,
      launch,
      access,
      max_descendants,
    }
  }

  pub(super) async fn verify_denials(&self, execution: &ExecutionEnvironment, intent: &Value) {
    let client = &execution.environment.client;
    let origin = &execution.environment.origin;
    let operation = |purpose: &str| format!("{}-{purpose}", execution.run);
    let project = &execution.resources.project;
    let mut denied_source = intent.clone();
    let id = Uuid::new_v4().to_string();
    denied_source["inputs"][0]["id"] = json!(id);
    denied_source["inputs"][0]["definition"]["target"] = self.manual.clone();
    let path = format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions");
    source_setup::publish(
      client,
      origin,
      &path,
      &operation("manual-only-source"),
      "if-none-match",
      "*",
      json!({"intent":denied_source}),
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
      let observed = get_json(client, format!("{origin}{path}/1/diagnostics")).await;
      assert_eq!(observed["admissions"], json!([]));
      if observed["scan"]["schedule"]["failure"] == "policy_denied" {
        break;
      }
      assert!(
        Instant::now() < deadline,
        "Source denial must be durable and specific: {observed}"
      );
      sleep(Duration::from_millis(100)).await;
    }
    denied_source["inputs"][0]["expected_version"] = json!(1);
    denied_source["inputs"][0]["definition"]["enabled"] = json!(false);
    source_setup::publish(
      client,
      origin,
      &path,
      &operation("stop-denied-source"),
      "if-match",
      "\"1\"",
      json!({"intent":denied_source}),
    )
    .await;

    let manual_denied = operation("manual-denied");
    let body = manual_body(execution, &self.automatic, &manual_denied).await;
    let response = client
      .post(format!("{origin}/api/v1/projects/{project}/factory-work"))
      .header("idempotency-key", manual_denied)
      .json(&body)
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let rejection = response.json::<Value>().await.unwrap();
    assert_eq!(
      rejection["diagnostics"]["errors"][0]["code"], "launch_policy_denied",
      "{rejection}"
    );
    for collection in ["factory-runs", "builds"] {
      let page = get_json(
        client,
        format!("{origin}/api/v1/projects/{project}/{collection}?limit=100"),
      )
      .await;
      assert_eq!(page["items"], json!([]), "denial creates no {collection}");
    }
  }

  pub(super) async fn verify_restart(
    &self,
    execution: &mut ExecutionEnvironment,
    run: &str,
    admissions_path: &str,
    diagnostics: &serde_json::Map<String, Value>,
  ) -> Value {
    let client = execution.environment.client.clone();
    let origin = execution.environment.origin.clone();
    let operation = |purpose: &str| format!("{}-{purpose}", execution.run);
    let project = &execution.resources.project;
    let db = sqlx::PgPool::connect(&required_string("OCTACITY_POSTGRES_URL"))
      .await
      .unwrap();
    let store = octacity_server_store_postgres::PostgresStore::new(db.clone());
    let run_id = run.parse().unwrap();
    let accepted = store.factory_run_snapshot(run_id).await.unwrap();
    let access = accepted.admitted_flow.access_profile().unwrap();
    assert_eq!(json!(access.reference()), self.access);
    assert_eq!(
      json!(access.definition().permissions)["max_descendants"],
      self.max_descendants
    );
    let builds = diagnostics["builds"]["items"].as_array().unwrap();
    assert_eq!(builds.len(), 2);
    for build in builds {
      let job = build["record"]["jobs"][0].as_str().unwrap().parse().unwrap();
      let template = store.job(job).await.unwrap().job.job_spec_template;
      let permissions = json!(template.managed_intent().unwrap().permissions());
      assert_eq!(permissions["max_descendants"], self.max_descendants);
      assert_eq!(permissions["network_hosts"], json!(["host.microsandbox.internal"]));
    }

    let automatic_before = retained(&client, &origin, run).await;
    let admissions = get_json(&client, admissions_path.to_owned()).await;
    assert_eq!(admissions["admissions"].as_array().unwrap().len(), 1);
    let manual_key = operation("manual-accepted");
    let manual_request = manual_body(execution, &self.manual, &manual_key).await;
    let path = format!("/api/v1/projects/{project}/factory-work");
    let manual = post_management(&client, &origin, &path, manual_key.as_str(), manual_request.clone()).await;
    let legacy_key = operation("legacy-accepted");
    let legacy_body = manual_body(execution, &self.legacy, &legacy_key).await;
    let legacy = post_management(&client, &origin, &path, legacy_key.as_str(), legacy_body.clone()).await;
    let legacy_run = string(&legacy, "factory_run_id");
    let legacy_before = get_json(&client, format!("{origin}/api/v1/factory-runs/{legacy_run}/work")).await;
    let legacy_snapshot = store.factory_run_snapshot(legacy_run.parse().unwrap()).await.unwrap();

    let policies = format!("/api/v1/projects/{project}/factory-policies");
    let replacement = post_management(
      &client,
      &origin,
      &policies,
      &operation("qualified-replace"),
      json!({
        "kind":"launch","id":self.launch["id"],"expected_current":self.launch,
        "definition":{"name":"Closed","allow_manual":false,"allow_automatic_source":false}
      }),
    )
    .await["policy"]["policy"]["reference"]
      .clone();
    assert_eq!(replacement["version"], 2);
    let replacement_access = post_management(
      &client,
      &origin,
      &policies,
      &operation("qualified-replace-access"),
      json!({
        "kind":"access","id":self.access["id"],"expected_current":self.access,
        "definition":{"name":"Closed access","permissions":octacity_server_factory::FactoryPermissionSet::deny_all()}
      }),
    )
    .await["policy"]["policy"]["reference"]
      .clone();
    assert_eq!(replacement_access["version"], 2);
    let defaults_path = format!("{policies}/defaults");
    let defaults = post_management(
      &client,
      &origin,
      &defaults_path,
      &operation("qualified-defaults"),
      json!({
        "expected_version":1,"launch_policy":replacement,"access_profile":replacement_access
      }),
    )
    .await;
    execution.environment.restart().await;

    assert_eq!(
      get_json(&client, format!("{origin}{defaults_path}")).await["defaults"],
      defaults["defaults"]
    );
    assert_eq!(
      get_json(&client, configuration_url(&origin, &self.legacy)).await,
      self.legacy_details
    );
    let automatic = get_json(&client, configuration_url(&origin, &self.automatic)).await;
    assert_eq!(
      automatic["configuration"]["admission_policy"],
      json!({"project_policy":self.launch})
    );
    assert_eq!(
      automatic["configuration"]["permission_ceiling"],
      json!({"project_policy":self.access})
    );
    assert_eq!(
      store.factory_run_snapshot(run_id).await.unwrap().admitted_flow,
      accepted.admitted_flow
    );
    assert_eq!(
      store
        .factory_run_snapshot(legacy_run.parse().unwrap())
        .await
        .unwrap()
        .admitted_flow,
      legacy_snapshot.admitted_flow
    );
    assert_eq!(
      get_json(&client, format!("{origin}/api/v1/factory-runs/{legacy_run}/work")).await,
      legacy_before
    );
    assert_eq!(retained(&client, &origin, run).await, automatic_before);
    let observed = get_json(&client, admissions_path.to_owned()).await;
    assert_eq!(observed["admissions"], admissions["admissions"]);
    for (key, body, original) in [
      (manual_key.as_str(), manual_request, manual),
      (legacy_key.as_str(), legacy_body, legacy),
    ] {
      let replay = post_management(&client, &origin, &path, key, body).await;
      assert_eq!(replay["disposition"], "replayed");
      for field in ["work_envelope_id", "factory_run_id"] {
        assert_eq!(replay[field], original[field]);
      }
    }
    let runs = get_json(
      &client,
      format!("{origin}/api/v1/projects/{project}/factory-runs?limit=100"),
    )
    .await;
    assert_eq!(runs["items"].as_array().unwrap().len(), 3);
    db.close().await;
    json!({"automatic":self.automatic,"manual":self.manual,"legacy":self.legacy,"launch_replacement":replacement,
      "access":self.access,"access_replacement":replacement_access,"accepted_runs":3,"automatic_results":automatic_before,"legacy_work":legacy_before})
  }
}

async fn publish_configuration(execution: &ExecutionEnvironment, key: &str, definition: &Value) -> Value {
  post_management(
    &execution.environment.client,
    &execution.environment.origin,
    "/api/v1/factory-configurations",
    key,
    json!({"id":Uuid::new_v4(),"project_id":execution.resources.project,"definition":definition}),
  )
  .await["resource"]["configuration"]["reference"]
    .clone()
}

fn configuration_url(origin: &str, reference: &Value) -> String {
  format!(
    "{origin}/api/v1/factory-configurations/{}/versions/{}",
    string(reference, "id"),
    reference["version"]
  )
}

async fn manual_body(execution: &ExecutionEnvironment, target: &Value, key: &str) -> Value {
  let client = &execution.environment.client;
  let origin = &execution.environment.origin;
  let project = &execution.resources.project;
  let path = format!("/api/v1/projects/{project}/input-documents");
  let task = post_management(
    client,
    origin,
    &path,
    key,
    json!({"name":key,"content":"Assess the requested qualification ticket."}),
  )
  .await;
  let acceptance_key = format!("{key}-acceptance");
  let acceptance = post_management(
    client,
    origin,
    &path,
    &acceptance_key,
    json!({"name":acceptance_key,"content":"Retain the exact qualification result."}),
  )
  .await;
  json!({"configuration_id":target["id"],"configuration_version":target["version"],
    "repository_id":execution.resources.repository,"repository_version":1,"external_identity":key,
    "source":{"kind":"exact_revision","value":execution.revision},
    "artifacts":{"task":task["artifact"],"acceptance":acceptance["artifact"],"specifications":[]},
    "classification":{"priority":40,"risk":"low","metadata":{}}})
}

async fn retained(client: &Client, origin: &str, run: &str) -> Value {
  let mut evidence = serde_json::Map::new();
  for collection in ["work", "node-completions", "builds", "calls"] {
    let page = get_json(
      client,
      format!("{origin}/api/v1/factory-runs/{run}/{collection}?limit=100"),
    )
    .await;
    assert!(page["next_cursor"].is_null());
    evidence.insert(collection.into(), page["items"].clone());
  }
  json!(evidence)
}
