//! Provisions operator-owned workload identity into one private job boundary.
//!
//! The control plane selects only a logical profile name. A provider resolves
//! that name locally, creates a short-lived job copy, and returns a lease whose
//! explicit revocation removes the copy after the execution backend has been
//! destroyed. Secret values and host source paths never cross the server-agent
//! protocol; orchestration sees only the job-private copy returned here.

#![warn(missing_docs)]

use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};

use async_trait::async_trait;
use thiserror::Error;
use tokio::{
  fs,
  io::{AsyncReadExt as _, AsyncWriteExt as _},
};
use tokio_util::sync::CancellationToken;
use tracing::warn;
use zeroize::Zeroizing;

/// Maximum accepted identity document size.
pub const MAX_WORKLOAD_IDENTITY_BYTES: u64 = 1024 * 1024;
const IDENTITY_DIRECTORY: &str = "identity";
const IDENTITY_FILE: &str = "token";

/// Failure while selecting, materializing, or revoking workload identity.
#[derive(Debug, Error)]
pub enum WorkloadIdentityError {
  /// The signed job selected no locally configured profile.
  #[error("workload identity profile '{0}' is not configured")]
  UnknownProfile(String),
  /// An operator-owned source ceased to be a bounded regular file.
  #[error("workload identity profile '{profile}' has an invalid source: {reason}")]
  InvalidSource {
    /// Logical profile selected by the signed job.
    profile: String,
    /// Validation failure without secret contents.
    reason: String,
  },
  /// Local identity filesystem I/O failed.
  #[error("failed to {operation} workload identity '{path}': {source}")]
  Io {
    /// Bounded operation name.
    operation: &'static str,
    /// Operator or job-owned path involved in the operation.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// Cancellation won before the identity became available to the backend.
  #[error("workload identity provisioning was cancelled")]
  Cancelled,
  /// The blocking, no-follow source-open task failed unexpectedly.
  #[error("workload identity source-open task failed: {0}")]
  OpenTask(#[source] tokio::task::JoinError),
  /// Provisioning failed and its partial private copy could not be removed.
  #[error("workload identity failed ({operation}) and cleanup also failed: {cleanup}")]
  OperationAndCleanup {
    /// Original provisioning failure.
    operation: Box<WorkloadIdentityError>,
    /// Cleanup failure for the partial job copy.
    cleanup: std::io::Error,
  },
}

/// One job-owned identity file that must be revoked explicitly.
pub struct WorkloadIdentityLease {
  path: PathBuf,
  value: Zeroizing<Vec<u8>>,
  active: bool,
}

impl WorkloadIdentityLease {
  /// Returns the host file to expose read-only inside the execution boundary.
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Returns the exact in-memory value for supervisor-boundary redaction.
  ///
  /// Callers must neither log nor persist this slice. It exists solely so the
  /// process supervising Octa can remove an accidentally printed identity
  /// before events or results enter durable agent state.
  pub fn sensitive_value(&self) -> &[u8] {
    &self.value
  }

  /// Removes the complete job-private identity directory.
  pub async fn revoke(mut self) -> Result<(), WorkloadIdentityError> {
    let directory = self.path.parent().expect("identity lease always has a parent");
    match fs::remove_dir_all(directory).await {
      Ok(()) => {
        self.active = false;
        Ok(())
      }
      Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
        self.active = false;
        Ok(())
      }
      Err(source) => Err(WorkloadIdentityError::Io {
        operation: "revoke",
        path: directory.to_owned(),
        source,
      }),
    }
  }
}

impl Drop for WorkloadIdentityLease {
  fn drop(&mut self) {
    if self.active {
      warn!(path = %self.path.display(), "workload identity lease dropped before revocation");
    }
  }
}

impl std::fmt::Debug for WorkloadIdentityLease {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("WorkloadIdentityLease")
      .field("path", &self.path)
      .field("value", &"[redacted]")
      .field("active", &self.active)
      .finish()
  }
}

/// Local port used by job orchestration to provision an identity profile.
#[async_trait]
pub trait WorkloadIdentityProvider: Send + Sync {
  /// Reports whether a signed profile can be resolved before source acquisition.
  fn supports(&self, profile: &str) -> bool;

  /// Creates one private identity copy below `job_root`.
  async fn provision(
    &self,
    profile: &str,
    job_root: &Path,
    cancellation: &CancellationToken,
  ) -> Result<WorkloadIdentityLease, WorkloadIdentityError>;
}

/// Provider that snapshots restricted operator-owned token files per job.
///
/// External OIDC, SPIFFE, or platform identity agents may rotate each source
/// file. The provider therefore reopens it for every job while configuration
/// validation remains responsible for protecting its parent path.
#[derive(Clone, Debug, Default)]
pub struct FileWorkloadIdentityProvider {
  profiles: BTreeMap<String, PathBuf>,
}

impl FileWorkloadIdentityProvider {
  /// Constructs a provider from already validated, canonical source files.
  pub fn new(profiles: BTreeMap<String, PathBuf>) -> Self {
    Self { profiles }
  }
}

#[async_trait]
impl WorkloadIdentityProvider for FileWorkloadIdentityProvider {
  fn supports(&self, profile: &str) -> bool {
    self.profiles.contains_key(profile)
  }

  async fn provision(
    &self,
    profile: &str,
    job_root: &Path,
    cancellation: &CancellationToken,
  ) -> Result<WorkloadIdentityLease, WorkloadIdentityError> {
    let source = self
      .profiles
      .get(profile)
      .ok_or_else(|| WorkloadIdentityError::UnknownProfile(profile.to_owned()))?;
    let input = open_source(source, cancellation).await?;
    let size = input
      .metadata()
      .map_err(|error| io("inspect opened source", source, error))?
      .len();
    if size == 0 || size > MAX_WORKLOAD_IDENTITY_BYTES {
      return Err(WorkloadIdentityError::InvalidSource {
        profile: profile.to_owned(),
        reason: format!("must be a non-empty regular file no larger than {MAX_WORKLOAD_IDENTITY_BYTES} bytes"),
      });
    }
    if cancellation.is_cancelled() {
      return Err(WorkloadIdentityError::Cancelled);
    }

    let directory = job_root.join(IDENTITY_DIRECTORY);
    create_private_directory(&directory).await?;
    let destination = directory.join(IDENTITY_FILE);
    let value = match copy_bounded(profile, input, size, &destination, cancellation).await {
      Ok(value) => value,
      Err(operation) => {
        return match fs::remove_dir_all(&directory).await {
          Ok(()) => Err(operation),
          Err(cleanup) => Err(WorkloadIdentityError::OperationAndCleanup {
            operation: Box::new(operation),
            cleanup,
          }),
        };
      }
    };
    Ok(WorkloadIdentityLease {
      path: destination,
      value,
      active: true,
    })
  }
}

async fn copy_bounded(
  profile: &str,
  input: std::fs::File,
  expected_size: u64,
  destination: &Path,
  cancellation: &CancellationToken,
) -> Result<Zeroizing<Vec<u8>>, WorkloadIdentityError> {
  let mut input = fs::File::from_std(input);
  let mut options = fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    options.mode(0o600);
  }
  let mut output = options
    .open(destination)
    .await
    .map_err(|error| io("create job copy", destination, error))?;
  let mut value = Zeroizing::new(Vec::with_capacity(expected_size as usize));
  let mut buffer = Zeroizing::new([0_u8; 8192]);
  loop {
    let read = tokio::select! {
      biased;
      _ = cancellation.cancelled() => return Err(WorkloadIdentityError::Cancelled),
      result = input.read(&mut buffer[..]) => result.map_err(|error| io("read", destination, error))?,
    };
    if read == 0 {
      break;
    }
    if value.len().saturating_add(read) > MAX_WORKLOAD_IDENTITY_BYTES as usize {
      return Err(WorkloadIdentityError::InvalidSource {
        profile: profile.to_owned(),
        reason: format!("grew beyond {MAX_WORKLOAD_IDENTITY_BYTES} bytes while reading"),
      });
    }
    output
      .write_all(&buffer[..read])
      .await
      .map_err(|error| io("copy", destination, error))?;
    value.extend_from_slice(&buffer[..read]);
  }
  if value.len() as u64 != expected_size {
    return Err(WorkloadIdentityError::InvalidSource {
      profile: profile.to_owned(),
      reason: format!(
        "changed size while reading; expected {expected_size} bytes and copied {}",
        value.len()
      ),
    });
  }
  output
    .flush()
    .await
    .map_err(|error| io("flush job copy", destination, error))?;
  output
    .sync_all()
    .await
    .map_err(|error| io("sync job copy", destination, error))?;
  Ok(value)
}

async fn open_source(source: &Path, cancellation: &CancellationToken) -> Result<std::fs::File, WorkloadIdentityError> {
  let source_path = source.to_owned();
  let mut opening = tokio::task::spawn_blocking(move || octacity_private_fs::open_regular_file_no_follow(&source_path));
  tokio::select! {
    biased;
    _ = cancellation.cancelled() => Err(WorkloadIdentityError::Cancelled),
    result = &mut opening => result.map_err(WorkloadIdentityError::OpenTask)?.map_err(|error| io("open", source, error)),
  }
}

async fn create_private_directory(path: &Path) -> Result<(), WorkloadIdentityError> {
  octacity_private_fs::create_private_directory(path).map_err(|error| io("create directory", path, error))
}

fn io(operation: &'static str, path: &Path, source: std::io::Error) -> WorkloadIdentityError {
  WorkloadIdentityError::Io {
    operation,
    path: path.to_owned(),
    source,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn provisions_bounded_private_copy_and_revokes_it() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    std::fs::write(&source, "signed-jwt").unwrap();
    let job_root = temporary.path().join("job");
    std::fs::create_dir(&job_root).unwrap();
    let provider = FileWorkloadIdentityProvider::new(BTreeMap::from([("ci".to_owned(), source)]));

    let lease = provider
      .provision("ci", &job_root, &CancellationToken::new())
      .await
      .unwrap();
    assert_eq!(std::fs::read_to_string(lease.path()).unwrap(), "signed-jwt");
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      assert_eq!(
        std::fs::metadata(lease.path()).unwrap().permissions().mode() & 0o777,
        0o600
      );
    }
    let identity_directory = lease.path().parent().unwrap().to_owned();
    lease.revoke().await.unwrap();
    assert!(!identity_directory.exists());
  }

  #[tokio::test]
  async fn rejects_unknown_empty_oversized_and_cancelled_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let job_root = temporary.path().join("job");
    std::fs::create_dir(&job_root).unwrap();
    let empty = temporary.path().join("empty");
    std::fs::write(&empty, []).unwrap();
    let provider = FileWorkloadIdentityProvider::new(BTreeMap::from([("ci".to_owned(), empty)]));
    assert!(!provider.supports("unknown"));
    assert!(matches!(
      provider
        .provision("unknown", &job_root, &CancellationToken::new())
        .await,
      Err(WorkloadIdentityError::UnknownProfile(_))
    ));
    assert!(matches!(
      provider.provision("ci", &job_root, &CancellationToken::new()).await,
      Err(WorkloadIdentityError::InvalidSource { .. })
    ));

    let oversized = temporary.path().join("oversized");
    std::fs::File::create(&oversized)
      .unwrap()
      .set_len(MAX_WORKLOAD_IDENTITY_BYTES + 1)
      .unwrap();
    let provider = FileWorkloadIdentityProvider::new(BTreeMap::from([("ci".to_owned(), oversized)]));
    assert!(matches!(
      provider.provision("ci", &job_root, &CancellationToken::new()).await,
      Err(WorkloadIdentityError::InvalidSource { .. })
    ));

    let source = temporary.path().join("source");
    std::fs::write(&source, "token").unwrap();
    let provider = FileWorkloadIdentityProvider::new(BTreeMap::from([("ci".to_owned(), source)]));
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
      provider.provision("ci", &job_root, &cancellation).await,
      Err(WorkloadIdentityError::Cancelled)
    ));
  }

  #[tokio::test]
  async fn rejects_an_identity_truncated_after_initial_inspection() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    std::fs::write(&source, "short").unwrap();
    let input = std::fs::File::open(&source).unwrap();
    let destination = temporary.path().join("copy");

    assert!(matches!(
      copy_bounded(
        "ci",
        input,
        32,
        &destination,
        &CancellationToken::new()
      )
      .await,
      Err(WorkloadIdentityError::InvalidSource { reason, .. }) if reason.contains("expected 32 bytes")
    ));
  }
}
