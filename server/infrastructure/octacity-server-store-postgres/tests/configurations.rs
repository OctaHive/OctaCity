mod support;

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, BuildId, IntegrationId, JobName, PipelineId,
  PipelineName, PipelineNodeId, PipelineVersion, PoolId, ProjectId, RepositoryId, RepositoryName, RepositoryVersion,
  Timestamp, TriggerId, TriggerOccurrenceId,
};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineDag, PipelineNode, PublishablePipelineDag,
};
use octacity_server_store::{
  ArtifactPolicy, BuildConfigurationDefinition, ConfigurationAgentRequirements, ConfigurationCachePolicy,
  ConfigurationNetworkPolicy, ConfigurationRetryPolicy, ConfigurationRuntimePolicy, ConfigurationStore as _,
  ConfigurationTriggerPolicy, CreateBuildConfiguration, CreatePipeline, CreateRepository, IdempotencyKey,
  ParameterDefinition, ParameterSchema, ParameterType, PipelineStore as _, PublishBuildConfigurationVersion,
  PublishPipelineVersion, RepositoryDefinition, RepositorySelectionPolicy, RetryClass, RuntimeClass, SourceReference,
  StoreError, TriggerKind,
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::{Value, json};
use support::TestDatabase;
use tokio::sync::Barrier;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_publication_preserves_build_configuration_and_pipeline_snapshots() {
  let database = TestDatabase::migrated().await;
  let project_id = id::<ProjectId>(1);
  let pool_id = id::<PoolId>(2);
  seed_project_and_pool(&database.pool, project_id, pool_id).await;
  let store = PostgresStore::new(database.pool.clone());
  let repository_id = id::<RepositoryId>(3);
  store
    .create_repository(CreateRepository {
      id: repository_id,
      project_id,
      name: RepositoryName::new("source").unwrap(),
      definition: repository_definition(),
      idempotency_key: key("create-repository"),
      published_at: time(10),
    })
    .await
    .unwrap();
  let pipeline_id = id::<PipelineId>(4);
  let pipeline_v1 = dag("build-v1");
  store
    .create_pipeline(CreatePipeline {
      id: pipeline_id,
      project_id,
      name: PipelineName::new("main").unwrap(),
      dag: pipeline_v1.clone(),
      idempotency_key: key("create-pipeline"),
      published_at: time(10),
    })
    .await
    .unwrap();
  let configuration_id = id::<BuildConfigurationId>(5);
  let configuration_v1 = configuration(repository_id, pipeline_id, PipelineVersion::INITIAL, pool_id, "debug");
  store
    .create_build_configuration(CreateBuildConfiguration {
      id: configuration_id,
      project_id,
      name: BuildConfigurationName::new("main").unwrap(),
      definition: configuration_v1.clone(),
      idempotency_key: key("create-configuration"),
      published_at: time(20),
    })
    .await
    .unwrap();
  let build_id = id::<BuildId>(6);
  seed_build(
    &database.pool,
    build_id,
    project_id,
    configuration_id,
    pipeline_id,
    repository_id,
  )
  .await;

  store
    .publish_pipeline_version(PublishPipelineVersion {
      id: pipeline_id,
      expected_current_version: PipelineVersion::INITIAL,
      dag: dag("build-v2"),
      idempotency_key: key("publish-pipeline-v2"),
      published_at: time(30),
    })
    .await
    .unwrap();
  let barrier = Arc::new(Barrier::new(2));
  let left = PostgresStore::new(independent_pool(&database.pool).await);
  let right = PostgresStore::new(independent_pool(&database.pool).await);
  let left_task = tokio::spawn(publish_after_barrier(
    left,
    publication(
      configuration_id,
      repository_id,
      pipeline_id,
      pool_id,
      "release",
      "publish-left",
    ),
    Arc::clone(&barrier),
  ));
  let right_task = tokio::spawn(publish_after_barrier(
    right,
    publication(
      configuration_id,
      repository_id,
      pipeline_id,
      pool_id,
      "audit",
      "publish-right",
    ),
    barrier,
  ));
  let outcomes = [left_task.await.unwrap(), right_task.await.unwrap()];
  assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
  assert_eq!(outcomes.iter().filter(|outcome| outcome.is_err()).count(), 1);

  let (stored_configuration, stored_pipeline): (Value, Value) = sqlx::query_as(
    "SELECT configuration.configuration_snapshot, pipeline.dag_snapshot \
     FROM builds AS build \
     JOIN build_configuration_versions AS configuration \
       ON configuration.build_configuration_id = build.build_configuration_id \
      AND configuration.version = build.build_configuration_version \
     JOIN pipeline_versions AS pipeline \
       ON pipeline.pipeline_id = build.pipeline_id AND pipeline.version = build.pipeline_version \
     WHERE build.id = $1",
  )
  .bind(build_id.as_uuid())
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    serde_json::from_value::<BuildConfigurationDefinition>(stored_configuration).unwrap(),
    configuration_v1
  );
  assert_eq!(
    serde_json::from_value::<PipelineDag>(stored_pipeline).unwrap(),
    pipeline_v1.as_dag().clone()
  );

  database.cleanup().await;
}

async fn publish_after_barrier(
  store: PostgresStore,
  request: PublishBuildConfigurationVersion,
  barrier: Arc<Barrier>,
) -> Result<octacity_server_store::BuildConfigurationMutationOutcome, StoreError> {
  barrier.wait().await;
  store.publish_build_configuration_version(request).await
}

fn publication(
  id: BuildConfigurationId,
  repository_id: RepositoryId,
  pipeline_id: PipelineId,
  pool_id: PoolId,
  profile: &str,
  idempotency_key: &str,
) -> PublishBuildConfigurationVersion {
  PublishBuildConfigurationVersion {
    id,
    expected_current_version: BuildConfigurationVersion::INITIAL,
    definition: configuration(
      repository_id,
      pipeline_id,
      PipelineVersion::new(2).unwrap(),
      pool_id,
      profile,
    ),
    idempotency_key: key(idempotency_key),
    published_at: time(40),
  }
}

async fn seed_project_and_pool(pool: &sqlx::PgPool, project_id: ProjectId, pool_id: PoolId) {
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'configuration-race', 1, now(), now())",
  )
  .bind(project_id.as_uuid())
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO pools (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
     VALUES ($1, 1, 'configuration-pool', true, 'accepting', '{}', 1, now())",
  )
  .bind(pool_id.as_uuid())
  .execute(pool)
  .await
  .unwrap();
}

async fn seed_build(
  pool: &sqlx::PgPool,
  build_id: BuildId,
  project_id: ProjectId,
  configuration_id: BuildConfigurationId,
  pipeline_id: PipelineId,
  repository_id: RepositoryId,
) {
  let trigger_id = id::<TriggerId>(7);
  let occurrence_id = id::<TriggerOccurrenceId>(8);
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, 1, $2, 1, 'manual', true, '{}', now())",
  )
  .bind(trigger_id.as_uuid())
  .bind(configuration_id.as_uuid())
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
        deduplication_identity, cause, causality, provider_metadata, source_time, state, request_digest, \
        created_at, updated_at) \
     VALUES ($1, $2, 1, $3, 1, 'manual', 'snapshot-build', '{\"kind\":\"manual\"}', \
             jsonb_build_object('root_occurrence_id', $1, 'parent_occurrence_id', NULL, 'depth', 0), '{}', now(), \
             'accepted', decode(repeat('aa', 32), 'hex'), now(), now())",
  )
  .bind(occurrence_id.as_uuid())
  .bind(trigger_id.as_uuid())
  .bind(configuration_id.as_uuid())
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, priority, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, 1, $4, 1, $5, 1, $6, '0123456789abcdef', '{}', '{}', 0, 'running', 1, now(), now())",
  )
  .bind(build_id.as_uuid())
  .bind(project_id.as_uuid())
  .bind(configuration_id.as_uuid())
  .bind(pipeline_id.as_uuid())
  .bind(repository_id.as_uuid())
  .bind(occurrence_id.as_uuid())
  .execute(pool)
  .await
  .unwrap();
}

fn repository_definition() -> RepositoryDefinition {
  RepositoryDefinition {
    vcs_integration_id: id::<IntegrationId>(9),
    repository_locator: octacity_server_domain::RepositoryLocator::new("octahive/octacity").unwrap(),
    selection: RepositorySelectionPolicy {
      allowed_references: BTreeSet::from([SourceReference::new("main").unwrap()]),
      default_reference: Some(SourceReference::new("main").unwrap()),
      allow_exact_revision: true,
    },
  }
}

fn configuration(
  repository_id: RepositoryId,
  pipeline_id: PipelineId,
  pipeline_version: PipelineVersion,
  pool_id: PoolId,
  profile: &str,
) -> BuildConfigurationDefinition {
  BuildConfigurationDefinition {
    enabled: true,
    repository_id,
    repository_version: RepositoryVersion::INITIAL,
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
      labels: BTreeMap::new(),
      minimum_cpu_millis: 1_000,
      minimum_memory_bytes: 1,
      minimum_disk_bytes: 1,
    },
    allowed_pools: BTreeSet::from([pool_id]),
    runtime: ConfigurationRuntimePolicy {
      class: RuntimeClass::Native,
      operating_system: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
      immutable_image: None,
      cpu_millis: 1_000,
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
      report_count: 0,
      report_bytes: 0,
      single_output_bytes: 1,
    },
    retry: ConfigurationRetryPolicy {
      max_attempts: 2,
      retry_on: BTreeSet::from([RetryClass::InfrastructureFailure]),
    },
  }
}

fn dag(node: &str) -> PublishablePipelineDag {
  let native = ExecutionCapability::new("native").unwrap();
  PublishablePipelineDag::new(
    vec![
      PipelineNode::new(
        PipelineNodeId::new(node).unwrap(),
        JobName::new(node).unwrap(),
        DependencyPolicy::AllSucceeded,
        vec![native.clone()],
        json!({"command": node}),
      )
      .unwrap(),
    ],
    vec![],
    &CapabilityCatalog::new([native]),
  )
  .unwrap()
}

async fn independent_pool(source: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::PgPool::connect_with((*source.connect_options()).clone())
    .await
    .expect("connect an independent server pool to the test database")
}

fn id<T>(value: u64) -> T
where
  T: std::str::FromStr,
  T::Err: std::fmt::Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
