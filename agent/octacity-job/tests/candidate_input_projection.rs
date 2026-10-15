//! Physical candidate-input privacy through the public stager and released runner.
//! The canaries qualify file projection, not Git reconstruction or a complete pilot.

#![cfg(unix)]

use std::{collections::BTreeMap, os::unix::fs::PermissionsExt as _, path::PathBuf, sync::Arc, time::Duration};

use octacity_execution::{
  ExecutionArchitecture, ExecutionOs, ExecutionPlatform, ExecutionTarget, FactoryExecutionLayout, NetworkAccess,
  OciIsolation, StartExecution,
};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_oci::OciBackend;
use octacity_job::{ProtectedInputStager, ProtectedInputStagerConfig};
use octacity_protocol::{
  ArtifactTransferCapability, CHANGE_SET_BUNDLE_INPUT, CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_MANIFEST_INPUT,
  CHANGE_SET_MANIFEST_MEDIA_TYPE, ChangeSetMaterializationV3, ExecutionSpec, OctaSpec, ProtectedInputManifestV3,
  ProtectedInputTransferV3, ProtectedInputV3,
};
use octacity_runner::{
  RunStatus, RunnerInstallation, RunnerJobRequest, RunnerRedactions, RunnerSupervisionPolicy, supervise,
};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

const PUBLIC_INPUT: &str = "/octacity/protected/public.txt";
const PUBLIC_BODY: &[u8] = b"declared-model-context";
const BUNDLE_BODY: &[u8] = b"host-only-binary-candidate-canary\0\xff";
const MANIFEST_BODY: &[u8] = b"{\"host_only_capture_canary\":true}";

fn required(name: &str) -> String {
  std::env::var(name).unwrap_or_else(|_| panic!("{name} is required for this explicit contract"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the pinned Microsandbox/firmware/image and LinuxARM Octa release; see docs/testing/backend-contracts.md"]
async fn candidate_inputs_remain_host_readable_and_are_absent_from_the_real_guest() {
  let work_root = PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT"));
  let fixture = tempfile::Builder::new()
    .prefix("candidate-")
    .tempdir_in(&work_root)
    .unwrap();
  let job_root = fixture.path();
  std::fs::set_permissions(job_root, std::fs::Permissions::from_mode(0o700)).unwrap();
  let source = job_root.join("source");
  let scratch = job_root.join("scratch");
  let output = job_root.join("output");
  let data = scratch.join("data");
  for path in [&source, &scratch, &output, &data] {
    octacity_private_fs::create_private_directory(path).unwrap();
  }
  let octafile = format!(
    "version: 1\ntasks:\n  inspect:\n    shell: |\n      set -eu\n      test \"$(cat {PUBLIC_INPUT})\" = declared-model-context\n      test ! -e {CHANGE_SET_BUNDLE_INPUT}\n      test ! -e {CHANGE_SET_MANIFEST_INPUT}\n      test ! -e /workspace/protected\n      test ! -e /workspace/runner-protected\n      ! printf tamper >> {PUBLIC_INPUT}\n"
  );
  // The ordinary Octafile is runner configuration, not a model-owned artifact.
  std::fs::write(source.join("Octafile.yml"), octafile).unwrap();
  let mut bodies = [
    ("public", PUBLIC_INPUT, "text/plain", PUBLIC_BODY),
    (
      "bundle",
      CHANGE_SET_BUNDLE_INPUT,
      CHANGE_SET_BUNDLE_MEDIA_TYPE,
      BUNDLE_BODY,
    ),
    (
      "manifest",
      CHANGE_SET_MANIFEST_INPUT,
      CHANGE_SET_MANIFEST_MEDIA_TYPE,
      MANIFEST_BODY,
    ),
  ];
  bodies.sort_by_key(|(id, _, _, _)| *id);
  let (origin, server) = input_server(&bodies).await;
  let expires_at_unix_ms = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as u64
    + 120_000;
  let transfers: Vec<_> = bodies
    .iter()
    .map(|(id, destination, media_type, bytes)| ProtectedInputTransferV3 {
      input: ProtectedInputV3 {
        artifact_id: (*id).to_owned(),
        size_bytes: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(bytes)),
        media_type: (*media_type).to_owned(),
        destination: (*destination).to_owned(),
      },
      capability: ArtifactTransferCapability {
        url: format!("{origin}/{id}"),
        required_headers: BTreeMap::new(),
        expires_at_unix_ms,
      },
    })
    .collect();
  // These are the immutable manifest facts supplied after JobSpec verification.
  // Cryptographic JobSpec verification is covered by the Job execution contract.
  let manifest = ProtectedInputManifestV3 {
    inputs: transfers.iter().map(|transfer| transfer.input.clone()).collect(),
  };
  manifest.validate().unwrap();
  let cancellation = CancellationToken::new();
  let stage_result = ProtectedInputStager::new(ProtectedInputStagerConfig {
    allowed_origins: vec![origin],
    download_timeout: Duration::from_secs(5),
  })
  .unwrap()
  .stage(&transfers, job_root, &cancellation)
  .await;
  // Always dispose the owned transport, including a failed staging attempt.
  server.abort();
  let _ = server.await;
  let mut staged = stage_result.unwrap();
  let materialization = ChangeSetMaterializationV3 {
    bundle_input: "bundle".to_owned(),
    manifest_input: "manifest".to_owned(),
    candidate_revision: "1234567890123456789012345678901234567890".to_owned(),
  };
  materialization.validate(&manifest).unwrap();
  staged.prepare_runner_projection(&materialization).unwrap();
  assert_eq!(
    std::fs::read(staged.path_for_input("bundle").unwrap()).unwrap(),
    BUNDLE_BODY
  );
  assert_eq!(
    std::fs::read(staged.path_for_input("manifest").unwrap()).unwrap(),
    MANIFEST_BODY
  );
  let protected_inputs = staged.revalidate_for_runner().unwrap().to_owned();

  let runner = RunnerInstallation::load(&PathBuf::from(required("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT"))).unwrap();
  let engine = MicrosandboxEngine::new(MicrosandboxEngineConfig {
    agent_id: "candidate-input-contract".to_owned(),
    state_root: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT")),
    work_root: work_root.clone(),
    runner_platform: "linux-aarch64".to_owned(),
    executable: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE")),
    libkrunfw: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW")),
    cleanup_timeout: Duration::from_secs(10),
    metrics_sample_interval: Duration::from_secs(1),
  })
  .unwrap();
  let backend = OciBackend::new(vec![Arc::new(engine)]).unwrap();
  let request = RunnerJobRequest {
    request_id: "candidate-input-privacy".to_owned(),
    octa: OctaSpec {
      version: runner.capabilities.octa_version.clone(),
      runner_sha256: runner.sha256.clone(),
      runner_protocol: *runner.capabilities.runner_protocols.iter().max().unwrap(),
      event_schema: *runner.capabilities.event_schemas.iter().max().unwrap(),
      plugin_protocol: *runner.capabilities.plugin_protocols.iter().max().unwrap(),
      plugin_digests: runner
        .plugins
        .iter()
        .map(|(name, plugin)| (name.clone(), plugin.sha256.clone()))
        .collect(),
    },
    execution: StartExecution {
      execution_id: "candidate-input-privacy".to_owned(),
      workspace_root: work_root,
      workspace: source.clone(),
      data_dir: data,
      workload_identity: None,
      cache: None,
      factory: Some(FactoryExecutionLayout {
        workspace_anchor: false,
        disk_accounting: octacity_execution::ExecutionDiskAccounting::WholeJob,
        protected_inputs,
        source,
        source_visible: true,
        source_read_only: true,
        source_writable_paths: vec![],
        tool_channel: None,
        scratch,
        output,
        // Admit the released runner/plugin thread trees; PID enforcement is
        // qualified separately with an exact small limit and a fork probe.
        process_limit: 64,
      }),
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 64 * 1024 * 1024,
      max_duration: Duration::from_secs(60),
      root: ExecutionTarget::Oci {
        reference: required("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE"),
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Arm64,
        },
        isolation: OciIsolation::Hypervisor,
      },
      network: NetworkAccess::Disabled,
    },
    spec: ExecutionSpec {
      octafile: None,
      commands: vec!["inspect".to_owned()],
      variables: BTreeMap::new(),
      arguments: vec![],
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    cache: None,
    redactions: RunnerRedactions::default(),
    cancellation_grace: Duration::from_secs(2),
    external_executables: BTreeMap::new(),
  };
  let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
  let events = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
  // Public supervision owns the execution deadline, kill and mandatory destroy.
  // Never timeout/drop this future and strand a VM outside its cleanup path.
  let completion = supervise(
    &runner,
    &backend,
    request,
    cancellation,
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await;
  drop(sender);
  events.await.unwrap();
  assert_eq!(
    std::fs::read(staged.path_for_input("bundle").unwrap()).unwrap(),
    BUNDLE_BODY
  );
  assert_eq!(
    std::fs::read(staged.path_for_input("manifest").unwrap()).unwrap(),
    MANIFEST_BODY
  );
  staged.cleanup().unwrap();
  let completion = completion.unwrap();
  assert_eq!(
    completion.status,
    RunStatus::Succeeded,
    "guest public read or source privacy failed"
  );
  assert!(completion.termination_reason.is_none());
}

async fn input_server(bodies: &[(&str, &str, &str, &[u8])]) -> (String, tokio::task::JoinHandle<()>) {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin = format!("http://{}", listener.local_addr().unwrap());
  let replies: Vec<_> = bodies
    .iter()
    .map(|(_, _, media_type, bytes)| (media_type.to_string(), bytes.to_vec()))
    .collect();
  let server = tokio::spawn(async move {
    for (media_type, bytes) in replies {
      let (mut peer, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .unwrap()
        .unwrap();
      let mut request = [0; 4096];
      let mut received = 0;
      while !request[..received].ends_with(b"\r\n\r\n") {
        assert!(received < request.len(), "fixture HTTP headers exceed their bound");
        let count = peer.read(&mut request[received..]).await.unwrap();
        assert_ne!(count, 0, "fixture HTTP headers ended early");
        received += count;
      }
      peer
        .write_all(
          format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {media_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      peer.write_all(&bytes).await.unwrap();
    }
  });
  (origin, server)
}
