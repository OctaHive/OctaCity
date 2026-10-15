//! Owns one leased attempt above source, runner, and transport components.
//!
//! The mutable phase lives in one task. Runner events cross a bounded channel,
//! while the delivery task owns only the concrete local spool and coordinator
//! calls. This keeps lease policy out of execution backends and keeps HTTP and
//! filesystem formats out of the job orchestrator.

#![warn(missing_docs)]

mod codex_hook;
#[cfg(unix)]
mod codex_reader_runtime;
#[cfg(unix)]
pub use codex_reader_runtime::{FactoryProtectedReadInstallation, FactoryProtectedReadRuntime};
mod confined_read_backend;
pub use confined_read_backend::ConfinedProtectedReadBackend;
mod delivery;
mod journal;
mod lifecycle;
mod protected_read_recipe;
mod rooted_read_backend;
mod spool;
mod tool_action_broker;

pub use codex_hook::{
  CodexHookError, CodexMcpReadRecipe, CodexPreToolUseInput, codex_denied_output, codex_permitted_output,
};
pub use lifecycle::{
  FactoryToolExecutionProvider, JobLifecycle, JobLifecycleConfig, JobLifecycleError, JobLifecycleOutcome,
  RecoveredAttempt, cleanup_incomplete_attempts, trusted_change_set_outputs,
};
pub use protected_read_recipe::ProtectedReadRecipe;
pub use rooted_read_backend::{ProtectedReadInventory, RootedProtectedReadBackend};

pub use spool::{SpoolError, SpoolLimits};
#[cfg(unix)]
pub use tool_action_broker::ProtectedReadChannel;
pub use tool_action_broker::{
  AgentToolActionBroker, AgentToolActionBrokerError, ProtectedReadBackend, ProtectedReadDispatcher,
};

#[cfg(test)]
mod capture_outputs_tests;
