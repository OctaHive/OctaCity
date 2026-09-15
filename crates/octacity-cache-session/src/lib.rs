//! Prepares the agent-owned boundary around an Octa cache session.
//!
//! Octa remains the cache engine. This crate only narrows a fenced server
//! grant through operator policy, creates an isolated persistent L1 scope,
//! writes a short-lived bearer to a private job file, and exposes the two
//! projections needed by execution backends and the runner supervisor. It
//! never interprets action keys, metadata, or cached blobs.

#![warn(missing_docs)]

use std::{
  collections::{BTreeMap, BTreeSet},
  num::NonZeroUsize,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use octa_cache_protocol::{CacheMode, Digest, DigestAlgorithm, LocalCacheCapacity, RuntimeIdentity};
use octacity_execution::ExecutionCacheMounts;
use octacity_protocol::{BeginCacheSessionResponse, CachePolicy, NetworkPolicy, PlatformSpec, RuntimeTarget};
use octacity_runner::RunnerCacheSession;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;
use url::Url;
use zeroize::Zeroizing;

const SESSION_DIRECTORY: &str = "cache-session";
const TOKEN_FILE: &str = "token";

mod scope;
#[cfg(test)]
use scope::ACCESS_LAYOUT_DIRECTORY;
#[cfg(test)]
use scope::RECLAIM_LAYOUT_DIRECTORY;
#[cfg(test)]
use scope::SCOPE_DIRECTORY_HEX_LENGTH;
use scope::{SCOPE_LAYOUT_DIRECTORY, ScopeLease, ScopeStore};

/// Operator policy consumed by the cache-session module.
///
/// Keeping this type here prevents the runtime module from depending on the
/// agent's TOML representation. The composition root performs the small
/// mapping after configuration validation.
#[derive(Clone, Debug)]
pub struct CacheSessionManagerConfig {
  /// Existing private root that owns persistent trust-scoped L1 directories.
  pub root: PathBuf,
  /// Capacity and collection watermarks for each L1 trust scope.
  pub capacity: LocalCacheCapacity,
  /// Maximum persistent trust scopes retained below `root`.
  pub max_scopes: usize,
  /// Whether signed jobs may read cached results.
  pub allow_read: bool,
  /// Whether signed jobs may publish cached results.
  pub allow_write: bool,
  /// Canonical HTTPS origins the coordinator may select.
  pub allowed_origins: Vec<String>,
  /// Optional owner-protected custom CA file.
  pub ca_certificate_file: Option<PathBuf>,
  /// Stable Native environment identity strings keyed by `os-architecture`.
  pub native_identities: BTreeMap<String, String>,
  /// Whole-operation remote request deadline.
  pub request_timeout_seconds: u64,
  /// Per-job remote blob-transfer concurrency.
  pub max_parallel_transfers: usize,
}

/// Per-job time and cancellation boundary for session preparation.
#[derive(Clone, Debug)]
pub struct CacheSessionContext {
  /// Current Unix timestamp supplied by the lifecycle composition root.
  pub now: u64,
  /// Time remaining on the single job execution deadline.
  pub remaining_job: Duration,
  /// Job cancellation observed during potentially expensive scope admission.
  pub cancellation: CancellationToken,
}

/// Validated operator policy used to narrow every cache grant.
#[derive(Debug)]
pub struct CacheSessionManager {
  root: PathBuf,
  capacity: LocalCacheCapacity,
  aggregate_max_bytes: u64,
  allowed_mode: CacheMode,
  allowed_origins: BTreeSet<String>,
  ca_certificate_file: Option<PathBuf>,
  native_identities: BTreeMap<String, String>,
  request_timeout_seconds: u64,
  max_parallel_transfers: NonZeroUsize,
  scopes: Arc<ScopeStore>,
}

/// Result of one inactive-cache reclamation pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheReclaimReport {
  /// Free bytes on the cache filesystem before reclamation.
  pub initial_free_bytes: u64,
  /// Free bytes observed after the final removal.
  pub final_free_bytes: u64,
  /// Number of complete inactive trust scopes removed.
  pub removed_scopes: usize,
}

/// A prepared cache session whose bearer remains live until explicit revoke.
pub struct PreparedCacheSession {
  execution: ExecutionCacheMounts,
  runner: RunnerCacheSession,
  sensitive_value: Option<Zeroizing<Vec<u8>>>,
  private_directory: Option<PathBuf>,
  scope: Option<ScopeLease>,
}

impl std::fmt::Debug for PreparedCacheSession {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("PreparedCacheSession")
      .field("local_directory", &self.execution.local_directory)
      .field("remote", &self.runner.remote_endpoint.is_some())
      .finish_non_exhaustive()
  }
}

/// Invalid grants, unsupported runtime identity, or private filesystem failure.
#[derive(Debug, Error)]
pub enum CacheSessionError {
  /// Host shutdown interrupted local cache reclamation.
  #[error("cache session operation was cancelled")]
  Cancelled,
  /// Operator or server values violate the cache-session contract.
  #[error("invalid cache session: {0}")]
  Invalid(String),
  /// Private session state could not be created or removed.
  #[error("failed to {operation} cache session path '{path}': {source}")]
  Io {
    /// Stable filesystem operation.
    operation: &'static str,
    /// Affected path.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
}

impl CacheSessionManager {
  /// Validates operator policy and prepares the persistent cache root.
  pub fn new(config: CacheSessionManagerConfig) -> Result<Self, CacheSessionError> {
    if !config.root.is_absolute() || !config.root.is_dir() {
      return Err(CacheSessionError::Invalid(
        "cache root must be an existing absolute directory".to_owned(),
      ));
    }
    let root = config
      .root
      .canonicalize()
      .map_err(|source| io("canonicalize cache root", &config.root, source))?;
    octacity_private_fs::validate_trusted_directory_chain(&root)
      .map_err(|source| io("validate cache root ancestry", &root, source))?;
    octacity_private_fs::validate_private_access(&root)
      .map_err(|source| io("validate private cache root", &root, source))?;
    config
      .capacity
      .validate()
      .map_err(|error| CacheSessionError::Invalid(error.to_string()))?;
    if config.max_scopes == 0 {
      return Err(CacheSessionError::Invalid(
        "cache max_scopes must be positive".to_owned(),
      ));
    }
    let aggregate_max_bytes = u64::try_from(config.max_scopes)
      .ok()
      .and_then(|scopes| config.capacity.max_bytes.checked_mul(scopes))
      .ok_or_else(|| CacheSessionError::Invalid("aggregate cache capacity is not representable".to_owned()))?;
    let allowed_mode = match (config.allow_read, config.allow_write) {
      (true, true) => CacheMode::ReadWrite,
      (true, false) => CacheMode::ReadOnly,
      (false, true) => CacheMode::WriteOnly,
      (false, false) => {
        return Err(CacheSessionError::Invalid(
          "cache policy must allow reading, writing, or both".to_owned(),
        ));
      }
    };
    if config.request_timeout_seconds == 0
      || config.request_timeout_seconds > octa_cache_protocol::MAX_REMOTE_CACHE_REQUEST_TIMEOUT_SECONDS
    {
      return Err(CacheSessionError::Invalid(
        "cache request timeout is outside Octa's supported bounds".to_owned(),
      ));
    }
    let max_parallel_transfers = NonZeroUsize::new(config.max_parallel_transfers)
      .ok_or_else(|| CacheSessionError::Invalid("cache transfer limit must be positive".to_owned()))?;
    if max_parallel_transfers.get() > octa_cache_protocol::MAX_REMOTE_CACHE_PARALLEL_TRANSFERS {
      return Err(CacheSessionError::Invalid(
        "cache transfer concurrency exceeds Octa's supported bound".to_owned(),
      ));
    }
    let mut allowed_origins = BTreeSet::new();
    for value in config.allowed_origins {
      let origin = canonical_origin(&value)?;
      if !allowed_origins.insert(origin) {
        return Err(CacheSessionError::Invalid(
          "cache remote origins contain a duplicate or equivalent origin".to_owned(),
        ));
      }
    }
    let ca_certificate_file = config.ca_certificate_file.map(validate_ca_certificate).transpose()?;
    for (platform, identity) in &config.native_identities {
      let parsed = platform.parse::<PlatformSpec>().map_err(CacheSessionError::Invalid)?;
      RuntimeIdentity::native(parsed.os.into(), parsed.architecture.into(), identity)
        .map_err(|error| CacheSessionError::Invalid(format!("Native identity '{platform}': {error}")))?;
    }
    let scopes = ScopeStore::new(root.clone(), config.max_scopes)?;
    Ok(Self {
      root,
      capacity: config.capacity,
      aggregate_max_bytes,
      allowed_mode,
      allowed_origins,
      ca_certificate_file,
      native_identities: config.native_identities,
      request_timeout_seconds: config.request_timeout_seconds,
      max_parallel_transfers,
      scopes,
    })
  }

  /// Materializes one grant without placing bearer bytes in a serialized DTO.
  pub async fn prepare(
    &self,
    policy: &CachePolicy,
    grant: BeginCacheSessionResponse,
    runtime: &RuntimeTarget,
    network: &NetworkPolicy,
    job_root: &Path,
    context: CacheSessionContext,
  ) -> Result<PreparedCacheSession, CacheSessionError> {
    if context.cancellation.is_cancelled() {
      return Err(CacheSessionError::Cancelled);
    }
    let mode = policy.mode().map_err(CacheSessionError::Invalid)?;
    if (mode.can_read() && !self.allowed_mode.can_read()) || (mode.can_write() && !self.allowed_mode.can_write()) {
      return Err(CacheSessionError::Invalid(
        "signed cache access exceeds operator-authorized read/write permissions".to_owned(),
      ));
    }
    let runtime = runtime_identity(runtime, &self.native_identities)?;
    // Validate all server-controlled values before consuming one of the
    // persistent scope slots. A malformed grant must not fill the bounded L1
    // scope set with directories that no job can use.
    let remote = match (network, grant.remote) {
      // A network-disabled job still uses its persistent L1. The server grant
      // is deliberately narrowed rather than weakening the signed job policy.
      (NetworkPolicy::Disabled, _) | (_, None) => None,
      (network, Some(mut remote)) => {
        let endpoint = validate_remote(&remote.endpoint, &self.allowed_origins)?;
        authorize_remote_network(network, &endpoint)?;
        if remote.bearer_token.is_empty()
          || remote.bearer_token.len() > octacity_protocol::MAX_CACHE_CREDENTIAL_BYTES
          || remote.bearer_token.chars().any(char::is_control)
        {
          return Err(CacheSessionError::Invalid(
            "remote cache bearer credential is empty, oversized, or contains control characters".to_owned(),
          ));
        }
        let remaining_seconds = context
          .remaining_job
          .as_secs()
          .checked_add(u64::from(context.remaining_job.subsec_nanos() != 0))
          .ok_or_else(|| CacheSessionError::Invalid("remaining cache lifetime overflows seconds".to_owned()))?;
        let required_until = context
          .now
          .checked_add(remaining_seconds)
          .and_then(|deadline| deadline.checked_add(self.request_timeout_seconds))
          .ok_or_else(|| CacheSessionError::Invalid("cache credential lifetime overflows Unix time".to_owned()))?;
        if remote.expires_at <= required_until {
          return Err(CacheSessionError::Invalid(
            "remote cache credential expires before the job and one final request can complete".to_owned(),
          ));
        }
        let bearer = Zeroizing::new(std::mem::take(&mut remote.bearer_token).into_bytes());
        Some((remote.endpoint.clone(), bearer))
      }
    };
    let scope = self
      .prepare_local_directory(&grant.scope_id, &runtime, context.cancellation.clone())
      .await?;
    if context.cancellation.is_cancelled() {
      return Err(CacheSessionError::Cancelled);
    }
    let local_directory = scope.directory.clone();
    let (remote_endpoint, token_file, private_directory, sensitive_value) = match remote {
      Some((endpoint, secret)) => {
        let directory = job_root.join(SESSION_DIRECTORY);
        create_private_path(&directory, true)?;
        let destination = directory.join(TOKEN_FILE);
        write_private_file(&destination, &secret).await?;
        (Some(endpoint), Some(destination), Some(directory), Some(secret))
      }
      None => (None, None, None, None),
    };

    Ok(PreparedCacheSession {
      execution: ExecutionCacheMounts {
        capacity_root: self.root.clone(),
        local_directory,
        local_capacity: self.capacity,
        aggregate_max_bytes: self.aggregate_max_bytes,
        token_file,
        ca_certificate_file: remote_endpoint.as_ref().and(self.ca_certificate_file.clone()),
      },
      runner: RunnerCacheSession {
        mode,
        namespace: policy.namespace.clone(),
        local_capacity: self.capacity,
        runtime,
        remote_endpoint,
        request_timeout_seconds: self.request_timeout_seconds,
        max_parallel_transfers: self.max_parallel_transfers,
      },
      sensitive_value,
      private_directory,
      scope: Some(scope),
    })
  }

  /// Removes at most one inactive LRU scope when the cache filesystem is below
  /// the configured free-space reserve.
  ///
  /// A scope is eligible only when its directory name has the exact internal
  /// digest shape, its path is a private regular directory, and no prepared
  /// session holds an in-memory lease. Foreign entries are never deleted.
  pub async fn reclaim_inactive(
    &self,
    minimum_free_bytes: u64,
    cancellation: CancellationToken,
  ) -> Result<CacheReclaimReport, CacheSessionError> {
    self.scopes.reclaim_one(minimum_free_bytes, cancellation).await
  }

  fn local_directory(&self, scope: &str, runtime: &RuntimeIdentity) -> Result<PathBuf, CacheSessionError> {
    if scope.is_empty()
      || scope.len() > octacity_protocol::MAX_COORDINATOR_IDENTIFIER_BYTES
      || scope.chars().any(char::is_control)
    {
      return Err(CacheSessionError::Invalid("cache scope identity is invalid".to_owned()));
    }
    // This digest chooses a private physical directory; it is not an action
    // identity or integrity proof. A future RuntimeIdentity wire change may
    // therefore cause a safe cold L1, but can never turn different runtimes
    // into a cache hit. The leading protocol-layout directory versions that
    // physical representation independently from Octa's action-key format.
    let encoded = serde_json::to_vec(runtime)
      .map_err(|error| CacheSessionError::Invalid(format!("runtime identity cannot be encoded: {error}")))?;
    let mut digest = Sha256::new();
    digest.update(scope.as_bytes());
    digest.update([0]);
    digest.update(encoded);
    Ok(
      self
        .root
        .join(SCOPE_LAYOUT_DIRECTORY)
        .join(format!("{:x}", digest.finalize())),
    )
  }

  /// Acquires one persistent trust scope through the store that owns process
  /// exclusion, the scope limit, and inactive LRU reclamation.
  async fn prepare_local_directory(
    &self,
    scope: &str,
    runtime: &RuntimeIdentity,
    cancellation: CancellationToken,
  ) -> Result<ScopeLease, CacheSessionError> {
    let directory = self.local_directory(scope, runtime)?;
    self.scopes.acquire(directory, cancellation).await
  }
}

impl PreparedCacheSession {
  /// Returns canonical host paths for the selected execution backend.
  pub fn execution(&self) -> &ExecutionCacheMounts {
    &self.execution
  }

  /// Returns semantic runner policy without any bearer value.
  pub fn runner(&self) -> &RunnerCacheSession {
    &self.runner
  }

  /// Returns bearer bytes solely for runner diagnostic redaction.
  pub fn sensitive_value(&self) -> Option<&[u8]> {
    self.sensitive_value.as_deref().map(Vec::as_slice)
  }

  /// Removes the private bearer before output collection and workspace cleanup.
  pub async fn revoke(mut self) -> Result<(), CacheSessionError> {
    if let Some(directory) = self.private_directory.take() {
      match fs::remove_dir_all(&directory).await {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(io("remove private directory", &directory, source)),
      }
    }
    self.sensitive_value.take();
    self.scope.take();
    Ok(())
  }
}

fn runtime_identity(
  runtime: &RuntimeTarget,
  native: &BTreeMap<String, String>,
) -> Result<RuntimeIdentity, CacheSessionError> {
  match runtime {
    RuntimeTarget::Native { platform } => {
      let key = platform.to_string();
      let identity = native.get(&key).ok_or_else(|| {
        CacheSessionError::Invalid(format!("Native cache environment identity '{key}' is not configured"))
      })?;
      RuntimeIdentity::native(platform.os.into(), platform.architecture.into(), identity)
        .map_err(|error| CacheSessionError::Invalid(error.to_string()))
    }
    RuntimeTarget::Oci { platform, image, .. } => {
      let (_, digest) = image
        .rsplit_once("@sha256:")
        .ok_or_else(|| CacheSessionError::Invalid("OCI cache runtime requires an immutable image".to_owned()))?;
      Ok(RuntimeIdentity::Oci {
        os: platform.os.into(),
        architecture: platform.architecture.into(),
        image: Digest::from_hex(DigestAlgorithm::Sha256, digest, 0)
          .map_err(|error| CacheSessionError::Invalid(error.to_string()))?,
      })
    }
  }
}

fn validate_remote(endpoint: &str, allowed_origins: &BTreeSet<String>) -> Result<Url, CacheSessionError> {
  let url = Url::parse(endpoint)
    .map_err(|error| CacheSessionError::Invalid(format!("invalid remote cache endpoint: {error}")))?;
  if url.scheme() != "https"
    || !url.username().is_empty()
    || url.password().is_some()
    || url.query().is_some()
    || url.fragment().is_some()
  {
    return Err(CacheSessionError::Invalid(
      "remote cache endpoint must be an HTTPS URL without credentials, query, or fragment".to_owned(),
    ));
  }
  let origin = url.origin().ascii_serialization();
  if !allowed_origins.contains(&origin) {
    return Err(CacheSessionError::Invalid(format!(
      "remote cache origin '{origin}' is not allowed"
    )));
  }
  Ok(url)
}

fn canonical_origin(value: &str) -> Result<String, CacheSessionError> {
  let url = Url::parse(value).map_err(|error| CacheSessionError::Invalid(format!("invalid cache origin: {error}")))?;
  if url.scheme() != "https"
    || !url.username().is_empty()
    || url.password().is_some()
    || url.path() != "/"
    || url.query().is_some()
    || url.fragment().is_some()
  {
    return Err(CacheSessionError::Invalid(
      "cache origin must contain only an HTTPS scheme and authority".to_owned(),
    ));
  }
  Ok(url.origin().ascii_serialization())
}

fn validate_ca_certificate(path: PathBuf) -> Result<PathBuf, CacheSessionError> {
  if !path.is_absolute() {
    return Err(CacheSessionError::Invalid(
      "cache CA certificate must be an absolute regular file".to_owned(),
    ));
  }
  let canonical = path
    .canonicalize()
    .map_err(|source| io("canonicalize cache CA certificate", &path, source))?;
  let metadata =
    std::fs::symlink_metadata(&canonical).map_err(|source| io("inspect cache CA certificate", &canonical, source))?;
  if !metadata.file_type().is_file() || canonical != path {
    return Err(CacheSessionError::Invalid(
      "cache CA certificate must be a canonical regular file".to_owned(),
    ));
  }
  octacity_private_fs::validate_trusted_owner(&canonical)
    .map_err(|source| io("validate cache CA owner", &canonical, source))?;
  octacity_private_fs::validate_private_access(&canonical)
    .map_err(|source| io("validate cache CA access", &canonical, source))?;
  let parent = canonical
    .parent()
    .ok_or_else(|| CacheSessionError::Invalid("cache CA certificate has no parent".to_owned()))?;
  octacity_private_fs::validate_trusted_directory_chain(parent)
    .map_err(|source| io("validate cache CA ancestry", parent, source))?;
  Ok(canonical)
}

fn authorize_remote_network(network: &NetworkPolicy, endpoint: &Url) -> Result<(), CacheSessionError> {
  match network {
    NetworkPolicy::Unrestricted => Ok(()),
    NetworkPolicy::Disabled => Err(CacheSessionError::Invalid(
      "a network-disabled job cannot use a remote cache".to_owned(),
    )),
    NetworkPolicy::Restricted { allowed_hosts } => {
      let host = endpoint
        .host_str()
        .ok_or_else(|| CacheSessionError::Invalid("remote cache endpoint has no host".to_owned()))?;
      if allowed_hosts.iter().any(|allowed| allowed == host) {
        Ok(())
      } else {
        Err(CacheSessionError::Invalid(format!(
          "remote cache host '{host}' is absent from the signed network allowlist"
        )))
      }
    }
  }
}

fn create_private_path(path: &Path, exclusive: bool) -> Result<(), CacheSessionError> {
  if !exclusive {
    match std::fs::symlink_metadata(path) {
      Ok(metadata) => return validate_existing_private_directory(path, &metadata),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(source) => return Err(io("inspect private directory", path, source)),
    }
  }
  match octacity_private_fs::create_private_directory(path) {
    Ok(()) => Ok(()),
    Err(source) if !exclusive && source.kind() == std::io::ErrorKind::AlreadyExists => {
      let metadata = std::fs::symlink_metadata(path)
        .map_err(|source| io("inspect concurrently created private directory", path, source))?;
      validate_existing_private_directory(path, &metadata)
    }
    Err(source) => Err(io("create private directory", path, source)),
  }
}

pub(crate) fn validate_existing_private_directory(
  path: &Path,
  metadata: &std::fs::Metadata,
) -> Result<(), CacheSessionError> {
  validate_directory_entry(path, metadata)?;
  octacity_private_fs::validate_private_access(path).map_err(|source| io("validate private directory", path, source))
}

/// Rejects every directory entry capable of redirecting a later traversal.
/// Windows junctions are reparse points but are not consistently exposed as
/// Rust symbolic links, so the platform-specific check is mandatory there.
fn validate_directory_entry(path: &Path, metadata: &std::fs::Metadata) -> Result<(), CacheSessionError> {
  let redirects = metadata.file_type().is_symlink() || platform_redirects(path)?;
  if !metadata.is_dir() || redirects {
    return Err(CacheSessionError::Invalid(format!(
      "cache path '{}' is not a regular non-redirecting directory",
      path.display()
    )));
  }
  Ok(())
}

#[cfg(windows)]
fn platform_redirects(path: &Path) -> Result<bool, CacheSessionError> {
  octacity_private_fs::is_reparse_point(path)
    .map_err(|source| io("inspect private directory reparse state", path, source))
}

#[cfg(not(windows))]
fn platform_redirects(_path: &Path) -> Result<bool, CacheSessionError> {
  Ok(false)
}

async fn write_private_file(path: &Path, value: &[u8]) -> Result<(), CacheSessionError> {
  let mut options = fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  options.mode(0o600);
  let mut file = options
    .open(path)
    .await
    .map_err(|source| io("create bearer", path, source))?;
  file
    .write_all(value)
    .await
    .map_err(|source| io("write bearer", path, source))?;
  file.sync_all().await.map_err(|source| io("sync bearer", path, source))
}

fn io(operation: &'static str, path: &Path, source: std::io::Error) -> CacheSessionError {
  CacheSessionError::Io {
    operation,
    path: path.to_owned(),
    source,
  }
}

#[cfg(test)]
mod tests;
