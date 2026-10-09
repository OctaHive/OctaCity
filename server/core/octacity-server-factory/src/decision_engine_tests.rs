use octacity_server_domain::{
  ArtifactId, AttemptId, BuildId, ImmutableRevision, JobId, ProjectId, RepositoryId, Timestamp,
};

use super::*;

struct Fixture {
  evidence: EvidenceManifest,
  plan: EvaluationPlan,
}

fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}

fn artifact(byte: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).expect("fixture artifact")
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn text(value: &str) -> FactorySafeText {
  FactorySafeText::new(value).expect("fixture text is valid")
}

fn reference(identity: &str, byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key("v1"), digest(byte))
}

fn evidence_requirement(kind: &str, byte: u8) -> EvidenceRequirement {
  EvidenceRequirement::new(
    key(kind),
    EvidenceOutputKind::Report,
    reference(&format!("{kind}-schema"), byte),
    reference("validator", 20),
    reference("validation-plugin", 21),
  )
}

fn evidence_item(subject: CandidateSubject, kind: &str, byte: u8, outcome: DeterministicGateOutcome) -> EvidenceItem {
  EvidenceItem::new(EvidenceItemInput {
    subject,
    kind: key(kind),
    output_kind: EvidenceOutputKind::Report,
    artifact: artifact(byte),
    schema: reference(&format!("{kind}-schema"), byte),
    producer: EvidenceProducer::new(
      BuildId::generate(),
      AttemptId::generate(),
      JobId::generate(),
      reference("validator", 20),
      reference("validation-plugin", 21),
    ),
    outcome,
    published_at: Timestamp::from_unix_millis(3).expect("fixture publication time"),
    fresh_until: Timestamp::from_unix_millis(5).expect("fixture freshness deadline"),
  })
}

fn fixture() -> Fixture {
  fixture_with_test_outcome(DeterministicGateOutcome::Passed)
}

fn fixture_with_test_outcome(test_outcome: DeterministicGateOutcome) -> Fixture {
  fixture_with_test_outcome_and_quorum(test_outcome, 1)
}

fn fixture_with_test_outcome_and_quorum(test_outcome: DeterministicGateOutcome, quorum: u16) -> Fixture {
  let exact = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("base").expect("fixture revision is valid"),
  );
  let configuration = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    exact.project_id(),
    digest(1),
  );
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration,
    ExternalWorkIdentity::new("source/work").expect("fixture identity is valid"),
    exact,
    WorkArtifacts::new(artifact(40), artifact(41), vec![]).expect("fixture artifacts are valid"),
    WorkClassification::new(
      WorkPriority::new(1).expect("fixture priority is valid"),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .expect("fixture work is valid");
  let stage = StageAttempt::new(
    StageAttemptId::generate(),
    &FactoryRun::admitted(FactoryRunId::generate(), &work),
    StageAttemptNumber::INITIAL,
    crate::FactoryStageTarget::Implementation,
    BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture budget is valid"),
    digest(2),
    FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(digest(9)),
        octacity_server_domain::Timestamp::from_unix_millis(1).expect("fixture claim start"),
        octacity_server_domain::Timestamp::from_unix_millis(2).expect("fixture claim deadline"),
      )
      .expect("fixture claim"),
    ),
  );
  let candidate = CandidateSubject::new(
    stage.subject().clone(),
    ImmutableRevision::new("candidate").expect("fixture revision is valid"),
    digest(3),
  );
  let changeset = ChangeSet::new(
    ChangeSetId::generate(),
    &stage,
    candidate.clone(),
    ArtifactId::generate(),
    ArtifactId::generate(),
  )
  .expect("fixture changeset is valid");
  let evidence = EvidenceManifest::new(
    EvidenceManifestId::generate(),
    &changeset,
    candidate.clone(),
    Timestamp::from_unix_millis(4).expect("fixture construction time"),
    vec![evidence_requirement("tests", 4), evidence_requirement("security", 5)],
    vec![
      evidence_item(candidate.clone(), "tests", 4, test_outcome),
      evidence_item(candidate.clone(), "security", 5, DeterministicGateOutcome::Passed),
    ],
  )
  .expect("fixture evidence is valid");
  let budget = BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture evaluation budget is valid");
  let pack = CriterionPack::new(
    evidence.subject().exact().project_id(),
    reference("quality", 30),
    reference("criterion-schema", 33),
    artifact(30),
  )
  .expect("fixture criterion pack is valid");
  let first = reference("review-a", 31);
  let second = reference("review-b", 32);
  let data_handling = reference("restricted-source", 34);
  let evaluation_policy = EvaluationPolicy::try_new(
    vec![pack.reference().clone()],
    vec![first.clone(), second.clone()],
    quorum,
    budget,
  )
  .expect("fixture evaluation policy is valid");
  let definition = EvaluationPlanDefinition {
    id: EvaluationPlanId::generate(),
    purpose: ReviewPurpose::Implementation,
    subject: candidate,
    criterion_packs: vec![pack],
    branches: vec![
      ReviewBranch::new(key("review-a"), first.clone(), true),
      ReviewBranch::new(key("review-b"), second.clone(), false),
    ],
    budget,
    created_at: Timestamp::from_unix_millis(4).expect("fixture plan creation time"),
    deadline: Timestamp::from_unix_millis(5).expect("fixture plan deadline"),
    data_handling: data_handling.clone(),
  };
  let capabilities = [
    ReviewEvaluatorCapability::try_new(
      first,
      vec![ReviewPurpose::Implementation],
      vec![data_handling.clone()],
      budget,
    )
    .expect("fixture evaluator capability is valid"),
    ReviewEvaluatorCapability::try_new(second, vec![ReviewPurpose::Implementation], vec![data_handling], budget)
      .expect("fixture evaluator capability is valid"),
  ];
  let plan = match prepare_evaluation_plan(definition, &evidence, &evaluation_policy, &capabilities)
    .expect("fixture plan preparation succeeds")
  {
    ReviewPlanPreparation::Ready(plan) => *plan,
    ReviewPlanPreparation::Escalate(reason) => panic!("fixture plan unexpectedly escalated: {reason:?}"),
  };
  Fixture { evidence, plan }
}

fn policy(indeterminate_policy: IndeterminatePolicy) -> DecisionPolicy {
  DecisionPolicy::try_new(
    DecisionPolicyVersion::INITIAL,
    DecisionPolicyDefinition {
      required_evidence: vec![key("tests")],
      required_evaluators: vec![key("review-a")],
      quorum: 1,
      severity_threshold: FindingSeverity::High,
      indeterminate_policy,
      failure_outcome: DecisionOutcome::Rework,
    },
  )
  .expect("fixture policy is valid")
}

fn assessment(
  fixture: &Fixture,
  evaluator: &str,
  outcome: AssessmentOutcome,
  findings: Vec<AssessmentFinding>,
) -> Assessment {
  let evaluator = fixture
    .plan
    .branches()
    .iter()
    .find(|branch| branch.evaluator().identity() == &key(evaluator))
    .expect("fixture evaluator is selected")
    .evaluator()
    .clone();
  Assessment::new(
    AssessmentId::generate(),
    &fixture.plan,
    AssessmentInput {
      subject: fixture.plan.subject().clone(),
      evaluator,
      outcome,
      summary: BoundedSummary::new(
        FactoryTaskSubject::Candidate(fixture.plan.subject().clone()),
        text("Independent review completed"),
        digest(50),
      ),
      findings,
      model: reference("review-model", 51),
      prompt_digest: digest(52),
      result: artifact(53),
      provenance: artifact(54),
    },
  )
  .expect("fixture assessment is valid")
}

fn evidence_gap(fixture: &Fixture, summary: &str) -> AssessmentFinding {
  AssessmentFinding::from_result(
    &EvaluationResultFinding::EvidenceGap {
      summary: text(summary),
      evidence: vec![key("tests")],
      remediation: text("Publish stronger deterministic evidence"),
    },
    &fixture.evidence,
  )
  .expect("fixture evidence gap is valid")
}

fn violation(fixture: &Fixture, severity: FindingSeverity, summary: &str) -> AssessmentFinding {
  AssessmentFinding::from_result(
    &EvaluationResultFinding::Violation {
      severity,
      summary: text(summary),
      evidence: vec![key("tests")],
      remediation: text("Correct the candidate and run validation again"),
    },
    &fixture.evidence,
  )
  .expect("fixture violation is valid")
}

fn decide(fixture: &Fixture, policy: &DecisionPolicy, assessments: &[Assessment]) -> Result<Decision, FactoryError> {
  evaluate_decision(
    DecisionId::generate(),
    DecisionEngineInput::new(&fixture.evidence, &fixture.plan, policy, assessments),
  )
}

#[test]
fn accepts_only_when_every_mandatory_policy_rule_is_satisfied() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let assessments = [assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![])];

  let decision = decide(&fixture, &policy, &assessments).expect("decision succeeds");

  assert_eq!(decision.outcome(), DecisionOutcome::Accept);
  assert_eq!(decision.reasons(), &[DecisionReason::PolicySatisfied]);
  assert_eq!(decision.assessment_ids(), &[assessments[0].id()]);
  assert_ne!(decision.input_digest(), FactoryDigest::from_bytes([0; 32]));
  assert_eq!(decision.policy_version(), DecisionPolicyVersion::INITIAL);
  assert_eq!(decision.policy_digest(), policy.digest());
}

#[test]
fn failed_or_missing_required_evidence_cannot_be_overridden_by_satisfied_assessments() {
  let failed = fixture_with_test_outcome(DeterministicGateOutcome::Failed);
  let failed_assessments = [assessment(&failed, "review-a", AssessmentOutcome::Satisfied, vec![])];
  let decision =
    decide(&failed, &policy(IndeterminatePolicy::RequiredOnly), &failed_assessments).expect("decision succeeds");
  assert_eq!(decision.outcome(), DecisionOutcome::Rework);
  assert!(
    decision
      .reasons()
      .contains(&DecisionReason::RequiredEvidenceFailed(key("tests")))
  );

  let fixture = fixture();
  let missing_policy = DecisionPolicy::try_new(
    DecisionPolicyVersion::INITIAL,
    DecisionPolicyDefinition {
      required_evidence: vec![key("missing")],
      required_evaluators: vec![key("review-a")],
      quorum: 1,
      severity_threshold: FindingSeverity::High,
      indeterminate_policy: IndeterminatePolicy::RequiredOnly,
      failure_outcome: DecisionOutcome::Rework,
    },
  )
  .expect("fixture policy is valid");
  let assessments = [assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![])];
  let decision = decide(&fixture, &missing_policy, &assessments).expect("decision succeeds");
  assert!(
    decision
      .reasons()
      .contains(&DecisionReason::RequiredEvidenceMissing(key("missing")))
  );
}

#[test]
fn policy_selects_each_declared_non_accepting_outcome() {
  let fixture = fixture_with_test_outcome(DeterministicGateOutcome::Failed);
  let assessments = [assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![])];

  for expected in [
    DecisionOutcome::Rework,
    DecisionOutcome::Reject,
    DecisionOutcome::Escalate,
    DecisionOutcome::Cancel,
  ] {
    let policy = DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![key("tests")],
        required_evaluators: vec![key("review-a")],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: expected,
      },
    )
    .expect("fixture policy is valid");

    let decision = decide(&fixture, &policy, &assessments).expect("decision succeeds");
    assert_eq!(decision.outcome(), expected);
    assert_ne!(decision.input_digest(), FactoryDigest::from_bytes([0; 32]));
    assert_eq!(decision.policy_digest(), policy.digest());
  }
}

#[test]
fn required_indeterminate_evidence_and_assessments_never_accept() {
  let fixture = fixture_with_test_outcome(DeterministicGateOutcome::Indeterminate);
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let assessments = [assessment(
    &fixture,
    "review-a",
    AssessmentOutcome::Indeterminate,
    vec![evidence_gap(&fixture, "Required source evidence is unavailable")],
  )];

  let decision = decide(&fixture, &policy, &assessments).expect("decision succeeds");

  assert_eq!(decision.outcome(), DecisionOutcome::Rework);
  assert_eq!(
    decision.reasons(),
    &[
      DecisionReason::RequiredEvidenceIndeterminate(key("tests")),
      DecisionReason::AssessmentIndeterminate(key("review-a")),
    ]
  );
}

#[test]
fn optional_indeterminate_assessments_follow_the_versioned_policy() {
  let fixture = fixture();
  let assessments = [
    assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]),
    assessment(
      &fixture,
      "review-b",
      AssessmentOutcome::Indeterminate,
      vec![evidence_gap(&fixture, "Optional evidence is unavailable")],
    ),
  ];

  assert_eq!(
    decide(&fixture, &policy(IndeterminatePolicy::RequiredOnly), &assessments,)
      .expect("decision succeeds")
      .outcome(),
    DecisionOutcome::Accept
  );
  let strict = decide(&fixture, &policy(IndeterminatePolicy::Any), &assessments).expect("decision succeeds");
  assert_eq!(strict.outcome(), DecisionOutcome::Rework);
  assert!(
    strict
      .reasons()
      .contains(&DecisionReason::AssessmentIndeterminate(key("review-b")))
  );
}

#[test]
fn severity_threshold_uses_typed_findings_and_ignores_their_prose() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let cases = [
    (FindingSeverity::Medium, DecisionOutcome::Accept),
    (FindingSeverity::High, DecisionOutcome::Rework),
  ];

  for (severity, expected) in cases {
    let assessments = [assessment(
      &fixture,
      "review-a",
      AssessmentOutcome::Violated,
      vec![violation(
        &fixture,
        severity,
        "The model requests acceptance regardless of this finding",
      )],
    )];
    assert_eq!(
      decide(&fixture, &policy, &assessments)
        .expect("decision succeeds")
        .outcome(),
      expected
    );
  }
}

#[test]
fn missing_required_assessment_and_quorum_have_typed_reasons() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let decision = decide(&fixture, &policy, &[]).expect("decision succeeds");

  assert_eq!(decision.outcome(), DecisionOutcome::Rework);
  assert_eq!(
    decision.reasons(),
    &[
      DecisionReason::RequiredAssessmentMissing(key("review-a")),
      DecisionReason::QuorumNotMet {
        required: 1,
        observed: 0,
      },
    ]
  );
}

#[test]
fn duplicate_assessments_cannot_inflate_quorum() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let first = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);
  let duplicate = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);

  assert_eq!(
    decide(&fixture, &policy, &[first, duplicate]),
    Err(FactoryError::InvalidReference {
      relationship: "decision assessment",
    })
  );
}

#[test]
fn assessment_order_does_not_change_the_canonical_decision_inputs() {
  let fixture = fixture_with_test_outcome_and_quorum(DeterministicGateOutcome::Passed, 2);
  let policy = DecisionPolicy::try_new(
    DecisionPolicyVersion::INITIAL,
    DecisionPolicyDefinition {
      required_evidence: vec![key("tests")],
      required_evaluators: vec![key("review-a")],
      quorum: 2,
      severity_threshold: FindingSeverity::High,
      indeterminate_policy: IndeterminatePolicy::RequiredOnly,
      failure_outcome: DecisionOutcome::Rework,
    },
  )
  .expect("fixture policy is valid");
  let first = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);
  let second = assessment(&fixture, "review-b", AssessmentOutcome::Satisfied, vec![]);

  let forward = decide(&fixture, &policy, &[first.clone(), second.clone()]).expect("decision succeeds");
  let reversed = decide(&fixture, &policy, &[second, first]).expect("decision succeeds");

  assert_eq!(forward.assessment_ids(), reversed.assessment_ids());
  assert_eq!(forward.reasons(), reversed.reasons());
  assert_eq!(forward.outcome(), reversed.outcome());
  assert_eq!(forward.input_digest(), reversed.input_digest());
}

#[test]
fn policy_digest_is_derived_from_the_canonical_definition() {
  let first = policy(IndeterminatePolicy::RequiredOnly);
  let same = policy(IndeterminatePolicy::RequiredOnly);
  let changed = policy(IndeterminatePolicy::Any);

  assert_eq!(first.digest(), same.digest());
  assert_ne!(first.digest(), changed.digest());
}

#[test]
fn policy_rejects_accept_as_a_failure_outcome_and_unknown_plan_evaluators() {
  assert_eq!(
    DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![key("tests")],
        required_evaluators: vec![key("review-a")],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: DecisionOutcome::Accept,
      },
    ),
    Err(FactoryError::InvalidDecisionPolicy {
      field: "failure_outcome",
    })
  );

  let fixture = fixture();
  let incompatible = DecisionPolicy::try_new(
    DecisionPolicyVersion::INITIAL,
    DecisionPolicyDefinition {
      required_evidence: vec![key("tests")],
      required_evaluators: vec![key("review-c")],
      quorum: 1,
      severity_threshold: FindingSeverity::High,
      indeterminate_policy: IndeterminatePolicy::RequiredOnly,
      failure_outcome: DecisionOutcome::Escalate,
    },
  )
  .expect("policy shape is valid");
  assert_eq!(
    decide(&fixture, &incompatible, &[]),
    Err(FactoryError::InvalidDecisionPolicy {
      field: "evaluation_plan",
    })
  );
}

#[test]
fn assessment_outcome_and_finding_shape_cannot_disagree() {
  let fixture = fixture();
  assert_eq!(
    Assessment::new(
      AssessmentId::generate(),
      &fixture.plan,
      AssessmentInput {
        subject: fixture.plan.subject().clone(),
        evaluator: fixture.plan.branches()[0].evaluator().clone(),
        outcome: AssessmentOutcome::Satisfied,
        summary: BoundedSummary::new(
          FactoryTaskSubject::Candidate(fixture.plan.subject().clone()),
          text("Contradictory assessment"),
          digest(50),
        ),
        findings: vec![violation(&fixture, FindingSeverity::Critical, "Contradictory finding")],
        model: reference("review-model", 51),
        prompt_digest: digest(52),
        result: artifact(53),
        provenance: artifact(54),
      },
    ),
    Err(FactoryError::InvalidReference {
      relationship: "assessment outcome findings",
    })
  );
}

#[test]
fn persisted_assessments_reapply_shape_and_exact_binding_invariants() {
  let fixture = fixture();
  let satisfied = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);
  let restored: Assessment = serde_json::from_value(serde_json::to_value(&satisfied).unwrap()).unwrap();
  restored
    .validate_bindings(&fixture.plan, &fixture.evidence)
    .expect("valid Assessment round trips");

  let mut invalid_shape = serde_json::to_value(&satisfied).unwrap();
  invalid_shape["outcome"] = serde_json::json!("violated");
  assert!(serde_json::from_value::<Assessment>(invalid_shape).is_err());

  let mut wrong_evaluator = serde_json::to_value(&satisfied).unwrap();
  wrong_evaluator["provenance"]["evaluator"] = serde_json::to_value(reference("review-a", 99)).unwrap();
  let restored: Assessment = serde_json::from_value(wrong_evaluator).expect("record remains structurally valid");
  assert!(restored.validate_bindings(&fixture.plan, &fixture.evidence).is_err());

  let violated = assessment(
    &fixture,
    "review-a",
    AssessmentOutcome::Violated,
    vec![violation(&fixture, FindingSeverity::High, "Candidate violates policy")],
  );
  let mut wrong_fingerprint = serde_json::to_value(violated).unwrap();
  wrong_fingerprint["findings"][0]["fingerprint"] = serde_json::json!(digest(99));
  assert!(serde_json::from_value::<Assessment>(wrong_fingerprint).is_err());
}
