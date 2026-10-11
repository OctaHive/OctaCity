//! Exact verification contracts over the existing immutable nested Flow runtime.
use crate::{
  AdmittedFlow, BudgetLimit, CandidateSubject, ContextSourceKind, DeterministicGateOutcome, FactoryArtifactReference,
  FactoryContextReference, FactoryDigest, FactoryError, FactoryKey, FlowDataMapping, FlowDefinition, FlowDefinitionRef,
  FlowNodeInput, FlowRunId, ImmutableReference, MAX_EVIDENCE_ITEMS, MAX_FLOW_DATA_BYTES, MAX_FLOW_DEFINITION_NODES,
  MAX_PERMISSION_ENTRIES_PER_CATEGORY, NetworkHost, WorkflowCycleId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Source exposure selected independently from control ordering.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSourceVisibility {
  /// Observe a deployed contract without repository or implementation inputs.
  BlackBox,
  /// Permit explicitly selected read-only repository inputs.
  ReadOnlySource,
}
/// Operator-owned scope for one ordinary command or reasoning check.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
  /// Arbitrary acceptance class; its name has no runtime semantics.
  pub class: FactoryKey,
  /// Explicit projection of an exact deployment and public contract from frozen input.
  pub target: FlowDataMapping,
  /// Whether repository content may be exposed to this check.
  pub source_visibility: VerificationSourceVisibility,
  /// Explicit network destinations; permissions can only narrow this set.
  pub endpoints: Vec<String>,
  /// Exact allowed verification credential profiles, never secret values.
  pub secret_profiles: Vec<FactoryKey>,
  /// Allowed environment workload identities.
  pub identity_profiles: Vec<FactoryKey>,
  /// Whether absence or failure must prevent verification acceptance.
  pub required: bool,
}
/// Exact immutable deployed subject selected without mutable environment discovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationTarget {
  /// Exact candidate deployed into this environment.
  pub subject: CandidateSubject,
  /// Immutable environment/deployment receipt supplied by a trusted predecessor.
  pub environment: ImmutableReference,
  /// Public contract selected by the explicit input projection.
  pub public_contract: FactoryArtifactReference,
  /// Exact deployed endpoint when this check uses network observations.
  /// Trusted compilation validates it against the declared host grants.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub endpoint: Option<String>,
  /// Exact earlier verification artifacts selected for this check's evidence context.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub evidence_inputs: Vec<FactoryArtifactReference>,
}
impl VerificationTarget {
  /// Returns the complete canonical retained inventory, including the public contract.
  /// Rejects duplicate Artifact identities and the common Evidence Manifest ceiling.
  pub fn retained_inputs(&self) -> Result<Vec<FactoryArtifactReference>, FactoryError> {
    let mut inputs = vec![self.public_contract.clone()];
    inputs.extend(self.evidence_inputs.iter().cloned());
    crate::evidence::canonical_evidence_inputs(inputs)
  }
}
/// Immutable plan parameters selected by a Flow, with no fixed check vocabulary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationPolicy {
  /// Exact handling policy for the selected inputs and evidence.
  pub data_handling: ImmutableReference,
  /// Minimum number of independently passing checks, in addition to all required checks.
  pub required_quorum: u16,
  /// Explicit selections keyed by arbitrary nodes of this definition.
  pub checks: BTreeMap<FactoryKey, VerificationCheck>,
}
impl VerificationPolicy {
  pub(crate) fn validate_definition(&self, definition: &FlowDefinition) -> Result<(), FactoryError> {
    if self.checks.is_empty()
      || self.checks.len() > MAX_FLOW_DEFINITION_NODES
      || self.required_quorum == 0
      || usize::from(self.required_quorum) > self.checks.len()
      || self.checks.iter().any(|(key, check)| {
        definition.node(key).is_none_or(|node| node.build().is_none())
          || definition.node(key).is_some_and(|node| {
            let grants = node.permissions();
            grants
              .network_hosts()
              .any(|host| !check.endpoints.iter().any(|allowed| allowed == host.as_str()))
              || grants
                .secret_profiles()
                .any(|profile| !check.secret_profiles.contains(profile))
              || grants
                .workload_identity_profiles()
                .any(|profile| !check.identity_profiles.contains(profile))
          })
          || check.endpoints.len() > MAX_PERMISSION_ENTRIES_PER_CATEGORY
          || check.endpoints.iter().any(|host| NetworkHost::new(host).is_err())
          || check.secret_profiles.len() > MAX_PERMISSION_ENTRIES_PER_CATEGORY
          || check.identity_profiles.len() > MAX_PERMISSION_ENTRIES_PER_CATEGORY
          || check.endpoints.windows(2).any(|p| p[0] >= p[1])
          || check.secret_profiles.windows(2).any(|p| p[0] >= p[1])
          || check.identity_profiles.windows(2).any(|p| p[0] >= p[1])
      })
    {
      return Err(invalid());
    }
    Ok(())
  }
}
/// Run-bound verification snapshot; the admitted closure owns graphs and execution.
/// References bind all node budgets, schemas, projections and profiles without
/// introducing a second state machine or a mutable configuration lookup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerificationPlan {
  admission_digest: FactoryDigest,
  definition: FlowDefinitionRef,
  flow_run_id: FlowRunId,
  cycle_id: WorkflowCycleId,
  subject: CandidateSubject,
  budget: BudgetLimit,
  policy: VerificationPolicy,
}
impl VerificationPlan {
  /// Freezes policy metadata for the initial root cycle; execution uses `for_input`.
  pub fn freeze(
    admitted: &AdmittedFlow,
    reference: FlowDefinitionRef,
    subject: CandidateSubject,
  ) -> Result<Self, FactoryError> {
    Self::prepare(
      admitted,
      reference,
      admitted.root_run().id(),
      admitted.initial_cycle().id(),
      subject,
    )
  }
  /// Returns the common plan after validating only this node's selected input.
  pub fn for_input(admitted: &AdmittedFlow, input: &FlowNodeInput) -> Result<Self, FactoryError> {
    Ok(VerificationCheckInput::new(admitted, input)?.plan)
  }
  fn prepare(
    admitted: &AdmittedFlow,
    reference: FlowDefinitionRef,
    flow_run_id: FlowRunId,
    cycle_id: WorkflowCycleId,
    subject: CandidateSubject,
  ) -> Result<Self, FactoryError> {
    admitted.validated()?;
    let definition = admitted.closure().definition(reference).ok_or_else(invalid)?;
    let policy = definition.execution().verification().ok_or_else(invalid)?.clone();
    policy.validate_definition(definition)?;
    Ok(Self {
      admission_digest: FactoryDigest::content_sha256(&serde_json::to_vec(admitted).map_err(|_| invalid())?),
      definition: reference,
      flow_run_id,
      cycle_id,
      subject,
      budget: definition.execution().budget(),
      policy,
    })
  }
  /// Restores initial-cycle policy metadata against the original admission and content identity.
  /// Dynamic execution plans are reconstructed by `FlowBuildIntent::restore` from its frozen input.
  pub fn restore(
    bytes: &[u8],
    admitted: &AdmittedFlow,
    definition: FlowDefinitionRef,
    subject: &CandidateSubject,
    digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let plan = Self::freeze(admitted, definition, subject.clone())?;
    if wire != serde_json::to_value(&plan).map_err(|_| invalid())? || plan.digest()? != digest {
      return Err(invalid());
    }
    Ok(plan)
  }
  /// Returns the exact candidate or deployable identity being checked.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }
  /// Returns the admitted nested Flow Definition, including its control and data contracts.
  #[must_use]
  pub const fn definition(&self) -> FlowDefinitionRef {
    self.definition
  }
  /// Returns the immutable environment, exposure and quorum policy.
  #[must_use]
  pub const fn policy(&self) -> &VerificationPolicy {
    &self.policy
  }
  /// Returns the enclosing hard execution budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
  /// Content-addresses admission, invocation, cycle, candidate and immutable policy.
  /// Each check separately binds its resolved target and evidence through its input digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.verification-plan.v1",
      &[&bytes],
    ))
  }
}
/// Exact per-node verification selection, separate from common immutable plan policy.
/// The frozen input digest binds target, environment, Context, provenance and generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerificationCheckInput {
  plan: VerificationPlan,
  check: FactoryKey,
  input_digest: FactoryDigest,
  target: VerificationTarget,
}
impl VerificationCheckInput {
  /// Resolves only the selected check's projection from its trusted frozen input.
  pub fn new(admitted: &AdmittedFlow, input: &FlowNodeInput) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    let context = input.context().ok_or_else(invalid)?;
    let subject = context.subject().candidate().ok_or_else(invalid)?.clone();
    let plan = VerificationPlan::prepare(
      admitted,
      input.definition(),
      input.flow_run_id(),
      input.cycle_id(),
      subject,
    )?;
    let check = plan.policy.checks.get(input.node()).ok_or_else(invalid)?;
    let target: VerificationTarget = serde_json::from_value(
      check
        .target
        .project_raw(&serde_json::to_value(input).map_err(|_| invalid())?)?,
    )
    .map_err(|_| invalid())?;
    if target.subject != plan.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    if target
      .endpoint
      .as_ref()
      .is_some_and(|endpoint| endpoint.is_empty() || endpoint.len() > 2048 || endpoint.contains('\0'))
    {
      return Err(invalid());
    }
    target.retained_inputs()?;
    if check.source_visibility == VerificationSourceVisibility::BlackBox
      && context
        .entries()
        .iter()
        .any(|entry| match (entry.source_kind(), entry.source()) {
          (ContextSourceKind::Task | ContextSourceKind::Specification, FactoryContextReference::Artifact(artifact)) => {
            artifact != &target.public_contract
          }
          (ContextSourceKind::Evidence, FactoryContextReference::Artifact(artifact)) => {
            !target.evidence_inputs.contains(artifact)
          }
          _ => true,
        })
    {
      return Err(invalid());
    }
    Ok(Self {
      plan,
      check: input.node().clone(),
      input_digest: input.digest()?,
      target,
    })
  }
  /// Returns the common immutable plan for this invocation and cycle.
  #[must_use]
  pub const fn plan(&self) -> &VerificationPlan {
    &self.plan
  }
  /// Returns the arbitrary declared check key.
  #[must_use]
  pub const fn check(&self) -> &FactoryKey {
    &self.check
  }
  /// Returns the exact frozen node input identity, including execution generation.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
  /// Returns the exact deployment, contract and own evidence selected by this check.
  #[must_use]
  pub const fn target(&self) -> &VerificationTarget {
    &self.target
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Verification Plan",
  }
}

/// One bounded integer measurement; metric names and units are configuration-owned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationMeasurement {
  /// Arbitrary metric key.
  pub name: FactoryKey,
  /// Measured value in the unit declared by the pinned check contract.
  pub value: u64,
}

/// Typed tool observation; accepting a Flow requires independently qualified evidence.
/// Plan identity binds admission and policy; input identity binds the resolved
/// deployment, target, Context and execution generation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationObservation {
  /// Exact common admitted plan and policy.
  pub plan_digest: FactoryDigest,
  /// Exact frozen check input; plan identity alone cannot authorize substituted targets.
  pub input_digest: FactoryDigest,
  /// Arbitrary declared node key that produced this observation.
  pub check: FactoryKey,
  /// Tool observation, independently qualified against retained Build provenance.
  pub outcome: DeterministicGateOutcome,
  /// Bounded diagnostics; no metric name has engine semantics.
  pub measurements: Vec<VerificationMeasurement>,
}
impl VerificationObservation {
  /// Checks scope and bounds before a trusted adapter qualifies the retained output.
  pub fn verify(&self, input: &VerificationCheckInput) -> Result<DeterministicGateOutcome, FactoryError> {
    if self.plan_digest != input.plan.digest()?
      || self.input_digest != input.input_digest
      || self.check != input.check
      || self.measurements.len() > MAX_EVIDENCE_ITEMS
      || self.measurements.iter().enumerate().any(|(index, metric)| {
        metric.value > i64::MAX as u64 || self.measurements[..index].iter().any(|other| other.name == metric.name)
      })
    {
      return Err(invalid());
    }
    Ok(self.outcome)
  }
}
