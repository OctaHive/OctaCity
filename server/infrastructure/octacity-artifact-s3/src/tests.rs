use octacity_artifact_store::{ArtifactId, ArtifactUploadId};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::*;
use crate::config::MAX_OPERATION_TIMEOUT;

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
    access_key: Zeroizing::new("access".to_owned()),
    secret_key: Zeroizing::new("secret".to_owned()),
    force_path_style: true,
    operation_timeout: Duration::from_secs(5),
    capability_recheck_interval: Duration::from_secs(300),
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

  for bucket in [".", "ab", "-bucket", "bucket-", "bucket..name", "127.0.0.1"] {
    let mut config = configuration("https://objects.example");
    config.bucket = bucket.to_owned();
    assert!(S3ArtifactStore::new(config).is_err(), "accepted bucket {bucket}");
  }
  for prefix in ["/ci", "ci/", "ci//v1"] {
    let mut config = configuration("https://objects.example");
    config.prefix = prefix.to_owned();
    assert!(S3ArtifactStore::new(config).is_err(), "accepted prefix {prefix}");
  }
  let mut excessive_timeout = configuration("https://objects.example");
  excessive_timeout.operation_timeout = MAX_OPERATION_TIMEOUT + Duration::from_millis(1);
  assert!(S3ArtifactStore::new(excessive_timeout).is_err());

  let mut zero_recheck = configuration("https://objects.example");
  zero_recheck.capability_recheck_interval = Duration::ZERO;
  assert!(S3ArtifactStore::new(zero_recheck).is_err());
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
  store.capability_state.lock().await.qualified_at = Some(std::time::Instant::now());
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
  assert!(
    store.capability_state.lock().await.qualified_at.is_none(),
    "a failed object operation must require capability requalification"
  );
  server.abort();
}

#[tokio::test]
async fn health_check_requires_mutation_permissions_not_only_bucket_inspection() {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let endpoint = format!("http://{}", listener.local_addr().unwrap());
  let store = S3ArtifactStore::new(configuration(&endpoint)).unwrap();
  let server = tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.unwrap();
    let request = read_request(&mut socket).await;
    assert!(request.starts_with("PUT "));
    assert!(request.contains("/octacity-artifacts/ci/v1/health/readiness-"));
    socket
      .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
      .await
      .unwrap();
  });

  assert_eq!(store.health_check().await, Err(S3ArtifactStoreHealthError::Unavailable));
  server.await.unwrap();
}

#[tokio::test]
async fn health_check_uses_marker_polling_and_requalifies_after_availability_loss() {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let endpoint = format!("http://{}", listener.local_addr().unwrap());
  let store = S3ArtifactStore::new(configuration(&endpoint)).unwrap();
  let server = tokio::spawn(async move {
    for (index, method) in ["PUT", "GET", "PUT", "GET", "DELETE"].into_iter().enumerate() {
      let (mut socket, _) = listener.accept().await.unwrap();
      let request = read_request(&mut socket).await;
      assert!(request.starts_with(method));
      assert!(request.contains("/octacity-artifacts/ci/v1/health/readiness-"));
      if index == 2 {
        assert!(request.to_ascii_lowercase().contains("x-amz-copy-source:"));
        let body = b"<CopyObjectResult><ETag>&quot;probe&quot;</ETag><LastModified>2026-09-18T00:00:00Z</LastModified></CopyObjectResult>";
        socket
          .write_all(
            format!(
              "HTTP/1.1 200 OK\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
              body.len()
            )
            .as_bytes(),
          )
          .await
          .unwrap();
        socket.write_all(body).await.unwrap();
      } else if method == "DELETE" {
        socket
          .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
          .await
          .unwrap();
      } else {
        socket
          .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
          .await
          .unwrap();
      }
    }
    let (mut socket, _) = listener.accept().await.unwrap();
    let request = read_request(&mut socket).await;
    assert!(request.starts_with("HEAD "));
    assert!(request.contains("/octacity-artifacts/ci/v1/health/readiness-"));
    assert!(request.contains("/source"));
    socket
      .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
      .await
      .unwrap();

    let (mut unavailable, _) = listener.accept().await.unwrap();
    let request = read_request(&mut unavailable).await;
    assert!(request.starts_with("HEAD "));
    unavailable
      .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
      .await
      .unwrap();

    let (mut requalification, _) = listener.accept().await.unwrap();
    let request = read_request(&mut requalification).await;
    assert!(request.starts_with("PUT "));
    requalification
      .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
      .await
      .unwrap();
  });

  store.health_check().await.unwrap();
  store.health_check().await.unwrap();
  assert_eq!(store.health_check().await, Err(S3ArtifactStoreHealthError::Unavailable));
  assert_eq!(store.health_check().await, Err(S3ArtifactStoreHealthError::Unavailable));
  server.await.unwrap();
}

#[tokio::test]
async fn expired_qualification_repeats_the_full_capability_probe() {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let endpoint = format!("http://{}", listener.local_addr().unwrap());
  let mut config = configuration(&endpoint);
  config.capability_recheck_interval = Duration::from_millis(1);
  let store = S3ArtifactStore::new(config).unwrap();
  store.capability_state.lock().await.qualified_at =
    Some(std::time::Instant::now().checked_sub(Duration::from_secs(1)).unwrap());
  let server = tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.unwrap();
    let request = read_request(&mut socket).await;
    assert!(request.starts_with("PUT "));
    socket
      .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
      .await
      .unwrap();
  });

  assert_eq!(store.health_check().await, Err(S3ArtifactStoreHealthError::Unavailable));
  server.await.unwrap();
}

#[tokio::test]
async fn object_failure_cannot_be_lost_behind_a_concurrent_successful_health_check() {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let endpoint = format!("http://{}", listener.local_addr().unwrap());
  let store = S3ArtifactStore::new(configuration(&endpoint)).unwrap();
  store.capability_state.lock().await.qualified_at = Some(std::time::Instant::now());
  let (request_seen_tx, request_seen_rx) = tokio::sync::oneshot::channel();
  let (respond_tx, respond_rx) = tokio::sync::oneshot::channel();
  let server = tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.unwrap();
    let request = read_request(&mut socket).await;
    assert!(request.starts_with("HEAD "));
    request_seen_tx.send(()).unwrap();
    respond_rx.await.unwrap();
    socket
      .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
      .await
      .unwrap();
  });

  let health_store = store.clone();
  let health = tokio::spawn(async move { health_store.health_check().await });
  request_seen_rx.await.unwrap();
  let operation_store = store.clone();
  let (operation_failed_tx, operation_failed_rx) = tokio::sync::oneshot::channel();
  let operation = tokio::spawn(async move {
    operation_store
      .within_operation(ArtifactStoreOperation::Delete, async {
        operation_failed_tx.send(()).unwrap();
        Err::<(), _>(ArtifactStoreError::Backend {
          operation: ArtifactStoreOperation::Delete,
        })
      })
      .await
  });
  operation_failed_rx.await.unwrap();
  respond_tx.send(()).unwrap();

  health.await.unwrap().unwrap();
  assert!(matches!(
    operation.await.unwrap(),
    Err(ArtifactStoreError::Backend { .. })
  ));
  assert!(
    store.capability_state.lock().await.qualified_at.is_none(),
    "the later object failure must win over the older successful probe"
  );
  server.await.unwrap();
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
