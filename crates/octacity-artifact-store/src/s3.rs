//! S3-compatible implementation with a private pending-to-published boundary.
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

use crate::{
  ArtifactObject, ArtifactStore, ArtifactStoreError, DownloadAuthorization, UploadAuthorization, validate_identifier,
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
  pub fn new(config: S3ArtifactStoreConfig) -> Result<Self, ArtifactStoreError> {
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
    self.key(&format!("uploads/{}", object.upload_id))
  }

  fn published_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("objects/{}/{}", object.artifact_id, object.sha256))
  }

  fn key(&self, suffix: &str) -> String {
    if self.prefix.is_empty() {
      suffix.to_owned()
    } else {
      format!("{}/{suffix}", self.prefix)
    }
  }

  async fn verified_etag(&self, key: &str, object: &ArtifactObject) -> Result<String, ArtifactStoreError> {
    let expected_checksum = STANDARD.encode(decode_sha256(&object.sha256)?);
    let head = self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(key)
      .checksum_mode(ChecksumMode::Enabled)
      .send()
      .await
      .map_err(map_head_error)?;
    let head_etag = head
      .e_tag()
      .ok_or_else(|| ArtifactStoreError::Integrity("object store omitted the generation identifier".to_owned()))?
      .to_owned();
    if head.content_length() != Some(object.size_bytes as i64) {
      return Err(ArtifactStoreError::Integrity(
        "object store omitted the length or it differs from the authorized size".to_owned(),
      ));
    }
    match head.checksum_sha256() {
      Some(checksum) if checksum == expected_checksum => Ok(head_etag),
      Some(_) => Err(ArtifactStoreError::Integrity(
        "object-store SHA-256 differs from the authorized digest".to_owned(),
      )),
      None => {
        self.verify_body(key, &head_etag, object).await?;
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
  async fn verify_body(&self, key: &str, etag: &str, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key)
      .if_match(etag)
      .send()
      .await
      .map_err(|source| backend("read object for verification", source))?;
    let mut body = response.body;
    let mut verifier = BodyVerifier::new(object)?;
    while let Some(chunk) = body.next().await {
      let chunk = chunk.map_err(|source| backend("read object verification body", source))?;
      verifier.update(&chunk)?;
    }
    verifier.finish()
  }

  async fn within_operation<T>(
    &self,
    operation: &'static str,
    future: impl Future<Output = Result<T, ArtifactStoreError>>,
  ) -> Result<T, ArtifactStoreError> {
    tokio::time::timeout(self.operation_timeout, future)
      .await
      .map_err(|_| ArtifactStoreError::TimedOut { operation })?
  }

  async fn published_is_valid(&self, object: &ArtifactObject) -> Result<bool, ArtifactStoreError> {
    let response = match self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(self.published_key(object))
      .send()
      .await
    {
      Ok(response) => response,
      Err(error) => match map_head_error(error) {
        ArtifactStoreError::NotFound => return Ok(false),
        error => return Err(error),
      },
    };
    let metadata = response.metadata();
    let expected_size = object.size_bytes.to_string();
    if response.content_length() != Some(object.size_bytes as i64)
      || metadata
        .and_then(|values| values.get(SHA256_METADATA))
        .map(String::as_str)
        != Some(object.sha256.as_str())
      || metadata
        .and_then(|values| values.get(SIZE_METADATA))
        .map(String::as_str)
        != Some(expected_size.as_str())
    {
      Err(ArtifactStoreError::Integrity(
        "published object metadata differs from its immutable identity".to_owned(),
      ))
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
  fn new(object: &ArtifactObject) -> Result<Self, ArtifactStoreError> {
    Ok(Self {
      expected_size: object.size_bytes,
      expected_sha256: decode_sha256(&object.sha256)?,
      bytes: 0,
      digest: Sha256::new(),
    })
  }

  fn update(&mut self, chunk: &[u8]) -> Result<(), ArtifactStoreError> {
    self.bytes = self
      .bytes
      .checked_add(chunk.len() as u64)
      .ok_or_else(|| ArtifactStoreError::Integrity("object size overflowed during verification".to_owned()))?;
    if self.bytes > self.expected_size {
      return Err(ArtifactStoreError::Integrity(
        "object body exceeds the authorized size".to_owned(),
      ));
    }
    self.digest.update(chunk);
    Ok(())
  }

  fn finish(self) -> Result<(), ArtifactStoreError> {
    if self.bytes != self.expected_size || self.digest.finalize().as_slice() != self.expected_sha256 {
      return Err(ArtifactStoreError::Integrity(
        "object body differs from the authorized size or SHA-256".to_owned(),
      ));
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
      .within_operation("authorize upload", async {
        validate_s3_object(object)?;
        let presigning = presigning(expires_in)?;
        let checksum = STANDARD.encode(decode_sha256(&object.sha256)?);
        let request = self
          .client
          .put_object()
          .bucket(&self.bucket)
          .key(self.pending_key(object))
          .content_length(object.size_bytes as i64)
          .content_type(&object.content_type)
          .checksum_sha256(checksum)
          .metadata(SHA256_METADATA, &object.sha256)
          .metadata(SIZE_METADATA, object.size_bytes.to_string())
          .presigned(presigning)
          .await
          .map_err(|source| backend("authorize upload", source))?;
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
      .within_operation("complete upload", async {
        validate_s3_object(object)?;
        // A retry may observe an object copied by an earlier request whose
        // response was lost. Re-verify its bytes before accepting it as the
        // publication boundary; metadata alone is sufficient only after this
        // method has established the immutable object.
        match self.verified_etag(&self.published_key(object), object).await {
          Ok(_) => {
            self.delete_pending(object).await?;
            return Ok(());
          }
          Err(ArtifactStoreError::NotFound) => {}
          Err(error) => return Err(error),
        }
        let pending = self.pending_key(object);
        let etag = self.verified_etag(&pending, object).await?;
        self
          .client
          .copy_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .copy_source(format!("{}/{pending}", self.bucket))
          .copy_source_if_match(etag)
          .content_type(&object.content_type)
          .metadata(SHA256_METADATA, &object.sha256)
          .metadata(SIZE_METADATA, object.size_bytes.to_string())
          .metadata_directive(aws_sdk_s3::types::MetadataDirective::Replace)
          .send()
          .await
          .map_err(|source| backend("publish object", source))?;
        if !self.published_is_valid(object).await? {
          return Err(ArtifactStoreError::Integrity(
            "object store did not publish the copied object".to_owned(),
          ));
        }
        self.delete_pending(object).await
      })
      .await
  }

  async fn authorize_download(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<DownloadAuthorization, ArtifactStoreError> {
    self
      .within_operation("authorize download", async {
        validate_s3_object(object)?;
        if !self.published_is_valid(object).await? {
          return Err(ArtifactStoreError::NotFound);
        }
        let request = self
          .client
          .get_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .presigned(presigning(expires_in)?)
          .await
          .map_err(|source| backend("authorize download", source))?;
        Ok(DownloadAuthorization {
          url: request.uri().to_owned(),
          expires_in,
        })
      })
      .await
  }

  async fn delete(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self
      .within_operation("delete object", async {
        validate_s3_object(object)?;
        self.delete_pending(object).await?;
        self
          .client
          .delete_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .send()
          .await
          .map_err(|source| backend("delete published object", source))?;
        Ok(())
      })
      .await
  }
}

impl S3ArtifactStore {
  async fn delete_pending(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self
      .client
      .delete_object()
      .bucket(&self.bucket)
      .key(self.pending_key(object))
      .send()
      .await
      .map_err(|source| backend("delete pending object", source))?;
    Ok(())
  }
}

fn validate_config(config: &S3ArtifactStoreConfig) -> Result<(), ArtifactStoreError> {
  let endpoint = config
    .endpoint
    .parse::<http::Uri>()
    .map_err(|_| ArtifactStoreError::Invalid("S3 endpoint is invalid".to_owned()))?;
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
    return Err(ArtifactStoreError::Invalid(
      "S3 endpoint must be an HTTPS origin, or a loopback HTTP origin for tests".to_owned(),
    ));
  }
  validate_identifier("S3 region", &config.region)?;
  if config.bucket.is_empty()
    || config.bucket.len() > 63
    || !config
      .bucket
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
  {
    return Err(ArtifactStoreError::Invalid("S3 bucket is invalid".to_owned()));
  }
  if config.prefix.len() > 256
    || config
      .prefix
      .split('/')
      .filter(|segment| !segment.is_empty())
      .any(|segment| validate_identifier("S3 prefix segment", segment).is_err())
  {
    return Err(ArtifactStoreError::Invalid("S3 object prefix is invalid".to_owned()));
  }
  if [config.access_key.as_str(), config.secret_key.as_str()]
    .iter()
    .any(|value| value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control))
  {
    return Err(ArtifactStoreError::Invalid(
      "S3 credentials are empty, oversized, or contain controls".to_owned(),
    ));
  }
  if config.operation_timeout.is_zero() {
    return Err(ArtifactStoreError::Invalid(
      "S3 operation timeout must be greater than zero".to_owned(),
    ));
  }
  Ok(())
}

fn validate_s3_object(object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
  object.validate()?;
  if object.size_bytes > MAX_SINGLE_OBJECT_BYTES {
    return Err(ArtifactStoreError::Invalid(format!(
      "S3 single-object uploads are limited to {MAX_SINGLE_OBJECT_BYTES} bytes"
    )));
  }
  Ok(())
}

fn presigning(expires_in: Duration) -> Result<PresigningConfig, ArtifactStoreError> {
  if expires_in.is_zero() || expires_in > MAX_PRESIGNED_LIFETIME {
    return Err(ArtifactStoreError::Invalid(
      "presigned authorization lifetime must be between one second and one hour".to_owned(),
    ));
  }
  PresigningConfig::expires_in(expires_in).map_err(|source| backend("configure presigning", source))
}

fn decode_sha256(value: &str) -> Result<[u8; 32], ArtifactStoreError> {
  let mut result = [0_u8; 32];
  for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
    result[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
  }
  Ok(result)
}

fn hex_digit(value: u8) -> Result<u8, ArtifactStoreError> {
  match value {
    b'0'..=b'9' => Ok(value - b'0'),
    b'a'..=b'f' => Ok(value - b'a' + 10),
    _ => Err(ArtifactStoreError::Invalid("artifact SHA-256 is invalid".to_owned())),
  }
}

fn map_head_error(error: SdkError<HeadObjectError>) -> ArtifactStoreError {
  if error.as_service_error().is_some_and(HeadObjectError::is_not_found) {
    ArtifactStoreError::NotFound
  } else {
    backend("inspect object", error)
  }
}

fn backend(operation: &'static str, source: impl std::error::Error + Send + Sync + 'static) -> ArtifactStoreError {
  ArtifactStoreError::Backend {
    operation,
    source: Box::new(source),
  }
}

#[cfg(test)]
mod tests {
  use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

  use super::*;

  fn object_for_body(body: &[u8]) -> ArtifactObject {
    ArtifactObject {
      artifact_id: "artifact-1".to_owned(),
      upload_id: "upload-1".to_owned(),
      size_bytes: body.len() as u64,
      sha256: format!("{:x}", Sha256::digest(body)),
      content_type: "application/octet-stream".to_owned(),
    }
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
    let object = ArtifactObject {
      artifact_id: "artifact-1".to_owned(),
      upload_id: "upload-1".to_owned(),
      size_bytes: 7,
      sha256: "a".repeat(64),
      content_type: "application/octet-stream".to_owned(),
    };
    assert_eq!(store.pending_key(&object), "ci/v1/uploads/upload-1");
    assert_eq!(
      store.published_key(&object),
      format!("ci/v1/objects/artifact-1/{}", "a".repeat(64))
    );
  }

  #[test]
  fn streamed_body_verification_requires_exact_length_and_sha256() {
    let object = object_for_body(b"immutable artifact bytes");
    let mut valid = BodyVerifier::new(&object).unwrap();
    valid.update(b"immutable ").unwrap();
    valid.update(b"artifact bytes").unwrap();
    assert!(valid.finish().is_ok());

    let mut changed = BodyVerifier::new(&object).unwrap();
    changed.update(b"immutable artifact bytez").unwrap();
    assert!(matches!(changed.finish(), Err(ArtifactStoreError::Integrity(_))));

    let mut truncated = BodyVerifier::new(&object).unwrap();
    truncated.update(b"immutable artifact").unwrap();
    assert!(matches!(truncated.finish(), Err(ArtifactStoreError::Integrity(_))));

    let mut oversized = BodyVerifier::new(&object).unwrap();
    assert!(matches!(
      oversized.update(b"immutable artifact bytes!"),
      Err(ArtifactStoreError::Integrity(_))
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
      store.verified_etag("pending", &object).await,
      Err(ArtifactStoreError::Backend { .. })
    ));
    server.await.unwrap();
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
    let object = ArtifactObject {
      artifact_id: "artifact-1".to_owned(),
      upload_id: "upload-1".to_owned(),
      size_bytes: 1,
      sha256: "a".repeat(64),
      content_type: "application/octet-stream".to_owned(),
    };

    assert!(matches!(
      store.complete_upload(&object).await,
      Err(ArtifactStoreError::TimedOut {
        operation: "complete upload"
      })
    ));
    server.abort();
  }

  #[tokio::test]
  async fn rejects_objects_too_large_for_single_put_and_copy() {
    let store = S3ArtifactStore::new(configuration("https://objects.example")).unwrap();
    let object = ArtifactObject {
      artifact_id: "artifact-1".to_owned(),
      upload_id: "upload-1".to_owned(),
      size_bytes: MAX_SINGLE_OBJECT_BYTES + 1,
      sha256: "a".repeat(64),
      content_type: "application/octet-stream".to_owned(),
    };

    assert!(matches!(
      store.authorize_upload(&object, Duration::from_secs(60)).await,
      Err(ArtifactStoreError::Invalid(message)) if message.contains("single-object")
    ));
  }
}
