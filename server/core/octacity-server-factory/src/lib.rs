//! Bounded Dark Factory identities, values, and immutable records.
//!
//! This crate owns provider-neutral Factory vocabulary and construction
//! invariants. It deliberately contains no persistence, transport, provider,
//! runner, or UI types.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod budget;
mod configuration;
#[cfg(test)]
mod configuration_tests;
mod decision_engine;
#[cfg(test)]
mod decision_engine_tests;
mod decision_signal;
#[cfg(test)]
mod decision_signal_tests;
mod enum_value;
mod error;
mod evaluation;
mod execution;
mod identity;
mod lifecycle;
mod lifecycle_decision;
#[cfg(test)]
mod lifecycle_decision_tests;
mod permission;
#[cfg(test)]
mod permission_tests;
mod subject;
mod value;

pub use budget::{
  BudgetLimit, BudgetResource, BudgetUsage, MAX_FACTORY_ATTEMPTS, MAX_FACTORY_COST_MICRO_UNITS,
  MAX_FACTORY_ELAPSED_MILLIS, MAX_FACTORY_OUTPUT_BYTES, MAX_FACTORY_TOKENS,
};
pub use configuration::{
  BuildConfigurationRef, DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProfileDraft,
  DeliveryPolicy, DeliveryPolicyDraft, EvaluationPolicy, EvaluationPolicyDraft, FactoryConfiguration,
  FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryReferenceChoice,
  FactoryStageDefinition, FactoryStageDraft, FactoryWipLimits, ImmutableReference, MAX_CONFIGURATION_CHOICES_PER_KIND,
  MAX_FACTORY_ACTIVE_RUNS, MAX_FACTORY_ACTIVE_STAGES, MAX_FACTORY_REWORK_CYCLES, MAX_FACTORY_STAGES, ReworkPolicy,
  ReworkPolicyDraft,
};
pub use decision_engine::{
  DecisionEngineInput, DecisionPolicy, DecisionPolicyDefinition, DeterministicGate, evaluate_decision,
};
pub use decision_signal::{
  DECISION_SIGNAL_PROBABILITY_SCALE, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalConsumption, DecisionSignalConsumptionKind, DecisionSignalDisposition,
  DecisionSignalInputMedia, DecisionSignalProbability, DecisionSignalProvider, DecisionSignalProviderCapability,
  DecisionSignalProviderFailure, DecisionSignalProviderFuture, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalProviderObservation, DecisionSignalProviderRequest,
  DecisionSignalProviderResult, DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain,
  DecisionSignalQuestionKind, DecisionSignalReceipt, DecisionSignalRouteSet, DecisionSignalScoreDomain,
  DecisionSignalThreshold, MAX_DECISION_SIGNAL_CHOICES, MAX_DECISION_SIGNAL_QUESTIONS, ToolRiskChoiceMapping,
  consume_routing_signal, consume_tool_risk_signal,
};
pub use enum_value::{
  AssessmentOutcome, DecisionOutcome, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose,
  DecisionSignalState, DeliveryState, DeterministicGateOutcome, FactoryRunState, FactoryStageKind, FindingSeverity,
  IndeterminatePolicy, MacroCallKind, MountMode, PermissionCategory, ReportingState, RiskClass,
};
pub use error::{FactoryChoiceKind, FactoryEntityKind, FactoryEnumKind, FactoryError, FactoryTextKind, TextRejection};
pub use evaluation::{
  Assessment, AssessmentFinding, ChangeSet, Decision, DecisionReason, EvaluationPlan, EvidenceItem, EvidenceManifest,
  MAX_ASSESSMENT_FINDINGS, MAX_CRITERION_PACKS, MAX_DECISION_ASSESSMENTS, MAX_DECISION_REASONS, MAX_EVALUATORS,
  MAX_EVIDENCE_ITEMS,
};
pub use execution::{
  DecisionSignalDigests, DecisionSignalRequest, FactoryClaimOwnership, MacroCall, StageAttempt, StageAttemptCompletion,
  StageAttemptOutcome,
};
pub use identity::{
  AssessmentId, ChangeSetId, DecisionId, DecisionSignalReceiptId, DecisionSignalRequestId, DeliveryAttemptId,
  EscalationId, EvaluationPlanId, EvidenceManifestId, FactoryConfigurationId, FactoryRunId, MacroCallId,
  ReportingAttemptId, StageAttemptId, WorkEnvelopeId,
};
pub use lifecycle::{
  DeliveryAttempt, Escalation, FactoryConfigurationRef, FactoryRun, MAX_WORK_SPECIFICATION_REFERENCES,
  ReportingAttempt, WorkArtifacts, WorkClassification, WorkEnvelope,
};
pub use lifecycle_decision::{
  DecisionSignalProgress, DeliveryIntent, DeliveryProgress, EvaluationBranch, EvaluationBranchState,
  EvaluationProgress, EvaluationState, FactoryClaim, FactoryClaimFence, FactoryDecisionGuard,
  FactoryEscalationDisposition, FactoryEscalationReason, FactoryLifecycleProgress, FactoryLifecycleSnapshot,
  FactoryNextAction, FactoryStageProgress, FactoryStageTarget, FactoryWaitReason, FactoryWipUsage, ReportingProgress,
  decide_next_action, validate_lifecycle_progress, validate_lifecycle_transition,
};
pub use permission::{
  CommandArgumentPattern, CommandPermission, FactoryOutputPermissions, FactoryPath, FactoryPermissionDraft,
  FactoryPermissionSet, FactoryResourceLimits, LocalPermissionCeiling, MAX_COMMAND_ARGUMENT_BYTES,
  MAX_COMMAND_ARGUMENTS, MAX_FACTORY_CPU_MILLIS, MAX_FACTORY_DISK_BYTES, MAX_FACTORY_MEMORY_BYTES,
  MAX_FACTORY_OUTPUT_COUNT, MAX_FACTORY_PROCESS_COUNT, MAX_PERMISSION_ENTRIES_PER_CATEGORY, MAX_PERMISSION_HOST_BYTES,
  MAX_PERMISSION_PATH_BYTES, MountPermission, NetworkHost, resolve_factory_permissions,
};
pub use subject::{CandidateSubject, ExactSubject};
pub use value::{
  DecisionPolicyVersion, DeliveryAttemptNumber, ExternalWorkIdentity, FactoryConfigurationVersion, FactoryDigest,
  FactoryKey, FactoryMetadata, FactoryRunVersion, FactoryText, MAX_EXTERNAL_WORK_IDENTITY_BYTES, MAX_FACTORY_KEY_BYTES,
  MAX_FACTORY_METADATA_BYTES, MAX_FACTORY_METADATA_ENTRIES, MAX_FACTORY_METADATA_KEY_BYTES,
  MAX_FACTORY_METADATA_VALUE_BYTES, MAX_FACTORY_TEXT_BYTES, MAX_WORK_PRIORITY, ReportingAttemptNumber,
  StageAttemptNumber, WorkPriority,
};
