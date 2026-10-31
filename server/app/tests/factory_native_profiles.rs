//! Native profile-file composition through ordinary PostgreSQL Build acceptance.
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use std::time::Duration;

#[allow(dead_code)]
#[path = "../../application/src/factory_job_test_support.rs"]
mod managed;
#[path = "support/factory_native_profiles.rs"]
mod native_fixture;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[allow(dead_code)]
#[path = "support/runtime.rs"]
mod runtime_fixture;
use native_fixture::{admit, publish_ordinary_configuration};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and object storage"]
async fn default_native_profiles_create_one_ordinary_build_and_recover_its_protected_manifest() {
  let database = postgres::TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = ProjectId::generate();
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    FactoryConfigurationRef::new(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([3; 32]),
    ),
    ExternalWorkIdentity::new("native-pilot-observation").unwrap(),
    ExactSubject::new(
      project,
      RepositoryId::generate(),
      ImmutableRevision::new("abc123").unwrap(),
    ),
    WorkArtifacts::new(
      FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([1; 32]), 10).unwrap(),
      FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([2; 32]), 10).unwrap(),
      vec![],
    )
    .unwrap(),
    WorkClassification::new(WorkPriority::new(1).unwrap(), RiskClass::Low, Default::default()),
  )
  .unwrap();
  let profile = publish_ordinary_configuration(&store, &work).await;
  let admission = admit(&store, work, &profile).await;
  assert_eq!(
    store.factory_run_snapshot(admission.run.id()).await.unwrap().run,
    admission.run,
    "the public admission must be readable before starting native workers"
  );
  let directory = tempfile::tempdir().unwrap();
  let config = native_fixture::runtime_config(directory.path(), &database.name, &profile);
  let runtime = octacity_server::ServerRuntime::start(config.clone()).await.unwrap();
  let authority = PostgresAuthoritativeStore::new(database.pool.clone(), postgres::test_signer());
  let accepted = tokio::time::timeout(Duration::from_secs(30), async {
    loop {
      let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
      if let Some(intent) = snapshot.flow.data.build_intents.first()
        && let Some(build) = authority
          .factory_build_for_operation(intent.operation_id().unwrap())
          .await
          .unwrap()
      {
        break build;
      }
      tokio::time::sleep(Duration::from_millis(25)).await;
    }
  })
  .await
  .expect("native configured owner must create an ordinary Build without a custom installer");
  assert_eq!(accepted.job_ids.len(), 1);
  let ordinary = store.build(accepted.build_id).await.unwrap();
  assert!(matches!(ordinary.origin, BuildOrigin::Factory(_)));
  assert_eq!(
    ordinary.build.immutable_revision,
    Some(admission.work.subject().repository().unwrap().base_revision().clone())
  );
  let job = store.job(accepted.job_ids[0]).await.unwrap();
  assert_eq!(job.job.pipeline_node_id, profile.pipeline_node);
  let protected = job
    .job
    .job_spec_template
    .managed_intent()
    .unwrap()
    .protected_inputs()
    .clone();
  assert_eq!(protected.inputs.len(), 4);
  assert!(
    protected
      .inputs
      .iter()
      .any(|input| input.destination == "/octacity/protected/Octafile.yml")
  );
  for input in &protected.inputs {
    let published = store
      .published_execution_input(input.artifact_id.parse().unwrap())
      .await
      .unwrap();
    assert_eq!(published.digest.to_string(), input.sha256);
    assert_eq!(published.size_bytes, input.size_bytes);
  }
  let frozen = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  runtime.shutdown().await.unwrap();
  let restarted = octacity_server::ServerRuntime::start(config).await.unwrap();
  let recovered = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(recovered.flow.attempts, frozen.flow.attempts);
  assert_eq!(recovered.flow.data.inputs, frozen.flow.data.inputs);
  assert_eq!(recovered.flow.data.build_intents, frozen.flow.data.build_intents);
  let replayed = authority
    .factory_build_for_operation(frozen.flow.data.build_intents[0].operation_id().unwrap())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(replayed, accepted);
  assert_eq!(
    store
      .job(accepted.job_ids[0])
      .await
      .unwrap()
      .job
      .job_spec_template
      .managed_intent()
      .unwrap()
      .protected_inputs(),
    &protected
  );
  restarted.shutdown().await.unwrap();
  database.cleanup().await;
}
