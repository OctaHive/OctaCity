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
mod artifact_transfer;
#[cfg(test)]
mod artifact_transfer_tests;
mod audit_query;
mod build_cqrs;
mod cache_data_plane;
mod cache_session;
#[cfg(test)]
mod cache_session_tests;
mod configuration_cqrs;
mod cqrs;
mod definition_cqrs;
mod diagnostic;
mod error;
mod external_trigger;
mod internal_trigger;
mod internal_trigger_management;
mod job_event_cqrs;
mod lease_expiry;
mod log_archive;
mod log_indexing;
mod log_search;
mod management_input;
mod manual_trigger;
mod pipeline_cqrs;
mod pool_cqrs;
mod project_cqrs;
mod project_policy;
mod projections;
mod retention;
mod retention_hold;
mod retry_policy;
mod schedule_cqrs;
mod snapshots;
mod telemetry;
mod transaction;
mod webhook_delivery;

pub use octacity_server_domain::{RetentionHoldVersion, Timestamp};
pub use octacity_server_store::{
  AuditActorKind, AuditOutcome, BuildLogStream, LogIndexPosition, LogSearchCursor, LogSearchError, LogSearchMode,
  LogSearchQuery, MAX_AUDIT_ACTOR_IDENTITY_BYTES, MAX_AUDIT_OPERATION_BYTES, MAX_AUDIT_PAGE_SIZE,
  MAX_AUDIT_REQUEST_IDENTITY_BYTES, MAX_AUDIT_TARGET_IDENTITY_BYTES, MAX_AUDIT_TARGET_KIND_BYTES,
  MAX_LOG_SEARCH_PAGE_SIZE, MAX_LOG_SEARCH_QUERY_BYTES, MAX_LOG_SEARCH_SNIPPET_BYTES, StoreError,
};

pub use agent_cqrs::{
  AgentCapacityProjection, AgentCommandOutcome, AgentHandlers, AgentInventoryProjection, AgentPageProjection,
  AgentProjection, AgentStatusProjection, DrainAgentCommand, GetAgentQuery, ListAgentsQuery, ReassignAgentPoolCommand,
};
pub use agent_enrollment::{AgentEnrollmentHandler, IssueAgentEnrollmentCommand, IssueAgentEnrollmentCommandOutcome};
pub use agent_execution::{
  AgentExecutionError, AgentExecutionService, AgentExecutionUseCases, AppendAgentEventsInput, CompleteAgentLeaseInput,
};
pub use agent_heartbeat::{AgentHeartbeatError, AgentHeartbeatInput, AgentHeartbeatService, AgentHeartbeatUseCases};
pub use agent_placement::{
  AcquireAgentLeaseInput, AgentLeaseError, AgentLeaseOutcome, AgentLeaseService, AgentLeaseUseCases, ReadyJobWaiter,
};
pub use agent_registration::{
  AgentOperation, AgentRegistrationError, AgentRegistrationInput, AgentRegistrationOutcome, AgentRegistrationService,
  AgentRegistrationUseCases, AuthorizeAgentInput, AuthorizedAgent,
};
pub use agent_telemetry::{
  AgentTelemetryBatch, AgentTelemetryError, AgentTelemetryExportError, AgentTelemetryExporter, AgentTelemetryInput,
  AgentTelemetryService, AgentTelemetryUseCases,
};
pub use artifact_transfer::{
  AgentArtifactError, AgentArtifactTransferUseCases, ArtifactDownloadProjection, ArtifactHandlers, ArtifactProjection,
  ArtifactProjectionKind, AuthorizeArtifactDownloadQuery, BeginAgentArtifactUploadInput,
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
  CreateTriggerDefinitionCommand, DefinitionHandlers, ProjectPolicyCommandOutcome, PublishProjectPolicyCommand,
  TriggerDefinitionCommandOutcome,
};
pub use error::{ApplicationError, ApplicationFailure};
pub use external_trigger::{
  AcceptWebhookDeliveryCommand, AuthenticatedWebhookEvent, CreateManagedWebhookCommand, CreateUnmanagedWebhookCommand,
  ManageWebhookRegistrationCommand, ManagedWebhookProjection, ManagedWebhookRegistration,
  ManagedWebhookRegistrationRequest, ManagedWebhookRegistrationStatus, UnmanagedWebhookProjection,
  VerifyWebhookDelivery, WebhookCallbackOrigin, WebhookDeliveryAccepted, WebhookDeliveryError, WebhookDeliveryFailure,
  WebhookDeliveryInputError, WebhookDeliveryVerifier, WebhookIngressService, WebhookManagementProvider,
  WebhookManagementService, WebhookVerificationError, WebhookVerificationFailure, WebhookVerificationRequirements,
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
pub use manual_trigger::{
  AcceptManualTriggerCommand, DurableManualTriggerService, EffectiveProjectPolicySource,
  EffectiveProjectPolicySourceError, ExactRevisionResolver, JobSpecToolchainPolicy, ManualSourceSelection,
  ManualTriggerCommand, ManualTriggerContext, ManualTriggerContextError, ManualTriggerContextProvider,
  ManualTriggerError, ManualTriggerInputError, ManualTriggerOutcome, ManualTriggerRetryBatchOutcome,
  ManualTriggerRetryWorker, ManualTriggerRetryWorkerError, ManualTriggerService, RevisionResolutionError,
  RevisionResolutionRequest, RevisionResolver, StoreBackedEffectiveProjectPolicySource,
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
pub use pipeline_cqrs::{
  CreatePipelineCommand, GetPipelineQuery, PipelineCommandOutcome, PipelineHandlers, PublishPipelineVersionCommand,
};
pub use pool_cqrs::{
  AgentPlatformProjection, AgentPoolAdmissionPolicyProjection, AgentPoolCommandOutcome, AgentPoolDrainStateProjection,
  AgentPoolFairnessPolicyProjection, AgentPoolHandlers, AgentPoolPageProjection, AgentPoolProjection,
  CreateAgentPoolCommand, DeleteAgentPoolCommand, DeleteAgentPoolCommandOutcome, GetAgentPoolQuery,
  ListAgentPoolsQuery, PublishAgentPoolVersionCommand,
};
pub use project_cqrs::{
  CreateProjectCommand, DeleteProjectCommand, DeleteProjectCommandOutcome, GetProjectQuery, ListProjectsQuery,
  MoveProjectCommand, ProjectCommandOutcome, ProjectHandlers, ProjectPageProjection, RenameProjectCommand,
};
pub use project_policy::{
  ArtifactPolicy, CacheNamespace, CachePolicy, ConcurrencyPolicy, EffectiveProjectPolicy, IdentityProfileName,
  MAX_POLICY_REFERENCE_BYTES, PolicyCategory, PolicyDirective, PolicyResolutionError, PolicySource, ProjectPolicy,
  ProjectPolicyDefinition, ProjectPolicyLayer, RetentionPolicy, RuntimeClass, SecretProfileName,
  resolve_project_policy,
};
pub use projections::{
  AgentRequirementsProjection, ArtifactPolicyProjection, AttemptProjection, BuildConfigurationProjection,
  BuildProjection, ConfigurationCacheProjection, ConfigurationRuntimeProjection, DagCausalityProjection,
  DagEdgeProjection, DagNodeProjection, DependencyPolicyProjection, JobAssignmentProjection, JobExecutionProjection,
  JobFailureClassification, JobOutputKind, JobOutputReference, JobPlacementProjection, JobProjection,
  JobProjectionFacts, JobQueueProjection, JobTerminalOutcomeProjection, NetworkPolicyProjection,
  ParameterDefinitionProjection, ParameterSchemaProjection, ParameterTypeProjection, ParameterValueProjection,
  PipelineEdgeProjection, PipelineNodeProjection, PipelineProjection, PlatformArchitectureProjection,
  PlatformOsProjection, ProjectProjection, ProjectSummaryProjection, ProjectionError, RepositoryProjection,
  RepositorySelectionProjection, RetryClassProjection, RetryPolicyProjection, RuntimeClassProjection,
  Sha256DigestProjection, TriggerCauseProjection, TriggerHistoryProjection, TriggerKindProjection,
};
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
