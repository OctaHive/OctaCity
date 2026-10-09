//! Bounded Dark Factory identities, values, and immutable records.
//!
//! This crate owns provider-neutral Factory vocabulary and construction
//! invariants. It deliberately contains no persistence, transport, provider,
//! runner, or UI types.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod assessment;
mod budget;
mod configuration;
#[cfg(test)]
mod configuration_tests;
mod credential;
#[cfg(test)]
mod credential_tests;
mod decision_engine;
#[cfg(test)]
mod decision_engine_tests;
mod decision_signal;
#[cfg(test)]
mod decision_signal_tests;
mod enum_value;
mod error;
mod evaluation;
mod evaluation_progress;
mod evidence;
#[cfg(test)]
mod evidence_tests;
mod execution;
mod flow;
mod flow_graph;
#[cfg(test)]
mod flow_graph_tests;
#[cfg(test)]
mod flow_interpreter_tests;
mod graph;
#[cfg(test)]
mod graph_tests;
mod identity;
mod lifecycle;
mod lifecycle_decision;
#[cfg(test)]
mod lifecycle_decision_tests;
mod permission;
#[cfg(test)]
mod permission_tests;
mod review_plan;
#[cfg(test)]
mod review_plan_tests;
mod subject;
mod task_contract;
#[cfg(test)]
mod task_contract_tests;
mod value;

pub use assessment::{
  Assessment, AssessmentEvidenceReference, AssessmentFinding, AssessmentFindingKind, AssessmentInput,
  AssessmentProvenance, MAX_ASSESSMENT_EVIDENCE_REFERENCES, MAX_ASSESSMENT_FINDINGS,
};
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
pub use credential::{
  AuthorizedFactoryCredentialProfile, FactoryCredentialConsumer, FactoryCredentialProfiles, FactoryCredentialPurpose,
};
pub use decision_engine::{DecisionEngineInput, DecisionPolicy, DecisionPolicyDefinition, evaluate_decision};
pub use decision_signal::{
  DECISION_SIGNAL_PROBABILITY_SCALE, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalConsumption, DecisionSignalConsumptionKind, DecisionSignalDisposition,
  DecisionSignalInputMedia, DecisionSignalProbability, DecisionSignalProvider, DecisionSignalProviderCapability,
  DecisionSignalProviderFailure, DecisionSignalProviderFuture, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalProviderObservation, DecisionSignalProviderRequest,
  DecisionSignalProviderResult, DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain,
  DecisionSignalQuestionKind, DecisionSignalReceipt, DecisionSignalRouteSet, DecisionSignalScoreDomain,
  DecisionSignalThreshold, MAX_DECISION_SIGNAL_CHOICES, MAX_DECISION_SIGNAL_QUESTIONS, MAX_DECISION_SIGNAL_RECEIPTS,
  ToolRiskChoiceMapping, consume_routing_signal, consume_tool_risk_signal,
};
pub use enum_value::{
  AssessmentOutcome, DecisionOutcome, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose,
  DecisionSignalState, DeliveryState, DeterministicGateOutcome, FactoryRunState, FactoryStageKind, FindingSeverity,
  FlowNodeKind, IndeterminatePolicy, MacroCallKind, MountMode, PermissionCategory, ReportingState, RiskClass,
};
pub use error::{FactoryChoiceKind, FactoryEntityKind, FactoryEnumKind, FactoryError, FactoryTextKind, TextRejection};
pub use evaluation::{Decision, DecisionReason, MAX_DECISION_ASSESSMENTS, MAX_DECISION_REASONS};
pub use evaluation_progress::{
  EvaluationBranch, EvaluationBranchResult, EvaluationBranchState, EvaluationProgress, EvaluationState,
};
pub use evidence::{
  ChangeSet, EvidenceItem, EvidenceItemInput, EvidenceManifest, EvidenceOutputKind, EvidenceProducer,
  EvidenceRequirement, MAX_EVIDENCE_ITEMS,
};
pub use execution::{
  DecisionSignalDigests, DecisionSignalRequest, FactoryClaimOwnership, MAX_MACRO_CALL_DEPENDENCIES,
  MAX_MACRO_CALL_DEPTH, MacroCall, MacroCallDeclaration, StageAttempt, StageAttemptCompletion, StageAttemptOutcome,
};
pub use flow::{
  AdmittedFlow, FlowAdmissionLimits, FlowContextProjection, FlowDataProjection, FlowDefinition, FlowDefinitionInput,
  FlowDefinitionRef, FlowDirective, FlowDirectiveInputs, FlowExecutionPolicy, FlowInterpreter, FlowNodeDefinition,
  FlowNodeDefinitionInput, FlowOutcomeDefinition, FlowOutcomeKind, FlowRun, FlowRunParent, FlowRuntimeHistory,
  FlowTerminalDefinition, FlowTransition, FlowTransitionTarget, MAX_FLOW_CONTEXT_SELECTIONS,
  MAX_FLOW_DEFINITION_CLOSURE, MAX_FLOW_DEFINITION_EDGES, MAX_FLOW_DEFINITION_NODES, MAX_FLOW_EXPANDED_NODES,
  MAX_FLOW_FAN_OUT, MAX_FLOW_NESTING_DEPTH, MAX_FLOW_REPEAT_COUNT, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptCompletionInput, NodeAttemptInput, NodeExecutionIdentity, PinnedFlowDefinitionClosure,
  ValidatedFlowDefinitionClosure, WorkflowCycle, validate_flow_runtime_history,
};
pub(crate) use flow_graph::{FactoryFlowEdge, analyze_factory_flow_graph};
pub use identity::{
  AssessmentId, ChangeSetId, ContextManifestId, DecisionId, DecisionSignalReceiptId, DecisionSignalRequestId,
  DeliveryAttemptId, EscalationId, EvaluationPlanId, EvidenceManifestId, FactoryConfigurationId, FactoryRunId,
  FlowDefinitionId, FlowRunId, MacroCallId, NodeAttemptId, ReportingAttemptId, RetrievalReceiptId, StageAttemptId,
  StageHandoffId, TaskEnvelopeId, WorkEnvelopeId, WorkflowCycleId,
};
pub use lifecycle::{
  DeliveryAttempt, Escalation, FactoryConfigurationRef, FactoryRun, MAX_WORK_SPECIFICATION_REFERENCES,
  ReportingAttempt, WorkArtifacts, WorkClassification, WorkEnvelope,
};
pub use lifecycle_decision::{
  DecisionSignalProgress, DeliveryIntent, DeliveryProgress, FactoryClaim, FactoryClaimFence, FactoryDecisionGuard,
  FactoryDecisionResources, FactoryEscalationDisposition, FactoryEscalationReason, FactoryLifecycleProgress,
  FactoryLifecycleSnapshot, FactoryNextAction, FactoryReworkStatus, FactoryStageProgress, FactoryStageTarget,
  FactoryWaitReason, FactoryWipUsage, ReportingProgress, decide_next_action, validate_lifecycle_progress,
  validate_lifecycle_transition,
};
pub use permission::{
  CommandArgumentPattern, CommandPermission, FactoryOutputPermissions, FactoryPath, FactoryPermissionDraft,
  FactoryPermissionSet, FactoryResourceLimits, LocalPermissionCeiling, MAX_COMMAND_ARGUMENT_BYTES,
  MAX_COMMAND_ARGUMENTS, MAX_FACTORY_CPU_MILLIS, MAX_FACTORY_DISK_BYTES, MAX_FACTORY_MEMORY_BYTES,
  MAX_FACTORY_OUTPUT_COUNT, MAX_FACTORY_PROCESS_COUNT, MAX_PERMISSION_ENTRIES_PER_CATEGORY, MAX_PERMISSION_HOST_BYTES,
  MAX_PERMISSION_PATH_BYTES, MountPermission, NetworkHost, resolve_factory_permissions,
};
pub use review_plan::{
  CriterionPack, EvaluationPlan, EvaluationPlanDefinition, MAX_CRITERION_PACKS, MAX_EVALUATORS, ReviewBranch,
  ReviewEvaluatorCapability, ReviewPlanEscalation, ReviewPlanPreparation, ReviewPurpose, prepare_evaluation_plan,
};
pub use subject::{CandidateSubject, ExactSubject};
pub use task_contract::{
  BoundedSummary, ContextManifest, ContextManifestEntry, ContextSourceKind, EvaluationResult, EvaluationResultFinding,
  FACTORY_TASK_CONTRACT_VERSION, FactoryArtifactReference, FactoryContextReference, FactoryProducedDeliverable,
  FactoryRepositoryFragmentReference, FactoryRepositoryPath, FactoryRepositoryRange, FactoryTaskControlDigests,
  FactoryTaskDeclaration, FactoryTaskDefinition, FactoryTaskDeliverable, FactoryTaskDigests, FactoryTaskEnvelope,
  FactoryTaskMode, FactoryTaskResultSchema, FactoryTaskSubject, FactoryTaskToolchain, ImplementationOutcome,
  ImplementationResult, MAX_CONTEXT_ENTRY_BYTES, MAX_CONTEXT_MANIFEST_BYTES, MAX_CONTEXT_MANIFEST_ENTRIES,
  MAX_FACTORY_REPOSITORY_PATH_BYTES, MAX_RETRIEVAL_FRAGMENTS, MAX_STAGE_HANDOFF_ITEMS, MAX_TASK_ARTIFACTS,
  MAX_TASK_DELIVERABLES, MAX_TASK_EVALUATION_FINDINGS, MAX_TASK_PRIOR_FINDINGS, MacroCallCompletion,
  MacroCallCompletionOutputs, MacroCallTerminal, RETRIEVAL_RECEIPT_VERSION, RepositoryFragment, RetrievalReceipt,
  StageHandoff, StageHandoffContent, StageHandoffDeclaration, StageHandoffOutcome, StageHandoffReferences,
};
pub use value::{
  DecisionPolicyVersion, DeliveryAttemptNumber, ExternalWorkIdentity, FactoryConfigurationVersion, FactoryDigest,
  FactoryKey, FactoryMetadata, FactoryRunVersion, FactorySafeText, FactoryText, FlowDefinitionVersion,
  MAX_EXTERNAL_WORK_IDENTITY_BYTES, MAX_FACTORY_KEY_BYTES, MAX_FACTORY_METADATA_BYTES, MAX_FACTORY_METADATA_ENTRIES,
  MAX_FACTORY_METADATA_KEY_BYTES, MAX_FACTORY_METADATA_VALUE_BYTES, MAX_FACTORY_SAFE_TEXT_BYTES,
  MAX_FACTORY_TEXT_BYTES, MAX_WORK_PRIORITY, NodeAttemptNumber, ReportingAttemptNumber, StageAttemptNumber,
  WorkPriority, WorkflowCycleNumber,
};
