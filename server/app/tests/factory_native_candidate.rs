//! Default native readonly candidate composition, not an Agent capture/model pilot.
use octacity_artifact_store::{ArtifactByteReader as _, ArtifactObject};
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use std::{sync::Arc, time::Duration};

#[path = "support/factory_native_candidate.rs"]
mod candidate_fixture;
#[allow(dead_code)]
#[path = "../../application/src/factory_job_test_support.rs"]
mod managed;
#[allow(dead_code)]
#[path = "support/factory_native_profiles.rs"]
mod native_fixture;
#[path = "support/factory_observability.rs"]
mod observability_fixture;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[allow(dead_code)]
#[path = "support/runtime.rs"]
mod runtime_fixture;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and object storage"]
async fn default_native_runtime_materializes_one_exact_readonly_candidate_and_recovers_its_inputs() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let project = ProjectId::generate();
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    FactoryConfigurationRef::new(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([3; 32]),
    ),
    ExternalWorkIdentity::new("native-readonly-candidate-observation").unwrap(),
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
  let mut profile = native_fixture::publish_ordinary_configuration(&store, &work).await;
  profile.candidate_capture_tool = Some(candidate_fixture::capture_tool());
  let plan = candidate_fixture::candidate_plan(&work, &profile);
  let admission = native_fixture::admit_flow(&store, work, &profile, plan.flow.clone()).await;
  let objects = candidate_fixture::objects();
  let artifacts =
    candidate_fixture::precomplete_producer(&database, store.clone(), objects.clone(), &profile, &admission, &plan)
      .await;
  let prepared_snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let prepared_reader = Arc::new(FactoryArtifactReader::new(
    store.clone(),
    Arc::new(PostgresAuthoritativeStore::new(
      database.pool.clone(),
      postgres::test_signer(),
    )),
    objects.clone(),
  ));
  FactoryArtifactFlowInputs::new(prepared_reader, Arc::new(SystemFactoryExecutionClock))
    .prepare(
      &prepared_snapshot,
      &FactoryNodeTarget {
        flow_run_id: plan.flow.root_run().id(),
        cycle_id: plan.flow.initial_cycle().id(),
        node: plan.consumer.clone(),
      },
    )
    .await
    .expect("native candidate frozen input fixture");
  let directory = tempfile::tempdir().unwrap();
  let config = native_fixture::runtime_config(directory.path(), &database.name, &profile);
  let events = observability_fixture::capture();
  let runtime = octacity_server::ServerRuntime::start(config.clone()).await.unwrap();
  let authority = Arc::new(PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres::test_signer(),
  ));
  let observation = tokio::time::timeout(Duration::from_secs(30), async {
    let mut cadence = tokio::time::interval(Duration::from_millis(25));
    loop {
      cadence.tick().await;
      let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
      let Some(intent) = snapshot
        .flow
        .data
        .build_intents
        .iter()
        .find(|intent| intent.input().node() == &plan.consumer)
        .cloned()
      else {
        continue;
      };
      let accepted = authority
        .factory_build_for_operation(intent.operation_id().unwrap())
        .await
        .unwrap();
      if accepted.is_some() {
        break (intent, accepted);
      }
      let failed = events
        .lock()
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .any(|event| {
          event["fields"]["worker"] == "factory-owner-worker"
            && event["fields"]["message"] == "durable worker pass failed"
        });
      if failed {
        break (intent, None);
      }
    }
  })
  .await;
  let observation = match observation {
    Ok(observation) => observation,
    Err(error) => {
      let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
      let safe_events: Vec<_> = events
        .lock()
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|event| event["fields"]["worker"] == "factory-owner-worker")
        .collect();
      runtime.shutdown().await.unwrap();
      panic!(
        "native candidate fixture did not reach consumer observation: {error}; attempts={}, intents={}, records={}, completions={}, stops={}; claim={:?}; owner events={safe_events:?}",
        snapshot.flow.attempts.len(),
        snapshot.flow.data.build_intents.len(),
        snapshot.flow.data.records.len(),
        snapshot.flow.completions.len(),
        snapshot.flow.data.stops.len(),
        snapshot.current_claim
      );
    }
  };
  let (intent, accepted) = observation;
  // Independently prove that this is a valid persisted selection. Thus missing
  // default-runtime wiring cannot be confused with malformed fixture context.
  let reader = Arc::new(FactoryArtifactReader::new(
    store.clone(),
    authority.clone(),
    objects.clone(),
  ));
  let source = FactoryCodexCandidateContextSource::new(
    store.clone(),
    Arc::new(FactoryCandidateContextSource::new(reader)),
    vec![profile.clone()],
    Arc::new(SystemFactoryExecutionClock),
  )
  .unwrap();
  let at = candidate_fixture::now();
  let context = source
    .read_retained_candidate_context(&intent, at, Timestamp::from_unix_millis(at.unix_millis() + 1).unwrap())
    .await
    .unwrap();
  assert_eq!(context.input_digest(), intent.input().digest().unwrap());
  assert!(
    context.model_context().is_empty(),
    "source artifacts grant no model text"
  );
  if accepted.is_none() {
    runtime.shutdown().await.unwrap();
    database.cleanup().await;
    panic!("default native runtime rejected a valid persisted readonly candidate before ordinary Build creation");
  }
  let accepted = accepted.unwrap();
  assert_eq!(accepted.job_ids.len(), 1);
  let job = store.job(accepted.job_ids[0]).await.unwrap();
  let managed = job.job.job_spec_template.managed_intent().unwrap();
  let protected = managed.protected_inputs().clone();
  assert_eq!(protected.inputs.len(), 6);
  let instruction = managed.factory().unwrap().change_set_materialization.as_ref().unwrap();
  assert_eq!(instruction.candidate_revision, candidate_fixture::CANDIDATE);
  assert_eq!(
    instruction.bundle_input,
    artifacts[0].identity().artifact_id.to_string()
  );
  assert_eq!(
    instruction.manifest_input,
    artifacts[1].identity().artifact_id.to_string()
  );
  assert!(managed.factory().unwrap().source_write_paths.is_none());
  assert!(managed.factory().unwrap().change_set_capture.is_none());
  assert!(managed.execution().tool_control.is_none());
  assert!(managed.permissions().tools.is_empty());
  assert!(
    managed
      .permissions()
      .mounts
      .iter()
      .filter(|mount| mount.root == octacity_protocol::FACTORY_SOURCE_ROOT)
      .all(|mount| mount.mode == octacity_protocol::FactoryMountModeV3::ReadOnly)
  );
  for input in &protected.inputs {
    let published = store
      .published_execution_input(input.artifact_id.parse().unwrap())
      .await
      .unwrap();
    assert_eq!(published.digest.to_string(), input.sha256);
    assert_eq!(published.size_bytes, input.size_bytes);
    if input.artifact_id != instruction.bundle_input && input.artifact_id != instruction.manifest_input {
      let bytes = objects
        .read_published(
          &ArtifactObject::new(
            published.artifact_id,
            published.upload_id,
            published.size_bytes,
            published.digest.to_string(),
            published.media_type.as_str(),
          )
          .unwrap(),
          published.size_bytes,
        )
        .await
        .unwrap();
      assert!(
        !bytes
          .windows(b"retained native candidate bundle canary".len())
          .any(|window| window == b"retained native candidate bundle canary")
      );
    }
  }
  let frozen = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  runtime.shutdown().await.unwrap();
  let restarted = octacity_server::ServerRuntime::start(config).await.unwrap();
  assert_eq!(
    authority
      .factory_build_for_operation(intent.operation_id().unwrap())
      .await
      .unwrap()
      .unwrap(),
    accepted
  );
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
  let recovered = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(recovered.flow.data.inputs, frozen.flow.data.inputs);
  let at = candidate_fixture::now();
  let retained = source
    .read_retained_candidate_context(&intent, at, Timestamp::from_unix_millis(at.unix_millis() + 1).unwrap())
    .await
    .unwrap();
  assert_eq!(retained.input_digest(), context.input_digest());
  assert_eq!(retained.bundle(), context.bundle());
  assert_eq!(retained.manifest(), context.manifest());
  restarted.shutdown().await.unwrap();
  database.cleanup().await;
}
