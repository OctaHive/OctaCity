use std::{
  collections::{BTreeMap, BTreeSet},
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
};

use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_application::{
  ApplicationError, BuildConfigurationHandlers, CommandHandler, CreateBuildConfigurationCommand, CreatePipelineCommand,
  CreateProjectCommand, CreateRepositoryCommand, GetBuildConfigurationQuery, GetPipelineQuery, GetProjectQuery,
  GetRepositoryQuery, ListProjectsQuery, MutationDisposition, PipelineHandlers, ProjectHandlers, QueryHandler,
};
use octacity_server_domain::{
  ArtifactPolicy, BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, EntityKind, IntegrationId,
  JobName, PipelineId, PipelineName, PipelineNodeId, PipelineVersion, PoolId, ProjectId, ProjectName, RepositoryId,
  RepositoryLocator, RepositoryName, RepositoryVersion, RuntimeClass, SourceReference, Timestamp,
};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineNode, PublishablePipelineDag,
};
use octacity_server_store::{
  BuildConfigurationDefinition, ConfigurationAgentRequirements, ConfigurationCachePolicy, ConfigurationNetworkPolicy,
  ConfigurationRetryPolicy, ConfigurationRuntimePolicy, ConfigurationTriggerPolicy, IdempotencyKey, ParameterSchema,
  RepositoryDefinition, RepositorySelectionPolicy, RetryClass, StoreError, TriggerKind,
  testing::{
    InMemoryConfigurationStore, InMemoryPipelineStore, InMemoryProjectStore, MutationEvidenceCounts,
    MutationEvidenceProbe, MutationFailurePoint,
  },
};
use serde_json::json;

#[test]
fn project_commands_and_queries_dispatch_through_the_in_memory_port() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let handlers = ProjectHandlers::new(store);
    let root_id = id::<ProjectId>(1);
    let child_id = id::<ProjectId>(2);

    let created = handlers
      .handle_command(CreateProjectCommand {
        id: root_id,
        parent_id: None,
        name: ProjectName::new("Root").unwrap(),
        idempotency_key: key("root"),
        created_at: time(1),
      })
      .await
      .unwrap();
    assert_eq!(created.disposition, MutationDisposition::Applied);

    handlers
      .handle_command(CreateProjectCommand {
        id: child_id,
        parent_id: Some(root_id),
        name: ProjectName::new("Child").unwrap(),
        idempotency_key: key("child"),
        created_at: time(2),
      })
      .await
      .unwrap();
    let project = handlers
      .handle_query(GetProjectQuery { project_id: child_id })
      .await
      .unwrap();
    let page = handlers
      .handle_query(ListProjectsQuery {
        parent_id: Some(root_id),
        after: None,
        limit: 10,
      })
      .await
      .unwrap();

    assert_eq!(project.ancestors[0].id, root_id);
    assert_eq!(page.projects[0].id, child_id);
  });
}

#[test]
fn every_injected_transaction_failure_rolls_back_the_complete_command() {
  run_ready(async {
    for failure in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ] {
      let store = Arc::new(InMemoryProjectStore::new());
      store.fail_next_create_at(failure).unwrap();
      let handlers = ProjectHandlers::new(Arc::clone(&store));
      let project_id = id::<ProjectId>(100);
      let command = CreateProjectCommand {
        id: project_id,
        parent_id: None,
        name: ProjectName::new("Transactional").unwrap(),
        idempotency_key: key("transactional-create"),
        created_at: time(100),
      };

      assert!(matches!(
        handlers.handle_command(command.clone()).await,
        Err(ApplicationError::Store(StoreError::Unavailable))
      ));
      assert!(matches!(
        handlers.handle_query(GetProjectQuery { project_id }).await,
        Err(ApplicationError::Store(StoreError::NotFound {
          entity: EntityKind::Project
        }))
      ));
      assert_eq!(
        store.mutation_evidence_counts().await,
        MutationEvidenceCounts {
          idempotency: 0,
          audit: 0,
          outbox: 0,
        },
        "{failure:?} leaked transactional evidence"
      );

      assert_eq!(
        handlers.handle_command(command).await.unwrap().disposition,
        MutationDisposition::Applied,
        "{failure:?} prevented a clean retry"
      );
      assert_eq!(
        store.mutation_evidence_counts().await,
        MutationEvidenceCounts {
          idempotency: 1,
          audit: 1,
          outbox: 1,
        },
        "{failure:?} did not commit the complete retry"
      );
    }
  });
}

#[test]
fn pipeline_commands_and_queries_return_application_projections() {
  run_ready(async {
    let store = Arc::new(InMemoryPipelineStore::new());
    let project_id = id::<ProjectId>(10);
    let pipeline_id = id::<PipelineId>(11);
    store.seed_project(project_id).unwrap();
    let handlers = PipelineHandlers::new(store);

    let outcome = handlers
      .handle_command(CreatePipelineCommand {
        id: pipeline_id,
        project_id,
        name: PipelineName::new("Build").unwrap(),
        dag: pipeline(),
        idempotency_key: key("pipeline"),
        published_at: time(10),
      })
      .await
      .unwrap();
    let queried = handlers
      .handle_query(GetPipelineQuery {
        pipeline_id,
        version: PipelineVersion::INITIAL,
      })
      .await
      .unwrap();

    assert_eq!(outcome.pipeline, queried);
    assert_eq!(queried.nodes[0].execution.commands, ["build"]);
  });
}

#[test]
fn configuration_commands_and_queries_use_only_backend_neutral_ports() {
  run_ready(async {
    let store = Arc::new(InMemoryConfigurationStore::new());
    let project_id = id::<ProjectId>(20);
    let repository_id = id::<RepositoryId>(21);
    let pipeline_id = id::<PipelineId>(22);
    let pool_id = id::<PoolId>(23);
    let configuration_id = id::<BuildConfigurationId>(24);
    store.seed_project(project_id).unwrap();
    store
      .seed_pipeline_version(project_id, pipeline_id, PipelineVersion::INITIAL)
      .unwrap();
    store.seed_pool(pool_id).unwrap();
    let handlers = BuildConfigurationHandlers::new(store);
    handlers
      .handle_command(CreateRepositoryCommand {
        id: repository_id,
        project_id,
        name: RepositoryName::new("Source").unwrap(),
        definition: repository_definition(),
        idempotency_key: key("repository"),
        published_at: time(20),
      })
      .await
      .unwrap();
    let repository = handlers
      .handle_query(GetRepositoryQuery {
        repository_id,
        version: RepositoryVersion::INITIAL,
      })
      .await
      .unwrap();

    let outcome = handlers
      .handle_command(CreateBuildConfigurationCommand {
        id: configuration_id,
        project_id,
        name: BuildConfigurationName::new("Release").unwrap(),
        definition: configuration_definition(repository_id, pipeline_id, pool_id),
        idempotency_key: key("configuration"),
        published_at: time(21),
      })
      .await
      .unwrap();
    let queried = handlers
      .handle_query(GetBuildConfigurationQuery {
        configuration_id,
        version: BuildConfigurationVersion::INITIAL,
      })
      .await
      .unwrap();

    assert_eq!(outcome.configuration, queried);
    assert_eq!(queried.repository_id, repository_id);
    assert_eq!(
      repository.repository_locator.as_str(),
      "https://example.test/source.git"
    );
  });
}

#[test]
fn create_replays_ignore_server_generated_identifiers() {
  run_ready(async {
    let project_store = Arc::new(InMemoryProjectStore::new());
    let project_handlers = ProjectHandlers::new(Arc::clone(&project_store));
    let first_project_id = id::<ProjectId>(40);
    let project = project_handlers
      .handle_command(CreateProjectCommand {
        id: first_project_id,
        parent_id: None,
        name: ProjectName::new("Replay project").unwrap(),
        idempotency_key: key("replay-project"),
        created_at: time(40),
      })
      .await
      .unwrap();
    let replayed_project = project_handlers
      .handle_command(CreateProjectCommand {
        id: id::<ProjectId>(41),
        parent_id: None,
        name: ProjectName::new("Replay project").unwrap(),
        idempotency_key: key("replay-project"),
        created_at: time(41),
      })
      .await
      .unwrap();
    assert_eq!(replayed_project.disposition, MutationDisposition::Replayed);
    assert_eq!(replayed_project.project.id, project.project.id);

    let pipeline_store = Arc::new(InMemoryPipelineStore::new());
    pipeline_store.seed_project(first_project_id).unwrap();
    let pipeline_handlers = PipelineHandlers::new(pipeline_store);
    let first_pipeline_id = id::<PipelineId>(42);
    let created_pipeline = pipeline_handlers
      .handle_command(CreatePipelineCommand {
        id: first_pipeline_id,
        project_id: first_project_id,
        name: PipelineName::new("Replay pipeline").unwrap(),
        dag: pipeline(),
        idempotency_key: key("replay-pipeline"),
        published_at: time(42),
      })
      .await
      .unwrap();
    let replayed_pipeline = pipeline_handlers
      .handle_command(CreatePipelineCommand {
        id: id::<PipelineId>(43),
        project_id: first_project_id,
        name: PipelineName::new("Replay pipeline").unwrap(),
        dag: pipeline(),
        idempotency_key: key("replay-pipeline"),
        published_at: time(43),
      })
      .await
      .unwrap();
    assert_eq!(replayed_pipeline.disposition, MutationDisposition::Replayed);
    assert_eq!(replayed_pipeline.pipeline.id, created_pipeline.pipeline.id);

    let configuration_store = Arc::new(InMemoryConfigurationStore::new());
    configuration_store.seed_project(first_project_id).unwrap();
    configuration_store
      .seed_pipeline_version(first_project_id, first_pipeline_id, PipelineVersion::INITIAL)
      .unwrap();
    let pool_id = id::<PoolId>(44);
    configuration_store.seed_pool(pool_id).unwrap();
    let configuration_handlers = BuildConfigurationHandlers::new(configuration_store);
    let first_repository_id = id::<RepositoryId>(45);
    let created_repository = configuration_handlers
      .handle_command(CreateRepositoryCommand {
        id: first_repository_id,
        project_id: first_project_id,
        name: RepositoryName::new("Replay repository").unwrap(),
        definition: repository_definition(),
        idempotency_key: key("replay-repository"),
        published_at: time(45),
      })
      .await
      .unwrap();
    let replayed_repository = configuration_handlers
      .handle_command(CreateRepositoryCommand {
        id: id::<RepositoryId>(46),
        project_id: first_project_id,
        name: RepositoryName::new("Replay repository").unwrap(),
        definition: repository_definition(),
        idempotency_key: key("replay-repository"),
        published_at: time(46),
      })
      .await
      .unwrap();
    assert_eq!(replayed_repository.disposition, MutationDisposition::Replayed);
    assert_eq!(replayed_repository.repository.id, created_repository.repository.id);

    let created_configuration = configuration_handlers
      .handle_command(CreateBuildConfigurationCommand {
        id: id::<BuildConfigurationId>(47),
        project_id: first_project_id,
        name: BuildConfigurationName::new("Replay configuration").unwrap(),
        definition: configuration_definition(first_repository_id, first_pipeline_id, pool_id),
        idempotency_key: key("replay-configuration"),
        published_at: time(47),
      })
      .await
      .unwrap();
    let replayed_configuration = configuration_handlers
      .handle_command(CreateBuildConfigurationCommand {
        id: id::<BuildConfigurationId>(48),
        project_id: first_project_id,
        name: BuildConfigurationName::new("Replay configuration").unwrap(),
        definition: configuration_definition(first_repository_id, first_pipeline_id, pool_id),
        idempotency_key: key("replay-configuration"),
        published_at: time(48),
      })
      .await
      .unwrap();
    assert_eq!(replayed_configuration.disposition, MutationDisposition::Replayed);
    assert_eq!(
      replayed_configuration.configuration.id,
      created_configuration.configuration.id
    );
  });
}

fn pipeline() -> PublishablePipelineDag {
  let capability = ExecutionCapability::new("native").unwrap();
  let node = PipelineNode::new(
    PipelineNodeId::new("build").unwrap(),
    JobName::new("Build").unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![capability.clone()],
    json!({"commands": ["build"]}),
  )
  .unwrap();
  PublishablePipelineDag::new(vec![node], Vec::new(), &CapabilityCatalog::new([capability])).unwrap()
}

fn repository_definition() -> RepositoryDefinition {
  let main = SourceReference::new("refs/heads/main").unwrap();
  RepositoryDefinition {
    vcs_integration_id: id::<IntegrationId>(30),
    repository_locator: RepositoryLocator::new("https://example.test/source.git").unwrap(),
    selection: RepositorySelectionPolicy {
      allowed_references: BTreeSet::from([main.clone()]),
      default_reference: Some(main),
      allow_exact_revision: true,
    },
  }
}

fn configuration_definition(
  repository_id: RepositoryId,
  pipeline_id: PipelineId,
  pool_id: PoolId,
) -> BuildConfigurationDefinition {
  BuildConfigurationDefinition {
    enabled: true,
    job_concurrency_limit: 2,
    repository_id,
    repository_version: RepositoryVersion::INITIAL,
    pipeline_id,
    pipeline_version: PipelineVersion::INITIAL,
    parameters: ParameterSchema {
      parameters: BTreeMap::new(),
      deny_unknown: true,
    },
    triggers: ConfigurationTriggerPolicy {
      allowed: BTreeSet::from([TriggerKind::Manual]),
    },
    agent_requirements: ConfigurationAgentRequirements {
      capabilities: BTreeSet::from([ExecutionCapability::new("native").unwrap()]),
      labels: BTreeMap::new(),
      minimum_cpu_millis: 1,
      minimum_memory_bytes: 1,
      minimum_disk_bytes: 1,
    },
    allowed_pools: BTreeSet::from([pool_id]),
    runtime: ConfigurationRuntimePolicy {
      class: RuntimeClass::Native,
      operating_system: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
      immutable_image: None,
      cpu_millis: 1,
      memory_bytes: 1,
      writable_disk_bytes: 1,
      timeout_seconds: 60,
      network: ConfigurationNetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: ConfigurationCachePolicy {
      namespace: None,
      read: false,
      write: false,
    },
    artifacts: ArtifactPolicy {
      artifact_count: 1,
      artifact_bytes: 1,
      report_count: 1,
      report_bytes: 1,
      single_output_bytes: 1,
    },
    retry: ConfigurationRetryPolicy {
      max_attempts: 1,
      retry_on: BTreeSet::<RetryClass>::new(),
    },
  }
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
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
  PipelineId,
  BuildConfigurationId,
  RepositoryId,
  PoolId,
  IntegrationId
);

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory CQRS handler unexpectedly waited for external I/O"),
  }
}
