use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use uuid::Uuid;

use crate as factory;
use crate::*;
mod journey;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(
    ArtifactId::from_uuid(Uuid::from_u128(u128::from(value))).unwrap(),
    digest(value),
    10,
  )
  .unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), digest(1))
}
fn subject() -> ExactSubject {
  ExactSubject::new(
    ProjectId::from_uuid(Uuid::from_u128(1)).unwrap(),
    RepositoryId::from_uuid(Uuid::from_u128(2)).unwrap(),
    ImmutableRevision::new("base").unwrap(),
  )
}
fn configuration() -> FactoryConfigurationRef {
  FactoryConfigurationRef::new(
    FactoryConfigurationId::from_uuid(Uuid::from_u128(3)).unwrap(),
    FactoryConfigurationVersion::INITIAL,
    subject().project_id(),
    digest(3),
  )
}
fn work_id(value: u128) -> WorkEnvelopeId {
  WorkEnvelopeId::from_uuid(Uuid::from_u128(value)).unwrap()
}
fn definition_ref(value: u128) -> FlowDefinitionRef {
  FlowDefinitionRef::new(
    FlowDefinitionId::from_uuid(Uuid::from_u128(value)).unwrap(),
    FlowDefinitionVersion::new(1).unwrap(),
    digest(4),
  )
}
fn work() -> WorkEnvelope {
  WorkEnvelope::new(
    work_id(10),
    configuration(),
    ExternalWorkIdentity::new("ticket").unwrap(),
    subject(),
    WorkArtifacts::new(artifact(10), artifact(11), vec![]).unwrap(),
    WorkClassification::new(
      WorkPriority::new(1).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap()
}
fn input(candidates: Vec<DuplicateCandidate>) -> EligibilityInput {
  EligibilityInput::new(&work(), artifact(12), candidates).unwrap()
}
fn candidate(value: u128) -> DuplicateCandidate {
  DuplicateCandidate {
    work_id: work_id(value),
    subject: subject(),
    evidence: artifact(13),
  }
}
fn provenance(input_digest: FactoryDigest, definition: FlowDefinitionRef) -> TriageProvenance {
  TriageProvenance {
    input_digest,
    definition,
    node_attempt_id: NodeAttemptId::from_uuid(Uuid::from_u128(15)).unwrap(),
    producer: reference("harness"),
    model_or_tool: reference("model"),
    task_digest: digest(16),
    result: artifact(17),
    observed_at: Timestamp::from_unix_millis(20).unwrap(),
  }
}
fn observation<T>(value: T) -> TriageObservation<T> {
  TriageObservation::new(value, artifact(18))
}
fn eligible(input: &EligibilityInput, definition: FlowDefinitionRef) -> EligibilityResult {
  EligibilityResult::new(
    input,
    provenance(input.digest().unwrap(), definition),
    observation(ProjectFit::InScope),
    input
      .duplicate_candidates()
      .iter()
      .map(|candidate| {
        observation(DuplicateAssessment {
          candidate_id: candidate.work_id,
          status: DuplicateStatus::Distinct,
        })
      })
      .collect(),
  )
  .unwrap()
}
fn classification_input() -> ClassificationInput {
  let input = input(vec![]);
  ClassificationInput::accepted(input.clone(), eligible(&input, definition_ref(20)), digest(21)).unwrap()
}

pub(crate) fn research_fixture(kind: WorkKind, size: WorkSize) -> (WorkEnvelope, AdmittedFlow, ClassificationReceipt) {
  let mut flow = journey::journey(false, false, budget());
  flow.triage.project_goals = artifact(12);
  let profile = FactoryTriageExecutionProfile {
    producer: reference("harness"),
    model_or_tool: reference("model"),
    task_digest: digest(16),
  };
  flow.triage.eligibility = profile.clone();
  flow.triage.classification = profile;
  flow.limits = FlowAdmissionLimits::product_defaults(
    4,
    BudgetLimit::new(16, 10000, 1000, 10000, 10000).unwrap(),
    FactoryPermissionSet::deny_all(),
  )
  .unwrap();
  let configuration = crate::configuration_tests::publish_flow_fixture(&configuration(), flow.clone());
  let work = work();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let admitted = AdmittedFlow::from_configuration(&configuration, &run).unwrap();
  let closure = admitted.validated().unwrap();
  let policy = TriagePolicy::for_flow(
    configuration.reference().clone(),
    &closure,
    flow.triage.definition,
    flow.triage.policy.clone(),
  )
  .unwrap();
  let input = EligibilityInput::new(&work, flow.triage.project_goals, vec![]).unwrap();
  let phase_reference = |role: TriageNode| {
    closure
      .closure()
      .definition(flow.triage.definition)
      .unwrap()
      .node(&role.key())
      .unwrap()
      .subflow_definition()
      .unwrap()
  };
  let result = eligible(&input, phase_reference(TriageNode::Eligibility));
  let evidence = accepted_evidence(&input, &result);
  let usage = BudgetUsage::default();
  let decision = policy.eligibility(&input, &result, &evidence, usage).unwrap();
  let eligibility = EligibilityReceipt {
    input: input.clone(),
    result: result.clone(),
    evidence: evidence.clone(),
    usage,
    decision,
  };
  let accepted = policy.classification_input(input, result, &evidence, usage).unwrap();
  let mut classified = classification();
  classified.work_kind = observation(kind);
  classified.size = observation(size);
  classified.reproducibility = observation(if kind == WorkKind::Defect {
    PreliminaryReproducibility::Inconclusive
  } else {
    PreliminaryReproducibility::NotApplicable
  });
  classified.recommended_route = observation(TriageRoute::Research);
  let result = TriageResult::new(
    &accepted,
    provenance(accepted.digest().unwrap(), phase_reference(TriageNode::Classification)),
    classified,
  )
  .unwrap();
  let decision = policy.route(&accepted, &result, &evidence, &[], usage).unwrap();
  let receipt = ClassificationReceipt {
    eligibility,
    result,
    evidence: vec![],
    usage,
    decision,
  };
  (work, admitted, receipt)
}

fn classification() -> TriageClassification {
  TriageClassification {
    work_kind: observation(WorkKind::Defect),
    component: observation(key("compiler")),
    severity: observation(FindingSeverity::High),
    size: observation(WorkSize::Small),
    risk: observation(RiskClass::Low),
    dependencies: vec![],
    reproducibility: observation(PreliminaryReproducibility::Reproduced),
    recommended_route: observation(TriageRoute::Development),
  }
}
fn result(input: &ClassificationInput, classification: TriageClassification) -> TriageResult {
  TriageResult::new(
    input,
    provenance(input.digest().unwrap(), definition_ref(22)),
    classification,
  )
  .unwrap()
}

#[test]
fn canonical_inputs_results_and_every_observation_retain_provenance() {
  let input = input(vec![candidate(30), candidate(29)]);
  let ordered = self::input(vec![candidate(29), candidate(30)]);
  assert_eq!(input, ordered);
  assert_eq!(input.digest().unwrap(), ordered.digest().unwrap());
  let eligibility = eligible(&input, definition_ref(20));
  let accepted = ClassificationInput::accepted(input, eligibility, digest(21)).unwrap();
  let result = result(&accepted, classification());
  let encoded = serde_json::to_vec(&result).unwrap();
  let replay: TriageResult = serde_json::from_slice(&encoded).unwrap();
  assert_eq!(result, replay);
  assert_eq!(result.digest().unwrap(), replay.digest().unwrap());
  assert_eq!(result.provenance().producer, reference("harness"));
  assert_eq!(result.classification().component.evidence(), &artifact(18));
  let replay: ClassificationInput = serde_json::from_slice(&serde_json::to_vec(&accepted).unwrap()).unwrap();
  assert_eq!(accepted, replay);
}

#[test]
fn missing_invented_duplicate_candidates_and_input_substitution_are_rejected() {
  let input = input(vec![candidate(30)]);
  let provenance = provenance(input.digest().unwrap(), definition_ref(20));
  assert!(EligibilityResult::new(&input, provenance.clone(), observation(ProjectFit::InScope), vec![]).is_err());
  assert!(
    EligibilityResult::new(
      &input,
      provenance.clone(),
      observation(ProjectFit::InScope),
      vec![observation(DuplicateAssessment {
        candidate_id: work_id(31),
        status: DuplicateStatus::Distinct
      })]
    )
    .is_err()
  );
  let mut substituted = provenance;
  substituted.input_digest = digest(99);
  assert!(EligibilityResult::new(&input, substituted, observation(ProjectFit::InScope), vec![]).is_err());
}

#[test]
fn bounds_duplicates_cross_project_and_self_references_are_rejected() {
  let mut wire = serde_json::to_value(input(vec![])).unwrap();
  wire["duplicate_candidates"] = serde_json::to_value(vec![candidate(30); MAX_TRIAGE_OBSERVATIONS + 1]).unwrap();
  assert!(serde_json::from_value::<EligibilityInput>(wire).is_err());
  let mut wire = serde_json::to_value(input(vec![])).unwrap();
  wire["duplicate_candidates"] = serde_json::to_value(vec![candidate(30), candidate(30)]).unwrap();
  assert!(serde_json::from_value::<EligibilityInput>(wire).is_err());
  let mut foreign = candidate(30);
  foreign.subject = ExactSubject::new(
    ProjectId::from_uuid(Uuid::from_u128(99)).unwrap(),
    subject().repository_id(),
    subject().base_revision().clone(),
  );
  let mut wire = serde_json::to_value(input(vec![])).unwrap();
  wire["duplicate_candidates"] = serde_json::to_value(vec![foreign]).unwrap();
  assert!(serde_json::from_value::<EligibilityInput>(wire).is_err());
  let accepted = classification_input();
  let mut observation = classification();
  observation.dependencies.push(self::observation(TriageDependency {
    work_id: work_id(10),
    work_digest: digest(10),
  }));
  assert!(
    TriageResult::new(
      &accepted,
      provenance(accepted.digest().unwrap(), definition_ref(22)),
      observation
    )
    .is_err()
  );
}

#[test]
fn terminal_eligibility_cannot_construct_or_deserialize_classification_input() {
  for fit in [ProjectFit::OutOfScope, ProjectFit::Inconclusive] {
    let input = input(vec![]);
    let eligibility = EligibilityResult::new(
      &input,
      provenance(input.digest().unwrap(), definition_ref(20)),
      observation(fit),
      vec![],
    )
    .unwrap();
    assert!(ClassificationInput::accepted(input, eligibility.clone(), digest(21)).is_err());
    let mut wire = serde_json::to_value(classification_input()).unwrap();
    wire["eligibility_result"] = serde_json::to_value(eligibility).unwrap();
    assert!(serde_json::from_value::<ClassificationInput>(wire).is_err());
  }
}

#[test]
fn unknown_routes_authority_fields_missing_provenance_and_wrong_kind_are_rejected() {
  let input = classification_input();
  let result = result(&input, classification());
  for (field, value) in [("route", "merge"), ("version", "invalid"), ("authority", "dispatch")] {
    let mut wire = serde_json::to_value(&result).unwrap();
    match field {
      "route" => wire["classification"]["recommended_route"]["value"] = value.into(),
      "version" => wire["schema_version"] = 2.into(),
      _ => wire["authority"] = value.into(),
    }
    assert!(serde_json::from_value::<TriageResult>(wire).is_err());
  }
  let mut wire = serde_json::to_value(&result).unwrap();
  wire["classification"]["component"]
    .as_object_mut()
    .unwrap()
    .remove("evidence");
  assert!(serde_json::from_value::<TriageResult>(wire).is_err());
  let mut classified = classification();
  classified.work_kind = observation(WorkKind::FeatureRequest);
  assert!(
    TriageResult::new(
      &input,
      provenance(input.digest().unwrap(), definition_ref(22)),
      classified
    )
    .is_err()
  );
}

fn budget() -> BudgetLimit {
  BudgetLimit::new(16, 1_000, 1_000, 1_000, 1_000).unwrap()
}
fn phase(id: u128, input: TriageSchema, output: TriageSchema, kind: FlowNodeKind) -> FlowDefinition {
  FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(Uuid::from_u128(id)).unwrap(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: Some(input.reference().unwrap()),
    entry: key("observe"),
    nodes: vec![
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key("observe"),
        kind,
        input_schema: Some(input.reference().unwrap()),
        outcomes: vec![FlowOutcomeDefinition::new(
          key("observed"),
          FlowOutcomeKind::Success,
          output.reference().unwrap(),
        )],
        budget: budget(),
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap(),
    ],
    transitions: vec![FlowTransition::new(
      key("observe"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("observed")),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      key("observed"),
      output.reference().unwrap(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap()
}
fn closure(classification_id: u128, routes: Vec<TriageRoute>) -> ValidatedFlowDefinitionClosure {
  let eligibility = phase(
    30,
    TriageSchema::EligibilityInput,
    TriageSchema::EligibilityResult,
    FlowNodeKind::Reasoning,
  );
  let classification = phase(
    classification_id,
    TriageSchema::ClassificationInput,
    TriageSchema::TriageResult,
    FlowNodeKind::Reasoning,
  );
  let root = compose_triage_flow(
    FlowDefinitionId::from_uuid(Uuid::from_u128(40)).unwrap(),
    FlowDefinitionVersion::new(1).unwrap(),
    &eligibility,
    &classification,
    routes,
    FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 1).unwrap(),
  )
  .unwrap();
  PinnedFlowDefinitionClosure::new(root.reference(), vec![root, classification, eligibility])
    .unwrap()
    .validate(FlowAdmissionLimits::product_defaults(1, budget(), FactoryPermissionSet::deny_all()).unwrap())
    .unwrap()
}
fn policy(closure: &ValidatedFlowDefinitionClosure) -> TriagePolicy {
  TriagePolicy::new(
    configuration(),
    closure,
    TriagePolicySettings {
      max_risk: RiskClass::Medium,
      small_work_route: Some(TriageRoute::ProtectedTest),
      budget: budget(),
    },
  )
  .unwrap()
}
fn phase_reference(closure: &ValidatedFlowDefinitionClosure, name: &str) -> FlowDefinitionRef {
  closure
    .closure()
    .definition(closure.closure().root())
    .unwrap()
    .node(&key(name))
    .unwrap()
    .subflow_definition()
    .unwrap()
}
fn accepted_evidence(input: &EligibilityInput, result: &EligibilityResult) -> Vec<AcceptedTriageEvidence> {
  let mut evidence = vec![
    AcceptedTriageEvidence::new(
      input.subject().clone(),
      input.digest().unwrap(),
      TriageEvidenceFact::ProjectFit(*result.project_fit().value()),
      result.project_fit().evidence().clone(),
      DeterministicGateOutcome::Passed,
    )
    .unwrap(),
  ];
  evidence.extend(result.duplicates().iter().map(|record| {
    AcceptedTriageEvidence::new(
      input.subject().clone(),
      input.digest().unwrap(),
      TriageEvidenceFact::Duplicate(record.value().clone()),
      record.evidence().clone(),
      DeterministicGateOutcome::Passed,
    )
    .unwrap()
  }));
  evidence
}
fn routing_evidence(input: &ClassificationInput, fact: TriageEvidenceFact) -> AcceptedTriageEvidence {
  AcceptedTriageEvidence::new(
    input.eligibility_input().subject().clone(),
    input.digest().unwrap(),
    fact,
    artifact(18),
    DeterministicGateOutcome::Passed,
  )
  .unwrap()
}
fn accepted_classification(
  closure: &ValidatedFlowDefinitionClosure,
  policy: &TriagePolicy,
) -> (ClassificationInput, Vec<AcceptedTriageEvidence>) {
  let input = input(vec![candidate(29)]);
  let result = eligible(&input, phase_reference(closure, "eligibility"));
  let evidence = accepted_evidence(&input, &result);
  (
    policy
      .classification_input(input, result, &evidence, BudgetUsage::default())
      .unwrap(),
    evidence,
  )
}
fn routed_result(
  closure: &ValidatedFlowDefinitionClosure,
  input: &ClassificationInput,
  classification: TriageClassification,
) -> TriageResult {
  TriageResult::new(
    input,
    provenance(input.digest().unwrap(), phase_reference(closure, "classification")),
    classification,
  )
  .unwrap()
}

#[test]
fn eligibility_policy_stops_duplicates_out_of_scope_uncertainty_and_exhaustion() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  for (fit, status, expected, reason) in [
    (
      ProjectFit::InScope,
      DuplicateStatus::Distinct,
      EligibilityOutcome::Classify,
      TriageReason::Eligible,
    ),
    (
      ProjectFit::InScope,
      DuplicateStatus::Duplicate,
      EligibilityOutcome::Rejection,
      TriageReason::Duplicate,
    ),
    (
      ProjectFit::OutOfScope,
      DuplicateStatus::Distinct,
      EligibilityOutcome::Rejection,
      TriageReason::OutOfScope,
    ),
    (
      ProjectFit::Inconclusive,
      DuplicateStatus::Distinct,
      EligibilityOutcome::Escalation,
      TriageReason::Inconclusive,
    ),
    (
      ProjectFit::InScope,
      DuplicateStatus::Inconclusive,
      EligibilityOutcome::Escalation,
      TriageReason::Inconclusive,
    ),
  ] {
    let input = input(vec![candidate(29)]);
    let result = EligibilityResult::new(
      &input,
      provenance(input.digest().unwrap(), phase_reference(&closure, "eligibility")),
      observation(fit),
      vec![observation(DuplicateAssessment {
        candidate_id: work_id(29),
        status,
      })],
    )
    .unwrap();
    let evidence = accepted_evidence(&input, &result);
    let decision = policy
      .eligibility(&input, &result, &evidence, BudgetUsage::default())
      .unwrap();
    assert_eq!((decision.outcome, decision.reason), (expected, reason));
    assert_eq!(decision.duplicate_id.is_some(), reason == TriageReason::Duplicate);
    assert_eq!(
      policy
        .classification_input(input.clone(), result.clone(), &evidence, BudgetUsage::default())
        .is_ok(),
      expected == EligibilityOutcome::Classify
    );
    assert_eq!(
      policy
        .eligibility(&input, &result, &[], BudgetUsage::default())
        .unwrap()
        .outcome,
      EligibilityOutcome::Escalation
    );
    assert_eq!(
      policy
        .eligibility(
          &input,
          &result,
          &evidence,
          BudgetUsage {
            attempts: 16,
            ..BudgetUsage::default()
          }
        )
        .unwrap()
        .reason,
      TriageReason::BudgetExhausted
    );
  }
}

#[test]
fn deterministic_policy_selects_all_eight_finite_routes_and_preserves_required_gates() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  let (input, eligibility_evidence) = accepted_classification(&closure, &policy);
  for recommended in TriageRoute::ALL {
    let mut classification = classification();
    classification.recommended_route = observation(recommended);
    let result = routed_result(&closure, &input, classification);
    let mut evidence = vec![routing_evidence(&input, TriageEvidenceFact::Reproduced)];
    if recommended == TriageRoute::AlreadyFixed {
      evidence.push(routing_evidence(&input, TriageEvidenceFact::AlreadyFixed));
    }
    if recommended == TriageRoute::VerificationOnly {
      evidence.push(routing_evidence(&input, TriageEvidenceFact::VerificationOnly));
    }
    let decision = policy
      .route(
        &input,
        &result,
        &eligibility_evidence,
        &evidence,
        BudgetUsage::default(),
      )
      .unwrap();
    let expected = if recommended == TriageRoute::Development {
      TriageRoute::ProtectedTest
    } else {
      recommended
    };
    assert_eq!(decision.route, expected);
    assert_eq!(decision.policy_digest, policy.digest().unwrap());
    assert_eq!(decision.result_digest, result.digest().unwrap());
  }
  let direct = TriagePolicy::new(
    configuration(),
    &closure,
    TriagePolicySettings {
      max_risk: RiskClass::Medium,
      small_work_route: Some(TriageRoute::Development),
      budget: budget(),
    },
  )
  .unwrap();
  let (input, eligibility_evidence) = accepted_classification(&closure, &direct);
  let result = routed_result(&closure, &input, classification());
  assert_eq!(
    direct
      .route(
        &input,
        &result,
        &eligibility_evidence,
        &[routing_evidence(&input, TriageEvidenceFact::Reproduced)],
        BudgetUsage::default()
      )
      .unwrap()
      .route,
    TriageRoute::Development
  );
}

#[test]
fn model_claims_do_not_override_reproduction_risk_size_evidence_or_budget() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  let (input, eligibility_evidence) = accepted_classification(&closure, &policy);
  let reproduced = vec![routing_evidence(&input, TriageEvidenceFact::Reproduced)];
  let result = routed_result(&closure, &input, classification());
  assert_eq!(
    policy
      .route(&input, &result, &eligibility_evidence, &[], BudgetUsage::default())
      .unwrap()
      .route,
    TriageRoute::Research
  );
  assert!(
    policy
      .route(&input, &result, &[], &reproduced, BudgetUsage::default())
      .is_err()
  );
  for repro in [
    PreliminaryReproducibility::Intermittent,
    PreliminaryReproducibility::EnvironmentSpecific,
    PreliminaryReproducibility::CannotReproduce,
    PreliminaryReproducibility::Inconclusive,
    PreliminaryReproducibility::NeedsHumanInput,
  ] {
    let mut classification = classification();
    classification.reproducibility = observation(repro);
    let result = routed_result(&closure, &input, classification);
    let route = policy
      .route(
        &input,
        &result,
        &eligibility_evidence,
        &reproduced,
        BudgetUsage::default(),
      )
      .unwrap()
      .route;
    assert_eq!(
      route,
      if repro == PreliminaryReproducibility::NeedsHumanInput {
        TriageRoute::Escalation
      } else {
        TriageRoute::Research
      }
    );
  }
  for recommended in [TriageRoute::AlreadyFixed, TriageRoute::VerificationOnly] {
    let mut classification = classification();
    classification.recommended_route = observation(recommended);
    let result = routed_result(&closure, &input, classification);
    assert_eq!(
      policy
        .route(&input, &result, &eligibility_evidence, &[], BudgetUsage::default())
        .unwrap()
        .route,
      TriageRoute::Escalation
    );
  }
  let mut large = classification();
  large.size = observation(WorkSize::Large);
  assert_eq!(
    policy
      .route(
        &input,
        &routed_result(&closure, &input, large),
        &eligibility_evidence,
        &reproduced,
        BudgetUsage::default()
      )
      .unwrap()
      .route,
    TriageRoute::Requirements
  );
  let mut risky = classification();
  risky.risk = observation(RiskClass::High);
  assert_eq!(
    policy
      .route(
        &input,
        &routed_result(&closure, &input, risky),
        &eligibility_evidence,
        &reproduced,
        BudgetUsage::default()
      )
      .unwrap()
      .reason,
    TriageReason::RiskExceeded
  );
  for usage in [
    BudgetUsage {
      attempts: 16,
      ..BudgetUsage::default()
    },
    BudgetUsage {
      elapsed_millis: 1_000,
      ..BudgetUsage::default()
    },
    BudgetUsage {
      tokens: 1_000,
      ..BudgetUsage::default()
    },
    BudgetUsage {
      cost_micro_units: 1_000,
      ..BudgetUsage::default()
    },
    BudgetUsage {
      output_bytes: 1_000,
      ..BudgetUsage::default()
    },
  ] {
    assert_eq!(
      policy
        .route(&input, &result, &eligibility_evidence, &reproduced, usage)
        .unwrap()
        .reason,
      TriageReason::BudgetExhausted
    );
  }
}

#[test]
fn feature_requests_use_requirements_or_the_configured_small_work_path() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  let (input, evidence) = accepted_classification(&closure, &policy);
  for size in [WorkSize::Small, WorkSize::Medium, WorkSize::Large] {
    let mut classification = classification();
    classification.work_kind = observation(WorkKind::FeatureRequest);
    classification.reproducibility = observation(PreliminaryReproducibility::NotApplicable);
    classification.size = observation(size);
    assert_eq!(
      policy
        .route(
          &input,
          &routed_result(&closure, &input, classification),
          &evidence,
          &[],
          BudgetUsage::default()
        )
        .unwrap()
        .route,
      if size == WorkSize::Small {
        TriageRoute::ProtectedTest
      } else {
        TriageRoute::Requirements
      }
    );
  }
}

#[test]
fn evidence_and_nested_definition_substitution_fail_and_replay_is_canonical() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  let (input, eligibility_evidence) = accepted_classification(&closure, &policy);
  let result = routed_result(&closure, &input, classification());
  let evidence = vec![
    routing_evidence(&input, TriageEvidenceFact::Reproduced),
    routing_evidence(&input, TriageEvidenceFact::VerificationOnly),
  ];
  let decision = policy
    .route(
      &input,
      &result,
      &eligibility_evidence,
      &evidence,
      BudgetUsage::default(),
    )
    .unwrap();
  let reversed = evidence.into_iter().rev().collect::<Vec<_>>();
  assert_eq!(
    policy
      .route(
        &input,
        &result,
        &eligibility_evidence,
        &reversed,
        BudgetUsage::default()
      )
      .unwrap(),
    decision
  );
  let foreign = AcceptedTriageEvidence::new(
    subject(),
    digest(99),
    TriageEvidenceFact::Reproduced,
    artifact(18),
    DeterministicGateOutcome::Passed,
  )
  .unwrap();
  assert!(
    policy
      .route(
        &input,
        &result,
        &eligibility_evidence,
        &[foreign],
        BudgetUsage::default()
      )
      .is_err()
  );
  assert!(
    AcceptedTriageEvidence::new(
      subject(),
      input.digest().unwrap(),
      TriageEvidenceFact::AlreadyFixed,
      artifact(18),
      DeterministicGateOutcome::Failed
    )
    .is_err()
  );
  let replaced = self::closure(32, TriageRoute::ALL.to_vec());
  let replaced_policy = self::policy(&replaced);
  assert_ne!(policy.digest().unwrap(), replaced_policy.digest().unwrap());
  assert!(
    replaced_policy
      .route(
        &input,
        &result,
        &eligibility_evidence,
        &reversed,
        BudgetUsage::default()
      )
      .is_err()
  );
  let replay: PinnedFlowDefinitionClosure =
    serde_json::from_slice(&serde_json::to_vec(closure.closure()).unwrap()).unwrap();
  let replay = replay.validate(closure.limits().clone()).unwrap();
  assert_eq!(self::policy(&replay), policy);
}

#[test]
fn required_undeclared_route_escalates_and_invalid_policy_or_phase_is_rejected() {
  let closure = closure(
    31,
    vec![
      TriageRoute::Requirements,
      TriageRoute::Escalation,
      TriageRoute::Rejection,
    ],
  );
  let policy = policy(&closure);
  let (input, evidence) = accepted_classification(&closure, &policy);
  let result = routed_result(&closure, &input, classification());
  let decision = policy
    .route(&input, &result, &evidence, &[], BudgetUsage::default())
    .unwrap();
  assert_eq!(
    (decision.route, decision.reason),
    (TriageRoute::Escalation, TriageReason::UndeclaredRoute)
  );
  assert!(
    TriagePolicy::new(
      configuration(),
      &closure,
      TriagePolicySettings {
        max_risk: RiskClass::Critical,
        small_work_route: Some(TriageRoute::AlreadyFixed),
        budget: budget()
      }
    )
    .is_err()
  );
  let wrong = phase(
    90,
    TriageSchema::TriageResult,
    TriageSchema::EligibilityResult,
    FlowNodeKind::DeterministicGate,
  );
  let classify = phase(
    91,
    TriageSchema::ClassificationInput,
    TriageSchema::TriageResult,
    FlowNodeKind::DeterministicGate,
  );
  assert!(
    compose_triage_flow(
      FlowDefinitionId::from_uuid(Uuid::from_u128(92)).unwrap(),
      FlowDefinitionVersion::new(1).unwrap(),
      &wrong,
      &classify,
      TriageRoute::ALL.to_vec(),
      FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 1).unwrap()
    )
    .is_err()
  );
}

fn ownership() -> FactoryClaimOwnership {
  FactoryClaimOwnership::new(
    key("triage-worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(digest(50)),
      Timestamp::from_unix_millis(1).unwrap(),
      Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  )
}

fn node_attempt(flow: &FlowRun, cycle: &WorkflowCycle, root: &FlowDefinition, name: &str, number: u64) -> NodeAttempt {
  let node = root.node(&key(name)).unwrap();
  NodeAttempt::new(
    flow,
    cycle,
    root,
    NodeAttemptInput {
      id: NodeAttemptId::from_uuid(Uuid::from_u128(u128::from(number) + 100)).unwrap(),
      node_key: key(name),
      node_kind: node.kind(),
      number: NodeAttemptNumber::new(number).unwrap(),
      input_digest: digest(51),
      budget: node.budget(),
      deadline: Timestamp::from_unix_millis(99).unwrap(),
      execution: NodeExecutionIdentity::BuiltIn,
      ownership: ownership(),
    },
  )
  .unwrap()
}

fn node_completion(attempt: &NodeAttempt, root: &FlowDefinition, outcome: &str) -> NodeAttemptCompletion {
  NodeAttemptCompletion::new(
    attempt,
    root,
    NodeAttemptCompletionInput {
      outcome: key(outcome),
      output_schema: root
        .node(attempt.node_key())
        .unwrap()
        .outcome(&key(outcome))
        .unwrap()
        .schema()
        .clone(),
      output_digest: digest(52),
      ownership: ownership(),
      usage: BudgetUsage::default(),
      observed_at: Timestamp::from_unix_millis(2).unwrap(),
    },
  )
  .unwrap()
}

#[test]
fn interpreter_never_dispatches_classification_after_terminal_eligibility_and_replays_the_same_edge() {
  let original = closure(31, TriageRoute::ALL.to_vec());
  let restored: PinnedFlowDefinitionClosure =
    serde_json::from_slice(&serde_json::to_vec(original.closure()).unwrap()).unwrap();
  let restored = restored.validate(original.limits().clone()).unwrap();
  for closure in [&original, &restored] {
    let root = closure.closure().definition(closure.closure().root()).unwrap();
    let factory_run = FactoryRun::admitted(FactoryRunId::from_uuid(Uuid::from_u128(61)).unwrap(), &work());
    let flow = FlowRun::root(&factory_run, root.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let interpreter = FlowInterpreter::new(closure);
    assert!(
      matches!(interpreter.start(&flow).unwrap(), FlowDirective::EnterSubflow { node, definition, .. } if node == key("eligibility") && definition == phase_reference(closure, "eligibility"))
    );
    let entry = node_attempt(&flow, &cycle, root, "eligibility", 1);
    let observed = node_completion(&entry, root, "observed");
    let policy = node_attempt(&flow, &cycle, root, "eligibility_policy", 2);
    for fit in [ProjectFit::OutOfScope, ProjectFit::Inconclusive] {
      let input = input(vec![]);
      let result = EligibilityResult::new(
        &input,
        provenance(input.digest().unwrap(), phase_reference(closure, "eligibility")),
        observation(fit),
        vec![],
      )
      .unwrap();
      let decision = self::policy(closure)
        .eligibility(
          &input,
          &result,
          &accepted_evidence(&input, &result),
          BudgetUsage::default(),
        )
        .unwrap();
      let terminal = decision.outcome.as_str();
      let completed = node_completion(&policy, root, terminal);
      let directives = interpreter
        .advance(
          &flow,
          &cycle,
          &policy,
          &completed,
          &[entry.clone(), policy.clone()],
          &[observed.clone(), completed.clone()],
        )
        .unwrap();
      assert_eq!(
        directives,
        vec![FlowDirective::Complete {
          terminal: key(terminal),
          schema_digest: TriageSchema::Decision.reference().unwrap().digest()
        }]
      );
      assert_eq!(
        serde_json::to_value(TriageDisposition::Eligibility(decision)).unwrap()["phase"],
        "eligibility"
      );
    }
    let completed = node_completion(&policy, root, "classify");
    let directives = interpreter
      .advance(
        &flow,
        &cycle,
        &policy,
        &completed,
        &[entry.clone(), policy.clone()],
        &[observed, completed.clone()],
      )
      .unwrap();
    assert!(
      matches!(&directives[..], [FlowDirective::EnterSubflow { node, definition, inputs }]
      if node == &key("classification") && *definition == phase_reference(closure, "classification") && inputs.context.is_empty() && inputs.data.len() == 1 && inputs.data[0].schema() == &TriageSchema::ClassificationInput.reference().unwrap())
    );
    let classify = node_attempt(&flow, &cycle, root, "classification", 3);
    let classified = node_completion(&classify, root, "observed");
    let route = node_attempt(&flow, &cycle, root, "routing_policy", 4);
    for selected in TriageRoute::ALL {
      let completed = node_completion(&route, root, selected.as_str());
      assert_eq!(
        interpreter
          .advance(
            &flow,
            &cycle,
            &route,
            &completed,
            &[classify.clone(), route.clone()],
            &[classified.clone(), completed.clone()]
          )
          .unwrap(),
        vec![FlowDirective::Complete {
          terminal: key(selected.as_str()),
          schema_digest: TriageSchema::Decision.reference().unwrap().digest()
        }]
      );
    }
  }
}

#[test]
fn admitted_risk_cannot_be_lowered_and_small_work_bypass_can_be_disabled() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  let mut wire = serde_json::to_value(input(vec![])).unwrap();
  wire["admission_risk"] = "high".into();
  let input: EligibilityInput = serde_json::from_value(wire).unwrap();
  let result = eligible(&input, phase_reference(&closure, "eligibility"));
  let evidence = accepted_evidence(&input, &result);
  let input = policy
    .classification_input(input, result, &evidence, BudgetUsage::default())
    .unwrap();
  let result = routed_result(&closure, &input, classification());
  assert_eq!(
    policy
      .route(
        &input,
        &result,
        &evidence,
        &[routing_evidence(&input, TriageEvidenceFact::Reproduced)],
        BudgetUsage::default()
      )
      .unwrap()
      .reason,
    TriageReason::RiskExceeded
  );

  let policy = TriagePolicy::new(
    configuration(),
    &closure,
    TriagePolicySettings {
      max_risk: RiskClass::Medium,
      small_work_route: None,
      budget: budget(),
    },
  )
  .unwrap();
  let (input, evidence) = accepted_classification(&closure, &policy);
  let result = routed_result(&closure, &input, classification());
  assert_eq!(
    policy
      .route(
        &input,
        &result,
        &evidence,
        &[routing_evidence(&input, TriageEvidenceFact::Reproduced)],
        BudgetUsage::default()
      )
      .unwrap()
      .route,
    TriageRoute::Requirements
  );
}

#[test]
fn altered_configuration_foreign_evidence_and_unaccepted_result_inputs_fail_closed() {
  let closure = closure(31, TriageRoute::ALL.to_vec());
  let policy = policy(&closure);
  let (input, evidence) = accepted_classification(&closure, &policy);
  let result = routed_result(&closure, &input, classification());
  let mut wire = serde_json::to_value(&result).unwrap();
  wire["provenance"]["input_digest"] = serde_json::to_value(digest(99)).unwrap();
  let altered: TriageResult = serde_json::from_value(wire).unwrap();
  assert!(
    policy
      .route(&input, &altered, &evidence, &[], BudgetUsage::default())
      .is_err()
  );
  let mut replaced = configuration();
  let mut wire = serde_json::to_value(&replaced).unwrap();
  wire["version"] = 2.into();
  replaced = serde_json::from_value(wire).unwrap();
  let replaced = TriagePolicy::new(
    replaced,
    &closure,
    TriagePolicySettings {
      max_risk: RiskClass::Medium,
      small_work_route: Some(TriageRoute::ProtectedTest),
      budget: budget(),
    },
  )
  .unwrap();
  assert!(
    replaced
      .route(&input, &result, &evidence, &[], BudgetUsage::default())
      .is_err()
  );
  let foreign = ExactSubject::new(
    ProjectId::from_uuid(Uuid::from_u128(999)).unwrap(),
    subject().repository_id(),
    subject().base_revision().clone(),
  );
  let proof = AcceptedTriageEvidence::new(
    foreign,
    input.digest().unwrap(),
    TriageEvidenceFact::Reproduced,
    artifact(18),
    DeterministicGateOutcome::Passed,
  )
  .unwrap();
  assert!(
    policy
      .route(&input, &result, &evidence, &[proof], BudgetUsage::default())
      .is_err()
  );
}

#[test]
fn triage_is_a_replaceable_nested_flow_inside_a_larger_root() {
  let triage = closure(31, TriageRoute::ALL.to_vec());
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(Uuid::from_u128(100)).unwrap(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: Some(TriageSchema::EligibilityInput.reference().unwrap()),
    entry: key("triage"),
    nodes: vec![
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key("triage"),
        kind: FlowNodeKind::SubflowCall,
        input_schema: Some(TriageSchema::EligibilityInput.reference().unwrap()),
        outcomes: TriageRoute::ALL
          .iter()
          .map(|route| {
            FlowOutcomeDefinition::new(
              key(route.as_str()),
              FlowOutcomeKind::Success,
              TriageSchema::Decision.reference().unwrap(),
            )
          })
          .collect(),
        budget: budget(),
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: Some(triage.closure().root()),
      })
      .unwrap(),
    ],
    transitions: TriageRoute::ALL
      .iter()
      .map(|route| {
        FlowTransition::new(
          key("triage"),
          key(route.as_str()),
          FlowTransitionTarget::Terminal(key(route.as_str())),
        )
      })
      .collect(),
    terminals: TriageRoute::ALL
      .iter()
      .map(|route| FlowTerminalDefinition::new(key(route.as_str()), TriageSchema::Decision.reference().unwrap()))
      .collect(),
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let mut definitions = triage.closure().definitions().to_vec();
  definitions.push(root.clone());
  let enclosing = PinnedFlowDefinitionClosure::new(root.reference(), definitions)
    .unwrap()
    .validate(triage.limits().clone())
    .unwrap();
  let bound = TriagePolicy::for_flow(
    configuration(),
    &enclosing,
    triage.closure().root(),
    TriagePolicySettings {
      max_risk: RiskClass::Medium,
      small_work_route: Some(TriageRoute::ProtectedTest),
      budget: budget(),
    },
  )
  .unwrap();
  assert_eq!(bound, policy(&triage));
  assert_eq!(enclosing.expanded_nodes(), 7);
  assert_eq!(enclosing.max_depth(), 3);
  let (input, evidence) = accepted_classification(&triage, &bound);
  let result = routed_result(&triage, &input, classification());
  assert_eq!(
    bound
      .route(&input, &result, &evidence, &[], BudgetUsage::default())
      .unwrap()
      .route,
    TriageRoute::Research
  );
}
