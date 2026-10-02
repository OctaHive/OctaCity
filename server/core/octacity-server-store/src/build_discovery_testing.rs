use std::{cmp::Reverse, collections::BTreeMap, collections::BTreeSet, sync::Mutex, sync::MutexGuard};

use async_trait::async_trait;
use octacity_server_domain::{BuildConfigurationId, BuildId, EntityKind, ProjectId};

use crate::pagination::finish_bounded_page;
use crate::{
  BuildDiscoveryStore, ListProjectBuilds, ProjectBuildPage, ProjectBuildSummary, ReadVisibilityKind, StoreError,
};

/// Deterministic process-local adapter for Project Build discovery tests.
#[derive(Default)]
pub struct InMemoryBuildDiscoveryStore {
  state: Mutex<BuildDiscoveryMemoryState>,
}

impl InMemoryBuildDiscoveryStore {
  /// Creates an empty Build discovery adapter.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one Project that may own Build Configurations and Builds.
  pub fn seed_project(&self, project_id: ProjectId) -> Result<(), StoreError> {
    self.lock()?.projects.insert(project_id);
    Ok(())
  }

  /// Seeds one Project-owned Build Configuration identity used by exact filters.
  pub fn seed_configuration(
    &self,
    project_id: ProjectId,
    configuration_id: BuildConfigurationId,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains(&project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    match state.configuration_projects.get(&configuration_id) {
      Some(owner) if *owner == project_id => Ok(()),
      Some(_) => Err(StoreError::Conflict {
        entity: EntityKind::Configuration,
      }),
      None => {
        state.configuration_projects.insert(configuration_id, project_id);
        Ok(())
      }
    }
  }

  /// Seeds one current Build summary after validating its Project ownership.
  pub fn seed_build(&self, summary: ProjectBuildSummary) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains(&summary.project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    if state.configuration_projects.get(&summary.configuration_id) != Some(&summary.project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Configuration,
      });
    }
    match state.builds.get(&summary.id) {
      Some(stored) if stored == &summary => Ok(()),
      Some(_) => Err(StoreError::Conflict {
        entity: EntityKind::Build,
      }),
      None => {
        state.builds.insert(summary.id, summary);
        Ok(())
      }
    }
  }

  fn lock(&self) -> Result<MutexGuard<'_, BuildDiscoveryMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Default)]
struct BuildDiscoveryMemoryState {
  projects: BTreeSet<ProjectId>,
  configuration_projects: BTreeMap<BuildConfigurationId, ProjectId>,
  builds: BTreeMap<BuildId, ProjectBuildSummary>,
}

#[async_trait]
impl BuildDiscoveryStore for InMemoryBuildDiscoveryStore {
  async fn list_project_builds(&self, request: ListProjectBuilds) -> Result<ProjectBuildPage, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains(&request.project_id()) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    if request.filter().configuration_id.is_some_and(|configuration_id| {
      state.configuration_projects.get(&configuration_id) != Some(&request.project_id())
    }) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Configuration,
      });
    }
    if request.visibility().kind() == ReadVisibilityKind::None {
      return Ok(ProjectBuildPage {
        items: Vec::new(),
        next_cursor: None,
      });
    }

    let filter = request.filter();
    let after = request.after();
    let mut items = state
      .builds
      .values()
      .filter(|summary| summary.project_id == request.project_id())
      .filter(|summary| request.visibility().allows(&summary.id))
      .filter(|summary| filter.configuration_id.is_none_or(|id| summary.configuration_id == id))
      .filter(|summary| filter.state.is_none_or(|state| summary.state == state))
      .filter(|summary| after.is_none_or(|cursor| summary.page_position() < cursor))
      .cloned()
      .collect::<Vec<_>>();
    items.sort_unstable_by_key(|summary| Reverse(summary.page_position()));
    items.truncate(usize::from(request.limit().get()).saturating_add(1));
    let next_cursor = finish_bounded_page(&mut items, request.limit(), ProjectBuildSummary::page_position);
    Ok(ProjectBuildPage { items, next_cursor })
  }
}
