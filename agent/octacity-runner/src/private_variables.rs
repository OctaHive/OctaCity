//! Bounded operator values authorized against immutable task declarations.
use std::{collections::BTreeMap, fmt};

use octacity_protocol::{MAX_FACTORY_PERMISSION_ENTRIES, MAX_FACTORY_WIRE_IDENTITY_BYTES, ProtectedInputV3};
use serde::{
  Deserialize, Deserializer,
  de::{MapAccess, Visitor},
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroize as _;
use zeroize::Zeroizing;

use crate::{RunnerJobRequest, RunnerRedactions, RunnerSupervisionError};

// A credential set has the same finite bound as an Octa secret profile. JSON
// escaping still fits inside the runner's independent input-frame ceiling.
pub(crate) const MAX_PRIVATE_VARIABLE_BYTES: usize = 1024 * 1024;

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
  /// Reads current private values from an operator-owned file without caching.
  /// Duplicate keys, unsafe paths/modes and decoder diagnostics fail closed.
  pub fn from_private_file(path: &std::path::Path) -> Result<Self, RunnerSupervisionError> {
    let read = || -> Result<Self, Box<dyn std::error::Error>> {
      let parent = path.parent().ok_or("private file has no parent")?;
      octacity_private_fs::validate_trusted_directory_chain(parent)?;
      octacity_private_fs::validate_private_access(parent)?;
      octacity_private_fs::validate_trusted_owner(path)?;
      octacity_private_fs::validate_private_access(path)?;
      // Encoded JSON can escape the 1 MiB decoded variable set.
      let bytes = Zeroizing::new(octacity_private_fs::read_bounded_regular_file(path, 8 * 1024 * 1024)?);
      let values: PrivateValues = serde_json::from_slice(&bytes)?;
      Ok(Self::new(
        values
          .0
          .into_iter()
          .map(|(key, mut value)| (key, std::mem::take(&mut *value)))
          .collect(),
      )?)
    };
    read().map_err(|_| error())
  }

  /// Adds one independently selected credential without exposing its value.
  /// A dedicated MCP profile supplies one opaque credential, renamed to the
  /// signed compiler variable. Operators never need to encode internal names
  /// into private files; extra inputs cannot hitchhike through this selection.
  pub fn with_selected_credential(&self, name: &str, profile: &Self) -> Result<Self, RunnerSupervisionError> {
    if profile.values.len() != 1 || self.values.contains_key(name) {
      return invalid();
    }
    let value = profile.values.values().next().ok_or_else(error)?;
    if self.values.values().any(|existing| existing == value) {
      return invalid();
    }
    let mut selected = self.clone();
    selected.values.insert(name.to_owned(), value.clone());
    selected.validate()?;
    Ok(selected)
  }

  /// Freezes one finite named set, without selecting any provider or model.
  pub fn new(values: BTreeMap<String, String>) -> Result<Self, RunnerSupervisionError> {
    let values = values
      .into_iter()
      .map(|(name, value)| (name, Zeroizing::new(value)))
      .collect::<BTreeMap<_, _>>();
    let selected = Self { values };
    selected.validate()?;
    Ok(selected)
  }

  fn validate(&self) -> Result<(), RunnerSupervisionError> {
    let values = &self.values;
    if values.is_empty() || values.len() > MAX_FACTORY_PERMISSION_ENTRIES {
      return invalid();
    }
    let mut total = 0_usize;
    for (name, value) in values {
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
    Ok(())
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
  /// Opaque values for the same credential exclusion rule on frozen outputs.
  pub fn sensitive_values(&self) -> crate::RunnerSensitiveValues {
    crate::RunnerSensitiveValues::new(self.variables.values.values().map(|value| value.as_bytes()))
  }

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

// Already decoded values, including a partially decoded map on error, are
// erased on drop. Neither serde errors nor file paths reach public diagnostics.
struct PrivateValue(Zeroizing<String>);
impl<'de> Deserialize<'de> for PrivateValue {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
  }
}

struct PrivateValues(BTreeMap<String, Zeroizing<String>>);
impl<'de> Deserialize<'de> for PrivateValues {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct ValuesVisitor;
    impl<'de> Visitor<'de> for ValuesVisitor {
      type Value = PrivateValues;
      fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded private named string map")
      }
      fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut values = BTreeMap::new();
        while let Some((key, PrivateValue(value))) = map.next_entry::<String, PrivateValue>()? {
          if values.len() >= MAX_FACTORY_PERMISSION_ENTRIES || values.contains_key(&key) {
            return Err(serde::de::Error::custom("private map exceeds its bound"));
          }
          values.insert(key, value);
        }
        Ok(PrivateValues(values))
      }
    }
    deserializer.deserialize_map(ValuesVisitor)
  }
}
