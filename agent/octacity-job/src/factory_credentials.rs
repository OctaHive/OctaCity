//! Operator-installed credentials selected only by signed logical identity.
use crate::{JobError, JobExecutor, StagedProtectedInputs, factory_preflight::ManagedFactoryExecution};
use octacity_protocol::MAX_FACTORY_WIRE_IDENTITY_BYTES;
use octacity_runner::{AuthorizedRunnerPrivateVariables, RunnerPrivateVariables};
use std::{collections::BTreeMap, fmt, path::PathBuf};

/// One immutable local credential installation, independent of provider format.
/// Values remain zeroing, opaque and absent from Debug and the public JobSpec.
#[derive(Clone)]
pub struct FactoryCredentialProfile {
  identity: String,
  source: CredentialSource,
}
#[derive(Clone)]
enum CredentialSource {
  Values(RunnerPrivateVariables),
  File(PathBuf),
}
impl FactoryCredentialProfile {
  /// Freezes one bounded logical identity and its private named variables.
  pub fn new(identity: String, values: BTreeMap<String, String>) -> Result<Self, JobError> {
    let variables = RunnerPrivateVariables::new(values).map_err(|_| invalid())?;
    validate_identity(&identity)?;
    Ok(Self {
      identity,
      source: CredentialSource::Values(variables),
    })
  }
  /// Uses the current private-file installation for every newly authorized Job.
  /// No token survives file revocation or rotation in an Agent-side cache.
  pub fn from_private_file(identity: String, path: PathBuf) -> Result<Self, JobError> {
    validate_identity(&identity)?;
    RunnerPrivateVariables::from_private_file(&path).map_err(|_| invalid())?;
    Ok(Self {
      identity,
      source: CredentialSource::File(path),
    })
  }
  fn variables(&self) -> Result<RunnerPrivateVariables, JobError> {
    match &self.source {
      CredentialSource::Values(values) => Ok(values.clone()),
      CredentialSource::File(path) => RunnerPrivateVariables::from_private_file(path).map_err(|_| invalid()),
    }
  }
}
fn validate_identity(identity: &str) -> Result<(), JobError> {
  if identity.is_empty()
    || identity.len() > MAX_FACTORY_WIRE_IDENTITY_BYTES
    || !identity
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
  {
    return Err(invalid());
  }
  Ok(())
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
  pub(super) fn resolve_factory_credentials(
    &self,
    intent: &ManagedFactoryExecution,
  ) -> Result<Option<RunnerPrivateVariables>, JobError> {
    let error = || JobError::FactoryPreflight(crate::FactoryPreflightError::SecretProfile);
    let Some(identity) = &intent.execution.credential_profile else {
      return Ok(None);
    };
    let mut variables = self
      .factory_credential_profiles
      .get(identity)
      .ok_or_else(error)?
      .variables()
      .map_err(|_| error())?;
    for (variable, identity) in &intent.execution.mcp_credentials {
      let profile = self
        .factory_credential_profiles
        .get(identity)
        .ok_or_else(error)?
        .variables()
        .map_err(|_| error())?;
      variables = variables
        .with_selected_credential(variable, &profile)
        .map_err(|_| error())?;
    }
    Ok(Some(variables))
  }
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
    let Some(variables) = self.resolve_factory_credentials(intent)? else {
      return Ok(None);
    };
    let error = || JobError::FactoryPreflight(crate::FactoryPreflightError::SecretProfile);
    let input = intent
      .protected_inputs
      .inputs
      .iter()
      .find(|input| input.artifact_id == intent.execution.octafile_input)
      .ok_or_else(error)?;
    let path = staged.path_for_input(&input.artifact_id).map_err(|_| error())?;
    let bytes = octacity_private_fs::read_bounded_regular_file(&path, octacity_runner::MAX_MANAGED_OCTAFILE_BYTES)
      .map_err(|_| error())?;
    variables
      .authorize(input, &bytes, &intent.execution.tasks)
      .map(Some)
      .map_err(|_| error())
  }
}
