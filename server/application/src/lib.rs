//! Transport-independent OctaCity server use cases.
//!
//! Typed commands, queries, transaction coordination, projections, and
//! application error mapping belong here. Transport and concrete persistence
//! types must remain outside this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod agents;
mod artifacts;
mod builds;
mod cqrs;
mod error;
mod factory;
mod integrations;
mod issues;
mod management;
mod management_security;
mod opaque_cursor;
mod operations;
#[cfg(test)]
mod test_support;
mod transaction;
mod triggers;

#[cfg(test)]
#[path = "../tests/support/factory_candidate_context.rs"]
mod factory_candidate_context_test_support;

// Shared tests use the same fixture names in this crate and in server integration tests.
#[cfg(test)]
use factory::execution::tests::{factory_flow_tests, factory_node_failure_tests, factory_node_test_support};
#[cfg(any(test, feature = "test-support"))]
pub use issues::issue_test_support;

pub use agents::agent_cqrs::MAX_AGENT_INVENTORY_ENTRIES;
pub use agents::agent_cqrs::SetAgentEnabledCommand;
pub use factory::reporting::factory_issue_classification_action::*;
pub use factory::reporting::factory_issue_status_action::*;
pub use issues::issue::*;
pub use issues::issue_activity::*;
pub use issues::issue_code_review::*;
pub use issues::issue_discussion::*;
pub use issues::issue_execution::*;
pub use issues::issue_execution_publication::*;
pub use issues::issue_outcome_links::*;
pub use issues::issue_source::*;
pub use management::project_text_intake::*;
pub use octacity_protocol::{MAX_AGENT_HOST_TEXT_BYTES, MAX_AGENT_IP_ADDRESSES};
pub use octacity_server_store::AgentResources;
/// Shared memory ports and fixtures for public transport conformance; excluded from production builds.
#[cfg(feature = "test-support")]
pub mod testing {
  pub use crate::factory::execution::factory_test_support::*;
  pub use crate::issue_test_support::*;
  pub use octacity_server_store::testing::{
    InMemoryFactoryConfigurationStore, MutationEvidenceProbe, role_profile_contract_intent,
  };
}
pub use builds::build_execution_context::{
  BuildExecutionContext, BuildExecutionContextError, BuildExecutionContextSource, StoreBackedBuildExecutionContext,
};
pub use factory::admission::factory_readiness::*;
pub use factory::admission::factory_triage_context::{
  MAX_TRIAGE_SUPPORTING_ALIAS_BYTES, TriageContextProblem, TriageProjectContextDraft,
};
pub use factory::artifacts::factory_artifact_flow_inputs::FactoryArtifactFlowInputs;
pub use factory::artifacts::factory_artifact_reader::{
  FactoryArtifactReader, FactoryNodeContextSource, FactoryProjectInputReader, FactoryRetainedArtifact,
  FactoryRetainedInput, ReadFactoryArtifact,
};
pub use factory::artifacts::factory_candidate_context::{
  FactoryCandidateContext, FactoryCandidateContextSource, ReadFactoryCandidateContext,
};
pub use factory::artifacts::factory_candidate_deliverables::{
  VerifiedFactoryCandidateDeliverables, verify_factory_node_candidate_deliverables,
};
pub use factory::artifacts::factory_candidate_publication::{
  FactoryCandidatePublication, FactoryCandidatePublicationAction, FactoryCandidatePublicationRequest,
  FactoryCandidatePublisher, FactoryPublicationFacts, FactoryPublicationRejection, FactoryPublicationReview,
  FactoryReviewState,
};
pub use factory::artifacts::factory_execution_input_publisher::{
  FactoryExecutionDocument, FactoryExecutionInputPublisher,
};
pub use factory::artifacts::factory_execution_input_retention::FactoryExecutionInputRetentionWorker;
pub use factory::artifacts::factory_stored_candidate_context::{
  FactoryNodeCandidateContextSource, FactoryRetainedCandidateContextSource, FactoryStoredCandidateContextSource,
};
pub use factory::authoring::factory_presets::*;
pub use factory::codex::factory_codex_candidate_context::{
  FactoryCodexCandidateContextSource, FactoryCodexCandidateReader, FactoryCodexCandidateReaderFactory,
};
pub use factory::codex::factory_codex_materializer::{
  FactoryCodexBuildMaterializer, FactoryCodexBuildProfile, FactoryCodexCapturePolicy, FactoryCodexReviewerPolicy,
};
pub use factory::codex::factory_codex_observation::project_codex_node_output;
pub use factory::codex::factory_codex_output_source::FactoryCodexBuildOutputSource;
pub use factory::codex::factory_codex_tool_risk::FactoryCodexToolRiskPolicy;
pub use factory::codex::factory_codex_usage::FactoryCodexUsagePolicy;
pub use factory::execution::factory_action::*;
pub use factory::execution::factory_action_observation::*;
pub use factory::execution::factory_action_projection::*;
pub use factory::execution::factory_action_queue::*;
pub use factory::execution::factory_build_materialization::{
  FactoryManagedBuildInput, FactoryManagedCandidateProof, FactoryManagedJobProgram, FactoryManagedSourcePlan,
  materialize_factory_managed_build, materialize_factory_managed_build_with_source,
};
pub use factory::execution::factory_build_service::{FactoryBuildMaterializer, FactoryOrdinaryBuildService};
pub use factory::execution::factory_decision_signal_node::{
  FactoryDecisionSignalNodeBinding, FactoryDecisionSignalNodeExecutor, FactoryDecisionSignalNodeOutput,
  FactoryDecisionSignalNodeProfile,
};
pub use factory::execution::factory_decision_signal_plan::FactoryDecisionSignalRequestPlan;
pub use factory::execution::factory_flow::{
  FactoryFlowInputPreparer, FactoryFlowRunner, FactoryScopedValidationService,
};
pub use factory::execution::factory_node::{
  FactoryExecutionClock, FactoryFlowCoordinator, FactoryFlowStep, FactoryNodeCancellationStep,
  FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor, FactoryNodeOutboxEffect,
  FactoryNodeRunner, FactoryNodeStep, FactoryNodeTarget, FactoryScopedValidationCall, SystemFactoryExecutionClock,
  prepare_factory_flow_input,
};
pub use factory::execution::factory_node_build::{
  FactoryNodeBuildAdapter, FactoryNodeBuildCancellation, FactoryNodeBuildCompletion, FactoryNodeBuildExecutionFacts,
  FactoryNodeBuildOutputDocument, FactoryNodeBuildOutputObservation, FactoryNodeBuildOutputSource,
  FactoryNodeBuildOutputs, FactoryNodeBuildStep,
};
pub use factory::execution::factory_node_registry::{
  FactoryNodeExecutorBinding, FactoryNodeExecutorRegistry, MAX_FACTORY_EXECUTOR_BINDINGS,
};
pub use factory::execution::factory_operator::FactoryOperatorHandlers;
pub use factory::execution::factory_profile_signal::{
  FactoryProfileSelectionDriver, FactoryProfileSelector, FactoryProfileSelectors,
};
pub use factory::execution::factory_promotion::FactoryPromotionBuildAdapter;
pub use factory::execution::factory_review_invocation::{
  FactoryReviewInvocation, FactoryReviewInvocationSource, FactoryStoredReviewInvocationSource,
};
pub use factory::execution::factory_review_planning::{FactoryReviewPlanningAction, FactoryReviewPlanningProfile};
pub use factory::reporting::factory_flow_action::{
  FactoryFlowActionAdapter, FactoryFlowActionObservation, FactoryFlowActionPlugin, FactoryFlowActionRequest,
  FactoryReporterRetryPolicy, FactoryWorkStatusAction, FactoryWorkStatusProjector, FactoryWorkStatusRequest,
};
pub use factory::reporting::factory_result_reporting::{
  FactoryReportingCase, FactoryReportingDimension, FactoryReportingSelection, FactoryReportingTarget,
  FactoryResultReportingDraft,
};
pub use factory::reporting::factory_work_status_contract::FactoryWorkStatusActionContract;
pub use factory::reporting::factory_work_status_outbox::{
  FactoryOutboxDispatchSummary, FactoryOutboxDispatcher, FactoryReporterWorkerSettings, FactoryWorkStatusOutboxWorker,
  accepted_factory_status_source_update, validate_accepted_factory_status_request,
};
pub use factory::sources::factory_source_completion::{SourceCompletionDocument, TRIAGE_COMPLETION_PRESET};
pub use factory::sources::factory_source_input_retention::{FactorySourceInputRetentionWorker, SourceInputCleanupPage};
pub use factory::sources::factory_source_mapping::{
  MAX_SOURCE_ACCEPTANCE_SELECTOR_BYTES, MappedWorkSource, SourceAcceptanceMapping, SourceMappingFailure,
  WorkSourceMapping, WorkSourceSnapshot, map_work_source_snapshot,
};
pub use factory::sources::factory_source_polling::{
  FactorySourceCandidateProcessor, FactorySourcePageReader, FactorySourcePoller, SourcePollOutcome, SourcePollPage,
  SourcePollRequest, SourcePollingPolicy,
};
pub use factory::sources::factory_source_preparation::{
  FactoryWorkSourcePreparer, SourcePreparationOutcome, SourcePreparationSettings,
};
pub use factory::sources::factory_stage_source_update::{
  FactoryStageSourceAuthorizer, FactoryStageSourceUpdate, validate_issue_push_producers,
};
pub use factory::validation::factory_evaluation_action::{
  FactoryEvaluationDecisionAction, FactoryEvaluationDecisionProfile,
};
pub use factory::validation::factory_protected_test_authoring::{
  FactoryProtectedTestAuthoringAdapter, FactoryProtectedTestPlanSource,
};
pub use factory::validation::factory_protected_test_executor::FactoryProtectedTestAuthoringExecutor;
pub use factory::validation::factory_protected_test_finalizer::FactoryProtectedTestOutputFinalizer;
pub use factory::validation::factory_protected_test_plan::{
  FactoryProtectedTestPolicy, FactoryStoredProtectedTestPlanSource, freeze_factory_protected_test_plan,
};
pub use factory::validation::factory_protected_test_profile::{
  FactoryProtectedTestAuthoringPolicy, FactoryProtectedTestAuthoringProfile,
};
pub use factory::validation::factory_protected_test_publication::{
  FactoryProtectedTestFileOutput, FactoryProtectedTestInventoryPublication, FactoryProtectedTestInventoryPublisher,
};
pub use factory::validation::factory_protected_tests::{
  FactoryProtectedTestInventory, factory_protected_test_materialization, restore_factory_protected_test_bundle,
  verify_factory_protected_test_bundle,
};
pub use factory::validation::factory_protected_validation::construct_factory_protected_validation_evidence_manifest;
pub use factory::validation::factory_protected_validation::{
  FactoryProtectedValidationClassResult, FactoryProtectedValidationMaterialization, FactoryProtectedValidationReport,
  FactoryProtectedValidationRequest, factory_protected_validation_materialization,
};
pub use factory::validation::factory_protected_validation_build::{
  FactoryProtectedValidationAdapter, FactoryProtectedValidationInputSource, FactoryScopedValidationVerdict,
};
pub use factory::validation::factory_protected_validation_evidence_action::{
  FactoryProtectedValidationEvidenceAction, FactoryProtectedValidationEvidenceProfile,
};
pub use factory::validation::factory_verification::{
  VerifiedFactoryVerificationCheck, evaluate_factory_verification, verify_factory_verification_check,
};
pub use integrations::source_polling::source_scan_schedule;
pub use integrations::work_source_observation::{
  ReadWorkSourceObservations, WorkSourceObservation, WorkSourceObservationFreshness, WorkSourceObservationPage,
  WorkSourceObservations,
};
pub use issues::issue_reporter::*;
pub use issues::issue_workflow::*;

pub use octacity_server_domain::{BuildSourceMode, BuildSourceSelection, ProjectId, RetentionHoldVersion, Timestamp};
pub use octacity_server_orchestrator::{AttemptState, BuildState};
pub use octacity_server_store::{
  AgentListVisibility, AgentPoolListVisibility, ArtifactListVisibility, AuditActorKind, AuditFactListVisibility,
  AuditOutcome, BuildConfigurationListVisibility, BuildListVisibility, BuildLogSearchVisibility, BuildLogStream,
  CacheSessionListVisibility, InternalTriggerListVisibility, JobEventReadVisibility, LogIndexPosition, LogSearchCursor,
  LogSearchError, LogSearchMode, LogSearchQuery, MAX_AUDIT_ACTOR_IDENTITY_BYTES, MAX_AUDIT_OPERATION_BYTES,
  MAX_AUDIT_PAGE_SIZE, MAX_AUDIT_REQUEST_IDENTITY_BYTES, MAX_AUDIT_TARGET_IDENTITY_BYTES, MAX_AUDIT_TARGET_KIND_BYTES,
  MAX_LOG_SEARCH_PAGE_SIZE, MAX_LOG_SEARCH_QUERY_BYTES, MAX_LOG_SEARCH_SNIPPET_BYTES, MAX_READ_VISIBILITY_IDENTITIES,
  MAX_RESOURCE_SEARCH_CONTEXT_BYTES, MAX_RESOURCE_SEARCH_LABEL_BYTES, MAX_RESOURCE_SEARCH_QUERY_BYTES,
  MutationAuditContext, PipelineListVisibility, ProjectListVisibility, ReadVisibilityError, ReadVisibilityKind,
  ReadVisibilityView, RepositoryListVisibility, ResourceSearchVisibility, StoreError, TriggerDefinitionListVisibility,
};

pub use agents::agent_cqrs::{
  AgentCapacityProjection, AgentCommandOutcome, AgentCurrentExecutionProjection, AgentCurrentLeaseStateProjection,
  AgentHandlers, AgentInventoryProjection, AgentPageProjection, AgentProjection, AgentStatusProjection,
  DrainAgentCommand, GetAgentQuery, ListAgentsQuery, ReassignAgentPoolCommand,
};
pub use agents::agent_enrollment::{
  AgentEnrollmentHandler, IssueAgentEnrollmentCommand, IssueAgentEnrollmentCommandOutcome,
};
pub use agents::agent_execution::{
  AgentExecutionError, AgentExecutionService, AgentExecutionUseCases, AgentOutputFinalization, AgentOutputFinalizer,
  AppendAgentEventsInput, CompleteAgentLeaseInput,
};
pub use agents::agent_heartbeat::{
  AgentHeartbeatError, AgentHeartbeatInput, AgentHeartbeatService, AgentHeartbeatUseCases,
};
pub use agents::agent_placement::{
  AcquireAgentLeaseInput, AgentLeaseError, AgentLeaseOutcome, AgentLeaseService, AgentLeaseUseCases,
  FailAgentLeaseAssignmentInput, ReadyJobWaiter,
};
pub use agents::agent_registration::{
  AgentOperation, AgentRegistrationError, AgentRegistrationInput, AgentRegistrationOutcome, AgentRegistrationService,
  AgentRegistrationUseCases, AuthorizeAgentInput, AuthorizedAgent,
};
pub use agents::agent_telemetry::{
  AgentTelemetryBatch, AgentTelemetryError, AgentTelemetryExportError, AgentTelemetryExporter, AgentTelemetryInput,
  AgentTelemetryService, AgentTelemetryUseCases,
};
pub use agents::agent_tool_action::{
  AgentScopedValidationInput, AgentToolActionError, AgentToolActionInput, AgentToolActionService,
  AgentToolActionUseCases, FailClosedToolActionAuthorizer, LeaseBoundScopedValidation, LeaseBoundScopedValidationInput,
  LeaseBoundToolActionAuthorizer, LeaseBoundToolActionInput,
};
pub use agents::lease_expiry::{LeaseExpiryBatchOutcome, LeaseExpiryWorker};
pub use agents::pool_cqrs::{
  AgentPlatformProjection, AgentPoolAdmissionPolicyProjection, AgentPoolCommandOutcome, AgentPoolDrainStateProjection,
  AgentPoolFairnessPolicyProjection, AgentPoolHandlers, AgentPoolPageProjection, AgentPoolProjection,
  CreateAgentPoolCommand, DeleteAgentPoolCommand, DeleteAgentPoolCommandOutcome, GetAgentPoolQuery,
  ListAgentPoolsQuery, PoolExecutionModeProjection, PoolExecutionTargetProjection, PublishAgentPoolVersionCommand,
};
pub use artifacts::artifact_transfer::{
  AgentArtifactError, AgentArtifactTransferUseCases, ArtifactDownloadProjection, ArtifactHandlers, ArtifactProjection,
  ArtifactProjectionKind, AuthorizeArtifactDownloadQuery, AuthorizeProtectedInputsInput, BeginAgentArtifactUploadInput,
  CompleteAgentArtifactUploadInput, GetArtifactQuery, ListBuildArtifactsQuery,
};
pub use artifacts::cache_data_plane::{
  CacheDataPlaneError, CacheDataPlaneService, CacheDataPlaneUseCases, CacheRequestAuthority, CacheWriteResult,
};
pub use artifacts::cache_session::{
  AgentCacheSessionError, AgentCacheSessionUseCases, BeginAgentCacheSessionInput, CacheSessionDiagnosticState,
  CacheSessionHandlers, CacheSessionProjection, GetCacheSessionQuery, ListBuildCacheSessionsQuery,
  RevokeAgentCacheSessionInput, validate_cache_endpoint,
};
pub use builds::build_cqrs::{
  AttemptDetailsProjection, BuildDetailsProjection, BuildHandlers, CancelBuildCommand, CancelBuildCommandOutcome,
  GetAttemptQuery, GetBuildQuery, GetJobQuery, RetryBuildCommand, RetryBuildCommandOutcome,
};
pub use builds::build_discovery::{
  BuildListFilter, BuildPageCursor, BuildPageCursorError, BuildPageInputError, BuildPageProjection,
  BuildSummaryProjection, ListProjectBuildsQuery, MAX_BUILD_LIST_PAGE_SIZE, MAX_BUILD_PAGE_CURSOR_BYTES,
};
pub use builds::job_event_cqrs::{
  JobEventLongPoll, JobEventPageProjection, JobEventProjection, JobEventWaiter, MAX_JOB_EVENT_WAIT, ReadJobEventsQuery,
};
pub use builds::retry_policy::{DurableRetryPolicy, DurableRetryPolicyError};
pub use cqrs::{Command, CommandHandler, MutationDisposition, Query, QueryHandler};
pub use error::{ApplicationError, ApplicationFailure};
pub use factory::admission::factory_admission::{
  AdmitManualFactoryWorkCommand, FactoryAdmissionHandlers, FactoryAdmissionOutcome, FactoryWorkAdmissionIntent,
};
pub use factory::artifacts::factory_node_capture::{
  FactoryChangeSetError, VerifiedFactoryArtifact, factory_node_candidate_materialization, factory_node_change_set,
  restore_factory_node_change_set, verify_factory_node_change_set,
};
pub use factory::artifacts::factory_node_capture_build::FactoryNodeCaptureBuildAdapter;
pub use factory::authoring::factory_configuration_choices::{
  FactoryChoiceIdentity, FactoryChoiceMetadata, FactoryChoiceReferenceProjection,
  FactoryConfigurationChoicePageProjection, FactoryConfigurationChoiceProjection, FactoryConfigurationDocumentBounds,
  ListFactoryConfigurationChoicesQuery, factory_configuration_choice_kinds, factory_configuration_document_bounds,
  factory_flow_node_kinds, factory_permission_categories, factory_signal_fallbacks, factory_signal_modes,
  factory_signal_purposes,
};
pub use factory::authoring::factory_configuration_cqrs::{
  CreateFactoryConfigurationCommand, FactoryConfigurationCommandOutcome, FactoryConfigurationHandlers,
  FactoryConfigurationPagePosition, FactoryConfigurationPageProjection, FactoryConfigurationProjection,
  FactoryConfigurationValidation, FactoryEffectiveNodePermissions, FactoryEffectiveProfilePermissions,
  FactoryValidationDiagnostics, FactoryValidationIssue, GetCurrentFactoryConfigurationQuery,
  GetFactoryConfigurationQuery, ListProjectFactoryConfigurationsQuery, ReplaceFactoryConfigurationCommand,
  ValidateFactoryConfigurationQuery,
};
pub use factory::codex::factory_codex::{
  CodexCompilationError, CompiledCodexTask, FactoryCodexAuthentication, FactoryCodexFlowCompilationOptions,
  FactoryCodexReviewCompilation, FactoryProtectedDocument, FactoryProtectedDocumentKind, codex_flow_task_digest,
  codex_prompt_digest, codex_prompt_provenance_digest, codex_protected_test_task_digest, compile_codex_flow_task,
  compile_codex_flow_task_with_context, compile_codex_flow_task_with_options, compile_codex_node_task,
  compile_codex_reviewer, compile_codex_task,
};
pub use factory::codex::factory_codex_observation::{
  CodexHarnessOutcome, FactoryCodexEvaluationObservation, FactoryCodexImplementationObservation,
  FactoryCodexMcpNodeRunContract, FactoryCodexNativeCapture, FactoryCodexNativeCaptureContract,
  FactoryCodexNodeRunContract, FactoryCodexNodeRunObservation, FactoryCodexObservationStatus,
  FactoryCodexOutputDocument, FactoryCodexProvenanceContract, FactoryCodexReviewerBinding,
  FactoryCodexTerminalObservation, FactoryImplementationReport, FactoryMcpBoundCapture, FactoryMcpCallObservation,
  FactoryMcpCallStatus, FactoryMcpEvidenceReference, FactoryMcpObservationBinding, FactoryNativeCaptureStatus,
  factory_mcp_conclusion_shape, factory_mcp_evidence_verifier, observe_codex_evaluation, observe_codex_implementation,
  verify_codex_mcp_node_run, verify_codex_native_capture, verify_codex_node_run, verify_codex_review_node_run,
};
pub use factory::codex::factory_tool_action::{
  DeterministicToolActionRule, PreparedToolAction, ProtectedToolActionGate, ToolActionGateError, ToolRiskAssessment,
  ToolRiskSignalEvaluation, ToolRiskSignalEvaluator,
};
pub use factory::codex::factory_tool_risk_authorizer::{
  FactoryToolRiskAuthorizer, FactoryToolRiskBinding, FactoryToolRiskNodeProfile, factory_tool_risk_task_digest,
};
pub use factory::execution::factory_build::{
  CancelFactoryBuild, CreateFactoryBuild, FactoryBuildAcceptance, FactoryBuildObservation, FactoryBuildPolicyLayers,
  FactoryCandidateMaterialization, FactoryNodeBuildCausality, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
  OrdinaryBuildCancellationApplication,
};
pub use factory::execution::factory_control::{
  CancelFactoryRunCommand, ConfirmFactoryNodeOperationCommand, FactoryControlHandlers, FactoryRunControlCommandOutcome,
  MAX_FACTORY_OPERATOR_REASON_BYTES, ResolveFactoryNodeCommand, RetryFactoryNodeCommand, SelectFactoryProfileCommand,
  SubmitFactoryNodeResponseCommand,
};
pub use factory::execution::factory_decision_signal::{
  DecisionSignalApplicationError, DecisionSignalCapabilityDiscovery, DecisionSignalDispatch,
  DecisionSignalDispatchAuthorization, DecisionSignalPortError, DecisionSignalReceiptPublication,
  DecisionSignalRequestObservation, DecisionSignalService,
};
pub use factory::execution::factory_decision_signal_receipts::FactoryDecisionSignalReceipts;
pub use factory::execution::factory_job_spec::{
  FactoryManagedJobSpecInput, FactoryManagedJobSpecTemplateError, derive_factory_managed_job_spec_template,
};
pub use factory::execution::factory_reconciliation::{
  FactoryReconciler, FactoryReconciliationBatchOutcome, FactoryReconciliationError, FactoryReconciliationShutdown,
};
pub use factory::profiles::factory_credentials::{
  FactoryCredentialResolutionError, ScopedFactoryCredential, resolve_factory_credential,
};
pub use factory::sources::factory_source_admission::{
  AdmitPreparedSourceFactoryWorkCommand, AdmitSourceFactoryWorkCommand, FactorySourceAdmissionHandlers,
  FactorySourceAuthorization, FactorySourceAuthorizer, FactorySourceHandoff, FactorySourceItem,
  FactorySourcePredecessorTarget, FactorySourcePredecessors, IssueSourcePredecessorReader,
  RecoverSourceFactoryWorkCommand, source_predecessor_wait,
};
pub use factory::validation::factory_evaluation::FactoryEvaluationDecision;
pub use factory::validation::factory_evidence::{
  FactoryEvidenceAttestation, FactoryEvidenceError, VerifiedFactoryEvidence, construct_factory_evidence_manifest,
};
pub use management::configuration_cqrs::{
  BuildConfigurationCommandOutcome, BuildConfigurationHandlers, CreateBuildConfigurationCommand,
  CreateRepositoryCommand, GetBuildConfigurationQuery, GetRepositoryQuery, PublishBuildConfigurationVersionCommand,
  PublishRepositoryVersionCommand, RepositoryCommandOutcome,
};
pub use management::definition_cqrs::{
  CreateTriggerDefinitionCommand, DefinitionHandlers, GetManualTriggerDefinitionQuery,
  ManualTriggerDefinitionProjection, ProjectPolicyCommandOutcome, PublishProjectPolicyCommand,
  TriggerDefinitionCommandOutcome,
};
pub use management::definition_discovery::{
  BuildConfigurationPageProjection, BuildConfigurationSummaryProjection, CurrentDefinitionPageError,
  CurrentDefinitionPageInput, ListProjectBuildConfigurationsQuery, ListProjectPipelinesQuery,
  ListProjectRepositoriesQuery, ListProjectTriggerDefinitionsQuery, MAX_CURRENT_DEFINITION_PAGE_SIZE,
  PipelinePageProjection, PipelineSummaryProjection, RepositoryPageProjection, RepositorySummaryProjection,
  TriggerDefinitionKindProjection, TriggerDefinitionPageProjection, TriggerDefinitionSummaryProjection,
};
pub use management::management_input::{
  InternalTriggerDefinitionInput, ManagedWebhookInput, ManagementInputError, ManagementInputFactory,
  ManualTriggerDefinitionInput, ManualTriggerInput, ScheduledTriggerDefinitionInput, UnmanagedWebhookInput,
};
pub use management::pipeline_cqrs::{
  CreatePipelineCommand, GetPipelineQuery, PipelineCommandOutcome, PipelineHandlers, PublishPipelineVersionCommand,
};
pub use management::project_cqrs::{
  CreateProjectCommand, DeleteProjectCommand, DeleteProjectCommandOutcome, GetProjectQuery, ListProjectsQuery,
  MoveProjectCommand, ProjectCommandOutcome, ProjectHandlers, ProjectPageProjection, RenameProjectCommand,
};
pub use management::project_policy::{
  ArtifactPolicy, CacheNamespace, CachePolicy, ConcurrencyPolicy, EffectiveProjectPolicy, IdentityProfileName,
  MAX_POLICY_REFERENCE_BYTES, PolicyCategory, PolicyDirective, PolicyResolutionError, PolicySource,
  ProjectExecutionTarget, ProjectPolicy, ProjectPolicyDefinition, ProjectPolicyLayer, RetentionPolicy, RuntimeClass,
  SecretProfileName, resolve_project_policy,
};
pub use management::projections::{
  AgentRequirementsProjection, ArtifactPolicyProjection, AttemptProjection, BuildConfigurationProjection,
  BuildOriginProjection, BuildProjection, ConfigurationCacheProjection, ConfigurationRuntimeProjection,
  DagCausalityProjection, DagEdgeProjection, DagNodeProjection, DependencyPolicyProjection,
  ExecutionGuaranteeProjection, JobAssignmentProjection, JobExecutionProjection, JobFailureClassification,
  JobOutputKind, JobOutputReference, JobPlacementProjection, JobProjection, JobProjectionFacts, JobQueueProjection,
  JobTerminalOutcomeProjection, NetworkPolicyProjection, ParameterDefinitionProjection, ParameterSchemaProjection,
  ParameterTypeProjection, ParameterValueProjection, PipelineEdgeProjection, PipelineNodeProjection,
  PipelineProjection, PlatformArchitectureProjection, PlatformOsProjection, PlatformProjection,
  ProjectNavigationProjection, ProjectProjection, ProjectSummaryProjection, ProjectionError, RepositoryProjection,
  RepositorySelectionProjection, RetryClassProjection, RetryPolicyProjection, RuntimeClassProjection,
  Sha256DigestProjection, TriggerCauseProjection, TriggerHistoryProjection, TriggerKindProjection,
};
pub use management::resource_search::{
  MAX_RESOURCE_SEARCH_CURSOR_BYTES, MAX_RESOURCE_SEARCH_RESULT_CONTEXT_BYTES, MAX_RESOURCE_SEARCH_RESULT_LABEL_BYTES,
  MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE, ResourceSearchCursor, ResourceSearchCursorError, ResourceSearchHandlers,
  ResourceSearchIdentityProjection, ResourceSearchInputError, ResourceSearchKind, ResourceSearchKinds,
  ResourceSearchPageProjection, ResourceSearchSummaryProjection, SearchResourcesQuery,
};
pub use management_security::{
  AuthorizedCommandHandler, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler,
  AuthorizedQueryHandler, IssueReadPrincipal, IssueTitleSearchPage, MAX_ISSUE_TITLE_CURSOR_BYTES,
  MAX_ISSUE_TITLE_QUERY_BYTES, MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES, MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES,
  MAX_MANAGEMENT_SECURITY_SCOPE_BYTES, MAX_MANAGEMENT_VISIBILITY_RESOURCES, ManagementAction, ManagementActor,
  ManagementActorKind, ManagementAuthorizationDenial, ManagementAuthorizationFailure, ManagementAuthorizationGrant,
  ManagementAuthorizationMapping, ManagementAuthorizationPolicy, ManagementAuthorizationTarget, ManagementClientKind,
  ManagementCommandUseCase, ManagementHandlerError, ManagementIngress, ManagementQueryUseCase,
  ManagementRequestAttributes, ManagementRequestContext, ManagementRequestId, ManagementResource,
  ManagementResourceIdentity, ManagementResourceKind, ManagementResourcePattern, ManagementResourceResult,
  ManagementSecurityError, ManagementSecurityScope, ManagementVisibility, ManagementVisibilityError,
  ManagementVisibilityInput, ManagementVisibilityKind, ManagementVisibilityTarget, ManagementVisibilityView,
  TrustedNetworkManagementPolicy,
};
pub use octacity_server_cache::CacheCredentialKey;
pub use octacity_server_cache::{
  ActionResultV1, BlobDescriptor, BlobEncoding, Digest, DigestAlgorithm, FindMissingBlobsRequestV1,
  FindMissingBlobsResponseV1, MAX_ACTION_RESULT_WIRE_BYTES, MAX_REMOTE_CACHE_METADATA_BYTES,
  REMOTE_CACHE_BLOB_CONTENT_TYPE, REMOTE_CACHE_JSON_CONTENT_TYPE, REMOTE_CACHE_PROTOCOL_HEADER,
  REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1, WriteActionRequestV1,
};
pub use octacity_server_secrets::AgentEnrollmentSecretKey;
pub use octacity_server_store::RegistrationEpoch;
pub use octacity_server_store::{AgentDrainMode, TerminalBuildEvent};
pub use octacity_server_store::{LeaseGrant, LeaseHeartbeatOutcome};
pub use operations::audit_query::{
  AuditActorProjection, AuditCursorInput, AuditCursorProjection, AuditFactPageProjection, AuditFactProjection,
  AuditFactQueryInput, AuditQueries, ListAuditFactsQuery,
};
pub use operations::log_archive::LogRedactor;
pub use operations::log_indexing::{LogIndexingBatchOutcome, LogIndexingWorker, LogIndexingWorkerError};
pub use operations::log_search::{
  BuildLogSearch, BuildLogSearchCursorInput, BuildLogSearchCursorProjection, BuildLogSearchError,
  BuildLogSearchFreshnessProjection, BuildLogSearchHitProjection, BuildLogSearchInput, BuildLogSearchPageProjection,
  GetBuildLogFreshnessQuery, SearchBuildLogsQuery,
};
pub use operations::operational_metadata::{
  GetOperationalMetadataQuery, ManagementOperationalMetadataProjection, OperationalMetadataQueries,
};
pub use operations::operator_attention::{
  ListOperatorAttentionQuery, MAX_OPERATOR_ATTENTION_CODE_BYTES, MAX_OPERATOR_ATTENTION_CURSOR_BYTES,
  MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE, MAX_OPERATOR_ATTENTION_SCOPE_TARGETS, MAX_OPERATOR_ATTENTION_SUMMARY_BYTES,
  OperatorAttentionCategory, OperatorAttentionCursor, OperatorAttentionCursorError, OperatorAttentionHandlers,
  OperatorAttentionId, OperatorAttentionInputError, OperatorAttentionItemProjection, OperatorAttentionPageProjection,
  OperatorAttentionScopeInput, OperatorAttentionSeverity, OperatorAttentionTarget,
};
pub use operations::restore::{RestoreReconciler, RestoreReconciliationError, RestoreReconciliationSummary};
pub use operations::retention::{BuildRetentionBatchOutcome, BuildRetentionWorker, BuildRetentionWorkerError};
pub use operations::retention_hold::{
  BuildResultHoldProjection, BuildResultHoldStateProjection, BuildResultRetentionCommandOutcome,
  BuildResultRetentionDeadlinesProjection, BuildResultRetentionHandlers, BuildResultRetentionProjection,
  BuildResultVisibilityProjection, GetBuildResultRetentionQuery, MAX_BUILD_RESULT_HOLD_REASON_BYTES,
  PlaceBuildResultHoldCommand, ReleaseBuildResultHoldCommand, RetentionAuditIdentityProjection,
};
pub use transaction::CommandTransaction;
pub use triggers::external_trigger::{
  AcceptWebhookDeliveryCommand, AuthenticatedWebhookEvent, CreateManagedWebhookCommand, CreateUnmanagedWebhookCommand,
  DeleteManagedWebhookRegistrationCommand, ManagedWebhookProjection, ManagedWebhookRegistration,
  ManagedWebhookRegistrationRequest, ManagedWebhookRegistrationStatus, ObserveManagedWebhookRegistrationCommand,
  RotateManagedWebhookRegistrationCommand, UnmanagedWebhookProjection, VerifyWebhookDelivery, WebhookCallbackOrigin,
  WebhookDeliveryAccepted, WebhookDeliveryError, WebhookDeliveryFailure, WebhookDeliveryInputError,
  WebhookDeliveryVerifier, WebhookIngressService, WebhookManagementProvider, WebhookManagementService,
  WebhookVerificationError, WebhookVerificationFailure, WebhookVerificationRequirements,
};
pub use triggers::internal_trigger::{InternalTriggerBatchOutcome, InternalTriggerWorker, InternalTriggerWorkerError};
pub use triggers::internal_trigger_management::{
  CreateInternalTriggerCommand, GetInternalTriggerQuery, InternalTriggerCommandOutcome, InternalTriggerDefinition,
  InternalTriggerHandlers, InternalTriggerPageProjection, InternalTriggerProjection, InternalTriggerSourceStrategy,
  ListInternalTriggersQuery, PublishInternalTriggerVersionCommand,
};
pub use triggers::manual_trigger::{
  AcceptManualTriggerCommand, DurableManualTriggerService, EffectiveProjectPolicySource,
  EffectiveProjectPolicySourceError, ExactRevisionResolver, JobSpecToolchainPolicy, ManualSourceSelection,
  ManualTriggerCommand, ManualTriggerContext, ManualTriggerContextError, ManualTriggerContextProvider,
  ManualTriggerError, ManualTriggerInputError, ManualTriggerOutcome, ManualTriggerRetryBatchOutcome,
  ManualTriggerRetryWorker, ManualTriggerRetryWorkerError, ManualTriggerService, RepositorySourceSelection,
  RevisionResolutionError, RevisionResolutionRequest, RevisionResolver, StoreBackedEffectiveProjectPolicySource,
  StoreBackedManualTriggerContext,
};
pub use triggers::schedule_cqrs::{
  CreateScheduleCommand, GetScheduleQuery, ScheduleBatchOutcome, ScheduleCommandOutcome, ScheduleHandlers,
  ScheduleProjection, ScheduleWorker, ScheduleWorkerError, ScheduledBuildDefinition,
};
pub use triggers::webhook_delivery::{
  ManagedWebhookBatchOutcome, ManagedWebhookRegistrationWorker, WebhookDeliveryBatchOutcome, WebhookDeliveryWorker,
  WebhookWorkerError,
};

/// Maximum number of Projects accepted by one management list query.
pub const MAX_PROJECT_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_PROJECT_PAGE_SIZE;
/// Maximum number of Agent Pools accepted by one management list query.
pub const MAX_AGENT_POOL_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_AGENT_POOL_PAGE_SIZE;
/// Maximum number of Agents accepted by one management list query.
pub const MAX_AGENT_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_AGENT_PAGE_SIZE;
/// Maximum exact platforms accepted by one Agent Pool admission allowlist.
pub const MAX_AGENT_POOL_ADMISSION_PLATFORMS: usize = octacity_server_store::MAX_POOL_ADMISSION_PLATFORMS;
/// Maximum provider-neutral execution targets in one Agent Pool allowlist.
pub const MAX_AGENT_POOL_EXECUTION_TARGETS: usize = octacity_server_store::MAX_POOL_EXECUTION_TARGETS;
/// Maximum statically configured Agent capacity of one Pool.
pub const MAX_AGENT_POOL_STATIC_CAPACITY: u32 = octacity_server_store::MAX_POOL_STATIC_CAPACITY;
/// Maximum number of Job events accepted by one management read query.
pub const MAX_JOB_EVENT_PAGE_SIZE: u16 = octacity_server_store::MAX_JOB_EVENT_READ_PAGE_SIZE as u16;
/// Maximum published Artifacts accepted by one management list query.
pub const MAX_ARTIFACT_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_ARTIFACT_PAGE_SIZE;
/// Maximum internal Trigger definitions accepted by one management list query.
pub const MAX_INTERNAL_TRIGGER_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_INTERNAL_TRIGGER_PAGE_SIZE;
/// Maximum cache-session diagnostics accepted by one management list query.
pub const MAX_CACHE_SESSION_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_CACHE_SESSION_PAGE_SIZE;
/// Maximum UTF-8 bytes in a scheduled Trigger cron expression.
pub const MAX_SCHEDULE_EXPRESSION_BYTES: usize = octacity_server_trigger::MAX_SCHEDULE_EXPRESSION_BYTES;
/// Maximum UTF-8 bytes in a scheduled Trigger IANA timezone name.
pub const MAX_SCHEDULE_TIMEZONE_BYTES: usize = octacity_server_trigger::MAX_SCHEDULE_TIMEZONE_BYTES;
/// Maximum occurrences one schedule catch-up batch may evaluate.
pub const MAX_SCHEDULE_CATCH_UP: u16 = octacity_server_trigger::MAX_SCHEDULE_CATCH_UP;
/// Maximum provider header names one unmanaged webhook verifier may receive.
pub const MAX_WEBHOOK_VERIFICATION_HEADERS: usize = octacity_server_store::MAX_WEBHOOK_VERIFICATION_HEADERS;
/// Maximum exact raw webhook body bytes admitted before provider authentication.
pub const MAX_WEBHOOK_DELIVERY_BYTES: usize = octacity_server_store::MAX_STORED_WEBHOOK_BODY_BYTES;
/// Maximum retained bytes in one allowlisted webhook header name.
pub const MAX_WEBHOOK_HEADER_NAME_BYTES: usize = octacity_server_store::MAX_WEBHOOK_HEADER_NAME_BYTES;
/// Maximum retained bytes in one allowlisted webhook header value.
pub const MAX_WEBHOOK_HEADER_VALUE_BYTES: usize = octacity_server_store::MAX_WEBHOOK_HEADER_VALUE_BYTES;

pub use factory::authoring::factory_flow_definition::{
  FactoryFlowDefinitionDraft, FactoryFlowDefinitionHandlers, FactoryFlowDefinitionValidation, FactoryFlowRootDraft,
  ValidateFactoryFlowDataSchemaQuery, ValidateFactoryFlowDefinitionQuery, factory_native_gate_policies,
};

pub use factory::authoring::factory_flow_definition::{
  CreateFactoryFlowDefinitionCommand, FactoryFlowDefinitionCommandOutcome, GetFactoryFlowDefinitionQuery,
  ListProjectFactoryFlowDefinitionsQuery, ReplaceFactoryFlowDefinitionCommand,
};
pub use octacity_server_store::{FactoryFlowDefinitionPage, PublishedFactoryFlowDefinition};

/// Persistence mutation disposition used by exact Flow publication outcomes.
pub use octacity_server_store::MutationDisposition as StoreMutationDisposition;

pub use management::management_input::factory::ManualFactoryWorkInput;

pub use factory::authoring::factory_discovery::{
  FactoryAuditProjection, FactoryCallProjection, FactoryDiagnosticCollection, FactoryDiagnosticPage,
  FactoryDiagnosticRecord, FactoryDiscoveryHandlers, FactoryNodeAttemptProjection, FactoryNodeCompletionProjection,
  FactoryNodeStopProjection, FactoryProfileBuildProjection, FactoryProfileCandidateProjection,
  FactoryProfileControlProjection, FactoryProfileSelectionProjection, FactoryProfileSignalProjection,
  FactoryWorkProjection, GetFactoryRunQuery, ListFactoryDiagnosticsQuery, ListFactoryRunsQuery,
  MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,
};
pub use management::management_input::factory::{FactoryNodeControlInput, FactoryRunListInput};
pub use octacity_server_factory::{
  FlowIssueReferenceKind, ISSUE_OUTCOME_DUPLICATE_COVERAGE, ISSUE_OUTCOME_REVIEW_VERDICTS,
  ISSUE_OUTCOME_TRIAGE_DECISIONS, MAX_DECISION_ASSESSMENTS, MAX_DECISION_REASONS, MAX_DECISION_SIGNAL_RECEIPTS,
  MAX_FACTORY_SAFE_TEXT_BYTES, MAX_ISSUE_OUTCOME_CONCLUSION_BYTES, MAX_ISSUE_OUTCOME_REFERENCES,
  MAX_ISSUE_OUTCOME_TITLE_BYTES, MAX_WORK_PRIORITY, flow_issue_builtin_contracts,
};
pub use octacity_server_store::{
  FactoryRunPage as FactoryRunPageProjection, FactoryRunSummary as FactoryRunSummaryProjection,
  MAX_FACTORY_RUN_SUMMARY_NODES,
};

pub use factory::authoring::factory_discovery::ListFactoryPhaseReadyQuery;
pub use octacity_server_store::PhaseReadyPage as FactoryPhaseReadyPageProjection;

pub use factory::authoring::factory_configuration_choices::{
  factory_decision_outcomes, factory_delivery_states, factory_finding_severities,
};

pub use factory::validation::factory_protected_test_context::{
  FactoryProtectedTestContext, FactoryProtectedTestObservationInputSource, FactoryProtectedTestObservationInputs,
  FactoryStoredProtectedTestContextSource,
};

pub use factory::validation::factory_protected_validation_input_source::FactoryStoredProtectedValidationInputSource;

pub use factory::execution::factory_phase_pool_selection::{FactoryPhasePoolCoordination, FactoryPhasePoolSelector};

pub use factory::commands::factory_command_observation::{
  FactoryCommandFinding, FactoryCommandObservationBinding, FactoryCommandObservationContract,
  verify_factory_command_observation,
};
pub use factory::commands::factory_command_output_source::FactoryCommandBuildOutputSource;

pub use factory::commands::factory_command::{
  CompiledFactoryCommandTask, FactoryCommandBuildProfile, FactoryCommandOctafile, FactoryCommandProgram,
  FactoryManagedCommandProof, MAX_FACTORY_COMMAND_OCTAFILE_BYTES, compile_factory_command_task,
};
pub use factory::commands::factory_command_materializer::FactoryCommandBuildMaterializer;
pub use factory::commands::factory_deployment_command::{
  FactoryDeploymentCommandAuthority, FactoryDeploymentCommandProfile, compile_factory_deployment_command_task,
};

pub use factory::codex::factory_codex_review_build::FactoryCodexReviewBuildAdapter;

pub use integrations::integration_administration::*;

pub use integrations::integration_connection::{
  GetIntegrationConnectionQuery, IntegrationConnectionHandlers, IntegrationConnectionPage, IntegrationId,
  IntegrationInstallationPage, IntegrationVersion, ListIntegrationConnectionsQuery, ListIntegrationInstallationsQuery,
  MAX_INTEGRATION_AUTHENTICATION_METHODS, MAX_INTEGRATION_DISCOVERY_PAGE_SIZE, MAX_INTEGRATION_ENDPOINT_BYTES,
  MAX_INTEGRATION_ENDPOINTS, MAX_INTEGRATION_NAME_BYTES, PublishIntegrationConnectionCommand,
  ValidateIntegrationConnectionQuery,
};

pub use integrations::integration_project_access::*;

pub use integrations::integration_check::*;

pub use factory::sources::factory_source_configuration::*;

pub use factory::reporting::factory_issue_status_authoring::FactoryIssueStatusAuthorizer;

pub use issues::issue_legacy::*;

pub use factory::profiles::factory_role_profiles::*;

pub use factory::authoring::factory_authoring_capabilities::*;
pub use factory::authoring::factory_constraints::*;
pub use factory::profiles::factory_execution_bindings::*;

pub use factory::profiles::factory_role_profile_catalog::*;

pub use factory::profiles::factory_role_catalog::*;

pub use factory::profiles::factory_skill_import::*;

pub use factory::codex::factory_codex_role_source::FactoryStoredCodexRoleInvocationSource;
pub use factory::profiles::factory_role_invocation::*;

pub use factory::codex::factory_codex_mcp::{FactoryCodexMcpConfiguration, FactoryCodexMcpServer};

pub use factory::profiles::factory_runtime_settings::*;

pub use factory::authoring::factory_project_policies::*;
