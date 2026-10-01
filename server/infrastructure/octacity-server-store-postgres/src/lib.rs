//! PostgreSQL persistence adapter.
//!
//! Migrations, SQL rows, transaction mechanics, locks, and conversion to the
//! backend-neutral store contract are isolated here.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod accept_trigger;
mod agent_credential_auth;
mod agent_credential_revocation;
mod agent_enrollment;
mod agent_mutation;
mod agent_query;
mod agent_registration;
mod agent_row;
mod artifact;
mod attempt_materialization;
mod audit_query;
mod build_configuration_mutation;
mod build_query;
mod cache;
mod cancel_build;
mod configuration_query;
mod configuration_row;
mod database;
mod definition_mutation;
mod external_trigger;
mod internal_trigger;
mod internal_trigger_definition;
mod job_claim;
mod job_completion;
mod job_event_query;
mod job_events;
mod lease;
mod lease_heartbeat;
mod lease_recovery;
mod log_index_work;
mod log_search;
mod migration;
mod mutation;
mod pipeline_mutation;
mod pipeline_query;
mod pipeline_row;
mod pool_mutation;
mod pool_query;
mod pool_row;
mod project_mutation;
mod project_policy_query;
mod project_query;
mod project_row;
mod read_visibility;
mod ready_queue_notification;
mod repository_mutation;
mod restore;
mod retention;
mod retention_hold;
mod retry_build;
mod schedule;
mod state;
mod store;
mod telemetry;
mod trigger_evaluation;
mod trigger_query;

use sqlx::PgPool;

pub use log_search::{LogSearchRebuildSummary, PostgresLogSearchIndex};
pub use migration::{
  MIGRATOR, MigrationStatus, PREVIOUS_BINARY_SCHEMA_VERSION, current_schema_version, migrate, migration_status,
  run_migrator,
};
pub use ready_queue_notification::ready_job_count;
pub use store::{PostgresAuthoritativeStore, PostgresStore};

/// PostgreSQL notification channel emitted after a ready-queue transaction commits.
pub const READY_JOB_NOTIFICATION_CHANNEL: &str = "octacity_ready_jobs";

/// Checks that PostgreSQL can execute a trivial query through the pool.
pub async fn health_check(pool: &PgPool) -> bool {
  sqlx::query_scalar::<_, i32>("SELECT 1")
    .fetch_one(pool)
    .await
    .is_ok_and(|value| value == 1)
}
