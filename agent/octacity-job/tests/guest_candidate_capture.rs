//! Physical write-prefix confinement followed by real trusted Git capture/restore.
//! This component contract does not grant a signed author role or run a model.

#![cfg(unix)]

use std::{
  collections::BTreeMap,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use octacity_changeset::{
  CaptureRequest, ChangeSetCapturer as _, ChangeSetMaterializer as _, GitCaptureTool, GitChangeSetCapturer,
  GitChangeSetMaterializer, MaterializationRequest,
};
use octacity_execution::{
  ExecutionArchitecture, ExecutionOs, ExecutionPlatform, ExecutionTarget, FactoryExecutionLayout, NetworkAccess,
  OciIsolation, StartExecution,
};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_oci::OciBackend;
use octacity_protocol::{
  CHANGE_SET_MANIFEST_OUTPUT, ChangeSetBinaryPolicyV3, ChangeSetCapturePolicyV3, ChangeSetCaptureV3,
  ChangeSetSecretPatternV3, ExecutionSpec, FactoryImmutableReferenceV3, OctaSpec,
};
use octacity_runner::{
  RunStatus, RunnerInstallation, RunnerJobRequest, RunnerRedactions, RunnerSupervisionPolicy, supervise,
};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

fn required(name: &str) -> String {
  std::env::var(name).unwrap_or_else(|_| panic!("{name} is required for this explicit component contract"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires pinned Microsandbox/firmware/image, released LinuxARM Octa, and operator-selected Git"]
async fn real_guest_change_is_confined_and_roundtrips_through_trusted_git() {
  let work_root = PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT"));
  let fixture = tempfile::Builder::new()
    .prefix("capture-")
    .tempdir_in(&work_root)
    .unwrap();
  std::fs::set_permissions(fixture.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
  octacity_private_fs::validate_private_access(fixture.path()).unwrap();
  let source = fixture.path().join("source");
  let allowed = source.join("selected");
  let protected = fixture.path().join("protected");
  let scratch = fixture.path().join("scratch");
  let output = fixture.path().join("output");
  let data = scratch.join("data");
  let source2 = fixture.path().join("source2");
  for path in [&source, &allowed, &protected, &scratch, &output, &data, &source2] {
    octacity_private_fs::create_private_directory(path).unwrap();
  }
  let git = PathBuf::from(required("OCTACITY_CONTRACT_GIT_EXECUTABLE"))
    .canonicalize()
    .unwrap();
  git_output(&git, &source, &["init", "--quiet", "--initial-branch=main"]).await;
  std::fs::write(allowed.join("value.txt"), b"original\n").unwrap();
  std::fs::write(source.join("outside.txt"), b"outside-must-remain\n").unwrap();
  git_output(&git, &source, &["add", "--all"]).await;
  git_output(&git, &source, &["commit", "--quiet", "-m", "component base"]).await;
  let base = git_output(&git, &source, &["rev-parse", "HEAD"]).await;
  let version = git_output(&git, &source, &["--version"]).await;
  let tool = GitCaptureTool {
    executable: git.clone(),
    identity: FactoryImmutableReferenceV3 {
      identity: "git".to_owned(),
      version: version.split_whitespace().nth(2).unwrap().to_owned(),
      sha256: hex::encode(Sha256::digest(std::fs::read(&git).unwrap())),
    },
  };
  std::fs::write(protected.join("public.txt"), b"immutable-input\n").unwrap();
  std::fs::write(source2.join("private.txt"), b"source2-private-canary\n").unwrap();
  std::fs::write(
    protected.join("Octafile.yml"),
    concat!(
      "version: 1\ntasks:\n  change:\n    dir: /workspace\n    shell: |\n      set -eu\n",
      "      test \"$(pwd)\" = /workspace\n",
      "      test ! -e /workspace/source2\n",
      "      printf 'changed-by-real-guest\\n' > /workspace/source/selected/value.txt\n",
      "      test \"$(cat /octacity/protected/public.txt)\" = immutable-input\n",
      "      ! printf tamper >> /workspace/source/outside.txt\n",
      "      ! printf tamper >> /workspace/source/.git/HEAD\n",
      "      ! printf tamper >> /octacity/protected/public.txt\n",
    ),
  )
  .unwrap();
  let runner = RunnerInstallation::load(&PathBuf::from(required("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT"))).unwrap();
  let engine = MicrosandboxEngine::new(MicrosandboxEngineConfig {
    agent_id: "bounded-capture-contract".to_owned(),
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
    request_id: "bounded-source-capture".to_owned(),
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
      execution_id: "bounded-source-capture".to_owned(),
      workspace_root: work_root,
      workspace: source.clone(),
      data_dir: data,
      workload_identity: None,
      cache: None,
      factory: Some(FactoryExecutionLayout {
        workspace_anchor: true,
        disk_accounting: octacity_execution::ExecutionDiskAccounting::WholeJob,
        protected_inputs: protected.clone(),
        source: source.clone(),
        source_visible: true,
        source_read_only: true,
        source_writable_paths: vec![allowed],
        tool_channel: None,
        scratch,
        output,
        // Qualification of this composed boundary is independent of the
        // separate exact PID-limit-8 probe; no signed Job limit is changed.
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
      octafile: Some("/octacity/protected/Octafile.yml".to_owned()),
      commands: vec!["change".to_owned()],
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
  let cancellation = CancellationToken::new();
  let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
  let drain = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
  for path in [protected.join("public.txt"), protected.join("Octafile.yml")] {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400)).unwrap();
  }
  std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o500)).unwrap();
  // Supervision owns mandatory teardown; never drop a timed-out VM future.
  let completion = supervise(
    &runner,
    &backend,
    request,
    cancellation,
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await;
  // Owned cleanup must work even if supervision returned an error.
  std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o700)).unwrap();
  drop(sender);
  drain.await.unwrap();
  assert_eq!(completion.unwrap().status, RunStatus::Succeeded);
  assert_eq!(
    std::fs::read(source.join("selected/value.txt")).unwrap(),
    b"changed-by-real-guest\n"
  );
  assert_eq!(
    std::fs::read(source.join("outside.txt")).unwrap(),
    b"outside-must-remain\n"
  );
  assert_eq!(
    std::fs::read(protected.join("public.txt")).unwrap(),
    b"immutable-input\n"
  );
  assert_eq!(git_output(&git, &source, &["rev-parse", "HEAD"]).await, base);

  let capture_root = fixture.path().join("capture");
  octacity_private_fs::create_private_directory(&capture_root).unwrap();
  let cancellation = CancellationToken::new();
  let deadline = cancel_after(&cancellation);
  let captured = GitChangeSetCapturer::new(tool.clone())
    .unwrap()
    .capture(
      CaptureRequest {
        workspace: source.clone(),
        destination: capture_root.clone(),
        base_revision: base.clone(),
        bundle_base_revision: base.clone(),
        node_attempt_id: "00000000-0000-0000-0000-000000000019".to_owned(),
        instruction: ChangeSetCaptureV3 {
          author_name: "Bounded component fixture".to_owned(),
          author_email: "component@octacity.invalid".to_owned(),
          committed_at: 1_767_225_600,
          patch_max_bytes: Some(4096),
          policy: ChangeSetCapturePolicyV3 {
            allowed_path_prefixes: vec!["selected".to_owned()],
            forbidden_control_paths: vec![".github/workflows".to_owned()],
            max_changed_paths: 1,
            max_file_bytes: 4096,
            max_total_bytes: 4096,
            allow_empty: false,
            binary_policy: ChangeSetBinaryPolicyV3::Reject,
            forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
          },

          observation: None,
        },

        observation: None,
      },
      cancellation,
    )
    .await;
  deadline.abort();
  let _ = deadline.await;
  let captured = captured.unwrap();
  assert_eq!(captured.manifest.base_revision, base);
  assert_eq!(captured.manifest.changed_paths.len(), 1);
  assert_eq!(captured.manifest.changed_paths[0].path, "selected/value.txt");
  assert_eq!(
    captured.manifest.changed_paths[0].content_sha256.as_deref(),
    Some("dc9895b28378e890a4f53b7775730ad14d88d378b7db9ca61f1c6bc64647bcca")
  );
  assert_ne!(captured.manifest.candidate_revision, base);
  assert_eq!(captured.manifest.capture_tool, tool.identity);

  let restored = fixture.path().join("restored");
  octacity_private_fs::create_private_directory(&restored).unwrap();
  git_output(
    &git,
    fixture.path(),
    &[
      "clone",
      "--quiet",
      "--no-local",
      source.to_str().unwrap(),
      restored.to_str().unwrap(),
    ],
  )
  .await;
  let restore_scratch = fixture.path().join("restore-scratch");
  octacity_private_fs::create_private_directory(&restore_scratch).unwrap();
  let cancellation = CancellationToken::new();
  let deadline = cancel_after(&cancellation);
  let materialized = GitChangeSetMaterializer::new(tool)
    .unwrap()
    .materialize(
      MaterializationRequest {
        workspace: restored.clone(),
        bundle: capture_root.join(&captured.manifest.bundle.name),
        manifest: capture_root.join(CHANGE_SET_MANIFEST_OUTPUT),
        expected_base_revision: base.clone(),
        expected_candidate_revision: captured.manifest.candidate_revision.clone(),
        scratch: restore_scratch,
      },
      cancellation,
    )
    .await;
  deadline.abort();
  let _ = deadline.await;
  assert_eq!(
    materialized.unwrap().candidate_revision,
    captured.manifest.candidate_revision
  );
  assert_eq!(
    std::fs::read(restored.join("selected/value.txt")).unwrap(),
    b"changed-by-real-guest\n"
  );
  assert_eq!(
    std::fs::read(restored.join("outside.txt")).unwrap(),
    b"outside-must-remain\n"
  );
}

fn cancel_after(cancellation: &CancellationToken) -> tokio::task::JoinHandle<()> {
  let cancellation = cancellation.clone();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(30)).await;
    cancellation.cancel();
  })
}

async fn git_output(git: &Path, workspace: &Path, arguments: &[&str]) -> String {
  let mut command = tokio::process::Command::new(git);
  command
    .env_clear()
    .args(arguments)
    .current_dir(workspace)
    .kill_on_drop(true)
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_AUTHOR_NAME", "Component fixture")
    .env("GIT_AUTHOR_EMAIL", "component@octacity.invalid")
    .env("GIT_COMMITTER_NAME", "Component fixture")
    .env("GIT_COMMITTER_EMAIL", "component@octacity.invalid");
  let output = tokio::time::timeout(Duration::from_secs(15), command.output())
    .await
    .unwrap()
    .unwrap();
  assert!(output.status.success(), "owned Git fixture setup failed");
  String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
