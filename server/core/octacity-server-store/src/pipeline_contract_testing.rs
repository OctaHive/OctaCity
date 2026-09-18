use std::sync::Arc;

use octacity_server_domain::{
  EntityKind, JobName, PipelineId, PipelineName, PipelineNodeId, PipelineVersion, ProjectId,
};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineEdge, PipelineNode, PublishablePipelineDag,
};
use serde_json::json;

use crate::test_support::{id, run_ready, time};
use crate::testing::{InMemoryPipelineStore, MutationEvidenceProbe};
use crate::{CreatePipeline, IdempotencyKey, MutationDisposition, PipelineStore, PublishPipelineVersion, StoreError};

/// Runs the reusable immutable Pipeline contract against one prepared adapter.
pub async fn verify_pipeline_store_contract<S, P>(store: Arc<S>, evidence: Arc<P>, project_id: ProjectId)
where
  S: PipelineStore + 'static,
  P: MutationEvidenceProbe + 'static,
{
  let pipeline_id = id::<PipelineId>(10);
  let initial_dag = dag("build");
  let created = store
    .create_pipeline(CreatePipeline {
      id: pipeline_id,
      project_id,
      name: PipelineName::new("main").unwrap(),
      dag: initial_dag.clone(),
      idempotency_key: key("create-main"),
      published_at: time(10),
    })
    .await
    .unwrap();
  assert_eq!(created.disposition, MutationDisposition::Applied);
  assert_eq!(created.pipeline.version, PipelineVersion::INITIAL);

  let replay = store
    .create_pipeline(CreatePipeline {
      id: pipeline_id,
      project_id,
      name: PipelineName::new("main").unwrap(),
      dag: initial_dag.clone(),
      idempotency_key: key("create-main"),
      published_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.pipeline, created.pipeline);

  assert_eq!(
    store
      .create_pipeline(CreatePipeline {
        id: id(11),
        project_id,
        name: PipelineName::new("other").unwrap(),
        dag: initial_dag.clone(),
        idempotency_key: key("create-main"),
        published_at: time(11),
      })
      .await
      .unwrap_err(),
    conflict()
  );
  assert_eq!(
    store
      .create_pipeline(CreatePipeline {
        id: id(11),
        project_id,
        name: PipelineName::new("main").unwrap(),
        dag: initial_dag.clone(),
        idempotency_key: key("duplicate-name"),
        published_at: time(11),
      })
      .await
      .unwrap_err(),
    conflict()
  );

  let second_dag = dag("package");
  let published = store
    .publish_pipeline_version(PublishPipelineVersion {
      id: pipeline_id,
      expected_current_version: PipelineVersion::INITIAL,
      dag: second_dag.clone(),
      idempotency_key: key("publish-v2"),
      published_at: time(20),
    })
    .await
    .unwrap();
  assert_eq!(published.disposition, MutationDisposition::Applied);
  assert_eq!(published.pipeline.version.get(), 2);

  let replay = store
    .publish_pipeline_version(PublishPipelineVersion {
      id: pipeline_id,
      expected_current_version: PipelineVersion::INITIAL,
      dag: second_dag,
      idempotency_key: key("publish-v2"),
      published_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.pipeline, published.pipeline);
  assert_eq!(
    store
      .pipeline_version(pipeline_id, PipelineVersion::INITIAL)
      .await
      .unwrap(),
    created.pipeline,
    "publishing a new version must not mutate the prior snapshot"
  );

  assert_eq!(
    store
      .publish_pipeline_version(PublishPipelineVersion {
        id: pipeline_id,
        expected_current_version: PipelineVersion::INITIAL,
        dag: dag("changed-v1"),
        idempotency_key: key("attempt-mutation"),
        published_at: time(30),
      })
      .await
      .unwrap_err(),
    conflict(),
    "a caller cannot replace an already published version"
  );
  assert_eq!(
    store
      .pipeline_version(pipeline_id, PipelineVersion::new(3).unwrap())
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Pipeline
    }
  );
  assert_eq!(
    store
      .create_pipeline(CreatePipeline {
        id: id(99),
        project_id: id(999),
        name: PipelineName::new("orphan").unwrap(),
        dag: initial_dag,
        idempotency_key: key("missing-project"),
        published_at: time(40),
      })
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );

  let counts = evidence.mutation_evidence_counts().await;
  assert_eq!(counts.idempotency, 2);
  assert_eq!(counts.audit, 2);
  assert_eq!(counts.outbox, 2);
}

/// Runs the Pipeline contract against the deterministic in-memory adapter.
pub fn verify_in_memory_pipeline_store_contract() {
  let store = Arc::new(InMemoryPipelineStore::new());
  let project_id = id::<ProjectId>(1);
  store.seed_project(project_id).unwrap();
  run_ready(
    verify_pipeline_store_contract(Arc::clone(&store), store, project_id),
    "in-memory Pipeline store operations must complete without I/O",
  );
}

fn dag(node_id: &str) -> PublishablePipelineDag {
  let native = ExecutionCapability::new("native").unwrap();
  let root = PipelineNode::new(
    PipelineNodeId::new(node_id).unwrap(),
    JobName::new(node_id).unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![native.clone()],
    json!({"command": node_id}),
  )
  .unwrap();
  let verify = PipelineNode::new(
    PipelineNodeId::new("verify").unwrap(),
    JobName::new("verify").unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![native.clone()],
    json!({"command": "verify"}),
  )
  .unwrap();
  PublishablePipelineDag::new(
    vec![verify, root],
    vec![PipelineEdge::new(
      PipelineNodeId::new(node_id).unwrap(),
      PipelineNodeId::new("verify").unwrap(),
    )],
    &CapabilityCatalog::new([native]),
  )
  .unwrap()
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Pipeline,
  }
}
