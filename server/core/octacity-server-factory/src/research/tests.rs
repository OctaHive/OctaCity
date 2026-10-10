use crate as factory;
use crate::*;
mod fixtures;
use fixtures::*;
use octacity_server_domain::{AttemptId, BuildConfigurationId, BuildConfigurationVersion, BuildId, JobId};

#[test]
fn research_execution_provenance_uses_a_new_schema_contract_version() {
  assert_eq!(RESEARCH_CONTRACT_VERSION, 2);
  for schema in [
    ResearchSchema::Input,
    ResearchSchema::DefectResult,
    ResearchSchema::FeatureResult,
    ResearchSchema::Handoff,
    ResearchSchema::Decision,
  ] {
    assert_eq!(schema.reference().unwrap().version().as_str(), "v2");
  }
  let (work, admitted, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let mut wire = serde_json::to_value(&input).unwrap();
  assert_eq!(wire["schema_version"], serde_json::json!(2));
  wire["schema_version"] = serde_json::json!(1);
  assert!(
    ResearchInput::restore(
      &serde_json::to_vec(&wire).unwrap(),
      &work,
      &admitted,
      input.policy_digest()
    )
    .is_err()
  );
}

fn input_fixture(kind: WorkKind, size: WorkSize) -> (WorkEnvelope, AdmittedFlow, ResearchInput) {
  let (work, admitted, accepted) = crate::triage::tests::research_fixture(kind, size);
  let input = fixtures::input(&work, &admitted, accepted, kind);
  (work, admitted, input)
}

#[test]
fn reproduction_report_derives_reproduced_from_checks_of_the_frozen_environment() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let ResearchDetails::Defect { environment, .. } = input.details() else {
    unreachable!()
  };
  let report = DefectReproductionReport::new(
    input.subject().clone(),
    input.digest().unwrap(),
    environment.content_digest(),
    vec![DefectReproductionCheck {
      environment_digest: environment.content_digest(),
      observation: DefectReproductionObservation::Reproduced,
    }],
  )
  .unwrap();
  assert_eq!(report.classify(&input).unwrap(), DefectResearchOutcome::Reproduced);
}

#[test]
fn reproduction_report_keeps_intermittent_environment_specific_and_missing_input_distinct() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let ResearchDetails::Defect { environment, .. } = input.details() else {
    unreachable!()
  };
  let baseline = environment.content_digest();
  use DefectReproductionObservation::{MissingInput, NotReproduced, Reproduced};
  for (checks, expected) in [
    (
      vec![(baseline, Reproduced), (baseline, NotReproduced)],
      DefectResearchOutcome::Intermittent,
    ),
    (
      vec![(baseline, NotReproduced), (digest(99), Reproduced)],
      DefectResearchOutcome::EnvironmentSpecific,
    ),
    (vec![(baseline, NotReproduced)], DefectResearchOutcome::CannotReproduce),
    (vec![(baseline, MissingInput)], DefectResearchOutcome::NeedsHumanInput),
  ] {
    let report = DefectReproductionReport::new(
      input.subject().clone(),
      input.digest().unwrap(),
      baseline,
      checks
        .into_iter()
        .map(|(environment_digest, observation)| DefectReproductionCheck {
          environment_digest,
          observation,
        })
        .collect(),
    )
    .unwrap();
    assert_eq!(report.classify(&input).unwrap(), expected);
  }
}

#[test]
fn frozen_research_inputs_restore_only_with_the_exact_accepted_work_and_policy() {
  for kind in [WorkKind::Defect, WorkKind::FeatureRequest] {
    let (work, admitted, input) = input_fixture(kind, WorkSize::Small);
    let bytes = serde_json::to_vec(&input).unwrap();
    assert!(ResearchInput::restore(&bytes, &work, &admitted, digest(99)).is_err());
    assert_eq!(
      ResearchInput::restore(&bytes, &work, &admitted, input.policy_digest()).unwrap(),
      input
    );
    let mut wire = serde_json::to_value(&input).unwrap();
    wire["accepted_triage"]["decision"]["route"] = serde_json::json!("development");
    assert!(
      ResearchInput::restore(
        &serde_json::to_vec(&wire).unwrap(),
        &work,
        &admitted,
        input.policy_digest()
      )
      .is_err()
    );
    let mut wire = serde_json::to_value(&input).unwrap();
    wire["schema_version"] = serde_json::json!(0);
    assert!(
      ResearchInput::restore(
        &serde_json::to_vec(&wire).unwrap(),
        &work,
        &admitted,
        input.policy_digest()
      )
      .is_err()
    );
  }
}

#[test]
fn missing_context_changed_environment_and_unfrozen_sources_fail_input_construction() {
  let (work, admitted, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let mut details = input.details().clone();
  if let ResearchDetails::Defect { environment, .. } = &mut details {
    *environment = artifact(99);
  }
  assert!(
    ResearchInput::new(
      &work,
      &admitted,
      input.accepted_triage().clone(),
      input.context().clone(),
      vec![],
      details,
      input.policy_digest(),
    )
    .is_err()
  );
  let (work, admitted, input) = input_fixture(WorkKind::FeatureRequest, WorkSize::Small);
  let mut details = input.details().clone();
  if let ResearchDetails::Feature { sources, .. } = &mut details {
    sources[0].content_digest = digest(99);
  }
  assert!(
    ResearchInput::new(
      &work,
      &admitted,
      input.accepted_triage().clone(),
      input.context().clone(),
      vec![],
      details,
      input.policy_digest(),
    )
    .is_err()
  );
  assert!(serde_json::from_str::<ResearchRoute>("\"development\"").is_err());
  assert!(serde_json::from_str::<ResearchRoute>("\"implementation_ready\"").is_err());
}

fn producer(tool: &str) -> EvidenceProducer {
  EvidenceProducer::new(
    BuildId::generate(),
    AttemptId::generate(),
    JobId::generate(),
    reference(tool),
    reference("runner"),
  )
}

fn policy(input: &ResearchInput) -> (ResearchPolicy, ValidatedFlowDefinitionClosure) {
  let closure = research_closure(input.kind(), &ResearchRoute::ALL);
  (
    ResearchPolicy::new(
      input.configuration().clone(),
      &closure,
      input.kind(),
      settings(input.kind()),
      profiles(&closure, input.subject().project_id()),
    )
    .unwrap(),
    closure,
  )
}

fn select_policy(input: &ResearchInput, policy: &ResearchPolicy) -> ResearchInput {
  let size = *input.accepted_triage().result.classification().size.value();
  let (work, admitted, _) = input_fixture(input.kind(), size);
  ResearchInput::new(
    &work,
    &admitted,
    input.accepted_triage().clone(),
    input.context().clone(),
    input.retrieval().to_vec(),
    input.details().clone(),
    policy.digest().unwrap(),
  )
  .unwrap()
}
fn result(
  input: &ResearchInput,
  closure: &ValidatedFlowDefinitionClosure,
  outcome: DefectResearchOutcome,
) -> ResearchResult {
  let provenance = ResearchProvenance {
    input_digest: input.digest().unwrap(),
    context_digest: input.context().digest().unwrap(),
    definition: closure.closure().root(),
    flow_run_id: FlowRunId::generate(),
    cycle_id: WorkflowCycleId::generate(),
    node: key("research"),
    node_attempt_id: NodeAttemptId::generate(),
    attempt: NodeAttemptNumber::new(1).unwrap(),
    execution_attempt: 1,
    producer: producer("research-tool"),
    model_or_tool: reference("research-model"),
    task_digest: digest(70),
    result: artifact(71),
    observed_at: time(20),
  };
  let summary = BoundedSummary::new(
    FactoryTaskSubject::Exact(input.subject().clone()),
    FactorySafeText::new("Bounded research observation").unwrap(),
    digest(72),
  );
  let observations = if input.kind() == WorkKind::Defect {
    ResearchObservations::Defect(Box::new(DefectResearchResult {
      outcome,
      reproduction_report: artifact(73),
      summary,
      unresolved_items: if outcome == DefectResearchOutcome::NeedsHumanInput {
        vec![FactorySafeText::new("Confirm the declared environment").unwrap()]
      } else {
        vec![]
      },
    }))
  } else {
    let ResearchDetails::Feature { sources, .. } = input.details() else {
      unreachable!()
    };
    ResearchObservations::Feature(Box::new(FeatureResearchResult {
      proposal: artifact(74),
      sources: sources.clone(),
      summary,
      assumptions: vec![FactorySafeText::new("The frozen API remains supported").unwrap()],
      alternatives: vec![FactorySafeText::new("Preserve current behavior").unwrap()],
      unresolved_questions: vec![FactorySafeText::new("Confirm acceptance scope").unwrap()],
    }))
  };
  ResearchResult::new(input, provenance, observations).unwrap()
}
fn evidence_record(
  input: &ResearchInput,
  result: &ResearchResult,
  fact: ResearchEvidenceFact,
) -> ResearchEvidenceRecord {
  ResearchEvidenceRecord {
    subject: input.subject().clone(),
    input_digest: input.digest().unwrap(),
    environment_digest: match input.details() {
      ResearchDetails::Defect { environment, .. } => Some(environment.content_digest()),
      _ => None,
    },
    fact,
    artifact: result.evidence_artifact().clone(),
    schema: reference("research-report"),
    output_kind: EvidenceOutputKind::Report,
    producer: producer("validator"),
    verified_at: time(21),
    fresh_until: time(100),
  }
}
fn proof(input: &ResearchInput, result: &ResearchResult, fact: ResearchEvidenceFact) -> AcceptedResearchEvidence {
  AcceptedResearchEvidence::new(
    input,
    result,
    evidence_record(input, result, fact),
    DeterministicGateOutcome::Passed,
  )
  .unwrap()
}
fn primary(input: &ResearchInput, result: &ResearchResult) -> AcceptedResearchEvidence {
  proof(
    input,
    result,
    match result.outcome() {
      ResearchOutcome::Defect(outcome) => ResearchEvidenceFact::Reproduction(outcome),
      _ => ResearchEvidenceFact::Proposal,
    },
  )
}
fn usage() -> BudgetUsage {
  BudgetUsage {
    attempts: 1,
    elapsed_millis: 10,
    tokens: 10,
    cost_micro_units: 10,
    output_bytes: 100,
  }
}

#[test]
fn defect_outcomes_remain_distinct_and_handoffs_grant_no_implementation_readiness() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (policy, closure) = policy(&input);
  for (outcome, route) in [
    (DefectResearchOutcome::Reproduced, ResearchRoute::ProtectedTest),
    (DefectResearchOutcome::Intermittent, ResearchRoute::Requirements),
    (DefectResearchOutcome::EnvironmentSpecific, ResearchRoute::Escalation),
    (DefectResearchOutcome::CannotReproduce, ResearchRoute::Rejection),
    (DefectResearchOutcome::NeedsHumanInput, ResearchRoute::Escalation),
  ] {
    let result = result(&input, &closure, outcome);
    let accepted = primary(&input, &result);
    let id = StageHandoffId::generate();
    let handoff = policy
      .accept(id, &input, &result, std::slice::from_ref(&accepted), usage(), time(22))
      .unwrap();
    assert_eq!(handoff.decision.route, route);
    assert_eq!(handoff.handoff.result().outcome(), ResearchOutcome::Defect(outcome));
    assert_eq!(handoff.handoff.subject(), input.subject());
    assert_eq!(handoff.handoff.context_digest(), input.context().digest().unwrap());
    assert_eq!(
      handoff,
      policy
        .accept(id, &input, &result, &[accepted], usage(), time(22))
        .unwrap()
    );
    let wire = serde_json::to_value(&handoff.handoff).unwrap();
    for forbidden in ["implementation_ready", "permissions", "route", "candidate", "changeset"] {
      assert!(wire.get(forbidden).is_none());
    }
    let restored: ResearchResult = serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(restored, result);
    restored.validate_input(&input).unwrap();
  }
}

#[test]
fn missing_failing_stale_or_substituted_evidence_cannot_authorize_a_successor() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (policy, closure) = policy(&input);
  let result = result(&input, &closure, DefectResearchOutcome::Reproduced);
  assert_eq!(
    policy.evaluate(&input, &result, &[], usage(), time(22)).unwrap().route,
    ResearchRoute::Escalation
  );
  let record = evidence_record(
    &input,
    &result,
    ResearchEvidenceFact::Reproduction(DefectResearchOutcome::Reproduced),
  );
  assert!(AcceptedResearchEvidence::new(&input, &result, record.clone(), DeterministicGateOutcome::Failed).is_err());
  let mut changed = record.clone();
  changed.environment_digest = Some(digest(99));
  assert!(AcceptedResearchEvidence::new(&input, &result, changed, DeterministicGateOutcome::Passed).is_err());
  let mut changed = record.clone();
  changed.fact = ResearchEvidenceFact::Reproduction(DefectResearchOutcome::CannotReproduce);
  assert!(AcceptedResearchEvidence::new(&input, &result, changed, DeterministicGateOutcome::Passed).is_err());
  let accepted = primary(&input, &result);
  assert_eq!(
    policy
      .evaluate(&input, &result, std::slice::from_ref(&accepted), usage(), time(100))
      .unwrap()
      .route,
    ResearchRoute::Escalation
  );
  assert!(
    policy
      .evaluate(&input, &result, &[accepted.clone(), accepted], usage(), time(22))
      .is_err()
  );
  let mut changed = record;
  changed.producer = producer("unapproved-validator");
  let accepted = AcceptedResearchEvidence::new(&input, &result, changed, DeterministicGateOutcome::Passed).unwrap();
  assert!(
    policy
      .evaluate(&input, &result, &[accepted], usage(), time(22))
      .is_err()
  );
}

#[test]
fn attempts_reserve_every_budget_category_and_reject_broader_authority() {
  let (work, admitted, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (policy, closure) = policy(&input);
  assert!(matches!(
    policy
      .next_attempt(&input, BudgetUsage::default(), &FactoryPermissionSet::deny_all())
      .unwrap(),
    ResearchAttemptDisposition::Dispatch { attempt: 1, .. }
  ));
  for usage in [
    BudgetUsage {
      attempts: 3,
      ..Default::default()
    },
    BudgetUsage {
      elapsed_millis: 2001,
      ..Default::default()
    },
    BudgetUsage {
      tokens: 201,
      ..Default::default()
    },
    BudgetUsage {
      cost_micro_units: 2001,
      ..Default::default()
    },
    BudgetUsage {
      output_bytes: 2001,
      ..Default::default()
    },
  ] {
    assert_eq!(
      policy
        .next_attempt(&input, usage, &FactoryPermissionSet::deny_all())
        .unwrap(),
      ResearchAttemptDisposition::Exhausted(ResearchRoute::Escalation)
    );
  }
  assert!(
    policy
      .next_attempt(
        &input,
        BudgetUsage {
          tokens: 301,
          ..Default::default()
        },
        &FactoryPermissionSet::deny_all()
      )
      .is_err()
  );
  let mut wire = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
  wire["network_hosts"] = serde_json::json!(["example.com"]);
  let broader: FactoryPermissionSet = serde_json::from_value(wire).unwrap();
  assert!(policy.next_attempt(&input, BudgetUsage::default(), &broader).is_err());
  let enlarged = closure
    .closure()
    .clone()
    .validate(FlowAdmissionLimits::product_defaults(1, research_budget(), broader.clone()).unwrap())
    .unwrap();
  let mut settings = policy.settings().clone();
  settings.permissions = broader.clone();
  let enlarged_policy = ResearchPolicy::new(
    input.configuration().clone(),
    &enlarged,
    input.kind(),
    settings,
    profiles(&enlarged, input.subject().project_id()),
  )
  .unwrap();
  let enlarged_input = select_policy(&input, &enlarged_policy);
  assert!(
    enlarged_policy
      .next_attempt(&enlarged_input, BudgetUsage::default(), &broader)
      .is_err()
  );
  let mut wire = serde_json::to_value(&admitted).unwrap();
  wire["limits"] = serde_json::to_value(
    FlowAdmissionLimits::product_defaults(
      admitted.limits().max_wip(),
      BudgetLimit::new(16, 1000, 1000, 1000, 1000).unwrap(),
      FactoryPermissionSet::deny_all(),
    )
    .unwrap(),
  )
  .unwrap();
  let narrower: AdmittedFlow = serde_json::from_value(wire).unwrap();
  let bounded_input = ResearchInput::new(
    &work,
    &narrower,
    input.accepted_triage().clone(),
    input.context().clone(),
    vec![],
    input.details().clone(),
    input.policy_digest(),
  )
  .unwrap();
  assert!(
    policy
      .next_attempt(
        &bounded_input,
        BudgetUsage::default(),
        &FactoryPermissionSet::deny_all()
      )
      .is_err()
  );
}

#[test]
fn large_work_needs_requirements_and_exhaustion_never_silently_continues() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Large);
  let (policy, closure) = policy(&input);
  let result = result(&input, &closure, DefectResearchOutcome::Reproduced);
  let accepted = primary(&input, &result);
  let decision = policy
    .evaluate(&input, &result, &[accepted], usage(), time(22))
    .unwrap();
  assert_eq!(decision.route, ResearchRoute::Requirements);
  assert_eq!(decision.reason, ResearchReason::RequirementsRequired);
  let result = self::result(&input, &closure, DefectResearchOutcome::CannotReproduce);
  let accepted = primary(&input, &result);
  let decision = policy
    .evaluate(
      &input,
      &result,
      &[accepted],
      BudgetUsage { attempts: 3, ..usage() },
      time(22),
    )
    .unwrap();
  assert_eq!(decision.route, ResearchRoute::Escalation);
  assert_eq!(decision.reason, ResearchReason::AttemptsExhausted);
  let routes = ResearchRoute::ALL
    .into_iter()
    .filter(|route| route != &ResearchRoute::Requirements)
    .collect::<Vec<_>>();
  let narrow = research_closure(input.kind(), &routes);
  let mut settings = settings(input.kind());
  for route in settings.outcomes.values_mut() {
    if *route == ResearchRoute::Requirements {
      *route = ResearchRoute::ProtectedTest;
    }
  }
  let missing_requirements = ResearchPolicy::new(
    input.configuration().clone(),
    &narrow,
    input.kind(),
    settings,
    profiles(&narrow, input.subject().project_id()),
  )
  .unwrap();
  let selected_input = select_policy(&input, &missing_requirements);
  let result = self::result(&selected_input, &narrow, DefectResearchOutcome::Reproduced);
  let decision = missing_requirements
    .evaluate(
      &selected_input,
      &result,
      &[primary(&selected_input, &result)],
      usage(),
      time(22),
    )
    .unwrap();
  assert_eq!(decision.route, ResearchRoute::Escalation);
  assert_eq!(decision.reason, ResearchReason::UndeclaredSuccessor);
}

#[test]
fn feature_proposals_preserve_sources_bounds_and_independent_resolution_evidence() {
  let (_, _, input) = input_fixture(WorkKind::FeatureRequest, WorkSize::Small);
  let (policy, closure) = policy(&input);
  let result = result(&input, &closure, DefectResearchOutcome::Reproduced);
  let accepted = primary(&input, &result);
  assert_eq!(
    policy
      .evaluate(&input, &result, std::slice::from_ref(&accepted), usage(), time(22))
      .unwrap()
      .route,
    ResearchRoute::Requirements
  );
  let mut settings = policy.settings().clone();
  settings
    .outcomes
    .insert(ResearchOutcome::FeatureProposal, ResearchRoute::TerminalResolution);
  let terminal = ResearchPolicy::new(
    input.configuration().clone(),
    &closure,
    input.kind(),
    settings,
    profiles(&closure, input.subject().project_id()),
  )
  .unwrap();
  let terminal_input = select_policy(&input, &terminal);
  let terminal_result = self::result(&terminal_input, &closure, DefectResearchOutcome::Reproduced);
  let terminal_accepted = primary(&terminal_input, &terminal_result);
  assert_eq!(
    terminal
      .evaluate(
        &terminal_input,
        &terminal_result,
        std::slice::from_ref(&terminal_accepted),
        usage(),
        time(22)
      )
      .unwrap()
      .route,
    ResearchRoute::Escalation
  );
  let terminal_fact = proof(
    &terminal_input,
    &terminal_result,
    ResearchEvidenceFact::TerminalResolution,
  );
  assert_eq!(
    terminal
      .evaluate(
        &terminal_input,
        &terminal_result,
        &[terminal_accepted, terminal_fact],
        usage(),
        time(22)
      )
      .unwrap()
      .route,
    ResearchRoute::TerminalResolution
  );
  let mut settings = policy.settings().clone();
  settings
    .outcomes
    .insert(ResearchOutcome::FeatureProposal, ResearchRoute::Verification);
  let verification = ResearchPolicy::new(
    input.configuration().clone(),
    &closure,
    input.kind(),
    settings,
    profiles(&closure, input.subject().project_id()),
  )
  .unwrap();
  let verification_input = select_policy(&input, &verification);
  let verification_result = self::result(&verification_input, &closure, DefectResearchOutcome::Reproduced);
  let verification_accepted = primary(&verification_input, &verification_result);
  assert_eq!(
    verification
      .evaluate(
        &verification_input,
        &verification_result,
        std::slice::from_ref(&verification_accepted),
        usage(),
        time(22)
      )
      .unwrap()
      .route,
    ResearchRoute::Escalation
  );
  assert_eq!(
    verification
      .evaluate(
        &verification_input,
        &verification_result,
        &[
          verification_accepted,
          proof(
            &verification_input,
            &verification_result,
            ResearchEvidenceFact::Verification
          )
        ],
        usage(),
        time(22)
      )
      .unwrap()
      .route,
    ResearchRoute::Verification
  );
  let mut wire = serde_json::to_value(&result).unwrap();
  wire["observations"]["result"]["sources"] = serde_json::json!([]);
  assert!(serde_json::from_value::<ResearchResult>(wire).is_err());
  let mut wire = serde_json::to_value(&result).unwrap();
  wire["observations"]["result"]["proposal"]["encoded_size"] = serde_json::json!(MAX_RESEARCH_PROPOSAL_BYTES + 1);
  assert!(serde_json::from_value::<ResearchResult>(wire).is_err());
  let mut settings = policy.settings().clone();
  settings.max_proposal_bytes = 1;
  let bounded = ResearchPolicy::new(
    input.configuration().clone(),
    &closure,
    input.kind(),
    settings,
    profiles(&closure, input.subject().project_id()),
  )
  .unwrap();
  let bounded_input = select_policy(&input, &bounded);
  let bounded_result = self::result(&bounded_input, &closure, DefectResearchOutcome::Reproduced);
  assert!(
    bounded
      .evaluate(
        &bounded_input,
        &bounded_result,
        &[primary(&bounded_input, &bounded_result)],
        usage(),
        time(22)
      )
      .is_err()
  );
}

#[test]
fn provider_authority_fields_model_substitution_and_undeclared_policy_routes_are_rejected() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (policy, closure) = policy(&input);
  let result = result(&input, &closure, DefectResearchOutcome::Reproduced);
  for forbidden in ["route", "implementation_ready", "permissions"] {
    let mut wire = serde_json::to_value(&result).unwrap();
    wire[forbidden] = serde_json::json!(true);
    assert!(serde_json::from_value::<ResearchResult>(wire).is_err());
  }
  let mut wire = serde_json::to_value(&result).unwrap();
  wire["provenance"]["model_or_tool"] = serde_json::to_value(reference("substituted-model")).unwrap();
  let substituted: ResearchResult = serde_json::from_value(wire).unwrap();
  assert!(
    policy
      .evaluate(
        &input,
        &substituted,
        &[primary(&input, &substituted)],
        usage(),
        time(22)
      )
      .is_err()
  );
  let narrow = research_closure(input.kind(), &[ResearchRoute::Escalation]);
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &narrow,
      input.kind(),
      settings(input.kind()),
      profiles(&narrow, input.subject().project_id())
    )
    .is_err()
  );
  let mut invalid = settings(input.kind());
  invalid
    .outcomes
    .remove(&ResearchOutcome::Defect(DefectResearchOutcome::Intermittent));
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &closure,
      input.kind(),
      invalid,
      profiles(&closure, input.subject().project_id())
    )
    .is_err()
  );
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &closure,
      input.kind(),
      settings(input.kind()),
      vec![]
    )
    .is_err()
  );
  let mut invalid = settings(input.kind());
  invalid.outcomes.insert(
    ResearchOutcome::Defect(DefectResearchOutcome::NeedsHumanInput),
    ResearchRoute::ProtectedTest,
  );
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &closure,
      input.kind(),
      invalid,
      profiles(&closure, input.subject().project_id())
    )
    .is_err()
  );
  let original = policy.digest().unwrap();
  let mut replacement = policy.settings().clone();
  replacement.small_work_max_risk = RiskClass::Medium;
  let replacement = ResearchPolicy::new(
    input.configuration().clone(),
    &closure,
    input.kind(),
    replacement,
    profiles(&closure, input.subject().project_id()),
  )
  .unwrap();
  assert_ne!(replacement.digest().unwrap(), original);
  assert!(
    replacement
      .evaluate(&input, &result, &[primary(&input, &result)], usage(), time(22))
      .is_err()
  );
  assert_eq!(policy.digest().unwrap(), original);
}

#[test]
fn repository_discovery_requires_the_exact_frozen_retrieval_receipt() {
  let (work, admitted, input) = input_fixture(WorkKind::FeatureRequest, WorkSize::Small);
  let subject = FactoryTaskSubject::Exact(input.subject().clone());
  let fragment = artifact(90);
  let range = FactoryRepositoryRange::new(
    input.subject().repository_id(),
    input.subject().base_revision().clone(),
    FactoryRepositoryPath::new("src/lib.rs").unwrap(),
    1,
    2,
  )
  .unwrap();
  let receipt = RetrievalReceipt::new(
    RetrievalReceiptId::generate(),
    subject.clone(),
    input.subject().base_revision().clone(),
    reference("index"),
    reference("embedding"),
    FactorySafeText::new("declared API contract").unwrap(),
    reference("retrieval-policy"),
    vec![RepositoryFragment::new(1, range, fragment.clone()).unwrap()],
    digest(91),
  )
  .unwrap();
  let mut entries = input.context().entries().to_vec();
  entries.push(
    ContextManifestEntry::new(
      ContextSourceKind::RepositoryFragment,
      key("repository.source"),
      subject.clone(),
      FactoryContextReference::repository_fragment(receipt.fragment_reference(1).unwrap()),
      fragment.content_digest(),
      fragment.encoded_size(),
      FactorySafeText::new("Declared source evidence").unwrap(),
      digest(92),
    )
    .unwrap(),
  );
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject,
    input.context().construction_policy_digest(),
    entries,
  )
  .unwrap();
  let mut details = input.details().clone();
  if let ResearchDetails::Feature { sources, .. } = &mut details {
    sources.push(ResearchSourceReference {
      source_kind: ContextSourceKind::RepositoryFragment,
      logical_identity: key("repository.source"),
      content_digest: fragment.content_digest(),
    });
  }
  assert!(
    ResearchInput::new(
      &work,
      &admitted,
      input.accepted_triage().clone(),
      context.clone(),
      vec![],
      details.clone(),
      input.policy_digest(),
    )
    .is_err()
  );
  let frozen = ResearchInput::new(
    &work,
    &admitted,
    input.accepted_triage().clone(),
    context,
    vec![receipt],
    details,
    input.policy_digest(),
  )
  .unwrap();
  assert_eq!(
    ResearchInput::restore(
      &serde_json::to_vec(&frozen).unwrap(),
      &work,
      &admitted,
      input.policy_digest()
    )
    .unwrap(),
    frozen
  );
  let mut wire = serde_json::to_value(&frozen).unwrap();
  wire["retrieval"][0]["embedding"] = serde_json::to_value(reference("changed-embedding")).unwrap();
  assert!(
    ResearchInput::restore(
      &serde_json::to_vec(&wire).unwrap(),
      &work,
      &admitted,
      input.policy_digest()
    )
    .is_err()
  );
  let (_, closure) = policy(&input);
  let old_result = result(&input, &closure, DefectResearchOutcome::Reproduced);
  assert!(old_result.validate_input(&frozen).is_err());
}

#[test]
fn reasoning_cannot_export_policy_successors_and_node_budgets_cannot_bypass_reservations() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (_, closure) = policy(&input);
  let root = closure.closure().definition(closure.closure().root()).unwrap();
  let altered = |field: &str, value: serde_json::Value| {
    let mut wire = serde_json::to_value(root).unwrap();
    let nodes = wire["nodes"].as_array_mut().unwrap();
    let node = nodes
      .iter_mut()
      .find(|node| node["key"] == if field == "kind" { "research_policy" } else { "research" })
      .unwrap();
    node[field] = value;
    let changed = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes: serde_json::from_value(wire["nodes"].clone()).unwrap(),
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    })
    .unwrap();
    PinnedFlowDefinitionClosure::new(changed.reference(), vec![changed])
      .unwrap()
      .validate(closure.limits().clone())
      .unwrap()
  };
  let changed = altered("kind", serde_json::json!("reasoning"));
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &changed,
      input.kind(),
      settings(input.kind()),
      profiles(&changed, input.subject().project_id())
    )
    .is_err()
  );
  let changed = altered("budget", serde_json::to_value(research_budget()).unwrap());
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &changed,
      input.kind(),
      settings(input.kind()),
      profiles(&changed, input.subject().project_id())
    )
    .is_err()
  );
  let mut invalid = settings(input.kind());
  invalid.exhausted_route = ResearchRoute::ProtectedTest;
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &closure,
      input.kind(),
      invalid,
      profiles(&closure, input.subject().project_id())
    )
    .is_err()
  );
}

#[test]
fn defect_build_intent_freezes_the_input_profile_budget_and_stable_operation() {
  let (work, admitted, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (policy, closure) = policy(&input);
  let run = FactoryRun::admitted(admitted.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, closure.closure().root()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let ownership = FactoryClaimOwnership::new(
    key("research.worker"),
    FactoryClaim::new(FactoryClaimFence::new(digest(90)), time(10), time(100)).unwrap(),
  );
  let node = NodeAttempt::new(
    &flow,
    &cycle,
    closure.closure().definition(flow.definition()).unwrap(),
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("research"),
      node_kind: FlowNodeKind::Reasoning,
      number: NodeAttemptNumber::new(1).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: attempt_budget(),
      deadline: time(99),
      execution: NodeExecutionIdentity::External(reference("research-tool")),
      ownership,
    },
  )
  .unwrap();
  let intent = ResearchBuildIntent::new(
    input.clone(),
    &policy,
    flow.clone(),
    node.clone(),
    BudgetUsage::default(),
    7,
  )
  .unwrap();
  assert_eq!(intent.input(), &input);
  assert_eq!(intent.profile().model_or_tool, reference("research-model"));
  assert_eq!(intent.budget(), attempt_budget());
  assert_eq!(intent.permissions(), &FactoryPermissionSet::deny_all());
  let bytes = serde_json::to_vec(&intent).unwrap();
  let restored = ResearchBuildIntent::restore(&bytes, &work, &admitted, &policy, &flow, &node).unwrap();
  assert_eq!(restored.operation_id().unwrap(), intent.operation_id().unwrap());
  assert_eq!(restored, intent);
}

#[test]
fn research_profiles_pin_build_configuration_and_reject_cross_project_execution() {
  let (_, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let closure = research_closure(WorkKind::Defect, &ResearchRoute::ALL);
  let mut selected = profiles(&closure, input.subject().project_id());
  selected[0].build_configuration = BuildConfigurationRef::new(
    BuildConfigurationId::generate(),
    BuildConfigurationVersion::new(2).unwrap(),
    input.subject().project_id(),
    digest(93),
  );
  selected[0].result_output = key("defect.observations");
  let policy = ResearchPolicy::new(
    input.configuration().clone(),
    &closure,
    WorkKind::Defect,
    settings(WorkKind::Defect),
    selected.clone(),
  )
  .unwrap();
  assert_eq!(
    policy
      .execution_profile(closure.closure().root(), &key("research"))
      .unwrap()
      .build_configuration,
    selected[0].build_configuration
  );
  selected[0].build_configuration = BuildConfigurationRef::new(
    BuildConfigurationId::generate(),
    BuildConfigurationVersion::new(2).unwrap(),
    octacity_server_domain::ProjectId::generate(),
    digest(93),
  );
  assert!(
    ResearchPolicy::new(
      input.configuration().clone(),
      &closure,
      WorkKind::Defect,
      settings(WorkKind::Defect),
      selected
    )
    .is_err()
  );
}

#[test]
fn research_intent_cannot_attach_the_frozen_work_to_another_run() {
  let (work, _, input) = input_fixture(WorkKind::Defect, WorkSize::Small);
  let (policy, closure) = policy(&input);
  let other = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&other, closure.closure().root()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let node = NodeAttempt::new(
    &flow,
    &cycle,
    closure.closure().definition(flow.definition()).unwrap(),
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("research"),
      node_kind: FlowNodeKind::Reasoning,
      number: NodeAttemptNumber::new(1).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: attempt_budget(),
      deadline: time(99),
      execution: NodeExecutionIdentity::External(reference("research-tool")),
      ownership: FactoryClaimOwnership::new(
        key("research.worker"),
        FactoryClaim::new(FactoryClaimFence::new(digest(90)), time(10), time(100)).unwrap(),
      ),
    },
  )
  .unwrap();
  assert!(ResearchBuildIntent::new(input, &policy, flow, node, BudgetUsage::default(), 7).is_err());
}
