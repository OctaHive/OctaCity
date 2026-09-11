//! Conservative startup cleanup for journal-proven interrupted attempts.
//!
//! Recovery never resumes execution and never trusts directory names alone.
//! The backend and workspace cleanup runs first in the agent; this module then
//! extracts bounded operator diagnostics and deletes only directories whose
//! complete journal is valid.

use std::{fs, path::Path};

use octacity_protocol::JobLifecycleState;
use tracing::warn;

use crate::journal::owned_attempt_state;

use super::{JobLifecycleError, is_attempt_directory};

/// Diagnostic summary recovered before an interrupted attempt is removed.
///
/// The identifier is a local SHA-256-derived attempt name and contains no
/// server-controlled job or lease text. Recovery never resumes execution; this
/// record exists so operators can distinguish where an interrupted attempt
/// stopped and whether it had unsent durable state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredAttempt {
  /// Stable local identifier of the removed attempt directory.
  pub attempt_id: String,
  /// Last lifecycle phase proven by the append-only journal.
  pub last_state: JobLifecycleState,
  /// Number of immutable event records still present during recovery.
  pub unacknowledged_events: usize,
  /// Whether the exact terminal completion document had been persisted.
  pub completion_persisted: bool,
}

/// Describes and removes only state directories proven to be interrupted attempts.
///
/// Callers first destroy backend resources and workspaces through
/// `JobExecutor::cleanup_orphans`; no previous process is ever resumed. The
/// returned summaries must be logged before the agent acquires more work so
/// recovery never becomes an invisible destructive operation.
pub fn cleanup_incomplete_attempts(state_root: &Path) -> Result<Vec<RecoveredAttempt>, JobLifecycleError> {
  let jobs = state_root.join("jobs");
  let entries = match fs::read_dir(&jobs) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(error) => return Err(JobLifecycleError::StateIo(error)),
  };
  let mut recovered = Vec::new();
  for entry in entries {
    let entry = entry.map_err(JobLifecycleError::StateIo)?;
    let path = entry.path();
    let name = entry.file_name();
    let Some(name) = name.to_str() else {
      continue;
    };
    if !is_attempt_directory(name) || !entry.file_type().map_err(JobLifecycleError::StateIo)?.is_dir() {
      warn!(path = %path.display(), "leaving unrecognized state entry untouched");
      continue;
    }
    let Some(state) = owned_attempt_state(&path) else {
      warn!(path = %path.display(), "leaving unrecognized state entry untouched");
      continue;
    };
    let summary = RecoveredAttempt {
      attempt_id: name.to_owned(),
      last_state: state,
      unacknowledged_events: event_record_count(&path)?,
      completion_persisted: path.join("completion.json").is_file(),
    };
    fs::remove_dir_all(&path).map_err(JobLifecycleError::StateIo)?;
    recovered.push(summary);
  }
  Ok(recovered)
}

fn event_record_count(attempt_root: &Path) -> Result<usize, JobLifecycleError> {
  let entries = match fs::read_dir(attempt_root.join("events")) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
    Err(error) => return Err(JobLifecycleError::StateIo(error)),
  };
  let mut count = 0_usize;
  for entry in entries {
    let entry = entry.map_err(JobLifecycleError::StateIo)?;
    if entry.file_type().map_err(JobLifecycleError::StateIo)?.is_file() {
      count = count.saturating_add(1);
    }
  }
  Ok(count)
}
