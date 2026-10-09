use octacity_server_domain::{
  ArtifactId, AttemptId, BuildId, ImmutableRevision, JobId, ProjectId, RepositoryId, Timestamp,
};

use super::*;

fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key")
}

fn reference(identity: &str, version: &str, byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(byte))
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).expect("fixture time")
}

fn budget(elapsed: u64) -> BudgetLimit {
  BudgetLimit::new(2, elapsed, 1_000, 1_000, 1_000).expect("fixture budget")
}

struct Fixture {
  project_id: ProjectId,
  evidence: EvidenceManifest,
  policy: EvaluationPolicy,
  packs: Vec<CriterionPack>,
  branches: Vec<ReviewBranch>,
  capabilities: Vec<ReviewEvaluatorCapability>,
  data_handling: ImmutableReference,
}

impl Fixture {
  fn definition(&self) -> EvaluationPlanDefinition {
    EvaluationPlanDefinition {
      id: EvaluationPlanId::generate(),
      purpose: ReviewPurpose::Implementation,
      subject: self.evidence.subject().clone(),
      criterion_packs: self.packs.clone(),
      branches: self.branches.clone(),
      budget: budget(1_000),
      created_at: time(10),
      deadline: time(1_010),
      data_handling: self.data_handling.clone(),
    }
  }
}

fn fixture() -> Fixture {
  let project_id = ProjectId::generate();
  let exact = ExactSubject::new(
    project_id,
    RepositoryId::generate(),
    ImmutableRevision::new("base").expect("fixture base"),
  );
  let configuration = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project_id,
    digest(1),
  );
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration,
    ExternalWorkIdentity::new("review/fixture").expect("fixture work identity"),
    exact,
    WorkArtifacts::new(artifact(30), artifact(31), vec![]).expect("fixture work artifacts"),
    WorkClassification::new(
      WorkPriority::new(1).expect("fixture priority"),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .expect("fixture work");
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let stage = StageAttempt::new(
    StageAttemptId::generate(),
    &run,
    StageAttemptNumber::INITIAL,
    FactoryStageTarget::Implementation,
    budget(1_000),
    digest(2),
    FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(FactoryClaimFence::new(digest(3)), time(1), time(2)).expect("fixture claim"),
    ),
  );
  let subject = CandidateSubject::new(
    stage.subject().clone(),
    ImmutableRevision::new("candidate").expect("fixture candidate"),
    digest(4),
  );
  let changeset = ChangeSet::new(
    ChangeSetId::generate(),
    &stage,
    subject.clone(),
    ArtifactId::generate(),
    ArtifactId::generate(),
  )
  .expect("fixture changeset");
  let schema = reference("test-schema", "v1", 5);
  let tool = reference("test-tool", "v1", 6);
  let plugin = reference("test-plugin", "v1", 7);
  let evidence = EvidenceManifest::new(
    EvidenceManifestId::generate(),
    &changeset,
    subject.clone(),
    time(9),
    vec![EvidenceRequirement::new(
      key("tests"),
      EvidenceOutputKind::Report,
      schema.clone(),
      tool.clone(),
      plugin.clone(),
    )],
    vec![EvidenceItem::new(EvidenceItemInput {
      subject,
      kind: key("tests"),
      output_kind: EvidenceOutputKind::Report,
      artifact: artifact(8),
      schema,
      producer: EvidenceProducer::new(
        BuildId::generate(),
        AttemptId::generate(),
        JobId::generate(),
        tool,
        plugin,
      ),
      outcome: DeterministicGateOutcome::Passed,
      published_at: time(8),
      fresh_until: time(2_000),
    })],
  )
  .expect("fixture evidence");
  let first_pack = criterion_pack(project_id, "quality", 10);
  let second_pack = criterion_pack(project_id, "security", 11);
  let first_evaluator = reference("review-a", "v1", 12);
  let second_evaluator = reference("review-b", "v1", 13);
  let data_handling = reference("restricted-source", "v1", 14);
  let policy = EvaluationPolicy::try_new(
    vec![first_pack.reference().clone(), second_pack.reference().clone()],
    vec![first_evaluator.clone(), second_evaluator.clone()],
    2,
    budget(1_000),
  )
  .expect("fixture evaluation policy");
  let branches = vec![
    ReviewBranch::new(key("review-b"), second_evaluator.clone(), false),
    ReviewBranch::new(key("review-a"), first_evaluator.clone(), true),
  ];
  let capabilities = vec![
    ReviewEvaluatorCapability::try_new(
      second_evaluator,
      vec![ReviewPurpose::Implementation],
      vec![data_handling.clone()],
      budget(1_000),
    )
    .expect("fixture capability"),
    ReviewEvaluatorCapability::try_new(
      first_evaluator,
      vec![ReviewPurpose::Requirements, ReviewPurpose::Implementation],
      vec![data_handling.clone()],
      budget(1_000),
    )
    .expect("fixture capability"),
  ];
  Fixture {
    project_id,
    evidence,
    policy,
    packs: vec![second_pack, first_pack],
    branches,
    capabilities,
    data_handling,
  }
}

fn artifact(byte: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).expect("fixture artifact")
}

fn criterion_pack(project_id: ProjectId, identity: &str, byte: u8) -> CriterionPack {
  CriterionPack::new(
    project_id,
    reference(identity, "v1", byte),
    reference("criterion-pack-schema", "v1", 20),
    artifact(byte),
  )
  .expect("fixture criterion pack")
}

fn planned(result: ReviewPlanPreparation) -> EvaluationPlan {
  match result {
    ReviewPlanPreparation::Ready(plan) => *plan,
    ReviewPlanPreparation::Escalate(reason) => panic!("unexpected escalation: {reason:?}"),
  }
}

#[test]
fn plan_binds_exact_operator_inputs_in_canonical_order() {
  let fixture = fixture();
  let plan = planned(
    prepare_evaluation_plan(
      fixture.definition(),
      &fixture.evidence,
      &fixture.policy,
      &fixture.capabilities,
    )
    .expect("review plan"),
  );

  assert_eq!(plan.purpose(), ReviewPurpose::Implementation);
  assert_eq!(plan.evidence_id(), fixture.evidence.id());
  assert_eq!(plan.subject(), fixture.evidence.subject());
  assert_eq!(plan.budget(), budget(1_000));
  assert_eq!(plan.created_at(), time(10));
  assert_eq!(plan.deadline(), time(1_010));
  assert_eq!(plan.data_handling(), &fixture.data_handling);
  assert_eq!(plan.required_quorum(), 2);
  assert_eq!(plan.criterion_packs()[0].reference().identity(), &key("quality"));
  assert_eq!(plan.branches()[0].key(), &key("review-a"));
  assert!(plan.branches()[0].is_required());
  assert_ne!(plan.digest().expect("plan digest"), digest(0));
}

#[test]
fn every_declared_factory_review_purpose_can_select_a_plan() {
  for purpose in [
    ReviewPurpose::Requirements,
    ReviewPurpose::Implementation,
    ReviewPurpose::CodeReview,
    ReviewPurpose::Verification,
    ReviewPurpose::Deployment,
  ] {
    let fixture = fixture();
    let mut definition = fixture.definition();
    definition.purpose = purpose;
    let capabilities = fixture
      .branches
      .iter()
      .map(|branch| {
        ReviewEvaluatorCapability::try_new(
          branch.evaluator().clone(),
          vec![purpose],
          vec![fixture.data_handling.clone()],
          budget(1_000),
        )
        .expect("purpose capability")
      })
      .collect::<Vec<_>>();

    let plan = planned(
      prepare_evaluation_plan(definition, &fixture.evidence, &fixture.policy, &capabilities).expect("purpose plan"),
    );
    assert_eq!(plan.purpose(), purpose);
  }
}

#[test]
fn configuration_replacement_cannot_change_an_active_plan() {
  let fixture = fixture();
  let plan = planned(
    prepare_evaluation_plan(
      fixture.definition(),
      &fixture.evidence,
      &fixture.policy,
      &fixture.capabilities,
    )
    .expect("review plan"),
  );
  let original_digest = plan.digest().expect("original plan digest");
  let replacement_pack = criterion_pack(fixture.project_id, "quality", 40);
  let replacement = EvaluationPolicy::try_new(
    vec![replacement_pack.reference().clone()],
    vec![reference("review-a", "v2", 41)],
    1,
    budget(500),
  )
  .expect("replacement policy");

  assert_ne!(replacement.criterion_packs(), fixture.policy.criterion_packs());
  assert_eq!(plan.digest().expect("frozen plan digest"), original_digest);
  assert_eq!(plan.criterion_packs()[0].reference().version(), &key("v1"));
  assert_eq!(plan.branches()[0].evaluator().version(), &key("v1"));
}

#[test]
fn unavailable_required_capability_escalates_before_a_plan_exists() {
  let fixture = fixture();
  let capabilities = fixture.capabilities[..1].to_vec();

  assert_eq!(
    prepare_evaluation_plan(fixture.definition(), &fixture.evidence, &fixture.policy, &capabilities,),
    Ok(ReviewPlanPreparation::Escalate(
      ReviewPlanEscalation::RequiredCapabilityUnavailable {
        branch: key("review-a"),
      },
    ))
  );
}

#[test]
fn incompatible_purpose_data_handling_and_budget_fail_closed() {
  let fixture = fixture();
  let first = fixture.branches[1].evaluator().clone();
  for capability in [
    ReviewEvaluatorCapability::try_new(
      first.clone(),
      vec![ReviewPurpose::CodeReview],
      vec![fixture.data_handling.clone()],
      budget(1_000),
    )
    .expect("purpose capability"),
    ReviewEvaluatorCapability::try_new(
      first.clone(),
      vec![ReviewPurpose::Implementation],
      vec![reference("public", "v1", 50)],
      budget(1_000),
    )
    .expect("data capability"),
    ReviewEvaluatorCapability::try_new(
      first,
      vec![ReviewPurpose::Implementation],
      vec![fixture.data_handling.clone()],
      budget(500),
    )
    .expect("budget capability"),
  ] {
    let mut capabilities = fixture.capabilities.clone();
    capabilities[1] = capability;
    assert!(matches!(
      prepare_evaluation_plan(fixture.definition(), &fixture.evidence, &fixture.policy, &capabilities,),
      Ok(ReviewPlanPreparation::Escalate(
        ReviewPlanEscalation::RequiredCapabilityUnavailable { .. }
      ))
    ));
  }
}

#[test]
fn plan_rejects_cross_project_packs_and_deadlines_outside_the_budget() {
  let fixture = fixture();
  let mut cross_project = fixture.definition();
  cross_project.criterion_packs[0] = criterion_pack(ProjectId::generate(), "security", 11);
  assert_eq!(
    prepare_evaluation_plan(cross_project, &fixture.evidence, &fixture.policy, &fixture.capabilities,),
    Err(FactoryError::InconsistentSubject)
  );

  let mut late = fixture.definition();
  late.deadline = time(1_011);
  assert_eq!(
    prepare_evaluation_plan(late, &fixture.evidence, &fixture.policy, &fixture.capabilities),
    Err(FactoryError::InvalidReference {
      relationship: "evaluation deadline",
    })
  );
}

#[test]
fn persisted_review_records_reapply_constructor_invariants() {
  let fixture = fixture();
  let plan = planned(
    prepare_evaluation_plan(
      fixture.definition(),
      &fixture.evidence,
      &fixture.policy,
      &fixture.capabilities,
    )
    .expect("review plan"),
  );

  let mut invalid_plan = serde_json::to_value(&plan).expect("serialize plan");
  invalid_plan["criterion_packs"] = serde_json::json!([]);
  assert!(serde_json::from_value::<EvaluationPlan>(invalid_plan).is_err());

  let mut invalid_manifest = serde_json::to_value(&fixture.evidence).expect("serialize evidence");
  invalid_manifest["items"] = serde_json::json!([]);
  assert!(serde_json::from_value::<EvidenceManifest>(invalid_manifest).is_err());

  let mut invalid_capability = serde_json::to_value(&fixture.capabilities[0]).expect("serialize capability");
  invalid_capability["purposes"] = serde_json::json!([]);
  assert!(serde_json::from_value::<ReviewEvaluatorCapability>(invalid_capability).is_err());

  let mut invalid_pack = serde_json::to_value(&fixture.packs[0]).expect("serialize criterion pack");
  invalid_pack["reference"]["digest"] = serde_json::json!(digest(99));
  assert!(serde_json::from_value::<CriterionPack>(invalid_pack).is_err());
}
