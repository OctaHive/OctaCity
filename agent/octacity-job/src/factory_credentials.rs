//! Operator-installed credentials selected only by signed logical identity.
use crate::{JobError, JobExecutor, StagedProtectedInputs, factory_preflight::ManagedFactoryExecution};
use octacity_protocol::MAX_FACTORY_WIRE_IDENTITY_BYTES;
use octacity_runner::{AuthorizedRunnerPrivateVariables, RunnerPrivateVariables};
use std::{collections::BTreeMap, fmt};

/// One immutable local credential installation, independent of provider format.
/// Values remain zeroing, opaque and absent from Debug and the public JobSpec.
#[derive(Clone)]
pub struct FactoryCredentialProfile {
  identity: String,
  variables: RunnerPrivateVariables,
}
impl FactoryCredentialProfile {
  /// Freezes one bounded logical identity and its private named variables.
  pub fn new(identity: String, values: BTreeMap<String, String>) -> Result<Self, JobError> {
    let variables = RunnerPrivateVariables::new(values).map_err(|_| invalid())?;
    if identity.is_empty()
      || identity.len() > MAX_FACTORY_WIRE_IDENTITY_BYTES
      || !identity
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
      return Err(invalid());
    }
    Ok(Self { identity, variables })
  }
}
impl fmt::Debug for FactoryCredentialProfile {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("FactoryCredentialProfile")
      .field("identity", &self.identity)
      .finish_non_exhaustive()
  }
}
fn invalid() -> JobError {
  JobError::Invalid("Factory credential installation is invalid".to_owned())
}
impl JobExecutor {
  /// Installs local profiles without copying any signed server grants or values.
  /// Repeated logical identities are ambiguous and rejected before execution.
  pub fn with_factory_credential_profiles(
    mut self,
    profiles: impl IntoIterator<Item = FactoryCredentialProfile>,
  ) -> Result<Self, JobError> {
    for profile in profiles {
      if self
        .factory_credential_profiles
        .insert(profile.identity.clone(), profile)
        .is_some()
      {
        return Err(invalid());
      }
    }
    Ok(self)
  }
  pub(super) fn authorize_factory_credentials(
    &self,
    intent: &ManagedFactoryExecution,
    staged: &StagedProtectedInputs,
  ) -> Result<Option<AuthorizedRunnerPrivateVariables>, JobError> {
    let Some(identity) = &intent.execution.credential_profile else {
      return Ok(None);
    };
    let error = || JobError::FactoryPreflight(crate::FactoryPreflightError::SecretProfile);
    let profile = self.factory_credential_profiles.get(identity).ok_or_else(error)?;
    let input = intent
      .protected_inputs
      .inputs
      .iter()
      .find(|input| input.artifact_id == intent.execution.octafile_input)
      .ok_or_else(error)?;
    let path = staged.path_for_input(&input.artifact_id).map_err(|_| error())?;
    let bytes = octacity_private_fs::read_bounded_regular_file(&path, octacity_runner::MAX_MANAGED_OCTAFILE_BYTES)
      .map_err(|_| error())?;
    profile
      .variables
      .authorize(input, &bytes, &intent.execution.tasks)
      .map(Some)
      .map_err(|_| error())
  }
}
