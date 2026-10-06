use std::{future::Future, pin::Pin};

use super::{
  DecisionSignalProviderCapability, DecisionSignalProviderFailure, DecisionSignalProviderRequest,
  DecisionSignalProviderResult,
};

/// Boxed provider future used by the provider-neutral dispatch seam.
pub type DecisionSignalProviderFuture<'a> =
  Pin<Box<dyn Future<Output = Result<DecisionSignalProviderResult, DecisionSignalProviderFailure>> + Send + 'a>>;

/// Provider-neutral Decision Signal seam implemented by infrastructure adapters.
pub trait DecisionSignalProvider: Send + Sync {
  /// Returns the exact immutable capability selected before request dispatch.
  fn capability(&self) -> &DecisionSignalProviderCapability;

  /// Evaluates one canonical bounded request without changing Factory state.
  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a>;
}
