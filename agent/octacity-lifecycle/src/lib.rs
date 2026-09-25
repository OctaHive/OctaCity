//! Owns one leased attempt above source, runner, and transport components.
//!
//! The mutable phase lives in one task. Runner events cross a bounded channel,
//! while the delivery task owns only the concrete local spool and coordinator
//! calls. This keeps lease policy out of execution backends and keeps HTTP and
//! filesystem formats out of the job orchestrator.

#![warn(missing_docs)]

mod delivery;
mod journal;
mod lifecycle;
mod spool;

pub use lifecycle::{
  JobLifecycle, JobLifecycleConfig, JobLifecycleError, JobLifecycleOutcome, RecoveredAttempt,
  cleanup_incomplete_attempts,
};
pub use spool::{SpoolError, SpoolLimits};
