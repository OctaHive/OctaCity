//! Runtime timing around the neutral connector; uncertain external usage stays reserved.
use octacity_server_application::FactoryExecutionClock;
use octacity_server_factory::{
  BudgetUsage, DecisionSignalProvider, DecisionSignalProviderCapability, DecisionSignalProviderFailure,
  DecisionSignalProviderFuture, DecisionSignalProviderObservationFuture, DecisionSignalProviderRequest,
  DecisionSignalState,
};
use std::{sync::Arc, time::Duration};

pub(super) struct BoundedFactorySignalProvider {
  inner: Arc<dyn DecisionSignalProvider>,
  clock: Arc<dyn FactoryExecutionClock>,
}

impl BoundedFactorySignalProvider {
  pub(super) fn new(inner: Arc<dyn DecisionSignalProvider>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self { inner, clock }
  }
}

impl DecisionSignalProvider for BoundedFactorySignalProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    self.inner.capability()
  }

  fn network_destinations(&self) -> Option<&[octacity_server_factory::NetworkHost]> {
    self.inner.network_destinations()
  }

  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    Box::pin(async move {
      let remaining = self
        .clock
        .now()
        .ok()
        .and_then(|now| {
          request
            .deadline()
            .unix_millis()
            .checked_sub(now.unix_millis())
            .and_then(|value| u64::try_from(value).ok())
        })
        .unwrap_or(0)
        .min(request.request().budget().max_elapsed_millis());
      if remaining == 0 {
        return Err(
          DecisionSignalProviderFailure::new(
            request.id(),
            request.provider().clone(),
            request.adapter().clone(),
            request.model().clone(),
            DecisionSignalState::TimedOut,
            BudgetUsage::default(),
          )
          .expect("timeout is a terminal failure"),
        );
      }
      match tokio::time::timeout(Duration::from_millis(remaining), self.inner.evaluate(request)).await {
        Ok(result) => result,
        Err(_) => Err(
          DecisionSignalProviderFailure::unobserved(
            request.id(),
            request.provider().clone(),
            request.adapter().clone(),
            request.model().clone(),
            DecisionSignalState::TimedOut,
          )
          .expect("timeout is a terminal failure"),
        ),
      }
    })
  }

  fn observe_evaluation<'a>(
    &'a self,
    request: &'a DecisionSignalProviderRequest,
  ) -> DecisionSignalProviderObservationFuture<'a> {
    Box::pin(async move {
      // Retained observation grants no dispatch authority. Its own finite read
      // window remains available after the original execution deadline.
      tokio::time::timeout(
        Duration::from_millis(request.request().budget().max_elapsed_millis()),
        self.inner.observe_evaluation(request),
      )
      .await
      .ok()
      .flatten()
    })
  }
}
