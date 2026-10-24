//! Immutable candidate closure; evaluating a policy and retaining a choice belong to admission owners.
use crate::{BudgetUsage, FactoryKey, FactoryRoleProfile, FactoryRoleRef, FlowBuildBinding};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One complete exact profile and its ordinary Build execution contract.
/// Private profile content is retained deliberately; diagnostics expose only its identity.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProfileCandidate {
  /// Immutable role/profile snapshot, verified by the existing profile content digest.
  pub profile: FactoryRoleProfile,
  /// Ordinary execution references and stage result/capture contract.
  pub build: FlowBuildBinding,
}
impl std::fmt::Debug for FlowProfileCandidate {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FlowProfileCandidate")
      .field("profile", self.profile.reference())
      .field("build", &self.build)
      .finish()
  }
}

/// One ordered rule using the common bounded typed expression vocabulary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProfileSelectionRule {
  /// Condition over the frozen stage input; evaluation is owned by the selector.
  pub when: crate::FlowPredicate,
  /// Complete declared candidate selected when the condition matches.
  pub candidate: FactoryKey,
}

/// Selection instructions frozen before a Run is admitted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowProfileSelectionPolicy {
  /// Wait for an explicit choice from the declared candidates.
  Manual {
    /// Optional fixed choice; omission retains an explicit wait for operator input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    candidate: Option<FactoryKey>,
  },
  /// Ordered typed conditions with an optional explicit catch-all candidate.
  Rules {
    /// Finite first-match rule list; no executable expressions or provider names.
    rules: Vec<FlowProfileSelectionRule>,
    /// Explicit catch-all when every condition is false; absence produces a no-match failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    otherwise: Option<FactoryKey>,
  },
  /// Exact purpose-bound selector contract and explicit input projection.
  /// This declaration does not call a provider or authorize a Build.
  DecisionApi {
    /// Exact installed selection policy; independent from ordinary route classification.
    policy: crate::ImmutableReference,
    /// Bounded projected facts and frozen criteria for the selection call.
    input: crate::FlowDataMapping,
  },
}

/// Closed recovery behavior; failure never invents another profile or grants authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowProfileSelectionFailure {
  /// Require an explicit operator choice.
  ManualSelection,
  /// Use one exact declared candidate, without recursive fallback.
  ReserveProfile {
    /// Logical key in this immutable candidate map.
    candidate: FactoryKey,
  },
}

/// Optional alternative to a static Build binding on a model-backed stage.
/// All candidates share the node's schemas, authority and result/capture contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProfileSelectionBinding {
  /// Exact common role version; no mutable role or profile aliases are resolved at execution.
  pub role: FactoryRoleRef,
  /// Finite arbitrary candidate names, independent of providers and tier names.
  pub candidates: BTreeMap<FactoryKey, FlowProfileCandidate>,
  /// Immutable selection instructions.
  pub policy: FlowProfileSelectionPolicy,
  /// Explicit bounded recovery behavior.
  pub failure: FlowProfileSelectionFailure,
  /// Selection allowance added to each candidate's execution ceiling, never to all candidates together.
  pub selection_budget: BudgetUsage,
}

impl FlowProfileCandidate {
  /// Qualifies this exact profile using the shared installed-capability contract.
  pub fn validate_capabilities(&self, choices: &crate::FactoryConfigurationChoices) -> Result<(), crate::FactoryError> {
    if choices.supports_build_binding(&self.build)
      && choices.role_profile_capabilities().iter().any(|(alias, capability)| {
        choices
          .available_role_profile_capability(alias, capability)
          .is_some_and(|capability| capability.validate(&self.profile).is_ok())
      })
    {
      Ok(())
    } else {
      Err(invalid())
    }
  }
}

impl FlowProfileSelectionBinding {
  /// Checks the pre-admitted provider allowance; a selector call is not an execution attempt.
  pub fn validate_signal_budget(&self, budget: crate::BudgetLimit) -> Result<(), crate::FactoryError> {
    if budget.max_elapsed_millis() > self.selection_budget.elapsed_millis
      || budget.max_tokens() > self.selection_budget.tokens
      || budget.max_cost_micro_units() > self.selection_budget.cost_micro_units
      || budget.max_output_bytes() > self.selection_budget.output_bytes
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Qualifies every complete profile against the shared installed, authorized catalog.
  /// Static stages need no role catalog; retained admission does not resolve new versions.
  pub fn validate_capabilities(&self, choices: &crate::FactoryConfigurationChoices) -> Result<(), crate::FactoryError> {
    for candidate in self.candidates.values() {
      candidate.validate_capabilities(choices)?;
    }
    if let FlowProfileSelectionPolicy::DecisionApi { policy, .. } = &self.policy
      && let Some(selector) = choices.profile_selector_metadata(policy)
      && !self.matches_signal_answers(&selector.candidates, &selector.baseline)
    {
      return Err(invalid());
    }
    Ok(())
  }

  /// Publication and retained runtime operations must agree on the same finite answer domain.
  pub(super) fn matches_signal_answers(&self, answers: &crate::DecisionSignalChoices, baseline: &FactoryKey) -> bool {
    self.candidates.contains_key(baseline)
      && answers.as_slice().len() == self.candidates.len()
      && answers
        .as_slice()
        .iter()
        .all(|answer| self.candidates.contains_key(answer))
  }

  pub(super) fn validate_node(&self, node: &crate::FlowNodeDefinition) -> Result<(), crate::FactoryError> {
    if node.build().is_some() || node.kind() != crate::FlowNodeKind::Reasoning {
      return Err(invalid());
    }
    if self.candidates.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.candidates.values().enumerate().any(|(index, candidate)| {
        self
          .candidates
          .values()
          .take(index)
          .any(|prior| prior.profile.reference() == candidate.profile.reference())
      })
      || matches!(&self.failure, FlowProfileSelectionFailure::ReserveProfile { candidate } if !self.candidates.contains_key(candidate))
    {
      return Err(invalid());
    }
    if let FlowProfileSelectionPolicy::Manual {
      candidate: Some(candidate),
    } = &self.policy
      && !self.candidates.contains_key(candidate)
    {
      return Err(invalid());
    }
    if let FlowProfileSelectionPolicy::Rules { rules, otherwise } = &self.policy {
      if rules.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
        || serde_json::to_vec(&self.policy).map_err(|_| invalid())?.len() > crate::MAX_FLOW_GATE_PARAMETER_BYTES
        || otherwise
          .as_ref()
          .is_some_and(|candidate| !self.candidates.contains_key(candidate))
        || rules.iter().any(|rule| !self.candidates.contains_key(&rule.candidate))
      {
        return Err(invalid());
      }
      super::predicate::validate_predicates(rules.iter().map(|rule| &rule.when))?;
    }
    let first = self.candidates.values().next().ok_or_else(invalid)?;
    for candidate in self.candidates.values() {
      let profile = candidate.profile.definition();
      let build = &candidate.build;
      let execution = profile.budget;
      self
        .selection_budget
        .checked_add(BudgetUsage {
          attempts: execution.max_attempts(),
          elapsed_millis: execution.max_elapsed_millis(),
          tokens: execution.max_tokens(),
          cost_micro_units: execution.max_cost_micro_units(),
          output_bytes: execution.max_output_bytes(),
        })
        .ok_or_else(invalid)?
        .validate(node.budget())?;
      build.validate_node(node)?;
      if candidate.profile.role() != &self.role
        || build.tool != profile.toolchain.executable
        || build.plugin != profile.toolchain.plugin
        || build.model_or_tool != profile.toolchain.model
        || build.build_configuration != profile.build_configuration
        || build.result_output != first.build.result_output
        || build.result_output_kind != first.build.result_output_kind
        || build.result_outcome != first.build.result_outcome
        || build.result_tool() != first.build.result_tool()
        || build.capture_write_paths != first.build.capture_write_paths
        || build.capture_deliverables != first.build.capture_deliverables
        || build.evidence != first.build.evidence
        || profile.credential_purpose != first.profile.definition().credential_purpose
        || !profile.permissions.is_no_broader_than(node.permissions())
        || profile
          .context_requirements
          .iter()
          .any(|name| !node.context_binding().is_some_and(|context| context.guarantees(name)))
      {
        return Err(invalid());
      }
    }
    Ok(())
  }
}
fn invalid() -> crate::FactoryError {
  crate::FactoryError::InvalidConfiguration {
    field: "Flow profile selection",
  }
}
