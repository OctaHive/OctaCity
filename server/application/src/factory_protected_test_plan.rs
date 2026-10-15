//! Immutable installed-profile parameters for arbitrary acceptance-test classes.
use crate::ApplicationError;
use octacity_server_domain::Timestamp;
use octacity_server_factory::ImmutableReference;
use octacity_server_factory::{
  AcceptedWorkContract, FactoryKey, FlowBuildIntent, ProtectedTestCoverage, ProtectedTestPlan,
};
use octacity_server_store::FactoryRunStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Restores plan authority from persisted accepted ancestry and one pinned profile.
/// Composition must load `policy` from the verified immutable profile document;
/// arbitrary node names and current-stage pointers are never used for selection.
pub struct FactoryStoredProtectedTestPlanSource<S> {
  store: Arc<S>,
  profile: ImmutableReference,
  policy: FactoryProtectedTestPolicy,
}
impl<S> FactoryStoredProtectedTestPlanSource<S> {
  /// Installs authoritative history and the parameters of one immutable profile.
  pub fn new(store: Arc<S>, profile: ImmutableReference, policy: FactoryProtectedTestPolicy) -> Self {
    Self { store, profile, policy }
  }
}
#[async_trait::async_trait]
impl<S: FactoryRunStore> crate::FactoryProtectedTestPlanSource for FactoryStoredProtectedTestPlanSource<S> {
  async fn frozen_plan(&self, intent: &FlowBuildIntent) -> Result<ProtectedTestPlan, ApplicationError> {
    if intent.profile().model_or_tool != self.profile {
      return Err(ApplicationError::invalid());
    }
    let snapshot = self
      .store
      .factory_run_snapshot(intent.input().factory_run_id())
      .await
      .map_err(ApplicationError::from)?;
    if !snapshot.flow.data.build_intents.contains(intent) {
      return Err(ApplicationError::invalid());
    }
    let accepted = intent
      .input()
      .sources()
      .iter()
      .filter_map(|selected| {
        let record = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|row| row.node_attempt_id() == selected.node_attempt_id())?;
        let node = snapshot
          .admitted_flow
          .closure()
          .definition(record.input().definition())?
          .node(record.input().node())?;
        let completion = snapshot
          .flow
          .completions
          .iter()
          .find(|row| row.node_attempt_id() == record.node_attempt_id())?;
        node
          .accepted_work_outcomes()
          .contains(completion.outcome())
          .then_some(record.node_attempt_id())
      })
      .collect::<Vec<_>>();
    let [source] = accepted.as_slice() else {
      return Err(ApplicationError::invalid());
    };
    let contract = AcceptedWorkContract::freeze(
      &snapshot.work,
      &snapshot.admitted_flow,
      &snapshot.flow.data,
      snapshot.flow.runtime(),
      *source,
    )
    .map_err(|_| ApplicationError::invalid())?;
    freeze_factory_protected_test_plan(intent, &contract, &self.policy)
  }
}

/// Operator-selected coverage and protection limits in an immutable tool profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PolicyWire")]
pub struct FactoryProtectedTestPolicy {
  #[serde(flatten)]
  coverage: ProtectedTestCoverage,
  retention_millis: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyWire {
  classes: Vec<FactoryKey>,
  max_files: u16,
  max_bytes: u64,
  reserved_inputs: u16,
  retention_millis: u64,
}
impl TryFrom<PolicyWire> for FactoryProtectedTestPolicy {
  type Error = ApplicationError;
  fn try_from(w: PolicyWire) -> Result<Self, Self::Error> {
    Self::new(
      ProtectedTestCoverage::new(w.classes, w.max_files, w.max_bytes, w.reserved_inputs)
        .map_err(|_| ApplicationError::invalid())?,
      w.retention_millis,
    )
  }
}
impl FactoryProtectedTestPolicy {
  /// Returns the bounded classes, file/byte ceilings and reserved input capacity.
  #[must_use]
  pub const fn coverage(&self) -> &ProtectedTestCoverage {
    &self.coverage
  }

  /// Validates arbitrary class keys and bounded file, byte and retention parameters.
  pub fn new(coverage: ProtectedTestCoverage, retention_millis: u64) -> Result<Self, ApplicationError> {
    if retention_millis == 0 || retention_millis > i64::MAX as u64 {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      coverage,
      retention_millis,
    })
  }
}
/// Freezes the exact canonical accepted contract, subject and configured coverage.
/// The accepted source must have been explicitly selected by this input; a later
/// control dependency alone cannot introduce a contract or hidden context.
pub fn freeze_factory_protected_test_plan(
  intent: &FlowBuildIntent,
  contract: &AcceptedWorkContract,
  policy: &FactoryProtectedTestPolicy,
) -> Result<ProtectedTestPlan, ApplicationError> {
  let invalid = ApplicationError::invalid;
  let source = contract
    .records()
    .iter()
    .find(|record| record.node_attempt_id() == contract.source_record())
    .ok_or_else(invalid)?;
  if contract.work().id() != intent.input().work_id()
    || contract.work().subject() != intent.input().subject()
    || !intent.input().sources().iter().any(|reference| {
      reference.node_attempt_id() == contract.source_record() && source.digest().ok() == Some(reference.digest())
    })
  {
    return Err(invalid());
  }
  let subject = intent.input().context().ok_or_else(invalid)?.subject().clone();
  let retain_until = Timestamp::from_unix_millis(
    intent
      .node()
      .started_at()
      .unix_millis()
      .checked_add(policy.retention_millis as i64)
      .ok_or_else(invalid)?,
  )
  .map_err(|_| invalid())?;
  ProtectedTestPlan::new(
    contract.digest().map_err(|_| invalid())?,
    subject,
    policy.coverage.clone(),
    retain_until,
  )
  .map_err(|_| invalid())
}
