use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, EntityKind, IntegrationId, PipelineId,
  PipelineVersion, PoolId, ProjectId, RepositoryId, RepositoryName, RepositoryVersion,
};
use octacity_server_pipeline::ExecutionCapability;
use serde_json::json;

use crate::test_support::{id, run_ready, time};
use crate::testing::{InMemoryConfigurationStore, MutationEvidenceProbe};
use crate::{
  ArtifactPolicy, BuildConfigurationDefinition, ConfigurationAgentRequirements, ConfigurationCachePolicy,
  ConfigurationNetworkPolicy, ConfigurationRetryPolicy, ConfigurationRuntimePolicy, ConfigurationStore,
  ConfigurationTriggerPolicy, CreateBuildConfiguration, CreateRepository, IdempotencyKey, MutationDisposition,
  ParameterDefinition, ParameterSchema, ParameterType, PublishBuildConfigurationVersion, PublishRepositoryVersion,
  RepositoryDefinition, RepositorySelectionPolicy, RetryClass, RuntimeClass, SourceReference, StoreError,
  StoreInputError, StoreOperation, TriggerKind,
};

/// Runs the reusable immutable Repository and Build Configuration contract.
pub async fn verify_configuration_store_contract<S, P>(
  store: Arc<S>,
  evidence: Arc<P>,
  project_id: ProjectId,
  pipeline_id: PipelineId,
  allowed_pool: PoolId,
) where
  S: ConfigurationStore + 'static,
  P: MutationEvidenceProbe + 'static,
{
  let repository_id = id::<RepositoryId>(100);
  let repository_v1 = repository_definition("octahive/octacity");
  let created_repository = store
    .create_repository(CreateRepository {
      id: repository_id,
      project_id,
      name: RepositoryName::new("source").unwrap(),
      definition: repository_v1.clone(),
      idempotency_key: key("create-repository"),
      published_at: time(10),
    })
    .await
    .unwrap();
  assert_eq!(created_repository.disposition, MutationDisposition::Applied);
  assert_eq!(created_repository.repository.version, RepositoryVersion::INITIAL);
  let replayed_repository = store
    .create_repository(CreateRepository {
      id: repository_id,
      project_id,
      name: RepositoryName::new("source").unwrap(),
      definition: repository_v1.clone(),
      idempotency_key: key("create-repository"),
      published_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(replayed_repository.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed_repository.repository, created_repository.repository);
  assert_eq!(
    store
      .create_repository(CreateRepository {
        id: id(101),
        project_id,
        name: RepositoryName::new("other").unwrap(),
        definition: repository_v1.clone(),
        idempotency_key: key("create-repository"),
        published_at: time(11),
      })
      .await
      .unwrap_err(),
    conflict(EntityKind::Repository)
  );
  assert_eq!(
    store
      .create_repository(CreateRepository {
        id: id(101),
        project_id,
        name: RepositoryName::new("source").unwrap(),
        definition: repository_v1.clone(),
        idempotency_key: key("duplicate-repository-name"),
        published_at: time(11),
      })
      .await
      .unwrap_err(),
    conflict(EntityKind::Repository)
  );

  let repository_v2 = repository_definition("octahive/octacity-renamed");
  let published_repository = store
    .publish_repository_version(PublishRepositoryVersion {
      id: repository_id,
      expected_current_version: RepositoryVersion::INITIAL,
      definition: repository_v2.clone(),
      idempotency_key: key("publish-repository-v2"),
      published_at: time(20),
    })
    .await
    .unwrap();
  assert_eq!(published_repository.repository.version.get(), 2);
  let replayed_repository_publication = store
    .publish_repository_version(PublishRepositoryVersion {
      id: repository_id,
      expected_current_version: RepositoryVersion::INITIAL,
      definition: repository_v2.clone(),
      idempotency_key: key("publish-repository-v2"),
      published_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(
    replayed_repository_publication.disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    replayed_repository_publication.repository,
    published_repository.repository
  );
  assert_eq!(
    store
      .repository_version(repository_id, RepositoryVersion::INITIAL)
      .await
      .unwrap(),
    created_repository.repository
  );
  assert_eq!(
    store
      .publish_repository_version(PublishRepositoryVersion {
        id: repository_id,
        expected_current_version: RepositoryVersion::INITIAL,
        definition: repository_definition("attempted/rewrite"),
        idempotency_key: key("rewrite-repository-v1"),
        published_at: time(30),
      })
      .await
      .unwrap_err(),
    conflict(EntityKind::Repository)
  );

  let configuration_id = id::<BuildConfigurationId>(200);
  let configuration_v1 = build_configuration(
    repository_id,
    RepositoryVersion::INITIAL,
    pipeline_id,
    PipelineVersion::INITIAL,
    allowed_pool,
    "debug",
  );
  let mut invalid_default = configuration_v1.clone();
  invalid_default
    .parameters
    .parameters
    .get_mut("profile")
    .unwrap()
    .default = Some(json!(42));
  assert_invalid_configuration(
    Arc::clone(&store),
    id(210),
    project_id,
    "invalid-default",
    invalid_default,
  )
  .await;
  let mut invalid_runtime = configuration_v1.clone();
  invalid_runtime.runtime.immutable_image = Some("mutable:latest".to_owned());
  assert_invalid_configuration(
    Arc::clone(&store),
    id(214),
    project_id,
    "invalid-runtime",
    invalid_runtime,
  )
  .await;
  let mut invalid_cache = configuration_v1.clone();
  invalid_cache.cache.read = true;
  assert_invalid_configuration(Arc::clone(&store), id(215), project_id, "invalid-cache", invalid_cache).await;
  let mut invalid_artifacts = configuration_v1.clone();
  invalid_artifacts.artifacts.report_bytes = 512;
  assert_invalid_configuration(
    Arc::clone(&store),
    id(216),
    project_id,
    "invalid-artifacts",
    invalid_artifacts,
  )
  .await;
  let mut invalid_retry = configuration_v1.clone();
  invalid_retry.retry.max_attempts = 0;
  assert_invalid_configuration(Arc::clone(&store), id(217), project_id, "invalid-retry", invalid_retry).await;
  let mut invalid_pools = configuration_v1.clone();
  invalid_pools.allowed_pools.clear();
  assert_invalid_configuration(Arc::clone(&store), id(218), project_id, "invalid-pools", invalid_pools).await;
  let mut invalid_triggers = configuration_v1.clone();
  invalid_triggers.triggers.allowed.clear();
  assert_invalid_configuration(
    Arc::clone(&store),
    id(219),
    project_id,
    "invalid-triggers",
    invalid_triggers,
  )
  .await;
  let mut missing_repository = configuration_v1.clone();
  missing_repository.repository_id = id(999);
  assert_missing_configuration_reference(
    Arc::clone(&store),
    id(211),
    project_id,
    "missing-repository",
    missing_repository,
    EntityKind::Repository,
  )
  .await;
  let mut missing_pipeline = configuration_v1.clone();
  missing_pipeline.pipeline_id = id(999);
  assert_missing_configuration_reference(
    Arc::clone(&store),
    id(212),
    project_id,
    "missing-pipeline",
    missing_pipeline,
    EntityKind::Pipeline,
  )
  .await;
  let mut missing_pool = configuration_v1.clone();
  missing_pool.allowed_pools = BTreeSet::from([id(999)]);
  assert_missing_configuration_reference(
    Arc::clone(&store),
    id(213),
    project_id,
    "missing-pool",
    missing_pool,
    EntityKind::Pool,
  )
  .await;
  let created_configuration = store
    .create_build_configuration(CreateBuildConfiguration {
      id: configuration_id,
      project_id,
      name: BuildConfigurationName::new("main").unwrap(),
      definition: configuration_v1.clone(),
      idempotency_key: key("create-configuration"),
      published_at: time(30),
    })
    .await
    .unwrap();
  assert_eq!(
    created_configuration.configuration.version,
    BuildConfigurationVersion::INITIAL
  );
  let replayed_configuration = store
    .create_build_configuration(CreateBuildConfiguration {
      id: configuration_id,
      project_id,
      name: BuildConfigurationName::new("main").unwrap(),
      definition: configuration_v1.clone(),
      idempotency_key: key("create-configuration"),
      published_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(replayed_configuration.disposition, MutationDisposition::Replayed);
  assert_eq!(
    replayed_configuration.configuration,
    created_configuration.configuration
  );
  assert_eq!(
    store
      .create_build_configuration(CreateBuildConfiguration {
        id: id(201),
        project_id,
        name: BuildConfigurationName::new("main").unwrap(),
        definition: configuration_v1.clone(),
        idempotency_key: key("duplicate-configuration-name"),
        published_at: time(31),
      })
      .await
      .unwrap_err(),
    conflict(EntityKind::Configuration)
  );

  let repository_v2_number = RepositoryVersion::new(2).unwrap();
  let pipeline_v2 = PipelineVersion::new(2).unwrap();
  let configuration_v2 = build_configuration(
    repository_id,
    repository_v2_number,
    pipeline_id,
    pipeline_v2,
    allowed_pool,
    "release",
  );
  let published_configuration = store
    .publish_build_configuration_version(PublishBuildConfigurationVersion {
      id: configuration_id,
      expected_current_version: BuildConfigurationVersion::INITIAL,
      definition: configuration_v2.clone(),
      idempotency_key: key("publish-configuration-v2"),
      published_at: time(40),
    })
    .await
    .unwrap();
  assert_eq!(published_configuration.configuration.version.get(), 2);
  let replayed_configuration_publication = store
    .publish_build_configuration_version(PublishBuildConfigurationVersion {
      id: configuration_id,
      expected_current_version: BuildConfigurationVersion::INITIAL,
      definition: configuration_v2,
      idempotency_key: key("publish-configuration-v2"),
      published_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(
    replayed_configuration_publication.disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    replayed_configuration_publication.configuration,
    published_configuration.configuration
  );
  assert_eq!(
    store
      .build_configuration_version(configuration_id, BuildConfigurationVersion::INITIAL)
      .await
      .unwrap(),
    created_configuration.configuration,
    "publishing a configuration must not rewrite the snapshot retained by an existing Build"
  );
  assert_eq!(
    store
      .publish_build_configuration_version(PublishBuildConfigurationVersion {
        id: configuration_id,
        expected_current_version: BuildConfigurationVersion::INITIAL,
        definition: configuration_v1,
        idempotency_key: key("rewrite-configuration-v1"),
        published_at: time(50),
      })
      .await
      .unwrap_err(),
    conflict(EntityKind::Configuration)
  );

  let counts = evidence.mutation_evidence_counts().await;
  assert_eq!(counts.idempotency, 4);
  assert_eq!(counts.audit, 4);
  assert_eq!(counts.outbox, 4);
}

async fn assert_missing_configuration_reference<S>(
  store: Arc<S>,
  id: BuildConfigurationId,
  project_id: ProjectId,
  key_value: &str,
  definition: BuildConfigurationDefinition,
  entity: EntityKind,
) where
  S: ConfigurationStore + 'static,
{
  assert_eq!(
    store
      .create_build_configuration(CreateBuildConfiguration {
        id,
        project_id,
        name: BuildConfigurationName::new(key_value).unwrap(),
        definition,
        idempotency_key: key(key_value),
        published_at: time(29),
      })
      .await
      .unwrap_err(),
    StoreError::NotFound { entity }
  );
}

async fn assert_invalid_configuration<S>(
  store: Arc<S>,
  id: BuildConfigurationId,
  project_id: ProjectId,
  key_value: &str,
  definition: BuildConfigurationDefinition,
) where
  S: ConfigurationStore + 'static,
{
  assert_eq!(
    store
      .create_build_configuration(CreateBuildConfiguration {
        id,
        project_id,
        name: BuildConfigurationName::new(key_value).unwrap(),
        definition,
        idempotency_key: key(key_value),
        published_at: time(29),
      })
      .await
      .unwrap_err(),
    StoreError::InvalidInput {
      operation: StoreOperation::CreateBuildConfiguration,
      source: StoreInputError::InvalidBuildConfiguration,
    }
  );
}

/// Runs the configuration contract against the deterministic in-memory adapter.
pub fn verify_in_memory_configuration_store_contract() {
  let store = Arc::new(InMemoryConfigurationStore::new());
  let project_id = id::<ProjectId>(1);
  let pipeline_id = id::<PipelineId>(2);
  let pool_id = id::<PoolId>(3);
  store.seed_project(project_id).unwrap();
  store
    .seed_pipeline_version(project_id, pipeline_id, PipelineVersion::INITIAL)
    .unwrap();
  store
    .seed_pipeline_version(project_id, pipeline_id, PipelineVersion::new(2).unwrap())
    .unwrap();
  store.seed_pool(pool_id).unwrap();
  run_ready(
    verify_configuration_store_contract(Arc::clone(&store), store, project_id, pipeline_id, pool_id),
    "in-memory Configuration store operations must complete without I/O",
  );
}

fn repository_definition(locator: &str) -> RepositoryDefinition {
  RepositoryDefinition {
    vcs_integration_id: id::<IntegrationId>(500),
    repository_locator: locator.to_owned(),
    selection: RepositorySelectionPolicy {
      allowed_references: BTreeSet::from([SourceReference::new("main").unwrap()]),
      default_reference: Some(SourceReference::new("main").unwrap()),
      allow_exact_revision: true,
    },
  }
}

fn build_configuration(
  repository_id: RepositoryId,
  repository_version: RepositoryVersion,
  pipeline_id: PipelineId,
  pipeline_version: PipelineVersion,
  pool_id: PoolId,
  profile: &str,
) -> BuildConfigurationDefinition {
  BuildConfigurationDefinition {
    enabled: true,
    repository_id,
    repository_version,
    pipeline_id,
    pipeline_version,
    parameters: ParameterSchema {
      parameters: BTreeMap::from([(
        "profile".to_owned(),
        ParameterDefinition {
          value_type: ParameterType::String,
          required: false,
          default: Some(json!(profile)),
        },
      )]),
      deny_unknown: true,
    },
    triggers: ConfigurationTriggerPolicy {
      allowed: BTreeSet::from([TriggerKind::Manual]),
    },
    agent_requirements: ConfigurationAgentRequirements {
      capabilities: BTreeSet::from([ExecutionCapability::new("native").unwrap()]),
      labels: BTreeMap::from([("tier".to_owned(), "builder".to_owned())]),
      minimum_cpu_millis: 1_000,
      minimum_memory_bytes: 1024 * 1024,
      minimum_disk_bytes: 1024 * 1024,
    },
    allowed_pools: BTreeSet::from([pool_id]),
    runtime: ConfigurationRuntimePolicy {
      class: RuntimeClass::Native,
      operating_system: "linux".to_owned(),
      architecture: "amd64".to_owned(),
      immutable_image: None,
      cpu_millis: 1_000,
      memory_bytes: 1024 * 1024,
      writable_disk_bytes: 1024 * 1024,
      timeout_seconds: 300,
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
      artifact_bytes: 1024,
      report_count: 1,
      report_bytes: 1024,
      single_output_bytes: 1024,
    },
    retry: ConfigurationRetryPolicy {
      max_attempts: 2,
      retry_on: BTreeSet::from([RetryClass::InfrastructureFailure]),
    },
  }
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

const fn conflict(entity: EntityKind) -> StoreError {
  StoreError::Conflict { entity }
}
