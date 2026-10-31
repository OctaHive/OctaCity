//! Installs the explicit provider-local bridge without adding workload grants.
use octacity_config::AgentConfig;
use octacity_job::JobError;
use octacity_lifecycle::{FactoryProtectedReadInstallation, FactoryProtectedReadRuntime, ProtectedReadRecipe};
use octacity_runner::{VerifiedExternalExecutable, verify_standalone_program};
use std::{collections::BTreeMap, path::Path};

type Installed = (
  BTreeMap<String, VerifiedExternalExecutable>,
  Option<FactoryProtectedReadRuntime>,
);
pub(super) fn load(
  config: &AgentConfig,
  mut executables: BTreeMap<String, VerifiedExternalExecutable>,
) -> Result<Installed, JobError> {
  let Some(reader) = &config.protected_reader else {
    return Ok((executables, None));
  };
  let Some(bridge) = &reader.bridge else {
    return Ok((executables, None));
  };
  let local = config.factory_permissions.clone().ok_or_else(invalid)?;
  let worker = verify_standalone_program(
    &reader.release_root,
    &reader.entrypoint,
    &reader.executable.sha256,
    &reader.platform,
  )
  .map_err(|_| invalid())?;
  let adapter = verify_standalone_program(
    &reader.release_root,
    &bridge.entrypoint,
    &bridge.sha256,
    &reader.platform,
  )
  .map_err(|_| invalid())?;
  if executables.contains_key(&reader.executable.identity) {
    return Err(invalid());
  }
  let recipe = ProtectedReadRecipe::new(
    reader.tool.clone(),
    reader.executable.clone(),
    reader.resources,
    reader.outputs.clone(),
  )
  .map_err(|_| invalid())?;
  let state = config.state_root.join("reader");
  private_directory(&state)?;
  let runtime = FactoryProtectedReadRuntime::new(
    FactoryProtectedReadInstallation {
      worker: worker.clone(),
      bridge: adapter,
      bridge_sha256: bridge.sha256.clone(),
      platform: reader.platform.clone(),
      hook_tool_name: bridge.hook_tool_name.clone(),
      recipe,
    },
    local,
    state,
  )
  .map_err(|_| invalid())?;
  executables.insert(
    reader.executable.identity.clone(),
    VerifiedExternalExecutable {
      guest_filename: None,
      product: reader.executable.identity.clone(),
      version: reader.executable.version.clone(),
      platform: reader.platform.clone(),
      executable: worker.executable,
      sha256: reader.executable.sha256.clone(),
      selection_environment: "OCTACITY_INSTALLED_READER_WORKER".to_owned(),
    },
  );
  Ok((executables, Some(runtime)))
}
fn private_directory(path: &Path) -> Result<(), JobError> {
  match octacity_private_fs::create_private_directory(path) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
    Err(_) => return Err(invalid()),
  }
  if !std::fs::symlink_metadata(path)
    .map_err(|_| invalid())?
    .file_type()
    .is_dir()
    || path.canonicalize().map_err(|_| invalid())? != path
  {
    return Err(invalid());
  }
  octacity_private_fs::validate_private_access(path).map_err(|_| invalid())?;
  octacity_private_fs::validate_trusted_directory_chain(path).map_err(|_| invalid())
}
fn invalid() -> JobError {
  JobError::Invalid("controlled reader installation is unavailable".to_owned())
}
