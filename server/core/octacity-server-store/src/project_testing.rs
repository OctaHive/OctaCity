use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, ProjectId, ProjectVersion};

use crate::testing::{MutationEvidenceCounts, MutationEvidenceProbe};
use crate::{
  CreateProject, DeleteProject, DeleteProjectOutcome, ListProjects, MoveProject, MutationDisposition, Project,
  ProjectDetails, ProjectMutationOutcome, ProjectPage, ProjectStore, RenameProject, StoreError,
  validate_project_ancestry,
};

/// Deterministic process-local Project-store adapter for application tests.
#[derive(Default)]
pub struct InMemoryProjectStore {
  state: Mutex<ProjectMemoryState>,
}

impl InMemoryProjectStore {
  /// Creates an empty Project hierarchy.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Marks a Project as referenced by a non-hierarchy domain resource.
  ///
  /// This test-support seam models references that later feature tasks add to
  /// the same authoritative database without exposing table-shaped APIs.
  pub fn seed_active_reference(&self, project_id: ProjectId) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains_key(&project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    state.active_references.insert(project_id);
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, ProjectMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Default)]
struct ProjectMemoryState {
  projects: BTreeMap<ProjectId, Project>,
  mutations: BTreeMap<(&'static str, String), StoredMutation>,
  active_references: BTreeSet<ProjectId>,
  evidence: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MutationFingerprint {
  Create {
    id: ProjectId,
    parent_id: Option<ProjectId>,
    name: octacity_server_domain::ProjectName,
  },
  Rename {
    id: ProjectId,
    expected_version: ProjectVersion,
    name: octacity_server_domain::ProjectName,
  },
  Move {
    id: ProjectId,
    expected_version: ProjectVersion,
    parent_id: Option<ProjectId>,
  },
  Delete {
    id: ProjectId,
    expected_version: ProjectVersion,
  },
}

#[derive(Clone)]
enum StoredOutcome {
  Project(Project),
  Deleted(ProjectId),
}

#[derive(Clone)]
struct StoredMutation {
  fingerprint: MutationFingerprint,
  outcome: StoredOutcome,
}

#[async_trait]
impl MutationEvidenceProbe for InMemoryProjectStore {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let state = self.lock().expect("in-memory Project store must remain available");
    let count = state.evidence.len();
    MutationEvidenceCounts {
      idempotency: count,
      audit: count,
      outbox: count,
    }
  }
}

#[async_trait]
impl ProjectStore for InMemoryProjectStore {
  async fn create_project(&self, request: CreateProject) -> Result<ProjectMutationOutcome, StoreError> {
    let scope = "create-project";
    let fingerprint = MutationFingerprint::Create {
      id: request.id,
      parent_id: request.parent_id,
      name: request.name.clone(),
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_project(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    require_parent(&state, request.parent_id)?;
    require_unique_name(&state, request.parent_id, &request.name, None)?;
    if state.projects.contains_key(&request.id) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::Project,
      });
    }
    let project = Project {
      id: request.id,
      parent_id: request.parent_id,
      name: request.name,
      version: ProjectVersion::INITIAL,
      created_at: request.created_at,
      updated_at: request.created_at,
    };
    state.projects.insert(project.id, project.clone());
    record_mutation(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Project(project.clone()),
    );
    Ok(ProjectMutationOutcome {
      disposition: MutationDisposition::Applied,
      project,
    })
  }

  async fn rename_project(&self, request: RenameProject) -> Result<ProjectMutationOutcome, StoreError> {
    let scope = "rename-project";
    let fingerprint = MutationFingerprint::Rename {
      id: request.id,
      expected_version: request.expected_version,
      name: request.name.clone(),
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_project(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let current = require_current(&state, request.id, request.expected_version)?.clone();
    require_mutation_time(&current, request.renamed_at)?;
    require_unique_name(&state, current.parent_id, &request.name, Some(request.id))?;
    let project = Project {
      name: request.name,
      version: next_version(current.version)?,
      updated_at: request.renamed_at,
      ..current
    };
    state.projects.insert(project.id, project.clone());
    record_mutation(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Project(project.clone()),
    );
    Ok(ProjectMutationOutcome {
      disposition: MutationDisposition::Applied,
      project,
    })
  }

  async fn move_project(&self, request: MoveProject) -> Result<ProjectMutationOutcome, StoreError> {
    let scope = "move-project";
    let fingerprint = MutationFingerprint::Move {
      id: request.id,
      expected_version: request.expected_version,
      parent_id: request.parent_id,
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_project(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let current = require_current(&state, request.id, request.expected_version)?.clone();
    require_mutation_time(&current, request.moved_at)?;
    require_parent(&state, request.parent_id)?;
    require_acyclic_move(&state, request.id, request.parent_id)?;
    require_unique_name(&state, request.parent_id, &current.name, Some(request.id))?;
    let project = Project {
      parent_id: request.parent_id,
      version: next_version(current.version)?,
      updated_at: request.moved_at,
      ..current
    };
    state.projects.insert(project.id, project.clone());
    record_mutation(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Project(project.clone()),
    );
    Ok(ProjectMutationOutcome {
      disposition: MutationDisposition::Applied,
      project,
    })
  }

  async fn project(&self, project_id: ProjectId) -> Result<ProjectDetails, StoreError> {
    let state = self.lock()?;
    let project = state.projects.get(&project_id).cloned().ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
    let mut ancestors = Vec::new();
    let mut parent_id = project.parent_id;
    while let Some(id) = parent_id {
      let parent = state.projects.get(&id).ok_or(StoreError::Unavailable)?;
      ancestors.push(parent.clone());
      parent_id = parent.parent_id;
    }
    ancestors.reverse();
    Ok(ProjectDetails { project, ancestors })
  }

  async fn list_projects(&self, request: ListProjects) -> Result<ProjectPage, StoreError> {
    let state = self.lock()?;
    if let Some(parent_id) = request.parent_id() {
      require_parent(&state, Some(parent_id))?;
    }
    let limit = usize::from(request.limit().get());
    let mut projects: Vec<_> = state
      .projects
      .values()
      .filter(|project| project.parent_id == request.parent_id() && request.after().is_none_or(|id| project.id > id))
      .take(limit + 1)
      .cloned()
      .collect();
    let has_more = projects.len() > limit;
    projects.truncate(limit);
    let next_cursor = has_more.then(|| projects.last().expect("a non-zero full page has a last item").id);
    Ok(ProjectPage { projects, next_cursor })
  }

  async fn delete_project(&self, request: DeleteProject) -> Result<DeleteProjectOutcome, StoreError> {
    let scope = "delete-project";
    let fingerprint = MutationFingerprint::Delete {
      id: request.id,
      expected_version: request.expected_version,
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_delete(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let current = require_current(&state, request.id, request.expected_version)?;
    require_mutation_time(current, request.deleted_at)?;
    if state.active_references.contains(&request.id)
      || state
        .projects
        .values()
        .any(|project| project.parent_id == Some(request.id))
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::Project,
      });
    }
    state.projects.remove(&request.id);
    record_mutation(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Deleted(request.id),
    );
    Ok(DeleteProjectOutcome {
      disposition: MutationDisposition::Applied,
      project_id: request.id,
    })
  }
}

fn replay_project(
  state: &ProjectMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &MutationFingerprint,
) -> Result<Option<ProjectMutationOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  let StoredOutcome::Project(project) = &stored.outcome else {
    return Err(StoreError::Unavailable);
  };
  Ok(Some(ProjectMutationOutcome {
    disposition: MutationDisposition::Replayed,
    project: project.clone(),
  }))
}

fn replay_delete(
  state: &ProjectMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &MutationFingerprint,
) -> Result<Option<DeleteProjectOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  let StoredOutcome::Deleted(project_id) = stored.outcome else {
    return Err(StoreError::Unavailable);
  };
  Ok(Some(DeleteProjectOutcome {
    disposition: MutationDisposition::Replayed,
    project_id,
  }))
}

fn record_mutation(
  state: &mut ProjectMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: MutationFingerprint,
  outcome: StoredOutcome,
) {
  state
    .mutations
    .insert((scope, key.to_owned()), StoredMutation { fingerprint, outcome });
  state.evidence.insert(format!("{scope}:{key}"));
}

fn require_parent(state: &ProjectMemoryState, parent_id: Option<ProjectId>) -> Result<(), StoreError> {
  if parent_id.is_some_and(|id| !state.projects.contains_key(&id)) {
    return Err(StoreError::NotFound {
      entity: EntityKind::Project,
    });
  }
  Ok(())
}

fn require_current(
  state: &ProjectMemoryState,
  project_id: ProjectId,
  expected_version: ProjectVersion,
) -> Result<&Project, StoreError> {
  let project = state.projects.get(&project_id).ok_or(StoreError::NotFound {
    entity: EntityKind::Project,
  })?;
  if project.version != expected_version {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  Ok(project)
}

fn require_unique_name(
  state: &ProjectMemoryState,
  parent_id: Option<ProjectId>,
  name: &octacity_server_domain::ProjectName,
  excluded_id: Option<ProjectId>,
) -> Result<(), StoreError> {
  if state
    .projects
    .values()
    .any(|project| project.parent_id == parent_id && &project.name == name && Some(project.id) != excluded_id)
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  Ok(())
}

fn require_acyclic_move(
  state: &ProjectMemoryState,
  project_id: ProjectId,
  mut parent_id: Option<ProjectId>,
) -> Result<(), StoreError> {
  let mut ancestry = Vec::new();
  while let Some(id) = parent_id {
    ancestry.push(id);
    parent_id = state.projects.get(&id).ok_or(StoreError::Unavailable)?.parent_id;
  }
  validate_project_ancestry(project_id, ancestry).map_err(|_| StoreError::Conflict {
    entity: EntityKind::Project,
  })
}

fn require_mutation_time(project: &Project, timestamp: octacity_server_domain::Timestamp) -> Result<(), StoreError> {
  if timestamp < project.updated_at {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  Ok(())
}

fn next_version(version: ProjectVersion) -> Result<ProjectVersion, StoreError> {
  version.next().map_err(|_| StoreError::Unavailable)
}
