use std::collections::BTreeMap;

use octacity_server_domain::{
  AttemptId, AttemptNumber, AttemptVersion, BuildConfigurationId, BuildConfigurationVersion, BuildId, BuildVersion,
  ImmutableRevision, PipelineId, PipelineVersion, ProjectId, RepositoryId, RepositoryVersion, Timestamp,
};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::ImmutableBuildInput;
use serde::{Deserialize, Serialize, de::IgnoredAny};
use serde_json::Value;

use crate::{EffectiveProjectPolicy, ManualSourceSelection, TriggerHistoryProjection, snapshots::BuildInputSnapshot};

use super::ProjectionError;

/// One primitive Build parameter safe for application reads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ParameterValueProjection {
  /// UTF-8 string value.
  String(String),
  /// Signed integral value.
  Integer(i64),
  /// Unsigned integral value outside the signed range.
  UnsignedInteger(u64),
  /// Boolean value.
  Boolean(bool),
}

impl TryFrom<Value> for ParameterValueProjection {
  type Error = ProjectionError;

  fn try_from(value: Value) -> Result<Self, Self::Error> {
    match value {
      Value::String(value) => Ok(Self::String(value)),
      Value::Bool(value) => Ok(Self::Boolean(value)),
      Value::Number(value) => value
        .as_i64()
        .map(Self::Integer)
        .or_else(|| value.as_u64().map(Self::UnsignedInteger))
        .ok_or(ProjectionError::InvalidBuildSnapshot),
      Value::Null | Value::Array(_) | Value::Object(_) => Err(ProjectionError::InvalidBuildSnapshot),
    }
  }
}

/// Safe application projection of one immutable Build and its current aggregate state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildProjection {
  /// Stable Build identity.
  pub id: BuildId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact Build Configuration identity and version.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Exact Pipeline identity.
  pub pipeline_id: PipelineId,
  /// Exact immutable Pipeline version.
  pub pipeline_version: PipelineVersion,
  /// Exact Repository identity.
  pub repository_id: RepositoryId,
  /// Exact immutable Repository version.
  pub repository_version: RepositoryVersion,
  /// Immutable source revision selected before Build acceptance.
  pub immutable_revision: ImmutableRevision,
  /// Resolved primitive Build parameters.
  pub parameters: BTreeMap<String, ParameterValueProjection>,
  /// Source expression retained for diagnosis.
  pub source: ManualSourceSelection,
  /// Exact effective policy and contributing Project policy versions.
  pub effective_policy: EffectiveProjectPolicy,
  /// Durable ready-queue priority.
  pub priority: i64,
  /// Current aggregate state.
  pub state: BuildState,
  /// Current optimistic state version.
  pub version: BuildVersion,
  /// Trigger occurrence and causal lineage that initiated the Build.
  pub trigger: TriggerHistoryProjection,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
}

impl BuildProjection {
  /// Decodes strict server-owned snapshots and omits internal toolchain policy.
  pub fn from_authoritative(
    build: &ImmutableBuildInput,
    state: BuildState,
    version: BuildVersion,
    trigger: TriggerHistoryProjection,
    created_at: Timestamp,
    updated_at: Timestamp,
  ) -> Result<Self, ProjectionError> {
    if trigger.build_id != Some(build.id)
      || trigger.target.configuration_id != build.configuration_id
      || trigger.target.configuration_version != build.configuration_version
    {
      return Err(ProjectionError::InvalidBuildSnapshot);
    }
    let input: BuildInputSnapshot =
      serde_json::from_value(build.input_snapshot.clone()).map_err(|_| ProjectionError::InvalidBuildSnapshot)?;
    let policy: BuildPolicyProjectionSnapshot = serde_json::from_value(build.effective_policy_snapshot.clone())
      .map_err(|_| ProjectionError::InvalidBuildSnapshot)?;
    let parameters = input
      .parameters
      .into_iter()
      .map(|(name, value)| ParameterValueProjection::try_from(value).map(|value| (name, value)))
      .collect::<Result<_, _>>()?;
    Ok(Self {
      id: build.id,
      project_id: build.project_id,
      configuration_id: build.configuration_id,
      configuration_version: build.configuration_version,
      pipeline_id: build.pipeline_id,
      pipeline_version: build.pipeline_version,
      repository_id: build.repository_id,
      repository_version: build.repository_version,
      immutable_revision: build.immutable_revision.clone(),
      parameters,
      source: input.source,
      effective_policy: policy.project,
      priority: build.priority,
      state,
      version,
      trigger,
      created_at,
      updated_at,
    })
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildPolicyProjectionSnapshot {
  project: EffectiveProjectPolicy,
  #[serde(rename = "job_spec_toolchain")]
  _job_spec_toolchain: IgnoredAny,
}

/// Safe application projection of one materialized Build Attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AttemptProjection {
  /// Stable Attempt identity.
  pub id: AttemptId,
  /// Owning Build.
  pub build_id: BuildId,
  /// Positive monotonic Attempt number within the Build.
  pub number: AttemptNumber,
  /// Prior failed Attempt retried by this Attempt, when present.
  pub retry_of_attempt_id: Option<AttemptId>,
  /// Current aggregate state.
  pub state: AttemptState,
  /// Current optimistic state version.
  pub version: AttemptVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
}
