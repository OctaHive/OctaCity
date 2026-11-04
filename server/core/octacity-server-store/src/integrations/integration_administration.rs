//! Safe administrative journal. Secret values and secret-derived digests never enter it.
use crate::*;
use octacity_server_domain::{EntityKind, IntegrationId, IntegrationVersion, Timestamp};
pub use octacity_server_secrets::CredentialPublication;
use octacity_server_secrets::{CredentialPublicationKind, LogicalSecretReference};
use serde::{Deserialize, Serialize};

/// An explicitly authorized connection operation, independent of Project-use grants.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntegrationAdministrationAction {
  /// Prepare and activate material behind an exact installed purpose handle.
  Credential {
    /// Logical installed reference, never a private path.
    reference: LogicalSecretReference,
    /// Provision, adoption, rotation or revocation.
    kind: CredentialPublicationKind,
    /// Explicitly reviewed activation after private publication and full selected readiness.
    /// Defaults to false; it never grants Project use or enables an input.
    #[serde(default)]
    activate_connection: bool,
  },
  /// Enable this exact metadata version only after every selected handle is configured.
  Activate,
  /// Stop future protected use; retain immutable metadata and material for dependents.
  Disable,
}
/// Safe replay intent. Transient input is deliberately excluded from serialization/hashing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationAdministrationIntent {
  /// Stable reusable connection.
  pub id: IntegrationId,
  /// Exact current metadata precondition.
  pub expected_metadata_version: IntegrationVersion,
  /// Exact administrative state precondition; zero means no administration yet.
  pub expected_revision: u64,
  /// Explicit administrative change.
  pub action: IntegrationAdministrationAction,
}
/// Begin or recover one administrative operation before private I/O.
#[derive(Clone, Debug)]
pub struct BeginIntegrationAdministration {
  /// Credential-free original intent.
  pub intent: IntegrationAdministrationIntent,
  /// Security-partitioned replay key.
  pub idempotency_key: IdempotencyKey,
  /// Original trusted time for bounded orphan cleanup.
  pub requested_at: Timestamp,
}
/// Retained safe operation state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationAdministrationStatus {
  /// Private preparation/activation may be resumed under the same identity.
  Prepared,
  /// Original completed receipt; replay performs no private writes.
  Completed,
  /// Superseded/expired preparation; activation is no longer authorized.
  Abandoned,
}
/// Safe stable receipt, also used to recover the private publication after a restart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationAdministrationReceipt {
  /// Original accepted intent, containing no secret value or digest of it.
  pub intent: IntegrationAdministrationIntent,
  /// Stable opaque administrative identity.
  pub operation: [u8; 32],
  /// Original revision, never renewed by replay.
  pub revision: u64,
  /// Original accepted time.
  pub requested_at: Timestamp,
  /// Private publication fence, absent for metadata-only operations.
  pub publication: Option<CredentialPublication>,
  /// Retained operation disposition.
  pub status: IntegrationAdministrationStatus,
}
/// Current material status without exposing contents, paths or provider responses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationCredentialStatus {
  /// Original logical reference.
  pub reference: LogicalSecretReference,
  /// Latest completed configured state, not proof of remote readiness.
  pub configured: bool,
}
/// Current administration is independent of retained immutable connection versions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationAdministrationState {
  /// Stable connection identity.
  pub id: IntegrationId,
  /// Optimistic administrative revision.
  pub revision: u64,
  /// Current disablement gate; false until explicit activation.
  pub enabled: bool,
  /// Explicitly activated immutable version, with no repinning of retained consumers.
  pub active_version: Option<IntegrationVersion>,
  /// Configured handles, retained during disablement for pinned consumers.
  pub credentials: Vec<IntegrationCredentialStatus>,
  /// One pending private publication; other writes conflict until recovery or disablement.
  pub pending: Option<CredentialPublication>,
}
impl IntegrationAdministrationState {
  fn credentials_ready(
    &self,
    metadata: &PublishedIntegrationConnection,
    replacement: Option<(&LogicalSecretReference, bool)>,
  ) -> bool {
    metadata
      .definition
      .resources
      .iter()
      .flat_map(|r| &r.capabilities)
      .all(|capability| match replacement {
        Some((reference, configured)) if capability.credential == *reference => configured,
        _ => self
          .credentials
          .iter()
          .any(|s| s.reference == capability.credential && s.configured),
      })
  }

  /// Fences only the current expired/cancelled preparation. Independent configured
  /// purposes and all referenced runtime material remain retained.
  pub fn abandon(&mut self, publication: &CredentialPublication) {
    if self.pending.as_ref() != Some(publication) {
      return;
    }
    if let Some(status) = self
      .credentials
      .iter_mut()
      .find(|c| c.reference == publication.reference)
    {
      status.configured = false;
    }
    self.pending = None;
    self.enabled = false;
  }
  /// Metadata-only publication starts disabled and unconfigured.
  pub fn new(id: IntegrationId) -> Self {
    Self {
      id,
      revision: 0,
      enabled: false,
      active_version: None,
      credentials: Vec::new(),
      pending: None,
    }
  }
  /// Applies a validated safe reservation while the adapter holds the connection lock.
  pub fn begin(
    &mut self,
    request: &BeginIntegrationAdministration,
    metadata: &PublishedIntegrationConnection,
    scope: &ManagementSecurityScope,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    if self.id != request.intent.id
      || self.revision != request.intent.expected_revision
      || metadata.version != request.intent.expected_metadata_version
      || (self.pending.is_some() && request.intent.action != IntegrationAdministrationAction::Disable)
    {
      return Err(administration_conflict());
    }
    let required: std::collections::BTreeSet<_> = metadata
      .definition
      .resources
      .iter()
      .flat_map(|r| r.capabilities.iter().map(|c| c.credential.clone()))
      .collect();
    if let IntegrationAdministrationAction::Credential { reference, .. } = &request.intent.action
      && !required.contains(reference)
    {
      return Err(administration_conflict());
    }
    match &request.intent.action {
      IntegrationAdministrationAction::Activate if !self.credentials_ready(metadata, None) => {
        return Err(administration_conflict());
      }
      IntegrationAdministrationAction::Credential {
        reference,
        kind,
        activate_connection: true,
      } if *kind == CredentialPublicationKind::Revoke || !self.credentials_ready(metadata, Some((reference, true))) => {
        return Err(administration_conflict());
      }
      _ => {}
    }
    let revision = self
      .revision
      .checked_add(1)
      .filter(|v| *v <= i64::MAX as u64)
      .ok_or_else(administration_conflict)?;
    let safe = serde_json::to_vec(&(scope.as_str(), &request.idempotency_key, &request.intent))
      .map_err(|_| StoreError::Unavailable)?;
    let operation =
      octacity_server_factory::FactoryDigest::sha256("octacity.integration-administration.v1", &[&safe]).as_bytes();
    let publication = match &request.intent.action {
      IntegrationAdministrationAction::Credential { reference, .. } => {
        Some(CredentialPublication::new(reference.clone(), operation, revision).map_err(|_| administration_conflict())?)
      }
      _ => None,
    };
    if request.intent.action == IntegrationAdministrationAction::Disable
      && let Some(pending) = self.pending.clone()
    {
      self.abandon(&pending);
    }
    self.revision = revision;
    self.enabled = request.intent.action == IntegrationAdministrationAction::Activate;
    if self.enabled {
      self.active_version = Some(metadata.version);
    }
    self.pending = publication.clone();
    Ok(IntegrationAdministrationReceipt {
      intent: request.intent.clone(),
      operation,
      revision,
      requested_at: request.requested_at,
      status: if publication.is_some() {
        IntegrationAdministrationStatus::Prepared
      } else {
        IntegrationAdministrationStatus::Completed
      },
      publication,
    })
  }
  /// Completes the current reservation while the adapter holds the connection lock.
  pub fn complete(
    &mut self,
    receipt: &mut IntegrationAdministrationReceipt,
    metadata: &PublishedIntegrationConnection,
  ) -> Result<(), StoreError> {
    if receipt.status == IntegrationAdministrationStatus::Completed {
      return Ok(());
    }
    if metadata.id != self.id
      || metadata.version != receipt.intent.expected_metadata_version
      || receipt.status != IntegrationAdministrationStatus::Prepared
      || self.pending.as_ref() != receipt.publication.as_ref()
      || self.revision != receipt.revision
    {
      return Err(administration_conflict());
    }
    let IntegrationAdministrationAction::Credential {
      reference,
      kind,
      activate_connection,
    } = &receipt.intent.action
    else {
      return Err(administration_conflict());
    };
    if *activate_connection
      && (*kind == CredentialPublicationKind::Revoke || !self.credentials_ready(metadata, Some((reference, true))))
    {
      return Err(administration_conflict());
    }
    if let Some(current) = self.credentials.iter_mut().find(|c| c.reference == *reference) {
      current.configured = *kind != CredentialPublicationKind::Revoke;
    } else {
      self.credentials.push(IntegrationCredentialStatus {
        reference: reference.clone(),
        configured: *kind != CredentialPublicationKind::Revoke,
      });
    }
    self.pending = None;
    if *activate_connection {
      self.enabled = true;
      self.active_version = Some(metadata.version);
    }
    receipt.status = IntegrationAdministrationStatus::Completed;
    Ok(())
  }
}
pub(crate) fn administration_conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Integration,
  }
}

/// Bounded retained-preparation cleanup, never deletion of referenced runtime material.
#[derive(Clone, Debug)]
pub struct IntegrationPreparationCleanup {
  /// Connection under current administrative authorization.
  pub id: IntegrationId,
  /// Trusted time of this cleanup pass, distinct from the preparation expiry cutoff.
  pub requested_at: Timestamp,
  /// Original preparation time upper bound, supplied by the trusted clock.
  pub older_than: Timestamp,
  /// At most 100 operations per pass; zero and larger values are rejected.
  pub limit: u16,
}
/// Atomic safe journal and lifecycle port, without provider-specific DTOs or raw values.
#[async_trait::async_trait]
pub trait IntegrationAdministrationStore: Send + Sync {
  /// Retains a stable preparation or returns the original terminal receipt before mutable resolution.
  async fn begin_integration_administration(
    &self,
    request: ManagementMutation<BeginIntegrationAdministration>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError>;
  /// Commits configured status and a safe audit/outbox receipt only for the current pending operation.
  async fn complete_integration_administration(
    &self,
    request: ManagementMutation<IntegrationAdministrationReceipt>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError>;
  /// Safe current lifecycle state; immutable versions/material are never destructively retired here.
  async fn integration_administration(&self, id: IntegrationId) -> Result<IntegrationAdministrationState, StoreError>;
  /// Atomically abandons expired operations with attributed audit/outbox evidence,
  /// then returns a bounded retryable cleanup batch. Terminal retries add no evidence.
  async fn integration_preparation_cleanup(
    &self,
    request: ManagementMutation<IntegrationPreparationCleanup>,
  ) -> Result<Vec<CredentialPublication>, StoreError>;
  /// Records private preparation erasure. A failed acknowledgement leaves it eligible for retry.
  async fn acknowledge_integration_preparation_cleanup(
    &self,
    request: ManagementMutation<CredentialPublication>,
  ) -> Result<(), StoreError>;
}
