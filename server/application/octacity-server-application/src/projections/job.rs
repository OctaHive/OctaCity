use std::collections::{BTreeMap, BTreeSet};

use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::{
  AgentId, ArtifactId, ArtifactName, AttemptId, JobId, JobVersion, PipelineNodeId, PoolId, RuntimeClass, Timestamp,
};
use octacity_server_job::{JobFailureClass, JobRequirements, JobState};
use octacity_server_pipeline::{DependencyPolicy, ExecutionCapability};
use octacity_server_store::MaterializedJob;
use serde::{Deserialize, Serialize};

use super::ProjectionError;

/// Backend-neutral placement inputs safe for management reads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobPlacementProjection {
  /// Required execution capabilities.
  pub capabilities: BTreeSet<ExecutionCapability>,
  /// Exact normalized Agent inventory labels.
  pub labels: BTreeMap<String, String>,
  /// Minimum CPU capacity in thousandths of one logical CPU.
  pub minimum_cpu_millis: u32,
  /// Minimum available memory in bytes.
  pub minimum_memory_bytes: u64,
  /// Minimum available workspace bytes.
  pub minimum_disk_bytes: u64,
  /// Required runtime and isolation class.
  pub runtime_class: RuntimeClass,
  /// Required operating system.
  pub operating_system: PlatformOs,
  /// Required CPU architecture.
  pub architecture: PlatformArchitecture,
}

impl From<JobRequirements> for JobPlacementProjection {
  fn from(value: JobRequirements) -> Self {
    Self {
      capabilities: value.capabilities,
      labels: value.labels,
      minimum_cpu_millis: value.minimum_cpu_millis,
      minimum_memory_bytes: value.minimum_memory_bytes,
      minimum_disk_bytes: value.minimum_disk_bytes,
      runtime_class: value.runtime_class,
      operating_system: value.operating_system,
      architecture: value.architecture,
    }
  }
}

/// Durable queue-order inputs for a ready Job.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct JobQueueProjection {
  /// Explicit Build priority.
  pub priority: i64,
  /// Time at which the Job most recently became ready.
  pub enqueued_at: Timestamp,
}

/// Non-secret assignment selected by placement and retained as history.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct JobAssignmentProjection {
  /// Pool selected for the execution Lease.
  pub selected_pool_id: PoolId,
  /// Agent assigned to the execution Lease.
  pub assigned_agent_id: AgentId,
}

/// Stable failure classification safe for retry and diagnostic reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobFailureClassification {
  /// Repository-controlled execution failed.
  Execution,
  /// Agent, runtime, transport, or storage infrastructure failed.
  Infrastructure,
  /// Durable cancellation ended execution.
  Cancelled,
  /// A predecessor outcome caused dependency-policy propagation.
  DependencyPolicy,
}

impl From<JobFailureClass> for JobFailureClassification {
  fn from(value: JobFailureClass) -> Self {
    match value {
      JobFailureClass::Execution => Self::Execution,
      JobFailureClass::Infrastructure => Self::Infrastructure,
    }
  }
}

/// Terminal Job outcome without protocol diagnostics or credentials.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct JobTerminalOutcomeProjection {
  /// Terminal Job state.
  pub(super) state: JobState,
  /// Classified reason when the terminal state is not successful.
  pub(super) failure: Option<JobFailureClassification>,
  /// Authoritative completion or propagation time.
  pub(super) completed_at: Timestamp,
}

impl JobTerminalOutcomeProjection {
  /// Returns the authoritative terminal state.
  #[must_use]
  pub const fn state(self) -> JobState {
    self.state
  }

  /// Returns the safe failure classification, when terminal success is absent.
  #[must_use]
  pub const fn failure(self) -> Option<JobFailureClassification> {
    self.failure
  }

  /// Returns the authoritative completion or propagation time.
  #[must_use]
  pub const fn completed_at(self) -> Timestamp {
    self.completed_at
  }

  /// Projects an authoritative successful completion.
  #[must_use]
  pub const fn succeeded(completed_at: Timestamp) -> Self {
    Self {
      state: JobState::Succeeded,
      failure: None,
      completed_at,
    }
  }

  /// Projects an authoritative classified execution failure.
  #[must_use]
  pub fn failed(class: JobFailureClass, completed_at: Timestamp) -> Self {
    Self {
      state: JobState::Failed,
      failure: Some(class.into()),
      completed_at,
    }
  }

  /// Projects authoritative terminal cancellation.
  #[must_use]
  pub const fn cancelled(completed_at: Timestamp) -> Self {
    Self {
      state: JobState::Cancelled,
      failure: Some(JobFailureClassification::Cancelled),
      completed_at,
    }
  }

  /// Projects a Job skipped by dependency policy.
  #[must_use]
  pub const fn skipped(completed_at: Timestamp) -> Self {
    Self {
      state: JobState::Skipped,
      failure: Some(JobFailureClassification::DependencyPolicy),
      completed_at,
    }
  }
}

/// Stable logical SHA-256 content identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Sha256DigestProjection(String);

impl Sha256DigestProjection {
  /// Constructs one lowercase hexadecimal SHA-256 digest.
  pub fn new(value: impl Into<String>) -> Result<Self, ProjectionError> {
    let value = value.into();
    if value.len() == 64
      && value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
      Ok(Self(value))
    } else {
      Err(ProjectionError::InvalidContentDigest)
    }
  }

  /// Borrows the canonical hexadecimal digest.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Logical produced-output classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobOutputKind {
  /// Produced opaque file.
  Artifact,
  /// Produced report with a plugin-defined media or format identifier.
  Report,
}

/// Logical immutable output reference without a physical object location.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JobOutputReference {
  /// Stable logical Artifact identity.
  pub id: ArtifactId,
  /// Bounded logical output name.
  pub name: ArtifactName,
  /// Produced-output classification.
  pub kind: JobOutputKind,
  /// Immutable content digest.
  pub sha256: Sha256DigestProjection,
  /// Exact byte length.
  pub size_bytes: u64,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Lifecycle and related facts supplied with one authoritative Job snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobProjectionFacts {
  /// Current scheduling and execution state.
  pub state: JobState,
  /// Current optimistic state version.
  pub version: JobVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
  /// Queue-order inputs when the Job is currently ready.
  pub queue: Option<JobQueueProjection>,
  /// Selected execution assignment, retained after an executed Job terminates.
  pub assignment: Option<JobAssignmentProjection>,
  /// Terminal outcome when available.
  pub terminal: Option<JobTerminalOutcomeProjection>,
  /// Greatest contiguous durable event sequence, or zero before the first event.
  pub event_cursor: u64,
  /// Logical published output references.
  pub outputs: Vec<JobOutputReference>,
}

/// Safe application projection of one materialized Pipeline Job.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JobProjection {
  /// Stable Job identity.
  pub id: JobId,
  /// Owning Attempt.
  pub attempt_id: AttemptId,
  /// Immutable Pipeline node identity.
  pub pipeline_node_id: PipelineNodeId,
  /// Direct predecessor Jobs in stable identity order.
  pub dependencies: Vec<JobId>,
  /// Immutable fan-in and failure-propagation policy.
  pub dependency_policy: DependencyPolicy,
  /// Effective Pool allowlist captured for placement.
  pub allowed_pools: Vec<PoolId>,
  /// Backend-neutral placement inputs.
  pub placement: JobPlacementProjection,
  /// Current scheduling and execution state.
  pub state: JobState,
  /// Current optimistic state version.
  pub version: JobVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
  /// Queue-order inputs when currently ready.
  pub queue: Option<JobQueueProjection>,
  /// Selected Pool and assigned Agent, retained after execution terminates.
  pub assignment: Option<JobAssignmentProjection>,
  /// Terminal outcome and safe failure classification.
  pub terminal: Option<JobTerminalOutcomeProjection>,
  /// Greatest contiguous durable event sequence, or zero before the first event.
  pub event_cursor: u64,
  /// Logical published output references.
  pub outputs: Vec<JobOutputReference>,
}

impl JobProjection {
  /// Projects a materialized Job while omitting its signed JobSpec and internal snapshot.
  pub fn from_authoritative(
    attempt_id: AttemptId,
    job: &MaterializedJob,
    facts: JobProjectionFacts,
  ) -> Result<Self, ProjectionError> {
    let queue_expected = facts.state == JobState::Ready;
    let assignment_required = matches!(
      facts.state,
      JobState::Leased | JobState::Running | JobState::Cancelling | JobState::Succeeded | JobState::Failed
    );
    let assignment_forbidden = matches!(facts.state, JobState::Blocked | JobState::Ready | JobState::Skipped);
    let terminal_valid = match facts.terminal {
      None => !facts.state.is_terminal(),
      Some(terminal) if terminal.state != facts.state => false,
      Some(terminal) => match terminal.state {
        JobState::Succeeded => terminal.failure.is_none(),
        JobState::Failed => matches!(
          terminal.failure,
          Some(JobFailureClassification::Execution | JobFailureClassification::Infrastructure)
        ),
        JobState::Cancelled => terminal.failure == Some(JobFailureClassification::Cancelled),
        JobState::Skipped => terminal.failure == Some(JobFailureClassification::DependencyPolicy),
        JobState::Blocked | JobState::Ready | JobState::Leased | JobState::Running | JobState::Cancelling => false,
      },
    };
    if queue_expected != facts.queue.is_some()
      || (assignment_required && facts.assignment.is_none())
      || (assignment_forbidden && facts.assignment.is_some())
      || !terminal_valid
    {
      return Err(ProjectionError::InvalidJobLifecycle);
    }
    let placement = JobPlacementProjection::from(job.requirements.clone());
    Ok(Self {
      id: job.id,
      attempt_id,
      pipeline_node_id: job.pipeline_node_id.clone(),
      dependencies: job.dependencies.clone(),
      dependency_policy: job.dependency_policy,
      allowed_pools: job.allowed_pools.clone(),
      placement,
      state: facts.state,
      version: facts.version,
      created_at: facts.created_at,
      updated_at: facts.updated_at,
      queue: facts.queue,
      assignment: facts.assignment,
      terminal: facts.terminal,
      event_cursor: facts.event_cursor,
      outputs: facts.outputs,
    })
  }
}
