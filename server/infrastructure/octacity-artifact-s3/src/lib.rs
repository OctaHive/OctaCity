//! S3-compatible artifact adapter with a private pending-to-published boundary.
//!
//! Agents upload only to a temporary key with a signed SHA-256 checksum. When
//! the object store exposes that verified checksum, completion validates it
//! with a metadata request. Compatible stores that omit checksums from HEAD
//! are verified by streaming the exact object generation through SHA-256; the
//! expected metadata is never treated as proof of the stored bytes. The exact
//! generation is then conditionally copied to a key derived from the immutable
//! artifact identity and digest. A still-live PUT capability therefore cannot
//! mutate a published artifact.

use std::{future::Future, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
  Client,
  config::{BehaviorVersion, Credentials, Region},
  error::SdkError,
  operation::head_object::HeadObjectError,
  presigning::PresigningConfig,
  types::ChecksumMode,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use octacity_artifact_store::{
  ArtifactIntegrityError, ArtifactObject, ArtifactStore, ArtifactStoreError, ArtifactStoreOperation,
  DownloadAuthorization, InvalidArtifactStoreRequest, UploadAuthorization,
};

const SHA256_METADATA: &str = "octacity-sha256";
const SIZE_METADATA: &str = "octacity-size";
const MAX_PRESIGNED_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// Maximum object size supported by one S3 PUT and one S3 CopyObject request.
const MAX_SINGLE_OBJECT_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// Explicit S3-compatible endpoint and server-owned credentials.
#[derive(Clone)]
pub struct S3ArtifactStoreConfig {
  /// S3 API endpoint, including scheme and optional port.
  pub endpoint: String,
  /// Signing region accepted by the service.
  pub region: String,
  /// Existing bucket owned by OctaCity.
  pub bucket: String,
  /// Optional safe object-key prefix shared by all OctaCity objects.
  pub prefix: String,
  /// Server-side S3 access key.
  pub access_key: String,
  /// Server-side S3 secret key.
  pub secret_key: String,
  /// Enables path-style addressing required by MinIO and some compatible stores.
  pub force_path_style: bool,
  /// Deadline for each complete storage operation, including body verification.
  pub operation_timeout: Duration,
}

/// Invalid S3 adapter configuration rejected before a client is created.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum S3ArtifactStoreConfigError {
  /// The endpoint is not a safe origin accepted by this adapter.
  #[error("S3 endpoint must be an HTTPS origin, or a loopback HTTP origin for tests")]
  InvalidEndpoint,
  /// The signing region is empty, oversized, or contains unsupported characters.
  #[error("S3 region is invalid")]
  InvalidRegion,
  /// The bucket name is outside the adapter's supported S3-compatible subset.
  #[error("S3 bucket is invalid")]
  InvalidBucket,
  /// The configured physical object prefix contains an unsafe segment.
  #[error("S3 object prefix is invalid")]
  InvalidPrefix,
  /// An adapter credential is empty, oversized, or contains control characters.
  #[error("S3 credentials are invalid")]
  InvalidCredentials,
  /// Storage operations were configured with a zero deadline.
  #[error("S3 operation timeout must be greater than zero")]
  InvalidOperationTimeout,
}

/// S3-compatible object store that never exposes credentials or physical keys.
#[derive(Clone)]
pub struct S3ArtifactStore {
  client: Client,
  bucket: String,
  prefix: String,
  operation_timeout: Duration,
}

impl S3ArtifactStore {
  /// Validates configuration and creates an isolated S3 client.
  pub fn new(config: S3ArtifactStoreConfig) -> Result<Self, S3ArtifactStoreConfigError> {
    validate_config(&config)?;
    let credentials = Credentials::new(config.access_key, config.secret_key, None, None, "octacity-server");
    let sdk_config = aws_sdk_s3::Config::builder()
      .behavior_version(BehaviorVersion::latest())
      .endpoint_url(config.endpoint)
      .region(Region::new(config.region))
      .credentials_provider(credentials)
      .force_path_style(config.force_path_style)
      .build();
    Ok(Self {
      client: Client::from_conf(sdk_config),
      bucket: config.bucket,
      prefix: config.prefix.trim_matches('/').to_owned(),
      operation_timeout: config.operation_timeout,
    })
  }

  fn pending_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("uploads/{}", object.upload_id()))
  }

  fn published_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("objects/{}/{}", object.artifact_id(), object.sha256()))
  }

  fn key(&self, suffix: &str) -> String {
    if self.prefix.is_empty() {
      suffix.to_owned()
    } else {
      format!("{}/{suffix}", self.prefix)
    }
  }

  async fn verified_etag(
    &self,
    key: &str,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<String, ArtifactStoreError> {
    let expected_checksum = STANDARD.encode(object.sha256_bytes());
    let head = self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(key)
      .checksum_mode(ChecksumMode::Enabled)
      .send()
      .await
      .map_err(|source| map_head_error(source, operation))?;
    let head_etag = head
      .e_tag()
      .ok_or(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::MissingGeneration,
      })?
      .to_owned();
    if head.content_length() != Some(object.size_bytes() as i64) {
      return Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::SizeMismatch,
      });
    }
    match head.checksum_sha256() {
      Some(checksum) if checksum == expected_checksum => Ok(head_etag),
      Some(_) => Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::DigestMismatch,
      }),
      None => {
        self.verify_body(key, &head_etag, object, operation).await?;
        Ok(head_etag)
      }
    }
  }

  /// Verifies one immutable object generation without buffering it in memory.
  ///
  /// Some S3-compatible stores enforce the checksum supplied on PUT but omit
  /// it from HEAD. The conditional GET binds this fallback to the generation
  /// inspected above, while the byte count prevents an oversized response from
  /// consuming unbounded network bandwidth before failure is reported.
  async fn verify_body(
    &self,
    key: &str,
    etag: &str,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<(), ArtifactStoreError> {
    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key)
      .if_match(etag)
      .send()
      .await
      .map_err(|source| backend(operation, source))?;
    let mut body = response.body;
    let mut verifier = BodyVerifier::new(object);
    while let Some(chunk) = body.next().await {
      let chunk = chunk.map_err(|source| backend(operation, source))?;
      verifier.update(&chunk)?;
    }
    verifier.finish()
  }

  async fn within_operation<T>(
    &self,
    operation: ArtifactStoreOperation,
    future: impl Future<Output = Result<T, ArtifactStoreError>>,
  ) -> Result<T, ArtifactStoreError> {
    tokio::time::timeout(self.operation_timeout, future)
      .await
      .map_err(|_| ArtifactStoreError::TimedOut { operation })?
  }

  async fn published_is_valid(
    &self,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<bool, ArtifactStoreError> {
    let response = match self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(self.published_key(object))
      .send()
      .await
    {
      Ok(response) => response,
      Err(error) => match map_head_error(error, operation) {
        ArtifactStoreError::NotFound => return Ok(false),
        error => return Err(error),
      },
    };
    let metadata = response.metadata();
    let expected_size = object.size_bytes().to_string();
    if response.content_length() != Some(object.size_bytes() as i64)
      || metadata
        .and_then(|values| values.get(SHA256_METADATA))
        .map(String::as_str)
        != Some(object.sha256())
      || metadata
        .and_then(|values| values.get(SIZE_METADATA))
        .map(String::as_str)
        != Some(expected_size.as_str())
    {
      Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::PublishedMetadataMismatch,
      })
    } else {
      Ok(true)
    }
  }
}

/// Incremental verifier shared by the network fallback and deterministic unit
/// tests. It rejects excess bytes immediately and accepts a stream only after
/// both its final length and digest match the server-authorized object.
struct BodyVerifier {
  expected_size: u64,
  expected_sha256: [u8; 32],
  bytes: u64,
  digest: Sha256,
}

impl BodyVerifier {
  fn new(object: &ArtifactObject) -> Self {
    Self {
      expected_size: object.size_bytes(),
      expected_sha256: object.sha256_bytes(),
      bytes: 0,
      digest: Sha256::new(),
    }
  }

  fn update(&mut self, chunk: &[u8]) -> Result<(), ArtifactStoreError> {
    self.bytes = self
      .bytes
      .checked_add(chunk.len() as u64)
      .ok_or(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::SizeOverflow,
      })?;
    if self.bytes > self.expected_size {
      return Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::SizeMismatch,
      });
    }
    self.digest.update(chunk);
    Ok(())
  }

  fn finish(self) -> Result<(), ArtifactStoreError> {
    if self.bytes != self.expected_size || self.digest.finalize().as_slice() != self.expected_sha256 {
      return Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::BodyMismatch,
      });
    }
    Ok(())
  }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
  async fn authorize_upload(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<UploadAuthorization, ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::AuthorizeUpload, async {
        validate_s3_object(object)?;
        let presigning = presigning(expires_in, ArtifactStoreOperation::AuthorizeUpload)?;
        let checksum = STANDARD.encode(object.sha256_bytes());
        let request = self
          .client
          .put_object()
          .bucket(&self.bucket)
          .key(self.pending_key(object))
          .content_length(object.size_bytes() as i64)
          .content_type(object.content_type())
          .checksum_sha256(checksum)
          .metadata(SHA256_METADATA, object.sha256())
          .metadata(SIZE_METADATA, object.size_bytes().to_string())
          .presigned(presigning)
          .await
          .map_err(|source| backend(ArtifactStoreOperation::AuthorizeUpload, source))?;
        let required_headers = request
          .headers()
          .filter(|(name, _)| !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("content-length"))
          .map(|(name, value)| (name.to_owned(), value.to_owned()))
          .collect();
        Ok(UploadAuthorization {
          url: request.uri().to_owned(),
          required_headers,
          expires_in,
        })
      })
      .await
  }

  async fn complete_upload(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::CompleteUpload, async {
        validate_s3_object(object)?;
        // A retry may observe an object copied by an earlier request whose
        // response was lost. Re-verify its bytes before accepting it as the
        // publication boundary; metadata alone is sufficient only after this
        // method has established the immutable object.
        match self
          .verified_etag(
            &self.published_key(object),
            object,
            ArtifactStoreOperation::CompleteUpload,
          )
          .await
        {
          Ok(_) => {
            self
              .delete_pending(object, ArtifactStoreOperation::CompleteUpload)
              .await?;
            return Ok(());
          }
          Err(ArtifactStoreError::NotFound) => {}
          Err(error) => return Err(error),
        }
        let pending = self.pending_key(object);
        let etag = self
          .verified_etag(&pending, object, ArtifactStoreOperation::CompleteUpload)
          .await?;
        self
          .client
          .copy_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .copy_source(format!("{}/{pending}", self.bucket))
          .copy_source_if_match(etag)
          .content_type(object.content_type())
          .metadata(SHA256_METADATA, object.sha256())
          .metadata(SIZE_METADATA, object.size_bytes().to_string())
          .metadata_directive(aws_sdk_s3::types::MetadataDirective::Replace)
          .send()
          .await
          .map_err(|source| backend(ArtifactStoreOperation::CompleteUpload, source))?;
        if !self
          .published_is_valid(object, ArtifactStoreOperation::CompleteUpload)
          .await?
        {
          return Err(ArtifactStoreError::Integrity {
            reason: ArtifactIntegrityError::PublicationMismatch,
          });
        }
        self
          .delete_pending(object, ArtifactStoreOperation::CompleteUpload)
          .await
      })
      .await
  }

  async fn authorize_download(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<DownloadAuthorization, ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::AuthorizeDownload, async {
        validate_s3_object(object)?;
        if !self
          .published_is_valid(object, ArtifactStoreOperation::AuthorizeDownload)
          .await?
        {
          return Err(ArtifactStoreError::NotFound);
        }
        let request = self
          .client
          .get_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .presigned(presigning(expires_in, ArtifactStoreOperation::AuthorizeDownload)?)
          .await
          .map_err(|source| backend(ArtifactStoreOperation::AuthorizeDownload, source))?;
        Ok(DownloadAuthorization {
          url: request.uri().to_owned(),
          expires_in,
        })
      })
      .await
  }

  async fn delete(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::Delete, async {
        validate_s3_object(object)?;
        self.delete_pending(object, ArtifactStoreOperation::Delete).await?;
        self
          .client
          .delete_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .send()
          .await
          .map_err(|source| backend(ArtifactStoreOperation::Delete, source))?;
        Ok(())
      })
      .await
  }
}

impl S3ArtifactStore {
  async fn delete_pending(
    &self,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<(), ArtifactStoreError> {
    self
      .client
      .delete_object()
      .bucket(&self.bucket)
      .key(self.pending_key(object))
      .send()
      .await
      .map_err(|source| backend(operation, source))?;
    Ok(())
  }
}

fn validate_config(config: &S3ArtifactStoreConfig) -> Result<(), S3ArtifactStoreConfigError> {
  let endpoint = config
    .endpoint
    .parse::<http::Uri>()
    .map_err(|_| S3ArtifactStoreConfigError::InvalidEndpoint)?;
  let secure = endpoint.scheme_str() == Some("https");
  let loopback =
    endpoint.scheme_str() == Some("http") && matches!(endpoint.host(), Some("127.0.0.1" | "::1" | "localhost"));
  if (!secure && !loopback)
    || endpoint.authority().is_none()
    || endpoint
      .authority()
      .is_some_and(|authority| authority.as_str().contains('@'))
    || endpoint.path() != "/"
    || endpoint.query().is_some()
  {
    return Err(S3ArtifactStoreConfigError::InvalidEndpoint);
  }
  if !is_safe_identifier(&config.region) {
    return Err(S3ArtifactStoreConfigError::InvalidRegion);
  }
  if config.bucket.is_empty()
    || config.bucket.len() > 63
    || !config
      .bucket
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
  {
    return Err(S3ArtifactStoreConfigError::InvalidBucket);
  }
  if config.prefix.len() > 256
    || config
      .prefix
      .split('/')
      .filter(|segment| !segment.is_empty())
      .any(|segment| !is_safe_identifier(segment))
  {
    return Err(S3ArtifactStoreConfigError::InvalidPrefix);
  }
  if [config.access_key.as_str(), config.secret_key.as_str()]
    .iter()
    .any(|value| value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control))
  {
    return Err(S3ArtifactStoreConfigError::InvalidCredentials);
  }
  if config.operation_timeout.is_zero() {
    return Err(S3ArtifactStoreConfigError::InvalidOperationTimeout);
  }
  Ok(())
}

fn is_safe_identifier(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 256
    && value
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_s3_object(object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
  if object.size_bytes() > MAX_SINGLE_OBJECT_BYTES {
    return Err(ArtifactStoreError::Invalid {
      reason: InvalidArtifactStoreRequest::UnsupportedObjectSize,
    });
  }
  Ok(())
}

fn presigning(expires_in: Duration, operation: ArtifactStoreOperation) -> Result<PresigningConfig, ArtifactStoreError> {
  if expires_in.is_zero() || expires_in > MAX_PRESIGNED_LIFETIME {
    return Err(ArtifactStoreError::Invalid {
      reason: InvalidArtifactStoreRequest::InvalidAuthorizationLifetime,
    });
  }
  PresigningConfig::expires_in(expires_in).map_err(|source| backend(operation, source))
}

fn map_head_error(error: SdkError<HeadObjectError>, operation: ArtifactStoreOperation) -> ArtifactStoreError {
  if error.as_service_error().is_some_and(HeadObjectError::is_not_found) {
    ArtifactStoreError::NotFound
  } else {
    backend(operation, error)
  }
}

fn backend(operation: ArtifactStoreOperation, _source: impl std::error::Error) -> ArtifactStoreError {
  ArtifactStoreError::Backend { operation }
}

#[cfg(test)]
mod tests {
  use octacity_artifact_store::{ArtifactId, ArtifactUploadId};
  use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
  use uuid::Uuid;

  use super::*;

  fn artifact_id() -> ArtifactId {
    ArtifactId::from_uuid(Uuid::from_u128(1)).unwrap()
  }

  fn upload_id() -> ArtifactUploadId {
    ArtifactUploadId::from_uuid(Uuid::from_u128(2)).unwrap()
  }

  fn object_for_body(body: &[u8]) -> ArtifactObject {
    ArtifactObject::new(
      artifact_id(),
      upload_id(),
      body.len() as u64,
      format!("{:x}", Sha256::digest(body)),
      "application/octet-stream",
    )
    .unwrap()
  }

  async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
      let bytes = socket.read(&mut chunk).await.unwrap();
      assert!(bytes > 0, "client closed before sending complete HTTP headers");
      request.extend_from_slice(&chunk[..bytes]);
      assert!(
        request.len() <= 16 * 1024,
        "test request headers exceeded fixture limit"
      );
    }
    String::from_utf8(request).unwrap()
  }

  fn configuration(endpoint: &str) -> S3ArtifactStoreConfig {
    S3ArtifactStoreConfig {
      endpoint: endpoint.to_owned(),
      region: "us-east-1".to_owned(),
      bucket: "octacity-artifacts".to_owned(),
      prefix: "ci/v1".to_owned(),
      access_key: "access".to_owned(),
      secret_key: "secret".to_owned(),
      force_path_style: true,
      operation_timeout: Duration::from_secs(5),
    }
  }

  #[test]
  fn configuration_rejects_remote_plaintext_and_unsafe_names() {
    assert!(S3ArtifactStore::new(configuration("http://objects.example")).is_err());
    assert!(S3ArtifactStore::new(configuration("http://127.0.0.1:9000")).is_ok());

    let mut unsafe_prefix = configuration("https://objects.example");
    unsafe_prefix.prefix = "../objects".to_owned();
    assert!(S3ArtifactStore::new(unsafe_prefix).is_err());

    let mut zero_timeout = configuration("https://objects.example");
    zero_timeout.operation_timeout = Duration::ZERO;
    assert!(S3ArtifactStore::new(zero_timeout).is_err());
  }

  #[test]
  fn physical_keys_are_private_and_deterministic() {
    let store = S3ArtifactStore::new(configuration("https://objects.example")).unwrap();
    let artifact_id = artifact_id();
    let upload_id = upload_id();
    let object = ArtifactObject::new(artifact_id, upload_id, 7, "a".repeat(64), "application/octet-stream").unwrap();
    assert_eq!(store.pending_key(&object), format!("ci/v1/uploads/{upload_id}"));
    assert_eq!(
      store.published_key(&object),
      format!("ci/v1/objects/{artifact_id}/{}", "a".repeat(64))
    );
  }

  #[test]
  fn streamed_body_verification_requires_exact_length_and_sha256() {
    let object = object_for_body(b"immutable artifact bytes");
    let mut valid = BodyVerifier::new(&object);
    valid.update(b"immutable ").unwrap();
    valid.update(b"artifact bytes").unwrap();
    assert!(valid.finish().is_ok());

    let mut changed = BodyVerifier::new(&object);
    changed.update(b"immutable artifact bytez").unwrap();
    assert!(matches!(changed.finish(), Err(ArtifactStoreError::Integrity { .. })));

    let mut truncated = BodyVerifier::new(&object);
    truncated.update(b"immutable artifact").unwrap();
    assert!(matches!(truncated.finish(), Err(ArtifactStoreError::Integrity { .. })));

    let mut oversized = BodyVerifier::new(&object);
    assert!(matches!(
      oversized.update(b"immutable artifact bytes!"),
      Err(ArtifactStoreError::Integrity { .. })
    ));
  }

  #[tokio::test]
  async fn checksum_fallback_binds_the_body_read_to_the_head_generation() {
    let body = b"verified bytes";
    let object = object_for_body(body);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let store = S3ArtifactStore::new(configuration(&endpoint)).unwrap();
    let server = tokio::spawn(async move {
      let (mut head, _) = listener.accept().await.unwrap();
      let request = read_request(&mut head).await;
      assert!(request.starts_with("HEAD "));
      head
        .write_all(
          format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\netag: \"generation-1\"\r\nconnection: close\r\n\r\n",
            body.len()
          )
          .as_bytes(),
        )
        .await
        .unwrap();

      let (mut get, _) = listener.accept().await.unwrap();
      let request = read_request(&mut get).await.to_ascii_lowercase();
      assert!(request.starts_with("get "));
      assert!(request.contains("\r\nif-match: \"generation-1\"\r\n"));
      let response_body = b"<Error><Code>PreconditionFailed</Code><Message>generation changed</Message></Error>";
      get
        .write_all(
          format!(
            "HTTP/1.1 412 Precondition Failed\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            response_body.len()
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      get.write_all(response_body).await.unwrap();
    });

    assert!(matches!(
      store
        .verified_etag("pending", &object, ArtifactStoreOperation::CompleteUpload)
        .await,
      Err(ArtifactStoreError::Backend {
        operation: ArtifactStoreOperation::CompleteUpload
      })
    ));
    server.await.unwrap();
  }

  #[test]
  fn backend_errors_discard_provider_diagnostics() {
    let error = backend(
      ArtifactStoreOperation::AuthorizeUpload,
      std::io::Error::other("https://storage.example/object?signature=backend-secret"),
    );

    assert!(!format!("{error:?} {error}").contains("backend-secret"));
    assert!(std::error::Error::source(&error).is_none());
  }

  #[tokio::test]
  async fn network_operations_obey_the_configured_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let mut config = configuration(&endpoint);
    config.operation_timeout = Duration::from_millis(100);
    let store = S3ArtifactStore::new(config).unwrap();
    let server = tokio::spawn(async move {
      let (_socket, _) = listener.accept().await.unwrap();
      std::future::pending::<()>().await;
    });
    let object = ArtifactObject::new(
      artifact_id(),
      upload_id(),
      1,
      "a".repeat(64),
      "application/octet-stream",
    )
    .unwrap();

    assert!(matches!(
      store.complete_upload(&object).await,
      Err(ArtifactStoreError::TimedOut {
        operation: ArtifactStoreOperation::CompleteUpload
      })
    ));
    server.abort();
  }

  #[tokio::test]
  async fn rejects_objects_too_large_for_single_put_and_copy() {
    let store = S3ArtifactStore::new(configuration("https://objects.example")).unwrap();
    let object = ArtifactObject::new(
      artifact_id(),
      upload_id(),
      MAX_SINGLE_OBJECT_BYTES + 1,
      "a".repeat(64),
      "application/octet-stream",
    )
    .unwrap();

    assert!(matches!(
      store.authorize_upload(&object, Duration::from_secs(60)).await,
      Err(ArtifactStoreError::Invalid {
        reason: InvalidArtifactStoreRequest::UnsupportedObjectSize
      })
    ));
  }
}
