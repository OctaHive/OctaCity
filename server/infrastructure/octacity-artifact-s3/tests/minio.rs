//! Opt-in contract test against a real S3-compatible MinIO service.
//!
//! Set `OCTACITY_MINIO_ENDPOINT`, `OCTACITY_MINIO_ACCESS_KEY`, and
//! `OCTACITY_MINIO_SECRET_KEY`, then run this ignored test explicitly. The test
//! creates and removes its own bucket and validates direct PUT, independently
//! verified publication, idempotent completion, short-lived capabilities,
//! outage recovery, corruption rejection, empty objects, and deletion through
//! the public `ArtifactStore` boundary.

use std::{
  env,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use aws_sdk_s3::{
  Client,
  config::{BehaviorVersion, Credentials, Region},
  primitives::ByteStream,
};
use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_artifact_store::{
  ArtifactId, ArtifactIntegrityError, ArtifactObject, ArtifactStore, ArtifactStoreError, ArtifactStoreOperation,
  ArtifactUploadId, LogChunkStore, LogChunkStoreError, LogChunkWrite, UploadAuthorization,
};
use octacity_server_artifacts::{BuildLogStream, LogChunkManifest};
use octacity_server_cache::{
  BlobDescriptor, BlobEncoding, CacheBlobObject, CacheBlobStore, CacheBlobStoreError, CacheBlobWrite, Digest,
};
use octacity_server_domain::JobId;
use reqwest::header::{HeaderName, HeaderValue};
use sha2::{Digest as _, Sha256};
use tokio::{io::copy_bidirectional, net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TEST_PREFIX: &str = "artifact-contract";

#[tokio::test]
#[ignore = "requires an explicitly configured MinIO service"]
async fn s3_store_satisfies_the_minio_contract() {
  let endpoint = required("OCTACITY_MINIO_ENDPOINT");
  let access_key = required("OCTACITY_MINIO_ACCESS_KEY");
  let secret_key = required("OCTACITY_MINIO_SECRET_KEY");
  let region = env::var("OCTACITY_MINIO_REGION").unwrap_or_else(|_| "us-east-1".to_owned());
  let bucket = format!("octacity-test-{}", Uuid::new_v4().simple());
  let client = minio_client(&endpoint, &region, &access_key, &secret_key);
  client.create_bucket().bucket(&bucket).send().await.unwrap();
  let proxy = FaultProxy::start(&endpoint).await;

  let store = S3ArtifactStore::new(S3ArtifactStoreConfig {
    endpoint: proxy.endpoint(),
    region,
    bucket: bucket.clone(),
    prefix: TEST_PREFIX.to_owned(),
    access_key: zeroize::Zeroizing::new(access_key),
    secret_key: zeroize::Zeroizing::new(secret_key),
    force_path_style: true,
    operation_timeout: Duration::from_secs(2),
    capability_recheck_interval: Duration::from_secs(300),
  })
  .unwrap();
  store.health_check().await.unwrap();
  store.health_check().await.unwrap();

  verify_upload_download_and_delete(&store).await;
  verify_cache_blob_contract(&store).await;
  verify_log_chunk_contract(&store).await;
  verify_mismatched_generations_remain_unpublished(&client, &bucket, &store).await;
  verify_capability_expiration(&store).await;
  verify_outage_and_recovery(&proxy, &store).await;

  delete_test_objects(&client, &bucket).await;
  client.delete_bucket().bucket(bucket).send().await.unwrap();
  proxy.shutdown().await;
}

async fn verify_log_chunk_contract(store: &S3ArtifactStore) {
  let bytes = b"redacted stdout".to_vec();
  let manifest = LogChunkManifest::prepare(
    JobId::from_uuid(Uuid::new_v4()).unwrap(),
    BuildLogStream::Stdout,
    10,
    12,
    &bytes,
  )
  .unwrap();
  assert_eq!(
    store.put_verified(&manifest, bytes.clone()).await.unwrap(),
    LogChunkWrite::Written
  );
  assert_eq!(
    store.put_verified(&manifest, bytes.clone()).await.unwrap(),
    LogChunkWrite::AlreadyPresent
  );
  assert_eq!(store.read_verified(&manifest).await.unwrap(), bytes);
  assert_eq!(
    store.put_verified(&manifest, b"different bytes".to_vec()).await,
    Err(LogChunkStoreError::Integrity)
  );
  store.delete_chunk(&manifest).await.unwrap();
  assert_eq!(store.read_verified(&manifest).await, Err(LogChunkStoreError::NotFound));
}

async fn verify_cache_blob_contract(store: &S3ArtifactStore) {
  let bytes = b"verified-cache-bundle".to_vec();
  let descriptor = BlobDescriptor {
    digest: Digest::blake3(&bytes),
    encoding: BlobEncoding::Identity,
    encoded_size_bytes: bytes.len() as u64,
    expanded_size_bytes: bytes.len() as u64,
    entry_count: 1,
  };
  let object = CacheBlobObject::new("a".repeat(64), descriptor).unwrap();
  assert_eq!(
    store.put_if_absent(&object, bytes.clone()).await.unwrap(),
    CacheBlobWrite::Written
  );
  assert_eq!(
    store.put_if_absent(&object, bytes.clone()).await.unwrap(),
    CacheBlobWrite::AlreadyPresent
  );
  assert_eq!(store.read(&object).await.unwrap(), bytes);
  assert!(matches!(
    store.put_if_absent(&object, b"different-cache-bytes".to_vec()).await,
    Err(CacheBlobStoreError::Integrity(_))
  ));
  CacheBlobStore::delete(store, &object).await.unwrap();
  assert_eq!(store.read(&object).await, Err(CacheBlobStoreError::NotFound));
}

async fn verify_upload_download_and_delete(store: &S3ArtifactStore) {
  let bytes = b"verified-minio-artifact";
  let stored = object(bytes);
  upload(store, &stored, bytes).await;
  store.complete_upload(&stored).await.unwrap();
  store.complete_upload(&stored).await.unwrap();

  let empty = object(b"");
  upload(store, &empty, b"").await;
  store.complete_upload(&empty).await.unwrap();
  let empty_download = store.authorize_download(&empty, Duration::from_secs(60)).await.unwrap();
  assert!(
    reqwest::get(empty_download.url)
      .await
      .unwrap()
      .bytes()
      .await
      .unwrap()
      .is_empty()
  );

  let download = store
    .authorize_download(&stored, Duration::from_secs(60))
    .await
    .unwrap();
  assert_eq!(
    reqwest::get(download.url).await.unwrap().bytes().await.unwrap(),
    bytes.as_slice()
  );

  let corrupt = object(b"expected");
  let target = store.authorize_upload(&corrupt, Duration::from_secs(60)).await.unwrap();
  let response = put_request(target, b"different").send().await.unwrap();
  assert!(
    !response.status().is_success(),
    "MinIO accepted bytes that violate the signed checksum"
  );

  ArtifactStore::delete(store, &stored).await.unwrap();
  ArtifactStore::delete(store, &empty).await.unwrap();
  assert!(
    store
      .authorize_download(&stored, Duration::from_secs(60))
      .await
      .is_err()
  );
}

async fn verify_mismatched_generations_remain_unpublished(client: &Client, bucket: &str, store: &S3ArtifactStore) {
  let digest_mismatch = object(b"expected");
  inject_pending_generation(client, bucket, &digest_mismatch, b"altered!").await;
  assert!(matches!(
    store.complete_upload(&digest_mismatch).await,
    Err(ArtifactStoreError::Integrity {
      reason: ArtifactIntegrityError::DigestMismatch | ArtifactIntegrityError::BodyMismatch
    })
  ));
  assert!(matches!(
    store
      .authorize_download(&digest_mismatch, Duration::from_secs(60))
      .await,
    Err(ArtifactStoreError::NotFound)
  ));

  let size_mismatch = object(b"expected-size");
  inject_pending_generation(client, bucket, &size_mismatch, b"short").await;
  assert!(matches!(
    store.complete_upload(&size_mismatch).await,
    Err(ArtifactStoreError::Integrity {
      reason: ArtifactIntegrityError::SizeMismatch
    })
  ));
  assert!(matches!(
    store.authorize_download(&size_mismatch, Duration::from_secs(60)).await,
    Err(ArtifactStoreError::NotFound)
  ));
}

async fn verify_capability_expiration(store: &S3ArtifactStore) {
  let pending = object(b"expired-upload");
  let expired_upload = store.authorize_upload(&pending, Duration::from_secs(1)).await.unwrap();

  let published = object(b"expired-download");
  upload(store, &published, b"expired-download").await;
  store.complete_upload(&published).await.unwrap();
  let expired_download = store
    .authorize_download(&published, Duration::from_secs(1))
    .await
    .unwrap();

  tokio::time::sleep(Duration::from_secs(2)).await;
  assert!(
    !put_request(expired_upload, b"expired-upload")
      .send()
      .await
      .unwrap()
      .status()
      .is_success()
  );
  assert!(!reqwest::get(expired_download.url).await.unwrap().status().is_success());
  assert!(matches!(
    store.complete_upload(&pending).await,
    Err(ArtifactStoreError::NotFound)
  ));
  assert!(matches!(
    store.authorize_download(&pending, Duration::from_secs(60)).await,
    Err(ArtifactStoreError::NotFound)
  ));
  ArtifactStore::delete(store, &published).await.unwrap();
}

async fn verify_outage_and_recovery(proxy: &FaultProxy, store: &S3ArtifactStore) {
  let bytes = b"survives-storage-outage";
  let object = object(bytes);
  upload(store, &object, bytes).await;

  proxy.set_available(false);
  assert!(matches!(
    store.complete_upload(&object).await,
    Err(
      ArtifactStoreError::Backend {
        operation: ArtifactStoreOperation::CompleteUpload
      } | ArtifactStoreError::TimedOut {
        operation: ArtifactStoreOperation::CompleteUpload
      }
    )
  ));

  proxy.set_available(true);
  store.health_check().await.unwrap();
  assert!(matches!(
    store.authorize_download(&object, Duration::from_secs(60)).await,
    Err(ArtifactStoreError::NotFound)
  ));
  store.complete_upload(&object).await.unwrap();
  let download = store
    .authorize_download(&object, Duration::from_secs(60))
    .await
    .unwrap();
  assert_eq!(
    reqwest::get(download.url).await.unwrap().bytes().await.unwrap(),
    bytes.as_slice()
  );
  ArtifactStore::delete(store, &object).await.unwrap();
}

async fn inject_pending_generation(client: &Client, bucket: &str, object: &ArtifactObject, bytes: &'static [u8]) {
  client
    .put_object()
    .bucket(bucket)
    .key(format!("{TEST_PREFIX}/uploads/{}", object.upload_id()))
    .content_type(object.content_type())
    .metadata("octacity-sha256", object.sha256())
    .metadata("octacity-size", object.size_bytes().to_string())
    .body(ByteStream::from_static(bytes))
    .send()
    .await
    .unwrap();
}

async fn delete_test_objects(client: &Client, bucket: &str) {
  let listed = client.list_objects_v2().bucket(bucket).send().await.unwrap();
  for object in listed.contents() {
    client
      .delete_object()
      .bucket(bucket)
      .key(object.key().unwrap())
      .send()
      .await
      .unwrap();
  }
}

async fn upload(store: &S3ArtifactStore, object: &ArtifactObject, bytes: &[u8]) {
  let authorization = store.authorize_upload(object, Duration::from_secs(60)).await.unwrap();
  put_request(authorization, bytes)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
}

fn put_request(authorization: UploadAuthorization, bytes: &[u8]) -> reqwest::RequestBuilder {
  let mut request = reqwest::Client::new().put(&authorization.url).body(bytes.to_vec());
  for (name, value) in authorization.required_headers {
    request = request.header(
      HeaderName::try_from(name).unwrap(),
      HeaderValue::try_from(value).unwrap(),
    );
  }
  request
}

fn required(name: &str) -> String {
  env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this explicit MinIO contract test"))
}

fn minio_client(endpoint: &str, region: &str, access_key: &str, secret_key: &str) -> Client {
  let config = aws_sdk_s3::Config::builder()
    .behavior_version(BehaviorVersion::latest())
    .endpoint_url(endpoint)
    .region(Region::new(region.to_owned()))
    .credentials_provider(Credentials::new(
      access_key,
      secret_key,
      None,
      None,
      "octacity-minio-test",
    ))
    .force_path_style(true)
    .build();
  Client::from_conf(config)
}

fn object(bytes: &[u8]) -> ArtifactObject {
  ArtifactObject::new(
    ArtifactId::generate(),
    ArtifactUploadId::generate(),
    bytes.len() as u64,
    format!("{:x}", Sha256::digest(bytes)),
    "application/octet-stream",
  )
  .unwrap()
}

struct FaultProxy {
  endpoint: String,
  control: Arc<ProxyControl>,
  shutdown: CancellationToken,
  task: JoinHandle<()>,
}

struct ProxyControl {
  available: AtomicBool,
  connections: Mutex<CancellationToken>,
}

impl FaultProxy {
  async fn start(upstream_endpoint: &str) -> Self {
    let upstream = reqwest::Url::parse(upstream_endpoint).unwrap();
    assert_eq!(upstream.scheme(), "http", "the MinIO contract proxy expects HTTP");
    let upstream_host = upstream.host_str().unwrap().to_owned();
    let upstream_port = upstream.port_or_known_default().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let control = Arc::new(ProxyControl {
      available: AtomicBool::new(true),
      connections: Mutex::new(CancellationToken::new()),
    });
    let shutdown = CancellationToken::new();
    let task_control = Arc::clone(&control);
    let task_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
      loop {
        let accepted = tokio::select! {
          () = task_shutdown.cancelled() => break,
          accepted = listener.accept() => accepted,
        };
        let Ok((mut client, _)) = accepted else {
          break;
        };
        if !task_control.available.load(Ordering::SeqCst) {
          continue;
        }
        let connection_cancellation = task_control.connections.lock().unwrap().clone();
        let upstream_host = upstream_host.clone();
        tokio::spawn(async move {
          let Ok(mut upstream) = tokio::net::TcpStream::connect((upstream_host.as_str(), upstream_port)).await else {
            return;
          };
          tokio::select! {
            () = connection_cancellation.cancelled() => {}
            _ = copy_bidirectional(&mut client, &mut upstream) => {}
          }
        });
      }
    });
    Self {
      endpoint,
      control,
      shutdown,
      task,
    }
  }

  fn endpoint(&self) -> String {
    self.endpoint.clone()
  }

  fn set_available(&self, available: bool) {
    if available {
      *self.control.connections.lock().unwrap() = CancellationToken::new();
      self.control.available.store(true, Ordering::SeqCst);
    } else {
      self.control.available.store(false, Ordering::SeqCst);
      self.control.connections.lock().unwrap().cancel();
    }
  }

  async fn shutdown(self) {
    self.control.connections.lock().unwrap().cancel();
    self.shutdown.cancel();
    self.task.await.unwrap();
  }
}
