//! Pre-dispatch selection facts share the exact frozen input and ordinary Flow journal.
use crate::{AdmittedFlow, FactoryDigest, FactoryError, FactoryKey, FlowNodeInput, FlowProfileSelectionPolicy};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

/// Deterministic provenance within the immutable ordered rule list.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowProfileRuleOutcome {
  /// First true rule, identified by its zero-based position in the pinned policy.
  Matched {
    /// Position in the admitted rule list.
    rule: usize,
  },
  /// Explicit otherwise candidate after every predicate evaluated false.
  CatchAll,
  /// Every rule was false and no explicit catch-all was declared.
  NoMatch,
  /// A criterion could not be evaluated; recovery is owned by the failure-policy consumer.
  Failed {
    /// Safe typed reason; raw input and instruction content are never copied here.
    reason: crate::FlowPredicateFailure,
  },
}

/// One retained recovery disposition; it does not grant dispatch authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowProfileSelectionRecovery {
  /// The exact configured reserve passed the current capability check.
  ReserveProfile,
  /// The configured policy requires an operator choice.
  ManualSelection,
  /// The configured reserve cannot be qualified; wait without another fallback.
  ReserveUnavailable,
}

/// Immutable selection for one frozen node occurrence, before an execution attempt exists.
/// Its input digest binds the Run, nested Flow, cycle, generation, schemas and candidate closure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowProfileSelectionRecord {
  input_digest: FactoryDigest,
  candidate: Option<FactoryKey>,
  started_at: Timestamp,
  deadline: Timestamp,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  rules: Option<FlowProfileRuleOutcome>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  recovery: Option<FlowProfileSelectionRecovery>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionWire {
  input_digest: FactoryDigest,
  candidate: Option<FactoryKey>,
  started_at: Timestamp,
  deadline: Timestamp,
  #[serde(default)]
  rules: Option<FlowProfileRuleOutcome>,
  #[serde(default)]
  recovery: Option<FlowProfileSelectionRecovery>,
}
impl FlowProfileSelectionRecord {
  /// Freezes the manual policy's choice or pending state without executing a capability.
  pub fn manual(input: &FlowNodeInput, admitted: &AdmittedFlow, at: Timestamp) -> Result<Self, FactoryError> {
    if !matches!(
      admitted
        .closure()
        .definition(input.definition())
        .and_then(|d| d.node(input.node()))
        .and_then(|n| n.profile_selection())
        .map(|s| &s.policy),
      Some(FlowProfileSelectionPolicy::Manual { .. })
    ) {
      return Err(invalid());
    }
    Self::select(input, admitted, at)
  }
  /// Evaluates only the pinned policy over the exact frozen input, without execution authority.
  pub fn select(input: &FlowNodeInput, admitted: &AdmittedFlow, at: Timestamp) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    let node = admitted
      .closure()
      .definition(input.definition())
      .and_then(|definition| definition.node(input.node()))
      .ok_or_else(invalid)?;
    let selection = node.profile_selection().ok_or_else(invalid)?;
    let (candidate, rules) = match &selection.policy {
      FlowProfileSelectionPolicy::Manual { candidate } => (candidate.clone(), None),
      FlowProfileSelectionPolicy::Rules { rules, otherwise } => {
        let mut remaining = super::predicate::MAX_EVALUATION_STEPS;
        let time = serde_json::Value::from(at.unix_millis());
        let mut result = match otherwise {
          Some(candidate) => (Some(candidate.clone()), Some(FlowProfileRuleOutcome::CatchAll)),
          None => (None, Some(FlowProfileRuleOutcome::NoMatch)),
        };
        for (index, rule) in rules.iter().enumerate() {
          match rule
            .when
            .evaluate(input.payload().value(), None, Some(&time), &mut remaining)
          {
            Ok(true) => {
              result = (
                Some(rule.candidate.clone()),
                Some(FlowProfileRuleOutcome::Matched { rule: index }),
              );
              break;
            }
            Ok(false) => (),
            Err(reason) => {
              result = (None, Some(FlowProfileRuleOutcome::Failed { reason }));
              break;
            }
          }
        }
        result
      }
      FlowProfileSelectionPolicy::DecisionApi { .. } => return Err(invalid()),
    };
    let deadline = at
      .unix_millis()
      .checked_add(i64::try_from(node.budget().max_elapsed_millis()).map_err(|_| invalid())?)
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(invalid)?;
    Ok(Self {
      input_digest: input.digest()?,
      recovery: None,
      candidate,
      rules,
      started_at: at,
      deadline,
    })
  }
  /// Whether a retained automatic failure still needs its configured one-shot disposition.
  #[must_use]
  pub fn needs_recovery(&self) -> bool {
    self.recovery.is_none()
      && matches!(
        self.rules,
        Some(FlowProfileRuleOutcome::Failed { .. } | FlowProfileRuleOutcome::NoMatch)
      )
  }
  /// Retained failure disposition, absent for ordinary selections and older raw failures.
  #[must_use]
  pub const fn recovery(&self) -> Option<FlowProfileSelectionRecovery> {
    self.recovery
  }
  /// Applies the pinned failure policy once, preserving the original error and limits.
  /// Capability facts are consulted only when making a new reserve choice, never on replay.
  pub fn recover(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    choices: Option<&crate::FactoryConfigurationChoices>,
  ) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    if input.digest()? != self.input_digest {
      return Err(invalid());
    }
    if !self.needs_recovery() {
      return Ok(self.clone());
    }
    let binding = admitted
      .closure()
      .definition(input.definition())
      .and_then(|d| d.node(input.node()))
      .and_then(|n| n.profile_selection())
      .ok_or_else(invalid)?;
    let recovery = match &binding.failure {
      crate::FlowProfileSelectionFailure::ManualSelection => FlowProfileSelectionRecovery::ManualSelection,
      crate::FlowProfileSelectionFailure::ReserveProfile { candidate } => {
        if binding
          .candidates
          .get(candidate)
          .is_some_and(|reserve| choices.is_some_and(|choices| reserve.validate_capabilities(choices).is_ok()))
        {
          FlowProfileSelectionRecovery::ReserveProfile
        } else {
          FlowProfileSelectionRecovery::ReserveUnavailable
        }
      }
    };
    self.clone().apply_recovery(input, admitted, recovery)
  }
  fn apply_recovery(
    mut self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    recovery: FlowProfileSelectionRecovery,
  ) -> Result<Self, FactoryError> {
    if !self.needs_recovery() {
      return Err(invalid());
    }
    let binding = admitted
      .closure()
      .definition(input.definition())
      .and_then(|d| d.node(input.node()))
      .and_then(|n| n.profile_selection())
      .ok_or_else(invalid)?;
    self.candidate = match (&binding.failure, recovery) {
      (
        crate::FlowProfileSelectionFailure::ReserveProfile { candidate },
        FlowProfileSelectionRecovery::ReserveProfile,
      ) => Some(candidate.clone()),
      (crate::FlowProfileSelectionFailure::ManualSelection, FlowProfileSelectionRecovery::ManualSelection)
      | (crate::FlowProfileSelectionFailure::ReserveProfile { .. }, FlowProfileSelectionRecovery::ReserveUnavailable) => {
        None
      }
      _ => return Err(invalid()),
    };
    self.recovery = Some(recovery);
    Ok(self)
  }
  /// Exact frozen input identifying this selection's immutable stage occurrence.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
  /// Selected candidate, or `None` for pending manual input or a typed rule failure.
  #[must_use]
  pub const fn candidate(&self) -> Option<&FactoryKey> {
    self.candidate.as_ref()
  }
  /// Rule provenance, absent for retained manual selections.
  #[must_use]
  pub const fn rules(&self) -> Option<&FlowProfileRuleOutcome> {
    self.rules.as_ref()
  }
  /// Original selection start; polling and owner takeover do not renew it.
  #[must_use]
  pub const fn started_at(&self) -> Timestamp {
    self.started_at
  }
  /// Original bounded stage deadline, shared by subsequent execution.
  #[must_use]
  pub const fn deadline(&self) -> Timestamp {
    self.deadline
  }
  /// Canonical retained fact identity, independent of observer ownership.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
    Ok(FactoryDigest::sha256(
      "octacity.factory.profile-selection.v1",
      &[&bytes],
    ))
  }
  /// Restores exact content against its frozen input and admitted selection policy.
  pub fn restore(
    bytes: &[u8],
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    let wire: SelectionWire = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let mut value = Self::select(input, admitted, wire.started_at)?;
    if let Some(recovery) = wire.recovery {
      value = value.apply_recovery(input, admitted, recovery)?;
    }
    if wire.input_digest != value.input_digest
      || wire.candidate != value.candidate
      || wire.rules != value.rules
      || wire.deadline != value.deadline
      || value.digest()? != digest
    {
      return Err(invalid());
    }
    Ok(value)
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow profile selection record",
  }
}
