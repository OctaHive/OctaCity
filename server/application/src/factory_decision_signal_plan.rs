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
    let projected = self.state_projection.project(input.payload(), &self.state_schema)?;
    let state = serde_json::to_string(projected.value()).map_err(|_| FactoryError::InvalidDecisionSignal {
      field: "state projection",
    })?;
    let provider_input = DecisionSignalProviderInput::new(
      DecisionSignalInputMedia::CanonicalJson,
      FactoryText::new(state)?,
      self.questions.clone(),
      self.consuming_question.clone(),
      self.profile.routes().map(|routes| routes.state().clone()),
    )?;
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
    let call_end = node
      .started_at()
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
      node.deadline().min(call_end),
    )
  }
}
