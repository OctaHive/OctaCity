//! Installed deterministic Decision over an explicitly selected completed review join.

use crate::{
  ApplicationError, FactoryEvaluationDecision, FactoryExecutionClock, FactoryFlowActionObservation,
  FactoryFlowActionPlugin, FactoryFlowActionRequest,
};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{FactoryRunSnapshot, FactoryRunStore};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

/// Installed policy and explicit outcome mapping; a model cannot supply either.
#[derive(Clone)]
pub struct FactoryEvaluationDecisionProfile {
  /// Exact installed action selector, independent of logical node names.
  pub action: FactoryKey,
  /// Immutable deterministic rules, including their content identity and version.
  pub policy: DecisionPolicy,
  /// One published output contract for each typed disposition.
  pub outcomes: Vec<(DecisionOutcome, FlowOutcomeDefinition)>,
}
impl FactoryEvaluationDecisionProfile {
  /// Binds the exact policy and canonically ordered disposition contracts.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let mut outcomes = self.outcomes.clone();
    outcomes.sort_by_key(|(disposition, _)| *disposition);
    let expected = DecisionOutcome::VALUES.iter().copied().collect::<BTreeSet<_>>();
    if outcomes.len() != expected.len()
      || outcomes
        .iter()
        .map(|(disposition, _)| *disposition)
        .collect::<BTreeSet<_>>()
        != expected
      || outcomes
        .iter()
        .map(|(_, outcome)| outcome.key())
        .collect::<BTreeSet<_>>()
        .len()
        != outcomes.len()
    {
      return Err(FactoryError::InvalidReference {
        relationship: "evaluation Decision profile",
      });
    }
    let bytes =
      serde_json::to_vec(&(&self.action, self.policy.version(), self.policy.digest(), outcomes)).map_err(|_| {
        FactoryError::InvalidReference {
          relationship: "evaluation Decision profile",
        }
      })?;
    Ok(FactoryDigest::sha256(
      "octacity.factory.evaluation-decision-profile.v1",
      &[&bytes],
    ))
  }
}

/// Metadata-only trusted action; the common Node owner persists and replays its result.
pub struct FactoryEvaluationDecisionAction<S> {
  identity: ImmutableReference,
  store: Arc<S>,
  profile: FactoryEvaluationDecisionProfile,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> FactoryEvaluationDecisionAction<S> {
  /// Installs immutable policy, authoritative history, and trusted time.
  #[must_use]
  pub fn new(
    identity: ImmutableReference,
    store: Arc<S>,
    profile: FactoryEvaluationDecisionProfile,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      identity,
      store,
      profile,
      clock,
    }
  }
}

// Whole typed records must be projected explicitly. Provenance alone does not
// authorize reading fields omitted by the configured input mapping.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selected {
  evidence: EvidenceManifest,
  plan: EvaluationPlan,
  assessments: Vec<Assessment>,
  signal_receipts: Vec<DecisionSignalReceipt>,
}

#[async_trait]
impl<S: FactoryRunStore> FactoryFlowActionPlugin for FactoryEvaluationDecisionAction<S> {
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.plugin() != &self.identity
      || binding.action() != &self.profile.action
      || binding.parameters() != &serde_json::json!({})
      || self.identity.digest() != self.profile.digest().map_err(|_| ApplicationError::invalid())?
      || self
        .profile
        .outcomes
        .iter()
        .any(|(_, outcome)| !outcomes.contains(outcome))
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    Ok(Some(BudgetUsage::default()))
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let snapshot = self.store.factory_run_snapshot(request.input.factory_run_id()).await?;
    let at = self.clock.now()?.max(request.observed_at);
    let selected: Selected = serde_json::from_value(request.input.payload().value().clone()).map_err(|_| invalid())?;
    if snapshot.work.id() != request.work_id
      || snapshot.work.subject() != request.subject
      || !snapshot.flow.data.inputs.contains(request.input)
      || !request.input.matches_candidate_resource(selected.plan.subject())
      || selected.plan.evidence_id() != selected.evidence.id()
      || selected.plan.subject() != selected.evidence.subject()
      || selected.plan.created_at() > at
      || selected.evidence.constructed_at() > at
      || selected.assessments.is_empty()
      || selected.assessments.len() > MAX_DECISION_ASSESSMENTS
      || selected.signal_receipts.len() > MAX_DECISION_SIGNAL_RECEIPTS
    {
      return Err(invalid());
    }
    let evidence_deadline = selected
      .evidence
      .items()
      .iter()
      .map(EvidenceItem::fresh_until)
      .min()
      .ok_or_else(invalid)?;
    let deadline = request
      .deadline
      .min(request.authority_expires_at)
      .min(selected.plan.deadline())
      .min(evidence_deadline);
    if at >= deadline {
      return Err(invalid());
    }
    let mut records = selected_records(&snapshot, request.input, at)?;
    take_record(
      &mut records,
      &selected.evidence,
      FlowDiscoveryFacts::evidence(&selected.evidence).map_err(|_| invalid())?,
    )?;
    take_record(
      &mut records,
      &selected.plan,
      FlowDiscoveryFacts::review_plan(&selected.plan).map_err(|_| invalid())?,
    )?;
    let mut progress = EvaluationProgress::from_plan(&selected.plan).map_err(|_| invalid())?;
    for assessment in &selected.assessments {
      assessment
        .validate_bindings(&selected.plan, &selected.evidence)
        .map_err(|_| invalid())?;
      let record = take_record(
        &mut records,
        assessment,
        FlowDiscoveryFacts::assessment(assessment).map_err(|_| invalid())?,
      )?;
      let attempt = snapshot
        .flow
        .attempts
        .iter()
        .find(|attempt| attempt.id() == record.node_attempt_id())
        .ok_or_else(invalid)?;
      let result =
        EvaluationBranchResult::new(&selected.plan, record.input(), attempt, assessment).map_err(|_| invalid())?;
      for state in [
        EvaluationBranchState::AttemptCreated,
        EvaluationBranchState::BuildActive,
        EvaluationBranchState::ResultPending,
      ] {
        progress = progress.advance_branch(result.branch(), state).map_err(|_| invalid())?;
      }
      progress = progress.record_result(result).map_err(|_| invalid())?;
    }
    for receipt in &selected.signal_receipts {
      take_record(&mut records, receipt, FlowDiscoveryFacts::decision_signal(receipt))?;
    }
    if !records.is_empty() {
      return Err(invalid());
    }
    progress
      .validate_bindings(
        &selected.plan,
        snapshot.flow.data.inputs.iter(),
        snapshot.flow.attempts.iter(),
        selected.assessments.iter(),
      )
      .map_err(|_| invalid())?;
    let id = DecisionId::from_uuid(uuid::Uuid::new_v5(
      &request.input.flow_run_id().as_uuid(),
      &request.operation_id.as_bytes(),
    ))
    .map_err(|_| invalid())?;
    let evaluation = FactoryEvaluationDecision::from_completed_join(
      id,
      &selected.evidence,
      &selected.plan,
      &self.profile.policy,
      &progress,
      &selected.assessments,
      &selected.signal_receipts,
    )
    .map_err(|_| invalid())?;
    let decision = evaluation.decision();
    let (_, outcome) = self
      .profile
      .outcomes
      .iter()
      .find(|(disposition, _)| *disposition == decision.outcome())
      .ok_or_else(invalid)?;
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference() == outcome.schema())
      .ok_or_else(invalid)?;
    let payload =
      FlowPayload::new(schema, serde_json::to_value(decision).map_err(|_| invalid())?).map_err(|_| invalid())?;
    let output_bytes =
      u64::try_from(serde_json::to_vec(&payload).map_err(|_| invalid())?.len()).map_err(|_| invalid())?;
    let finished_at = self.clock.now()?.max(at);
    let elapsed_millis = u64::try_from(
      finished_at
        .unix_millis()
        .checked_sub(request.started_at.unix_millis())
        .ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    let usage = BudgetUsage {
      output_bytes,
      elapsed_millis,
      ..BudgetUsage::default()
    };
    usage.validate(request.budget).map_err(|_| invalid())?;
    if finished_at >= deadline {
      return Err(invalid());
    }
    Ok(FactoryFlowActionObservation {
      discovery: Some(FlowDiscoveryFacts::decision(decision)),
      outcome: outcome.key().clone(),
      payload,
      usage,
      outbox: vec![],
    })
  }
}

fn selected_records<'a>(
  snapshot: &'a FactoryRunSnapshot,
  input: &FlowNodeInput,
  at: Timestamp,
) -> Result<Vec<&'a FlowNodeRecord>, ApplicationError> {
  let invalid = ApplicationError::invalid;
  input
    .sources()
    .iter()
    // The selected candidate record proves resource provenance; it is not an
    // additional assessment or an implicit projection into the decision payload.
    .filter(|source| input.accepted_resource_record() != Some(*source))
    .map(|source| {
      let record = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|record| record.node_attempt_id() == source.node_attempt_id())
        .ok_or_else(invalid)?;
      let completion = snapshot
        .flow
        .completions
        .iter()
        .find(|completion| completion.node_attempt_id() == source.node_attempt_id())
        .ok_or_else(invalid)?;
      if record.digest().map_err(|_| invalid())? != source.digest()
        || completion.output_digest() != source.digest()
        || record.input().factory_run_id() != input.factory_run_id()
        || record.input().work_id() != input.work_id()
        || !snapshot.flow.data.inputs.contains(record.input())
        || record.observed_at() > at
        || completion.observed_at() > at
      {
        return Err(invalid());
      }
      Ok(record)
    })
    .collect()
}

fn take_record<'a, T: Serialize>(
  records: &mut Vec<&'a FlowNodeRecord>,
  value: &T,
  discovery: FlowDiscoveryFacts,
) -> Result<&'a FlowNodeRecord, ApplicationError> {
  let value = serde_json::to_value(value).map_err(|_| ApplicationError::invalid())?;
  let index = records
    .iter()
    .position(|record| record.payload().value() == &value && record.discovery() == Some(&discovery))
    .ok_or_else(ApplicationError::invalid)?;
  Ok(records.remove(index))
}
