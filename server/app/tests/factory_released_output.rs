//! Read-only reproduction over a retained real packaged-Agent Job.
use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_artifact_store::MAX_ARTIFACT_READ_BYTES;
use octacity_server_application::{
  FactoryArtifactReader, FactoryCodexBuildOutputSource, FactoryExecutionClock, FactoryNodeBuildOutputObservation,
  FactoryNodeBuildOutputSource, ReadFactoryArtifact,
};
use octacity_server_domain::{JobId, Timestamp};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest};
use octacity_server_store::{ArtifactListVisibility, ArtifactRecordStore, BuildQueryStore, ListPublishedArtifacts};
use octacity_server_store_postgres::PostgresStore;
use std::{
  path::PathBuf,
  sync::Arc,
  time::{Duration, SystemTime, UNIX_EPOCH},
};
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, octacity_server_application::ApplicationError> {
    Timestamp::from_unix_millis(
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap(),
    )
    .map_err(|_| octacity_server_application::ApplicationError::unavailable())
  }
}
#[tokio::test]
#[ignore = "requires a retained actual pilot directory/Job; performs reads only"]
async fn retained_released_codex_job_passes_native_output_verification() {
  let root =
    PathBuf::from(std::env::var("OCTACITY_FACTORY_REPRO_ROOT").expect("explicit private pilot directory required"));
  let job: JobId = std::env::var("OCTACITY_FACTORY_REPRO_JOB").unwrap().parse().unwrap();
  let config: toml::Value = toml::from_str(&std::fs::read_to_string(root.join("server.toml")).unwrap()).unwrap();
  let read = |value: &toml::Value| {
    std::fs::read_to_string(value.as_str().unwrap())
      .unwrap()
      .trim()
      .to_owned()
  };
  let pool = sqlx::PgPool::connect(&read(&config["postgres"]["url_file"]))
    .await
    .unwrap();
  let store = Arc::new(PostgresStore::new(pool.clone()));
  let storage = &config["object_storage"];
  let bytes = Arc::new(
    S3ArtifactStore::new(S3ArtifactStoreConfig {
      endpoint: storage["endpoint"].as_str().unwrap().to_owned(),
      region: storage["region"].as_str().unwrap().to_owned(),
      bucket: storage["bucket"].as_str().unwrap().to_owned(),
      prefix: storage
        .get("prefix")
        .and_then(toml::Value::as_str)
        .unwrap_or("")
        .to_owned(),
      access_key: zeroize::Zeroizing::new(read(&storage["access_key_file"])),
      secret_key: zeroize::Zeroizing::new(read(&storage["secret_key_file"])),
      force_path_style: storage["force_path_style"].as_bool().unwrap(),
      operation_timeout: Duration::from_secs(5),
      capability_recheck_interval: Duration::from_secs(30),
    })
    .unwrap(),
  );
  let job = store.job(job).await.unwrap();
  let attempt = store.attempt(job.attempt_id).await.unwrap();
  let build = store.build(attempt.build_id).await.unwrap();
  let artifacts = store
    .list_published_artifacts(ListPublishedArtifacts {
      build_id: attempt.build_id,
      attempt_id: Some(job.attempt_id),
      limit: 16,
      visibility: ArtifactListVisibility::all(),
    })
    .await
    .unwrap();
  assert_eq!(
    artifacts.len(),
    3,
    "real observer Job must publish exactly the installed CodeX record set"
  );
  let reader = Arc::new(FactoryArtifactReader::new(store.clone(), store.clone(), bytes.clone()));
  let now = Clock.now().unwrap();
  let until = Timestamp::from_unix_millis(now.unix_millis() + 1).unwrap();
  for upload in artifacts {
    let id = upload.artifact.identity();
    reader
      .read(ReadFactoryArtifact {
        project_id: build.build.project_id,
        artifact: FactoryArtifactReference::new(
          id.artifact_id,
          FactoryDigest::from_bytes(id.digest.as_bytes()),
          id.size_bytes,
        )
        .unwrap(),
        maximum_bytes: MAX_ARTIFACT_READ_BYTES,
        observed_at: now,
        required_until: until,
      })
      .await
      .expect("real published output bytes must pass Project/retention/digest/size and S3 verification");
  }
  let catalog = octacity_server::FactoryRuntimeProfileCatalog::load(&root.join("profiles.json"))
    .await
    .unwrap();
  let source = FactoryCodexBuildOutputSource::new(
    store.clone(),
    store.clone(),
    store.clone(),
    bytes,
    catalog.codex,
    reader,
    Arc::new(Clock),
  )
  .unwrap();
  let observed = source
    .published_outputs(attempt.build_id, attempt.id, MAX_ARTIFACT_READ_BYTES)
    .await
    .expect("native installed compiler and real CodeX records must agree");
  assert!(
    matches!(observed, FactoryNodeBuildOutputObservation::Verified(_)),
    "actual successful Job must retain a verified semantic result"
  );
  pool.close().await;
}
