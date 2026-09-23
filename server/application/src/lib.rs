//! Transport-independent OctaCity server use cases.
//!
//! Typed commands, queries, transaction coordination, projections, and
//! application error mapping belong here. Transport and concrete persistence
//! types must remain outside this crate.

#![forbid(unsafe_code)]

mod agent_cqrs;
mod agent_enrollment;
mod agent_execution;
mod agent_heartbeat;
mod agent_lease;
mod agent_placement;
mod agent_registration;
mod artifact_transfer;
#[cfg(test)]
mod artifact_transfer_tests;
mod build_cqrs;
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
mod job_event_cqrs;
mod lease_expiry;
mod management_input;
mod manual_trigger;
mod pipeline_cqrs;
mod pool_cqrs;
mod project_cqrs;
mod project_policy;
mod projections;
mod retry_policy;
mod schedule_cqrs;
mod snapshots;
mod transaction;
mod webhook_delivery;

use std::sync::Arc;

use octacity_server_domain::ProjectId;
use octacity_server_store::{
  LogIndexWorkStore, LogSearchError, LogSearchFreshness, LogSearchIndex, LogSearchPage, LogSearchQuery, StoreError,
};
use thiserror::Error;

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
pub use artifact_transfer::{
  AgentArtifactError, AgentArtifactTransferUseCases, ArtifactDownloadProjection, ArtifactHandlers, ArtifactProjection,
  ArtifactProjectionKind, AuthorizeArtifactDownloadQuery, BeginAgentArtifactUploadInput,
  CompleteAgentArtifactUploadInput, GetArtifactQuery, ListBuildArtifactsQuery,
};
pub use build_cqrs::{
  AttemptDetailsProjection, BuildDetailsProjection, BuildHandlers, CancelBuildCommand, CancelBuildCommandOutcome,
  GetAttemptQuery, GetBuildQuery, GetJobQuery, RetryBuildCommand, RetryBuildCommandOutcome,
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
pub use internal_trigger::{
  InternalTriggerBatchOutcome, InternalTriggerDefinition, InternalTriggerWorker, InternalTriggerWorkerError,
};
pub use job_event_cqrs::{
  JobEventLongPoll, JobEventPageProjection, JobEventProjection, JobEventWaiter, MAX_JOB_EVENT_WAIT, ReadJobEventsQuery,
};
pub use lease_expiry::{LeaseExpiryBatchOutcome, LeaseExpiryWorker};
pub use management_input::{
  ManagedWebhookInput, ManagementInputError, ManagementInputFactory, ManualTriggerDefinitionInput, ManualTriggerInput,
  ScheduledTriggerDefinitionInput, UnmanagedWebhookInput,
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
pub use octacity_server_secrets::AgentEnrollmentSecretKey;
pub use octacity_server_store::AgentDrainMode;
pub use octacity_server_store::RegistrationEpoch;
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

/// Typed application query for bounded redacted Build-log search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchBuildLogsQuery {
  /// Backend-neutral validated search shape.
  pub search: LogSearchQuery,
}

impl Query for SearchBuildLogsQuery {
  type Outcome = LogSearchPage;
}

/// Typed application query for Build-log projection freshness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBuildLogFreshnessQuery {
  /// Project whose authoritative and indexed watermarks are requested.
  pub project_id: ProjectId,
}

impl Query for GetBuildLogFreshnessQuery {
  type Outcome = LogSearchFreshness;
}

/// Application query service that combines authoritative indexing work with a
/// replaceable derived Build-log search projection.
pub struct BuildLogSearch<W, I> {
  work: Arc<W>,
  index: Arc<I>,
}

impl<W, I> BuildLogSearch<W, I>
where
  W: LogIndexWorkStore,
  I: LogSearchIndex,
{
  /// Creates a query service from authoritative and derived store ports.
  pub fn new(work: Arc<W>, index: Arc<I>) -> Self {
    Self { work, index }
  }

  /// Searches logs and reports freshness against the durable committed watermark.
  pub async fn search(&self, query: LogSearchQuery) -> Result<LogSearchPage, BuildLogSearchError> {
    query.validate().map_err(|source| {
      BuildLogSearchError::SearchIndex(LogSearchError::invalid(
        octacity_server_store::LogSearchOperation::Search,
        source,
      ))
    })?;
    let committed_through = self
      .work
      .committed_log_index_position(query.project_id)
      .await
      .map_err(BuildLogSearchError::AuthoritativeStore)?;
    self
      .index
      .search(query)
      .await
      .map(|indexed| LogSearchPage {
        hits: indexed.hits,
        next_cursor: indexed.next_cursor,
        freshness: LogSearchFreshness {
          indexed_through: indexed.indexed_through,
          committed_through,
        },
      })
      .map_err(BuildLogSearchError::SearchIndex)
  }

  /// Reports projection freshness against the durable committed watermark.
  pub async fn freshness(&self, project_id: ProjectId) -> Result<LogSearchFreshness, BuildLogSearchError> {
    let committed_through = self
      .work
      .committed_log_index_position(project_id)
      .await
      .map_err(BuildLogSearchError::AuthoritativeStore)?;
    let indexed_through = self
      .index
      .indexed_through(project_id)
      .await
      .map_err(BuildLogSearchError::SearchIndex)?;
    Ok(LogSearchFreshness {
      indexed_through,
      committed_through,
    })
  }
}

#[async_trait::async_trait]
impl<W, I> QueryHandler<SearchBuildLogsQuery> for BuildLogSearch<W, I>
where
  W: LogIndexWorkStore + 'static,
  I: LogSearchIndex + 'static,
{
  type Error = BuildLogSearchError;

  async fn handle_query(&self, query: SearchBuildLogsQuery) -> Result<LogSearchPage, Self::Error> {
    self.search(query.search).await
  }
}

#[async_trait::async_trait]
impl<W, I> QueryHandler<GetBuildLogFreshnessQuery> for BuildLogSearch<W, I>
where
  W: LogIndexWorkStore + 'static,
  I: LogSearchIndex + 'static,
{
  type Error = BuildLogSearchError;

  async fn handle_query(&self, query: GetBuildLogFreshnessQuery) -> Result<LogSearchFreshness, Self::Error> {
    self.freshness(query.project_id).await
  }
}

/// Safe application-level failure from a Build-log search query.
#[derive(Debug, Error)]
pub enum BuildLogSearchError {
  /// The authoritative watermark could not be read.
  #[error("authoritative log-index watermark is unavailable")]
  AuthoritativeStore(#[source] StoreError),
  /// The derived search projection rejected or could not execute the query.
  #[error("build-log search index failed")]
  SearchIndex(#[source] LogSearchError),
}
