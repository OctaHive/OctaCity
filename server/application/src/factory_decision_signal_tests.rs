use std::{
  collections::BTreeMap,
  future::Future,
  sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
  },
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalConsumptionKind, DecisionSignalDigests, DecisionSignalDisposition,
  DecisionSignalFallback, DecisionSignalInputMedia, DecisionSignalMode, DecisionSignalProbability,
  DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProvider, DecisionSignalProviderCapability,
  DecisionSignalProviderFailure, DecisionSignalProviderFuture, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalProviderRequest, DecisionSignalProviderResult, DecisionSignalPurpose,
  DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind,
  DecisionSignalReceipt, DecisionSignalReceiptId, DecisionSignalRequest, DecisionSignalRequestId,
  DecisionSignalRouteSet, DecisionSignalState, DecisionSignalThreshold, ExactSubject, ExternalWorkIdentity,
  FactoryClaim, FactoryClaimFence, FactoryClaimOwnership, FactoryConfigurationId, FactoryConfigurationRef,
  FactoryConfigurationVersion, FactoryDigest, FactoryKey, FactoryMetadata, FactoryRun, FactoryRunId, FactoryText,
  ImmutableReference, RiskClass, StageAttempt, StageAttemptId, StageAttemptNumber, ToolRiskChoiceMapping,
  WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};

use super::factory_decision_signal::{
  DecisionSignalApplicationError, DecisionSignalClock, DecisionSignalPortError, DecisionSignalReceiptPublication,
  DecisionSignalRequestObservation, DecisionSignalService,
};

#[test]
fn exact_replay_observes_the_receipt_without_requerying_or_switching_models() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = service(provider.clone(), receipts.clone());
    let receipt_id = DecisionSignalReceiptId::generate();

    let first = service.route(receipt_id, &request, key("left")).await.unwrap();
    let replay = service.route(receipt_id, &request, key("left")).await.unwrap();

    assert_eq!(first, replay);
    assert_eq!(provider.dispatches(), 1);
    assert_eq!(replay.model(), request.model());
    assert_eq!(receipts.len(), 1);
  });
}

#[test]
fn provider_failure_is_purpose_scoped() {
  run_ready(async {
    let routing = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let routing_provider = Arc::new(CountingProvider::failing(&routing));
    let routing_receipts = Arc::new(MemoryReceipts::default());
    let failed = service(routing_provider, routing_receipts)
      .route(DecisionSignalReceiptId::generate(), &routing, key("left"))
      .await
      .unwrap();
    assert_eq!(failed.state(), DecisionSignalState::Unavailable);
    assert_eq!(failed.request().request().purpose(), DecisionSignalPurpose::Routing);

    let tool_risk = provider_request(DecisionSignalPurpose::ToolRisk, DecisionSignalMode::BoundedControl);
    let tool_provider = Arc::new(CountingProvider::successful(&tool_risk, "right"));
    let tool_receipts = Arc::new(MemoryReceipts::default());
    let mapping = ToolRiskChoiceMapping::try_new(
      tool_risk.choices(),
      vec![
        (key("left"), DecisionSignalDisposition::Allow),
        (key("right"), DecisionSignalDisposition::Deny),
      ],
    )
    .unwrap();
    let assessed = service(tool_provider, tool_receipts)
      .assess_tool_risk(DecisionSignalReceiptId::generate(), &tool_risk, &mapping)
      .await
      .unwrap();
    assert_eq!(assessed.state(), DecisionSignalState::Completed);
    assert_eq!(assessed.request().request().purpose(), DecisionSignalPurpose::ToolRisk);
    assert_eq!(
      assessed.consumption().final_disposition(),
      &DecisionSignalDisposition::Deny
    );
  });
}

#[test]
fn shadow_never_changes_execution() {
  run_ready(async {
    let shadow = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::Shadow);
    let shadow_provider = Arc::new(CountingProvider::successful(&shadow, "right"));
    let shadow_receipts = Arc::new(MemoryReceipts::default());
    let compared = service(shadow_provider, shadow_receipts)
      .route(DecisionSignalReceiptId::generate(), &shadow, key("left"))
      .await
      .unwrap();
    assert_eq!(compared.consumption().kind(), DecisionSignalConsumptionKind::Shadow);
    assert_eq!(
      compared.consumption().final_disposition(),
      &DecisionSignalDisposition::Route(key("left"))
    );
  });
}

#[test]
fn expired_requests_publish_a_timeout_without_contacting_the_provider() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts,
      Arc::new(FixedClock(request.deadline())),
    );

    let receipt = service
      .route(DecisionSignalReceiptId::generate(), &request, key("left"))
      .await
      .unwrap();

    assert_eq!(receipt.state(), DecisionSignalState::TimedOut);
    assert_eq!(provider.dispatches(), 0);
  });
}

#[test]
fn provider_results_completed_after_the_deadline_are_consumed_as_timeouts() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let clock = Arc::new(MutableClock::new(100));
    let provider = Arc::new(CountingProvider::successful_with_completion_time(
      &request,
      "right",
      clock.clone(),
      request.deadline().unix_millis(),
    ));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(provider.clone(), receipts.clone(), receipts, clock);

    let receipt = service
      .route(DecisionSignalReceiptId::generate(), &request, key("left"))
      .await
      .unwrap();

    assert_eq!(receipt.state(), DecisionSignalState::TimedOut);
    assert_eq!(provider.dispatches(), 1);
    assert_eq!(receipt.usage().tokens, 10);
  });
}

fn service(provider: Arc<CountingProvider>, receipts: Arc<MemoryReceipts>) -> DecisionSignalService {
  DecisionSignalService::for_provider_with_clock(
    provider,
    receipts.clone(),
    receipts,
    Arc::new(FixedClock(Timestamp::from_unix_millis(100).unwrap())),
  )
}

struct FixedClock(Timestamp);

impl DecisionSignalClock for FixedClock {
  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError> {
    Ok(self.0)
  }
}

struct MutableClock(AtomicI64);

impl MutableClock {
  fn new(value: i64) -> Self {
    Self(AtomicI64::new(value))
  }

  fn set(&self, value: i64) {
    self.0.store(value, Ordering::Release);
  }
}

impl DecisionSignalClock for MutableClock {
  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError> {
    Ok(Timestamp::from_unix_millis(self.0.load(Ordering::Acquire)).unwrap())
  }
}

struct CountingProvider {
  capability: DecisionSignalProviderCapability,
  result: Mutex<Option<Result<DecisionSignalProviderResult, DecisionSignalProviderFailure>>>,
  dispatches: Mutex<u32>,
  completion_clock: Option<(Arc<MutableClock>, i64)>,
}

impl CountingProvider {
  fn successful(request: &DecisionSignalProviderRequest, selected: &str) -> Self {
    Self {
      capability: capability(request),
      result: Mutex::new(Some(Ok(result(request, selected)))),
      dispatches: Mutex::new(0),
      completion_clock: None,
    }
  }

  fn successful_with_completion_time(
    request: &DecisionSignalProviderRequest,
    selected: &str,
    clock: Arc<MutableClock>,
    completion_time: i64,
  ) -> Self {
    Self {
      capability: capability(request),
      result: Mutex::new(Some(Ok(result(request, selected)))),
      dispatches: Mutex::new(0),
      completion_clock: Some((clock, completion_time)),
    }
  }

  fn failing(request: &DecisionSignalProviderRequest) -> Self {
    Self {
      capability: capability(request),
      result: Mutex::new(Some(Err(
        DecisionSignalProviderFailure::new(
          request.id(),
          request.provider().clone(),
          request.adapter().clone(),
          request.model().clone(),
          DecisionSignalState::Unavailable,
          BudgetUsage::default(),
        )
        .unwrap(),
      ))),
      dispatches: Mutex::new(0),
      completion_clock: None,
    }
  }

  fn dispatches(&self) -> u32 {
    *self.dispatches.lock().unwrap()
  }
}

impl DecisionSignalProvider for CountingProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }

  fn evaluate<'a>(&'a self, _request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    *self.dispatches.lock().unwrap() += 1;
    if let Some((clock, completion_time)) = &self.completion_clock {
      clock.set(*completion_time);
    }
    let result = self.result.lock().unwrap().clone().expect("fixture result");
    Box::pin(async move { result })
  }
}

#[derive(Default)]
struct MemoryReceipts {
  values: Mutex<BTreeMap<DecisionSignalRequestId, DecisionSignalReceipt>>,
}

impl MemoryReceipts {
  fn len(&self) -> usize {
    self.values.lock().unwrap().len()
  }
}

#[async_trait]
impl DecisionSignalRequestObservation for MemoryReceipts {
  async fn observe_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalPortError> {
    Ok(self.values.lock().unwrap().get(&request.id()).cloned())
  }
}

#[async_trait]
impl DecisionSignalReceiptPublication for MemoryReceipts {
  async fn publish_receipt(
    &self,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalPortError> {
    let mut values = self.values.lock().unwrap();
    if let Some(existing) = values.get(&receipt.request_id()) {
      return (existing == &receipt)
        .then(|| existing.clone())
        .ok_or(DecisionSignalPortError::Conflict);
    }
    values.insert(receipt.request_id(), receipt.clone());
    Ok(receipt)
  }
}

fn provider_request(purpose: DecisionSignalPurpose, mode: DecisionSignalMode) -> DecisionSignalProviderRequest {
  let stage = stage();
  let choices = DecisionSignalChoices::try_new(vec![key("left"), key("right")]).unwrap();
  let routes = (purpose == DecisionSignalPurpose::Routing)
    .then(|| DecisionSignalRouteSet::new(key("current-state"), choices.clone()));
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose,
    provider: exact("provider", "v1", 3),
    adapter: exact("adapter", "v1", 4),
    model: exact("model", "v7", 5),
    question_set: exact("questions", "v2", 6),
    policy: exact("policy", "v4", 7),
    mode,
    fallback: DecisionSignalFallback::Escalate,
    budget: budget(),
    routes,
  })
  .unwrap();
  let input = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"state\":\"redacted\"}").unwrap(),
    vec![
      DecisionSignalQuestion::new(
        exact("route", "v1", 8),
        FactoryText::new("Choose the declared route that matches the redacted state.").unwrap(),
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
    (purpose == DecisionSignalPurpose::Routing).then(|| key("current-state")),
  )
  .unwrap();
  let capability = capability_for_profile(&profile);
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage,
    purpose,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  );
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      purpose,
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
    vec![request.request().purpose()],
    vec![request.input().media()],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn capability_for_profile(profile: &DecisionSignalProfile) -> DecisionSignalProviderCapability {
  DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    exact("semantics", "v1", 9),
    exact("private-data", "v1", 11),
    vec![profile.purpose()],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn result(request: &DecisionSignalProviderRequest, selected: &str) -> DecisionSignalProviderResult {
  DecisionSignalProviderResult::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    vec![DecisionSignalAnswer::new(
      request.input().questions()[0].reference().clone(),
      DecisionSignalAnswerValue::FiniteChoice {
        selected: key(selected),
        selected_probability: DecisionSignalProbability::new(800_000).unwrap(),
        runner_up_probability: DecisionSignalProbability::new(200_000).unwrap(),
      },
    )],
    BudgetUsage {
      attempts: 1,
      elapsed_millis: 5,
      tokens: 10,
      cost_micro_units: 0,
      output_bytes: 20,
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
    WorkArtifacts::new(ArtifactId::generate(), ArtifactId::generate(), vec![]).unwrap(),
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

fn exact(identity: &str, version: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(value))
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory Decision Signal future must be immediately ready"),
  }
}
