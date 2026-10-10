use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{TriagePolicy, TriagePolicySettings, TriageRoute, TriageSchema, invalid};
use crate::{
  BudgetLimit, FactoryArtifactReference, FactoryError, FactoryKey, FactoryWipLimits, FlowAdmissionLimits,
  FlowDefinitionRef, FlowTransitionTarget, PhasePoolPolicy, PinnedFlowDefinitionClosure,
};

/// Exact execution identity frozen before one triage phase can run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryTriageExecutionProfile {
  /// Exact server execution adapter or harness.
  pub producer: crate::ImmutableReference,
  /// Exact model or deterministic tool, without aliases.
  pub model_or_tool: crate::ImmutableReference,
  /// Exact task/prompt policy.
  pub task_digest: crate::FactoryDigest,
}

/// Operator-published intake settings retained with every admitted Flow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryTriageConfiguration {
  /// Exact replaceable two-phase composition called by the root entry.
  pub definition: FlowDefinitionRef,
  /// Frozen Project goals supplied to eligibility.
  pub project_goals: FactoryArtifactReference,
  /// Exact execution profile for eligibility.
  pub eligibility: FactoryTriageExecutionProfile,
  /// Exact execution profile for classification.
  pub classification: FactoryTriageExecutionProfile,
  /// Deterministic eligibility and routing bounds.
  pub policy: TriagePolicySettings,
  /// Frozen Project priority used by phase selection.
  pub project_priority: i32,
  /// Durable pool policy for every non-terminal root successor.
  pub pools: BTreeMap<TriageRoute, PhasePoolPolicy>,
  /// Exact execution capabilities needed by each successor.
  pub capabilities: BTreeMap<TriageRoute, BTreeSet<crate::ImmutableReference>>,
}

/// Exact root and reachable definitions selected by one Factory Configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryFlowConfiguration {
  /// Immutable complete reachable definition closure.
  pub closure: PinnedFlowDefinitionClosure,
  /// Admission-wide execution and authority ceilings.
  pub limits: FlowAdmissionLimits,
  /// Typed two-phase intake and phase-ready successor policies.
  pub triage: FactoryTriageConfiguration,
}

impl FactoryFlowConfiguration {
  /// Revalidates selected definitions, bounds, and finite route-to-pool mappings.
  pub fn validate(
    &self,
    configuration: &crate::FactoryConfigurationRef,
    budget: BudgetLimit,
    wip: FactoryWipLimits,
  ) -> Result<(), FactoryError> {
    let validated = self.closure.clone().validate(self.limits.clone())?;
    if !self.limits.budget().fits_within(budget) || u32::from(self.limits.max_wip()) > wip.max_active_stages() {
      return Err(invalid("configured Flow bounds"));
    }
    let root = self
      .closure
      .definition(self.closure.root())
      .ok_or_else(|| invalid("root Flow"))?;
    let entry = root.node(root.entry()).ok_or_else(|| invalid("root entry"))?;
    let triage = self
      .closure
      .definition(self.triage.definition)
      .ok_or_else(|| invalid("triage definition"))?;
    if self
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes())
      .filter_map(|node| node.phase_pool())
      .any(|phase| !self.triage.pools.values().any(|policy| &policy.phase == phase))
    {
      return Err(invalid("configured node phase pool"));
    }
    super::composition::validate_composition(triage)?;
    if entry.subflow_definition() != Some(triage.reference())
      || root.input_schema() != Some(&TriageSchema::EligibilityInput.reference()?)
    {
      return Err(invalid("root triage entry"));
    }
    let mut routes = BTreeMap::new();
    for terminal in triage.terminals() {
      let route = TriageRoute::ALL
        .into_iter()
        .find(|route| route.as_str() == terminal.key().as_str())
        .ok_or_else(|| invalid("triage terminal"))?;
      let transitions = root
        .transitions()
        .iter()
        .filter(|edge| edge.predecessor() == root.entry() && edge.outcome() == terminal.key())
        .collect::<Vec<_>>();
      let [edge] = transitions.as_slice() else {
        return Err(invalid("single triage successor"));
      };
      if edge.max_repeats() != 0 {
        return Err(invalid("intake cannot repeat"));
      }
      match edge.target() {
        FlowTransitionTarget::Terminal(_) => {
          if !matches!(
            route,
            TriageRoute::Rejection | TriageRoute::Escalation | TriageRoute::AlreadyFixed
          ) {
            return Err(invalid("missing phase successor"));
          }
        }
        FlowTransitionTarget::Node(node) => {
          if matches!(
            route,
            TriageRoute::Rejection | TriageRoute::Escalation | TriageRoute::AlreadyFixed
          ) {
            return Err(invalid("terminal triage dispatch"));
          }
          let policy = self
            .triage
            .pools
            .get(&route)
            .ok_or_else(|| invalid("phase pool policy"))?;
          policy.validate()?;
          if policy.phase.as_str() != route.as_str()
            || !policy.budget.fits_within(budget)
            || u32::from(policy.max_wip) > wip.max_active_stages()
          {
            return Err(invalid("phase pool bounds"));
          }
          let target = root.node(node).ok_or_else(|| invalid("phase successor"))?;
          if target.input_schema() != Some(&TriageSchema::Decision.reference()?)
            || target.phase_pool().is_some_and(|phase| phase != &policy.phase)
          {
            return Err(invalid("phase decision input"));
          }
          routes.insert(route, policy.clone());
        }
      }
    }
    if self
      .triage
      .capabilities
      .iter()
      .any(|(route, capabilities)| !routes.contains_key(route) || capabilities.len() > 16)
    {
      return Err(invalid("phase capabilities"));
    }
    if routes != self.triage.pools {
      return Err(invalid("unused phase pool policy"));
    }
    // Policy construction below also validates risk, bypass, and fallback bounds.
    TriagePolicy::for_flow(
      configuration.clone(),
      &validated,
      self.triage.definition,
      self.triage.policy.clone(),
    )?;
    Ok(())
  }

  /// Returns the unique declared root successor for a selected route.
  pub fn successor(&self, route: TriageRoute) -> Option<&FactoryKey> {
    let root = self.closure.definition(self.closure.root())?;
    root.transitions().iter().find_map(|edge| {
      if edge.predecessor() != root.entry() || edge.outcome().as_str() != route.as_str() {
        return None;
      }
      match edge.target() {
        FlowTransitionTarget::Node(node) => Some(node),
        _ => None,
      }
    })
  }
}

impl FactoryTriageExecutionProfile {
  /// Verifies the exact producer, model/tool, and task selected before execution.
  #[must_use]
  pub fn matches(&self, provenance: &super::TriageProvenance) -> bool {
    provenance.producer == self.producer
      && provenance.model_or_tool == self.model_or_tool
      && provenance.task_digest == self.task_digest
  }
}
