use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};

use crate::{
  BudgetLimit, BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalDigests, DecisionSignalDisposition, DecisionSignalFallback,
  DecisionSignalInputMedia, DecisionSignalMode, DecisionSignalProbability, DecisionSignalProfile,
  DecisionSignalProfileDefinition, DecisionSignalProviderCapability, DecisionSignalProviderFailure,
  DecisionSignalProviderInput, DecisionSignalProviderLimits, DecisionSignalProviderObservation,
  DecisionSignalProviderRequest, DecisionSignalProviderResult, DecisionSignalPurpose, DecisionSignalQuestion,
  DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind, DecisionSignalReceiptId,
  DecisionSignalRequest, DecisionSignalRequestId, DecisionSignalRouteSet, DecisionSignalScoreDomain,
  DecisionSignalState, DecisionSignalThreshold, ExactSubject, ExternalWorkIdentity, FactoryArtifactReference,
  FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryDigest, FactoryError,
  FactoryKey, FactoryMetadata, FactoryRun, FactoryRunId, FactoryText, ImmutableReference, MAX_DECISION_SIGNAL_CHOICES,
  RiskClass, StageAttempt, StageAttemptId, StageAttemptNumber, ToolRiskChoiceMapping, WorkArtifacts,
  WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority, consume_routing_signal, consume_tool_risk_signal,
};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key")
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).expect("fixture artifact")
}

fn exact(identity: &str, version: &str, digest_value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(digest_value))
}

fn budget() -> BudgetLimit {
  BudgetLimit::new(1, 1_000, 1_000, 1_000, 1_000).expect("fixture budget")
}

fn stage() -> StageAttempt {
  let subject = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("base").expect("fixture revision"),
  );
  let configuration = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    subject.project_id(),
    digest(1),
  );
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration,
    ExternalWorkIdentity::new("source/1").expect("fixture identity"),
    subject,
    WorkArtifacts::new(artifact(90), artifact(91), vec![]).expect("fixture artifacts"),
    WorkClassification::new(
      WorkPriority::new(0).expect("fixture priority"),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .expect("fixture work");
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  StageAttempt::new(
    StageAttemptId::generate(),
    &run,
    StageAttemptNumber::INITIAL,
    crate::FactoryStageTarget::Implementation,
    budget(),
    digest(2),
    crate::FactoryClaimOwnership::new(
      key("worker"),
      crate::FactoryClaim::new(
        crate::FactoryClaimFence::new(digest(9)),
        Timestamp::from_unix_millis(1).expect("fixture claim start"),
        Timestamp::from_unix_millis(2).expect("fixture claim deadline"),
      )
      .expect("fixture claim"),
    ),
  )
}

fn choices(values: &[&str]) -> DecisionSignalChoices {
  DecisionSignalChoices::try_new(values.iter().map(|value| key(value)).collect()).expect("fixture choices")
}

fn choice_question(identity: &str, values: &[&str]) -> DecisionSignalQuestion {
  let choices = choices(values);
  DecisionSignalQuestion::new(
    exact(identity, "v1", 31),
    FactoryText::new("Choose the declared outcome that matches the redacted state.").expect("fixture instructions"),
    DecisionSignalQuestionDomain::FiniteChoice(choices.clone()),
    DecisionSignalQuestionCriteria::FiniteChoice(
      choices
        .as_slice()
        .iter()
        .map(|choice| {
          DecisionSignalChoiceCriterion::new(
            choice.clone(),
            FactoryText::new(format!("State belongs to the '{}' outcome.", choice.as_str()))
              .expect("fixture criterion"),
          )
        })
        .collect(),
    ),
  )
  .expect("fixture question")
}

fn provider_input(purpose: DecisionSignalPurpose, values: &[&str]) -> DecisionSignalProviderInput {
  let consuming_question = key("route-or-risk");
  DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"state\":\"redacted\"}").expect("fixture state"),
    vec![choice_question("route-or-risk", values)],
    consuming_question,
    (purpose == DecisionSignalPurpose::Routing).then(|| key("current-state")),
  )
  .expect("fixture provider input")
}

fn profile(
  purpose: DecisionSignalPurpose,
  mode: DecisionSignalMode,
  fallback: DecisionSignalFallback,
  choice_values: &[&str],
) -> DecisionSignalProfile {
  profile_with_budget(purpose, mode, fallback, choice_values, budget())
}

fn profile_with_budget(
  purpose: DecisionSignalPurpose,
  mode: DecisionSignalMode,
  fallback: DecisionSignalFallback,
  choice_values: &[&str],
  budget: BudgetLimit,
) -> DecisionSignalProfile {
  DecisionSignalProfile::from_resolved(DecisionSignalProfileDefinition {
    purpose,
    provider: exact("provider", "v1", 3),
    adapter: exact("adapter", "v1", 32),
    model: exact("model", "v7", 4),
    question_set: exact("questions", "v2", 5),
    policy: exact("policy", "v4", 6),
    mode,
    fallback,
    budget,
    routes: (purpose == DecisionSignalPurpose::Routing)
      .then(|| DecisionSignalRouteSet::new(key("current-state"), choices(choice_values))),
  })
}

fn provider_request(
  purpose: DecisionSignalPurpose,
  mode: DecisionSignalMode,
  fallback: DecisionSignalFallback,
  choice_values: &[&str],
  minimum_probability: u32,
  minimum_margin: u32,
) -> DecisionSignalProviderRequest {
  let stage = stage();
  provider_request_for_stage(
    &stage,
    purpose,
    mode,
    fallback,
    choice_values,
    budget(),
    minimum_probability,
    minimum_margin,
  )
}

#[expect(
  clippy::too_many_arguments,
  reason = "test fixture exposes every signal-policy dimension"
)]
pub(crate) fn provider_request_for_stage(
  stage: &StageAttempt,
  purpose: DecisionSignalPurpose,
  mode: DecisionSignalMode,
  fallback: DecisionSignalFallback,
  choice_values: &[&str],
  request_budget: BudgetLimit,
  minimum_probability: u32,
  minimum_margin: u32,
) -> DecisionSignalProviderRequest {
  let profile = profile_with_budget(purpose, mode, fallback, choice_values, request_budget);
  let input = provider_input(purpose, choice_values);
  let semantics = exact("semantics", "v3", 7);
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    semantics.clone(),
    exact("private-data", "v1", 26),
    vec![purpose],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 4, MAX_DECISION_SIGNAL_CHOICES as u16).expect("fixture limits"),
    request_budget,
  )
  .expect("fixture capability");
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    stage,
    purpose,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    profile.budget(),
  );
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      purpose,
      semantics,
      exact("calibration", "v5", 9),
      DecisionSignalProbability::new(minimum_probability).expect("fixture probability"),
      DecisionSignalProbability::new(minimum_margin).expect("fixture margin"),
    )
    .expect("fixture threshold"),
    Timestamp::from_unix_millis(10_000).expect("fixture deadline"),
  )
  .expect("fixture provider request")
}

fn result(
  request: &DecisionSignalProviderRequest,
  selected_choice: &str,
  probability: u32,
  runner_up: u32,
) -> DecisionSignalProviderResult {
  let budget = request.request().budget();
  DecisionSignalProviderResult::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    vec![DecisionSignalAnswer::new(
      request.input().questions()[0].reference().clone(),
      DecisionSignalAnswerValue::FiniteChoice {
        selected: key(selected_choice),
        selected_probability: DecisionSignalProbability::new(probability).expect("fixture probability"),
        runner_up_probability: DecisionSignalProbability::new(runner_up).expect("fixture probability"),
      },
    )],
    BudgetUsage {
      attempts: budget.max_attempts().min(1),
      elapsed_millis: budget.max_elapsed_millis().min(5),
      tokens: budget.max_tokens().min(10),
      cost_micro_units: budget.max_cost_micro_units().min(2),
      output_bytes: budget.max_output_bytes().min(20),
    },
  )
  .expect("fixture result")
}

pub(crate) fn observation(
  request: &DecisionSignalProviderRequest,
  selected_choice: &str,
  probability: u32,
  runner_up: u32,
) -> DecisionSignalProviderObservation {
  DecisionSignalProviderObservation::Result(result(request, selected_choice, probability, runner_up))
}

#[test]
fn finite_choices_reject_empty_duplicate_and_oversized_domains() {
  assert!(DecisionSignalChoices::try_new(vec![]).is_err());
  assert!(DecisionSignalChoices::try_new(vec![key("one"), key("one")]).is_err());
  assert!(
    DecisionSignalChoices::try_new(
      (0..=MAX_DECISION_SIGNAL_CHOICES)
        .map(|index| key(&format!("choice-{index}")))
        .collect()
    )
    .is_err()
  );
}

#[test]
fn canonical_json_and_versioned_question_domains_are_bound_to_the_input_digest() {
  let question = choice_question("route-or-risk", &["left", "right"]);
  let first = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"z\":2,\"a\":1}").expect("fixture JSON"),
    vec![question.clone()],
    key("route-or-risk"),
    None,
  )
  .expect("JSON is canonicalized");
  let second = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"a\":1,\"z\":2}").expect("fixture JSON"),
    vec![question],
    key("route-or-risk"),
    None,
  )
  .expect("canonical JSON is accepted");

  assert_eq!(first.state_document().as_str(), "{\"a\":1,\"z\":2}");
  assert_eq!(first.digest(), second.digest());
  assert!(
    DecisionSignalProviderInput::new(
      DecisionSignalInputMedia::CanonicalJson,
      FactoryText::new("not JSON").expect("bounded text"),
      second.questions().to_vec(),
      key("route-or-risk"),
      None,
    )
    .is_err()
  );
}

#[test]
fn request_rejects_asserted_input_policy_and_route_drift() {
  let stage = stage();
  let profile = profile(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::Shadow,
    DecisionSignalFallback::Escalate,
    &["left", "right"],
  );
  let input = provider_input(DecisionSignalPurpose::Routing, &["left", "invented"]);
  let semantics = exact("semantics", "v1", 40);
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    semantics.clone(),
    exact("private-data", "v1", 41),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 4, 4).expect("limits"),
    budget(),
  )
  .expect("capability");
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage,
    DecisionSignalPurpose::Routing,
    DecisionSignalDigests::new(digest(42), profile.policy().digest()),
    budget(),
  );
  assert!(
    DecisionSignalProviderRequest::new(
      request,
      &profile,
      &capability,
      input,
      DecisionSignalThreshold::new(
        DecisionSignalPurpose::Routing,
        semantics,
        exact("calibration", "v1", 43),
        DecisionSignalProbability::new(600_000).expect("probability"),
        DecisionSignalProbability::new(100_000).expect("margin"),
      )
      .expect("threshold"),
      Timestamp::from_unix_millis(1).expect("deadline"),
    )
    .is_err()
  );
}

#[test]
fn capability_rejects_duplicate_declarations_and_request_limit_drift() {
  let profile = profile(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::Shadow,
    DecisionSignalFallback::Escalate,
    &["left", "right"],
  );
  let semantics = exact("semantics", "v1", 27);
  assert!(
    DecisionSignalProviderCapability::new(
      profile.provider().clone(),
      profile.adapter().clone(),
      profile.model().clone(),
      semantics.clone(),
      exact("private-data", "v1", 28),
      vec![DecisionSignalPurpose::Routing, DecisionSignalPurpose::Routing],
      vec![DecisionSignalInputMedia::CanonicalJson],
      vec![DecisionSignalQuestionKind::FiniteChoice],
      DecisionSignalProviderLimits::new(1_024, 1, 2).expect("limits"),
      budget(),
    )
    .is_err()
  );

  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    semantics.clone(),
    exact("private-data", "v1", 28),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::Utf8Text],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 1, 2).expect("limits"),
    budget(),
  )
  .expect("capability");
  let stage = stage();
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage,
    DecisionSignalPurpose::Routing,
    DecisionSignalDigests::new(digest(29), profile.policy().digest()),
    budget(),
  );
  assert!(
    DecisionSignalProviderRequest::new(
      request,
      &profile,
      &capability,
      provider_input(DecisionSignalPurpose::Routing, &["left", "right"]),
      DecisionSignalThreshold::new(
        DecisionSignalPurpose::Routing,
        semantics,
        exact("calibration", "v1", 30),
        DecisionSignalProbability::new(600_000).expect("probability"),
        DecisionSignalProbability::new(100_000).expect("margin"),
      )
      .expect("threshold"),
      Timestamp::from_unix_millis(1).expect("deadline"),
    )
    .is_err()
  );
}

#[test]
fn request_requires_exact_provider_model_semantics_and_purpose() {
  let stage = stage();
  let profile = profile(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::Shadow,
    DecisionSignalFallback::Escalate,
    &["left", "right"],
  );
  let semantics = exact("semantics", "v1", 11);
  let capability = DecisionSignalProviderCapability::new(
    exact("other-provider", "v1", 12),
    profile.adapter().clone(),
    profile.model().clone(),
    semantics.clone(),
    exact("private-data", "v1", 26),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 4, 4).expect("limits"),
    budget(),
  )
  .expect("capability shape is valid");
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage,
    DecisionSignalPurpose::Routing,
    DecisionSignalDigests::new(digest(13), profile.policy().digest()),
    budget(),
  );
  let threshold = DecisionSignalThreshold::new(
    DecisionSignalPurpose::Routing,
    semantics,
    exact("calibration", "v1", 14),
    DecisionSignalProbability::new(700_000).expect("probability"),
    DecisionSignalProbability::new(100_000).expect("margin"),
  )
  .expect("threshold");

  assert!(matches!(
    DecisionSignalProviderRequest::new(
      request,
      &profile,
      &capability,
      provider_input(DecisionSignalPurpose::Routing, &["left", "right"]),
      threshold,
      Timestamp::from_unix_millis(1).expect("deadline"),
    ),
    Err(FactoryError::InvalidDecisionSignal { .. })
  ));
}

#[test]
fn provider_result_answers_every_typed_question_and_rejects_an_out_of_domain_score() {
  let stage = stage();
  let profile = profile(
    DecisionSignalPurpose::ToolRisk,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Deny,
    &["allow", "deny"],
  );
  let input = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{}").expect("fixture JSON"),
    vec![
      DecisionSignalQuestion::new(
        exact("route-or-risk", "v1", 31),
        FactoryText::new("Choose whether the action is permitted or denied.").expect("fixture instructions"),
        DecisionSignalQuestionDomain::FiniteChoice(choices(&["allow", "deny"])),
        DecisionSignalQuestionCriteria::FiniteChoice(vec![
          DecisionSignalChoiceCriterion::new(
            key("allow"),
            FactoryText::new("The action remains inside every authoritative boundary.").expect("fixture criterion"),
          ),
          DecisionSignalChoiceCriterion::new(
            key("deny"),
            FactoryText::new("The action must not execute under the current risk policy.").expect("fixture criterion"),
          ),
        ]),
      )
      .expect("fixture choice question"),
      DecisionSignalQuestion::new(
        exact("risk-score", "v2", 44),
        FactoryText::new("Rate the action risk from negligible to critical.").expect("fixture instructions"),
        DecisionSignalQuestionDomain::BoundedScore(DecisionSignalScoreDomain::new(0, 10).expect("score domain")),
        DecisionSignalQuestionCriteria::BoundedScore(
          (0..=10)
            .map(|score| FactoryText::new(format!("Risk tier {score}.")).expect("fixture score criterion"))
            .collect(),
        ),
      )
      .expect("fixture score question"),
    ],
    key("route-or-risk"),
    None,
  )
  .expect("provider input");
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage,
    DecisionSignalPurpose::ToolRisk,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  );
  let semantics = exact("semantics", "v1", 45);
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    semantics.clone(),
    exact("private-data", "v1", 46),
    vec![DecisionSignalPurpose::ToolRisk],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![
      DecisionSignalQuestionKind::FiniteChoice,
      DecisionSignalQuestionKind::BoundedScore,
    ],
    DecisionSignalProviderLimits::new(1_024, 4, 4).expect("limits"),
    budget(),
  )
  .expect("capability");
  let request = DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      DecisionSignalPurpose::ToolRisk,
      semantics,
      exact("calibration", "v1", 47),
      DecisionSignalProbability::new(600_000).expect("probability"),
      DecisionSignalProbability::new(100_000).expect("margin"),
    )
    .expect("threshold"),
    Timestamp::from_unix_millis(1).expect("deadline"),
  )
  .expect("request");
  let answers = |score| {
    vec![
      DecisionSignalAnswer::new(
        request.input().questions()[0].reference().clone(),
        DecisionSignalAnswerValue::FiniteChoice {
          selected: key("allow"),
          selected_probability: DecisionSignalProbability::new(900_000).expect("probability"),
          runner_up_probability: DecisionSignalProbability::new(100_000).expect("probability"),
        },
      ),
      DecisionSignalAnswer::new(
        request.input().questions()[1].reference().clone(),
        DecisionSignalAnswerValue::BoundedScore {
          score,
          confidence: DecisionSignalProbability::new(800_000).expect("confidence"),
        },
      ),
    ]
  };
  let result = |score| {
    DecisionSignalProviderResult::new(
      request.id(),
      request.provider().clone(),
      request.adapter().clone(),
      request.model().clone(),
      request.probability_semantics().clone(),
      answers(score),
      BudgetUsage::default(),
    )
    .expect("result shape")
  };
  let mapping = ToolRiskChoiceMapping::try_new(
    request.choices(),
    vec![
      (key("allow"), DecisionSignalDisposition::Allow),
      (key("deny"), DecisionSignalDisposition::Deny),
    ],
  )
  .expect("mapping");

  let accepted = consume_tool_risk_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    DecisionSignalProviderObservation::Result(result(7)),
    &mapping,
  )
  .expect("signal");
  assert_eq!(accepted.state(), DecisionSignalState::Completed);
  let rejected = consume_tool_risk_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    DecisionSignalProviderObservation::Result(result(11)),
    &mapping,
  )
  .expect("signal");
  assert_eq!(rejected.state(), DecisionSignalState::Invalid);
}

#[test]
fn calibrated_threshold_is_purpose_specific_and_checks_probability_and_margin() {
  let routing = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Escalate,
    &["baseline", "fast"],
    700_000,
    200_000,
  );
  let below_margin = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &routing,
    observation(&routing, "fast", 700_000, 500_001),
    key("baseline"),
  )
  .expect("signal is consumed");
  assert_eq!(
    below_margin.consumption().final_disposition(),
    &DecisionSignalDisposition::Escalate
  );
  let accepted = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &routing,
    observation(&routing, "fast", 700_000, 500_000),
    key("baseline"),
  )
  .expect("threshold boundary is accepted");
  assert_eq!(
    accepted.consumption().final_disposition(),
    &DecisionSignalDisposition::Route(key("fast"))
  );

  let tool_profile = profile(
    DecisionSignalPurpose::ToolRisk,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Deny,
    &["allow", "deny"],
  );
  let stage = stage();
  let semantics = routing.probability_semantics().clone();
  let capability = DecisionSignalProviderCapability::new(
    tool_profile.provider().clone(),
    tool_profile.adapter().clone(),
    tool_profile.model().clone(),
    semantics.clone(),
    exact("private-data", "v1", 26),
    vec![DecisionSignalPurpose::ToolRisk],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 4, 3).expect("limits"),
    budget(),
  )
  .expect("capability");
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage,
    DecisionSignalPurpose::ToolRisk,
    DecisionSignalDigests::new(digest(17), tool_profile.policy().digest()),
    budget(),
  );
  assert!(matches!(
    DecisionSignalProviderRequest::new(
      request,
      &tool_profile,
      &capability,
      provider_input(DecisionSignalPurpose::ToolRisk, &["allow", "deny"]),
      routing.threshold().clone(),
      Timestamp::from_unix_millis(1).expect("deadline"),
    ),
    Err(FactoryError::InvalidDecisionSignal { .. })
  ));
}

#[test]
fn routing_rejects_undeclared_or_wrong_identity_results_with_fail_closed_fallback() {
  let request = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Deny,
    &["baseline", "fast"],
    600_000,
    100_000,
  );
  let undeclared = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    observation(&request, "invented", 900_000, 100_000),
    key("baseline"),
  )
  .expect("invalid provider result is normalized");
  assert_eq!(undeclared.state(), DecisionSignalState::Invalid);
  assert_eq!(undeclared.result(), None);
  assert_eq!(
    undeclared.consumption().final_disposition(),
    &DecisionSignalDisposition::Deny
  );

  for (provider, model, semantics) in [
    (
      exact("other-provider", "v1", 19),
      request.model().clone(),
      request.probability_semantics().clone(),
    ),
    (
      request.provider().clone(),
      exact("other-model", "v1", 20),
      request.probability_semantics().clone(),
    ),
    (
      request.provider().clone(),
      request.model().clone(),
      exact("other-semantics", "v1", 21),
    ),
  ] {
    let mismatched = DecisionSignalProviderResult::new(
      request.id(),
      provider,
      request.adapter().clone(),
      model,
      semantics,
      vec![DecisionSignalAnswer::new(
        request.input().questions()[0].reference().clone(),
        DecisionSignalAnswerValue::FiniteChoice {
          selected: key("fast"),
          selected_probability: DecisionSignalProbability::new(900_000).expect("probability"),
          runner_up_probability: DecisionSignalProbability::new(100_000).expect("probability"),
        },
      )],
      BudgetUsage::default(),
    )
    .expect("result shape");
    let receipt = consume_routing_signal(
      DecisionSignalReceiptId::generate(),
      &request,
      DecisionSignalProviderObservation::Result(mismatched),
      key("baseline"),
    )
    .expect("identity mismatch is normalized");
    assert_eq!(receipt.state(), DecisionSignalState::Invalid);
    assert_eq!(
      receipt.consumption().final_disposition(),
      &DecisionSignalDisposition::Deny
    );
  }
}

#[test]
fn rollout_modes_never_give_shadow_or_advisory_authority() {
  for (mode, expected_kind) in [
    (DecisionSignalMode::Shadow, crate::DecisionSignalConsumptionKind::Shadow),
    (
      DecisionSignalMode::Advisory,
      crate::DecisionSignalConsumptionKind::Advisory,
    ),
  ] {
    let request = provider_request(
      DecisionSignalPurpose::Routing,
      mode,
      DecisionSignalFallback::Escalate,
      &["baseline", "fast"],
      600_000,
      100_000,
    );
    let receipt = consume_routing_signal(
      DecisionSignalReceiptId::generate(),
      &request,
      observation(&request, "fast", 900_000, 100_000),
      key("baseline"),
    )
    .expect("signal is consumed");
    assert_eq!(receipt.consumption().kind(), expected_kind);
    assert_eq!(
      receipt.consumption().recommendation(),
      Some(&DecisionSignalDisposition::Route(key("fast")))
    );
    assert_eq!(
      receipt.consumption().final_disposition(),
      &DecisionSignalDisposition::Route(key("baseline"))
    );
  }
}

#[test]
fn tool_risk_can_only_preserve_or_narrow_an_authorized_action() {
  let request = provider_request(
    DecisionSignalPurpose::ToolRisk,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Deny,
    &["preserve", "deny", "escalate"],
    600_000,
    100_000,
  );
  let mapping = ToolRiskChoiceMapping::try_new(
    request.choices(),
    vec![
      (key("preserve"), DecisionSignalDisposition::Allow),
      (key("deny"), DecisionSignalDisposition::Deny),
      (key("escalate"), DecisionSignalDisposition::Escalate),
    ],
  )
  .expect("complete mapping");
  for (answer, expected) in [
    ("preserve", DecisionSignalDisposition::Allow),
    ("deny", DecisionSignalDisposition::Deny),
    ("escalate", DecisionSignalDisposition::Escalate),
  ] {
    let receipt = consume_tool_risk_signal(
      DecisionSignalReceiptId::generate(),
      &request,
      observation(&request, answer, 900_000, 100_000),
      &mapping,
    )
    .expect("tool signal is consumed");
    assert_eq!(receipt.consumption().baseline(), &DecisionSignalDisposition::Allow);
    assert_eq!(receipt.consumption().final_disposition(), &expected);
  }
  assert!(
    ToolRiskChoiceMapping::try_new(
      request.choices(),
      vec![(key("preserve"), DecisionSignalDisposition::Route(key("invented")))]
    )
    .is_err()
  );
}

#[test]
fn provider_failure_is_secret_safe_and_fails_closed_only_in_bounded_control() {
  let bounded = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Escalate,
    &["baseline", "fast"],
    600_000,
    100_000,
  );
  let failure = DecisionSignalProviderFailure::new(
    bounded.id(),
    bounded.provider().clone(),
    bounded.adapter().clone(),
    bounded.model().clone(),
    DecisionSignalState::Unavailable,
    BudgetUsage::default(),
  )
  .expect("failure");
  let receipt = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &bounded,
    DecisionSignalProviderObservation::Failure(failure),
    key("baseline"),
  )
  .expect("failure is consumed");
  assert_eq!(receipt.state(), DecisionSignalState::Unavailable);
  assert_eq!(receipt.usage(), BudgetUsage::default());
  assert_eq!(
    receipt.consumption().final_disposition(),
    &DecisionSignalDisposition::Escalate
  );
  assert!(
    DecisionSignalProviderFailure::new(
      bounded.id(),
      bounded.provider().clone(),
      bounded.adapter().clone(),
      bounded.model().clone(),
      DecisionSignalState::Completed,
      BudgetUsage::default(),
    )
    .is_err()
  );
}

#[test]
fn receipt_replay_reuses_recorded_disposition_and_rejects_request_drift() {
  let request = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Escalate,
    &["baseline", "fast"],
    600_000,
    100_000,
  );
  let receipt = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    observation(&request, "fast", 900_000, 100_000),
    key("baseline"),
  )
  .expect("receipt");
  assert_eq!(
    receipt.replay(&request).expect("exact replay").final_disposition(),
    &DecisionSignalDisposition::Route(key("fast"))
  );

  let replay_profile = profile(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Escalate,
    &["baseline", "fast"],
  );
  let replay_capability = DecisionSignalProviderCapability::new(
    replay_profile.provider().clone(),
    replay_profile.adapter().clone(),
    replay_profile.model().clone(),
    request.probability_semantics().clone(),
    request.data_handling().clone(),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 4, MAX_DECISION_SIGNAL_CHOICES as u16).expect("limits"),
    budget(),
  )
  .expect("capability");
  let changed_deadline = DecisionSignalProviderRequest::new(
    request.request().clone(),
    &replay_profile,
    &replay_capability,
    request.input().clone(),
    request.threshold().clone(),
    Timestamp::from_unix_millis(10_001).expect("deadline"),
  )
  .expect("request with changed deadline");
  assert_ne!(request.digest(), changed_deadline.digest());
  assert!(receipt.replay(&changed_deadline).is_err());

  let drifted = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Escalate,
    &["baseline", "slow"],
    600_000,
    100_000,
  );
  assert!(matches!(
    receipt.replay(&drifted),
    Err(FactoryError::InvalidReference { .. })
  ));
}

#[test]
fn restored_provider_request_revalidates_every_canonical_digest() {
  let request = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::Advisory,
    DecisionSignalFallback::Escalate,
    &["baseline", "fast"],
    600_000,
    100_000,
  );
  request.validate_integrity().expect("fresh request is integral");

  for field in ["digest", "policy"] {
    let mut encoded = serde_json::to_value(&request).expect("serialize request");
    encoded[field] = serde_json::to_value(exact("tampered", "v1", 250)).expect("serialize tampered reference");
    if field == "digest" {
      encoded[field] = serde_json::to_value(digest(250)).expect("serialize tampered digest");
    }
    let restored: DecisionSignalProviderRequest =
      serde_json::from_value(encoded).expect("request remains structurally valid");
    assert_eq!(
      restored.validate_integrity(),
      Err(FactoryError::InvalidDecisionSignal {
        field: "provider request integrity",
      })
    );
  }
}

#[test]
fn restored_receipt_revalidates_its_result_and_canonical_digest() {
  let request = provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::Advisory,
    DecisionSignalFallback::Escalate,
    &["baseline", "fast"],
    600_000,
    100_000,
  );
  let receipt = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    observation(&request, "fast", 900_000, 100_000),
    key("baseline"),
  )
  .expect("receipt");
  receipt.validate_integrity().expect("fresh receipt is integral");

  let mut encoded = serde_json::to_value(receipt).expect("serialize receipt");
  encoded["receipt_digest"] = serde_json::to_value(digest(251)).expect("serialize tampered digest");
  let restored: crate::DecisionSignalReceipt =
    serde_json::from_value(encoded).expect("receipt remains structurally valid");
  assert_eq!(
    restored.validate_integrity(),
    Err(FactoryError::InvalidDecisionSignal {
      field: "receipt integrity",
    })
  );
}
