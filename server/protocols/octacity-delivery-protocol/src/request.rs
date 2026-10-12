use crate::*;
use serde::{Deserialize, Serialize};

/// Exact independently negotiated operation request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
  /// Selected Delivery protocol version.
  pub protocol_version: u16,
  /// Bounded response correlation identity.
  pub request_id: String,
  /// Trusted operation; merge is deliberately absent.
  pub command: Command,
}
/// Trusted Delivery operations, never available through the read-only VCS protocol.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// Observe exact external state without creating a write.
  Observe(DeliveryIntent),
  /// Observe before publishing the accepted candidate for review, without merge.
  Publish(DeliveryIntent),
  /// Stop only the original operation; never create a missing publication.
  Cancel {
    /// Stable logical delivery identity.
    target_delivery_id: String,
  },
}
/// Capability-specific credential class, enforced again against installed credentials.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialClass {
  /// Trusted branch/review publication; source reads and reporting cannot claim it.
  DeliveryWrite,
}
/// Protected handle rather than a forge token. Debug never reveals its contents.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialHandle {
  /// Required installed grant class.
  pub class: CredentialClass,
  /// Opaque broker/installation-owned handle.
  pub handle: String,
}
impl std::fmt::Debug for CredentialHandle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("CredentialHandle")
      .field("class", &self.class)
      .field("handle", &"<redacted>")
      .finish()
  }
}
/// Exact repository selected by the trusted caller, not by a model session.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryAccess {
  /// Stable provider-neutral repository identity.
  pub repository_id: String,
  /// Operator-selected repository locator interpreted only inside the provider plugin.
  pub repository_locator: String,
  /// Protected capability-scoped credential handle.
  pub credential: CredentialHandle,
}
impl std::fmt::Debug for RepositoryAccess {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("RepositoryAccess")
      .field("repository_id", &self.repository_id)
      .field("repository_locator", &"<redacted>")
      .field("credential", &self.credential)
      .finish()
  }
}
/// Protected content-addressed input. The adapter verifies bytes before Git activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactInput {
  /// Opaque retained-input handle, never a caller-selected filesystem path or URL.
  pub handle: String,
  /// Expected canonical content digest.
  pub sha256: String,
  /// Exact byte size, bounded by the corresponding input class.
  pub size_bytes: u64,
}
/// Immutable operator-selected branch policy. No force, rebase or merge field exists.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetPolicy {
  /// Target base reference whose current revision must still equal the accepted base.
  pub base_reference: String,
  /// One candidate reference tied to the stable delivery identity.
  pub candidate_reference: String,
}
/// Bounded public review metadata; no prompt, transcript or credential fields exist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewMetadata {
  /// Human-visible title, at most 256 bytes.
  pub title: String,
  /// Safe caller projection, at most 4096 bytes; normal Markdown newlines are allowed.
  pub summary: String,
  /// Whether the review is initially a draft.
  pub draft: bool,
}
/// Complete exact accepted intent retained across response loss and process restart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryIntent {
  /// Stable logical delivery identity, distinct from process request correlation.
  pub delivery_id: String,
  /// Protected repository/credential selection.
  pub repository: RepositoryAccess,
  /// Exact original immutable base; no implicit rebase.
  pub base_revision: String,
  /// Exact accepted candidate commit; no branch alias.
  pub candidate_revision: String,
  /// Exact accepted canonical ChangeSet manifest identity.
  pub changeset_digest: String,
  /// Immutable accepted Decision/control-proof identity.
  pub decision_digest: String,
  /// Immutable operator provider profile, excluding rotated credential contents.
  pub configuration_digest: String,
  /// Exact permitted network hosts; the provider checks its actual API/Git origins
  /// against this frozen ceiling before resolving credentials or doing network I/O.
  pub network_hosts: Vec<String>,
  /// Pinned target policy.
  pub target: TargetPolicy,
  /// Trusted captured candidate bundle.
  pub bundle: ArtifactInput,
  /// Trusted canonical capture manifest.
  pub manifest: ArtifactInput,
  /// Bounded review projection selected by the operator.
  pub review: ReviewMetadata,
}
impl Request {
  /// Validates strict scope, version and bounds before starting a provider process.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != DELIVERY_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    field(&self.request_id)?;
    match &self.command {
      Command::Observe(intent) | Command::Publish(intent) => intent.validate(),
      Command::Cancel { target_delivery_id } => digest(target_delivery_id),
    }
  }
}
impl DeliveryIntent {
  /// Validates immutable identities and bounded metadata without provider effects.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    for value in [
      &self.delivery_id,
      &self.configuration_digest,
      &self.changeset_digest,
      &self.decision_digest,
      &self.bundle.sha256,
      &self.manifest.sha256,
    ] {
      digest(value)?;
    }
    for value in [
      &self.repository.repository_id,
      &self.repository.repository_locator,
      &self.repository.credential.handle,
      &self.base_revision,
      &self.candidate_revision,
      &self.target.base_reference,
      &self.target.candidate_reference,
      &self.bundle.handle,
      &self.manifest.handle,
    ] {
      field(value)?;
    }
    if self.network_hosts.is_empty() || self.network_hosts.len() > 256 {
      return Err(ProtocolError::Invalid);
    }
    let mut unique = std::collections::BTreeSet::new();
    for host in &self.network_hosts {
      field(host)?;
      let canonical_ip = host
        .parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.to_string() == *host);
      let canonical_dns = host.len() <= 253
        && host.bytes().any(|byte| byte.is_ascii_lowercase())
        && host.split('.').all(|label| {
          !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
              .bytes()
              .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        });
      if (!canonical_ip && !canonical_dns) || !unique.insert(host) {
        return Err(ProtocolError::Invalid);
      }
    }
    if self.base_revision == self.candidate_revision
      || self.target.base_reference == self.target.candidate_reference
      || self.manifest.sha256 != self.changeset_digest
      || self.bundle.handle == self.manifest.handle
      || self.bundle.size_bytes == 0
      || self.bundle.size_bytes > MAX_DELIVERY_BUNDLE_BYTES
      || self.manifest.size_bytes == 0
      || self.manifest.size_bytes > MAX_DELIVERY_MANIFEST_BYTES
      || self.review.title.is_empty()
      || self.review.title.len() > 256
      || self.review.title.chars().any(char::is_control)
      || self.review.summary.len() > 4096
      || self
        .review
        .summary
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}
