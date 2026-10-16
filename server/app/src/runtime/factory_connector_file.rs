//! Private installed connector files; provider payloads belong to adapters.
use std::{
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};

use octacity_server_decision_signal::DecisionSignalProviderRegistry;
use octacity_server_factory::{
  DecisionSignalProvider, DecisionSignalProviderCapability, DecisionSignalProviderFuture,
  DecisionSignalProviderObservation, DecisionSignalProviderObservationFuture, DecisionSignalProviderRequest,
  FactoryKey,
};
use serde::Deserialize;

use super::{FactoryRuntimeInstallationError, FactoryRuntimeReadiness};

const MAX_CONNECTOR_BYTES: u64 = 4 * 1024 * 1024;

/// Combines Factory dependency readiness with observed installed connector failures.
/// It never probes providers, dispatches a request or affects ordinary readiness.
pub struct FactoryRuntimeSignalReadiness {
  dependencies: Arc<dyn FactoryRuntimeReadiness>,
  connectors: Vec<FactoryRuntimeSignalConnector>,
}
impl FactoryRuntimeSignalReadiness {
  /// Uses the same registrations whose transports are installed in Factory policies.
  pub fn new(dependencies: Arc<dyn FactoryRuntimeReadiness>, connectors: Vec<FactoryRuntimeSignalConnector>) -> Self {
    Self {
      dependencies,
      connectors,
    }
  }
}
#[async_trait::async_trait]
impl FactoryRuntimeReadiness for FactoryRuntimeSignalReadiness {
  async fn available(&self) -> bool {
    !self
      .connectors
      .iter()
      .any(FactoryRuntimeSignalConnector::has_observed_failure)
      && self.dependencies.available().await
  }
}

/// Trusted association between one connector and the credential it actually uses.
/// The registration owns no provider wire format and carries no secret bytes.
#[derive(Clone)]
pub struct FactoryRuntimeSignalConnector {
  credential_profile: FactoryKey,
  provider: Arc<dyn DecisionSignalProvider>,
  observed_failure: Arc<AtomicBool>,
}
impl FactoryRuntimeSignalConnector {
  /// Registers a connector whose credential was independently resolved by the
  /// trusted host. Task or Flow metadata must never construct this association.
  pub fn new(credential_profile: FactoryKey, provider: Arc<dyn DecisionSignalProvider>) -> Self {
    let observed_failure = Arc::new(AtomicBool::new(false));
    Self {
      credential_profile,
      provider: Arc::new(ObservedSignalProvider {
        inner: provider,
        observed_failure: observed_failure.clone(),
      }),
      observed_failure,
    }
  }
  /// Exact server credential selected during installation.
  pub fn credential_profile(&self) -> &FactoryKey {
    &self.credential_profile
  }
  /// Exact connector capability, independent of its logical credential alias.
  pub fn capability(&self) -> &DecisionSignalProviderCapability {
    self.provider.capability()
  }
  /// Installed transport; only trusted composition may attach it to a policy.
  pub fn provider(&self) -> &Arc<dyn DecisionSignalProvider> {
    &self.provider
  }
  /// Reports a failed or interrupted evaluation/recovery without another request.
  /// Installation alone does not establish remote availability.
  pub fn has_observed_failure(&self) -> bool {
    self.observed_failure.load(Ordering::Acquire)
  }
}

struct ObservedSignalProvider {
  inner: Arc<dyn DecisionSignalProvider>,
  observed_failure: Arc<AtomicBool>,
}
impl DecisionSignalProvider for ObservedSignalProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    self.inner.capability()
  }
  fn network_destinations(&self) -> Option<&[octacity_server_factory::NetworkHost]> {
    self.inner.network_destinations()
  }
  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    Box::pin(async move {
      let mut observation = ExchangeObservation {
        failed: &self.observed_failure,
        outcome_failed: Some(true),
      };
      let result = self.inner.evaluate(request).await;
      observation.outcome_failed = Some(
        !result
          .as_ref()
          .is_ok_and(|response| DecisionSignalProviderObservation::Result(response.clone()).is_valid_for(request)),
      );
      result
    })
  }
  fn observe_evaluation<'a>(
    &'a self,
    request: &'a DecisionSignalProviderRequest,
  ) -> DecisionSignalProviderObservationFuture<'a> {
    Box::pin(async move {
      let mut observation = ExchangeObservation {
        failed: &self.observed_failure,
        outcome_failed: Some(true),
      };
      let recovered = self.inner.observe_evaluation(request).await;
      // Absence preserves the previous state. Only an exact successful result
      // establishes health; failure, invalid data and interruption cannot.
      observation.outcome_failed = recovered
        .as_ref()
        .map(|result| !matches!(result, DecisionSignalProviderObservation::Result(_)) || !result.is_valid_for(request));
      recovered
    })
  }
}

struct ExchangeObservation<'a> {
  failed: &'a AtomicBool,
  outcome_failed: Option<bool>,
}
impl Drop for ExchangeObservation<'_> {
  fn drop(&mut self) {
    if let Some(failed) = self.outcome_failed {
      self.failed.store(failed, Ordering::Release);
    }
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorCatalog {
  version: u16,
  connectors: Vec<ConnectorFiles>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorFiles {
  kind: FactoryKey,
  credential_profile: FactoryKey,
  configuration_file: PathBuf,
  credential_file: PathBuf,
}

/// Loads installed provider adapters from bounded owner-only regular files.
/// The versioned catalog contains adapter kinds and private file references;
/// provider configuration is decoded only by the selected adapter. No request
/// is evaluated during installation, and failures never include private paths.
pub async fn load_factory_signal_connectors(
  path: &Path,
) -> Result<Vec<FactoryRuntimeSignalConnector>, FactoryRuntimeInstallationError> {
  use FactoryRuntimeInstallationError::Invalid;
  let bytes = super::assembly::read_private_bytes("Decision Signal connectors", path, MAX_CONNECTOR_BYTES)
    .await
    .map_err(|_| Invalid)?;
  let catalog: ConnectorCatalog = serde_json::from_slice(&bytes).map_err(|_| Invalid)?;
  if catalog.version != 1 || catalog.connectors.len() > octacity_server_application::MAX_FACTORY_EXECUTOR_BINDINGS {
    return Err(Invalid);
  }
  let mut providers = Vec::with_capacity(catalog.connectors.len());
  for connector in catalog.connectors {
    if !connector.configuration_file.is_absolute() || !connector.credential_file.is_absolute() {
      return Err(Invalid);
    }
    let configuration = super::assembly::read_private_bytes(
      "Decision Signal connector configuration",
      &connector.configuration_file,
      MAX_CONNECTOR_BYTES,
    )
    .await
    .map_err(|_| Invalid)?;
    let credential = super::assembly::read_credential("Decision Signal credential", &connector.credential_file)
      .await
      .map_err(|_| Invalid)?;
    providers.push(FactoryRuntimeSignalConnector::new(
      connector.credential_profile,
      install_decision_signal_connector(&connector.kind, &configuration, credential.to_string())
        .map_err(|_| Invalid)?,
    ));
  }
  let mut credentials = std::collections::BTreeMap::<_, Vec<_>>::new();
  for connector in &providers {
    credentials
      .entry(connector.credential_profile())
      .or_default()
      .push(connector.provider().clone());
  }
  for transports in credentials.into_values() {
    DecisionSignalProviderRegistry::try_new(transports).map_err(|_| Invalid)?;
  }
  Ok(providers)
}

// Only the composition root chooses concrete providers. Registry/core never
// import a provider implementation or decode its configuration.
fn install_decision_signal_connector(
  kind: &FactoryKey,
  configuration: &[u8],
  credential: String,
) -> Result<Arc<dyn DecisionSignalProvider>, FactoryRuntimeInstallationError> {
  use FactoryRuntimeInstallationError::Invalid;
  let credential = zeroize::Zeroizing::new(credential);
  match kind.as_str() {
    "jev" => {
      use octacity_server_decision_signal_jev::{JevApiKey, JevDecisionSignalProvider};
      let key = JevApiKey::new(credential.to_string()).map_err(|_| Invalid)?;
      Ok(Arc::new(
        JevDecisionSignalProvider::from_deployment_json(configuration, key).map_err(|_| Invalid)?,
      ))
    }
    "openai-decisions" => {
      use octacity_server_decision_signal_openai::OpenAiDecisionSignalProvider;
      Ok(Arc::new(
        OpenAiDecisionSignalProvider::from_deployment_json(configuration, credential.to_string())
          .map_err(|_| Invalid)?,
      ))
    }
    _ => Err(Invalid),
  }
}
