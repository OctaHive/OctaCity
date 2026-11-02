//! Authoritative launch rules and accepted receipt recovery through real HTTP and PostgreSQL.
use super::*;
use octacity_server_application::*;
use octacity_server_factory::*;
use std::sync::atomic::AtomicUsize;

struct Executor(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl FactoryNodeExecutor for Executor {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    Ok(FactoryNodeCancellationStep::Stopped {
      execution: None,
      usage: BudgetUsage::default(),
    })
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    self.0.fetch_add(1, Ordering::SeqCst);
    let definition = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .unwrap();
    let outcome = &definition.node(request.input.node()).unwrap().outcomes()[0];
    let payload = FlowPayload::new(request.admitted.data_schema(outcome.schema()).unwrap(), json!(true)).unwrap();
    let record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: outcome.key().clone(),
        payload,
        producer: request.attempt.execution().clone(),
        observed_at: request.observed_at,
      },
    )
    .unwrap();
    Ok(FactoryNodeExecutionStep::Completed {
      execution: None,
      record: Box::new(record),
      usage: BudgetUsage::default(),
      outbox: vec![],
    })
  }
}
struct Installation(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl octacity_server::FactoryRuntimeInstaller for Installation {
  async fn install(
    &self,
    resources: octacity_server::FactoryRuntimeResources,
  ) -> Result<octacity_server::FactoryRuntimeInstallation, octacity_server::FactoryRuntimeInstallationError> {
    let mut installed =
      octacity_server::FactoryRuntimeInstaller::install(&installed_fixture::Installation, resources).await?;
    installed.executors.retain(|(_, identity, _)| {
      *identity
        == NodeExecutionIdentity::External(ImmutableReference::new(
          "fixture.observer".parse().unwrap(),
          "v1".parse().unwrap(),
          FactoryDigest::from_bytes([3; 32]),
        ))
    });
    installed.executors[0].2 = Arc::new(Executor(self.0.clone()));
    Ok(installed)
  }
}
async fn request(f: &Fixture, suffix: &str, key: &str, body: &Value, expected: StatusCode) -> Value {
  let response = f
    .client
    .post(format!("http://{}/api/v1{suffix}", f.runtime.management_addr()))
    .header("idempotency-key", key)
    .json(body)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let value = response.json::<Value>().await.unwrap();
  assert_eq!(status, expected, "{value}");
  value
}
async fn runs(f: &Fixture) -> Value {
  f.client
    .get(format!(
      "http://{}/api/v1/projects/{}/factory-runs",
      f.runtime.management_addr(),
      f.project
    ))
    .send()
    .await
    .unwrap()
    .json::<Value>()
    .await
    .unwrap()
}
async fn wait_completed(f: &Fixture, id: &str) {
  let mut last = Value::Null;
  let result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
    loop {
      let page = runs(f).await;
      last = page.clone();
      if page["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == id && r["state"] == "completed")
      {
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await;
  assert!(
    result.is_ok(),
    "the admitted operation must complete before restart/replay: {last}"
  );
}
#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn manual_policy_denial_and_accepted_replay_survive_policy_replacement_and_server_restart() {
  let calls = Arc::new(AtomicUsize::new(0));
  let installer = Arc::new(Installation(calls.clone()));
  let f = Fixture::with_installer(Some(installer.clone())).await;
  let (_, access) = super::configuration::policies(&f).await;
  let launch_id = uuid::Uuid::new_v4().to_string();
  let denied = f.post("", "automatic-only", &json!({"kind":"launch","id":launch_id,"definition":{"name":"Automatic only","allow_manual":false,"allow_automatic_source":true}}), StatusCode::OK).await;
  let flow = super::configuration::flow(&f).await;
  let configuration_id = uuid::Uuid::new_v4().to_string();
  let candidate = json!({"id":configuration_id,"project_id":f.project,"definition":{
    "admission_policy":{"project_policy":denied["policy"]["policy"]["reference"]},"permission_ceiling":{"project_policy":access},
    "hard_budget":installed_fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8},"enabled":true,"flow":flow}});
  let configuration = request(
    &f,
    "/factory-configurations",
    "automatic-target",
    &candidate,
    StatusCode::CREATED,
  )
  .await;
  let repository = request(
    &f,
    "/repositories",
    "repository",
    &json!({"project_id":f.project,"name":"Source","definition":{
    "vcs_integration_id":uuid::Uuid::new_v4().to_string(),"repository_locator":"https://example.test/source.git",
    "selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}}),
    StatusCode::CREATED,
  )
  .await;
  let mut body = json!({"configuration_id":configuration["resource"]["configuration"]["reference"]["id"],"configuration_version":1,
    "repository_id":repository["resource"]["id"],"repository_version":1,"external_identity":"manual/retained",
    "source":{"kind":"exact_revision","value":"0123456789abcdef0123456789abcdef01234567"},
    "artifacts":{"task":{"artifact_id":uuid::Uuid::new_v4().to_string(),"content_digest":"ab".repeat(32),"encoded_size":1},"acceptance":{"artifact_id":uuid::Uuid::new_v4().to_string(),"content_digest":"cd".repeat(32),"encoded_size":1},"specifications":[]},
    "classification":{"priority":40,"risk":"low","metadata":{"origin":"automatic_source"}}});
  let path = format!("/projects/{}/factory-work", f.project);
  let failure = request(&f, &path, "same-admission-key", &body, StatusCode::BAD_REQUEST).await;
  assert_eq!(failure["diagnostics"]["errors"][0]["code"], "launch_policy_denied");
  assert!(runs(&f).await["items"].as_array().unwrap().is_empty());
  assert_eq!(calls.load(Ordering::SeqCst), 0);
  let builds = f
    .client
    .get(format!(
      "http://{}/api/v1/projects/{}/builds",
      f.runtime.management_addr(),
      f.project
    ))
    .send()
    .await
    .unwrap()
    .json::<Value>()
    .await
    .unwrap();
  assert!(
    builds["items"].as_array().is_some_and(Vec::is_empty),
    "denied admission must create no Build: {builds}"
  );
  openapi_contract::assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "FactoryValidationDiagnostics",
    &failure["diagnostics"],
  );
  let allowed = f.post("", "manual-version", &json!({"kind":"launch","id":launch_id,"expected_current":denied["policy"]["policy"]["reference"],"definition":{"name":"Manual reviewed","allow_manual":true,"allow_automatic_source":false}}),StatusCode::OK).await;
  let mut candidate = candidate;
  candidate["id"] = json!(uuid::Uuid::new_v4().to_string());
  candidate["definition"]["admission_policy"] = json!({"project_policy":allowed["policy"]["policy"]["reference"]});
  let allowed_configuration = request(
    &f,
    "/factory-configurations",
    "manual-target",
    &candidate,
    StatusCode::CREATED,
  )
  .await;
  body["configuration_id"] = allowed_configuration["resource"]["configuration"]["reference"]["id"].clone();
  let mut disabled = candidate.clone();
  disabled["id"] = json!(uuid::Uuid::new_v4().to_string());
  disabled["definition"]["enabled"] = json!(false);
  let disabled = request(
    &f,
    "/factory-configurations",
    "disabled-target",
    &disabled,
    StatusCode::CREATED,
  )
  .await;
  let mut disabled_work = body.clone();
  disabled_work["configuration_id"] = disabled["resource"]["configuration"]["reference"]["id"].clone();
  request(&f, &path, "disabled-work", &disabled_work, StatusCode::BAD_REQUEST).await;
  assert!(
    runs(&f).await["items"].as_array().unwrap().is_empty(),
    "manual permission cannot enable a disabled Configuration"
  );
  let accepted = request(&f, &path, "same-admission-key", &body, StatusCode::CREATED).await;
  let run = accepted["factory_run_id"].as_str().unwrap();
  wait_completed(&f, run).await;
  assert_eq!(calls.load(Ordering::SeqCst), 1);
  let snapshot = octacity_server_store::FactoryRunStore::factory_run_snapshot(
    &octacity_server_store_postgres::PostgresStore::new(f.database.pool.clone()),
    run.parse().unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(
    serde_json::to_value(snapshot.admitted_flow.access_profile().unwrap().reference()).unwrap(),
    access
  );
  let replaced = f.post("", "deny-new-version", &json!({"kind":"launch","id":launch_id,"expected_current":allowed["policy"]["policy"]["reference"],"definition":{"name":"Closed","allow_manual":false,"allow_automatic_source":false}}), StatusCode::OK).await;
  let newer_access = f
    .post(
      "",
      "replace-access-after-admission",
      &json!({
        "kind":"access", "id":access["id"], "expected_current":access,
        "definition":{"name":"New default after admission", "permissions":super::configuration::host_permissions(&[])}
      }),
      StatusCode::OK,
    )
    .await;
  f.post(
    "/defaults",
    "changed-defaults",
    &json!({"expected_version":null,"launch_policy":replaced["policy"]["policy"]["reference"],"access_profile":newer_access["policy"]["policy"]["reference"]}),
    StatusCode::OK,
  )
  .await;
  let before = runs(&f).await;
  let f = f.restart_with_installer(Some(installer)).await;
  let replay = request(&f, &path, "same-admission-key", &body, StatusCode::CREATED).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["work_envelope_id"], accepted["work_envelope_id"]);
  assert_eq!(replay["factory_run_id"], accepted["factory_run_id"]);
  assert_eq!(runs(&f).await, before);
  let restored = octacity_server_store::FactoryRunStore::factory_run_snapshot(
    &octacity_server_store_postgres::PostgresStore::new(f.database.pool.clone()),
    run.parse().unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(restored.admitted_flow, snapshot.admitted_flow);
  assert_eq!(calls.load(Ordering::SeqCst), 1);
  f.stop().await;
}
