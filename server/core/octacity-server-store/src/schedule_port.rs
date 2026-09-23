use async_trait::async_trait;
use octacity_server_domain::{TriggerId, TriggerVersion};

use crate::{
  ClaimDueSchedules, CompleteScheduleClaim, CreateSchedule, DueScheduleClaim, MutationDisposition, ScheduleRecord,
  StoreError, TriggerDefinitionMutationOutcome,
};

/// Durable schedule management and replica-safe work claims.
#[async_trait]
pub trait ScheduleStore: Send + Sync {
  /// Atomically creates a scheduled Trigger and its first cursor.
  async fn create_schedule(&self, request: CreateSchedule) -> Result<TriggerDefinitionMutationOutcome, StoreError>;

  /// Reads one exact schedule for management projection.
  async fn schedule(&self, trigger_id: TriggerId, version: TriggerVersion) -> Result<ScheduleRecord, StoreError>;

  /// Claims a bounded batch without sharing live ownership across replicas.
  async fn claim_due_schedules(&self, request: ClaimDueSchedules) -> Result<Vec<DueScheduleClaim>, StoreError>;

  /// Advances one cursor only while the caller still owns its claim.
  async fn complete_schedule_claim(&self, request: CompleteScheduleClaim) -> Result<MutationDisposition, StoreError>;
}
