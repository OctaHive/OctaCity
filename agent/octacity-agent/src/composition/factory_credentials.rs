//! Selects private credential files; their existing owner rechecks values per Job.

use octacity_config::{AgentConfig, FactoryCredentialProfileConfig};
use octacity_job::{FactoryCredentialProfile, JobError};

pub(super) fn load(config: &AgentConfig) -> Result<Vec<FactoryCredentialProfile>, JobError> {
  config
    .factory_credential_profiles
    .iter()
    .map(|(identity, source)| match source {
      FactoryCredentialProfileConfig::Variables(path) => {
        FactoryCredentialProfile::from_private_file(identity.clone(), path.clone())
      }
      FactoryCredentialProfileConfig::JsonFile {
        path,
        destination,
        sensitive_fields,
        persist_updates,
      } => FactoryCredentialProfile::from_private_json_file(
        identity.clone(),
        path.clone(),
        destination.clone(),
        sensitive_fields.clone(),
        *persist_updates,
      ),
    })
    .collect()
}
