use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_factory::*;
use octacity_server_store::testing::{
  ArtifactLeaseFixture, InMemoryArtifactRecordStore, InMemoryFactoryConfigurationStore,
};
use std::sync::Arc;

struct Selected(FactoryProtectedValidationMaterialization);
#[async_trait]
impl FactoryProtectedValidationInputSource for Selected {
  async fn materialization(
    &self,
    _: &FlowBuildIntent,
  ) -> Result<FactoryProtectedValidationMaterialization, ApplicationError> {
    Ok(self.0.clone())
  }
}
struct NoReads;
#[async_trait]
impl ArtifactByteReader for NoReads {
  async fn read_published(&self, _: &ArtifactObject, _: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    panic!("installed profile validation must not read artifact bytes")
  }
}
#[test]
fn installed_validation_evidence_action_accepts_only_its_exact_operator_profile() {
  use crate::factory_node_test_support::fixtures::{key, reference};
  let fixture = crate::factory_protected_test_tests::verify_validation_materialization(false);
  let profile = FactoryProtectedValidationEvidenceProfile {
    action: key("collect-exact-validation"),
    ready: FlowOutcomeDefinition::new(
      key("qualified"),
      FlowOutcomeKind::Success,
      reference("evidence-manifest"),
    ),
    report: fixture.requirement.clone(),
    facts: fixture.facts_requirement.clone(),
  };
  let identity = ImmutableReference::new(
    key("installed-validation-evidence"),
    key("v1"),
    profile.digest().unwrap(),
  );
  let artifact_identity = fixture.document.record.identity();
  let artifact_store = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access: octacity_server_store::LeaseAccess {
      lease_id: artifact_identity.lease_id,
      fence: octacity_server_store::LeaseFence::from_bytes([1; 32]),
      agent_id: octacity_server_domain::AgentId::generate(),
      registration_epoch: octacity_server_store::RegistrationEpoch::new(1).unwrap(),
    },
    build_id: artifact_identity.build_id,
    attempt_id: artifact_identity.attempt_id,
    attempt: octacity_server_domain::AttemptNumber::FIRST,
    job_id: artifact_identity.job_id,
    expires_at: octacity_server_domain::Timestamp::from_unix_millis(1000).unwrap(),
  }));
  let action = FactoryProtectedValidationEvidenceAction::new(
    identity.clone(),
    Arc::new(InMemoryFactoryConfigurationStore::new()),
    Arc::new(FactoryArtifactReader::new(
      artifact_store,
      fixture.builds.clone(),
      Arc::new(NoReads),
    )),
    Arc::new(Selected(fixture.materialized.clone())),
    profile.clone(),
    crate::factory_node_test_support::clock(),
  );
  let binding = FlowActionBinding::new(identity.clone(), profile.action.clone(), serde_json::json!({})).unwrap();
  action.validate(&binding, std::slice::from_ref(&profile.ready)).unwrap();
  let other = FlowActionBinding::new(identity, key("substituted-action"), serde_json::json!({})).unwrap();
  assert!(action.validate(&other, std::slice::from_ref(&profile.ready)).is_err());
  let changed = FlowOutcomeDefinition::new(
    profile.ready.key().clone(),
    FlowOutcomeKind::Success,
    reference("different-schema"),
  );
  assert!(action.validate(&binding, &[changed]).is_err());
  let mut drift = profile.clone();
  drift.report = EvidenceRequirement::new(
    key("substituted-report"),
    EvidenceOutputKind::Report,
    fixture.requirement.schema().clone(),
    fixture.requirement.tool().clone(),
    fixture.requirement.plugin().clone(),
  );
  assert_ne!(drift.digest().unwrap(), profile.digest().unwrap());
  // Shared fixture must retain the same complete candidate-bearing input and Build result.
  assert_eq!(fixture.materialized.input(), fixture.intent.input());
  assert_eq!(fixture.completed.record.input(), fixture.intent.input());
  assert_eq!(fixture.work.subject(), fixture.intent.input().subject());
  assert!(fixture.admitted.data_schema(fixture.schema.reference()).is_some());
}

fn at(n: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(n).unwrap()
}
fn extended_fixture(
  substitute_facts: bool,
  profile: &FactoryProtectedValidationEvidenceProfile,
  output: FlowDataSchema,
) -> crate::factory_protected_test_tests::ValidationFixture {
  use crate::factory_node_test_support::fixtures::key;
  use std::collections::BTreeMap;
  let identity = ImmutableReference::new(
    key("installed-validation-evidence"),
    key("v1"),
    profile.digest().unwrap(),
  );
  let reference_shape = crate::factory_review_planning_tests::shape(&serde_json::json!({
    "artifact_id":octacity_server_domain::ArtifactId::generate(),
    "content_digest":FactoryDigest::from_bytes([1;32]),"encoded_size":1
  }));
  let input_schema = FlowDataSchema::new(
    key("selected-validation-artifacts"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (key("report"), FlowFieldSchema::required(reference_shape.clone())),
        (key("facts"), FlowFieldSchema::required(reference_shape)),
      ]),
    },
  )
  .unwrap();
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![identity.clone()],
    ..Default::default()
  })
  .unwrap();
  let action = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("retain_qualified_inputs"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(input_schema.reference().clone()),
    outcomes: vec![profile.ready.clone()],
    budget: BudgetLimit::new(1, 1000, 100, 1000, 10000).unwrap(),
    permissions: permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(FlowActionBinding::new(identity, profile.action.clone(), serde_json::json!({})).unwrap())
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      BTreeMap::from([
        (
          key("report"),
          FlowInputSource::Predecessor {
            view: FlowRecordView::Metadata,
            path: format!("/build/outputs/{}/artifact", profile.report.kind()),
          },
        ),
        (
          key("facts"),
          FlowInputSource::Predecessor {
            view: FlowRecordView::Metadata,
            path: format!(
              "/build/outputs/{}/artifact",
              if substitute_facts {
                "audit.report"
              } else {
                profile.facts.kind().as_str()
              }
            ),
          },
        ),
      ]),
      FlowDataMapping::new(FlowValueMapping::Object {
        fields: BTreeMap::from([
          (key("report"), FlowValueMapping::Pointer { path: "/report".into() }),
          (key("facts"), FlowValueMapping::Pointer { path: "/facts".into() }),
        ]),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  crate::factory_protected_test_tests::validation_fixture_with_flow(false, |work, prior, previous_input, previous| {
    let original = prior.closure().definition(prior.closure().root()).unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: original.reference().id(),
      version: original.reference().version(),
      input_schema: original.input_schema().cloned(),
      entry: original.entry().clone(),
      nodes: vec![original.nodes()[0].clone(), action.clone()],
      transitions: vec![
        FlowTransition::new(
          original.entry().clone(),
          original.nodes()[0].outcomes()[0].key().clone(),
          FlowTransitionTarget::Node(action.key().clone()),
        ),
        FlowTransition::new(
          action.key().clone(),
          profile.ready.key().clone(),
          FlowTransitionTarget::Terminal(key("finished")),
        ),
      ],
      terminals: vec![FlowTerminalDefinition::new(key("finished"), output.reference().clone())],
      context_projections: vec![],
      data_projections: vec![],
      execution: FlowExecutionPolicy::new(prior.limits().budget(), permissions.clone(), 1).unwrap(),
    })
    .unwrap();
    let run = FactoryRun::admitted(prior.root_run().factory_run_id(), work);
    let flow = FlowRun::root(&run, definition.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let mut schemas = prior.data_schemas().to_vec();
    schemas.extend([input_schema.clone(), output.clone()]);
    let admitted = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      FlowAdmissionLimits::product_defaults(4, prior.limits().budget(), permissions.clone()).unwrap(),
      flow,
      cycle,
      prior.wip_limits(),
    )
    .unwrap()
    .with_data_schemas(schemas)
    .unwrap();
    let input = FlowNodeInput::new(
      work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      previous_input.node().clone(),
      previous_input.payload().clone(),
    )
    .unwrap()
    .with_context(previous_input.context().unwrap().clone(), vec![])
    .unwrap();
    let node = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      admitted.closure().definition(admitted.closure().root()).unwrap(),
      NodeAttemptInput {
        started_at: previous.started_at(),
        id: previous.id(),
        node_key: previous.node_key().clone(),
        node_kind: previous.node_kind(),
        number: previous.number(),
        input_digest: input.digest().unwrap(),
        budget: previous.budget(),
        deadline: previous.deadline(),
        execution: previous.execution().clone(),
        ownership: FactoryClaimOwnership::new(previous.owner().clone(), previous.claim()),
      },
    )
    .unwrap();
    (admitted, input, node)
  })
}

struct ReportBytes {
  id: octacity_server_domain::ArtifactId,
  bytes: Vec<u8>,
  reads: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl ArtifactByteReader for ReportBytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    assert_eq!(
      object.artifact_id(),
      self.id,
      "only the explicitly selected report may be read"
    );
    assert!(self.bytes.len() as u64 <= maximum);
    self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(self.bytes.clone())
  }
}
#[test]
fn selected_completed_validation_produces_one_frozen_manifest_and_replays_after_owner_restart() {
  verify_evidence_publication(false);
}
#[test]
fn selecting_a_different_report_as_validation_facts_is_rejected_before_artifact_bytes() {
  verify_evidence_publication(true);
}
fn verify_evidence_publication(substitute_facts: bool) {
  use crate::factory_node_test_support::fixtures::key;
  use octacity_server_domain::*;
  use octacity_server_store::*;
  let sample = crate::factory_protected_test_tests::verify_validation_materialization(false);
  let output = FlowDataSchema::new(
    key("exact-evidence-manifest"),
    key("v1"),
    crate::factory_review_planning_tests::shape(&serde_json::to_value(&sample.evidence).unwrap()),
  )
  .unwrap();
  let profile = FactoryProtectedValidationEvidenceProfile {
    action: key("collect-exact-validation"),
    ready: FlowOutcomeDefinition::new(key("qualified"), FlowOutcomeKind::Success, output.reference().clone()),
    report: sample.requirement,
    facts: sample.facts_requirement,
  };
  let fixture = extended_fixture(substitute_facts, &profile, output);
  crate::factory_admission_tests::run_ready(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let run = FactoryRun::admitted(fixture.intent.input().factory_run_id(), &fixture.work);
    let audit = octacity_server_store::testing::management_mutation_with_request((), "validation-admission")
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
    let mut start = octacity_server_store::testing::factory_flow_contract_transition(&snapshot, &claim, 11);
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
    let mut complete = octacity_server_store::testing::factory_flow_contract_transition(&snapshot, &claim, 32);
    complete
      .append
      .flow
      .data
      .build_executions
      .push(fixture.completed.execution.clone());
    complete.append.flow.data.records.push(fixture.completed.record.clone());
    complete
      .append
      .flow
      .completions
      .push(fixture.completed.completion.clone());
    let mut usage = fixture.completed.completion.usage();
    usage.attempts += 1;
    complete.budget = FactoryBudgetRecord::new(run.id(), complete.next_run.version(), usage, at(32));
    complete.current.budget_id = complete.budget.id;
    store.commit_factory_run_transition(complete).await.unwrap();
    let identity = fixture.document.record.identity();
    let lease = LeaseAccess {
      lease_id: identity.lease_id,
      fence: LeaseFence::from_bytes([1; 32]),
      agent_id: AgentId::generate(),
      registration_epoch: RegistrationEpoch::new(1).unwrap(),
    };
    let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
      access: lease,
      build_id: identity.build_id,
      attempt_id: identity.attempt_id,
      attempt: AttemptNumber::FIRST,
      job_id: identity.job_id,
      expires_at: at(1000),
    }));
    let upload = artifacts
      .begin_artifact_upload(BeginArtifactUpload {
        artifact_id: identity.artifact_id,
        upload_id: ArtifactUploadId::generate(),
        idempotency_key: IdempotencyKey::new("validation-report").unwrap(),
        lease,
        job_id: identity.job_id,
        attempt: AttemptNumber::FIRST,
        logical_name: identity.logical_name.clone(),
        producer_run_id: 1,
        producer_task_id: 1,
        artifact_type: identity.artifact_type.clone(),
        media_type: identity.media_type.clone(),
        transport_media_type: identity.media_type.clone(),
        size_bytes: identity.size_bytes,
        digest: identity.digest,
        reserved_at: at(30),
        capability_expires_at: at(1000),
      })
      .await
      .unwrap();
    let verify = VerifyArtifactUpload {
      upload_id: upload.upload.upload_id,
      lease,
      job_id: identity.job_id,
      attempt: AttemptNumber::FIRST,
      observed_at: at(30),
    };
    artifacts.begin_artifact_verification(verify).await.unwrap();
    artifacts
      .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
      .await
      .unwrap();
    let bytes = Arc::new(ReportBytes {
      id: identity.artifact_id,
      bytes: fixture.document.bytes.clone(),
      reads: std::sync::atomic::AtomicUsize::new(0),
    });
    let action_identity = ImmutableReference::new(
      key("installed-validation-evidence"),
      key("v1"),
      profile.digest().unwrap(),
    );
    let action = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
      FactoryProtectedValidationEvidenceAction::new(
        action_identity,
        store.clone(),
        Arc::new(FactoryArtifactReader::new(
          artifacts,
          fixture.builds.clone(),
          bytes.clone(),
        )),
        Arc::new(Selected(fixture.materialized.clone())),
        profile.clone(),
        crate::factory_node_test_support::clock(),
      ),
    )));
    let target = FactoryNodeTarget {
      flow_run_id: fixture.intent.input().flow_run_id(),
      cycle_id: fixture.intent.input().cycle_id(),
      node: key("retain_qualified_inputs"),
    };
    let runner = FactoryNodeRunner::new(store.clone(), action.clone(), crate::factory_node_test_support::clock());
    assert_eq!(
      runner
        .run_once(run.id(), claim.id, target.clone(), None, at(40))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    if substitute_facts {
      assert!(
        runner
          .run_once(run.id(), claim.id, target.clone(), None, at(41))
          .await
          .is_err()
      );
      assert_eq!(bytes.reads.load(std::sync::atomic::Ordering::SeqCst), 0);
      let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
      assert!(
        !snapshot
          .flow
          .data
          .records
          .iter()
          .any(|record| record.input().node() == &target.node)
      );
      return;
    }
    assert!(matches!(
      runner
        .run_once(run.id(), claim.id, target.clone(), None, at(41))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.input().node() == &target.node)
      .unwrap();
    let manifest: EvidenceManifest = serde_json::from_value(record.payload().value().clone()).unwrap();
    assert_eq!(manifest.items().len(), 1);
    assert_eq!(manifest.inputs().len(), 4);
    assert_eq!(manifest.items()[0].outcome(), DeterministicGateOutcome::Passed);
    assert_eq!(manifest.items()[0].fresh_until(), at(500));
    assert_eq!(manifest.constructed_at(), at(40));
    assert_eq!(
      record.discovery(),
      Some(&FlowDiscoveryFacts::evidence(&manifest).unwrap())
    );
    assert_eq!(bytes.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    let restarted = FactoryNodeRunner::new(store.clone(), action, crate::factory_node_test_support::clock());
    assert!(matches!(
      restarted
        .run_once(run.id(), claim.id, target, None, at(42))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    assert_eq!(store.factory_run_snapshot(run.id()).await.unwrap(), snapshot);
    assert_eq!(
      bytes.reads.load(std::sync::atomic::Ordering::SeqCst),
      1,
      "retained completion does not reread report bytes"
    );
  });
}
