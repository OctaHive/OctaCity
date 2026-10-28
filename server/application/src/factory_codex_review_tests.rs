//! Real capture, metadata owner and retained ordinary evaluator publications.
use super::*;
use crate::factory_candidate_context_test_support::{
  ReadonlyCandidateFixtureProfile, candidate_context_fixture_with_preparation,
};

struct ReviewClock;
impl FactoryExecutionClock for ReviewClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(70))
  }
}
struct ReviewInputs;
#[async_trait]
impl FactoryFlowInputPreparer for ReviewInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    prepare_factory_flow_input(snapshot, target)
  }
}
async fn prepared_review_source() -> (
  SourceFixture,
  Arc<dyn FactoryRetainedCandidateContextSource>,
  Arc<dyn FactoryNodeCandidateContextSource>,
) {
  use crate::factory_review_planning_tests::{EvidenceAction, capture_review_contract};
  let (contracts, planning, mut profile) = capture_review_contract();
  profile.pipeline_node = authoritative_store_contract_fixture().request.jobs[0]
    .pipeline_node_id
    .clone();
  profile.candidate_capture_tool = Some(fixtures::reference("git.capture"));
  let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
  grants["resources"]["elapsed_millis"] = serde_json::json!(1000);
  profile.project_permissions = serde_json::from_value(grants).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
  let definition = contracts
    .admitted_flow
    .closure()
    .definition(contracts.admitted_flow.root_run().definition())
    .unwrap();
  let evidence_node = definition
    .nodes()
    .iter()
    .find(|n| {
      n.action()
        .is_some_and(|a| a.plugin().identity().as_str() == "qualified-evidence")
    })
    .unwrap()
    .clone();
  let plan_node = definition
    .nodes()
    .iter()
    .find(|n| {
      n.action()
        .is_some_and(|a| a.plugin().identity().as_str() == "installed-planner")
    })
    .unwrap()
    .clone();
  let review_node = definition
    .nodes()
    .iter()
    .find(|n| n.kind() == FlowNodeKind::Reasoning)
    .unwrap()
    .clone();
  let pack = planning.criterion_packs[0].artifact().clone();
  let pack_bytes = b"Review the actual candidate against the selected deterministic evidence.".to_vec();
  assert_eq!(FactoryDigest::content_sha256(&pack_bytes), pack.content_digest());
  let schemas = contracts.admitted_flow.data_schemas().to_vec();
  let terminals = definition.terminals().to_vec();
  let planning_id = plan_node.action().unwrap().plugin().clone();
  let evidence_id = evidence_node.action().unwrap().plugin().clone();
  let evidence_outcome = evidence_node.outcomes()[0].clone();
  let retained_profile = profile.clone();
  let review_grants = profile.project_permissions.clone();
  let configured_evidence = evidence_node.clone();
  let configured_plan = plan_node.clone();
  let candidate = candidate_context_fixture_with_preparation(
    ReadonlyCandidateFixtureProfile {
      work: contracts.work,
      build_configuration: profile.build_configuration.clone(),
      toolchain: profile.toolchain.clone(),
      permissions: profile.project_permissions.clone(),
      credentials: profile.credentials.clone(),
      instructions: profile.instructions.clone(),
    },
    None,
    move |flow| {
      let producer = flow.nodes[0].clone();
      let consumer_key = flow.nodes[1].key().clone();
      let mut encoded = serde_json::to_value(&review_node).unwrap();
      encoded["key"] = serde_json::to_value(&consumer_key).unwrap();
      encoded["permissions"] = serde_json::to_value(&review_grants).unwrap();
      let review = serde_json::from_value(encoded).unwrap();
      flow.nodes = vec![
        producer.clone(),
        configured_evidence.clone(),
        configured_plan.clone(),
        review,
      ];
      flow.transitions = vec![
        FlowTransition::new(
          producer.key().clone(),
          producer.outcomes()[0].key().clone(),
          FlowTransitionTarget::Node(configured_evidence.key().clone()),
        ),
        FlowTransition::new(
          configured_evidence.key().clone(),
          configured_evidence.outcomes()[0].key().clone(),
          FlowTransitionTarget::Node(configured_plan.key().clone()),
        ),
        FlowTransition::new(
          configured_plan.key().clone(),
          FactoryKey::new("planned").unwrap(),
          FlowTransitionTarget::Node(consumer_key.clone()),
        ),
        FlowTransition::new(
          configured_plan.key().clone(),
          FactoryKey::new("escalated").unwrap(),
          FlowTransitionTarget::Terminal(FactoryKey::new("needs_operator").unwrap()),
        ),
        FlowTransition::new(
          consumer_key,
          FactoryKey::new("checked").unwrap(),
          FlowTransitionTarget::Terminal(FactoryKey::new("finished").unwrap()),
        ),
      ];
      flow.terminals = terminals;
      flow.data_projections.clear();
      flow.execution = FlowExecutionPolicy::new(
        BudgetLimit::new(16, 30_000, 1000, 10000, 200000).unwrap(),
        contracts.admitted_flow.limits().permissions().clone(),
        1,
      )
      .unwrap();
    },
    schemas,
    Some((pack, pack_bytes)),
    async move |runs, claim, run_id, candidate| {
      let snapshot = runs.factory_run_snapshot(run_id).await.unwrap();
      let record = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|r| r.node_attempt_id() == candidate.node_attempt_id())
        .unwrap();
      let output = record
        .build_provenance()
        .unwrap()
        .outputs()
        .iter()
        .find(|o| o.output_kind == EvidenceOutputKind::Report)
        .unwrap();
      let evidence = EvidenceManifest::new(
        EvidenceManifestId::generate(),
        candidate,
        candidate.subject().clone(),
        at(32),
        vec![EvidenceRequirement::new(
          FactoryKey::new("tests").unwrap(),
          output.output_kind,
          output.schema.clone(),
          output.producer.tool().clone(),
          output.producer.plugin().clone(),
        )],
        vec![EvidenceItem::new(EvidenceItemInput {
          subject: candidate.subject().clone(),
          kind: FactoryKey::new("tests").unwrap(),
          output_kind: output.output_kind,
          artifact: output.artifact.clone(),
          schema: output.schema.clone(),
          producer: output.producer.clone(),
          outcome: DeterministicGateOutcome::Passed,
          published_at: output.verified_at,
          fresh_until: at(2000),
        })],
      )
      .unwrap();
      let executors = Arc::new(
        FactoryNodeExecutorRegistry::new(vec![
          (
            FlowNodeKind::TrustedAction,
            NodeExecutionIdentity::External(evidence_id),
            Arc::new(FactoryFlowActionAdapter::new(Arc::new(EvidenceAction {
              manifest: evidence,
              outcome: evidence_outcome,
              trusted: true,
            }))) as Arc<dyn FactoryNodeExecutor>,
          ),
          (
            FlowNodeKind::TrustedAction,
            NodeExecutionIdentity::External(planning_id.clone()),
            Arc::new(FactoryFlowActionAdapter::new(Arc::new(
              FactoryReviewPlanningAction::new(
                planning_id,
                runs.clone(),
                planning,
                crate::factory_node_test_support::clock(),
              ),
            ))),
          ),
          (
            FlowNodeKind::Reasoning,
            NodeExecutionIdentity::External(profile.toolchain.executable.clone()),
            Arc::new(NeverDispatch),
          ),
        ])
        .unwrap(),
      );
      let runner = FactoryFlowRunner::new(
        runs.clone(),
        executors,
        Arc::new(ReviewInputs),
        crate::factory_node_test_support::clock(),
      );
      for time in 33..60 {
        runner.reconcile_flow(run_id, claim, at(time)).await.unwrap();
        let snapshot = runs.factory_run_snapshot(run_id).await.unwrap();
        if snapshot
          .flow
          .data
          .records
          .iter()
          .any(|r| r.input().node() == plan_node.key())
        {
          return at(time + 1);
        }
      }
      panic!("real metadata owner must freeze the plan before the evaluator is prepared")
    },
  )
  .await;
  let profile = retained_profile;
  let input = candidate.intent.input();
  let invocation = FactoryStoredReviewInvocationSource::new(candidate.runs.clone())
    .invocation(input, &profile.reviewer.as_ref().unwrap().evaluator, at(70))
    .await
    .unwrap();
  let compiled = compile_codex_flow_task_with_options(
    &candidate.intent,
    &candidate.admitted,
    &profile.instructions,
    &profile.credentials,
    profile.consumer,
    &profile.usage_policy,
    FactoryCodexFlowCompilationOptions {
      role: None,
      candidate_context: Some(&candidate.context),
      reviewer: Some(FactoryCodexReviewCompilation {
        policy: profile.reviewer.as_ref().unwrap(),
        invocation: &invocation,
      }),
      ..Default::default()
    },
  )
  .unwrap();
  let source_inputs = [
    (
      candidate.context.bundle(),
      octacity_protocol::CHANGE_SET_BUNDLE_INPUT,
      octacity_protocol::CHANGE_SET_BUNDLE_MEDIA_TYPE,
    ),
    (
      candidate.context.manifest(),
      octacity_protocol::CHANGE_SET_MANIFEST_INPUT,
      octacity_protocol::CHANGE_SET_MANIFEST_MEDIA_TYPE,
    ),
  ]
  .into_iter()
  .map(|(record, destination, media_type)| {
    let identity = record.identity();
    octacity_protocol::ProtectedInputV3 {
      artifact_id: identity.artifact_id.to_string(),
      destination: destination.into(),
      media_type: media_type.into(),
      size_bytes: identity.size_bytes,
      sha256: identity.digest.to_string(),
    }
  })
  .collect();
  let schema = candidate
    .admitted
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .outcome(candidate.intent.result_outcome())
    .unwrap()
    .schema();
  let fixture = finish_prepared_source(PreparedSourceInput {
    native_mcp: false,
    runs: candidate.runs.clone(),
    base: authoritative_store_contract_fixture(),
    profile,
    intent: candidate.intent.clone(),
    context: Arc::new(ContextBytes(
      candidate.text.record.identity().artifact_id,
      candidate.text.bytes.clone(),
    )),
    schema: candidate.admitted.data_schema(schema).unwrap().clone(),
    compiled,
    source: Some((
      octacity_protocol::ChangeSetMaterializationV3 {
        bundle_input: candidate.bundle_id.to_string(),
        manifest_input: candidate.manifest_id.to_string(),
        candidate_revision: candidate
          .intent
          .input()
          .context()
          .unwrap()
          .subject()
          .candidate()
          .unwrap()
          .candidate_revision()
          .to_string(),
      },
      source_inputs,
    )),
    capture_paths: vec![],
    observation_override: None,
    model_result: serde_json::json!({"outcome":"satisfied","summary":"Selected criteria satisfied.","findings":[]}),
  })
  .await;
  let source = Arc::new(FactoryStoredCandidateContextSource::new(
    candidate.runs,
    candidate.source,
    fixtures::reference("git.capture"),
    Arc::new(ReviewClock),
  ));
  (fixture, source.clone(), source)
}

#[test]
fn retained_readonly_reviewer_job_produces_only_server_qualified_assessment() {
  crate::factory_admission_tests::run_ready(async {
    let (fixture, candidate, _) = prepared_review_source().await;
    let source = FactoryCodexBuildOutputSource::new(
      fixture.runs.clone(),
      fixture.query.clone(),
      fixture.artifacts.clone(),
      fixture.bytes.clone(),
      vec![fixture.profile.clone()],
      fixture.context.clone(),
      Arc::new(ReviewClock),
    )
    .unwrap()
    .with_retained_candidate_context_source(candidate);
    assert!(matches!(
      source
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
    let source =
      source.with_review_invocation_source(Arc::new(FactoryStoredReviewInvocationSource::new(fixture.runs.clone())));
    let FactoryNodeBuildOutputObservation::Verified(outputs) = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
      .await
      .unwrap()
    else {
      panic!("qualified readonly evaluator result")
    };
    let assessment: Assessment =
      serde_json::from_value(outputs.documents[0].data.as_ref().unwrap().payload().value().clone()).unwrap();
    let invocation = FactoryStoredReviewInvocationSource::new(fixture.runs.clone())
      .invocation(
        fixture.intent.input(),
        &fixture.profile.reviewer.as_ref().unwrap().evaluator,
        at(70),
      )
      .await
      .unwrap();
    assessment
      .validate_bindings(invocation.plan(), invocation.evidence())
      .unwrap();
    assert_eq!(assessment.outcome(), AssessmentOutcome::Satisfied);
    assert_eq!(
      assessment.provenance().result().artifact_id(),
      outputs.documents[0].record.identity().artifact_id
    );
    assert!(
      outputs.documents[0]
        .bytes
        .windows(b"assessment_id".len())
        .all(|s| s != b"assessment_id"),
      "the raw model document does not author the immutable Assessment"
    );
  });
}

#[test]
fn reviewer_materialization_qualifies_the_same_plan_before_publishing_and_signing_readonly_job() {
  crate::factory_admission_tests::run_ready(async {
    use crate::factory_command_materializer_tests::{Bytes, Context, Inputs, managed_context};
    let (fixture, _, candidate) = prepared_review_source().await;
    let snapshot = fixture
      .runs
      .factory_run_snapshot(fixture.intent.input().factory_run_id())
      .await
      .unwrap();
    let profile = &fixture.profile;
    let mut context = managed_context(
      &profile.build_configuration,
      &profile.pipeline_node,
      &profile.toolchain.plugin,
      &fixture.intent,
    );
    context.configuration.definition.runtime.network = ConfigurationNetworkPolicy::Restricted {
      allowed_hosts: std::collections::BTreeSet::from([
        octacity_server_domain::NetworkHost::new("api.openai.com").unwrap()
      ]),
    };
    context
      .effective_policy
      .policy
      .secret_profiles
      .insert(SecretProfileName::new("model-coding").unwrap());
    let inputs = Arc::new(Inputs::default());
    let materializer = FactoryCodexBuildMaterializer::new(
      Arc::new(Context(context)),
      Arc::new(FactoryExecutionInputPublisher::new(
        inputs.clone(),
        Arc::new(Bytes),
        Arc::new(ReviewClock),
      )),
      vec![profile.clone()],
      fixture.context.clone(),
      Arc::new(ReviewClock),
    )
    .unwrap()
    .with_candidate_context_source(candidate);
    let ownership = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
    assert!(matches!(
      materializer
        .materialize(&fixture.intent, &snapshot.admitted_flow, ownership.clone(), at(70))
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
    assert!(
      inputs.0.lock().unwrap().is_empty(),
      "unqualified review cannot publish private execution inputs"
    );
    let materializer = materializer
      .with_review_invocation_source(Arc::new(FactoryStoredReviewInvocationSource::new(fixture.runs.clone())));
    let accepted = materializer
      .materialize(&fixture.intent, &snapshot.admitted_flow, ownership, at(70))
      .await
      .unwrap();
    accepted.validate_fresh().unwrap();
    assert_eq!(accepted.jobs.len(), 1);
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 6);
    assert!(managed.factory().change_set_materialization.is_some());
    assert!(managed.factory().source_write_paths.is_none());
    assert!(managed.factory().change_set_capture.is_none());
    assert!(managed.permissions().tools.is_empty());
    assert_eq!(managed.execution().credential_profile.as_deref(), Some("model-coding"));
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
  });
}
#[test]
fn reviewer_owner_commits_typed_assessment_discovery_once_after_ordinary_job_verification() {
  crate::factory_admission_tests::run_ready(async {
    let (fixture, candidate, _) = prepared_review_source().await;
    let invocations = Arc::new(FactoryStoredReviewInvocationSource::new(fixture.runs.clone()));
    let source = Arc::new(
      FactoryCodexBuildOutputSource::new(
        fixture.runs.clone(),
        fixture.query.clone(),
        fixture.artifacts.clone(),
        fixture.bytes.clone(),
        vec![fixture.profile.clone()],
        fixture.context.clone(),
        Arc::new(ReviewClock),
      )
      .unwrap()
      .with_retained_candidate_context_source(candidate)
      .with_review_invocation_source(invocations.clone()),
    );
    let mut builds = crate::factory_node_test_support::Builds::new_at(fixture.query.build.created_at);
    builds.acceptance = FactoryBuildAcceptance {
      build_id: fixture.query.build.build.id,
      attempt_id: fixture.query.attempt.id,
      job_ids: vec![fixture.query.attempt.jobs[0].id()],
      effective_policy_digest: fixture.intent.permissions().digest(),
    };
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    *builds.latest.lock().unwrap() = Some(fixture.query.attempt.clone());
    let builds = Arc::new(builds);
    let adapter = FactoryCodexReviewBuildAdapter::new(
      builds.clone(),
      source,
      invocations,
      fixture.profile.reviewer.as_ref().unwrap().evaluator.clone(),
    );
    let owner = FactoryNodeRunner::new(fixture.runs.clone(), Arc::new(adapter), Arc::new(ReviewClock));
    let snapshot = fixture
      .runs
      .factory_run_snapshot(fixture.intent.input().factory_run_id())
      .await
      .unwrap();
    let claim = snapshot.current_claim.unwrap().id;
    let target = FactoryNodeTarget {
      flow_run_id: fixture.intent.input().flow_run_id(),
      cycle_id: fixture.intent.input().cycle_id(),
      node: fixture.intent.input().node().clone(),
    };
    let mut completed = None;
    for time in 70..75 {
      if let FactoryNodeStep::Completed(record) = owner
        .run_once(snapshot.run.id(), claim, target.clone(), None, at(time))
        .await
        .unwrap()
      {
        completed = Some(record);
        break;
      }
    }
    let record = completed.expect("ordinary evaluator must complete through its owner");
    let assessment: Assessment = serde_json::from_value(record.payload().value().clone()).unwrap();
    assert_eq!(
      record.discovery(),
      Some(&FlowDiscoveryFacts::assessment(&assessment).unwrap())
    );
    let persisted = fixture.runs.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    assert!(matches!(
      owner
        .run_once(snapshot.run.id(), claim, target, None, at(75))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    assert_eq!(
      fixture.runs.factory_run_snapshot(snapshot.run.id()).await.unwrap(),
      persisted
    );
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}
