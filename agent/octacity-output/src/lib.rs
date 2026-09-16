//! Freezes, validates, and publishes runner-declared artifacts and reports.
//!
//! The module reads declarations only after the execution backend is gone. It
//! snapshots every output into agent-owned staging, computes SHA-256 over the
//! exact upload bytes, then uses the coordinator's fenced begin/complete flow.
//! Object-store credentials and bucket details remain server-side.

#![warn(missing_docs)]

mod prepare;
mod snapshot;
#[cfg(test)]
mod tests;

use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use async_trait::async_trait;
use octacity_coordinator::{CoordinatorError, OutputUploadCoordinator, Registration, RetryPolicy};
use octacity_protocol::{
  BeginOutputUploadRequest, COORDINATOR_PROTOCOL_VERSION, CompleteOutputUploadRequest, LeaseAssignment, LeaseFence,
  OutputLimits,
};
use reqwest::{
  StatusCode, Url,
  header::{
    AUTHORIZATION, CONTENT_LENGTH, COOKIE, HOST, HeaderName, HeaderValue, PROXY_AUTHORIZATION, TRANSFER_ENCODING,
  },
};
use thiserror::Error;
use tokio::{
  fs::File,
  time::{Instant, sleep, timeout_at},
};
use tokio_util::{io::ReaderStream, sync::CancellationToken};
use uuid::Uuid;

use prepare::PreparedOutput;

/// Mutable workspace inputs consumed only by the output-freezing phase.
pub struct FreezeOutputs<'a> {
  /// Frozen host workspace retained by `octacity-job`.
  pub workspace: &'a Path,
  /// Opaque runner results containing task-level declarations.
  pub results: &'a [serde_json::Value],
  /// Signed count and aggregate-byte limits.
  pub limits: &'a OutputLimits,
  /// Agent-owned directory for immutable upload snapshots.
  pub staging_root: &'a Path,
}

/// Opaque immutable snapshots produced before the lifecycle enters Uploading.
///
/// The concrete paths and metadata remain private so orchestration cannot
/// mutate or reinterpret bytes after validation. Implementations that have no
/// output state may return [`FrozenOutputs::empty`].
pub struct FrozenOutputs {
  outputs: Vec<PreparedOutput>,
  staging_root: Option<PathBuf>,
}

impl FrozenOutputs {
  /// Creates an empty frozen set for publishers with no filesystem outputs.
  pub fn empty() -> Self {
    Self {
      outputs: Vec::new(),
      staging_root: None,
    }
  }

  fn prepared(outputs: Vec<PreparedOutput>, staging_root: PathBuf) -> Self {
    Self {
      outputs,
      staging_root: Some(staging_root),
    }
  }
}

/// Fenced coordinator inputs consumed only by the upload phase.
pub struct PublishOutputs<'a> {
  /// Current registration epoch.
  pub registration: &'a Registration,
  /// Current fenced lease.
  pub lease: &'a LeaseAssignment,
  /// Immutable snapshots created by the preceding freeze phase.
  pub frozen: FrozenOutputs,
}

/// Failure while freezing or publishing job outputs.
///
/// Job-local validation, I/O, and upload failures become a terminal
/// `InfrastructureFailed` result. Lease loss, lifecycle durability failures,
/// and unconfirmed staging cleanup remain non-terminal lifecycle errors.
#[derive(Debug, Error)]
pub enum OutputError {
  /// Declarations, paths, entry types, stability, or quotas are invalid.
  #[error("invalid job output: {0}")]
  Invalid(String),
  /// Snapshot filesystem I/O failed.
  #[error("failed to {operation} output '{path}': {source}")]
  Io {
    /// Bounded operation name.
    operation: &'static str,
    /// Workspace or staging path involved.
    path: PathBuf,
    /// Underlying error.
    source: std::io::Error,
  },
  /// Fenced upload authorization or publication failed.
  #[error("coordinator output operation failed: {0}")]
  Coordinator(#[from] CoordinatorError),
  /// Direct upload transport failed.
  #[error("output upload failed: {0}")]
  Upload(String),
  /// Cancellation won before all objects were published.
  #[error("output publication was cancelled")]
  Cancelled,
  /// Publication failed and immutable staging could not be removed.
  #[error("output publication failed ({operation}) and staging cleanup also failed: {cleanup}")]
  OperationAndCleanup {
    /// Original preparation or upload failure.
    operation: Box<OutputError>,
    /// Cleanup failure for the private staging directory.
    cleanup: std::io::Error,
  },
  /// Immutable staging could not be removed after publication.
  #[error("failed to remove output staging '{path}': {source}")]
  Cleanup {
    /// Agent-owned staging path that remains on disk.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
}

/// Lifecycle port with an explicit freeze-to-upload boundary.
#[async_trait]
pub trait OutputPublisher: Send + Sync {
  /// Validates declarations and snapshots their exact bytes before Uploading.
  async fn freeze(
    &self,
    request: FreezeOutputs<'_>,
    cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError>;

  /// Publishes a previously frozen set before the workspace can be removed.
  async fn publish(&self, request: PublishOutputs<'_>, cancellation: CancellationToken) -> Result<(), OutputError>;
}

/// Explicit policy for presigned single-PUT publication.
#[derive(Clone, Debug)]
pub struct PresignedOutputPublisherConfig {
  /// Exact HTTPS origins allowed for server-issued upload URLs.
  pub allowed_origins: Vec<String>,
  /// Bound on entries traversed across one directory artifact.
  pub max_archive_entries: usize,
  /// Wall-clock limit for each direct object-store PUT.
  pub upload_timeout: Duration,
  /// Retry policy for replayable PUT requests.
  pub retry: RetryPolicy,
}

/// Publisher that uploads immutable snapshots to server-presigned targets.
pub struct PresignedOutputPublisher {
  coordinator: Arc<dyn OutputUploadCoordinator>,
  client: reqwest::Client,
  allowed_origins: BTreeSet<String>,
  max_archive_entries: usize,
  upload_timeout: Duration,
  retry: RetryPolicy,
}

impl PresignedOutputPublisher {
  /// Validates policy and creates a credential-free upload client.
  pub fn new(
    coordinator: Arc<dyn OutputUploadCoordinator>,
    config: PresignedOutputPublisherConfig,
  ) -> Result<Self, OutputError> {
    if config.allowed_origins.is_empty() || config.max_archive_entries == 0 || config.upload_timeout.is_zero() {
      return Err(OutputError::Invalid(
        "upload origins, archive entry limit, and upload timeout must be configured".to_owned(),
      ));
    }
    config.retry.validate()?;
    let allowed_origins = config
      .allowed_origins
      .iter()
      .map(|origin| canonical_origin(origin))
      .collect::<Result<BTreeSet<_>, _>>()?;
    if allowed_origins.len() != config.allowed_origins.len() {
      return Err(OutputError::Invalid(
        "upload origins must not contain duplicates".to_owned(),
      ));
    }
    let client = reqwest::Client::builder()
      .connect_timeout(config.upload_timeout)
      // A redirect could move immutable job bytes and signed headers to an
      // origin that the operator never allowed. Presigned targets are exact.
      .redirect(reqwest::redirect::Policy::none())
      .build()
      .map_err(|error| OutputError::Upload(error.to_string()))?;
    Ok(Self {
      coordinator,
      client,
      allowed_origins,
      max_archive_entries: config.max_archive_entries,
      upload_timeout: config.upload_timeout,
      retry: config.retry,
    })
  }

  async fn upload(
    &self,
    target: &octacity_protocol::BeginOutputUploadResponse,
    output: &PreparedOutput,
    cancellation: &CancellationToken,
  ) -> Result<(), OutputError> {
    let url = Url::parse(&target.put_url)
      .map_err(|_| OutputError::Invalid("server returned an invalid upload URL".to_owned()))?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
      return Err(OutputError::Invalid(
        "server returned an upload URL containing user information or a fragment".to_owned(),
      ));
    }
    if !self.allowed_origins.contains(&url.origin().ascii_serialization()) {
      return Err(OutputError::Invalid(
        "server returned an upload URL outside allowed origins".to_owned(),
      ));
    }
    let headers = target
      .required_headers
      .iter()
      .map(|(name, value)| {
        let name =
          HeaderName::try_from(name).map_err(|_| OutputError::Invalid("upload header name is invalid".to_owned()))?;
        if matches!(
          name,
          HOST | CONTENT_LENGTH | TRANSFER_ENCODING | AUTHORIZATION | COOKIE | PROXY_AUTHORIZATION
        ) {
          return Err(OutputError::Invalid(
            "server returned a transport-owned or credential-bearing upload header".to_owned(),
          ));
        }
        let value = HeaderValue::try_from(value)
          .map_err(|_| OutputError::Invalid("upload header value is invalid".to_owned()))?;
        Ok((name, value))
      })
      .collect::<Result<Vec<_>, OutputError>>()?;

    let target_deadline = upload_target_deadline(target.expires_at)?;
    let mut last = None;
    for attempt in 1..=self.retry.max_attempts {
      if Instant::now() >= target_deadline {
        return Err(OutputError::Upload("presigned upload target expired".to_owned()));
      }
      let file = File::open(output.path()).await.map_err(|source| OutputError::Io {
        operation: "open staged upload",
        path: output.path().to_owned(),
        source,
      })?;
      let mut request = self
        .client
        .put(url.clone())
        .header(CONTENT_LENGTH, output.metadata().size_bytes)
        .body(reqwest::Body::wrap_stream(ReaderStream::new(file)));
      for (name, value) in &headers {
        request = request.header(name, value);
      }
      let deadline = Instant::now()
        .checked_add(self.upload_timeout)
        .unwrap_or(target_deadline)
        .min(target_deadline);
      let result = tokio::select! {
        () = cancellation.cancelled() => return Err(OutputError::Cancelled),
        result = timeout_at(deadline, request.send()) => result,
      };
      match result {
        Ok(Ok(response)) if response.status().is_success() => return Ok(()),
        Ok(Ok(response)) if retryable_upload_status(response.status()) => {
          last = Some(format!("HTTP {}", response.status()));
        }
        Ok(Ok(response)) => {
          return Err(OutputError::Upload(format!(
            "object store returned HTTP {}",
            response.status()
          )));
        }
        Ok(Err(error)) => last = Some(redacted_transport_error(error)),
        Err(_) => last = Some("request timed out".to_owned()),
      }
      if attempt < self.retry.max_attempts {
        let delay = self.retry.delay(&target.upload_id, attempt, self.retry.max_delay);
        if Instant::now()
          .checked_add(delay)
          .is_none_or(|end| end >= target_deadline)
        {
          return Err(OutputError::Upload(
            "presigned upload target expired before retry".to_owned(),
          ));
        }
        tokio::select! {
          () = cancellation.cancelled() => return Err(OutputError::Cancelled),
          () = sleep(delay) => {}
        }
      }
    }
    Err(OutputError::Upload(format!(
      "retries exhausted: {}",
      last.unwrap_or_else(|| "unknown transport failure".to_owned())
    )))
  }
}

#[async_trait]
impl OutputPublisher for PresignedOutputPublisher {
  async fn freeze(
    &self,
    request: FreezeOutputs<'_>,
    cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    let outputs = match prepare::prepare(
      request.workspace,
      request.results,
      request.limits,
      request.staging_root,
      self.max_archive_entries,
      cancellation.clone(),
    )
    .await
    {
      Ok(outputs) => outputs,
      Err(operation) => return Err(cleanup_failed_staging(operation, request.staging_root).await),
    };
    Ok(FrozenOutputs::prepared(outputs, request.staging_root.to_owned()))
  }

  async fn publish(&self, request: PublishOutputs<'_>, cancellation: CancellationToken) -> Result<(), OutputError> {
    let FrozenOutputs { outputs, staging_root } = request.frozen;
    let operation = async {
      for (index, output) in outputs.iter().enumerate() {
        if cancellation.is_cancelled() {
          return Err(OutputError::Cancelled);
        }
        let request_id = Uuid::new_v4().to_string();
        let begin = BeginOutputUploadRequest {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request_id.clone(),
          registration_id: request.registration.registration_id.clone(),
          lease: LeaseFence::from(request.lease),
          upload_key: upload_key(request.lease, index, output),
          output: output.metadata().clone(),
        };
        let target = self
          .coordinator
          .begin_output_upload(request.registration, request.lease, &begin, cancellation.clone())
          .await?;
        self.upload(&target, output, &cancellation).await?;
        let complete = CompleteOutputUploadRequest {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: Uuid::new_v4().to_string(),
          registration_id: request.registration.registration_id.clone(),
          lease: LeaseFence::from(request.lease),
          upload_id: target.upload_id,
        };
        self
          .coordinator
          .complete_output_upload(request.registration, request.lease, &complete, cancellation.clone())
          .await?;
      }
      Ok(())
    }
    .await;
    cleanup_staging(operation, staging_root.as_deref()).await
  }
}

async fn cleanup_failed_staging(operation: OutputError, staging_root: &Path) -> OutputError {
  match tokio::fs::remove_dir_all(staging_root).await {
    Ok(()) => operation,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => operation,
    Err(cleanup) => OutputError::OperationAndCleanup {
      operation: Box::new(operation),
      cleanup,
    },
  }
}

async fn cleanup_staging(operation: Result<(), OutputError>, staging_root: Option<&Path>) -> Result<(), OutputError> {
  let cleanup = match staging_root {
    Some(staging_root) => match tokio::fs::remove_dir_all(staging_root).await {
      Ok(()) => Ok(()),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(error) => Err((staging_root, error)),
    },
    None => Ok(()),
  };
  match (operation, cleanup) {
    (Ok(()), Ok(())) => Ok(()),
    (Err(operation), Ok(())) => Err(operation),
    (Ok(()), Err((path, source))) => Err(OutputError::Cleanup {
      path: path.to_owned(),
      source,
    }),
    (Err(operation), Err((_path, cleanup))) => Err(OutputError::OperationAndCleanup {
      operation: Box::new(operation),
      cleanup,
    }),
  }
}

fn canonical_origin(value: &str) -> Result<String, OutputError> {
  let url = Url::parse(value).map_err(|_| OutputError::Invalid(format!("invalid upload origin '{value}'")))?;
  let local_http = url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "::1" | "localhost"));
  if (url.scheme() != "https" && !local_http)
    || !url.username().is_empty()
    || url.password().is_some()
    || url.path() != "/"
    || url.query().is_some()
    || url.fragment().is_some()
  {
    return Err(OutputError::Invalid(format!("invalid upload origin '{value}'")));
  }
  Ok(url.origin().ascii_serialization())
}

fn retryable_upload_status(status: StatusCode) -> bool {
  status == StatusCode::REQUEST_TIMEOUT || status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn upload_target_deadline(expires_at: u64) -> Result<Instant, OutputError> {
  let expiry = std::time::UNIX_EPOCH
    .checked_add(Duration::from_secs(expires_at))
    .ok_or_else(|| OutputError::Invalid("upload target expiry is too far in the future".to_owned()))?;
  let remaining = expiry
    .duration_since(std::time::SystemTime::now())
    .ok()
    .filter(|remaining| !remaining.is_zero())
    .ok_or_else(|| OutputError::Invalid("server returned an expired upload target".to_owned()))?;
  Instant::now()
    .checked_add(remaining)
    .ok_or_else(|| OutputError::Invalid("upload target expiry is too far in the future".to_owned()))
}

/// Reqwest attaches request URLs to transport errors. Presigned query strings
/// are bearer capabilities, so only the URL-free diagnostic may cross this
/// module's interface or reach tracing.
fn redacted_transport_error(error: reqwest::Error) -> String {
  error.without_url().to_string()
}

fn upload_key(lease: &LeaseAssignment, index: usize, output: &PreparedOutput) -> String {
  use sha2::{Digest as _, Sha256};
  let mut digest = Sha256::new();
  let index = index.to_be_bytes();
  for value in [
    lease.lease_id.as_bytes(),
    lease.fencing_token.as_bytes(),
    index.as_slice(),
    output.metadata().sha256.as_bytes(),
  ] {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
  }
  format!("{:x}", digest.finalize())
}
