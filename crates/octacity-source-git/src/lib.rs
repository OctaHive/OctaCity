use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  process::Stdio,
};

use http::Uri;
use octacity_source_plugin::MaterializeRequest;
use serde::Deserialize;
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt as _},
  process::Command,
};
use tokio_util::sync::CancellationToken;

const DEFAULT_DIAGNOSTIC_BYTES: usize = 64 * 1024;
const MAX_DIAGNOSTIC_BYTES: usize = 512 * 1024;

#[derive(Debug, Error)]
pub enum GitSourceError {
  #[error("invalid Git source request: {0}")]
  Invalid(String),
  #[error("Git source materialization was cancelled")]
  Cancelled,
  #[error("failed to start Git during {step}: {source}")]
  Spawn { step: &'static str, source: std::io::Error },
  #[error("failed to wait for Git during {step}: {source}")]
  Wait { step: &'static str, source: std::io::Error },
  #[error("Git {step} failed with status {status}: {diagnostic}")]
  Command {
    step: &'static str,
    status: String,
    diagnostic: String,
  },
  #[error("Git {step} exceeded its diagnostic output limit")]
  OutputLimit { step: &'static str },
  #[error("workspace exceeds the configured {limit}-byte limit")]
  WorkspaceLimit { limit: u64 },
  #[error("failed to inspect workspace '{path}': {source}")]
  Workspace { path: PathBuf, source: std::io::Error },
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

#[derive(Debug)]
pub struct MaterializedGitSource {
  pub revision: String,
  pub provenance: BTreeMap<String, String>,
}

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
    if metadata.permissions().mode() & 0o111 == 0 {
      return Err(GitSourceError::Invalid(
        "settings.git_path has no execute bit".to_owned(),
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
  let mut command = Command::new(&settings.git_path);
  command
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
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  command.env(
    "GIT_ALLOW_PROTOCOL",
    if settings.allow_file { "file:https" } else { "https" },
  );
  command.env(
    "GIT_CONFIG_GLOBAL",
    git_config.unwrap_or_else(|| Path::new(null_device())),
  );
  let mut child = command
    .spawn()
    .map_err(|source| GitSourceError::Spawn { step, source })?;
  let stdout = child.stdout.take().expect("Git stdout was configured as piped");
  let stderr = child.stderr.take().expect("Git stderr was configured as piped");
  let limit = settings.max_diagnostic_bytes;
  let stdout_task = tokio::spawn(read_bounded(stdout, limit));
  let stderr_task = tokio::spawn(read_bounded(stderr, limit));

  let status = tokio::select! {
    status = child.wait() => status.map_err(|source| GitSourceError::Wait { step, source })?,
    () = cancellation.cancelled() => {
      kill_process_tree(&mut child);
      let _ = child.wait().await;
      let _ = stdout_task.await;
      let _ = stderr_task.await;
      return Err(GitSourceError::Cancelled);
    }
  };
  let stdout = stdout_task
    .await
    .map_err(|error| GitSourceError::Wait {
      step,
      source: std::io::Error::other(error),
    })?
    .map_err(|source| GitSourceError::Wait { step, source })?;
  let stderr = stderr_task
    .await
    .map_err(|error| GitSourceError::Wait {
      step,
      source: std::io::Error::other(error),
    })?
    .map_err(|source| GitSourceError::Wait { step, source })?;
  if stdout.truncated || stderr.truncated {
    return Err(GitSourceError::OutputLimit { step });
  }
  if !status.success() {
    return Err(GitSourceError::Command {
      step,
      status: status.to_string(),
      diagnostic: sanitize_diagnostic(&stderr.bytes),
    });
  }
  String::from_utf8(stdout.bytes).map_err(|_| GitSourceError::Invalid(format!("Git {step} returned non-UTF-8 output")))
}

struct BoundedOutput {
  bytes: Vec<u8>,
  truncated: bool,
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> Result<BoundedOutput, std::io::Error> {
  let mut bytes = Vec::with_capacity(limit.min(8192));
  let mut buffer = [0_u8; 8192];
  let mut truncated = false;
  loop {
    let read = reader.read(&mut buffer).await?;
    if read == 0 {
      break;
    }
    let remaining = limit.saturating_sub(bytes.len());
    bytes.extend_from_slice(&buffer[..read.min(remaining)]);
    truncated |= read > remaining;
  }
  Ok(BoundedOutput { bytes, truncated })
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

fn kill_process_tree(child: &mut tokio::process::Child) {
  let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
  use std::io::Write as _;

  use super::*;

  #[test]
  fn validates_remote_transport_policy() {
    assert!(validate_remote("https://example.com/repository.git", false).is_ok());
    assert!(validate_remote("http://example.com/repository.git", false).is_err());
    assert!(validate_remote("https://user@example.com/repository.git", false).is_err());
    assert!(validate_remote("https://example.com/repository.git?token=secret", false).is_err());

    let local = std::env::current_dir().unwrap();
    let local = local.to_str().unwrap();
    assert!(validate_remote(local, false).is_err());
    assert!(validate_remote(local, true).is_ok());
  }

  #[test]
  fn recognizes_only_full_lowercase_object_ids() {
    assert!(is_object_id(&"a".repeat(40)));
    assert!(is_object_id(&"b".repeat(64)));
    assert!(!is_object_id(&"a".repeat(39)));
    assert!(!is_object_id(&"A".repeat(40)));
    assert!(!is_object_id(&"z".repeat(40)));
  }

  #[test]
  fn enforces_workspace_size_without_following_special_entries() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("large"), [0_u8; 32]).unwrap();
    assert!(enforce_workspace_limit(workspace.path(), 32).is_ok());
    assert!(matches!(
      enforce_workspace_limit(workspace.path(), 31),
      Err(GitSourceError::WorkspaceLimit { limit: 31 })
    ));
  }

  #[tokio::test]
  async fn bounded_reader_drains_but_marks_truncated_output() {
    let output = read_bounded(&b"abcdef"[..], 3).await.unwrap();
    assert_eq!(output.bytes, b"abc");
    assert!(output.truncated);
    assert_eq!(sanitize_diagnostic(b"bad\0 message\n"), "bad message");
  }

  #[test]
  fn validates_operator_settings_and_credential_handles() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join(if cfg!(windows) { "git.exe" } else { "git" });
    create_executable(&executable);
    let mut settings = GitSettings {
      git_path: executable,
      allow_file: false,
      max_diagnostic_bytes: DEFAULT_DIAGNOSTIC_BYTES,
    };
    assert!(validate_settings(&mut settings).is_ok());
    settings.max_diagnostic_bytes = 0;
    assert!(validate_settings(&mut settings).is_err());

    assert_eq!(validate_credentials(&BTreeMap::new()).unwrap(), None);
    let unknown = BTreeMap::from([("token".to_owned(), "/credential".to_owned())]);
    assert!(validate_credentials(&unknown).is_err());
    let relative = BTreeMap::from([("git_config".to_owned(), "credential".to_owned())]);
    assert!(validate_credentials(&relative).is_err());

    let credential = temp.path().join("credential");
    fs::write(&credential, "[credential]\nhelper =\n").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let configured = BTreeMap::from([("git_config".to_owned(), credential.to_string_lossy().into_owned())]);
    assert_eq!(
      validate_credentials(&configured).unwrap(),
      Some(credential.canonicalize().unwrap())
    );
  }

  fn create_executable(path: &Path) {
    fs::File::create(path).unwrap().write_all(b"executable").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
  }
}
