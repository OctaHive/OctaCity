//! Real VM fixture using an explicitly selected test-only Linux ARM64 client.

use std::{
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend as _, ExecutionOs, ExecutionPlatform, ExecutionTarget,
  FactoryExecutionLayout, FactoryToolChannel, NetworkAccess, OciIsolation, RunnerProgram, StartExecution,
};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_oci::OciBackend;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

fn required(name: &str) -> String {
  std::env::var(name).unwrap_or_else(|_| panic!("{name} is required for this explicit contract"))
}

pub struct GuestProbe {
  _directory: tempfile::TempDir,
  runner: RunnerProgram,
  request: StartExecution,
  engine: MicrosandboxEngine,
}

impl GuestProbe {
  pub fn new(program_environment: &str) -> Self {
    let work_root = PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT"));
    let directory = tempfile::Builder::new()
      .prefix("probe-")
      .tempdir_in(&work_root)
      .unwrap();
    let job = directory.path();
    let source = job.join("source");
    let scratch = job.join("scratch");
    let output = job.join("output");
    let protected = job.join("protected");
    let data = scratch.join("data");
    for path in [&source, &scratch, &output, &protected, &data] {
      octacity_private_fs::create_private_directory(path).unwrap();
    }
    std::fs::write(source.join("input"), b"backend-only-source").unwrap();
    std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o500)).unwrap();
    let release = job.join("probe-release");
    octacity_private_fs::create_private_directory(&release).unwrap();
    let executable = release.join("probe");
    std::fs::copy(required(program_environment), &executable).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let runner = RunnerProgram::standalone(release, executable).unwrap();
    let engine = MicrosandboxEngine::new(MicrosandboxEngineConfig {
      agent_id: "guest-probe-contract".to_owned(),
      state_root: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT")),
      work_root: work_root.clone(),
      runner_platform: "linux-aarch64".to_owned(),
      executable: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE")),
      libkrunfw: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW")),
      cleanup_timeout: Duration::from_secs(10),
      metrics_sample_interval: Duration::from_secs(1),
    })
    .unwrap();
    let request = StartExecution {
      execution_id: format!("guest-probe-{}", job.file_name().unwrap().to_str().unwrap()),
      workspace_root: work_root,
      workspace: source.clone(),
      data_dir: data,
      workload_identity: None,
      cache: None,
      factory: Some(FactoryExecutionLayout {
        workspace_anchor: false,
        disk_accounting: octacity_execution::ExecutionDiskAccounting::WholeJob,
        protected_inputs: protected,
        source,
        source_visible: false,
        source_read_only: true,
        source_writable_paths: vec![],
        tool_channel: None,
        scratch,
        output,
        process_limit: 8,
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
    };
    Self {
      _directory: directory,
      runner,
      request,
      engine,
    }
  }

  pub fn work_root(&self) -> &Path {
    &self.request.workspace_root
  }

  pub fn source_file(&self) -> PathBuf {
    self.request.workspace.join("input")
  }

  pub async fn run(self, channel: Option<FactoryToolChannel>, process_limit: u32) -> (Option<i32>, Vec<u8>) {
    let (exit, bytes, _) = self.run_configured(channel, process_limit, None, false).await;
    (exit, bytes)
  }

  pub async fn run_read_worker(self, input: &[u8]) -> (Option<i32>, Vec<u8>) {
    let (exit, bytes, _) = self.run_configured(None, 8, Some(input), false).await;
    (exit, bytes)
  }

  pub async fn run_with_owned_disk(self, input: &[u8]) -> (Option<i32>, Vec<u8>, octacity_execution::ResourceUsage) {
    let mut fixture = self;
    fixture.request.writable_disk_bytes = 16 * 1024 * 1024;
    let layout = fixture.request.factory.as_mut().unwrap();
    layout.disk_accounting = octacity_execution::ExecutionDiskAccounting::OwnedWritableMounts;
    let job = layout.source.parent().unwrap();
    let sibling = job.join("other-owner-scratch");
    octacity_private_fs::create_private_directory(&sibling).unwrap();
    for (path, size) in [
      (layout.source.join("readonly-large"), 32 * 1024 * 1024),
      (sibling.join("other-owner-large"), 32 * 1024 * 1024),
      (layout.scratch.join("existing-owned"), 2 * 1024 * 1024),
      (layout.output.join("existing-owned"), 2 * 1024 * 1024),
    ] {
      std::fs::File::create(path).unwrap().set_len(size).unwrap();
    }
    let (exit, bytes, usage) = fixture.run_configured(None, 8, Some(input), true).await;
    (exit, bytes, usage.unwrap())
  }

  async fn run_configured(
    mut self,
    channel: Option<FactoryToolChannel>,
    process_limit: u32,
    input: Option<&[u8]>,
    sample_owned_disk: bool,
  ) -> (Option<i32>, Vec<u8>, Option<octacity_execution::ResourceUsage>) {
    let layout = self.request.factory.as_mut().unwrap();
    layout.tool_channel = channel;
    layout.process_limit = process_limit;
    layout.source_visible = input.is_some();
    let backend = OciBackend::new(vec![Arc::new(self.engine)]).unwrap();
    let mut execution = backend
      .start(&self.runner, self.request, CancellationToken::new())
      .await
      .unwrap();
    assert!(execution.paths().plugin_projection.is_none());
    let mut io = execution.take_io().unwrap();
    if let Some(input) = input {
      io.stdin.write_all(&(input.len() as u32).to_be_bytes()).await.unwrap();
      io.stdin.write_all(input).await.unwrap();
      io.stdin.flush().await.unwrap();
    }
    drop(io.stdin);
    let mut stdout = io.stdout;
    let mut stderr = io.stderr;
    let out = tokio::spawn(async move {
      let mut bytes = vec![];
      stdout.read_to_end(&mut bytes).await.unwrap();
      bytes
    });
    let err = tokio::spawn(async move {
      let mut bytes = vec![];
      stderr.read_to_end(&mut bytes).await.unwrap();
      bytes
    });
    let exit = tokio::time::timeout(Duration::from_secs(60), execution.wait()).await;
    if exit.is_err() {
      execution.kill().await.unwrap();
    }
    let usage = if sample_owned_disk {
      Some(execution.sample_usage().await)
    } else {
      None
    };
    execution.destroy().await.unwrap();
    let bytes = out.await.unwrap();
    let diagnostics = err.await.unwrap();
    assert!(diagnostics.is_empty());
    (exit.unwrap().unwrap().code, bytes, usage.transpose().unwrap())
  }
}
