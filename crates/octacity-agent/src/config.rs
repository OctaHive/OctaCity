use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::{Path, PathBuf},
};

use crate::source::{RegistryError, SourcePluginRegistry};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::VerifyingKey;
use http::Uri;
use octacity_protocol::BackendKind;
use serde::Deserialize;
use thiserror::Error;
use tracing::{debug, info};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
  pub agent_id: String,
  pub server_url: String,
  pub credential_file: PathBuf,
  pub server_signing_keys: BTreeMap<String, String>,
  #[serde(default)]
  pub labels: BTreeMap<String, String>,
  pub work_root: PathBuf,
  pub state_root: PathBuf,
  pub octa_release_root: PathBuf,
  pub source_plugins_dir: PathBuf,
  pub enabled_execution_backends: Vec<BackendKind>,
  #[serde(default)]
  pub allow_native_execution: bool,
  #[serde(default)]
  pub native_cgroup_root: Option<PathBuf>,
  pub allowed_upload_origins: Vec<String>,
  pub max_workspace_bytes: u64,
  pub max_spool_bytes: u64,
  pub poll_timeout_seconds: u64,
  pub heartbeat_interval_seconds: u64,
  pub lease_safety_margin_seconds: u64,
  pub graceful_cancel_timeout_seconds: u64,
  pub cleanup_timeout_seconds: u64,
}

#[derive(Debug)]
pub struct ValidatedConfig {
  pub config: AgentConfig,
  pub signing_keys: BTreeMap<String, VerifyingKey>,
  pub source_plugins: SourcePluginRegistry,
}

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("failed to inspect configuration '{path}': {source}")]
  Inspect { path: PathBuf, source: std::io::Error },
  #[error("configuration '{path}' exceeds the {MAX_CONFIG_BYTES}-byte limit")]
  TooLarge { path: PathBuf },
  #[error("failed to read configuration '{path}': {source}")]
  Read { path: PathBuf, source: std::io::Error },
  #[error("failed to parse configuration '{path}': {source}")]
  Parse {
    path: PathBuf,
    source: Box<toml::de::Error>,
  },
  #[error("invalid agent configuration: {0}")]
  Invalid(String),
  #[error(transparent)]
  SourcePlugins(#[from] RegistryError),
}

impl AgentConfig {
  pub fn load(path: &Path) -> Result<Self, ConfigError> {
    debug!(config = %path.display(), "loading agent configuration");
    let metadata = fs::metadata(path).map_err(|source| ConfigError::Inspect {
      path: path.to_owned(),
      source,
    })?;
    if metadata.len() > MAX_CONFIG_BYTES {
      return Err(ConfigError::TooLarge { path: path.to_owned() });
    }
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
      path: path.to_owned(),
      source,
    })?;
    debug!(config = %path.display(), bytes = contents.len(), "read agent configuration");
    toml::from_str(&contents).map_err(|source| ConfigError::Parse {
      path: path.to_owned(),
      source: Box::new(source),
    })
  }

  pub fn validate(mut self) -> Result<ValidatedConfig, ConfigError> {
    non_empty("agent_id", &self.agent_id)?;
    validate_server_url(&self.server_url)?;
    validate_regular_file("credential_file", &self.credential_file)?;
    validate_credential_permissions(&self.credential_file)?;
    self.credential_file = self
      .credential_file
      .canonicalize()
      .map_err(|error| ConfigError::Invalid(format!("credential_file: {error}")))?;

    if self.server_signing_keys.is_empty() {
      return invalid("server_signing_keys must contain at least one key");
    }
    let signing_keys = self
      .server_signing_keys
      .iter()
      .map(|(id, encoded)| decode_signing_key(id, encoded))
      .collect::<Result<_, _>>()?;

    let roots = [
      ("work_root", canonical_directory("work_root", &self.work_root)?),
      ("state_root", canonical_directory("state_root", &self.state_root)?),
      (
        "octa_release_root",
        canonical_directory("octa_release_root", &self.octa_release_root)?,
      ),
      (
        "source_plugins_dir",
        canonical_directory("source_plugins_dir", &self.source_plugins_dir)?,
      ),
    ];
    validate_distinct_roots(&roots)?;
    self.work_root = roots[0].1.clone();
    self.state_root = roots[1].1.clone();
    self.octa_release_root = roots[2].1.clone();
    self.source_plugins_dir = roots[3].1.clone();
    validate_plugin_directory_permissions(&self.source_plugins_dir)?;
    let source_plugins = SourcePluginRegistry::discover(&self.source_plugins_dir)?;

    for (name, value) in &self.labels {
      non_empty("label name", name)?;
      non_empty("label value", value)?;
    }
    if self.enabled_execution_backends.is_empty() {
      return invalid("enabled_execution_backends must contain at least one backend");
    }
    let backends: BTreeSet<_> = self.enabled_execution_backends.iter().copied().collect();
    if backends.len() != self.enabled_execution_backends.len() {
      return invalid("enabled_execution_backends must not contain duplicates");
    }
    if backends.contains(&BackendKind::Native) && !self.allow_native_execution {
      return invalid("NativeBackend requires allow_native_execution = true");
    }
    if backends.contains(&BackendKind::Native) {
      let root = self
        .native_cgroup_root
        .as_deref()
        .ok_or_else(|| ConfigError::Invalid("NativeBackend requires native_cgroup_root".to_owned()))?;
      self.native_cgroup_root = Some(canonical_directory("native_cgroup_root", root)?);
    } else if self.native_cgroup_root.is_some() {
      return invalid("native_cgroup_root is only valid when NativeBackend is enabled");
    }

    if self.allowed_upload_origins.is_empty() {
      return invalid("allowed_upload_origins must contain at least one origin");
    }
    let unique_origins: BTreeSet<_> = self.allowed_upload_origins.iter().collect();
    if unique_origins.len() != self.allowed_upload_origins.len() {
      return invalid("allowed_upload_origins must not contain duplicates");
    }
    for origin in &self.allowed_upload_origins {
      validate_upload_origin(origin)?;
    }

    if self.max_workspace_bytes == 0 || self.max_spool_bytes == 0 {
      return invalid("workspace and spool limits must be greater than zero");
    }
    for (name, value) in [
      ("poll_timeout_seconds", self.poll_timeout_seconds),
      ("heartbeat_interval_seconds", self.heartbeat_interval_seconds),
      ("lease_safety_margin_seconds", self.lease_safety_margin_seconds),
      ("graceful_cancel_timeout_seconds", self.graceful_cancel_timeout_seconds),
      ("cleanup_timeout_seconds", self.cleanup_timeout_seconds),
    ] {
      if value == 0 {
        return invalid(format!("{name} must be greater than zero"));
      }
    }
    if self.heartbeat_interval_seconds >= self.lease_safety_margin_seconds {
      return invalid("heartbeat_interval_seconds must be shorter than lease_safety_margin_seconds");
    }

    info!(
      agent_id = %self.agent_id,
      backends = self.enabled_execution_backends.len(),
      source_plugins = source_plugins.len(),
      "validated agent configuration"
    );
    Ok(ValidatedConfig {
      config: self,
      signing_keys,
      source_plugins,
    })
  }
}

fn validate_server_url(value: &str) -> Result<(), ConfigError> {
  let url = parse_absolute_url("server_url", value)?;
  if url.scheme_str() != Some("https") {
    return invalid("server_url must use https");
  }
  Ok(())
}

fn validate_upload_origin(value: &str) -> Result<(), ConfigError> {
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

fn decode_signing_key(id: &str, encoded: &str) -> Result<(String, VerifyingKey), ConfigError> {
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

fn canonical_directory(name: &str, path: &Path) -> Result<PathBuf, ConfigError> {
  if !path.is_absolute() {
    return invalid(format!("{name} must be absolute"));
  }
  let canonical = path
    .canonicalize()
    .map_err(|error| ConfigError::Invalid(format!("{name} '{}': {error}", path.display())))?;
  if !canonical.is_dir() {
    return invalid(format!("{name} '{}' is not a directory", path.display()));
  }
  Ok(canonical)
}

fn validate_distinct_roots(roots: &[(&str, PathBuf)]) -> Result<(), ConfigError> {
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

fn validate_regular_file(name: &str, path: &Path) -> Result<(), ConfigError> {
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
fn validate_credential_permissions(path: &Path) -> Result<(), ConfigError> {
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
fn validate_credential_permissions(_path: &Path) -> Result<(), ConfigError> {
  Ok(())
}

fn non_empty(name: &str, value: &str) -> Result<(), ConfigError> {
  if value.trim().is_empty() {
    invalid(format!("{name} must not be empty"))
  } else {
    Ok(())
  }
}

#[cfg(unix)]
fn validate_plugin_directory_permissions(path: &Path) -> Result<(), ConfigError> {
  use std::os::unix::fs::PermissionsExt as _;

  let mode = fs::metadata(path)
    .map_err(|error| ConfigError::Invalid(format!("source_plugins_dir '{}': {error}", path.display())))?
    .permissions()
    .mode();
  if mode & 0o022 != 0 {
    return invalid(format!(
      "source_plugins_dir '{}' must not be writable by group or others",
      path.display()
    ));
  }
  Ok(())
}

#[cfg(not(unix))]
fn validate_plugin_directory_permissions(_path: &Path) -> Result<(), ConfigError> {
  Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
  Err(ConfigError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
  use std::{fs::File, io::Write as _};

  use super::*;
  use tempfile::TempDir;

  struct Fixture {
    _temp: TempDir,
    config: AgentConfig,
  }

  impl Fixture {
    fn new() -> Self {
      let temp = tempfile::tempdir().unwrap();
      let credential = temp.path().join("credential");
      File::create(&credential).unwrap().write_all(b"token").unwrap();
      #[cfg(unix)]
      {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
      }
      let directory = |name: &str| {
        let path = temp.path().join(name);
        fs::create_dir(&path).unwrap();
        path
      };
      let signing_key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
      let config = AgentConfig {
        agent_id: "agent-1".to_owned(),
        server_url: "https://octacity.example".to_owned(),
        credential_file: credential,
        server_signing_keys: BTreeMap::from([(
          "primary".to_owned(),
          BASE64.encode(signing_key.verifying_key().as_bytes()),
        )]),
        labels: BTreeMap::from([("region".to_owned(), "test".to_owned())]),
        work_root: directory("work"),
        state_root: directory("state"),
        octa_release_root: directory("octa"),
        source_plugins_dir: directory("sources"),
        enabled_execution_backends: vec![BackendKind::Native],
        allow_native_execution: true,
        native_cgroup_root: Some(directory("cgroup")),
        allowed_upload_origins: vec!["https://objects.example".to_owned()],
        max_workspace_bytes: 1024,
        max_spool_bytes: 1024,
        poll_timeout_seconds: 30,
        heartbeat_interval_seconds: 5,
        lease_safety_margin_seconds: 15,
        graceful_cancel_timeout_seconds: 5,
        cleanup_timeout_seconds: 10,
      };
      Self { _temp: temp, config }
    }
  }

  #[test]
  fn validates_a_provisioned_agent() {
    let fixture = Fixture::new();
    let validated = fixture.config.validate().unwrap();
    assert_eq!(validated.signing_keys.len(), 1);
  }

  #[test]
  fn rejects_native_execution_without_explicit_consent() {
    let mut fixture = Fixture::new();
    fixture.config.allow_native_execution = false;
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("allow_native_execution")
    );
  }

  #[test]
  fn validates_backend_configuration_as_one_explicit_mode() {
    let mut fixture = Fixture::new();
    fixture.config.native_cgroup_root = None;
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("native_cgroup_root")
    );

    let mut fixture = Fixture::new();
    fixture.config.enabled_execution_backends = vec![BackendKind::Native, BackendKind::Native];
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("duplicates")
    );

    let mut fixture = Fixture::new();
    fixture.config.enabled_execution_backends = vec![BackendKind::Microsandbox];
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("only valid")
    );
  }

  #[test]
  fn rejects_overlapping_roots() {
    let mut fixture = Fixture::new();
    fixture.config.state_root = fixture.config.work_root.join("state");
    fs::create_dir(&fixture.config.state_root).unwrap();
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("must not overlap")
    );
  }

  #[test]
  fn rejects_non_origin_upload_urls() {
    let mut fixture = Fixture::new();
    fixture.config.allowed_upload_origins = vec!["https://objects.example/bucket".to_owned()];
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("only scheme")
    );
  }

  #[test]
  fn validates_upload_origins_limits_and_lease_timing() {
    let mut fixture = Fixture::new();
    fixture.config.allowed_upload_origins.clear();
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("at least one")
    );

    let mut fixture = Fixture::new();
    fixture.config.allowed_upload_origins = vec!["https://objects.example".to_owned(); 2];
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("duplicates")
    );

    let mut fixture = Fixture::new();
    fixture.config.max_workspace_bytes = 0;
    assert!(fixture.config.validate().unwrap_err().to_string().contains("limits"));

    let mut fixture = Fixture::new();
    fixture.config.poll_timeout_seconds = 0;
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("poll_timeout_seconds")
    );

    let mut fixture = Fixture::new();
    fixture.config.heartbeat_interval_seconds = fixture.config.lease_safety_margin_seconds;
    assert!(fixture.config.validate().unwrap_err().to_string().contains("shorter"));
  }

  #[test]
  fn rejects_invalid_identity_and_server_material() {
    let mut fixture = Fixture::new();
    fixture.config.labels.insert(String::new(), "value".to_owned());
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("label name")
    );

    let mut fixture = Fixture::new();
    fixture.config.server_signing_keys.clear();
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("at least one")
    );

    let mut fixture = Fixture::new();
    fixture
      .config
      .server_signing_keys
      .insert("primary".to_owned(), "not-base64".to_owned());
    assert!(fixture.config.validate().unwrap_err().to_string().contains("base64"));

    let mut fixture = Fixture::new();
    fixture.config.server_url = "http://octacity.example".to_owned();
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("must use https")
    );
  }

  #[cfg(unix)]
  #[test]
  fn rejects_a_group_writable_source_plugin_registry() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::new();
    fs::set_permissions(&fixture.config.source_plugins_dir, fs::Permissions::from_mode(0o775)).unwrap();
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("must not be writable by group or others")
    );
  }

  #[test]
  fn rejects_unknown_configuration_fields() {
    assert!(toml::from_str::<AgentConfig>("agent_id = 'a'\nunknown = true").is_err());
  }

  #[test]
  fn example_configuration_stays_parseable() {
    let config: AgentConfig = toml::from_str(include_str!("../../../docs/agent.example.toml")).unwrap();
    assert_eq!(config.agent_id, "linux-builder-01");
    assert!(decode_signing_key("primary", &config.server_signing_keys["primary"]).is_ok());
  }
}
