//! Explicit Project access to an immutable connection selection; never remote token authority.
use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId, Timestamp};
use serde::{Deserialize, Serialize};

/// Exact grant replacement; an empty resource set explicitly revokes all use.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationProjectAccessIntent {
  /// Reusable connection being granted.
  pub connection_id: IntegrationId,
  /// Independently authorized owning Project.
  pub project_id: ProjectId,
  /// Exact immutable metadata selection, never implicitly redirected.
  pub connection_version: IntegrationVersion,
  /// Absence creates access; presence requires that current grant version.
  pub expected_current_version: Option<IntegrationVersion>,
  /// Explicit resource/purpose subset; empty means revoked.
  pub resources: Vec<IntegrationResourceSelection>,
}
/// Attributed atomic grant publication.
#[derive(Clone, Debug)]
pub struct PublishIntegrationProjectAccess {
  /// Credential-free original reviewed intent.
  pub intent: IntegrationProjectAccessIntent,
  /// Scoped exact transport replay key.
  pub idempotency_key: IdempotencyKey,
  /// Original trusted publication time.
  pub published_at: Timestamp,
}
impl PublishIntegrationProjectAccess {
  /// Validates the selected subset against the exact connection version before publication.
  pub fn resolve(
    &self,
    metadata: &PublishedIntegrationConnection,
  ) -> Result<PublishedIntegrationProjectAccess, StoreError> {
    let intent = &self.intent;
    let invalid = || {
      StoreError::invalid(
        StoreOperation::PublishIntegrationProjectAccess,
        StoreInputError::InvalidIntegrationConnection,
      )
    };
    if metadata.id != intent.connection_id
      || metadata.version != intent.connection_version
      || intent.resources.len() > MAX_INTEGRATION_RESOURCES
    {
      return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for selected in &intent.resources {
      let resource = metadata
        .definition
        .resources
        .iter()
        .find(|r| r.reference == selected.reference)
        .ok_or_else(invalid)?;
      if !seen.insert(&selected.reference)
        || selected.purposes.is_empty()
        || selected.purposes.len() > 4
        || selected
          .purposes
          .iter()
          .collect::<std::collections::BTreeSet<_>>()
          .len()
          != selected.purposes.len()
        || selected
          .purposes
          .iter()
          .any(|p| !resource.capabilities.iter().any(|c| c.purpose == *p))
      {
        return Err(invalid());
      }
    }
    let version = intent
      .expected_current_version
      .map_or(Ok(IntegrationVersion::INITIAL), |v| v.next())
      .map_err(|_| invalid())?;
    if version.get() > i64::MAX as u64 {
      return Err(invalid());
    }
    Ok(PublishedIntegrationProjectAccess {
      connection_id: intent.connection_id,
      project_id: intent.project_id,
      connection_version: intent.connection_version,
      version,
      resources: intent.resources.clone(),
      published_at: self.published_at,
    })
  }
}
/// One immutable grant revision; it does not activate the connection or any pipeline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedIntegrationProjectAccess {
  /// Exact connection identity.
  pub connection_id: IntegrationId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Frozen metadata version.
  pub connection_version: IntegrationVersion,
  /// Independent optimistic grant version.
  pub version: IntegrationVersion,
  /// Explicit resource/purpose grants.
  pub resources: Vec<IntegrationResourceSelection>,
  /// Accepted publication time.
  pub published_at: Timestamp,
}
/// Original grant receipt, including historical exact replay.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationProjectAccessOutcome {
  /// Newly applied or replayed without regranting historical access.
  pub disposition: MutationDisposition,
  /// Original immutable grant revision.
  pub access: PublishedIntegrationProjectAccess,
}
/// Narrow atomic Project grant authority shared by management and runtime consumers.
#[async_trait::async_trait]
pub trait IntegrationProjectAccessStore: Send + Sync {
  /// Publishes exact resources and purposes with current-version and replay checks.
  async fn publish_integration_project_access(
    &self,
    request: ManagementMutation<PublishIntegrationProjectAccess>,
  ) -> Result<IntegrationProjectAccessOutcome, StoreError>;
  /// Reads current grant state; absence never implies use authority.
  async fn integration_project_access(
    &self,
    connection_id: IntegrationId,
    project_id: ProjectId,
  ) -> Result<Option<PublishedIntegrationProjectAccess>, StoreError>;
}

/// Pinned managed resource request, usable by task cards without a Factory input or Run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationResourceAccess {
  /// Original reusable connection.
  pub connection_id: IntegrationId,
  /// Original immutable selection, not the latest replacement.
  pub connection_version: IntegrationVersion,
  /// Project making this protected use.
  pub project_id: ProjectId,
  /// Exact native resource definition.
  pub resource: octacity_server_factory::ImmutableReference,
  /// Independently required purpose.
  pub purpose: IntegrationPurpose,
}
