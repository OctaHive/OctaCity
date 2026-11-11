//! Immutable, configurable composition selected before Work admission.
use crate::{
  BudgetLimit, FactoryError, FactoryKey, FactoryWipLimits, FlowAdmissionLimits, PinnedFlowDefinitionClosure,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Exact producing Flow and decisions allowed to start a downstream execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowAcceptedPredecessor {
  /// Immutable definition of the producing Flow.
  pub definition: crate::FlowDefinitionRef,
  /// Required completed terminal.
  pub terminal: FactoryKey,
  /// Declared successful outcomes eligible for this target.
  pub outcomes: BTreeSet<FactoryKey>,
}
/// Complete immutable graph, data catalogue and named queue policies.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryFlowConfiguration {
  /// Exact root and all reachable nested definitions.
  pub closure: PinnedFlowDefinitionClosure,
  /// Admission-wide resource and authority ceilings.
  pub limits: FlowAdmissionLimits,
  /// Exact contracts for configurable node inputs, outputs and verifier facts.
  pub data_schemas: Vec<crate::FlowDataSchema>,
  /// Arbitrary queue names, policies and explicit selection projections.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub pools: BTreeMap<FactoryKey, crate::FlowPoolSettings>,
  /// Required accepted upstream result for this configuration's incoming fields.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub accepted_predecessor: Option<FlowAcceptedPredecessor>,
  /// Explicit producer-independent Issue snapshot; omitted on all earlier publications.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub issue_snapshot: Option<crate::FlowIssueSnapshotInput>,
}
impl FactoryFlowConfiguration {
  /// Constructs a bounded composition; logical stage names have no execution meaning.
  pub fn new(
    closure: PinnedFlowDefinitionClosure,
    limits: FlowAdmissionLimits,
    mut data_schemas: Vec<crate::FlowDataSchema>,
    pools: BTreeMap<FactoryKey, crate::FlowPoolSettings>,
  ) -> Result<Self, FactoryError> {
    data_schemas.sort_by(|left, right| left.reference().cmp(right.reference()));
    let configuration = Self {
      closure,
      limits,
      data_schemas,
      pools,
      accepted_predecessor: None,
      issue_snapshot: None,
    };
    configuration.validate_generic()?;
    Ok(configuration)
  }
  fn validate_generic(&self) -> Result<(), FactoryError> {
    self.closure.clone().validate(self.limits.clone())?;
    crate::flow::validate_data_catalogue(&self.closure, &self.data_schemas)?;
    if let Some(input) = &self.issue_snapshot {
      input.validate_structure(self)?;
    }
    if let Some(policy) = &self.accepted_predecessor {
      let mut schemas = self.incoming_schemas();
      if policy.definition == self.closure.root()
        || policy.outcomes.is_empty()
        || policy.outcomes.len() > usize::from(crate::MAX_FLOW_FAN_OUT)
        || schemas
          .next()
          .is_none_or(|expected| schemas.any(|schema| schema != expected))
      {
        return Err(FactoryError::InvalidConfiguration {
          field: "accepted Flow predecessor",
        });
      }
    }
    crate::phase_pool::validate_pool_settings(&self.closure, &self.limits, &self.pools)
  }
  /// Pins the reviewed predecessor contract at publication.
  pub fn with_accepted_predecessor(mut self, predecessor: FlowAcceptedPredecessor) -> Result<Self, FactoryError> {
    self.accepted_predecessor = Some(predecessor);
    self.validate_generic()?;
    Ok(self)
  }
  /// Retains the reviewed Issue input declaration without changing node projection grammar.
  pub fn with_issue_snapshot(mut self, input: crate::FlowIssueSnapshotInput) -> Result<Self, FactoryError> {
    self.issue_snapshot = Some(input);
    self.validate_generic()?;
    Ok(self)
  }
  /// Returns exact contracts explicitly selected by incoming field bindings.
  pub fn incoming_schemas(&self) -> impl Iterator<Item = &crate::ImmutableReference> {
    self
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes())
      .filter_map(|node| node.input_binding())
      .flat_map(|binding| binding.incoming_schemas())
  }
  /// Revalidates a publication against its enclosing Factory ceilings.
  pub fn validate(
    &self,
    configuration: &crate::FactoryConfigurationRef,
    budget: BudgetLimit,
    wip: FactoryWipLimits,
  ) -> Result<(), FactoryError> {
    if let Some(violation) = self.limits.budget().first_excess(budget) {
      return Err(FactoryError::BudgetCeilingExceeded {
        context: Box::new(crate::FlowBudgetCeilingViolation {
          definition: self.closure.root(),
          node: None,
          ceiling: crate::FlowBudgetCeiling::Configuration,
          violation,
        }),
      });
    }
    if u32::from(self.limits.max_wip()) > wip.max_active_nodes() {
      return Err(FactoryError::ConfigurationNodeCapacityExceeded {
        requested: u32::from(self.limits.max_wip()),
        maximum: wip.max_active_nodes(),
      });
    }
    if self
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes())
      .any(|node| {
        node
          .profile_selection()
          .is_some_and(|selection| selection.role.project_id != configuration.project_id())
      })
    {
      return Err(FactoryError::InconsistentSubject);
    }
    self.validate_generic()
  }
}
