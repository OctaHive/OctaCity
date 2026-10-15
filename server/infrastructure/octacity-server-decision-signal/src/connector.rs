use std::sync::Arc;

use octacity_server_factory::{DecisionSignalProvider, FactoryKey};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{JevApiKey, JevDecisionSignalProvider};

/// Fixed, secret-safe failure from installed connector discovery.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("Decision Signal connector configuration is invalid")]
pub struct DecisionSignalConnectorError;

/// Installs one built-in transport adapter without contacting its provider.
///
/// Configuration belongs to the selected adapter. The host reads credentials
/// separately from operator-owned private files. Unknown adapter kinds fail
/// closed; custom providers can still use the neutral registry directly.
pub fn install_decision_signal_connector(
  kind: &FactoryKey,
  configuration: &[u8],
  credential: String,
) -> Result<Arc<dyn DecisionSignalProvider>, DecisionSignalConnectorError> {
  let credential = Zeroizing::new(credential);
  match kind.as_str() {
    "jev" => {
      let key = JevApiKey::new(credential.to_string()).map_err(|_| DecisionSignalConnectorError)?;
      let provider = JevDecisionSignalProvider::from_deployment_json(configuration, key)
        .map_err(|_| DecisionSignalConnectorError)?;
      Ok(Arc::new(provider))
    }
    _ => Err(DecisionSignalConnectorError),
  }
}
