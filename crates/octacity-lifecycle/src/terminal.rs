//! Fenced terminalization after execution resources have been destroyed.
//!
//! Completion is a distinct phase because heartbeats must remain active while
//! durable events drain and until the coordinator acknowledges the final
//! document. This private phase object owns every task handle needed to stop
//! that work exactly once on either success or failure.

use std::{fs, path::PathBuf};

use octacity_coordinator::{CoordinatorError, LeaseMonitorOutcome};
use octacity_protocol::{
  COORDINATOR_PROTOCOL_VERSION, CompleteLeaseRequest, JobCompletionStatus, JobLifecycleState, LeaseAssignment,
  LeaseFence, ResourceUsageSnapshot,
};
use tokio::{sync::watch, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;
use uuid::Uuid;

use super::{
  JobLifecycle, JobLifecycleError, JobLifecycleOutcome, attempt_events::AttemptEventState, completion_id,
  delivery_failure, fatal_lease, persist_completion, stop_delivery,
};

/// Inputs transferred from execution into the terminal completion phase.
pub(super) struct TerminalAttempt<'a> {
  pub(super) lifecycle: &'a JobLifecycle,
  pub(super) lease: LeaseAssignment,
  pub(super) fence: LeaseFence,
  pub(super) attempt_root: PathBuf,
  pub(super) events: AttemptEventState,
  pub(super) delivery_stop: CancellationToken,
  pub(super) delivery: JoinHandle<Result<(), JobLifecycleError>>,
  pub(super) delivery_finished: bool,
  pub(super) monitor: JoinHandle<Result<LeaseMonitorOutcome, CoordinatorError>>,
  pub(super) lease_outcome: Option<LeaseMonitorOutcome>,
  pub(super) draining: watch::Receiver<bool>,
}

impl TerminalAttempt<'_> {
  pub(super) async fn complete(
    mut self,
    status: JobCompletionStatus,
    final_usage: Option<ResourceUsageSnapshot>,
    results: Vec<serde_json::Value>,
  ) -> Result<JobLifecycleOutcome, JobLifecycleError> {
    if self.lease_outcome.is_none() && self.monitor.is_finished() {
      self.lease_outcome = Some((&mut self.monitor).await.map_err(JobLifecycleError::Join)??);
    }
    let fatal_outcome = match self.lease_outcome.take() {
      Some(outcome) if fatal_lease(&outcome) => Some(outcome),
      outcome => {
        self.lease_outcome = outcome;
        None
      }
    };
    if let Some(outcome) = fatal_outcome {
      return self.fail(JobLifecycleError::LeaseLost(outcome)).await;
    }
    if let Some(error) = self.finished_delivery_error().await {
      return self.fail(error).await;
    }

    if let Err(error) = self
      .events
      .transition(JobLifecycleState::Completing, &self.delivery_stop)
      .await
    {
      let error = self.finished_delivery_error().await.unwrap_or(error);
      return self.fail(error).await;
    }
    if let Err(error) = self.flush_while_owned().await {
      let error = self.finished_delivery_error().await.unwrap_or(error);
      return self.fail(error).await;
    }

    let last_event_sequence = self.events.last_sequence().await;
    let completion = CompleteLeaseRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: Uuid::new_v4().simple().to_string(),
      registration_id: self.lifecycle.registration.registration_id.clone(),
      lease: (&self.lease).into(),
      completion_id: completion_id(&self.fence),
      last_event_sequence,
      status,
      final_usage,
      results,
    };
    if let Err(error) = persist_completion(&self.attempt_root, &completion) {
      return self.fail(error).await;
    }
    if let Err(error) = self.send_completion(&completion).await {
      return self.fail(error).await;
    }
    if let Err(error) = self.stop_delivery().await {
      self.monitor.abort();
      return Err(error);
    }
    if self.lease_outcome.is_none() {
      // Heartbeats are needed until the completion acknowledgement, not merely
      // until the runner exits.
      self.monitor.abort();
    }
    fs::remove_dir_all(&self.attempt_root).map_err(JobLifecycleError::StateIo)?;
    info!(job_id = %self.lease.job_id, attempt = self.lease.attempt, ?status, "completed fenced job lifecycle");
    Ok(JobLifecycleOutcome {
      status,
      drain: *self.draining.borrow(),
      last_event_sequence,
    })
  }

  async fn flush_while_owned(&mut self) -> Result<(), JobLifecycleError> {
    if self.lease_outcome.is_some() {
      return self.events.flush(&self.delivery_stop).await;
    }
    let flush = self.events.flush(&self.delivery_stop);
    tokio::pin!(flush);
    tokio::select! {
      result = &mut flush => result,
      outcome = &mut self.monitor => {
        let outcome = outcome.map_err(JobLifecycleError::Join)??;
        if fatal_lease(&outcome) {
          Err(JobLifecycleError::LeaseLost(outcome))
        } else {
          self.lease_outcome = Some(outcome);
          // The monitor future is terminal and must never be polled again.
          // Cancellation still permits already-produced events and the
          // cancelled completion status to be flushed under the current fence.
          flush.await
        }
      }
    }
  }

  async fn send_completion(&mut self, completion: &CompleteLeaseRequest) -> Result<(), JobLifecycleError> {
    let complete = self.lifecycle.coordinator.complete_lease(
      &self.lifecycle.registration,
      &self.lease,
      completion,
      self.delivery_stop.clone(),
    );
    tokio::pin!(complete);
    if self.lease_outcome.is_some() {
      return complete.await.map_err(JobLifecycleError::from);
    }
    tokio::select! {
      result = &mut complete => result.map_err(JobLifecycleError::from),
      outcome = &mut self.monitor => {
        let outcome = outcome.map_err(JobLifecycleError::Join)??;
        if fatal_lease(&outcome) {
          Err(JobLifecycleError::LeaseLost(outcome))
        } else {
          self.lease_outcome = Some(outcome);
          complete.await.map_err(JobLifecycleError::from)
        }
      }
    }
  }

  async fn stop_delivery(&mut self) -> Result<(), JobLifecycleError> {
    stop_delivery(
      &self.delivery_stop,
      &self.events.work_available,
      &mut self.delivery,
      &mut self.delivery_finished,
    )
    .await
  }

  async fn finished_delivery_error(&mut self) -> Option<JobLifecycleError> {
    if self.delivery_finished || !self.delivery.is_finished() {
      return None;
    }
    self.delivery_finished = true;
    Some(delivery_failure((&mut self.delivery).await))
  }

  async fn fail(mut self, error: JobLifecycleError) -> Result<JobLifecycleOutcome, JobLifecycleError> {
    let _ = self.stop_delivery().await;
    self.monitor.abort();
    Err(error)
  }
}
