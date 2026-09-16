//! Converts untrusted runner declarations into immutable upload snapshots.

use std::{
  collections::BTreeSet,
  fs,
  path::{Component, Path, PathBuf},
};

use octacity_protocol::{OutputKind, OutputLimits, OutputUploadMetadata};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
  OutputError,
  snapshot::{check_cancel, snapshot_directory, snapshot_file},
};

/// Media type for deterministic POSIX-style tar directory snapshots.
pub(super) const DIRECTORY_ARCHIVE_CONTENT_TYPE: &str = "application/vnd.octacity.directory.tar.v1";
const FILE_CONTENT_TYPE: &str = "application/octet-stream";

/// One frozen file containing the exact bytes authorized for upload.
#[derive(Debug)]
pub(super) struct PreparedOutput {
  path: PathBuf,
  metadata: OutputUploadMetadata,
}

impl PreparedOutput {
  /// Returns the immutable staging file opened by the uploader.
  pub(super) fn path(&self) -> &Path {
    &self.path
  }

  /// Returns metadata computed from the exact staged bytes.
  pub(super) fn metadata(&self) -> &OutputUploadMetadata {
    &self.metadata
  }
}

/// Narrow read model for the stable runner fields needed by publication.
/// Keeping it private avoids coupling the agent to Octa executor internals.
#[derive(Deserialize)]
struct ExecutionView {
  run_id: u64,
  #[serde(default)]
  tasks: Vec<TaskView>,
}

#[derive(Deserialize)]
struct TaskView {
  task_id: u64,
  #[serde(default)]
  artifacts: Vec<ArtifactView>,
  #[serde(default)]
  reports: Vec<ReportView>,
}

#[derive(Deserialize)]
struct ArtifactView {
  name: String,
  path: String,
  #[serde(default)]
  content_type: Option<String>,
}

#[derive(Deserialize)]
struct ReportView {
  name: String,
  path: String,
  format: String,
}

/// One logical output rebound to the runner task that produced it.
struct Declaration {
  run_id: u64,
  task_id: u64,
  value: DeclarationValue,
}

enum DeclarationValue {
  Artifact(ArtifactView),
  Report(ReportView),
}

impl Declaration {
  fn kind(&self) -> OutputKind {
    match &self.value {
      DeclarationValue::Artifact(_) => OutputKind::Artifact,
      DeclarationValue::Report(_) => OutputKind::Report,
    }
  }

  fn name(&self) -> &str {
    match &self.value {
      DeclarationValue::Artifact(value) => &value.name,
      DeclarationValue::Report(value) => &value.name,
    }
  }

  fn path(&self) -> &str {
    match &self.value {
      DeclarationValue::Artifact(value) => &value.path,
      DeclarationValue::Report(value) => &value.path,
    }
  }
}

pub(super) async fn prepare(
  workspace: &Path,
  results: &[serde_json::Value],
  limits: &OutputLimits,
  staging_root: &Path,
  max_archive_entries: usize,
  cancellation: CancellationToken,
) -> Result<Vec<PreparedOutput>, OutputError> {
  let workspace = workspace.to_owned();
  let results = results.to_vec();
  let limits = limits.clone();
  let staging_root = staging_root.to_owned();
  tokio::task::spawn_blocking(move || {
    prepare_blocking(
      &workspace,
      &results,
      &limits,
      &staging_root,
      max_archive_entries,
      &cancellation,
    )
  })
  .await
  .map_err(|error| OutputError::Invalid(format!("output preparation task failed: {error}")))?
}

fn prepare_blocking(
  workspace: &Path,
  results: &[serde_json::Value],
  limits: &OutputLimits,
  staging_root: &Path,
  max_archive_entries: usize,
  cancellation: &CancellationToken,
) -> Result<Vec<PreparedOutput>, OutputError> {
  let declarations = declarations(results)?;
  if declarations.is_empty() {
    return Ok(Vec::new());
  }
  check_cancel(cancellation)?;
  let canonical_workspace = workspace
    .canonicalize()
    .map_err(|source| io("canonicalize workspace", workspace, source))?;
  if !canonical_workspace.is_dir() {
    return Err(OutputError::Invalid("output workspace is not a directory".to_owned()));
  }
  let artifact_count = declarations
    .iter()
    .filter(|value| value.kind() == OutputKind::Artifact)
    .count();
  let report_count = declarations
    .iter()
    .filter(|value| value.kind() == OutputKind::Report)
    .count();
  if artifact_count > limits.artifact_count as usize || report_count > limits.report_count as usize {
    return Err(OutputError::Invalid(
      "runner declarations exceed signed output count limits".to_owned(),
    ));
  }
  let mut identities = BTreeSet::new();
  for declaration in &declarations {
    if !identities.insert((
      declaration.run_id,
      declaration.task_id,
      declaration.kind(),
      declaration.name(),
    )) {
      return Err(OutputError::Invalid(format!(
        "task {} in run {} declares duplicate {:?} output '{}'",
        declaration.task_id,
        declaration.run_id,
        declaration.kind(),
        declaration.name()
      )));
    }
  }
  create_private_staging(staging_root)?;

  let mut prepared = Vec::with_capacity(declarations.len());
  let mut artifact_bytes = 0_u64;
  let mut report_bytes = 0_u64;
  for (index, declaration) in declarations.into_iter().enumerate() {
    check_cancel(cancellation)?;
    let relative = relative_path(declaration.path())?;
    let source = canonical_workspace.join(&relative);
    #[cfg(windows)]
    reject_windows_reparse_point(&source)?;
    let metadata = fs::symlink_metadata(&source).map_err(|error| io("inspect declared path", &source, error))?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
      return Err(OutputError::Invalid(format!(
        "output '{}' is not a regular file or directory",
        declaration.name()
      )));
    }
    let canonical_source = source
      .canonicalize()
      .map_err(|error| io("canonicalize declared path", &source, error))?;
    if !canonical_source.starts_with(&canonical_workspace) {
      return Err(OutputError::Invalid(format!(
        "output '{}' escapes the workspace",
        declaration.name()
      )));
    }
    if matches!(&declaration.value, DeclarationValue::Report(_)) && !metadata.is_file() {
      return Err(OutputError::Invalid(format!(
        "report '{}' must be a regular file",
        declaration.name()
      )));
    }
    let total = match declaration.kind() {
      OutputKind::Artifact => &mut artifact_bytes,
      OutputKind::Report => &mut report_bytes,
    };
    let limit = match declaration.kind() {
      OutputKind::Artifact => limits.artifact_bytes,
      OutputKind::Report => limits.report_bytes,
    };
    let remaining = limit
      .checked_sub(*total)
      .ok_or_else(|| OutputError::Invalid("staged outputs exceed signed aggregate byte limits".to_owned()))?;
    let maximum_output_bytes = remaining.min(limits.single_output_bytes);
    let staged = staging_root.join(format!("{index:08}.blob"));
    let (size_bytes, sha256) = if metadata.is_file() {
      snapshot_file(&source, &staged, maximum_output_bytes, cancellation)?
    } else {
      snapshot_directory(
        &source,
        &canonical_source,
        &staged,
        max_archive_entries,
        maximum_output_bytes,
        cancellation,
      )?
    };
    *total = total
      .checked_add(size_bytes)
      .ok_or_else(|| OutputError::Invalid("output byte count overflowed".to_owned()))?;
    if *total > limit {
      return Err(OutputError::Invalid(
        "staged outputs exceed signed aggregate byte limits".to_owned(),
      ));
    }
    let (content_type, report_format) = match &declaration.value {
      DeclarationValue::Artifact(value) => (value.content_type.clone(), None),
      DeclarationValue::Report(value) => (None, Some(value.format.clone())),
    };
    let upload = PreparedOutput {
      path: staged,
      metadata: OutputUploadMetadata {
        run_id: declaration.run_id,
        task_id: declaration.task_id,
        kind: declaration.kind(),
        name: declaration.name().to_owned(),
        content_type,
        report_format,
        transport_content_type: if metadata.is_dir() {
          DIRECTORY_ARCHIVE_CONTENT_TYPE
        } else {
          FILE_CONTENT_TYPE
        }
        .to_owned(),
        size_bytes,
        sha256,
      },
    };
    upload
      .metadata
      .validate()
      .map_err(|error| OutputError::Invalid(error.to_string()))?;
    prepared.push(upload);
  }
  Ok(prepared)
}

#[cfg(windows)]
fn reject_windows_reparse_point(path: &Path) -> Result<(), OutputError> {
  if octacity_private_fs::is_reparse_point(path).map_err(|error| io("inspect output reparse point", path, error))? {
    Err(OutputError::Invalid(format!(
      "output '{}' is a Windows reparse point",
      path.display()
    )))
  } else {
    Ok(())
  }
}

fn declarations(results: &[serde_json::Value]) -> Result<Vec<Declaration>, OutputError> {
  let mut declarations = Vec::new();
  for result in results {
    let result: ExecutionView = serde_json::from_value(result.clone())
      .map_err(|error| OutputError::Invalid(format!("runner result has invalid output declarations: {error}")))?;
    for task in result.tasks {
      declarations.extend(task.artifacts.into_iter().map(|artifact| Declaration {
        run_id: result.run_id,
        task_id: task.task_id,
        value: DeclarationValue::Artifact(artifact),
      }));
      declarations.extend(task.reports.into_iter().map(|report| Declaration {
        run_id: result.run_id,
        task_id: task.task_id,
        value: DeclarationValue::Report(report),
      }));
    }
  }
  Ok(declarations)
}

fn relative_path(value: &str) -> Result<PathBuf, OutputError> {
  if value.is_empty() || value.contains(['\\', ':', '\0']) || value.chars().any(char::is_control) {
    return Err(OutputError::Invalid(format!("output path '{value}' is not portable")));
  }
  let mut result = PathBuf::new();
  for component in Path::new(value).components() {
    match component {
      Component::Normal(component) => result.push(component),
      _ => return Err(OutputError::Invalid(format!("output path '{value}' is not normalized"))),
    }
  }
  Ok(result)
}

fn create_private_staging(path: &Path) -> Result<(), OutputError> {
  octacity_private_fs::create_private_directory(path).map_err(|source| io("create staging directory", path, source))
}

fn io(operation: &'static str, path: &Path, source: std::io::Error) -> OutputError {
  OutputError::Io {
    operation,
    path: path.to_owned(),
    source,
  }
}

#[cfg(test)]
mod tests {
  use std::fs::File;

  use sha2::{Digest as _, Sha256};

  use super::*;

  fn limits() -> OutputLimits {
    OutputLimits {
      artifact_count: 8,
      artifact_bytes: 1024 * 1024,
      report_count: 8,
      report_bytes: 1024 * 1024,
      single_output_bytes: 1024 * 1024,
    }
  }

  fn result() -> serde_json::Value {
    serde_json::json!({
      "run_id": 17,
      "tasks": [{
        "task_id": 23,
        "artifacts": [{"name": "application", "path": "dist", "content_type": "application/x-demo"}],
        "reports": [{"name": "tests", "path": "reports/result.xml", "format": "junit"}]
      }]
    })
  }

  #[tokio::test]
  async fn snapshots_files_and_deterministic_directory_archives() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir_all(workspace.join("dist/nested")).unwrap();
    fs::create_dir_all(workspace.join("reports")).unwrap();
    fs::write(workspace.join("dist/z.txt"), "z").unwrap();
    fs::write(workspace.join("dist/nested/a.txt"), "a").unwrap();
    fs::write(workspace.join("reports/result.xml"), "<testsuite/>").unwrap();

    let first = prepare(
      &workspace,
      &[result()],
      &limits(),
      &temporary.path().join("first"),
      32,
      CancellationToken::new(),
    )
    .await
    .unwrap();
    let second = prepare(
      &workspace,
      &[result()],
      &limits(),
      &temporary.path().join("second"),
      32,
      CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(first.len(), 2);
    assert_eq!(first[0].metadata, second[0].metadata);
    assert_eq!((first[0].metadata.run_id, first[0].metadata.task_id), (17, 23));
    assert_eq!(fs::read(first[0].path()).unwrap(), fs::read(second[0].path()).unwrap());
    assert_eq!(first[0].metadata.transport_content_type, DIRECTORY_ARCHIVE_CONTENT_TYPE);
    assert_eq!(first[0].metadata.size_bytes, 3584);
    assert_eq!(
      first[0].metadata.sha256,
      "ff8b318bbb040179f60ba5ec207e457c6590ad10b757fc614b0485e36eb8be35"
    );
    assert_eq!(first[1].metadata.report_format.as_deref(), Some("junit"));
    let paths = tar::Archive::new(File::open(first[0].path()).unwrap())
      .entries()
      .unwrap()
      .map(|entry| entry.unwrap().path().unwrap().into_owned())
      .collect::<Vec<_>>();
    assert_eq!(paths, ["nested", "nested/a.txt", "z.txt"].map(PathBuf::from));
  }

  #[tokio::test]
  async fn preserves_a_valid_empty_file_output() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("empty"), []).unwrap();
    let result = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "empty", "path": "empty"}]}]
    });

    let outputs = prepare(
      &workspace,
      &[result],
      &limits(),
      &temporary.path().join("staging"),
      8,
      CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(outputs[0].metadata.size_bytes, 0);
    assert_eq!(outputs[0].metadata.sha256, format!("{:x}", Sha256::digest([])));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn rejects_socket_outputs() {
    use std::os::unix::net::UnixListener;

    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let _socket = UnixListener::bind(workspace.join("service.sock")).unwrap();
    let result = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "socket", "path": "service.sock"}]}]
    });

    assert!(matches!(
      prepare(
        &workspace,
        &[result],
        &limits(),
        &temporary.path().join("staging"),
        8,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(message)) if message.contains("not a regular file or directory")
    ));
  }

  #[tokio::test]
  async fn rejects_traversal_count_byte_and_entry_limit_violations() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir_all(workspace.join("dist")).unwrap();
    fs::write(workspace.join("dist/file"), "payload").unwrap();
    fs::write(workspace.join("dist/second"), "payload").unwrap();
    let traversal = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "bad", "path": "../outside"}]}]
    });
    assert!(matches!(
      prepare(
        &workspace,
        &[traversal],
        &limits(),
        &temporary.path().join("traversal"),
        8,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(_))
    ));

    let duplicate = serde_json::json!({
      "run_id": 17,
      "tasks": [{
        "task_id": 23,
        "artifacts": [
          {"name": "bad", "path": "dist/file"},
          {"name": "bad", "path": "dist/second"}
        ]
      }]
    });
    let error = prepare(
      &workspace,
      &[duplicate],
      &limits(),
      &temporary.path().join("duplicate"),
      8,
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("duplicate"));

    let mut no_artifacts = limits();
    no_artifacts.artifact_count = 0;
    no_artifacts.artifact_bytes = 0;
    let artifact = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "bad", "path": "dist/file"}]}]
    });
    assert!(matches!(
      prepare(
        &workspace,
        &[artifact],
        &no_artifacts,
        &temporary.path().join("count"),
        8,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(_))
    ));

    let directory = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "directory", "path": "dist"}]}]
    });
    assert!(matches!(
      prepare(
        &workspace,
        std::slice::from_ref(&directory),
        &limits(),
        &temporary.path().join("entries"),
        1,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(_))
    ));

    let mut one_byte = limits();
    one_byte.single_output_bytes = 1;
    assert!(matches!(
      prepare(
        &workspace,
        &[directory],
        &one_byte,
        &temporary.path().join("bytes"),
        8,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(_))
    ));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn rejects_symlinks_that_escape_the_declared_directory() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir_all(workspace.join("dist")).unwrap();
    fs::write(workspace.join("outside"), "secret").unwrap();
    symlink("../outside", workspace.join("dist/link")).unwrap();
    let artifact = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "bad", "path": "dist"}]}]
    });
    assert!(matches!(
      prepare(
        &workspace,
        &[artifact],
        &limits(),
        &temporary.path().join("staging"),
        8,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(_))
    ));
  }

  #[cfg(windows)]
  #[tokio::test]
  async fn rejects_nested_directory_junctions() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    let artifact_root = workspace.join("dist");
    let outside = temporary.path().join("outside");
    fs::create_dir_all(&artifact_root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret"), "must not be archived").unwrap();
    let junction = artifact_root.join("junction");
    let status = std::process::Command::new("cmd")
      .args(["/C", "mklink", "/J"])
      .arg(&junction)
      .arg(&outside)
      .status()
      .unwrap();
    assert!(status.success(), "failed to create the test directory junction");
    let artifact = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "bad", "path": "dist"}]}]
    });

    assert!(matches!(
      prepare(
        &workspace,
        &[artifact],
        &limits(),
        &temporary.path().join("staging"),
        8,
        CancellationToken::new()
      )
      .await,
      Err(OutputError::Invalid(message)) if message.contains("reparse point")
    ));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn preserves_safe_symlinks_and_rejects_nonportable_entry_names() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir_all(workspace.join("dist")).unwrap();
    fs::write(workspace.join("dist/target"), "payload").unwrap();
    symlink("target", workspace.join("dist/link")).unwrap();
    let artifact = serde_json::json!({
      "run_id": 17,
      "tasks": [{"task_id": 23, "artifacts": [{"name": "safe", "path": "dist"}]}]
    });
    let prepared = prepare(
      &workspace,
      std::slice::from_ref(&artifact),
      &limits(),
      &temporary.path().join("safe-staging"),
      8,
      CancellationToken::new(),
    )
    .await
    .unwrap();
    let link_target = tar::Archive::new(File::open(prepared[0].path()).unwrap())
      .entries()
      .unwrap()
      .map(Result::unwrap)
      .find(|entry| entry.path().unwrap() == Path::new("link"))
      .unwrap()
      .link_name()
      .unwrap()
      .unwrap()
      .into_owned();
    assert_eq!(link_target, Path::new("target"));

    fs::write(workspace.join("dist/bad\\name"), "payload").unwrap();
    let error = prepare(
      &workspace,
      &[artifact],
      &limits(),
      &temporary.path().join("invalid-staging"),
      8,
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("not portable"));
  }
}
