//! Bounded Dark Factory identities, values, and immutable records.
//!
//! This crate owns provider-neutral Factory vocabulary and construction
//! invariants. It deliberately contains no persistence, transport, provider,
//! runner, or UI types.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub use flow::{FlowProfileAdmissionOverride, FlowProfileAdmissionOverrides, FlowStageAddress};

/// Maximum exact installed execution identities admitted by one Factory owner.
pub const MAX_FACTORY_EXECUTOR_BINDINGS: usize = 256;

mod assessment;
mod budget;
mod configuration;
#[cfg(test)]
mod configuration_tests;
#[cfg(test)]
mod configured_flow_tests;
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
#[cfg(test)]
mod flow_action_tests;
#[cfg(test)]
mod flow_build_tests;
#[cfg(test)]
mod flow_data_tests;
#[cfg(test)]
mod flow_gate_program_tests;
#[cfg(test)]
mod flow_gate_tests;
#[cfg(test)]
mod flow_journal_tests;
#[cfg(test)]
mod flow_tagged_schema_tests;
#[cfg(test)]
mod flow_template_tests;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
mod flow_test_support;
#[cfg(any(test, feature = "test-support"))]
use crate as factory;
mod candidate_deliverable;
mod flow_graph;
#[cfg(test)]
mod flow_graph_tests;
#[cfg(test)]
mod flow_interpreter_tests;
#[cfg(test)]
mod flow_mapping_tests;
mod graph;
#[cfg(test)]
mod graph_tests;
mod identity;
mod journey;
mod lifecycle;
mod lifecycle_decision;
#[cfg(test)]
mod lifecycle_decision_tests;
mod permission;
#[cfg(test)]
mod permission_tests;
mod phase_pool;
mod protected_test;
pub use candidate_deliverable::{
  CandidateDeliverableBinding, CandidateDeliverableContract, CandidateDeliverablePolicy,
  CandidateDeliverableRequirement, CandidateFileObservation,
};
#[cfg(test)]
mod candidate_deliverable_tests;
#[cfg(test)]
mod protected_test_tests;
mod review_plan;
mod role_avatar;
mod role_profile;
pub use role_avatar::{FactoryRoleAvatar, MAX_FACTORY_ROLE_AVATAR_BYTES};
mod role_profile_capability;
mod role_profile_execution;
mod role_profile_instructions;
mod role_template;
pub use role_profile_execution::{
  FactoryRoleEnvironmentValue, FactoryRoleIntegrationBinding, FactoryRoleMcpBinding, FactoryRoleProfileExecution,
};
pub use role_profile_instructions::FactoryInstructions;
mod verification_plan;
pub use verification_plan::{
  VerificationCheck, VerificationCheckInput, VerificationMeasurement, VerificationObservation, VerificationPlan,
  VerificationPolicy, VerificationSourceVisibility, VerificationTarget,
};
#[cfg(test)]
mod review_plan_tests;
mod source_group;
mod source_input;
mod source_reporting;
pub use source_reporting::*;
mod source_predicate;
pub use source_group::{MAX_WORK_SOURCE_GROUP_MEMBERS, WorkSourceGroup, WorkSourceSelection, WorkSourceSelectionPass};
pub use source_predicate::{
  MAX_WORK_SOURCE_PREDICATE_LABEL_BYTES, MAX_WORK_SOURCE_PREDICATE_LABELS, MAX_WORK_SOURCE_PREDICATE_STATE_BYTES,
  WorkSourceLabelPresence, WorkSourcePredicate, WorkSourcePredicateMismatch,
};
mod subject;
pub use source_input::{
  WorkSourceBinding, WorkSourceCapability, WorkSourceCapabilityRole, WorkSourceConnectionRef, WorkSourceInput,
  WorkSourceInputContext, WorkSourceInputDraft, WorkSourceRepositoryRef,
};
#[cfg(test)]
mod source_input_tests;
mod task_contract;
#[cfg(test)]
mod task_contract_tests;
mod value;
#[cfg(test)]
mod verification_plan_tests;

pub use assessment::{
  Assessment, AssessmentEvidenceReference, AssessmentFinding, AssessmentFindingKind, AssessmentInput,
  AssessmentProvenance, MAX_ASSESSMENT_EVIDENCE_REFERENCES, MAX_ASSESSMENT_FINDINGS,
};
pub use budget::{
  BudgetLimit, BudgetResource, BudgetUsage, MAX_FACTORY_ATTEMPTS, MAX_FACTORY_COST_MICRO_UNITS,
  MAX_FACTORY_ELAPSED_MILLIS, MAX_FACTORY_OUTPUT_BYTES, MAX_FACTORY_TOKENS,
};
pub use configuration::{
  BuildConfigurationRef, DecisionSignalProfile, DecisionSignalProfileDefinition, EvaluationPolicy,
  FactoryChoiceCompatibility, FactoryChoiceIdentity, FactoryChoiceMetadata, FactoryConfiguration,
  FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft,
  FactoryProfileSelectorMetadata, FactoryReferenceChoice, FactoryWipLimits, ImmutableReference,
  MAX_CONFIGURATION_CHOICES_PER_KIND, MAX_FACTORY_ACTIVE_NODES, MAX_FACTORY_ACTIVE_RUNS,
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
  DecisionSignalProviderLimits, DecisionSignalProviderObservation, DecisionSignalProviderObservationFuture,
  DecisionSignalProviderRequest, DecisionSignalProviderResult, DecisionSignalQuestion, DecisionSignalQuestionCriteria,
  DecisionSignalQuestionDomain, DecisionSignalQuestionKind, DecisionSignalReceipt, DecisionSignalRouteSet,
  DecisionSignalScoreDomain, DecisionSignalThreshold, MAX_DECISION_SIGNAL_CHOICES, MAX_DECISION_SIGNAL_QUESTIONS,
  MAX_DECISION_SIGNAL_RECEIPTS, ToolRiskChoiceMapping, consume_profile_selection_signal, consume_routing_signal,
  consume_tool_risk_signal,
};
pub use enum_value::{
  AssessmentOutcome, DecisionOutcome, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose,
  DecisionSignalState, DeliveryState, DeterministicGateOutcome, FactoryRunState, FindingSeverity, FlowNodeKind,
  IndeterminatePolicy, MountMode, PermissionCategory, ReportingState, RiskClass,
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
pub use execution::{DecisionSignalDigests, DecisionSignalRequest, FactoryClaimOwnership};
pub use flow::{
  AdmittedFlow, FlowActionBinding, FlowAdmissionLimits, FlowBuildBinding, FlowBuildExecution, FlowBuildIntent,
  FlowBuildProfile, FlowBuildProvenance, FlowComparison, FlowContextBinding, FlowContextProjection,
  FlowContextSelection, FlowDataHistory, FlowDataMapping, FlowDataProjection, FlowDataSchema, FlowDefinition,
  FlowDefinitionInput, FlowDefinitionRef, FlowDefinitionTemplate, FlowDirective, FlowDirectiveInputs,
  FlowDiscoveryFacts, FlowDiscoveryKind, FlowExecutionPolicy, FlowFieldSchema, FlowGateBinding, FlowGateProgram,
  FlowGateRule, FlowHumanDisposition, FlowHumanGateBinding, FlowHumanOperation, FlowIncomingData, FlowInputBinding,
  FlowInputPreparation, FlowInputSource, FlowInterpreter, FlowNodeDefinition, FlowNodeDefinitionInput, FlowNodeInput,
  FlowNodeRecord, FlowNodeStop, FlowNodeStopReason, FlowOperand, FlowOutcomeDefinition, FlowOutcomeKind, FlowPayload,
  FlowPredicate, FlowPredicateFailure, FlowProfileCandidate, FlowProfileRuleOutcome, FlowProfileSelectionBinding,
  FlowProfileSelectionError, FlowProfileSelectionFailure, FlowProfileSelectionPolicy, FlowProfileSelectionRecord,
  FlowProfileSelectionRecovery, FlowProfileSelectionRule, FlowProfileSelectionSignal, FlowProgress, FlowQuantifier,
  FlowRecordObservation, FlowRecordReference, FlowRecordView, FlowRun, FlowRunParent, FlowRuntimeHistory,
  FlowTemplateDeliverableBinding, FlowTemplateDeliverablePolicy, FlowTemplateNode, FlowTemplateOutcome,
  FlowTemplateSchema, FlowTerminalDefinition, FlowTransition, FlowTransitionTarget, FlowValueMapping, FlowValueSchema,
  FlowVerifiedBuildData, FlowVerifiedBuildOutput, MAX_FLOW_ACTION_PARAMETER_BYTES, MAX_FLOW_CONTEXT_SELECTIONS,
  MAX_FLOW_DATA_BYTES, MAX_FLOW_DEFINITION_CLOSURE, MAX_FLOW_DEFINITION_EDGES, MAX_FLOW_DEFINITION_NODES,
  MAX_FLOW_EXPANDED_NODES, MAX_FLOW_FAN_OUT, MAX_FLOW_GATE_PARAMETER_BYTES, MAX_FLOW_INPUT_BINDING_BYTES,
  MAX_FLOW_MAPPING_BYTES, MAX_FLOW_NESTING_DEPTH, MAX_FLOW_REPEAT_COUNT, MAX_FLOW_SCHEMA_BYTES, MAX_FLOW_SCHEMA_DEPTH,
  MAX_FLOW_SCHEMA_ENTRIES, MAX_FLOW_SCHEMA_ITEMS, MAX_FLOW_VARIANT_NAME_BYTES, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptCompletionInput, NodeAttemptInput, NodeExecutionIdentity, PinnedFlowDefinitionClosure,
  PublishedFlowTemplate, ValidatedFlowDefinitionClosure, WorkflowCycle, validate_flow_runtime_history,
};
pub(crate) use flow_graph::{FactoryFlowEdge, analyze_factory_flow_graph};
pub use identity::{
  AssessmentId, ChangeSetId, ContextManifestId, DecisionId, DecisionSignalReceiptId, DecisionSignalRequestId,
  DeliveryAttemptId, EscalationId, EvaluationPlanId, EvidenceManifestId, FactoryConfigurationId, FactoryRoleId,
  FactoryRoleProfileId, FactoryRunId, FlowDefinitionId, FlowRunId, NodeAttemptId, ReportingAttemptId,
  RetrievalReceiptId, TaskEnvelopeId, WorkEnvelopeId, WorkSourceGroupId, WorkSourceInputId, WorkflowCycleId,
};
pub use journey::FactoryFlowConfiguration;
pub use lifecycle::{
  DeliveryAttempt, Escalation, FactoryConfigurationRef, FactoryRun, MAX_WORK_SPECIFICATION_REFERENCES,
  ReportingAttempt, WorkArtifacts, WorkClassification, WorkEnvelope,
};
pub use lifecycle_decision::{
  FactoryClaim, FactoryClaimFence, FactoryLifecycleProgress, FactoryWipUsage, validate_lifecycle_progress,
  validate_lifecycle_transition,
};
pub use permission::{
  CommandArgumentPattern, CommandPermission, FactoryOutputPermissions, FactoryPath, FactoryPermissionDraft,
  FactoryPermissionSet, FactoryResourceLimits, LocalPermissionCeiling, MAX_COMMAND_ARGUMENT_BYTES,
  MAX_COMMAND_ARGUMENTS, MAX_FACTORY_CPU_MILLIS, MAX_FACTORY_DISK_BYTES, MAX_FACTORY_MEMORY_BYTES,
  MAX_FACTORY_OUTPUT_COUNT, MAX_FACTORY_PROCESS_COUNT, MAX_PERMISSION_ENTRIES_PER_CATEGORY, MAX_PERMISSION_HOST_BYTES,
  MAX_PERMISSION_PATH_BYTES, MountPermission, NetworkHost, resolve_factory_permissions,
};
pub use phase_pool::{
  FlowPoolSettings, MAX_PHASE_POOL_BATCH, MAX_PHASE_POOL_WIP, PhasePoolOrder, PhasePoolPolicy, reserve_phase_budget,
};
pub use protected_test::{
  ProtectedTestBundle, ProtectedTestCoverage, ProtectedTestFile, ProtectedTestHandoff, ProtectedTestPlan,
};
pub use review_plan::{
  CriterionPack, EvaluationPlan, EvaluationPlanDefinition, MAX_CRITERION_PACKS, MAX_EVALUATORS, ReviewBranch,
  ReviewEvaluatorCapability, ReviewPlanEscalation, ReviewPlanPreparation, ReviewPurpose, prepare_evaluation_plan,
};
pub use role_profile::{
  FactoryRole, FactoryRoleDefinition, FactoryRoleProfile, FactoryRoleProfileDefinition, FactoryRoleProfileRef,
  FactoryRoleRef, MAX_FACTORY_ROLE_DEFINITION_BYTES, MAX_FACTORY_ROLE_INSTRUCTION_BYTES,
  MAX_FACTORY_ROLE_PROFILE_ENTRIES,
};
pub use subject::{CandidateSubject, ExactSubject};
pub use task_contract::{
  BoundedSummary, ContextManifest, ContextManifestEntry, ContextSourceKind, EvaluationResult, EvaluationResultFinding,
  FACTORY_TASK_CONTRACT_VERSION, FactoryArtifactReference, FactoryContextReference, FactoryProducedDeliverable,
  FactoryRepositoryFragmentReference, FactoryRepositoryPath, FactoryRepositoryRange, FactoryRoleExecutionPackage,
  FactoryRoleExecutionPackageInput, FactoryRoleProjectRule, FactoryRoleSkillBundle, FactoryTaskControlDigests,
  FactoryTaskDeclaration, FactoryTaskDefinition, FactoryTaskDeliverable, FactoryTaskDigests, FactoryTaskEnvelope,
  FactoryTaskMode, FactoryTaskResultSchema, FactoryTaskSubject, FactoryTaskToolchain, ImplementationOutcome,
  ImplementationResult, MAX_CONTEXT_ENTRY_BYTES, MAX_CONTEXT_MANIFEST_BYTES, MAX_CONTEXT_MANIFEST_ENTRIES,
  MAX_FACTORY_REPOSITORY_PATH_BYTES, MAX_RETRIEVAL_FRAGMENTS, MAX_TASK_ARTIFACTS, MAX_TASK_DELIVERABLES,
  MAX_TASK_EVALUATION_FINDINGS, MAX_TASK_PRIOR_FINDINGS, RETRIEVAL_RECEIPT_VERSION, RepositoryFragment,
  RetrievalReceipt,
};
pub use value::{
  DecisionPolicyVersion, DeliveryAttemptNumber, ExternalWorkIdentity, FactoryConfigurationVersion, FactoryDigest,
  FactoryKey, FactoryMetadata, FactoryRoleProfileVersion, FactoryRoleVersion, FactoryRunVersion,
  FactoryRuntimeSettingsVersion, FactorySafeText, FactoryText, FlowDefinitionVersion, MAX_EXTERNAL_WORK_IDENTITY_BYTES,
  MAX_FACTORY_KEY_BYTES, MAX_FACTORY_METADATA_BYTES, MAX_FACTORY_METADATA_ENTRIES, MAX_FACTORY_METADATA_KEY_BYTES,
  MAX_FACTORY_METADATA_VALUE_BYTES, MAX_FACTORY_SAFE_TEXT_BYTES, MAX_FACTORY_TEXT_BYTES, MAX_WORK_PRIORITY,
  NodeAttemptNumber, ReportingAttemptNumber, WorkPriority, WorkSourceGroupVersion, WorkSourceInputVersion,
  WorkflowCycleNumber, contains_sensitive_material,
};

#[cfg(test)]
mod accepted_work_tests;

mod accepted_work;
pub use accepted_work::AcceptedWorkContract;

/// Explicit test fixtures for isolated capability and persistence contracts.
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

#[cfg(any(test, feature = "test-support"))]
pub use testing::FactoryStageKind;

#[cfg(test)]
mod promotion_plan_tests;

mod promotion_plan;
pub use promotion_plan::{PromotionObservation, PromotionPlan, PromotionPolicy};

pub use role_template::{
  FactoryRoleProfileTemplate, FactoryRoleTemplate, FactoryRoleTemplateBinding, FactoryRoleTemplateInstance,
  factory_role_templates,
};

pub use role_profile_capability::{
  FactoryRoleModelCatalog, FactoryRoleModelChoice, FactoryRoleProfileCapability, FactoryRoleProfileIncompatibility,
};

mod mcp;
pub use mcp::{
  FactoryMcpConfiguration, FactoryMcpServer, FactoryMcpTransport, MAX_FACTORY_MCP_ARGUMENT_BYTES,
  MAX_FACTORY_MCP_CONFIGURATION_BYTES, MAX_FACTORY_MCP_SELECTION_ENTRIES, MAX_FACTORY_MCP_SERVER_NAME_BYTES,
  MAX_FACTORY_MCP_TOOL_NAME_BYTES, validate_factory_mcp_server_name,
};
mod runtime_settings;
pub use runtime_settings::*;
