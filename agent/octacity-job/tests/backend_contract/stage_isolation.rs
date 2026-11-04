//! Two ordinary Jobs with disjoint executable, Artifact and network grants.
use super::*;
use octacity_execution::FACTORY_SCRATCH_ROOT;
use octacity_job::JobCompletion;
use octacity_runner::VerifiedExternalExecutable;
use std::os::unix::fs::PermissionsExt;

const TOOL: &str = "stage-tool";
const TEST_BYTES: &[u8] = b"private-test-canary";
const TEST_PATH: &str = "/octacity/protected/tests/contract.yml";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a qualified Microsandbox and its released Linux Octa bundle"]
async fn selected_stage_isolation_denies_ungranted_artifact_tool_and_network() {
  let root = required_path("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT");
  // The path is later supplied as data in a shell probe.
  let work = tempfile::Builder::new()
    .prefix("stage's work-")
    .tempdir_in(root)
    .unwrap();
  let bytes = required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES");
  let runner = RunnerInstallation::load(&required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT")).unwrap();
  let host = required_network_host("OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST");
  let mut ordinary = oci_v2_specification(
    ExecutionMode::Virtualization,
    host_platform(),
    linux_platform(),
    required_string("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE"),
    bytes,
    unix_now(),
    &runner,
  );
  ordinary.runtime.network = NetworkPolicy::Restricted {
    allowed_hosts: vec![host.clone()],
  };
  let trusted_body = octafile(&[
    &format!("test \"$(cat {TEST_PATH})\" = private-test-canary"),
    &format!("cp {TEST_PATH} {FACTORY_SCRATCH_ROOT}/protected-test"),
    &format!("test \"$(/opt/octacity/tools/{TOOL})\" = stage-tool-canary"),
    &format!("curl --silent --show-error --max-time 10 https://{host}/ >/dev/null"),
  ]);
  let mut trusted = oci_v3_specification(&ordinary, &runner, &trusted_body);
  trusted.job_id = "isolated-author".to_owned();
  let (installation, tool) = stage_tool(&runner);
  trusted.permissions.executables.push(FactoryImmutableReferenceV3 {
    identity: tool.product.clone(),
    version: tool.version.clone(),
    sha256: tool.sha256.clone(),
  });
  let mut configuration = executor_config(work.path().to_owned(), bytes, false, vec![host.clone()]);
  configuration.factory_permissions = Some(trusted.permissions.clone());
  configuration.external_executables.insert(tool.product.clone(), tool);
  let backend = Arc::new(OciBackend::new(vec![managed_microsandbox_engine(work.path(), "stage-isolation")]).unwrap());
  let route = ExecutionBackendRoute::new(
    ExecutionCapabilityV2 {
      provider: ExecutionProviderId::new(MICROSANDBOX_ENGINE_NAME).unwrap(),
      mode: ExecutionMode::Virtualization,
      host_platform: host_platform(),
      target_platform: linux_platform(),
      guarantees: guarantees_for(ExecutionMode::Virtualization),
      immutable_images: true,
    },
    ExecutionEnvironmentId::new("stage-isolation-contract").unwrap(),
    backend,
  )
  .unwrap()
  .with_factory_enforcement(FactoryEnforcementCapabilityV3::ALL)
  .unwrap();
  let executor = JobExecutor::new(
    runner.clone(),
    Arc::new(FixtureSource { oci_probe: None }),
    Arc::new(FileWorkloadIdentityProvider::new(BTreeMap::new())),
    BTreeMap::new(),
    configuration,
  )
  .unwrap()
  .with_execution_backends([route])
  .unwrap();
  executor.cleanup_orphans().await.unwrap();
  let (origin, private_server) = protected_input_server(TEST_BYTES.to_vec()).await;
  let mut private = protected_transfer(TEST_BYTES, &origin);
  private.input.artifact_id = "protected-test-artifact".to_owned();
  private.input.destination = TEST_PATH.to_owned();
  private.capability.url = format!("{origin}/{}", private.input.artifact_id);
  let (executor, author) = run_job(executor, trusted, trusted_body, vec![private], vec![origin]).await;
  private_server.await.unwrap();
  let retained = author.workspace().parent().unwrap().join("scratch/protected-test");
  assert_eq!(std::fs::read(&retained).unwrap(), TEST_BYTES);
  let retained = retained.to_str().unwrap().replace('\'', "'\"'\"'");

  ordinary.runtime.network = NetworkPolicy::Disabled;
  let coder_body = octafile(&[
    &format!("if cat '{retained}' >/dev/null 2>&1; then echo 'foreign workspace readable'; exit 1; fi"),
    &format!("if cat {TEST_PATH} >/dev/null 2>&1; then echo 'protected Artifact readable'; exit 1; fi"),
    &format!("if /opt/octacity/tools/{TOOL} >/dev/null 2>&1; then echo 'undeclared tool runnable'; exit 1; fi"),
    &format!(
      "if curl --silent --max-time 3 https://{host}/ >/dev/null 2>&1; then echo 'undeclared host reachable'; exit 1; fi"
    ),
  ]);
  let mut coder = oci_v3_specification(&ordinary, &runner, &coder_body);
  coder.job_id = "isolated-coder".to_owned();
  let (executor, coder) = run_job(executor, coder, coder_body, vec![], vec![]).await;
  coder.cleanup().await.unwrap();
  author.cleanup().await.unwrap();
  executor.cleanup_orphans().await.unwrap();
  drop(installation);
}

fn octafile(commands: &[&str]) -> Vec<u8> {
  format!(
    "version: 1\ntasks:\n  contract:\n    shell: |\n      set -eu\n      {}\n",
    commands.join("\n      ")
  )
  .into_bytes()
}

fn stage_tool(runner: &RunnerInstallation) -> (tempfile::TempDir, VerifiedExternalExecutable) {
  // Exercise the projection of an operator-verified tool independently of plugin discovery.
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join(TOOL);
  let script = b"#!/bin/sh\nprintf '%s\\n' stage-tool-canary\n";
  std::fs::write(&path, script).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
  let tool = VerifiedExternalExecutable {
    guest_filename: Some(TOOL.to_owned()),
    product: TOOL.to_owned(),
    version: "fixture-v1".to_owned(),
    platform: runner.capabilities.platform.clone(),
    executable: path.canonicalize().unwrap(),
    sha256: hex::encode(Sha256::digest(script)),
    selection_environment: "OCTA_TEST_STAGE_TOOL".to_owned(),
  };
  (directory, tool)
}

async fn run_job(
  executor: JobExecutor,
  mut spec: JobSpecV3,
  body: Vec<u8>,
  mut transfers: Vec<ProtectedInputTransferV3>,
  mut origins: Vec<String>,
) -> (JobExecutor, JobCompletion) {
  let (origin, server) = protected_input_server(body.clone()).await;
  transfers.push(protected_transfer(&body, &origin));
  transfers.sort_by(|left, right| left.input.artifact_id.cmp(&right.input.artifact_id));
  spec.protected_inputs.inputs = transfers.iter().map(|transfer| transfer.input.clone()).collect();
  let spec = spec.into();
  validate_contract_job(&spec);
  origins.push(origin);
  let executor = executor.with_protected_input_stager(protected_stager(origins));
  let (events, _receiver) = mpsc::channel(128);
  let completed = executor
    .execute(
      ExecuteJobRequest {
        spec,
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: transfers,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap();
  server.await.unwrap();
  assert_eq!(
    completed.runner().status,
    RunStatus::Succeeded,
    "{:?}",
    completed.runner()
  );
  (executor, completed)
}
