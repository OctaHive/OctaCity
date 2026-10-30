#![cfg(unix)]

//! Black-box release-candidate Agent gate against the real server and store.
//!
//! The test is ignored in portable suites because real execution providers need
//! provisioned release machines. Linux exercises Native, containerd, and
//! Microsandbox; Apple Silicon macOS exercises provider-neutral isolation or
//! virtualization with a Linux guest. In every case the server, Agent, source
//! plugin, Octa runner, and task plugins come from isolated, checksummed release
//! installations rather than `target/`.

use std::{
  env,
  fs::{self, File},
  net::{SocketAddr, TcpListener},
  path::{Path, PathBuf},
  process::Stdio,
  time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_release_harness::InstalledToolchain as Toolchain;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::{
  process::{Child, Command},
  time::{Instant, sleep, timeout},
};
use uuid::Uuid;

#[path = "release_vertical_slice/agent.rs"]
mod agent;
#[path = "release_vertical_slice/backend.rs"]
mod backend;
#[path = "release_vertical_slice/backend_config_tests.rs"]
mod backend_config_tests;
#[path = "release_vertical_slice/browser.rs"]
mod browser;
#[path = "release_vertical_slice/issue_source_status.rs"]
mod issue_source_status;
#[path = "release_vertical_slice/linux_native.rs"]
mod linux_native;
#[path = "release_vertical_slice/project_source.rs"]
mod project_source;
#[path = "release_vertical_slice/release.rs"]
mod release;
#[path = "release_vertical_slice/server.rs"]
mod server;
#[path = "release_vertical_slice/server_config_tests.rs"]
mod server_config_tests;
#[path = "release_vertical_slice/source_checkpoint.rs"]
mod source_checkpoint;
#[path = "release_vertical_slice/source_environment.rs"]
mod source_environment;
#[path = "release_vertical_slice/source_evidence.rs"]
mod source_evidence;
#[path = "release_vertical_slice/source_factory.rs"]
mod source_factory;
#[path = "release_vertical_slice/source_factory_configuration.rs"]
mod source_factory_configuration;
#[path = "release_vertical_slice/source_factory_effect.rs"]
mod source_factory_effect;
#[path = "release_vertical_slice/source_factory_second.rs"]
mod source_factory_second;
#[path = "release_vertical_slice/source_installation.rs"]
mod source_installation;
#[path = "release_vertical_slice/source_provider.rs"]
mod source_provider;
#[path = "release_vertical_slice/source_revocation.rs"]
mod source_revocation;
#[path = "release_vertical_slice/source_setup.rs"]
mod source_setup;
#[path = "release_vertical_slice/source_triage.rs"]
mod source_triage;
#[path = "release_vertical_slice/source_triage_execution.rs"]
mod source_triage_execution;
#[path = "release_vertical_slice/support.rs"]
mod support;
use server::*;
#[path = "release_vertical_slice/environment.rs"]
mod environment;
use environment::*;

use agent::{AgentConfigOverrides, AgentReleasePaths, AgentToolExecutable, spawn_agent, write_agent_config};
use backend::ReleaseBackend;
use support::{get_json, post_management, publish_policy_and_trigger_definition, resource_id, string};

const SIGNING_SEED: [u8; 32] = [7; 32];
const SOURCE_REPOSITORY: &str = "https://github.com/OctaHive/octa.git";
const SOURCE_REVISION: &str = include_str!("../../../../.github/octa-source-revision");
const RELEASE_JOB_TIMEOUT_SECONDS: u64 = 300;
const RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS: u64 = 5;
const RELEASE_JOB_AUTHORITY_MARGIN_SECONDS: u64 = 30;
const RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS: u64 =
  (RELEASE_JOB_TIMEOUT_SECONDS + RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS + RELEASE_JOB_AUTHORITY_MARGIN_SECONDS) * 1_000;
const RELEASE_OBJECT_OPERATION_TIMEOUT_MILLISECONDS: u64 = 5_000;
const RELEASE_READINESS_TIMEOUT_MILLISECONDS: u64 = 15_000;

#[test]
fn released_cache_credentials_outlive_jobs_and_the_final_cache_request() {
  let required_lifetime_milliseconds = (RELEASE_JOB_TIMEOUT_SECONDS + RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS) * 1_000;

  assert!(RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS > required_lifetime_milliseconds);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated released products and a provisioned Linux Native runner"]
async fn released_linux_native_matrix_satisfies_the_end_to_end_contract() {
  linux_native::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated server, Agent, and Octa release roots plus a provisioned Native or Microsandbox machine"]
async fn released_agent_completes_a_sequential_pipeline_through_rest_and_postgres() {
  run_released_oci_vertical_slice(None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated released products and a provisioned Linux containerd runner"]
async fn released_linux_containerd_matrix_satisfies_the_end_to_end_contract() {
  run_released_oci_vertical_slice(Some("containerd")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated released products and a provisioned Linux/KVM Microsandbox runner"]
async fn released_linux_microsandbox_matrix_satisfies_the_end_to_end_contract() {
  linux_native::run_microsandbox().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated released products and a provisioned Apple Silicon Microsandbox runner"]
async fn released_macos_microsandbox_matrix_satisfies_the_end_to_end_contract() {
  linux_native::run_microsandbox().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated released products and a provisioned Apple VF isolation runner"]
async fn released_macos_apple_vf_matrix_satisfies_the_end_to_end_contract() {
  run_released_oci_vertical_slice(Some("apple-vf-isolation")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires matching released server and console products, PostgreSQL, nginx, and Chromium"]
async fn released_console_satisfies_the_same_origin_browser_contract() {
  browser::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Server/Console, native GitHub adapter, disposable PostgreSQL/S3, nginx and Chromium"]
async fn packaged_source_only_import_creates_one_visible_canonical_issue_without_execution() {
  project_source::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Server/Console, PostgreSQL, object storage, nginx and native Source adapter"]
async fn packaged_source_gateway_preserves_scoped_issue_mcp_authority() {
  source_triage::issue_mcp().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Server/Console, PostgreSQL, object storage, nginx and native Source adapter"]
async fn packaged_triage_prepares_raw_unlabeled_input_without_a_canonical_card() {
  source_triage::raw_source().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Server/Agent/Octa, PostgreSQL, object storage, nginx and qualified Microsandbox"]
async fn packaged_source_uses_the_qualified_agent_and_installed_job_contract() {
  source_triage_execution::agent_bootstrap().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Source, reviewed triage profiles, native Codex/MCP peers and qualified Agent"]
async fn packaged_triage_uses_two_distinct_profiles_before_accepted_results() {
  source_triage_execution::pipeline(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged triage execution, native MCP peers, PostgreSQL/S3 and Chromium"]
async fn packaged_triage_console_uses_real_management_routes() {
  source_triage_execution::console().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged triage execution, native MCP peers, PostgreSQL/S3 and Chromium"]
async fn packaged_triage_console_reads_retained_results() {
  source_triage_execution::console_results().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Source, native Codex/MCP peers, accepted reproduction and qualified Agent"]
async fn packaged_triage_reproduces_raw_unlabeled_defect_before_classification() {
  source_triage_execution::pipeline(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Source installation, disposable PostgreSQL/S3, nginx and Chromium"]
async fn packaged_issue_status_mapping_delivers_without_execution() {
  issue_source_status::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Source installation, disposable PostgreSQL/S3, nginx and Chromium"]
async fn packaged_read_only_connection_with_installed_reporter_keeps_status_local() {
  issue_source_status::read_only().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Source installation, disposable PostgreSQL/S3, nginx and Chromium"]
async fn packaged_factory_source_admission_retains_canonical_issue_and_configured_status() {
  source_factory::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Source installation, disposable PostgreSQL/S3 and nginx"]
async fn packaged_failed_qualification_drop_releases_the_gateway_listener() {
  let environment = source_environment::SourceEnvironment::start().await;
  let client = environment.client.clone();
  let origin = environment.origin.clone();
  let original_evidence = environment.evidence.clone();
  drop(environment);
  let deadline = Instant::now() + Duration::from_secs(3);
  loop {
    if client.get(&origin).send().await.is_err() {
      break;
    }
    assert!(
      Instant::now() < deadline,
      "a failed qualification must not leave a gateway worker listening"
    );
    sleep(Duration::from_millis(20)).await;
  }
  let mut next = source_environment::SourceEnvironment::start().await;
  let next_evidence = next.evidence.clone();
  next.stop().await;
  assert_ne!(
    original_evidence, next_evidence,
    "a later environment must not overwrite earlier process/browser evidence"
  );
}

async fn run_released_oci_vertical_slice(expected_backend: Option<&str>) {
  if let Some(expected) = expected_backend {
    assert_eq!(required_string("OCTACITY_RELEASE_BACKEND"), expected);
  }
  let backend = ReleaseBackend::from_environment();
  let postgres_url = required_string("OCTACITY_POSTGRES_URL");
  let object_endpoint = required_string("OCTACITY_MINIO_ENDPOINT");
  let evidence = required_path("OCTACITY_RELEASE_EVIDENCE_DIR", false);
  fs::create_dir_all(&evidence).unwrap();

  let release = release::load(&backend);
  let temporary = tempfile::tempdir().unwrap();
  let client = Client::new();
  let (management_addr, agent_addr) = unused_loopback_addresses();
  let server_config = write_server_config(
    temporary.path(),
    &ServerConfigInput {
      postgres_url: &postgres_url,
      object_endpoint: &object_endpoint,
      cache_endpoint: "https://cache.example",
      toolchain: &release.toolchain,
      management_addr,
      agent_addr,
      cache_addr: None,
    },
  );
  let server_stdout = evidence.join("server.stdout.log");
  let server_stderr = evidence.join("server.stderr.log");
  let mut server = spawn_server(&release.server_binary, &server_config, &server_stdout, &server_stderr);
  let management_origin = format!("http://{management_addr}");
  let agent_origin = format!("http://{agent_addr}");
  wait_for_server_ready(&client, &management_origin, &mut server, &server_stdout).await;
  let run = Uuid::new_v4().simple().to_string();

  let resources = create_pipeline_resources(&client, &management_origin, &run, &backend).await;
  let enrollment = issue_enrollment(&client, &management_origin, &run, &resources.pool_id, &backend).await;
  let agent_id = format!("release-{}-{run}", backend.name());
  let agent_config = write_agent_config(
    temporary.path(),
    &agent_origin,
    &agent_id,
    &enrollment,
    AgentReleasePaths::from(&release),
    &backend,
    &AgentConfigOverrides::restricted_without_outputs(&[&object_endpoint]),
  );
  let workspace_baseline = backend_workspace_entries(&backend);
  let stdout_path = evidence.join("agent.stdout.log");
  let stderr_path = evidence.join("agent.stderr.log");
  let mut agent = spawn_agent(&release.agent_binary, &agent_config, &stdout_path, &stderr_path);

  let build = wait_for_terminal_build(
    &client,
    &management_origin,
    &resources.build_id,
    &mut agent,
    &stderr_path,
  )
  .await;
  assert_eq!(build["state"], "succeeded", "Build did not succeed: {build}");
  let attempt = get_json(
    &client,
    format!("{management_origin}/api/v1/attempts/{}", resources.attempt_id),
  )
  .await;
  let jobs = attempt["jobs"].as_array().expect("Attempt jobs must be an array");
  assert_eq!(jobs.len(), 2, "the release slice must execute two DAG nodes");
  assert!(jobs.iter().all(|job| job["state"] == "succeeded"));
  assert!(jobs.iter().all(|job| job["event_cursor"].as_u64().unwrap_or(0) > 0));

  let mut job_evidence = Vec::new();
  for job in jobs {
    let job_id = string(job, "id");
    let detail = get_json(&client, format!("{management_origin}/api/v1/jobs/{job_id}")).await;
    assert_eq!(detail["terminal"]["state"], "succeeded");
    let events = get_json(
      &client,
      format!("{management_origin}/api/v1/jobs/{job_id}/events?after=0&limit=100&wait_ms=0"),
    )
    .await;
    assert!(!events["items"].as_array().unwrap().is_empty());
    job_evidence.push(json!({"job": detail, "events": events}));
  }

  drain_agent(&client, &management_origin, &agent_id, &run).await;
  let status = timeout(Duration::from_secs(45), agent.wait())
    .await
    .expect("released Agent did not stop after drain")
    .expect("failed to wait for released Agent");
  assert!(
    status.success(),
    "released Agent exited unsuccessfully; see {stderr_path:?}"
  );
  assert_eq!(
    backend_workspace_entries(&backend),
    workspace_baseline,
    "the released Agent left backend workspace state behind"
  );

  fs::write(
    evidence.join("vertical-slice.json"),
    serde_json::to_vec_pretty(&json!({
      "backend": backend.name(),
      "server_release": release.server_manifest,
      "agent_release": release.agent_manifest,
      "octa_capabilities": release.octa_capabilities,
      "build": build,
      "attempt": attempt,
      "jobs": job_evidence
    }))
    .unwrap(),
  )
  .unwrap();
  shutdown_server(&mut server, &server_stdout).await;
}

struct PipelineResources {
  pool_id: String,
  build_id: String,
  attempt_id: String,
}

async fn create_pipeline_resources(
  client: &Client,
  origin: &str,
  run: &str,
  backend: &ReleaseBackend,
) -> PipelineResources {
  let pool = post_management(
    client,
    origin,
    "/api/v1/agent-pools",
    &format!("{run}-pool"),
    json!({
      "name": format!("release-{}-{run}", backend.name()),
      "definition": {
        "enabled": true,
        "drain_state": "accepting",
        "admission_policy": backend.pool_admission_policy(),
        "concurrency_limit": 1,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": 1
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
    json!({
      "parent_id": null,
      "name": format!("release-{run}")
    }),
  )
  .await;
  let project_id = resource_id(&project);
  let pipeline = post_management(
    client,
    origin,
    "/api/v1/pipelines",
    &format!("{run}-pipeline"),
    json!({
      "project_id": project_id,
      "name": "release",
      "dag": {
        "nodes": [pipeline_node("build", "Build", backend), pipeline_node("test", "Test", backend)],
        "edges": [{"predecessor": "build", "dependent": "test"}]
      }
    }),
  )
  .await;
  let pipeline_id = resource_id(&pipeline);
  let repository = post_management(
    client,
    origin,
    "/api/v1/repositories",
    &format!("{run}-repository"),
    json!({
      "project_id": project_id,
      "name": "octa-fixture",
      "definition": {
        "vcs_integration_id": Uuid::new_v4().to_string(),
        "repository_locator": SOURCE_REPOSITORY,
        "selection": {"allowed_references": [], "default_reference": null, "allow_exact_revision": true}
      }
    }),
  )
  .await;
  let repository_id = resource_id(&repository);
  let configuration = post_management(
    client,
    origin,
    "/api/v1/build-configurations",
    &format!("{run}-configuration"),
    build_configuration(&project_id, &repository_id, &pipeline_id, &pool_id, backend),
  )
  .await;
  let configuration_id = resource_id(&configuration);
  let trigger_id = publish_policy_and_trigger_definition(
    client,
    origin,
    &project_id,
    &repository_id,
    &configuration_id,
    &pool_id,
    backend,
  )
  .await;
  let trigger = post_management(
    client,
    origin,
    "/api/v1/triggers/manual",
    &format!("{run}-trigger"),
    json!({
      "trigger_id": trigger_id,
      "trigger_version": 1,
      "configuration_id": configuration_id,
      "configuration_version": 1,
      "deduplication_identity": format!("{run}-trigger"),
      "source": {"kind": "exact_revision", "value": SOURCE_REVISION.trim()},
      "parameters": {},
      "priority": 50
    }),
  )
  .await;
  assert_eq!(trigger["outcome"], "accepted");
  PipelineResources {
    pool_id,
    build_id: string(&trigger, "build_id"),
    attempt_id: string(&trigger, "attempt_id"),
  }
}

fn pipeline_node(id: &str, name: &str, backend: &ReleaseBackend) -> Value {
  json!({
    "id": id,
    "name": name,
    "dependency_policy": "all_succeeded",
    "required_capabilities": backend.required_capabilities(),
    "execution": {
      "octafile": "example/simple/Octafile.yml",
      "commands": ["echo"],
      "parallel": false,
      "failfast": true
    }
  })
}

fn build_configuration(project: &str, repository: &str, pipeline: &str, pool: &str, backend: &ReleaseBackend) -> Value {
  json!({
    "project_id": project,
    "name": "release",
    "definition": {
      "enabled": true,
      "job_concurrency_limit": 1,
      "repository_id": repository,
      "repository_version": 1,
      "pipeline_id": pipeline,
      "pipeline_version": 1,
      "parameters": {"parameters": {}, "deny_unknown": true},
      "triggers": ["manual"],
      "agent_requirements": {
        "capabilities": backend.required_capabilities(),
        "labels": {},
        "minimum_cpu_millis": 1000,
        "minimum_memory_bytes": 536870912_u64,
        "minimum_disk_bytes": backend.workspace_bytes()
      },
      "allowed_pools": [pool],
      "runtime": {
        "class": backend.runtime_class(),
        "operating_system": "linux",
        "architecture": backend.guest_architecture(),
        "host_platform": backend.host_platform(),
        "required_guarantees": backend.required_guarantees(),
        "immutable_image": backend.immutable_image(),
        "cpu_millis": 1000,
        "memory_bytes": 536870912_u64,
        "writable_disk_bytes": backend.workspace_bytes(),
        "timeout_seconds": RELEASE_JOB_TIMEOUT_SECONDS,
        "network": {"mode": "disabled"},
        "workload_identity_profile": null
      },
      "cache": {"namespace": null, "read": false, "write": false},
      "artifacts": {"artifact_count": 0, "artifact_bytes": 0, "report_count": 0, "report_bytes": 0, "single_output_bytes": 0},
      "retry": {"max_attempts": 1, "retry_on": []}
    }
  })
}

async fn issue_enrollment(client: &Client, origin: &str, run: &str, pool: &str, backend: &ReleaseBackend) -> String {
  let (operating_system, architecture) = backend.agent_platform();
  let response = post_management(
    client,
    origin,
    "/api/v1/agent-enrollments",
    &format!("{run}-enrollment"),
    json!({
      "pool_id": pool,
      "pool_version": 1,
      "expected_platform": {"operating_system": operating_system, "architecture": architecture}
    }),
  )
  .await;
  string(&response, "credential")
}

async fn wait_for_terminal_build(
  client: &Client,
  origin: &str,
  build_id: &str,
  agent: &mut Child,
  log: &Path,
) -> Value {
  let deadline = Instant::now() + Duration::from_secs(600);
  loop {
    if let Some(status) = agent.try_wait().unwrap() {
      panic!(
        "released Agent exited before Build completion with {status}: {}",
        fs::read_to_string(log).unwrap_or_default()
      );
    }
    let build = get_json(client, format!("{origin}/api/v1/builds/{build_id}")).await;
    if matches!(build["state"].as_str(), Some("succeeded" | "failed" | "cancelled")) {
      return build;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for released Agent Build; see {log:?}"
    );
    sleep(Duration::from_millis(500)).await;
  }
}

async fn drain_agent(client: &Client, origin: &str, agent_name: &str, run: &str) {
  let page = get_json(client, format!("{origin}/api/v1/agents?limit=100")).await;
  let agent = page["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|agent| agent["name"] == agent_name)
    .unwrap_or_else(|| panic!("registered Agent '{agent_name}' was not queryable"));
  let response = client
    .post(format!("{origin}/api/v1/agents/{}/drain", string(agent, "id")))
    .header("idempotency-key", format!("{run}-drain"))
    .header("if-match", format!("\"{}\"", agent["version"].as_u64().unwrap()))
    .json(&json!({"mode": "graceful"}))
    .send()
    .await
    .unwrap();
  let status = response.status();
  let body = response.bytes().await.unwrap();
  assert!(
    status.is_success(),
    "Agent drain returned {status}: {}",
    String::from_utf8_lossy(&body)
  );
}

fn backend_workspace_entries(backend: &ReleaseBackend) -> Vec<std::ffi::OsString> {
  let work_root = match backend {
    ReleaseBackend::Native { work_root, .. }
    | ReleaseBackend::Microsandbox { work_root, .. }
    | ReleaseBackend::Containerd { work_root, .. }
    | ReleaseBackend::AppleVf { work_root, .. } => work_root,
  };
  let mut entries = fs::read_dir(work_root)
    .unwrap_or_else(|error| panic!("failed to inspect backend work root {work_root:?}: {error}"))
    .map(|entry| entry.map(|entry| entry.file_name()))
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
  entries.sort();
  entries
}

#[test]
fn generated_server_configuration_preserves_release_infrastructure_contract() {
  let directory = tempfile::tempdir().unwrap();
  let toolchain = Toolchain {
    source_version: "0.1.0".to_owned(),
    source_digest: "1".repeat(64),
    octa_version: "0.3.0".to_owned(),
    runner_digest: "2".repeat(64),
    runner_protocol: 1,
    event_schema: 1,
    plugin_protocol: 1,
    plugin_digests: std::collections::BTreeMap::from([("shell".to_owned(), "3".repeat(64))]),
  };
  let config = write_server_config(
    directory.path(),
    &ServerConfigInput {
      postgres_url: "postgres://octacity@127.0.0.1/postgres",
      object_endpoint: "http://127.0.0.1:9000",
      cache_endpoint: "https://127.0.0.1:8443",
      toolchain: &toolchain,
      management_addr: "127.0.0.1:18080".parse().unwrap(),
      agent_addr: "127.0.0.1:18081".parse().unwrap(),
      cache_addr: None,
    },
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  assert_eq!(document["object_storage"]["force_path_style"].as_bool(), Some(true));
  assert_eq!(
    document["object_storage"]["operation_timeout_milliseconds"].as_integer(),
    Some(RELEASE_OBJECT_OPERATION_TIMEOUT_MILLISECONDS as i64)
  );
  assert_eq!(
    document["readiness_check_timeout_milliseconds"].as_integer(),
    Some(RELEASE_READINESS_TIMEOUT_MILLISECONDS as i64)
  );
  assert_eq!(
    document["agent_lease_lifetime_milliseconds"].as_integer(),
    Some(RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS as i64)
  );
  assert_eq!(
    document["cache"]["session_lifetime_milliseconds"].as_integer(),
    Some(RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS as i64)
  );
}
