//! Supervises the complete lifecycle of one `octa-runner` invocation.
//!
//! This module joins the backend-neutral execution handle with the shared
//! runner protocol. It validates message ordering, forwards events and resource
//! samples, enforces cancellation deadlines, and always destroys the backend.

use std::time::Duration;

use octa_runner_protocol::RunStatus;
use octacity_execution::{ExecutionBackend, ExecutionError, ResourceUsage, StartExecution};
use octacity_protocol::{ExecutionSpec, OctaSpec};
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt as _},
  sync::mpsc,
  time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
  installation::{RunnerInstallation, RunnerInstallationError},
  protocol::{RunnerEvent, RunnerProtocolError},
};

mod lifecycle;
mod redaction;
#[path = "supervisor_validation.rs"]
mod validation;

pub use redaction::RunnerRedactions;

const MAX_RUNNER_STDERR_BYTES: usize = 64 * 1024;

/// Operational timing policy for runner supervision.
///
/// These values are agent policy rather than wire-protocol constants, so they
/// are supplied by configuration and can be tuned without changing JobSpec.
#[derive(Clone, Debug)]
pub struct RunnerSupervisionPolicy {
  /// Maximum time allowed for the runner protocol handshake.
  pub hello_timeout: Duration,
  /// Interval between backend resource-accounting samples.
  pub resource_sample_interval: Duration,
  /// Maximum duration of one accounting call.
  pub resource_sample_timeout: Duration,
  /// Consecutive accounting failures tolerated before failing the job.
  pub max_accounting_failures: usize,
}

impl RunnerSupervisionPolicy {
  /// Rejects policies that could disable lifecycle or accounting bounds.
  pub fn validate(&self) -> Result<(), RunnerSupervisionError> {
    if self.hello_timeout.is_zero()
      || self.resource_sample_interval.is_zero()
      || self.resource_sample_timeout.is_zero()
      || self.max_accounting_failures == 0
    {
      return invalid("runner supervision timings and failure limit must be greater than zero");
    }
    Ok(())
  }
}

impl Default for RunnerSupervisionPolicy {
  fn default() -> Self {
    Self {
      hello_timeout: Duration::from_secs(5),
      resource_sample_interval: Duration::from_secs(5),
      resource_sample_timeout: Duration::from_secs(1),
      max_accounting_failures: 3,
    }
  }
}

/// Inputs that bind a signed job to one backend execution and protocol request.
#[derive(Debug)]
pub struct RunnerJobRequest {
  /// Identifier shared by the backend execution and runner protocol.
  pub request_id: String,
  /// Exact installed Octa release required by the signed job.
  pub octa: OctaSpec,
  /// Backend-neutral execution request.
  pub execution: StartExecution,
  /// Runner tasks, environment, and output configuration.
  pub spec: ExecutionSpec,
  /// Sensitive byte strings removed from untrusted runner messages.
  ///
  /// This is agent-local policy and is deliberately absent from the runner
  /// wire request, JobSpec, and every persisted lifecycle record.
  pub redactions: RunnerRedactions,
  /// Time allowed for graceful runner cancellation before a forced kill.
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
    if self.cancellation_grace.is_zero() {
      return invalid("cancellation timeout must be greater than zero");
    }
    Ok(())
  }
}

/// External reason that forced an otherwise active runner to stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationReason {
  /// The control plane or local agent requested cancellation.
  Cancelled,
  /// The job exhausted its execution deadline.
  TimedOut,
}

/// Ordered information emitted to the future server-agent transport.
#[derive(Clone, Debug, PartialEq)]
pub enum RunnerStreamItem {
  /// Structured event emitted by `octa-runner`.
  Event(RunnerEvent),
  /// Cumulative resource usage sampled from the execution backend.
  ResourceUsage(ResourceUsage),
  /// Resource accounting temporarily failed but remains below policy limits.
  AccountingUnavailable {
    /// Number of consecutive accounting failures observed so far.
    consecutive_failures: usize,
  },
}

/// Terminal runner result plus the final backend accounting snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct RunnerCompletion {
  /// Runner-reported terminal job status.
  pub status: RunStatus,
  /// Validated task results returned by the runner.
  pub results: Vec<serde_json::Value>,
  /// Final cumulative resource accounting snapshot.
  pub final_usage: ResourceUsage,
  /// External stop reason, when cancellation or timeout initiated termination.
  pub termination_reason: Option<TerminationReason>,
}

#[derive(Debug, Error)]
/// Failure while validating, executing, or cleaning up a runner job.
pub enum RunnerSupervisionError {
  #[error(transparent)]
  /// The installed runner does not satisfy the signed job.
  Installation(#[from] RunnerInstallationError),
  #[error(transparent)]
  /// The selected execution backend failed.
  Execution(#[from] ExecutionError),
  #[error(transparent)]
  /// Reading or writing runner protocol frames failed.
  ProtocolIo(#[from] RunnerProtocolError),
  #[error("invalid runner request: {0}")]
  /// The request violates a supervisor invariant.
  Invalid(String),
  #[error("octa-runner violated its protocol: {0}")]
  /// Runner messages arrived with an invalid value or lifecycle order.
  Protocol(String),
  #[error("octa-runner failed: {0}")]
  /// The runner reported an operational failure.
  Runner(String),
  #[error("runner event consumer stopped")]
  /// The downstream event channel closed while the job was active.
  EventConsumerStopped,
  #[error("resource accounting failed {0} consecutive times")]
  /// Backend accounting exceeded the configured consecutive failure limit.
  AccountingUnavailable(usize),
  #[error("octa-runner did not stop within the cancellation grace period")]
  /// The runner ignored cancellation until the grace deadline expired.
  CancellationTimeout,
  #[error("runner startup exhausted the execution deadline")]
  /// Backend startup consumed the complete job deadline.
  StartupTimeout,
  #[error("runner supervision failed ({operation}) and backend cleanup also failed: {cleanup}")]
  /// Both the primary operation and mandatory backend cleanup failed.
  OperationAndCleanup {
    /// Original supervision failure.
    operation: Box<RunnerSupervisionError>,
    /// Subsequent backend cleanup failure.
    cleanup: ExecutionError,
  },
}

/// Starts, drives, and unconditionally destroys one runner execution.
pub async fn supervise(
  runner: &RunnerInstallation,
  backend: &dyn ExecutionBackend,
  mut job: RunnerJobRequest,
  cancellation: CancellationToken,
  events: &mpsc::Sender<RunnerStreamItem>,
  policy: &RunnerSupervisionPolicy,
) -> Result<RunnerCompletion, RunnerSupervisionError> {
  job.validate()?;
  policy.validate()?;
  if cancellation.is_cancelled() {
    return Err(ExecutionError::Cancelled.into());
  }
  let deadline = Instant::now()
    .checked_add(job.execution.max_duration)
    .ok_or_else(|| RunnerSupervisionError::Invalid("execution timeout is too large".to_owned()))?;
  runner.verify(&job.octa)?;
  job.execution.max_duration = deadline.saturating_duration_since(Instant::now());
  if job.execution.max_duration.is_zero() {
    return Err(RunnerSupervisionError::StartupTimeout);
  }
  let program = runner.program();
  let mut execution = backend
    .start(&program, job.execution.clone(), cancellation.clone())
    .await?;
  let io = match execution.take_io() {
    Ok(io) => io,
    Err(error) => {
      let operation = RunnerSupervisionError::Execution(error);
      return match execution.destroy().await {
        Ok(()) => Err(operation),
        Err(cleanup) => Err(RunnerSupervisionError::OperationAndCleanup {
          operation: Box::new(operation),
          cleanup,
        }),
      };
    }
  };
  // stderr is diagnostic-only: protocol messages are accepted exclusively on
  // stdout, and the bounded drain prevents a noisy runner from deadlocking.
  let stderr_task = tokio::spawn(read_bounded(io.stderr, MAX_RUNNER_STDERR_BYTES));
  let result = lifecycle::drive(
    &mut *execution,
    (io.stdin, io.stdout),
    &mut job,
    deadline,
    cancellation,
    events,
    policy,
  )
  .await;
  let termination = if result.is_err() {
    execution.kill().await
  } else {
    Ok(())
  };
  let destruction = execution.destroy().await;
  let cleanup = match (termination, destruction) {
    (Ok(()), Ok(())) => Ok(()),
    (Err(termination), Ok(())) => Err(termination),
    (Ok(()), Err(destruction)) => Err(destruction),
    (Err(termination), Err(destruction)) => Err(termination.with_cleanup(destruction)),
  };
  let result = match (result, join_stderr(stderr_task).await) {
    (Ok(_), Ok(stderr)) if stderr.truncated => Err(protocol("runner stderr exceeded its bounded limit")),
    (Ok(completion), Ok(stderr)) => {
      if !stderr.bytes.is_empty() {
        debug!(bytes = stderr.bytes.len(), "octa-runner wrote diagnostic stderr");
      }
      Ok(completion)
    }
    (Ok(_), Err(error)) => Err(error),
    (Err(operation), _) => Err(operation),
  };
  match (result, cleanup) {
    (result, Ok(())) => result,
    (Ok(_), Err(cleanup)) => Err(cleanup.into()),
    (Err(operation), Err(cleanup)) => Err(RunnerSupervisionError::OperationAndCleanup {
      operation: Box::new(operation),
      cleanup,
    }),
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
#[path = "supervisor_tests.rs"]
mod tests;
