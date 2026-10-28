//! Installed Issue readers reuse private runtime-file handling and Issue authorization/storage.
//! Bearers are purpose-separated (`ocmcp.<connection-id>.<64 lowercase hex digits>`).
//! No Source, Reporter, AI or trusted-network identity can substitute for this verifier.
use crate::config::IssueMcpReader;
use crate::{RuntimeAssemblyError, ServerConfig};
use octacity_server_api_mcp::{IssueMcpApplication, IssueMcpAuthority, issue_mcp_router};
use octacity_server_application::{
  IssueHandlers, IssueReadPrincipal, ManagementResource, ManagementResourceIdentity, ManagementResourceKind,
  ManagementVisibility, ProviderCredentialValue, Timestamp, TrustedNetworkManagementPolicy,
};
use octacity_server_store::{CredentialDigest, IssueStore};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, sync::Arc};

const TOKEN_SECRET_HEX_DIGITS: usize = 64;
const MAX_CREDENTIAL_FILE_BYTES: u64 = 256;

struct InstalledReaders(BTreeMap<String, IssueMcpReader>);
impl InstalledReaders {
  async fn credential(reader: &IssueMcpReader) -> Option<zeroize::Zeroizing<Vec<u8>>> {
    let mut bytes = super::assembly::read_private_bytes(
      "Issue MCP connection",
      &reader.credential_file,
      MAX_CREDENTIAL_FILE_BYTES,
    )
    .await
    .ok()?;
    while matches!(bytes.last(), Some(b'\r' | b'\n')) {
      bytes.pop();
    }
    let value = std::str::from_utf8(&bytes).ok()?;
    let secret = value.strip_prefix(&format!("ocmcp.{}.", reader.id))?;
    if secret.len() != TOKEN_SECRET_HEX_DIGITS
      || !secret
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
      return None;
    }
    Some(bytes)
  }
}
#[async_trait::async_trait]
impl IssueMcpAuthority for InstalledReaders {
  async fn authenticate(&self, credential: &ProviderCredentialValue, now: Timestamp) -> Option<IssueReadPrincipal> {
    let bytes = credential.expose_for_installation();
    let value = std::str::from_utf8(bytes).ok()?;
    let mut parts = value.split('.');
    if parts.next()? != "ocmcp" {
      return None;
    }
    let reader = self.0.get(parts.next()?)?;
    if now >= reader.expires_at_unix_ms {
      return None;
    }
    let expected = Self::credential(reader).await?;
    // Reuse the existing fixed-size credential verifier; never compare secret strings.
    let presented = CredentialDigest::from_bytes(Sha256::digest(bytes).into());
    let installed = CredentialDigest::from_bytes(Sha256::digest(&*expected).into());
    if !installed.matches(presented) {
      return None;
    }
    let visibility = match &reader.issue_ids {
      None => ManagementVisibility::all(),
      Some(issues) if issues.is_empty() => ManagementVisibility::none(),
      Some(issues) => ManagementVisibility::restricted(issues.iter().map(|id| {
        ManagementResource::instance(
          ManagementResourceKind::Issue,
          ManagementResourceIdentity::new(id.to_string()).expect("validated Issue ID"),
        )
        .expect("Issue resource")
      }))
      .ok()?,
    };
    IssueReadPrincipal::new(
      format!("issue-mcp:{}", reader.id),
      reader.project_id,
      visibility,
      reader.expires_at_unix_ms,
    )
    .ok()
  }
}

pub(super) async fn validate(config: &ServerConfig) -> Result<(), RuntimeAssemblyError> {
  for reader in config.issue_mcp_readers() {
    if InstalledReaders::credential(reader).await.is_none() {
      return Err(RuntimeAssemblyError::InvalidCredential {
        purpose: "Issue MCP connection",
      });
    }
  }
  Ok(())
}

pub(super) async fn router<S: IssueStore + 'static>(
  config: &ServerConfig,
  store: Arc<S>,
) -> Result<Option<axum::Router>, RuntimeAssemblyError> {
  if config.issue_mcp_readers().is_empty() {
    return Ok(None);
  }
  validate(config).await?;
  let readers = InstalledReaders(
    config
      .issue_mcp_readers()
      .iter()
      .cloned()
      .map(|reader| (reader.id.clone(), reader))
      .collect(),
  );
  // The adapter's closed IssueReadPrincipal owns the exact read policy. This policy
  // is only used by IssueHandlers for unrelated command/planning paths, never exposed by MCP.
  let queries = Arc::new(IssueHandlers::new(store, Arc::new(TrustedNetworkManagementPolicy)));
  Ok(Some(issue_mcp_router(IssueMcpApplication::new(
    Arc::new(readers),
    queries,
  ))))
}
