//! Private provider operations retained by the ordinary pre-dispatch selection journal.
use crate::*;
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

/// Exact operation and bounded reservation, independent of execution attempts and worker ownership.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProfileSelectionSignal {
  request: DecisionSignalProviderRequest,
  baseline: FactoryKey,
  reserved: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  unobservable: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  receipt: Option<DecisionSignalReceipt>,
}
impl std::fmt::Debug for FlowProfileSelectionSignal {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FlowProfileSelectionSignal")
      .field("request_id", &self.request.id())
      .field("reserved", &self.reserved)
      .field("terminal", &self.receipt.is_some())
      .finish()
  }
}
impl FlowProfileSelectionSignal {
  /// Retained redacted request. It must never be reconstructed from current context on replay.
  pub const fn request(&self) -> &DecisionSignalProviderRequest {
    &self.request
  }
  /// Explicit deterministic baseline for shadow/advisory consumption.
  pub const fn baseline(&self) -> &FactoryKey {
    &self.baseline
  }
  /// A reservation authorizes one dispatch; its replay authorizes observation only.
  pub const fn reserved(&self) -> bool {
    self.reserved
  }
  /// The original call cannot be observed; its full reservation stays charged, without a fabricated receipt.
  pub const fn unobservable(&self) -> bool {
    self.unobservable
  }
  /// Original terminal observation, including safe failure and measured cost.
  pub const fn receipt(&self) -> Option<&DecisionSignalReceipt> {
    self.receipt.as_ref()
  }
  pub(super) fn candidate(&self) -> Option<&FactoryKey> {
    if self.unobservable && self.request.mode() != DecisionSignalMode::BoundedControl {
      return Some(&self.baseline);
    }
    match self.receipt.as_ref()?.consumption().final_disposition() {
      DecisionSignalDisposition::Profile(candidate) => Some(candidate),
      _ => None,
    }
  }
  pub(super) fn failed(&self) -> bool {
    (self.unobservable || self.receipt.is_some()) && self.candidate().is_none()
  }
  pub(super) fn supersede(&mut self) {
    if self.reserved && self.receipt.is_none() {
      self.unobservable = true;
    }
  }
  fn usage(&self) -> BudgetUsage {
    if let Some(receipt) = &self.receipt {
      return BudgetUsage {
        attempts: 0,
        ..receipt.usage()
      };
    }
    if self.unobservable {
      return self.reserved_usage();
    }
    BudgetUsage::default()
  }
  fn reserved_usage(&self) -> BudgetUsage {
    let budget = self.request.request().budget();
    BudgetUsage {
      attempts: 0,
      elapsed_millis: budget.max_elapsed_millis(),
      tokens: budget.max_tokens(),
      cost_micro_units: budget.max_cost_micro_units().into(),
      output_bytes: budget.max_output_bytes(),
    }
  }
  pub(super) fn validate(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    selection: &FlowProfileSelectionRecord,
  ) -> Result<(), FactoryError> {
    self.request.validate_integrity()?;
    let node = admitted
      .closure()
      .definition(input.definition())
      .and_then(|d| d.node(input.node()))
      .ok_or_else(invalid)?;
    let binding = node.profile_selection().ok_or_else(invalid)?;
    binding.validate_signal_budget(self.request.request().budget())?;
    let FlowProfileSelectionPolicy::DecisionApi { input: projection, .. } = &binding.policy else {
      return Err(invalid());
    };
    let projected = projection.project_raw(input.payload().value())?;
    let state = serde_json::to_string(&projected).map_err(|_| invalid())?;
    let request = self.request.request();
    let call_end = selection
      .started_at()
      .unix_millis()
      .checked_add(i64::try_from(request.budget().max_elapsed_millis()).map_err(|_| invalid())?)
      .ok_or_else(invalid)?;
    if selection.input_digest() != input.digest()?
      || input.profile_override().is_some()
      || request.purpose() != DecisionSignalPurpose::ProfileSelection
      || request.frozen_input_digest() != input.digest()?
      || request.run_id() != input.factory_run_id()
      || request.subject() != input.subject()
      || self.request.input().media() != DecisionSignalInputMedia::CanonicalJson
      || self.request.state_document().as_str() != state
      || !request.budget().fits_within(node.budget())
      || self.request.deadline() > selection.deadline()
      || self.request.deadline().unix_millis() > call_end
      || self.request.deadline() <= selection.started_at()
      || !binding.matches_signal_answers(self.request.choices(), &self.baseline)
    {
      return Err(invalid());
    }
    if self.unobservable && (!self.reserved || self.receipt.is_some()) {
      return Err(invalid());
    }
    if let Some(receipt) = &self.receipt {
      receipt.replay(&self.request)?;
      if (!self.reserved && (receipt.usage() != BudgetUsage::default() || receipt.result().is_some()))
        || receipt.consumption().baseline() != &DecisionSignalDisposition::Profile(self.baseline.clone())
        || !matches!(
          receipt.consumption().final_disposition(),
          DecisionSignalDisposition::Profile(_) | DecisionSignalDisposition::Deny | DecisionSignalDisposition::Escalate
        )
        || self
          .candidate()
          .is_some_and(|key| !binding.candidates.contains_key(key))
      {
        return Err(invalid());
      }
    }
    Ok(())
  }
}
impl FlowProfileSelectionRecord {
  /// Recovers an absent installed connector using retained facts, never a replacement provider.
  pub fn unavailable_selector(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    at: Timestamp,
  ) -> Result<Self, FactoryError> {
    let Some(signal) = self.signal() else {
      return self.fail_selector(input, admitted, FlowProfileSelectionError::Unavailable, at);
    };
    if signal.reserved() {
      return self.mark_signal_unobservable(at);
    }
    let request = signal.request();
    let identity = FactoryDigest::sha256(
      "octacity.factory.unavailable-profile-receipt.v1",
      &[request.id().as_uuid().as_bytes()],
    );
    let receipt_id = DecisionSignalReceiptId::from_uuid(uuid::Uuid::from_bytes(
      identity.as_bytes()[..16].try_into().expect("digest contains a UUID"),
    ))?;
    let receipt = consume_profile_selection_signal(
      receipt_id,
      request,
      DecisionSignalProviderObservation::Failure(DecisionSignalProviderFailure::new(
        request.id(),
        request.provider().clone(),
        request.adapter().clone(),
        request.model().clone(),
        if at >= request.deadline() {
          DecisionSignalState::TimedOut
        } else {
          DecisionSignalState::Unavailable
        },
        BudgetUsage::default(),
      )?),
      signal.baseline().clone(),
    )?;
    self.consume_signal(input, admitted, receipt, at)
  }
  /// Provider resources charged once to the ordinary Run, separately from execution attempts.
  pub fn signal_usage(&self) -> BudgetUsage {
    self
      .signal()
      .map_or(BudgetUsage::default(), FlowProfileSelectionSignal::usage)
  }
  /// Unresolved ceiling, separate from measured or conservatively settled usage.
  pub fn pending_signal_usage(&self) -> BudgetUsage {
    self
      .signal()
      .filter(|s| s.reserved && s.receipt.is_none() && !s.unobservable)
      .map_or(BudgetUsage::default(), FlowProfileSelectionSignal::reserved_usage)
  }
  /// New measured resources relative to the preceding retained fact for the same stage.
  pub fn additional_signal_usage(&self, previous: Option<&Self>) -> Result<BudgetUsage, FactoryError> {
    let usage = self.signal_usage();
    let before = previous.map_or(BudgetUsage::default(), Self::signal_usage);
    usage.checked_sub(before).ok_or_else(invalid)
  }
  /// Retained provider operation, absent for rules and explicit manual selections.
  pub const fn signal(&self) -> Option<&FlowProfileSelectionSignal> {
    self.signal.as_ref()
  }
  /// Freezes the complete bounded redacted request before any external call.
  pub fn prepare_signal(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    request: DecisionSignalProviderRequest,
    baseline: FactoryKey,
    at: Timestamp,
  ) -> Result<Self, FactoryError> {
    if self.signal.is_some() || self.candidate().is_some() || self.generation() != 0 || self.recovery().is_some() {
      return Err(invalid());
    }
    let signal = FlowProfileSelectionSignal {
      request,
      baseline,
      reserved: false,
      receipt: None,
      unobservable: false,
    };
    signal.validate(input, admitted, self)?;
    let mut next = self.clone();
    next.signal = Some(signal);
    next.automatic = Some(self.next_revision(None, at)?);
    Ok(next)
  }
  /// Reserves the one immutable call. The enclosing journal supplies current claim/version fencing.
  pub fn reserve_signal(&self, at: Timestamp) -> Result<Self, FactoryError> {
    let signal = self.signal.as_ref().ok_or_else(invalid)?;
    if signal.reserved || signal.receipt.is_some() || self.manual_control().is_some() || at >= signal.request.deadline()
    {
      return Err(invalid());
    }
    let mut next = self.clone();
    next.signal.as_mut().ok_or_else(invalid)?.reserved = true;
    next.automatic = Some(self.next_revision(None, at)?);
    Ok(next)
  }
  /// Retains the purpose-specific receipt and its candidate, without authorizing a Build.
  pub fn consume_signal(
    &self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    receipt: DecisionSignalReceipt,
    at: Timestamp,
  ) -> Result<Self, FactoryError> {
    let signal = self.signal.as_ref().ok_or_else(invalid)?;
    if signal.unobservable || signal.receipt.is_some() || self.manual_control().is_some() || self.recovery().is_some() {
      return Err(invalid());
    }
    let mut next = self.clone();
    next.signal.as_mut().ok_or_else(invalid)?.receipt = Some(receipt);
    let signal = next.signal.as_ref().ok_or_else(invalid)?;
    signal.validate(input, admitted, self)?;
    next.candidate = signal.candidate().cloned();
    next.automatic = Some(self.next_revision(None, at)?);
    Ok(next)
  }
  /// Ends uncertain automatic selection without another query or invented measured usage.
  pub fn mark_signal_unobservable(&self, at: Timestamp) -> Result<Self, FactoryError> {
    let signal = self.signal.as_ref().ok_or_else(invalid)?;
    if !signal.reserved || signal.receipt.is_some() || signal.unobservable || self.manual_control().is_some() {
      return Err(invalid());
    }
    let mut next = self.clone();
    next.signal.as_mut().ok_or_else(invalid)?.unobservable = true;
    next.candidate = next.signal.as_ref().and_then(|s| s.candidate()).cloned();
    next.automatic = Some(self.next_revision(None, at)?);
    Ok(next)
  }
  pub(super) fn verify_signal_predecessor(
    &self,
    previous: &Self,
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
  ) -> Result<(), FactoryError> {
    let signal = self.signal.as_ref().ok_or_else(invalid)?;
    let expected = if previous.signal.is_none() {
      previous.prepare_signal(
        input,
        admitted,
        signal.request.clone(),
        signal.baseline.clone(),
        self.recorded_at(),
      )?
    } else if signal.unobservable {
      previous.mark_signal_unobservable(self.recorded_at())?
    } else if let Some(receipt) = &signal.receipt {
      previous.consume_signal(input, admitted, receipt.clone(), self.recorded_at())?
    } else {
      previous.reserve_signal(self.recorded_at())?
    };
    if &expected != self {
      return Err(invalid());
    }
    Ok(())
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidDecisionSignal {
    field: "profile selection operation",
  }
}
fn is_false(value: &bool) -> bool {
  !value
}
