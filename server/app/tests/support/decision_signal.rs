//! Provider-neutral receipt fixture bound to an existing persisted node.
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str, byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([byte; 32]))
}
pub fn request(input: &FlowNodeInput, node: &NodeAttempt) -> DecisionSignalProviderRequest {
  request_for_purpose(input, node, DecisionSignalPurpose::Routing)
}
pub fn tool_risk_request(input: &FlowNodeInput, node: &NodeAttempt) -> DecisionSignalProviderRequest {
  request_for_purpose(input, node, DecisionSignalPurpose::ToolRisk)
}
fn request_for_purpose(
  input: &FlowNodeInput,
  node: &NodeAttempt,
  purpose: DecisionSignalPurpose,
) -> DecisionSignalProviderRequest {
  let budget = BudgetLimit::new(1, 1000, 100, 1000, 1000).unwrap();
  let choices = DecisionSignalChoices::try_new(vec![key("continue"), key("escalate")]).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose,
    provider: reference("qualified.provider", 1),
    adapter: reference("qualified.adapter", 2),
    model: reference("qualified.model", 3),
    question_set: reference("routing.questions", 4),
    policy: reference("routing.policy", 5),
    mode: DecisionSignalMode::Shadow,
    fallback: DecisionSignalFallback::Escalate,
    budget,
    routes: (purpose == DecisionSignalPurpose::Routing)
      .then(|| DecisionSignalRouteSet::new(key("selected-summary"), choices.clone())),
  })
  .unwrap();
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    reference("probability.semantics", 6),
    reference("private.data-handling", 7),
    vec![purpose],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1024, 20, 24).unwrap(),
    budget,
  )
  .unwrap();
  let data = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"summary\":\"private selected task text; never log this\"}").unwrap(),
    vec![
      DecisionSignalQuestion::new(
        reference("route.question", 8),
        FactoryText::new("Select the declared choice for this summary.").unwrap(),
        DecisionSignalQuestionDomain::FiniteChoice(choices),
        DecisionSignalQuestionCriteria::FiniteChoice(vec![
          DecisionSignalChoiceCriterion::new(
            key("continue"),
            FactoryText::new("The selected summary is sufficient.").unwrap(),
          ),
          DecisionSignalChoiceCriterion::new(
            key("escalate"),
            FactoryText::new("The selected summary is insufficient.").unwrap(),
          ),
        ]),
      )
      .unwrap(),
    ],
    key("route.question"),
    (purpose == DecisionSignalPurpose::Routing).then(|| key("selected-summary")),
  )
  .unwrap();
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    input,
    node,
    purpose,
    DecisionSignalDigests::new(data.digest(), profile.policy().digest()),
    budget,
  )
  .unwrap();
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    data,
    DecisionSignalThreshold::new(
      purpose,
      capability.probability_semantics().clone(),
      reference("calibration", 9),
      DecisionSignalProbability::new(600000).unwrap(),
      DecisionSignalProbability::new(100000).unwrap(),
    )
    .unwrap(),
    if purpose == DecisionSignalPurpose::Routing {
      Timestamp::from_unix_millis(1000).unwrap()
    } else {
      node.deadline()
    },
  )
  .unwrap()
}
pub fn receipt(request: &DecisionSignalProviderRequest) -> DecisionSignalReceipt {
  consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    request,
    DecisionSignalProviderObservation::Failure(
      DecisionSignalProviderFailure::new(
        request.id(),
        request.provider().clone(),
        request.adapter().clone(),
        request.model().clone(),
        DecisionSignalState::Unavailable,
        BudgetUsage {
          attempts: 1,
          elapsed_millis: 5,
          tokens: 7,
          cost_micro_units: 3.into(),
          output_bytes: 0,
        },
      )
      .unwrap(),
    ),
    key("continue"),
  )
  .unwrap()
}
