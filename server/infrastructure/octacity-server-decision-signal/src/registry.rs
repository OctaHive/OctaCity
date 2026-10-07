use std::{collections::BTreeMap, fmt, sync::Arc};

use octacity_server_factory::{
  DecisionSignalProvider, DecisionSignalProviderCapability, DecisionSignalProviderRequest,
};
use thiserror::Error;

/// Immutable provider registry keyed by exact provider, adapter, and model identity.
pub struct DecisionSignalProviderRegistry {
  providers: BTreeMap<ProviderKey, Arc<dyn DecisionSignalProvider>>,
}

impl DecisionSignalProviderRegistry {
  /// Builds an immutable registry and rejects duplicate exact capabilities.
  pub fn try_new(
    providers: impl IntoIterator<Item = Arc<dyn DecisionSignalProvider>>,
  ) -> Result<Self, DecisionSignalRegistryError> {
    let mut registered = BTreeMap::new();
    for provider in providers {
      let key = ProviderKey::from_capability(provider.capability());
      if registered.insert(key.clone(), provider).is_some() {
        return Err(DecisionSignalRegistryError::Duplicate { key: key.display() });
      }
    }
    Ok(Self { providers: registered })
  }

  /// Returns capabilities in deterministic exact-identity order.
  pub fn capabilities(&self) -> impl Iterator<Item = &DecisionSignalProviderCapability> {
    self.providers.values().map(|provider| provider.capability())
  }

  /// Resolves only the exact provider, adapter, and model frozen by a request.
  pub fn resolve(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Arc<dyn DecisionSignalProvider>, DecisionSignalRegistryError> {
    let key = ProviderKey::from_request(request);
    let provider = self
      .providers
      .get(&key)
      .ok_or_else(|| DecisionSignalRegistryError::NotInstalled { key: key.display() })?;
    let capability = provider.capability();
    if !capability.supports(request.request().purpose())
      || capability.probability_semantics() != request.probability_semantics()
    {
      return Err(DecisionSignalRegistryError::Incompatible { key: key.display() });
    }
    Ok(Arc::clone(provider))
  }

  /// Returns the number of exact provider/model capabilities.
  #[must_use]
  pub fn len(&self) -> usize {
    self.providers.len()
  }

  /// Reports whether no Decision Signal providers are installed.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.providers.is_empty()
  }
}

impl fmt::Debug for DecisionSignalProviderRegistry {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("DecisionSignalProviderRegistry")
      .field("providers", &self.providers.keys().collect::<Vec<_>>())
      .finish()
  }
}

/// Secret-free exact registry discovery failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DecisionSignalRegistryError {
  /// Two adapters advertised one exact provider/adapter/model capability.
  #[error("duplicate Decision Signal provider capability '{key}'")]
  Duplicate {
    /// Safe logical exact identity.
    key: String,
  },
  /// No adapter advertises the exact frozen provider/adapter/model identity.
  #[error("Decision Signal provider capability '{key}' is not installed")]
  NotInstalled {
    /// Safe logical exact identity.
    key: String,
  },
  /// The exact adapter cannot satisfy the request purpose or semantics.
  #[error("Decision Signal provider capability '{key}' is incompatible")]
  Incompatible {
    /// Safe logical exact identity.
    key: String,
  },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProviderKey {
  provider: String,
  adapter: String,
  model: String,
}

impl ProviderKey {
  fn from_capability(capability: &DecisionSignalProviderCapability) -> Self {
    Self {
      provider: encode_reference(capability.provider()),
      adapter: encode_reference(capability.adapter()),
      model: encode_reference(capability.model()),
    }
  }

  fn from_request(request: &DecisionSignalProviderRequest) -> Self {
    Self {
      provider: encode_reference(request.provider()),
      adapter: encode_reference(request.adapter()),
      model: encode_reference(request.model()),
    }
  }

  fn display(&self) -> String {
    format!("{}/{}/{}", self.provider, self.adapter, self.model)
  }
}

fn encode_reference(reference: &octacity_server_factory::ImmutableReference) -> String {
  format!(
    "{}@{}#{}",
    reference.identity(),
    reference.version(),
    reference.digest()
  )
}
