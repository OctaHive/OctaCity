//! Immutable installed Signal preparation, independent of Flow and node names.
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Operator-installed redaction, question and provider contract for one bounded call.
/// Contains private question instructions, so no Debug representation is provided.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryDecisionSignalRequestPlan {
  /// Exact provider/model/policy selection and rollout authority.
  pub profile: DecisionSignalProfile,
  /// Qualified immutable capability, separately checked against live discovery at dispatch.
  pub capability: DecisionSignalProviderCapability,
  /// Explicit questions from the exact installed question set.
  pub questions: Vec<DecisionSignalQuestion>,
  /// Finite question whose answer deterministic consumption may use.
  pub consuming_question: FactoryKey,
  /// Exact probability and calibration contract.
  pub threshold: DecisionSignalThreshold,
  /// Contract validating the complete projected state document.
  pub state_schema: FlowDataSchema,
  /// Selects only configured frozen payload fields; no implicit context or transcript.
  pub state_projection: FlowDataMapping,
  /// Hard call ceiling, no broader than either the profile or common Node budget.
  pub call_budget: BudgetLimit,
}
impl FactoryDecisionSignalRequestPlan {
  /// Prepares a stable canonical request without I/O or renewed dispatch authority.
  /// The same attempt and exact plan survive worker replacement; another attempt
  /// gets a distinct request. Durable authorization remains mandatory before I/O.
  pub fn prepare(
    &self,
    input: &FlowNodeInput,
    node: &NodeAttempt,
  ) -> Result<DecisionSignalProviderRequest, FactoryError> {
    if !self.call_budget.fits_within(node.budget()) {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "common Node call budget",
      });
    }
    let provider_input = self.project_input(input)?;
    let plan = serde_json::to_vec(self).map_err(|_| FactoryError::InvalidDecisionSignal { field: "request plan" })?;
    if plan.len() > MAX_FLOW_DATA_BYTES {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "request plan size",
      });
    }
    let identity = FactoryDigest::sha256(
      "octacity.factory.signal-node-request.v1",
      &[&input.digest()?.as_bytes(), &plan],
    );
    let id = DecisionSignalRequestId::from_uuid(Uuid::new_v5(&node.id().as_uuid(), &identity.as_bytes()))?;
    let request = DecisionSignalRequest::new(
      id,
      input,
      node,
      self.profile.purpose(),
      DecisionSignalDigests::new(provider_input.digest(), self.profile.policy().digest()),
      self.call_budget,
    )?;
    self.finish(request, provider_input, node.started_at(), node.deadline())
  }

  /// Prepares the same bounded provider contract before any execution attempt exists.
  /// The caller retains it under the Flow fence before authorization can reserve a call.
  /// Exact installed-policy matching belongs to the purpose-specific consumer;
  /// this helper only prepares immutable data and grants no dispatch authority.
  pub fn prepare_profile_selection(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    selection: &FlowProfileSelectionRecord,
  ) -> Result<DecisionSignalProviderRequest, FactoryError> {
    FlowProfileSelectionRecord::restore(
      &serde_json::to_vec(selection).map_err(|_| FactoryError::InvalidDecisionSignal { field: "selection" })?,
      input,
      admitted,
      selection.digest()?,
    )?;
    let node = admitted
      .closure()
      .definition(input.definition())
      .and_then(|d| d.node(input.node()))
      .ok_or(FactoryError::InvalidDecisionSignal {
        field: "selection node",
      })?;
    let binding = node.profile_selection().ok_or(FactoryError::InvalidDecisionSignal {
      field: "selection binding",
    })?;
    binding.validate_signal_budget(self.call_budget)?;
    if !matches!(&binding.policy, FlowProfileSelectionPolicy::DecisionApi { input, .. }
      if input == &self.state_projection)
      || selection.input_digest() != input.digest()?
      || self.profile.purpose() != DecisionSignalPurpose::ProfileSelection
      || !self.call_budget.fits_within(node.budget())
    {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "profile request plan",
      });
    }
    let provider_input = self.project_input(input)?;
    let plan = serde_json::to_vec(self).map_err(|_| FactoryError::InvalidDecisionSignal { field: "request plan" })?;
    if plan.len() > MAX_FLOW_DATA_BYTES {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "request plan size",
      });
    }
    let identity = FactoryDigest::sha256(
      "octacity.factory.profile-signal-request.v1",
      &[&input.digest()?.as_bytes(), &plan],
    );
    let id = DecisionSignalRequestId::from_uuid(Uuid::new_v5(&input.flow_run_id().as_uuid(), &identity.as_bytes()))?;
    let request = DecisionSignalRequest::profile_selection(
      id,
      input,
      DecisionSignalDigests::new(provider_input.digest(), self.profile.policy().digest()),
      self.call_budget,
    )?;
    self.finish(request, provider_input, selection.started_at(), selection.deadline())
  }

  fn project_input(&self, input: &FlowNodeInput) -> Result<DecisionSignalProviderInput, FactoryError> {
    let projected = self.state_projection.project(input.payload(), &self.state_schema)?;
    let state = serde_json::to_string(projected.value()).map_err(|_| FactoryError::InvalidDecisionSignal {
      field: "state projection",
    })?;
    DecisionSignalProviderInput::new(
      DecisionSignalInputMedia::CanonicalJson,
      FactoryText::new(state)?,
      self.questions.clone(),
      self.consuming_question.clone(),
      self.profile.routes().map(|routes| routes.state().clone()),
    )
  }

  fn finish(
    &self,
    request: DecisionSignalRequest,
    provider_input: DecisionSignalProviderInput,
    started_at: octacity_server_domain::Timestamp,
    deadline: octacity_server_domain::Timestamp,
  ) -> Result<DecisionSignalProviderRequest, FactoryError> {
    let call_end = started_at
      .unix_millis()
      .checked_add(
        i64::try_from(self.call_budget.max_elapsed_millis())
          .map_err(|_| FactoryError::InvalidDecisionSignal { field: "call deadline" })?,
      )
      .and_then(|value| octacity_server_domain::Timestamp::from_unix_millis(value).ok())
      .ok_or(FactoryError::InvalidDecisionSignal { field: "call deadline" })?;
    DecisionSignalProviderRequest::new(
      request,
      &self.profile,
      &self.capability,
      provider_input,
      self.threshold.clone(),
      deadline.min(call_end),
    )
  }
}
