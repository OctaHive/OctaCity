use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, PipelineId, PipelineName, PipelineVersion, ProjectId};

use crate::testing::{MutationEvidenceCounts, MutationEvidenceProbe};
use crate::{
  CreatePipeline, MutationDisposition, PipelineMutationOutcome, PipelineStore, PublishPipelineVersion,
  PublishedPipeline, StoreError, StoreInputError, StoreOperation,
};

/// Deterministic process-local Pipeline-store adapter for application tests.
#[derive(Default)]
pub struct InMemoryPipelineStore {
  state: Mutex<PipelineMemoryState>,
}

impl InMemoryPipelineStore {
  /// Creates an empty Pipeline store.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one Project that may own Pipeline resources.
  pub fn seed_project(&self, project_id: ProjectId) -> Result<(), StoreError> {
    self.lock()?.projects.insert(project_id);
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, PipelineMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Default)]
struct PipelineMemoryState {
  projects: BTreeSet<ProjectId>,
  pipelines: BTreeMap<PipelineId, PipelineMetadata>,
  names: BTreeMap<(ProjectId, PipelineName), PipelineId>,
  versions: BTreeMap<(PipelineId, PipelineVersion), PublishedPipeline>,
  mutations: BTreeMap<(&'static str, String), StoredMutation>,
  evidence: BTreeSet<String>,
}

#[derive(Clone)]
struct PipelineMetadata {
  project_id: ProjectId,
  name: PipelineName,
  current_version: PipelineVersion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MutationFingerprint {
  Create {
    id: PipelineId,
    project_id: ProjectId,
    name: PipelineName,
    dag: octacity_server_pipeline::PublishablePipelineDag,
  },
  Publish {
    id: PipelineId,
    expected_current_version: PipelineVersion,
    dag: octacity_server_pipeline::PublishablePipelineDag,
  },
}

#[derive(Clone)]
struct StoredMutation {
  fingerprint: MutationFingerprint,
  pipeline: PublishedPipeline,
}

#[async_trait]
impl MutationEvidenceProbe for InMemoryPipelineStore {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let state = self.lock().expect("in-memory Pipeline store must remain available");
    let count = state.evidence.len();
    MutationEvidenceCounts {
      idempotency: count,
      audit: count,
      outbox: count,
    }
  }
}

#[async_trait]
impl PipelineStore for InMemoryPipelineStore {
  async fn create_pipeline(&self, request: CreatePipeline) -> Result<PipelineMutationOutcome, StoreError> {
    validate_dag(&request.dag, StoreOperation::CreatePipeline)?;
    let scope = "create-pipeline";
    let fingerprint = MutationFingerprint::Create {
      id: request.id,
      project_id: request.project_id,
      name: request.name.clone(),
      dag: request.dag.clone(),
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    if !state.projects.contains(&request.project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    if state.pipelines.contains_key(&request.id)
      || state.names.contains_key(&(request.project_id, request.name.clone()))
    {
      return Err(conflict());
    }
    let pipeline = PublishedPipeline {
      id: request.id,
      project_id: request.project_id,
      name: request.name.clone(),
      version: PipelineVersion::INITIAL,
      dag: request.dag.into_dag(),
      published_at: request.published_at,
    };
    state.pipelines.insert(
      pipeline.id,
      PipelineMetadata {
        project_id: pipeline.project_id,
        name: pipeline.name.clone(),
        current_version: pipeline.version,
      },
    );
    state
      .names
      .insert((pipeline.project_id, pipeline.name.clone()), pipeline.id);
    state.versions.insert((pipeline.id, pipeline.version), pipeline.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      &pipeline,
    );
    Ok(PipelineMutationOutcome {
      disposition: MutationDisposition::Applied,
      pipeline,
    })
  }

  async fn publish_pipeline_version(
    &self,
    request: PublishPipelineVersion,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    validate_dag(&request.dag, StoreOperation::PublishPipelineVersion)?;
    let scope = "publish-pipeline-version";
    let fingerprint = MutationFingerprint::Publish {
      id: request.id,
      expected_current_version: request.expected_current_version,
      dag: request.dag.clone(),
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let metadata = state.pipelines.get(&request.id).cloned().ok_or(StoreError::NotFound {
      entity: EntityKind::Pipeline,
    })?;
    if metadata.current_version != request.expected_current_version {
      return Err(conflict());
    }
    let current = state
      .versions
      .get(&(request.id, metadata.current_version))
      .ok_or(StoreError::Unavailable)?;
    if request.published_at < current.published_at {
      return Err(conflict());
    }
    let version = request
      .expected_current_version
      .next()
      .map_err(|_| StoreError::Unavailable)?;
    let pipeline = PublishedPipeline {
      id: request.id,
      project_id: metadata.project_id,
      name: metadata.name,
      version,
      dag: request.dag.into_dag(),
      published_at: request.published_at,
    };
    state
      .pipelines
      .get_mut(&request.id)
      .expect("Pipeline was read under the same in-memory transaction")
      .current_version = version;
    state.versions.insert((pipeline.id, pipeline.version), pipeline.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      &pipeline,
    );
    Ok(PipelineMutationOutcome {
      disposition: MutationDisposition::Applied,
      pipeline,
    })
  }

  async fn pipeline_version(
    &self,
    pipeline_id: PipelineId,
    version: PipelineVersion,
  ) -> Result<PublishedPipeline, StoreError> {
    self
      .lock()?
      .versions
      .get(&(pipeline_id, version))
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Pipeline,
      })
  }
}

fn validate_dag(
  dag: &octacity_server_pipeline::PublishablePipelineDag,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  dag.validate().map_err(|_| StoreError::InvalidInput {
    operation,
    source: StoreInputError::InvalidPipelineSnapshot,
  })
}

fn replay(
  state: &PipelineMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &MutationFingerprint,
) -> Result<Option<PipelineMutationOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(conflict());
  }
  Ok(Some(PipelineMutationOutcome {
    disposition: MutationDisposition::Replayed,
    pipeline: stored.pipeline.clone(),
  }))
}

fn record(
  state: &mut PipelineMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: MutationFingerprint,
  pipeline: &PublishedPipeline,
) {
  state.mutations.insert(
    (scope, key.to_owned()),
    StoredMutation {
      fingerprint,
      pipeline: pipeline.clone(),
    },
  );
  state.evidence.insert(format!("{scope}:{key}"));
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Pipeline,
  }
}
