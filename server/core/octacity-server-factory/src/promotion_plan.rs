//! Immutable deployment selections at installed trusted-action boundaries.
use crate::{
  AdmittedFlow, CandidateSubject, EvidenceRequirement, FactoryDigest, FactoryError, FactoryKey, FlowGateProgram,
  FlowNodeDefinition, FlowNodeInput, FlowNodeKind, ImmutableReference, MAX_FLOW_DATA_BYTES,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Operator-owned rollout parameters; strategy names never select runtime branches.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionPolicy {
  /// Arbitrary strategy capability understood by the selected deployment Pipeline.
  pub strategy: FactoryKey,
  /// Exact environment contracts selected at publication.
  pub environments: BTreeMap<FactoryKey, ImmutableReference>,
  /// Bounded cohort/allocation parameters passed to that Pipeline.
  pub allocation: serde_json::Value,
  /// Finite observation window inherited by bounded checks.
  pub observation_window_millis: u64,
  /// Exact independently verified output contracts required by this policy.
  pub evidence_requirements: Vec<EvidenceRequirement>,
  /// Environment aliases whose writes require production authorization.
  #[serde(default)]
  pub production_environments: Vec<FactoryKey>,
  /// Explicit operator opt-in to immediate production for immutable low-risk Work.
  #[serde(default)]
  pub allow_immediate_low_risk: bool,
  /// Required deterministic/human predecessor gates. Their exact finite outcomes
  /// are selected by this action's admitted Record input bindings. The owner must
  /// prepare those sources from committed records in the current Flow/cycle.
  pub required_gates: Vec<FactoryKey>,
  /// Exact configured deterministic or human disposition node, including its finite outcomes.
  pub criteria: FactoryKey,
}
/// Immutable exact action input and rollout policy; this value grants no readiness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PromotionPlan {
  admission_digest: FactoryDigest,
  input_digest: FactoryDigest,
  subject: CandidateSubject,
  policy: PromotionPolicy,
  criteria: FlowNodeDefinition,
  budget: crate::BudgetLimit,
}
impl PromotionPlan {
  /// Freezes only the original admitted action binding and its exact candidate input.
  pub fn freeze(admitted: &AdmittedFlow, input: &FlowNodeInput) -> Result<Self, FactoryError> {
    admitted.validated()?;
    input.verify_admission(admitted)?;
    let definition = admitted.closure().definition(input.definition()).ok_or_else(invalid)?;
    let node = definition.node(input.node()).ok_or_else(invalid)?;
    if node.kind() != FlowNodeKind::TrustedAction {
      return Err(invalid());
    }
    let binding = node.action().ok_or_else(invalid)?;
    let policy: PromotionPolicy =
      serde_json::from_value(binding.parameters().get("plan").ok_or_else(invalid)?.clone()).map_err(|_| invalid())?;
    if policy.environments.is_empty()
      || policy.environments.len() > crate::MAX_PERMISSION_ENTRIES_PER_CATEGORY
      || !policy.allocation.is_object()
      || policy.observation_window_millis == 0
      || policy.observation_window_millis > node.budget().max_elapsed_millis()
      || policy.evidence_requirements.len() > crate::MAX_EVIDENCE_ITEMS
      || policy.production_environments.len() > policy.environments.len()
      || policy
        .production_environments
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != policy.production_environments.len()
      || policy
        .production_environments
        .iter()
        .any(|key| !policy.environments.contains_key(key))
      || policy.required_gates.len() > crate::MAX_FLOW_DEFINITION_NODES
      || policy
        .required_gates
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != policy.required_gates.len()
      || policy.required_gates.iter().any(|key| {
        definition
          .node(key)
          .is_none_or(|gate| !matches!(gate.kind(), FlowNodeKind::DeterministicGate | FlowNodeKind::HumanGate))
          || node.input_binding().is_none_or(|binding| !binding.selects_record(key))
      })
      || !policy.evidence_requirements.is_empty()
        && node
          .build()
          .is_none_or(|binding| binding.evidence != policy.evidence_requirements)
    {
      return Err(invalid());
    }
    let criteria = definition.node(&policy.criteria).ok_or_else(invalid)?;
    match criteria.kind() {
      FlowNodeKind::DeterministicGate => {
        FlowGateProgram::from_node(criteria)?;
      }
      FlowNodeKind::HumanGate => {}
      _ => return Err(invalid()),
    }
    let subject = input
      .context()
      .and_then(|context| context.subject().candidate())
      .ok_or_else(invalid)?
      .clone();
    Ok(Self {
      admission_digest: FactoryDigest::content_sha256(&serde_json::to_vec(admitted).map_err(|_| invalid())?),
      input_digest: input.digest()?,
      subject,
      policy,
      criteria: criteria.clone(),
      budget: node.budget(),
    })
  }
  /// Checks the exact Work scope and explicit production ceiling before any effect.
  /// The owner and store verify prepared gate sources against committed records;
  /// this check neither commits a transition nor bypasses owner fencing.
  pub fn authorize(
    &self,
    work: &crate::WorkEnvelope,
    admitted: &AdmittedFlow,
    input: &FlowNodeInput,
  ) -> Result<(), FactoryError> {
    input.verify_work(work, admitted)?;
    if Self::freeze(admitted, input)? != *self
      || input.sources().len() < self.policy.required_gates.len()
      || !self.policy.production_environments.is_empty()
        && self.policy.required_gates.is_empty()
        && (!self.policy.allow_immediate_low_risk || work.risk() != crate::RiskClass::Low)
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Returns the admitted deterministic/human node with its finite outcomes and bindings.
  /// Disposition still runs through normal owner/control contracts; this plan grants no decision.
  #[must_use]
  pub const fn criteria(&self) -> &FlowNodeDefinition {
    &self.criteria
  }
  /// Returns the immutable action resource ceiling, independent of elapsed wall time on replay.
  #[must_use]
  pub const fn budget(&self) -> crate::BudgetLimit {
    self.budget
  }
  /// Restores against authoritative admission/input identities, rejecting replacement or tampering.
  pub fn restore(
    bytes: &[u8],
    admitted: &AdmittedFlow,
    input: &FlowNodeInput,
    digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let plan = Self::freeze(admitted, input)?;
    if value != serde_json::to_value(&plan).map_err(|_| invalid())? || plan.digest()? != digest {
      return Err(invalid());
    }
    Ok(plan)
  }
  /// Returns the complete immutable plan identity, including scope and exact frozen input.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(FactoryDigest::sha256("octacity.factory.promotion-plan.v1", &[&bytes]))
  }
  /// Returns the exact candidate selected by trusted Context construction.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }
  /// Returns configured environments, strategy, bounds and criteria.
  #[must_use]
  pub const fn policy(&self) -> &PromotionPolicy {
    &self.policy
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Promotion Plan",
  }
}

/// Typed bounded receipt from an independently verified deployment Build output.
/// Parsing this value alone grants no authority; the owner requires actual Build
/// provenance, current fencing and the configured deterministic/human disposition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionObservation {
  /// Complete immutable plan identity.
  pub plan_digest: FactoryDigest,
  /// Exact frozen action input, including cycle and generation.
  pub input_digest: FactoryDigest,
  /// Exact accepted candidate; never a mutable branch name.
  pub subject: CandidateSubject,
  /// Operator-selected environment alias.
  pub environment: FactoryKey,
  /// Exact immutable environment contract selected by that alias.
  pub environment_contract: ImmutableReference,
  /// Durable CI/CD deployment receipt; CI/CD owns rollout and rollback mechanics.
  pub deployment: ImmutableReference,
  /// Independently qualified observation, never an invented Flow outcome.
  pub outcome: crate::DeterministicGateOutcome,
  /// Authoritative start of this finite observation window.
  pub started_at: octacity_server_domain::Timestamp,
  /// Authoritative end; success must cover the complete selected window.
  pub finished_at: octacity_server_domain::Timestamp,
  /// Bounded non-negative measurements with operator-owned names.
  pub measurements: BTreeMap<FactoryKey, u64>,
}
impl PromotionObservation {
  /// Checks exact scope and bounds before retaining the actual Build proof.
  pub fn verify(
    &self,
    plan: &PromotionPlan,
    input: &FlowNodeInput,
    started_at: octacity_server_domain::Timestamp,
    deadline: octacity_server_domain::Timestamp,
    at: octacity_server_domain::Timestamp,
  ) -> Result<(), FactoryError> {
    let elapsed = self
      .finished_at
      .unix_millis()
      .checked_sub(self.started_at.unix_millis())
      .and_then(|value| u64::try_from(value).ok())
      .ok_or_else(invalid)?;
    if self.plan_digest != plan.digest()?
      || self.input_digest != input.digest()?
      || self.subject != plan.subject
      || plan.policy.environments.get(&self.environment) != Some(&self.environment_contract)
      || self.started_at < started_at
      || self.finished_at > at
      || at > deadline
      || elapsed == 0
      || elapsed > plan.policy.observation_window_millis
      || self.outcome == crate::DeterministicGateOutcome::Passed && elapsed != plan.policy.observation_window_millis
      || self.measurements.len() > crate::MAX_EVIDENCE_ITEMS
      || self.measurements.values().any(|value| *value > i64::MAX as u64)
    {
      return Err(invalid());
    }
    Ok(())
  }
}
