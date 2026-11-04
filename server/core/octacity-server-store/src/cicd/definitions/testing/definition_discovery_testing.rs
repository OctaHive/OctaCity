use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, ProjectId, TriggerId, TriggerVersion};

use crate::pagination::finish_bounded_page;
use crate::{
  CurrentTriggerDefinitionPage, CurrentTriggerDefinitionSummary, ListProjectTriggerDefinitions, ReadVisibilityKind,
  StoreError, TriggerDefinitionDiscoveryStore,
};

/// Deterministic process-local current-Trigger discovery adapter for application tests.
///
/// Its seed type is deliberately credential-free, so fixtures cannot make
/// secret material observable through the discovery contract.
#[derive(Default)]
pub struct InMemoryTriggerDefinitionDiscoveryStore {
  state: Mutex<TriggerDiscoveryMemoryState>,
}

impl InMemoryTriggerDefinitionDiscoveryStore {
  /// Creates an empty Trigger-definition discovery adapter.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one Project that may own current Trigger definitions.
  pub fn seed_project(&self, project_id: ProjectId) -> Result<(), StoreError> {
    self.lock()?.projects.insert(project_id);
    Ok(())
  }

  /// Seeds one immutable Trigger-definition version and advances current when newer.
  pub fn seed_version(&self, summary: CurrentTriggerDefinitionSummary) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains(&summary.project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    let key = (summary.id, summary.version);
    if let Some(stored) = state.versions.get(&key) {
      return if stored == &summary {
        Ok(())
      } else {
        Err(StoreError::Conflict {
          entity: EntityKind::Trigger,
        })
      };
    }
    state
      .current_versions
      .entry(summary.id)
      .and_modify(|version| *version = (*version).max(summary.version))
      .or_insert(summary.version);
    state.versions.insert(key, summary);
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, TriggerDiscoveryMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Default)]
struct TriggerDiscoveryMemoryState {
  projects: BTreeSet<ProjectId>,
  current_versions: BTreeMap<TriggerId, TriggerVersion>,
  versions: BTreeMap<(TriggerId, TriggerVersion), CurrentTriggerDefinitionSummary>,
}

#[async_trait]
impl TriggerDefinitionDiscoveryStore for InMemoryTriggerDefinitionDiscoveryStore {
  async fn list_project_trigger_definitions(
    &self,
    request: ListProjectTriggerDefinitions,
  ) -> Result<CurrentTriggerDefinitionPage, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains(&request.project_id()) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    if request.visibility().kind() == ReadVisibilityKind::None {
      return Ok(CurrentTriggerDefinitionPage {
        items: Vec::new(),
        next_cursor: None,
      });
    }
    let limit = usize::from(request.limit().get());
    let mut items = Vec::with_capacity(limit.saturating_add(1));
    for (id, version) in &state.current_versions {
      let current = state.versions.get(&(*id, *version)).ok_or(StoreError::Unavailable)?;
      if current.project_id != request.project_id()
        || !request.visibility().allows(id)
        || request.after().is_some_and(|after| *id <= after)
      {
        continue;
      }
      items.push(current.clone());
      if items.len() > limit {
        break;
      }
    }
    let next_cursor = finish_bounded_page(&mut items, request.limit(), |item| item.id);
    Ok(CurrentTriggerDefinitionPage { items, next_cursor })
  }
}
