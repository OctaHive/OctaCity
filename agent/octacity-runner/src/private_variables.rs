//! Bounded operator values authorized against immutable task declarations.
use std::{collections::BTreeMap, fmt};

use octacity_protocol::{MAX_FACTORY_PERMISSION_ENTRIES, MAX_FACTORY_WIRE_IDENTITY_BYTES, ProtectedInputV3};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroize as _;
use zeroize::Zeroizing;

use crate::{RunnerJobRequest, RunnerRedactions, RunnerSupervisionError};

// A credential set has the same finite bound as an Octa secret profile. JSON
// escaping still fits inside the runner's independent input-frame ceiling.
const MAX_PRIVATE_VARIABLE_BYTES: usize = 1024 * 1024;

/// Maximum protected task-document bytes parsed to authorize private values.
/// Separate from the credential-value budget: public instructions, JSON escaping
/// and immutable metadata do not consume credential capacity.
pub const MAX_MANAGED_OCTAFILE_BYTES: u64 = 8 * 1024 * 1024;

/// Operator-owned private values; never included in public execution intent.
/// Copies are zeroed when dropped and Debug omits names and values.
#[derive(Clone, Default)]
pub struct RunnerPrivateVariables {
  values: BTreeMap<String, Zeroizing<String>>,
}

impl RunnerPrivateVariables {
  /// Freezes one finite named set, without selecting any provider or model.
  pub fn new(values: BTreeMap<String, String>) -> Result<Self, RunnerSupervisionError> {
    let values = values
      .into_iter()
      .map(|(name, value)| (name, Zeroizing::new(value)))
      .collect::<BTreeMap<_, _>>();
    if values.is_empty() || values.len() > MAX_FACTORY_PERMISSION_ENTRIES {
      return invalid();
    }
    let mut total = 0_usize;
    for (name, value) in &values {
      if name.is_empty()
        || name.len() > MAX_FACTORY_WIRE_IDENTITY_BYTES
        || name.chars().any(char::is_control)
        || value.is_empty()
        || value.contains('\0')
      {
        return invalid();
      }
      total = total
        .checked_add(name.len())
        .and_then(|total| total.checked_add(value.len()))
        .ok_or_else(error)?;
      if total > MAX_PRIVATE_VARIABLE_BYTES {
        return invalid();
      }
    }
    Ok(Self { values })
  }

  /// Verifies exact protected bytes and authorizes only secret required inputs.
  /// The returned opaque proof retains the immutable file binding for the
  /// independent check immediately before backend start.
  pub fn authorize(
    &self,
    input: &ProtectedInputV3,
    bytes: &[u8],
    tasks: &[String],
  ) -> Result<AuthorizedRunnerPrivateVariables, RunnerSupervisionError> {
    if self.values.is_empty()
      || tasks.is_empty()
      || bytes.len() as u64 > MAX_MANAGED_OCTAFILE_BYTES
      || bytes.len() as u64 != input.size_bytes
      || hex::encode(Sha256::digest(bytes)) != input.sha256
    {
      return invalid();
    }
    let document: Value = serde_yaml_ng::from_slice(bytes).map_err(|_| error())?;
    // Private overrides support the closed, flat shape emitted by trusted
    // compilers. Includes, environment files, discovered tasks, dependencies,
    // and scoped variable overrides cannot acquire authority from this proof.
    let root = document.as_object().ok_or_else(error)?;
    if root
      .keys()
      .any(|name| !matches!(name.as_str(), "version" | "vars" | "tasks"))
      || document.get("version") != Some(&Value::from(1))
    {
      return invalid();
    }
    let declarations = document.get("vars").and_then(Value::as_object).ok_or_else(error)?;
    let declared_tasks = document.get("tasks").and_then(Value::as_object).ok_or_else(error)?;
    if declared_tasks.values().any(|task| {
      task.as_object().is_none_or(|task| {
        task
          .keys()
          .any(|name| !matches!(name.as_str(), "cmds" | "shell" | "dir"))
          || task
            .get("dir")
            .is_some_and(|dir| dir.as_str() != Some(octacity_protocol::FACTORY_WORKSPACE_ROOT))
          || task
            .get("cmds")
            .and_then(Value::as_array)
            .is_some_and(|commands| commands.iter().any(|command| command.get("task").is_some()))
      })
    }) || tasks.iter().any(|task| !declared_tasks.contains_key(task))
      || self.values.keys().any(|name| {
        declarations.get(name).is_none_or(|declaration| {
          declaration.as_object().is_none_or(|declaration| {
            declaration.len() != 2
              || declaration.get("required") != Some(&Value::Bool(true))
              || declaration.get("secret") != Some(&Value::Bool(true))
          })
        })
      })
    {
      return invalid();
    }
    Ok(AuthorizedRunnerPrivateVariables {
      input: input.clone(),
      variables: self.clone(),
      tasks: tasks.to_vec(),
    })
  }
}

/// Private values bound to one exact immutable task and selected task set.
/// Construction requires declaration authorization; values remain inaccessible.
#[derive(Clone)]
pub struct AuthorizedRunnerPrivateVariables {
  input: ProtectedInputV3,
  variables: RunnerPrivateVariables,
  tasks: Vec<String>,
}

impl AuthorizedRunnerPrivateVariables {
  pub(crate) fn verify_for_job(&self, job: &RunnerJobRequest) -> Result<(), RunnerSupervisionError> {
    if job.spec.commands != self.tasks
      || job.spec.octafile.as_deref() != Some(self.input.destination.as_str())
      || job.spec.secrets_profile.is_some()
      || self
        .variables
        .values
        .keys()
        .any(|name| job.spec.variables.contains_key(name))
    {
      return invalid();
    }
    let root = &job.execution.factory.as_ref().ok_or_else(error)?.protected_inputs;
    let relative = std::path::Path::new(&self.input.destination)
      .strip_prefix(octacity_protocol::PROTECTED_INPUT_ROOT)
      .map_err(|_| error())?;
    let reader = octacity_private_fs::RootedReadDirectory::open(root).map_err(|_| error())?;
    let bytes = reader
      .read_bounded(relative, MAX_MANAGED_OCTAFILE_BYTES)
      .map_err(|_| error())?;
    self.variables.authorize(&self.input, &bytes, &self.tasks)?;
    Ok(())
  }
  pub(crate) fn redact(&self, redactions: &mut RunnerRedactions) {
    redactions.extend(self.variables.values.values().map(|value| value.as_bytes()));
  }
  fn inject(&self, request: &mut octa_runner_protocol::RunRequest) -> Result<Vec<String>, RunnerSupervisionError> {
    if self
      .variables
      .values
      .keys()
      .any(|name| request.variables.contains_key(name))
    {
      return invalid();
    }
    for (name, value) in &self.variables.values {
      request.variables.insert(name.clone(), value.to_string());
    }
    Ok(self.variables.values.keys().cloned().collect())
  }
}

/// The temporary wire command is never Debug-formatted; private copies are
/// zeroed on every successful, failed or cancelled write path.
pub(crate) struct PrivateRunnerCommand {
  pub(crate) command: octa_runner_protocol::RunnerCommand,
  private_names: Vec<String>,
}
impl PrivateRunnerCommand {
  pub(crate) fn new(
    mut command: octa_runner_protocol::RunnerCommand,
    variables: Option<&AuthorizedRunnerPrivateVariables>,
  ) -> Result<Self, RunnerSupervisionError> {
    let private_names = match (&mut command, variables) {
      (octa_runner_protocol::RunnerCommand::Start { request, .. }, Some(variables)) => variables.inject(request)?,
      (_, None) => Vec::new(),
      _ => return invalid(),
    };
    Ok(Self { command, private_names })
  }
}
impl Drop for PrivateRunnerCommand {
  fn drop(&mut self) {
    if let octa_runner_protocol::RunnerCommand::Start { request, .. } = &mut self.command {
      for name in &self.private_names {
        if let Some(value) = request.variables.get_mut(name) {
          value.zeroize()
        }
        request.variables.remove(name);
      }
    }
  }
}

impl fmt::Debug for RunnerPrivateVariables {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("RunnerPrivateVariables")
      .field("count", &self.values.len())
      .finish_non_exhaustive()
  }
}
impl fmt::Debug for AuthorizedRunnerPrivateVariables {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("AuthorizedRunnerPrivateVariables")
      .field("input_sha256", &self.input.sha256)
      .field("variable_count", &self.variables.values.len())
      .field("task_count", &self.tasks.len())
      .finish_non_exhaustive()
  }
}
fn error() -> RunnerSupervisionError {
  RunnerSupervisionError::Invalid("private overrides do not match immutable task declarations".to_owned())
}
fn invalid<T>() -> Result<T, RunnerSupervisionError> {
  Err(error())
}
