//! Released Linux Native end-to-end acceptance matrix.

use std::{
  collections::BTreeSet,
  ffi::OsString,
  fs,
  net::{SocketAddr, TcpListener},
  path::Path,
  time::Duration,
};

use chrono::{Datelike as _, Timelike as _, Utc};
use octacity_release_harness::InstalledRelease;
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tempfile::TempDir;
use tokio::{
  process::Child,
  time::{Instant, sleep, timeout},
};
use uuid::Uuid;

use super::{
  AgentConfigOverrides, AgentReleasePaths, ReleaseBackend, ServerConfigInput, drain_agent, get_json, issue_enrollment,
  post_management, private_file, release, required_path, required_string, resource_id, set_mode, shutdown_server,
  spawn_agent, spawn_server, string, wait_for_server_ready, wait_for_terminal_build, write_agent_config,
  write_server_config,
};

#[path = "cache_proxy.rs"]
mod cache_proxy;
mod checks;

use cache_proxy::TlsCacheProxy;
use checks::*;

const FIXTURE_OCTAFILE: &str = "fixtures/release/linux-native/Octafile.yml";
const CACHE_NAMESPACE: &str = "release-linux-native";
const ARTIFACT_CONTENT: &[u8] = b"OctaCity released Linux Native artifact\n";
const MEMORY_BYTES: u64 = 512 * 1024 * 1024;
const OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

struct MatrixResources {
  pool_id: String,
  project_id: String,
  manual_configuration_id: String,
  scheduled_configuration_id: String,
  cancellation_configuration_id: String,
  manual_trigger_id: String,
  cancellation_trigger_id: String,
  internal_trigger_id: String,
}

struct BuildRun {
  build: Value,
  attempt: Value,
  jobs: Vec<Value>,
}

struct MatrixAgent {
  child: Child,
  cache_directory: Option<TempDir>,
}

struct AgentStartInput<'a> {
  client: &'a Client,
  management_origin: &'a str,
  agent_origin: &'a str,
  temporary: &'a Path,
  evidence: &'a Path,
  evidence_name: &'a str,
  agent_name: &'a str,
  run: &'a str,
  pool_id: &'a str,
  release: &'a InstalledRelease,
  backend: &'a ReleaseBackend,
  cache_proxy: &'a TlsCacheProxy,
  object_endpoint: &'a str,
}

struct CancellationInput<'a> {
  client: &'a Client,
  origin: &'a str,
  run: &'a str,
  trigger_id: &'a str,
  configuration: &'a str,
  revision: &'a str,
  stderr: &'a Path,
  cgroup_root: &'a Path,
  cgroup_baseline: &'a BTreeSet<OsString>,
  maximum_disk_bytes: u64,
}

struct ConfigurationInput<'a> {
  client: &'a Client,
  origin: &'a str,
  run: &'a str,
  name: &'a str,
  project_id: &'a str,
  repository_id: &'a str,
  pipeline_id: &'a str,
  pool_id: &'a str,
  triggers: &'a [&'a str],
  cache: bool,
  backend: &'a ReleaseBackend,
}

impl MatrixAgent {
  fn child_mut(&mut self) -> &mut Child {
    &mut self.child
  }

  fn cleanup_cache(&mut self) {
    self
      .cache_directory
      .take()
      .expect("matrix Agent must own its cache directory")
      .close()
      .expect("matrix Agent cache directory must be removable");
  }
}

pub(super) async fn run() {
  let backend = ReleaseBackend::from_environment();
  assert!(matches!(backend, ReleaseBackend::Native { .. }));
  let postgres_url = required_string("OCTACITY_POSTGRES_URL");
  let object_endpoint = required_string("OCTACITY_MINIO_ENDPOINT");
  let source_repository = required_string("OCTACITY_RELEASE_SOURCE_REPOSITORY");
  let evidence = required_path("OCTACITY_RELEASE_EVIDENCE_DIR", false);
  fs::create_dir_all(&evidence).unwrap();
  let release = release::load(&backend);
  let revision = release.server_manifest.octacity_revision().to_owned();
  let temporary = tempfile::tempdir().unwrap();
  let pool = PgPoolOptions::new()
    .max_connections(4)
    .connect(&postgres_url)
    .await
    .unwrap();

  let (work_root, cgroup_root) = native_roots(&backend);
  let cache_root = native_cache_root(&backend);
  let work_baseline = directory_entries(work_root);
  let cgroup_baseline = directory_entries(cgroup_root);
  let cache_baseline = directory_entries(cache_root);
  let management_addr = unused_loopback_address();
  let agent_addr = unused_loopback_address();
  let cache_addr = unused_loopback_address();
  let cache_proxy = TlsCacheProxy::start(cache_addr, temporary.path()).await;
  let server_config = write_server_config(
    temporary.path(),
    &ServerConfigInput {
      postgres_url: &postgres_url,
      object_endpoint: &object_endpoint,
      cache_endpoint: &cache_proxy.origin,
      toolchain: &release.toolchain,
      management_addr,
      agent_addr,
      cache_addr: Some(cache_addr),
    },
  );
  let server_stdout = evidence.join("server.stdout.log");
  let server_stderr = evidence.join("server.stderr.log");
  let mut server = spawn_server(&release.server_binary, &server_config, &server_stdout, &server_stderr);
  let client = Client::new();
  let management_origin = format!("http://{management_addr}");
  let agent_origin = format!("http://{agent_addr}");
  wait_for_server_ready(&client, &management_origin, &mut server, &server_stderr).await;
  let run_id = Uuid::new_v4().simple().to_string();
  let resources = create_resources(&client, &management_origin, &run_id, &source_repository, &backend).await;

  let agent_a_name = format!("release-native-a-{run_id}");
  let mut agent_a = start_matrix_agent(AgentStartInput {
    client: &client,
    management_origin: &management_origin,
    agent_origin: &agent_origin,
    temporary: temporary.path(),
    evidence: &evidence,
    evidence_name: "agent-a",
    agent_name: &agent_a_name,
    run: &run_id,
    pool_id: &resources.pool_id,
    release: &release,
    backend: &backend,
    cache_proxy: &cache_proxy,
    object_endpoint: &object_endpoint,
  })
  .await;

  let manual = accept_manual_build(
    &client,
    &management_origin,
    &run_id,
    &resources.manual_trigger_id,
    &resources.manual_configuration_id,
    &revision,
  )
  .await;
  let replay = accept_manual_build(
    &client,
    &management_origin,
    &run_id,
    &resources.manual_trigger_id,
    &resources.manual_configuration_id,
    &revision,
  )
  .await;
  assert_eq!(manual["build_id"], replay["build_id"]);
  assert_eq!(manual["trigger_occurrence_id"], replay["trigger_occurrence_id"]);
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(occurrence_count(&pool, &resources.manual_trigger_id).await, 1);

  let manual_run = wait_for_successful_run(
    &client,
    &management_origin,
    &string(&manual, "build_id"),
    agent_a.child_mut(),
    &evidence.join("agent-a.stderr.log"),
  )
  .await;
  assert_dag_and_events(&manual_run, Some(backend.workspace_bytes()));
  let downstream_build_id = wait_for_trigger_build(&pool, &resources.internal_trigger_id).await;
  let downstream_run = wait_for_successful_run(
    &client,
    &management_origin,
    &downstream_build_id,
    agent_a.child_mut(),
    &evidence.join("agent-a.stderr.log"),
  )
  .await;
  assert_internal_causality(&manual, &manual_run.build, &downstream_run.build);
  assert_successful_job_events(&downstream_run);
  assert_eq!(occurrence_count(&pool, &resources.internal_trigger_id).await, 1);

  let artifacts = verify_artifacts(&client, &management_origin, &manual_run.build).await;
  let searches = verify_log_search(&client, &management_origin, &resources.project_id, &manual_run.build).await;
  stop_agent(
    &client,
    &management_origin,
    &agent_a_name,
    &run_id,
    &mut agent_a,
    &evidence.join("agent-a.stderr.log"),
  )
  .await;

  let agent_b_name = format!("release-native-b-{run_id}");
  let mut agent_b = start_matrix_agent(AgentStartInput {
    client: &client,
    management_origin: &management_origin,
    agent_origin: &agent_origin,
    temporary: temporary.path(),
    evidence: &evidence,
    evidence_name: "agent-b",
    agent_name: &agent_b_name,
    run: &run_id,
    pool_id: &resources.pool_id,
    release: &release,
    backend: &backend,
    cache_proxy: &cache_proxy,
    object_endpoint: &object_endpoint,
  })
  .await;
  let schedule = create_schedule(
    &client,
    &management_origin,
    &run_id,
    &resources.scheduled_configuration_id,
    &revision,
  )
  .await;
  let scheduled_trigger_id = resource_id(&schedule);
  let scheduled_build_id = wait_for_trigger_build(&pool, &scheduled_trigger_id).await;
  let scheduled_run = wait_for_successful_run(
    &client,
    &management_origin,
    &scheduled_build_id,
    agent_b.child_mut(),
    &evidence.join("agent-b.stderr.log"),
  )
  .await;
  assert_eq!(scheduled_run.build["trigger"]["kind"], "scheduled");
  assert_successful_job_events(&scheduled_run);
  assert_eq!(occurrence_count(&pool, &scheduled_trigger_id).await, 1);
  assert!(
    scheduled_run.jobs.iter().any(has_cache_hit),
    "the second Agent had an empty L1, so the scheduled Build must restore from remote L2"
  );

  let cancelled_run = run_and_cancel(
    CancellationInput {
      client: &client,
      origin: &management_origin,
      run: &run_id,
      trigger_id: &resources.cancellation_trigger_id,
      configuration: &resources.cancellation_configuration_id,
      revision: &revision,
      stderr: &evidence.join("agent-b.stderr.log"),
      cgroup_root,
      cgroup_baseline: &cgroup_baseline,
      maximum_disk_bytes: backend.workspace_bytes(),
    },
    agent_b.child_mut(),
  )
  .await;
  stop_agent(
    &client,
    &management_origin,
    &agent_b_name,
    &run_id,
    &mut agent_b,
    &evidence.join("agent-b.stderr.log"),
  )
  .await;

  agent_a.cleanup_cache();
  agent_b.cleanup_cache();
  sleep(Duration::from_secs(1)).await;
  assert_eq!(directory_entries(work_root), work_baseline, "Native workspaces leaked");
  assert_eq!(directory_entries(cgroup_root), cgroup_baseline, "Native cgroups leaked");
  assert_eq!(
    directory_entries(cache_root),
    cache_baseline,
    "Native L1 cache scopes leaked"
  );
  let metrics = client
    .get(format!("{management_origin}/metrics"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .text()
    .await
    .unwrap();
  assert_metrics_exercised(
    &metrics,
    &[
      "octacity_server_http_requests",
      "octacity_server_trigger_decisions",
      "octacity_server_orchestrator_transitions",
      "octacity_server_lease_operations",
      "octacity_server_cache_operations",
      "octacity_server_artifact_operations",
    ],
  );
  fs::write(evidence.join("metrics.prom"), &metrics).unwrap();
  fs::write(
    evidence.join("linux-native-matrix.json"),
    serde_json::to_vec_pretty(&json!({
      "server_release": release.server_manifest,
      "agent_release": release.agent_manifest,
      "manual": manual_run.build,
      "manual_replay": replay,
      "downstream": downstream_run.build,
      "scheduled": scheduled_run.build,
      "cancelled": cancelled_run,
      "artifacts": artifacts,
      "log_search": searches,
    }))
    .unwrap(),
  )
  .unwrap();
  cache_proxy.shutdown().await;
  shutdown_server(&mut server, &server_stderr).await;
}

async fn create_resources(
  client: &Client,
  origin: &str,
  run: &str,
  source_repository: &str,
  backend: &ReleaseBackend,
) -> MatrixResources {
  let (operating_system, architecture) = backend.agent_platform();
  let pool = post_management(
    client,
    origin,
    "/api/v1/agent-pools",
    &format!("{run}-pool"),
    json!({
      "name": format!("release-native-{run}"),
      "definition": {
        "enabled": true,
        "drain_state": "accepting",
        "admission_policy": {"mode": "allowlist", "platforms": [{
          "operating_system": operating_system,
          "architecture": architecture
        }]},
        "concurrency_limit": 1,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": 2
      }
    }),
  )
  .await;
  let pool_id = resource_id(&pool);
  let project = post_management(
    client,
    origin,
    "/api/v1/projects",
    &format!("{run}-project"),
    json!({"parent_id": null, "name": format!("release-native-{run}")}),
  )
  .await;
  let project_id = resource_id(&project);
  let repository = post_management(
    client,
    origin,
    "/api/v1/repositories",
    &format!("{run}-repository"),
    json!({
      "project_id": project_id,
      "name": "octacity-release-fixture",
      "definition": {
        "vcs_integration_id": Uuid::new_v4().to_string(),
        "repository_locator": source_repository,
        "selection": {"allowed_references": [], "default_reference": null, "allow_exact_revision": true}
      }
    }),
  )
  .await;
  let repository_id = resource_id(&repository);

  let main_pipeline = create_pipeline(
    client,
    origin,
    run,
    &project_id,
    "main",
    &["cacheable", "downstream"],
    true,
  )
  .await;
  let scheduled_pipeline = create_pipeline(client, origin, run, &project_id, "scheduled", &["cacheable"], false).await;
  let downstream_pipeline =
    create_pipeline(client, origin, run, &project_id, "downstream", &["downstream"], false).await;
  let cancellation_pipeline = create_pipeline(client, origin, run, &project_id, "cancel", &["slow"], false).await;
  let manual_configuration_id = create_configuration(ConfigurationInput {
    client,
    origin,
    run,
    name: "manual",
    project_id: &project_id,
    repository_id: &repository_id,
    pipeline_id: &main_pipeline,
    pool_id: &pool_id,
    triggers: &["manual"],
    cache: true,
    backend,
  })
  .await;
  let scheduled_configuration_id = create_configuration(ConfigurationInput {
    client,
    origin,
    run,
    name: "scheduled",
    project_id: &project_id,
    repository_id: &repository_id,
    pipeline_id: &scheduled_pipeline,
    pool_id: &pool_id,
    triggers: &["scheduled"],
    cache: true,
    backend,
  })
  .await;
  let downstream_configuration_id = create_configuration(ConfigurationInput {
    client,
    origin,
    run,
    name: "downstream",
    project_id: &project_id,
    repository_id: &repository_id,
    pipeline_id: &downstream_pipeline,
    pool_id: &pool_id,
    triggers: &["internal"],
    cache: false,
    backend,
  })
  .await;
  let cancellation_configuration_id = create_configuration(ConfigurationInput {
    client,
    origin,
    run,
    name: "cancel",
    project_id: &project_id,
    repository_id: &repository_id,
    pipeline_id: &cancellation_pipeline,
    pool_id: &pool_id,
    triggers: &["manual"],
    cache: false,
    backend,
  })
  .await;
  publish_matrix_policy(client, origin, run, &project_id, &repository_id, &pool_id).await;
  let manual_trigger_id = create_manual_definition(client, origin, run, "main", &manual_configuration_id).await;
  let cancellation_trigger_id =
    create_manual_definition(client, origin, run, "cancel", &cancellation_configuration_id).await;
  let internal = post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/internal",
    &format!("{run}-internal-definition"),
    json!({
      "upstream_configuration_id": manual_configuration_id,
      "upstream_configuration_version": 1,
      "configuration_id": downstream_configuration_id,
      "configuration_version": 1,
      "outcome": "succeeded",
      "source": {"kind": "inherit_revision"},
      "parameters": {},
      "priority": 40,
      "enabled": true
    }),
  )
  .await;
  MatrixResources {
    pool_id,
    project_id,
    manual_configuration_id,
    scheduled_configuration_id,
    cancellation_configuration_id,
    manual_trigger_id,
    cancellation_trigger_id,
    internal_trigger_id: resource_id(&internal),
  }
}

async fn create_pipeline(
  client: &Client,
  origin: &str,
  run: &str,
  project_id: &str,
  name: &str,
  tasks: &[&str],
  sequential: bool,
) -> String {
  let nodes = tasks
    .iter()
    .map(|task| {
      json!({
        "id": task,
        "name": task,
        "dependency_policy": "all_succeeded",
        "required_capabilities": ["native", "shell"],
        "execution": {
          "octafile": FIXTURE_OCTAFILE,
          "commands": [task],
          "parallel": false,
          "failfast": true
        }
      })
    })
    .collect::<Vec<_>>();
  let edges = if sequential {
    vec![json!({"predecessor": tasks[0], "dependent": tasks[1]})]
  } else {
    Vec::new()
  };
  let response = post_management(
    client,
    origin,
    "/api/v1/pipelines",
    &format!("{run}-{name}-pipeline"),
    json!({"project_id": project_id, "name": name, "dag": {"nodes": nodes, "edges": edges}}),
  )
  .await;
  resource_id(&response)
}

async fn create_configuration(input: ConfigurationInput<'_>) -> String {
  let response = post_management(
    input.client,
    input.origin,
    "/api/v1/build-configurations",
    &format!("{}-{}-configuration", input.run, input.name),
    json!({
      "project_id": input.project_id,
      "name": input.name,
      "definition": {
        "enabled": true,
        "job_concurrency_limit": 1,
        "repository_id": input.repository_id,
        "repository_version": 1,
        "pipeline_id": input.pipeline_id,
        "pipeline_version": 1,
        "parameters": {"parameters": {}, "deny_unknown": true},
        "triggers": input.triggers,
        "agent_requirements": {
          "capabilities": ["native", "shell"],
          "labels": {},
          "minimum_cpu_millis": 1000,
          "minimum_memory_bytes": MEMORY_BYTES,
          "minimum_disk_bytes": input.backend.workspace_bytes()
        },
        "allowed_pools": [input.pool_id],
        "runtime": {
          "class": "native",
          "operating_system": "linux",
          "architecture": input.backend.guest_architecture(),
          "immutable_image": null,
          "cpu_millis": 1000,
          "memory_bytes": MEMORY_BYTES,
          "writable_disk_bytes": input.backend.workspace_bytes(),
          "timeout_seconds": 300,
          "network": {"mode": if input.cache { "unrestricted" } else { "disabled" }},
          "workload_identity_profile": null
        },
        "cache": {
          "namespace": if input.cache { Value::String(CACHE_NAMESPACE.to_owned()) } else { Value::Null },
          "read": input.cache,
          "write": input.cache
        },
        "artifacts": {
          "artifact_count": 1,
          "artifact_bytes": OUTPUT_BYTES,
          "report_count": 1,
          "report_bytes": OUTPUT_BYTES,
          "single_output_bytes": OUTPUT_BYTES
        },
        "retry": {"max_attempts": 1, "retry_on": []}
      }
    }),
  )
  .await;
  resource_id(&response)
}

async fn publish_matrix_policy(
  client: &Client,
  origin: &str,
  run: &str,
  project_id: &str,
  repository_id: &str,
  pool_id: &str,
) {
  post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project_id}/policy-versions"),
    &format!("{run}-policy"),
    json!({"policy": {
      "pools": {"mode": "replace", "value": [pool_id]},
      "repositories": {"mode": "replace", "value": [repository_id]},
      "secret_profiles": {"mode": "replace", "value": []},
      "identity_profiles": {"mode": "replace", "value": []},
      "runtimes": {"mode": "replace", "value": ["native"]},
      "cache": {"mode": "replace", "value": {
        "namespaces": [CACHE_NAMESPACE], "read": true, "write": true, "max_bytes": OUTPUT_BYTES
      }},
      "artifacts": {"mode": "replace", "value": {
        "artifact_count": 1, "artifact_bytes": OUTPUT_BYTES,
        "report_count": 1, "report_bytes": OUTPUT_BYTES, "single_output_bytes": OUTPUT_BYTES
      }},
      "concurrency": {"mode": "replace", "value": {"active_builds": 8, "active_jobs": 1}},
      "retention": {"mode": "replace", "value": {
        "build_seconds": 86400, "log_seconds": 86400,
        "artifact_seconds": 86400, "cache_seconds": 86400
      }}
    }}),
  )
  .await;
}

async fn create_manual_definition(client: &Client, origin: &str, run: &str, name: &str, configuration: &str) -> String {
  let response = post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/manual",
    &format!("{run}-{name}-manual-definition"),
    json!({"configuration_id": configuration, "configuration_version": 1, "enabled": true, "definition": {}}),
  )
  .await;
  resource_id(&response)
}

async fn accept_manual_build(
  client: &Client,
  origin: &str,
  identity: &str,
  trigger_id: &str,
  configuration_id: &str,
  revision: &str,
) -> Value {
  post_management(
    client,
    origin,
    "/api/v1/triggers/manual",
    identity,
    json!({
      "trigger_id": trigger_id,
      "trigger_version": 1,
      "configuration_id": configuration_id,
      "configuration_version": 1,
      "deduplication_identity": identity,
      "source": {"kind": "exact_revision", "value": revision},
      "parameters": {},
      "priority": 50
    }),
  )
  .await
}

async fn create_schedule(client: &Client, origin: &str, run: &str, configuration: &str, revision: &str) -> Value {
  let occurrence = Utc::now() + chrono::Duration::seconds(15);
  let expression = format!(
    "{} {} {} {} {} * {}",
    occurrence.second(),
    occurrence.minute(),
    occurrence.hour(),
    occurrence.day(),
    occurrence.month(),
    occurrence.year()
  );
  post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/scheduled",
    &format!("{run}-schedule"),
    json!({
      "configuration_id": configuration,
      "configuration_version": 1,
      "enabled": true,
      "schedule": {"expression": expression, "timezone": "UTC", "missed_run_policy": {"kind": "run_once"}},
      "build": {
        "source": {"kind": "exact_revision", "value": revision},
        "parameters": {},
        "priority": 45
      }
    }),
  )
  .await
}

async fn start_matrix_agent(input: AgentStartInput<'_>) -> MatrixAgent {
  let enrollment = issue_enrollment(
    input.client,
    input.management_origin,
    &format!("{}-{}", input.run, input.evidence_name),
    input.pool_id,
    input.backend,
  )
  .await;
  let directory = input.temporary.join(input.evidence_name);
  fs::create_dir(&directory).unwrap();
  let cache_directory = tempfile::Builder::new()
    .prefix(&format!("{}-{}-", input.run, input.evidence_name))
    .tempdir_in(native_cache_root(input.backend))
    .unwrap();
  set_mode(cache_directory.path(), 0o700);
  let upload_origins = [input.object_endpoint];
  let config = write_agent_config(
    &directory,
    input.agent_origin,
    input.agent_name,
    &enrollment,
    AgentReleasePaths::from(input.release),
    input.backend,
    &AgentConfigOverrides {
      cache_root: Some(cache_directory.path()),
      cache_read: true,
      cache_write: true,
      remote_cache_origin: Some(&input.cache_proxy.origin),
      cache_ca_certificate: Some(&input.cache_proxy.ca_certificate),
      unrestricted_network: true,
      upload_origins: &upload_origins,
      output_limit_bytes: Some(OUTPUT_BYTES),
    },
  );
  let child = spawn_agent(
    &input.release.agent_binary,
    &config,
    &input.evidence.join(format!("{}.stdout.log", input.evidence_name)),
    &input.evidence.join(format!("{}.stderr.log", input.evidence_name)),
  );
  MatrixAgent {
    child,
    cache_directory: Some(cache_directory),
  }
}

async fn stop_agent(client: &Client, origin: &str, name: &str, run: &str, agent: &mut MatrixAgent, stderr: &Path) {
  drain_agent(client, origin, name, &format!("{run}-{name}")).await;
  let status = timeout(Duration::from_secs(45), agent.child.wait())
    .await
    .expect("released Agent did not stop after drain")
    .unwrap();
  assert!(
    status.success(),
    "released Agent exited unsuccessfully: {}",
    fs::read_to_string(stderr).unwrap_or_default()
  );
}

async fn run_and_cancel(input: CancellationInput<'_>, agent: &mut Child) -> Value {
  let identity = format!("{}-cancel", input.run);
  let accepted = accept_manual_build(
    input.client,
    input.origin,
    &identity,
    input.trigger_id,
    input.configuration,
    input.revision,
  )
  .await;
  let build_id = string(&accepted, "build_id");
  let attempt_id = string(&accepted, "attempt_id");
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    if let Some(status) = agent.try_wait().unwrap() {
      panic!(
        "released Agent exited before cancellation with {status}: {}",
        fs::read_to_string(input.stderr).unwrap_or_default()
      );
    }
    let attempt = get_json(input.client, format!("{}/api/v1/attempts/{attempt_id}", input.origin)).await;
    let job = &attempt["jobs"][0];
    let events = get_json(
      input.client,
      format!(
        "{}/api/v1/jobs/{}/events?after=0&limit=100&wait_ms=0",
        input.origin,
        string(job, "id")
      ),
    )
    .await;
    let started = events["items"].as_array().unwrap().iter().any(|event| {
      event["payload"]["source"] == "agent"
        && event["payload"]["event"]["type"] == "state_changed"
        && event["payload"]["event"]["state"] == "running"
    });
    let sampled = events["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|event| event["payload"]["source"] == "agent" && event["payload"]["event"]["type"] == "resource_usage");
    if started && sampled {
      assert_native_resource_controls(input.cgroup_root, input.cgroup_baseline);
      break;
    }
    assert!(
      Instant::now() < deadline,
      "cancelled Build never started with resource accounting"
    );
    sleep(Duration::from_millis(250)).await;
  }
  let cancellation = post_management(
    input.client,
    input.origin,
    &format!("/api/v1/builds/{build_id}/cancel"),
    &format!("{}-cancel-request", input.run),
    json!({}),
  )
  .await;
  let build = wait_for_terminal_build(input.client, input.origin, &build_id, agent, input.stderr).await;
  assert_eq!(build["state"], "cancelled");
  let attempt = get_json(input.client, format!("{}/api/v1/attempts/{attempt_id}", input.origin)).await;
  let job = &attempt["jobs"][0];
  let events = get_json(
    input.client,
    format!(
      "{}/api/v1/jobs/{}/events?after=0&limit=256&wait_ms=0",
      input.origin,
      string(job, "id")
    ),
  )
  .await;
  assert_lifecycle_order(events["items"].as_array().unwrap());
  assert_resource_samples(events["items"].as_array().unwrap(), input.maximum_disk_bytes);
  json!({"accepted": accepted, "cancellation": cancellation, "build": build, "attempt": attempt, "events": events})
}

async fn wait_for_trigger_build(pool: &PgPool, trigger_id: &str) -> String {
  let trigger_id = Uuid::parse_str(trigger_id).unwrap();
  let deadline = Instant::now() + Duration::from_secs(120);
  loop {
    let build_id = sqlx::query_scalar::<_, Uuid>(
      "SELECT build_id FROM trigger_occurrences WHERE trigger_id = $1 AND build_id IS NOT NULL ORDER BY created_at LIMIT 1",
    )
    .bind(trigger_id)
    .fetch_optional(pool)
    .await
    .unwrap();
    if let Some(build_id) = build_id {
      return build_id.to_string();
    }
    assert!(Instant::now() < deadline, "timed out waiting for Trigger {trigger_id}");
    sleep(Duration::from_millis(500)).await;
  }
}

async fn occurrence_count(pool: &PgPool, trigger_id: &str) -> i64 {
  sqlx::query_scalar("SELECT COUNT(*) FROM trigger_occurrences WHERE trigger_id = $1")
    .bind(Uuid::parse_str(trigger_id).unwrap())
    .fetch_one(pool)
    .await
    .unwrap()
}

fn assert_native_resource_controls(cgroup_root: &Path, baseline: &BTreeSet<OsString>) {
  let current = directory_entries(cgroup_root);
  let active = current.difference(baseline).collect::<Vec<_>>();
  assert_eq!(active.len(), 1, "one Native execution cgroup must be active");
  let cgroup = cgroup_root.join(active[0]);
  assert_eq!(
    fs::read_to_string(cgroup.join("memory.max")).unwrap().trim(),
    MEMORY_BYTES.to_string()
  );
  assert_eq!(fs::read_to_string(cgroup.join("memory.swap.max")).unwrap().trim(), "0");
  assert_eq!(fs::read_to_string(cgroup.join("pids.max")).unwrap().trim(), "4096");
  let cpu = fs::read_to_string(cgroup.join("cpu.max")).unwrap();
  let values = cpu
    .split_whitespace()
    .map(|value| value.parse::<u64>().unwrap())
    .collect::<Vec<_>>();
  assert_eq!(values.len(), 2);
  assert_eq!(values[0], values[1], "1000 CPU millis must enforce one complete CPU");
}

fn native_roots(backend: &ReleaseBackend) -> (&Path, &Path) {
  match backend {
    ReleaseBackend::Native {
      work_root, cgroup_root, ..
    } => (work_root, cgroup_root),
    ReleaseBackend::Microsandbox { .. } => unreachable!(),
  }
}

fn native_cache_root(backend: &ReleaseBackend) -> &Path {
  match backend {
    ReleaseBackend::Native { cache_root, .. } => cache_root,
    ReleaseBackend::Microsandbox { .. } => unreachable!(),
  }
}

fn directory_entries(path: &Path) -> BTreeSet<OsString> {
  fs::read_dir(path)
    .unwrap()
    .map(|entry| entry.unwrap().file_name())
    .collect()
}

fn unused_loopback_address() -> SocketAddr {
  TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap()
}
