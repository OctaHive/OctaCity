//! Append-only phase journal used only to recognize and clean interrupted work.
//!
//! The journal is intentionally not a resumable workflow log. Each transition
//! is synced before the corresponding external phase proceeds, allowing the
//! next agent process to prove that a directory belongs to OctaCity and report
//! the last valid phase before deleting it. Corrupt or foreign state never
//! grants deletion authority.

use std::{fs, io::Write as _, path::Path};

use octacity_protocol::JobLifecycleState;
use serde::{Deserialize, Serialize};

use crate::JobLifecycleError;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct JournalRecord {
  state: JobLifecycleState,
}

pub(crate) struct JobJournal {
  file: fs::File,
  state: Option<JobLifecycleState>,
}

impl JobJournal {
  pub(crate) fn create(path: &Path) -> Result<Self, JobLifecycleError> {
    let file = fs::OpenOptions::new()
      .create_new(true)
      .write(true)
      .open(path)
      .map_err(JobLifecycleError::StateIo)?;
    Ok(Self { file, state: None })
  }

  pub(crate) fn transition(&mut self, next: JobLifecycleState) -> Result<(), JobLifecycleError> {
    if !valid_transition(self.state, next) {
      return Err(JobLifecycleError::InvalidTransition {
        from: self.state,
        to: next,
      });
    }
    let mut line = serde_json::to_vec(&JournalRecord { state: next }).map_err(JobLifecycleError::StateJson)?;
    line.push(b'\n');
    self.file.write_all(&line).map_err(JobLifecycleError::StateIo)?;
    self.file.sync_data().map_err(JobLifecycleError::StateIo)?;
    self.state = Some(next);
    Ok(())
  }
}

fn valid_transition(current: Option<JobLifecycleState>, next: JobLifecycleState) -> bool {
  use JobLifecycleState::{Cleaning, Completing, Freezing, Preparing, Running, Uploading};
  matches!(
    (current, next),
    (None, Preparing)
      | (Some(Preparing), Running | Cleaning)
      | (Some(Running), Freezing | Cleaning)
      | (Some(Freezing), Uploading | Cleaning)
      | (Some(Uploading), Cleaning)
      | (Some(Cleaning), Completing)
  )
}

/// Returns the last valid state only for a journal entirely owned by this
/// lifecycle implementation. A corrupt, empty, or foreign file is never used
/// as authority to delete its parent directory.
pub(crate) fn owned_attempt_state(path: &Path) -> Option<JobLifecycleState> {
  let journal = path.join("journal.jsonl");
  let Ok(contents) = fs::read_to_string(journal) else {
    return None;
  };
  if contents.is_empty() {
    return None;
  }
  let mut state = None;
  for line in contents.lines() {
    let Ok(record) = serde_json::from_str::<JournalRecord>(line) else {
      return None;
    };
    if !valid_transition(state, record.state) {
      return None;
    }
    state = Some(record.state);
  }
  state
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accepts_only_ordered_lifecycle_transitions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.jsonl");
    let mut journal = JobJournal::create(&path).unwrap();
    journal.transition(JobLifecycleState::Preparing).unwrap();
    journal.transition(JobLifecycleState::Running).unwrap();
    assert!(journal.transition(JobLifecycleState::Preparing).is_err());
    journal.transition(JobLifecycleState::Freezing).unwrap();
    journal.transition(JobLifecycleState::Uploading).unwrap();
    journal.transition(JobLifecycleState::Cleaning).unwrap();
    journal.transition(JobLifecycleState::Completing).unwrap();
    assert_eq!(
      owned_attempt_state(directory.path()),
      Some(JobLifecycleState::Completing)
    );
  }

  #[test]
  fn does_not_claim_corrupt_or_foreign_directories() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("journal.jsonl"), "not-json\n").unwrap();
    assert_eq!(owned_attempt_state(directory.path()), None);
  }
}
