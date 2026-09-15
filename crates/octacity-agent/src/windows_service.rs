//! Windows Service Control Manager adapter for the agent worker.
//!
//! SCM owns process startup and stop delivery, while [`crate::app`] continues
//! to own the platform-independent worker lifecycle. The adapter translates
//! SCM controls into the same cancellation token used by console signals and
//! reports every lifecycle transition back to SCM.

use std::{
  ffi::OsString,
  io,
  path::PathBuf,
  sync::{Arc, Mutex, OnceLock},
  time::Duration,
};

use tokio_util::sync::CancellationToken;
use tracing::{error, warn};
use windows_service::{
  define_windows_service,
  service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
  service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
  service_dispatcher,
};

use crate::{app, composition::Components};

const TRANSITION_WAIT_HINT: Duration = Duration::from_secs(30);
const TRANSITION_UPDATE_INTERVAL: Duration = Duration::from_secs(10);
const MAX_SERVICE_NAME_BYTES: usize = 256;
static ARGUMENTS: OnceLock<ServiceArguments> = OnceLock::new();

struct ServiceArguments {
  name: String,
  config: PathBuf,
}

define_windows_service!(ffi_service_main, service_main);

/// Registers the generated entry point with SCM and blocks until it stops.
pub(super) fn dispatch(name: String, config: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
  validate_service_name(&name)?;
  ARGUMENTS
    .set(ServiceArguments {
      name: name.clone(),
      config,
    })
    .map_err(|_| {
      io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Windows service dispatcher already initialized",
      )
    })?;
  service_dispatcher::start(name, ffi_service_main)?;
  Ok(())
}

fn service_main(_arguments: Vec<OsString>) {
  if let Err(error) = run_service() {
    error!(%error, "Windows service worker failed");
  }
}

fn run_service() -> Result<(), Box<dyn std::error::Error>> {
  let arguments = ARGUMENTS
    .get()
    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Windows service arguments are unavailable"))?;
  let shutdown = CancellationToken::new();
  let handler_shutdown = shutdown.clone();
  let (phase_sender, phase_receiver) = tokio::sync::watch::channel(ServiceState::StartPending);
  let handler_phase = phase_sender.clone();
  let status_slot = Arc::new(Mutex::new(None::<ServiceStatusHandle>));
  let handler_status = status_slot.clone();
  let event_handler = move |control| match control {
    ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
    ServiceControl::Stop | ServiceControl::Shutdown => {
      handler_shutdown.cancel();
      handler_phase.send_replace(ServiceState::StopPending);
      if let Ok(slot) = handler_status.lock()
        && let Some(handle) = *slot
      {
        let _ = handle.set_service_status(service_status(
          ServiceState::StopPending,
          ServiceControlAccept::empty(),
          ServiceExitCode::Win32(0),
          1,
          TRANSITION_WAIT_HINT,
        ));
      }
      ServiceControlHandlerResult::NoError
    }
    _ => ServiceControlHandlerResult::NotImplemented,
  };
  let status_handle = service_control_handler::register(&arguments.name, event_handler)?;
  *status_slot
    .lock()
    .map_err(|_| io::Error::other("Windows service status lock is poisoned"))? = Some(status_handle);
  let worker = if shutdown.is_cancelled() {
    status_handle.set_service_status(service_status(
      ServiceState::StopPending,
      ServiceControlAccept::empty(),
      ServiceExitCode::Win32(0),
      1,
      TRANSITION_WAIT_HINT,
    ))?;
    Ok(())
  } else {
    status_handle.set_service_status(service_status(
      ServiceState::StartPending,
      ServiceControlAccept::empty(),
      ServiceExitCode::Win32(0),
      1,
      TRANSITION_WAIT_HINT,
    ))?;
    match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
      Ok(runtime) => runtime.block_on(run_worker(
        &arguments.config,
        status_handle,
        shutdown.clone(),
        phase_sender,
        phase_receiver,
      )),
      Err(error) => Err(error.into()),
    }
  };

  let exit_code = if worker.is_ok() {
    ServiceExitCode::Win32(0)
  } else {
    ServiceExitCode::ServiceSpecific(1)
  };
  status_handle.set_service_status(service_status(
    ServiceState::Stopped,
    ServiceControlAccept::empty(),
    exit_code,
    0,
    Duration::ZERO,
  ))?;
  worker
}

async fn run_worker(
  config: &std::path::Path,
  status_handle: ServiceStatusHandle,
  shutdown: CancellationToken,
  phase_sender: tokio::sync::watch::Sender<ServiceState>,
  phase_receiver: tokio::sync::watch::Receiver<ServiceState>,
) -> Result<(), Box<dyn std::error::Error>> {
  let reporter_done = CancellationToken::new();
  let reporter = tokio::spawn(report_pending_status(
    status_handle,
    phase_receiver,
    reporter_done.clone(),
    shutdown.clone(),
  ));
  let operation = async {
    if shutdown.is_cancelled() {
      phase_sender.send_replace(ServiceState::StopPending);
      return Ok(());
    }
    let mut components = Components::load(config).await?;
    if shutdown.is_cancelled() {
      phase_sender.send_replace(ServiceState::StopPending);
      return Ok(());
    }
    status_handle.set_service_status(service_status(
      ServiceState::Running,
      ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
      ServiceExitCode::Win32(0),
      0,
      Duration::ZERO,
    ))?;
    phase_sender.send_replace(ServiceState::Running);
    app::run_loaded(&mut components, shutdown).await
  }
  .await;

  reporter_done.cancel();
  match reporter.await {
    Ok(Ok(())) => {}
    Ok(Err(error)) if operation.is_ok() => return Err(error.into()),
    Ok(Err(error)) => warn!(%error, "failed to refresh a pending Windows service status"),
    Err(error) if operation.is_ok() => return Err(io::Error::other(error).into()),
    Err(error) => warn!(%error, "Windows service status reporter failed"),
  }
  operation
}

/// Advances SCM checkpoints while startup or shutdown work is still making
/// progress. Without this heartbeat SCM can declare a healthy long transition
/// hung once its fixed wait hint expires.
async fn report_pending_status(
  status_handle: ServiceStatusHandle,
  mut phase: tokio::sync::watch::Receiver<ServiceState>,
  done: CancellationToken,
  shutdown: CancellationToken,
) -> io::Result<()> {
  let mut state = *phase.borrow_and_update();
  let mut checkpoint = 1_u32;
  loop {
    tokio::select! {
      _ = done.cancelled() => return Ok(()),
      changed = phase.changed() => {
        if changed.is_err() {
          return Ok(());
        }
        state = *phase.borrow_and_update();
        checkpoint = 1;
      }
      _ = tokio::time::sleep(TRANSITION_UPDATE_INTERVAL), if is_pending(state) => {
        checkpoint = checkpoint.saturating_add(1);
        if let Err(error) = status_handle.set_service_status(service_status(
          state,
          ServiceControlAccept::empty(),
          ServiceExitCode::Win32(0),
          checkpoint,
          TRANSITION_WAIT_HINT,
        )) {
          shutdown.cancel();
          return Err(io::Error::other(error));
        }
      }
    }
  }
}

fn is_pending(state: ServiceState) -> bool {
  matches!(state, ServiceState::StartPending | ServiceState::StopPending)
}

fn service_status(
  state: ServiceState,
  controls: ServiceControlAccept,
  exit_code: ServiceExitCode,
  checkpoint: u32,
  wait_hint: Duration,
) -> ServiceStatus {
  ServiceStatus {
    service_type: ServiceType::OWN_PROCESS,
    current_state: state,
    controls_accepted: controls,
    exit_code,
    checkpoint,
    wait_hint,
    process_id: None,
  }
}

fn validate_service_name(name: &str) -> Result<(), io::Error> {
  if name.is_empty()
    || name.len() > MAX_SERVICE_NAME_BYTES
    || !name.as_bytes()[0].is_ascii_alphanumeric()
    || !name
      .bytes()
      .skip(1)
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
  {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "Windows service name must start with an ASCII letter or digit and contain at most 256 letters, digits, dots, underscores, or hyphens",
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_invalid_service_names() {
    assert!(validate_service_name("").is_err());
    assert!(validate_service_name("agent\nservice").is_err());
    assert!(validate_service_name("agent/service").is_err());
    assert!(validate_service_name("-agent").is_err());
    assert!(validate_service_name(&"a".repeat(MAX_SERVICE_NAME_BYTES + 1)).is_err());
    assert!(validate_service_name("OctaCityAgent").is_ok());
  }

  #[test]
  fn pending_and_running_statuses_have_valid_control_fields() {
    let pending = service_status(
      ServiceState::StartPending,
      ServiceControlAccept::empty(),
      ServiceExitCode::Win32(0),
      7,
      TRANSITION_WAIT_HINT,
    );
    assert_eq!(pending.checkpoint, 7);
    assert!(pending.controls_accepted.is_empty());

    let running = service_status(
      ServiceState::Running,
      ServiceControlAccept::STOP,
      ServiceExitCode::Win32(0),
      0,
      Duration::ZERO,
    );
    assert_eq!(running.checkpoint, 0);
    assert!(running.controls_accepted.contains(ServiceControlAccept::STOP));
  }
}
