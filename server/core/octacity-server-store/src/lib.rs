//! Backend-neutral persistence ports shaped around complete atomic use cases.
//!
//! Table CRUD, SQL rows, transactions, and database-specific error types do
//! not belong in this crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod agent_model;
mod agent_port;
mod build_control;
mod build_query;
mod build_query_port;
mod configuration_model;
mod configuration_port;
mod credential_port;
mod credentials;
mod definition_model;
mod definition_port;
mod error;
mod external_trigger_model;
mod external_trigger_port;
mod idempotency;
mod internal_trigger_model;
mod internal_trigger_port;
mod job_model;
mod lease_recovery;
mod lease_recovery_port;
mod log_search;
mod log_search_port;
mod model;
mod pipeline_model;
mod pipeline_port;
mod pool_model;
mod pool_port;
mod port;
mod project_model;
mod project_policy;
mod project_policy_port;
mod project_port;
mod schedule_model;
mod schedule_port;
mod trigger_evaluation_model;
mod trigger_evaluation_port;
mod trigger_port;
mod webhook_delivery_model;

#[cfg(any(test, feature = "test-support"))]
mod log_search_testing;

#[cfg(any(test, feature = "test-support"))]
mod log_search_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod credential_testing;

#[cfg(any(test, feature = "test-support"))]
mod credential_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod configuration_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod configuration_testing;

#[cfg(any(test, feature = "test-support"))]
mod authoritative_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod project_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod project_testing;

#[cfg(any(test, feature = "test-support"))]
mod pipeline_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod pipeline_testing;

#[cfg(any(test, feature = "test-support"))]
mod pool_testing;

#[cfg(any(test, feature = "test-support"))]
mod pool_contract_testing;

#[cfg(any(test, feature = "test-support"))]
mod agent_testing;

#[cfg(any(test, feature = "test-support"))]
mod test_support;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use agent_model::{
  AgentDrainMode, AgentPage, AgentStatus, DrainAgent, DrainAgentOutcome, EnrolledAgent, ListAgents,
  MAX_AGENT_PAGE_SIZE, ReassignAgentPool, ReassignAgentPoolOutcome,
};
pub use agent_port::AgentStore;
pub use build_control::{
  CancelBuild, CancellationDisposition, MAX_RETRY_BUILD_BYTES, RetryBuild, RetryDisposition, retry_graph_is_equivalent,
};
pub use build_query::{AttemptRecord, BuildRecord, JobAssignmentRecord, JobQueueRecord, JobRecord, JobTerminalRecord};
pub use build_query_port::BuildQueryStore;
pub use configuration_model::{
  BuildConfigurationDefinition, BuildConfigurationMutationOutcome, ConfigurationAgentRequirements,
  ConfigurationCachePolicy, ConfigurationNetworkPolicy, ConfigurationRetryPolicy, ConfigurationRuntimePolicy,
  ConfigurationTriggerPolicy, CreateBuildConfiguration, CreateRepository, MAX_AGENT_REQUIREMENT_LABELS,
  MAX_BUILD_CONFIGURATION_BYTES, MAX_CONFIGURATION_LABEL_BYTES, MAX_CONFIGURATION_PARAMETERS, MAX_CONFIGURATION_POOLS,
  MAX_REPOSITORY_DEFINITION_BYTES, MAX_REPOSITORY_REFERENCES, MAX_RETRY_ATTEMPTS, ParameterDefinition,
  ParameterResolutionError, ParameterSchema, ParameterType, PublishBuildConfigurationVersion, PublishRepositoryVersion,
  PublishedBuildConfiguration, PublishedRepository, RepositoryDefinition, RepositoryMutationOutcome,
  RepositorySelectionPolicy, RetryClass,
};
pub use configuration_port::ConfigurationStore;
pub use credential_port::AgentCredentialStore;
pub use credentials::{
  AgentCredentialTarget, AgentPlatform, AgentRegistrationOutcome, AgentRegistrationProof,
  AuthenticateAgentRegistration, AuthenticatedAgentRegistration, CredentialDigest, CredentialSecret,
  ExpectedAgentPlatform, FreshRegistrationCredential, IssueAgentEnrollment, IssueAgentEnrollmentOutcome,
  MAX_AGENT_INVENTORY_BYTES, MAX_AGENT_PLATFORM_LABEL_BYTES, RegisterAgent, RegistrationValidity,
  RevokeAgentCredential,
};
pub use definition_model::{
  CreateTriggerDefinition, ProjectPolicyMutationOutcome, PublishProjectPolicy, TriggerDefinitionMutationOutcome,
};
pub use definition_port::DefinitionStore;
pub use error::{StoreError, StoreInputError, StoreOperation};
pub use external_trigger_model::{
  ClaimManagedWebhookOperations, CreateManagedWebhook, CreateUnmanagedWebhook, EnqueueManagedWebhookOperation,
  FailManagedWebhookOperation, MAX_MANAGED_WEBHOOK_OPERATION_BATCH_SIZE, MAX_WEBHOOK_ADAPTER_ID_BYTES,
  MAX_WEBHOOK_HEADER_NAME_BYTES, MAX_WEBHOOK_MATERIAL_HANDLE_BYTES, MAX_WEBHOOK_VERIFICATION_HEADERS,
  ManagedWebhookDefinition, ManagedWebhookMutationOutcome, ManagedWebhookOperation, ManagedWebhookOperationClaim,
  ManagedWebhookRecord, ManagedWebhookRegistration, ManagedWebhookRegistrationStatus, RecordManagedWebhookRegistration,
  UnmanagedWebhookDefinition, UnmanagedWebhookMutationOutcome, WEBHOOK_ADAPTER_SHA256_BYTES, WebhookIntegrationRecord,
  canonical_webhook_headers,
};
pub use external_trigger_port::{
  ManagedWebhookOperationStore, ManagedWebhookRegistrationStore, WebhookConfigurationStore,
  WebhookDeliveryAdmissionStore, WebhookDeliveryQueryStore, WebhookDeliveryWorkStore, WebhookIntegrationReader,
};
pub use idempotency::{IdempotencyKey, MAX_IDEMPOTENCY_KEY_BYTES};
pub use internal_trigger_model::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, InternalTriggerEventClaim, InternalTriggerMatch,
  MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE, MAX_INTERNAL_TRIGGER_MATCHES_PER_EVENT,
};
pub use internal_trigger_port::InternalTriggerEventStore;
pub use job_model::{
  AppendJobEvents, AppendJobEventsOutcome, CompletionDisposition, DurableJobEvent, JobClaim, JobClaimOutcome,
  JobCompletion, JobCompletionKind, JobEventPage, LeaseAccess, LeaseGrant, LeaseHeartbeatOutcome, LeaseWindow,
  ReadJobEvents, RenewLease, complete_job_state, start_job_execution,
};
pub use lease_recovery::{
  ClaimExpiredLeases, ExpiredLeaseClaim, LeaseRecoveryAction, MAX_LEASE_EXPIRY_BATCH_SIZE, MAX_WORKER_OWNER_BYTES,
  RecoverExpiredLease, RecoverExpiredLeaseOutcome, WorkerOwner,
};
pub use lease_recovery_port::LeaseRecoveryStore;
pub use log_search::{
  BuildLogStream, DeleteLogSearchDocuments, IndexedLogSearchPage, LogIndexPosition, LogSearchCursor, LogSearchDocument,
  LogSearchError, LogSearchFreshness, LogSearchHit, LogSearchInputError, LogSearchMode, LogSearchMutationDisposition,
  LogSearchOperation, LogSearchPage, LogSearchQuery, MAX_LOG_SEARCH_DOCUMENT_BYTES, MAX_LOG_SEARCH_PAGE_SIZE,
  MAX_LOG_SEARCH_QUERY_BYTES, MAX_LOG_SEARCH_SNIPPET_BYTES, WriteLogSearchDocument,
};
pub use log_search_port::{LogIndexWorkStore, LogSearchIndex};
pub use model::{
  AcceptTrigger, AcceptTriggerOutcome, EventDigest, EventSequence, ImmutableBuildInput, JobEventKind, LeaseFence,
  MAX_ACCEPT_TRIGGER_BYTES, MAX_ALLOWED_POOLS_PER_JOB, MAX_JOB_DEPENDENCIES, MAX_JOB_EVENT_BATCH_BYTES,
  MAX_JOB_EVENT_BATCH_SIZE, MAX_JOB_EVENT_KIND_BYTES, MAX_JOB_EVENT_PAYLOAD_BYTES, MAX_JOB_EVENT_READ_PAGE_SIZE,
  MAX_MATERIALIZED_DEPENDENCY_EDGES, MAX_MATERIALIZED_JOBS, MAX_STRUCTURED_DOCUMENT_BYTES, MaterializedJob,
  MaterializedJobPayload, MutationDisposition, RegistrationEpoch, SuppressTrigger, SuppressTriggerOutcome,
  TriggerAcceptanceProbe, TriggerEvaluationOutcome, TriggerIntentDigest, TriggerIntentDigestError,
};
pub use octacity_server_domain::{ArtifactPolicy, ImmutableRevision, NetworkHost, RuntimeClass, SourceReference};
pub use octacity_server_domain::{EnrollmentCredentialId, LogChunkId, LogIndexingWorkId, RegistrationCredentialId};
pub use octacity_server_trigger::{
  InternalTriggerProtection, MAX_INTERNAL_TRIGGER_DEPTH, MissedRunPolicy, NormalizedTriggerOccurrence,
  ScheduleDefinition, TriggerCausality, TriggerCause, TriggerDeduplicationKey, TriggerDefinitionRef, TriggerEventKind,
  TriggerInputError, TriggerKind, TriggerMetadata, TriggerOccurrenceIntent, TriggerOccurrenceState, TriggerTarget,
  derive_internal_causality, validate_internal_ancestry,
};
pub use pipeline_model::{CreatePipeline, PipelineMutationOutcome, PublishPipelineVersion, PublishedPipeline};
pub use pipeline_port::PipelineStore;
pub use pool_model::{
  AgentPoolDefinition, AgentPoolMutationOutcome, AgentPoolPage, CreateAgentPool, DeleteAgentPool,
  DeleteAgentPoolOutcome, ListAgentPools, MAX_AGENT_POOL_PAGE_SIZE, MAX_POOL_ADMISSION_PLATFORMS,
  MAX_POOL_STATIC_CAPACITY, PoolAdmissionPolicy, PoolFairnessPolicy, PublishAgentPoolVersion, PublishedAgentPool,
  validate_pool_drain_transition,
};
pub use pool_port::AgentPoolStore;
pub use port::{
  AuthoritativeStore, BuildControlStore, JobEventReadStore, JobExecutionStore, LeaseHeartbeatStore,
  TriggerAcceptanceStore,
};
pub use project_model::{
  CreateProject, DeleteProject, DeleteProjectOutcome, ListProjects, MAX_PROJECT_PAGE_SIZE, MoveProject, Project,
  ProjectDetails, ProjectHierarchyError, ProjectMutationOutcome, ProjectPage, RenameProject, validate_project_ancestry,
};
pub use project_policy::ProjectPolicyDocument;
pub use project_policy_port::ProjectPolicyStore;
pub use project_port::ProjectStore;
pub use schedule_model::{
  ClaimDueSchedules, CompleteScheduleClaim, CreateSchedule, DueScheduleClaim, MAX_SCHEDULE_CLAIM_BATCH_SIZE,
  ScheduleRecord,
};
pub use schedule_port::ScheduleStore;
pub use trigger_evaluation_model::{
  ClaimTriggerEvaluations, CompleteTriggerEvaluation, FailTriggerEvaluation, MAX_TRIGGER_EVALUATION_BATCH_SIZE,
  MAX_TRIGGER_EVALUATION_DIAGNOSTIC_BYTES, MAX_TRIGGER_EVALUATION_PAYLOAD_BYTES, RecordTriggerEvaluationRevision,
  ReserveTriggerEvaluation, TriggerEvaluationClaim, TriggerEvaluationReservation,
};
pub use trigger_evaluation_port::TriggerEvaluationWorkStore;
pub use trigger_port::TriggerDefinitionStore;
pub use webhook_delivery_model::{
  ClaimWebhookDeliveries, CompleteWebhookDelivery, EnqueueWebhookDelivery, FailWebhookDelivery,
  MAX_STORED_WEBHOOK_BODY_BYTES, MAX_WEBHOOK_DELIVERY_BATCH_SIZE, MAX_WEBHOOK_DIAGNOSTIC_BYTES,
  MAX_WEBHOOK_HEADER_VALUE_BYTES, MAX_WEBHOOK_METADATA_ENTRIES, MAX_WEBHOOK_METADATA_NAME_BYTES,
  MAX_WEBHOOK_VALUE_BYTES, NormalizedWebhookEvent, RecordWebhookEvent, RecordWebhookEventOutcome,
  SuppressWebhookDelivery, WebhookDeliveryClaim, WebhookDeliveryDiagnostic, WebhookDeliveryId, WebhookDeliveryState,
  WebhookDeliveryWork, WebhookFailureCode,
};

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, BTreeSet};

  use octacity_server_domain::{BuildId, Timestamp};
  use serde_json::json;

  use super::testing::{
    InMemoryStore, authoritative_store_contract_fixture, verify_in_memory_agent_credential_contract,
    verify_in_memory_log_search_index_contract, verify_in_memory_store_contract,
  };
  use super::{
    AppendJobEvents, DurableJobEvent, EventSequence, IdempotencyKey, JobEventKind, LeaseAccess, LeaseFence,
    ListProjects, MAX_IDEMPOTENCY_KEY_BYTES, MAX_JOB_EVENT_BATCH_SIZE, MAX_JOB_EVENT_PAYLOAD_BYTES,
    MAX_MATERIALIZED_JOBS, MAX_PROJECT_PAGE_SIZE, MAX_WEBHOOK_HEADER_NAME_BYTES, MutationDisposition,
    RegistrationEpoch, StoreError, StoreInputError, StoreOperation, TriggerAcceptanceStore, TriggerEventKind,
    UnmanagedWebhookDefinition, WEBHOOK_ADAPTER_SHA256_BYTES, canonical_webhook_headers,
  };

  #[test]
  fn in_memory_adapter_satisfies_the_authoritative_store_contract() {
    verify_in_memory_store_contract();
  }

  #[test]
  fn materialized_job_count_is_bounded_before_template_validation() {
    let fixture = authoritative_store_contract_fixture();
    let jobs = vec![fixture.request.jobs[0].clone(); MAX_MATERIALIZED_JOBS + 1];
    let unrelated_build = super::test_support::id::<BuildId>(999);

    assert_eq!(
      super::model::validate_materialized_jobs(StoreOperation::AcceptTrigger, unrelated_build, &jobs),
      Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::TooManyJobs,
      )),
    );
  }

  #[test]
  fn accepted_build_requires_a_positive_typed_job_concurrency_limit() {
    let mut request = authoritative_store_contract_fixture().request;
    request.build.project_job_concurrency_limit = 0;

    assert_eq!(
      request.validate(),
      Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::InvalidBuildSchedulingPolicy,
      )),
    );
  }

  #[test]
  fn failed_ready_signing_rolls_back_the_complete_in_memory_acceptance() {
    let fixture = authoritative_store_contract_fixture();
    let store = InMemoryStore::new();
    store.seed_authoritative_contract_prerequisites(&fixture).unwrap();
    let mut invalid = fixture.request.clone();
    invalid.accepted_at = Timestamp::from_unix_millis(-1).unwrap();
    super::test_support::run_ready(
      async {
        assert_eq!(
          store.accept_trigger(invalid).await.unwrap_err(),
          StoreError::Unavailable
        );
        assert_eq!(
          store.accept_trigger(fixture.request).await.unwrap().disposition,
          MutationDisposition::Applied
        );
      },
      "the in-memory transaction unexpectedly yielded",
    );
  }

  #[test]
  fn in_memory_adapter_satisfies_the_agent_credential_contract() {
    verify_in_memory_agent_credential_contract();
  }

  #[test]
  fn in_memory_adapter_satisfies_the_log_search_index_contract() {
    verify_in_memory_log_search_index_contract();
  }

  #[test]
  fn in_memory_adapter_satisfies_the_project_store_contract() {
    super::testing::verify_in_memory_project_store_contract();
  }

  #[test]
  fn in_memory_adapter_satisfies_the_pipeline_store_contract() {
    super::testing::verify_in_memory_pipeline_store_contract();
  }

  #[test]
  fn in_memory_adapter_satisfies_the_configuration_store_contract() {
    super::testing::verify_in_memory_configuration_store_contract();
  }

  #[test]
  fn in_memory_adapter_satisfies_the_agent_pool_store_contract() {
    super::testing::verify_in_memory_agent_pool_store_contract();
  }

  #[test]
  fn agent_credentials_are_redacted_by_all_formatters() {
    let marker = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let secret = super::CredentialSecret::from_bytes(*b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let debug = format!("{secret:?}");
    let display = format!("{secret}");
    assert!(!debug.contains(marker));
    assert!(!display.contains(marker));
    assert_eq!(debug, "CredentialSecret([REDACTED])");
    assert_eq!(display, "[REDACTED]");
  }

  #[test]
  fn durable_events_compute_canonical_digests_and_enforce_bounds() {
    let time = Timestamp::from_unix_millis(1).unwrap();
    let kind = JobEventKind::new("progress").unwrap();
    let left = DurableJobEvent::new(
      EventSequence::new(1).unwrap(),
      kind.clone(),
      time,
      json!({"b": 2, "a": 1}),
    )
    .unwrap();
    let right = DurableJobEvent::new(
      EventSequence::new(1).unwrap(),
      kind.clone(),
      time,
      json!({"a": 1, "b": 2}),
    )
    .unwrap();
    assert_eq!(left.digest(), right.digest());

    assert_eq!(
      DurableJobEvent::new(
        EventSequence::new(2).unwrap(),
        kind,
        time,
        json!({"text": "x".repeat(MAX_JOB_EVENT_PAYLOAD_BYTES)}),
      )
      .unwrap_err(),
      StoreInputError::EventPayloadTooLarge
    );
  }

  #[test]
  fn event_batches_are_bounded_even_when_callers_mutate_public_collections() {
    let mut events = Vec::with_capacity(MAX_JOB_EVENT_BATCH_SIZE + 1);
    for sequence in 1..=MAX_JOB_EVENT_BATCH_SIZE + 1 {
      events.push(
        DurableJobEvent::new(
          EventSequence::new(u64::try_from(sequence).unwrap()).unwrap(),
          JobEventKind::new("progress").unwrap(),
          Timestamp::from_unix_millis(1).unwrap(),
          json!({}),
        )
        .unwrap(),
      );
    }
    let lease = LeaseAccess {
      lease_id: "00000000-0000-0000-0000-000000000001".parse().unwrap(),
      fence: LeaseFence::from_bytes([1; 32]),
      agent_id: "00000000-0000-0000-0000-000000000002".parse().unwrap(),
      registration_epoch: RegistrationEpoch::new(1).unwrap(),
    };
    assert_eq!(
      AppendJobEvents::new(lease, events, Timestamp::from_unix_millis(2).unwrap()).unwrap_err(),
      StoreError::InvalidInput {
        operation: StoreOperation::AppendJobEvents,
        source: StoreInputError::EventBatchTooLarge,
      }
    );
  }

  #[test]
  fn project_query_and_idempotency_inputs_are_bounded() {
    assert_eq!(IdempotencyKey::new(""), Err(StoreInputError::InvalidIdempotencyKey));
    assert_eq!(
      IdempotencyKey::new("x".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1)),
      Err(StoreInputError::InvalidIdempotencyKey)
    );
    for invalid_limit in [0, MAX_PROJECT_PAGE_SIZE + 1] {
      assert_eq!(
        ListProjects::new(None, None, invalid_limit),
        Err(StoreError::InvalidInput {
          operation: StoreOperation::ListProjects,
          source: StoreInputError::InvalidProjectPageSize,
        })
      );
    }
  }

  #[test]
  fn webhook_header_validation_is_shared_by_construction_and_store_revalidation() {
    for invalid in [
      vec!["x-signature".to_owned(), "x-signature".to_owned()],
      vec!["X-Signature".to_owned()],
      vec!["x".repeat(MAX_WEBHOOK_HEADER_NAME_BYTES + 1)],
    ] {
      assert_eq!(
        canonical_webhook_headers(invalid),
        Err(StoreInputError::InvalidWebhookDefinition)
      );
    }

    let definition = UnmanagedWebhookDefinition {
      adapter_id: "fixture".to_owned(),
      adapter_sha256: "0".repeat(WEBHOOK_ADAPTER_SHA256_BYTES),
      verification_material_handle: "secret:webhook".to_owned(),
      verification_headers: BTreeSet::from(["x".repeat(MAX_WEBHOOK_HEADER_NAME_BYTES + 1)]),
      repository_id: super::test_support::id(101),
      event_kind: TriggerEventKind::new("push").unwrap(),
      parameters: BTreeMap::new(),
      priority: 0,
    };
    assert_eq!(definition.validate(), Err(StoreInputError::InvalidWebhookDefinition));
  }
}
