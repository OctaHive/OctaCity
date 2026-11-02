//! Project policy journal; authorization is accepted by application owners before calling this port.
use crate::{IdempotencyKey, ManagementMutation, MutationDisposition, StoreError};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};

/// The two explicitly supported Project policy documents, retaining their distinct core types.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "policy", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryProjectPolicyRecord {
  /// Launch rules do not confer execution permissions.
  Launch(FactoryLaunchPolicy),
  /// Access ceiling does not enable admission.
  Access(Box<FactoryAccessProfile>),
}
impl FactoryProjectPolicyRecord {
  /// Exact immutable publication identity.
  #[must_use]
  pub fn reference(&self) -> FactoryProjectPolicyRef {
    match self {
      Self::Launch(value) => FactoryProjectPolicyRef::Launch(value.reference().clone()),
      Self::Access(value) => FactoryProjectPolicyRef::Access(value.reference().clone()),
    }
  }
}

/// Current catalog metadata; full permission declarations remain in exact document reads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryProjectPolicySummary {
  /// Exact current version, suitable for an explicit read or replacement precondition.
  pub reference: FactoryProjectPolicyRef,
  /// Bounded display name, never an executable selection alias.
  pub name: FactoryText,
}
impl From<&FactoryProjectPolicyRecord> for FactoryProjectPolicySummary {
  fn from(value: &FactoryProjectPolicyRecord) -> Self {
    Self {
      reference: value.reference(),
      name: match value {
        FactoryProjectPolicyRecord::Launch(value) => value.definition().name.clone(),
        FactoryProjectPolicyRecord::Access(value) => value.definition().name.clone(),
      },
    }
  }
}
/// Bounded catalog ordered by typed kind then stable identity, independently of version changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryProjectPolicyPage {
  /// Current metadata for the authorized Project only.
  pub items: Vec<FactoryProjectPolicySummary>,
  /// Exclusive stable kind/identity cursor; its version does not select the next page's head.
  pub next_cursor: Option<FactoryProjectPolicyRef>,
}

/// Typed policy binding; variants cannot be substituted by a shared UUID or display name.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "reference", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryProjectPolicyRef {
  /// Exact launch policy version.
  Launch(FactoryLaunchPolicyRef),
  /// Exact execution access profile version.
  Access(FactoryAccessProfileRef),
}
impl FactoryProjectPolicyRef {
  /// Owning Project, retained in the verified core digest.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    match self {
      Self::Launch(value) => value.project_id,
      Self::Access(value) => value.project_id,
    }
  }
  /// Stable identity within its typed kind and Project.
  #[must_use]
  pub fn id(&self) -> uuid::Uuid {
    match self {
      Self::Launch(value) => value.id.as_uuid(),
      Self::Access(value) => value.id.as_uuid(),
    }
  }
  /// Positive revision; adapters must reject values outside their integer representation.
  #[must_use]
  pub const fn version(&self) -> u64 {
    match self {
      Self::Launch(value) => value.version.get(),
      Self::Access(value) => value.version.get(),
    }
  }
  /// Verified core content digest.
  #[must_use]
  pub const fn digest(&self) -> &FactoryDigest {
    match self {
      Self::Launch(value) => &value.digest,
      Self::Access(value) => &value.digest,
    }
  }
}

/// Original immutable publication and the exact displayed head; None creates a new identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyIntent {
  /// Complete bounded core publication. Neither publication kind executes anything.
  pub policy: FactoryProjectPolicyRecord,
  /// Exact current head required for replacement, including digest and owner.
  pub expected_current: Option<FactoryProjectPolicyRef>,
}
impl FactoryProjectPolicyIntent {
  /// Enforces consecutive, same-owner typed revisions before a backend encodes integers.
  pub fn validate(&self) -> Result<(), StoreError> {
    let reference = self.policy.reference();
    let next = match &self.expected_current {
      None => Some(1),
      Some(current)
        if current.project_id() == reference.project_id()
          && current.id() == reference.id()
          && std::mem::discriminant(current) == std::mem::discriminant(&reference) =>
      {
        current.version().checked_add(1)
      }
      Some(_) => None,
    };
    if next != Some(reference.version()) || reference.version() > i64::MAX as u64 {
      return Err(crate::StoreError::invalid(
        crate::StoreOperation::PublishFactoryProjectPolicy,
        crate::StoreInputError::InvalidFactoryProjectPolicy,
      ));
    }
    Ok(())
  }
}
/// Accepted management publication with scoped replay and original acceptance time.
#[derive(Clone, Debug)]
pub struct PublishFactoryProjectPolicy {
  /// Reviewed immutable business intent.
  pub intent: FactoryProjectPolicyIntent,
  /// Caller-selected key, partitioned by accepted management scope.
  pub idempotency_key: IdempotencyKey,
  /// First acceptance time; response-loss retries retain the original.
  pub published_at: Timestamp,
}
/// Original publication receipt; later head/default changes cannot replace it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyOutcome {
  /// Applied or historical replay.
  pub disposition: MutationDisposition,
  /// Exact accepted document.
  pub policy: FactoryProjectPolicyRecord,
  /// Original acceptance time.
  pub published_at: Timestamp,
}

/// Historical replay probe before mutable installed-authority qualification; reserves no key.
#[derive(Clone, Debug)]
pub struct ReplayFactoryProjectPolicy {
  /// Exact original immutable contents and precondition.
  pub intent: FactoryProjectPolicyIntent,
  /// Original caller/security-scoped identity.
  pub idempotency_key: IdempotencyKey,
}

/// Exact Project defaults for new drafts; existing draft and Configuration pins are unchanged.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyDefaults {
  /// Existing owner.
  pub project_id: ProjectId,
  /// Positive immutable selection revision, bounded to the store integer range.
  pub version: u64,
  /// Explicit launch selection; None deliberately leaves new drafts unresolved.
  pub launch_policy: Option<FactoryLaunchPolicyRef>,
  /// Explicit access selection; None never becomes unrestricted authority.
  pub access_profile: Option<FactoryAccessProfileRef>,
}
/// Full reviewed default selection. Replacing either pin requires the displayed revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyDefaultsIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// None creates; Some requires the exact current selection revision.
  pub expected_version: Option<u64>,
  /// Explicit immutable launch pin or deliberate absence.
  pub launch_policy: Option<FactoryLaunchPolicyRef>,
  /// Explicit immutable access pin or deliberate absence.
  pub access_profile: Option<FactoryAccessProfileRef>,
}
impl FactoryProjectPolicyDefaultsIntent {
  /// Validates owner and integer bounds, producing the next immutable selection.
  pub fn defaults(&self) -> Result<FactoryProjectPolicyDefaults, StoreError> {
    let version = self
      .expected_version
      .map_or(Some(1), |v| v.checked_add(1))
      .filter(|v| *v <= i64::MAX as u64 && self.expected_version != Some(0));
    let valid_owner = self
      .launch_policy
      .as_ref()
      .is_none_or(|r| r.project_id == self.project_id && r.version.get() <= i64::MAX as u64)
      && self
        .access_profile
        .as_ref()
        .is_none_or(|r| r.project_id == self.project_id && r.version.get() <= i64::MAX as u64);
    if !valid_owner || version.is_none() {
      return Err(crate::StoreError::invalid(
        crate::StoreOperation::SelectFactoryProjectPolicyDefaults,
        crate::StoreInputError::InvalidFactoryProjectPolicy,
      ));
    }
    Ok(FactoryProjectPolicyDefaults {
      project_id: self.project_id,
      version: version.unwrap(),
      launch_policy: self.launch_policy.clone(),
      access_profile: self.access_profile.clone(),
    })
  }
}
/// Authorized explicit default selection with original intent and replay key.
#[derive(Clone, Debug)]
pub struct SelectFactoryProjectPolicyDefaults {
  /// Reviewed selection, including its optimistic precondition.
  pub intent: FactoryProjectPolicyDefaultsIntent,
  /// Caller key partitioned by accepted management scope.
  pub idempotency_key: IdempotencyKey,
  /// Original first acceptance time.
  pub published_at: Timestamp,
}
/// Historical receipt; replay does not adopt current policies or defaults.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyDefaultsOutcome {
  /// Applied or exact replay.
  pub disposition: MutationDisposition,
  /// Original immutable default selection.
  pub defaults: FactoryProjectPolicyDefaults,
  /// Original first acceptance time.
  pub published_at: Timestamp,
}
/// Stale heads are distinct from malformed requests and scoped idempotency conflicts.
#[derive(Debug, thiserror::Error)]
pub enum FactoryProjectPolicyPublicationError {
  /// A competing publication changed the exact displayed head.
  #[error("Project policy precondition failed")]
  PreconditionFailed,
  /// Backend-neutral storage failure.
  #[error(transparent)]
  Store(#[from] StoreError),
}
/// Complete management operations, never table CRUD or a replacement authorization system.
/// Callers authorize the owning Project for reads, commands and replay. Mutations require
/// the existing accepted management context; adapters retain its actor and scoped identity.
#[async_trait::async_trait]
pub trait FactoryProjectPolicyStore: Send + Sync {
  /// Recovers an authorized original receipt without resolving current installation or heads.
  async fn replay_factory_project_policy(
    &self,
    request: ManagementMutation<ReplayFactoryProjectPolicy>,
  ) -> Result<Option<FactoryProjectPolicyOutcome>, StoreError>;
  /// Reviews bounded contents and the current head without reserving a key or publishing.
  /// Publication repeats this check under the Project transaction lock.
  async fn validate_factory_project_policy(
    &self,
    intent: &FactoryProjectPolicyIntent,
  ) -> Result<(), FactoryProjectPolicyPublicationError>;
  /// Reads a retained exact compatibility binding; absence is an actionable unmapped reference.
  async fn factory_legacy_policy_mapping(
    &self,
    project: ProjectId,
    legacy: &crate::FactoryLegacyPolicyReference,
  ) -> Result<Option<crate::FactoryLegacyPolicyMapping>, StoreError>;
  /// Reads initialization provenance without creating any settings.
  async fn factory_project_policy_provisioning(
    &self,
    project: ProjectId,
  ) -> Result<Option<crate::FactoryProjectPolicyProvisioning>, StoreError>;
  /// Atomically initializes/backfills Factory settings, preserving previous operator changes.
  async fn provision_factory_project_policies(
    &self,
    request: ManagementMutation<crate::ProvisionFactoryProjectPolicies>,
  ) -> Result<crate::ProvisionFactoryProjectPoliciesOutcome, StoreError>;
  /// Lists current metadata under the common definition page bound, with an owned exclusive cursor.
  async fn list_factory_project_policies(
    &self,
    project: ProjectId,
    after: Option<&FactoryProjectPolicyRef>,
    limit: u16,
  ) -> Result<FactoryProjectPolicyPage, StoreError>;
  /// Reads current explicit defaults without provisioning or selecting by catalog order.
  async fn factory_project_policy_defaults(
    &self,
    project: ProjectId,
  ) -> Result<Option<FactoryProjectPolicyDefaults>, StoreError>;
  /// Atomically appends an exact default selection, receipt, audit and outbox.
  async fn select_factory_project_policy_defaults(
    &self,
    request: ManagementMutation<SelectFactoryProjectPolicyDefaults>,
  ) -> Result<FactoryProjectPolicyDefaultsOutcome, FactoryProjectPolicyPublicationError>;
  /// Reads only the exact pin in the authorized Project; never substitutes a newer version.
  async fn factory_project_policy(
    &self,
    project: ProjectId,
    reference: &FactoryProjectPolicyRef,
  ) -> Result<Option<FactoryProjectPolicyRecord>, StoreError>;
  /// Atomically appends content, scoped receipt, audit and outbox. No execution side effects.
  async fn publish_factory_project_policy(
    &self,
    request: ManagementMutation<PublishFactoryProjectPolicy>,
  ) -> Result<FactoryProjectPolicyOutcome, FactoryProjectPolicyPublicationError>;
}
