//! Atomic journal-and-spool mutations owned by the attempt state machine.
//!
//! A lifecycle transition and its corresponding agent event must never drift
//! apart. This private concrete owner keeps that invariant in one place and
//! publishes only cloned synchronization handles to the delivery worker.

use std::{path::Path, sync::Arc};

use octacity_protocol::{AgentLifecycleEvent, AttemptEventKind, HostSnapshot, JobLifecycleState, LeaseAssignment};
use octacity_runner::RunnerStreamItem;
use tokio::sync::{Mutex, Notify, watch};
use tokio_util::sync::CancellationToken;

use super::{JobLifecycleError, resource_snapshot, stream_kind};
use crate::{
  delivery::{append, flush, transition},
  journal::JobJournal,
  spool::{EventSpool, SpoolLimits},
};

/// Concrete event state mutated only by the lifecycle task.
pub(super) struct AttemptEventState {
  journal: JobJournal,
  pub(super) spool: Arc<Mutex<EventSpool>>,
  pub(super) work_available: Arc<Notify>,
  progress: watch::Receiver<u64>,
  pub(super) running: bool,
}

impl AttemptEventState {
  pub(super) fn create(
    attempt_root: &Path,
    lease: &LeaseAssignment,
    limits: SpoolLimits,
  ) -> Result<(Self, watch::Sender<u64>), JobLifecycleError> {
    let mut journal = JobJournal::create(&attempt_root.join("journal.jsonl"))?;
    // Establish recoverable ownership before any later setup operation can
    // fail and leave an attempt directory behind.
    journal.transition(JobLifecycleState::Preparing)?;
    let mut spool = EventSpool::create(attempt_root.to_owned(), lease.into(), limits)?;
    spool.append(AttemptEventKind::Agent {
      event: AgentLifecycleEvent::StateChanged {
        state: JobLifecycleState::Preparing,
      },
    })?;
    let (delivery_progress, progress) = watch::channel(0_u64);
    Ok((
      Self {
        journal,
        spool: Arc::new(Mutex::new(spool)),
        work_available: Arc::new(Notify::new()),
        progress,
        running: false,
      },
      delivery_progress,
    ))
  }

  pub(super) async fn transition(
    &mut self,
    state: JobLifecycleState,
    cancellation: &CancellationToken,
  ) -> Result<(), JobLifecycleError> {
    transition(
      &mut self.journal,
      &self.spool,
      &self.work_available,
      &self.progress,
      state,
      cancellation,
    )
    .await
  }

  pub(super) async fn record_runner_item(
    &mut self,
    item: RunnerStreamItem,
    snapshots: &watch::Sender<HostSnapshot>,
    cancellation: &CancellationToken,
  ) -> Result<(), JobLifecycleError> {
    if !self.running {
      self.transition(JobLifecycleState::Running, cancellation).await?;
      self.running = true;
    }
    if let RunnerStreamItem::ResourceUsage(usage) = &item {
      let usage = resource_snapshot(usage);
      snapshots.send_modify(|snapshot| {
        if let Some(active) = &mut snapshot.active_job {
          active.resource_usage = Some(usage.clone());
        }
      });
    }
    append(
      &self.spool,
      &self.work_available,
      &self.progress,
      stream_kind(item),
      cancellation,
    )
    .await
  }

  pub(super) async fn flush(&self, cancellation: &CancellationToken) -> Result<(), JobLifecycleError> {
    flush(&self.spool, &self.work_available, &self.progress, cancellation).await
  }

  pub(super) async fn last_sequence(&self) -> u64 {
    self.spool.lock().await.last_sequence()
  }
}
