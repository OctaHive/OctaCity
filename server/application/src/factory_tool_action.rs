//! Protected authorization gate for untrusted coding-harness tool proposals.

use async_trait::async_trait;
use octacity_protocol::{
  FactoryPermissionSetV3, FactoryToolActionDecisionSourceV3, FactoryToolActionDecisionV3,
  FactoryToolActionDispositionV3, FactoryToolActionProposalV3,
};
use octacity_server_factory::{
  DecisionSignalDisposition, DecisionSignalFallback, DecisionSignalProviderRequest, DecisionSignalPurpose,
  DecisionSignalReceiptId, FactoryDigest, FactoryText, ToolRiskChoiceMapping,
};
use serde::Serialize;
use thiserror::Error;

use crate::{DecisionSignalApplicationError, DecisionSignalService};

/// Stable, secret-free failure while validating a protected action proposal.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ToolActionGateError {
  /// The untrusted proposal was malformed or exceeded a protocol bound.
  #[error("protected tool-action proposal is invalid")]
  InvalidProposal,
  /// Signed or local/backend permission input was malformed.
  #[error("protected tool-action permission ceiling is invalid")]
  InvalidPermissionCeiling,
  /// A secret-free decision record could not be constructed.
  #[error("protected tool-action decision is invalid")]
  InvalidDecision,
}

/// Deterministic result selected after the permission envelope is proven.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeterministicToolActionRule {
  /// Permit the in-envelope action without consulting a provider.
  Allow,
  /// Deny the in-envelope action without consulting a provider.
  Deny,
  /// Assess the in-envelope action through the configured bounded signal.
  Assess,
}

/// Canonical action retained privately while its digest is assessed.
pub struct PreparedToolAction {
  canonical: octacity_protocol::CanonicalFactoryToolActionV3,
  proposal_digest: FactoryDigest,
  state_document: FactoryText,
}

impl std::fmt::Debug for PreparedToolAction {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("PreparedToolAction")
      .field("proposal_digest", &self.proposal_digest)
      .finish_non_exhaustive()
  }
}

impl PreparedToolAction {
  /// Returns the exact digest used to bind a later Agent decision.
  #[must_use]
  pub const fn proposal_digest(&self) -> FactoryDigest {
    self.proposal_digest
  }

  /// Returns the canonical redacted document accepted for `tool_risk` input.
  #[must_use]
  pub const fn state_document(&self) -> &FactoryText {
    &self.state_document
  }
}

/// Immutable inputs for one optional tool-risk assessment.
pub struct ToolRiskAssessment<'a> {
  /// Identity for the immutable receipt published by the Decision Signal service.
  pub receipt_id: DecisionSignalReceiptId,
  /// Exact provider request whose redacted state must match the proposal.
  pub request: &'a DecisionSignalProviderRequest,
  /// Complete finite-choice mapping that cannot produce broader authority.
  pub mapping: &'a ToolRiskChoiceMapping,
}

/// Secret-free result returned after the signal service persists its receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolRiskSignalEvaluation {
  disposition: FactoryToolActionDispositionV3,
  receipt_digest: FactoryDigest,
}

impl ToolRiskSignalEvaluation {
  /// Constructs a result that can only preserve allow or narrow to deny/escalate.
  #[must_use]
  pub const fn new(disposition: FactoryToolActionDispositionV3, receipt_digest: FactoryDigest) -> Self {
    Self {
      disposition,
      receipt_digest,
    }
  }
}

/// Narrow application operation consumed by the protected action gate.
#[async_trait]
pub trait ToolRiskSignalEvaluator: Send + Sync {
  /// Assesses an already-authorized redacted action and durably consumes its result.
  async fn assess(
    &self,
    receipt_id: DecisionSignalReceiptId,
    request: &DecisionSignalProviderRequest,
    mapping: &ToolRiskChoiceMapping,
  ) -> Result<ToolRiskSignalEvaluation, DecisionSignalApplicationError>;
}

#[async_trait]
impl ToolRiskSignalEvaluator for DecisionSignalService {
  async fn assess(
    &self,
    receipt_id: DecisionSignalReceiptId,
    request: &DecisionSignalProviderRequest,
    mapping: &ToolRiskChoiceMapping,
  ) -> Result<ToolRiskSignalEvaluation, DecisionSignalApplicationError> {
    let receipt = self.assess_tool_risk(receipt_id, request, mapping).await?;
    let disposition = match receipt.consumption().final_disposition() {
      DecisionSignalDisposition::Allow => FactoryToolActionDispositionV3::Allow,
      DecisionSignalDisposition::Deny => FactoryToolActionDispositionV3::Deny,
      DecisionSignalDisposition::Escalate => FactoryToolActionDispositionV3::Escalate,
      DecisionSignalDisposition::Route(_) => FactoryToolActionDispositionV3::Deny,
    };
    Ok(ToolRiskSignalEvaluation::new(disposition, receipt.receipt_digest()))
  }
}

/// Applies deterministic permission policy before an optional non-authoritative signal.
pub struct ProtectedToolActionGate<'a> {
  signal: Option<&'a dyn ToolRiskSignalEvaluator>,
}

impl<'a> ProtectedToolActionGate<'a> {
  /// Creates a gate with an optional server-side signal operation.
  #[must_use]
  pub const fn new(signal: Option<&'a dyn ToolRiskSignalEvaluator>) -> Self {
    Self { signal }
  }

  /// Canonicalizes an untrusted proposal and creates only redacted provider input.
  pub fn prepare(proposal: FactoryToolActionProposalV3) -> Result<PreparedToolAction, ToolActionGateError> {
    let canonical = proposal
      .canonicalize()
      .map_err(|_| ToolActionGateError::InvalidProposal)?;
    let proposal_digest =
      FactoryDigest::from_lower_hex(&canonical.proposal_sha256()).map_err(|_| ToolActionGateError::InvalidProposal)?;
    let state_document = redacted_state_document(proposal_digest, canonical.redacted_summary())?;
    Ok(PreparedToolAction {
      canonical,
      proposal_digest,
      state_document,
    })
  }

  /// Applies signed and local/backend ceilings, then hard policy, then an optional signal.
  ///
  /// A provider is never called for an invalid or out-of-envelope action. Every
  /// returned decision contains only proposal and receipt digests; the Agent
  /// must still revalidate the unchanged proposal before permitting execution.
  pub async fn authorize(
    &self,
    prepared: &PreparedToolAction,
    signed_permissions: &FactoryPermissionSetV3,
    local_backend_permissions: &FactoryPermissionSetV3,
    deterministic_rule: DeterministicToolActionRule,
    assessment: Option<ToolRiskAssessment<'_>>,
  ) -> Result<FactoryToolActionDecisionV3, ToolActionGateError> {
    let inside_signed = prepared
      .canonical
      .is_permitted_by(signed_permissions)
      .map_err(|_| ToolActionGateError::InvalidPermissionCeiling)?;
    let inside_local = prepared
      .canonical
      .is_permitted_by(local_backend_permissions)
      .map_err(|_| ToolActionGateError::InvalidPermissionCeiling)?;
    if !inside_signed || !inside_local {
      return decision(
        prepared.proposal_digest,
        FactoryToolActionDispositionV3::Deny,
        FactoryToolActionDecisionSourceV3::HardPolicy,
        None,
      );
    }

    match deterministic_rule {
      DeterministicToolActionRule::Allow => decision(
        prepared.proposal_digest,
        FactoryToolActionDispositionV3::Allow,
        FactoryToolActionDecisionSourceV3::HardPolicy,
        None,
      ),
      DeterministicToolActionRule::Deny => decision(
        prepared.proposal_digest,
        FactoryToolActionDispositionV3::Deny,
        FactoryToolActionDecisionSourceV3::HardPolicy,
        None,
      ),
      DeterministicToolActionRule::Assess => self.assess(prepared, assessment).await,
    }
  }

  async fn assess(
    &self,
    prepared: &PreparedToolAction,
    assessment: Option<ToolRiskAssessment<'_>>,
  ) -> Result<FactoryToolActionDecisionV3, ToolActionGateError> {
    let Some(assessment) = assessment else {
      return fail_closed(prepared.proposal_digest, FactoryToolActionDispositionV3::Deny);
    };
    if assessment.request.request().purpose() != DecisionSignalPurpose::ToolRisk
      || assessment.request.state_document() != &prepared.state_document
    {
      return fail_closed(prepared.proposal_digest, FactoryToolActionDispositionV3::Deny);
    }
    let fallback = fallback(assessment.request.fallback());
    let Some(signal) = self.signal else {
      return fail_closed(prepared.proposal_digest, fallback);
    };
    match signal
      .assess(assessment.receipt_id, assessment.request, assessment.mapping)
      .await
    {
      Ok(evaluation) => decision(
        prepared.proposal_digest,
        evaluation.disposition,
        FactoryToolActionDecisionSourceV3::DecisionSignal,
        Some(evaluation.receipt_digest),
      ),
      Err(_) => fail_closed(prepared.proposal_digest, fallback),
    }
  }
}

#[derive(Serialize)]
struct RedactedToolActionState<'a> {
  schema: &'static str,
  proposal_sha256: String,
  summary: &'a octacity_protocol::FactoryToolActionSummaryV3,
}

fn redacted_state_document(
  proposal_digest: FactoryDigest,
  summary: octacity_protocol::FactoryToolActionSummaryV3,
) -> Result<FactoryText, ToolActionGateError> {
  let document = serde_json::to_value(&RedactedToolActionState {
    schema: "octacity.factory.tool-action.v1",
    proposal_sha256: proposal_digest.to_string(),
    summary: &summary,
  })
  .and_then(|document| serde_json::to_string(&document))
  .map_err(|_| ToolActionGateError::InvalidProposal)?;
  FactoryText::new(document).map_err(|_| ToolActionGateError::InvalidProposal)
}

fn fallback(value: DecisionSignalFallback) -> FactoryToolActionDispositionV3 {
  match value {
    DecisionSignalFallback::Deny => FactoryToolActionDispositionV3::Deny,
    DecisionSignalFallback::Escalate => FactoryToolActionDispositionV3::Escalate,
  }
}

fn fail_closed(
  proposal_digest: FactoryDigest,
  disposition: FactoryToolActionDispositionV3,
) -> Result<FactoryToolActionDecisionV3, ToolActionGateError> {
  decision(
    proposal_digest,
    disposition,
    FactoryToolActionDecisionSourceV3::FailClosed,
    None,
  )
}

fn decision(
  proposal_digest: FactoryDigest,
  disposition: FactoryToolActionDispositionV3,
  source: FactoryToolActionDecisionSourceV3,
  receipt_digest: Option<FactoryDigest>,
) -> Result<FactoryToolActionDecisionV3, ToolActionGateError> {
  FactoryToolActionDecisionV3::new(
    proposal_digest.to_string(),
    disposition,
    source,
    receipt_digest.map(|digest| digest.to_string()),
  )
  .map_err(|_| ToolActionGateError::InvalidDecision)
}
