//! Transport-independent OctaCity server use cases.
//!
//! Typed commands, queries, transaction coordination, projections, and
//! application error mapping belong here. Transport and concrete persistence
//! types must remain outside this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod agent_cqrs;
mod agent_enrollment;
mod agent_execution;
mod agent_heartbeat;
mod agent_lease;
mod agent_placement;
mod agent_registration;
mod agent_telemetry;
#[cfg(test)]
mod agent_telemetry_tests;
mod agent_tool_action;
mod artifact_transfer;
#[cfg(test)]
mod artifact_transfer_tests;
mod audit_query;
mod build_cqrs;
mod build_discovery;
mod cache_data_plane;
mod cache_session;
#[cfg(test)]
mod cache_session_tests;
mod configuration_cqrs;
mod cqrs;
mod definition_cqrs;
mod definition_discovery;
mod diagnostic;
mod error;
mod external_trigger;
mod factory_admission;
mod factory_triage;
#[cfg(test)]
mod factory_triage_tests;
pub use factory_triage::{
  FactoryTriageCoordinator, FactoryTriageDiscovery, FactoryTriageEvidenceValidator, FactoryTriagePhaseExecutor,
  FactoryTriagePhaseInput, FactoryTriagePhaseRequest, FactoryTriagePhaseResult, FactoryTriageRunner, FactoryTriageStep,
};
#[cfg(test)]
mod factory_admission_tests;
mod factory_build_bridge;
#[cfg(test)]
mod factory_build_bridge_tests;
mod factory_changeset;
mod factory_codex;
mod factory_codex_contract;
mod factory_codex_observation;
#[cfg(test)]
mod factory_codex_observation_tests;
#[cfg(test)]
mod factory_codex_tests;
mod factory_configuration_cqrs;
#[cfg(test)]
mod factory_configuration_cqrs_tests;
mod factory_context;
#[cfg(test)]
mod factory_context_tests;
mod factory_control;
#[cfg(test)]
mod factory_control_tests;
mod factory_credentials;
#[cfg(test)]
mod factory_credentials_tests;
mod factory_decision_signal;
#[cfg(test)]
mod factory_decision_signal_tests;
mod factory_evaluation;
mod factory_evidence;
mod factory_job_spec;
#[cfg(test)]
mod factory_job_spec_tests;
mod factory_reconciliation;
#[cfg(test)]
mod factory_reconciliation_tests;
mod factory_tool_action;
#[cfg(test)]
mod factory_tool_action_tests;
#[cfg(test)]
mod factory_vertical_slice_tests;
mod internal_trigger;
mod internal_trigger_management;
mod job_event_cqrs;
mod lease_expiry;
mod log_archive;
mod log_indexing;
mod log_search;
mod management_input;
mod management_security;
mod manual_trigger;
mod opaque_cursor;
mod operational_metadata;
mod operator_attention;
mod pipeline_cqrs;
mod pool_cqrs;
mod project_cqrs;
mod project_policy;
mod projections;
mod resource_search;
mod restore;
mod retention;
mod retention_hold;
mod retry_policy;
mod schedule_cqrs;
mod snapshots;
mod telemetry;
#[cfg(test)]
mod test_support;
mod transaction;
mod webhook_delivery;

pub use octacity_server_domain::{RetentionHoldVersion, Timestamp};
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

pub use agent_cqrs::{
  AgentCapacityProjection, AgentCommandOutcome, AgentCurrentExecutionProjection, AgentCurrentLeaseStateProjection,
  AgentHandlers, AgentInventoryProjection, AgentPageProjection, AgentProjection, AgentStatusProjection,
  DrainAgentCommand, GetAgentQuery, ListAgentsQuery, ReassignAgentPoolCommand,
};
pub use agent_enrollment::{AgentEnrollmentHandler, IssueAgentEnrollmentCommand, IssueAgentEnrollmentCommandOutcome};
pub use agent_execution::{
  AgentExecutionError, AgentExecutionService, AgentExecutionUseCases, AppendAgentEventsInput, CompleteAgentLeaseInput,
};
pub use agent_heartbeat::{AgentHeartbeatError, AgentHeartbeatInput, AgentHeartbeatService, AgentHeartbeatUseCases};
pub use agent_placement::{
  AcquireAgentLeaseInput, AgentLeaseError, AgentLeaseOutcome, AgentLeaseService, AgentLeaseUseCases,
  FailAgentLeaseAssignmentInput, ReadyJobWaiter,
};
pub use agent_registration::{
  AgentOperation, AgentRegistrationError, AgentRegistrationInput, AgentRegistrationOutcome, AgentRegistrationService,
  AgentRegistrationUseCases, AuthorizeAgentInput, AuthorizedAgent,
};
pub use agent_telemetry::{
  AgentTelemetryBatch, AgentTelemetryError, AgentTelemetryExportError, AgentTelemetryExporter, AgentTelemetryInput,
  AgentTelemetryService, AgentTelemetryUseCases,
};
pub use agent_tool_action::{
  AgentToolActionError, AgentToolActionInput, AgentToolActionService, AgentToolActionUseCases,
  FailClosedToolActionAuthorizer, LeaseBoundToolActionAuthorizer, LeaseBoundToolActionInput,
};
pub use artifact_transfer::{
  AgentArtifactError, AgentArtifactTransferUseCases, ArtifactDownloadProjection, ArtifactHandlers, ArtifactProjection,
  ArtifactProjectionKind, AuthorizeArtifactDownloadQuery, AuthorizeProtectedInputsInput, BeginAgentArtifactUploadInput,
  CompleteAgentArtifactUploadInput, GetArtifactQuery, ListBuildArtifactsQuery,
};
pub use audit_query::{
  AuditActorProjection, AuditCursorInput, AuditCursorProjection, AuditFactPageProjection, AuditFactProjection,
  AuditFactQueryInput, AuditQueries, ListAuditFactsQuery,
};
pub use build_cqrs::{
  AttemptDetailsProjection, BuildDetailsProjection, BuildHandlers, CancelBuildCommand, CancelBuildCommandOutcome,
  GetAttemptQuery, GetBuildQuery, GetJobQuery, RetryBuildCommand, RetryBuildCommandOutcome,
};
pub use build_discovery::{
  BuildListFilter, BuildPageCursor, BuildPageCursorError, BuildPageInputError, BuildPageProjection,
  BuildSummaryProjection, ListProjectBuildsQuery, MAX_BUILD_LIST_PAGE_SIZE, MAX_BUILD_PAGE_CURSOR_BYTES,
};
pub use cache_data_plane::{
  CacheDataPlaneError, CacheDataPlaneService, CacheDataPlaneUseCases, CacheRequestAuthority, CacheWriteResult,
};
pub use cache_session::{
  AgentCacheSessionError, AgentCacheSessionUseCases, BeginAgentCacheSessionInput, CacheSessionDiagnosticState,
  CacheSessionHandlers, CacheSessionProjection, GetCacheSessionQuery, ListBuildCacheSessionsQuery,
  RevokeAgentCacheSessionInput, validate_cache_endpoint,
};
pub use configuration_cqrs::{
  BuildConfigurationCommandOutcome, BuildConfigurationHandlers, CreateBuildConfigurationCommand,
  CreateRepositoryCommand, GetBuildConfigurationQuery, GetRepositoryQuery, PublishBuildConfigurationVersionCommand,
  PublishRepositoryVersionCommand, RepositoryCommandOutcome,
};
pub use cqrs::{Command, CommandHandler, MutationDisposition, Query, QueryHandler};
pub use definition_cqrs::{
  CreateTriggerDefinitionCommand, DefinitionHandlers, GetManualTriggerDefinitionQuery,
  ManualTriggerDefinitionProjection, ProjectPolicyCommandOutcome, PublishProjectPolicyCommand,
  TriggerDefinitionCommandOutcome,
};
pub use definition_discovery::{
  BuildConfigurationPageProjection, BuildConfigurationSummaryProjection, CurrentDefinitionPageError,
  CurrentDefinitionPageInput, ListProjectBuildConfigurationsQuery, ListProjectPipelinesQuery,
  ListProjectRepositoriesQuery, ListProjectTriggerDefinitionsQuery, MAX_CURRENT_DEFINITION_PAGE_SIZE,
  PipelinePageProjection, PipelineSummaryProjection, RepositoryPageProjection, RepositorySummaryProjection,
  TriggerDefinitionKindProjection, TriggerDefinitionPageProjection, TriggerDefinitionSummaryProjection,
};
pub use error::{ApplicationError, ApplicationFailure};
pub use external_trigger::{
  AcceptWebhookDeliveryCommand, AuthenticatedWebhookEvent, CreateManagedWebhookCommand, CreateUnmanagedWebhookCommand,
  DeleteManagedWebhookRegistrationCommand, ManagedWebhookProjection, ManagedWebhookRegistration,
  ManagedWebhookRegistrationRequest, ManagedWebhookRegistrationStatus, ObserveManagedWebhookRegistrationCommand,
  RotateManagedWebhookRegistrationCommand, UnmanagedWebhookProjection, VerifyWebhookDelivery, WebhookCallbackOrigin,
  WebhookDeliveryAccepted, WebhookDeliveryError, WebhookDeliveryFailure, WebhookDeliveryInputError,
  WebhookDeliveryVerifier, WebhookIngressService, WebhookManagementProvider, WebhookManagementService,
  WebhookVerificationError, WebhookVerificationFailure, WebhookVerificationRequirements,
};
pub use factory_admission::{AdmitManualFactoryWorkCommand, FactoryAdmissionHandlers, FactoryAdmissionOutcome};
pub use factory_build_bridge::{
  CreateFactoryBuild, FactoryBuildAcceptance, FactoryBuildBridge, FactoryBuildBridgeError, FactoryBuildCausality,
  FactoryBuildDispatchOutcome, FactoryBuildObservation, FactoryBuildOutputSource, FactoryBuildPolicyLayers,
  FactoryBuildPolicyRequest, FactoryBuildPolicySource, FactoryBuildPolicySourceError, FactoryCandidateMaterialization,
  OrdinaryBuildApplication, OrdinaryBuildApplicationError,
};
pub use factory_changeset::{
  FactoryChangeSetAcceptance, FactoryChangeSetError, VerifiedFactoryArtifact, accept_factory_change_set,
};
pub use factory_codex::{
  CodexCompilationError, CompiledCodexTask, FactoryProtectedDocument, FactoryProtectedDocumentKind,
  codex_prompt_digest, codex_prompt_provenance_digest, compile_codex_reviewer, compile_codex_task,
};
pub use factory_codex_observation::{
  CodexHarnessOutcome, FactoryCodexEvaluationCompletion, FactoryCodexEvaluationObservation,
  FactoryCodexImplementationObservation, FactoryCodexObservationStatus, FactoryCodexOutputDocument,
  FactoryCodexReviewerBinding, FactoryCodexTerminalObservation, FactoryImplementationReport, observe_codex_evaluation,
  observe_codex_implementation,
};
pub use factory_configuration_cqrs::{
  CreateFactoryConfigurationCommand, FactoryConfigurationCommandOutcome, FactoryConfigurationHandlers,
  FactoryConfigurationProjection, FactoryConfigurationValidation, GetCurrentFactoryConfigurationQuery,
  GetFactoryConfigurationQuery, ReplaceFactoryConfigurationCommand, ValidateFactoryConfigurationQuery,
};
pub use factory_context::{
  FactoryContextError, FactoryContextSelection, PrepareFactoryCallContext, PreparedFactoryCallContext,
  prepare_factory_call_context,
};
pub use factory_control::{
  CancelFactoryRunCommand, FactoryControlHandlers, FactoryRunControlCommandOutcome, RequestFactoryDeliveryCommand,
  ResolveFactoryEscalationCommand, RetryFactoryStageCommand,
};
pub use factory_credentials::{FactoryCredentialResolutionError, ScopedFactoryCredential, resolve_factory_credential};
pub use factory_decision_signal::{
  DecisionSignalApplicationError, DecisionSignalCapabilityDiscovery, DecisionSignalDispatch, DecisionSignalPortError,
  DecisionSignalReceiptPublication, DecisionSignalRequestObservation, DecisionSignalService,
};
pub use factory_evaluation::FactoryEvaluationDecision;
pub use factory_evidence::{
  FactoryEvidenceAttestation, FactoryEvidenceError, VerifiedFactoryEvidence, construct_factory_evidence_manifest,
};
pub use factory_job_spec::{
  FactoryManagedJobSpecInput, FactoryManagedJobSpecTemplateError, derive_factory_managed_job_spec_template,
};
pub use factory_reconciliation::{
  FactoryReconciler, FactoryReconciliationBatchOutcome, FactoryReconciliationError, FactoryReconciliationShutdown,
};
pub use factory_tool_action::{
  DeterministicToolActionRule, PreparedToolAction, ProtectedToolActionGate, ToolActionGateError, ToolRiskAssessment,
  ToolRiskSignalEvaluation, ToolRiskSignalEvaluator,
};
pub use internal_trigger::{InternalTriggerBatchOutcome, InternalTriggerWorker, InternalTriggerWorkerError};
pub use internal_trigger_management::{
  CreateInternalTriggerCommand, GetInternalTriggerQuery, InternalTriggerCommandOutcome, InternalTriggerDefinition,
  InternalTriggerHandlers, InternalTriggerPageProjection, InternalTriggerProjection, InternalTriggerSourceStrategy,
  ListInternalTriggersQuery, PublishInternalTriggerVersionCommand,
};
pub use job_event_cqrs::{
  JobEventLongPoll, JobEventPageProjection, JobEventProjection, JobEventWaiter, MAX_JOB_EVENT_WAIT, ReadJobEventsQuery,
};
pub use lease_expiry::{LeaseExpiryBatchOutcome, LeaseExpiryWorker};
pub use log_archive::LogRedactor;
pub use log_indexing::{LogIndexingBatchOutcome, LogIndexingWorker, LogIndexingWorkerError};
pub use log_search::{
  BuildLogSearch, BuildLogSearchCursorInput, BuildLogSearchCursorProjection, BuildLogSearchError,
  BuildLogSearchFreshnessProjection, BuildLogSearchHitProjection, BuildLogSearchInput, BuildLogSearchPageProjection,
  GetBuildLogFreshnessQuery, SearchBuildLogsQuery,
};
pub use management_input::{
  InternalTriggerDefinitionInput, ManagedWebhookInput, ManagementInputError, ManagementInputFactory,
  ManualTriggerDefinitionInput, ManualTriggerInput, ScheduledTriggerDefinitionInput, UnmanagedWebhookInput,
};
pub use management_security::{
  AuthorizedCommandHandler, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler,
  AuthorizedQueryHandler, MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES, MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES,
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
pub use manual_trigger::{
  AcceptManualTriggerCommand, DurableManualTriggerService, EffectiveProjectPolicySource,
  EffectiveProjectPolicySourceError, ExactRevisionResolver, JobSpecToolchainPolicy, ManualSourceSelection,
  ManualTriggerCommand, ManualTriggerContext, ManualTriggerContextError, ManualTriggerContextProvider,
  ManualTriggerError, ManualTriggerInputError, ManualTriggerOutcome, ManualTriggerRetryBatchOutcome,
  ManualTriggerRetryWorker, ManualTriggerRetryWorkerError, ManualTriggerService, RepositorySourceSelection,
  RevisionResolutionError, RevisionResolutionRequest, RevisionResolver, StoreBackedEffectiveProjectPolicySource,
  StoreBackedManualTriggerContext,
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
pub use operational_metadata::{
  GetOperationalMetadataQuery, ManagementOperationalMetadataProjection, OperationalMetadataQueries,
};
pub use operator_attention::{
  ListOperatorAttentionQuery, MAX_OPERATOR_ATTENTION_CODE_BYTES, MAX_OPERATOR_ATTENTION_CURSOR_BYTES,
  MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE, MAX_OPERATOR_ATTENTION_SCOPE_TARGETS, MAX_OPERATOR_ATTENTION_SUMMARY_BYTES,
  OperatorAttentionCategory, OperatorAttentionCursor, OperatorAttentionCursorError, OperatorAttentionHandlers,
  OperatorAttentionId, OperatorAttentionInputError, OperatorAttentionItemProjection, OperatorAttentionPageProjection,
  OperatorAttentionScopeInput, OperatorAttentionSeverity, OperatorAttentionTarget,
};
pub use pipeline_cqrs::{
  CreatePipelineCommand, GetPipelineQuery, PipelineCommandOutcome, PipelineHandlers, PublishPipelineVersionCommand,
};
pub use pool_cqrs::{
  AgentPlatformProjection, AgentPoolAdmissionPolicyProjection, AgentPoolCommandOutcome, AgentPoolDrainStateProjection,
  AgentPoolFairnessPolicyProjection, AgentPoolHandlers, AgentPoolPageProjection, AgentPoolProjection,
  CreateAgentPoolCommand, DeleteAgentPoolCommand, DeleteAgentPoolCommandOutcome, GetAgentPoolQuery,
  ListAgentPoolsQuery, PoolExecutionModeProjection, PoolExecutionTargetProjection, PublishAgentPoolVersionCommand,
};
pub use project_cqrs::{
  CreateProjectCommand, DeleteProjectCommand, DeleteProjectCommandOutcome, GetProjectQuery, ListProjectsQuery,
  MoveProjectCommand, ProjectCommandOutcome, ProjectHandlers, ProjectPageProjection, RenameProjectCommand,
};
pub use project_policy::{
  ArtifactPolicy, CacheNamespace, CachePolicy, ConcurrencyPolicy, EffectiveProjectPolicy, IdentityProfileName,
  MAX_POLICY_REFERENCE_BYTES, PolicyCategory, PolicyDirective, PolicyResolutionError, PolicySource,
  ProjectExecutionTarget, ProjectPolicy, ProjectPolicyDefinition, ProjectPolicyLayer, RetentionPolicy, RuntimeClass,
  SecretProfileName, resolve_project_policy,
};
pub use projections::{
  AgentRequirementsProjection, ArtifactPolicyProjection, AttemptProjection, BuildConfigurationProjection,
  BuildProjection, ConfigurationCacheProjection, ConfigurationRuntimeProjection, DagCausalityProjection,
  DagEdgeProjection, DagNodeProjection, DependencyPolicyProjection, ExecutionGuaranteeProjection,
  JobAssignmentProjection, JobExecutionProjection, JobFailureClassification, JobOutputKind, JobOutputReference,
  JobPlacementProjection, JobProjection, JobProjectionFacts, JobQueueProjection, JobTerminalOutcomeProjection,
  NetworkPolicyProjection, ParameterDefinitionProjection, ParameterSchemaProjection, ParameterTypeProjection,
  ParameterValueProjection, PipelineEdgeProjection, PipelineNodeProjection, PipelineProjection,
  PlatformArchitectureProjection, PlatformOsProjection, PlatformProjection, ProjectNavigationProjection,
  ProjectProjection, ProjectSummaryProjection, ProjectionError, RepositoryProjection, RepositorySelectionProjection,
  RetryClassProjection, RetryPolicyProjection, RuntimeClassProjection, Sha256DigestProjection, TriggerCauseProjection,
  TriggerHistoryProjection, TriggerKindProjection,
};
pub use resource_search::{
  MAX_RESOURCE_SEARCH_CURSOR_BYTES, MAX_RESOURCE_SEARCH_RESULT_CONTEXT_BYTES, MAX_RESOURCE_SEARCH_RESULT_LABEL_BYTES,
  MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE, ResourceSearchCursor, ResourceSearchCursorError, ResourceSearchHandlers,
  ResourceSearchIdentityProjection, ResourceSearchInputError, ResourceSearchKind, ResourceSearchKinds,
  ResourceSearchPageProjection, ResourceSearchSummaryProjection, SearchResourcesQuery,
};
pub use restore::{RestoreReconciler, RestoreReconciliationError, RestoreReconciliationSummary};
pub use retention::{BuildRetentionBatchOutcome, BuildRetentionWorker, BuildRetentionWorkerError};
pub use retention_hold::{
  BuildResultHoldProjection, BuildResultHoldStateProjection, BuildResultRetentionCommandOutcome,
  BuildResultRetentionDeadlinesProjection, BuildResultRetentionHandlers, BuildResultRetentionProjection,
  BuildResultVisibilityProjection, GetBuildResultRetentionQuery, MAX_BUILD_RESULT_HOLD_REASON_BYTES,
  PlaceBuildResultHoldCommand, ReleaseBuildResultHoldCommand, RetentionAuditIdentityProjection,
};
pub use retry_policy::{DurableRetryPolicy, DurableRetryPolicyError};
pub use schedule_cqrs::{
  CreateScheduleCommand, GetScheduleQuery, ScheduleBatchOutcome, ScheduleCommandOutcome, ScheduleHandlers,
  ScheduleProjection, ScheduleWorker, ScheduleWorkerError, ScheduledBuildDefinition,
};
pub use transaction::CommandTransaction;
pub use webhook_delivery::{
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
