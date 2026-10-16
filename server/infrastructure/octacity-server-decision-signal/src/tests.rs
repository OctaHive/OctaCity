use super::DecisionSignalProviderRegistry;
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalDigests, DecisionSignalFallback, DecisionSignalInputMedia, DecisionSignalMode,
  DecisionSignalProbability, DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProvider,
  DecisionSignalProviderCapability, DecisionSignalProviderFuture, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalProviderRequest, DecisionSignalProviderResult, DecisionSignalPurpose,
  DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind,
  DecisionSignalRequest, DecisionSignalRequestId, DecisionSignalRouteSet, DecisionSignalThreshold, ExactSubject,
  ExternalWorkIdentity, FactoryArtifactReference, FactoryConfigurationId, FactoryConfigurationRef,
  FactoryConfigurationVersion, FactoryDigest, FactoryKey, FactoryMetadata, FactoryText, FlowNodeInput,
  ImmutableReference, NodeAttempt, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId,
  WorkPriority,
};
use std::sync::Arc;

#[test]
fn registry_replaces_exact_models_without_factory_core_changes() {
  let old_request = request("fixture-model-20260917", DecisionSignalMode::Shadow);
  let new_request = request("fixture-model-20261001", DecisionSignalMode::Shadow);
  let old: Arc<dyn DecisionSignalProvider> = Arc::new(FakeProvider {
    capability: capability(&old_request),
  });
  let new: Arc<dyn DecisionSignalProvider> = Arc::new(FakeProvider {
    capability: capability(&new_request),
  });
  let registry = DecisionSignalProviderRegistry::try_new([old, new]).unwrap();

  assert_eq!(registry.len(), 2);
  assert_eq!(
    registry.resolve(&old_request).unwrap().capability().model(),
    old_request.model()
  );
  assert_eq!(
    registry.resolve(&new_request).unwrap().capability().model(),
    new_request.model()
  );
}

struct FakeProvider {
  capability: DecisionSignalProviderCapability,
}

impl DecisionSignalProvider for FakeProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }

  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    Box::pin(async move { Ok(provider_result(request)) })
  }
}

fn request(model_version: &str, mode: DecisionSignalMode) -> DecisionSignalProviderRequest {
  let node = node_fixture();
  let choices = DecisionSignalChoices::try_new(vec![key("left"), key("right")]).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::Routing,
    provider: exact("fixture-provider", "v1", 3),
    adapter: exact("fixture-adapter", "v1", 4),
    model: ImmutableReference::new(key("fixture-model"), key(model_version), digest(5)),
    question_set: exact("questions", "v1", 6),
    policy: exact("policy", "v1", 7),
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
        exact("route", "v1", 8),
        FactoryText::new("Choose the route that should receive this work.").unwrap(),
        DecisionSignalQuestionDomain::FiniteChoice(choices),
        DecisionSignalQuestionCriteria::FiniteChoice(vec![
          DecisionSignalChoiceCriterion::new(
            key("left"),
            FactoryText::new("Use the left route for the first declared case.").unwrap(),
          ),
          DecisionSignalChoiceCriterion::new(
            key("right"),
            FactoryText::new("Use the right route for the second declared case.").unwrap(),
          ),
        ]),
      )
      .unwrap(),
    ],
    key("route"),
    Some(key("current-state")),
  )
  .unwrap();
  let capability = capability_for_profile(&profile);
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &node.0,
    &node.1,
    DecisionSignalPurpose::Routing,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  )
  .expect("fixture signal request");
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      DecisionSignalPurpose::Routing,
      capability.probability_semantics().clone(),
      exact("calibration", "v1", 10),
      DecisionSignalProbability::new(600_000).unwrap(),
      DecisionSignalProbability::new(100_000).unwrap(),
    )
    .unwrap(),
    Timestamp::from_unix_millis(10_000).unwrap(),
  )
  .unwrap()
}

fn capability(request: &DecisionSignalProviderRequest) -> DecisionSignalProviderCapability {
  DecisionSignalProviderCapability::new(
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    request.data_handling().clone(),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(4_096, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn capability_for_profile(profile: &DecisionSignalProfile) -> DecisionSignalProviderCapability {
  DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    exact("fixture-probability", "v1", 9),
    exact("provider-private", "v1", 11),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(4_096, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn provider_result(request: &DecisionSignalProviderRequest) -> DecisionSignalProviderResult {
  DecisionSignalProviderResult::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    vec![DecisionSignalAnswer::new(
      request.input().questions()[0].reference().clone(),
      DecisionSignalAnswerValue::FiniteChoice {
        selected: key("right"),
        selected_probability: DecisionSignalProbability::new(900_000).unwrap(),
        runner_up_probability: DecisionSignalProbability::new(100_000).unwrap(),
      },
    )],
    BudgetUsage {
      attempts: 1,
      elapsed_millis: 7,
      tokens: 10,
      cost_micro_units: 0,
      output_bytes: 100,
    },
  )
  .unwrap()
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
