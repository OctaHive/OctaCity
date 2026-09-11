//! Validates external URLs, filesystem trust, keys, and runtime-specific settings.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::VerifyingKey;
use http::Uri;

use super::{ConfigError, OciEngineConfig};

/// Validates that configured OCI engines exactly implement the enabled mode.
///
/// Duplicate engine kinds are rejected because routing must never depend on
/// configuration order.
pub(super) fn validate_oci_engines(engines: &mut [OciEngineConfig], oci_enabled: bool) -> Result<(), ConfigError> {
  if oci_enabled && engines.is_empty() {
    return invalid("Oci runtime requires at least one explicitly configured oci_engine");
  }
  if !oci_enabled && !engines.is_empty() {
    return invalid("oci_engines are only valid when the Oci runtime is enabled");
  }
  let mut kinds = BTreeSet::new();
  for engine in engines {
    let kind = match engine {
      OciEngineConfig::Microsandbox {
        executable,
        libkrunfw,
        metrics_sample_interval_seconds,
      } => {
        *executable = canonical_regular_file("Microsandbox executable", executable)?;
        *libkrunfw = canonical_regular_file("Microsandbox libkrunfw", libkrunfw)?;
        if *metrics_sample_interval_seconds == 0 {
          return invalid("Microsandbox metrics_sample_interval_seconds must be greater than zero");
        }
        "microsandbox"
      }
      OciEngineConfig::Containerd {
        endpoint,
        namespace,
        snapshotter,
        runtime,
        registry_config_dir,
        pids_limit,
        open_files_limit,
      } => {
        if !endpoint.is_absolute() {
          return invalid("containerd endpoint must be an absolute path");
        }
        non_empty("containerd namespace", namespace)?;
        non_empty("containerd snapshotter", snapshotter)?;
        non_empty("containerd runtime", runtime)?;
        if *pids_limit == 0 || *open_files_limit == 0 {
          return invalid("containerd pids_limit and open_files_limit must be greater than zero");
        }
        if let Some(path) = registry_config_dir {
          *path = canonical_directory("containerd registry_config_dir", path)?;
        }
        "containerd"
      }
    };
    if !kinds.insert(kind) {
      return invalid(format!("oci_engines must not contain duplicate '{kind}' engines"));
    }
  }
  Ok(())
}

/// Validates the complete, explicit environment inherited by Native jobs.
pub(super) fn validate_native_environment(environment: &BTreeMap<String, String>) -> Result<(), ConfigError> {
  if environment.get("PATH").is_none_or(|path| path.trim().is_empty()) {
    return invalid("native_environment must define a non-empty PATH");
  }
  for (name, value) in environment {
    if name.is_empty() || name.contains(['=', '\0']) || value.contains('\0') {
      return invalid("native_environment contains an invalid name or NUL byte");
    }
  }
  Ok(())
}

/// Rejects non-files and returns a canonical path for later trust checks.
pub(super) fn canonical_regular_file(name: &str, path: &Path) -> Result<PathBuf, ConfigError> {
  validate_regular_file(name, path)?;
  path
    .canonicalize()
    .map_err(|error| ConfigError::Invalid(format!("{name}: {error}")))
}

/// Resolves a path once so runtime code never depends on a mutable working directory.
pub(super) fn canonical_path(name: &str, path: &Path) -> Result<PathBuf, ConfigError> {
  path
    .canonicalize()
    .map_err(|error| ConfigError::Invalid(format!("{name} '{}': {error}", path.display())))
}

/// Requires the coordinator endpoint to use authenticated TLS.
pub(super) fn validate_server_url(value: &str) -> Result<(), ConfigError> {
  let url = parse_absolute_url("server_url", value)?;
  if url.scheme_str() != Some("https") {
    return invalid("server_url must use https");
  }
  if url.path_and_query().is_some_and(|path| path.as_str() != "/") {
    return invalid("server_url must contain only scheme, host, and optional port");
  }
  Ok(())
}

/// Accepts only origins, preventing signed upload URLs from widening authority.
pub(super) fn validate_upload_origin(value: &str) -> Result<(), ConfigError> {
  let url = parse_absolute_url("upload origin", value)?;
  let local_http = url.scheme_str() == Some("http") && matches!(url.host(), Some("127.0.0.1" | "::1" | "localhost"));
  if url.scheme_str() != Some("https") && !local_http {
    return invalid(format!(
      "upload origin '{value}' must use https (or loopback http for development)"
    ));
  }
  if url.path_and_query().is_some_and(|path| path.as_str() != "/") {
    return invalid(format!(
      "upload origin '{value}' must contain only scheme, host, and optional port"
    ));
  }
  Ok(())
}

fn parse_absolute_url(name: &str, value: &str) -> Result<Uri, ConfigError> {
  let uri: Uri = value
    .parse()
    .map_err(|_| ConfigError::Invalid(format!("{name} '{value}' is not a valid absolute URL")))?;
  let authority = uri
    .authority()
    .ok_or_else(|| ConfigError::Invalid(format!("{name} '{value}' has no authority")))?;
  if uri.scheme().is_none() || uri.host().is_none() || authority.as_str().contains('@') {
    return invalid(format!("{name} '{value}' must not contain user information"));
  }
  Ok(uri)
}

/// Decodes one explicitly identified Ed25519 server verification key.
pub(super) fn decode_signing_key(id: &str, encoded: &str) -> Result<(String, VerifyingKey), ConfigError> {
  non_empty("server signing key id", id)?;
  let bytes = BASE64
    .decode(encoded)
    .map_err(|_| ConfigError::Invalid(format!("server signing key '{id}' is not valid base64")))?;
  let bytes: [u8; 32] = bytes
    .try_into()
    .map_err(|_| ConfigError::Invalid(format!("server signing key '{id}' must contain 32 bytes")))?;
  let key = VerifyingKey::from_bytes(&bytes)
    .map_err(|_| ConfigError::Invalid(format!("server signing key '{id}' is not a valid Ed25519 public key")))?;
  Ok((id.to_owned(), key))
}

/// Canonicalizes an operator-owned directory and rejects unsafe Unix permissions.
pub(super) fn canonical_directory(name: &str, path: &Path) -> Result<PathBuf, ConfigError> {
  if !path.is_absolute() {
    return invalid(format!("{name} must be absolute"));
  }
  let canonical = path
    .canonicalize()
    .map_err(|error| ConfigError::Invalid(format!("{name} '{}': {error}", path.display())))?;
  if !canonical.is_dir() {
    return invalid(format!("{name} '{}' is not a directory", path.display()));
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(&canonical)
      .map_err(|error| ConfigError::Invalid(format!("{name} '{}': {error}", canonical.display())))?
      .permissions()
      .mode();
    if mode & 0o022 != 0 {
      return invalid(format!(
        "{name} '{}' must not be writable by group or other users",
        canonical.display()
      ));
    }
  }
  Ok(canonical)
}

/// Prevents cleanup for one configured root from reaching another root.
pub(super) fn validate_distinct_roots(roots: &[(&str, PathBuf)]) -> Result<(), ConfigError> {
  for (index, (left_name, left)) in roots.iter().enumerate() {
    for (right_name, right) in &roots[index + 1..] {
      if left.starts_with(right) || right.starts_with(left) {
        return invalid(format!(
          "{left_name} '{}' and {right_name} '{}' must not overlap",
          left.display(),
          right.display()
        ));
      }
    }
  }
  Ok(())
}

/// Checks a path without following a final symlink.
pub(super) fn validate_regular_file(name: &str, path: &Path) -> Result<(), ConfigError> {
  if !path.is_absolute() {
    return invalid(format!("{name} must be absolute"));
  }
  let metadata = fs::symlink_metadata(path)
    .map_err(|error| ConfigError::Invalid(format!("{name} '{}': {error}", path.display())))?;
  if !metadata.file_type().is_file() {
    return invalid(format!("{name} '{}' must be a regular file", path.display()));
  }
  Ok(())
}

#[cfg(unix)]
/// Ensures enrollment credentials are readable only by their owner.
pub(super) fn validate_credential_permissions(path: &Path) -> Result<(), ConfigError> {
  use std::os::unix::fs::PermissionsExt as _;

  let mode = fs::metadata(path)
    .map_err(|error| ConfigError::Invalid(format!("credential_file '{}': {error}", path.display())))?
    .permissions()
    .mode();
  if mode & 0o077 != 0 {
    return invalid(format!(
      "credential_file '{}' must not be accessible by group or others",
      path.display()
    ));
  }
  Ok(())
}

#[cfg(not(unix))]
pub(super) fn validate_credential_permissions(_path: &Path) -> Result<(), ConfigError> {
  Ok(())
}

pub(super) fn non_empty(name: &str, value: &str) -> Result<(), ConfigError> {
  if value.trim().is_empty() {
    invalid(format!("{name} must not be empty"))
  } else {
    Ok(())
  }
}

pub(super) fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
  Err(ConfigError::Invalid(message.into()))
}
