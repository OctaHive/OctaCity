//! Captures runner-owned workspaces as immutable, reproducible evidence.
//!
//! The coding harness never supplies candidate identity. After a successful
//! writable Factory stage, the first adapter uses Git plumbing with a private
//! index and disabled hooks. The lifecycle-facing port remains provider-neutral.

#![warn(missing_docs)]

use std::{
  fs::{self, File},
  io::Read as _,
  path::{Path, PathBuf},
};

use async_trait::async_trait;
use octacity_protocol::{
  CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT, CHANGE_SET_PATCH_OUTPUT, CapturedChangeSetFileV1,
  CapturedChangeSetManifestV1, CapturedChangeSetPathV1, ChangeSetCaptureV3, FactoryImmutableReferenceV3,
  MAX_CHANGE_SET_PATH_INDEX_BYTES,
};
use processkit::{Command, ErrorReason, OutputBufferPolicy, OverflowMode, Stdin};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

mod materialize;
mod policy;

pub use materialize::{
  ChangeSetMaterializer, GitChangeSetMaterializer, MaterializationError, MaterializationRequest, MaterializedChangeSet,
};

const MAX_GIT_IDENTITY_OUTPUT_BYTES: usize = 4 * 1024;

/// Local trusted Git installation used only by the Agent capture boundary.
#[derive(Clone, Debug)]
pub struct GitCaptureTool {
  /// Absolute executable path selected by the operator.
  pub executable: PathBuf,
  /// Immutable product identity, version, and executable digest.
  pub identity: FactoryImmutableReferenceV3,
}

/// Exact immutable inputs for one post-run capture.
#[derive(Clone, Debug)]
pub struct CaptureRequest {
  /// Owned materialized Git workspace.
  pub workspace: PathBuf,
  /// Empty private directory in which capture products are created.
  pub destination: PathBuf,
  /// Direct parent of the candidate commit present at workspace `HEAD`.
  pub base_revision: String,
  /// Original exact source revision used as the self-contained bundle prerequisite.
  pub bundle_base_revision: String,
  /// Immutable Stage Attempt identity.
  pub stage_attempt_id: String,
  /// Server-selected deterministic commit instruction.
  pub instruction: ChangeSetCaptureV3,
}

/// One immutable file produced by trusted capture.
pub type CapturedFile = CapturedChangeSetFileV1;
/// One changed path and its exact Git modes.
pub type ChangedPath = CapturedChangeSetPathV1;
/// Canonical manifest stored beside the bundle and optional patch.
pub type ChangeSetManifest = CapturedChangeSetManifestV1;

/// Complete local result retained until generic output publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedChangeSet {
  /// Private Agent-owned directory containing the named capture files.
  /// This path is local lifecycle state and must never be persisted as provenance.
  pub root: PathBuf,
  /// Parsed canonical manifest.
  pub manifest: ChangeSetManifest,
  /// Manifest file metadata.
  pub manifest_file: CapturedFile,
}

/// Stable failure classes for trusted ChangeSet capture.
#[derive(Debug, Error)]
pub enum CaptureError {
  /// Input paths or signed capture fields are invalid.
  #[error("invalid ChangeSet capture request: {0}")]
  Invalid(String),
  /// Candidate content violated the immutable signed capture policy.
  #[error("ChangeSet capture policy rejected the candidate: {0}")]
  Policy(#[from] CapturePolicyViolation),
  /// Creating or reading one capture file failed.
  #[error("ChangeSet capture filesystem operation failed: {0}")]
  Filesystem(#[from] std::io::Error),
  /// The selected capture provider could not be started or supervised.
  #[error("ChangeSet capture provider process failed during {step}: {source}")]
  ProviderProcess {
    /// Stable operation label.
    step: &'static str,
    /// Process failure without unbounded Git output.
    source: std::io::Error,
  },
  /// The selected provider rejected the capture operation.
  #[error("ChangeSet capture provider rejected the operation during {step}")]
  ProviderRejected {
    /// Stable operation label.
    step: &'static str,
  },
  /// Optional patch exceeded its signed bound.
  #[error("ChangeSet patch exceeds its signed byte bound")]
  PatchLimit,
  /// Capture was cancelled before publication.
  #[error("ChangeSet capture was cancelled")]
  Cancelled,
}

/// Secret-safe reason that an untrusted candidate was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum CapturePolicyViolation {
  /// The workspace or Git metadata has an untrusted owner.
  #[error("workspace ownership is not trusted")]
  WorkspaceOwnership,
  /// The resolved repository or capture destination crossed its owned boundary.
  #[error("workspace boundary is invalid")]
  WorkspaceBoundary,
  /// The workspace or candidate is not based directly on the signed revision.
  #[error("candidate base revision does not match signed intent")]
  BaseMismatch,
  /// A changed path is malformed or outside its allowlist.
  #[error("changed path is outside capture policy")]
  Path,
  /// A protected repository control path was modified.
  #[error("repository control path is immutable")]
  ControlFile,
  /// A changed symbolic link can escape or redirect the candidate tree.
  #[error("changed symbolic link is unsafe")]
  Symlink,
  /// A submodule or nested repository boundary changed.
  #[error("submodule changes are not permitted")]
  Submodule,
  /// The changed-path count exceeds its signed bound.
  #[error("changed path count exceeds its signed bound")]
  ChangedPathCount,
  /// One candidate entry exceeds its signed byte bound.
  #[error("changed file exceeds its signed byte bound")]
  FileSize,
  /// Aggregate candidate bytes exceed their signed bound.
  #[error("changed bytes exceed their signed aggregate bound")]
  TotalBytes,
  /// A candidate entry uses an unsupported Git mode or object kind.
  #[error("changed file mode is not permitted")]
  Mode,
  /// Binary content is forbidden by signed policy.
  #[error("binary content is not permitted")]
  Binary,
  /// Candidate content matched a selected secret detector.
  #[error("candidate contains forbidden secret material")]
  Secret,
  /// Signed policy does not permit an unchanged candidate.
  #[error("empty candidate is not permitted")]
  EmptyChange,
}

/// Hook-free trusted Git capture component.
#[derive(Clone, Debug)]
pub struct GitChangeSetCapturer {
  tool: GitCaptureTool,
}

/// Trusted Agent-side capture boundary used by the job lifecycle.
#[async_trait]
pub trait ChangeSetCapturer: Send + Sync {
  /// Captures an immutable candidate from one successful writable workspace.
  async fn capture(
    &self,
    request: CaptureRequest,
    cancellation: CancellationToken,
  ) -> Result<CapturedChangeSet, CaptureError>;
}

impl GitChangeSetCapturer {
  /// Creates a capturer from an operator-verified executable identity.
  pub fn new(tool: GitCaptureTool) -> Result<Self, CaptureError> {
    if !tool.executable.is_absolute() {
      return Err(CaptureError::Invalid("Git executable must be absolute".to_owned()));
    }
    tool.identity.validate_identity().map_err(CaptureError::Invalid)?;
    if digest_file(&tool.executable)? != tool.identity.sha256 {
      return Err(CaptureError::Invalid(
        "Git executable digest does not match operator configuration".to_owned(),
      ));
    }
    Ok(Self { tool })
  }

  async fn capture_inner(
    &self,
    request: CaptureRequest,
    cancellation: CancellationToken,
  ) -> Result<CapturedChangeSet, CaptureError> {
    request.instruction.validate().map_err(CaptureError::Invalid)?;
    validate_directory(&request.workspace, false)?;
    validate_directory(&request.destination, true)?;
    policy::validate_workspace_ownership(&request.workspace, &request.destination)?;
    let actual_tool_digest = digest_file(&self.tool.executable)?;
    if actual_tool_digest != self.tool.identity.sha256 {
      return Err(CaptureError::Invalid("Git executable digest changed".to_owned()));
    }

    let repository = request.destination.join("capture.git");
    let source_objects = request.workspace.join(".git/objects").canonicalize()?;
    let object_format = source_git_text(
      &self.tool.executable,
      &request.workspace,
      "object format discovery",
      ["rev-parse", "--show-object-format"],
      &cancellation,
    )
    .await?;
    let workspace_head = source_git_text(
      &self.tool.executable,
      &request.workspace,
      "workspace HEAD verification",
      ["rev-parse", "--verify", "HEAD^{commit}"],
      &cancellation,
    )
    .await?;
    if workspace_head.trim() != request.base_revision {
      return Err(CapturePolicyViolation::BaseMismatch.into());
    }
    initialize_private_repository(
      &self.tool.executable,
      &request.destination,
      &repository,
      object_format.trim(),
      &cancellation,
    )
    .await?;
    let index = repository.join("index");
    let hooks = repository.join("hooks");
    fs::remove_dir_all(&hooks)?;
    fs::create_dir(&hooks)?;
    let environment = GitEnvironment {
      repository: &repository,
      workspace: &request.workspace,
      source_objects: &source_objects,
      index: &index,
      hooks: &hooks,
      instruction: &request.instruction,
    };
    policy::validate_repository_boundary(&self.tool.executable, &request.workspace, &environment, &cancellation)
      .await?;
    let base_object = format!("{}^{{commit}}", request.base_revision);
    let base = git_text(
      &self.tool.executable,
      &environment,
      "base verification",
      ["rev-parse", "--verify", base_object.as_str()],
      None,
      &cancellation,
    )
    .await?;
    if base.trim() != request.base_revision {
      return Err(CapturePolicyViolation::BaseMismatch.into());
    }
    git_text(
      &self.tool.executable,
      &environment,
      "bundle base ancestry verification",
      [
        "merge-base",
        "--is-ancestor",
        request.bundle_base_revision.as_str(),
        request.base_revision.as_str(),
      ],
      None,
      &cancellation,
    )
    .await
    .map_err(|_| CapturePolicyViolation::BaseMismatch)?;
    git_text(
      &self.tool.executable,
      &environment,
      "private index initialization",
      ["read-tree", request.base_revision.as_str()],
      None,
      &cancellation,
    )
    .await?;
    git_text(
      &self.tool.executable,
      &environment,
      "workspace staging",
      ["add", "--all", "--", ".", ":(exclude).git"],
      None,
      &cancellation,
    )
    .await?;
    let tree = git_text(
      &self.tool.executable,
      &environment,
      "tree creation",
      ["write-tree"],
      None,
      &cancellation,
    )
    .await?;
    let message = format!("OctaCity Factory Stage {}\n", request.stage_attempt_id);
    let candidate = git_text(
      &self.tool.executable,
      &environment,
      "commit creation",
      ["commit-tree", tree.trim(), "-p", request.base_revision.as_str()],
      Some(message.into_bytes()),
      &cancellation,
    )
    .await?;
    let candidate = candidate.trim().to_owned();

    let changed = git_bytes(
      &self.tool.executable,
      &environment,
      "changed-path manifest",
      [
        "diff-tree",
        "--no-commit-id",
        "-r",
        "--raw",
        "-z",
        "--no-renames",
        request.base_revision.as_str(),
        candidate.as_str(),
      ],
      None,
      MAX_CHANGE_SET_PATH_INDEX_BYTES as usize,
      &cancellation,
    )
    .await?;
    let changed_paths = parse_raw_diff(&changed)?;
    let contains_binary = policy::validate_candidate(
      policy::CandidateValidation {
        git: &self.tool.executable,
        workspace: &request.workspace,
        environment: &environment,
        base: &request.base_revision,
        candidate: &candidate,
        cancellation: &cancellation,
      },
      &changed_paths,
      &request.instruction.policy,
    )
    .await?;

    let capture_ref = format!("refs/octacity/capture/{}", request.stage_attempt_id);
    git_text(
      &self.tool.executable,
      &environment,
      "temporary capture reference",
      ["update-ref", capture_ref.as_str(), candidate.as_str()],
      None,
      &cancellation,
    )
    .await?;
    let bundle_path = request.destination.join(CHANGE_SET_BUNDLE_OUTPUT);
    let bundle_result = git_text(
      &self.tool.executable,
      &environment,
      "bundle creation",
      [
        "bundle",
        "create",
        bundle_path
          .to_str()
          .ok_or_else(|| CaptureError::Invalid("bundle path is not UTF-8".to_owned()))?,
        capture_ref.as_str(),
        &format!("^{}", request.bundle_base_revision),
      ],
      None,
      &cancellation,
    )
    .await;
    let remove_ref = git_text(
      &self.tool.executable,
      &environment,
      "temporary capture reference cleanup",
      ["update-ref", "-d", capture_ref.as_str()],
      None,
      &cancellation,
    )
    .await;
    bundle_result?;
    remove_ref?;
    let bundle = captured_file(&bundle_path)?;

    let patch = if let Some(max_bytes) = request.instruction.patch_max_bytes {
      let bytes = if contains_binary {
        git_bytes(
          &self.tool.executable,
          &environment,
          "binary patch creation",
          [
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            request.base_revision.as_str(),
            candidate.as_str(),
          ],
          None,
          max_bytes as usize,
          &cancellation,
        )
        .await?
      } else {
        git_bytes(
          &self.tool.executable,
          &environment,
          "patch creation",
          [
            "diff",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            request.base_revision.as_str(),
            candidate.as_str(),
          ],
          None,
          max_bytes as usize,
          &cancellation,
        )
        .await?
      };
      if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(CaptureError::PatchLimit);
      }
      policy::validate_patch(&bytes, &request.instruction.policy.forbidden_secret_patterns)?;
      let path = request.destination.join(CHANGE_SET_PATCH_OUTPUT);
      fs::write(&path, bytes)?;
      Some(captured_file(&path)?)
    } else {
      None
    };

    let root = request.destination.clone();
    let manifest = ChangeSetManifest {
      format_version: 1,
      base_revision: request.base_revision,
      candidate_revision: candidate,
      stage_attempt_id: request.stage_attempt_id,
      capture_tool: self.tool.identity.clone(),
      changed_paths,
      bundle,
      patch,
    };
    manifest.validate().map_err(CaptureError::Invalid)?;
    let manifest_path = request.destination.join(CHANGE_SET_MANIFEST_OUTPUT);
    let mut bytes = serde_json::to_vec(&manifest).map_err(|error| CaptureError::Invalid(error.to_string()))?;
    bytes.push(b'\n');
    fs::write(&manifest_path, bytes)?;
    let manifest_file = captured_file(&manifest_path)?;
    fs::remove_dir_all(repository)?;
    Ok(CapturedChangeSet {
      root,
      manifest,
      manifest_file,
    })
  }
}

#[async_trait]
impl ChangeSetCapturer for GitChangeSetCapturer {
  async fn capture(
    &self,
    request: CaptureRequest,
    cancellation: CancellationToken,
  ) -> Result<CapturedChangeSet, CaptureError> {
    let destination = request.destination.clone();
    let result = self.capture_inner(request, cancellation).await;
    if result.is_err() {
      cleanup_capture_destination(&destination)?;
    }
    result
  }
}

#[cfg(test)]
mod tests;

pub(crate) struct GitEnvironment<'a> {
  repository: &'a Path,
  workspace: &'a Path,
  source_objects: &'a Path,
  index: &'a Path,
  hooks: &'a Path,
  instruction: &'a ChangeSetCaptureV3,
}

async fn source_git_text<const N: usize>(
  executable: &Path,
  workspace: &Path,
  step: &'static str,
  arguments: [&str; N],
  cancellation: &CancellationToken,
) -> Result<String, CaptureError> {
  let result = Command::new(executable)
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", null_device())
    .env("GIT_TERMINAL_PROMPT", "0")
    .args(arguments)
    .current_dir(workspace)
    .cancel_on(cancellation.clone())
    .output_buffer(
      OutputBufferPolicy::unbounded()
        .with_max_bytes(MAX_GIT_IDENTITY_OUTPUT_BYTES)
        .with_overflow(OverflowMode::Error),
    )
    .output_bytes()
    .await
    .map_err(|source| process_error(step, source))?;
  if !result.is_success() {
    return Err(CaptureError::ProviderRejected { step });
  }
  String::from_utf8(result.into_stdout()).map_err(|_| CaptureError::Invalid(format!("Git {step} output is not UTF-8")))
}

async fn initialize_private_repository(
  executable: &Path,
  destination: &Path,
  repository: &Path,
  object_format: &str,
  cancellation: &CancellationToken,
) -> Result<(), CaptureError> {
  if !matches!(object_format, "sha1" | "sha256") {
    return Err(CaptureError::Invalid(
      "Git repository uses an unsupported object format".to_owned(),
    ));
  }
  let repository = repository
    .to_str()
    .ok_or_else(|| CaptureError::Invalid("private Git repository path is not UTF-8".to_owned()))?;
  let object_format = format!("--object-format={object_format}");
  let result = Command::new(executable)
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", null_device())
    .env("GIT_TERMINAL_PROMPT", "0")
    .args(["init", "--quiet", "--bare", object_format.as_str(), repository])
    .current_dir(destination)
    .cancel_on(cancellation.clone())
    .output_buffer(
      OutputBufferPolicy::unbounded()
        .with_max_bytes(MAX_GIT_IDENTITY_OUTPUT_BYTES)
        .with_overflow(OverflowMode::Error),
    )
    .output_bytes()
    .await
    .map_err(|source| process_error("private repository initialization", source))?;
  if !result.is_success() {
    return Err(CaptureError::ProviderRejected {
      step: "private repository initialization",
    });
  }
  Ok(())
}

pub(crate) async fn git_text<const N: usize>(
  executable: &Path,
  environment: &GitEnvironment<'_>,
  step: &'static str,
  arguments: [&str; N],
  stdin: Option<Vec<u8>>,
  cancellation: &CancellationToken,
) -> Result<String, CaptureError> {
  let bytes = git_bytes(
    executable,
    environment,
    step,
    arguments,
    stdin,
    MAX_GIT_IDENTITY_OUTPUT_BYTES,
    cancellation,
  )
  .await?;
  String::from_utf8(bytes).map_err(|_| CaptureError::Invalid(format!("Git {step} output is not UTF-8")))
}

pub(crate) async fn git_bytes<const N: usize>(
  executable: &Path,
  environment: &GitEnvironment<'_>,
  step: &'static str,
  arguments: [&str; N],
  stdin: Option<Vec<u8>>,
  max_output_bytes: usize,
  cancellation: &CancellationToken,
) -> Result<Vec<u8>, CaptureError> {
  let timestamp = format!("{} +0000", environment.instruction.committed_at);
  let mut command = Command::new(executable)
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", null_device())
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_DIR", environment.repository)
    .env("GIT_WORK_TREE", environment.workspace)
    .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", environment.source_objects)
    .env("GIT_INDEX_FILE", environment.index)
    .env("GIT_AUTHOR_NAME", &environment.instruction.author_name)
    .env("GIT_AUTHOR_EMAIL", &environment.instruction.author_email)
    .env("GIT_AUTHOR_DATE", &timestamp)
    .env("GIT_COMMITTER_NAME", &environment.instruction.author_name)
    .env("GIT_COMMITTER_EMAIL", &environment.instruction.author_email)
    .env("GIT_COMMITTER_DATE", &timestamp)
    .arg("--no-optional-locks")
    .arg("-c")
    .arg("core.bare=false")
    .arg("-c")
    .arg(format!("core.hooksPath={}", environment.hooks.display()))
    .arg("-c")
    .arg("commit.gpgSign=false")
    .args(arguments)
    .current_dir(environment.workspace)
    .cancel_on(cancellation.clone())
    .output_buffer(
      OutputBufferPolicy::unbounded()
        .with_max_bytes(max_output_bytes)
        .with_overflow(OverflowMode::Error),
    );
  if let Some(bytes) = stdin {
    command = command.stdin(Stdin::from_bytes(bytes));
  }
  let result = command
    .output_bytes()
    .await
    .map_err(|source| process_error(step, source))?;
  if !result.is_success() {
    return Err(CaptureError::ProviderRejected { step });
  }
  Ok(result.into_stdout())
}

fn process_error(step: &'static str, source: processkit::Error) -> CaptureError {
  match source.reason() {
    ErrorReason::Cancelled { .. } => CaptureError::Cancelled,
    _ => CaptureError::ProviderProcess {
      step,
      source: std::io::Error::other(source),
    },
  }
}

fn parse_raw_diff(bytes: &[u8]) -> Result<Vec<ChangedPath>, CaptureError> {
  let mut fields = bytes.split(|byte| *byte == 0).filter(|field| !field.is_empty());
  let mut paths = Vec::new();
  while let Some(header) = fields.next() {
    let header =
      std::str::from_utf8(header).map_err(|_| CaptureError::Invalid("Git diff header is not UTF-8".to_owned()))?;
    let mut header = header.strip_prefix(':').unwrap_or(header).split_ascii_whitespace();
    let old_mode = header
      .next()
      .ok_or_else(|| CaptureError::Invalid("Git diff omitted old mode".to_owned()))?;
    let new_mode = header
      .next()
      .ok_or_else(|| CaptureError::Invalid("Git diff omitted new mode".to_owned()))?;
    let _old_object = header
      .next()
      .ok_or_else(|| CaptureError::Invalid("Git diff omitted old object".to_owned()))?;
    let _new_object = header
      .next()
      .ok_or_else(|| CaptureError::Invalid("Git diff omitted new object".to_owned()))?;
    let status = header
      .next()
      .ok_or_else(|| CaptureError::Invalid("Git diff omitted status".to_owned()))?;
    let path = fields
      .next()
      .ok_or_else(|| CaptureError::Invalid("Git diff omitted path".to_owned()))?;
    paths.push(ChangedPath {
      status: status.to_owned(),
      path: std::str::from_utf8(path)
        .map_err(|_| CaptureError::Invalid("changed path is not UTF-8".to_owned()))?
        .to_owned(),
      old_mode: old_mode.to_owned(),
      new_mode: new_mode.to_owned(),
    });
  }
  paths.sort_by(|left, right| left.path.cmp(&right.path));
  Ok(paths)
}

fn validate_directory(path: &Path, empty: bool) -> Result<(), CaptureError> {
  if !path.is_absolute() {
    return Err(CaptureError::Invalid("capture paths must be absolute".to_owned()));
  }
  let metadata = fs::symlink_metadata(path)?;
  if !metadata.is_dir() || metadata.file_type().is_symlink() {
    return Err(CaptureError::Invalid(
      "capture path must be a real directory".to_owned(),
    ));
  }
  if empty && fs::read_dir(path)?.next().is_some() {
    return Err(CaptureError::Invalid("capture destination must be empty".to_owned()));
  }
  Ok(())
}

fn cleanup_capture_destination(destination: &Path) -> Result<(), CaptureError> {
  for entry in fs::read_dir(destination)? {
    let path = entry?.path();
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
      fs::remove_dir_all(path)?;
    } else {
      fs::remove_file(path)?;
    }
  }
  Ok(())
}

fn captured_file(path: &Path) -> Result<CapturedFile, CaptureError> {
  let metadata = fs::symlink_metadata(path)?;
  if !metadata.is_file() || metadata.file_type().is_symlink() {
    return Err(CaptureError::Invalid(
      "capture output must be a regular file".to_owned(),
    ));
  }
  Ok(CapturedFile {
    name: path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| CaptureError::Invalid("capture output name is not UTF-8".to_owned()))?
      .to_owned(),
    size_bytes: metadata.len(),
    sha256: digest_file(path)?,
  })
}

fn digest_file(path: &Path) -> Result<String, CaptureError> {
  let mut file = File::open(path)?;
  let mut digest = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let read = file.read(&mut buffer)?;
    if read == 0 {
      break;
    }
    digest.update(&buffer[..read]);
  }
  Ok(hex::encode(digest.finalize()))
}

const fn null_device() -> &'static str {
  if cfg!(windows) { "NUL" } else { "/dev/null" }
}
