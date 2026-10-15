//! Real read worker VM through the consumed action contract; no leased-Job qualification.
#![cfg(unix)]

use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionDiskAccounting, ExecutionError, ExecutionExit, ExecutionIo,
  ExecutionOs, ExecutionPaths, ExecutionPlatform, ExecutionProgram, ExecutionTarget, ExecutionWriter,
  FactoryExecutionLayout, FactoryExecutionPartition, NetworkAccess, OciIsolation, ResourceUsage, RunningExecution,
  StartExecution,
};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_oci::OciBackend;
use octacity_job::{AuthorizedFactoryToolAction, enforce_factory_tool_action};
use octacity_lifecycle::{
  ConfinedProtectedReadBackend, ProtectedReadBackend, ProtectedReadInventory, ProtectedReadRecipe,
};
use octacity_protocol::{
  FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryImmutableReferenceV3, FactoryMountModeV3,
  FactoryMountPermissionV3, FactoryOutputPermissionsV3, FactoryPermissionSetV3, FactoryResourceLimitsV3,
  FactoryToolActionDecisionSourceV3, FactoryToolActionDecisionV3, FactoryToolActionDispositionV3,
  FactoryToolActionProposalV3,
};
use std::{
  os::unix::fs::PermissionsExt as _,
  path::PathBuf,
  sync::{Arc, Mutex},
  time::Duration,
};
use std::{
  pin::Pin,
  sync::atomic::{AtomicBool, Ordering},
  task::{Context, Poll},
};
use tokio::{io::AsyncWrite, sync::Notify};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct WorkerLatch {
  blocked: Notify,
  terminated: Notify,
  killed: AtomicBool,
  destroyed: AtomicBool,
  samples: Mutex<Vec<ResourceUsage>>,
}

struct LatchedBackend {
  block_input: bool,
  inner: Arc<dyn ExecutionBackend>,
  latch: Arc<WorkerLatch>,
}

#[async_trait::async_trait]
impl ExecutionBackend for LatchedBackend {
  async fn start(
    &self,
    program: &ExecutionProgram,
    request: StartExecution,
    cancel: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    Ok(Box::new(LatchedExecution {
      block_input: self.block_input,
      inner: self.inner.start(program, request, cancel).await?,
      latch: self.latch.clone(),
    }))
  }
  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    self.inner.cleanup_orphans().await
  }
}

struct LatchedExecution {
  block_input: bool,
  inner: Box<dyn RunningExecution>,
  latch: Arc<WorkerLatch>,
}

#[async_trait::async_trait]
impl RunningExecution for LatchedExecution {
  fn paths(&self) -> &ExecutionPaths {
    self.inner.paths()
  }
  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    let mut io = self.inner.take_io()?;
    if self.block_input {
      io.stdin = Box::pin(BlockedInput {
        _inner: io.stdin,
        latch: self.latch.clone(),
      });
    }
    Ok(io)
  }
  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    let sample = self.inner.sample_usage().await?;
    self.latch.samples.lock().unwrap().push(sample.clone());
    Ok(sample)
  }
  async fn close_input(&mut self) -> Result<(), ExecutionError> {
    self.inner.close_input().await
  }
  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    self.inner.wait().await
  }
  async fn kill(&mut self) -> Result<(), ExecutionError> {
    self.inner.kill().await?;
    self.latch.killed.store(true, Ordering::SeqCst);
    Ok(())
  }
  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError> {
    self.inner.destroy().await?;
    self.latch.destroyed.store(true, Ordering::SeqCst);
    self.latch.terminated.notify_one();
    Ok(())
  }
}

struct BlockedInput {
  // Retain the real guest stdin: dropping it would let the worker finish on EOF.
  _inner: ExecutionWriter,
  latch: Arc<WorkerLatch>,
}
impl AsyncWrite for BlockedInput {
  fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<std::io::Result<usize>> {
    self.latch.blocked.notify_one();
    Poll::Pending
  }
  fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
    Poll::Pending
  }
  fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
    Poll::Pending
  }
}

fn required(name: &str) -> String {
  std::env::var(name).unwrap_or_else(|_| panic!("{name} required"))
}
fn private(path: &std::path::Path) {
  std::fs::create_dir(path).unwrap();
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

struct Inventory {
  recipe: ProtectedReadRecipe,
  program: ExecutionProgram,
}
impl ProtectedReadInventory for Inventory {
  fn installed_rooted_reader(&self) -> Option<ProtectedReadRecipe> {
    Some(self.recipe.clone())
  }
  fn installed_rooted_reader_program(&self) -> Option<ExecutionProgram> {
    Some(self.program.clone())
  }
}

struct ReaderFixture {
  reader: Arc<ConfinedProtectedReadBackend>,
  main: StartExecution,
  program: ExecutionProgram,
  backend: Arc<dyn ExecutionBackend>,
  proposal: FactoryToolActionProposalV3,
  permissions: FactoryPermissionSetV3,
  _directory: tempfile::TempDir,
}

impl ReaderFixture {
  fn authorized_read(&self) -> AuthorizedFactoryToolAction {
    let decision = FactoryToolActionDecisionV3::new(
      self.proposal.clone().canonicalize().unwrap().proposal_sha256(),
      FactoryToolActionDispositionV3::Allow,
      FactoryToolActionDecisionSourceV3::HardPolicy,
      None,
    )
    .unwrap();
    enforce_factory_tool_action(self.proposal.clone(), &decision, &self.permissions, &self.permissions).unwrap()
  }
}

fn reader_fixture(wrap: impl FnOnce(Arc<dyn ExecutionBackend>) -> Arc<dyn ExecutionBackend>) -> ReaderFixture {
  let work = PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT"));
  let directory = tempfile::Builder::new().prefix("reader-").tempdir_in(&work).unwrap();
  let job = directory.path();
  let source = job.join("source");
  let scratch = job.join("scratch");
  let output = job.join("output");
  let protected = job.join("protected");
  let reader_scratch = job.join("reader-scratch");
  let reader_output = job.join("reader-output");
  let reader_protected = job.join("reader-protected");
  for path in [
    &source,
    &scratch,
    &output,
    &protected,
    &reader_scratch,
    &reader_output,
    &reader_protected,
  ] {
    private(path);
  }
  let data = scratch.join("data");
  private(&data);
  let reader_data = reader_scratch.join("data");
  private(&reader_data);
  for path in [&protected, &reader_protected] {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o500)).unwrap();
  }
  std::fs::write(source.join("input"), b"admitted source").unwrap();
  let resources = FactoryResourceLimitsV3 {
    cpu_millis: 1000,
    memory_bytes: 512 * 1024 * 1024,
    disk_bytes: 16 * 1024 * 1024,
    process_count: 8,
    elapsed_millis: 30_000,
  };
  let outputs = FactoryOutputPermissionsV3 {
    kinds: vec![],
    max_artifact_count: 0,
    max_artifact_bytes: 0,
    max_report_count: 0,
    max_report_bytes: 0,
  };
  let executable = PathBuf::from(required("OCTACITY_CONTRACT_PROTECTED_READER"));
  let program = octacity_runner::verify_standalone_program(
    executable.parent().unwrap(),
    std::path::Path::new(executable.file_name().unwrap()),
    &required("OCTACITY_CONTRACT_PROTECTED_READER_SHA256"),
    "linux-aarch64",
  )
  .unwrap();
  let executable_ref = FactoryImmutableReferenceV3 {
    identity: "installed-reader-worker".to_owned(),
    version: "1.0.0".to_owned(),
    sha256: required("OCTACITY_CONTRACT_PROTECTED_READER_SHA256"),
  };
  let tool = FactoryImmutableReferenceV3 {
    identity: "installed-read-operation".to_owned(),
    version: "1.0.0".to_owned(),
    sha256: "2".repeat(64),
  };
  let inventory = Inventory {
    recipe: ProtectedReadRecipe::new(tool.clone(), executable_ref.clone(), resources, outputs.clone()).unwrap(),
    program,
  };
  let main = StartExecution {
    execution_id: format!("main-{}", job.file_name().unwrap().to_str().unwrap()),
    workspace_root: work.clone(),
    workspace: source.clone(),
    data_dir: data,
    workload_identity: None,
    cache: None,
    factory: Some(FactoryExecutionLayout {
      workspace_anchor: false,
      disk_accounting: ExecutionDiskAccounting::WholeJob,
      protected_inputs: protected,
      source: source.clone(),
      source_visible: false,
      source_read_only: true,
      source_writable_paths: vec![],
      tool_channel: None,
      scratch,
      output,
      process_limit: 128,
    }),
    cpu_millis: 3000,
    memory_bytes: 1536 * 1024 * 1024,
    writable_disk_bytes: 128 * 1024 * 1024,
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
  let mut helper = main.clone();
  helper.execution_id.push_str("-reader");
  helper.data_dir = reader_data;
  helper.cpu_millis = resources.cpu_millis;
  helper.memory_bytes = resources.memory_bytes;
  helper.writable_disk_bytes = resources.disk_bytes;
  helper.max_duration = Duration::from_millis(resources.elapsed_millis);
  let layout = helper.factory.as_mut().unwrap();
  layout.source_visible = true;
  layout.scratch = reader_scratch;
  layout.output = reader_output;
  layout.protected_inputs = reader_protected;
  layout.process_limit = resources.process_count;
  let partition = FactoryExecutionPartition::new(main, helper).unwrap();
  let engine = MicrosandboxEngine::new(MicrosandboxEngineConfig {
    agent_id: "confined-reader-contract".to_owned(),
    state_root: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT")),
    work_root: work,
    runner_platform: "linux-aarch64".to_owned(),
    executable: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE")),
    libkrunfw: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW")),
    cleanup_timeout: Duration::from_secs(10),
    metrics_sample_interval: Duration::from_secs(1),
  })
  .unwrap();
  let main_backend: Arc<dyn ExecutionBackend> = Arc::new(OciBackend::new(vec![Arc::new(engine)]).unwrap());
  let backend = wrap(main_backend.clone());
  let (narrowed, reader) = ConfinedProtectedReadBackend::prepare(
    &inventory,
    backend,
    partition,
    tokio::time::Instant::now() + Duration::from_secs(60),
  )
  .unwrap();
  assert_eq!(narrowed.cpu_millis, 2000);
  assert_eq!(narrowed.memory_bytes, 1024 * 1024 * 1024);
  let proposal = inventory
    .recipe
    .action("/workspace/source/input")
    .unwrap()
    .into_proposal();
  let permissions = FactoryPermissionSetV3 {
    plugins: vec![],
    executables: vec![executable_ref.clone()],
    tools: vec![tool],
    commands: vec![FactoryCommandPermissionV3 {
      executable: executable_ref,
      arguments: vec![FactoryCommandArgumentV3::Any { max_bytes: 256 }],
    }],
    max_descendants: 1,
    mounts: vec![FactoryMountPermissionV3 {
      root: "/workspace/source".to_owned(),
      mode: FactoryMountModeV3::ReadOnly,
    }],
    network_hosts: vec![],
    secret_profiles: vec![],
    workload_identity_profiles: vec![],
    resources,
    outputs,
  };
  ReaderFixture {
    reader: Arc::new(reader),
    main: narrowed,
    program: inventory.program,
    backend: main_backend,
    proposal,
    permissions,
    _directory: directory,
  }
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and actual verified LinuxARM read worker"]
async fn a_confined_worker_reads_only_the_exact_consumed_action_under_a_reserved_job_share() {
  let fixture = reader_fixture(std::convert::identity);
  assert_eq!(
    fixture
      .reader
      .read(fixture.authorized_read(), 64, CancellationToken::new())
      .await
      .unwrap(),
    b"admitted source"
  );
  fixture.reader.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and actual verified LinuxARM read worker"]
async fn cancellation_of_a_blocked_real_worker_awaits_teardown_and_clean_shutdown() {
  let latch = Arc::new(WorkerLatch::default());
  let fixture = reader_fixture(|inner| {
    Arc::new(LatchedBackend {
      block_input: true,
      inner,
      latch: latch.clone(),
    })
  });
  let cancellation = CancellationToken::new();
  let reading = fixture.reader.read(fixture.authorized_read(), 64, cancellation.clone());
  tokio::pin!(reading);
  tokio::select! {
    result = &mut reading => panic!("real worker ended before reaching the stdin latch: {result:?}"),
    () = latch.blocked.notified() => {},
  }
  cancellation.cancel();
  let result = reading.await;
  let shutdown = fixture.reader.shutdown().await;
  assert_eq!(result, Err(octacity_lifecycle::AgentToolActionBrokerError::Cancelled));
  // Both flags are set only after the actual MSB teardown operation succeeds.
  assert!(latch.killed.load(Ordering::SeqCst));
  assert!(latch.destroyed.load(Ordering::SeqCst));
  shutdown.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and actual verified LinuxARM read worker"]
async fn dropping_a_pending_read_stops_its_real_worker_before_owner_shutdown() {
  let latch = Arc::new(WorkerLatch::default());
  let fixture = reader_fixture(|inner| {
    Arc::new(LatchedBackend {
      block_input: true,
      inner,
      latch: latch.clone(),
    })
  });
  let mut reading = fixture
    .reader
    .read(fixture.authorized_read(), 64, CancellationToken::new());
  tokio::select! {
    result = &mut reading => panic!("real worker ended before reaching the stdin latch: {result:?}"),
    () = latch.blocked.notified() => {},
  }
  drop(reading);
  // This bound is above the real backend's 10-second cleanup allowance and
  // below the worker's 30-second window: expiry cannot masquerade as drop cleanup.
  let detached_cleanup = tokio::time::timeout(Duration::from_secs(15), latch.terminated.notified()).await;
  let shutdown = fixture.reader.shutdown().await;
  assert!(
    detached_cleanup.is_ok(),
    "dropping a read must cancel its tracked worker"
  );
  assert!(latch.killed.load(Ordering::SeqCst));
  assert!(latch.destroyed.load(Ordering::SeqCst));
  shutdown.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and actual verified LinuxARM read worker"]
async fn completed_reader_usage_retains_both_serial_workers_instead_of_the_last_sample() {
  let observed = Arc::new(WorkerLatch::default());
  let fixture = reader_fixture(|inner| {
    Arc::new(LatchedBackend {
      block_input: false,
      inner,
      latch: observed.clone(),
    })
  });
  for _ in 0..2 {
    assert_eq!(
      fixture
        .reader
        .read(fixture.authorized_read(), 64, CancellationToken::new())
        .await
        .unwrap(),
      b"admitted source"
    );
  }
  fixture.reader.shutdown().await.unwrap();
  let usage = fixture.reader.completed_usage().unwrap();
  let samples = observed.samples.lock().unwrap();
  assert_eq!(samples.len(), 2);
  // Independent backend observations: both physical executions must be billed.
  let first = &samples[0];
  let second = &samples[1];
  assert!(first.elapsed_ms > 0 && second.elapsed_ms > 0);
  assert_eq!(usage.elapsed_ms, first.elapsed_ms + second.elapsed_ms);
  assert_eq!(usage.cpu_time_ms, first.cpu_time_ms + second.cpu_time_ms);
  assert_eq!(usage.io_read_bytes, first.io_read_bytes + second.io_read_bytes);
  assert_eq!(usage.io_written_bytes, first.io_written_bytes + second.io_written_bytes);
  assert_eq!(usage.memory_current_bytes, 0, "both real workers have been destroyed");
  assert_eq!(
    usage.memory_peak_bytes,
    first.memory_peak_bytes.max(second.memory_peak_bytes)
  );
  assert_eq!(usage.disk_current_bytes, second.disk_current_bytes);
  assert_eq!(usage.disk_peak_bytes, first.disk_peak_bytes.max(second.disk_peak_bytes));
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and actual verified LinuxARM read worker"]
async fn killing_the_main_execution_joins_its_blocked_physical_reader() {
  let latch = Arc::new(WorkerLatch::default());
  let fixture = reader_fixture(|inner| {
    Arc::new(LatchedBackend {
      block_input: true,
      inner,
      latch: latch.clone(),
    })
  });
  let main = fixture
    .backend
    .start(&fixture.program, fixture.main.clone(), CancellationToken::new())
    .await
    .unwrap();
  let mut owner = fixture.reader.clone().attach(main);
  // The real main VM waits for input in its source-hidden scratch workspace.
  // Retain its streams so EOF cannot masquerade as owner-driven termination.
  let _main_io = owner.take_io().unwrap();
  let reading = fixture
    .reader
    .read(fixture.authorized_read(), 64, CancellationToken::new());
  tokio::pin!(reading);
  tokio::select! {
    result = &mut reading => panic!("physical reader ended before its input latch: {result:?}"),
    () = latch.blocked.notified() => {},
  }
  let killed = owner.kill().await;
  let result = reading.await;
  let destroyed = owner.destroy().await;
  killed.unwrap();
  destroyed.unwrap();
  assert_eq!(result, Err(octacity_lifecycle::AgentToolActionBrokerError::Cancelled));
  assert!(latch.killed.load(Ordering::SeqCst));
  assert!(
    latch.destroyed.load(Ordering::SeqCst),
    "owner termination must await actual helper destruction"
  );
  fixture.reader.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and actual verified LinuxARM read worker"]
async fn final_owner_accounting_includes_the_completed_reader_and_seals_new_reads() {
  let helper_samples = Arc::new(WorkerLatch::default());
  let main_samples = Arc::new(WorkerLatch::default());
  let fixture = reader_fixture(|inner| {
    Arc::new(LatchedBackend {
      block_input: false,
      inner,
      latch: helper_samples.clone(),
    })
  });
  let main_backend = LatchedBackend {
    block_input: false,
    inner: fixture.backend.clone(),
    latch: main_samples.clone(),
  };
  let main = main_backend
    .start(&fixture.program, fixture.main.clone(), CancellationToken::new())
    .await
    .unwrap();
  let mut owner = fixture.reader.clone().attach(main);
  let _main_io = owner.take_io().unwrap();
  assert_eq!(
    fixture
      .reader
      .read(fixture.authorized_read(), 64, CancellationToken::new())
      .await
      .unwrap(),
    b"admitted source"
  );
  let final_usage = owner.sample_final_usage().await;
  let after_seal = fixture
    .reader
    .read(fixture.authorized_read(), 64, CancellationToken::new())
    .await;
  // Always tear down both real VMs even if the accounting assertions fail.
  owner.kill().await.unwrap();
  owner.destroy().await.unwrap();
  let usage = final_usage.unwrap();
  assert!(after_seal.is_err(), "final accounting must stop further physical reads");
  let main_samples = main_samples.samples.lock().unwrap();
  let helper_samples = helper_samples.samples.lock().unwrap();
  assert_eq!(main_samples.len(), 1);
  assert_eq!(helper_samples.len(), 1);
  let main = &main_samples[0];
  let helper = &helper_samples[0];
  // The main Job wall clock is not the sum of concurrent VM durations.
  assert_eq!(usage.elapsed_ms, main.elapsed_ms);
  assert_eq!(usage.cpu_time_ms, main.cpu_time_ms + helper.cpu_time_ms);
  assert_eq!(usage.io_read_bytes, main.io_read_bytes + helper.io_read_bytes);
  assert_eq!(usage.io_written_bytes, main.io_written_bytes + helper.io_written_bytes);
  assert_eq!(usage.memory_current_bytes, main.memory_current_bytes);
  assert_eq!(
    usage.memory_peak_bytes,
    main.memory_peak_bytes + helper.memory_peak_bytes
  );
  assert_eq!(
    usage.disk_current_bytes,
    main.disk_current_bytes + helper.disk_current_bytes
  );
  assert_eq!(usage.disk_peak_bytes, main.disk_peak_bytes + helper.disk_peak_bytes);
}
