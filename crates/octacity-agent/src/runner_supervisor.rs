use std::{future::pending, time::Duration};

use octacity_protocol::ExecutionSpec;
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt as _, BufReader},
  sync::mpsc,
  time::{Instant, MissedTickBehavior, interval, sleep_until, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
  execution::{ExecutionBackend, ExecutionError, ExecutionExit, ResourceUsage, RunningExecution, StartExecution},
  runner_protocol::{
    RUNNER_EVENT_SCHEMA_VERSION, RunRequest, RunStatus, RunnerCommand, RunnerEvent, RunnerMessage, RunnerProtocolError,
    write_command,
  },
};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const RESOURCE_SAMPLE_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_ACCOUNTING_FAILURES: usize = 3;
const MAX_RUNNER_STDERR_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct RunnerJobRequest {
  pub request_id: String,
  pub execution: StartExecution,
  pub spec: ExecutionSpec,
  pub timeout: Duration,
  pub cancellation_grace: Duration,
}

impl RunnerJobRequest {
  fn validate(&self) -> Result<(), RunnerSupervisionError> {
    self.execution.validate()?;
    if self.request_id != self.execution.execution_id {
      return invalid("request_id must equal execution_id");
    }
    if self.spec.commands.is_empty() || self.spec.commands.iter().any(String::is_empty) {
      return invalid("runner commands must contain at least one non-empty task");
    }
    if self.timeout.is_zero() || self.cancellation_grace.is_zero() {
      return invalid("execution and cancellation timeouts must be greater than zero");
    }
    Ok(())
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationReason {
  Cancelled,
  TimedOut,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunnerStreamItem {
  Event(RunnerEvent),
  ResourceUsage(ResourceUsage),
  AccountingUnavailable { consecutive_failures: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunnerCompletion {
  pub status: RunStatus,
  pub results: Vec<serde_json::Value>,
  pub final_usage: ResourceUsage,
  pub termination_reason: Option<TerminationReason>,
}

#[derive(Debug, Error)]
pub enum RunnerSupervisionError {
  #[error(transparent)]
  Execution(#[from] ExecutionError),
  #[error(transparent)]
  ProtocolIo(#[from] RunnerProtocolError),
  #[error("invalid runner request: {0}")]
  Invalid(String),
  #[error("octa-runner violated its protocol: {0}")]
  Protocol(String),
  #[error("octa-runner failed: {0}")]
  Runner(String),
  #[error("runner event consumer stopped")]
  EventConsumerStopped,
  #[error("resource accounting failed {0} consecutive times")]
  AccountingUnavailable(usize),
  #[error("octa-runner did not stop within the cancellation grace period")]
  CancellationTimeout,
}

pub async fn supervise(
  backend: &dyn ExecutionBackend,
  job: RunnerJobRequest,
  cancellation: CancellationToken,
  events: &mpsc::Sender<RunnerStreamItem>,
) -> Result<RunnerCompletion, RunnerSupervisionError> {
  job.validate()?;
  let mut execution = backend.start(job.execution.clone()).await?;
  let run = build_run_request(&job.spec, execution.paths());
  let io = match execution.take_io() {
    Ok(io) => io,
    Err(error) => {
      let _ = execution.destroy().await;
      return Err(error.into());
    }
  };
  let stderr_task = tokio::spawn(read_bounded(io.stderr, MAX_RUNNER_STDERR_BYTES));
  let result = drive(&mut *execution, io.stdin, io.stdout, &job, run, cancellation, events).await;
  if result.is_err() {
    let _ = execution.kill().await;
  }
  let cleanup = execution.destroy().await;
  let stderr = join_stderr(stderr_task).await;

  cleanup?;
  let stderr = stderr?;
  if stderr.truncated {
    return Err(protocol("runner stderr exceeded its bounded limit"));
  }
  if !stderr.bytes.is_empty() {
    debug!(bytes = stderr.bytes.len(), "octa-runner wrote diagnostic stderr");
  }
  result
}

async fn drive(
  execution: &mut dyn RunningExecution,
  mut stdin: crate::execution::ExecutionWriter,
  stdout: crate::execution::ExecutionReader,
  job: &RunnerJobRequest,
  run: RunRequest,
  cancellation: CancellationToken,
  events: &mpsc::Sender<RunnerStreamItem>,
) -> Result<RunnerCompletion, RunnerSupervisionError> {
  let mut stdout = BufReader::new(stdout);
  let hello = timeout(HELLO_TIMEOUT, crate::runner_protocol::read_message(&mut stdout))
    .await
    .map_err(|_| protocol("hello timed out"))??
    .ok_or_else(|| protocol("stdout closed before hello"))?;
  validate_hello(&hello, &job.execution.octa)?;
  debug!(request_id = %job.request_id, "validated octa-runner hello");

  write_command(
    &mut stdin,
    &RunnerCommand::Start {
      protocol_version: job.execution.octa.runner_protocol,
      request_id: job.request_id.clone(),
      request: Box::new(run),
    },
  )
  .await?;

  let operation_deadline = sleep_until(Instant::now() + job.timeout);
  tokio::pin!(operation_deadline);
  let mut stop_deadline = None;
  let mut stop_reason = None;
  let mut accepted = false;
  let mut last_event_sequence = None;
  let mut accounting_failures = 0;
  let mut sampler = interval(RESOURCE_SAMPLE_INTERVAL);
  sampler.set_missed_tick_behavior(MissedTickBehavior::Skip);
  sampler.tick().await;
  info!(request_id = %job.request_id, "started octa-runner request");

  loop {
    tokio::select! {
      message = crate::runner_protocol::read_message(&mut stdout) => {
        let message = match message {
          Ok(Some(message)) => message,
          Ok(None) => return Err(protocol("stdout closed before a terminal message")),
          Err(error) => return Err(error.into()),
        };
        match message {
          RunnerMessage::Accepted { request_id } if request_id == job.request_id && !accepted => {
            accepted = true;
            debug!(request_id = %job.request_id, "octa-runner accepted request");
          },
          RunnerMessage::Event { request_id, event } if request_id == job.request_id && accepted => {
            validate_event(&event, &mut last_event_sequence)?;
            if !deliver(events, RunnerStreamItem::Event(event), &cancellation).await? {
              begin_stop(
                &mut stop_reason,
                &mut stop_deadline,
                TerminationReason::Cancelled,
                job.cancellation_grace,
                &mut stdin,
                &job.request_id,
              ).await;
            }
          },
          RunnerMessage::Finished { request_id, status, results } if request_id == job.request_id && accepted => {
            let final_usage = final_usage(execution).await?;
            let exit = timeout(job.cancellation_grace, execution.wait())
              .await
              .map_err(|_| RunnerSupervisionError::CancellationTimeout)??;
            validate_exit(status, exit)?;
            info!(request_id = %job.request_id, ?status, "octa-runner finished request");
            return Ok(RunnerCompletion {
              status,
              results,
              final_usage,
              termination_reason: stop_reason,
            });
          },
          RunnerMessage::Error { request_id, message }
            if request_id.as_deref().is_none_or(|request_id| request_id == job.request_id) => {
              return Err(RunnerSupervisionError::Runner(message));
            },
          _ => return Err(protocol("unexpected, duplicate, or incorrectly correlated message")),
        }
      },
      () = cancellation.cancelled(), if stop_reason.is_none() => {
        warn!(request_id = %job.request_id, "cancelling octa-runner request");
        begin_stop(
          &mut stop_reason,
          &mut stop_deadline,
          TerminationReason::Cancelled,
          job.cancellation_grace,
          &mut stdin,
          &job.request_id,
        ).await;
      },
      () = &mut operation_deadline, if stop_reason.is_none() => {
        warn!(request_id = %job.request_id, "octa-runner request timed out");
        begin_stop(
          &mut stop_reason,
          &mut stop_deadline,
          TerminationReason::TimedOut,
          job.cancellation_grace,
          &mut stdin,
          &job.request_id,
        ).await;
      },
      () = wait_for_deadline(stop_deadline), if stop_reason.is_some() => {
        execution.kill().await?;
        return Err(RunnerSupervisionError::CancellationTimeout);
      },
      _ = sampler.tick(), if stop_reason.is_none() => {
        match timeout(RESOURCE_SAMPLE_TIMEOUT, execution.sample_usage()).await {
          Ok(Ok(usage)) => {
            accounting_failures = 0;
            if !deliver(events, RunnerStreamItem::ResourceUsage(usage), &cancellation).await? {
              begin_stop(
                &mut stop_reason,
                &mut stop_deadline,
                TerminationReason::Cancelled,
                job.cancellation_grace,
                &mut stdin,
                &job.request_id,
              ).await;
            }
          },
          Ok(Err(error)) => {
            accounting_failures += 1;
            warn!(request_id = %job.request_id, consecutive_failures = accounting_failures, error = %error, "resource sample failed");
            if !deliver(
              events,
              RunnerStreamItem::AccountingUnavailable {
                consecutive_failures: accounting_failures,
              },
              &cancellation,
            ).await? {
              begin_stop(
                &mut stop_reason,
                &mut stop_deadline,
                TerminationReason::Cancelled,
                job.cancellation_grace,
                &mut stdin,
                &job.request_id,
              ).await;
            }
          },
          Err(_) => {
            accounting_failures += 1;
            warn!(request_id = %job.request_id, consecutive_failures = accounting_failures, "resource sample timed out");
            if !deliver(
              events,
              RunnerStreamItem::AccountingUnavailable {
                consecutive_failures: accounting_failures,
              },
              &cancellation,
            ).await? {
              begin_stop(
                &mut stop_reason,
                &mut stop_deadline,
                TerminationReason::Cancelled,
                job.cancellation_grace,
                &mut stdin,
                &job.request_id,
              ).await;
            }
          },
        }
        if accounting_failures >= MAX_ACCOUNTING_FAILURES {
          return Err(RunnerSupervisionError::AccountingUnavailable(accounting_failures));
        }
      },
    }
  }
}

fn build_run_request(spec: &ExecutionSpec, paths: &crate::execution::ExecutionPaths) -> RunRequest {
  RunRequest {
    workspace: paths.workspace.clone(),
    octafile: spec.octafile.as_ref().map(Into::into),
    data_dir: paths.data_dir.clone(),
    plugins_dir: paths.plugins_dir.clone(),
    plugin_lock: Some(paths.plugin_lock.clone()),
    secrets_profile: spec.secrets_profile.as_ref().map(Into::into),
    plugins: Vec::new(),
    default_plugin: None,
    commands: spec.commands.clone(),
    variables: spec.variables.clone(),
    arguments: spec.arguments.clone(),
    concurrency: spec.concurrency,
    parallel: spec.parallel,
    failfast: spec.failfast,
    dry: false,
    force: false,
    quiet: false,
    silence: None,
  }
}

fn validate_hello(
  message: &RunnerMessage,
  expectation: &octacity_protocol::OctaSpec,
) -> Result<(), RunnerSupervisionError> {
  match message {
    RunnerMessage::Hello {
      protocol_version,
      octa_version,
      event_schema_version,
      plugin_protocol_version,
    } if *protocol_version == expectation.runner_protocol
      && octa_version == &expectation.version
      && *event_schema_version == expectation.event_schema
      && *plugin_protocol_version == expectation.plugin_protocol =>
    {
      Ok(())
    }
    RunnerMessage::Hello { .. } => Err(protocol("hello does not match the signed Octa requirement")),
    _ => Err(protocol("first message was not hello")),
  }
}

fn validate_event(event: &RunnerEvent, previous: &mut Option<u64>) -> Result<(), RunnerSupervisionError> {
  if event.schema_version != RUNNER_EVENT_SCHEMA_VERSION || event.timestamp.is_empty() {
    return Err(protocol("runner event has an invalid schema version or timestamp"));
  }
  if !matches!(event.category.as_str(), "execution" | "diagnostic" | "document") {
    return Err(protocol("runner event has an unknown category"));
  }
  let expected = previous.map_or(0, |sequence| sequence.saturating_add(1));
  if event.sequence != expected {
    return Err(protocol(format!(
      "runner event sequence {} followed {:?}, expected {expected}",
      event.sequence, previous
    )));
  }
  *previous = Some(event.sequence);
  Ok(())
}

fn validate_exit(status: RunStatus, exit: ExecutionExit) -> Result<(), RunnerSupervisionError> {
  let expected = match status {
    RunStatus::Succeeded => 0,
    RunStatus::Failed => 1,
    RunStatus::Cancelled => 130,
  };
  if exit.code == Some(expected) {
    Ok(())
  } else {
    Err(protocol(format!(
      "terminal status does not match process exit status {:?}",
      exit.code
    )))
  }
}

async fn final_usage(execution: &mut dyn RunningExecution) -> Result<ResourceUsage, RunnerSupervisionError> {
  timeout(RESOURCE_SAMPLE_TIMEOUT, execution.sample_usage())
    .await
    .map_err(|_| RunnerSupervisionError::AccountingUnavailable(MAX_ACCOUNTING_FAILURES))?
    .map_err(Into::into)
}

async fn deliver(
  events: &mpsc::Sender<RunnerStreamItem>,
  item: RunnerStreamItem,
  cancellation: &CancellationToken,
) -> Result<bool, RunnerSupervisionError> {
  tokio::select! {
    result = events.send(item) => {
      result.map_err(|_| RunnerSupervisionError::EventConsumerStopped)?;
      Ok(true)
    },
    () = cancellation.cancelled() => Ok(false),
  }
}

async fn begin_stop(
  stop_reason: &mut Option<TerminationReason>,
  stop_deadline: &mut Option<Instant>,
  reason: TerminationReason,
  grace: Duration,
  stdin: &mut crate::execution::ExecutionWriter,
  request_id: &str,
) {
  if stop_reason.is_some() {
    return;
  }
  *stop_reason = Some(reason);
  *stop_deadline = Some(Instant::now() + grace);
  let _ = write_command(
    stdin,
    &RunnerCommand::Cancel {
      request_id: request_id.to_owned(),
    },
  )
  .await;
}

async fn wait_for_deadline(deadline: Option<Instant>) {
  match deadline {
    Some(deadline) => sleep_until(deadline).await,
    None => pending().await,
  }
}

struct BoundedStderr {
  bytes: Vec<u8>,
  truncated: bool,
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> Result<BoundedStderr, std::io::Error> {
  let mut bytes = Vec::with_capacity(limit.min(8192));
  let mut buffer = [0_u8; 8192];
  let mut truncated = false;
  loop {
    let read = reader.read(&mut buffer).await?;
    if read == 0 {
      return Ok(BoundedStderr { bytes, truncated });
    }
    let remaining = limit.saturating_sub(bytes.len());
    bytes.extend_from_slice(&buffer[..read.min(remaining)]);
    truncated |= read > remaining;
  }
}

async fn join_stderr(
  task: tokio::task::JoinHandle<Result<BoundedStderr, std::io::Error>>,
) -> Result<BoundedStderr, RunnerSupervisionError> {
  task
    .await
    .map_err(|error| ExecutionError::Io(std::io::Error::other(error)))?
    .map_err(ExecutionError::Io)
    .map_err(Into::into)
}

fn invalid<T>(message: impl Into<String>) -> Result<T, RunnerSupervisionError> {
  Err(RunnerSupervisionError::Invalid(message.into()))
}

fn protocol(message: impl Into<String>) -> RunnerSupervisionError {
  RunnerSupervisionError::Protocol(message.into())
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
      Arc,
      atomic::{AtomicBool, Ordering},
    },
  };

  use async_trait::async_trait;
  use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

  use super::*;
  use crate::execution::{ExecutionIo, ExecutionPaths, ExecutionReader, ExecutionWriter};

  #[derive(Clone, Copy)]
  enum Scenario {
    Success,
    Cancelled,
    BadHello,
  }

  struct FakeBackend {
    destroyed: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
    scenario: Scenario,
  }

  struct FakeExecution {
    io: Option<ExecutionIo>,
    paths: ExecutionPaths,
    destroyed: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
    exit_code: i32,
  }

  #[async_trait]
  impl ExecutionBackend for FakeBackend {
    async fn start(&self, request: StartExecution) -> Result<Box<dyn RunningExecution>, ExecutionError> {
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
          plugins_dir: PathBuf::from("/opt/octa/plugins"),
          plugin_lock: PathBuf::from("/opt/octa/Octa.lock"),
        },
        destroyed: self.destroyed.clone(),
        killed: self.killed.clone(),
        exit_code: match self.scenario {
          Scenario::Cancelled => 130,
          Scenario::Success | Scenario::BadHello => 0,
        },
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
      Ok(())
    }
  }

  async fn fake_runner(input: tokio::io::DuplexStream, mut output: tokio::io::DuplexStream, scenario: Scenario) {
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
    let mut input = BufReader::new(input);
    let mut command = String::new();
    input.read_line(&mut command).await.unwrap();
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
        b"{\"type\":\"accepted\",\"request_id\":\"job-1-attempt-1\"}\n{\"type\":\"event\",\"request_id\":\"job-1-attempt-1\",\"event\":{\"schema_version\":3,\"sequence\":0,\"timestamp\":\"2026-09-10T00:00:00Z\",\"category\":\"execution\",\"data\":{\"type\":\"run_started\"}}}\n{\"type\":\"finished\",\"request_id\":\"job-1-attempt-1\",\"status\":\"succeeded\",\"results\":[]}\n",
      )
      .await
      .unwrap();
  }

  fn job(workspace: &std::path::Path) -> RunnerJobRequest {
    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    RunnerJobRequest {
      request_id: "job-1-attempt-1".to_owned(),
      execution: StartExecution {
        execution_id: "job-1-attempt-1".to_owned(),
        workspace: workspace.to_owned(),
        data_dir: workspace.join("data"),
        octa: octacity_protocol::OctaSpec {
          version: "0.3.0".to_owned(),
          runner_sha256: DIGEST.to_owned(),
          runner_protocol: 1,
          event_schema: 3,
          plugin_protocol: 1,
          plugin_digests: BTreeMap::new(),
        },
        cpu_millis: 1000,
        memory_bytes: 1024,
        writable_disk_bytes: 1024,
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
      timeout: Duration::from_secs(2),
      cancellation_grace: Duration::from_secs(1),
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
    };
    let (sender, mut receiver) = mpsc::channel(8);
    let completion = supervise(
      &backend,
      job(&workspace.path().canonicalize().unwrap()),
      CancellationToken::new(),
      &sender,
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
    };
    let (sender, _receiver) = mpsc::channel(8);
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let completion = supervise(
      &backend,
      job(&workspace.path().canonicalize().unwrap()),
      cancellation,
      &sender,
    )
    .await
    .unwrap();

    assert_eq!(completion.status, RunStatus::Cancelled);
    assert_eq!(completion.termination_reason, Some(TerminationReason::Cancelled));
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
    };
    let (sender, _receiver) = mpsc::channel(8);

    let error = supervise(
      &backend,
      job(&workspace.path().canonicalize().unwrap()),
      CancellationToken::new(),
      &sender,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("signed Octa requirement"));
    assert!(killed.load(Ordering::SeqCst));
    assert!(destroyed.load(Ordering::SeqCst));
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
    request.timeout = Duration::ZERO;
    assert!(request.validate().unwrap_err().to_string().contains("timeouts"));

    let expectation = &request.execution.octa;
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
      sequence: 0,
      timestamp: "2026-09-10T00:00:00Z".to_owned(),
      category: "diagnostic".to_owned(),
      data: serde_json::Map::from_iter([("type".to_owned(), serde_json::json!("message"))]),
    };
    assert!(validate_event(&event, &mut sequence).is_ok());
    event.sequence = 2;
    assert!(validate_event(&event, &mut sequence).is_err());
    event.sequence = 1;
    event.category = "unknown".to_owned();
    assert!(validate_event(&event, &mut sequence).is_err());

    assert!(validate_exit(RunStatus::Succeeded, ExecutionExit { code: Some(0) }).is_ok());
    assert!(validate_exit(RunStatus::Failed, ExecutionExit { code: Some(1) }).is_ok());
    assert!(validate_exit(RunStatus::Cancelled, ExecutionExit { code: Some(130) }).is_ok());
    assert!(validate_exit(RunStatus::Succeeded, ExecutionExit { code: None }).is_err());
  }
}
