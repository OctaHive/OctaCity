//! Deterministic Codex executable scenarios for the released Native Agent.

use std::{collections::BTreeSet, ffi::OsString, fs, path::Path};

use octacity_release_harness::InstalledRelease;
use reqwest::Client;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::*;

pub(super) struct CodexScenarioInput<'a> {
  pub(super) client: &'a Client,
  pub(super) management_origin: &'a str,
  pub(super) agent_origin: &'a str,
  pub(super) temporary: &'a Path,
  pub(super) evidence: &'a Path,
  pub(super) run_id: &'a str,
  pub(super) resources: &'a MatrixResources,
  pub(super) backend: &'a ReleaseBackend,
  pub(super) release: &'a InstalledRelease,
  pub(super) cache_proxy: &'a TlsCacheProxy,
  pub(super) object_endpoint: &'a str,
  pub(super) revision: &'a str,
  pub(super) native_roots: Option<(&'a Path, &'a Path)>,
  pub(super) cgroup_baseline: Option<&'a BTreeSet<OsString>>,
  pub(super) work_root: &'a Path,
  pub(super) server_stdout: &'a Path,
  pub(super) server_stderr: &'a Path,
}

struct CodexResources {
  successful_configuration: String,
  successful_trigger: String,
  overflow_configuration: String,
  overflow_trigger: String,
  cancellation_configuration: String,
  cancellation_trigger: String,
}

pub(super) async fn run(input: CodexScenarioInput<'_>) -> Option<Value> {
  if !fixture_enabled() || !matches!(input.backend, ReleaseBackend::Native { .. }) {
    return None;
  }
  let fixture = Path::new(env!("CARGO_BIN_EXE_octacity-codex-fixture"));
  let digest = hex::encode(Sha256::digest(fs::read(fixture).unwrap()));
  let tool = AgentToolExecutable {
    product: "codex-cli",
    path: fixture,
    version: "0.130.0",
    platform: &input.release.octa_platform,
    sha256: &digest,
  };
  let resources = create_resources(
    input.client,
    input.management_origin,
    input.run_id,
    input.resources,
    input.backend,
  )
  .await;
  let agent_name = format!("release-{}-codex-{}", input.backend.name(), input.run_id);
  let mut agent = start_matrix_agent(AgentStartInput {
    client: input.client,
    management_origin: input.management_origin,
    agent_origin: input.agent_origin,
    temporary: input.temporary,
    evidence: input.evidence,
    evidence_name: "agent-codex",
    agent_name: &agent_name,
    run: input.run_id,
    pool_id: &input.resources.pool_id,
    release: input.release,
    backend: input.backend,
    cache_proxy: input.cache_proxy,
    object_endpoint: input.object_endpoint,
    tool_executable: Some(&tool),
  })
  .await;
  performance::wait_for_agent_registration(input.client, input.management_origin, &agent_name).await;
  let agent_stderr = input.evidence.join("agent-codex.stderr.log");
  let agent_stdout = input.evidence.join("agent-codex.stdout.log");
  let identity = format!("{}-codex", input.run_id);
  let accepted = accept_manual_build(
    input.client,
    input.management_origin,
    &identity,
    &resources.successful_trigger,
    &resources.successful_configuration,
    input.revision,
  )
  .await;
  let successful = wait_for_successful_run(
    input.client,
    input.management_origin,
    &string(&accepted, "build_id"),
    agent.child_mut(),
    &agent_stderr,
  )
  .await;
  assert_run_has_no_codex_sensitive_text(&successful);
  let outputs = verify_codex_outputs(input.client, input.management_origin, &successful.build).await;

  let overflow = accept_manual_build(
    input.client,
    input.management_origin,
    &format!("{}-codex-overflow", input.run_id),
    &resources.overflow_trigger,
    &resources.overflow_configuration,
    input.revision,
  )
  .await;
  let overflow_build = wait_for_terminal_build(
    input.client,
    input.management_origin,
    &string(&overflow, "build_id"),
    agent.child_mut(),
    &agent_stderr,
  )
  .await;
  assert_eq!(overflow_build["state"], "failed");
  let overflow_outputs = verify_no_outputs(input.client, input.management_origin, &overflow_build).await;
  let overflow_events =
    verify_failed_codex_events(input.client, input.management_origin, &overflow, &overflow_build).await;
  if let (Some((_, cgroup_root)), Some(baseline)) = (input.native_roots, input.cgroup_baseline) {
    wait_for_directory_baseline(cgroup_root, baseline, "Codex overflow cgroup").await;
  }

  let cancelled = run_and_cancel(
    CancellationInput {
      build: ManualBuildInput {
        client: input.client,
        origin: input.management_origin,
        run: &identity,
        trigger_id: &resources.cancellation_trigger,
        configuration: &resources.cancellation_configuration,
        revision: input.revision,
        stderr: &agent_stderr,
      },
      native_cgroup: input
        .native_roots
        .zip(input.cgroup_baseline)
        .map(|((_, root), baseline)| NativeCgroupAssertion { root, baseline }),
      ready_marker: Some(ReadyMarkerAssertion {
        root: input.work_root,
        file_name: "codex-fixture-descendant-ready",
      }),
      maximum_disk_bytes: input.backend.workspace_bytes(),
    },
    agent.child_mut(),
  )
  .await;
  let cancelled_outputs = verify_no_outputs(input.client, input.management_origin, &cancelled["build"]).await;
  if let (Some((_, cgroup_root)), Some(baseline)) = (input.native_roots, input.cgroup_baseline) {
    wait_for_directory_baseline(cgroup_root, baseline, "Codex cancellation cgroup").await;
  }

  let readiness = accept_manual_build(
    input.client,
    input.management_origin,
    &format!("{}-post-codex-readiness", input.run_id),
    &input.resources.manual_trigger_id,
    &input.resources.manual_configuration_id,
    input.revision,
  )
  .await;
  let readiness = wait_for_successful_run(
    input.client,
    input.management_origin,
    &string(&readiness, "build_id"),
    agent.child_mut(),
    &agent_stderr,
  )
  .await;
  assert!(readiness.build.get("factory").is_none());
  assert!(
    input
      .client
      .get(format!("{}/health/ready", input.management_origin))
      .send()
      .await
      .is_ok_and(|response| response.status().is_success())
  );
  stop_agent(
    input.client,
    input.management_origin,
    &agent_name,
    input.run_id,
    &mut agent,
    &agent_stderr,
  )
  .await;
  for path in [input.server_stdout, input.server_stderr, &agent_stdout, &agent_stderr] {
    assert!(
      !fs::read_to_string(path)
        .unwrap_or_default()
        .contains("release-secret-canary-must-not-appear"),
      "Codex credential leaked to {}",
      path.display()
    );
  }
  let evidence = json!({
    "successful": {"build": successful.build, "outputs": outputs},
    "overflow": {"build": overflow_build, "outputs": overflow_outputs, "events": overflow_events},
    "cancelled": {"run": cancelled, "outputs": cancelled_outputs},
    "ordinary_readiness": readiness.build,
  });
  assert!(!evidence.to_string().contains("release-secret-canary-must-not-appear"));
  Some(evidence)
}

async fn create_resources(
  client: &Client,
  origin: &str,
  run: &str,
  resources: &MatrixResources,
  backend: &ReleaseBackend,
) -> CodexResources {
  async fn create(
    client: &Client,
    origin: &str,
    run: &str,
    resources: &MatrixResources,
    backend: &ReleaseBackend,
    name: &str,
    task: &str,
  ) -> (String, String) {
    let pipeline = create_pipeline(client, origin, run, &resources.project_id, name, &[task], backend).await;
    let input = ConfigurationInput {
      client,
      origin,
      run,
      name,
      project_id: &resources.project_id,
      repository_id: &resources.repository_id,
      pipeline_id: &pipeline,
      pool_id: &resources.pool_id,
      triggers: &["manual"],
      cache: false,
      artifact_count: 2,
      backend,
    };
    let configuration = post_management(
      client,
      origin,
      "/api/v1/build-configurations",
      &format!("{run}-{name}-configuration"),
      matrix_configuration_body(&input),
    )
    .await;
    let configuration = resource_id(&configuration);
    let trigger = create_manual_definition(client, origin, run, name, &configuration).await;
    (configuration, trigger)
  }

  let (successful_configuration, successful_trigger) =
    create(client, origin, run, resources, backend, "codex", "codex-fixture").await;
  let (overflow_configuration, overflow_trigger) = create(
    client,
    origin,
    run,
    resources,
    backend,
    "codex-overflow",
    "codex-overflow",
  )
  .await;
  let (cancellation_configuration, cancellation_trigger) =
    create(client, origin, run, resources, backend, "codex-cancel", "codex-cancel").await;
  CodexResources {
    successful_configuration,
    successful_trigger,
    overflow_configuration,
    overflow_trigger,
    cancellation_configuration,
    cancellation_trigger,
  }
}

fn fixture_enabled() -> bool {
  std::env::var("OCTACITY_RELEASE_CODEX_FIXTURE").is_ok_and(|value| value == "1")
}
