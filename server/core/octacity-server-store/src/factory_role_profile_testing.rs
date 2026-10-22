use crate::{
  testing::{InMemoryFactoryConfigurationStore, recorded_management_audit},
  *,
};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::*;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct RoleProfileMemoryState {
  roles: BTreeMap<FactoryRoleId, BTreeMap<FactoryRoleVersion, FactoryRole>>,
  profiles: BTreeMap<FactoryRoleProfileId, BTreeMap<FactoryRoleProfileVersion, FactoryRoleProfile>>,
  mutations: BTreeMap<ManagementIdempotencyKey, (FactoryRoleProfilesIntent, FactoryRoleProfilesOutcome)>,
}
fn missing(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}
fn invalid() -> StoreError {
  StoreError::invalid(
    StoreOperation::PublishFactoryRoleProfiles,
    StoreInputError::InvalidFactoryRoleProfiles,
  )
}
fn replay(
  state: &RoleProfileMemoryState,
  key: &ManagementIdempotencyKey,
  intent: &FactoryRoleProfilesIntent,
) -> Result<Option<FactoryRoleProfilesOutcome>, StoreError> {
  let Some((prior, outcome)) = state.mutations.get(key) else {
    return Ok(None);
  };
  if prior != intent {
    return Err(StoreError::Conflict {
      entity: EntityKind::FactoryRole,
    });
  }
  let mut outcome = outcome.clone();
  outcome.disposition = MutationDisposition::Replayed;
  Ok(Some(outcome))
}
#[async_trait::async_trait]
impl FactoryRoleProfileStore for InMemoryFactoryConfigurationStore {
  async fn list_factory_roles(
    &self,
    project: ProjectId,
    after: Option<FactoryRoleId>,
    limit: u16,
  ) -> Result<FactoryRoleCatalogPage<FactoryRoleId, FactoryRoleSummary>, StoreError> {
    let limit = check_factory_role_catalog_limit(limit, StoreOperation::ListFactoryRoles)?;
    let state = self.lock()?;
    if !state.projects.contains(&project) {
      return Err(missing(EntityKind::Project));
    }
    let mut items: Vec<_> = state
      .role_profiles
      .roles
      .values()
      .filter_map(|versions| versions.last_key_value())
      .map(|(_, role)| role)
      .filter(|role| role.reference().project_id == project)
      .map(FactoryRoleSummary::from)
      .collect();
    let total = items.len() as u64;
    items.retain(|item| after.is_none_or(|after| item.reference.id > after));
    let more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = more.then(|| items.last().unwrap().reference.id);
    Ok(FactoryRoleCatalogPage {
      total,
      items,
      next_cursor,
    })
  }
  async fn list_factory_role_profiles(
    &self,
    project: ProjectId,
    role: FactoryRoleId,
    after: Option<FactoryRoleProfileId>,
    limit: u16,
  ) -> Result<FactoryRoleCatalogPage<FactoryRoleProfileId, FactoryRoleProfileSummary>, StoreError> {
    let limit = check_factory_role_catalog_limit(limit, StoreOperation::ListFactoryRoleProfiles)?;
    let state = self.lock()?;
    if !state
      .role_profiles
      .roles
      .get(&role)
      .and_then(|versions| versions.last_key_value())
      .is_some_and(|(_, role)| role.reference().project_id == project)
    {
      return Err(missing(EntityKind::FactoryRole));
    }
    let mut items: Vec<_> = state
      .role_profiles
      .profiles
      .values()
      .filter_map(|versions| versions.last_key_value())
      .map(|(_, profile)| profile)
      .filter(|profile| profile.reference().project_id == project && profile.role().id == role)
      .map(FactoryRoleProfileSummary::from)
      .collect();
    let total = items.len() as u64;
    items.retain(|item| after.is_none_or(|after| item.reference.id > after));
    let more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = more.then(|| items.last().unwrap().reference.id);
    Ok(FactoryRoleCatalogPage {
      total,
      items,
      next_cursor,
    })
  }

  async fn replay_factory_role_profiles(
    &self,
    request: ManagementMutation<ReplayFactoryRoleProfiles>,
  ) -> Result<Option<FactoryRoleProfilesOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    replay(
      &self.lock()?.role_profiles,
      &audit.scoped_idempotency_key(&request.idempotency_key),
      &request.intent,
    )
  }
  async fn publish_factory_role_profiles(
    &self,
    request: ManagementMutation<PublishFactoryRoleProfiles>,
  ) -> Result<FactoryRoleProfilesOutcome, FactoryRoleProfilePublicationError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(prior) = replay(&state.role_profiles, &key, &request.intent)? {
      return Ok(prior);
    }
    request.intent.validate()?;
    let project = request.intent.project_id;
    if !state.projects.contains(&project) {
      return Err(missing(EntityKind::Project).into());
    }
    let mut next = state.role_profiles.clone();
    let role = if let Some(intent) = &request.intent.role {
      let current = next
        .roles
        .get(&intent.id)
        .and_then(|rows| rows.last_key_value().map(|(_, role)| role));
      if current.is_some_and(|role| role.reference().project_id != project) {
        return Err(missing(EntityKind::FactoryRole).into());
      }
      let role = match (current, intent.expected_current_version) {
        (None, None) => FactoryRole::publish(intent.id, project, intent.definition.clone()),
        (Some(current), Some(expected)) if current.reference().version == expected => current.replace(
          FactoryRoleVersion::new(expected.get().checked_add(1).ok_or_else(invalid)?).map_err(|_| invalid())?,
          intent.definition.clone(),
        ),
        (Some(_), None) => {
          return Err(
            StoreError::Duplicate {
              entity: EntityKind::FactoryRole,
            }
            .into(),
          );
        }
        _ => return Err(FactoryRoleProfilePublicationError::PreconditionFailed),
      }
      .map_err(|_| invalid())?;
      next
        .roles
        .entry(intent.id)
        .or_default()
        .insert(role.reference().version, role.clone());
      Some(role)
    } else {
      None
    };
    let mut profiles = vec![];
    for intent in &request.intent.profiles {
      let current = next
        .profiles
        .get(&intent.id)
        .and_then(|rows| rows.last_key_value().map(|(_, profile)| profile));
      if current.is_some_and(|profile| profile.reference().project_id != project) {
        return Err(missing(EntityKind::FactoryRoleProfile).into());
      }
      let role = next
        .roles
        .get(&intent.role.id)
        .and_then(|rows| rows.get(&intent.role.version))
        .filter(|role| role.reference() == &intent.role && role.reference().project_id == project)
        .ok_or_else(|| missing(EntityKind::FactoryRole))?;
      let profile = match (current, intent.expected_current_version) {
        (None, None) => FactoryRoleProfile::publish(intent.id, project, role, intent.definition.clone()),
        (Some(current), Some(expected)) if current.reference().version == expected => current.replace(
          FactoryRoleProfileVersion::new(expected.get().checked_add(1).ok_or_else(invalid)?).map_err(|_| invalid())?,
          role,
          intent.definition.clone(),
        ),
        (Some(_), None) => {
          return Err(
            StoreError::Duplicate {
              entity: EntityKind::FactoryRoleProfile,
            }
            .into(),
          );
        }
        _ => return Err(FactoryRoleProfilePublicationError::PreconditionFailed),
      }
      .map_err(|_| invalid())?;
      next
        .profiles
        .entry(intent.id)
        .or_default()
        .insert(profile.reference().version, profile.clone());
      profiles.push(profile);
    }
    let outcome = FactoryRoleProfilesOutcome {
      disposition: MutationDisposition::Applied,
      role,
      profiles,
      published_at: request.published_at,
    };
    next.mutations.insert(key.clone(), (request.intent, outcome.clone()));
    state.role_profiles = next;
    state.evidence.insert(format!(
      "publish-factory-role-profiles:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(recorded_management_audit(
      &audit,
      StoreOperation::PublishFactoryRoleProfiles,
      EntityKind::Project,
      project,
    ));
    Ok(outcome)
  }
  async fn factory_role(
    &self,
    project: ProjectId,
    id: FactoryRoleId,
    version: Option<FactoryRoleVersion>,
  ) -> Result<FactoryRole, StoreError> {
    let state = self.lock()?;
    state
      .role_profiles
      .roles
      .get(&id)
      .and_then(|rows| match version {
        Some(v) => rows.get(&v),
        None => rows.last_key_value().map(|(_, row)| row),
      })
      .filter(|row| row.reference().project_id == project)
      .cloned()
      .ok_or_else(|| missing(EntityKind::FactoryRole))
  }
  async fn factory_role_profile(
    &self,
    project: ProjectId,
    id: FactoryRoleProfileId,
    version: Option<FactoryRoleProfileVersion>,
  ) -> Result<FactoryRoleProfile, StoreError> {
    let state = self.lock()?;
    state
      .role_profiles
      .profiles
      .get(&id)
      .and_then(|rows| match version {
        Some(v) => rows.get(&v),
        None => rows.last_key_value().map(|(_, row)| row),
      })
      .filter(|row| row.reference().project_id == project)
      .cloned()
      .ok_or_else(|| missing(EntityKind::FactoryRoleProfile))
  }
}
