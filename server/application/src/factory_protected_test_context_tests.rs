use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
fn key(s: &str) -> FactoryKey {
  FactoryKey::new(s).unwrap()
}
struct Plan(ProtectedTestPlan);
#[async_trait]
impl FactoryProtectedTestPlanSource for Plan {
  async fn frozen_plan(&self, _: &FlowBuildIntent) -> Result<ProtectedTestPlan, ApplicationError> {
    Ok(self.0.clone())
  }
}
struct InventoryBytes {
  id: ArtifactId,
  bytes: Vec<u8>,
  reads: AtomicUsize,
}
#[async_trait]
impl ArtifactByteReader for InventoryBytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    assert_eq!(
      object.artifact_id(),
      self.id,
      "private test source bytes are only transferred to the signed validator Job"
    );
    assert!(self.bytes.len() as u64 <= maximum);
    self.reads.fetch_add(1, Ordering::SeqCst);
    Ok(self.bytes.clone())
  }
}
async fn publish(
  artifacts: &InMemoryArtifactRecordStore,
  document: &FactoryNodeBuildOutputDocument,
  lease: LeaseAccess,
) {
  let id = document.record.identity();
  let upload = artifacts
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: id.artifact_id,
      upload_id: ArtifactUploadId::generate(),
      idempotency_key: IdempotencyKey::new(id.logical_name.as_str()).unwrap(),
      lease,
      job_id: id.job_id,
      attempt: AttemptNumber::FIRST,
      logical_name: id.logical_name.clone(),
      producer_run_id: 1,
      producer_task_id: 1,
      artifact_type: id.artifact_type.clone(),
      media_type: id.media_type.clone(),
      transport_media_type: id.media_type.clone(),
      size_bytes: id.size_bytes,
      digest: id.digest,
      reserved_at: at(30),
      capability_expires_at: at(1000),
    })
    .await
    .unwrap();
  let verify = VerifyArtifactUpload {
    upload_id: upload.upload.upload_id,
    lease,
    job_id: id.job_id,
    attempt: AttemptNumber::FIRST,
    observed_at: at(30),
  };
  artifacts.begin_artifact_verification(verify).await.unwrap();
  artifacts
    .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
}
#[test]
fn retained_protected_tests_restore_exact_selected_inventory_and_file_identities_after_restart() {
  let fixture = crate::factory_protected_test_tests::retained_author_fixture();
  crate::factory_admission_tests::run_ready(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let run = FactoryRun::admitted(fixture.intent.input().factory_run_id(), &fixture.work);
    let audit = management_mutation_with_request((), "test-inventory-retention")
      .audit()
      .clone();
    store
      .seed_factory_run(
        PublishedFactoryAdmission {
          work: fixture.work.clone(),
          run: run.clone(),
          flow: fixture.admitted.clone(),
          admitted_at: at(1),
        },
        FactoryDigest::from_bytes([1; 32]),
        &audit,
      )
      .unwrap();
    let claim = FactoryRunClaimRecord::new(
      run.id(),
      fixture.intent.node().owner().clone(),
      fixture.intent.node().claim(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([1; 32]),
          key("accepted"),
          at(10),
        ),
      })
      .await
      .unwrap();
    let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
    let mut start = factory_flow_contract_transition(&snapshot, &claim, 11);
    start.append.flow.attempts.push(fixture.intent.node().clone());
    start.append.flow.data.inputs.push(fixture.intent.input().clone());
    start.append.flow.data.build_intents.push(fixture.intent.clone());
    start.budget = FactoryBudgetRecord::new(
      run.id(),
      start.next_run.version(),
      BudgetUsage {
        attempts: 1,
        ..Default::default()
      },
      at(11),
    );
    start.current.budget_id = start.budget.id;
    store.commit_factory_run_transition(start).await.unwrap();
    let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
    let mut done = factory_flow_contract_transition(&snapshot, &claim, 32);
    done.append.flow.data.build_executions.push(fixture.execution.clone());
    done.append.flow.data.records.push(fixture.record.clone());
    done.append.flow.completions.push(fixture.completion.clone());
    let mut usage = fixture.completion.usage();
    usage.attempts += 1;
    done.budget = FactoryBudgetRecord::new(run.id(), done.next_run.version(), usage, at(32));
    done.current.budget_id = done.budget.id;
    store.commit_factory_run_transition(done).await.unwrap();
    let id = fixture.manifest.record.identity();
    let lease = LeaseAccess {
      lease_id: id.lease_id,
      fence: LeaseFence::from_bytes([1; 32]),
      agent_id: AgentId::generate(),
      registration_epoch: RegistrationEpoch::new(1).unwrap(),
    };
    let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
      access: lease,
      build_id: id.build_id,
      attempt_id: id.attempt_id,
      attempt: AttemptNumber::FIRST,
      job_id: id.job_id,
      expires_at: at(1000),
    }));
    publish(&artifacts, &fixture.manifest, lease).await;
    publish(&artifacts, &fixture.file, lease).await;
    let bytes = Arc::new(InventoryBytes {
      id: id.artifact_id,
      bytes: fixture.manifest.bytes.clone(),
      reads: AtomicUsize::new(0),
    });
    let prior = fixture.intent.input().context().unwrap();
    let artifact = fixture.bundle.artifact().clone();
    let selection = ContextManifestEntry::new(
      ContextSourceKind::Evidence,
      key("selected-private-tests"),
      prior.subject().clone(),
      FactoryContextReference::Artifact(artifact.clone()),
      artifact.content_digest(),
      artifact.encoded_size(),
      FactorySafeText::new("Explicit retained test inventory").unwrap(),
      FactoryDigest::from_bytes([3; 32]),
    )
    .unwrap();
    let context = ContextManifest::new(
      ContextManifestId::generate(),
      prior.subject().clone(),
      prior.construction_policy_digest(),
      vec![selection],
    )
    .unwrap();
    let input = fixture.intent.input().clone().with_context(context, vec![]).unwrap();
    let reader = Arc::new(FactoryArtifactReader::new(artifacts, fixture.builds, bytes.clone()));
    let source = FactoryStoredProtectedTestContextSource::new(
      store.clone(),
      reader.clone(),
      Arc::new(Plan(fixture.bundle.plan().clone())),
      fixture.requirement.clone(),
      crate::factory_node_test_support::clock(),
    );
    assert!(
      source.read(fixture.intent.input(), at(32), at(899)).await.is_err(),
      "a producing execution is not an implicit private-inventory selection"
    );
    assert_eq!(bytes.reads.load(Ordering::SeqCst), 0);
    let restored = source.read(&input, at(32), at(899)).await.unwrap();
    assert_eq!(restored.bundle(), &fixture.bundle);
    assert_eq!(restored.files().len(), 1);
    assert_eq!(
      restored.files()[0].identity().artifact_id,
      fixture.file.record.identity().artifact_id
    );
    assert_eq!(restored.manifest().identity().artifact_id, id.artifact_id);
    assert_eq!(bytes.reads.load(Ordering::SeqCst), 1);
    let restarted = FactoryStoredProtectedTestContextSource::new(
      store,
      reader,
      Arc::new(Plan(fixture.bundle.plan().clone())),
      fixture.requirement,
      crate::factory_node_test_support::clock(),
    );
    let again = restarted.read(&input, at(33), at(899)).await.unwrap();
    assert_eq!(again.bundle(), restored.bundle());
    assert_eq!(again.files(), restored.files());
    assert_eq!(
      bytes.reads.load(Ordering::SeqCst),
      2,
      "new owner restores immutable inventory from retained bytes"
    );
  });
}
