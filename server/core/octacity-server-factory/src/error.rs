use thiserror::Error;

/// Stable classification of an entity owned by the Factory core.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FactoryEntityKind {
  /// Immutable Factory Configuration identity.
  FactoryConfiguration,
  /// Stable identity of a Project-owned Work Source input.
  WorkSourceInput,
  /// Stable identity of a Project-owned exclusive Work Source input group.
  WorkSourceGroup,
  /// Immutable admitted Work Envelope.
  WorkEnvelope,
  /// Durable Factory Run.
  FactoryRun,
  /// Stable immutable Flow Definition.
  FlowDefinition,
  /// Root or nested Flow Run.
  FlowRun,
  /// Append-only Flow node attempt.
  NodeAttempt,
  /// Append-only workflow cycle.
  WorkflowCycle,
  /// One immutable versioned Factory Task Envelope.
  TaskEnvelope,
  /// One immutable ordered call-context manifest.
  ContextManifest,
  /// One immutable revision-bound repository retrieval receipt.
  RetrievalReceipt,
  /// One Decision Signal request.
  DecisionSignalRequest,
  /// Immutable Decision Signal result receipt.
  DecisionSignalReceipt,
  /// Immutable source candidate ChangeSet.
  ChangeSet,
  /// Immutable deterministic evidence manifest.
  EvidenceManifest,
  /// Immutable evaluation plan.
  EvaluationPlan,
  /// Immutable evaluator assessment.
  Assessment,
  /// Deterministic candidate decision.
  Decision,
  /// Human or policy escalation.
  Escalation,
  /// Append-only delivery attempt.
  DeliveryAttempt,
  /// Append-only external reporting attempt.
  ReportingAttempt,
}

/// Stable classification of bounded textual Factory values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FactoryTextKind {
  /// Provider-scoped external work identity.
  ExternalWorkIdentity,
  /// Open provider-neutral key such as a connector or policy identity.
  Key,
  /// Human-readable summary or reason.
  Text,
  /// Durable model-authored text that must not contain credential-shaped material.
  SafeText,
  /// Metadata key.
  MetadataKey,
  /// Metadata value.
  MetadataValue,
}

/// Stable reason a bounded textual value was rejected.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextRejection {
  /// The value is empty.
  Empty,
  /// The UTF-8 byte length exceeds its declared limit.
  TooLong,
  /// Leading or trailing whitespace makes the value ambiguous.
  SurroundingWhitespace,
  /// The value contains a control character.
  ControlCharacter,
  /// The value contains a character outside its declared alphabet.
  InvalidCharacter,
  /// The first character is outside its declared alphabet.
  InvalidStart,
  /// The key may contain sensitive material and cannot be persisted as metadata.
  Sensitive,
}

/// Stable classification of open enums parsed at Factory seams.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FactoryEnumKind {
  /// Closed Factory Flow node kind.
  FlowNode,
  /// Factory Run state.
  RunState,
  /// Work risk class.
  Risk,
  /// Assessment outcome.
  AssessmentOutcome,
  /// Deterministic Decision outcome.
  DecisionOutcome,
  /// Typed evaluator finding severity.
  FindingSeverity,
  /// Deterministic evidence-gate outcome.
  DeterministicGateOutcome,
  /// Policy for evaluator indeterminate outcomes.
  IndeterminatePolicy,
  /// Delivery attempt state.
  DeliveryState,
  /// Reporting attempt state.
  ReportingState,
  /// Decision Signal purpose.
  DecisionSignalPurpose,
  /// Decision Signal terminal classification.
  DecisionSignalState,
  /// Decision Signal rollout mode.
  DecisionSignalMode,
  /// Fail-closed action used when a Decision Signal cannot be consumed.
  DecisionSignalFallback,
  /// Factory permission category advertised at an enforcement seam.
  PermissionCategory,
  /// Filesystem mount access mode.
  MountMode,
}

/// Stable category used while resolving a configuration alias.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryChoiceKind {
  /// Exact installed node execution capability.
  NodeCapability,
  /// Exact deployment execution policy.
  ExecutionPolicy,
  /// Exact supported execution target.
  ExecutionTarget,
  /// Exact semantic enforcement capability.
  EnforcementCapability,
  /// Logical credential reference, without secret material.
  SecretProfile,
  /// Logical workload identity reference, without credentials.
  WorkloadIdentityProfile,
  /// Immutable admission policy.
  AdmissionPolicy,
  /// Project-owned immutable Build Configuration version.
  BuildConfiguration,
  /// Immutable Factory permission ceiling.
  PermissionCeiling,
  /// Decision Signal service provider.
  DecisionSignalProvider,
  /// Decision Signal provider adapter.
  DecisionSignalAdapter,
  /// Exact Decision Signal model.
  DecisionSignalModel,
  /// Versioned Decision Signal question set.
  DecisionSignalQuestionSet,
  /// Versioned Decision Signal consumption policy.
  DecisionSignalPolicy,
  /// Immutable evaluation criterion pack.
  CriterionPack,
  /// Immutable evaluator connector selection.
  Evaluator,
  /// Versioned delivery adapter.
  DeliveryAdapter,
  /// Immutable delivery target policy.
  DeliveryPolicy,
}

impl FactoryChoiceKind {
  /// Complete technical choice categories supported by this release.
  pub const ALL: [Self; 18] = [
    Self::NodeCapability,
    Self::ExecutionPolicy,
    Self::ExecutionTarget,
    Self::EnforcementCapability,
    Self::SecretProfile,
    Self::WorkloadIdentityProfile,
    Self::AdmissionPolicy,
    Self::BuildConfiguration,
    Self::PermissionCeiling,
    Self::DecisionSignalProvider,
    Self::DecisionSignalAdapter,
    Self::DecisionSignalModel,
    Self::DecisionSignalQuestionSet,
    Self::DecisionSignalPolicy,
    Self::CriterionPack,
    Self::Evaluator,
    Self::DeliveryAdapter,
    Self::DeliveryPolicy,
  ];
  /// Stable category name for configuration choices and authorization identities.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::NodeCapability => "node_capability",
      Self::ExecutionPolicy => "execution_policy",
      Self::ExecutionTarget => "execution_target",
      Self::EnforcementCapability => "enforcement_capability",
      Self::SecretProfile => "secret_profile",
      Self::WorkloadIdentityProfile => "workload_identity_profile",
      Self::AdmissionPolicy => "admission_policy",
      Self::BuildConfiguration => "build_configuration",
      Self::PermissionCeiling => "permission_ceiling",
      Self::DecisionSignalProvider => "decision_signal_provider",
      Self::DecisionSignalAdapter => "decision_signal_adapter",
      Self::DecisionSignalModel => "decision_signal_model",
      Self::DecisionSignalQuestionSet => "decision_signal_question_set",
      Self::DecisionSignalPolicy => "decision_signal_policy",
      Self::CriterionPack => "criterion_pack",
      Self::Evaluator => "evaluator",
      Self::DeliveryAdapter => "delivery_adapter",
      Self::DeliveryPolicy => "delivery_policy",
    }
  }
}

/// Validation failure returned while constructing Factory domain values.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum FactoryError {
  /// An opaque Factory identifier is nil.
  #[error("invalid {entity:?} identifier")]
  InvalidIdentifier {
    /// Entity whose identifier was rejected.
    entity: FactoryEntityKind,
  },
  /// A bounded textual value is malformed.
  #[error("invalid {kind:?} text: {reason:?}")]
  InvalidText {
    /// Semantic kind of rejected text.
    kind: FactoryTextKind,
    /// Stable rejection reason.
    reason: TextRejection,
  },
  /// A serialized enum value is unknown to this release.
  #[error("unsupported {kind:?} value")]
  UnsupportedEnum {
    /// Enum whose value was not recognized.
    kind: FactoryEnumKind,
  },
  /// A SHA-256 digest is not canonical lowercase hexadecimal text.
  #[error("invalid factory digest")]
  InvalidDigest,
  /// A version or append-only attempt number is zero.
  #[error("factory version or attempt number must start at one")]
  InvalidSequence,
  /// A Work priority is outside the supported inclusive range.
  #[error("factory work priority is outside its declared bound")]
  InvalidPriority,
  /// Metadata repeats a key.
  #[error("factory metadata contains a duplicate key")]
  DuplicateMetadataKey,
  /// Metadata contains too many entries or exceeds its encoded byte bound.
  #[error("factory metadata exceeds its declared bound")]
  MetadataLimitExceeded,
  /// A hard budget is zero or exceeds a supported safety ceiling.
  #[error("factory budget contains an unsafe bound for {resource:?}")]
  InvalidBudget {
    /// Budget category whose bound is invalid.
    resource: crate::BudgetResource,
  },
  /// Recorded usage exceeds its immutable hard limit.
  #[error("factory budget usage exceeds {resource:?}")]
  BudgetExceeded {
    /// Budget category whose usage exceeded its limit.
    resource: crate::BudgetResource,
  },
  /// A related immutable record is absent, duplicated, or incompatible.
  #[error("factory record contains an invalid {relationship} reference")]
  InvalidReference {
    /// Stable relationship name safe to expose in diagnostics.
    relationship: &'static str,
  },
  /// Related records do not describe the same exact Project, Repository, base, or candidate.
  #[error("factory records describe inconsistent exact subjects")]
  InconsistentSubject,
  /// A bounded collection exceeds its declared item count.
  #[error("factory {collection} collection exceeds its declared bound")]
  CollectionLimitExceeded {
    /// Stable collection name safe to expose in diagnostics.
    collection: &'static str,
  },
  /// A configuration choice catalog repeats one alias.
  #[error("factory configuration contains a duplicate {kind:?} alias")]
  DuplicateChoiceAlias {
    /// Choice category containing the duplicate alias.
    kind: FactoryChoiceKind,
  },
  /// A configuration draft references an alias absent from its authorized catalog.
  #[error("factory configuration references an unknown {kind:?} alias")]
  UnknownChoiceAlias {
    /// Choice category that could not resolve the alias.
    kind: FactoryChoiceKind,
  },
  /// A cross-field Factory Configuration invariant is not satisfied.
  #[error("invalid factory configuration field: {field}")]
  InvalidConfiguration {
    /// Stable field or relationship name safe to expose in diagnostics.
    field: &'static str,
  },
  /// A Factory permission value is malformed or outside a safety bound.
  #[error("invalid factory permission in {category:?}")]
  InvalidPermission {
    /// Permission category whose value was rejected.
    category: crate::PermissionCategory,
  },
  /// A Factory permission collection repeats an authority grant.
  #[error("factory permission contains a duplicate {category:?} grant")]
  DuplicatePermission {
    /// Permission category containing the duplicate grant.
    category: crate::PermissionCategory,
  },
  /// The selected Agent/backend cannot enforce one required permission category.
  #[error("factory permission category {category:?} is not enforceable")]
  UnenforceablePermission {
    /// Required semantic enforcement capability that is absent.
    category: crate::PermissionCategory,
  },
  /// A Factory reconciler claim has an invalid time window.
  #[error("factory claim has an invalid time window")]
  InvalidClaim,
  /// A Factory decision was attempted with a fence that is no longer current.
  #[error("factory claim fence is stale")]
  StaleClaim,
  /// A Factory decision was attempted after its claim expired.
  #[error("factory claim is expired")]
  ExpiredClaim,
  /// Persisted lifecycle facts do not match the current Factory Run state.
  #[error("factory lifecycle facts are invalid for {state:?}")]
  InvalidLifecycle {
    /// Current durable state whose facts were inconsistent.
    state: crate::FactoryRunState,
  },
  /// Persisted Factory work-in-progress usage exceeds its immutable ceiling.
  #[error("factory work-in-progress usage exceeds its configured ceiling")]
  WipExceeded,
  /// An immutable Decision policy contains an unsafe or contradictory rule.
  #[error("invalid factory decision policy field: {field}")]
  InvalidDecisionPolicy {
    /// Stable policy field whose invariant was not satisfied.
    field: &'static str,
  },
  /// A provider-neutral Decision Signal contract or consuming policy is invalid.
  #[error("invalid Decision Signal field: {field}")]
  InvalidDecisionSignal {
    /// Stable field name safe to expose in diagnostics.
    field: &'static str,
  },
  /// A versioned Task Envelope, result, handoff, or context record is invalid.
  #[error("invalid Factory task contract field: {field}")]
  InvalidTaskContract {
    /// Stable field or relationship name safe to expose in diagnostics.
    field: &'static str,
  },
}
