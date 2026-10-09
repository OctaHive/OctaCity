//! Fail-closed validation of untrusted candidate paths and Git blobs.

use std::{fs, path::Path};

use octacity_protocol::{
  ChangeSetBinaryPolicyV3, ChangeSetCapturePolicyV3, ChangeSetSecretPatternV3, MAX_FACTORY_PATH_BYTES,
};
use tokio_util::sync::CancellationToken;

use crate::{CaptureError, CapturePolicyViolation, ChangedPath, GitEnvironment, git_bytes, git_text};

pub(crate) fn validate_workspace_ownership(workspace: &Path, destination: &Path) -> Result<(), CaptureError> {
  let workspace = workspace.canonicalize()?;
  let destination = destination.canonicalize()?;
  if workspace.starts_with(&destination) || destination.starts_with(&workspace) {
    return Err(CapturePolicyViolation::WorkspaceBoundary.into());
  }
  octacity_private_fs::validate_trusted_owner(&workspace).map_err(|_| CapturePolicyViolation::WorkspaceOwnership)?;
  let git_directory = workspace.join(".git");
  let metadata = fs::symlink_metadata(&git_directory).map_err(|_| CapturePolicyViolation::WorkspaceBoundary)?;
  if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&git_directory)? {
    return Err(CapturePolicyViolation::WorkspaceBoundary.into());
  }
  octacity_private_fs::validate_trusted_owner(&git_directory)
    .map_err(|_| CapturePolicyViolation::WorkspaceOwnership)?;
  Ok(())
}

pub(crate) async fn validate_repository_boundary(
  git: &Path,
  workspace: &Path,
  environment: &GitEnvironment<'_>,
  cancellation: &CancellationToken,
) -> Result<(), CaptureError> {
  let top = git_text(
    git,
    environment,
    "repository boundary verification",
    ["rev-parse", "--show-toplevel"],
    None,
    cancellation,
  )
  .await?;
  let top = Path::new(top.trim())
    .canonicalize()
    .map_err(|_| CapturePolicyViolation::WorkspaceBoundary)?;
  if top != workspace.canonicalize()? {
    return Err(CapturePolicyViolation::WorkspaceBoundary.into());
  }
  Ok(())
}

pub(crate) struct CandidateValidation<'a> {
  pub git: &'a Path,
  pub workspace: &'a Path,
  pub environment: &'a GitEnvironment<'a>,
  pub base: &'a str,
  pub candidate: &'a str,
  pub cancellation: &'a CancellationToken,
}

pub(crate) async fn validate_candidate(
  validation: CandidateValidation<'_>,
  changed_paths: &[ChangedPath],
  policy: &ChangeSetCapturePolicyV3,
) -> Result<bool, CaptureError> {
  if changed_paths.is_empty() && !policy.allow_empty {
    return Err(CapturePolicyViolation::EmptyChange.into());
  }
  if changed_paths.len() > policy.max_changed_paths as usize {
    return Err(CapturePolicyViolation::ChangedPathCount.into());
  }
  git_text(
    validation.git,
    validation.environment,
    "candidate ancestry verification",
    ["merge-base", "--is-ancestor", validation.base, validation.candidate],
    None,
    validation.cancellation,
  )
  .await
  .map_err(|_| CaptureError::from(CapturePolicyViolation::BaseMismatch))?;
  let parent = git_text(
    validation.git,
    validation.environment,
    "candidate parent verification",
    ["rev-parse", &format!("{}^", validation.candidate)],
    None,
    validation.cancellation,
  )
  .await?;
  if parent.trim() != validation.base {
    return Err(CapturePolicyViolation::BaseMismatch.into());
  }

  let mut total_bytes = 0_u64;
  let mut contains_binary = false;
  for changed in changed_paths {
    validate_changed_path(changed, policy)?;
    if changed.new_mode == "000000" {
      continue;
    }
    validate_materialized_owner(validation.workspace, &changed.path)?;
    let object = format!("{}:{}", validation.candidate, changed.path);
    let size = git_text(
      validation.git,
      validation.environment,
      "changed blob size inspection",
      ["cat-file", "-s", object.as_str()],
      None,
      validation.cancellation,
    )
    .await?
    .trim()
    .parse::<u64>()
    .map_err(|_| CapturePolicyViolation::FileSize)?;
    if size > policy.max_file_bytes {
      return Err(CapturePolicyViolation::FileSize.into());
    }
    total_bytes = total_bytes
      .checked_add(size)
      .ok_or(CapturePolicyViolation::TotalBytes)?;
    if total_bytes > policy.max_total_bytes {
      return Err(CapturePolicyViolation::TotalBytes.into());
    }
    let bytes = git_bytes(
      validation.git,
      validation.environment,
      "changed blob inspection",
      ["cat-file", "blob", object.as_str()],
      None,
      policy.max_file_bytes as usize,
      validation.cancellation,
    )
    .await?;
    if bytes.len() as u64 != size {
      return Err(CapturePolicyViolation::FileSize.into());
    }
    if changed.new_mode == "120000" {
      validate_symlink_target(&changed.path, &bytes)?;
    } else if is_binary(&bytes) {
      contains_binary = true;
      if policy.binary_policy == ChangeSetBinaryPolicyV3::Reject {
        return Err(CapturePolicyViolation::Binary.into());
      }
    }
    if policy
      .forbidden_secret_patterns
      .iter()
      .any(|pattern| matches_secret(*pattern, &bytes))
    {
      return Err(CapturePolicyViolation::Secret.into());
    }
  }
  Ok(contains_binary)
}

fn validate_changed_path(changed: &ChangedPath, policy: &ChangeSetCapturePolicyV3) -> Result<(), CaptureError> {
  if !matches!(changed.status.as_str(), "A" | "D" | "M" | "T") {
    return Err(CapturePolicyViolation::Path.into());
  }
  validate_portable_path(&changed.path)?;
  if !policy
    .allowed_path_prefixes
    .iter()
    .any(|prefix| path_matches_prefix(&changed.path, prefix))
  {
    return Err(CapturePolicyViolation::Path.into());
  }
  if policy
    .forbidden_control_paths
    .iter()
    .any(|prefix| path_matches_prefix(&changed.path, prefix))
  {
    return Err(CapturePolicyViolation::ControlFile.into());
  }
  for mode in [&changed.old_mode, &changed.new_mode] {
    if !matches!(mode.as_str(), "000000" | "100644" | "100755" | "120000") {
      return if mode == "160000" {
        Err(CapturePolicyViolation::Submodule.into())
      } else {
        Err(CapturePolicyViolation::Mode.into())
      };
    }
  }
  Ok(())
}

fn validate_portable_path(path: &str) -> Result<(), CaptureError> {
  if path.is_empty()
    || path.len() > MAX_FACTORY_PATH_BYTES
    || path.starts_with('/')
    || path.ends_with('/')
    || path.contains(['\\', ':', '\0'])
    || path.chars().any(char::is_control)
    || path
      .split('/')
      .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
  {
    Err(CapturePolicyViolation::Path.into())
  } else {
    Ok(())
  }
}

fn path_matches_prefix(path: &str, prefix: &str) -> bool {
  prefix == "." || path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

fn validate_materialized_owner(workspace: &Path, path: &str) -> Result<(), CaptureError> {
  let materialized = workspace.join(path);
  let metadata = fs::symlink_metadata(&materialized).map_err(|_| CapturePolicyViolation::WorkspaceBoundary)?;
  if is_reparse_point(&materialized)? {
    return Err(CapturePolicyViolation::Symlink.into());
  }
  octacity_private_fs::validate_trusted_owner(&materialized).map_err(|_| CapturePolicyViolation::WorkspaceOwnership)?;
  if metadata.file_type().is_symlink() || metadata.is_file() {
    Ok(())
  } else {
    Err(CapturePolicyViolation::Mode.into())
  }
}

fn validate_symlink_target(path: &str, bytes: &[u8]) -> Result<(), CaptureError> {
  let target = std::str::from_utf8(bytes).map_err(|_| CapturePolicyViolation::Symlink)?;
  if target.is_empty()
    || target.starts_with('/')
    || target.contains(['\\', ':', '\0'])
    || target.chars().any(char::is_control)
  {
    return Err(CapturePolicyViolation::Symlink.into());
  }
  let mut resolved = path.split('/').collect::<Vec<_>>();
  resolved.pop();
  for segment in target.split('/') {
    match segment {
      "" | "." => {}
      ".." if resolved.pop().is_none() => return Err(CapturePolicyViolation::Symlink.into()),
      ".." => {}
      value => resolved.push(value),
    }
  }
  Ok(())
}

fn is_binary(bytes: &[u8]) -> bool {
  bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

fn matches_secret(pattern: ChangeSetSecretPatternV3, bytes: &[u8]) -> bool {
  match pattern {
    ChangeSetSecretPatternV3::PemPrivateKey => contains(bytes, b"-----BEGIN") && contains(bytes, b"PRIVATE KEY-----"),
    ChangeSetSecretPatternV3::AwsAccessKeyId => prefixed_token(bytes, &[b"AKIA", b"ASIA"], 16, aws_key_byte),
    ChangeSetSecretPatternV3::GitHubToken => prefixed_token(
      bytes,
      &[b"ghp_", b"gho_", b"ghu_", b"ghs_", b"ghr_", b"github_pat_"],
      20,
      token_byte,
    ),
    ChangeSetSecretPatternV3::OpenAiApiKey => {
      prefixed_token(bytes, &[b"sk-proj-", b"sk-svcacct-"], 20, token_byte)
        || prefixed_token(bytes, &[b"sk-"], 32, token_byte)
    }
  }
}

pub(crate) fn validate_patch(
  bytes: &[u8],
  forbidden_patterns: &[ChangeSetSecretPatternV3],
) -> Result<(), CaptureError> {
  if forbidden_patterns.iter().any(|pattern| matches_secret(*pattern, bytes)) {
    Err(CapturePolicyViolation::Secret.into())
  } else {
    Ok(())
  }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
  !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn prefixed_token(bytes: &[u8], prefixes: &[&[u8]], suffix_length: usize, allowed: fn(u8) -> bool) -> bool {
  prefixes.iter().any(|prefix| {
    bytes
      .windows(prefix.len() + suffix_length)
      .any(|window| window.starts_with(prefix) && window[prefix.len()..].iter().copied().all(allowed))
  })
}

fn aws_key_byte(byte: u8) -> bool {
  byte.is_ascii_uppercase() || byte.is_ascii_digit()
}

fn token_byte(byte: u8) -> bool {
  byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> Result<bool, CaptureError> {
  octacity_private_fs::is_reparse_point(path).map_err(CaptureError::Filesystem)
}

#[cfg(not(windows))]
fn is_reparse_point(_path: &Path) -> Result<bool, CaptureError> {
  Ok(false)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_nonportable_and_traversing_changed_paths() {
    let policy = ChangeSetCapturePolicyV3 {
      allowed_path_prefixes: vec![".".to_owned()],
      forbidden_control_paths: Vec::new(),
      max_changed_paths: 1,
      max_file_bytes: 1,
      max_total_bytes: 1,
      allow_empty: false,
      binary_policy: ChangeSetBinaryPolicyV3::Reject,
      forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
    };
    for path in ["../escape", "/absolute", "nested\\escape", "nested//file"] {
      let changed = ChangedPath {
        status: "A".to_owned(),
        path: path.to_owned(),
        old_mode: "000000".to_owned(),
        new_mode: "100644".to_owned(),
      };
      assert!(matches!(
        validate_changed_path(&changed, &policy),
        Err(CaptureError::Policy(CapturePolicyViolation::Path))
      ));
    }
  }

  #[test]
  fn recognizes_every_stable_secret_pattern_without_echoing_values() {
    let examples: [(ChangeSetSecretPatternV3, &[u8]); 4] = [
      (
        ChangeSetSecretPatternV3::PemPrivateKey,
        b"-----BEGIN PRIVATE KEY-----\nprivate\n-----END PRIVATE KEY-----",
      ),
      (ChangeSetSecretPatternV3::AwsAccessKeyId, b"AKIA0123456789ABCDEF"),
      (
        ChangeSetSecretPatternV3::GitHubToken,
        b"github_pat_0123456789abcdefghij",
      ),
      (ChangeSetSecretPatternV3::OpenAiApiKey, b"sk-proj-0123456789abcdefghij"),
    ];
    for (pattern, value) in examples {
      assert!(matches_secret(pattern, value));
    }
  }
}
