//! Runner lifecycle, deadlines, cancellation, and cleanup tests.

use std::{
  collections::BTreeMap,
  future::pending,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

use super::*;
use crate::{
  protocol::RunnerMessage,
  supervisor::lifecycle::{DeliveryOutcome, begin_stop, deliver, handle_delivery, wait_for_deadline},
  supervisor::validation::{validate_event, validate_exit, validate_hello},
};
use octacity_execution::{
  ExecutionArchitecture, ExecutionExit, ExecutionIo, ExecutionOs, ExecutionPaths, ExecutionPlatform, ExecutionReader,
  ExecutionTarget, ExecutionWriter, NetworkAccess, RunnerProgram, RunningExecution,
};

#[derive(Clone, Copy)]
enum Scenario {
  Success,
  Cancelled,
  BadHello,
  Silent,
  HelloThenSilent,
  AcceptedThenSilent,
  EofAfterAccepted,
  RunnerError,
  AccountingFails,
  AccountingHangs,
}

struct FakeBackend {
  destroyed: Arc<AtomicBool>,
  killed: Arc<AtomicBool>,
  scenario: Scenario,
  cleanup_fails: bool,
}

struct FakeExecution {
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  destroyed: Arc<AtomicBool>,
  killed: Arc<AtomicBool>,
  exit_code: i32,
  cleanup_fails: bool,
  scenario: Scenario,
}

#[async_trait]
impl ExecutionBackend for FakeBackend {
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    _cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    let (agent_stdin, runner_input) = tokio::io::duplex(4096);
    let (runner_output, agent_stdout) = tokio::io::duplex(4096);
    let (agent_stderr, runner_stderr) = tokio::io::duplex(64);
    drop(agent_stderr);
    tokio::spawn(fake_runner(runner_input, runner_output, self.scenario));
    Ok(Box::new(FakeExecution {
      io: Some(ExecutionIo {
        stdin: Box::pin(agent_stdin) as ExecutionWriter,
        stdout: Box::pin(agent_stdout) as ExecutionReader,
        stderr: Box::pin(runner_stderr) as ExecutionReader,
      }),
      paths: ExecutionPaths {
        workspace: request.workspace,
        data_dir: request.data_dir,
        plugins_dir: runner.plugins_dir.clone(),
        plugin_lock: runner.plugin_lock.clone(),
      },
      destroyed: self.destroyed.clone(),
      killed: self.killed.clone(),
      exit_code: match self.scenario {
        Scenario::Cancelled => 130,
        Scenario::Success
        | Scenario::BadHello
        | Scenario::Silent
        | Scenario::HelloThenSilent
        | Scenario::AcceptedThenSilent
        | Scenario::EofAfterAccepted
        | Scenario::RunnerError
        | Scenario::AccountingFails
        | Scenario::AccountingHangs => 0,
      },
      cleanup_fails: self.cleanup_fails,
      scenario: self.scenario,
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    Ok(())
  }
}

#[async_trait]
impl RunningExecution for FakeExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    if matches!(self.scenario, Scenario::AccountingHangs) {
      pending::<()>().await;
    }
    if matches!(self.scenario, Scenario::AccountingFails) {
      return Err(ExecutionError::Backend("fixture accounting failure".to_owned()));
    }
    Ok(ResourceUsage {
      elapsed_ms: 10,
      cpu_time_ms: 2,
      memory_current_bytes: 1024,
      memory_peak_bytes: 2048,
      disk_current_bytes: 4096,
      disk_peak_bytes: 4096,
      io_read_bytes: 12,
      io_written_bytes: 34,
      network_received_bytes: None,
      network_transmitted_bytes: None,
    })
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    Ok(ExecutionExit {
      code: Some(self.exit_code),
    })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    self.killed.store(true, Ordering::SeqCst);
    Ok(())
  }

  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError> {
    self.destroyed.store(true, Ordering::SeqCst);
    if self.cleanup_fails {
      Err(ExecutionError::Backend("fixture cleanup failure".to_owned()))
    } else {
      Ok(())
    }
  }
}

async fn fake_runner(input: tokio::io::DuplexStream, mut output: tokio::io::DuplexStream, scenario: Scenario) {
  if matches!(scenario, Scenario::Silent) {
    pending::<()>().await;
    return;
  }
  if matches!(scenario, Scenario::BadHello) {
    output
        .write_all(
          b"{\"type\":\"hello\",\"protocol_version\":2,\"octa_version\":\"0.3.0\",\"event_schema_version\":3,\"plugin_protocol_version\":1}\n",
        )
        .await
        .unwrap();
    return;
  }
  output
      .write_all(
        b"{\"type\":\"hello\",\"protocol_version\":1,\"octa_version\":\"0.3.0\",\"event_schema_version\":3,\"plugin_protocol_version\":1}\n",
      )
      .await
      .unwrap();
  if matches!(scenario, Scenario::HelloThenSilent) {
    pending::<()>().await;
    return;
  }
  let mut input = BufReader::new(input);
  let mut command = String::new();
  input.read_line(&mut command).await.unwrap();
  if matches!(scenario, Scenario::RunnerError) {
    output
      .write_all(b"{\"type\":\"error\",\"request_id\":\"job-1-attempt-1\",\"message\":\"fixture failure\"}\n")
      .await
      .unwrap();
    return;
  }
  if matches!(scenario, Scenario::AcceptedThenSilent | Scenario::EofAfterAccepted) {
    output
      .write_all(b"{\"type\":\"accepted\",\"request_id\":\"job-1-attempt-1\"}\n")
      .await
      .unwrap();
    if matches!(scenario, Scenario::AcceptedThenSilent) {
      pending::<()>().await;
    }
    return;
  }
  if matches!(scenario, Scenario::Cancelled) {
    output
      .write_all(b"{\"type\":\"accepted\",\"request_id\":\"job-1-attempt-1\"}\n")
      .await
      .unwrap();
    command.clear();
    input.read_line(&mut command).await.unwrap();
    assert!(command.contains("cancel"));
    output
      .write_all(
        b"{\"type\":\"finished\",\"request_id\":\"job-1-attempt-1\",\"status\":\"cancelled\",\"results\":[]}\n",
      )
      .await
      .unwrap();
    return;
  }
  output
    .write_all(
      b"{\"type\":\"accepted\",\"request_id\":\"job-1-attempt-1\"}\n{\"type\":\"event\",\"request_id\":\"job-1-attempt-1\",\"event\":{\"schema_version\":3,\"sequence\":0,\"timestamp\":\"2026-09-10T00:00:00Z\",\"category\":\"execution\",\"data\":{\"type\":\"run_started\"}}}\n",
    )
    .await
    .unwrap();
  if matches!(scenario, Scenario::AccountingFails | Scenario::AccountingHangs) {
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  output
    .write_all(b"{\"type\":\"finished\",\"request_id\":\"job-1-attempt-1\",\"status\":\"succeeded\",\"results\":[]}\n")
    .await
    .unwrap();
}

fn job(workspace: &std::path::Path) -> RunnerJobRequest {
  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
  RunnerJobRequest {
    request_id: "job-1-attempt-1".to_owned(),
    octa: octacity_protocol::OctaSpec {
      version: "0.3.0".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 1,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    execution: StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: workspace.to_owned(),
      workspace: workspace.to_owned(),
      data_dir: workspace.join("data"),
      workload_identity: None,
      cpu_millis: 1000,
      memory_bytes: 1024,
      writable_disk_bytes: 1024,
      max_duration: Duration::from_secs(2),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Unrestricted,
    },
    spec: ExecutionSpec {
      octafile: None,
      secrets_profile: None,
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
    },
    redactions: RunnerRedactions::default(),
    cancellation_grace: Duration::from_secs(1),
  }
}

fn installation() -> RunnerInstallation {
  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
  RunnerInstallation {
    root: PathBuf::from("/opt/octa"),
    executable: PathBuf::from("/opt/octa/octa-runner"),
    plugins_dir: PathBuf::from("/opt/octa/plugins"),
    default_plugin_lock: PathBuf::from("/opt/octa/Octa.lock"),
    sha256: DIGEST.to_owned(),
    capabilities: crate::RunnerCapabilities {
      octa_version: "0.3.0".to_owned(),
      runner_protocols: vec![1],
      event_schemas: vec![3],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      platform: "any".to_owned(),
      features: Vec::new(),
      build_commit: None,
    },
    plugins: BTreeMap::new(),
  }
}

#[tokio::test]
async fn supervises_the_published_runner_protocol_and_always_destroys() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: Arc::new(AtomicBool::new(false)),
    scenario: Scenario::Success,
    cleanup_fails: false,
  };
  let (sender, mut receiver) = mpsc::channel(8);
  let completion = supervise(
    &installation(),
    &backend,
    job(&workspace.path().canonicalize().unwrap()),
    CancellationToken::new(),
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap();

  assert_eq!(completion.status, RunStatus::Succeeded);
  assert_eq!(completion.final_usage.cpu_time_ms, 2);
  assert!(completion.termination_reason.is_none());
  assert!(matches!(receiver.recv().await, Some(RunnerStreamItem::Event(_))));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cancellation_is_forwarded_and_reported_by_the_runner() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: Arc::new(AtomicBool::new(false)),
    scenario: Scenario::Cancelled,
    cleanup_fails: false,
  };
  let (sender, _receiver) = mpsc::channel(8);
  let cancellation = CancellationToken::new();
  let cancellation_request = cancellation.clone();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(10)).await;
    cancellation_request.cancel();
  });

  let completion = supervise(
    &installation(),
    &backend,
    job(&workspace.path().canonicalize().unwrap()),
    cancellation,
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap();

  assert_eq!(completion.status, RunStatus::Cancelled);
  assert_eq!(completion.termination_reason, Some(TerminationReason::Cancelled));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cancellation_during_the_runner_handshake_kills_and_destroys() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let killed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: killed.clone(),
    scenario: Scenario::Silent,
    cleanup_fails: false,
  };
  let (sender, _receiver) = mpsc::channel(8);
  let cancellation = CancellationToken::new();
  let cancellation_request = cancellation.clone();
  tokio::spawn(async move {
    tokio::task::yield_now().await;
    cancellation_request.cancel();
  });

  let error = supervise(
    &installation(),
    &backend,
    job(&workspace.path().canonicalize().unwrap()),
    cancellation,
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap_err();

  assert!(matches!(
    error,
    RunnerSupervisionError::Execution(ExecutionError::Cancelled)
  ));
  assert!(killed.load(Ordering::SeqCst));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn blocked_start_delivery_cannot_outlive_the_job_deadline() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let killed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: killed.clone(),
    scenario: Scenario::HelloThenSilent,
    cleanup_fails: false,
  };
  let mut request = job(&workspace.path().canonicalize().unwrap());
  request.execution.max_duration = Duration::from_millis(30);
  request.spec.variables.insert("large".to_owned(), "x".repeat(64 * 1024));
  let (sender, _receiver) = mpsc::channel(8);

  let error = supervise(
    &installation(),
    &backend,
    request,
    CancellationToken::new(),
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap_err();

  assert!(matches!(error, RunnerSupervisionError::StartupTimeout));
  assert!(killed.load(Ordering::SeqCst));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn protocol_failures_kill_and_destroy_the_execution() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let killed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: killed.clone(),
    scenario: Scenario::BadHello,
    cleanup_fails: false,
  };
  let (sender, _receiver) = mpsc::channel(8);

  let error = supervise(
    &installation(),
    &backend,
    job(&workspace.path().canonicalize().unwrap()),
    CancellationToken::new(),
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap_err();

  assert!(error.to_string().contains("signed Octa requirement"));
  assert!(killed.load(Ordering::SeqCst));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn applies_the_job_deadline_to_the_runner_handshake() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let killed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: killed.clone(),
    scenario: Scenario::Silent,
    cleanup_fails: false,
  };
  let mut request = job(&workspace.path().canonicalize().unwrap());
  request.execution.max_duration = Duration::from_millis(10);
  let (sender, _receiver) = mpsc::channel(8);

  let error = supervise(
    &installation(),
    &backend,
    request,
    CancellationToken::new(),
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap_err();

  assert!(error.to_string().contains("hello timed out"));
  assert!(killed.load(Ordering::SeqCst));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn event_backpressure_cannot_outlive_the_job_deadline() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let killed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: killed.clone(),
    scenario: Scenario::Success,
    cleanup_fails: false,
  };
  let mut request = job(&workspace.path().canonicalize().unwrap());
  request.execution.max_duration = Duration::from_millis(30);
  let (sender, _receiver) = mpsc::channel(1);
  sender
    .send(RunnerStreamItem::AccountingUnavailable {
      consecutive_failures: 1,
    })
    .await
    .unwrap();

  let result = tokio::time::timeout(
    Duration::from_secs(1),
    supervise(
      &installation(),
      &backend,
      request,
      CancellationToken::new(),
      &sender,
      &RunnerSupervisionPolicy {
        hello_timeout: Duration::from_secs(1),
        resource_sample_interval: Duration::from_secs(1),
        resource_sample_timeout: Duration::from_millis(20),
        max_accounting_failures: 1,
      },
    ),
  )
  .await;

  assert!(
    result.is_ok(),
    "a full event channel must not disable the signed deadline"
  );
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn preserves_protocol_and_cleanup_failures() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let backend = FakeBackend {
    destroyed: Arc::new(AtomicBool::new(false)),
    killed: Arc::new(AtomicBool::new(false)),
    scenario: Scenario::BadHello,
    cleanup_fails: true,
  };
  let (sender, _receiver) = mpsc::channel(8);

  let error = supervise(
    &installation(),
    &backend,
    job(&workspace.path().canonicalize().unwrap()),
    CancellationToken::new(),
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap_err();

  assert!(matches!(error, RunnerSupervisionError::OperationAndCleanup { .. }));
  assert!(error.to_string().contains("signed Octa requirement"));
  assert!(error.to_string().contains("fixture cleanup failure"));
}

#[tokio::test]
async fn rejects_runner_errors_and_eof_before_terminal_result() {
  for (scenario, expected) in [
    (Scenario::RunnerError, "fixture failure"),
    (Scenario::EofAfterAccepted, "stdout closed"),
  ] {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join("data")).unwrap();
    let destroyed = Arc::new(AtomicBool::new(false));
    let killed = Arc::new(AtomicBool::new(false));
    let backend = FakeBackend {
      destroyed: destroyed.clone(),
      killed: killed.clone(),
      scenario,
      cleanup_fails: false,
    };
    let (sender, _receiver) = mpsc::channel(8);

    let error = supervise(
      &installation(),
      &backend,
      job(&workspace.path().canonicalize().unwrap()),
      CancellationToken::new(),
      &sender,
      &RunnerSupervisionPolicy::default(),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains(expected));
    assert!(killed.load(Ordering::SeqCst));
    assert!(destroyed.load(Ordering::SeqCst));
  }
}

#[tokio::test]
async fn forces_a_runner_that_outlives_its_execution_deadline() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let destroyed = Arc::new(AtomicBool::new(false));
  let killed = Arc::new(AtomicBool::new(false));
  let backend = FakeBackend {
    destroyed: destroyed.clone(),
    killed: killed.clone(),
    scenario: Scenario::AcceptedThenSilent,
    cleanup_fails: false,
  };
  let mut request = job(&workspace.path().canonicalize().unwrap());
  request.execution.max_duration = Duration::from_millis(20);
  request.cancellation_grace = Duration::from_millis(10);
  let (sender, _receiver) = mpsc::channel(8);

  let error = supervise(
    &installation(),
    &backend,
    request,
    CancellationToken::new(),
    &sender,
    &RunnerSupervisionPolicy::default(),
  )
  .await
  .unwrap_err();

  assert!(matches!(error, RunnerSupervisionError::CancellationTimeout));
  assert!(killed.load(Ordering::SeqCst));
  assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn fails_after_bounded_resource_accounting_errors_and_timeouts() {
  for scenario in [Scenario::AccountingFails, Scenario::AccountingHangs] {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join("data")).unwrap();
    let destroyed = Arc::new(AtomicBool::new(false));
    let killed = Arc::new(AtomicBool::new(false));
    let backend = FakeBackend {
      destroyed: destroyed.clone(),
      killed: killed.clone(),
      scenario,
      cleanup_fails: false,
    };
    let (sender, mut receiver) = mpsc::channel(8);
    let policy = RunnerSupervisionPolicy {
      hello_timeout: Duration::from_secs(1),
      resource_sample_interval: Duration::from_millis(1),
      resource_sample_timeout: Duration::from_millis(2),
      max_accounting_failures: 1,
    };

    let error = supervise(
      &installation(),
      &backend,
      job(&workspace.path().canonicalize().unwrap()),
      CancellationToken::new(),
      &sender,
      &policy,
    )
    .await
    .unwrap_err();

    assert!(matches!(error, RunnerSupervisionError::AccountingUnavailable(1)));
    let mut accounting_event = false;
    while let Ok(item) = receiver.try_recv() {
      accounting_event |= matches!(
        item,
        RunnerStreamItem::AccountingUnavailable {
          consecutive_failures: 1
        }
      );
    }
    assert!(accounting_event);
    assert!(killed.load(Ordering::SeqCst));
    assert!(destroyed.load(Ordering::SeqCst));
  }
}

#[test]
fn validates_job_events_and_terminal_exit_codes() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let mut request = job(&workspace.path().canonicalize().unwrap());
  assert!(request.validate().is_ok());
  request.request_id = "another-attempt".to_owned();
  assert!(request.validate().unwrap_err().to_string().contains("request_id"));
  request = job(&workspace.path().canonicalize().unwrap());
  request.spec.commands.clear();
  assert!(request.validate().unwrap_err().to_string().contains("commands"));
  request = job(&workspace.path().canonicalize().unwrap());
  request.cancellation_grace = Duration::ZERO;
  assert!(
    request
      .validate()
      .unwrap_err()
      .to_string()
      .contains("cancellation timeout")
  );

  let expectation = &request.octa;
  let hello = RunnerMessage::Hello {
    protocol_version: 1,
    octa_version: "0.3.0".to_owned(),
    event_schema_version: 3,
    plugin_protocol_version: 1,
  };
  assert!(validate_hello(&hello, expectation).is_ok());
  assert!(
    validate_hello(
      &RunnerMessage::Accepted {
        request_id: "id".to_owned()
      },
      expectation
    )
    .is_err()
  );

  let mut sequence = None;
  let mut event = RunnerEvent {
    schema_version: 3,
    sequence: 1,
    timestamp: "2026-09-10T00:00:00Z".to_owned(),
    category: "diagnostic".to_owned(),
    data: serde_json::Map::from_iter([("type".to_owned(), serde_json::json!("message"))]),
  };
  assert!(validate_event(&event, expectation.event_schema, &mut sequence).is_ok());
  event.sequence = 3;
  assert!(validate_event(&event, expectation.event_schema, &mut sequence).is_err());
  event.sequence = 2;
  event.category = "unknown".to_owned();
  assert!(validate_event(&event, expectation.event_schema, &mut sequence).is_err());
  event.category = "diagnostic".to_owned();
  event.sequence = 0;
  sequence = Some(u64::MAX);
  assert!(
    validate_event(&event, expectation.event_schema, &mut sequence)
      .unwrap_err()
      .to_string()
      .contains("overflowed")
  );

  assert!(validate_exit(RunStatus::Succeeded, ExecutionExit { code: Some(0) }).is_ok());
  assert!(validate_exit(RunStatus::Failed, ExecutionExit { code: Some(1) }).is_ok());
  assert!(validate_exit(RunStatus::Cancelled, ExecutionExit { code: Some(130) }).is_ok());
  assert!(validate_exit(RunStatus::Succeeded, ExecutionExit { code: None }).is_err());
}

#[tokio::test]
async fn bounds_event_delivery_by_consumer_cancellation_and_deadline() {
  let (sender, mut receiver) = mpsc::channel(1);
  let cancellation = CancellationToken::new();
  assert_eq!(
    deliver(
      &sender,
      RunnerStreamItem::AccountingUnavailable {
        consecutive_failures: 1,
      },
      &cancellation,
      Instant::now() + Duration::from_secs(1),
    )
    .await
    .unwrap(),
    DeliveryOutcome::Delivered
  );
  assert!(receiver.recv().await.is_some());

  drop(receiver);
  assert!(matches!(
    deliver(
      &sender,
      RunnerStreamItem::AccountingUnavailable {
        consecutive_failures: 2,
      },
      &cancellation,
      Instant::now() + Duration::from_secs(1),
    )
    .await,
    Err(RunnerSupervisionError::EventConsumerStopped)
  ));

  let (sender, _receiver) = mpsc::channel(1);
  sender
    .send(RunnerStreamItem::AccountingUnavailable {
      consecutive_failures: 1,
    })
    .await
    .unwrap();
  cancellation.cancel();
  assert_eq!(
    deliver(
      &sender,
      RunnerStreamItem::AccountingUnavailable {
        consecutive_failures: 2,
      },
      &cancellation,
      Instant::now() + Duration::from_secs(1),
    )
    .await
    .unwrap(),
    DeliveryOutcome::Cancelled
  );

  assert_eq!(
    deliver(
      &sender,
      RunnerStreamItem::AccountingUnavailable {
        consecutive_failures: 3,
      },
      &CancellationToken::new(),
      Instant::now(),
    )
    .await
    .unwrap(),
    DeliveryOutcome::TimedOut
  );
}

#[tokio::test]
async fn maps_delivery_stop_reasons_and_keeps_the_first_reason() {
  let workspace = tempfile::tempdir().unwrap();
  std::fs::create_dir(workspace.path().join("data")).unwrap();
  let request = job(&workspace.path().canonicalize().unwrap());
  let (writer, mut reader) = tokio::io::duplex(4096);
  let mut stdin: ExecutionWriter = Box::pin(writer);
  let mut reason = None;
  let mut deadline = None;

  handle_delivery(
    DeliveryOutcome::Delivered,
    &mut reason,
    &mut deadline,
    &request,
    &mut stdin,
  )
  .await
  .unwrap();
  assert_eq!(reason, None);

  handle_delivery(
    DeliveryOutcome::Cancelled,
    &mut reason,
    &mut deadline,
    &request,
    &mut stdin,
  )
  .await
  .unwrap();
  assert_eq!(reason, Some(TerminationReason::Cancelled));
  assert!(deadline.is_some());
  let mut command = String::new();
  BufReader::new(&mut reader).read_line(&mut command).await.unwrap();
  assert!(command.contains("\"type\":\"cancel\""));

  let original_deadline = deadline;
  begin_stop(
    &mut reason,
    &mut deadline,
    TerminationReason::TimedOut,
    request.cancellation_grace,
    &mut stdin,
    &request.request_id,
  )
  .await;
  assert_eq!(reason, Some(TerminationReason::Cancelled));
  assert_eq!(deadline, original_deadline);

  let (writer, _reader) = tokio::io::duplex(4096);
  let mut stdin: ExecutionWriter = Box::pin(writer);
  reason = None;
  deadline = None;
  handle_delivery(
    DeliveryOutcome::TimedOut,
    &mut reason,
    &mut deadline,
    &request,
    &mut stdin,
  )
  .await
  .unwrap();
  assert_eq!(reason, Some(TerminationReason::TimedOut));

  wait_for_deadline(Some(Instant::now())).await;
  assert!(
    tokio::time::timeout(Duration::from_millis(1), wait_for_deadline(None))
      .await
      .is_err()
  );
}
