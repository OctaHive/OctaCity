//! Signal-only packaged Server/PostgreSQL qualification. Work artifact references
//! are deliberately unselected: retained inputs and real Agent execution belong
//! to the full pilot, while this slice exercises real connector dispatch only.
#![cfg(unix)]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_release_harness::InstalledToolchain as Toolchain;
use octacity_server_factory::*;
use reqwest::Client;
use serde_json::{Value, json};
use std::{
  env,
  fs::{self, File},
  net::{SocketAddr, TcpListener},
  path::{Path, PathBuf},
  process::Stdio,
  time::Duration,
};
use tokio::{
  process::{Child, Command},
  time::{Instant, sleep, timeout},
};
use uuid::Uuid;
#[allow(dead_code)]
#[path = "release_vertical_slice/backend.rs"]
mod backend;
#[allow(dead_code)]
#[path = "release_vertical_slice/environment.rs"]
mod environment;
#[path = "factory_pilot/jev.rs"]
mod jev;
#[allow(dead_code)]
#[path = "release_vertical_slice/release.rs"]
mod release;
#[allow(dead_code)]
#[path = "release_vertical_slice/server.rs"]
mod server;
#[path = "factory_pilot/signals.rs"]
mod signals;
#[allow(dead_code)]
#[path = "release_vertical_slice/support.rs"]
mod support;
use backend::ReleaseBackend;
use environment::*;
use server::*;
use support::{get_json, post_management, resource_id};
const SIGNING_SEED: [u8; 32] = [7; 32];
const RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS: u64 = 600_000;
const RELEASE_OBJECT_OPERATION_TIMEOUT_MILLISECONDS: u64 = 5_000;
const RELEASE_READINESS_TIMEOUT_MILLISECONDS: u64 = 15_000;
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn mapping(value: Value) -> FlowDataMapping {
  FlowDataMapping::new(FlowValueMapping::Constant { value }).unwrap()
}
struct PilotDatabase {
  admin: sqlx::PgPool,
  name: String,
  url: String,
}
impl PilotDatabase {
  async fn create() -> Self {
    let original = required_string("OCTACITY_POSTGRES_URL");
    let admin = sqlx::PgPool::connect(&original).await.unwrap();
    let name = format!("octacity_signal_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
      .execute(&admin)
      .await
      .unwrap();
    let mut url = url::Url::parse(&original).unwrap();
    url.set_path(&name);
    Self {
      admin,
      name,
      url: url.into(),
    }
  }
  async fn cleanup(self) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "DROP DATABASE \"{}\" WITH (FORCE)",
      self.name
    )))
    .execute(&self.admin)
    .await
    .unwrap();
    self.admin.close().await;
  }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires checksummed current Server, PostgreSQL and object storage"]
async fn packaged_server_keeps_the_baseline_and_frozen_shadow_receipt_after_restart() {
  exercise(jev::JevResponseMode::Success).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires checksummed current Server, PostgreSQL and object storage"]
async fn packaged_server_retains_unknown_jev_usage_after_restart_without_another_post() {
  exercise(jev::JevResponseMode::DropAfterAccept).await;
}
async fn exercise(mode: jev::JevResponseMode) {
  const CREDENTIAL: &str = "sk-glm5-pilot-private-signal-credential";
  let backend = ReleaseBackend::from_environment();
  let release = release::load(&backend);
  let database = PilotDatabase::create().await;
  let root = tempfile::tempdir().unwrap();
  let root = root.path().canonicalize().unwrap();
  let client = Client::new();
  let (management_addr, agent_addr) = unused_loopback_addresses();
  let origin = format!("http://{management_addr}");
  let object_endpoint = required_string("OCTACITY_MINIO_ENDPOINT");
  let input = ServerConfigInput {
    postgres_url: &database.url,
    object_endpoint: &object_endpoint,
    cache_endpoint: "https://cache.example",
    toolchain: &release.toolchain,
    management_addr,
    agent_addr,
    cache_addr: None,
  };
  let config = write_server_config(&root, &input);
  object_credentials(&root);
  let log = root.join("server.stdout.log");
  let errors = root.join("server.stderr.log");
  let mut process = spawn_server(&release.server_binary, &config, &log, &errors);
  wait_for_server_ready(&client, &origin, &mut process, &log).await;
  let project = post_management(
    &client,
    &origin,
    "/api/v1/projects",
    "project",
    json!({"parent_id":null,"name":"Signal-only public contract"}),
  )
  .await;
  let project = resource_id(&project);
  let repository=post_management(&client,&origin,"/api/v1/repositories","repository",json!({"project_id":project,"name":"Declared source","definition":{"vcs_integration_id":Uuid::new_v4(),"repository_locator":"https://pilot.invalid/unselected.git","selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}})).await;
  let repository = resource_id(&repository);
  shutdown_server(&mut process, &log).await;
  let schema = FlowDataSchema::new(key("public-state"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let choices = [key("a-baseline"), key("z-recommendation")];
  let plan = signals::plan(
    DecisionSignalPurpose::Routing,
    schema.clone(),
    mapping(json!(true)),
    &choices,
  );
  let fixture = match mode {
    jev::JevResponseMode::Success => {
      jev::JevHttpsFixture::start(plan.capability.model().version().as_str(), CREDENTIAL).await
    }
    jev::JevResponseMode::DropAfterAccept => {
      jev::JevHttpsFixture::start_with_mode(plan.capability.model().version().as_str(), CREDENTIAL, mode).await
    }
  };
  let operation = key(&format!("operation-{}", Uuid::new_v4().simple()));
  let outcome = key("observed");
  let binding = signals::routing_binding(
    project.parse().unwrap(),
    operation,
    outcome.clone(),
    plan,
    mapping(json!(true)),
    choices[0].clone(),
    NetworkHost::new("localhost").unwrap(),
  );
  let profiles=private_file(&root,"profiles.json",&json!({"version":1,"projects":[{"project_id":project,"references":[{"kind":"admission_policy","alias":"admission","reference":binding.plan.profile.policy()},{"kind":"permission_ceiling","alias":"ceiling","reference":binding.plan.profile.policy()},{"kind":"node_capability","alias":"connector-adapter","reference":binding.plan.capability.adapter()},{"kind":"secret_profile","alias":binding.credential_profile,"reference":binding.plan.profile.policy()}],"build_configurations":[]}],"codex":[],"signals":[signals::routing_entry(&binding)]}).to_string());
  let provider = private_file(
    &root,
    "jev.json",
    &fixture.connector_configuration(&binding.plan.capability).to_string(),
  );
  let credential = private_file(&root, "jev-key", CREDENTIAL);
  let connectors=private_file(&root,"connectors.json",&json!({"version":1,"connectors":[{"kind":"jev","credential_profile":binding.credential_profile,"configuration_file":provider,"credential_file":credential}]}).to_string());
  let config = write_factory_server_config(
    &root,
    &input,
    &FactoryServerFiles {
      profiles: &profiles,
      connectors: &connectors,
    },
  );
  object_credentials(&root);
  process = spawn_server(&release.server_binary, &config, &log, &errors);
  wait_for_server_ready(&client, &origin, &mut process, &log).await;
  let selected = format!("selected-{}", Uuid::new_v4().simple());
  let budget = json!({"max_attempts":4,"max_elapsed_millis":60000,"max_tokens":4000,"max_cost_micro_units":4000,"max_output_bytes":65536});
  let permissions = &binding.project_permissions;
  let definition = json!({"root":{"input_schema":schema.reference(),"entry":selected,"nodes":[{"key":selected,"kind":"decision_signal","input_schema":schema.reference(),"outcomes":[{"key":outcome,"kind":"success","schema":schema.reference()}],"budget":budget,"permissions":permissions,"required":true,"subflow":null,"action":binding.action,"input_binding":{"sources":{},"mapping":{"value":{"mapping":"constant","value":true}}}}],"transitions":[{"predecessor":selected,"outcome":outcome,"target":{"kind":"terminal","key":"done"},"max_repeats":0,"exhausted_terminal":null,"starts_cycle":false}],"terminals":[{"key":"done","schema":schema.reference()}],"context_projections":[],"data_projections":[],"execution":{"budget":budget,"permissions":permissions,"max_active_nodes":1}},"limits":{"max_depth":2,"max_expanded_nodes":4,"max_repeat_count":1,"max_fan_out":2,"max_wip":2,"budget":budget,"permissions":permissions},"data_schemas":[schema],"pools":{}});
  let flow = post_management(
    &client,
    &origin,
    "/api/v1/flow-definitions",
    "flow",
    json!({"id":Uuid::new_v4(),"project_id":project,"definition":definition}),
  )
  .await;
  let configuration=post_management(&client,&origin,"/api/v1/factory-configurations","configuration",json!({"id":Uuid::new_v4(),"project_id":project,"definition":{"admission_policy":"admission","permission_ceiling":"ceiling","hard_budget":budget,"wip_limits":{"max_active_runs":4,"max_active_nodes":4},"enabled":true,"flow":flow["resource"]["flow"]}})).await;
  let admitted=post_management(&client,&origin,&format!("/api/v1/projects/{project}/factory-work"),"work",json!({"configuration_id":configuration["resource"]["configuration"]["reference"]["id"],"configuration_version":1,"repository_id":repository,"repository_version":1,"external_identity":"signal-only/work","source":{"kind":"exact_revision","value":"0123456789abcdef0123456789abcdef01234567"},"artifacts":{"task":{"artifact_id":Uuid::new_v4(),"content_digest":"ab".repeat(32),"encoded_size":1},"acceptance":{"artifact_id":Uuid::new_v4(),"content_digest":"cd".repeat(32),"encoded_size":1},"specifications":[]},"classification":{"priority":40,"risk":"low","metadata":{"private-note":"unselected-private-work-canary"}}})).await;
  let run = admitted["factory_run_id"].as_str().unwrap();
  let pool = sqlx::PgPool::connect(&database.url).await.unwrap();
  timeout(Duration::from_secs(30), async {
    loop {
      let ready = client.get(format!("{origin}/health/factory")).send().await.unwrap();
      if matches!(mode, jev::JevResponseMode::Success) {
        if get_json(&client, format!("{origin}/api/v1/factory-runs/{run}")).await["state"] == "completed" {
          break;
        }
      } else if !ready.status().is_success() && fixture.captured().len() == 1 {
        break;
      }
      sleep(Duration::from_millis(25)).await;
    }
  })
  .await
  .expect("the actual installed connector must produce its observed outcome");
  let captured = fixture.captured();
  assert_eq!(captured.len(), 1);
  assert_eq!(captured[0].body["state"], true);
  assert!(!captured[0].body.to_string().contains("unselected-private-work-canary"));
  let requests:Vec<(Uuid,Vec<u8>,Vec<u8>)>=sqlx::query_as("SELECT request_id, request_digest, document FROM factory_decision_signal_requests WHERE run_id=$1 ORDER BY request_id").bind(run.parse::<Uuid>().unwrap()).fetch_all(&pool).await.unwrap();
  assert_eq!(requests.len(), 1);
  let request: DecisionSignalProviderRequest = serde_json::from_slice(&requests[0].2).unwrap();
  request.validate_integrity().unwrap();
  assert_eq!(request.digest().as_bytes().as_slice(), requests[0].1.as_slice());
  assert_eq!(
    captured[0].idempotency_key.as_deref(),
    Some(requests[0].0.to_string().as_str())
  );
  let receipts: Vec<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
    "SELECT receipt_digest,document FROM factory_decision_signal_receipts WHERE run_id=$1 ORDER BY request_id",
  )
  .bind(run.parse::<Uuid>().unwrap())
  .fetch_all(&pool)
  .await
  .unwrap();
  let before = get_json(&client, format!("{origin}/api/v1/factory-runs/{run}/decision-signals")).await;
  if matches!(mode, jev::JevResponseMode::Success) {
    assert_eq!(receipts.len(), 1);
    let receipt: DecisionSignalReceipt = serde_json::from_slice(&receipts[0].1).unwrap();
    receipt.validate_integrity().unwrap();
    assert_eq!(receipt.receipt_digest().as_bytes().as_slice(), receipts[0].0.as_slice());
    assert_eq!(receipt.consumption().kind(), DecisionSignalConsumptionKind::Shadow);
    assert_eq!(
      receipt.consumption().baseline(),
      &DecisionSignalDisposition::Route(choices[0].clone())
    );
    assert_eq!(
      receipt.consumption().recommendation(),
      Some(&DecisionSignalDisposition::Route(choices[1].clone()))
    );
    assert_eq!(
      receipt.consumption().final_disposition(),
      receipt.consumption().baseline()
    );
    assert_eq!(before["items"].as_array().unwrap().len(), 1);
  } else {
    assert!(
      receipts.is_empty(),
      "unknown provider usage must not produce a free terminal receipt"
    );
    assert!(before["items"].as_array().unwrap().is_empty());
  }
  shutdown_server(&mut process, &log).await;
  let previous_claims: i64 = sqlx::query_scalar("SELECT count(*) FROM factory_run_claims WHERE run_id=$1")
    .bind(run.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
  process = spawn_server(&release.server_binary, &config, &log, &errors);
  wait_for_server_ready(&client, &origin, &mut process, &log).await;
  if matches!(mode, jev::JevResponseMode::DropAfterAccept) {
    // Wait for persisted successor ownership rather than assuming a startup
    // delay proves the restarted worker has actually revisited this Run.
    timeout(Duration::from_secs(60), async {
      loop {
        let claims: i64 = sqlx::query_scalar("SELECT count(*) FROM factory_run_claims WHERE run_id=$1")
          .bind(run.parse::<Uuid>().unwrap())
          .fetch_one(&pool)
          .await
          .unwrap();
        if claims > previous_claims {
          break;
        }
        sleep(Duration::from_millis(25)).await;
      }
    })
    .await
    .expect("the restarted server must reacquire the unresolved Run");
  }
  let after = get_json(&client, format!("{origin}/api/v1/factory-runs/{run}/decision-signals")).await;
  assert_eq!(after, before);
  assert!(
    client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .unwrap()
      .status()
      .is_success(),
    "ordinary readiness remains independent"
  );
  shutdown_server(&mut process, &log).await;
  // Graceful shutdown drains the selected worker pass before comparing the
  // final durable state and POST count, including any in-flight observation.
  let restarted_requests:Vec<(Uuid,Vec<u8>,Vec<u8>)>=sqlx::query_as("SELECT request_id,request_digest,document FROM factory_decision_signal_requests WHERE run_id=$1 ORDER BY request_id").bind(run.parse::<Uuid>().unwrap()).fetch_all(&pool).await.unwrap();
  assert_eq!(restarted_requests, requests);
  let restarted_receipts: Vec<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
    "SELECT receipt_digest,document FROM factory_decision_signal_receipts WHERE run_id=$1 ORDER BY request_id",
  )
  .bind(run.parse::<Uuid>().unwrap())
  .fetch_all(&pool)
  .await
  .unwrap();
  assert_eq!(restarted_receipts, receipts);
  assert_eq!(
    fixture.captured().len(),
    1,
    "server restart must neither replay a receipt nor redispatch unknown usage"
  );
  pool.close().await;
  fixture.shutdown().await;
  database.cleanup().await;
}
fn object_credentials(root: &Path) {
  private_file(root, "object-access-key", &required_string("OCTACITY_MINIO_ACCESS_KEY"));
  private_file(root, "object-secret-key", &required_string("OCTACITY_MINIO_SECRET_KEY"));
}
