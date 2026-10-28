//! Selects private credential files; their existing owner rechecks values per Job.

use octacity_config::AgentConfig;
use octacity_job::{FactoryCredentialProfile, JobError};

pub(super) fn load(config: &AgentConfig) -> Result<Vec<FactoryCredentialProfile>, JobError> {
  config
    .factory_credential_profiles
    .iter()
    .map(|(identity, path)| FactoryCredentialProfile::from_private_file(identity.clone(), path.clone()))
    .collect()
}
