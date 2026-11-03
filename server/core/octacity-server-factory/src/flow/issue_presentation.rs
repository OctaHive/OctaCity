//! The Issue display allowlist retained on an immutable Flow outcome.

use crate::{
  FactoryKey, FactoryText, FlowDefinitionId, FlowDefinitionRef, FlowValueSchema, MAX_CONTEXT_ENTRY_BYTES,
  MAX_WORK_PRIORITY,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum UTF-8 bytes allowed in a displayed conclusion.
pub const MAX_ISSUE_OUTCOME_CONCLUSION_BYTES: usize = 1024;
/// Maximum UTF-8 bytes allowed in a custom card title.
pub const MAX_ISSUE_OUTCOME_TITLE_BYTES: usize = 128;
/// Maximum retained resource references per card.
pub const MAX_ISSUE_OUTCOME_REFERENCES: u16 = 8;

/// Allowed decisions in a triage Issue card.
pub const ISSUE_OUTCOME_TRIAGE_DECISIONS: &[&str] = &["eligible", "duplicate", "out_of_scope", "needs_input", "review"];
/// Allowed duplicate coverage states in a triage Issue card.
pub const ISSUE_OUTCOME_DUPLICATE_COVERAGE: &[&str] = &["complete", "partial", "unknown"];
/// Allowed verdicts in a review Issue card.
pub const ISSUE_OUTCOME_REVIEW_VERDICTS: &[&str] = &["approved", "changes_required", "needs_review"];

/// Versioned presentation of an accepted outcome on its associated Issue.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowIssuePresentation {
  /// Conclusion and decision, with optional coverage and assessment fields.
  TriageV1 {
    /// Explicit shared slot, or the built-in slot when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slot: Option<FactoryKey>,
  },
  /// Conclusion and a bounded list of retained Project documents.
  SpecificationsV1 {
    /// Explicit shared slot, or the built-in slot when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slot: Option<FactoryKey>,
  },
  /// Conclusion, verdict and a retained Issue review association.
  ReviewV1 {
    /// Explicit shared slot, or the built-in slot when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slot: Option<FactoryKey>,
  },
  /// A short conclusion and an explicit allowlist of retained references.
  CustomV1 {
    /// Explicit shared slot, or a stable Flow/node/outcome slot when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slot: Option<FactoryKey>,
    /// Human-facing card title.
    title: FactoryText,
    /// Required bounded text in the outcome's closed object schema.
    conclusion_field: FactoryKey,
    /// Fields that can supply links, independently authorized by their read owners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    references: Vec<FlowIssueReferenceBinding>,
  },
}

/// One typed resource selector in a custom card.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueReferenceBinding {
  /// Output field containing the reference.
  pub field: FactoryKey,
  /// Existing resource owner that interprets the retained reference.
  pub kind: FlowIssueReferenceKind,
}

/// Supported resource types. A binding never carries a caller-supplied URL.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowIssueReferenceKind {
  /// Exact retained Project input document.
  Document,
  /// Exact retained Build or Project Artifact.
  Artifact,
  /// Retained Issue code-review association digest.
  Review,
}

/// Stable identity of an Issue card, independent of Flow publication version.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FlowIssueSlot {
  /// Explicit or built-in slot shared across Flow definitions.
  Named(FactoryKey),
  /// Default custom slot scoped to a stable Flow and its declared outcome.
  Custom {
    /// Stable Flow identity.
    flow: FlowDefinitionId,
    /// Producing step key.
    node: FactoryKey,
    /// Declared outcome key.
    outcome: FactoryKey,
  },
}

impl std::fmt::Display for FlowIssueSlot {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Named(slot) => write!(formatter, "{slot}"),
      // Factory keys cannot contain colons, so every component and the namespace are unambiguous.
      Self::Custom { flow, node, outcome } => write!(formatter, "custom:{flow}:{node}:{outcome}"),
    }
  }
}

/// Exact declarations that can both publish to the same Issue slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowIssueSlotConflict {
  /// Stable slot shared by both declarations.
  pub slot: FlowIssueSlot,
  /// A pair of conflicting producers in canonical definition order.
  pub producers: [FlowIssueOutcomeLocation; 2],
}

/// One declared outcome in an exact Flow definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowIssueOutcomeLocation {
  /// Exact definition that owns this outcome.
  pub definition: FlowDefinitionRef,
  /// Declaring step key.
  pub node: FactoryKey,
  /// Declared outcome key.
  pub outcome: FactoryKey,
  /// Outcome position in the canonical definition.
  pub outcome_index: usize,
}

impl std::fmt::Display for FlowIssueSlotConflict {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let [first, second] = &self.producers;
    write!(
      formatter,
      "{} has ambiguous producers {}.{} and {}.{}",
      self.slot, first.node, first.outcome, second.node, second.outcome
    )
  }
}

/// Safe position of an incompatible Issue presentation in a published graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowIssuePresentationViolation {
  /// Exact enclosing definition.
  pub definition: FlowDefinitionRef,
  /// Declaring node in that definition.
  pub node: FactoryKey,
  /// Position in the canonical list of outcomes.
  pub outcome_index: usize,
  /// Field relative to the outcome or its resolved data schema.
  pub field: &'static str,
}

impl FlowIssuePresentation {
  /// Resolves the stable slot for an outcome on the exact producing step.
  #[must_use]
  pub fn slot(&self, flow: FlowDefinitionId, node: &FactoryKey, outcome: &FactoryKey) -> FlowIssueSlot {
    let (slot, default) = match self {
      Self::TriageV1 { slot } => (slot, "triage"),
      Self::SpecificationsV1 { slot } => (slot, "specifications"),
      Self::ReviewV1 { slot } => (slot, "review"),
      Self::CustomV1 { slot: Some(slot), .. } => return FlowIssueSlot::Named(slot.clone()),
      Self::CustomV1 { slot: None, .. } => {
        return FlowIssueSlot::Custom {
          flow,
          node: node.clone(),
          outcome: outcome.clone(),
        };
      }
    };
    FlowIssueSlot::Named(
      slot
        .clone()
        .unwrap_or_else(|| FactoryKey::new(default).expect("static Issue slot")),
    )
  }

  pub(super) fn validate(&self, shape: &FlowValueSchema) -> Result<(), &'static str> {
    let FlowValueSchema::Object { fields } = shape else {
      return Err("schema/shape");
    };
    let field = |name: &str| {
      fields
        .iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, field)| field)
    };
    let required = |name: &str| {
      field(name)
        .filter(|field| field.is_required())
        .map(|field| field.value())
    };
    let (conclusion, path) = match self {
      Self::CustomV1 { conclusion_field, .. } => (conclusion_field.as_str(), "issue_presentation/conclusion_field"),
      _ => ("conclusion", "schema/shape/fields/conclusion"),
    };
    check(
      required(conclusion).is_some_and(|shape| {
        FlowValueSchema::String {
          min_bytes: 1,
          max_bytes: MAX_ISSUE_OUTCOME_CONCLUSION_BYTES,
        }
        .covers(shape)
      }),
      path,
    )?;

    match self {
      Self::TriageV1 { .. } => {
        check(
          required("decision").is_some_and(|shape| finite_keys(shape, Some(ISSUE_OUTCOME_TRIAGE_DECISIONS))),
          "schema/shape/fields/decision",
        )?;
        check(
          field("duplicate_coverage")
            .is_none_or(|field| finite_keys(field.value(), Some(ISSUE_OUTCOME_DUPLICATE_COVERAGE))),
          "schema/shape/fields/duplicate_coverage",
        )?;
        check(
          field("priority").is_none_or(|field| {
            FlowValueSchema::Integer {
              minimum: 0,
              maximum: i64::from(MAX_WORK_PRIORITY),
            }
            .covers(field.value())
          }),
          "schema/shape/fields/priority",
        )?;
        for (name, path) in [
          ("size", "schema/shape/fields/size"),
          ("component", "schema/shape/fields/component"),
        ] {
          check(field(name).is_none_or(|field| finite_keys(field.value(), None)), path)?;
        }
        Ok(())
      }
      Self::SpecificationsV1 { .. } => check(
        matches!(required("documents"),
        Some(FlowValueSchema::Array { item, min_items, max_items })
          if *min_items > 0 && *max_items <= MAX_ISSUE_OUTCOME_REFERENCES
            && FlowIssueReferenceKind::Document.accepts_shape(item)),
        "schema/shape/fields/documents",
      ),
      Self::ReviewV1 { .. } => {
        check(
          required("verdict").is_some_and(|shape| finite_keys(shape, Some(ISSUE_OUTCOME_REVIEW_VERDICTS))),
          "schema/shape/fields/verdict",
        )?;
        check(
          required("review").is_some_and(|shape| FlowIssueReferenceKind::Review.accepts_shape(shape)),
          "schema/shape/fields/review",
        )
      }
      Self::CustomV1 {
        title,
        conclusion_field,
        references,
        ..
      } => {
        check(
          title.as_str().len() <= MAX_ISSUE_OUTCOME_TITLE_BYTES,
          "issue_presentation/title",
        )?;
        let mut selected = BTreeSet::from([conclusion_field]);
        check(
          references.len() <= usize::from(MAX_ISSUE_OUTCOME_REFERENCES)
            && references.iter().all(|binding| {
              selected.insert(&binding.field)
                && fields
                  .get(&binding.field)
                  .is_some_and(|field| binding.kind.accepts_shape(field.value()))
            }),
          "issue_presentation/references",
        )
      }
    }
  }
}

impl FlowIssueReferenceKind {
  fn accepts_shape(self, shape: &FlowValueSchema) -> bool {
    let exact_text = |length| FlowValueSchema::String {
      min_bytes: length,
      max_bytes: length,
    };
    match self {
      Self::Review => exact_text(64).covers(shape),
      Self::Document | Self::Artifact => {
        use crate::FlowFieldSchema;
        let fields = [
          ("artifact_id", exact_text(36)),
          ("content_digest", exact_text(64)),
          (
            "encoded_size",
            FlowValueSchema::Integer {
              minimum: 1,
              maximum: MAX_CONTEXT_ENTRY_BYTES as i64,
            },
          ),
        ]
        .into_iter()
        .map(|(key, value)| {
          (
            FactoryKey::new(key).expect("static reference field"),
            FlowFieldSchema::required(value),
          )
        })
        .collect();
        FlowValueSchema::Object { fields }.covers(shape)
      }
    }
  }
}

fn finite_keys(shape: &FlowValueSchema, allowed: Option<&[&str]>) -> bool {
  matches!(shape, FlowValueSchema::Enum { values } if values.iter().all(|value| {
    value.as_str().is_some_and(|key| {
      allowed.map_or_else(|| FactoryKey::new(key).is_ok(), |choices| choices.contains(&key))
    })
  }))
}

fn check(compatible: bool, field: &'static str) -> Result<(), &'static str> {
  compatible.then_some(()).ok_or(field)
}
