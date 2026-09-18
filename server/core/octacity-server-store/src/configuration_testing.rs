use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, EntityKind, PipelineId, PipelineVersion,
  PoolId, ProjectId, RepositoryId, RepositoryName, RepositoryVersion,
};

use crate::testing::{MutationEvidenceCounts, MutationEvidenceProbe};
use crate::{
  BuildConfigurationMutationOutcome, ConfigurationStore, CreateBuildConfiguration, CreateRepository,
  MutationDisposition, PublishBuildConfigurationVersion, PublishRepositoryVersion, PublishedBuildConfiguration,
  PublishedRepository, RepositoryMutationOutcome, StoreError, StoreOperation,
};

/// Deterministic process-local adapter for configuration application tests.
#[derive(Default)]
pub struct InMemoryConfigurationStore {
  state: Mutex<ConfigurationMemoryState>,
}

impl InMemoryConfigurationStore {
  /// Creates an empty adapter.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one Project allowed to own configuration identities.
  pub fn seed_project(&self, project_id: ProjectId) -> Result<(), StoreError> {
    self.lock()?.projects.insert(project_id);
    Ok(())
  }

  /// Seeds one immutable Pipeline version owned by a Project.
  pub fn seed_pipeline_version(
    &self,
    project_id: ProjectId,
    id: PipelineId,
    version: PipelineVersion,
  ) -> Result<(), StoreError> {
    self.lock()?.pipelines.insert((id, version), project_id);
    Ok(())
  }

  /// Seeds one Agent Pool identity that a configuration may allow.
  pub fn seed_pool(&self, pool_id: PoolId) -> Result<(), StoreError> {
    self.lock()?.pools.insert(pool_id);
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, ConfigurationMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Default)]
struct ConfigurationMemoryState {
  projects: BTreeSet<ProjectId>,
  pipelines: BTreeMap<(PipelineId, PipelineVersion), ProjectId>,
  pools: BTreeSet<PoolId>,
  repositories: BTreeMap<RepositoryId, RepositoryMetadata>,
  repository_names: BTreeMap<(ProjectId, RepositoryName), RepositoryId>,
  repository_versions: BTreeMap<(RepositoryId, RepositoryVersion), PublishedRepository>,
  configurations: BTreeMap<BuildConfigurationId, ConfigurationMetadata>,
  configuration_names: BTreeMap<(ProjectId, BuildConfigurationName), BuildConfigurationId>,
  configuration_versions: BTreeMap<(BuildConfigurationId, BuildConfigurationVersion), PublishedBuildConfiguration>,
  mutations: BTreeMap<(&'static str, String), StoredMutation>,
  evidence: BTreeSet<String>,
}

#[derive(Clone)]
struct RepositoryMetadata {
  project_id: ProjectId,
  name: RepositoryName,
  current_version: RepositoryVersion,
}

#[derive(Clone)]
struct ConfigurationMetadata {
  project_id: ProjectId,
  name: BuildConfigurationName,
  current_version: BuildConfigurationVersion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MutationFingerprint {
  CreateRepository(CreateRepositoryIntent),
  PublishRepository(PublishRepositoryIntent),
  CreateConfiguration(CreateConfigurationIntent),
  PublishConfiguration(PublishConfigurationIntent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CreateRepositoryIntent {
  project_id: ProjectId,
  name: RepositoryName,
  definition: crate::RepositoryDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublishRepositoryIntent {
  id: RepositoryId,
  expected_current_version: RepositoryVersion,
  definition: crate::RepositoryDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CreateConfigurationIntent {
  project_id: ProjectId,
  name: BuildConfigurationName,
  definition: crate::BuildConfigurationDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublishConfigurationIntent {
  id: BuildConfigurationId,
  expected_current_version: BuildConfigurationVersion,
  definition: crate::BuildConfigurationDefinition,
}

#[derive(Clone)]
enum StoredOutcome {
  Repository(PublishedRepository),
  Configuration(Box<PublishedBuildConfiguration>),
}

#[derive(Clone)]
struct StoredMutation {
  fingerprint: MutationFingerprint,
  outcome: StoredOutcome,
}

#[async_trait]
impl MutationEvidenceProbe for InMemoryConfigurationStore {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let state = self
      .lock()
      .expect("in-memory Configuration store must remain available");
    let count = state.evidence.len();
    MutationEvidenceCounts {
      idempotency: count,
      audit: count,
      outbox: count,
    }
  }
}

#[async_trait]
impl ConfigurationStore for InMemoryConfigurationStore {
  async fn create_repository(&self, request: CreateRepository) -> Result<RepositoryMutationOutcome, StoreError> {
    request
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::CreateRepository, source))?;
    let scope = "create-repository";
    let fingerprint = MutationFingerprint::CreateRepository(CreateRepositoryIntent {
      project_id: request.project_id,
      name: request.name.clone(),
      definition: request.definition.clone(),
    });
    let mut state = self.lock()?;
    if let Some(outcome) = replay_repository(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    if !state.projects.contains(&request.project_id) {
      return Err(not_found(EntityKind::Project));
    }
    if state.repositories.contains_key(&request.id)
      || state
        .repository_names
        .contains_key(&(request.project_id, request.name.clone()))
    {
      return Err(conflict(EntityKind::Repository));
    }
    let repository = PublishedRepository {
      id: request.id,
      project_id: request.project_id,
      name: request.name.clone(),
      version: RepositoryVersion::INITIAL,
      definition: request.definition,
      published_at: request.published_at,
    };
    state.repositories.insert(
      repository.id,
      RepositoryMetadata {
        project_id: repository.project_id,
        name: repository.name.clone(),
        current_version: repository.version,
      },
    );
    state
      .repository_names
      .insert((repository.project_id, repository.name.clone()), repository.id);
    state
      .repository_versions
      .insert((repository.id, repository.version), repository.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Repository(repository.clone()),
    );
    Ok(RepositoryMutationOutcome {
      disposition: MutationDisposition::Applied,
      repository,
    })
  }

  async fn publish_repository_version(
    &self,
    request: PublishRepositoryVersion,
  ) -> Result<RepositoryMutationOutcome, StoreError> {
    request
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::PublishRepositoryVersion, source))?;
    let scope = "publish-repository-version";
    let fingerprint = MutationFingerprint::PublishRepository(PublishRepositoryIntent {
      id: request.id,
      expected_current_version: request.expected_current_version,
      definition: request.definition.clone(),
    });
    let mut state = self.lock()?;
    if let Some(outcome) = replay_repository(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let metadata = state
      .repositories
      .get(&request.id)
      .cloned()
      .ok_or_else(|| not_found(EntityKind::Repository))?;
    if metadata.current_version != request.expected_current_version {
      return Err(conflict(EntityKind::Repository));
    }
    let current = state
      .repository_versions
      .get(&(request.id, metadata.current_version))
      .ok_or(StoreError::Unavailable)?;
    if request.published_at < current.published_at {
      return Err(conflict(EntityKind::Repository));
    }
    let version = request
      .expected_current_version
      .next()
      .map_err(|_| StoreError::Unavailable)?;
    let repository = PublishedRepository {
      id: request.id,
      project_id: metadata.project_id,
      name: metadata.name,
      version,
      definition: request.definition,
      published_at: request.published_at,
    };
    state
      .repositories
      .get_mut(&request.id)
      .expect("Repository was read under the same in-memory transaction")
      .current_version = version;
    state
      .repository_versions
      .insert((repository.id, version), repository.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Repository(repository.clone()),
    );
    Ok(RepositoryMutationOutcome {
      disposition: MutationDisposition::Applied,
      repository,
    })
  }

  async fn repository_version(
    &self,
    repository_id: RepositoryId,
    version: RepositoryVersion,
  ) -> Result<PublishedRepository, StoreError> {
    self
      .lock()?
      .repository_versions
      .get(&(repository_id, version))
      .cloned()
      .ok_or_else(|| not_found(EntityKind::Repository))
  }

  async fn create_build_configuration(
    &self,
    request: CreateBuildConfiguration,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    request
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::CreateBuildConfiguration, source))?;
    let scope = "create-build-configuration";
    let fingerprint = MutationFingerprint::CreateConfiguration(CreateConfigurationIntent {
      project_id: request.project_id,
      name: request.name.clone(),
      definition: request.definition.clone(),
    });
    let mut state = self.lock()?;
    if let Some(outcome) = replay_configuration(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    if !state.projects.contains(&request.project_id) {
      return Err(not_found(EntityKind::Project));
    }
    validate_configuration_references(&state, request.project_id, &request.definition)?;
    if state.configurations.contains_key(&request.id)
      || state
        .configuration_names
        .contains_key(&(request.project_id, request.name.clone()))
    {
      return Err(conflict(EntityKind::Configuration));
    }
    let configuration = PublishedBuildConfiguration {
      id: request.id,
      project_id: request.project_id,
      name: request.name.clone(),
      version: BuildConfigurationVersion::INITIAL,
      definition: request.definition,
      published_at: request.published_at,
    };
    state.configurations.insert(
      configuration.id,
      ConfigurationMetadata {
        project_id: configuration.project_id,
        name: configuration.name.clone(),
        current_version: configuration.version,
      },
    );
    state
      .configuration_names
      .insert((configuration.project_id, configuration.name.clone()), configuration.id);
    state
      .configuration_versions
      .insert((configuration.id, configuration.version), configuration.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Configuration(Box::new(configuration.clone())),
    );
    Ok(BuildConfigurationMutationOutcome {
      disposition: MutationDisposition::Applied,
      configuration,
    })
  }

  async fn publish_build_configuration_version(
    &self,
    request: PublishBuildConfigurationVersion,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    request
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::PublishBuildConfigurationVersion, source))?;
    let scope = "publish-build-configuration-version";
    let fingerprint = MutationFingerprint::PublishConfiguration(PublishConfigurationIntent {
      id: request.id,
      expected_current_version: request.expected_current_version,
      definition: request.definition.clone(),
    });
    let mut state = self.lock()?;
    if let Some(outcome) = replay_configuration(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let metadata = state
      .configurations
      .get(&request.id)
      .cloned()
      .ok_or_else(|| not_found(EntityKind::Configuration))?;
    validate_configuration_references(&state, metadata.project_id, &request.definition)?;
    if metadata.current_version != request.expected_current_version {
      return Err(conflict(EntityKind::Configuration));
    }
    let current = state
      .configuration_versions
      .get(&(request.id, metadata.current_version))
      .ok_or(StoreError::Unavailable)?;
    if request.published_at < current.published_at {
      return Err(conflict(EntityKind::Configuration));
    }
    let version = request
      .expected_current_version
      .next()
      .map_err(|_| StoreError::Unavailable)?;
    let configuration = PublishedBuildConfiguration {
      id: request.id,
      project_id: metadata.project_id,
      name: metadata.name,
      version,
      definition: request.definition,
      published_at: request.published_at,
    };
    state
      .configurations
      .get_mut(&request.id)
      .expect("Build Configuration was read under the same in-memory transaction")
      .current_version = version;
    state
      .configuration_versions
      .insert((configuration.id, version), configuration.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Configuration(Box::new(configuration.clone())),
    );
    Ok(BuildConfigurationMutationOutcome {
      disposition: MutationDisposition::Applied,
      configuration,
    })
  }

  async fn build_configuration_version(
    &self,
    configuration_id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<PublishedBuildConfiguration, StoreError> {
    self
      .lock()?
      .configuration_versions
      .get(&(configuration_id, version))
      .cloned()
      .ok_or_else(|| not_found(EntityKind::Configuration))
  }
}

fn validate_configuration_references(
  state: &ConfigurationMemoryState,
  project_id: ProjectId,
  definition: &crate::BuildConfigurationDefinition,
) -> Result<(), StoreError> {
  if !state
    .repository_versions
    .contains_key(&(definition.repository_id, definition.repository_version))
  {
    return Err(not_found(EntityKind::Repository));
  }
  if state
    .pipelines
    .get(&(definition.pipeline_id, definition.pipeline_version))
    != Some(&project_id)
  {
    return Err(not_found(EntityKind::Pipeline));
  }
  if definition.allowed_pools.iter().any(|pool| !state.pools.contains(pool)) {
    return Err(not_found(EntityKind::Pool));
  }
  Ok(())
}

fn replay_repository(
  state: &ConfigurationMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &MutationFingerprint,
) -> Result<Option<RepositoryMutationOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(conflict(EntityKind::Repository));
  }
  match &stored.outcome {
    StoredOutcome::Repository(repository) => Ok(Some(RepositoryMutationOutcome {
      disposition: MutationDisposition::Replayed,
      repository: repository.clone(),
    })),
    StoredOutcome::Configuration(_) => Err(StoreError::Unavailable),
  }
}

fn replay_configuration(
  state: &ConfigurationMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &MutationFingerprint,
) -> Result<Option<BuildConfigurationMutationOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(conflict(EntityKind::Configuration));
  }
  match &stored.outcome {
    StoredOutcome::Configuration(configuration) => Ok(Some(BuildConfigurationMutationOutcome {
      disposition: MutationDisposition::Replayed,
      configuration: configuration.as_ref().clone(),
    })),
    StoredOutcome::Repository(_) => Err(StoreError::Unavailable),
  }
}

fn record(
  state: &mut ConfigurationMemoryState,
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

const fn not_found(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}

const fn conflict(entity: EntityKind) -> StoreError {
  StoreError::Conflict { entity }
}
