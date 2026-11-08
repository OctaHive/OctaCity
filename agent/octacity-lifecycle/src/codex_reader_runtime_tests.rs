#[cfg(unix)]
mod codex_reader_runtime_tests {
  use super::*;
  use crate::{FactoryProtectedReadInstallation, FactoryProtectedReadRuntime, ProtectedReadRecipe};
  use octacity_execution::{
    ExecutionArchitecture, ExecutionBackend, ExecutionDiskAccounting, ExecutionError, ExecutionOs, ExecutionPlatform,
    ExecutionProgram, ExecutionTarget, FactoryExecutionLayout, NetworkAccess, OciIsolation, RunningExecution,
    StartExecution,
  };
  use octacity_job::FactoryToolExecutionScope;
  use std::{fs, os::unix::fs::PermissionsExt as _, path::PathBuf};

  #[derive(Default)]
  struct BackendBoundary(Mutex<Vec<StartExecution>>);

  #[async_trait]
  impl ExecutionBackend for BackendBoundary {
    fn factory_tool_projection(&self, _: &ExecutionTarget) -> Option<PathBuf> {
      Some("/octacity/tools".into())
    }
    async fn start(
      &self,
      _: &ExecutionProgram,
      request: StartExecution,
      _: CancellationToken,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      request.validate()?;
      self.0.lock().unwrap().push(request);
      Err(ExecutionError::Unavailable("test backend startup boundary".into()))
    }
    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      Ok(())
    }
  }

  #[tokio::test]
  async fn selected_source_controlled_reader_hides_code_from_the_main_workload() {
    let directory = tempfile::Builder::new().prefix("cr-").tempdir_in("/tmp").unwrap();
    let root = directory.path().canonicalize().unwrap();
    let release = root.join("release");
    octacity_private_fs::create_private_directory(&release).unwrap();
    for name in ["worker", "bridge"] {
      // Immutable guest executable metadata; no test binary is executed.
      let mut bytes = vec![0u8; 64];
      bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
      bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
      bytes.extend_from_slice(name.as_bytes());
      fs::write(release.join(name), bytes).unwrap();
      fs::set_permissions(release.join(name), fs::Permissions::from_mode(0o500)).unwrap();
    }
    let worker_digest = "3641fb4e36b1ae74979c0aab7af6ad258d31b88ef4251955e031052c09884fde";
    let bridge_digest = "da0f0809071e46681aef869a5a5e0eadc6727c1d47eb974b5c793b8808a20e37";
    let worker = octacity_runner::verify_standalone_program(
      &release,
      std::path::Path::new("worker"),
      worker_digest,
      "linux-aarch64",
    )
    .unwrap();
    let bridge = octacity_runner::verify_standalone_program(
      &release,
      std::path::Path::new("bridge"),
      bridge_digest,
      "linux-aarch64",
    )
    .unwrap();
    let mut executable = reference("protected-reader");
    executable.sha256 = worker_digest.into();
    let installation = FactoryProtectedReadInstallation {
      worker: worker.clone(),
      bridge,
      bridge_sha256: bridge_digest.into(),
      platform: "linux-aarch64".into(),
      hook_tool_name: "mcp__installed_source__read_file".into(),
      recipe: ProtectedReadRecipe::new(reference("installed-reader"), executable, limits(), outputs()).unwrap(),
    };
    let state = root.join("state");
    octacity_private_fs::create_private_directory(&state).unwrap();
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let registration_fixture = broker(coordinator.clone(), Duration::from_secs(10), CancellationToken::new());
    let owner = FactoryProtectedReadRuntime::new(installation, permissions(), state)
      .unwrap()
      .bind(
        coordinator.clone(),
        registration_fixture.registration.clone(),
        registration_fixture.lease.clone(),
      )
      .unwrap();
    let job = root.join("job");
    octacity_private_fs::create_private_directory(&job).unwrap();
    for name in ["source", "source-view", "scratch", "output", "protected"] {
      octacity_private_fs::create_private_directory(&job.join(name)).unwrap();
    }
    fs::write(job.join("source/private"), "not selected").unwrap();
    fs::write(job.join("source-view/input"), "selected source").unwrap();
    fs::set_permissions(job.join("protected"), fs::Permissions::from_mode(0o500)).unwrap();
    let data = job.join("scratch/data");
    octacity_private_fs::create_private_directory(&data).unwrap();
    let target = ExecutionTarget::Oci {
      reference: format!("registry.test/guest@sha256:{DIGEST}"),
      platform: ExecutionPlatform {
        os: ExecutionOs::Linux,
        architecture: ExecutionArchitecture::Arm64,
      },
      isolation: OciIsolation::Hypervisor,
    };
    let physical = Arc::new(BackendBoundary::default());
    let cancellation = CancellationToken::new();
    let backend = owner
      .prepare(FactoryToolExecutionScope {
        job_id: "job".into(),
        attempt: 1,
        backend: physical.clone(),
        target: target.clone(),
        control: octacity_protocol::FactoryToolControlV3 {
          plugin: "codex".into(),
          capability: "codex.blocking-pre-tool-authorization.v1".into(),
          mode: octacity_protocol::FactoryToolControlModeV3::ScopedValidation,
          response_limits: reader_response_limits(),
        },
        permissions: permissions(),
        deadline: tokio::time::Instant::now() + Duration::from_secs(10),
        cancellation: cancellation.clone(),
      })
      .unwrap();
    let request = StartExecution {
      execution_id: "selected-controlled".into(),
      workspace_root: root.clone(),
      workspace: job.join("source"),
      data_dir: data,
      workload_identity: None,
      cache: None,
      factory: Some(FactoryExecutionLayout {
        workspace_anchor: false,
        disk_accounting: ExecutionDiskAccounting::WholeJob,
        protected_inputs: job.join("protected"),
        source: job.join("source"),
        source_view: Some(job.join("source-view")),
        source_visible: true,
        source_read_only: true,
        source_writable_paths: vec![],
        tool_channel: None,
        scratch: job.join("scratch"),
        output: job.join("output"),
        process_limit: 8,
      }),
      cpu_millis: 2000,
      memory_bytes: 16 * 1024 * 1024,
      writable_disk_bytes: 16 * 1024 * 1024,
      // Job preparation has consumed part of the installed 30-second ceiling.
      max_duration: Duration::from_secs(29),
      root: target,
      network: NetworkAccess::Disabled,
    };
    request.validate().unwrap();
    assert!(backend.start(&worker, request, cancellation).await.is_err());
    let starts = physical.0.lock().unwrap();
    assert_eq!(
      starts.len(),
      1,
      "valid selected-source execution must reach the qualified physical backend"
    );
    assert!(starts[0].max_duration <= Duration::from_secs(29));
    let main = starts[0].factory.as_ref().unwrap();
    assert!(!main.source_visible);
    assert!(main.source_view.is_none());
    let bridge_config: serde_json::Value = serde_json::from_slice(
      &fs::read(main.scratch.join(".octacity/controlled-reader.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(bridge_config["scoped_validation"], true);
    assert_eq!(fs::read_link(main.scratch.join(".octacity/scoped-bridge")).unwrap().parent().unwrap(), std::path::Path::new("/octacity/tools"));
    assert!(coordinator.requests.lock().unwrap().is_empty());
  }
}
