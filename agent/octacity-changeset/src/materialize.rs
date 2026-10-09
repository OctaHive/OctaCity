//! Reconstructs one accepted candidate from its exact base and trusted bundle.

use std::{
  fs,
  path::{Path, PathBuf},
};

use async_trait::async_trait;
use octacity_private_fs::{validate_private_access, validate_trusted_owner};
use octacity_protocol::{CapturedChangeSetManifestV1, MAX_CHANGE_SET_MANIFEST_BYTES, MAX_CHANGE_SET_TOTAL_BYTES};
use processkit::{Command, ErrorReason, OutputBufferPolicy, OverflowMode};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::{GitCaptureTool, digest_file, parse_raw_diff};

const MAX_GIT_OUTPUT_BYTES: usize = MAX_CHANGE_SET_TOTAL_BYTES as usize;

/// Exact immutable inputs required to reconstruct one accepted candidate.
#[derive(Clone, Debug)]
pub struct MaterializationRequest {
  /// Freshly materialized repository whose `HEAD` must equal the original base.
  pub workspace: PathBuf,
  /// Verified protected Git bundle downloaded for this Job lease.
  pub bundle: PathBuf,
  /// Verified protected canonical capture manifest downloaded for this Job lease.
  pub manifest: PathBuf,
  /// Original immutable source revision from the signed JobSpec.
  pub expected_base_revision: String,
  /// Accepted candidate revision selected by the Factory Run.
  pub expected_candidate_revision: String,
  /// Agent-private scratch directory used only for disabled hooks.
  pub scratch: PathBuf,
}

/// Exact candidate reconstructed in a disposable source workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedChangeSet {
  /// Original exact base verified before bundle import.
  pub base_revision: String,
  /// Exact candidate commit now checked out in the workspace.
  pub candidate_revision: String,
}

/// Secret-safe failures from trusted ChangeSet reconstruction.
#[derive(Debug, Error)]
pub enum MaterializationError {
  /// Signed paths or candidate identities are malformed or inconsistent.
  #[error("invalid ChangeSet materialization request: {0}")]
  Invalid(String),
  /// Protected bundle or manifest bytes do not agree with trusted metadata.
  #[error("ChangeSet materialization integrity verification failed")]
  Integrity,
  /// Agent-private filesystem state is absent or unsafe.
  #[error("ChangeSet materialization filesystem operation failed")]
  Filesystem,
  /// The selected materialization provider could not be started or supervised.
  #[error("ChangeSet materialization provider process failed during {step}: {source}")]
  ProviderProcess {
    /// Stable operation label.
    step: &'static str,
    /// Process failure without unbounded Git output.
    source: std::io::Error,
  },
  /// The selected provider rejected an exact-base or bundle operation.
  #[error("ChangeSet materialization provider rejected the operation during {step}")]
  ProviderRejected {
    /// Stable operation label.
    step: &'static str,
  },
  /// Materialization was cancelled before runner start.
  #[error("ChangeSet materialization was cancelled")]
  Cancelled,
}

/// Trusted Agent boundary for reconstructing an accepted ChangeSet.
#[async_trait]
pub trait ChangeSetMaterializer: Send + Sync {
  /// Verifies and checks out the exact candidate without consulting a remote branch.
  async fn materialize(
    &self,
    request: MaterializationRequest,
    cancellation: CancellationToken,
  ) -> Result<MaterializedChangeSet, MaterializationError>;
}

/// Hook-free Git implementation of exact ChangeSet reconstruction.
#[derive(Clone, Debug)]
pub struct GitChangeSetMaterializer {
  tool: GitCaptureTool,
}

impl GitChangeSetMaterializer {
  /// Creates a materializer from the same operator-verified Git used for capture.
  pub fn new(tool: GitCaptureTool) -> Result<Self, MaterializationError> {
    if !tool.executable.is_absolute() {
      return Err(MaterializationError::Invalid(
        "Git executable must be absolute".to_owned(),
      ));
    }
    tool
      .identity
      .validate_identity()
      .map_err(MaterializationError::Invalid)?;
    if materialization_digest(&tool.executable)? != tool.identity.sha256 {
      return Err(MaterializationError::Invalid(
        "Git executable digest does not match operator configuration".to_owned(),
      ));
    }
    Ok(Self { tool })
  }

  async fn materialize_inner(
    &self,
    request: MaterializationRequest,
    cancellation: CancellationToken,
  ) -> Result<MaterializedChangeSet, MaterializationError> {
    validate_request_paths(&request)?;
    validate_private_access(&request.workspace).map_err(|_| MaterializationError::Filesystem)?;
    validate_trusted_owner(&request.workspace).map_err(|_| MaterializationError::Filesystem)?;
    validate_private_access(&request.scratch).map_err(|_| MaterializationError::Filesystem)?;
    validate_trusted_owner(&request.scratch).map_err(|_| MaterializationError::Filesystem)?;
    if materialization_digest(&self.tool.executable)? != self.tool.identity.sha256 {
      return Err(MaterializationError::Invalid(
        "Git executable digest changed".to_owned(),
      ));
    }

    let manifest = canonical_manifest(&request.manifest)?;
    validate_manifest(&manifest, &request, &self.tool)?;
    validate_bundle(&manifest, &request.bundle)?;

    let hooks = request.scratch.join("change-set-disabled-hooks");
    fs::create_dir(&hooks).map_err(|_| MaterializationError::Filesystem)?;
    let environment = MaterializationEnvironment { hooks: &hooks };
    let operation = async {
      let base = git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "base verification",
        ["rev-parse", "--verify", "HEAD^{commit}"],
        &cancellation,
      )
      .await?;
      if base.trim() != request.expected_base_revision {
        return Err(MaterializationError::Integrity);
      }
      git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "bundle verification",
        ["bundle", "verify", utf8_path(&request.bundle)?],
        &cancellation,
      )
      .await?;
      let heads = git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "bundle head inspection",
        ["bundle", "list-heads", utf8_path(&request.bundle)?],
        &cancellation,
      )
      .await?;
      let bundle_ref = exact_bundle_ref(&heads, &request.expected_candidate_revision)?;
      let temporary_ref = "refs/octacity/materialized/candidate";
      git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "bundle import",
        [
          "fetch",
          "--no-tags",
          "--force",
          utf8_path(&request.bundle)?,
          &format!("{bundle_ref}:{temporary_ref}"),
        ],
        &cancellation,
      )
      .await?;
      let verification =
        verify_candidate(&self.tool.executable, &request, &manifest, &environment, &cancellation).await;
      if let Err(error) = verification {
        let _ = delete_ref(
          &self.tool.executable,
          &request.workspace,
          &environment,
          temporary_ref,
          &cancellation,
        )
        .await;
        return Err(error);
      }
      git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "candidate checkout",
        ["reset", "--hard", request.expected_candidate_revision.as_str()],
        &cancellation,
      )
      .await?;
      git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "candidate cleanup",
        ["clean", "-dffx"],
        &cancellation,
      )
      .await?;
      delete_ref(
        &self.tool.executable,
        &request.workspace,
        &environment,
        temporary_ref,
        &cancellation,
      )
      .await?;
      let head = git_text(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "candidate identity verification",
        ["rev-parse", "--verify", "HEAD^{commit}"],
        &cancellation,
      )
      .await?;
      let status = git_bytes(
        &self.tool.executable,
        &request.workspace,
        &environment,
        "candidate cleanliness verification",
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        &cancellation,
      )
      .await?;
      if head.trim() != request.expected_candidate_revision || !status.is_empty() {
        return Err(MaterializationError::Integrity);
      }
      Ok(MaterializedChangeSet {
        base_revision: request.expected_base_revision.clone(),
        candidate_revision: request.expected_candidate_revision.clone(),
      })
    }
    .await;
    let cleanup = fs::remove_dir(&hooks);
    match (operation, cleanup) {
      (Ok(value), Ok(())) => Ok(value),
      (Err(error), _) => Err(error),
      (Ok(_), Err(_)) => Err(MaterializationError::Filesystem),
    }
  }
}

#[async_trait]
impl ChangeSetMaterializer for GitChangeSetMaterializer {
  async fn materialize(
    &self,
    request: MaterializationRequest,
    cancellation: CancellationToken,
  ) -> Result<MaterializedChangeSet, MaterializationError> {
    self.materialize_inner(request, cancellation).await
  }
}

async fn verify_candidate(
  git: &Path,
  request: &MaterializationRequest,
  manifest: &CapturedChangeSetManifestV1,
  environment: &MaterializationEnvironment<'_>,
  cancellation: &CancellationToken,
) -> Result<(), MaterializationError> {
  let ancestry = git_text(
    git,
    &request.workspace,
    environment,
    "candidate ancestry verification",
    [
      "rev-list",
      "--parents",
      "-n",
      "1",
      request.expected_candidate_revision.as_str(),
    ],
    cancellation,
  )
  .await?;
  let revisions = ancestry.split_ascii_whitespace().collect::<Vec<_>>();
  if revisions.as_slice()
    != [
      request.expected_candidate_revision.as_str(),
      manifest.base_revision.as_str(),
    ]
  {
    return Err(MaterializationError::Integrity);
  }
  let merge_base = git_text(
    git,
    &request.workspace,
    environment,
    "original base ancestry verification",
    [
      "merge-base",
      request.expected_base_revision.as_str(),
      request.expected_candidate_revision.as_str(),
    ],
    cancellation,
  )
  .await?;
  if merge_base.trim() != request.expected_base_revision {
    return Err(MaterializationError::Integrity);
  }
  let changed = git_bytes(
    git,
    &request.workspace,
    environment,
    "candidate manifest verification",
    [
      "diff-tree",
      "--no-commit-id",
      "-r",
      "--raw",
      "-z",
      "--no-renames",
      manifest.base_revision.as_str(),
      request.expected_candidate_revision.as_str(),
    ],
    cancellation,
  )
  .await?;
  let changed = parse_raw_diff(&changed).map_err(|_| MaterializationError::Integrity)?;
  if changed != manifest.changed_paths {
    return Err(MaterializationError::Integrity);
  }
  Ok(())
}

async fn delete_ref(
  git: &Path,
  workspace: &Path,
  environment: &MaterializationEnvironment<'_>,
  reference: &str,
  cancellation: &CancellationToken,
) -> Result<(), MaterializationError> {
  git_text(
    git,
    workspace,
    environment,
    "temporary materialization reference cleanup",
    ["update-ref", "-d", reference],
    cancellation,
  )
  .await
  .map(|_| ())
}

fn validate_request_paths(request: &MaterializationRequest) -> Result<(), MaterializationError> {
  for path in [&request.workspace, &request.scratch] {
    if !path.is_absolute() || !real_directory(path) {
      return Err(MaterializationError::Invalid(
        "materialization workspace and scratch must be real absolute directories".to_owned(),
      ));
    }
  }
  if request.workspace == request.scratch || !request.bundle.is_absolute() || !request.manifest.is_absolute() {
    return Err(MaterializationError::Invalid(
      "materialization paths must be distinct absolute paths".to_owned(),
    ));
  }
  for path in [&request.bundle, &request.manifest] {
    let metadata = fs::symlink_metadata(path).map_err(|_| MaterializationError::Filesystem)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
      return Err(MaterializationError::Invalid(
        "ChangeSet inputs must be regular files".to_owned(),
      ));
    }
  }
  if fs::read_dir(&request.scratch)
    .map_err(|_| MaterializationError::Filesystem)?
    .next()
    .is_some()
  {
    return Err(MaterializationError::Invalid(
      "materialization scratch must be empty".to_owned(),
    ));
  }
  Ok(())
}

fn canonical_manifest(path: &Path) -> Result<CapturedChangeSetManifestV1, MaterializationError> {
  if fs::symlink_metadata(path)
    .map_err(|_| MaterializationError::Filesystem)?
    .len()
    > MAX_CHANGE_SET_MANIFEST_BYTES
  {
    return Err(MaterializationError::Integrity);
  }
  let bytes = fs::read(path).map_err(|_| MaterializationError::Filesystem)?;
  CapturedChangeSetManifestV1::decode_canonical(&bytes).map_err(|_| MaterializationError::Integrity)
}

fn validate_manifest(
  manifest: &CapturedChangeSetManifestV1,
  request: &MaterializationRequest,
  tool: &GitCaptureTool,
) -> Result<(), MaterializationError> {
  if manifest.candidate_revision != request.expected_candidate_revision || manifest.capture_tool != tool.identity {
    return Err(MaterializationError::Integrity);
  }
  Ok(())
}

fn validate_bundle(manifest: &CapturedChangeSetManifestV1, path: &Path) -> Result<(), MaterializationError> {
  let metadata = fs::symlink_metadata(path).map_err(|_| MaterializationError::Filesystem)?;
  if metadata.len() != manifest.bundle.size_bytes || materialization_digest(path)? != manifest.bundle.sha256 {
    return Err(MaterializationError::Integrity);
  }
  Ok(())
}

fn exact_bundle_ref<'a>(heads: &'a str, candidate: &str) -> Result<&'a str, MaterializationError> {
  let mut lines = heads.lines();
  let line = lines.next().ok_or(MaterializationError::Integrity)?;
  if lines.next().is_some() {
    return Err(MaterializationError::Integrity);
  }
  let (revision, reference) = line.split_once(' ').ok_or(MaterializationError::Integrity)?;
  if revision != candidate || !reference.starts_with("refs/octacity/capture/") {
    return Err(MaterializationError::Integrity);
  }
  Ok(reference)
}

struct MaterializationEnvironment<'a> {
  hooks: &'a Path,
}

async fn git_text<const N: usize>(
  executable: &Path,
  workspace: &Path,
  environment: &MaterializationEnvironment<'_>,
  step: &'static str,
  arguments: [&str; N],
  cancellation: &CancellationToken,
) -> Result<String, MaterializationError> {
  let bytes = git_bytes(executable, workspace, environment, step, arguments, cancellation).await?;
  String::from_utf8(bytes).map_err(|_| MaterializationError::Integrity)
}

async fn git_bytes<const N: usize>(
  executable: &Path,
  workspace: &Path,
  environment: &MaterializationEnvironment<'_>,
  step: &'static str,
  arguments: [&str; N],
  cancellation: &CancellationToken,
) -> Result<Vec<u8>, MaterializationError> {
  let result = Command::new(executable)
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", null_device())
    .env("GIT_TERMINAL_PROMPT", "0")
    .arg("--no-optional-locks")
    .arg("-c")
    .arg(format!("core.hooksPath={}", environment.hooks.display()))
    .arg("-c")
    .arg("commit.gpgSign=false")
    .arg("-c")
    .arg("submodule.recurse=false")
    .args(arguments)
    .current_dir(workspace)
    .cancel_on(cancellation.clone())
    .output_buffer(
      OutputBufferPolicy::unbounded()
        .with_max_bytes(MAX_GIT_OUTPUT_BYTES)
        .with_overflow(OverflowMode::Error),
    )
    .output_bytes()
    .await
    .map_err(|source| match source.reason() {
      ErrorReason::Cancelled { .. } => MaterializationError::Cancelled,
      _ => MaterializationError::ProviderProcess {
        step,
        source: std::io::Error::other(source),
      },
    })?;
  if !result.is_success() {
    return Err(MaterializationError::ProviderRejected { step });
  }
  Ok(result.into_stdout())
}

fn materialization_digest(path: &Path) -> Result<String, MaterializationError> {
  digest_file(path).map_err(|_| MaterializationError::Filesystem)
}

fn real_directory(path: &Path) -> bool {
  fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
}

fn utf8_path(path: &Path) -> Result<&str, MaterializationError> {
  path
    .to_str()
    .ok_or_else(|| MaterializationError::Invalid("materialization path is not UTF-8".to_owned()))
}

const fn null_device() -> &'static str {
  if cfg!(windows) { "NUL" } else { "/dev/null" }
}
