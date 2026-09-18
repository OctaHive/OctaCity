use std::{
  collections::{BTreeMap, BTreeSet},
  future::Future,
  sync::{Arc, Mutex},
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_protocol::{OctaSpec, PlatformArchitecture, PlatformOs};
use octacity_server_application::{
  AcceptManualTriggerCommand, ArtifactPolicy, CachePolicy, CommandHandler, ConcurrencyPolicy, EffectiveProjectPolicy,
  IdentityProfileName, JobAssignmentProjection, JobProjection, JobProjectionFacts, JobQueueProjection,
  JobSpecToolchainPolicy, JobTerminalOutcomeProjection, ManualSourceSelection, ManualTriggerCommand,
  ManualTriggerContext, ManualTriggerContextError, ManualTriggerContextProvider, ManualTriggerError,
  ManualTriggerInputError, ManualTriggerOutcome, ManualTriggerService, MutationDisposition as ApplicationDisposition,
  PolicySource, RetentionPolicy, RevisionResolutionError, RevisionResolutionRequest, RevisionResolver, RuntimeClass,
  SecretProfileName,
};
use octacity_server_domain::{
  AgentId, AttemptId, BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, BuildId,
  ImmutableRevision, IntegrationId, JobId, JobName, JobVersion, LeaseId, PipelineId, PipelineName, PipelineNodeId,
  PipelineVersion, PoolId, ProjectId, ProjectPolicyVersion, RepositoryId, RepositoryName, RepositoryVersion,
  SourceReference, Timestamp, TriggerId, TriggerIdentity, TriggerVersion,
};
use octacity_server_job::{JobState, SourcePluginPolicy};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineEdge, PipelineNode, PublishablePipelineDag,
};
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, BuildConfigurationDefinition, ConfigurationAgentRequirements,
  ConfigurationCachePolicy, ConfigurationNetworkPolicy, ConfigurationRetryPolicy, ConfigurationRuntimePolicy,
  ConfigurationTriggerPolicy, MutationDisposition as StoreDisposition, ParameterDefinition, ParameterSchema,
  ParameterType, PublishedBuildConfiguration, PublishedPipeline, PublishedRepository, RepositoryDefinition,
  RepositorySelectionPolicy, RetryClass, StoreError, SuppressTrigger, SuppressTriggerOutcome, TriggerAcceptanceProbe,
  TriggerDefinitionRef, TriggerEvaluationOutcome, TriggerKind, TriggerTarget,
};
use serde_json::json;

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
      .handle_command(AcceptManualTriggerCommand {
        trigger: fixture.command.clone(),
        accepted_at: time(200),
      })
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
    assert_eq!(root.job_spec_template.pipeline_node_id(), &root.pipeline_node_id);
    assert_eq!(resolver.requests.lock().unwrap().len(), 1);
    assert!(matches!(
      &resolver.requests.lock().unwrap()[0].selection,
      ManualSourceSelection::Reference(reference) if reference.as_str() == "refs/heads/main"
    ));

    let mut replay = fixture.command;
    replay.observed_at = time(999);
    let replay_service = ManualTriggerService::new(store.clone(), Arc::new(UnreachableContext), resolver.clone());
    let replayed = replay_service.accept(replay, time(300)).await.unwrap();
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
      service.accept(fixture.command, time(200)).await,
      Err(ManualTriggerError::Invalid(
        ManualTriggerInputError::ReferenceNotAllowed
      ))
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
    assert!(store.requests.lock().unwrap().is_empty());
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

    let outcome = service.accept(fixture.command.clone(), time(200)).await.unwrap();
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
    let replayed = replay_service.accept(fixture.command, time(300)).await.unwrap();
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
      service.accept(fixture.command, time(200)).await,
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
      service.accept(fixture.command, time(200)).await,
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
      service.accept(fixture.command, time(200)).await,
      Err(ManualTriggerError::Context(ManualTriggerContextError::Store(
        StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Trigger
        }
      )))
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
  });
}

struct Fixture {
  command: ManualTriggerCommand,
  context: ManualTriggerContext,
  shared_pool: PoolId,
}

fn fixture() -> Fixture {
  let project_id = id::<ProjectId>(1);
  let repository_id = id::<RepositoryId>(2);
  let pipeline_id = id::<PipelineId>(3);
  let configuration_id = id::<BuildConfigurationId>(4);
  let configured_pool = id::<PoolId>(5);
  let shared_pool = id::<PoolId>(6);
  let policy_only_pool = id::<PoolId>(7);
  let capability = ExecutionCapability::new("native").unwrap();
  let root = PipelineNode::new(
    PipelineNodeId::new("root").unwrap(),
    JobName::new("Root").unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![capability.clone()],
    json!({"commands": ["build"]}),
  )
  .unwrap();
  let child = PipelineNode::new(
    PipelineNodeId::new("child").unwrap(),
    JobName::new("Child").unwrap(),
    DependencyPolicy::AllSucceeded,
    Vec::new(),
    json!({"commands": ["test"]}),
  )
  .unwrap();
  let dag = PublishablePipelineDag::new(
    vec![root, child],
    vec![PipelineEdge::new(
      PipelineNodeId::new("root").unwrap(),
      PipelineNodeId::new("child").unwrap(),
    )],
    &CapabilityCatalog::new([capability.clone()]),
  )
  .unwrap()
  .into_dag();
  let artifacts = ArtifactPolicy {
    artifact_count: 10,
    artifact_bytes: 1_000_000,
    report_count: 10,
    report_bytes: 1_000_000,
    single_output_bytes: 100_000,
  };
  let configuration = PublishedBuildConfiguration {
    id: configuration_id,
    project_id,
    name: BuildConfigurationName::new("Build").unwrap(),
    version: BuildConfigurationVersion::INITIAL,
    definition: BuildConfigurationDefinition {
      enabled: true,
      repository_id,
      repository_version: RepositoryVersion::INITIAL,
      pipeline_id,
      pipeline_version: PipelineVersion::INITIAL,
      parameters: ParameterSchema {
        parameters: BTreeMap::from([
          (
            "required".to_owned(),
            ParameterDefinition {
              value_type: ParameterType::String,
              required: true,
              default: None,
            },
          ),
          (
            "with_default".to_owned(),
            ParameterDefinition {
              value_type: ParameterType::Boolean,
              required: false,
              default: Some(json!(true)),
            },
          ),
        ]),
        deny_unknown: true,
      },
      triggers: ConfigurationTriggerPolicy {
        allowed: BTreeSet::from([TriggerKind::Manual]),
      },
      agent_requirements: ConfigurationAgentRequirements {
        capabilities: BTreeSet::from([capability]),
        labels: BTreeMap::from([("tier".to_owned(), "trusted".to_owned())]),
        minimum_cpu_millis: 1_000,
        minimum_memory_bytes: 1_024,
        minimum_disk_bytes: 2_048,
      },
      allowed_pools: BTreeSet::from([configured_pool, shared_pool]),
      runtime: ConfigurationRuntimePolicy {
        class: RuntimeClass::Native,
        operating_system: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
        immutable_image: None,
        cpu_millis: 1_000,
        memory_bytes: 1_024,
        writable_disk_bytes: 2_048,
        timeout_seconds: 60,
        network: ConfigurationNetworkPolicy::Disabled,
        workload_identity_profile: None,
      },
      cache: ConfigurationCachePolicy {
        namespace: None,
        read: false,
        write: false,
      },
      artifacts,
      retry: ConfigurationRetryPolicy {
        max_attempts: 1,
        retry_on: BTreeSet::<RetryClass>::new(),
      },
    },
    published_at: time(10),
  };
  let repository = PublishedRepository {
    id: repository_id,
    project_id,
    name: RepositoryName::new("Source").unwrap(),
    version: RepositoryVersion::INITIAL,
    definition: RepositoryDefinition {
      vcs_integration_id: id::<IntegrationId>(8),
      repository_locator: octacity_server_domain::RepositoryLocator::new("https://example.test/repository.git")
        .unwrap(),
      selection: RepositorySelectionPolicy {
        allowed_references: BTreeSet::from([SourceReference::new("refs/heads/main").unwrap()]),
        default_reference: Some(SourceReference::new("refs/heads/main").unwrap()),
        allow_exact_revision: true,
      },
    },
    published_at: time(10),
  };
  let pipeline = PublishedPipeline {
    id: pipeline_id,
    project_id,
    name: PipelineName::new("Build").unwrap(),
    version: PipelineVersion::INITIAL,
    dag,
    published_at: time(10),
  };
  let effective_policy = EffectiveProjectPolicy {
    sources: vec![PolicySource {
      project_id,
      version: ProjectPolicyVersion::INITIAL,
    }],
    policy: octacity_server_application::ProjectPolicy {
      pools: BTreeSet::from([shared_pool, policy_only_pool]),
      repositories: BTreeSet::from([repository_id]),
      secret_profiles: BTreeSet::<SecretProfileName>::new(),
      identity_profiles: BTreeSet::<IdentityProfileName>::new(),
      runtimes: BTreeSet::from([RuntimeClass::Native]),
      cache: CachePolicy {
        namespaces: BTreeSet::new(),
        read: false,
        write: false,
        max_bytes: 0,
      },
      artifacts,
      concurrency: ConcurrencyPolicy {
        active_builds: 10,
        active_jobs: 10,
      },
      retention: RetentionPolicy {
        build_seconds: 86_400,
        log_seconds: 86_400,
        artifact_seconds: 86_400,
        cache_seconds: 86_400,
      },
    },
  };
  Fixture {
    command: ManualTriggerCommand {
      trigger: TriggerDefinitionRef {
        id: id::<TriggerId>(9),
        version: TriggerVersion::INITIAL,
      },
      target: TriggerTarget {
        configuration_id,
        configuration_version: BuildConfigurationVersion::INITIAL,
      },
      deduplication_identity: TriggerIdentity::new("manual:request-1").unwrap(),
      source: ManualSourceSelection::DefaultReference,
      parameters: BTreeMap::from([("required".to_owned(), json!("caller"))]),
      priority: 10,
      observed_at: time(100),
    },
    context: ManualTriggerContext {
      configuration,
      repository,
      pipeline,
      effective_policy,
      job_spec_toolchain: JobSpecToolchainPolicy {
        source: SourcePluginPolicy::new("git", "1.0.0", DIGEST, "url").unwrap(),
        octa: OctaSpec {
          version: "0.4.0".to_owned(),
          runner_sha256: DIGEST.to_owned(),
          runner_protocol: 3,
          event_schema: 3,
          plugin_protocol: 1,
          plugin_digests: BTreeMap::new(),
        },
        validity: octacity_server_job::JobSpecValidity::new(600).unwrap(),
      },
    },
    shared_pool,
  }
}

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct StaticContext(ManualTriggerContext);

#[async_trait]
impl ManualTriggerContextProvider for StaticContext {
  async fn load(
    &self,
    _trigger: TriggerDefinitionRef,
    _target: TriggerTarget,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
    Ok(self.0.clone())
  }
}

struct UnreachableContext;

#[async_trait]
impl ManualTriggerContextProvider for UnreachableContext {
  async fn load(
    &self,
    _trigger: TriggerDefinitionRef,
    _target: TriggerTarget,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
    panic!("an accepted manual Trigger must replay before loading mutable context")
  }
}

struct RejectingContext;

#[async_trait]
impl ManualTriggerContextProvider for RejectingContext {
  async fn load(
    &self,
    _trigger: TriggerDefinitionRef,
    _target: TriggerTarget,
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
