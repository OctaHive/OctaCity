use std::{
  collections::{BTreeMap, BTreeSet},
  future::Future,
  sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_protocol::{ExecutionMode, PlatformArchitecture, PlatformOs, PlatformSpec, guarantees_for};
use octacity_server_application::*;
use octacity_server_application::{
  AcceptManualTriggerCommand, AcceptWebhookDeliveryCommand, ApplicationFailure, ArtifactPolicy,
  AuthenticatedWebhookEvent, CachePolicy, CommandHandler, ConcurrencyPolicy, CreateManagedWebhookCommand,
  DeleteManagedWebhookRegistrationCommand, DurableManualTriggerService, DurableRetryPolicy, EffectiveProjectPolicy,
  IdentityProfileName, JobAssignmentProjection, JobProjection, JobProjectionFacts, JobQueueProjection,
  JobSpecToolchainPolicy, JobTerminalOutcomeProjection, ManagedWebhookRegistration, ManagedWebhookRegistrationRequest,
  ManagedWebhookRegistrationStatus, ManagedWebhookRegistrationWorker, ManualSourceSelection, ManualTriggerCommand,
  ManualTriggerContext, ManualTriggerContextError, ManualTriggerContextProvider, ManualTriggerError,
  ManualTriggerInputError, ManualTriggerOutcome, ManualTriggerRetryWorker, ManualTriggerService,
  MutationDisposition as ApplicationDisposition, ObserveManagedWebhookRegistrationCommand, PolicySource,
  ProjectExecutionTarget, RetentionPolicy, RevisionResolutionError, RevisionResolutionRequest, RevisionResolver,
  RotateManagedWebhookRegistrationCommand, RuntimeClass, ScheduleWorker, ScheduledBuildDefinition, SecretProfileName,
  VerifyWebhookDelivery, WebhookCallbackOrigin, WebhookDeliveryVerifier, WebhookDeliveryWorker, WebhookIngressService,
  WebhookManagementProvider, WebhookManagementService, WebhookVerificationError,
};
use octacity_server_domain::{
  AgentId, AttemptId, BuildConfigurationId, BuildConfigurationVersion, BuildId, EntityKind, ImmutableRevision,
  IntegrationId, JobId, JobVersion, LeaseId, PipelineId, PoolId, ProjectId, RepositoryId, SourceReference, Timestamp,
  TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_job::JobState;
use octacity_server_pipeline::{CapabilityCatalog, ExecutionCapability, PublishablePipelineDag};
use octacity_server_store::testing::InMemoryStore;
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, AuditActor, AuditActorKind, ClaimDueSchedules, ClaimTriggerEvaluations,
  CompleteTriggerEvaluation, ConfigurationNetworkPolicy, CreateManagedWebhook, CreateSchedule, CreateTriggerDefinition,
  CreateUnmanagedWebhook, EnqueueManagedWebhookOperation, FailManagedWebhookOperation, FailTriggerEvaluation,
  IdempotencyKey, ManagedWebhookMutationOutcome, ManagedWebhookOperation, ManagedWebhookOperationClaim,
  ManagedWebhookOperationStore, ManagedWebhookRecord, ManagedWebhookRegistrationStore, ManagementMutation,
  MissedRunPolicy, MutationAuditContext, MutationDisposition as StoreDisposition, RecordManagedWebhookRegistration,
  ReserveTriggerEvaluation, ScheduleDefinition, ScheduleStore, StoreError, SuppressTrigger, SuppressTriggerOutcome,
  TriggerAcceptanceProbe, TriggerCausality, TriggerDefinitionRef, TriggerEvaluationClaim, TriggerEvaluationOutcome,
  TriggerEvaluationReservation, TriggerEvaluationWorkStore, TriggerEventKind, TriggerKind, TriggerTarget,
  UnmanagedWebhookDefinition, UnmanagedWebhookMutationOutcome, WebhookConfigurationStore,
  WebhookDeliveryAdmissionStore, WebhookDeliveryQueryStore, WebhookDeliveryWorkStore, WebhookIntegrationReader,
  WebhookIntegrationRecord, WorkerOwner,
};
use serde_json::json;

#[allow(dead_code)] // Shared compiler/source tests use additional fixture observations.
#[path = "support/factory_candidate_context.rs"]
mod factory_candidate_context_test_support;
#[allow(dead_code)] // Shared fixture consumers opt into their relevant public contracts.
#[path = "../src/factory_node_test_support.rs"]
mod factory_node_test_support;

#[path = "support/management_command.rs"]
mod management_command_support;
use management_command_support::management_command;

fn management_audit() -> MutationAuditContext {
  MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    },
    octacity_server_store::ManagementSecurityScope::trusted_network(),
    "application-contract-request",
  )
  .unwrap()
}

#[test]
fn manual_trigger_resolves_source_and_materializes_the_complete_dag() {
  run(async {
    let fixture = fixture();
    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("0123456789abcdef"));
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context.clone())),
      resolver.clone(),
    );

    let outcome = service
      .accept_management(fixture.command.clone(), time(200), management_audit())
      .await
      .unwrap();
    let ManualTriggerOutcome::Accepted {
      disposition,
      ready_job_ids,
      ..
    } = outcome
    else {
      panic!("enabled configuration must create a Build");
    };
    let request = store.one_request();
    assert_eq!(disposition, ApplicationDisposition::Applied);
    assert_eq!(request.build.immutable_revision.as_str(), "0123456789abcdef");
    assert_eq!(request.attempt_number.get(), 1);
    assert_eq!(request.jobs.len(), 2);
    assert_eq!(ready_job_ids.len(), 1);
    let root = request.jobs.iter().find(|job| job.dependencies.is_empty()).unwrap();
    let child = request.jobs.iter().find(|job| !job.dependencies.is_empty()).unwrap();
    assert_eq!(child.dependencies, [root.id]);
    assert_eq!(root.allowed_pools, [fixture.shared_pool]);
    assert_eq!(child.allowed_pools, [fixture.shared_pool]);
    let projected_root = JobProjection::from_authoritative(
      request.attempt_id,
      root,
      JobProjectionFacts {
        state: JobState::Ready,
        version: JobVersion::INITIAL,
        created_at: time(200),
        updated_at: time(200),
        queue: Some(JobQueueProjection {
          priority: request.build.priority,
          enqueued_at: time(200),
        }),
        assignment: None,
        terminal: None,
        event_cursor: 0,
        outputs: Vec::new(),
      },
    )
    .unwrap();
    let projected_json = serde_json::to_value(projected_root).unwrap();
    assert!(
      projected_json["placement"]["capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("native"))
    );

    let assignment = JobAssignmentProjection {
      selected_pool_id: fixture.shared_pool,
      assigned_agent_id: id::<AgentId>(80),
    };
    let completed = JobProjection::from_authoritative(
      request.attempt_id,
      root,
      JobProjectionFacts {
        state: JobState::Succeeded,
        version: JobVersion::INITIAL,
        created_at: time(200),
        updated_at: time(300),
        queue: None,
        assignment: Some(assignment),
        terminal: Some(JobTerminalOutcomeProjection::succeeded(time(300))),
        event_cursor: 1,
        outputs: Vec::new(),
      },
    )
    .unwrap();
    assert_eq!(completed.assignment, Some(assignment));
    assert!(serde_json::to_value(completed).unwrap()["assignment"].is_object());
    assert_eq!(
      JobProjection::from_authoritative(
        request.attempt_id,
        root,
        JobProjectionFacts {
          state: JobState::Succeeded,
          version: JobVersion::INITIAL,
          created_at: time(200),
          updated_at: time(300),
          queue: None,
          assignment: None,
          terminal: Some(JobTerminalOutcomeProjection::succeeded(time(300))),
          event_cursor: 1,
          outputs: Vec::new(),
        },
      ),
      Err(octacity_server_application::ProjectionError::InvalidJobLifecycle),
    );
    assert!(projected_json.get("signed_job_spec").is_none());
    assert_eq!(request.build.input_snapshot["parameters"]["required"], json!("caller"));
    assert_eq!(request.build.input_snapshot["parameters"]["with_default"], json!(true));
    assert_eq!(request.build.retention.metadata, time(86_400_200));
    assert_eq!(request.build.retention.logs, time(86_400_200));
    assert_eq!(request.build.retention.artifacts, time(86_400_200));
    assert_eq!(request.build.retention.reports, time(86_400_200));
    let job_spec_template = serde_json::to_string(&root.job_spec_template).unwrap();
    assert!(job_spec_template.contains("ci/secrets.yml"));
    assert!(!job_spec_template.contains("provider_configuration"));
    assert!(!job_spec_template.contains("credential"));
    assert_eq!(root.job_spec_template.pipeline_node_id(), &root.pipeline_node_id);
    assert_eq!(resolver.requests.lock().unwrap().len(), 1);
    assert!(matches!(
      &resolver.requests.lock().unwrap()[0].selection,
      ManualSourceSelection::Reference(reference) if reference.as_str() == "refs/heads/main"
    ));

    let mut replay = fixture.command;
    replay.observed_at = time(999);
    let replay_service = ManualTriggerService::new(store.clone(), Arc::new(UnreachableContext), resolver.clone());
    let replayed = replay_service
      .accept_management(replay, time(300), management_audit())
      .await
      .unwrap();
    let ManualTriggerOutcome::Accepted {
      disposition,
      build_id,
      attempt_id,
      ..
    } = replayed
    else {
      panic!("accepted evaluation must replay its Build");
    };
    assert_eq!(disposition, ApplicationDisposition::Replayed);
    assert_eq!(resolver.requests.lock().unwrap().len(), 1);
    let requests = store.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(build_id, requests[0].build.id);
    assert_eq!(attempt_id, requests[0].attempt_id);
  });
}

#[path = "manual_trigger/retry.rs"]
mod retry;
#[path = "manual_trigger/schedule.rs"]
mod schedule;
#[path = "manual_trigger/security_scope.rs"]
mod security_scope;
#[path = "manual_trigger/webhook.rs"]
mod webhook;

#[path = "manual_trigger/codex_capture.rs"]
mod codex_capture;
#[path = "manual_trigger/command.rs"]
mod command;
#[test]
fn invalid_source_is_rejected_before_vcs_or_store_side_effects() {
  run(async {
    let mut fixture = fixture();
    fixture.command.source = ManualSourceSelection::Reference(SourceReference::new("refs/heads/other").unwrap());
    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("unused"));
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    );

    assert!(matches!(
      service
        .accept_management(fixture.command, time(200), management_audit())
        .await,
      Err(ManualTriggerError::Invalid(
        ManualTriggerInputError::ReferenceNotAllowed
      ))
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
    assert!(store.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn secret_profile_outside_effective_policy_is_rejected_before_side_effects() {
  run(async {
    let mut fixture = fixture();
    fixture.context.effective_policy.policy.secret_profiles.clear();
    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("unused"));
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    );

    assert!(matches!(
      service
        .accept_management(fixture.command, time(200), management_audit())
        .await,
      Err(ManualTriggerError::Invalid(
        ManualTriggerInputError::SecretProfileNotAllowed
      ))
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
    assert!(store.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn provider_neutral_runtime_requires_an_exact_project_execution_target() {
  run(async {
    let mut fixture = fixture();
    let platform = PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    };
    fixture.context.configuration.definition.runtime.class = RuntimeClass::Host;
    fixture.context.configuration.definition.runtime.host_platform = Some(platform);
    fixture.context.configuration.definition.runtime.required_guarantees = guarantees_for(ExecutionMode::Host);
    fixture
      .context
      .effective_policy
      .policy
      .runtimes
      .insert(RuntimeClass::Host);

    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("unused"));
    let rejected = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context.clone())),
      resolver.clone(),
    )
    .accept_management(fixture.command.clone(), time(200), management_audit())
    .await;
    assert!(matches!(
      rejected,
      Err(ManualTriggerError::Invalid(ManualTriggerInputError::RuntimeNotAllowed))
    ));
    assert!(store.requests.lock().unwrap().is_empty());

    fixture
      .context
      .effective_policy
      .policy
      .execution_targets
      .insert(ProjectExecutionTarget {
        mode: ExecutionMode::Host,
        host_platform: platform,
        target_platform: platform,
        required_guarantees: guarantees_for(ExecutionMode::Host),
      });
    let accepted = ManualTriggerService::new(store.clone(), Arc::new(StaticContext(fixture.context)), resolver)
      .accept_management(fixture.command, time(200), management_audit())
      .await;
    assert!(matches!(accepted, Ok(ManualTriggerOutcome::Accepted { .. })));
    assert_eq!(store.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn disabled_configuration_is_durably_suppressed_and_replayed_without_vcs() {
  run(async {
    let mut fixture = fixture();
    fixture.context.configuration.definition.enabled = false;
    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("unused"));
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    );

    let outcome = service
      .accept_management(fixture.command.clone(), time(200), management_audit())
      .await
      .unwrap();
    let ManualTriggerOutcome::Suppressed {
      disposition,
      trigger_occurrence_id,
    } = outcome
    else {
      panic!("disabled configuration must suppress the Trigger");
    };
    assert_eq!(disposition, ApplicationDisposition::Applied);
    assert!(store.requests.lock().unwrap().is_empty());
    assert_eq!(store.suppressions.lock().unwrap().len(), 1);
    assert!(resolver.requests.lock().unwrap().is_empty());

    let replay_service = ManualTriggerService::new(store.clone(), Arc::new(UnreachableContext), resolver.clone());
    let replayed = replay_service
      .accept_management(fixture.command, time(300), management_audit())
      .await
      .unwrap();
    let ManualTriggerOutcome::Suppressed {
      disposition,
      trigger_occurrence_id: replayed_occurrence_id,
    } = replayed
    else {
      panic!("suppressed evaluation must replay suppression");
    };
    assert_eq!(disposition, ApplicationDisposition::Replayed);
    assert_eq!(replayed_occurrence_id, trigger_occurrence_id);
    assert_eq!(store.suppressions.lock().unwrap().len(), 1);
    assert!(resolver.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn retried_internal_event_creates_one_causally_linked_build() {
  run(async {
    let mut fixture = fixture();
    fixture
      .context
      .configuration
      .definition
      .triggers
      .allowed
      .insert(TriggerKind::Internal);
    fixture.command.deduplication_identity = TriggerIdentity::new("internal:outbox-1").unwrap();
    fixture.command.observed_at = time(300);
    let store = Arc::new(RecordingStore::default());
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    );
    let root_occurrence_id = id(50);
    let parent_occurrence_id = id(51);
    let causality = TriggerCausality::derived(root_occurrence_id, parent_occurrence_id, 1);
    let source_build_id = id::<BuildId>(52);
    let event_kind = TriggerEventKind::new("build.succeeded").unwrap();

    let first = service
      .accept_internal(
        fixture.command.clone(),
        time(400),
        source_build_id,
        event_kind.clone(),
        causality,
      )
      .await
      .unwrap();
    let replay_service = ManualTriggerService::new(
      store.clone(),
      Arc::new(UnreachableContext),
      Arc::new(RecordingResolver::succeed("unused")),
    );
    let replayed = replay_service
      .accept_internal(
        fixture.command,
        time(500),
        source_build_id,
        event_kind.clone(),
        causality,
      )
      .await
      .unwrap();

    assert!(matches!(
      first,
      ManualTriggerOutcome::Accepted {
        disposition: ApplicationDisposition::Applied,
        ..
      }
    ));
    assert!(matches!(
      replayed,
      ManualTriggerOutcome::Accepted {
        disposition: ApplicationDisposition::Replayed,
        ..
      }
    ));
    let requests = store.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].trigger.causality, causality);
    assert!(matches!(
      &requests[0].trigger.cause,
      octacity_server_store::TriggerCause::Internal { source_build_id: id, event_kind: kind }
        if *id == source_build_id && *kind == event_kind
    ));
  });
}

#[test]
fn open_schema_rejects_non_primitive_parameters_before_vcs_resolution() {
  run(async {
    let mut fixture = fixture();
    fixture.context.configuration.definition.parameters.deny_unknown = false;
    fixture
      .command
      .parameters
      .insert("nested".to_owned(), json!({"value": 1}));
    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("unused"));
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    );

    assert!(matches!(
      service
        .accept_management(fixture.command, time(200), management_audit())
        .await,
      Err(ManualTriggerError::Invalid(
        ManualTriggerInputError::InvalidParameterValue(name)
      )) if name == "nested"
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
    assert!(store.requests.lock().unwrap().is_empty());
    assert!(store.suppressions.lock().unwrap().is_empty());
  });
}

#[test]
fn one_failed_atomic_store_call_cannot_be_reported_as_success() {
  run(async {
    let fixture = fixture();
    let store = Arc::new(RecordingStore::failing());
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    );

    assert!(matches!(
      service
        .accept_management(fixture.command, time(200), management_audit())
        .await,
      Err(ManualTriggerError::Store(StoreError::Unavailable))
    ));
    assert_eq!(store.calls(), 1);
    assert!(store.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn unknown_manual_trigger_is_rejected_before_vcs_resolution() {
  run(async {
    let fixture = fixture();
    let store = Arc::new(RecordingStore::default());
    let resolver = Arc::new(RecordingResolver::succeed("unused"));
    let service = ManualTriggerService::new(store, Arc::new(RejectingContext), resolver.clone());

    assert!(matches!(
      service
        .accept_management(fixture.command, time(200), management_audit())
        .await,
      Err(ManualTriggerError::Context(ManualTriggerContextError::Store(
        StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Trigger
        }
      )))
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
  });
}

#[path = "support/manual_trigger_context.rs"]
mod manual_trigger_context;

struct Fixture {
  command: ManualTriggerCommand,
  context: ManualTriggerContext,
  shared_pool: PoolId,
}

fn fixture() -> Fixture {
  let (context, shared_pool) = manual_trigger_context::context();
  Fixture {
    command: ManualTriggerCommand {
      trigger: TriggerDefinitionRef {
        id: id::<TriggerId>(9),
        version: TriggerVersion::INITIAL,
      },
      target: TriggerTarget {
        configuration_id: context.configuration.id,
        configuration_version: context.configuration.version,
      },
      deduplication_identity: TriggerIdentity::new("manual:request-1").unwrap(),
      source: ManualSourceSelection::DefaultReference,
      parameters: BTreeMap::from([("required".to_owned(), json!("caller"))]),
      priority: 10,
      observed_at: time(100),
    },
    context,
    shared_pool,
  }
}

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn build_context_loads_exact_versions_without_a_trigger_and_rejects_another_project() {
  run(async {
    use octacity_server_application::{
      EffectiveProjectPolicySource, EffectiveProjectPolicySourceError, StoreBackedBuildExecutionContext,
    };
    use octacity_server_store::testing::{InMemoryConfigurationStore, InMemoryPipelineStore, management_mutation};
    use octacity_server_store::{
      ConfigurationStore, CreateBuildConfiguration, CreatePipeline, CreateRepository, PipelineStore,
    };
    struct Policies(EffectiveProjectPolicy);
    #[async_trait]
    impl EffectiveProjectPolicySource for Policies {
      async fn effective_policy(
        &self,
        _: ProjectId,
      ) -> Result<EffectiveProjectPolicy, EffectiveProjectPolicySourceError> {
        Ok(self.0.clone())
      }
    }
    let expected = fixture().context;
    let configurations = Arc::new(InMemoryConfigurationStore::new());
    let pipelines = Arc::new(InMemoryPipelineStore::new());
    let project = expected.configuration.project_id;
    configurations.seed_project(project).unwrap();
    configurations
      .seed_pipeline_version(project, expected.pipeline.id, expected.pipeline.version)
      .unwrap();
    for pool in &expected.configuration.definition.allowed_pools {
      configurations.seed_pool(*pool).unwrap();
    }
    pipelines.seed_project(project).unwrap();
    pipelines
      .create_pipeline(management_mutation(CreatePipeline {
        id: expected.pipeline.id,
        project_id: project,
        name: expected.pipeline.name.clone(),
        dag: PublishablePipelineDag::new(
          expected.pipeline.dag.nodes().to_vec(),
          expected.pipeline.dag.edges().to_vec(),
          &CapabilityCatalog::new([ExecutionCapability::new("native").unwrap()]),
        )
        .unwrap(),
        idempotency_key: IdempotencyKey::new("build-context-pipeline").unwrap(),
        published_at: expected.pipeline.published_at,
      }))
      .await
      .unwrap();
    configurations
      .create_repository(management_mutation(CreateRepository {
        id: expected.repository.id,
        project_id: project,
        name: expected.repository.name.clone(),
        definition: expected.repository.definition.clone(),
        idempotency_key: IdempotencyKey::new("build-context-repository").unwrap(),
        published_at: expected.repository.published_at,
      }))
      .await
      .unwrap();
    configurations
      .create_build_configuration(management_mutation(CreateBuildConfiguration {
        id: expected.configuration.id,
        project_id: project,
        name: expected.configuration.name.clone(),
        definition: expected.configuration.definition.clone(),
        idempotency_key: IdempotencyKey::new("build-context-configuration").unwrap(),
        published_at: expected.configuration.published_at,
      }))
      .await
      .unwrap();
    let loader = StoreBackedBuildExecutionContext::new(
      configurations,
      pipelines,
      Arc::new(Policies(expected.effective_policy.clone())),
      expected.job_spec_toolchain.clone(),
    );
    let actual = loader
      .load(project, expected.configuration.id, expected.configuration.version)
      .await
      .unwrap();
    assert_eq!(actual, expected);
    assert!(
      loader
        .load(
          ProjectId::generate(),
          expected.configuration.id,
          expected.configuration.version
        )
        .await
        .is_err()
    );
    assert!(
      loader
        .load(
          project,
          expected.configuration.id,
          BuildConfigurationVersion::new(2).unwrap()
        )
        .await
        .is_err(),
      "missing immutable versions cannot fall back to current configuration"
    );
  });
}

struct StaticContext(ManualTriggerContext);

mod factory {
  pub use octacity_server_factory::*;
}
use factory_node_test_support::fixtures as flow_fixture;
#[allow(dead_code)]
#[path = "../src/factory_job_test_support.rs"]
mod managed_fixture;

fn factory_materialization_fixture() -> (
  octacity_server_application::FactoryManagedBuildInput,
  factory::AdmittedFlow,
  factory::WorkEnvelope,
  factory::FlowBuildBinding,
) {
  use factory::*;
  use octacity_server_application::{FactoryBuildPolicyLayers, FactoryManagedBuildInput, FactoryManagedJobProgram};
  let mut context = fixture().context;
  let canonical = managed_fixture::canonical_v3_spec();
  let runtime = &mut context.configuration.definition.runtime;
  runtime.class = RuntimeClass::Virtualization;
  runtime.operating_system = canonical.runtime.target.target_platform.os;
  runtime.architecture = canonical.runtime.target.target_platform.architecture;
  runtime.host_platform = Some(canonical.runtime.target.host_platform);
  runtime.required_guarantees = canonical.runtime.target.required_guarantees.iter().copied().collect();
  runtime.immutable_image = canonical.runtime.target.immutable_image.clone();
  runtime.memory_bytes = canonical.runtime.memory_bytes;
  runtime.writable_disk_bytes = canonical.runtime.writable_disk_bytes;
  runtime.timeout_seconds = 1;
  runtime.network = ConfigurationNetworkPolicy::Restricted {
    allowed_hosts: vec![octacity_server_domain::NetworkHost::new("api.openai.com").unwrap()]
      .into_iter()
      .collect(),
  };
  context.configuration.definition.secrets_profile = Some(SecretProfileName::new("model-coding").unwrap());
  context.configuration.definition.artifacts = ArtifactPolicy {
    artifact_count: 10,
    artifact_bytes: 1024,
    report_count: 10,
    report_bytes: 1024,
    single_output_bytes: 1024,
  };
  context.effective_policy.policy.artifacts = context.configuration.definition.artifacts;
  context
    .effective_policy
    .policy
    .secret_profiles
    .insert(SecretProfileName::new("model-coding").unwrap());
  context
    .effective_policy
    .policy
    .execution_targets
    .insert(ProjectExecutionTarget {
      mode: ExecutionMode::Virtualization,
      host_platform: canonical.runtime.target.host_platform,
      target_platform: canonical.runtime.target.target_platform,
      required_guarantees: canonical
        .runtime
        .target
        .required_guarantees
        .clone()
        .into_iter()
        .collect(),
    });
  context.job_spec_toolchain.octa.plugin_digests = canonical.octa.plugin_digests.clone();
  let previous = flow_fixture::work();
  let subject = ExactSubject::new(
    context.configuration.project_id,
    context.repository.id,
    ImmutableRevision::new("base").unwrap(),
  );
  let work = WorkEnvelope::new(
    previous.id(),
    FactoryConfigurationRef::new(
      previous.configuration().id(),
      previous.configuration().version(),
      subject.project_id(),
      FactoryDigest::from_bytes([3; 32]),
    ),
    previous.external_identity().clone(),
    subject,
    previous.artifacts().clone(),
    WorkClassification::new(previous.priority(), previous.risk(), previous.metadata().clone()),
  )
  .unwrap();
  let mut grants = serde_json::to_value(managed_fixture::permissions(false)).unwrap();
  grants["resources"]["elapsed_millis"] = json!(1000);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  let mut binding = flow_fixture::binding(work.subject().project_id());
  binding.tool = managed_fixture::reference("codex-cli", "0.116.0");
  binding.plugin = managed_fixture::reference("codex", "0.5.0");
  binding.build_configuration = BuildConfigurationRef::new(
    context.configuration.id,
    context.configuration.version,
    context.configuration.project_id,
    FactoryDigest::from_bytes([62; 32]),
  );
  let (_, admitted, _, input, attempt) = flow_fixture::execution_fixture_with_permissions(
    FlowNodeKind::BuildCommand,
    work.clone(),
    binding.clone(),
    None,
    grants.clone(),
  );
  let ownership = FactoryClaimOwnership::new(
    flow_fixture::key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([64; 32])),
      time(10),
      time(100),
    )
    .unwrap(),
  );
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(input.definition()).unwrap(),
    NodeAttemptInput {
      id: attempt.id(),
      node_key: input.node().clone(),
      node_kind: FlowNodeKind::BuildCommand,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: attempt.budget(),
      started_at: time(10),
      deadline: time(1010),
      execution: attempt.execution().clone(),
      ownership: ownership.clone(),
    },
  )
  .unwrap();
  let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 17).unwrap();
  let programs = context
    .pipeline
    .dag
    .nodes()
    .iter()
    .map(|node| {
      (
        node.id().clone(),
        FactoryManagedJobProgram {
          execution: canonical.execution.clone(),
          protected_inputs: canonical.protected_inputs.clone(),
          required_enforcement: canonical.required_enforcement.clone(),
        },
      )
    })
    .collect();
  let input = FactoryManagedBuildInput {
    context,
    intent: intent.clone(),
    ownership,
    parameters: BTreeMap::from([("required".to_owned(), json!("factory caller"))]),
    programs,
    policy_layers: FactoryBuildPolicyLayers {
      project: grants.clone(),
      configuration: grants.clone(),
      task: grants.clone(),
      local: LocalPermissionCeiling::fully_enforced(grants),
    },
    accepted_at: time(12),
  };
  (input, admitted, work, binding)
}

#[test]
fn factory_materialization_rejects_candidate_context_without_verified_materialization() {
  use factory::*;
  use octacity_server_application::materialize_factory_managed_build;
  let (mut request, admitted, work, _) = factory_materialization_fixture();
  let candidate = FactoryTaskSubject::Candidate(CandidateSubject::new(
    work.subject().clone(),
    ImmutableRevision::new("selected-candidate").unwrap(),
    FactoryDigest::from_bytes([91; 32]),
  ));
  let task = work.artifacts().task();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    candidate.clone(),
    FactoryDigest::from_bytes([92; 32]),
    vec![
      ContextManifestEntry::new(
        ContextSourceKind::Task,
        FactoryKey::new("selected-task").unwrap(),
        candidate,
        FactoryContextReference::Artifact(task.clone()),
        task.content_digest(),
        task.encoded_size(),
        FactorySafeText::new("Explicit candidate task input").unwrap(),
        FactoryDigest::from_bytes([93; 32]),
      )
      .unwrap(),
    ],
  )
  .unwrap();
  let input = request.intent.input().clone().with_context(context, vec![]).unwrap();
  let previous = request.intent.node();
  let node = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(input.definition()).unwrap(),
    NodeAttemptInput {
      id: previous.id(),
      node_key: input.node().clone(),
      node_kind: previous.node_kind(),
      number: previous.number(),
      input_digest: input.digest().unwrap(),
      budget: previous.budget(),
      started_at: previous.started_at(),
      deadline: previous.deadline(),
      execution: previous.execution().clone(),
      ownership: request.ownership.clone(),
    },
  )
  .unwrap();
  request.intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), request.intent.priority()).unwrap();
  assert!(
    materialize_factory_managed_build(request).is_err(),
    "a compiler without a verified ChangeSet must not execute the base while claiming a selected candidate"
  );
}

#[test]
fn factory_materialization_compiles_the_published_dag_without_trigger_causality() {
  use factory::*;
  use octacity_server_application::materialize_factory_managed_build;
  let (input, _, work, binding) = factory_materialization_fixture();
  let intent = input.intent.clone();
  let first = materialize_factory_managed_build(input.clone()).unwrap();
  assert_eq!(first, materialize_factory_managed_build(input.clone()).unwrap());
  first.validate_fresh().unwrap();
  assert_eq!(first.jobs.len(), 2);
  let root = first.jobs.iter().find(|job| job.dependencies.is_empty()).unwrap();
  let child = first.jobs.iter().find(|job| !job.dependencies.is_empty()).unwrap();
  assert_eq!(child.dependencies, [root.id]);
  assert_eq!(root.allowed_pools, [fixture().shared_pool]);
  assert_eq!(first.build.priority, 17);
  assert_eq!(first.build.immutable_revision.as_str(), "base");
  assert_eq!(
    first.build.input_snapshot["parameters"]["required"],
    json!("factory caller")
  );
  assert_eq!(root.job_spec_template.execution_deadline(), Some(time(1010)));
  assert_eq!(
    root
      .job_spec_template
      .managed_intent()
      .unwrap()
      .factory()
      .node_attempt_id,
    intent.node().id().to_string()
  );
  let mut substituted_binding = binding;
  substituted_binding.plugin = managed_fixture::reference("uninstalled-plugin", "1.0.0");
  let (_, substituted_flow, _, substituted_input, substituted_node) = flow_fixture::execution_fixture_with_permissions(
    FlowNodeKind::BuildCommand,
    work,
    substituted_binding,
    None,
    input.policy_layers.task.clone(),
  );
  let substituted_node = NodeAttempt::new(
    substituted_flow.root_run(),
    substituted_flow.initial_cycle(),
    substituted_flow
      .closure()
      .definition(substituted_input.definition())
      .unwrap(),
    NodeAttemptInput {
      id: substituted_node.id(),
      node_key: substituted_input.node().clone(),
      node_kind: FlowNodeKind::BuildCommand,
      number: NodeAttemptNumber::INITIAL,
      input_digest: substituted_input.digest().unwrap(),
      budget: substituted_node.budget(),
      started_at: time(10),
      deadline: time(1010),
      execution: substituted_node.execution().clone(),
      ownership: input.ownership.clone(),
    },
  )
  .unwrap();
  let mut substituted = input.clone();
  substituted.intent = FlowBuildIntent::new(
    substituted_input,
    &substituted_flow,
    substituted_node,
    Default::default(),
    17,
  )
  .unwrap();
  assert!(
    materialize_factory_managed_build(substituted).is_err(),
    "an undeclared plugin cannot be signed using another program's grants"
  );
  let mut incomplete = input;
  incomplete.programs.remove(child.job_spec_template.pipeline_node_id());
  assert!(
    materialize_factory_managed_build(incomplete).is_err(),
    "every published Pipeline node needs an explicitly compiled program"
  );
}

#[test]
fn installed_codex_materializer_publishes_selected_context_and_prepares_an_ordinary_job() {
  use factory::*;
  use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
  use octacity_server_application::{
    ApplicationError, BuildExecutionContext, BuildExecutionContextError, BuildExecutionContextSource,
    FactoryBuildMaterializer, FactoryCodexBuildMaterializer, FactoryCodexBuildProfile, FactoryExecutionClock,
    FactoryExecutionInputPublisher, FactoryNodeContextSource,
  };
  use octacity_server_store::{
    FactoryExecutionInputDisposition, FactoryExecutionInputStore, PublishFactoryExecutionInputs,
  };
  struct ContextSource(BuildExecutionContext);
  #[async_trait]
  impl BuildExecutionContextSource for ContextSource {
    async fn load(
      &self,
      project: ProjectId,
      id: BuildConfigurationId,
      version: BuildConfigurationVersion,
    ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
      assert_eq!(
        (project, id, version),
        (
          self.0.configuration.project_id,
          self.0.configuration.id,
          self.0.configuration.version
        )
      );
      Ok(self.0.clone())
    }
  }
  #[derive(Default)]
  struct Inputs(Mutex<Vec<PublishFactoryExecutionInputs>>);
  #[async_trait]
  impl octacity_server_store::FactoryRoleInvocationStore for Inputs {}
  #[async_trait]
  impl FactoryExecutionInputStore for Inputs {
    async fn reserve_factory_execution_inputs(
      &self,
      request: PublishFactoryExecutionInputs,
    ) -> Result<FactoryExecutionInputDisposition, StoreError> {
      request.validate()?;
      Ok(if self.0.lock().unwrap().contains(&request) {
        FactoryExecutionInputDisposition::Published
      } else {
        FactoryExecutionInputDisposition::Pending
      })
    }
    async fn publish_factory_execution_inputs(&self, request: PublishFactoryExecutionInputs) -> Result<(), StoreError> {
      self.0.lock().unwrap().push(request);
      Ok(())
    }
  }
  #[derive(Default)]
  struct Bytes(Mutex<BTreeMap<String, Vec<u8>>>);
  #[async_trait]
  impl ArtifactBytePublisher for Bytes {
    async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
      self.0.lock().unwrap().insert(object.artifact_id().to_string(), bytes);
      Ok(())
    }
  }
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(time(12))
    }
  }
  run(async {
    let (base, _, original, mut binding) = factory_materialization_fixture();
    let selected_bytes = b"Selected context facts.".to_vec();
    let artifact = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::content_sha256(&selected_bytes),
      selected_bytes.len() as u64,
    )
    .unwrap();
    let original = WorkEnvelope::new(
      original.id(),
      original.configuration().clone(),
      original.external_identity().clone(),
      original.subject().clone(),
      WorkArtifacts::new(artifact.clone(), original.artifacts().acceptance().clone(), vec![]).unwrap(),
      WorkClassification::new(original.priority(), original.risk(), original.metadata().clone()),
    )
    .unwrap();
    struct SelectedContext(octacity_server_domain::ArtifactId, Vec<u8>);
    #[async_trait]
    impl FactoryNodeContextSource for SelectedContext {
      async fn read_context(
        &self,
        input: &FlowNodeInput,
        _: Timestamp,
        deadline: Timestamp,
      ) -> Result<BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>, ApplicationError> {
        assert_eq!(input.context().unwrap().entries().len(), 1);
        assert_eq!(deadline, time(1010));
        Ok(BTreeMap::from([(self.0, self.1.clone())]))
      }
    }
    let mut context = base.context;
    let node = context.pipeline.dag.nodes()[0].clone();
    context.pipeline.dag = PublishablePipelineDag::new(
      vec![node.clone()],
      vec![],
      &CapabilityCatalog::new(node.required_capabilities().iter().cloned()),
    )
    .unwrap()
    .into_dag();
    let mut grants = serde_json::to_value(base.policy_layers.task).unwrap();
    for mount in grants["mounts"].as_array_mut().unwrap() {
      if mount["root"] == "/workspace/source" {
        mount["mode"] = json!("read_only");
      }
    }
    grants["secret_profiles"] = json!(["model-evaluation"]);
    grants["outputs"]["kinds"] = json!(["codex-run-trace", "codex-run-provenance", "codex-run-result"]);
    let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
    context
      .effective_policy
      .policy
      .secret_profiles
      .insert(SecretProfileName::new("model-evaluation").unwrap());
    context.configuration.definition.secrets_profile = Some(SecretProfileName::new("model-evaluation").unwrap());
    let instructions = FactoryText::new("Measure the declared payload.").unwrap();
    binding.task_digest = octacity_server_application::codex_flow_task_digest(
      &instructions,
      &flow_fixture::key("model-evaluation"),
      FactoryCredentialConsumer::ReadOnlyReview,
      &octacity_server_application::FactoryCodexUsagePolicy::ReportedCost,
    );
    binding.result_output = flow_fixture::key("codex-run-result");
    let (_, admitted, schema, input, previous) = flow_fixture::execution_fixture_with_permissions(
      FlowNodeKind::Reasoning,
      original.clone(),
      binding,
      None,
      grants.clone(),
    );
    let _ = schema;
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      admitted.closure().definition(input.definition()).unwrap(),
      NodeAttemptInput {
        id: previous.id(),
        node_key: input.node().clone(),
        node_kind: FlowNodeKind::Reasoning,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: previous.budget(),
        started_at: time(10),
        deadline: time(1010),
        execution: previous.execution().clone(),
        ownership: base.ownership.clone(),
      },
    )
    .unwrap();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 17).unwrap();
    let profile = FactoryCodexBuildProfile {
      authentication: crate::FactoryCodexAuthentication::default(),
      mcp_configurations: vec![],
      toolchain: FactoryTaskToolchain {
        executable: intent.profile().tool.clone(),
        plugin: intent.profile().plugin.clone(),
        model: intent.profile().model_or_tool.clone(),
      },
      build_configuration: intent.profile().build_configuration.clone(),
      instructions,
      credentials: FactoryCredentialProfiles::new(
        flow_fixture::key("model-coding"),
        flow_fixture::key("model-evaluation"),
        flow_fixture::key("source-read"),
        flow_fixture::key("delivery-write"),
      )
      .unwrap(),
      consumer: FactoryCredentialConsumer::ReadOnlyReview,
      usage_policy: octacity_server_application::FactoryCodexUsagePolicy::ReportedCost,
      tool_risk: None,
      tool_response_limits: None,
      reviewer: None,
      candidate_capture_tool: None,
      capture: None,
      pipeline_node: node.id().clone(),
      parameters: base.parameters,
      project_permissions: grants.clone(),
      local_permissions: LocalPermissionCeiling::fully_enforced(grants),
    };
    let inputs = Arc::new(Inputs::default());
    let bytes = Arc::new(Bytes::default());
    let publisher = Arc::new(FactoryExecutionInputPublisher::new(
      inputs.clone(),
      bytes.clone(),
      Arc::new(Clock),
    ));
    let mut other_profile = profile.clone();
    other_profile.instructions = FactoryText::new("Another independently installed observation contract.").unwrap();
    let mut repriced = profile.clone();
    repriced.usage_policy = octacity_server_application::FactoryCodexUsagePolicy::TokenRates {
      model: repriced.toolchain.model.clone(),
      input_micro_units_per_million: 10,
      cached_input_micro_units_per_million: 10,
      output_micro_units_per_million: 20,
    };
    let materializer = FactoryCodexBuildMaterializer::new(
      Arc::new(ContextSource(context.clone())),
      publisher,
      vec![other_profile, repriced, profile.clone()],
      Arc::new(SelectedContext(artifact.artifact_id(), selected_bytes.clone())),
      Arc::new(Clock),
    )
    .unwrap();
    let candidate_context = ContextManifest::new(
      ContextManifestId::generate(),
      FactoryTaskSubject::Candidate(CandidateSubject::new(
        intent.input().subject().clone(),
        ImmutableRevision::new("unmaterialized-candidate").unwrap(),
        FactoryDigest::from_bytes([91; 32]),
      )),
      FactoryDigest::from_bytes([92; 32]),
      vec![],
    )
    .unwrap();
    let candidate_input = intent.input().clone().with_context(candidate_context, vec![]).unwrap();
    let previous = intent.node();
    let candidate_node = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      admitted.closure().definition(candidate_input.definition()).unwrap(),
      NodeAttemptInput {
        id: previous.id(),
        node_key: candidate_input.node().clone(),
        node_kind: previous.node_kind(),
        number: previous.number(),
        input_digest: candidate_input.digest().unwrap(),
        budget: previous.budget(),
        started_at: previous.started_at(),
        deadline: previous.deadline(),
        execution: previous.execution().clone(),
        ownership: base.ownership.clone(),
      },
    )
    .unwrap();
    let candidate_intent = FlowBuildIntent::new(
      candidate_input,
      &admitted,
      candidate_node,
      Default::default(),
      intent.priority(),
    )
    .unwrap();
    assert!(
      matches!(
        materializer
          .materialize(&candidate_intent, &admitted, base.ownership.clone(), time(12))
          .await,
        Err(octacity_server_application::OrdinaryBuildApplicationError::Invalid)
      ),
      "an unqualified candidate must fail before selected context or protected publication I/O"
    );
    assert!(inputs.0.lock().unwrap().is_empty());
    assert!(bytes.0.lock().unwrap().is_empty());
    let prepared = materializer
      .materialize(&intent, &admitted, base.ownership.clone(), time(12))
      .await
      .unwrap();
    prepared.validate_fresh().unwrap();
    assert_eq!(prepared.jobs.len(), 1);
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
    assert_eq!(bytes.0.lock().unwrap().len(), 4);
    let job = &prepared.jobs[0];
    let managed = job.job_spec_template.managed_intent().unwrap();
    assert_eq!(
      serde_json::to_value(managed).unwrap()["execution"]["credential_profile"],
      "model-evaluation"
    );
    assert_eq!(job.job_spec_template.execution_deadline(), Some(time(1010)));
    assert_eq!(
      prepared.build.input_snapshot["parameters"]["required"],
      "factory caller"
    );
    let replay = materializer
      .materialize(&intent, &admitted, base.ownership, time(12))
      .await
      .unwrap();
    assert_eq!(prepared, replay);
    assert_eq!(
      inputs.0.lock().unwrap().len(),
      1,
      "replay recovers the published manifest"
    );
    let mut denied = profile;
    denied.project_permissions = FactoryPermissionSet::deny_all();
    let denied_inputs = Arc::new(Inputs::default());
    let denied_bytes = Arc::new(Bytes::default());
    let denied_publisher = Arc::new(FactoryExecutionInputPublisher::new(
      denied_inputs.clone(),
      denied_bytes.clone(),
      Arc::new(Clock),
    ));
    let denied_materializer = FactoryCodexBuildMaterializer::new(
      Arc::new(ContextSource(context)),
      denied_publisher,
      vec![denied],
      Arc::new(SelectedContext(artifact.artifact_id(), selected_bytes)),
      Arc::new(Clock),
    )
    .unwrap();
    assert!(
      denied_materializer
        .materialize(&intent, &admitted, prepared.ownership.clone(), time(12))
        .await
        .is_err()
    );
    assert_eq!(
      denied_inputs.0.lock().unwrap().len(),
      0,
      "Project-denied execution must not publish protected input"
    );
    assert_eq!(denied_bytes.0.lock().unwrap().len(), 0);
  });
}

#[async_trait]
impl ManualTriggerContextProvider for StaticContext {
  async fn load_for(
    &self,
    _trigger: TriggerDefinitionRef,
    _target: TriggerTarget,
    _kind: TriggerKind,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
    Ok(self.0.clone())
  }
}

struct UnreachableContext;

#[async_trait]
impl ManualTriggerContextProvider for UnreachableContext {
  async fn load_for(
    &self,
    _trigger: TriggerDefinitionRef,
    _target: TriggerTarget,
    _kind: TriggerKind,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
    panic!("an accepted manual Trigger must replay before loading mutable context")
  }
}

struct RejectingContext;

#[async_trait]
impl ManualTriggerContextProvider for RejectingContext {
  async fn load_for(
    &self,
    _trigger: TriggerDefinitionRef,
    _target: TriggerTarget,
    _kind: TriggerKind,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
    Err(ManualTriggerContextError::Store(StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::Trigger,
    }))
  }
}

struct RecordingResolver {
  requests: Mutex<Vec<RevisionResolutionRequest>>,
  outcome: Result<ImmutableRevision, RevisionResolutionError>,
}

impl RecordingResolver {
  fn succeed(revision: &str) -> Self {
    Self {
      requests: Mutex::new(Vec::new()),
      outcome: Ok(ImmutableRevision::new(revision).unwrap()),
    }
  }
}

#[async_trait]
impl RevisionResolver for RecordingResolver {
  async fn resolve(&self, request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    self.requests.lock().unwrap().push(request);
    self.outcome.clone()
  }
}

#[derive(Default)]
struct RecordingStore {
  requests: Mutex<Vec<AcceptTrigger>>,
  suppressions: Mutex<Vec<SuppressTrigger>>,
  management_audits: Mutex<Vec<MutationAuditContext>>,
  fail: bool,
  calls: Mutex<usize>,
}

impl RecordingStore {
  fn failing() -> Self {
    Self {
      fail: true,
      ..Self::default()
    }
  }

  fn calls(&self) -> usize {
    *self.calls.lock().unwrap()
  }

  fn one_request(&self) -> AcceptTrigger {
    self.requests.lock().unwrap()[0].clone()
  }
}

#[async_trait]
impl octacity_server_store::TriggerAcceptanceStore for RecordingStore {
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
    if let Some(existing) = self.suppressions.lock().unwrap().iter().find(|existing| {
      existing.trigger.id == request.trigger.id
        || existing.trigger.deduplication_key() == request.trigger.deduplication_key()
    }) {
      if existing.intent_digest != request.intent_digest {
        return Err(StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::Trigger,
        });
      }
      return Ok(Some(TriggerEvaluationOutcome::Suppressed(SuppressTriggerOutcome {
        disposition: StoreDisposition::Replayed,
        trigger_occurrence_id: existing.trigger.id,
      })));
    }
    let requests = self.requests.lock().unwrap();
    let Some(existing) = requests.iter().find(|existing| {
      existing.trigger.id == request.trigger.id
        || existing.trigger.deduplication_key() == request.trigger.deduplication_key()
    }) else {
      return Ok(None);
    };
    if existing.intent_digest != request.intent_digest {
      return Err(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Trigger,
      });
    }
    Ok(Some(TriggerEvaluationOutcome::Accepted(AcceptTriggerOutcome {
      disposition: StoreDisposition::Replayed,
      trigger_occurrence_id: existing.trigger.id,
      build_id: existing.build.id,
      attempt_id: existing.attempt_id,
      ready_jobs: existing
        .jobs
        .iter()
        .filter(|job| job.dependencies.is_empty())
        .map(|job| job.id)
        .collect(),
    })))
  }

  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    *self.calls.lock().unwrap() += 1;
    if self.fail {
      return Err(StoreError::Unavailable);
    }
    let ready_jobs = request
      .jobs
      .iter()
      .filter(|job| job.dependencies.is_empty())
      .map(|job| job.id)
      .collect();
    let outcome = AcceptTriggerOutcome {
      disposition: StoreDisposition::Applied,
      trigger_occurrence_id: request.trigger.id,
      build_id: request.build.id,
      attempt_id: request.attempt_id,
      ready_jobs,
    };
    self.requests.lock().unwrap().push(request);
    Ok(outcome)
  }

  async fn accept_management_trigger(
    &self,
    request: ManagementMutation<AcceptTrigger>,
  ) -> Result<AcceptTriggerOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let outcome = self.accept_trigger(request).await?;
    if outcome.disposition == StoreDisposition::Applied {
      self.management_audits.lock().unwrap().push(audit);
    }
    Ok(outcome)
  }

  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    *self.calls.lock().unwrap() += 1;
    if self.fail {
      return Err(StoreError::Unavailable);
    }
    let outcome = SuppressTriggerOutcome {
      disposition: StoreDisposition::Applied,
      trigger_occurrence_id: request.trigger.id,
    };
    self.suppressions.lock().unwrap().push(request);
    Ok(outcome)
  }

  async fn suppress_management_trigger(
    &self,
    request: ManagementMutation<SuppressTrigger>,
  ) -> Result<SuppressTriggerOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let outcome = self.suppress_trigger(request).await?;
    if outcome.disposition == StoreDisposition::Applied {
      self.management_audits.lock().unwrap().push(audit);
    }
    Ok(outcome)
  }
}

fn run(future: impl Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  assert!(matches!(future.as_mut().poll(&mut context), Poll::Ready(())));
}

fn id<T>(value: u128) -> T
where
  T: FromUuid,
{
  T::from_uuid(uuid::Uuid::from_u128(value))
}

trait FromUuid {
  fn from_uuid(value: uuid::Uuid) -> Self;
}

macro_rules! impl_from_uuid {
  ($($type:ty),+ $(,)?) => {$(
    impl FromUuid for $type {
      fn from_uuid(value: uuid::Uuid) -> Self {
        <$type>::from_uuid(value).unwrap()
      }
    }
  )+};
}

impl_from_uuid!(
  ProjectId,
  RepositoryId,
  PipelineId,
  BuildConfigurationId,
  PoolId,
  TriggerOccurrenceId,
  IntegrationId,
  TriggerId,
  BuildId,
  AttemptId,
  JobId,
  AgentId,
  LeaseId,
);

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}

fn factory_candidate_materialization_fixture() -> (
  octacity_server_application::FactoryManagedBuildInput,
  factory::ChangeSet,
  octacity_server_artifacts::ArtifactRecord,
  octacity_server_artifacts::ArtifactRecord,
) {
  use factory::*;
  use octacity_protocol::{
    CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, CHANGE_SET_MANIFEST_OUTPUT,
  };
  use octacity_server_artifacts::{
    ArtifactContentDigest, ArtifactEvent, ArtifactIdentity, ArtifactMediaType, ArtifactRecord, ArtifactRetentionPolicy,
    ArtifactType,
  };
  let (mut request, admitted, work, _) = factory_materialization_fixture();
  let candidate_subject = CandidateSubject::new(
    work.subject().clone(),
    ImmutableRevision::new("0123456789abcdef0123456789abcdef0123456789").unwrap(),
    FactoryDigest::from_bytes([91; 32]),
  );
  let build_id = BuildId::generate();
  let attempt_id = AttemptId::generate();
  let job_id = JobId::generate();
  let lease_id = LeaseId::generate();
  let artifact = |name: &str, media: &str, byte: u8| {
    let identity = ArtifactIdentity {
      artifact_id: octacity_server_domain::ArtifactId::generate(),
      build_id,
      attempt_id,
      job_id,
      lease_id,
      logical_name: octacity_server_domain::ArtifactName::new(name).unwrap(),
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new(media).unwrap(),
      size_bytes: 64,
      digest: ArtifactContentDigest::from_bytes([byte; 32]),
      retention: ArtifactRetentionPolicy::DeleteAfter(time(13)),
    };
    let pending = ArtifactRecord::pending(identity.clone(), time(10)).unwrap();
    let verifying = pending
      .transition(&identity, pending.version(), ArtifactEvent::BeginVerification, time(11))
      .unwrap();
    verifying
      .transition(&identity, verifying.version(), ArtifactEvent::Publish, time(11))
      .unwrap()
  };
  let bundle = artifact(CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_BUNDLE_MEDIA_TYPE, 94);
  let manifest = artifact(CHANGE_SET_MANIFEST_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, 95);
  let candidate = ChangeSet::new(
    ChangeSetId::generate(),
    request.intent.input(),
    request.intent.node(),
    candidate_subject.clone(),
    bundle.identity().artifact_id,
    manifest.identity().artifact_id,
  )
  .unwrap();
  let subject = FactoryTaskSubject::Candidate(candidate_subject);
  let entries = [&bundle, &manifest]
    .into_iter()
    .map(|record| {
      let identity = record.identity();
      let reference = FactoryArtifactReference::new(
        identity.artifact_id,
        FactoryDigest::from_bytes(identity.digest.as_bytes()),
        identity.size_bytes,
      )
      .unwrap();
      ContextManifestEntry::new(
        ContextSourceKind::Evidence,
        FactoryKey::new(identity.logical_name.as_str()).unwrap(),
        subject.clone(),
        FactoryContextReference::Artifact(reference.clone()),
        reference.content_digest(),
        reference.encoded_size(),
        FactorySafeText::new("Explicit retained candidate input").unwrap(),
        FactoryDigest::from_bytes([96; 32]),
      )
      .unwrap()
    })
    .collect();
  let selected = request
    .intent
    .input()
    .clone()
    .with_context(
      ContextManifest::new(
        ContextManifestId::generate(),
        subject,
        FactoryDigest::from_bytes([97; 32]),
        entries,
      )
      .unwrap(),
      vec![],
    )
    .unwrap();
  let previous = request.intent.node();
  let node = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(selected.definition()).unwrap(),
    NodeAttemptInput {
      id: previous.id(),
      node_key: selected.node().clone(),
      node_kind: previous.node_kind(),
      number: previous.number(),
      input_digest: selected.digest().unwrap(),
      budget: previous.budget(),
      started_at: previous.started_at(),
      deadline: previous.deadline(),
      execution: previous.execution().clone(),
      ownership: request.ownership.clone(),
    },
  )
  .unwrap();
  request.intent =
    FlowBuildIntent::new(selected, &admitted, node, Default::default(), request.intent.priority()).unwrap();
  (request, candidate, bundle, manifest)
}

#[test]
fn factory_materialization_signs_only_explicit_retained_candidate_inputs() {
  use octacity_server_application::{
    FactoryManagedCandidateProof, FactoryManagedSourcePlan, materialize_factory_managed_build_with_source,
  };
  let (input, candidate, bundle, manifest) = factory_candidate_materialization_fixture();
  let proof = FactoryManagedCandidateProof::verify(
    input.intent.input(),
    candidate.clone(),
    bundle.clone(),
    manifest.clone(),
    time(12),
  )
  .unwrap();
  let plan = FactoryManagedSourcePlan {
    command: None,
    candidate: Some(proof),
    capture: None,
  };
  let request = materialize_factory_managed_build_with_source(input.clone(), plan.clone()).unwrap();
  request.validate_fresh().unwrap();
  assert_eq!(
    request,
    materialize_factory_managed_build_with_source(input.clone(), plan.clone()).unwrap()
  );
  assert_eq!(request.build.immutable_revision.as_str(), "base");
  assert_eq!(request.jobs.len(), 2);
  for job in &request.jobs {
    assert_eq!(job.job_spec_template.source_revision().as_str(), "base");
    let managed = job.job_spec_template.managed_intent().unwrap();
    let materialization = managed.factory().change_set_materialization.as_ref().unwrap();
    assert_eq!(
      materialization.candidate_revision,
      candidate.subject().candidate_revision().as_str()
    );
    assert_eq!(materialization.bundle_input, bundle.identity().artifact_id.to_string());
    assert_eq!(
      materialization.manifest_input,
      manifest.identity().artifact_id.to_string()
    );
    assert!(managed.factory().change_set_capture.is_none());
    assert!(managed.factory().source_write_paths.is_none());
    let compiler = &input.programs[job.job_spec_template.pipeline_node_id()].protected_inputs;
    assert_eq!(managed.protected_inputs().inputs.len(), compiler.inputs.len() + 2);
    for original in &compiler.inputs {
      assert!(managed.protected_inputs().inputs.contains(original));
    }
    for record in [&bundle, &manifest] {
      let protected = managed
        .protected_inputs()
        .inputs
        .iter()
        .find(|artifact| artifact.artifact_id == record.identity().artifact_id.to_string())
        .unwrap();
      assert_eq!(protected.sha256, record.identity().digest.to_string());
      assert_eq!(protected.size_bytes, record.identity().size_bytes);
    }
  }
  let mut expired = input.clone();
  expired.accepted_at = time(13);
  assert!(
    materialize_factory_managed_build_with_source(expired, plan.clone()).is_err(),
    "proof-time retention cannot authorize later expired artifacts"
  );
  let (foreign, _, _, _) = factory_candidate_materialization_fixture();
  assert!(
    materialize_factory_managed_build_with_source(foreign, plan).is_err(),
    "proof cannot be reused for another frozen input"
  );
  let pending = octacity_server_artifacts::ArtifactRecord::pending(bundle.identity().clone(), time(10)).unwrap();
  assert!(FactoryManagedCandidateProof::verify(input.intent.input(), candidate, pending, manifest, time(12)).is_err());
}

#[test]
fn factory_materialization_signs_capture_only_inside_the_frozen_profile_scope() {
  use factory::*;
  use octacity_protocol::{
    ChangeSetBinaryPolicyV3, ChangeSetCapturePolicyV3, ChangeSetCaptureV3, ChangeSetSecretPatternV3,
  };
  use octacity_server_application::{
    FactoryManagedSourcePlan, materialize_factory_managed_build, materialize_factory_managed_build_with_source,
  };
  let (mut request, _, work, mut binding) = factory_materialization_fixture();
  binding.capture_write_paths = Some(vec![FactoryRepositoryPath::new("specifications/approved").unwrap()]);
  binding.capture_deliverables = Some(serde_json::from_value(json!({
    "accepted_contract": {"value": {"mapping": "constant", "value": "a".repeat(64)}},
    "policy": {"permitted_paths": ["specifications/approved"], "forbidden_paths": ["private"], "required": [{"kind": "public-design", "paths": ["specifications/approved"], "min_files": 1}]}
  })).unwrap());
  let (_, admitted, _, input, previous) = flow_fixture::execution_fixture_with_permissions(
    FlowNodeKind::BuildCommand,
    work,
    binding,
    None,
    request.policy_layers.task.clone(),
  );
  let node = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(input.definition()).unwrap(),
    NodeAttemptInput {
      id: previous.id(),
      node_key: input.node().clone(),
      node_kind: previous.node_kind(),
      number: previous.number(),
      input_digest: input.digest().unwrap(),
      budget: previous.budget(),
      started_at: time(10),
      deadline: time(1010),
      execution: previous.execution().clone(),
      ownership: request.ownership.clone(),
    },
  )
  .unwrap();
  request.intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 17).unwrap();
  let capture = ChangeSetCaptureV3 {
    author_name: "Configured operator".to_owned(),
    author_email: "operator@example.test".to_owned(),
    committed_at: 1,
    patch_max_bytes: Some(1024),
    policy: ChangeSetCapturePolicyV3 {
      allowed_path_prefixes: vec!["specifications/approved".to_owned()],
      forbidden_control_paths: vec![".github/workflows".to_owned()],
      max_changed_paths: 2,
      max_file_bytes: 128,
      max_total_bytes: 256,
      allow_empty: false,
      binary_policy: ChangeSetBinaryPolicyV3::Reject,
      forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
    },

    observation: None,
  };
  capture.validate().unwrap();
  let plan = FactoryManagedSourcePlan {
    command: None,
    candidate: None,
    capture: Some(capture.clone()),
  };
  let materialized = materialize_factory_managed_build_with_source(request.clone(), plan.clone()).unwrap();
  materialized.validate_fresh().unwrap();
  for job in &materialized.jobs {
    let managed = job.job_spec_template.managed_intent().unwrap();
    assert_eq!(managed.factory().change_set_capture.as_ref(), Some(&capture));
    assert_eq!(
      managed.factory().source_write_paths.as_ref().unwrap(),
      &["specifications/approved"]
    );
    assert!(managed.factory().change_set_materialization.is_none());
    assert_eq!(
      managed.protected_inputs(),
      &request.programs[job.job_spec_template.pipeline_node_id()].protected_inputs
    );
  }
  assert!(
    materialize_factory_managed_build(request.clone()).is_err(),
    "readonly callers cannot silently skip a required capture"
  );
  for paths in [
    vec![".".to_owned()],
    vec!["specifications".to_owned()],
    vec!["other".to_owned()],
  ] {
    let mut widened = capture.clone();
    widened.policy.allowed_path_prefixes = paths;
    assert!(
      materialize_factory_managed_build_with_source(
        request.clone(),
        FactoryManagedSourcePlan {
          command: None,
          candidate: None,
          capture: Some(widened)
        }
      )
      .is_err()
    );
  }
  let mut invalid = capture;
  invalid.author_email = "invalid".to_owned();
  assert!(
    materialize_factory_managed_build_with_source(
      request,
      FactoryManagedSourcePlan {
        command: None,
        candidate: None,
        capture: Some(invalid)
      }
    )
    .is_err()
  );
  let (readonly, _, _, _) = factory_materialization_fixture();
  assert!(
    materialize_factory_managed_build_with_source(readonly.clone(), plan).is_err(),
    "capture caller cannot add write scope absent from the admitted profile"
  );
  assert_eq!(
    materialize_factory_managed_build(readonly.clone()).unwrap(),
    materialize_factory_managed_build_with_source(readonly, FactoryManagedSourcePlan::default()).unwrap()
  );
}

#[test]
fn factory_materialization_preserves_rejection_of_noncanonical_compiler_inputs() {
  use octacity_server_application::materialize_factory_managed_build;
  let (mut request, _, _, _) = factory_materialization_fixture();
  let program = request.programs.values_mut().next().unwrap();
  assert!(program.protected_inputs.inputs.len() > 1);
  program.protected_inputs.inputs.reverse();
  assert!(
    materialize_factory_managed_build(request).is_err(),
    "source-plan assembly cannot canonicalize a malformed compiler manifest into signing authority"
  );
}

#[test]
fn installed_readonly_codex_materializer_uses_verified_candidate_source_without_write_authority() {
  use factory::*;
  use factory_candidate_context_test_support::{ReadonlyCandidateFixtureProfile, candidate_context_fixture};
  use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
  use octacity_server_store::{
    FactoryExecutionInputDisposition, FactoryExecutionInputStore, PublishFactoryExecutionInputs,
  };
  struct ContextSource(BuildExecutionContext);
  #[async_trait]
  impl BuildExecutionContextSource for ContextSource {
    async fn load(
      &self,
      project: ProjectId,
      id: BuildConfigurationId,
      version: BuildConfigurationVersion,
    ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
      assert_eq!(
        (project, id, version),
        (
          self.0.configuration.project_id,
          self.0.configuration.id,
          self.0.configuration.version
        )
      );
      Ok(self.0.clone())
    }
  }
  #[derive(Default)]
  struct Inputs(Mutex<Vec<PublishFactoryExecutionInputs>>);
  #[async_trait]
  impl octacity_server_store::FactoryRoleInvocationStore for Inputs {}
  #[async_trait]
  impl FactoryExecutionInputStore for Inputs {
    async fn reserve_factory_execution_inputs(
      &self,
      request: PublishFactoryExecutionInputs,
    ) -> Result<FactoryExecutionInputDisposition, StoreError> {
      request.validate()?;
      Ok(FactoryExecutionInputDisposition::Pending)
    }
    async fn publish_factory_execution_inputs(&self, request: PublishFactoryExecutionInputs) -> Result<(), StoreError> {
      self.0.lock().unwrap().push(request);
      Ok(())
    }
  }
  struct Bytes;
  #[async_trait]
  impl ArtifactBytePublisher for Bytes {
    async fn publish_verified(&self, _: &ArtifactObject, _: Vec<u8>) -> Result<(), ArtifactStoreError> {
      Ok(())
    }
  }
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(time(32))
    }
  }
  struct NoRawCandidate;
  #[async_trait]
  impl FactoryNodeContextSource for NoRawCandidate {
    async fn read_context(
      &self,
      _: &FlowNodeInput,
      _: Timestamp,
      _: Timestamp,
    ) -> Result<BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>, ApplicationError> {
      panic!("candidate bundle/manifest must use the opaque stored source, not raw text context")
    }
  }
  run(async {
    let (base, _, work, binding) = factory_materialization_fixture();
    let mut grants = serde_json::to_value(&base.policy_layers.task).unwrap();
    for mount in grants["mounts"].as_array_mut().unwrap() {
      if mount["root"] == "/workspace/source" {
        mount["mode"] = json!("read_only");
      }
    }
    grants["secret_profiles"] = json!(["model-evaluation"]);
    grants["outputs"]["kinds"] = json!(["codex-run-trace", "codex-run-provenance", "codex-run-result"]);
    let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
    let credentials = FactoryCredentialProfiles::new(
      flow_fixture::key("model-coding"),
      flow_fixture::key("model-evaluation"),
      flow_fixture::key("source-read"),
      flow_fixture::key("delivery-write"),
    )
    .unwrap();
    let toolchain = FactoryTaskToolchain {
      executable: binding.tool,
      plugin: binding.plugin,
      model: binding.model_or_tool,
    };
    let fixture = candidate_context_fixture(ReadonlyCandidateFixtureProfile {
      work,
      build_configuration: binding.build_configuration,
      toolchain: toolchain.clone(),
      permissions: grants.clone(),
      credentials: credentials.clone(),
      instructions: FactoryText::new("Measure only the selected immutable candidate.").unwrap(),
    })
    .await;
    let mut context = base.context;
    let node = context.pipeline.dag.nodes()[0].clone();
    context.pipeline.dag = PublishablePipelineDag::new(
      vec![node.clone()],
      vec![],
      &CapabilityCatalog::new(node.required_capabilities().iter().cloned()),
    )
    .unwrap()
    .into_dag();
    context.configuration.definition.secrets_profile = Some(SecretProfileName::new("model-evaluation").unwrap());
    context
      .effective_policy
      .policy
      .secret_profiles
      .insert(SecretProfileName::new("model-evaluation").unwrap());
    let profile = FactoryCodexBuildProfile {
      authentication: crate::FactoryCodexAuthentication::default(),
      mcp_configurations: vec![],
      toolchain,
      build_configuration: fixture.intent.profile().build_configuration.clone(),
      instructions: fixture.instructions.clone(),
      credentials,
      consumer: FactoryCredentialConsumer::ReadOnlyReview,
      usage_policy: FactoryCodexUsagePolicy::ReportedCost,
      tool_risk: None,
      tool_response_limits: None,
      reviewer: None,
      candidate_capture_tool: None,
      capture: None,
      pipeline_node: node.id().clone(),
      parameters: base.parameters,
      project_permissions: grants.clone(),
      local_permissions: LocalPermissionCeiling::fully_enforced(grants),
    };
    let inputs = Arc::new(Inputs::default());
    let publisher = Arc::new(FactoryExecutionInputPublisher::new(
      inputs.clone(),
      Arc::new(Bytes),
      Arc::new(Clock),
    ));
    let materializer = FactoryCodexBuildMaterializer::new(
      Arc::new(ContextSource(context)),
      publisher,
      vec![profile],
      Arc::new(NoRawCandidate),
      Arc::new(Clock),
    )
    .unwrap();
    let ownership = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
    assert!(
      materializer
        .materialize(&fixture.intent, &fixture.admitted, ownership.clone(), time(32))
        .await
        .is_err(),
      "default installation still rejects candidate authority before publication"
    );
    assert!(inputs.0.lock().unwrap().is_empty());
    let source = Arc::new(FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      flow_fixture::reference("git.capture"),
      Arc::new(Clock),
    ));
    let materializer = materializer.with_candidate_context_source(source);
    let accepted = materializer
      .materialize(&fixture.intent, &fixture.admitted, ownership, time(32))
      .await
      .unwrap();
    assert_eq!(accepted.intent, fixture.intent);
    assert_eq!(accepted.jobs.len(), 1);
    let spec = &accepted.jobs[0].job_spec_template;
    assert_eq!(spec.source_revision().as_str(), "base");
    let managed = spec.managed_intent().unwrap();
    let factory = managed.factory();
    let candidate = factory.change_set_materialization.as_ref().unwrap();
    assert_eq!(
      candidate.candidate_revision,
      "0123456789abcdef0123456789abcdef0123456789"
    );
    assert_eq!(
      candidate.bundle_input,
      fixture.context.bundle().identity().artifact_id.to_string()
    );
    assert_eq!(
      candidate.manifest_input,
      fixture.context.manifest().identity().artifact_id.to_string()
    );
    assert!(factory.source_write_paths.is_none());
    assert!(factory.change_set_capture.is_none());
    assert_eq!(managed.protected_inputs().inputs.len(), 6);
    assert!(managed.permissions().tools.is_empty());
    assert_eq!(
      managed.permissions().secret_profiles,
      vec!["model-evaluation".to_owned()]
    );
    assert!(
      managed
        .permissions()
        .mounts
        .iter()
        .filter(|mount| mount.root == octacity_protocol::FACTORY_SOURCE_ROOT)
        .all(|mount| mount.mode == octacity_protocol::FactoryMountModeV3::ReadOnly)
    );
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
  });
}
