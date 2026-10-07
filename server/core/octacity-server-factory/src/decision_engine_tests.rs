use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};

use super::*;

struct Fixture {
  evidence: EvidenceManifest,
  plan: EvaluationPlan,
}

fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn text(value: &str) -> FactoryText {
  FactoryText::new(value).expect("fixture text is valid")
}

fn fixture() -> Fixture {
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
    WorkArtifacts::new(ArtifactId::generate(), ArtifactId::generate(), vec![]).expect("fixture artifacts are valid"),
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
    vec![
      EvidenceItem::new(key("tests"), ArtifactId::generate(), digest(4)),
      EvidenceItem::new(key("security"), ArtifactId::generate(), digest(5)),
    ],
  )
  .expect("fixture evidence is valid");
  let plan = EvaluationPlan::new(
    EvaluationPlanId::generate(),
    &evidence,
    candidate,
    vec![key("quality")],
    vec![key("review-a"), key("review-b")],
  )
  .expect("fixture plan is valid");
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

fn gate(kind: &str, evidence_digest: FactoryDigest, outcome: DeterministicGateOutcome) -> DeterministicGate {
  DeterministicGate::new(key(kind), evidence_digest, outcome)
}

fn assessment(
  fixture: &Fixture,
  evaluator: &str,
  outcome: AssessmentOutcome,
  findings: Vec<AssessmentFinding>,
) -> Assessment {
  Assessment::new(
    AssessmentId::generate(),
    &fixture.plan,
    fixture.plan.subject().clone(),
    key(evaluator),
    outcome,
    findings,
  )
  .expect("fixture assessment is valid")
}

fn decide(
  fixture: &Fixture,
  policy: &DecisionPolicy,
  gates: &[DeterministicGate],
  assessments: &[Assessment],
) -> Result<Decision, FactoryError> {
  evaluate_decision(
    DecisionId::generate(),
    DecisionEngineInput::new(&fixture.evidence, &fixture.plan, policy, gates, assessments),
  )
}

#[test]
fn accepts_only_when_every_mandatory_policy_rule_is_satisfied() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];
  let assessments = [assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![])];

  let decision = decide(&fixture, &policy, &gates, &assessments).expect("decision succeeds");

  assert_eq!(decision.outcome(), DecisionOutcome::Accept);
  assert_eq!(decision.reasons(), &[DecisionReason::PolicySatisfied]);
  assert_eq!(decision.assessment_ids(), &[assessments[0].id()]);
  assert_ne!(decision.input_digest(), FactoryDigest::from_bytes([0; 32]));
  assert_eq!(decision.policy_version(), DecisionPolicyVersion::INITIAL);
  assert_eq!(decision.policy_digest(), policy.digest());
}

#[test]
fn failed_or_missing_required_evidence_cannot_be_overridden_by_satisfied_assessments() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let assessments = [assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![])];
  let cases = [
    (
      vec![gate("tests", digest(4), DeterministicGateOutcome::Failed)],
      DecisionReason::RequiredEvidenceFailed(key("tests")),
    ),
    (vec![], DecisionReason::RequiredEvidenceMissing(key("tests"))),
  ];

  for (gates, reason) in cases {
    let decision = decide(&fixture, &policy, &gates, &assessments).expect("decision succeeds");
    assert_eq!(decision.outcome(), DecisionOutcome::Rework);
    assert!(decision.reasons().contains(&reason));
  }
}

#[test]
fn policy_selects_each_declared_non_accepting_outcome() {
  let fixture = fixture();
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Failed)];
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

    let decision = decide(&fixture, &policy, &gates, &assessments).expect("decision succeeds");
    assert_eq!(decision.outcome(), expected);
    assert_ne!(decision.input_digest(), FactoryDigest::from_bytes([0; 32]));
    assert_eq!(decision.policy_digest(), policy.digest());
  }
}

#[test]
fn required_indeterminate_evidence_and_assessments_never_accept() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Indeterminate)];
  let assessments = [assessment(
    &fixture,
    "review-a",
    AssessmentOutcome::Indeterminate,
    vec![AssessmentFinding::EvidenceGap {
      summary: text("Required source evidence is unavailable"),
    }],
  )];

  let decision = decide(&fixture, &policy, &gates, &assessments).expect("decision succeeds");

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
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];
  let assessments = [
    assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]),
    assessment(
      &fixture,
      "review-b",
      AssessmentOutcome::Indeterminate,
      vec![AssessmentFinding::EvidenceGap {
        summary: text("Optional evidence is unavailable"),
      }],
    ),
  ];

  assert_eq!(
    decide(
      &fixture,
      &policy(IndeterminatePolicy::RequiredOnly),
      &gates,
      &assessments,
    )
    .expect("decision succeeds")
    .outcome(),
    DecisionOutcome::Accept
  );
  let strict = decide(&fixture, &policy(IndeterminatePolicy::Any), &gates, &assessments).expect("decision succeeds");
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
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];
  let cases = [
    (FindingSeverity::Medium, DecisionOutcome::Accept),
    (FindingSeverity::High, DecisionOutcome::Rework),
  ];

  for (severity, expected) in cases {
    let assessments = [assessment(
      &fixture,
      "review-a",
      AssessmentOutcome::Violated,
      vec![AssessmentFinding::Violation {
        severity,
        summary: text("The model requests acceptance regardless of this finding"),
      }],
    )];
    assert_eq!(
      decide(&fixture, &policy, &gates, &assessments)
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
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];

  let decision = decide(&fixture, &policy, &gates, &[]).expect("decision succeeds");

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
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];
  let first = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);
  let duplicate = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);

  assert_eq!(
    decide(&fixture, &policy, &gates, &[first, duplicate]),
    Err(FactoryError::InvalidReference {
      relationship: "decision assessment",
    })
  );
}

#[test]
fn assessment_order_does_not_change_the_canonical_decision_inputs() {
  let fixture = fixture();
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
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];
  let first = assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![]);
  let second = assessment(&fixture, "review-b", AssessmentOutcome::Satisfied, vec![]);

  let forward = decide(&fixture, &policy, &gates, &[first.clone(), second.clone()]).expect("decision succeeds");
  let reversed = decide(&fixture, &policy, &gates, &[second, first]).expect("decision succeeds");

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
fn gates_must_reference_exact_manifest_content() {
  let fixture = fixture();
  let policy = policy(IndeterminatePolicy::RequiredOnly);
  let assessments = [assessment(&fixture, "review-a", AssessmentOutcome::Satisfied, vec![])];
  let gates = [gate("tests", digest(99), DeterministicGateOutcome::Passed)];

  assert_eq!(
    decide(&fixture, &policy, &gates, &assessments),
    Err(FactoryError::InvalidReference {
      relationship: "decision gate evidence",
    })
  );
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
  let gates = [gate("tests", digest(4), DeterministicGateOutcome::Passed)];
  assert_eq!(
    decide(&fixture, &incompatible, &gates, &[]),
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
      fixture.plan.subject().clone(),
      key("review-a"),
      AssessmentOutcome::Satisfied,
      vec![AssessmentFinding::Violation {
        severity: FindingSeverity::Critical,
        summary: text("Contradictory finding"),
      }],
    ),
    Err(FactoryError::InvalidReference {
      relationship: "assessment outcome findings",
    })
  );
}
