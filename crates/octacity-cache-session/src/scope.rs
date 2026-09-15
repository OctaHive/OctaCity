//! Persistent cache-scope ownership, LRU metadata, and reclamation.
//!
//! The agent exposes only one scope directory to untrusted execution. This
//! module keeps the process lock and access markers outside that mount, so a
//! job cannot make an active scope look reclaimable or protect stale data by
//! rewriting its own timestamps.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs::{File, OpenOptions},
  path::{Path, PathBuf},
  sync::{Arc, Mutex as StdMutex},
  time::SystemTime,
};

use fs4::{FileExt, TryLockError};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use crate::{CacheReclaimReport, CacheSessionError, create_private_path, io, validate_existing_private_directory};

pub(crate) const SCOPE_LAYOUT_DIRECTORY: &str = "v1";
pub(crate) const ACCESS_LAYOUT_DIRECTORY: &str = "access-v1";
pub(crate) const RECLAIM_LAYOUT_DIRECTORY: &str = "reclaim-v1";
const ROOT_LOCK_FILE: &str = ".octacity.lock";
/// Lowercase hexadecimal length of the SHA-256 physical scope identifier.
pub(super) const SCOPE_DIRECTORY_HEX_LENGTH: usize = 64;

#[derive(Debug, Default)]
struct ScopeState {
  active: BTreeMap<PathBuf, usize>,
  reclaiming: BTreeSet<PathBuf>,
}

/// Exclusive owner of one cache root within and across agent processes.
#[derive(Debug)]
pub(super) struct ScopeStore {
  root: PathBuf,
  max_scopes: usize,
  state: StdMutex<ScopeState>,
  /// Serializes filesystem admission snapshots and reclamation decisions.
  /// Existing scope leases do not hold this lock while jobs use their cache.
  admission: Arc<AsyncMutex<()>>,
  _process_lock: File,
}

/// In-memory lease proving that one prepared job still uses a scope.
pub(super) struct ScopeLease {
  store: Arc<ScopeStore>,
  pub(super) directory: PathBuf,
}

impl ScopeStore {
  pub(super) fn new(root: PathBuf, max_scopes: usize) -> Result<Arc<Self>, CacheSessionError> {
    let lock_path = root.join(ROOT_LOCK_FILE);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
      use std::os::unix::fs::OpenOptionsExt as _;
      options.mode(0o600);
    }
    let process_lock = options
      .open(&lock_path)
      .map_err(|source| io("open cache ownership lock", &lock_path, source))?;
    let metadata = process_lock
      .metadata()
      .map_err(|source| io("inspect cache ownership lock", &lock_path, source))?;
    if !metadata.file_type().is_file() {
      return Err(CacheSessionError::Invalid(
        "cache ownership lock is not a regular file".to_owned(),
      ));
    }
    octacity_private_fs::validate_private_access(&lock_path)
      .map_err(|source| io("validate cache ownership lock", &lock_path, source))?;
    match FileExt::try_lock(&process_lock) {
      Ok(()) => {}
      Err(TryLockError::WouldBlock) => {
        return Err(CacheSessionError::Invalid(
          "cache root is already owned by another agent process".to_owned(),
        ));
      }
      Err(TryLockError::Error(source)) => return Err(io("lock cache root", &lock_path, source)),
    }
    create_private_path(&root.join(RECLAIM_LAYOUT_DIRECTORY), false)?;
    Ok(Arc::new(Self {
      root,
      max_scopes,
      state: StdMutex::new(ScopeState::default()),
      admission: Arc::new(AsyncMutex::new(())),
      _process_lock: process_lock,
    }))
  }

  /// Acquires or creates one scope on the blocking pool. At the scope-count
  /// limit, one inactive LRU scope is completely removed before creation.
  pub(super) async fn acquire(
    self: &Arc<Self>,
    directory: PathBuf,
    cancellation: CancellationToken,
  ) -> Result<ScopeLease, CacheSessionError> {
    let admission = self.admission.clone();
    let guard = tokio::select! {
      () = cancellation.cancelled() => return Err(CacheSessionError::Cancelled),
      guard = admission.lock_owned() => guard,
    };
    let store = self.clone();
    tokio::task::spawn_blocking(move || {
      let _guard = guard;
      store.acquire_blocking(directory, &cancellation)
    })
    .await
    .map_err(join_error)?
  }

  /// Removes at most one inactive LRU scope on the blocking pool.
  ///
  /// Bounding a pass to one directory lets the admission loop observe
  /// shutdown between scopes while deletion of one nested tree remains an
  /// atomic ownership operation.
  pub(super) async fn reclaim_one(
    self: &Arc<Self>,
    minimum_free_bytes: u64,
    cancellation: CancellationToken,
  ) -> Result<CacheReclaimReport, CacheSessionError> {
    let admission = self.admission.clone();
    let guard = tokio::select! {
      () = cancellation.cancelled() => return Err(CacheSessionError::Cancelled),
      guard = admission.lock_owned() => guard,
    };
    let store = self.clone();
    tokio::task::spawn_blocking(move || {
      let _guard = guard;
      store.reclaim_one_blocking(minimum_free_bytes, &cancellation)
    })
    .await
    .map_err(join_error)?
  }

  fn acquire_blocking(
    self: &Arc<Self>,
    directory: PathBuf,
    cancellation: &CancellationToken,
  ) -> Result<ScopeLease, CacheSessionError> {
    let layout = self.root.join(SCOPE_LAYOUT_DIRECTORY);
    let access = self.root.join(ACCESS_LAYOUT_DIRECTORY);
    let reclaim = self.root.join(RECLAIM_LAYOUT_DIRECTORY);
    create_private_path(&layout, false)?;
    create_private_path(&reclaim, false)?;
    loop {
      check_cancelled(Some(cancellation))?;
      // Reclamation may walk an arbitrarily large job-written tree. It claims
      // each tombstone under the state lock, then performs filesystem I/O
      // without preventing a completed job from dropping its scope lease.
      cleanup_abandoned_tombstones(&reclaim, &access, self, Some(cancellation))?;
      let mut state = self
        .state
        .lock()
        .map_err(|_| CacheSessionError::Invalid("cache scope state lock is poisoned".to_owned()))?;
      if state.reclaiming.contains(&directory) {
        return Err(CacheSessionError::Invalid(
          "requested cache scope is currently being reclaimed".to_owned(),
        ));
      }
      match std::fs::symlink_metadata(&directory) {
        Ok(_) => {
          create_private_path(&directory, false)?;
          touch_last_used(&access, &directory)?;
          *state.active.entry(directory.clone()).or_default() += 1;
          return Ok(ScopeLease {
            store: self.clone(),
            directory,
          });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
          // Inspect the bounded on-disk set without serializing lease drops
          // behind directory and marker metadata reads. Recheck the target
          // after reacquiring the lock because another admission may have
          // created it while this thread scanned.
          drop(state);
          let observed = scope_entries(&layout, &access)?;
          state = self
            .state
            .lock()
            .map_err(|_| CacheSessionError::Invalid("cache scope state lock is poisoned".to_owned()))?;
          if state.reclaiming.contains(&directory) {
            return Err(CacheSessionError::Invalid(
              "requested cache scope is currently being reclaimed".to_owned(),
            ));
          }
          match std::fs::symlink_metadata(&directory) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io("inspect cache scope", &directory, source)),
          }
          let inactive = inactive_scopes(observed, &state);
          let scope_count = inactive
            .len()
            .saturating_add(state.active.len())
            .saturating_add(state.reclaiming.len());
          if scope_count < self.max_scopes {
            create_private_path(&directory, false)?;
            touch_last_used(&access, &directory)?;
            *state.active.entry(directory.clone()).or_default() += 1;
            return Ok(ScopeLease {
              store: self.clone(),
              directory,
            });
          }
          let Some(oldest) = oldest_scope(inactive) else {
            return Err(CacheSessionError::Invalid(
              "cache has no inactive scope available for reclamation".to_owned(),
            ));
          };
          state.reclaiming.insert(oldest.clone());
          drop(state);
          let removal = remove_scope(&oldest, &reclaim, &access, Some(cancellation));
          self.finish_reclamation(&oldest)?;
          removal?;
        }
        Err(source) => return Err(io("inspect cache scope", &directory, source)),
      }
    }
  }

  fn reclaim_one_blocking(
    &self,
    minimum_free_bytes: u64,
    cancellation: &CancellationToken,
  ) -> Result<CacheReclaimReport, CacheSessionError> {
    check_cancelled(Some(cancellation))?;
    let initial_free_bytes = fs4::available_space(&self.root)
      .map_err(|source| io("inspect cache filesystem free space", &self.root, source))?;
    if initial_free_bytes >= minimum_free_bytes {
      return Ok(CacheReclaimReport {
        initial_free_bytes,
        final_free_bytes: initial_free_bytes,
        removed_scopes: 0,
      });
    }

    let reclaim = self.root.join(RECLAIM_LAYOUT_DIRECTORY);
    let access = self.root.join(ACCESS_LAYOUT_DIRECTORY);
    // A crash can leave a quarantined scope consuming the capacity that this
    // pass is trying to recover. Complete those removals before measuring free
    // space; each deletion is claimed briefly under the in-memory state lock.
    let recovered_scopes = cleanup_abandoned_tombstones(&reclaim, &access, self, Some(cancellation))?;
    let recovered_free_bytes = fs4::available_space(&self.root)
      .map_err(|source| io("inspect cache filesystem free space", &self.root, source))?;
    if recovered_free_bytes >= minimum_free_bytes {
      return Ok(CacheReclaimReport {
        initial_free_bytes,
        final_free_bytes: recovered_free_bytes,
        removed_scopes: recovered_scopes,
      });
    }

    let layout = self.root.join(SCOPE_LAYOUT_DIRECTORY);
    let observed = scope_entries(&layout, &access)?;
    let candidate = {
      let mut state = self
        .state
        .lock()
        .map_err(|_| CacheSessionError::Invalid("cache scope state lock is poisoned".to_owned()))?;
      let candidate = oldest_scope(inactive_scopes(observed, &state));
      if let Some(directory) = &candidate {
        state.reclaiming.insert(directory.clone());
      }
      candidate
    };
    let Some(directory) = candidate else {
      return Ok(CacheReclaimReport {
        initial_free_bytes,
        final_free_bytes: recovered_free_bytes,
        removed_scopes: recovered_scopes,
      });
    };
    let removal = remove_scope(&directory, &reclaim, &access, Some(cancellation));
    self.finish_reclamation(&directory)?;
    removal?;
    let final_free_bytes = fs4::available_space(&self.root)
      .map_err(|source| io("inspect cache filesystem free space", &self.root, source))?;
    Ok(CacheReclaimReport {
      initial_free_bytes,
      final_free_bytes,
      removed_scopes: recovered_scopes.saturating_add(1),
    })
  }

  fn finish_reclamation(&self, directory: &Path) -> Result<(), CacheSessionError> {
    self
      .state
      .lock()
      .map_err(|_| CacheSessionError::Invalid("cache scope state lock is poisoned".to_owned()))?
      .reclaiming
      .remove(directory);
    Ok(())
  }

  /// Claims one tombstone by filename so an abandoned-cleanup pass cannot race
  /// a scope currently being moved out of the live namespace.
  fn claim_reclamation(&self, directory: &Path) -> Result<bool, CacheSessionError> {
    let name = directory
      .file_name()
      .ok_or_else(|| CacheSessionError::Invalid("cache reclamation entry has no name".to_owned()))?;
    let mut state = self
      .state
      .lock()
      .map_err(|_| CacheSessionError::Invalid("cache scope state lock is poisoned".to_owned()))?;
    if state
      .reclaiming
      .iter()
      .any(|candidate| candidate.file_name() == Some(name))
    {
      return Ok(false);
    }
    state.reclaiming.insert(directory.to_owned());
    Ok(true)
  }
}

impl Drop for ScopeLease {
  fn drop(&mut self) {
    let Ok(mut state) = self.store.state.lock() else {
      return;
    };
    let remove = match state.active.get_mut(&self.directory) {
      Some(count) if *count > 1 => {
        *count -= 1;
        false
      }
      Some(_) => true,
      None => false,
    };
    if remove {
      state.active.remove(&self.directory);
    }
  }
}

fn scope_entries(layout: &Path, access: &Path) -> Result<Vec<(SystemTime, PathBuf)>, CacheSessionError> {
  let entries = match std::fs::read_dir(layout) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(source) => return Err(io("read cache scopes", layout, source)),
  };
  let mut result = Vec::new();
  for entry in entries {
    let entry = entry.map_err(|source| io("read cache scope entry", layout, source))?;
    let path = entry.path();
    let name = entry.file_name();
    let valid_name = name.to_str().is_some_and(valid_scope_name);
    if !valid_name {
      return Err(CacheSessionError::Invalid(format!(
        "cache scope root contains an unexpected entry '{}'",
        path.display()
      )));
    }
    let metadata = std::fs::symlink_metadata(&path).map_err(|source| io("inspect cache scope", &path, source))?;
    validate_existing_private_directory(&path, &metadata)?;
    let marker = access_marker(access, &path)?;
    let modified = match std::fs::symlink_metadata(&marker) {
      Ok(metadata) if metadata.file_type().is_file() => metadata.modified(),
      Ok(_) => {
        return Err(CacheSessionError::Invalid(format!(
          "cache access marker '{}' is not a regular file",
          marker.display()
        )));
      }
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        std::fs::metadata(&path).and_then(|metadata| metadata.modified())
      }
      Err(error) => Err(error),
    }
    .map_err(|source| io("inspect cache scope age", &path, source))?;
    result.push((modified, path));
  }
  Ok(result)
}

fn inactive_scopes(entries: Vec<(SystemTime, PathBuf)>, state: &ScopeState) -> Vec<(SystemTime, PathBuf)> {
  entries
    .into_iter()
    .filter(|(_, path)| !state.active.contains_key(path) && !state.reclaiming.contains(path))
    .collect()
}

fn oldest_scope(scopes: Vec<(SystemTime, PathBuf)>) -> Option<PathBuf> {
  scopes
    .into_iter()
    .min_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
    .map(|(_, directory)| directory)
}

/// Updates LRU state outside the scope mounted into untrusted execution.
fn touch_last_used(access: &Path, directory: &Path) -> Result<(), CacheSessionError> {
  create_private_path(access, false)?;
  let marker = access_marker(access, directory)?;
  match std::fs::remove_file(&marker) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(source) => return Err(io("replace cache scope access marker", &marker, source)),
  }
  let mut options = OpenOptions::new();
  options.create_new(true).write(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
  }
  options
    .open(&marker)
    .and_then(|file| file.sync_all())
    .map_err(|source| io("update cache scope access marker", &marker, source))
}

fn access_marker(access: &Path, directory: &Path) -> Result<PathBuf, CacheSessionError> {
  let name = directory
    .file_name()
    .ok_or_else(|| CacheSessionError::Invalid("cache scope has no directory name".to_owned()))?;
  Ok(access.join(name))
}

fn remove_scope(
  directory: &Path,
  reclaim: &Path,
  access: &Path,
  cancellation: Option<&CancellationToken>,
) -> Result<(), CacheSessionError> {
  check_cancelled(cancellation)?;
  create_private_path(reclaim, false)?;
  let name = directory
    .file_name()
    .ok_or_else(|| CacheSessionError::Invalid("cache scope has no directory name".to_owned()))?;
  let tombstone = reclaim.join(name);
  match std::fs::symlink_metadata(&tombstone) {
    Ok(_) => {
      return Err(CacheSessionError::Invalid(format!(
        "cache reclamation tombstone '{}' already exists",
        tombstone.display()
      )));
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(source) => return Err(io("inspect cache reclamation tombstone", &tombstone, source)),
  }
  std::fs::rename(directory, &tombstone).map_err(|source| io("quarantine inactive cache scope", directory, source))?;
  let marker = access_marker(access, directory)?;
  match std::fs::remove_file(&marker) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(source) => return Err(io("remove cache scope access marker", &marker, source)),
  };
  // The atomic rename removes the scope from the live namespace before the
  // first cancellable delete. A crash or cancellation can therefore leave only
  // a host-owned tombstone, never a half-deleted scope that a job can reopen.
  remove_tree(&tombstone, cancellation)
}

/// Completes reclamations interrupted after their live scope was atomically
/// quarantined. Entries still owned by a concurrent in-process reclaimer are
/// skipped; the process-wide root lock excludes other agents.
fn cleanup_abandoned_tombstones(
  reclaim: &Path,
  access: &Path,
  store: &ScopeStore,
  cancellation: Option<&CancellationToken>,
) -> Result<usize, CacheSessionError> {
  let mut removed = 0_usize;
  let entries = std::fs::read_dir(reclaim).map_err(|source| io("read cache reclamation directory", reclaim, source))?;
  for entry in entries {
    check_cancelled(cancellation)?;
    let entry = entry.map_err(|source| io("read cache reclamation entry", reclaim, source))?;
    let path = entry.path();
    let name = entry.file_name();
    if !name.to_str().is_some_and(valid_scope_name) {
      return Err(CacheSessionError::Invalid(format!(
        "cache reclamation root contains an unexpected entry '{}'",
        path.display()
      )));
    }
    if !store.claim_reclamation(&path)? {
      continue;
    }
    let removal = (|| {
      let metadata =
        std::fs::symlink_metadata(&path).map_err(|source| io("inspect cache reclamation tombstone", &path, source))?;
      validate_existing_private_directory(&path, &metadata)?;
      let marker = access.join(&name);
      match std::fs::remove_file(&marker) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(io("remove abandoned cache access marker", &marker, source)),
      }
      remove_tree(&path, cancellation)
    })();
    store.finish_reclamation(&path)?;
    removal?;
    removed = removed.saturating_add(1);
  }
  Ok(removed)
}

fn valid_scope_name(name: &str) -> bool {
  name.len() == SCOPE_DIRECTORY_HEX_LENGTH
    && name
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Removes a tree without following symlinks and observes shutdown between
/// directory entries. A partially removed inactive scope remains recognizable
/// and is safely completed by a later reclamation pass or process restart.
fn remove_tree(path: &Path, cancellation: Option<&CancellationToken>) -> Result<(), CacheSessionError> {
  enum Frame {
    Visit(PathBuf),
    Children {
      directory: PathBuf,
      entries: Box<std::fs::ReadDir>,
    },
  }

  let mut stack = vec![Frame::Visit(path.to_owned())];
  while let Some(frame) = stack.pop() {
    check_cancelled(cancellation)?;
    match frame {
      Frame::Visit(path) => {
        let metadata = match std::fs::symlink_metadata(&path) {
          Ok(metadata) => metadata,
          Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
          Err(source) => return Err(io("inspect inactive cache entry", &path, source)),
        };
        #[cfg(windows)]
        if octacity_private_fs::is_reparse_point(&path)
          .map_err(|source| io("inspect inactive cache reparse state", &path, source))?
        {
          return Err(CacheSessionError::Invalid(format!(
            "inactive cache entry '{}' is a Windows reparse point",
            path.display()
          )));
        }
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
          let entries =
            std::fs::read_dir(&path).map_err(|source| io("read inactive cache directory", &path, source))?;
          stack.push(Frame::Children {
            directory: path,
            entries: Box::new(entries),
          });
        } else {
          remove_file_if_present(&path)?;
        }
      }
      Frame::Children { directory, mut entries } => match entries.next() {
        Some(entry) => {
          let entry = entry.map_err(|source| io("read inactive cache entry", &directory, source))?;
          stack.push(Frame::Children { directory, entries });
          stack.push(Frame::Visit(entry.path()));
        }
        None => {
          // Windows can deny directory removal while its enumeration handle is
          // open. End the traversal handle before removing the directory.
          drop(entries);
          match std::fs::remove_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io("remove inactive cache directory", &directory, source)),
          }
        }
      },
    }
  }
  Ok(())
}

fn remove_file_if_present(path: &Path) -> Result<(), CacheSessionError> {
  match std::fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(source) => Err(io("remove inactive cache entry", path, source)),
  }
}

fn check_cancelled(cancellation: Option<&CancellationToken>) -> Result<(), CacheSessionError> {
  if cancellation.is_some_and(CancellationToken::is_cancelled) {
    return Err(CacheSessionError::Cancelled);
  }
  Ok(())
}

fn join_error(error: tokio::task::JoinError) -> CacheSessionError {
  CacheSessionError::Invalid(format!("cache filesystem worker failed: {error}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn removes_a_deep_job_written_tree_without_call_stack_recursion() {
    const DEPTH: usize = 256;

    let root = tempfile::tempdir().unwrap();
    let tree = root.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    let mut directory = tree.clone();
    for _ in 0..DEPTH {
      directory.push("d");
      std::fs::create_dir(&directory).unwrap();
    }
    std::fs::write(directory.join("leaf"), b"cache").unwrap();

    remove_tree(&tree, None).unwrap();

    assert!(!tree.exists());
  }
}
