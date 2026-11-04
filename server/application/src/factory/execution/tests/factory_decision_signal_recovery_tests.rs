use super::*;
use octacity_server_factory::{DecisionSignalProviderObservation, DecisionSignalProviderObservationFuture};

#[test]
fn foreign_provider_outcomes_cannot_settle_the_current_request() {
  run_ready(async {
    for (failed, late) in [(false, false), (true, false), (false, true), (true, true)] {
      let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
      let unrelated = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
      let clock = Arc::new(MutableClock::new(100));
      let provider = CountingProvider::successful_with_completion_time(
        &request,
        "right",
        clock.clone(),
        if late {
          request.deadline().unix_millis() + 1
        } else {
          100
        },
      );
      *provider.result.lock().unwrap() = Some(if failed {
        Err(
          DecisionSignalProviderFailure::new(
            unrelated.id(),
            unrelated.provider().clone(),
            unrelated.adapter().clone(),
            unrelated.model().clone(),
            DecisionSignalState::Unavailable,
            result(&unrelated, "right").usage(),
          )
          .unwrap(),
        )
      } else {
        Ok(result(&unrelated, "right"))
      });
      let provider = Arc::new(provider);
      let receipts = Arc::new(MemoryReceipts::default());
      let service = DecisionSignalService::for_provider_with_clock(
        provider.clone(),
        receipts.clone(),
        receipts.clone(),
        receipts.clone(),
        clock,
      );
      assert!(
        matches!(
          service
            .route(DecisionSignalReceiptId::generate(), &request, key("left"))
            .await,
          Err(DecisionSignalApplicationError::AwaitingProviderObservation)
        ),
        "response timing cannot conceal a different evaluation's identity or usage"
      );
      assert_eq!(receipts.len(), 0);
      assert_eq!(provider.dispatches(), 1);
      assert!(
        crate::DecisionSignalDispatchAuthorization::was_reserved(receipts.as_ref(), &request)
          .await
          .unwrap()
      );
    }
  });
}

struct RecoveringProvider {
  capability: DecisionSignalProviderCapability,
  evaluations: Mutex<u32>,
  observations: Mutex<u32>,
  outcome: DecisionSignalProviderObservation,
}
impl DecisionSignalProvider for RecoveringProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }
  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    *self.evaluations.lock().unwrap() += 1;
    Box::pin(async move {
      Err(
        DecisionSignalProviderFailure::unobserved(
          request.id(),
          request.provider().clone(),
          request.adapter().clone(),
          request.model().clone(),
          DecisionSignalState::Unavailable,
        )
        .unwrap(),
      )
    })
  }
  fn observe_evaluation<'a>(
    &'a self,
    request: &'a DecisionSignalProviderRequest,
  ) -> DecisionSignalProviderObservationFuture<'a> {
    *self.observations.lock().unwrap() += 1;
    let _ = request;
    Box::pin(async move { Some(self.outcome.clone()) })
  }
}

#[test]
fn a_reserved_signal_can_settle_late_by_readonly_observation_without_evaluating_again() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(RecoveringProvider {
      capability: capability(&request),
      evaluations: Mutex::new(0),
      observations: Mutex::new(0),
      outcome: DecisionSignalProviderObservation::Result(result(&request, "right")),
    });
    let receipts = Arc::new(MemoryReceipts::default());
    let clock = Arc::new(MutableClock::new(100));
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts.clone(),
      clock.clone(),
    );
    assert!(matches!(
      service
        .route(DecisionSignalReceiptId::generate(), &request, key("left"))
        .await,
      Err(DecisionSignalApplicationError::AwaitingProviderObservation)
    ));
    assert_eq!(receipts.len(), 0);
    clock.set(request.deadline().unix_millis() + 1);
    let recovered = service
      .route(DecisionSignalReceiptId::generate(), &request, key("left"))
      .await
      .unwrap();
    assert_eq!(recovered.state(), DecisionSignalState::TimedOut);
    assert_eq!(
      recovered.usage().tokens,
      10,
      "late settlement retains measured external usage"
    );
    assert_eq!(
      recovered.consumption().final_disposition(),
      &DecisionSignalDisposition::Escalate
    );
    let replayed = service
      .route(DecisionSignalReceiptId::generate(), &request, key("left"))
      .await
      .unwrap();
    assert_eq!(recovered.receipt_digest(), replayed.receipt_digest());
    assert_eq!(
      *provider.evaluations.lock().unwrap(),
      1,
      "recovery cannot renew dispatch authority"
    );
    assert_eq!(*provider.observations.lock().unwrap(), 1);
    assert_eq!(receipts.len(), 1);
  });
}

#[test]
fn an_observation_for_another_request_cannot_release_unknown_reserved_usage() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let unrelated = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(RecoveringProvider {
      capability: capability(&request),
      evaluations: Mutex::new(0),
      observations: Mutex::new(0),
      outcome: DecisionSignalProviderObservation::Result(result(&unrelated, "right")),
    });
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts.clone(),
      Arc::new(FixedClock(Timestamp::from_unix_millis(100).unwrap())),
    );
    for _ in 0..2 {
      assert!(
        matches!(
          service
            .route(DecisionSignalReceiptId::generate(), &request, key("left"))
            .await,
          Err(DecisionSignalApplicationError::AwaitingProviderObservation)
        ),
        "a different evaluation cannot provide measured usage for this pending request"
      );
    }
    assert_eq!(receipts.len(), 0);
    assert_eq!(*provider.evaluations.lock().unwrap(), 1);
    assert_eq!(*provider.observations.lock().unwrap(), 1);
  });
}
