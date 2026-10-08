use std::sync::{Arc, Mutex};

use super::{DecisionSignalProviderRegistry, JevDecisionSignalProvider};
use crate::jev::{JevHttpResponse, JevTransport, JevTransportError, JevTransportFuture};
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalDigests, DecisionSignalFallback, DecisionSignalInputMedia, DecisionSignalMode,
  DecisionSignalProbability, DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProvider,
  DecisionSignalProviderCapability, DecisionSignalProviderFuture, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalProviderRequest, DecisionSignalProviderResult, DecisionSignalPurpose,
  DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind,
  DecisionSignalRequest, DecisionSignalRequestId, DecisionSignalRouteSet, DecisionSignalState, DecisionSignalThreshold,
  ExactSubject, ExternalWorkIdentity, FactoryArtifactReference, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership,
  FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryDigest, FactoryKey,
  FactoryMetadata, FactoryRun, FactoryRunId, FactoryText, ImmutableReference, RiskClass, StageAttempt, StageAttemptId,
  StageAttemptNumber, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};

#[tokio::test]
async fn jev_and_fake_adapters_satisfy_one_provider_neutral_contract() {
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let response = jev_response();
  let transport = Arc::new(StubTransport::new(response));
  let jev = JevDecisionSignalProvider::for_test(
    "https://jev-ai.org/api/v1/systemone/",
    capability(&request),
    transport.clone(),
  )
  .unwrap();
  assert_provider_conformance(&jev, &request).await;
  let captured = transport.captured();
  assert_eq!(captured.idempotency_key, request.id().as_uuid().to_string());
  assert!(
    String::from_utf8(captured.body)
      .unwrap()
      .contains("\"instructions\":\"Choose the route that should receive this work.\"")
  );

  let fake = FakeProvider {
    capability: capability(&request),
  };
  assert_provider_conformance(&fake, &request).await;
}

#[tokio::test]
async fn jev_rejects_probability_keys_that_do_not_exactly_match_the_declared_domain() {
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let response = jev_response().replace("\"left\":0.1", "\"other\":0.1");
  let jev = JevDecisionSignalProvider::for_test(
    "https://jev-ai.org/api/v1/systemone/",
    capability(&request),
    Arc::new(StubTransport::new(response)),
  )
  .unwrap();

  let failure = jev.evaluate(&request).await.unwrap_err();

  assert_eq!(failure.state(), DecisionSignalState::Invalid);
}

#[tokio::test]
async fn jev_rejects_a_conflict_status_url_outside_the_pinned_origin() {
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let transport = Arc::new(ConflictTransport::with_status_url(
    jev_response(),
    "https://attacker.invalid/api/v1/requests/dec_fixture/",
  ));
  let jev = JevDecisionSignalProvider::for_test(
    "https://jev-ai.org/api/v1/systemone/",
    capability(&request),
    transport.clone(),
  )
  .unwrap();

  let failure = jev.evaluate(&request).await.unwrap_err();

  assert_eq!(failure.state(), DecisionSignalState::Invalid);
  assert_eq!(transport.posts(), 1);
  assert!(transport.observed_url.lock().unwrap().is_none());
}

#[tokio::test]
async fn jev_idempotency_conflict_observes_the_original_request_instead_of_reposting() {
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let transport = Arc::new(ConflictTransport::new(jev_response()));
  let jev = JevDecisionSignalProvider::for_test(
    "https://jev-ai.org/api/v1/systemone/",
    capability(&request),
    transport.clone(),
  )
  .unwrap();

  assert_provider_conformance(&jev, &request).await;

  assert_eq!(transport.posts(), 1);
  assert_eq!(
    transport.observed_url(),
    "https://jev-ai.org/api/v1/requests/dec_fixture/"
  );
}

#[test]
fn registry_replaces_exact_models_without_factory_core_changes() {
  let old_request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let new_request = request("jev-1.13-20261001", DecisionSignalMode::Shadow);
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

async fn assert_provider_conformance(provider: &dyn DecisionSignalProvider, request: &DecisionSignalProviderRequest) {
  let capability = provider.capability();
  assert_eq!(capability.provider(), request.provider());
  assert_eq!(capability.adapter(), request.adapter());
  assert_eq!(capability.model(), request.model());
  assert!(capability.supports(request.request().purpose()));

  let result = provider.evaluate(request).await.unwrap();
  assert_eq!(result.request_id(), request.id());
  assert_eq!(result.provider(), request.provider());
  assert_eq!(result.adapter(), request.adapter());
  assert_eq!(result.model(), request.model());
  assert_eq!(result.probability_semantics(), request.probability_semantics());
  assert_eq!(result.answers().len(), request.input().questions().len());
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

#[derive(Clone)]
struct CapturedRequest {
  idempotency_key: String,
  body: Vec<u8>,
}

struct StubTransport {
  response: Mutex<Option<Vec<u8>>>,
  captured: Mutex<Option<CapturedRequest>>,
}

impl StubTransport {
  fn new(response: String) -> Self {
    Self {
      response: Mutex::new(Some(response.into_bytes())),
      captured: Mutex::new(None),
    }
  }

  fn captured(&self) -> CapturedRequest {
    self.captured.lock().unwrap().clone().unwrap()
  }
}

impl JevTransport for StubTransport {
  fn post<'a>(
    &'a self,
    _endpoint: &'a reqwest::Url,
    idempotency_key: &'a str,
    body: Vec<u8>,
  ) -> JevTransportFuture<'a> {
    *self.captured.lock().unwrap() = Some(CapturedRequest {
      idempotency_key: idempotency_key.to_owned(),
      body,
    });
    let response = self.response.lock().unwrap().take().unwrap();
    Box::pin(async move {
      Ok(JevHttpResponse {
        status: reqwest::StatusCode::OK,
        body: response,
      })
    })
  }

  fn get<'a>(&'a self, _endpoint: &'a reqwest::Url) -> JevTransportFuture<'a> {
    Box::pin(async { Err(JevTransportError::Unavailable) })
  }
}

struct ConflictTransport {
  response: Mutex<Option<Vec<u8>>>,
  status_url: String,
  posts: Mutex<u32>,
  observed_url: Mutex<Option<String>>,
}

impl ConflictTransport {
  fn new(response: String) -> Self {
    Self::with_status_url(response, "https://jev-ai.org/api/v1/requests/dec_fixture/")
  }

  fn with_status_url(response: String, status_url: &str) -> Self {
    Self {
      response: Mutex::new(Some(response.into_bytes())),
      status_url: status_url.to_owned(),
      posts: Mutex::new(0),
      observed_url: Mutex::new(None),
    }
  }

  fn posts(&self) -> u32 {
    *self.posts.lock().unwrap()
  }

  fn observed_url(&self) -> String {
    self.observed_url.lock().unwrap().clone().unwrap()
  }
}

impl JevTransport for ConflictTransport {
  fn post<'a>(
    &'a self,
    _endpoint: &'a reqwest::Url,
    _idempotency_key: &'a str,
    _body: Vec<u8>,
  ) -> JevTransportFuture<'a> {
    *self.posts.lock().unwrap() += 1;
    let body = format!(
      "{{\"error\":{{\"code\":\"request_already_completed\"}},\"request_id\":\"dec_fixture\",\"status_url\":{:?}}}",
      self.status_url
    );
    Box::pin(async move {
      Ok(JevHttpResponse {
        status: reqwest::StatusCode::CONFLICT,
        body: body.into_bytes(),
      })
    })
  }

  fn get<'a>(&'a self, endpoint: &'a reqwest::Url) -> JevTransportFuture<'a> {
    *self.observed_url.lock().unwrap() = Some(endpoint.to_string());
    let response = self.response.lock().unwrap().take().unwrap();
    Box::pin(async move {
      Ok(JevHttpResponse {
        status: reqwest::StatusCode::OK,
        body: response,
      })
    })
  }
}

fn jev_response() -> String {
  "{\"id\":\"dec_fixture\",\"model\":\"jev-1.13\",\"model_version\":\"jev-1.13-20260917\",\"provider\":\"jev-ai.org\",\"answers\":{\"route\":{\"type\":\"choice\",\"choice\":\"right\",\"probabilities\":{\"left\":0.1,\"right\":0.9},\"confidence\":0.9}},\"usage\":{\"input_tokens\":8,\"output_tokens\":2,\"charged_tokens\":8,\"charged_credits\":0,\"wallet\":\"tokens\"},\"latency_ms\":7}".to_owned()
}

fn request(model_version: &str, mode: DecisionSignalMode) -> DecisionSignalProviderRequest {
  let stage = stage();
  let choices = DecisionSignalChoices::try_new(vec![key("left"), key("right")]).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::Routing,
    provider: exact("jev", "v1", 3),
    adapter: exact("jev-http", "v1", 4),
    model: ImmutableReference::new(key("jev-1.13"), key(model_version), digest(5)),
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
    &stage,
    DecisionSignalPurpose::Routing,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  );
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
    exact("jev-choice-probability", "v1", 9),
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

fn stage() -> StageAttempt {
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
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  StageAttempt::new(
    StageAttemptId::generate(),
    &run,
    StageAttemptNumber::INITIAL,
    octacity_server_factory::FactoryStageTarget::Implementation,
    budget(),
    digest(2),
    FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(digest(12)),
        Timestamp::from_unix_millis(1).unwrap(),
        Timestamp::from_unix_millis(2).unwrap(),
      )
      .unwrap(),
    ),
  )
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
