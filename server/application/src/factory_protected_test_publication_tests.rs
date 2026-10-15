//! Inventory publication must reference actual fenced output reservations.
use crate::factory_node_test_support::{Builds, fixtures::*};
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};

fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(50))
  }
}
#[derive(Default)]
struct Bytes(Mutex<BTreeMap<ArtifactId, Vec<u8>>>, std::sync::atomic::AtomicUsize);
#[async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    self.0.lock().unwrap().insert(object.artifact_id(), bytes);
    Ok(())
  }
}

struct Fixture {
  admitted: AdmittedFlow,
  intent: FlowBuildIntent,
  execution: FlowBuildExecution,
  lease: LeaseAccess,
  job_id: JobId,
  store: Arc<InMemoryArtifactRecordStore>,
  file: ArtifactUploadRecord,
  plan: ProtectedTestPlan,
  inventory: EvidenceRequirement,
  files: [FactoryProtectedTestFileOutput; 1],
  bytes: Arc<Bytes>,
}
async fn fixture() -> Fixture {
  fixture_with_schema(None).await
}
async fn fixture_with_schema(schema: Option<FlowDataSchema>) -> Fixture {
  let inventory = EvidenceRequirement::new(
    key("private.inventory"),
    EvidenceOutputKind::Artifact,
    reference("tests.inventory"),
    reference("audit.command"),
    reference("runner"),
  );
  let work = work();
  let mut binding = binding(work.subject().project_id());
  binding.evidence = vec![inventory.clone()];
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    outputs: FactoryOutputPermissions::try_new(
      vec![key("private.inventory"), key("hidden.contract"), key("audit.report")],
      4,
      9_000,
      1,
      1_000,
    )
    .unwrap(),
    ..FactoryPermissionDraft::default()
  })
  .unwrap();
  let (_, admitted, _, input, node) =
    execution_fixture_with_permissions(FlowNodeKind::Reasoning, work, binding, schema, permissions);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let lease = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([41; 32]),
    agent_id: AgentId::generate(),
    registration_epoch: RegistrationEpoch::new(3).unwrap(),
  };
  let job_id = execution.jobs()[0];
  let store = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access: lease,
    build_id: execution.build_id(),
    attempt_id: execution.attempt_id(),
    attempt: AttemptNumber::FIRST,
    job_id,
    expires_at: at(5_000),
  }));
  let content = b"assert_eq!(actual, PRIVATE_EXPECTATION);";
  let file = store
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: ArtifactId::generate(),
      upload_id: ArtifactUploadId::generate(),
      idempotency_key: IdempotencyKey::new("actual.private.file").unwrap(),
      lease,
      job_id,
      attempt: AttemptNumber::FIRST,
      logical_name: ArtifactName::new("hidden.contract").unwrap(),
      producer_run_id: 17,
      producer_task_id: 23,
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new("text/plain").unwrap(),
      transport_media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      size_bytes: content.len() as u64,
      digest: ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(content).as_bytes()),
      reserved_at: at(30),
      capability_expires_at: at(4_000),
    })
    .await
    .unwrap();
  let verification = VerifyArtifactUpload {
    upload_id: file.upload.upload_id,
    lease,
    job_id,
    attempt: AttemptNumber::FIRST,
    observed_at: at(31),
  };
  store.begin_artifact_verification(verification).await.unwrap();
  let file = store
    .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
  let plan = ProtectedTestPlan::new(
    FactoryDigest::from_bytes([27; 32]),
    intent.input().context().unwrap().subject().clone(),
    ProtectedTestCoverage::new(vec![key("project.contract")], 4, 9_000, 4).unwrap(),
    at(900),
  )
  .unwrap();
  let files = [FactoryProtectedTestFileOutput {
    output: key("hidden.contract"),
    class: key("project.contract"),
    path: FactoryRepositoryPath::new("contract.rs").unwrap(),
  }];
  let bytes = Arc::new(Bytes::default());
  Fixture {
    admitted,
    intent,
    execution,
    lease,
    job_id,
    store,
    file,
    plan,
    inventory,
    files,
    bytes,
  }
}

#[test]
fn protected_inventory_uses_published_file_ids_and_replays_the_same_publication() {
  crate::factory_admission_tests::run_ready(async {
    let Fixture {
      admitted,
      intent,
      execution,
      lease,
      job_id,
      store,
      file,
      plan,
      inventory,
      files,
      bytes,
    } = fixture().await;
    let publisher = FactoryProtectedTestInventoryPublisher::new(store.clone(), bytes.clone(), Arc::new(Clock));
    let request = FactoryProtectedTestInventoryPublication {
      admitted: &admitted,
      intent: &intent,
      execution: &execution,
      plan: &plan,
      inventory: &inventory,
      files: &files,
      completion: AgentOutputFinalization {
        lease,
        job_id,
        attempt: AttemptNumber::FIRST,
        observed_at: at(50),
      },
    };
    let first = publisher.publish(request).await.unwrap();
    let replay = publisher.publish(request).await.unwrap();
    assert_eq!(first, replay);
    assert_eq!(
      bytes.1.load(std::sync::atomic::Ordering::SeqCst),
      1,
      "replay must not rewrite existing object bytes"
    );
    let retained = store.published_artifact(first.artifact_id()).await.unwrap();
    assert_eq!(retained.artifact.identity().job_id, job_id);
    assert_eq!(retained.artifact.identity().lease_id, lease.lease_id);
    assert_eq!(retained.artifact.identity().logical_name.as_str(), "private.inventory");
    {
      let writes = bytes.0.lock().unwrap();
      assert_eq!(writes.len(), 1, "retry reuses the original published bytes");
      let raw = &writes[&first.artifact_id()];
      let captured: FactoryProtectedTestInventory = serde_json::from_slice(raw).unwrap();
      assert_eq!(captured.plan_digest, plan.digest().unwrap());
      assert_eq!(captured.input_digest, intent.input().digest().unwrap());
      assert_eq!(captured.node_attempt_id, intent.node().id());
      assert_eq!(captured.files.len(), 1);
      assert_eq!(
        captured.files[0].artifact.artifact_id(),
        file.artifact.identity().artifact_id
      );
      assert_eq!(
        captured.files[0].artifact.content_digest().as_bytes(),
        file.artifact.identity().digest.as_bytes()
      );
      assert_eq!(captured.files[0].class, key("project.contract"));
      assert_eq!(captured.files[0].path.as_str(), "contract.rs");
      assert!(!String::from_utf8_lossy(raw).contains("PRIVATE_EXPECTATION"));
    }
    let mut fenced = request;
    fenced.completion.lease.fence = LeaseFence::from_bytes([99; 32]);
    assert!(
      publisher.publish(fenced).await.is_err(),
      "a published inventory does not authorize a substituted Lease fence"
    );
    assert_eq!(bytes.0.lock().unwrap().len(), 1);
  });
}

#[test]
fn protected_inventory_rejects_a_replayed_reservation_with_another_storage_identity() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    let request = FactoryProtectedTestInventoryPublication {
      admitted: &f.admitted,
      intent: &f.intent,
      execution: &f.execution,
      plan: &f.plan,
      inventory: &f.inventory,
      files: &f.files,
      completion: AgentOutputFinalization {
        lease: f.lease,
        job_id: f.job_id,
        attempt: AttemptNumber::FIRST,
        observed_at: at(50),
      },
    };
    let first = FactoryProtectedTestInventoryPublisher::new(f.store.clone(), f.bytes.clone(), Arc::new(Clock))
      .publish(request)
      .await
      .unwrap();
    let original = f.store.published_artifact(first.artifact_id()).await.unwrap();
    let store = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
      access: f.lease,
      build_id: f.execution.build_id(),
      attempt_id: f.execution.attempt_id(),
      attempt: AttemptNumber::FIRST,
      job_id: f.job_id,
      expires_at: at(5_000),
    }));
    // A previous reservation can legitimately replay a different proposed ID.
    // Retain the real file ID, then reserve the inventory's exact replay key
    // under a different storage identity before the publisher attempts it.
    for (upload, key, publish) in [
      (&f.file, IdempotencyKey::new("actual.private.file").unwrap(), true),
      (&original, original.idempotency_key.clone(), false),
    ] {
      let identity = upload.artifact.identity();
      let reserved = store
        .begin_artifact_upload(BeginArtifactUpload {
          artifact_id: if publish {
            identity.artifact_id
          } else {
            ArtifactId::generate()
          },
          upload_id: ArtifactUploadId::generate(),
          idempotency_key: key,
          lease: f.lease,
          job_id: f.job_id,
          attempt: AttemptNumber::FIRST,
          logical_name: identity.logical_name.clone(),
          producer_run_id: upload.producer_run_id,
          producer_task_id: upload.producer_task_id,
          artifact_type: identity.artifact_type.clone(),
          media_type: identity.media_type.clone(),
          transport_media_type: upload.transport_media_type.clone(),
          size_bytes: identity.size_bytes,
          digest: identity.digest,
          reserved_at: at(30),
          capability_expires_at: at(4_000),
        })
        .await
        .unwrap();
      if publish {
        let verification = VerifyArtifactUpload {
          upload_id: reserved.upload.upload_id,
          lease: f.lease,
          job_id: f.job_id,
          attempt: AttemptNumber::FIRST,
          observed_at: at(31),
        };
        store.begin_artifact_verification(verification).await.unwrap();
        store
          .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
          .await
          .unwrap();
      }
    }
    let bytes = Arc::new(Bytes::default());
    let publisher = FactoryProtectedTestInventoryPublisher::new(store, bytes.clone(), Arc::new(Clock));
    assert!(
      publisher.publish(request).await.is_err(),
      "publication must not verify another reserved identity"
    );
    assert!(
      bytes.0.lock().unwrap().is_empty(),
      "mismatched reservation must be rejected before object writes"
    );
  });
}
struct ShortRetentionStore(Arc<InMemoryArtifactRecordStore>);
#[async_trait]
impl ArtifactRecordStore for ShortRetentionStore {
  async fn reserve_artifact(&self, _: ReserveArtifact) -> Result<ArtifactRecord, StoreError> {
    unreachable!()
  }
  async fn artifact(&self, _: ArtifactId) -> Result<ArtifactRecord, StoreError> {
    unreachable!()
  }
  async fn transition_artifact(&self, _: TransitionArtifact) -> Result<ArtifactRecord, StoreError> {
    unreachable!()
  }
  async fn begin_artifact_upload(
    &self,
    request: BeginArtifactUpload,
  ) -> Result<BeginArtifactUploadOutcome, StoreError> {
    let now = request.reserved_at;
    let mut outcome = self.0.begin_artifact_upload(request).await?;
    let mut identity = outcome.upload.artifact.identity().clone();
    identity.retention = octacity_server_artifacts::ArtifactRetentionPolicy::DeleteAfter(at(100));
    outcome.upload.artifact = ArtifactRecord::pending(identity, now).unwrap();
    Ok(outcome)
  }
  async fn artifact_upload(&self, _: ArtifactUploadId) -> Result<ArtifactUploadRecord, StoreError> {
    unreachable!()
  }
  async fn begin_artifact_verification(&self, _: VerifyArtifactUpload) -> Result<ArtifactUploadRecord, StoreError> {
    panic!("a shorter stored retention must be rejected before verification or object writes")
  }
  async fn finish_artifact_verification(
    &self,
    _: VerifyArtifactUpload,
    _: ArtifactVerificationResult,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    unreachable!()
  }
  async fn published_artifact(&self, _: ArtifactId) -> Result<ArtifactUploadRecord, StoreError> {
    unreachable!()
  }
  async fn list_published_artifacts(
    &self,
    query: ListPublishedArtifacts,
  ) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
    self.0.list_published_artifacts(query).await
  }
}

#[test]
fn protected_inventory_cannot_promise_more_retention_than_the_reserved_artifact() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    let bytes = Arc::new(Bytes::default());
    let publisher = FactoryProtectedTestInventoryPublisher::new(
      Arc::new(ShortRetentionStore(f.store)),
      bytes.clone(),
      Arc::new(Clock),
    );
    let request = FactoryProtectedTestInventoryPublication {
      admitted: &f.admitted,
      intent: &f.intent,
      execution: &f.execution,
      plan: &f.plan,
      inventory: &f.inventory,
      files: &f.files,
      completion: AgentOutputFinalization {
        lease: f.lease,
        job_id: f.job_id,
        attempt: AttemptNumber::FIRST,
        observed_at: at(50),
      },
    };
    assert!(publisher.publish(request).await.is_err());
    assert!(bytes.0.lock().unwrap().is_empty());
  });
}

#[test]
fn protected_authoring_publishes_only_the_closed_public_handoff_under_the_admitted_schema() {
  crate::factory_admission_tests::run_ready(async {
    let schema = supplied_base_flow_template()
      .data_schemas()
      .unwrap()
      .remove(&key("protected_test_handoff"))
      .unwrap();
    for (schema, accepted) in [(None, false), (Some(schema), true)] {
      let f = fixture_with_schema(schema).await;
      let publisher = FactoryProtectedTestInventoryPublisher::new(f.store.clone(), f.bytes.clone(), Arc::new(Clock));
      let request = FactoryProtectedTestInventoryPublication {
        admitted: &f.admitted,
        intent: &f.intent,
        execution: &f.execution,
        plan: &f.plan,
        inventory: &f.inventory,
        files: &f.files,
        completion: AgentOutputFinalization {
          lease: f.lease,
          job_id: f.job_id,
          attempt: AttemptNumber::FIRST,
          observed_at: at(50),
        },
      };
      let outcome = publisher.publish_outputs(request).await;
      if !accepted {
        assert!(
          outcome.is_err(),
          "the generated handoff must satisfy the frozen outcome schema"
        );
        assert!(
          f.bytes.0.lock().unwrap().is_empty(),
          "schema rejection must precede publication"
        );
        continue;
      }
      outcome.unwrap();
      publisher.publish_outputs(request).await.unwrap();
      assert_eq!(
        f.bytes.1.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "each derived object is physically written once"
      );
      let records = f
        .store
        .list_published_artifacts(ListPublishedArtifacts {
          build_id: f.execution.build_id(),
          attempt_id: Some(f.execution.attempt_id()),
          limit: MAX_ARTIFACT_PAGE_SIZE,
          visibility: ArtifactListVisibility::all(),
        })
        .await
        .unwrap();
      assert_eq!(
        records.len(),
        3,
        "one private file, one inventory and one public report"
      );
      assert!(records.iter().all(|record| {
        let identity = record.artifact.identity();
        identity.job_id == f.job_id && identity.lease_id == f.lease.lease_id
      }));
      let report = records
        .iter()
        .find(|record| record.artifact.identity().logical_name.as_str() == "audit.report")
        .unwrap();
      assert!(matches!(
        report.artifact.identity().artifact_type,
        ArtifactType::Report(_)
      ));
      let writes = f.bytes.0.lock().unwrap();
      assert_eq!(
        writes.len(),
        2,
        "both completion replays must reuse the same immutable bytes"
      );
      let public = &writes[&report.artifact.identity().artifact_id];
      assert_eq!(
        serde_json::from_slice::<serde_json::Value>(public).unwrap(),
        serde_json::json!({
          "accepted_contract": "1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b",
          "classes": ["project.contract"]
        })
      );
    }
  });
}

struct MovingPublicationClock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for MovingPublicationClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0.load(std::sync::atomic::Ordering::SeqCst)))
  }
}
struct DeadlineDuringPublication(Arc<MovingPublicationClock>);
#[async_trait]
impl ArtifactBytePublisher for DeadlineDuringPublication {
  async fn publish_verified(&self, _: &ArtifactObject, _: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.0.0.store(100, std::sync::atomic::Ordering::SeqCst);
    Ok(())
  }
}
#[test]
fn protected_inventory_cannot_become_visible_when_its_original_deadline_passes_during_object_io() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    assert_eq!(f.intent.node().deadline(), at(100));
    let clock = Arc::new(MovingPublicationClock(std::sync::atomic::AtomicI64::new(50)));
    let publisher = FactoryProtectedTestInventoryPublisher::new(
      f.store.clone(),
      Arc::new(DeadlineDuringPublication(clock.clone())),
      clock,
    );
    let result = publisher
      .publish(FactoryProtectedTestInventoryPublication {
        admitted: &f.admitted,
        intent: &f.intent,
        execution: &f.execution,
        plan: &f.plan,
        inventory: &f.inventory,
        files: &f.files,
        completion: AgentOutputFinalization {
          lease: f.lease,
          job_id: f.job_id,
          attempt: AttemptNumber::FIRST,
          observed_at: at(50),
        },
      })
      .await;
    assert!(
      result.is_err(),
      "an unexpired ordinary Lease must not extend the original node deadline"
    );
    let records = f
      .store
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: f.execution.build_id(),
        attempt_id: Some(f.execution.attempt_id()),
        limit: MAX_ARTIFACT_PAGE_SIZE,
        visibility: ArtifactListVisibility::all(),
      })
      .await
      .unwrap();
    assert_eq!(
      records.len(),
      1,
      "only the previously published private file can remain visible"
    );
  });
}
