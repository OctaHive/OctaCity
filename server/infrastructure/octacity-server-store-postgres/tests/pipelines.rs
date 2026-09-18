mod support;

use std::sync::Arc;

use octacity_server_domain::{
  EntityKind, JobName, PipelineId, PipelineName, PipelineNodeId, PipelineVersion, ProjectId, Timestamp,
};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineNode, PublishablePipelineDag,
};
use octacity_server_store::{CreatePipeline, IdempotencyKey, PipelineStore as _, PublishPipelineVersion, StoreError};
use octacity_server_store_postgres::PostgresStore;
use serde_json::json;
use support::TestDatabase;
use tokio::sync::Barrier;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_publications_append_one_next_version() {
  let database = TestDatabase::migrated().await;
  let project_id = ProjectId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'pipeline-race', 1, now(), now())",
  )
  .bind(project_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let pipeline_id = PipelineId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
  store
    .create_pipeline(CreatePipeline {
      id: pipeline_id,
      project_id,
      name: PipelineName::new("main").unwrap(),
      dag: dag("initial"),
      idempotency_key: key("create"),
      published_at: time(10),
    })
    .await
    .unwrap();

  let left = PostgresStore::new(independent_pool(&database.pool).await);
  let right = PostgresStore::new(independent_pool(&database.pool).await);
  let barrier = Arc::new(Barrier::new(2));
  let left_task = tokio::spawn(publish_after_barrier(
    left,
    publish(pipeline_id, "left", "publish-left"),
    barrier.clone(),
  ));
  let right_task = tokio::spawn(publish_after_barrier(
    right,
    publish(pipeline_id, "right", "publish-right"),
    barrier,
  ));
  let outcomes = [left_task.await.unwrap(), right_task.await.unwrap()];
  assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
  assert_eq!(
    outcomes
      .iter()
      .filter(|outcome| matches!(outcome, Err(error) if *error == conflict()))
      .count(),
    1
  );
  let winner = outcomes.into_iter().find_map(Result::ok).unwrap();
  assert_eq!(
    store
      .pipeline_version(pipeline_id, PipelineVersion::new(2).unwrap())
      .await
      .unwrap(),
    winner.pipeline
  );

  database.cleanup().await;
}

async fn publish_after_barrier(
  store: PostgresStore,
  request: PublishPipelineVersion,
  barrier: Arc<Barrier>,
) -> Result<octacity_server_store::PipelineMutationOutcome, StoreError> {
  barrier.wait().await;
  store.publish_pipeline_version(request).await
}

fn publish(id: PipelineId, node: &str, idempotency_key: &str) -> PublishPipelineVersion {
  PublishPipelineVersion {
    id,
    expected_current_version: PipelineVersion::INITIAL,
    dag: dag(node),
    idempotency_key: key(idempotency_key),
    published_at: time(20),
  }
}

fn dag(node_id: &str) -> PublishablePipelineDag {
  let native = ExecutionCapability::new("native").unwrap();
  PublishablePipelineDag::new(
    vec![
      PipelineNode::new(
        PipelineNodeId::new(node_id).unwrap(),
        JobName::new(node_id).unwrap(),
        DependencyPolicy::AllSucceeded,
        vec![native.clone()],
        json!({"command": node_id}),
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

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Pipeline,
  }
}
