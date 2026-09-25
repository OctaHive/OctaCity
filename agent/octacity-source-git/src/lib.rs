//! Secure Git implementation of the OctaCity source-plugin contract.
//!
//! The plugin invokes a configured Git executable directly, never through a
//! shell. It fetches a constrained reference, verifies the exact lowercase
//! commit object requested by the signed job, and checks workspace size both
//! before and after checkout.

#![warn(missing_docs)]

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use http::Uri;
use octacity_source_plugin::MaterializeRequest;
use processkit::{Command, ErrorReason, OutputBufferPolicy, OverflowMode};
use serde::Deserialize;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const DEFAULT_DIAGNOSTIC_BYTES: usize = 64 * 1024;
const MAX_DIAGNOSTIC_BYTES: usize = 512 * 1024;

/// Validation, process, or workspace failure reported by the Git provider.
#[derive(Debug, Error)]
pub enum GitSourceError {
  /// Provider settings or signed parameters are invalid.
  #[error("invalid Git source request: {0}")]
  Invalid(String),
  /// Cancellation won while a Git subprocess was active.
  #[error("Git source materialization was cancelled")]
  Cancelled,
  /// The configured Git executable could not be started or observed.
  #[error("failed to run Git during {step}: {source}")]
  Process {
    /// Materialization step that failed.
    step: &'static str,
    /// Underlying process I/O failure.
    source: std::io::Error,
  },
  /// Git exited unsuccessfully.
  #[error("Git {step} failed with status {status}: {diagnostic}")]
  Command {
    /// Materialization step that failed.
    step: &'static str,
    /// Portable description of Git's exit status.
    status: String,
    /// Sanitized bounded stderr/stdout tail.
    diagnostic: String,
  },
  /// Git produced more diagnostic output than operator policy allows.
  #[error("Git {step} exceeded its diagnostic output limit")]
  OutputLimit {
    /// Materialization step that exceeded the bound.
    step: &'static str,
  },
  /// Materialized files exceed the signed workspace limit.
  #[error("workspace exceeds the configured {limit}-byte limit")]
  WorkspaceLimit {
    /// Maximum permitted workspace bytes.
    limit: u64,
  },
  /// Filesystem inspection of the workspace failed.
  #[error("failed to inspect workspace '{path}': {source}")]
  Workspace {
    /// Path that could not be inspected.
    path: PathBuf,
    /// Underlying filesystem failure.
    source: std::io::Error,
  },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitSettings {
  git_path: PathBuf,
  #[serde(default)]
  allow_file: bool,
  #[serde(default = "default_diagnostic_bytes")]
  max_diagnostic_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitParameters {
  url: String,
}

/// Exact revision and provenance returned after a successful checkout.
#[derive(Debug)]
pub struct MaterializedGitSource {
  /// Exact lowercase commit object ID checked out by Git.
  pub revision: String,
  /// Provider metadata recorded for audit and display.
  pub provenance: BTreeMap<String, String>,
}

/// Fetches and checks out the exact Git commit described by `request`.
pub async fn materialize(
  request: &MaterializeRequest,
  cancellation: CancellationToken,
) -> Result<MaterializedGitSource, GitSourceError> {
  request.validate().map_err(GitSourceError::Invalid)?;
  let mut settings: GitSettings = from_map(&request.settings, "settings")?;
  let parameters: GitParameters = from_map(&request.parameters, "parameters")?;
  validate_settings(&mut settings)?;
  validate_request(request, &parameters, settings.allow_file)?;
  let git_config = validate_credentials(&request.credential_files)?;

  let destination = Path::new(&request.destination);
  run_git(
    &settings,
    git_config.as_deref(),
    destination,
    "repository initialization",
    ["init", "--quiet", "--initial-branch=octacity", "--template=", "."],
    &cancellation,
  )
  .await?;
  run_git(
    &settings,
    git_config.as_deref(),
    destination,
    "remote configuration",
    ["remote", "add", "origin", parameters.url.as_str()],
    &cancellation,
  )
  .await?;

  // A friendly ref may narrow the fetch, but it never replaces the immutable
  // commit identity that is verified below and used for checkout.
  let fetch_target = request.reference.as_deref().unwrap_or(&request.revision);
  if request.reference.is_some() {
    run_git(
      &settings,
      git_config.as_deref(),
      destination,
      "reference validation",
      ["check-ref-format", fetch_target],
      &cancellation,
    )
    .await?;
  }
  run_git(
    &settings,
    git_config.as_deref(),
    destination,
    "fetch",
    ["fetch", "--quiet", "--no-tags", "--depth=1", "origin", fetch_target],
    &cancellation,
  )
  .await?;
  enforce_workspace_limit(destination, request.max_workspace_bytes)?;

  // `^{commit}` rejects non-commit objects while `--end-of-options` prevents a
  // revision beginning with '-' from being interpreted as a Git option.
  let object = format!("{}^{{commit}}", request.revision);
  let resolved = run_git(
    &settings,
    git_config.as_deref(),
    destination,
    "revision verification",
    ["rev-parse", "--verify", "--end-of-options", object.as_str()],
    &cancellation,
  )
  .await?;
  let resolved = resolved.trim();
  if resolved != request.revision {
    return Err(GitSourceError::Invalid(format!(
      "resolved revision '{resolved}' does not equal requested revision '{}'",
      request.revision
    )));
  }

  run_git(
    &settings,
    git_config.as_deref(),
    destination,
    "detached checkout",
    ["checkout", "--quiet", "--detach", "--force", resolved],
    &cancellation,
  )
  .await?;
  enforce_workspace_limit(destination, request.max_workspace_bytes)?;

  Ok(MaterializedGitSource {
    revision: resolved.to_owned(),
    provenance: BTreeMap::from([
      ("provider".to_owned(), "git".to_owned()),
      ("revision".to_owned(), resolved.to_owned()),
    ]),
  })
}

fn from_map<T: for<'de> Deserialize<'de>>(
  values: &BTreeMap<String, serde_json::Value>,
  name: &str,
) -> Result<T, GitSourceError> {
  let object = serde_json::Map::from_iter(values.iter().map(|(key, value)| (key.clone(), value.clone())));
  serde_json::from_value(serde_json::Value::Object(object))
    .map_err(|error| GitSourceError::Invalid(format!("invalid Git {name}: {error}")))
}

fn validate_settings(settings: &mut GitSettings) -> Result<(), GitSourceError> {
  if !settings.git_path.is_absolute() {
    return Err(GitSourceError::Invalid("settings.git_path must be absolute".to_owned()));
  }
  settings.git_path = settings
    .git_path
    .canonicalize()
    .map_err(|error| GitSourceError::Invalid(format!("settings.git_path: {error}")))?;
  let metadata =
    fs::metadata(&settings.git_path).map_err(|error| GitSourceError::Invalid(format!("settings.git_path: {error}")))?;
  if !metadata.file_type().is_file() {
    return Err(GitSourceError::Invalid(
      "settings.git_path must resolve to a regular file".to_owned(),
    ));
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = metadata.permissions().mode();
    if mode & 0o111 == 0 || mode & 0o022 != 0 {
      return Err(GitSourceError::Invalid(
        "settings.git_path must be executable and not writable by group or other users".to_owned(),
      ));
    }
  }
  if settings.max_diagnostic_bytes == 0 || settings.max_diagnostic_bytes > MAX_DIAGNOSTIC_BYTES {
    return Err(GitSourceError::Invalid(format!(
      "settings.max_diagnostic_bytes must be between 1 and {MAX_DIAGNOSTIC_BYTES}"
    )));
  }
  Ok(())
}

fn validate_request(
  request: &MaterializeRequest,
  parameters: &GitParameters,
  allow_file: bool,
) -> Result<(), GitSourceError> {
  if !is_object_id(&request.revision) {
    return Err(GitSourceError::Invalid(
      "revision must be a full lowercase SHA-1 or SHA-256 object ID".to_owned(),
    ));
  }
  if let Some(reference) = &request.reference
    && !(reference.starts_with("refs/heads/") || reference.starts_with("refs/tags/"))
  {
    return Err(GitSourceError::Invalid(
      "reference must be a fully qualified branch or tag ref".to_owned(),
    ));
  }
  validate_remote(&parameters.url, allow_file)?;

  let destination = Path::new(&request.destination);
  let metadata =
    fs::symlink_metadata(destination).map_err(|error| GitSourceError::Invalid(format!("destination: {error}")))?;
  if !metadata.file_type().is_dir() {
    return Err(GitSourceError::Invalid(
      "destination must be a real directory, not a symlink".to_owned(),
    ));
  }
  let mut entries =
    fs::read_dir(destination).map_err(|error| GitSourceError::Invalid(format!("destination: {error}")))?;
  if entries.next().is_some() {
    return Err(GitSourceError::Invalid("destination must be empty".to_owned()));
  }
  Ok(())
}

fn validate_remote(remote: &str, allow_file: bool) -> Result<(), GitSourceError> {
  if Path::new(remote).is_absolute() {
    return if allow_file {
      Ok(())
    } else {
      Err(GitSourceError::Invalid("local Git transports are disabled".to_owned()))
    };
  }
  let uri: Uri = remote
    .parse()
    .map_err(|_| GitSourceError::Invalid("parameters.url must be an absolute HTTPS URL".to_owned()))?;
  if uri.scheme_str() != Some("https")
    || uri.host().is_none()
    || uri.authority().is_some_and(|value| value.as_str().contains('@'))
  {
    return Err(GitSourceError::Invalid(
      "parameters.url must be an HTTPS URL without user information".to_owned(),
    ));
  }
  if uri.query().is_some() {
    return Err(GitSourceError::Invalid(
      "parameters.url must not contain a query string with credentials".to_owned(),
    ));
  }
  Ok(())
}

fn validate_credentials(files: &BTreeMap<String, String>) -> Result<Option<PathBuf>, GitSourceError> {
  if files.keys().any(|name| name != "git_config") {
    return Err(GitSourceError::Invalid(
      "Git accepts only the 'git_config' credential handle".to_owned(),
    ));
  }
  let Some(path) = files.get("git_config") else {
    return Ok(None);
  };
  let path = Path::new(path);
  if !path.is_absolute() {
    return Err(GitSourceError::Invalid(
      "git_config credential path must be absolute".to_owned(),
    ));
  }
  let metadata =
    fs::symlink_metadata(path).map_err(|error| GitSourceError::Invalid(format!("git_config credential: {error}")))?;
  if !metadata.file_type().is_file() {
    return Err(GitSourceError::Invalid(
      "git_config credential must be a regular file, not a symlink".to_owned(),
    ));
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o077 != 0 {
      return Err(GitSourceError::Invalid(
        "git_config credential must not be accessible by group or others".to_owned(),
      ));
    }
  }
  path
    .canonicalize()
    .map(Some)
    .map_err(|error| GitSourceError::Invalid(format!("git_config credential: {error}")))
}

async fn run_git<const N: usize>(
  settings: &GitSettings,
  git_config: Option<&Path>,
  directory: &Path,
  step: &'static str,
  arguments: [&str; N],
  cancellation: &CancellationToken,
) -> Result<String, GitSourceError> {
  if cancellation.is_cancelled() {
    return Err(GitSourceError::Cancelled);
  }
  let protocol_file = if settings.allow_file { "always" } else { "never" };
  let command = Command::new(&settings.git_path)
    // Git inherits no ambient credentials or user configuration. The operator
    // may supply one reviewed config file, while protocols, hooks, LFS filters,
    // and background maintenance are constrained explicitly below.
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_LFS_SKIP_SMUDGE", "1")
    .env("LC_ALL", "C")
    .arg("--no-optional-locks")
    .arg("-c")
    .arg("core.hooksPath=.git/octacity-disabled-hooks")
    .arg("-c")
    .arg(format!("protocol.file.allow={protocol_file}"))
    .arg("-c")
    .arg("filter.lfs.required=false")
    .arg("-c")
    .arg("filter.lfs.smudge=")
    .arg("-c")
    .arg("maintenance.auto=false")
    .arg("-c")
    .arg("gc.auto=0")
    .arg("-c")
    .arg("fetch.writeCommitGraph=false")
    .args(arguments)
    .current_dir(directory)
    .env(
      "GIT_ALLOW_PROTOCOL",
      if settings.allow_file { "file:https" } else { "https" },
    )
    .env(
      "GIT_CONFIG_GLOBAL",
      git_config.unwrap_or_else(|| Path::new(null_device())),
    )
    .cancel_on(cancellation.clone())
    .output_buffer(
      OutputBufferPolicy::unbounded()
        .with_max_bytes(settings.max_diagnostic_bytes)
        .with_overflow(OverflowMode::Error),
    );
  let result = command.output_bytes().await.map_err(|source| match source.reason() {
    ErrorReason::Cancelled { .. } => GitSourceError::Cancelled,
    ErrorReason::OutputTooLarge { .. } => GitSourceError::OutputLimit { step },
    _ => GitSourceError::Process {
      step,
      source: std::io::Error::other(source),
    },
  })?;
  if !result.is_success() {
    let status = describe_outcome(result.outcome());
    return Err(GitSourceError::Command {
      step,
      status,
      diagnostic: sanitize_diagnostic(result.stderr().as_bytes()),
    });
  }
  String::from_utf8(result.into_stdout())
    .map_err(|_| GitSourceError::Invalid(format!("Git {step} returned non-UTF-8 output")))
}

fn describe_outcome(outcome: processkit::Outcome) -> String {
  if let Some(code) = outcome.code() {
    format!("exit code {code}")
  } else if let Some(signal) = outcome.signal() {
    format!("signal {signal}")
  } else {
    outcome.name().replace('_', " ")
  }
}

fn sanitize_diagnostic(bytes: &[u8]) -> String {
  String::from_utf8_lossy(bytes)
    .chars()
    .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
    .collect::<String>()
    .trim()
    .to_owned()
}

fn enforce_workspace_limit(root: &Path, limit: u64) -> Result<(), GitSourceError> {
  let mut total = 0_u64;
  let mut pending = vec![root.to_owned()];
  while let Some(directory) = pending.pop() {
    let entries = fs::read_dir(&directory).map_err(|source| GitSourceError::Workspace {
      path: directory.clone(),
      source,
    })?;
    for entry in entries {
      let entry = entry.map_err(|source| GitSourceError::Workspace {
        path: directory.clone(),
        source,
      })?;
      let path = entry.path();
      let metadata = fs::symlink_metadata(&path).map_err(|source| GitSourceError::Workspace {
        path: path.clone(),
        source,
      })?;
      if metadata.file_type().is_dir() {
        pending.push(path);
      } else {
        total = total.saturating_add(metadata.len());
        if total > limit {
          return Err(GitSourceError::WorkspaceLimit { limit });
        }
      }
    }
  }
  Ok(())
}

fn is_object_id(value: &str) -> bool {
  matches!(value.len(), 40 | 64)
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn default_diagnostic_bytes() -> usize {
  DEFAULT_DIAGNOSTIC_BYTES
}

#[cfg(unix)]
fn null_device() -> &'static str {
  "/dev/null"
}

#[cfg(windows)]
fn null_device() -> &'static str {
  "NUL"
}

#[cfg(test)]
mod tests;
