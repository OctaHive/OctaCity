use serde::{Deserialize, Serialize};

use super::{ResearchAttemptDisposition, ResearchInput, ResearchPolicy, digest, invalid};
use crate::{
  AdmittedFlow, BudgetLimit, BudgetUsage, FactoryDigest, FactoryError, FactoryPermissionSet, FlowBuildProfile,
  FlowNodeKind, FlowRun, NodeAttempt, NodeExecutionIdentity, WorkEnvelope,
};

/// Frozen ordinary-Build intent for an already persisted generic research Node Attempt.
///
/// Persist these bytes before external dispatch. The owning Flow interpreter
/// remains responsible for readiness, fencing, retry admission, and committing
/// the returned typed completion. It derives `usage_before` from retained external
/// executions, independently of generic Flow node numbering. Replay observes the same Build.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchBuildIntent {
  input: ResearchInput,
  flow: FlowRun,
  node: NodeAttempt,
  usage_before: BudgetUsage,
  priority: i64,
  profile: FlowBuildProfile,
  permissions: FactoryPermissionSet,
  budget: BudgetLimit,
}

impl ResearchBuildIntent {
  /// Binds a frozen input, Build priority and exact profile to one bounded Node Attempt.
  pub fn new(
    input: ResearchInput,
    policy: &ResearchPolicy,
    flow: FlowRun,
    node: NodeAttempt,
    usage_before: BudgetUsage,
    priority: i64,
  ) -> Result<Self, FactoryError> {
    let profile = policy
      .execution_profile(flow.definition(), node.node_key())
      .ok_or_else(|| invalid("research execution profile"))?
      .clone();
    let definition = policy
      .closure()
      .definition(flow.definition())
      .ok_or_else(|| invalid("research execution definition"))?;
    let declared = definition
      .node(node.node_key())
      .ok_or_else(|| invalid("research execution node"))?;
    if !matches!(node.node_kind(), FlowNodeKind::Reasoning | FlowNodeKind::BuildCommand)
      || node.node_kind() != declared.kind()
      || node.flow_run_id() != flow.id()
      || node.factory_run_id() != flow.factory_run_id()
      || flow.factory_run_id() != input.factory_run_id()
      || node.input_digest() != input.digest()?
      || node.execution() != &NodeExecutionIdentity::External(profile.tool.clone())
      || node.budget() != declared.budget()
      || node.stage_projection_id().is_some()
    {
      return Err(invalid("research Node Attempt binding"));
    }
    let ResearchAttemptDisposition::Dispatch {
      attempt: _,
      budget,
      permissions,
    } = policy.next_attempt(&input, usage_before, declared.permissions())?
    else {
      return Err(invalid("research attempts exhausted"));
    };
    if !node.budget().fits_within(budget) {
      return Err(invalid("research Node Attempt budget"));
    }
    let budget = node.budget();
    Ok(Self {
      input,
      flow,
      node,
      usage_before,
      priority,
      profile,
      permissions: *permissions,
      budget,
    })
  }

  /// Reconstructs stored intent through current exact Work, policy, and durable node identities.
  pub fn restore(
    bytes: &[u8],
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    policy: &ResearchPolicy,
    flow: &FlowRun,
    node: &NodeAttempt,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > super::MAX_RESEARCH_CONTRACT_BYTES {
      return Err(invalid("research intent bytes"));
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
      input: serde_json::Value,
      flow: FlowRun,
      node: NodeAttempt,
      usage_before: BudgetUsage,
      priority: i64,
      profile: FlowBuildProfile,
      permissions: FactoryPermissionSet,
      budget: BudgetLimit,
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| invalid("research intent schema"))?;
    if &wire.flow != flow || &wire.node != node {
      return Err(invalid("research durable intent identity"));
    }
    let input = ResearchInput::restore(
      &serde_json::to_vec(&wire.input).map_err(|_| invalid("research intent input"))?,
      work,
      admitted,
      policy.digest()?,
    )?;
    let intent = Self::new(
      input,
      policy,
      flow.clone(),
      node.clone(),
      wire.usage_before,
      wire.priority,
    )?;
    if wire.profile != intent.profile || wire.permissions != intent.permissions || wire.budget != intent.budget {
      return Err(invalid("research intent authority"));
    }
    Ok(intent)
  }

  /// Stable operation identity including every immutable input and resource reservation.
  pub fn operation_id(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.research-build-intent.v1", self)
  }
  /// Returns exact subject, symptoms, environment, prior observations, and frozen context.
  #[must_use]
  pub const fn input(&self) -> &ResearchInput {
    &self.input
  }
  /// Returns the admitted Flow execution selected before dispatch.
  #[must_use]
  pub const fn flow(&self) -> &FlowRun {
    &self.flow
  }
  /// Returns the append-only Node Attempt selected before dispatch.
  #[must_use]
  pub const fn node(&self) -> &NodeAttempt {
    &self.node
  }
  /// Returns the exact per-node model or command selection.
  #[must_use]
  pub const fn profile(&self) -> &FlowBuildProfile {
    &self.profile
  }
  /// Returns the immutable narrowed execution authority.
  #[must_use]
  pub const fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }
  /// Returns the authorized per-attempt ceiling.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
  /// Returns cumulative consumption preceding this intent.
  #[must_use]
  pub const fn usage_before(&self) -> BudgetUsage {
    self.usage_before
  }
  /// Returns the one-based external execution ordinal, excluding orchestration nodes.
  #[must_use]
  pub const fn execution_attempt(&self) -> u32 {
    // Construction reserves one attempt and proves this addition is within policy bounds.
    self.usage_before.attempts + 1
  }
  /// Returns the immutable ordinary-Build queue priority, retained across restart.
  #[must_use]
  pub const fn priority(&self) -> i64 {
    self.priority
  }
}
