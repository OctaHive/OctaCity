use serde::{Deserialize, Serialize};

use super::{DefectResearchOutcome, ResearchDetails, ResearchInput, invalid};
use crate::{ExactSubject, FactoryDigest, FactoryError};

/// One deterministic reproduction check made by the pinned command or tool.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectReproductionObservation {
  /// The declared regression check observed the defect.
  Reproduced,
  /// The declared regression check completed without observing the defect.
  NotReproduced,
  /// Required inputs were unavailable; this is not a negative reproduction.
  MissingInput,
}

/// One check and the exact environment descriptor used to execute it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefectReproductionCheck {
  /// Immutable environment descriptor content identity.
  pub environment_digest: FactoryDigest,
  /// Deterministic observation, independent of research prose.
  pub observation: DefectReproductionObservation,
}

/// Bounded reproduction report published as an ordinary Build output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefectReproductionReport {
  subject: ExactSubject,
  input_digest: FactoryDigest,
  environment_digest: FactoryDigest,
  checks: Vec<DefectReproductionCheck>,
}

impl DefectReproductionReport {
  /// Records actual checks for an exact frozen research input and environment.
  pub fn new(
    subject: ExactSubject,
    input_digest: FactoryDigest,
    environment_digest: FactoryDigest,
    checks: Vec<DefectReproductionCheck>,
  ) -> Result<Self, FactoryError> {
    if checks.is_empty() || checks.len() > super::MAX_RESEARCH_ITEMS {
      return Err(invalid("reproduction check bound"));
    }
    Ok(Self {
      subject,
      input_digest,
      environment_digest,
      checks,
    })
  }

  /// Derives a typed outcome from checks, never from a provider-selected route.
  pub fn classify(&self, input: &ResearchInput) -> Result<DefectResearchOutcome, FactoryError> {
    let ResearchDetails::Defect { environment, .. } = input.details() else {
      return Err(invalid("reproduction requires defect input"));
    };
    if self.subject != *input.subject()
      || self.input_digest != input.digest()?
      || self.environment_digest != environment.content_digest()
      || self.checks.is_empty()
      || self.checks.len() > super::MAX_RESEARCH_ITEMS
    {
      return Err(invalid("reproduction input binding"));
    }
    let baseline = |observation| {
      self
        .checks
        .iter()
        .any(|check| check.environment_digest == self.environment_digest && check.observation == observation)
    };
    use DefectReproductionObservation::{MissingInput, NotReproduced, Reproduced};
    // Missing inputs take precedence over negative results; a different
    // environment cannot substitute for the frozen baseline.
    Ok(
      if self.checks.iter().any(|check| check.observation == MissingInput)
        || !baseline(Reproduced) && !baseline(NotReproduced)
      {
        DefectResearchOutcome::NeedsHumanInput
      } else if baseline(Reproduced) && baseline(NotReproduced) {
        DefectResearchOutcome::Intermittent
      } else if baseline(Reproduced) {
        DefectResearchOutcome::Reproduced
      } else if self.checks.iter().any(|check| check.observation == Reproduced) {
        DefectResearchOutcome::EnvironmentSpecific
      } else {
        DefectResearchOutcome::CannotReproduce
      },
    )
  }
}
