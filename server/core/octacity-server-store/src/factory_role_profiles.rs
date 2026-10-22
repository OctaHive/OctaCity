//! Atomic Project AI-agent definition publication; no execution or provider authority.
use crate::{IdempotencyKey, ManagementMutation, MutationDisposition, StoreError};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};

/// Maximum profile publications accepted in one atomic copy/edit.
pub const MAX_FACTORY_ROLE_PROFILE_PUBLICATIONS: usize = 64;

/// One role creation or exact-current replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleIntent {
  /// Stable Project-owned identity.
  pub id: FactoryRoleId,
  /// None creates; Some requires this exact current version.
  pub expected_current_version: Option<FactoryRoleVersion>,
  /// Complete immutable shared instructions.
  pub definition: FactoryRoleDefinition,
}
/// One independently versioned profile publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfileIntent {
  /// Stable profile identity.
  pub id: FactoryRoleProfileId,
  /// None creates; Some requires this exact current version.
  pub expected_current_version: Option<FactoryRoleProfileVersion>,
  /// Exact owned role, including a role published in this same transaction.
  pub role: FactoryRoleRef,
  /// Complete immutable settings; publication does not qualify installed support.
  pub definition: FactoryRoleProfileDefinition,
}
/// Reviewed atomic copy/edit intent. Empty publications and duplicate profile IDs are invalid.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfilesIntent {
  /// Owning existing Project.
  pub project_id: ProjectId,
  /// Optional shared role publication.
  pub role: Option<FactoryRoleIntent>,
  /// Bounded profile publications; unrelated retained profiles are unchanged.
  pub profiles: Vec<FactoryRoleProfileIntent>,
}
impl From<FactoryRoleTemplateInstance> for FactoryRoleProfilesIntent {
  fn from(instance: FactoryRoleTemplateInstance) -> Self {
    Self {
      project_id: instance.role.reference().project_id,
      role: Some(FactoryRoleIntent {
        id: instance.role.reference().id,
        expected_current_version: None,
        definition: instance.role.definition().clone(),
      }),
      profiles: instance
        .profiles
        .into_values()
        .map(|profile| FactoryRoleProfileIntent {
          id: profile.reference().id,
          expected_current_version: None,
          role: profile.role().clone(),
          definition: profile.definition().clone(),
        })
        .collect(),
    }
  }
}
impl FactoryRoleProfilesIntent {
  /// Checks bounded nonempty operation shape before any transactional changes.
  pub fn validate(&self) -> Result<(), StoreError> {
    let mut ids = std::collections::BTreeSet::new();
    if (self.role.is_none() && self.profiles.is_empty())
      || self.profiles.len() > MAX_FACTORY_ROLE_PROFILE_PUBLICATIONS
      || self.profiles.iter().any(|profile| !ids.insert(profile.id))
    {
      return Err(crate::StoreError::invalid(
        crate::StoreOperation::PublishFactoryRoleProfiles,
        crate::StoreInputError::InvalidFactoryRoleProfiles,
      ));
    }
    Ok(())
  }
}
/// Authorized publication with original business intent and acceptance time.
#[derive(Clone, Debug)]
pub struct PublishFactoryRoleProfiles {
  /// Original immutable business intent, excluding request identity/time.
  pub intent: FactoryRoleProfilesIntent,
  /// Scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first acceptance time, retained on replay.
  pub published_at: Timestamp,
}
/// Probe used before resolving mutable installed choices; it never creates state.
#[derive(Clone, Debug)]
pub struct ReplayFactoryRoleProfiles {
  /// Exact original intent.
  pub intent: FactoryRoleProfilesIntent,
  /// Scoped replay identity.
  pub idempotency_key: IdempotencyKey,
}
/// Original receipt, independent of later current heads or catalog changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfilesOutcome {
  /// First application or exact historical replay.
  pub disposition: MutationDisposition,
  /// Published role, when requested.
  pub role: Option<FactoryRole>,
  /// Published profiles in the requested order.
  pub profiles: Vec<FactoryRoleProfile>,
  /// Original accepted time.
  pub published_at: Timestamp,
}
/// Separates stale optimistic preconditions from conflicting mutation identities.
#[derive(Debug, thiserror::Error)]
pub enum FactoryRoleProfilePublicationError {
  /// Some exact current head changed; no portion of the intent was committed.
  #[error("role/profile publication precondition is no longer current")]
  PreconditionFailed,
  /// Safe backend-neutral storage classification.
  #[error(transparent)]
  Store(#[from] StoreError),
}
/// Operation-shaped authority shared by memory and PostgreSQL implementations.
/// Each publication atomically commits all definitions, heads, scoped intent/receipt,
/// audit and outbox evidence. Failed publications commit nothing; exact replay
/// precedes current-version checks and returns the original receipt without new evidence.
#[async_trait::async_trait]
pub trait FactoryRoleProfileStore: Send + Sync {
  /// Lists current safe role metadata in stable ID order under the common page bound.
  async fn list_factory_roles(
    &self,
    project: ProjectId,
    after: Option<FactoryRoleId>,
    limit: u16,
  ) -> Result<crate::FactoryRoleCatalogPage<FactoryRoleId, crate::FactoryRoleSummary>, StoreError>;
  /// Lists current safe profiles for one owned role, independent of its current version.
  async fn list_factory_role_profiles(
    &self,
    project: ProjectId,
    role: FactoryRoleId,
    after: Option<FactoryRoleProfileId>,
    limit: u16,
  ) -> Result<crate::FactoryRoleCatalogPage<FactoryRoleProfileId, crate::FactoryRoleProfileSummary>, StoreError>;
  /// Probes an exact historical intent without revalidating current heads.
  async fn replay_factory_role_profiles(
    &self,
    request: ManagementMutation<ReplayFactoryRoleProfiles>,
  ) -> Result<Option<FactoryRoleProfilesOutcome>, StoreError>;
  /// Publishes role/profile definitions only; never creates a task, Run or Build.
  async fn publish_factory_role_profiles(
    &self,
    request: ManagementMutation<PublishFactoryRoleProfiles>,
  ) -> Result<FactoryRoleProfilesOutcome, FactoryRoleProfilePublicationError>;
  /// Reads an owned immutable version; None selects the current publication.
  async fn factory_role(
    &self,
    project: ProjectId,
    id: FactoryRoleId,
    version: Option<FactoryRoleVersion>,
  ) -> Result<FactoryRole, StoreError>;
  /// Reads an owned immutable profile; role versions never advance implicitly.
  async fn factory_role_profile(
    &self,
    project: ProjectId,
    id: FactoryRoleProfileId,
    version: Option<FactoryRoleProfileVersion>,
  ) -> Result<FactoryRoleProfile, StoreError>;
}
