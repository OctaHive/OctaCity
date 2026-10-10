//! Immutable, configurable composition selected before Work admission.
use crate::{
  BudgetLimit, FactoryError, FactoryKey, FactoryWipLimits, FlowAdmissionLimits, PinnedFlowDefinitionClosure,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    };
    configuration.validate_generic()?;
    Ok(configuration)
  }
  fn validate_generic(&self) -> Result<(), FactoryError> {
    self.closure.clone().validate(self.limits.clone())?;
    crate::flow::validate_data_catalogue(&self.closure, &self.data_schemas)?;
    crate::phase_pool::validate_pool_settings(&self.closure, &self.limits, &self.pools)
  }
  /// Revalidates a publication against its enclosing Factory ceilings.
  pub fn validate(
    &self,
    _configuration: &crate::FactoryConfigurationRef,
    budget: BudgetLimit,
    wip: FactoryWipLimits,
  ) -> Result<(), FactoryError> {
    if !self.limits.budget().fits_within(budget) || u32::from(self.limits.max_wip()) > wip.max_active_stages() {
      return Err(FactoryError::InvalidConfiguration {
        field: "configured Flow bounds",
      });
    }
    self.validate_generic()
  }
}
