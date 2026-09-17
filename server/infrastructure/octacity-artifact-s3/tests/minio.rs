//! Opt-in contract test against a real S3-compatible MinIO service.
//!
//! Set `OCTACITY_MINIO_ENDPOINT`, `OCTACITY_MINIO_ACCESS_KEY`, and
//! `OCTACITY_MINIO_SECRET_KEY`, then run this ignored test explicitly. The test
//! creates and removes its own bucket and validates direct PUT, verified
//! publication, idempotent completion, download, corruption rejection, and
//! empty objects, and deletion through the public `ArtifactStore` boundary.

use std::{env, time::Duration};

use aws_sdk_s3::{
  Client,
  config::{BehaviorVersion, Credentials, Region},
};
use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_artifact_store::{ArtifactId, ArtifactObject, ArtifactStore, ArtifactUploadId, UploadAuthorization};
use reqwest::header::{HeaderName, HeaderValue};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

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

  let store = S3ArtifactStore::new(S3ArtifactStoreConfig {
    endpoint,
    region,
    bucket: bucket.clone(),
    prefix: "phase6".to_owned(),
    access_key: zeroize::Zeroizing::new(access_key),
    secret_key: zeroize::Zeroizing::new(secret_key),
    force_path_style: true,
    operation_timeout: Duration::from_secs(30),
    capability_recheck_interval: Duration::from_secs(300),
  })
  .unwrap();
  store.health_check().await.unwrap();
  store.health_check().await.unwrap();
  let bytes = b"verified-minio-artifact";
  let stored = object(bytes);
  upload(&store, &stored, bytes).await;
  store.complete_upload(&stored).await.unwrap();
  store.complete_upload(&stored).await.unwrap();

  let empty = object(b"");
  upload(&store, &empty, b"").await;
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

  store.delete(&stored).await.unwrap();
  store.delete(&empty).await.unwrap();
  assert!(
    store
      .authorize_download(&stored, Duration::from_secs(60))
      .await
      .is_err()
  );
  delete_test_objects(&client, &bucket).await;
  client.delete_bucket().bucket(bucket).send().await.unwrap();
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
