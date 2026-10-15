//! Trusted review planning over exact selected records in the common node journal.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::FactoryRunStore;
use serde::Serialize;
use std::sync::Arc;

/// Frozen operator policy and installed evaluator metadata; model payloads cannot change it.
#[derive(Clone, Serialize)]
pub struct FactoryReviewPlanningProfile {
  /// Exact installed action selector, independent of logical node names.
  pub action: FactoryKey,
  /// Published outcome and schema for a complete immutable plan.
  pub ready: FlowOutcomeDefinition,
  /// Published outcome and schema for metadata-only capability escalation.
  pub escalation: FlowOutcomeDefinition,
  /// Review purpose supported by the exact evaluator inventory.
  pub purpose: ReviewPurpose,
  /// Project-owned immutable criterion packs.
  pub criterion_packs: Vec<CriterionPack>,
  /// Exact configured evaluator branches, including required branches and substitutes.
  pub branches: Vec<ReviewBranch>,
  /// Immutable selection, quorum and aggregate review budget.
  pub policy: EvaluationPolicy,
  /// Exact data-handling policy checked before evaluator dispatch.
  pub data_handling: ImmutableReference,
  /// Frozen capability metadata; no evaluator is contacted by this action.
  pub capabilities: Vec<ReviewEvaluatorCapability>,
}
impl FactoryReviewPlanningProfile {
  /// Content identity includes policy, capabilities, action and both published outcomes.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid_profile())?;
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid_profile());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.review-planning-profile.v1",
      &[&bytes],
    ))
  }
}
fn invalid_profile() -> FactoryError {
  FactoryError::InvalidReference {
    relationship: "review planning profile",
  }
}
/// Installed metadata-only planning capability; persistence and replay belong to the Node owner.
pub struct FactoryReviewPlanningAction<S> {
  identity: ImmutableReference,
  store: Arc<S>,
  profile: FactoryReviewPlanningProfile,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> FactoryReviewPlanningAction<S> {
  /// Installs one exact profile and authoritative journal, without a second result store.
  #[must_use]
  pub fn new(
    identity: ImmutableReference,
    store: Arc<S>,
    profile: FactoryReviewPlanningProfile,
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
#[async_trait]
impl<S: FactoryRunStore> FactoryFlowActionPlugin for FactoryReviewPlanningAction<S> {
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.plugin() != &self.identity
      || binding.action() != &self.profile.action
      || binding.parameters() != &serde_json::json!({})
      || self.identity.digest() != self.profile.digest().map_err(|_| ApplicationError::invalid())?
      || self.profile.ready.key() == self.profile.escalation.key()
      || !outcomes.contains(&self.profile.ready)
      || !outcomes.contains(&self.profile.escalation)
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
    // This capability performs no external dispatch or protected byte reads.
    Ok(Some(BudgetUsage::default()))
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(request.input.factory_run_id()).await?;
    let observed_at = self.clock.now()?.max(request.observed_at);
    let [source] = request.input.sources() else {
      return Err(ApplicationError::invalid());
    };
    if snapshot.work.id() != request.work_id
      || snapshot.work.subject() != request.subject
      || !snapshot.flow.data.inputs.contains(request.input)
      || !self.profile.policy.budget().fits_within(request.budget)
      || observed_at >= request.deadline
      || observed_at >= request.authority_expires_at
    {
      return Err(ApplicationError::invalid());
    }
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|row| row.node_attempt_id() == source.node_attempt_id())
      .ok_or_else(ApplicationError::invalid)?;
    if record.digest().map_err(|_| ApplicationError::invalid())? != source.digest()
      || record.payload().value() != request.input.payload().value()
    {
      return Err(ApplicationError::invalid());
    }
    let evidence: EvidenceManifest =
      serde_json::from_value(request.input.payload().value().clone()).map_err(|_| ApplicationError::invalid())?;
    if record.discovery() != Some(&FlowDiscoveryFacts::evidence(&evidence).map_err(|_| ApplicationError::invalid())?)
      || evidence.subject().exact() != request.subject
      || evidence.constructed_at() > observed_at
      || evidence.items().iter().any(|item| observed_at >= item.fresh_until())
    {
      return Err(ApplicationError::invalid());
    }
    let policy_deadline = request
      .started_at
      .unix_millis()
      .checked_add(
        i64::try_from(self.profile.policy.budget().max_elapsed_millis()).map_err(|_| ApplicationError::invalid())?,
      )
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(ApplicationError::invalid)?;
    let id = EvaluationPlanId::from_uuid(uuid::Uuid::new_v5(
      &request.input.flow_run_id().as_uuid(),
      &request.operation_id.as_bytes(),
    ))
    .map_err(|_| ApplicationError::invalid())?;
    let evidence_deadline = evidence
      .items()
      .iter()
      .map(EvidenceItem::fresh_until)
      .min()
      .ok_or_else(ApplicationError::invalid)?;
    let prepared = prepare_evaluation_plan(
      EvaluationPlanDefinition {
        id,
        purpose: self.profile.purpose,
        subject: evidence.subject().clone(),
        criterion_packs: self.profile.criterion_packs.clone(),
        branches: self.profile.branches.clone(),
        budget: self.profile.policy.budget(),
        created_at: request.started_at,
        deadline: request.deadline.min(policy_deadline).min(evidence_deadline),
        data_handling: self.profile.data_handling.clone(),
      },
      &evidence,
      &self.profile.policy,
      &self.profile.capabilities,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let (outcome, discovery, value) = match prepared {
      ReviewPlanPreparation::Ready(plan) => (
        &self.profile.ready,
        Some(FlowDiscoveryFacts::review_plan(&plan).map_err(|_| ApplicationError::invalid())?),
        serde_json::to_value(&plan).map_err(|_| ApplicationError::invalid())?,
      ),
      ReviewPlanPreparation::Escalate(reason) => (
        &self.profile.escalation,
        None,
        match reason {
          ReviewPlanEscalation::RequiredCapabilityUnavailable { branch } => {
            serde_json::json!({"reason":"required_capability_unavailable","branch":branch})
          }
          ReviewPlanEscalation::QuorumCapabilityUnavailable { required, available } => {
            serde_json::json!({"reason":"quorum_capability_unavailable","required":required,"available":available})
          }
        },
      ),
    };
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference() == outcome.schema())
      .ok_or_else(ApplicationError::invalid)?;
    let payload = FlowPayload::new(schema, value).map_err(|_| ApplicationError::invalid())?;
    let output_bytes = u64::try_from(
      serde_json::to_vec(&payload)
        .map_err(|_| ApplicationError::invalid())?
        .len(),
    )
    .map_err(|_| ApplicationError::invalid())?;
    if output_bytes
      > self
        .profile
        .policy
        .budget()
        .max_output_bytes()
        .min(request.budget.max_output_bytes())
    {
      return Err(ApplicationError::invalid());
    }
    let finished_at = self.clock.now()?.max(observed_at);
    if finished_at
      >= request
        .deadline
        .min(evidence_deadline)
        .min(policy_deadline)
        .min(request.authority_expires_at)
    {
      return Err(ApplicationError::invalid());
    }
    let elapsed_millis = u64::try_from(
      finished_at
        .unix_millis()
        .checked_sub(request.started_at.unix_millis())
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    Ok(FactoryFlowActionObservation {
      discovery,
      outcome: outcome.key().clone(),
      payload,
      usage: BudgetUsage {
        output_bytes,
        elapsed_millis,
        ..BudgetUsage::default()
      },
      outbox: vec![],
    })
  }
}
