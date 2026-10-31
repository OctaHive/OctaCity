//! Candidate resources come from accepted owner records, not mutable refs or payload assertions.
use super::*;
use octacity_server_domain::{AttemptId, BuildId, JobId};
use octacity_server_factory::*;

#[path = "factory_resource_build_tests.rs"]
mod build_resources;
#[path = "factory_resource_publication_tests.rs"]
mod publication_resources;

#[test]
fn retained_work_dispositions_bind_correlation_without_claiming_a_repository_resource() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let escalation = Escalation::new(
      EscalationId::generate(),
      &fixture.snapshot.run,
      None,
      FactoryText::new("Operator input is required").unwrap(),
    )
    .unwrap();
    let facts = FlowDiscoveryFacts::escalation(&escalation);
    assert!(facts.exact_subject().is_none());
    let record = fixture.records[0].clone().with_discovery(facts);
    assert!(
      record.is_ok(),
      "Work disposition must survive without VCS provenance: {record:?}"
    );
    let unrelated = resource_snapshot(false).await;
    let other = Escalation::new(
      EscalationId::generate(),
      &unrelated.run,
      None,
      FactoryText::new("Unrelated disposition").unwrap(),
    )
    .unwrap();
    assert!(
      fixture.records[0]
        .clone()
        .with_discovery(FlowDiscoveryFacts::escalation(&other))
        .is_err()
    );
    assert!(record.unwrap().discovery().unwrap().published_candidate().is_none());
  });
}

#[test]
fn candidate_resource_uses_the_exact_accepted_producer_without_resolving_a_consumer_default_ref() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let preparation = fixture.preparation();
    let flow = preparation.admitted;
    let schema = preparation.schema;
    let prepared = FlowNodeInput::prepare(fixture.preparation());
    assert!(
      prepared.is_ok(),
      "an accepted owner candidate must become an exact selected resource: {prepared:?}"
    );
    let prepared = prepared.unwrap();
    assert_eq!(
      prepared.task_subject().unwrap(),
      FactoryTaskSubject::Candidate(fixture.candidate.clone())
    );
    assert_eq!(prepared.subject(), fixture.snapshot.work.subject());
    assert_eq!(
      prepared.sources().len(),
      1,
      "resource selection retains the exact producing record without its transcript"
    );
    assert_eq!(
      prepared.sources()[0].node_attempt_id(),
      fixture.records[0].node_attempt_id()
    );
    assert_eq!(prepared.sources()[0].digest(), fixture.records[0].digest().unwrap());
    prepared.verify_preparation(fixture.preparation()).unwrap();
    let retry = prepared.retry().unwrap();
    let restored = FlowNodeInput::restore(
      &serde_json::to_vec(&retry).unwrap(),
      &fixture.snapshot.work,
      flow,
      flow.root_run(),
      flow.initial_cycle(),
      schema,
      retry.digest().unwrap(),
    )
    .unwrap();
    assert_eq!(restored, retry);
    assert_eq!(fixture.resolver.call_count(), 1);
  });
}

#[test]
fn installed_context_owner_qualifies_an_explicit_candidate_against_the_selected_resource() {
  run_ready(async {
    let content = b"Explicit candidate evidence";
    let document = FactoryArtifactReference::new(
      ArtifactId::generate(),
      FactoryDigest::content_sha256(content),
      content.len() as u64,
    )
    .unwrap();
    let binding = FlowContextBinding::new(
      Some(
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!("retained-candidate"),
        })
        .unwrap(),
      ),
      [(
        key("evidence"),
        FlowContextSelection {
          when_present: None,
          source_kind: ContextSourceKind::Specification,
          source: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::to_value(FactoryContextReference::Artifact(document.clone())).unwrap(),
          })
          .unwrap(),
        },
      )]
      .into(),
    )
    .unwrap();
    let mut fixture = candidate_fixture(Some(binding)).await;
    fixture.snapshot.flow.attempts = fixture.attempts.to_vec();
    fixture.snapshot.flow.completions = fixture.completions.to_vec();
    fixture.snapshot.flow.data.records = fixture.records.to_vec();
    fixture.snapshot.flow.data.inputs = fixture.inputs.to_vec();
    let lease = octacity_server_store::testing::authoritative_store_contract_fixture();
    let artifacts = Arc::new(octacity_server_store::testing::InMemoryArtifactRecordStore::new(
      octacity_server_store::testing::ArtifactLeaseFixture {
        access: octacity_server_store::LeaseAccess {
          lease_id: octacity_server_domain::LeaseId::generate(),
          fence: octacity_server_store::LeaseFence::from_bytes([7; 32]),
          agent_id: lease.agent_id,
          registration_epoch: lease.registration_epoch,
        },
        build_id: lease.request.build.id,
        attempt_id: lease.request.attempt_id,
        attempt: octacity_server_domain::AttemptNumber::FIRST,
        job_id: lease.request.jobs[0].id,
        expires_at: time(10_000),
      },
    ));
    let bytes = Arc::new(crate::factory_test_support::InMemoryArtifactBytes::default());
    bytes.0.lock().unwrap().insert(document.artifact_id(), content.to_vec());
    let publication = octacity_server_store::PublishedProjectInput {
      project_id: fixture.snapshot.work.subject().project_id(),
      preparation_id: digest(119),
      mapping: key("selected-context.v1"),
      object: octacity_server_store::FactoryInputObject {
        role: octacity_server_store::FactoryInputRole::Task,
        artifact: document,
        upload_id: octacity_server_domain::ArtifactUploadId::generate(),
        media_type: octacity_server_store::ArtifactMediaType::new("text/plain").unwrap(),
      },
      published_at: time(0),
      expires_at: time(100_000),
    };
    let reader = Arc::new(
      crate::FactoryArtifactReader::new(
        artifacts,
        Arc::new(crate::factory_node_test_support::Builds::new()),
        bytes.clone(),
      )
      .with_project_inputs(Arc::new(SelectedDocument(publication))),
    );
    let preparer = crate::FactoryArtifactFlowInputs::new(reader, crate::factory_node_test_support::clock());
    let target = crate::FactoryNodeTarget {
      flow_run_id: fixture.snapshot.admitted_flow.root_run().id(),
      cycle_id: fixture.snapshot.admitted_flow.initial_cycle().id(),
      node: fixture.consumer.clone(),
    };
    let prepared = crate::FactoryFlowInputPreparer::prepare(&preparer, &fixture.snapshot, &target).await;
    assert!(
      prepared.is_ok(),
      "an explicit selected candidate is independent of Work repository correlation: {prepared:?}"
    );
    let prepared = prepared.unwrap();
    assert_eq!(
      prepared.context().unwrap().subject(),
      &FactoryTaskSubject::Candidate(fixture.candidate)
    );
    assert_eq!(prepared.subject(), fixture.snapshot.work.subject());
    assert_eq!(bytes.2.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(fixture.resolver.call_count(), 1);
  });
}

struct SelectedDocument(octacity_server_store::PublishedProjectInput);
#[async_trait::async_trait]
impl octacity_server_store::ProjectInputQueryStore for SelectedDocument {
  async fn project_input_artifact(
    &self,
    project: ProjectId,
    artifact: ArtifactId,
  ) -> Result<Option<octacity_server_store::PublishedProjectInput>, octacity_server_store::StoreError> {
    Ok((self.0.project_id == project && self.0.object.artifact.artifact_id() == artifact).then(|| self.0.clone()))
  }
}

struct CandidateFixture {
  snapshot: octacity_server_store::FactoryRunSnapshot,
  consumer: FactoryKey,
  attempts: [NodeAttempt; 1],
  completions: [NodeAttemptCompletion; 1],
  records: [FlowNodeRecord; 1],
  inputs: [FlowNodeInput; 1],
  candidate: CandidateSubject,
  resolver: Arc<RecordingResolver>,
}
fn consumer_record(fixture: &CandidateFixture, input: &FlowNodeInput) -> FlowNodeRecord {
  let flow = &fixture.snapshot.admitted_flow;
  let root = flow.closure().definition(flow.closure().root()).unwrap();
  let node = root.node(&fixture.consumer).unwrap();
  let ownership = FactoryClaimOwnership::new(
    key("consumer-owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(103)), time(32), time(100)).unwrap(),
  );
  let attempt = NodeAttempt::new(
    flow.root_run(),
    flow.initial_cycle(),
    root,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: node.key().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      started_at: time(32),
      deadline: time(100),
      execution: NodeExecutionIdentity::External(node.action().unwrap().plugin().clone()),
      ownership,
    },
  )
  .unwrap();
  FlowNodeRecord::new(
    input,
    &attempt,
    root,
    FlowRecordObservation {
      outcome: key("succeeded"),
      payload: FlowPayload::new(
        flow
          .data_schema(node.outcome(&key("succeeded")).unwrap().schema())
          .unwrap(),
        serde_json::json!(true),
      )
      .unwrap(),
      producer: attempt.execution().clone(),
      observed_at: time(33),
    },
  )
  .unwrap()
}

#[test]
fn contextless_candidate_result_metadata_reports_the_selected_execution_revision() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let input = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    assert!(input.context().is_none());
    let record = consumer_record(&fixture, &input);
    assert_eq!(record.metadata().unwrap()["execution_revision"], "retained-candidate");
  });
}

#[test]
fn candidate_bound_result_rejects_another_revision_or_changeset_from_the_same_base() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let input = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    let record = consumer_record(&fixture, &input);
    let facts = |subject| {
      FlowDiscoveryFacts::promotion(&PromotionObservation {
        plan_digest: digest(105),
        input_digest: input.digest().unwrap(),
        subject,
        environment: key("test"),
        environment_contract: exact("test-environment", 106),
        deployment: exact("test-deployment", 107),
        outcome: DeterministicGateOutcome::Passed,
        started_at: time(32),
        finished_at: time(33),
        measurements: Default::default(),
      })
    };
    assert!(record.clone().with_discovery(facts(fixture.candidate.clone())).is_ok());
    for other in [
      CandidateSubject::new(
        fixture.candidate.exact().clone(),
        "other-candidate".parse().unwrap(),
        fixture.candidate.changeset_digest(),
      ),
      CandidateSubject::new(
        fixture.candidate.exact().clone(),
        fixture.candidate.candidate_revision().clone(),
        digest(104),
      ),
    ] {
      assert!(
        record.clone().with_discovery(facts(other)).is_err(),
        "a shared base cannot qualify a different candidate revision or ChangeSet"
      );
    }
  });
}

#[test]
fn trusted_candidate_publication_can_produce_a_new_candidate_from_the_selected_base() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let input = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    let next = CandidateSubject::new(
      fixture.candidate.exact().clone(),
      "next-candidate".parse().unwrap(),
      digest(108),
    );
    assert!(
      consumer_record(&fixture, &input)
        .with_discovery(FlowDiscoveryFacts::candidate(next))
        .is_ok(),
      "trusted publication describes its new output, while review facts must describe the selected input"
    );
  });
}

impl CandidateFixture {
  fn preparation(&self) -> FlowInputPreparation<'_> {
    let flow = &self.snapshot.admitted_flow;
    let node = flow
      .closure()
      .definition(flow.closure().root())
      .unwrap()
      .node(&self.consumer)
      .unwrap();
    let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
    FlowInputPreparation {
      work: &self.snapshot.work,
      admitted: flow,
      flow: flow.root_run(),
      cycle: flow.initial_cycle(),
      node: self.consumer.clone(),
      schema,
      history: FlowRuntimeHistory {
        stops: &[],
        flow_runs: &self.snapshot.flow.runs,
        cycles: &self.snapshot.flow.cycles,
        attempts: &self.attempts,
        completions: &self.completions,
      },
      records: &self.records,
      inputs: &self.inputs,
      incoming: None,
    }
  }
}
async fn candidate_fixture(context_binding: Option<FlowContextBinding>) -> CandidateFixture {
  candidate_fixture_with_build(context_binding, false).await
}
async fn candidate_fixture_with_build(context_binding: Option<FlowContextBinding>, build: bool) -> CandidateFixture {
  let CodeFixture {
    store,
    repositories,
    request,
    ..
  } = code_fixture_for_execution(
    FlowCodeRevision::AdmittedBase,
    FlowCodeScope::Repository,
    true,
    context_binding,
    build,
  )
  .await;
  let resolver = Arc::new(RecordingResolver::default());
  let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone()).with_repositories(repositories);
  let admission = admit(&handlers, request).await.unwrap();
  assert_eq!(
    resolver.call_count(),
    1,
    "accepted-candidate consumers must not resolve an unused default branch"
  );
  let snapshot = store.factory_run_snapshot(admission.factory_run_id).await.unwrap();
  let flow = &snapshot.admitted_flow;
  let root = flow.closure().definition(flow.closure().root()).unwrap();
  let consumer = root
    .nodes()
    .iter()
    .find(|node| {
      matches!(
        node.code(),
        Some(FlowCodeBinding::Source {
          revision: FlowCodeRevision::AcceptedCandidate { .. },
          ..
        })
      )
    })
    .unwrap();
  let producer = root.node(root.entry()).unwrap();
  let schema = flow.data_schema(producer.input_schema().unwrap()).unwrap();
  let input = FlowNodeInput::new(
    &snapshot.work,
    flow,
    flow.root_run(),
    flow.initial_cycle(),
    producer.key().clone(),
    FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
  )
  .unwrap();
  let ownership = FactoryClaimOwnership::new(
    key("resource-owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(101)), time(30), time(100)).unwrap(),
  );
  let attempt = NodeAttempt::new(
    flow.root_run(),
    flow.initial_cycle(),
    root,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: producer.key().clone(),
      node_kind: producer.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: producer.budget(),
      started_at: time(30),
      deadline: time(100),
      execution: NodeExecutionIdentity::External(producer.action().unwrap().plugin().clone()),
      ownership: ownership.clone(),
    },
  )
  .unwrap();
  let candidate = CandidateSubject::new(
    input.exact_resource().unwrap().clone(),
    "retained-candidate".parse().unwrap(),
    digest(102),
  );
  let record = FlowNodeRecord::new(
    &input,
    &attempt,
    root,
    FlowRecordObservation {
      outcome: key("succeeded"),
      payload: FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
      producer: attempt.execution().clone(),
      observed_at: time(31),
    },
  )
  .unwrap()
  .with_discovery(FlowDiscoveryFacts::candidate(candidate.clone()))
  .unwrap();
  let completion = record
    .completion(
      &attempt,
      root,
      ownership,
      BudgetUsage {
        attempts: 1,
        elapsed_millis: 1,
        ..Default::default()
      },
      time(31),
    )
    .unwrap();
  let consumer = consumer.key().clone();
  let attempts = [attempt];
  let completions = [completion];
  let records = [record];
  let inputs = [input];
  CandidateFixture {
    snapshot,
    consumer,
    attempts,
    completions,
    records,
    inputs,
    candidate,
    resolver,
  }
}

#[test]
fn selected_candidate_context_needs_no_duplicate_payload_revision_mapping() {
  run_ready(async {
    let document = artifact(110);
    let binding = FlowContextBinding::new(
      None,
      [(
        key("guidance"),
        FlowContextSelection {
          when_present: None,
          source_kind: ContextSourceKind::Specification,
          source: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::to_value(FactoryContextReference::Artifact(document.clone())).unwrap(),
          })
          .unwrap(),
        },
      )]
      .into(),
    )
    .unwrap();
    let fixture = candidate_fixture(Some(binding)).await;
    let prepared = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    let subject = prepared.task_subject().unwrap();
    let entry = ContextManifestEntry::new(
      ContextSourceKind::Specification,
      key("guidance"),
      subject.clone(),
      FactoryContextReference::Artifact(document.clone()),
      document.content_digest(),
      document.encoded_size(),
      FactorySafeText::new("Explicit selected document").unwrap(),
      digest(111),
    )
    .unwrap();
    let context = ContextManifest::new(ContextManifestId::generate(), subject, digest(112), vec![entry]).unwrap();
    let prepared = prepared.with_context(context, vec![]).unwrap();
    assert!(
      prepared.verify_preparation(fixture.preparation()).is_ok(),
      "the frozen accepted candidate supplies Context provenance without a second selector"
    );
    let preparation = fixture.preparation();
    let restored = FlowNodeInput::restore(
      &serde_json::to_vec(&prepared).unwrap(),
      preparation.work,
      preparation.admitted,
      preparation.flow,
      preparation.cycle,
      preparation.schema,
      prepared.digest().unwrap(),
    )
    .unwrap();
    assert_eq!(restored, prepared);
  });
}

#[test]
fn accepted_candidate_resource_rejects_plain_results_and_unqualified_subject_substitution() {
  run_ready(async {
    let mut fixture = candidate_fixture(None).await;
    let root = fixture
      .snapshot
      .admitted_flow
      .closure()
      .definition(fixture.snapshot.admitted_flow.closure().root())
      .unwrap();
    let schema = fixture
      .snapshot
      .admitted_flow
      .data_schema(fixture.inputs[0].payload().schema())
      .unwrap();
    let plain = FlowNodeRecord::new(
      &fixture.inputs[0],
      &fixture.attempts[0],
      root,
      FlowRecordObservation {
        outcome: key("succeeded"),
        payload: FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
        producer: fixture.attempts[0].execution().clone(),
        observed_at: time(31),
      },
    )
    .unwrap();
    let foreign = CandidateSubject::new(
      ExactSubject::new(
        fixture.snapshot.work.subject().project_id(),
        RepositoryId::generate(),
        "0123456789abcdef".parse().unwrap(),
      ),
      "foreign-candidate".parse().unwrap(),
      digest(115),
    );
    assert!(
      plain
        .clone()
        .with_discovery(FlowDiscoveryFacts::candidate(foreign))
        .is_err(),
      "same Project cannot substitute another repository's candidate"
    );
    fixture.completions[0] = plain
      .completion(
        &fixture.attempts[0],
        root,
        FactoryClaimOwnership::new(fixture.completions[0].owner().clone(), fixture.completions[0].claim()),
        fixture.completions[0].usage(),
        time(31),
      )
      .unwrap();
    fixture.records[0] = plain;
    assert!(
      FlowNodeInput::prepare(fixture.preparation()).is_err(),
      "an accepted payload without retained candidate facts must not become code provenance"
    );
  });
}

#[test]
fn candidate_materialization_binds_selected_code_instead_of_requiring_the_work_repository() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let prepared = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    // Retained Artifact metadata uses the existing capture fixture; this test exercises
    // resource qualification, while byte/capture verification has its own public contracts.
    let (_, admitted, _, legacy_input, legacy_node) =
      crate::factory_node_test_support::fixtures::execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(legacy_input, &admitted, legacy_node, BudgetUsage::default(), 7).unwrap();
    let builds = crate::factory_node_test_support::Builds::new();
    let outputs = crate::factory_node_test_support::capture_outputs(&intent, &builds, "0123456789abcdef");
    let bundle = &outputs.documents[0].record;
    let manifest = &outputs.documents[1].record;
    let changes = ChangeSet::new(
      ChangeSetId::generate(),
      &fixture.inputs[0],
      &fixture.attempts[0],
      fixture.candidate.clone(),
      bundle.identity().artifact_id,
      manifest.identity().artifact_id,
    )
    .unwrap();
    let subject = prepared.task_subject().unwrap();
    let entries = [bundle, manifest]
      .into_iter()
      .map(|record| {
        let identity = record.identity();
        let artifact = FactoryArtifactReference::new(
          identity.artifact_id,
          FactoryDigest::from_bytes(identity.digest.as_bytes()),
          identity.size_bytes,
        )
        .unwrap();
        ContextManifestEntry::new(
          ContextSourceKind::Evidence,
          key(identity.logical_name.as_str()),
          subject.clone(),
          FactoryContextReference::Artifact(artifact.clone()),
          artifact.content_digest(),
          artifact.encoded_size(),
          FactorySafeText::new("Explicit retained candidate bundle").unwrap(),
          digest(116),
        )
        .unwrap()
      })
      .collect();
    let context = ContextManifest::new(ContextManifestId::generate(), subject, digest(117), entries).unwrap();
    let prepared = prepared.with_context(context, vec![]).unwrap();
    let materialized = crate::factory_node_candidate_materialization(&prepared, &changes, bundle, manifest, time(31));
    assert!(
      materialized.is_ok(),
      "selected-resource provenance must qualify independently of Work correlation: {materialized:?}"
    );
    let materialized = materialized.unwrap();
    assert_eq!(materialized.base_revision.as_str(), "0123456789abcdef");
    assert_eq!(materialized.candidate_revision.as_str(), "retained-candidate");
    assert!(prepared.subject().repository().is_err());
  });
}

#[test]
fn evaluation_branch_result_binds_the_selected_candidate_resource_independently_of_work() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let input = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    let root = fixture
      .snapshot
      .admitted_flow
      .closure()
      .definition(input.definition())
      .unwrap();
    let changes = ChangeSet::new(
      ChangeSetId::generate(),
      &fixture.inputs[0],
      &fixture.attempts[0],
      fixture.candidate.clone(),
      ArtifactId::generate(),
      ArtifactId::generate(),
    )
    .unwrap();
    let report_schema = exact("resource-report", 120);
    let report_tool = exact("resource-check", 121);
    let report_plugin = exact("resource-plugin", 122);
    let evidence = EvidenceManifest::new(
      EvidenceManifestId::generate(),
      &changes,
      fixture.candidate.clone(),
      time(31),
      vec![EvidenceRequirement::new(
        key("tests"),
        EvidenceOutputKind::Report,
        report_schema.clone(),
        report_tool.clone(),
        report_plugin.clone(),
      )],
      vec![EvidenceItem::new(EvidenceItemInput {
        subject: fixture.candidate.clone(),
        kind: key("tests"),
        output_kind: EvidenceOutputKind::Report,
        artifact: artifact(136),
        schema: report_schema,
        producer: EvidenceProducer::new(
          BuildId::generate(),
          AttemptId::generate(),
          JobId::generate(),
          report_tool,
          report_plugin,
        ),
        outcome: DeterministicGateOutcome::Passed,
        published_at: time(30),
        fresh_until: time(100),
      })],
    )
    .unwrap();
    let pack = CriterionPack::new(
      fixture.snapshot.work.subject().project_id(),
      exact("resource-quality", 123),
      exact("resource-criteria-schema", 124),
      artifact(123),
    )
    .unwrap();
    let evaluator = exact("resource-reviewer", 125);
    let data_handling = exact("resource-data-policy", 126);
    let budget = BudgetLimit::new(2, 1000, 100, 100, 1000).unwrap();
    let policy = EvaluationPolicy::try_new(vec![pack.reference().clone()], vec![evaluator.clone()], 1, budget).unwrap();
    let capability = ReviewEvaluatorCapability::try_new(
      evaluator.clone(),
      vec![ReviewPurpose::Implementation],
      vec![data_handling.clone()],
      budget,
    )
    .unwrap();
    let plan = prepare_evaluation_plan(
      EvaluationPlanDefinition {
        id: EvaluationPlanId::generate(),
        purpose: ReviewPurpose::Implementation,
        subject: fixture.candidate.clone(),
        criterion_packs: vec![pack],
        branches: vec![ReviewBranch::new(key("review"), evaluator.clone(), true)],
        budget,
        created_at: time(32),
        deadline: time(100),
        data_handling,
      },
      &evidence,
      &policy,
      &[capability],
    )
    .unwrap();
    let ReviewPlanPreparation::Ready(plan) = plan else {
      panic!("metadata fixture must qualify")
    };
    let subject = input.task_subject().unwrap();
    let selected = artifact(127);
    let entry = ContextManifestEntry::new(
      ContextSourceKind::Evidence,
      key("evidence"),
      subject.clone(),
      FactoryContextReference::Artifact(selected.clone()),
      selected.content_digest(),
      selected.encoded_size(),
      FactorySafeText::new("Explicit selected evidence").unwrap(),
      digest(128),
    )
    .unwrap();
    let input = input
      .with_context(
        ContextManifest::new(ContextManifestId::generate(), subject.clone(), digest(129), vec![entry]).unwrap(),
        vec![],
      )
      .unwrap();
    let owner = FactoryClaimOwnership::new(
      key("review-owner"),
      FactoryClaim::new(FactoryClaimFence::new(digest(130)), time(32), time(100)).unwrap(),
    );
    let node = root.node(input.node()).unwrap();
    let attempt = NodeAttempt::new(
      fixture.snapshot.admitted_flow.root_run(),
      fixture.snapshot.admitted_flow.initial_cycle(),
      root,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: node.key().clone(),
        node_kind: node.kind(),
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: node.budget(),
        started_at: time(33),
        deadline: time(100),
        execution: NodeExecutionIdentity::External(evaluator.clone()),
        ownership: owner,
      },
    )
    .unwrap();
    let assessment = Assessment::new(
      AssessmentId::generate(),
      &plan,
      AssessmentInput {
        subject: fixture.candidate.clone(),
        evaluator,
        outcome: AssessmentOutcome::Satisfied,
        summary: BoundedSummary::new(
          subject,
          FactorySafeText::new("Selected evidence satisfies the criteria").unwrap(),
          digest(131),
        ),
        findings: vec![],
        model: exact("bounded-model", 132),
        prompt_digest: digest(133),
        result: artifact(134),
        provenance: artifact(135),
      },
    )
    .unwrap();
    let result = EvaluationBranchResult::new(&plan, &input, &attempt, &assessment);
    assert!(
      result.is_ok(),
      "exact node resource, not Work repository equality, binds an assessment: {result:?}"
    );
    assert!(input.subject().repository().is_err());
  });
}
