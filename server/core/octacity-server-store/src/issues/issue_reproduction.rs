//! Shared meaning of reproduction choice IDs; canonical Project property values remain strings.

/// Preliminary reproduction vocabulary, independent of Project choice display names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueReproductionClaim {
  /// Independently observed reproduction of the reported behavior.
  Reproduced,
  /// Independently observed absence of the reported behavior under the selected criterion.
  NotReproduced,
  /// No reproduction outcome is known.
  Unknown,
  /// Additional input is required.
  NeedsInput,
  /// Investigation remains necessary.
  NeedsResearch,
  /// Reproduction does not apply to this Issue.
  NotApplicable,
}

impl IssueReproductionClaim {
  /// Interprets a stable choice ID; unsupported IDs establish no reproduction claim.
  pub fn from_choice(value: &str) -> Option<Self> {
    match value {
      "reproduced" => Some(Self::Reproduced),
      "not_reproduced" => Some(Self::NotReproduced),
      "unknown" => Some(Self::Unknown),
      "needs_input" => Some(Self::NeedsInput),
      "needs_research" => Some(Self::NeedsResearch),
      "not_applicable" => Some(Self::NotApplicable),
      _ => None,
    }
  }

  /// Observed outcomes require independently qualified evidence; unresolved routes do not.
  pub const fn requires_verified_evidence(self) -> bool {
    matches!(self, Self::Reproduced | Self::NotReproduced)
  }
}
