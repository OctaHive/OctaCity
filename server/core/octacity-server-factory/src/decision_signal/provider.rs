use std::{future::Future, pin::Pin};

use super::{
  DecisionSignalProviderCapability, DecisionSignalProviderFailure, DecisionSignalProviderObservation,
  DecisionSignalProviderRequest, DecisionSignalProviderResult,
};

/// Boxed provider future used by the provider-neutral dispatch seam.
pub type DecisionSignalProviderFuture<'a> =
  Pin<Box<dyn Future<Output = Result<DecisionSignalProviderResult, DecisionSignalProviderFailure>> + Send + 'a>>;

/// Read-only recovery of a previously dispatched evaluation, without creating
/// another evaluation or reserving additional external usage.
pub type DecisionSignalProviderObservationFuture<'a> =
  Pin<Box<dyn Future<Output = Option<DecisionSignalProviderObservation>> + Send + 'a>>;

/// Provider-neutral Decision Signal seam implemented by infrastructure adapters.
pub trait DecisionSignalProvider: Send + Sync {
  /// Returns the exact immutable capability selected before request dispatch.
  fn capability(&self) -> &DecisionSignalProviderCapability;

  /// Declares every actual transport destination for installation qualification.
  /// `None` means the transport has no qualified network declaration; an empty
  /// slice explicitly promises that evaluation and recovery perform no network
  /// I/O. Adapters must derive destinations from their trusted configuration,
  /// independently of provider identities and caller-supplied request metadata.
  fn network_destinations(&self) -> Option<&[crate::NetworkHost]> {
    None
  }

  /// Evaluates one canonical bounded request without changing Factory state.
  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a>;

  /// Reads an authoritative measured outcome for this exact immutable request.
  /// This operation must never evaluate the request, even if it is unknown to
  /// the provider. Unsupported or uncertain recovery retains the pending usage.
  fn observe_evaluation<'a>(
    &'a self,
    _request: &'a DecisionSignalProviderRequest,
  ) -> DecisionSignalProviderObservationFuture<'a> {
    Box::pin(async { None })
  }
}
