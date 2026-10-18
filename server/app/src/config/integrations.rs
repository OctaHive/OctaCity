use octacity_server_domain::IntegrationId;
use serde::Deserialize;

/// Opt-in local transport: the kernel authenticates a single configured proxy UID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalIntegrationManagement {
  pub(crate) socket_path: std::path::PathBuf,
  pub(crate) peer_uid: u32,
  pub(crate) principal: String,
  #[serde(default)]
  pub(crate) catalog_file: Option<std::path::PathBuf>,
  #[serde(default = "default_check_timeout")]
  pub(crate) operation_timeout_milliseconds: u64,
  #[serde(default = "default_check_grace")]
  pub(crate) cancellation_grace_milliseconds: u64,
}
fn default_check_timeout() -> u64 {
  10_000
}
fn default_check_grace() -> u64 {
  1_000
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct VcsIntegrationConfig {
  integration_id: IntegrationId,
  adapter_id: String,
  adapter_sha256: String,
  credential_handle: String,
}

impl std::fmt::Debug for VcsIntegrationConfig {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("VcsIntegrationConfig")
      .field("integration_id", &self.integration_id)
      .field("adapter_id", &self.adapter_id)
      .field("adapter_sha256", &self.adapter_sha256)
      .field("credential_handle", &"<redacted>")
      .finish()
  }
}

impl VcsIntegrationConfig {
  pub(crate) const fn integration_id(&self) -> IntegrationId {
    self.integration_id
  }

  pub(crate) fn adapter_id(&self) -> &str {
    &self.adapter_id
  }

  pub(crate) fn adapter_sha256(&self) -> &str {
    &self.adapter_sha256
  }

  pub(crate) fn credential_handle(&self) -> &str {
    &self.credential_handle
  }

  pub(super) fn validate(&self) -> bool {
    let valid_field = |value: &str| {
      !value.is_empty()
        && value.len() <= octacity_vcs_protocol::MAX_FIELD_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
    };
    valid_field(&self.adapter_id)
      && valid_field(&self.credential_handle)
      && self.adapter_sha256.len() == 64
      && self
        .adapter_sha256
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
  }
}
