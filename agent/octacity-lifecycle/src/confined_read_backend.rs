//! One serialized, owner-tracked read worker using the existing execution lifecycle.

use std::{
  future::Future,
  path::Path,
  pin::Pin,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionError, ExecutionOs, ExecutionProgram, ExecutionTarget,
  FACTORY_SOURCE_ROOT, FactoryExecutionPartition, ResourceUsage, RunningExecution, StartExecution,
};
use octacity_job::AuthorizedFactoryToolAction;
use octacity_protocol::{MAX_PROTECTED_READ_REQUEST_BYTES, ProtectedReadWorkerRequestV1};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  sync::{Semaphore, oneshot},
  task::JoinHandle,
  time::Instant,
};
use tokio_util::sync::CancellationToken;

use crate::{AgentToolActionBrokerError as Error, ProtectedReadBackend, ProtectedReadInventory, ProtectedReadRecipe};

#[path = "confined_read_execution.rs"]
mod execution;

/// Physically confined reader with one original Job window and a reserved share.
///
/// The returned narrowed workload must be used by the same execution owner.
/// That owner must await `shutdown` during teardown. This component grants no
/// tool authority and does not qualify a complete controlled coding-harness Job.
pub struct ConfinedProtectedReadBackend {
  shared: Arc<ReaderState>,
}

struct ReaderState {
  backend: Arc<dyn ExecutionBackend>,
  program: ExecutionProgram,
  recipe: ProtectedReadRecipe,
  request: StartExecution,
  deadline: Instant,
  serial: Arc<Semaphore>,
  cancellation: CancellationToken,
  tasks: Mutex<Vec<JoinHandle<()>>>,
  stopped: AtomicBool,
  failed: AtomicBool,
  usage: Mutex<Option<ResourceUsage>>,
}

impl ConfinedProtectedReadBackend {
  /// Pins the installed worker and exact allocation before either VM starts.
  /// No paths, executable or resource defaults are inferred from a model action.
  pub fn prepare(
    inventory: &dyn ProtectedReadInventory,
    backend: Arc<dyn ExecutionBackend>,
    partition: FactoryExecutionPartition,
    absolute_deadline: Instant,
  ) -> Result<(StartExecution, Self), Error> {
    let recipe = inventory.installed_rooted_reader().ok_or(Error::Denied)?;
    let program = inventory.installed_rooted_reader_program().ok_or(Error::Denied)?;
    let (mut main, helper) = partition.into_parts();
    let template = recipe.action(FACTORY_SOURCE_ROOT)?.into_proposal();
    let resources = template.resources;
    if helper.cpu_millis != resources.cpu_millis
      || helper.memory_bytes != resources.memory_bytes
      || helper.writable_disk_bytes != resources.disk_bytes
      || helper
        .factory
        .as_ref()
        .is_none_or(|layout| layout.process_limit != resources.process_count)
      || helper.max_duration != Duration::from_millis(resources.elapsed_millis)
      || program.plugin_projection.is_some()
      || !program.external_executables.is_empty()
    {
      return Err(Error::Mismatch);
    }
    verify_program(&program, &helper, &template.executable.sha256)?;
    let remaining = absolute_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return Err(Error::TimedOut);
    }
    main.max_duration = main.max_duration.min(remaining);
    let reader = Self {
      shared: Arc::new(ReaderState {
        backend,
        program,
        recipe,
        request: helper,
        deadline: absolute_deadline,
        serial: Arc::new(Semaphore::new(1)),
        cancellation: CancellationToken::new(),
        tasks: Mutex::new(vec![]),
        stopped: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        usage: Mutex::new(None),
      }),
    };
    Ok((main, reader))
  }

  /// Cancels and joins the tracked physical worker before its owner can finish.
  /// A failed cleanup remains an error rather than permitting a later worker.
  pub async fn shutdown(&self) -> Result<(), ExecutionError> {
    self.shared.stopped.store(true, Ordering::SeqCst);
    self.shared.cancellation.cancel();
    let _drained = self.shared.serial.acquire().await.map_err(|_| cleanup_error())?;
    let tasks = std::mem::take(&mut *self.shared.tasks.lock().map_err(|_| cleanup_error())?);
    for task in tasks {
      if task.await.is_err() {
        self.shared.failed.store(true, Ordering::SeqCst);
      }
    }
    if self.shared.failed.load(Ordering::SeqCst) {
      Err(cleanup_error())
    } else {
      Ok(())
    }
  }

  /// Cumulative accounting for workers whose physical destruction completed.
  /// CPU, I/O and worker elapsed time accumulate across serialized reads;
  /// memory and writable-disk peaks share the same reserved allocation. This
  /// snapshot excludes an active worker and is final only after `shutdown`.
  /// It does not replace the main execution's elapsed Job window.
  pub fn completed_usage(&self) -> Result<ResourceUsage, ExecutionError> {
    if self.shared.failed.load(Ordering::SeqCst) {
      return Err(cleanup_error());
    }
    Ok(
      self
        .shared
        .usage
        .lock()
        .map_err(|_| cleanup_error())?
        .clone()
        .unwrap_or_default(),
    )
  }
}

impl Drop for ConfinedProtectedReadBackend {
  fn drop(&mut self) {
    // The tracked task retains the physical handle and completes teardown.
    // Normal owners additionally await shutdown to observe its result.
    self.shared.stopped.store(true, Ordering::SeqCst);
    self.shared.cancellation.cancel();
  }
}

impl ProtectedReadBackend for ConfinedProtectedReadBackend {
  fn read<'a>(
    &'a self,
    action: AuthorizedFactoryToolAction,
    max_bytes: u64,
    cancellation: CancellationToken,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + 'a>> {
    Box::pin(async move {
      let proposal = action.into_proposal();
      let path = proposal.paths.first().ok_or(Error::Mismatch)?;
      if self.shared.recipe.action(&path.path)?.into_proposal() != proposal {
        return Err(Error::Mismatch);
      }
      let relative = Path::new(&path.path)
        .strip_prefix(FACTORY_SOURCE_ROOT)
        .map_err(|_| Error::Mismatch)?;
      let input = ProtectedReadWorkerRequestV1 {
        schema_version: 1,
        relative_path: relative.to_str().ok_or(Error::Mismatch)?.to_owned(),
        max_bytes,
      };
      input.validate().map_err(|_| Error::Mismatch)?;
      let permit = tokio::select! {
        biased;
        () = cancellation.cancelled() => return Err(Error::Cancelled),
        () = self.shared.cancellation.cancelled() => return Err(Error::Cancelled),
        () = tokio::time::sleep_until(self.shared.deadline) => return Err(Error::TimedOut),
        permit = self.shared.serial.clone().acquire_owned() => permit.map_err(|_| Error::Denied)?,
      };
      if self.shared.stopped.load(Ordering::SeqCst) || self.shared.failed.load(Ordering::SeqCst) {
        return Err(Error::Denied);
      }
      let (receive, caller_guard) = {
        let mut tasks = self.shared.tasks.lock().map_err(|_| Error::Denied)?;
        if self.shared.stopped.load(Ordering::SeqCst) {
          return Err(Error::Denied);
        }
        let shared = self.shared.clone();
        let local_cancel = shared.cancellation.child_token();
        let caller_guard = local_cancel.clone().drop_guard();
        let (send, receive) = oneshot::channel();
        tasks.retain(|task| !task.is_finished());
        tasks.push(tokio::spawn(async move {
          let _permit = permit;
          let result = run_worker(shared, input, local_cancel, cancellation).await;
          let _ = send.send(result);
        }));
        (receive, caller_guard)
      };
      let result = receive.await.map_err(|_| Error::Denied)?;
      drop(caller_guard);
      result
    })
  }
}

async fn run_worker(
  shared: Arc<ReaderState>,
  input: ProtectedReadWorkerRequestV1,
  cancellation: CancellationToken,
  caller_cancellation: CancellationToken,
) -> Result<Vec<u8>, Error> {
  if cancellation.is_cancelled() || caller_cancellation.is_cancelled() {
    return Err(Error::Cancelled);
  }
  let template = shared.recipe.action(FACTORY_SOURCE_ROOT)?.into_proposal();
  verify_program(&shared.program, &shared.request, &template.executable.sha256)?;
  let mut request = shared.request.clone();
  request.max_duration = request
    .max_duration
    .min(shared.deadline.saturating_duration_since(Instant::now()));
  if request.max_duration.is_zero() {
    return Err(Error::TimedOut);
  }
  let deadline = Instant::now()
    .checked_add(request.max_duration)
    .ok_or(Error::TimedOut)?
    .min(shared.deadline);
  let watchdog = tokio::spawn({
    let cancel = cancellation.clone();
    async move {
      tokio::select! {
        () = cancel.cancelled() => {},
        () = caller_cancellation.cancelled() => cancel.cancel(),
        () = tokio::time::sleep_until(deadline) => cancel.cancel(),
      }
    }
  });
  // Startup is awaited with its cancellation token, never discarded after it
  // may have created a VM. The qualified backend owns startup rollback.
  let result = run_started_worker(&shared, request, input, cancellation).await;
  watchdog.abort();
  let _ = watchdog.await;
  result
}

async fn run_started_worker(
  shared: &ReaderState,
  request: StartExecution,
  input: ProtectedReadWorkerRequestV1,
  cancellation: CancellationToken,
) -> Result<Vec<u8>, Error> {
  let mut execution = match shared
    .backend
    .start(&shared.program, request, cancellation.clone())
    .await
  {
    Ok(execution) => execution,
    Err(_) => {
      shared.failed.store(true, Ordering::SeqCst);
      return Err(Error::Denied);
    }
  };
  let operation = read_child(&mut *execution, input, &cancellation).await;
  let sample = tokio::select! {
    biased;
    () = tokio::time::sleep_until(shared.deadline) => None,
    sample = execution.sample_usage() => sample.ok(),
  };
  let killed = if operation.is_err() {
    execution.kill().await
  } else {
    Ok(())
  };
  let destroyed = execution.destroy().await;
  if killed.is_err() || destroyed.is_err() {
    shared.failed.store(true, Ordering::SeqCst);
    return Err(Error::Denied);
  }
  if sample.is_none_or(|sample| record_completed_usage(shared, sample).is_err()) {
    shared.failed.store(true, Ordering::SeqCst);
  }
  if cancellation.is_cancelled() {
    return Err(Error::Cancelled);
  }
  if shared.failed.load(Ordering::SeqCst) {
    return Err(Error::Denied);
  }
  operation
}

fn record_completed_usage(shared: &ReaderState, mut sample: ResourceUsage) -> Result<(), ExecutionError> {
  // The process has been destroyed. Its shared writable directories remain
  // owned by the Job, whereas no memory remains charged as a current process.
  sample.memory_current_bytes = 0;
  let mut completed = shared.usage.lock().map_err(|_| cleanup_error())?;
  if let Some(previous) = completed.as_ref() {
    let add = |left: u64, right: u64| left.checked_add(right).ok_or_else(cleanup_error);
    let network = |left: Option<u64>, right: Option<u64>| match (left, right) {
      (Some(left), Some(right)) => add(left, right).map(Some),
      _ => Ok(None),
    };
    sample.elapsed_ms = add(previous.elapsed_ms, sample.elapsed_ms)?;
    sample.cpu_time_ms = add(previous.cpu_time_ms, sample.cpu_time_ms)?;
    sample.io_read_bytes = add(previous.io_read_bytes, sample.io_read_bytes)?;
    sample.io_written_bytes = add(previous.io_written_bytes, sample.io_written_bytes)?;
    sample.memory_peak_bytes = previous.memory_peak_bytes.max(sample.memory_peak_bytes);
    sample.disk_peak_bytes = previous.disk_peak_bytes.max(sample.disk_peak_bytes);
    sample.network_received_bytes = network(previous.network_received_bytes, sample.network_received_bytes)?;
    sample.network_transmitted_bytes = network(previous.network_transmitted_bytes, sample.network_transmitted_bytes)?;
  }
  *completed = Some(sample);
  Ok(())
}

async fn read_child(
  execution: &mut dyn RunningExecution,
  input: ProtectedReadWorkerRequestV1,
  cancellation: &CancellationToken,
) -> Result<Vec<u8>, Error> {
  let io = execution.take_io().map_err(|_| Error::Denied)?;
  let mut stdin = io.stdin;
  let mut stdout = io.stdout;
  let stderr = tokio::spawn(async move {
    let mut bytes = vec![];
    io.stderr
      .take(MAX_PROTECTED_READ_REQUEST_BYTES as u64 + 1)
      .read_to_end(&mut bytes)
      .await
      .map(|_| bytes)
  });
  let operation = async {
    let body = serde_json::to_vec(&input).map_err(|_| Error::Denied)?;
    if body.len() > MAX_PROTECTED_READ_REQUEST_BYTES {
      return Err(Error::Denied);
    }
    stdin
      .write_all(&(body.len() as u32).to_be_bytes())
      .await
      .map_err(|_| Error::Denied)?;
    stdin.write_all(&body).await.map_err(|_| Error::Denied)?;
    stdin.flush().await.map_err(|_| Error::Denied)?;
    drop(stdin);
    let status = stdout.read_u8().await.map_err(|_| Error::Denied)?;
    let length = stdout.read_u32().await.map_err(|_| Error::Denied)?;
    if status != 0 || u64::from(length) > input.max_bytes {
      return Err(Error::Denied);
    }
    let mut bytes = vec![0; length as usize];
    stdout.read_exact(&mut bytes).await.map_err(|_| Error::Denied)?;
    if execution.wait().await.map_err(|_| Error::Denied)?.code != Some(0) {
      return Err(Error::Denied);
    }
    Ok(bytes)
  };
  let result =
    tokio::select! { biased; () = cancellation.cancelled() => Err(Error::Cancelled), result = operation => result };
  stderr.abort();
  let _ = stderr.await;
  result
}

fn verify_program(program: &ExecutionProgram, request: &StartExecution, sha: &str) -> Result<(), Error> {
  let ExecutionTarget::Oci { platform, .. } = request.root else {
    return Err(Error::Denied);
  };
  if platform.os != ExecutionOs::Linux {
    return Err(Error::Denied);
  }
  let platform = match platform.architecture {
    ExecutionArchitecture::Amd64 => "linux-x86_64",
    ExecutionArchitecture::Arm64 => "linux-aarch64",
  };
  let relative = program
    .executable
    .strip_prefix(&program.release_root)
    .map_err(|_| Error::Mismatch)?;
  let verified = octacity_runner::verify_standalone_program(&program.release_root, relative, sha, platform)
    .map_err(|_| Error::Mismatch)?;
  if verified != *program {
    return Err(Error::Mismatch);
  }
  Ok(())
}

fn cleanup_error() -> ExecutionError {
  ExecutionError::Backend("protected reader cleanup or accounting failed".to_owned())
}
