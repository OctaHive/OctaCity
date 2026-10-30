//! Local credential files share execution scratch lifetime and never enter public inputs.
use crate::RunnerSupervisionError;
#[cfg(unix)]
use octacity_protocol::{FACTORY_PRIVATE_CREDENTIAL_ROOT, FACTORY_SCRATCH_ROOT};
use std::{
  fmt,
  path::{Component, Path, PathBuf},
  sync::{Arc, Mutex},
};
#[cfg(unix)]
use std::{fs, io::Write as _};
use zeroize::{Zeroize as _, Zeroizing};

const MAX_PRIVATE_JSON_BYTES: u64 = 64 * 1024;
mod persistence;

/// One currently authorized private JSON file, with no host path in diagnostics.
/// The caller must bind its logical profile to the signed credential selection.
pub struct RunnerPrivateJsonFile {
  source: PathBuf,
  destination: PathBuf,
  bytes: Zeroizing<Vec<u8>>,
  redactions: Vec<Zeroizing<Vec<u8>>>,
  fields: Option<Vec<String>>,
}
impl RunnerPrivateJsonFile {
  /// Read the current owner-private regular file without following a link.
  pub fn load(source: &Path, destination: &str) -> Result<Self, RunnerSupervisionError> {
    Self::load_selected(source, destination, None)
  }

  /// Select sensitive JSON subtrees using operator-installed JSON pointers.
  /// Missing selections fail closed; ordinary authentication metadata need not
  /// be treated as a credential merely because it shares the same file.
  pub fn load_scoped(source: &Path, destination: &str, fields: &[String]) -> Result<Self, RunnerSupervisionError> {
    Self::load_selected(source, destination, Some(fields))
  }

  fn load_selected(
    source: &Path,
    destination: &str,
    fields: Option<&[String]>,
  ) -> Result<Self, RunnerSupervisionError> {
    let read = || -> Result<Self, Box<dyn std::error::Error>> {
      let destination = Path::new(destination);
      if destination.as_os_str().is_empty()
        || destination.as_os_str().len() > 1024
        || destination.to_string_lossy().contains(['\\', ':'])
        || destination
          .components()
          .any(|part| !matches!(part, Component::Normal(_)))
      {
        return Err("invalid relative credential destination".into());
      }
      let parent = source.parent().ok_or("credential source has no parent")?;
      octacity_private_fs::validate_trusted_directory_chain(parent)?;
      octacity_private_fs::validate_private_access(parent)?;
      octacity_private_fs::validate_trusted_owner(source)?;
      octacity_private_fs::validate_private_access(source)?;
      let bytes = Zeroizing::new(octacity_private_fs::read_bounded_regular_file(
        source,
        MAX_PRIVATE_JSON_BYTES,
      )?);
      let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
      let object = value.is_object();
      let mut redactions = Vec::new();
      let selected = match fields {
        Some(fields) if fields.is_empty() || fields.len() > 64 => false,
        Some(fields) => {
          fields.iter().all(|field| {
            value.pointer(field).is_some_and(|value| {
              collect_redactions(value, &mut redactions);
              true
            })
          }) && !redactions.is_empty()
        }
        None => {
          collect_redactions(&value, &mut redactions);
          true
        }
      };
      erase_json(&mut value);
      if !object || !selected {
        return Err("credential JSON must be an object".into());
      }
      Ok(Self {
        source: source.to_owned(),
        destination: destination.to_owned(),
        bytes,
        redactions,
        fields: fields.map(<[String]>::to_vec),
      })
    };
    read().map_err(|_| invalid())
  }

  /// Sensitive values for the existing supervisor output filter, including
  /// individual nested tokens that a native client may mention in diagnostics.
  pub fn redaction_values(&self) -> impl Iterator<Item = &[u8]> {
    std::iter::once(self.bytes.as_slice()).chain(self.redactions.iter().map(|value| value.as_slice()))
  }

  /// Copy once into the private execution scratch tree; never replace existing files.
  pub fn publish(&self, scratch: &Path) -> Result<RunnerPrivateCredentialGuard, RunnerSupervisionError> {
    self.publish_with_updates(scratch, None)
  }

  /// Select a serialized Agent-owned credential installation whose native
  /// updates must survive the execution. Never select a personal home cache:
  /// bootstrap a separate private installation and reseed only while stopped.
  pub fn publish_persistent(&self, scratch: &Path) -> Result<RunnerPrivateCredentialGuard, RunnerSupervisionError> {
    let updates = persistence::UpdateLease::acquire(&self.source, &self.bytes).map_err(|_| invalid())?;
    self.publish_with_updates(scratch, Some(updates))
  }

  fn publish_with_updates(
    &self,
    scratch: &Path,
    updates: Option<persistence::UpdateLease>,
  ) -> Result<RunnerPrivateCredentialGuard, RunnerSupervisionError> {
    #[cfg(unix)]
    {
      use std::os::unix::fs::OpenOptionsExt as _;
      let write = || -> std::io::Result<PathBuf> {
        octacity_private_fs::validate_trusted_directory_chain(scratch)?;
        octacity_private_fs::validate_private_access(scratch)?;
        let relative = Path::new(FACTORY_PRIVATE_CREDENTIAL_ROOT)
          .strip_prefix(FACTORY_SCRATCH_ROOT)
          .map_err(|_| std::io::Error::other("invalid credential layout"))?;
        let root = scratch.join(relative);
        private_directory(&root)?;
        let target = root.join(&self.destination);
        let mut current = root;
        for part in self
          .destination
          .parent()
          .ok_or_else(|| std::io::Error::other("invalid destination"))?
          .components()
        {
          current.push(part);
          private_directory(&current)?;
        }
        let mut file = fs::OpenOptions::new()
          .write(true)
          .create_new(true)
          .mode(0o600)
          .open(&target)?;
        if let Err(error) = file.write_all(&self.bytes) {
          drop(file);
          let _ = fs::remove_file(&target);
          return Err(error);
        }
        Ok(target)
      };
      let path = write().map_err(|_| invalid())?;
      Ok(RunnerPrivateCredentialGuard(Arc::new(Mutex::new(WatchedCredential {
        path,
        fields: self.fields.clone(),
        current: self.bytes.clone(),
        seen: self
          .redaction_values()
          .map(|value| Zeroizing::new(value.to_vec()))
          .collect(),
        updates,
      }))))
    }
    #[cfg(not(unix))]
    {
      let _ = (scratch, updates);
      Err(invalid())
    }
  }
}

/// Per-execution file inspection shared by supervision and final publication.
/// Retains previous values after native-client refresh without retaining a home
/// directory or publishing any credential data to the coordinator.
#[derive(Clone)]
pub struct RunnerPrivateCredentialGuard(Arc<Mutex<WatchedCredential>>);

struct WatchedCredential {
  path: PathBuf,
  fields: Option<Vec<String>>,
  current: Zeroizing<Vec<u8>>,
  seen: Vec<Zeroizing<Vec<u8>>>,
  updates: Option<persistence::UpdateLease>,
}

/// Opaque, zeroing values selected for one output privacy check.
#[derive(Default)]
pub struct RunnerSensitiveValues(Vec<Zeroizing<Vec<u8>>>);
impl RunnerSensitiveValues {
  /// Copy values into an opaque zeroing snapshot for post-execution checks.
  pub fn new<'a>(values: impl IntoIterator<Item = &'a [u8]>) -> Self {
    let mut selected = Vec::new();
    for value in values.into_iter().filter(|value| !value.is_empty()) {
      selected.push(Zeroizing::new(value.to_vec()));
      // Native clients consume trimmed token files. Protect both representations
      // when line-oriented output omits the original trailing newline.
      let trimmed = value.trim_ascii();
      if !trimmed.is_empty() && trimmed != value {
        selected.push(Zeroizing::new(trimmed.to_vec()));
      }
    }
    selected.sort_unstable_by(|left, right| left.as_slice().cmp(right.as_slice()));
    selected.dedup();
    Self(selected)
  }
  /// Borrow values only for the duration of a filter or publication check.
  pub fn values(&self) -> impl Iterator<Item = &[u8]> {
    self.0.iter().map(|value| value.as_slice())
  }

  pub(crate) fn into_values(self) -> Vec<Zeroizing<Vec<u8>>> {
    self.0
  }
}

impl RunnerPrivateCredentialGuard {
  /// Retain native-client updates in the explicitly selected private owner.
  /// Call after backend destruction, including unsuccessful native executions.
  /// Ordinary copied files have no write-back authority.
  pub fn persist_updates(&self) -> Result<(), RunnerSupervisionError> {
    let mut state = self.0.lock().map_err(|_| invalid())?;
    if state.updates.is_none() {
      return Ok(());
    }
    state.refresh()?;
    let WatchedCredential { updates, current, .. } = &mut *state;
    if let Some(updates) = updates {
      updates.persist(current).map_err(|_| invalid())?;
    }
    Ok(())
  }
  /// Re-read the execution copy and retain both original and refreshed values.
  /// Missing, unsafe, malformed or excessively rotated files fail closed.
  pub fn snapshot(&self) -> Result<RunnerSensitiveValues, RunnerSupervisionError> {
    let mut state = self.0.lock().map_err(|_| invalid())?;
    state.refresh()?;
    Ok(RunnerSensitiveValues(state.seen.clone()))
  }

  pub(crate) fn updated_values(&self) -> Result<Option<RunnerSensitiveValues>, RunnerSupervisionError> {
    let mut state = self.0.lock().map_err(|_| invalid())?;
    Ok(state.refresh()?.then(|| RunnerSensitiveValues(state.seen.clone())))
  }
}
impl WatchedCredential {
  fn refresh(&mut self) -> Result<bool, RunnerSupervisionError> {
    let file = RunnerPrivateJsonFile::load_selected(&self.path, "credential.json", self.fields.as_deref())?;
    if file.bytes == self.current {
      return Ok(false);
    }
    let additional = file
      .redaction_values()
      .filter(|value| !self.seen.iter().any(|seen| seen.as_slice() == *value))
      .map(|value| Zeroizing::new(value.to_vec()))
      .collect::<Vec<_>>();
    let total = self
      .seen
      .iter()
      .chain(&additional)
      .map(|value| value.len())
      .sum::<usize>();
    if total > crate::private_variables::MAX_PRIVATE_VARIABLE_BYTES {
      return Err(invalid());
    }
    self.seen.extend(additional);
    self.current = file.bytes;
    Ok(true)
  }
}
impl fmt::Debug for RunnerPrivateCredentialGuard {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("RunnerPrivateCredentialGuard { .. }")
  }
}
impl fmt::Debug for RunnerSensitiveValues {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("RunnerSensitiveValues { .. }")
  }
}
fn collect_redactions(value: &serde_json::Value, selected: &mut Vec<Zeroizing<Vec<u8>>>) {
  match value {
    serde_json::Value::String(text) if !text.is_empty() => selected.push(Zeroizing::new(text.as_bytes().to_vec())),
    serde_json::Value::Array(values) => values.iter().for_each(|value| collect_redactions(value, selected)),
    serde_json::Value::Object(values) => values.values().for_each(|value| collect_redactions(value, selected)),
    _ => {}
  }
}
#[cfg(unix)]
fn private_directory(path: &Path) -> std::io::Result<()> {
  if let Err(error) = octacity_private_fs::create_private_directory(path) {
    if error.kind() != std::io::ErrorKind::AlreadyExists {
      return Err(error);
    }
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
      return Err(std::io::Error::other("invalid private directory"));
    }
    octacity_private_fs::validate_trusted_owner(path)?;
    octacity_private_fs::validate_private_access(path)?;
  }
  Ok(())
}
fn erase_json(value: &mut serde_json::Value) {
  match value {
    serde_json::Value::String(text) => text.zeroize(),
    serde_json::Value::Array(values) => values.iter_mut().for_each(erase_json),
    serde_json::Value::Object(values) => {
      for (mut name, mut value) in std::mem::take(values) {
        name.zeroize();
        erase_json(&mut value);
      }
    }
    _ => {}
  }
}
impl fmt::Debug for RunnerPrivateJsonFile {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("RunnerPrivateJsonFile")
      .field("byte_count", &self.bytes.len())
      .finish_non_exhaustive()
  }
}
fn invalid() -> RunnerSupervisionError {
  RunnerSupervisionError::Invalid("private credential file installation is invalid".into())
}
