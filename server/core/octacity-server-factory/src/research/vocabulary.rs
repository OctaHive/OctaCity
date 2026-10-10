use crate::{FactoryDigest, FactoryError, FactoryKey, ImmutableReference, WorkKind};
use serde::{Deserialize, Serialize};

/// Version of the provider-neutral Research Flow contracts.
pub const RESEARCH_CONTRACT_VERSION: u16 = 2;
/// Maximum sources, observations, or narrative items in one research collection.
pub const MAX_RESEARCH_ITEMS: usize = 64;
/// Maximum bounded research attempts in one immutable plan.
pub const MAX_RESEARCH_ATTEMPTS: u32 = 16;
/// Product ceiling on one retained feature proposal.
pub const MAX_RESEARCH_PROPOSAL_BYTES: u64 = 1024 * 1024;
/// Maximum encoded bytes in one frozen research contract.
pub const MAX_RESEARCH_CONTRACT_BYTES: usize = 1024 * 1024;

/// Distinct defect research observations; none grants implementation authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectResearchOutcome {
  /// The exact failure was deterministically reproduced.
  Reproduced,
  /// Reproduction varies across bounded attempts.
  Intermittent,
  /// Reproduction depends on the frozen environment contract.
  EnvironmentSpecific,
  /// The bounded experiment did not reproduce the failure.
  CannotReproduce,
  /// Missing inputs require human clarification.
  NeedsHumanInput,
}

/// Finite observations consumed by deterministic research routing.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "outcome", deny_unknown_fields)]
pub enum ResearchOutcome {
  /// One defect reproduction observation.
  Defect(DefectResearchOutcome),
  /// A bounded feature proposal with frozen sources.
  FeatureProposal,
}

/// Finite successors permitted after research; implementation readiness is a later gate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchRoute {
  /// Author and independently review requirements.
  Requirements,
  /// Enter the Accepted Work Contract and protected test-authoring path.
  ProtectedTest,
  /// Verify existing behavior without dispatching implementation.
  Verification,
  /// Request human disposition.
  Escalation,
  /// Reject with retained evidence.
  Rejection,
  /// Resolve through a separately accepted deterministic terminal fact.
  TerminalResolution,
}

impl ResearchRoute {
  /// Complete stable route vocabulary.
  pub const ALL: [Self; 6] = [
    Self::Requirements,
    Self::ProtectedTest,
    Self::Verification,
    Self::Escalation,
    Self::Rejection,
    Self::TerminalResolution,
  ];

  /// Returns the exact declared Flow terminal key.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Requirements => "requirements",
      Self::ProtectedTest => "protected_test",
      Self::Verification => "verification",
      Self::Escalation => "escalation",
      Self::Rejection => "rejection",
      Self::TerminalResolution => "terminal_resolution",
    }
  }
}

/// Exact immutable input, result, and successor payload schemas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResearchSchema {
  /// Frozen Research Input.
  Input,
  /// Non-authoritative defect observations.
  DefectResult,
  /// Non-authoritative feature proposal.
  FeatureResult,
  /// Bounded research Stage Handoff.
  Handoff,
  /// Code-owned successor disposition.
  Decision,
}

impl ResearchSchema {
  /// Returns the supported schema version and its immutable contract identity.
  pub fn reference(self) -> Result<ImmutableReference, FactoryError> {
    let name = match self {
      Self::Input => "research.input",
      Self::DefectResult => "research.defect-result",
      Self::FeatureResult => "research.feature-result",
      Self::Handoff => "research.handoff",
      Self::Decision => "research.decision",
    };
    let version = format!("v{RESEARCH_CONTRACT_VERSION}");
    Ok(ImmutableReference::new(
      FactoryKey::new(name)?,
      FactoryKey::new(&version)?,
      FactoryDigest::sha256(
        "octacity.factory.research-schema.v1",
        &[name.as_bytes(), version.as_bytes()],
      ),
    ))
  }

  /// Selects the typed observation schema for the admitted Work kind.
  #[must_use]
  pub const fn result(kind: WorkKind) -> Self {
    match kind {
      WorkKind::Defect => Self::DefectResult,
      WorkKind::FeatureRequest => Self::FeatureResult,
    }
  }
}
