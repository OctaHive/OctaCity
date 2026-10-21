//! Exact request journal reads compose existing observations and independently authorized Job streams.
use super::*;
pub use octacity_protocol::MAX_PUBLIC_OPERATION_PATH_BYTES;
use octacity_server_store::IssueTimelineStore;
use serde::{Deserialize, Serialize};

/// One bounded common journal collection under an exact execution request.
#[derive(Clone, Debug)]
pub struct ListIssueExecutionJournalQuery {
  execution: GetIssueExecutionQuery,
  collection: FactoryDiagnosticCollection,
  after: Option<String>,
  limit: u16,
}

const EVENT_OBSERVATION: &str = "issue-job-event-sequence-v1";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventCursor {
  scope: FactoryDigest,
  after_sequence: u64,
}
/// Exact linked Job stream; ordinary retry never changes this selection.
#[derive(Clone, Debug)]
pub struct ReadIssueExecutionJournalEventsQuery {
  job: GetIssueExecutionJobQuery,
  after: Option<EventCursor>,
  limit: u16,
}
/// Dedicated expansion of one original event through the existing authorized content reader.
#[derive(Clone, Debug)]
pub struct GetIssueExecutionJournalEventQuery {
  job: GetIssueExecutionJobQuery,
  sequence: u64,
}
impl GetIssueExecutionJournalEventQuery {
  /// Exact selected Job and retained Build linkage.
  pub fn job(&self) -> &GetIssueExecutionJobQuery {
    &self.job
  }
  /// Original requested durable sequence.
  pub const fn sequence(&self) -> u64 {
    self.sequence
  }
  /// Rejects zero; the first event has positive sequence one.
  pub fn try_new(job: GetIssueExecutionJobQuery, sequence: u64) -> Result<Self, ApplicationError> {
    if sequence == 0 {
      return Err(ApplicationError::invalid());
    }
    Ok(Self { job, sequence })
  }
}
impl Query for GetIssueExecutionJournalEventQuery {
  type Outcome = JobEventPageProjection;
}
impl ManagementAuthorizationTarget for GetIssueExecutionJournalEventQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ReadJobEventsQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::JobEvent,
      ManagementResourceKind::Job,
      self.job.job_id,
    )
  }
}
impl ReadIssueExecutionJournalEventsQuery {
  /// Exact selected Job and retained Build linkage.
  pub fn job(&self) -> &GetIssueExecutionJobQuery {
    &self.job
  }
  /// Uses the common bounded cursor codec and diagnostic page ceiling.
  pub fn try_new(job: GetIssueExecutionJobQuery, after: Option<String>, limit: u16) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    let after = after
      .map(|value| {
        crate::opaque_cursor::decode_canonical(&value, MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
          .map_err(|()| ApplicationError::invalid())
      })
      .transpose()?;
    Ok(Self { job, after, limit })
  }
}
impl Query for ReadIssueExecutionJournalEventsQuery {
  type Outcome = IssueExecutionJournalEvents;
}
impl ManagementAuthorizationTarget for ReadIssueExecutionJournalEventsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ReadJobEventsQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::JobEvent,
      ManagementResourceKind::Job,
      self.job.job_id,
    )
  }
}
impl crate::ManagementVisibilityTarget for ReadIssueExecutionJournalEventsQuery {
  type Visibility = octacity_server_store::JobEventReadVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Job];
  fn visibility_from(
    grant: &ManagementAuthorizationGrant,
  ) -> Result<Self::Visibility, crate::ManagementVisibilityError> {
    <ReadJobEventsQuery as crate::ManagementVisibilityTarget>::visibility_from(grant)
  }
}
/// Closed safe metadata; unsupported facts never become prose-derived observations.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueJournalEventFacts {
  /// Actual adapter-reported operation, with original runner/output correlation.
  Operation {
    /// Validated public command or file metadata.
    observation: octacity_protocol::OperationObservation,
    /// Original inner sequence, independent of the durable Job sequence.
    runner_sequence: u64,
    /// Original runner execution when reported.
    run_id: Option<u64>,
    /// Original runner step when reported.
    step_id: Option<u64>,
    /// Original bounded command reference; no invocation arguments.
    command_id: Option<octacity_server_factory::FactoryKey>,
  },
  /// Actual Agent lifecycle observation; not a business status.
  Lifecycle {
    /// Recorded Agent phase.
    state: octacity_protocol::JobLifecycleState,
  },
  /// A successful backend sample, independent of reserved Factory limits and model usage.
  ResourceUsage {
    /// Measured cumulative counters with their original units.
    usage: octacity_protocol::ResourceUsageSnapshot,
  },
  /// No supported public detail; the dedicated authorized log reader remains available.
  NotReported,
}
/// One stable original event reference without raw log or protocol payload.
#[derive(Clone, Debug, Serialize)]
pub struct IssueJournalEvent {
  /// Original durable sequence under the selected Job.
  pub sequence: u64,
  /// Original source observation time, not this HTTP read's time.
  pub occurred_at_unix_ms: i64,
  /// Allowlisted metadata or explicit unsupported detail.
  pub facts: IssueJournalEventFacts,
}
/// Unavailable retained sequence interval; no content is reconstructed from a gap.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct IssueJournalGap {
  /// First absent durable sequence, inclusive.
  pub from_sequence: u64,
  /// Last absent durable sequence, inclusive.
  pub through_sequence: u64,
}
/// Independent live Job observation; cursor remains resumable on an empty tail.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExecutionJournalEvents {
  /// Exact progress, worker/profile and independently authorized log targets.
  pub job: IssueExecutionJobProjection,
  /// Stable live-stream observation contract; never an acceptance verdict.
  pub observation: &'static str,
  /// Explicit collection coverage, independent of Run completion.
  pub coverage: IssueResultCoverage,
  /// Events advanced after the Job snapshot; refresh metadata without rewinding the stream.
  pub job_stale: bool,
  /// Canonical continuation after the last consumed durable sequence.
  pub cursor: String,
  /// Bounded safe rows, including unsupported events without invented facts.
  pub events: Vec<IssueJournalEvent>,
  /// Explicit unavailable ranges encountered within this bounded traversal.
  pub gaps: Vec<IssueJournalGap>,
}
/// Composes existing exact-Job and event services; owns no event persistence or lifecycle.
pub struct IssueExecutionJournalHandlers<S, F, B, E> {
  jobs: Arc<IssueExecutionJobHandlers<S, F, B>>,
  events: Arc<E>,
}
impl<S, F, B, E> IssueExecutionJournalHandlers<S, F, B, E> {
  /// Uses the already installed event reader, keeping manual Issue composition optional.
  pub fn new(executions: Arc<IssueExecutionHandlers<S, F>>, jobs: Arc<B>, events: Arc<E>) -> Self {
    Self {
      jobs: Arc::new(IssueExecutionJobHandlers::new(executions, jobs)),
      events,
    }
  }
}
impl<
  S: IssueExecutionStore + IssueStore,
  F: FactoryDiscoveryStore + FactoryRunStore + 'static,
  B: BuildQueryStore + 'static,
  E,
> IssueExecutionJournalHandlers<S, F, B, E>
{
  async fn visible_job(
    &self,
    context: &ManagementRequestContext,
    query: GetIssueExecutionJobQuery,
    visibility: &octacity_server_store::JobEventReadVisibility,
  ) -> Result<IssueExecutionJobProjection, ApplicationError> {
    if !visibility.allows(&query.job_id) {
      return Err(ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Job,
      }));
    }
    AuthorizedQueryHandler::new(self.jobs.executions.policy.clone(), self.jobs.clone())
      .handle_query(context, query)
      .await
      .map_err(execution_read_error)
  }
}
#[async_trait::async_trait]
impl<
  S: IssueExecutionStore + IssueStore,
  F: FactoryDiscoveryStore + FactoryRunStore + 'static,
  B: BuildQueryStore + 'static,
  E: ManagementQueryUseCase<ReadJobEventsQuery, Error = ApplicationError> + 'static,
> ManagementQueryUseCase<GetIssueExecutionJournalEventQuery> for IssueExecutionJournalHandlers<S, F, B, E>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionJournalEventQuery,
  ) -> Result<JobEventPageProjection, ApplicationError> {
    let visibility = grant
      .visibility_for::<ReadJobEventsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    self.visible_job(context, query.job.clone(), &visibility).await?;
    let page = self
      .events
      .execute_management_query(
        context,
        grant,
        ReadJobEventsQuery {
          job_id: query.job.job_id,
          after_sequence: query.sequence - 1,
          limit: 1,
          wait: std::time::Duration::ZERO,
        },
      )
      .await?;
    if page.events.first().map(|event| event.sequence) != Some(query.sequence) {
      return Err(ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Job,
      }));
    }
    Ok(page)
  }
}
#[async_trait::async_trait]
impl<
  S: IssueExecutionStore + IssueStore,
  F: FactoryDiscoveryStore + FactoryRunStore + 'static,
  B: BuildQueryStore + 'static,
  E: ManagementQueryUseCase<ReadJobEventsQuery, Error = ApplicationError> + 'static,
> ManagementQueryUseCase<ReadIssueExecutionJournalEventsQuery> for IssueExecutionJournalHandlers<S, F, B, E>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ReadIssueExecutionJournalEventsQuery,
  ) -> Result<IssueExecutionJournalEvents, ApplicationError> {
    let visibility = grant
      .visibility_for::<ReadIssueExecutionJournalEventsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let job = self.visible_job(context, query.job.clone(), &visibility).await?;
    let scope = crate::issue_activity::observation_scope_for(
      context,
      visibility.view(),
      serde_json::json!({
        "association":job.association,
        "build_execution":query.job.build_execution,"job":query.job.job_id
      }),
      EVENT_OBSERVATION,
    );
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let after_sequence = query.after.map_or(0, |c| c.after_sequence);
    let mut result = IssueExecutionJournalEvents {
      job,
      observation: EVENT_OBSERVATION,
      coverage: IssueResultCoverage::Unavailable,
      job_stale: false,
      cursor: crate::opaque_cursor::encode(&EventCursor { scope, after_sequence }),
      events: Vec::new(),
      gaps: Vec::new(),
    };
    let page = match self
      .events
      .execute_management_query(
        context,
        grant,
        ReadJobEventsQuery {
          job_id: query.job.job_id,
          after_sequence,
          limit: query.limit,
          wait: std::time::Duration::ZERO,
        },
      )
      .await
    {
      Ok(page) => page,
      Err(error) if error.classification() == ApplicationFailure::Unavailable => return Ok(result),
      Err(error) => return Err(error),
    };
    // The live stream may already have advanced past the earlier Job snapshot.
    // Its authoritative retained cursor also validates a previously issued tail.
    if after_sequence > result.job.log.event_cursor.max(page.cursor) {
      return Err(ApplicationError::invalid());
    }
    let mut through = after_sequence;
    let mut gaps = Vec::new();
    for event in &page.events {
      if event.sequence > through.saturating_add(1) {
        gaps.push(IssueJournalGap {
          from_sequence: through + 1,
          through_sequence: event.sequence - 1,
        });
      }
      through = event.sequence;
    }
    // The authoritative Job retains its acknowledgement even when every log/event
    // row has expired. An empty store page must not silently rewind or loop forever.
    if page.events.is_empty() {
      let retained_through = result.job.log.event_cursor.max(page.cursor).max(after_sequence);
      if retained_through > through {
        gaps.push(IssueJournalGap {
          from_sequence: through + 1,
          through_sequence: retained_through,
        });
      }
      through = retained_through;
    }
    result.coverage = IssueResultCoverage::Partial;
    result.job_stale = through > result.job.log.event_cursor;
    result.cursor = crate::opaque_cursor::encode(&EventCursor {
      scope,
      after_sequence: through,
    });
    result.events = page.events.into_iter().map(project_event).collect();
    result.gaps = gaps;
    Ok(result)
  }
}
fn project_event(event: JobEventProjection) -> IssueJournalEvent {
  use octacity_protocol::{AgentLifecycleEvent, AttemptEventKind};
  let facts = match serde_json::from_value::<AttemptEventKind>(event.payload) {
    Ok(AttemptEventKind::Agent {
      event: AgentLifecycleEvent::StateChanged { state },
    }) => IssueJournalEventFacts::Lifecycle { state },
    Ok(AttemptEventKind::Agent {
      event: AgentLifecycleEvent::ResourceUsage { usage },
    }) => IssueJournalEventFacts::ResourceUsage { usage },
    Ok(AttemptEventKind::Runner { event }) => match event.public_operation_observation() {
      Some(observation) => IssueJournalEventFacts::Operation {
        observation,
        runner_sequence: event.sequence,
        run_id: event.data.get("run_id").and_then(serde_json::Value::as_u64),
        step_id: event.data.get("step_id").and_then(serde_json::Value::as_u64),
        command_id: event
          .data
          .get("command_id")
          .and_then(serde_json::Value::as_str)
          .and_then(|s| s.parse().ok()),
      },
      None => IssueJournalEventFacts::NotReported,
    },
    _ => IssueJournalEventFacts::NotReported,
  };
  IssueJournalEvent {
    sequence: event.sequence,
    occurred_at_unix_ms: event.occurred_at_unix_ms,
    facts,
  }
}
impl ListIssueExecutionJournalQuery {
  /// Original request selected by the caller.
  pub fn execution(&self) -> &GetIssueExecutionQuery {
    &self.execution
  }
  /// Existing common journal collection.
  pub const fn collection(&self) -> FactoryDiagnosticCollection {
    self.collection
  }
  /// Reuses common diagnostic bounds and the original collection continuation.
  pub fn try_new(
    execution: GetIssueExecutionQuery,
    collection: FactoryDiagnosticCollection,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      execution,
      collection,
      after,
      limit,
    })
  }
}
impl crate::ManagementVisibilityTarget for ListIssueExecutionJournalQuery {
  type Visibility = octacity_server_store::IssueListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Issue];
  fn visibility_from(
    grant: &ManagementAuthorizationGrant,
  ) -> Result<Self::Visibility, crate::ManagementVisibilityError> {
    <ListProjectIssuesQuery as crate::ManagementVisibilityTarget>::visibility_from(grant)
  }
}
impl crate::ManagementVisibilityTarget for GetIssueExecutionJournalEventQuery {
  type Visibility = octacity_server_store::JobEventReadVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Job];
  fn visibility_from(
    grant: &ManagementAuthorizationGrant,
  ) -> Result<Self::Visibility, crate::ManagementVisibilityError> {
    <ReadJobEventsQuery as crate::ManagementVisibilityTarget>::visibility_from(grant)
  }
}
impl Query for ListIssueExecutionJournalQuery {
  type Outcome = IssueExecutionJournalPage;
}
impl ManagementAuthorizationTarget for ListIssueExecutionJournalQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Separate request/Run state and version-fenced hierarchy, without aggregate input or log bodies.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExecutionJournalPage {
  /// Original reviewed request, requirement relationship and independently visible Run.
  pub execution: IssueExecutionProjection,
  /// Common paged records; absent while the original request has no Work yet.
  pub records: Option<IssueResultPage>,
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore + IssueTimelineStore, F: FactoryDiscoveryStore + FactoryRunStore + 'static>
  ManagementQueryUseCase<ListIssueExecutionJournalQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueExecutionJournalQuery,
  ) -> Result<IssueExecutionJournalPage, ApplicationError> {
    let execution = self.execution_projection(context, grant, query.execution).await?;
    let records = match &execution.request.association {
      Some(association) => Some(
        <Self as ManagementQueryUseCase<ListIssueResultsQuery>>::execute_management_query(
          self,
          context,
          grant,
          ListIssueResultsQuery::try_new(
            GetIssueWorkExecutionQuery {
              project_id: association.project_id,
              issue_id: association.issue_id,
              work_id: association.work_id,
            },
            query.collection,
            query.after,
            query.limit,
          )?,
        )
        .await?,
      ),
      None => {
        if query.after.is_some() {
          return Err(ApplicationError::invalid());
        }
        None
      }
    };
    Ok(IssueExecutionJournalPage { execution, records })
  }
}
