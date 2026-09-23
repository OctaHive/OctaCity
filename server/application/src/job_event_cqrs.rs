use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_server_domain::JobId;
use octacity_server_store::{JobEventReadStore, ReadJobEvents, StoreError, StoreInputError, StoreOperation};
use serde_json::Value;

use crate::{ApplicationError, Query, QueryHandler};

/// Greatest bounded wait accepted by the management Job-event query.
pub const MAX_JOB_EVENT_WAIT: Duration = Duration::from_secs(30);

/// Typed application query for an ordered durable Job-event page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadJobEventsQuery {
  /// Job whose stream is read.
  pub job_id: JobId,
  /// Greatest sequence already observed by the caller; zero starts the stream.
  pub after_sequence: u64,
  /// Maximum page size.
  pub limit: u16,
  /// Maximum time to wait when the first durable read is empty.
  pub wait: Duration,
}

impl Query for ReadJobEventsQuery {
  type Outcome = JobEventPageProjection;
}

/// Transport-independent projection of one immutable Job event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobEventProjection {
  /// Positive sequence within one Job stream.
  pub sequence: u64,
  /// Stable provider-neutral event classification.
  pub kind: String,
  /// Source-observed Unix time in milliseconds.
  pub occurred_at_unix_ms: i64,
  /// Bounded canonical event payload.
  pub payload: Value,
}

/// Ordered Job-event page with a resumable durable cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobEventPageProjection {
  /// Contiguous events after the requested cursor.
  pub events: Vec<JobEventProjection>,
  /// Last returned sequence, or the current durable cursor when empty.
  pub cursor: u64,
}

/// Replaceable wake-up hint used between authoritative reads.
///
/// Implementations must complete no later than `timeout`. A wake-up is never
/// treated as evidence that an event committed; it only schedules a re-read.
#[async_trait]
pub trait JobEventWaiter: Send + Sync {
  /// Waits for a possible stream change or the supplied bound.
  async fn wait_for_job_events(&self, job_identity: &str, observed_cursor: u64, timeout: Duration);
}

/// Long-poll query service that never retains store state while waiting.
pub struct JobEventLongPoll<S, W> {
  store: Arc<S>,
  waiter: Arc<W>,
}

impl<S, W> JobEventLongPoll<S, W>
where
  S: JobEventReadStore,
  W: JobEventWaiter,
{
  /// Creates the service from one authoritative read port and one wake-up hint.
  pub fn new(store: Arc<S>, waiter: Arc<W>) -> Self {
    Self { store, waiter }
  }

  async fn read(&self, query: ReadJobEventsQuery) -> Result<JobEventPageProjection, ApplicationError> {
    if query.wait > MAX_JOB_EVENT_WAIT {
      return Err(
        StoreError::InvalidInput {
          operation: StoreOperation::ReadJobEvents,
          source: StoreInputError::InvalidJobEventWait,
        }
        .into(),
      );
    }
    let request = ReadJobEvents::new(query.job_id, query.after_sequence, query.limit)?;
    let initial = self.store.read_job_events(request).await?;
    if !initial.events.is_empty() || query.wait.is_zero() {
      return Ok(project(initial));
    }

    self
      .waiter
      .wait_for_job_events(&query.job_id.to_string(), initial.cursor, query.wait)
      .await;

    // Notifications are hints only. This second authoritative read is required
    // after both a wake-up and a timeout, so lost notifications and restarts do
    // not affect correctness.
    self
      .store
      .read_job_events(request)
      .await
      .map(project)
      .map_err(ApplicationError::from)
  }
}

#[async_trait]
impl<S, W> QueryHandler<ReadJobEventsQuery> for JobEventLongPoll<S, W>
where
  S: JobEventReadStore + 'static,
  W: JobEventWaiter + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ReadJobEventsQuery) -> Result<JobEventPageProjection, Self::Error> {
    self.read(query).await
  }
}

fn project(page: octacity_server_store::JobEventPage) -> JobEventPageProjection {
  JobEventPageProjection {
    events: page
      .events
      .into_iter()
      .map(|event| JobEventProjection {
        sequence: event.sequence().get(),
        kind: event.kind().as_str().to_owned(),
        occurred_at_unix_ms: event.occurred_at().unix_millis(),
        payload: event.payload().clone(),
      })
      .collect(),
    cursor: page.cursor,
  }
}
