//! Atomic connection publication and safe replay, independent of installed-provider I/O.
use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
use octacity_server_factory::FactoryDigest;
use serde::{Deserialize, Serialize};

/// Exact non-secret caller intent; time and mutable installation facts are not replay keys.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationConnectionIntent {
  /// Stable connection identity; existing VCS integration identities may be explicitly adopted.
  pub id: IntegrationId,
  /// Absence creates version one; presence requires exactly that current version.
  pub expected_current_version: Option<IntegrationVersion>,
  /// Credential-free installed selection.
  pub draft: IntegrationConnectionDraft,
}
/// Read-only exact replay probe, checked before mutable installation resolution.
#[derive(Clone, Debug)]
pub struct ReplayIntegrationConnection {
  /// Original immutable business intent.
  pub intent: IntegrationConnectionIntent,
  /// Caller-selected key, partitioned by accepted management scope.
  pub idempotency_key: IdempotencyKey,
}
/// Complete trusted atomic publication request. No provider or credential is invoked.
#[derive(Clone, Debug)]
pub struct PublishIntegrationConnection {
  /// Exact caller intent.
  pub intent: IntegrationConnectionIntent,
  /// Current immutable verified installation declaration supplied by the application.
  pub installed: InstalledIntegration,
  /// Stable replay key.
  pub idempotency_key: IdempotencyKey,
  /// Accepted publication time, never renewed by replay.
  pub published_at: Timestamp,
}
impl PublishIntegrationConnection {
  /// Resolves and bounds the immutable version before any authoritative writes.
  pub fn resolve(&self) -> Result<PublishedIntegrationConnection, StoreError> {
    let version = self
      .intent
      .expected_current_version
      .map_or(Ok(IntegrationVersion::INITIAL), |v| v.next())
      .map_err(|_| crate::integration_connection::invalid_connection())?;
    if version.get() > i64::MAX as u64 {
      return Err(crate::integration_connection::invalid_connection());
    }
    let definition = IntegrationConnectionDefinition::resolve(&self.intent.draft, &self.installed)?;
    Ok(PublishedIntegrationConnection {
      id: self.intent.id,
      version,
      digest: definition.content_digest()?,
      definition,
      published_at: self.published_at,
    })
  }
}
/// One retained immutable metadata version; no readiness or grant is implied.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedIntegrationConnection {
  /// Stable reusable connection identity.
  pub id: IntegrationId,
  /// Positive immutable version.
  pub version: IntegrationVersion,
  /// Canonical safe definition content identity.
  pub digest: FactoryDigest,
  /// Frozen installed endpoint/resource/capability metadata.
  pub definition: IntegrationConnectionDefinition,
  /// Original accepted publication time.
  pub published_at: Timestamp,
}
/// Atomic publication receipt, including exact historical replay.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationConnectionMutationOutcome {
  /// Newly applied or exact replay, with no repeated audit/outbox effect.
  pub disposition: MutationDisposition,
  /// Original immutable version, even after newer versions exist.
  pub connection: PublishedIntegrationConnection,
}
/// Minimal reusable metadata authority. Project grants and worker activation belong elsewhere.
#[async_trait::async_trait]
pub trait IntegrationConnectionStore: Send + Sync {
  /// Reads current metadata with policy visibility applied before pagination.
  async fn list_integration_connections(
    &self,
    request: ReadIntegrationConnections,
  ) -> Result<IntegrationConnectionPage, StoreError>;
  /// Returns the exact receipt before resolving mutable installed choices again.
  async fn replay_integration_connection(
    &self,
    request: ManagementMutation<ReplayIntegrationConnection>,
  ) -> Result<Option<IntegrationConnectionMutationOutcome>, StoreError>;
  /// Commits a version, current pointer, replay result, safe audit and outbox atomically.
  async fn publish_integration_connection(
    &self,
    request: ManagementMutation<PublishIntegrationConnection>,
  ) -> Result<IntegrationConnectionMutationOutcome, StoreError>;
  /// Reads an immutable version; absence selects the current version.
  async fn integration_connection(
    &self,
    id: IntegrationId,
    version: Option<IntegrationVersion>,
  ) -> Result<PublishedIntegrationConnection, StoreError>;
}

/// Maximum visible connections or installations returned by one management page.
pub const MAX_INTEGRATION_DISCOVERY_PAGE_SIZE: u16 = 100;

/// Bounded metadata discovery; no credential access or Project grant is implied.
#[derive(Clone, Debug)]
pub struct ReadIntegrationConnections {
  /// Server-derived connection visibility.
  pub visibility: IntegrationConnectionListVisibility,
  /// Exclusive stable connection identity.
  pub after: Option<IntegrationId>,
  /// Positive page ceiling, at most 100.
  pub limit: u16,
}
impl ReadIntegrationConnections {
  /// Rejects unbounded reads independently of transport validation.
  pub fn validate(&self) -> Result<(), StoreError> {
    if !(1..=MAX_INTEGRATION_DISCOVERY_PAGE_SIZE).contains(&self.limit) {
      return Err(crate::integration_connection::invalid_connection());
    }
    Ok(())
  }
}
/// Current visible connections in identity order.
#[derive(Clone, Debug)]
pub struct IntegrationConnectionPage {
  /// Visible metadata only.
  pub items: Vec<PublishedIntegrationConnection>,
  /// Last visible identity only when another visible row exists.
  pub next_cursor: Option<IntegrationId>,
}
