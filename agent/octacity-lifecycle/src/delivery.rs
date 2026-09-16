//! Independent replay loop for one concrete durable event spool.
//!
//! The worker sends only the oldest contiguous batch and advances local state
//! only after the coordinator acknowledges that prefix. It owns no execution
//! or lease policy: retryable transport failures preserve the batch, while a
//! permanent rejection is returned to the single lifecycle owner. This keeps
//! heartbeat progress independent from potentially backpressured event output.

use std::{sync::Arc, time::Duration};

use octacity_coordinator::{CoordinatorClient, CoordinatorError, Registration};
use octacity_protocol::{AgentLifecycleEvent, AppendEventsResponse, AttemptEventKind, JobLifecycleState};
use tokio::{
  sync::{Mutex, Notify, watch},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::{JobLifecycleError, SpoolError, journal::JobJournal, spool::EventSpool};

pub(crate) struct DeliveryTask {
  pub(crate) coordinator: Arc<dyn CoordinatorClient>,
  pub(crate) registration: Registration,
  pub(crate) lease: octacity_protocol::LeaseAssignment,
  pub(crate) spool: Arc<Mutex<EventSpool>>,
  pub(crate) work_available: Arc<Notify>,
  pub(crate) progress: watch::Sender<u64>,
  pub(crate) retry_delay: Duration,
  pub(crate) stop: CancellationToken,
}

impl DeliveryTask {
  /// Starts the only task allowed to advance the coordinator acknowledgement.
  ///
  /// Retryable failures retain the exact batch and begin another bounded HTTP
  /// retry cycle. Permanent protocol, authentication, or fencing failures are
  /// returned to the lifecycle owner so they cannot turn a full spool into an
  /// indefinitely blocked job.
  pub(crate) fn spawn(self) -> JoinHandle<Result<(), JobLifecycleError>> {
    tokio::spawn(async move {
      let Self {
        coordinator,
        registration,
        lease,
        spool,
        work_available,
        progress,
        retry_delay,
        stop,
      } = self;
      loop {
        let batch = spool.lock().await.pending_batch();
        if batch.is_empty() {
          tokio::select! {
            () = stop.cancelled() => return Ok(()),
            () = work_available.notified() => continue,
          }
        }
        let result = coordinator
          .append_events(&registration, &lease, &batch, stop.clone())
          .await;
        match result {
          Ok(AppendEventsResponse {
            acknowledged_sequence, ..
          }) => {
            if let Err(error) = spool.lock().await.acknowledge(acknowledged_sequence) {
              warn!(%error, "failed to persist event acknowledgement");
              stop.cancel();
              return Err(error.into());
            }
            progress.send_replace(acknowledged_sequence);
          }
          Err(CoordinatorError::Cancelled) if stop.is_cancelled() => return Ok(()),
          Err(error) if error.is_retryable() => {
            warn!(%error, "durable event delivery failed; retaining batch for replay");
            tokio::select! {
              () = stop.cancelled() => return Ok(()),
              () = tokio::time::sleep(retry_delay) => {},
            }
          }
          Err(error) => {
            // Wake blocked append/flush callers immediately. The returned
            // error remains available through the join handle, while the token
            // prevents any later lifecycle phase from adding unsendable data.
            stop.cancel();
            return Err(error.into());
          }
        }
      }
    })
  }
}

pub(crate) async fn transition(
  journal: &mut JobJournal,
  spool: &Arc<Mutex<EventSpool>>,
  work_available: &Notify,
  progress: &watch::Receiver<u64>,
  state: JobLifecycleState,
  cancellation: &CancellationToken,
) -> Result<(), JobLifecycleError> {
  journal.transition(state)?;
  append(
    spool,
    work_available,
    progress,
    AttemptEventKind::Agent {
      event: AgentLifecycleEvent::StateChanged { state },
    },
    cancellation,
  )
  .await
}

pub(crate) async fn append(
  spool: &Arc<Mutex<EventSpool>>,
  work_available: &Notify,
  progress: &watch::Receiver<u64>,
  kind: AttemptEventKind,
  cancellation: &CancellationToken,
) -> Result<(), JobLifecycleError> {
  let mut progress = progress.clone();
  loop {
    // Do not retain the guard while waiting for the acknowledgement that needs it.
    let result = {
      let mut spool = spool.lock().await;
      spool.append(kind.clone())
    };
    match result {
      Ok(sequence) => {
        debug!(sequence, "persisted job event");
        work_available.notify_one();
        return Ok(());
      }
      Err(SpoolError::Full) => {
        tokio::select! {
          () = cancellation.cancelled() => return Err(CoordinatorError::Cancelled.into()),
          result = progress.changed() => {
            if result.is_err() {
              return Err(CoordinatorError::Cancelled.into());
            }
          },
        }
      }
      Err(error) => return Err(error.into()),
    }
  }
}

pub(crate) async fn flush(
  spool: &Arc<Mutex<EventSpool>>,
  work_available: &Notify,
  progress: &watch::Receiver<u64>,
  cancellation: &CancellationToken,
) -> Result<(), JobLifecycleError> {
  let mut progress = progress.clone();
  loop {
    if spool.lock().await.is_empty() {
      return Ok(());
    }
    work_available.notify_one();
    tokio::select! {
      () = cancellation.cancelled() => return Err(CoordinatorError::Cancelled.into()),
      result = progress.changed() => {
        if result.is_err() {
          return Err(CoordinatorError::Cancelled.into());
        }
      },
    }
  }
}
