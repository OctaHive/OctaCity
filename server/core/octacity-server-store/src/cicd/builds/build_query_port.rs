use async_trait::async_trait;
use octacity_server_domain::{AttemptId, BuildId, JobId};

use crate::{AttemptRecord, BuildRecord, JobRecord, StoreError};

/// Read-only authoritative access to Build, Attempt, and Job diagnostics.
#[async_trait]
pub trait BuildQueryStore: Send + Sync {
  /// Reads one Build and its immutable initiating facts.
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError>;

  /// Reads one Attempt and its complete materialized diagnostic DAG.
  async fn attempt(&self, attempt_id: AttemptId) -> Result<AttemptRecord, StoreError>;

  /// Reads one Job with queue, assignment, terminal, and event-cursor facts.
  async fn job(&self, job_id: JobId) -> Result<JobRecord, StoreError>;

  /// Reads the latest Attempt of one Build for replay-safe retry materialization.
  async fn latest_attempt(&self, build_id: BuildId) -> Result<AttemptRecord, StoreError>;
}
