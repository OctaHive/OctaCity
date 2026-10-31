use std::num::NonZeroU16;

use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryKey, FactoryRunId, FactoryRunState,
  FactoryRunVersion, WorkEnvelopeId,
};

use crate::{
  FactoryConfigurationListVisibility, FactoryRunListVisibility, StoreError, StoreInputError, StoreOperation,
};

/// Maximum current Factory Configurations returned by one Project page.
pub const MAX_FACTORY_CONFIGURATION_PAGE_SIZE: u16 = 200;
/// Maximum current Factory Runs returned by one discovery page.
pub const MAX_FACTORY_RUN_PAGE_SIZE: u16 = 200;

/// Composite position in deterministic newest-first Factory Configuration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactoryConfigurationPagePosition {
  /// Authoritative identity creation time.
  pub created_at: Timestamp,
  /// Stable identity used to break equal-time ties.
  pub configuration_id: FactoryConfigurationId,
}

/// Bounded Project-scoped current Factory Configuration discovery request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectFactoryConfigurations {
  project_id: ProjectId,
  after: Option<FactoryConfigurationPagePosition>,
  limit: NonZeroU16,
  visibility: FactoryConfigurationListVisibility,
}

impl ListProjectFactoryConfigurations {
  /// Constructs one validated current Factory Configuration page request.
  pub fn new(
    project_id: ProjectId,
    after: Option<FactoryConfigurationPagePosition>,
    limit: u16,
    visibility: FactoryConfigurationListVisibility,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_FACTORY_CONFIGURATION_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListProjectFactoryConfigurations,
        StoreInputError::InvalidFactoryConfigurationPageSize,
      ))?;
    Ok(Self {
      project_id,
      after,
      limit,
      visibility,
    })
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the exclusive continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<FactoryConfigurationPagePosition> {
    self.after
  }

  /// Returns the validated positive page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns authorization-derived visibility.
  #[must_use]
  pub const fn visibility(&self) -> &FactoryConfigurationListVisibility {
    &self.visibility
  }
}

/// Safe current immutable Factory Configuration facts used by discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentFactoryConfigurationSummary {
  /// Stable configuration identity.
  pub id: FactoryConfigurationId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Current immutable version.
  pub version: FactoryConfigurationVersion,
  /// Whether new Work may select the current version.
  pub enabled: bool,
  /// Authoritative identity creation time.
  pub created_at: Timestamp,
  /// Authoritative current-version publication time.
  pub published_at: Timestamp,
}

impl CurrentFactoryConfigurationSummary {
  /// Returns this summary's deterministic newest-first position.
  #[must_use]
  pub const fn page_position(&self) -> FactoryConfigurationPagePosition {
    FactoryConfigurationPagePosition {
      created_at: self.created_at,
      configuration_id: self.id,
    }
  }
}

/// One deterministic page of visible current Factory Configurations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentFactoryConfigurationPage {
  /// Matching visible count before cursor pagination.
  pub total: u64,
  /// Current summaries in deterministic newest-first order.
  pub items: Vec<CurrentFactoryConfigurationSummary>,
  /// Exclusive continuation position when another item exists.
  pub next_cursor: Option<FactoryConfigurationPagePosition>,
}

/// Exact optional predicates applied after visibility and before ordering.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryRunFilter {
  /// Exact Factory Configuration identity, when selected.
  pub configuration_id: Option<FactoryConfigurationId>,
  /// Exact Work Source adapter kind, when selected.
  pub source: Option<FactoryKey>,
  /// Exact current lifecycle state, when selected.
  pub state: Option<FactoryRunState>,
  /// Exact authoritative disposition for the current candidate; signals do not match.
  pub decision: Option<octacity_server_factory::DecisionOutcome>,
  /// Exact current-candidate delivery state.
  pub delivery_state: Option<octacity_server_factory::DeliveryState>,
  /// Inclusive lower admission-time bound.
  pub admitted_from: Option<Timestamp>,
  /// Exclusive upper admission-time bound.
  pub admitted_before: Option<Timestamp>,
}

/// Composite position in deterministic newest-first Factory Run order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactoryRunPagePosition {
  /// Authoritative Work admission time.
  pub admitted_at: Timestamp,
  /// Stable identity used to break equal-time ties.
  pub run_id: FactoryRunId,
}

/// Bounded Project-scoped or global Factory Run discovery request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryRuns {
  project_id: Option<ProjectId>,
  filter: FactoryRunFilter,
  after: Option<FactoryRunPagePosition>,
  limit: NonZeroU16,
  visibility: FactoryRunListVisibility,
}

impl ListFactoryRuns {
  /// Constructs one validated Factory Run discovery request.
  pub fn new(
    project_id: Option<ProjectId>,
    filter: FactoryRunFilter,
    after: Option<FactoryRunPagePosition>,
    limit: u16,
    visibility: FactoryRunListVisibility,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_FACTORY_RUN_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListFactoryRuns,
        StoreInputError::InvalidFactoryRunPageSize,
      ))?;
    if filter
      .admitted_from
      .zip(filter.admitted_before)
      .is_some_and(|(from, before)| from >= before)
    {
      return Err(StoreError::invalid(
        StoreOperation::ListFactoryRuns,
        StoreInputError::InvalidFactoryRunFilter,
      ));
    }
    Ok(Self {
      project_id,
      filter,
      after,
      limit,
      visibility,
    })
  }

  /// Returns an optional Project scope; absence denotes the global collection.
  #[must_use]
  pub const fn project_id(&self) -> Option<ProjectId> {
    self.project_id
  }

  /// Returns exact predicates applied to visible rows.
  #[must_use]
  pub const fn filter(&self) -> &FactoryRunFilter {
    &self.filter
  }

  /// Returns the exclusive continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<FactoryRunPagePosition> {
    self.after
  }

  /// Returns the validated positive page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns authorization-derived visibility.
  #[must_use]
  pub const fn visibility(&self) -> &FactoryRunListVisibility {
    &self.visibility
  }
}

/// Maximum active node summaries embedded in one collection item.
pub const MAX_FACTORY_RUN_SUMMARY_NODES: usize = 32;
/// Safe current facts, derived only from the common owner-verified journal.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunSummaryDiagnostics {
  /// Exact admitted repository base, or no VCS base for Project/input Work.
  pub base_revision: Option<octacity_server_domain::ImmutableRevision>,
  /// Latest exact candidate; no subject is inferred from prose or logical node names.
  pub candidate: Option<octacity_server_factory::CandidateSubject>,
  /// Bounded active attempts in stable Flow/cycle/node/attempt order.
  pub current_nodes: Vec<FactoryRunNodeSummary>,
  /// Total active attempts before truncating their summaries.
  pub active_node_count: u64,
  /// Current authoritative aggregate consumption and its admission-time ceiling.
  pub budget: FactoryRunBudgetSummary,
  /// Latest disposition for this exact current candidate, independently of signals.
  pub latest_decision: Option<FactoryRunDecisionSummary>,
  /// Latest delivery for this exact current candidate.
  pub delivery: Option<FactoryRunDeliverySummary>,
  /// Latest escalation identity without private reason text.
  pub escalation: Option<octacity_server_factory::EscalationId>,
}
/// One active attempt without input, execution commands or context payloads.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunNodeSummary {
  /// Exact retained attempt identity.
  pub id: octacity_server_factory::NodeAttemptId,
  /// Owning root or nested Flow instance.
  pub flow_run_id: octacity_server_factory::FlowRunId,
  /// Exact workflow cycle.
  pub cycle_id: octacity_server_factory::WorkflowCycleId,
  /// Caller-configured logical node key.
  pub node: FactoryKey,
  /// Positive attempt number.
  pub attempt: u64,
}
/// Authoritative budget counters, including zero consumable allowances.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunBudgetSummary {
  /// Consumption from the current immutable budget record.
  pub usage: octacity_server_factory::BudgetUsage,
  /// Limits frozen at admission.
  pub limit: octacity_server_factory::BudgetLimit,
}
/// Safe authoritative disposition without reasons, assessments or findings.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunDecisionSummary {
  /// Immutable Decision identity.
  pub id: octacity_server_factory::DecisionId,
  /// Exact subject to distinguish rework candidates.
  pub subject: octacity_server_factory::CandidateSubject,
  /// Authoritative disposition.
  pub outcome: octacity_server_factory::DecisionOutcome,
}
/// Safe delivery state without connector payloads, credentials or private URLs.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunDeliverySummary {
  /// Immutable delivery attempt identity.
  pub id: octacity_server_factory::DeliveryAttemptId,
  /// Correlated authoritative Decision.
  pub decision_id: octacity_server_factory::DecisionId,
  /// Exact candidate to which delivery applies.
  pub subject: octacity_server_factory::CandidateSubject,
  /// Closed delivery state.
  pub state: octacity_server_factory::DeliveryState,
}
impl FactoryRunSummaryDiagnostics {
  /// Projects a validated snapshot without publishing arbitrary result payload fields.
  pub fn from_snapshot(snapshot: &crate::FactoryRunSnapshot) -> Result<Self, StoreError> {
    let mut records = snapshot.flow.data.records.iter().collect::<Vec<_>>();
    records.sort_by_key(|record| (record.observed_at(), record.node_attempt_id()));
    let context = snapshot
      .flow
      .attempts
      .iter()
      .filter_map(|attempt| {
        snapshot
          .flow
          .data
          .inputs
          .iter()
          .find(|input| input.digest().ok() == Some(attempt.input_digest()))
          .and_then(|input| input.context())
          .and_then(|context| context.subject().candidate())
          .map(|candidate| (attempt.started_at(), attempt.id(), candidate))
      })
      .max_by_key(|(at, id, _)| (*at, *id))
      .map(|(_, _, candidate)| candidate);
    let facts = records.iter().rev().filter_map(|record| record.discovery());
    let selected = facts
      .clone()
      .find(|facts| facts.published_candidate().is_some())
      .or_else(|| facts.into_iter().find(|facts| facts.candidate_subject().is_some()));
    let candidate = selected
      .and_then(|facts| facts.select_current_candidate(context))
      .or(context)
      .cloned();
    let matching = records
      .iter()
      .rev()
      .filter_map(|record| record.discovery())
      .filter(|facts| facts.candidate_subject() == candidate.as_ref())
      .collect::<Vec<_>>();
    let latest_decision = matching.iter().find_map(|facts| {
      facts.decision_summary().map(|(id, outcome)| FactoryRunDecisionSummary {
        id,
        outcome,
        subject: facts.candidate_subject().expect("Decision carries a candidate").clone(),
      })
    });
    let delivery = matching.iter().find_map(|facts| {
      facts
        .delivery_summary()
        .map(|(id, decision_id, state)| FactoryRunDeliverySummary {
          id,
          decision_id,
          state,
          subject: facts.candidate_subject().expect("delivery carries a candidate").clone(),
        })
    });
    let escalation = records
      .iter()
      .rev()
      .find_map(|record| record.discovery().and_then(|facts| facts.escalation_id()));
    let mut active = snapshot
      .flow
      .attempts
      .iter()
      .filter(|attempt| {
        !snapshot
          .flow
          .completions
          .iter()
          .any(|completion| completion.node_attempt_id() == attempt.id())
          && !snapshot
            .flow
            .data
            .stops
            .iter()
            .any(|stop| stop.node_attempt_id() == attempt.id())
      })
      .map(|attempt| FactoryRunNodeSummary {
        id: attempt.id(),
        flow_run_id: attempt.flow_run_id(),
        cycle_id: attempt.workflow_cycle_id(),
        node: attempt.node_key().clone(),
        attempt: attempt.number().get(),
      })
      .collect::<Vec<_>>();
    active.sort_by(|left, right| {
      (left.flow_run_id, left.cycle_id, &left.node, left.attempt).cmp(&(
        right.flow_run_id,
        right.cycle_id,
        &right.node,
        right.attempt,
      ))
    });
    let active_node_count = active.len() as u64;
    active.truncate(MAX_FACTORY_RUN_SUMMARY_NODES);
    let usage = snapshot
      .budgets
      .iter()
      .find(|row| row.id == snapshot.current.budget_id)
      .ok_or(StoreError::Unavailable)?
      .usage;
    Ok(Self {
      base_revision: snapshot
        .work
        .subject()
        .repository()
        .ok()
        .map(|subject| subject.base_revision().clone()),
      candidate,
      current_nodes: active,
      active_node_count,
      budget: FactoryRunBudgetSummary {
        usage,
        limit: snapshot.admitted_flow.limits().budget(),
      },
      latest_decision,
      delivery,
      escalation,
    })
  }
}

/// Safe current Factory Run and admitted Work facts used by discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunSummary {
  /// Optional independently authorized task navigation; absence is not proof of no association.
  pub issue_association: Option<crate::IssueWorkAssociation>,
  /// Bounded safe current facts derived from the owner journal.
  pub diagnostics: FactoryRunSummaryDiagnostics,
  /// Stable Factory Run identity.
  pub id: FactoryRunId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Immutable admitted Work identity.
  pub work_id: WorkEnvelopeId,
  /// Exact Factory Configuration identity.
  pub configuration_id: FactoryConfigurationId,
  /// Exact immutable Factory Configuration version.
  pub configuration_version: FactoryConfigurationVersion,
  /// Provider-neutral Work Source kind.
  pub source: FactoryKey,
  /// Source-scoped external Work identity.
  pub external_identity: ExternalWorkIdentity,
  /// Current durable lifecycle state.
  pub state: FactoryRunState,
  /// Current optimistic aggregate version.
  pub version: FactoryRunVersion,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
  /// Authoritative last transition time.
  pub updated_at: Timestamp,
}

impl FactoryRunSummary {
  /// Returns this summary's deterministic newest-first position.
  #[must_use]
  pub const fn page_position(&self) -> FactoryRunPagePosition {
    FactoryRunPagePosition {
      admitted_at: self.admitted_at,
      run_id: self.id,
    }
  }
}

/// One deterministic page of visible matching Factory Runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunPage {
  /// Matching visible count before cursor pagination.
  pub total: u64,
  /// Current summaries in deterministic newest-first order.
  pub items: Vec<FactoryRunSummary>,
  /// Exclusive continuation position when another item exists.
  pub next_cursor: Option<FactoryRunPagePosition>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn factory_discovery_requests_enforce_bounds_and_time_order() {
    let project_id = ProjectId::generate();
    assert!(
      ListProjectFactoryConfigurations::new(project_id, None, 0, FactoryConfigurationListVisibility::all(),).is_err()
    );
    assert!(
      ListFactoryRuns::new(
        None,
        FactoryRunFilter::default(),
        None,
        0,
        FactoryRunListVisibility::all()
      )
      .is_err()
    );
    let error = ListFactoryRuns::new(
      None,
      FactoryRunFilter {
        admitted_from: Some(Timestamp::from_unix_millis(2).unwrap()),
        admitted_before: Some(Timestamp::from_unix_millis(1).unwrap()),
        ..FactoryRunFilter::default()
      },
      None,
      1,
      FactoryRunListVisibility::all(),
    )
    .unwrap_err();
    assert!(matches!(
      error,
      StoreError::InvalidInput {
        source: StoreInputError::InvalidFactoryRunFilter,
        ..
      }
    ));
  }
}
