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

/// Immutable predecessor, publication time and optional operator authority for one revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectionRevision {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  control_id: Option<FactoryDigest>,
  pending_digest: FactoryDigest,
  chosen_at: Timestamp,
  #[serde(default = "first_generation", skip_serializing_if = "is_first_generation")]
  generation: u32,
}
fn first_generation() -> u32 {
  1
}
fn is_first_generation(value: &u32) -> bool {
  *value == 1
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
  #[serde(default, skip_serializing_if = "Option::is_none")]
  manual: Option<SelectionRevision>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  automatic: Option<SelectionRevision>,
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
  #[serde(default)]
  manual: Option<SelectionRevision>,
  #[serde(default)]
  automatic: Option<SelectionRevision>,
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
    let (candidate, rules) = if let Some(entry) = input.profile_override() {
      let candidate = selection
        .candidates
        .iter()
        .find(|(_, candidate)| candidate.profile.reference() == &entry.profile)
        .map(|(key, _)| key.clone())
        .ok_or_else(invalid)?;
      (Some(candidate), None)
    } else {
      match &selection.policy {
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
      }
    };
    let deadline = at
      .unix_millis()
      .checked_add(i64::try_from(node.budget().max_elapsed_millis()).map_err(|_| invalid())?)
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(invalid)?;
    Ok(Self {
      input_digest: input.digest()?,
      recovery: None,
      manual: None,
      automatic: None,
      candidate,
      rules,
      started_at: at,
      deadline,
    })
  }
  /// Supersedes one undispatched selection while preserving the original stage limits.
  /// The enclosing store must verify the control link under the same Run fence.
  pub fn choose(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    profile: &crate::FactoryRoleProfileRef,
    control_id: FactoryDigest,
    at: Timestamp,
  ) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    if self.input_digest != input.digest()? || at < self.recorded_at() || at > self.deadline {
      return Err(invalid());
    }
    let binding = admitted
      .closure()
      .definition(input.definition())
      .and_then(|d| d.node(input.node()))
      .and_then(|n| n.profile_selection())
      .ok_or_else(invalid)?;
    let candidate = binding
      .candidates
      .iter()
      .find(|(_, c)| c.profile.reference() == profile)
      .map(|(key, _)| key.clone())
      .ok_or_else(invalid)?;
    let mut chosen = self.clone();
    chosen.candidate = Some(candidate);
    chosen.manual = Some(self.next_revision(Some(control_id), at)?);
    chosen.automatic = None;
    Ok(chosen)
  }
  /// Accepted control consumed by this owner fact, absent on the original policy fact.
  #[must_use]
  pub fn manual_control(&self) -> Option<FactoryDigest> {
    self.manual.as_ref().and_then(|m| m.control_id)
  }
  /// Selection generation, distinct from the frozen input execution generation.
  #[must_use]
  pub fn generation(&self) -> u32 {
    self.revision().map_or(0, |m| m.generation)
  }
  /// Exact immutable predecessor superseded by this revision.
  #[must_use]
  pub fn pending_digest(&self) -> Option<FactoryDigest> {
    self.revision().map(|m| m.pending_digest)
  }
  /// Publication time of this fact; the stage start and deadline are never renewed.
  #[must_use]
  pub fn recorded_at(&self) -> Timestamp {
    self.revision().map_or(self.started_at, |m| m.chosen_at)
  }
  fn revision(&self) -> Option<&SelectionRevision> {
    self.manual.as_ref().or(self.automatic.as_ref())
  }
  fn next_revision(&self, control_id: Option<FactoryDigest>, at: Timestamp) -> Result<SelectionRevision, FactoryError> {
    if at < self.recorded_at() || at > self.deadline {
      return Err(invalid());
    }
    Ok(SelectionRevision {
      control_id,
      pending_digest: self.digest()?,
      chosen_at: at,
      generation: self.generation().checked_add(1).ok_or_else(invalid)?,
    })
  }
  /// Publishes the existing one-shot recovery policy over an already retained raw failure.
  /// A current Run version/fence and absence of a newer control are checked by the store.
  pub fn recover_pending(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    choices: Option<&crate::FactoryConfigurationChoices>,
    at: Timestamp,
  ) -> Result<Self, FactoryError> {
    if !self.needs_recovery() || self.revision().is_some() {
      return Err(invalid());
    }
    let mut recovered = self.recover(input, admitted, choices)?;
    recovered.automatic = Some(self.next_revision(None, at)?);
    Ok(recovered)
  }
  pub(super) fn verify_predecessor(
    &self,
    previous: &Self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
  ) -> Result<(), FactoryError> {
    let expected = if self.automatic.is_some() {
      if !previous.needs_recovery() || previous.revision().is_some() {
        return Err(invalid());
      }
      let mut expected = previous
        .clone()
        .apply_recovery(input, admitted, self.recovery.ok_or_else(invalid)?)?;
      expected.automatic = Some(previous.next_revision(None, self.recorded_at())?);
      expected
    } else {
      let profile = admitted
        .closure()
        .definition(input.definition())
        .and_then(|d| d.node(input.node()))
        .and_then(|n| n.profile_selection())
        .and_then(|s| self.candidate.as_ref().and_then(|key| s.candidates.get(key)))
        .ok_or_else(invalid)?;
      previous.choose(
        input,
        admitted,
        profile.profile.reference(),
        self.manual_control().ok_or_else(invalid)?,
        self.recorded_at(),
      )?
    };
    if expected != *self {
      return Err(invalid());
    }
    Ok(())
  }
  /// Whether a retained automatic failure still needs its configured one-shot disposition.
  #[must_use]
  pub fn needs_recovery(&self) -> bool {
    self.manual.is_none()
      && self.recovery.is_none()
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
    if wire.manual.is_some() && wire.automatic.is_some() {
      return Err(invalid());
    }
    if let Some(manual) = &wire.manual {
      if manual.control_id.is_none()
        || manual.generation == 0
        || manual.chosen_at < value.started_at
        || manual.chosen_at > value.deadline
        || !admitted
          .closure()
          .definition(input.definition())
          .and_then(|d| d.node(input.node()))
          .and_then(|n| n.profile_selection())
          .is_some_and(|s| {
            wire
              .candidate
              .as_ref()
              .is_some_and(|key| s.candidates.contains_key(key))
          })
      {
        return Err(invalid());
      }
      // The complete journal checks the exact predecessor and accepted control.
      // Structural restoration cannot require history to arrive in digest order.
      value.candidate = wire.candidate.clone();
      value.manual = Some(manual.clone());
    }
    if let Some(automatic) = &wire.automatic {
      if automatic.control_id.is_some()
        || automatic.generation != 1
        || automatic.chosen_at < value.started_at
        || automatic.chosen_at > value.deadline
        || value.recovery.is_none()
      {
        return Err(invalid());
      }
      value.automatic = Some(automatic.clone());
    }
    if wire.automatic != value.automatic
      || wire.manual != value.manual
      || wire.input_digest != value.input_digest
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
