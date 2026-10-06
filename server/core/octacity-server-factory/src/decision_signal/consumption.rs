use std::collections::BTreeMap;

use crate::{
  BudgetUsage, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose, DecisionSignalReceiptId,
  DecisionSignalRequestId, DecisionSignalState, ExactSubject, FactoryDigest, FactoryError, FactoryKey, FactoryRunId,
  ImmutableReference,
};

use super::{
  contract::{DecisionSignalProviderObservation, DecisionSignalProviderRequest, DecisionSignalProviderResult},
  question::DecisionSignalChoices,
};

/// Deterministic disposition produced after consuming a non-authoritative signal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionSignalDisposition {
  /// Follow one finite route declared by the immutable request.
  Route(FactoryKey),
  /// Preserve the unchanged, already-authorized tool action.
  Allow,
  /// Deny routing or the proposed tool action.
  Deny,
  /// Require explicit policy or human escalation.
  Escalate,
}

/// How deterministic code used a provider observation relative to its baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionSignalConsumptionKind {
  /// Shadow mode recorded comparison evidence only.
  Shadow,
  /// Advisory mode retained the recommendation but executed the baseline.
  Advisory,
  /// Bounded control accepted a calibrated recommendation.
  BoundedControl,
  /// Bounded control applied its fail-closed fallback.
  Fallback,
}

/// Immutable deterministic outcome of consuming one provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalConsumption {
  baseline: DecisionSignalDisposition,
  recommendation: Option<DecisionSignalDisposition>,
  final_disposition: DecisionSignalDisposition,
  kind: DecisionSignalConsumptionKind,
}

impl DecisionSignalConsumption {
  /// Returns the deterministic policy result that would apply without bounded control.
  #[must_use]
  pub const fn baseline(&self) -> &DecisionSignalDisposition {
    &self.baseline
  }

  /// Returns the calibrated provider recommendation when one was usable.
  #[must_use]
  pub const fn recommendation(&self) -> Option<&DecisionSignalDisposition> {
    self.recommendation.as_ref()
  }

  /// Returns the code-owned disposition that callers may act on.
  #[must_use]
  pub const fn final_disposition(&self) -> &DecisionSignalDisposition {
    &self.final_disposition
  }

  /// Returns how rollout policy treated the signal.
  #[must_use]
  pub const fn kind(&self) -> DecisionSignalConsumptionKind {
    self.kind
  }
}

/// Mapping from finite provider answers to authority-narrowing tool dispositions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolRiskChoiceMapping {
  choices: DecisionSignalChoices,
  dispositions: BTreeMap<FactoryKey, DecisionSignalDisposition>,
}

impl ToolRiskChoiceMapping {
  /// Constructs a complete mapping that can only allow, deny, or escalate.
  pub fn try_new(
    choices: &DecisionSignalChoices,
    entries: Vec<(FactoryKey, DecisionSignalDisposition)>,
  ) -> Result<Self, FactoryError> {
    let mut mapping = BTreeMap::new();
    for (choice, disposition) in entries {
      if !matches!(
        disposition,
        DecisionSignalDisposition::Allow | DecisionSignalDisposition::Deny | DecisionSignalDisposition::Escalate
      ) || mapping.insert(choice, disposition).is_some()
      {
        return Err(FactoryError::InvalidDecisionSignal {
          field: "tool-risk choice mapping",
        });
      }
    }
    if mapping.len() != choices.as_slice().len()
      || choices.as_slice().iter().any(|choice| !mapping.contains_key(choice))
    {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "tool-risk choice mapping",
      });
    }
    Ok(Self {
      choices: choices.clone(),
      dispositions: mapping,
    })
  }

  fn disposition(&self, choice: &FactoryKey) -> Option<DecisionSignalDisposition> {
    self.dispositions.get(choice).cloned()
  }
}

/// Immutable receipt binding provider observation to its deterministic consuming disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalReceipt {
  id: DecisionSignalReceiptId,
  request: DecisionSignalProviderRequest,
  state: DecisionSignalState,
  usage: BudgetUsage,
  result: Option<DecisionSignalProviderResult>,
  consumption: DecisionSignalConsumption,
  receipt_digest: FactoryDigest,
}

impl DecisionSignalReceipt {
  /// Returns the immutable receipt identity.
  #[must_use]
  pub const fn id(&self) -> DecisionSignalReceiptId {
    self.id
  }

  /// Returns the stable logical request identity.
  #[must_use]
  pub const fn request_id(&self) -> DecisionSignalRequestId {
    self.request.id()
  }

  /// Returns the owning Factory Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.request.request().run_id()
  }

  /// Returns the exact immutable subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    self.request.request().subject()
  }

  /// Returns the exact provider-adapter identity frozen by the request.
  #[must_use]
  pub const fn provider(&self) -> &ImmutableReference {
    self.request.provider()
  }

  /// Returns the exact provider adapter identity frozen by the request.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    self.request.adapter()
  }

  /// Returns the exact model identity frozen by the request.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    self.request.model()
  }

  /// Returns the exact question-set identity.
  #[must_use]
  pub const fn question_set(&self) -> &ImmutableReference {
    self.request.question_set()
  }

  /// Returns the exact deterministic consumption-policy identity.
  #[must_use]
  pub const fn policy(&self) -> &ImmutableReference {
    self.request.policy()
  }

  /// Returns the provider/model-specific probability semantics.
  #[must_use]
  pub const fn probability_semantics(&self) -> &ImmutableReference {
    self.request.probability_semantics()
  }

  /// Returns the exact provider data-handling policy.
  #[must_use]
  pub const fn data_handling(&self) -> &ImmutableReference {
    self.request.data_handling()
  }

  /// Returns the rollout mode under which the observation was consumed.
  #[must_use]
  pub const fn mode(&self) -> DecisionSignalMode {
    self.request.mode()
  }

  /// Returns the fail-closed fallback frozen by the request.
  #[must_use]
  pub const fn fallback(&self) -> DecisionSignalFallback {
    self.request.fallback()
  }

  /// Returns the safe terminal provider classification.
  #[must_use]
  pub const fn state(&self) -> DecisionSignalState {
    self.state
  }

  /// Returns the schema-valid typed result, when one was accepted.
  #[must_use]
  pub const fn result(&self) -> Option<&DecisionSignalProviderResult> {
    self.result.as_ref()
  }

  /// Returns bounded usage for either a completed result or terminal failure.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Returns the exact immutable provider request captured by this receipt.
  #[must_use]
  pub const fn request(&self) -> &DecisionSignalProviderRequest {
    &self.request
  }

  /// Returns the deterministic consuming disposition.
  #[must_use]
  pub const fn consumption(&self) -> &DecisionSignalConsumption {
    &self.consumption
  }

  /// Returns the exact canonical receipt digest.
  #[must_use]
  pub const fn receipt_digest(&self) -> FactoryDigest {
    self.receipt_digest
  }

  /// Replays the recorded disposition only when every immutable request input still agrees.
  pub fn replay(&self, request: &DecisionSignalProviderRequest) -> Result<&DecisionSignalConsumption, FactoryError> {
    if self.request != *request {
      return Err(FactoryError::InvalidReference {
        relationship: "Decision Signal receipt replay",
      });
    }
    Ok(&self.consumption)
  }
}

/// Consumes a routing signal without permitting an undeclared route or direct transition.
pub fn consume_routing_signal(
  id: DecisionSignalReceiptId,
  request: &DecisionSignalProviderRequest,
  observation: DecisionSignalProviderObservation,
  baseline_route: FactoryKey,
) -> Result<DecisionSignalReceipt, FactoryError> {
  if request.request().purpose() != DecisionSignalPurpose::Routing || !request.choices().contains(&baseline_route) {
    return Err(FactoryError::InvalidDecisionSignal {
      field: "routing baseline",
    });
  }
  let normalized = normalize_observation(request, observation);
  let recommendation = normalized
    .result
    .as_ref()
    .and_then(|result| result.consuming_choice(request))
    .filter(|(_, selected, runner_up)| request.threshold().accepts(*selected, *runner_up))
    .map(|(selected, _, _)| DecisionSignalDisposition::Route(selected.clone()));
  Ok(build_receipt(
    id,
    request,
    normalized,
    DecisionSignalDisposition::Route(baseline_route),
    recommendation,
  ))
}

/// Consumes a tool-risk signal only after deterministic policy has authorized the unchanged action.
pub fn consume_tool_risk_signal(
  id: DecisionSignalReceiptId,
  request: &DecisionSignalProviderRequest,
  observation: DecisionSignalProviderObservation,
  mapping: &ToolRiskChoiceMapping,
) -> Result<DecisionSignalReceipt, FactoryError> {
  if request.request().purpose() != DecisionSignalPurpose::ToolRisk || mapping.choices != *request.choices() {
    return Err(FactoryError::InvalidDecisionSignal {
      field: "tool-risk purpose",
    });
  }
  let normalized = normalize_observation(request, observation);
  let recommendation = normalized
    .result
    .as_ref()
    .and_then(|result| result.consuming_choice(request))
    .filter(|(_, selected, runner_up)| request.threshold().accepts(*selected, *runner_up))
    .and_then(|(selected, _, _)| mapping.disposition(selected));
  Ok(build_receipt(
    id,
    request,
    normalized,
    DecisionSignalDisposition::Allow,
    recommendation,
  ))
}

struct NormalizedObservation {
  state: DecisionSignalState,
  usage: BudgetUsage,
  result: Option<DecisionSignalProviderResult>,
}

fn normalize_observation(
  request: &DecisionSignalProviderRequest,
  observation: DecisionSignalProviderObservation,
) -> NormalizedObservation {
  match observation {
    DecisionSignalProviderObservation::Result(result) if result.is_valid_for(request) => {
      let usage = result.usage();
      NormalizedObservation {
        state: DecisionSignalState::Completed,
        usage,
        result: Some(result),
      }
    }
    DecisionSignalProviderObservation::Failure(failure) if failure.is_valid_for(request) => NormalizedObservation {
      state: failure.state(),
      usage: failure.usage(),
      result: None,
    },
    DecisionSignalProviderObservation::Result(_) | DecisionSignalProviderObservation::Failure(_) => {
      NormalizedObservation {
        state: DecisionSignalState::Invalid,
        usage: BudgetUsage::default(),
        result: None,
      }
    }
  }
}

fn build_receipt(
  id: DecisionSignalReceiptId,
  request: &DecisionSignalProviderRequest,
  observation: NormalizedObservation,
  baseline: DecisionSignalDisposition,
  recommendation: Option<DecisionSignalDisposition>,
) -> DecisionSignalReceipt {
  let fallback = match request.fallback() {
    DecisionSignalFallback::Deny => DecisionSignalDisposition::Deny,
    DecisionSignalFallback::Escalate => DecisionSignalDisposition::Escalate,
  };
  let (final_disposition, kind) = match request.mode() {
    DecisionSignalMode::Shadow => (baseline.clone(), DecisionSignalConsumptionKind::Shadow),
    DecisionSignalMode::Advisory => (baseline.clone(), DecisionSignalConsumptionKind::Advisory),
    DecisionSignalMode::BoundedControl => match recommendation.clone() {
      Some(disposition) => (disposition, DecisionSignalConsumptionKind::BoundedControl),
      None => (fallback, DecisionSignalConsumptionKind::Fallback),
    },
  };
  let consumption = DecisionSignalConsumption {
    baseline,
    recommendation,
    final_disposition,
    kind,
  };
  let receipt_digest = digest_receipt(
    id,
    request,
    observation.state,
    observation.usage,
    &observation.result,
    &consumption,
  );
  DecisionSignalReceipt {
    id,
    request: request.clone(),
    state: observation.state,
    usage: observation.usage,
    result: observation.result,
    consumption,
    receipt_digest,
  }
}

fn digest_receipt(
  id: DecisionSignalReceiptId,
  request: &DecisionSignalProviderRequest,
  state: DecisionSignalState,
  usage: BudgetUsage,
  result: &Option<DecisionSignalProviderResult>,
  consumption: &DecisionSignalConsumption,
) -> FactoryDigest {
  let id = id.as_uuid();
  let request_digest = request.digest().as_bytes();
  let result_digest = result
    .as_ref()
    .map_or([0_u8; 32], |value| value.result_digest().as_bytes());
  let usage = format!(
    "{}\0{}\0{}\0{}\0{}",
    usage.attempts, usage.elapsed_millis, usage.tokens, usage.cost_micro_units, usage.output_bytes
  );
  let consumption = encode_consumption(consumption);
  FactoryDigest::sha256(
    "octacity.decision-signal.receipt.v1",
    &[
      id.as_bytes(),
      request_digest.as_slice(),
      state.as_str().as_bytes(),
      usage.as_bytes(),
      result_digest.as_slice(),
      consumption.as_slice(),
    ],
  )
}

fn encode_consumption(consumption: &DecisionSignalConsumption) -> Vec<u8> {
  let mut value = encode_disposition(consumption.baseline());
  value.push(0);
  if let Some(recommendation) = consumption.recommendation() {
    value.extend_from_slice(&encode_disposition(recommendation));
  }
  value.push(0);
  value.extend_from_slice(&encode_disposition(consumption.final_disposition()));
  value.push(0);
  value.extend_from_slice(match consumption.kind() {
    DecisionSignalConsumptionKind::Shadow => b"shadow",
    DecisionSignalConsumptionKind::Advisory => b"advisory",
    DecisionSignalConsumptionKind::BoundedControl => b"bounded_control",
    DecisionSignalConsumptionKind::Fallback => b"fallback",
  });
  value
}

fn encode_disposition(disposition: &DecisionSignalDisposition) -> Vec<u8> {
  match disposition {
    DecisionSignalDisposition::Route(route) => format!("route\0{route}").into_bytes(),
    DecisionSignalDisposition::Allow => b"allow".to_vec(),
    DecisionSignalDisposition::Deny => b"deny".to_vec(),
    DecisionSignalDisposition::Escalate => b"escalate".to_vec(),
  }
}
