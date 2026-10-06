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
use tokio::{
  process::Child,
  time::{Instant, sleep, timeout},
};
use uuid::Uuid;

use super::{
  AgentConfigOverrides, AgentReleasePaths, AgentToolExecutable, RELEASE_JOB_TIMEOUT_SECONDS, ReleaseBackend,
  ServerConfigInput, drain_agent, get_json, issue_enrollment, post_management, private_file, release, required_path,
  required_string, resource_id, shutdown_server, spawn_agent, spawn_server, string, wait_for_server_ready,
  wait_for_terminal_build, write_agent_config, write_server_config,
};

#[path = "cache_proxy.rs"]
mod cache_proxy;
mod checks;
#[path = "linux_native/codex.rs"]
mod codex;
#[path = "linux_native/performance.rs"]
mod performance;
#[path = "linux_native/scenarios.rs"]
mod scenarios;
#[path = "linux_native/support.rs"]
mod support;

use cache_proxy::TlsCacheProxy;
use checks::*;
use scenarios::{
  CancellationInput, ManualBuildInput, NativeCgroupAssertion, ReadyMarkerAssertion, RetryInput, run_and_cancel,
  run_and_retry,
};
use support::*;

const FIXTURE_OCTAFILE: &str = "server/tests/octacity-release-harness/fixtures/release-matrix/Octafile.yml";
const CACHE_NAMESPACE: &str = "release-linux-native";
const MICROSANDBOX_HOST_ALIAS: &str = "host.microsandbox.internal";
const ARTIFACT_BYTES: usize = 1024 * 1024;
const MEMORY_BYTES: u64 = 512 * 1024 * 1024;
const OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

struct MatrixResources {
  pool_id: String,
  project_id: String,
  repository_id: String,
  manual_configuration_id: String,
  scheduled_configuration_id: String,
  cancellation_configuration_id: String,
  retry_configuration_id: String,
  manual_trigger_id: String,
  cancellation_trigger_id: String,
  retry_trigger_id: String,
  internal_trigger_id: String,
}

struct BuildRun {
  build: Value,
  attempt: Value,
  jobs: Vec<Value>,
}

struct MatrixAgent {
  child: Child,
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
  tool_executable: Option<&'a AgentToolExecutable<'a>>,
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
  artifact_count: u16,
  backend: &'a ReleaseBackend,
}

impl MatrixAgent {
  fn child_mut(&mut self) -> &mut Child {
    &mut self.child
  }
}

pub(super) async fn run() {
  run_matrix("native").await;
}

pub(super) async fn run_microsandbox() {
  run_matrix("microsandbox").await;
}

async fn run_matrix(expected_backend: &str) {
  let backend = ReleaseBackend::from_environment();
  assert_eq!(backend.name(), expected_backend);
  assert!(
    matches!(
      backend,
      ReleaseBackend::Native { .. } | ReleaseBackend::Microsandbox { .. }
    ),
    "the extended release matrix supports Native and Microsandbox"
  );
  let postgres_url = required_string("OCTACITY_POSTGRES_URL");
  let object_endpoint = required_string("OCTACITY_MINIO_ENDPOINT");
  let source_repository = required_string("OCTACITY_RELEASE_SOURCE_REPOSITORY");
  let evidence = required_path("OCTACITY_RELEASE_EVIDENCE_DIR", false);
  fs::create_dir_all(&evidence).unwrap();
  let mut performance = performance::Recorder::new(backend.name());
  let release = release::load(&backend);
  let revision = release.server_manifest.octacity_revision().to_owned();
  let temporary = tempfile::tempdir().unwrap();
  let pool = PgPoolOptions::new()
    .max_connections(4)
    .connect(&postgres_url)
    .await
    .unwrap();

  let work_root = backend_work_root(&backend);
  let native_roots = native_roots(&backend);
  let cache_root = native_cache_root(&backend);
  let available_workspace_bytes = available_disk_bytes(work_root);
  assert!(
    available_workspace_bytes >= backend.workspace_bytes(),
    "{} work filesystem has {available_workspace_bytes} available bytes but the release Job requires {}",
    backend.name(),
    backend.workspace_bytes()
  );
  let work_baseline = directory_entries(work_root);
  let cgroup_baseline = native_roots.map(|(_, cgroup_root)| directory_entries(cgroup_root));
  let cache_baseline = cache_root.map(directory_entries);
  let management_addr = unused_loopback_address();
  let agent_addr = unused_loopback_address();
  let cache_addr = unused_loopback_address();
  let cache_proxy = TlsCacheProxy::start(cache_addr, temporary.path(), cache_proxy_advertised_host(&backend)).await;
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
  wait_for_server_ready(&client, &management_origin, &mut server, &server_stdout).await;
  let run_id = Uuid::new_v4().simple().to_string();
  let resources = create_resources(&client, &management_origin, &run_id, &source_repository, &backend).await;

  let agent_a_name = format!("release-{}-a-{run_id}", backend.name());
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
    tool_executable: None,
  })
  .await;
  performance::wait_for_agent_registration(&client, &management_origin, &agent_a_name).await;

  let trigger_started = Instant::now();
  let manual = accept_manual_build(
    &client,
    &management_origin,
    &run_id,
    &resources.manual_trigger_id,
    &resources.manual_configuration_id,
    &revision,
  )
  .await;
  performance.duration("trigger_latency_ms", trigger_started.elapsed());
  let replay_started = Instant::now();
  let replay = accept_manual_build(
    &client,
    &management_origin,
    &run_id,
    &resources.manual_trigger_id,
    &resources.manual_configuration_id,
    &revision,
  )
  .await;
  performance.duration("trigger_latency_ms", replay_started.elapsed());
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
  performance::record_run(&mut performance, &manual_run);
  write_cache_diagnostics(&evidence, "manual", &manual_run);
  assert_dag_and_events(&manual_run, Some(backend.workspace_bytes()));
  assert!(manual_run.build.get("factory").is_none());
  assert!(manual_run.build.get("factory_run_id").is_none());
  let downstream_build_id = wait_for_trigger_build(&pool, &resources.internal_trigger_id).await;
  let downstream_run = wait_for_successful_run(
    &client,
    &management_origin,
    &downstream_build_id,
    agent_a.child_mut(),
    &evidence.join("agent-a.stderr.log"),
  )
  .await;
  performance.sample(
    "dag_transition_latency_ms",
    "milliseconds",
    downstream_run.build["created_at_unix_ms"]
      .as_i64()
      .unwrap()
      .saturating_sub(manual_run.build["updated_at_unix_ms"].as_i64().unwrap()) as f64,
  );
  performance::record_run(&mut performance, &downstream_run);
  assert_internal_causality(&manual, &manual_run.build, &downstream_run.build);
  assert_successful_job_events(&downstream_run);
  assert_eq!(occurrence_count(&pool, &resources.internal_trigger_id).await, 1);

  let (artifacts, artifact_throughput) = verify_artifacts(&client, &management_origin, &manual_run.build).await;
  performance.sample(
    "artifact_throughput_bytes_per_second",
    "bytes_per_second",
    artifact_throughput,
  );
  let (searches, index_lag, query_latency) =
    verify_log_search(&client, &management_origin, &resources.project_id, &manual_run).await;
  for sample in index_lag {
    performance.duration("log_index_lag_ms", sample);
  }
  for sample in query_latency {
    performance.duration("log_query_latency_ms", sample);
  }
  for sample in performance::rest_latency(&client, &management_origin, &string(&manual, "build_id")).await {
    performance.duration("rest_latency_ms", sample);
  }
  for sample in performance::database_contention(&pool, &string(&manual, "build_id")).await {
    performance.duration("database_contention_latency_ms", sample);
  }
  stop_agent(
    &client,
    &management_origin,
    &agent_a_name,
    &run_id,
    &mut agent_a,
    &evidence.join("agent-a.stderr.log"),
  )
  .await;
  if let (Some(cache_root), Some(cache_baseline)) = (cache_root, cache_baseline.as_ref()) {
    remove_agent_cache_entries(cache_root, cache_baseline);
  }

  let agent_b_name = format!("release-{}-b-{run_id}", backend.name());
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
    tool_executable: None,
  })
  .await;
  performance::wait_for_agent_registration(&client, &management_origin, &agent_b_name).await;
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
  performance::record_run(&mut performance, &scheduled_run);
  assert_eq!(scheduled_run.build["trigger"]["kind"], "scheduled");
  assert_successful_job_events(&scheduled_run);
  assert_eq!(occurrence_count(&pool, &scheduled_trigger_id).await, 1);
  let scheduled_cache_diagnostics = write_cache_diagnostics(&evidence, "scheduled", &scheduled_run);
  assert!(
    scheduled_run.jobs.iter().any(has_cache_hit),
    "the second Agent had an empty L1, so the scheduled Build must restore from remote L2: {}",
    serde_json::to_string_pretty(&scheduled_cache_diagnostics).unwrap()
  );
  performance.duration("cache_restore_ms", performance::cache_restore_duration(&scheduled_run));

  let retried_run = run_and_retry(
    RetryInput {
      build: ManualBuildInput {
        client: &client,
        origin: &management_origin,
        run: &run_id,
        trigger_id: &resources.retry_trigger_id,
        configuration: &resources.retry_configuration_id,
        revision: &revision,
        stderr: &evidence.join("agent-b.stderr.log"),
      },
    },
    agent_b.child_mut(),
  )
  .await;

  let cancelled_run = run_and_cancel(
    CancellationInput {
      build: ManualBuildInput {
        client: &client,
        origin: &management_origin,
        run: &run_id,
        trigger_id: &resources.cancellation_trigger_id,
        configuration: &resources.cancellation_configuration_id,
        revision: &revision,
        stderr: &evidence.join("agent-b.stderr.log"),
      },
      native_cgroup: native_roots
        .zip(cgroup_baseline.as_ref())
        .map(|((_, cgroup_root), baseline)| NativeCgroupAssertion {
          root: cgroup_root,
          baseline,
        }),
      ready_marker: None,
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

  let codex = codex::run(codex::CodexScenarioInput {
    client: &client,
    management_origin: &management_origin,
    agent_origin: &agent_origin,
    temporary: temporary.path(),
    evidence: &evidence,
    run_id: &run_id,
    resources: &resources,
    backend: &backend,
    release: &release,
    cache_proxy: &cache_proxy,
    object_endpoint: &object_endpoint,
    revision: &revision,
    native_roots,
    cgroup_baseline: cgroup_baseline.as_ref(),
    work_root,
    server_stdout: &server_stdout,
    server_stderr: &server_stderr,
  })
  .await;

  if let (Some(cache_root), Some(cache_baseline)) = (cache_root, cache_baseline.as_ref()) {
    remove_agent_cache_entries(cache_root, cache_baseline);
  }
  if performance::enabled() {
    let (registrations, polls, errors, registered_agents) = performance::agent_soak(
      &client,
      &management_origin,
      &agent_origin,
      &run_id,
      &resources.pool_id,
      &backend,
      &pool,
    )
    .await;
    for sample in registrations {
      performance.duration("agent_soak_registration_ms", sample);
    }
    for sample in polls {
      performance.duration("agent_soak_poll_ms", sample);
    }
    performance.sample("agent_soak_errors", "errors", errors as f64);
    performance.sample("agent_soak_agents", "agents", registered_agents as f64);
  }
  sleep(Duration::from_secs(1)).await;
  assert_eq!(
    directory_entries(work_root),
    work_baseline,
    "{} workspaces leaked",
    backend.name()
  );
  if let (Some((_, cgroup_root)), Some(cgroup_baseline)) = (native_roots, cgroup_baseline.as_ref()) {
    assert_eq!(
      directory_entries(cgroup_root),
      *cgroup_baseline,
      "Native cgroups leaked"
    );
  }
  if let (Some(cache_root), Some(cache_baseline)) = (cache_root, cache_baseline.as_ref()) {
    assert_eq!(
      directory_entries(cache_root),
      *cache_baseline,
      "Native L1 cache scopes leaked"
    );
  }
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
    evidence.join(format!("{}-matrix.json", backend.name())),
    serde_json::to_vec_pretty(&json!({
      "server_release": release.server_manifest,
      "agent_release": release.agent_manifest,
      "manual": manual_run.build,
      "manual_replay": replay,
      "downstream": downstream_run.build,
      "scheduled": scheduled_run.build,
      "retried": retried_run,
      "cancelled": cancelled_run,
      "codex": codex,
      "artifacts": artifacts,
      "log_search": searches,
    }))
    .unwrap(),
  )
  .unwrap();
  performance.write(&evidence);
  cache_proxy.shutdown().await;
  shutdown_server(&mut server, &server_stdout).await;
}

async fn create_resources(
  client: &Client,
  origin: &str,
  run: &str,
  source_repository: &str,
  backend: &ReleaseBackend,
) -> MatrixResources {
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
        "admission_policy": backend.pool_admission_policy(),
        "concurrency_limit": 1,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": if performance::enabled() { 128 } else { 2 }
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
    backend,
  )
  .await;
  let scheduled_pipeline =
    create_pipeline(client, origin, run, &project_id, "scheduled", &["cacheable"], backend).await;
  let downstream_pipeline =
    create_pipeline(client, origin, run, &project_id, "downstream", &["downstream"], backend).await;
  let cancellation_pipeline = create_pipeline(client, origin, run, &project_id, "cancel", &["slow"], backend).await;
  let retry_pipeline = create_pipeline(client, origin, run, &project_id, "retry", &["failing"], backend).await;
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
    artifact_count: 1,
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
    artifact_count: 1,
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
    artifact_count: 1,
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
    artifact_count: 1,
    backend,
  })
  .await;
  let retry_configuration_id = create_configuration(ConfigurationInput {
    client,
    origin,
    run,
    name: "retry",
    project_id: &project_id,
    repository_id: &repository_id,
    pipeline_id: &retry_pipeline,
    pool_id: &pool_id,
    triggers: &["manual"],
    cache: false,
    artifact_count: 1,
    backend,
  })
  .await;
  publish_matrix_policy(client, origin, run, &project_id, &repository_id, &pool_id, backend).await;
  let manual_trigger_id = create_manual_definition(client, origin, run, "main", &manual_configuration_id).await;
  let cancellation_trigger_id =
    create_manual_definition(client, origin, run, "cancel", &cancellation_configuration_id).await;
  let retry_trigger_id = create_manual_definition(client, origin, run, "retry", &retry_configuration_id).await;
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
    repository_id,
    manual_configuration_id,
    scheduled_configuration_id,
    cancellation_configuration_id,
    retry_configuration_id,
    manual_trigger_id,
    cancellation_trigger_id,
    retry_trigger_id,
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
  backend: &ReleaseBackend,
) -> String {
  let nodes = tasks
    .iter()
    .map(|task| matrix_pipeline_node(task, backend))
    .collect::<Vec<_>>();
  let edges = tasks
    .windows(2)
    .map(|pair| json!({"predecessor": pair[0], "dependent": pair[1]}))
    .collect::<Vec<_>>();
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
    matrix_configuration_body(&input),
  )
  .await;
  resource_id(&response)
}

fn matrix_configuration_body(input: &ConfigurationInput<'_>) -> Value {
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
        "capabilities": input.backend.required_capabilities(),
        "labels": {},
        "minimum_cpu_millis": 1000,
        "minimum_memory_bytes": MEMORY_BYTES,
        "minimum_disk_bytes": input.backend.workspace_bytes()
      },
      "allowed_pools": [input.pool_id],
      "runtime": {
        "class": input.backend.runtime_class(),
        "operating_system": "linux",
        "architecture": input.backend.guest_architecture(),
        "host_platform": input.backend.host_platform(),
        "required_guarantees": input.backend.required_guarantees(),
        "immutable_image": input.backend.immutable_image(),
        "cpu_millis": 1000,
        "memory_bytes": MEMORY_BYTES,
        "writable_disk_bytes": input.backend.workspace_bytes(),
        "timeout_seconds": RELEASE_JOB_TIMEOUT_SECONDS,
        "network": {"mode": if input.cache { "unrestricted" } else { "disabled" }},
        "workload_identity_profile": null
      },
      "cache": {
        "namespace": if input.cache { Value::String(CACHE_NAMESPACE.to_owned()) } else { Value::Null },
        "read": input.cache,
        "write": input.cache
      },
      "artifacts": {
        "artifact_count": input.artifact_count,
        "artifact_bytes": OUTPUT_BYTES,
        "report_count": 1,
        "report_bytes": OUTPUT_BYTES,
        "single_output_bytes": OUTPUT_BYTES
      },
      "retry": {"max_attempts": 1, "retry_on": []}
    }
  })
}

fn matrix_pipeline_node(task: &str, backend: &ReleaseBackend) -> Value {
  json!({
    "id": task,
    "name": task,
    "dependency_policy": "all_succeeded",
    "required_capabilities": backend.required_capabilities(),
    "execution": {
      "octafile": FIXTURE_OCTAFILE,
      "commands": [task],
      "parallel": false,
      "failfast": true
    }
  })
}

async fn publish_matrix_policy(
  client: &Client,
  origin: &str,
  run: &str,
  project_id: &str,
  repository_id: &str,
  pool_id: &str,
  backend: &ReleaseBackend,
) {
  post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project_id}/policy-versions"),
    &format!("{run}-policy"),
    matrix_policy_body(repository_id, pool_id, backend),
  )
  .await;
}

fn matrix_policy_body(repository_id: &str, pool_id: &str, backend: &ReleaseBackend) -> Value {
  json!({"policy": {
    "pools": {"mode": "replace", "value": [pool_id]},
    "repositories": {"mode": "replace", "value": [repository_id]},
    "secret_profiles": {"mode": "replace", "value": []},
    "identity_profiles": {"mode": "replace", "value": []},
    "runtimes": {"mode": "replace", "value": [backend.runtime_class()]},
    "execution_targets": {
      "mode": "replace",
      "value": backend.execution_target().into_iter().collect::<Vec<_>>()
    },
    "cache": {"mode": "replace", "value": {
      "namespaces": [CACHE_NAMESPACE], "read": true, "write": true, "max_bytes": OUTPUT_BYTES
    }},
    "artifacts": {"mode": "replace", "value": {
      "artifact_count": 2, "artifact_bytes": OUTPUT_BYTES,
      "report_count": 1, "report_bytes": OUTPUT_BYTES, "single_output_bytes": OUTPUT_BYTES
    }},
    "concurrency": {"mode": "replace", "value": {"active_builds": 8, "active_jobs": 1}},
    "retention": {"mode": "replace", "value": {
      "build_seconds": 86400, "log_seconds": 86400,
      "artifact_seconds": 86400, "cache_seconds": 86400
    }}
  }})
}

#[cfg(test)]
mod tests {
  use std::path::{Path, PathBuf};

  use super::*;

  #[test]
  fn microsandbox_matrix_documents_request_provider_neutral_virtualization() {
    let backend = ReleaseBackend::Microsandbox {
      work_root: PathBuf::from("/work"),
      state_root: PathBuf::from("/state"),
      executable: PathBuf::from("/opt/microsandbox/bin/msb"),
      libkrunfw: PathBuf::from("/opt/microsandbox/lib/libkrunfw.so"),
      environment_identity: "microsandbox-linux-guest-v1".to_owned(),
      image: format!("example.invalid/octa@sha256:{}", "a".repeat(64)),
      workspace_bytes: 1024 * 1024 * 1024,
    };
    let client = Client::new();
    let configuration = matrix_configuration_body(&ConfigurationInput {
      client: &client,
      origin: "http://127.0.0.1",
      run: "run",
      name: "manual",
      project_id: "project",
      repository_id: "repository",
      pipeline_id: "pipeline",
      pool_id: "pool",
      triggers: &["manual"],
      cache: true,
      artifact_count: 1,
      backend: &backend,
    });
    let execution_target = backend.execution_target().unwrap();

    assert_eq!(
      configuration["definition"]["agent_requirements"]["capabilities"],
      json!(["shell"])
    );
    assert_eq!(configuration["definition"]["runtime"]["class"], "virtualization");
    assert_eq!(
      configuration["definition"]["runtime"]["host_platform"],
      execution_target["host_platform"]
    );
    assert_eq!(
      configuration["definition"]["runtime"]["required_guarantees"],
      execution_target["required_guarantees"]
    );
    assert_eq!(
      matrix_pipeline_node("cacheable", &backend)["required_capabilities"],
      json!(["shell"])
    );
    assert_eq!(
      matrix_policy_body("repository", "pool", &backend)["policy"]["runtimes"]["value"],
      json!(["virtualization"])
    );
    assert_eq!(
      matrix_policy_body("repository", "pool", &backend)["policy"]["execution_targets"]["value"],
      json!([execution_target.clone()])
    );
    assert_eq!(
      backend.pool_admission_policy()["execution_targets"],
      json!([execution_target])
    );
    assert_eq!(cache_proxy_advertised_host(&backend), "host.microsandbox.internal");
  }

  #[test]
  fn release_artifact_fixture_matches_download_assertions_and_compression_limit() {
    const CACHE_PUBLICATION_COMPRESSION_RATIO_LIMIT: usize = 1_000;

    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture_path = Path::new(FIXTURE_OCTAFILE);
    let fixture_root = fixture_path.parent().unwrap();
    let fixture = repository.join(fixture_path);
    let document: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(fixture).unwrap()).unwrap();
    let shell = document["tasks"]["cacheable"]["shell"].as_str().unwrap();
    let shell = shell.strip_prefix("sleep 2 && ").unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join(fixture_root);
    fs::create_dir_all(workspace.join("input")).unwrap();
    fs::copy(
      repository.join(fixture_root).join("input/message.txt"),
      workspace.join("input/message.txt"),
    )
    .unwrap();

    let output = std::process::Command::new("sh")
      .args(["-eu", "-c", shell])
      .current_dir(&workspace)
      .output()
      .unwrap();
    assert!(
      output.status.success(),
      "release cache fixture failed: {}",
      String::from_utf8_lossy(&output.stderr)
    );

    let payload = fs::read(workspace.join("dist/performance.bin")).unwrap();
    assert_release_artifact_payload(&payload);
    let encoded = zstd::stream::encode_all(payload.as_slice(), 3).unwrap();
    assert!(
      payload.len() <= encoded.len().saturating_mul(CACHE_PUBLICATION_COMPRESSION_RATIO_LIMIT),
      "release cache fixture expands {}:1, above the cache publication limit",
      payload.len() / encoded.len().max(1)
    );
  }
}

#[test]
fn native_cache_cleanup_preserves_the_mounted_filesystem_baseline() {
  let directory = tempfile::tempdir().unwrap();
  let baseline_entry = directory.path().join("lost+found");
  fs::create_dir(&baseline_entry).unwrap();
  let baseline = directory_entries(directory.path());
  fs::write(directory.path().join(".octacity.lock"), []).unwrap();
  let scope = directory.path().join("v1/scope");
  fs::create_dir_all(&scope).unwrap();
  fs::write(scope.join("entry"), b"cached").unwrap();

  remove_agent_cache_entries(directory.path(), &baseline);

  assert!(baseline_entry.is_dir());
  assert_eq!(directory_entries(directory.path()), baseline);
}
