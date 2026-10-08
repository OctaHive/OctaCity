use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use uuid::Uuid;

use crate::{FactoryEntityKind, FactoryError};

macro_rules! factory_id {
  ($name:ident, $entity:expr, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(Uuid);

    impl $name {
      /// Generates a new random opaque identifier.
      #[must_use]
      pub fn generate() -> Self {
        Self(Uuid::new_v4())
      }

      /// Constructs an identifier from a non-nil UUID.
      pub fn from_uuid(value: Uuid) -> Result<Self, FactoryError> {
        if value.is_nil() {
          return Err(FactoryError::InvalidIdentifier { entity: $entity });
        }
        Ok(Self(value))
      }

      /// Returns the UUID without assigning it additional semantics.
      #[must_use]
      pub const fn as_uuid(self) -> Uuid {
        self.0
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.hyphenated())
      }
    }

    impl FromStr for $name {
      type Err = FactoryError;

      fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = Uuid::parse_str(value).map_err(|_| FactoryError::InvalidIdentifier { entity: $entity })?;
        if parsed.is_nil() || parsed.hyphenated().to_string() != value {
          return Err(FactoryError::InvalidIdentifier { entity: $entity });
        }
        Ok(Self(parsed))
      }
    }

    impl Serialize for $name {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
        S: Serializer,
      {
        serializer.collect_str(self)
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
      }
    }
  };
}

factory_id!(
  FactoryConfigurationId,
  FactoryEntityKind::FactoryConfiguration,
  "Stable identity shared by immutable Factory Configuration versions."
);
factory_id!(
  WorkEnvelopeId,
  FactoryEntityKind::WorkEnvelope,
  "Opaque identity of one admitted immutable Work Envelope."
);
factory_id!(
  FactoryRunId,
  FactoryEntityKind::FactoryRun,
  "Opaque identity of one durable Factory Run."
);
factory_id!(
  StageAttemptId,
  FactoryEntityKind::StageAttempt,
  "Opaque identity of one append-only Factory Stage Attempt."
);
factory_id!(
  MacroCallId,
  FactoryEntityKind::MacroCall,
  "Opaque identity of one bounded macro reasoning call."
);
factory_id!(
  TaskEnvelopeId,
  FactoryEntityKind::TaskEnvelope,
  "Opaque identity of one immutable versioned Factory Task Envelope."
);
factory_id!(
  StageHandoffId,
  FactoryEntityKind::StageHandoff,
  "Opaque identity of one immutable completed-stage handoff."
);
factory_id!(
  ContextManifestId,
  FactoryEntityKind::ContextManifest,
  "Opaque identity of one immutable ordered call-context manifest."
);
factory_id!(
  RetrievalReceiptId,
  FactoryEntityKind::RetrievalReceipt,
  "Opaque identity of one immutable revision-bound repository retrieval receipt."
);
factory_id!(
  DecisionSignalRequestId,
  FactoryEntityKind::DecisionSignalRequest,
  "Opaque identity of one durable Decision Signal request."
);
factory_id!(
  DecisionSignalReceiptId,
  FactoryEntityKind::DecisionSignalReceipt,
  "Opaque identity of one immutable Decision Signal receipt."
);
factory_id!(
  ChangeSetId,
  FactoryEntityKind::ChangeSet,
  "Opaque identity of one immutable candidate ChangeSet."
);
factory_id!(
  EvidenceManifestId,
  FactoryEntityKind::EvidenceManifest,
  "Opaque identity of one immutable Evidence Manifest."
);
factory_id!(
  EvaluationPlanId,
  FactoryEntityKind::EvaluationPlan,
  "Opaque identity of one immutable Evaluation Plan."
);
factory_id!(
  AssessmentId,
  FactoryEntityKind::Assessment,
  "Opaque identity of one immutable evaluator Assessment."
);
factory_id!(
  DecisionId,
  FactoryEntityKind::Decision,
  "Opaque identity of one deterministic candidate Decision."
);
factory_id!(
  EscalationId,
  FactoryEntityKind::Escalation,
  "Opaque identity of one Factory escalation."
);
factory_id!(
  DeliveryAttemptId,
  FactoryEntityKind::DeliveryAttempt,
  "Opaque identity of one append-only delivery attempt."
);
factory_id!(
  ReportingAttemptId,
  FactoryEntityKind::ReportingAttempt,
  "Opaque identity of one append-only Work Reporter attempt."
);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn opaque_ids_reject_nil_and_noncanonical_text() {
    assert_eq!(
      FactoryRunId::from_uuid(Uuid::nil()),
      Err(FactoryError::InvalidIdentifier {
        entity: FactoryEntityKind::FactoryRun,
      })
    );
    assert_eq!(
      "550E8400-E29B-41D4-A716-446655440000".parse::<FactoryRunId>(),
      Err(FactoryError::InvalidIdentifier {
        entity: FactoryEntityKind::FactoryRun,
      })
    );
  }
}
