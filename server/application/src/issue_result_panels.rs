//! Subject-bound task result projections over the existing retained execution owner.
use super::*;
use octacity_server_factory::{CandidateSubject, FactoryDigest, FactoryRunVersion};
use octacity_server_store::{FactoryRunSnapshot, FactoryRunStore};
use serde::{Deserialize, Serialize};

/// Bounded Checks selection; record is an exact retained candidate publication, never a latest-head hint.
#[derive(Clone, Debug)]
pub struct ListIssueChecksQuery {
  execution: GetIssueWorkExecutionQuery,
  record: FactoryDigest,
  after: Option<String>,
  limit: u16,
}
impl ListIssueChecksQuery {
  /// Exact original Issue-associated Work selection.
  #[must_use]
  pub fn execution(&self) -> &GetIssueWorkExecutionQuery {
    &self.execution
  }
  /// Uses ordinary Issue observation and Factory page bounds.
  pub fn try_new(
    execution: GetIssueWorkExecutionQuery,
    record: FactoryDigest,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0
      || limit > MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE
      || after
        .as_ref()
        .is_some_and(|s| s.len() > MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      execution,
      record,
      after,
      limit,
    })
  }
}
impl Query for ListIssueChecksQuery {
  type Outcome = IssueChecksPage;
}
impl ManagementAuthorizationTarget for ListIssueChecksQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
pub use octacity_server_store::IssueCheckState;
/// One safe check fact; no report body, prompts or private execution specification.
#[derive(Clone, Debug, Serialize)]
pub struct IssueCheckProjection {
  /// Stable check identity within this exact subject.
  pub id: FactoryDigest,
  /// Declared display name.
  pub name: String,
  /// Whether frozen verification policy requires this check.
  pub required: bool,
  /// Actual ordinary execution producer, independently visible.
  pub producer: Option<octacity_server_factory::EvidenceProducer>,
  /// Exact immutable observation record, suitable for the existing journal detail reader.
  pub record: Option<FactoryDigest>,
  /// Original check observation time.
  pub observed_at: Option<Timestamp>,
  /// Retained lifecycle, never an overall task verdict.
  pub state: IssueCheckState,
}
/// One exact candidate's bounded checks; a partial page cannot establish acceptance.
#[derive(Clone, Debug, Serialize)]
pub struct IssueChecksPage {
  /// Original Issue/Work/Run lineage.
  pub association: octacity_server_store::IssueWorkAssociation,
  /// Selected candidate, never substituted from current provider state.
  pub subject: CandidateSubject,
  /// Original producing candidate record.
  pub record: FactoryDigest,
  /// Separately observed execution version.
  pub run_version: FactoryRunVersion,
  /// Explicit collection coverage.
  pub coverage: IssueResultCoverage,
  /// Independently visible check facts.
  pub items: Vec<IssueCheckProjection>,
  /// Subject/visibility/version-bound continuation.
  pub next_cursor: Option<String>,
}
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + FactoryRunStore + 'static>
  IssueExecutionHandlers<S, F>
{
  async fn candidate_snapshot(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    execution: &GetIssueWorkExecutionQuery,
    record: FactoryDigest,
  ) -> Result<
    (
      octacity_server_store::IssueWorkAssociation,
      FactoryRunSnapshot,
      CandidateSubject,
    ),
    ApplicationError,
  > {
    let association = self.visible_issue_work(grant, execution).await?;
    self.visible_execution_run(context, &association).await?;
    let snapshot = self.factory.factory_run_snapshot(association.run_id).await?;
    let subject = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.digest().ok() == Some(record))
      .and_then(|r| r.discovery())
      .and_then(|d| d.published_candidate())
      .cloned()
      .ok_or_else(ApplicationError::invalid)?;
    if subject.exact() != snapshot.work.subject() {
      return Err(ApplicationError::invalid());
    }
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      subject.exact().repository_id(),
    )
    .await?;
    Ok((association, snapshot, subject))
  }
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + FactoryRunStore + 'static>
  ManagementQueryUseCase<ListIssueChecksQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueChecksQuery,
  ) -> Result<IssueChecksPage, ApplicationError> {
    let (association, snapshot, subject) = self
      .candidate_snapshot(context, grant, &query.execution, query.record)
      .await?;
    let mut items = Vec::new();
    for definition in snapshot.admitted_flow.closure().definitions() {
      let Some(policy) = definition.execution().verification() else {
        continue;
      };
      for (name, declared) in &policy.checks {
        let input = snapshot.flow.data.inputs.iter().rev().find(|input| {
          input.definition() == definition.reference()
            && input.node() == name
            && octacity_server_factory::VerificationCheckInput::new(&snapshot.admitted_flow, input)
              .is_ok_and(|check| check.plan().subject() == &subject)
        });
        let attempt = input.and_then(|input| {
          snapshot
            .flow
            .attempts
            .iter()
            .rev()
            .find(|a| input.digest().ok() == Some(a.input_digest()))
        });
        let record = attempt.and_then(|a| {
          snapshot
            .flow
            .data
            .records
            .iter()
            .find(|r| r.node_attempt_id() == a.id())
        });
        let stop = attempt.and_then(|a| snapshot.flow.data.stops.iter().find(|s| s.node_attempt_id() == a.id()));
        let output = record
          .and_then(|r| r.build_provenance())
          .and_then(|p| p.outputs().iter().find(|o| o.name == p.profile().result_output));
        let mut state = if let (Some(record), Some(input)) = (record, input) {
          verified_check_state(record, input, &snapshot.admitted_flow).unwrap_or(IssueCheckState::Indeterminate)
        } else if let Some(stop) = stop {
          match stop.reason() {
            octacity_server_factory::FlowNodeStopReason::Cancelled { .. } => IssueCheckState::Cancelled,
            octacity_server_factory::FlowNodeStopReason::ExecutionFailed => IssueCheckState::Fail,
          }
        } else if attempt.is_some() {
          IssueCheckState::Running
        } else if input.is_some() {
          IssueCheckState::Pending
        } else {
          IssueCheckState::Missing
        };
        let mut producer = output.map(|o| o.producer.clone());
        let mut record_id = record
          .map(|r| r.digest())
          .transpose()
          .map_err(|_| ApplicationError::invalid())?;
        let mut observed_at = record
          .map(|r| r.observed_at())
          .or_else(|| stop.map(|s| s.observed_at()))
          .or_else(|| attempt.map(|a| a.started_at()));
        if let Some(p) = &producer
          && !producer_visible(self.policy.as_ref(), context, p).await
        {
          state = IssueCheckState::Unavailable;
          producer = None;
          record_id = None;
          observed_at = None;
        }

        let reference = serde_json::to_vec(&definition.reference()).map_err(|_| ApplicationError::invalid())?;
        items.push(IssueCheckProjection {
          id: FactoryDigest::sha256("octacity.issue-check.v1", &[&reference, name.as_str().as_bytes()]),
          name: name.as_str().to_owned(),
          required: declared.required,
          producer,
          record: record_id,
          observed_at,
          state,
        });
      }
    }
    items.sort_by_key(|item| item.id);
    let mut pagination = PanelPagination::new(
      context,
      grant,
      serde_json::json!({"kind":"checks","association":association,"record":query.record}),
      snapshot.run.version(),
      query.after,
      query.limit,
      &items.iter().map(|i| i.id).collect::<Vec<_>>(),
    )?;
    if pagination.coverage == IssueResultCoverage::Partial {
      pagination.coverage = self
        .result_coverage(context, &association, snapshot.run.version())
        .await;
    }
    let next_cursor = pagination.apply(&mut items);
    Ok(IssueChecksPage {
      association,
      subject,
      record: query.record,
      run_version: snapshot.run.version(),
      coverage: pagination.coverage,
      items,
      next_cursor,
    })
  }
}

// An ordinary successful Build is insufficient: only the exact retained typed
// report, input/plan and independently verified output can establish this fact.
fn verified_check_state(
  record: &octacity_server_factory::FlowNodeRecord,
  input: &octacity_server_factory::FlowNodeInput,
  admitted: &octacity_server_factory::AdmittedFlow,
) -> Option<IssueCheckState> {
  let provenance = record.build_provenance()?;
  let output = provenance
    .outputs()
    .iter()
    .find(|o| o.name == provenance.profile().result_output)?;
  let data = output.data.as_ref()?;
  if data.payload() != record.payload() {
    return None;
  }
  let selected = octacity_server_factory::VerificationCheckInput::new(admitted, input).ok()?;
  let observation: octacity_server_factory::VerificationObservation =
    serde_json::from_value(data.payload().value().clone()).ok()?;
  Some(match observation.verify(&selected).ok()? {
    octacity_server_factory::DeterministicGateOutcome::Passed => IssueCheckState::Pass,
    octacity_server_factory::DeterministicGateOutcome::Failed => IssueCheckState::Fail,
    octacity_server_factory::DeterministicGateOutcome::Indeterminate => IssueCheckState::Indeterminate,
  })
}

/// Both typed panels share the canonical opaque codec and exact membership/version fence.
pub(super) struct PanelPagination {
  scope: FactoryDigest,
  version: FactoryRunVersion,
  membership: FactoryDigest,
  index: usize,
  limit: usize,
  pub(super) coverage: IssueResultCoverage,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PanelCursor {
  scope: FactoryDigest,
  version: FactoryRunVersion,
  membership: FactoryDigest,
  index: usize,
}
impl PanelPagination {
  pub(super) fn new(
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    selection: serde_json::Value,
    version: FactoryRunVersion,
    after: Option<String>,
    limit: u16,
    membership: &[FactoryDigest],
  ) -> Result<Self, ApplicationError> {
    let length = membership.len();
    let scope = crate::issue_activity::observation_scope(context, grant, selection, "issue-subject-results-v1")?;
    let membership = FactoryDigest::sha256(
      "octacity.issue-panel-membership.v1",
      &[&serde_json::to_vec(membership).map_err(|_| ApplicationError::invalid())?],
    );
    let cursor = after
      .map(|value| {
        crate::opaque_cursor::decode_canonical::<PanelCursor>(&value, MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
          .map_err(|()| ApplicationError::invalid())
      })
      .transpose()?;
    if cursor.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let stale = cursor.as_ref().is_some_and(|c| c.version != version);
    let membership_changed = cursor.as_ref().is_some_and(|c| c.membership != membership);
    let index = cursor.map_or(0, |c| c.index);
    if !stale && (index > length || membership_changed) {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      scope,
      version,
      membership,
      index,
      limit: usize::from(limit),
      coverage: if stale {
        IssueResultCoverage::Stale
      } else {
        IssueResultCoverage::Partial
      },
    })
  }
  pub(super) fn apply<T>(&self, items: &mut Vec<T>) -> Option<String> {
    if self.coverage != IssueResultCoverage::Partial {
      items.clear();
      return None;
    }
    let length = items.len();
    let end = self.index.saturating_add(self.limit).min(length);
    items.truncate(end);
    items.drain(..self.index);
    (end < length).then(|| {
      crate::opaque_cursor::encode(&PanelCursor {
        scope: self.scope,
        version: self.version,
        membership: self.membership,
        index: end,
      })
    })
  }
}
