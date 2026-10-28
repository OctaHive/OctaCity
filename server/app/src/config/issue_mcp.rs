//! Private deployment-owned references for independent read-only Issue MCP connections.
use super::ServerConfigError;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use serde::Deserialize;
use std::{collections::BTreeSet, path::PathBuf};

const MAX_READERS: usize = 128;
const MAX_READER_ID_BYTES: usize = 64;

/// No token values or mutation scopes are accepted in process configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueMcpReader {
  pub(crate) id: String,
  pub(crate) project_id: ProjectId,
  pub(crate) credential_file: PathBuf,
  pub(crate) expires_at_unix_ms: Timestamp,
  /// Absent means all Issues in exactly this Project; empty means no Issues.
  pub(crate) issue_ids: Option<BTreeSet<IssueId>>,
}
impl IssueMcpReader {
  pub(super) fn validate_all(readers: &[Self]) -> Result<(), ServerConfigError> {
    let valid = readers.len() <= MAX_READERS
      && readers.iter().map(|reader| &reader.id).collect::<BTreeSet<_>>().len() == readers.len()
      && readers.iter().all(|reader| {
        !reader.id.is_empty()
          && reader.id.len() <= MAX_READER_ID_BYTES
          && reader
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
          && reader.credential_file.is_absolute()
          && reader.credential_file.file_name().is_some()
          && reader.expires_at_unix_ms.unix_millis() > 0
          && reader
            .issue_ids
            .as_ref()
            .is_none_or(|issues| issues.len() <= octacity_server_application::MAX_MANAGEMENT_VISIBILITY_RESOURCES)
      });
    if valid {
      Ok(())
    } else {
      Err(ServerConfigError::Invalid("Issue MCP readers require unique bounded identities, absolute private credential files and explicit Project/read scope and expiry".into()))
    }
  }
}
