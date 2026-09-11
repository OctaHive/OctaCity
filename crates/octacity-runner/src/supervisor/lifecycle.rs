//! Drives the bounded runner protocol after a backend process has started.

use std::{future::pending, time::Duration};

use octa_runner_protocol::RunnerCommand;
use octacity_execution::{ExecutionError, ExecutionReader, ExecutionWriter, ResourceUsage, RunningExecution};
use tokio::{
  io::BufReader,
  sync::mpsc,
  time::{Instant, MissedTickBehavior, interval, sleep_until, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::{
  RunnerCompletion, RunnerJobRequest, RunnerStreamItem, RunnerSupervisionError, RunnerSupervisionPolicy,
  TerminationReason, protocol,
  validation::{build_run_request, validate_event, validate_exit, validate_hello},
};
use crate::protocol::{RunnerMessage, write_command};

pub(super) async fn drive(
  execution: &mut dyn RunningExecution,
  protocol_io: (ExecutionWriter, ExecutionReader),
  job: &RunnerJobRequest,
  operation_deadline: Instant,
  cancellation: CancellationToken,
  events: &mpsc::Sender<RunnerStreamItem>,
  policy: &RunnerSupervisionPolicy,
) -> Result<RunnerCompletion, RunnerSupervisionError> {
  let (mut stdin, stdout) = protocol_io;
  let deadline = operation_deadline;
  let run = build_run_request(&job.spec, execution.paths());
  let mut stdout = BufReader::new(stdout);
  let hello_deadline = operation_deadline.min(Instant::now() + policy.hello_timeout);
  let hello = tokio::select! {
    () = cancellation.cancelled() => return Err(ExecutionError::Cancelled.into()),
    result = timeout_at(hello_deadline, crate::protocol::read_message(&mut stdout)) => {
      result
        .map_err(|_| protocol("hello timed out"))??
        .ok_or_else(|| protocol("stdout closed before hello"))?
    }
  };
  validate_hello(&hello, &job.octa)?;
  debug!(request_id = %job.request_id, "validated octa-runner hello");

  let start_command = RunnerCommand::Start {
    protocol_version: job.octa.runner_protocol,
    request_id: job.request_id.clone(),
    request: Box::new(run),
  };
  tokio::select! {
    () = cancellation.cancelled() => return Err(ExecutionError::Cancelled.into()),
    result = timeout_at(deadline, write_command(&mut stdin, &start_command)) => {
      result.map_err(|_| RunnerSupervisionError::StartupTimeout)??
    },
  }

  let operation_deadline = sleep_until(operation_deadline);
  tokio::pin!(operation_deadline);
  let mut stop_deadline = None;
  let mut stop_reason = None;
  let mut accepted = false;
  let mut last_event_sequence = None;
  let mut accounting_failures = 0;
  let mut sampler = interval(policy.resource_sample_interval);
  sampler.set_missed_tick_behavior(MissedTickBehavior::Skip);
  sampler.tick().await;
  info!(request_id = %job.request_id, "started octa-runner request");

  // Once stopping begins, resource sampling stops but the runner may still
  // return its structured terminal result during the grace period.
  loop {
    tokio::select! {
      message = crate::protocol::read_message(&mut stdout) => {
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
            match deliver(
              events,
              RunnerStreamItem::Event(event),
              &cancellation,
              stop_deadline.unwrap_or(deadline),
            ).await? {
              DeliveryOutcome::Delivered => {}
              DeliveryOutcome::Cancelled => {
                begin_stop(
                  &mut stop_reason,
                  &mut stop_deadline,
                  TerminationReason::Cancelled,
                  job.cancellation_grace,
                  &mut stdin,
                  &job.request_id,
                ).await;
              }
              DeliveryOutcome::TimedOut if stop_reason.is_some() => {
                execution.kill().await?;
                return Err(RunnerSupervisionError::CancellationTimeout);
              }
              DeliveryOutcome::TimedOut => {
                begin_stop(
                  &mut stop_reason,
                  &mut stop_deadline,
                  TerminationReason::TimedOut,
                  job.cancellation_grace,
                  &mut stdin,
                  &job.request_id,
                ).await;
              }
            }
          },
          RunnerMessage::Finished { request_id, status, results } if request_id == job.request_id && accepted => {
            let final_usage = final_usage(
              execution,
              policy.resource_sample_timeout,
              policy.max_accounting_failures,
            ).await?;
            // The runner keeps a cancellation reader on stdin until its input
            // reaches EOF. Close our control pipe before waiting for the
            // process, otherwise a successful runner can remain alive after
            // emitting its terminal message.
            drop(stdin);
            let exit = timeout(job.cancellation_grace, async {
              execution.close_input().await?;
              execution.wait().await
            })
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
        match timeout(policy.resource_sample_timeout, execution.sample_usage()).await {
          Ok(Ok(usage)) => {
            accounting_failures = 0;
            handle_delivery(
              deliver(events, RunnerStreamItem::ResourceUsage(usage), &cancellation, deadline).await?,
              &mut stop_reason,
              &mut stop_deadline,
              job,
              &mut stdin,
            ).await?;
          },
          Ok(Err(error)) => {
            accounting_failures += 1;
            warn!(request_id = %job.request_id, consecutive_failures = accounting_failures, error = %error, "resource sample failed");
            handle_delivery(deliver(
              events,
              RunnerStreamItem::AccountingUnavailable {
                consecutive_failures: accounting_failures,
              },
              &cancellation,
              deadline,
            ).await?, &mut stop_reason, &mut stop_deadline, job, &mut stdin).await?;
          },
          Err(_) => {
            accounting_failures += 1;
            warn!(request_id = %job.request_id, consecutive_failures = accounting_failures, "resource sample timed out");
            handle_delivery(deliver(
              events,
              RunnerStreamItem::AccountingUnavailable {
                consecutive_failures: accounting_failures,
              },
              &cancellation,
              deadline,
            ).await?, &mut stop_reason, &mut stop_deadline, job, &mut stdin).await?;
          },
        }
        if accounting_failures >= policy.max_accounting_failures {
          return Err(RunnerSupervisionError::AccountingUnavailable(accounting_failures));
        }
      },
    }
  }
}

async fn final_usage(
  execution: &mut dyn RunningExecution,
  timeout_duration: Duration,
  max_accounting_failures: usize,
) -> Result<ResourceUsage, RunnerSupervisionError> {
  timeout(timeout_duration, execution.sample_usage())
    .await
    .map_err(|_| RunnerSupervisionError::AccountingUnavailable(max_accounting_failures))?
    .map_err(Into::into)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeliveryOutcome {
  Delivered,
  Cancelled,
  TimedOut,
}

pub(super) async fn deliver(
  events: &mpsc::Sender<RunnerStreamItem>,
  item: RunnerStreamItem,
  cancellation: &CancellationToken,
  deadline: Instant,
) -> Result<DeliveryOutcome, RunnerSupervisionError> {
  tokio::select! {
    result = events.send(item) => {
      result.map_err(|_| RunnerSupervisionError::EventConsumerStopped)?;
      Ok(DeliveryOutcome::Delivered)
    },
    () = cancellation.cancelled() => Ok(DeliveryOutcome::Cancelled),
    () = sleep_until(deadline) => Ok(DeliveryOutcome::TimedOut),
  }
}

pub(super) async fn handle_delivery(
  outcome: DeliveryOutcome,
  stop_reason: &mut Option<TerminationReason>,
  stop_deadline: &mut Option<Instant>,
  job: &RunnerJobRequest,
  stdin: &mut ExecutionWriter,
) -> Result<(), RunnerSupervisionError> {
  match outcome {
    DeliveryOutcome::Delivered => Ok(()),
    DeliveryOutcome::Cancelled => {
      begin_stop(
        stop_reason,
        stop_deadline,
        TerminationReason::Cancelled,
        job.cancellation_grace,
        stdin,
        &job.request_id,
      )
      .await;
      Ok(())
    }
    DeliveryOutcome::TimedOut => {
      begin_stop(
        stop_reason,
        stop_deadline,
        TerminationReason::TimedOut,
        job.cancellation_grace,
        stdin,
        &job.request_id,
      )
      .await;
      Ok(())
    }
  }
}

pub(super) async fn begin_stop(
  stop_reason: &mut Option<TerminationReason>,
  stop_deadline: &mut Option<Instant>,
  reason: TerminationReason,
  grace: Duration,
  stdin: &mut ExecutionWriter,
  request_id: &str,
) {
  if stop_reason.is_some() {
    return;
  }
  *stop_reason = Some(reason);
  let deadline = Instant::now() + grace;
  *stop_deadline = Some(deadline);
  let result = timeout_at(
    deadline,
    write_command(
      stdin,
      &RunnerCommand::Cancel {
        request_id: request_id.to_owned(),
      },
    ),
  )
  .await;
  if !matches!(result, Ok(Ok(()))) {
    debug!(
      request_id,
      "could not deliver runner cancellation before forced-stop deadline"
    );
  }
}

pub(super) async fn wait_for_deadline(deadline: Option<Instant>) {
  match deadline {
    Some(deadline) => sleep_until(deadline).await,
    None => pending().await,
  }
}
