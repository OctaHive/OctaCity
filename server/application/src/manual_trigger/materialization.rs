use std::collections::{BTreeMap, BTreeSet};

use octacity_server_domain::{AttemptId, BuildId, ImmutableRevision, JobId, PipelineNodeId, TriggerOccurrenceId};
use octacity_server_job::JobRequirements;
use octacity_server_store::{
  BuildRetentionDeadlines, MaterializedJob, MaterializedJobPayload, PublishedPipeline, StoreError,
};
use serde_json::Value;
use uuid::Uuid;

use crate::{snapshots::BuildInputSnapshot, snapshots::BuildPolicySnapshot};

use super::{
  job_spec::JobSpecTemplateDeriver,
  model::{ManualTriggerCommand, ManualTriggerContext, ManualTriggerError},
  preparation::PreparedManualTrigger,
};

pub(super) struct ManualBuildIdentities {
  pub(super) build_id: BuildId,
  pub(super) attempt_id: AttemptId,
  jobs: BTreeMap<PipelineNodeId, JobId>,
}

impl ManualBuildIdentities {
  pub(super) fn occurrence_id(command: &ManualTriggerCommand) -> TriggerOccurrenceId {
    Self::occurrence_id_for("manual", command)
  }

  pub(super) fn occurrence_id_for(kind: &str, command: &ManualTriggerCommand) -> TriggerOccurrenceId {
    let occurrence_name = format!(
      "{kind}:{}:{}",
      command.trigger.version.get(),
      command.deduplication_identity
    );
    let occurrence_uuid = Uuid::new_v5(&command.trigger.id.as_uuid(), occurrence_name.as_bytes());
    TriggerOccurrenceId::from_uuid(occurrence_uuid).expect("UUIDv5 is never nil")
  }

  pub(super) fn derive(occurrence_id: TriggerOccurrenceId, pipeline: &PublishedPipeline) -> Self {
    let occurrence_uuid = occurrence_id.as_uuid();
    let build_uuid = Uuid::new_v5(&occurrence_uuid, b"build");
    let attempt_uuid = Uuid::new_v5(&build_uuid, b"attempt:1");
    let attempt_id = AttemptId::from_uuid(attempt_uuid).expect("UUIDv5 is never nil");
    let jobs = pipeline
      .dag
      .nodes()
      .iter()
      .map(|node| {
        let uuid = Uuid::new_v5(&attempt_id.as_uuid(), node.id().as_str().as_bytes());
        (node.id().clone(), JobId::from_uuid(uuid).expect("UUIDv5 is never nil"))
      })
      .collect();
    Self {
      build_id: BuildId::from_uuid(build_uuid).expect("UUIDv5 is never nil"),
      attempt_id,
      jobs,
    }
  }
}

pub(super) fn materialize_jobs(
  context: &ManualTriggerContext,
  identities: &ManualBuildIdentities,
  prepared: &PreparedManualTrigger,
  immutable_revision: &ImmutableRevision,
) -> Result<Vec<MaterializedJob>, ManualTriggerError> {
  let definition = &context.configuration.definition;
  let job_specs = JobSpecTemplateDeriver::new(context, identities.build_id, prepared, immutable_revision)?;
  let mut dependencies_by_node = BTreeMap::<&PipelineNodeId, Vec<JobId>>::new();
  for edge in context.pipeline.dag.edges() {
    dependencies_by_node
      .entry(edge.dependent())
      .or_default()
      .push(identities.jobs[edge.predecessor()]);
  }
  context
    .pipeline
    .dag
    .nodes()
    .iter()
    .map(|node| {
      let job_id = identities.jobs[node.id()];
      let dependencies = dependencies_by_node.get(node.id()).cloned().unwrap_or_default();
      let mut capabilities: BTreeSet<_> = definition.agent_requirements.capabilities.iter().cloned().collect();
      capabilities.extend(node.required_capabilities().iter().cloned());
      let requirements = JobRequirements {
        capabilities,
        labels: definition.agent_requirements.labels.clone(),
        minimum_cpu_millis: definition
          .agent_requirements
          .minimum_cpu_millis
          .max(definition.runtime.cpu_millis),
        minimum_memory_bytes: definition
          .agent_requirements
          .minimum_memory_bytes
          .max(definition.runtime.memory_bytes),
        minimum_disk_bytes: definition
          .agent_requirements
          .minimum_disk_bytes
          .max(definition.runtime.writable_disk_bytes),
        runtime_class: definition.runtime.class,
        operating_system: definition.runtime.operating_system,
        architecture: definition.runtime.architecture,
      };
      MaterializedJob::new(
        job_id,
        node.id().clone(),
        dependencies,
        node.dependency_policy(),
        prepared.allowed_pools.clone(),
        MaterializedJobPayload::new(requirements, job_specs.derive(node)?)
          .map_err(ManualTriggerError::Materialization)?,
      )
      .map_err(ManualTriggerError::Materialization)
    })
    .collect()
}

pub(super) fn input_snapshot(prepared: &PreparedManualTrigger) -> Result<Value, ManualTriggerError> {
  encode_snapshot(&BuildInputSnapshot {
    parameters: prepared.parameters.clone(),
    source: prepared.source.clone(),
  })
}

pub(super) fn effective_policy_snapshot(context: &ManualTriggerContext) -> Result<Value, ManualTriggerError> {
  encode_snapshot(&BuildPolicySnapshot {
    project: context.effective_policy.clone(),
    job_spec_toolchain: context.job_spec_toolchain.clone(),
  })
}

pub(super) fn retention_deadlines(
  context: &ManualTriggerContext,
  accepted_at: octacity_server_domain::Timestamp,
) -> Result<BuildRetentionDeadlines, ManualTriggerError> {
  let policy = context.effective_policy.policy.retention;
  Ok(BuildRetentionDeadlines {
    metadata: retention_deadline(accepted_at, policy.build_seconds)?,
    logs: retention_deadline(accepted_at, policy.log_seconds)?,
    artifacts: retention_deadline(accepted_at, policy.artifact_seconds)?,
    reports: retention_deadline(accepted_at, policy.artifact_seconds)?,
  })
}

fn retention_deadline(
  accepted_at: octacity_server_domain::Timestamp,
  seconds: u64,
) -> Result<octacity_server_domain::Timestamp, ManualTriggerError> {
  let milliseconds = i64::try_from(seconds)
    .ok()
    .and_then(|seconds| seconds.checked_mul(1_000))
    .and_then(|duration| accepted_at.unix_millis().checked_add(duration))
    .ok_or(ManualTriggerError::Invalid(
      super::model::ManualTriggerInputError::RetentionDeadlineOutOfRange,
    ))?;
  octacity_server_domain::Timestamp::from_unix_millis(milliseconds)
    .map_err(|_| ManualTriggerError::Invalid(super::model::ManualTriggerInputError::RetentionDeadlineOutOfRange))
}

pub(super) fn encode_snapshot(value: &impl serde::Serialize) -> Result<Value, ManualTriggerError> {
  let value = serde_json::to_value(value).map_err(|_| ManualTriggerError::SnapshotEncoding)?;
  value
    .is_object()
    .then_some(value)
    .ok_or(ManualTriggerError::SnapshotEncoding)
}

pub(super) fn classify_error(error: StoreError) -> ManualTriggerError {
  match error {
    StoreError::InvalidInput { source, .. } => ManualTriggerError::Materialization(source),
    other => ManualTriggerError::Store(other),
  }
}
