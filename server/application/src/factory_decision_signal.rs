use std::{
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  BudgetUsage, DecisionSignalProvider, DecisionSignalProviderCapability, DecisionSignalProviderFailure,
  DecisionSignalProviderObservation, DecisionSignalProviderRequest, DecisionSignalReceipt, DecisionSignalReceiptId,
  DecisionSignalState, FactoryError, FactoryKey, ToolRiskChoiceMapping, consume_routing_signal,
  consume_tool_risk_signal,
};
use thiserror::Error;

/// Stable secret-free failure shared by Decision Signal application ports.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DecisionSignalPortError {
  /// The operation is temporarily unavailable and may be observed again.
  #[error("Decision Signal operation is unavailable")]
  Unavailable,
  /// Immutable state conflicts with the attempted publication.
  #[error("Decision Signal operation conflicts with immutable state")]
  Conflict,
}

/// Discovers the exact immutable capability selected for one request.
#[async_trait]
pub trait DecisionSignalCapabilityDiscovery: Send + Sync {
  /// Returns the exact provider, adapter, model, limits, and semantics.
  async fn discover_capability(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<DecisionSignalProviderCapability, DecisionSignalPortError>;
}

/// Observes a previously published receipt by stable logical request identity.
#[async_trait]
pub trait DecisionSignalRequestObservation: Send + Sync {
  /// Returns the immutable receipt when the logical request is already terminal.
  async fn observe_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalPortError>;
}

/// Dispatches one exact canonical request without consuming its result.
#[async_trait]
pub trait DecisionSignalDispatch: Send + Sync {
  /// Returns a typed provider result or a terminal secret-safe failure.
  async fn dispatch_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<octacity_server_factory::DecisionSignalProviderResult, DecisionSignalProviderFailure>;
}

/// Atomically publishes one immutable consumed Decision Signal receipt.
#[async_trait]
pub trait DecisionSignalReceiptPublication: Send + Sync {
  /// Publishes or exactly replays the receipt for its stable logical request.
  async fn publish_receipt(
    &self,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalPortError>;
}

#[async_trait]
impl<T> DecisionSignalCapabilityDiscovery for T
where
  T: DecisionSignalProvider + ?Sized,
{
  async fn discover_capability(
    &self,
    _request: &DecisionSignalProviderRequest,
  ) -> Result<DecisionSignalProviderCapability, DecisionSignalPortError> {
    Ok(self.capability().clone())
  }
}

#[async_trait]
impl<T> DecisionSignalDispatch for T
where
  T: DecisionSignalProvider + ?Sized,
{
  async fn dispatch_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<octacity_server_factory::DecisionSignalProviderResult, DecisionSignalProviderFailure> {
    self.evaluate(request).await
  }
}

/// Failure while coordinating observation, provider dispatch, consumption, and publication.
#[derive(Debug, Error)]
pub enum DecisionSignalApplicationError {
  /// The authoritative wall clock could not be represented safely.
  #[error("Decision Signal clock is unavailable")]
  ClockUnavailable,
  /// Durable receipt observation failed before safe dispatch or replay.
  #[error("Decision Signal receipt observation failed")]
  Observation(#[source] DecisionSignalPortError),
  /// Pure receipt construction rejected inconsistent application inputs.
  #[error("Decision Signal receipt construction failed")]
  Consumption(#[source] FactoryError),
  /// The immutable receipt could neither be published nor recovered by observation.
  #[error("Decision Signal receipt publication failed")]
  Publication(#[source] DecisionSignalPortError),
}

/// Application coordinator for one exact Decision Signal provider selection.
///
/// The coordinator always observes durable state before provider dispatch and
/// after an uncertain publication. Provider output is consumed only by the
/// pure Factory policy, so adapters cannot commit lifecycle transitions.
pub struct DecisionSignalService {
  capabilities: Arc<dyn DecisionSignalCapabilityDiscovery>,
  observations: Arc<dyn DecisionSignalRequestObservation>,
  dispatcher: Arc<dyn DecisionSignalDispatch>,
  publisher: Arc<dyn DecisionSignalReceiptPublication>,
  clock: Arc<dyn DecisionSignalClock>,
}

impl DecisionSignalService {
  /// Binds operation-shaped ports for one resolved exact provider selection.
  #[must_use]
  pub fn new(
    capabilities: Arc<dyn DecisionSignalCapabilityDiscovery>,
    observations: Arc<dyn DecisionSignalRequestObservation>,
    dispatcher: Arc<dyn DecisionSignalDispatch>,
    publisher: Arc<dyn DecisionSignalReceiptPublication>,
  ) -> Self {
    Self::with_clock(
      capabilities,
      observations,
      dispatcher,
      publisher,
      Arc::new(SystemDecisionSignalClock),
    )
  }

  fn with_clock(
    capabilities: Arc<dyn DecisionSignalCapabilityDiscovery>,
    observations: Arc<dyn DecisionSignalRequestObservation>,
    dispatcher: Arc<dyn DecisionSignalDispatch>,
    publisher: Arc<dyn DecisionSignalReceiptPublication>,
    clock: Arc<dyn DecisionSignalClock>,
  ) -> Self {
    Self {
      capabilities,
      observations,
      dispatcher,
      publisher,
      clock,
    }
  }

  /// Binds one provider resolved by an infrastructure registry to the
  /// application operations while keeping the registry out of this layer.
  #[must_use]
  pub fn for_provider(
    provider: Arc<dyn DecisionSignalProvider>,
    observations: Arc<dyn DecisionSignalRequestObservation>,
    publisher: Arc<dyn DecisionSignalReceiptPublication>,
  ) -> Self {
    let operations = Arc::new(ResolvedProviderOperations(provider));
    Self::new(operations.clone(), observations, operations, publisher)
  }

  #[cfg(test)]
  pub(super) fn for_provider_with_clock(
    provider: Arc<dyn DecisionSignalProvider>,
    observations: Arc<dyn DecisionSignalRequestObservation>,
    publisher: Arc<dyn DecisionSignalReceiptPublication>,
    clock: Arc<dyn DecisionSignalClock>,
  ) -> Self {
    let operations = Arc::new(ResolvedProviderOperations(provider));
    Self::with_clock(operations.clone(), observations, operations, publisher, clock)
  }

  /// Obtains and consumes a routing signal against one declared baseline route.
  pub async fn route(
    &self,
    receipt_id: DecisionSignalReceiptId,
    request: &DecisionSignalProviderRequest,
    baseline_route: FactoryKey,
  ) -> Result<DecisionSignalReceipt, DecisionSignalApplicationError> {
    self
      .execute(request, |observation| {
        consume_routing_signal(receipt_id, request, observation, baseline_route)
      })
      .await
  }

  /// Obtains and consumes a tool-risk signal after deterministic authorization.
  pub async fn assess_tool_risk(
    &self,
    receipt_id: DecisionSignalReceiptId,
    request: &DecisionSignalProviderRequest,
    mapping: &ToolRiskChoiceMapping,
  ) -> Result<DecisionSignalReceipt, DecisionSignalApplicationError> {
    self
      .execute(request, |observation| {
        consume_tool_risk_signal(receipt_id, request, observation, mapping)
      })
      .await
  }

  async fn execute(
    &self,
    request: &DecisionSignalProviderRequest,
    consume: impl FnOnce(DecisionSignalProviderObservation) -> Result<DecisionSignalReceipt, FactoryError>,
  ) -> Result<DecisionSignalReceipt, DecisionSignalApplicationError> {
    if let Some(receipt) = self.observe_exact(request).await? {
      return Ok(receipt);
    }

    if self.now()? >= request.deadline() {
      let receipt = consume(unavailable_observation(request, DecisionSignalState::TimedOut))
        .map_err(DecisionSignalApplicationError::Consumption)?;
      return self.publish_or_observe(request, receipt).await;
    }

    let observation = match self.capabilities.discover_capability(request).await {
      Ok(capability) if capability_matches(request, &capability) => {
        if self.now()? >= request.deadline() {
          unavailable_observation(request, DecisionSignalState::TimedOut)
        } else {
          match self.dispatcher.dispatch_request(request).await {
            Ok(result) if self.now()? < request.deadline() => DecisionSignalProviderObservation::Result(result),
            Ok(result) => timed_out_observation(request, result.usage()),
            Err(failure) if self.now()? < request.deadline() => DecisionSignalProviderObservation::Failure(failure),
            Err(failure) => timed_out_observation(request, failure.usage()),
          }
        }
      }
      Ok(_) => unavailable_observation(request, DecisionSignalState::Invalid),
      Err(_) => unavailable_observation(request, DecisionSignalState::Unavailable),
    };
    let receipt = consume(observation).map_err(DecisionSignalApplicationError::Consumption)?;
    self.publish_or_observe(request, receipt).await
  }

  async fn publish_or_observe(
    &self,
    request: &DecisionSignalProviderRequest,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalApplicationError> {
    match self.publisher.publish_receipt(receipt).await {
      Ok(published) => {
        published
          .replay(request)
          .map_err(DecisionSignalApplicationError::Consumption)?;
        Ok(published)
      }
      Err(error) => match self.observe_exact(request).await? {
        Some(published) => Ok(published),
        None => Err(DecisionSignalApplicationError::Publication(error)),
      },
    }
  }

  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError> {
    self.clock.now()
  }

  async fn observe_exact(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalApplicationError> {
    let receipt = self
      .observations
      .observe_request(request)
      .await
      .map_err(DecisionSignalApplicationError::Observation)?;
    receipt
      .map(|receipt| {
        receipt
          .replay(request)
          .map_err(DecisionSignalApplicationError::Consumption)?;
        Ok(receipt)
      })
      .transpose()
  }
}

pub(super) trait DecisionSignalClock: Send + Sync {
  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError>;
}

struct SystemDecisionSignalClock;

impl DecisionSignalClock for SystemDecisionSignalClock {
  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError> {
    let milliseconds = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .ok()
      .and_then(|duration| i64::try_from(duration.as_millis()).ok())
      .ok_or(DecisionSignalApplicationError::ClockUnavailable)?;
    Timestamp::from_unix_millis(milliseconds).map_err(|_| DecisionSignalApplicationError::ClockUnavailable)
  }
}

struct ResolvedProviderOperations(Arc<dyn DecisionSignalProvider>);

#[async_trait]
impl DecisionSignalCapabilityDiscovery for ResolvedProviderOperations {
  async fn discover_capability(
    &self,
    _request: &DecisionSignalProviderRequest,
  ) -> Result<DecisionSignalProviderCapability, DecisionSignalPortError> {
    Ok(self.0.capability().clone())
  }
}

#[async_trait]
impl DecisionSignalDispatch for ResolvedProviderOperations {
  async fn dispatch_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<octacity_server_factory::DecisionSignalProviderResult, DecisionSignalProviderFailure> {
    self.0.evaluate(request).await
  }
}

fn capability_matches(request: &DecisionSignalProviderRequest, capability: &DecisionSignalProviderCapability) -> bool {
  request.provider() == capability.provider()
    && request.adapter() == capability.adapter()
    && request.model() == capability.model()
    && request.probability_semantics() == capability.probability_semantics()
    && capability.supports(request.request().purpose())
}

fn unavailable_observation(
  request: &DecisionSignalProviderRequest,
  state: DecisionSignalState,
) -> DecisionSignalProviderObservation {
  DecisionSignalProviderObservation::Failure(
    DecisionSignalProviderFailure::new(
      request.id(),
      request.provider().clone(),
      request.adapter().clone(),
      request.model().clone(),
      state,
      BudgetUsage::default(),
    )
    .expect("non-completed Decision Signal failures are valid"),
  )
}

fn timed_out_observation(
  request: &DecisionSignalProviderRequest,
  usage: BudgetUsage,
) -> DecisionSignalProviderObservation {
  DecisionSignalProviderObservation::Failure(
    DecisionSignalProviderFailure::new(
      request.id(),
      request.provider().clone(),
      request.adapter().clone(),
      request.model().clone(),
      DecisionSignalState::TimedOut,
      usage,
    )
    .expect("timed-out Decision Signal failures are valid"),
  )
}
