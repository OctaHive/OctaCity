use std::path::{Path, PathBuf};

use serde::Deserialize;

const MAX_POSTGRES_CONNECTIONS: u32 = 1_024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PostgresConfig {
  pub(crate) url_file: PathBuf,
  #[serde(default = "default_postgres_connections")]
  pub(crate) max_connections: u32,
}

impl PostgresConfig {
  pub(crate) fn validate(&self) -> Result<(), String> {
    validate_path("postgres.url_file", &self.url_file)?;
    if self.max_connections == 0 || self.max_connections > MAX_POSTGRES_CONNECTIONS {
      return Err(format!(
        "postgres.max_connections must be between 1 and {MAX_POSTGRES_CONNECTIONS}"
      ));
    }
    Ok(())
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectStorageConfig {
  pub(crate) endpoint: String,
  pub(crate) region: String,
  pub(crate) bucket: String,
  #[serde(default)]
  pub(crate) prefix: String,
  pub(crate) access_key_file: PathBuf,
  pub(crate) secret_key_file: PathBuf,
  #[serde(default)]
  pub(crate) force_path_style: bool,
  #[serde(default = "default_object_operation_milliseconds")]
  operation_timeout_milliseconds: u64,
  #[serde(default = "default_object_capability_recheck_milliseconds")]
  capability_recheck_interval_milliseconds: u64,
}

impl ObjectStorageConfig {
  pub(crate) fn validate(&self) -> Result<(), String> {
    validate_path("object_storage.access_key_file", &self.access_key_file)?;
    validate_path("object_storage.secret_key_file", &self.secret_key_file)?;
    octacity_artifact_s3::validate_s3_settings(
      &self.endpoint,
      &self.region,
      &self.bucket,
      &self.prefix,
      self.operation_timeout(),
      self.capability_recheck_interval(),
    )
    .map_err(|error| format!("object_storage configuration is invalid: {error}"))?;
    Ok(())
  }

  pub(crate) const fn operation_timeout(&self) -> std::time::Duration {
    std::time::Duration::from_millis(self.operation_timeout_milliseconds)
  }

  pub(crate) const fn capability_recheck_interval(&self) -> std::time::Duration {
    std::time::Duration::from_millis(self.capability_recheck_interval_milliseconds)
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SigningConfig {
  pub(crate) key_id: String,
  pub(crate) key_file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCredentialConfig {
  pub(crate) enrollment_key_file: PathBuf,
}

impl AgentCredentialConfig {
  pub(crate) fn validate(&self) -> Result<(), String> {
    validate_path("agent_credentials.enrollment_key_file", &self.enrollment_key_file)
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct JobSpecConfig {
  pub(crate) policy_file: PathBuf,
}

impl JobSpecConfig {
  pub(crate) fn validate(&self) -> Result<(), String> {
    validate_path("job_spec.policy_file", &self.policy_file)
  }
}

impl SigningConfig {
  pub(crate) fn validate(&self) -> Result<(), String> {
    if !octacity_protocol::valid_signing_key_id(&self.key_id) {
      return Err("signing.key_id is invalid".to_owned());
    }
    validate_path("signing.key_file", &self.key_file)
  }
}

fn validate_path(field: &str, path: &Path) -> Result<(), String> {
  if path.as_os_str().is_empty() {
    Err(format!("{field} must not be empty"))
  } else {
    Ok(())
  }
}

const fn default_postgres_connections() -> u32 {
  10
}

const fn default_object_operation_milliseconds() -> u64 {
  5_000
}

const fn default_object_capability_recheck_milliseconds() -> u64 {
  5 * 60 * 1_000
}
