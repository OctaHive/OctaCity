use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion, DecisionSignalChoices,
  DecisionSignalDigests, DecisionSignalFallback, DecisionSignalInputMedia, DecisionSignalMode,
  DecisionSignalProbability, DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProvider,
  DecisionSignalProviderCapability, DecisionSignalProviderInput, DecisionSignalProviderLimits,
  DecisionSignalProviderRequest, DecisionSignalPurpose, DecisionSignalQuestion, DecisionSignalQuestionCriteria,
  DecisionSignalQuestionDomain, DecisionSignalQuestionKind, DecisionSignalRequest, DecisionSignalRequestId,
  DecisionSignalRouteSet, DecisionSignalState, DecisionSignalThreshold, ExactSubject, ExternalWorkIdentity,
  FactoryArtifactReference, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion,
  FactoryDigest, FactoryKey, FactoryMetadata, FactoryText, FlowNodeInput, ImmutableReference, NodeAttempt, RiskClass,
  WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};
use std::sync::Arc;

#[path = "openai_https_fixture.rs"]
mod https;

fn openai_capability() -> DecisionSignalProviderCapability {
  capability_for_model("gpt-6-luna")
}

fn capability_for_model(model: &str) -> DecisionSignalProviderCapability {
  DecisionSignalProviderCapability::new(
    exact("openai", "v1", 31),
    exact("openai-decisions-http", "v1", 32),
    exact(model, "unversioned-shadow-v1", 33),
    exact("openai-choice-probabilities", "v1", 34),
    exact("operator-openai-data-policy", "v1", 35),
    vec![DecisionSignalPurpose::Routing, DecisionSignalPurpose::ToolRisk],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(4_096, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn configuration() -> serde_json::Value {
  serde_json::json!({
    "version":1,
    "endpoint":"https://api.openai.com/v1/decisions",
    "capability":openai_capability(),
    "timeout_milliseconds":1000,
    "input_cost_micro_units_per_million_tokens":100000
  })
}

#[test]
fn installed_openai_connector_exposes_its_neutral_capability_without_a_remote_call() {
  let provider = install_openai(
    &serde_json::to_vec(&configuration()).unwrap(),
    "sk-proj-openai-installation-canary".to_owned(),
  )
  .expect("the OpenAI Decisions connector must be installable independently of JEV");
  assert_eq!(provider.capability(), &openai_capability());
  assert_eq!(provider.network_destinations().unwrap()[0].as_str(), "api.openai.com");
}

fn openai_request(mode: DecisionSignalMode) -> DecisionSignalProviderRequest {
  request_for(openai_capability(), mode)
}

fn request_for(
  capability: DecisionSignalProviderCapability,
  mode: DecisionSignalMode,
) -> DecisionSignalProviderRequest {
  let node = node_fixture();
  let choices = DecisionSignalChoices::try_new(vec![key("left"), key("right")]).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::Routing,
    provider: capability.provider().clone(),
    adapter: capability.adapter().clone(),
    model: capability.model().clone(),
    question_set: exact("openai-routing", "v1", 36),
    policy: exact("shadow-policy", "v1", 37),
    mode,
    fallback: DecisionSignalFallback::Escalate,
    budget: budget(),
    routes: Some(DecisionSignalRouteSet::new(key("current-state"), choices.clone())),
  })
  .unwrap();
  let input = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"state\":\"redacted\"}").unwrap(),
    vec![
      DecisionSignalQuestion::new(
        exact("route", "v1", 38),
        FactoryText::new("Choose the route that should receive this work.").unwrap(),
        DecisionSignalQuestionDomain::FiniteChoice(choices),
        DecisionSignalQuestionCriteria::FiniteChoice(vec![
          DecisionSignalChoiceCriterion::new(key("left"), FactoryText::new("First declared case.").unwrap()),
          DecisionSignalChoiceCriterion::new(key("right"), FactoryText::new("Second declared case.").unwrap()),
        ]),
      )
      .unwrap(),
    ],
    key("route"),
    Some(key("current-state")),
  )
  .unwrap();
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &node.0,
    &node.1,
    DecisionSignalPurpose::Routing,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  )
  .unwrap();
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      DecisionSignalPurpose::Routing,
      capability.probability_semantics().clone(),
      exact("unqualified-shadow", "v1", 39),
      DecisionSignalProbability::new(600_000).unwrap(),
      DecisionSignalProbability::new(100_000).unwrap(),
    )
    .unwrap(),
    Timestamp::from_unix_millis(10_000).unwrap(),
  )
  .unwrap()
}

fn response() -> serde_json::Value {
  serde_json::json!({"model":"gpt-6-luna","answers":[{
    "name":"route","type":"choice","choice":"right","confidence":0.7,
    "probabilities":[{"value":"left","probability":0.1},{"value":"right","probability":0.9}]
  }],"usage":{"input_tokens":42,"output_tokens":0,"total_tokens":42,
    "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
    "output_tokens_details":{"reasoning_tokens":0}}})
}

fn installed(fixture: &https::Fixture) -> Arc<dyn DecisionSignalProvider> {
  let mut config = configuration();
  config["endpoint"] = serde_json::json!(fixture.endpoint);
  config["root_certificates_pem"] = serde_json::json!([fixture.root]);
  install_openai(&serde_json::to_vec(&config).unwrap(), https::KEY.to_owned()).unwrap()
}

#[tokio::test]
async fn openai_https_choice_preserves_request_semantics_and_measured_usage() {
  let fixture = https::Fixture::start(response()).await;
  let provider = installed(&fixture);
  let request = openai_request(DecisionSignalMode::Shadow);
  let result = provider
    .evaluate(&request)
    .await
    .expect("typed choice must pass the common provider boundary");
  assert_eq!(result.request_id(), request.id());
  assert_eq!(result.model(), request.model());
  assert_eq!(result.usage().tokens, 42);
  assert_eq!(
    result.usage().cost_micro_units,
    5,
    "round the configured input-token charge upward"
  );
  assert!(
    matches!(result.answers()[0].value(), DecisionSignalAnswerValue::FiniteChoice {
    selected, selected_probability, runner_up_probability,
  } if selected.as_str() == "right" && selected_probability.parts_per_million() == 900_000
    && runner_up_probability.parts_per_million() == 100_000)
  );
  let captured = fixture.captured.lock().unwrap().clone();
  assert_eq!(captured.len(), 1);
  assert!(captured[0].0);
  assert_eq!(
    captured[0].1,
    serde_json::json!({"model":"gpt-6-luna",
    "input":"{\"state\":\"redacted\"}","questions":[{"name":"route","type":"choice",
    "instructions":"Choose the route that should receive this work.","choices":[
      {"value":"left","description":"First declared case."},
      {"value":"right","description":"Second declared case."}]}]})
  );
  fixture.stop().await;
}

#[tokio::test]
async fn openai_unversioned_model_cannot_leave_shadow_or_contact_a_provider_for_another_binding() {
  let fixture = https::Fixture::start(response()).await;
  let provider = installed(&fixture);
  for request in [
    openai_request(DecisionSignalMode::Advisory),
    openai_request(DecisionSignalMode::BoundedControl),
    request_for(capability_for_model("different-model"), DecisionSignalMode::Shadow),
  ] {
    let failure = provider
      .evaluate(&request)
      .await
      .expect_err("an unqualified model must not dispatch");
    assert_eq!(failure.state(), DecisionSignalState::Invalid);
    assert_eq!(failure.usage(), Some(BudgetUsage::default()));
  }
  assert!(fixture.captured.lock().unwrap().is_empty());
  assert!(
    provider
      .observe_evaluation(&openai_request(DecisionSignalMode::Shadow))
      .await
      .is_none()
  );
  assert!(
    fixture.captured.lock().unwrap().is_empty(),
    "observation must never manufacture another evaluation"
  );
  fixture.stop().await;
}

#[tokio::test]
async fn openai_refuses_incomplete_duplicate_or_unnormalized_probabilities() {
  let original = response();
  let mut duplicate = original.clone();
  duplicate["answers"][0]["probabilities"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!({"value":"right","probability":0.9}));
  let mut extra = original.clone();
  extra["answers"][0]["probabilities"][0]["value"] = serde_json::json!("undeclared");
  let mut missing = original.clone();
  missing["answers"][0]["probabilities"].as_array_mut().unwrap().remove(0);
  let mut sum = original.clone();
  sum["answers"][0]["probabilities"][0]["probability"] = serde_json::json!(0.4);
  let mut confidence = original.clone();
  confidence["answers"][0]["confidence"] = serde_json::json!(1.5);
  for body in [duplicate, extra, missing, sum, confidence] {
    let fixture = https::Fixture::start(body).await;
    let failure = installed(&fixture)
      .evaluate(&openai_request(DecisionSignalMode::Shadow))
      .await
      .expect_err("a distribution must cover exactly the declared choices and sum to one");
    assert_eq!(failure.state(), DecisionSignalState::Invalid);
    assert!(
      failure.usage().is_none(),
      "an invalid wire result must not invent free usage"
    );
    fixture.stop().await;
  }
}

#[test]
fn openai_installation_rejects_forged_capabilities_and_unbounded_settings() {
  let original = configuration();
  let changes = [
    ("/capability/provider/identity", serde_json::json!("jev")),
    ("/capability/adapter/identity", serde_json::json!("jev-http")),
    ("/capability/model/identity", serde_json::json!("another-model")),
    (
      "/capability/model/version",
      serde_json::json!("invented-immutable-snapshot"),
    ),
    (
      "/capability/probability_semantics/identity",
      serde_json::json!("jev-choice-probability"),
    ),
    (
      "/capability/supported_question_kinds",
      serde_json::json!(["BoundedScore"]),
    ),
    ("/capability/limits/max_input_bytes", serde_json::json!(0)),
    ("/timeout_milliseconds", serde_json::json!(300001)),
    ("/input_cost_micro_units_per_million_tokens", serde_json::json!(0)),
  ];
  for (path, value) in changes {
    let mut invalid = original.clone();
    *invalid.pointer_mut(path).unwrap() = value;
    assert!(
      install_openai(&serde_json::to_vec(&invalid).unwrap(), https::KEY.to_owned()).is_err(),
      "must refuse {path}"
    );
  }
}

#[tokio::test]
async fn openai_refuses_inconsistent_usage_without_inventing_a_measured_charge() {
  let mut body = response();
  body["usage"]["total_tokens"] = serde_json::json!(41);
  let fixture = https::Fixture::start(body).await;
  let failure = installed(&fixture)
    .evaluate(&openai_request(DecisionSignalMode::Shadow))
    .await
    .expect_err("provider usage must be internally consistent");
  assert_eq!(failure.state(), DecisionSignalState::Invalid);
  assert!(failure.usage().is_none());
  fixture.stop().await;
}

#[tokio::test]
async fn openai_lost_response_rate_limit_timeout_and_refusal_preserve_unknown_usage_and_never_redispatch() {
  let refusal = serde_json::json!({"model":"gpt-6-luna","answers":[{
    "type":"refusal","name":"route","private":"provider-refusal-canary"}],"usage":{"input_tokens":42,"output_tokens":0,"total_tokens":42}});
  for (body, status, delay, expected) in [
    (None, 200, 0, DecisionSignalState::Unavailable),
    (
      Some(serde_json::json!({"error":"provider-error-canary"})),
      429,
      0,
      DecisionSignalState::Unavailable,
    ),
    (Some(response()), 200, 1000, DecisionSignalState::TimedOut),
    (Some(refusal), 200, 0, DecisionSignalState::Invalid),
  ] {
    let fixture = https::Fixture::reply(body, status, delay).await;
    let mut config = configuration();
    config["endpoint"] = serde_json::json!(fixture.endpoint);
    config["root_certificates_pem"] = serde_json::json!([fixture.root]);
    config["timeout_milliseconds"] = serde_json::json!(300);
    let provider = install_openai(&serde_json::to_vec(&config).unwrap(), https::KEY.to_owned()).unwrap();
    let request = openai_request(DecisionSignalMode::Shadow);
    let failure = provider.evaluate(&request).await.unwrap_err();
    assert_eq!(failure.state(), expected);
    assert!(failure.usage().is_none());
    assert!(!format!("{failure:?}").contains("canary"));
    assert_eq!(fixture.captured.lock().unwrap().len(), 1);
    assert!(provider.observe_evaluation(&request).await.is_none());
    assert_eq!(
      fixture.captured.lock().unwrap().len(),
      1,
      "read-only recovery never repeats a billable POST"
    );
    fixture.stop().await;
  }
}

#[tokio::test]
async fn openai_https_preserves_private_ca_hostname_and_response_bounds() {
  let fixture = https::Fixture::start(response()).await;
  let mut config = configuration();
  config["endpoint"] = serde_json::json!(fixture.endpoint);
  let provider = install_openai(&serde_json::to_vec(&config).unwrap(), https::KEY.to_owned()).unwrap();
  assert_eq!(
    provider
      .evaluate(&openai_request(DecisionSignalMode::Shadow))
      .await
      .unwrap_err()
      .state(),
    DecisionSignalState::Unavailable
  );
  config["root_certificates_pem"] = serde_json::json!([fixture.root]);
  config["endpoint"] = serde_json::json!(fixture.endpoint.replace("localhost", "127.0.0.1"));
  let provider = install_openai(&serde_json::to_vec(&config).unwrap(), https::KEY.to_owned()).unwrap();
  assert_eq!(
    provider
      .evaluate(&openai_request(DecisionSignalMode::Shadow))
      .await
      .unwrap_err()
      .state(),
    DecisionSignalState::Unavailable
  );
  assert!(fixture.captured.lock().unwrap().is_empty());
  fixture.stop().await;
  let fixture = https::Fixture::start(serde_json::json!({"private":"x".repeat(4*1024*1024+1)})).await;
  let failure = installed(&fixture)
    .evaluate(&openai_request(DecisionSignalMode::Shadow))
    .await
    .unwrap_err();
  assert_eq!(failure.state(), DecisionSignalState::Invalid);
  assert!(failure.usage().is_none());
  fixture.stop().await;
}

#[test]
fn openai_credentials_and_private_endpoints_never_enter_diagnostics() {
  let mut config = configuration();
  config["endpoint"] = serde_json::json!("https://private-tenant-canary.invalid/v1/decisions");
  let provider = crate::OpenAiDecisionSignalProvider::from_deployment_json(
    &serde_json::to_vec(&config).unwrap(),
    https::KEY.to_owned(),
  )
  .unwrap();
  assert!(!format!("{provider:?}").contains("canary"));
  config["credential"] = serde_json::json!(https::KEY);
  let error = crate::OpenAiDecisionSignalProvider::from_deployment_json(
    &serde_json::to_vec(&config).unwrap(),
    https::KEY.to_owned(),
  )
  .unwrap_err();
  assert!(!format!("{error:?}").contains("canary"));
}

fn install_openai(
  config: &[u8],
  credential: String,
) -> Result<Arc<dyn DecisionSignalProvider>, crate::OpenAiDecisionSignalError> {
  Ok(Arc::new(crate::OpenAiDecisionSignalProvider::from_deployment_json(
    config, credential,
  )?))
}
fn node_fixture() -> (FlowNodeInput, NodeAttempt) {
  let subject = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("base").unwrap(),
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
    ExternalWorkIdentity::new("source/1").unwrap(),
    subject,
    WorkArtifacts::new(artifact(90), artifact(91), vec![]).unwrap(),
    WorkClassification::new(
      WorkPriority::new(0).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap();
  octacity_server_factory::testing::capability_node_fixture(&work, None)
}
fn budget() -> BudgetLimit {
  BudgetLimit::new(2, 10_000, 10_000, 2_000_000, 10_000).unwrap()
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}

fn exact(identity: &str, version: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(value))
}
