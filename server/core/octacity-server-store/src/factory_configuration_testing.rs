use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfiguration, FactoryConfigurationChoices, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryDigest,
};

use crate::factory_run_testing::StoredFactoryRun;
use crate::testing::{
  ManagementAuditProbe, MutationEvidenceCounts, MutationEvidenceProbe, RecordedManagementAuditFact,
  recorded_management_audit,
};
use crate::{
  AdmitFactoryWork, CreateFactoryConfiguration, FactoryAdmissionContext, FactoryAdmissionMutationOutcome,
  FactoryAdmissionProbe, FactoryAdmissionStore, FactoryConfigurationAvailability, FactoryConfigurationMutationIntent,
  FactoryConfigurationMutationOutcome, FactoryConfigurationStore, FactoryWorkSourceScope, ManagementIdempotencyKey,
  ManagementMutation, MutationDisposition, PublishedFactoryAdmission, PublishedFactoryConfiguration,
  PublishedRepository, ReadFactoryAdmissionContext, ReplaceFactoryConfiguration, ReplaceFactoryConfigurationError,
  ReplayFactoryConfigurationMutation, StoreError, StoreOperation,
};

/// Deterministic process-local Factory Configuration adapter for application tests.
#[derive(Default)]
pub struct InMemoryFactoryConfigurationStore {
  state: Mutex<FactoryConfigurationMemoryState>,
}

impl InMemoryFactoryConfigurationStore {
  /// Creates an empty adapter with Factory mode disabled for every Project.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one Project without enabling Factory mode.
  pub fn seed_project(&self, project_id: ProjectId) -> Result<(), StoreError> {
    self.lock()?.projects.insert(project_id);
    Ok(())
  }

  /// Enables Factory mode for one seeded Project with trusted exact choices.
  pub fn enable_project(&self, project_id: ProjectId, choices: FactoryConfigurationChoices) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains(&project_id) {
      return Err(not_found(EntityKind::Project));
    }
    state.choices.insert(project_id, choices);
    Ok(())
  }

  /// Disables Factory mode for one seeded Project without deleting immutable history.
  pub fn disable_project(&self, project_id: ProjectId) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains(&project_id) {
      return Err(not_found(EntityKind::Project));
    }
    state.choices.remove(&project_id);
    Ok(())
  }

  /// Seeds one exact immutable Factory Configuration version for admission tests.
  pub fn seed_factory_configuration_version(
    &self,
    configuration: PublishedFactoryConfiguration,
  ) -> Result<(), StoreError> {
    let reference = configuration.configuration.reference();
    let mut state = self.lock()?;
    if !state.projects.contains(&reference.project_id()) {
      return Err(not_found(EntityKind::Project));
    }
    let key = (reference.id(), reference.version());
    if state
      .versions
      .get(&key)
      .is_some_and(|current| current != &configuration)
    {
      return Err(conflict());
    }
    state.current_versions.insert(reference.id(), reference.version());
    state.versions.insert(key, configuration);
    Ok(())
  }

  /// Seeds one admitted Factory Run for worker and store contract tests.
  pub fn seed_factory_run(
    &self,
    admission: PublishedFactoryAdmission,
    intent_digest: FactoryDigest,
    audit: &crate::MutationAuditContext,
  ) -> Result<(), StoreError> {
    let run_id = admission.run.id();
    let mut state = self.lock()?;
    if state.factory_runs.contains_key(&run_id) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::FactoryRun,
      });
    }
    let stored = StoredFactoryRun::admitted(&admission, intent_digest, audit)?;
    state.factory_runs.insert(run_id, stored);
    Ok(())
  }

  /// Seeds one exact immutable Repository version for admission tests.
  pub fn seed_repository_version(&self, repository: PublishedRepository) -> Result<(), StoreError> {
    repository
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::ReadFactoryAdmissionContext, source))?;
    let mut state = self.lock()?;
    if !state.projects.contains(&repository.project_id) {
      return Err(not_found(EntityKind::Project));
    }
    let key = (repository.id, repository.version);
    if state
      .repositories
      .get(&key)
      .is_some_and(|current| current != &repository)
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::Repository,
      });
    }
    state.repositories.insert(key, repository);
    Ok(())
  }

  pub(super) fn lock(&self) -> Result<MutexGuard<'_, FactoryConfigurationMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Default)]
pub(super) struct FactoryConfigurationMemoryState {
  projects: BTreeSet<ProjectId>,
  choices: BTreeMap<ProjectId, FactoryConfigurationChoices>,
  current_versions: BTreeMap<FactoryConfigurationId, FactoryConfigurationVersion>,
  versions: BTreeMap<(FactoryConfigurationId, FactoryConfigurationVersion), PublishedFactoryConfiguration>,
  repositories: BTreeMap<
    (
      octacity_server_domain::RepositoryId,
      octacity_server_domain::RepositoryVersion,
    ),
    PublishedRepository,
  >,
  mutations: BTreeMap<(&'static str, ManagementIdempotencyKey), StoredMutation>,
  admission_mutations: BTreeMap<ManagementIdempotencyKey, StoredAdmission>,
  admissions: BTreeMap<(FactoryWorkSourceScope, ExternalWorkIdentity), StoredAdmission>,
  evidence: BTreeSet<String>,
  audit: BTreeSet<RecordedManagementAuditFact>,
  pub(super) factory_runs: BTreeMap<octacity_server_factory::FactoryRunId, StoredFactoryRun>,
}

#[derive(Clone)]
struct StoredMutation {
  intent: FactoryConfigurationMutationIntent,
  outcome: PublishedFactoryConfiguration,
}

#[derive(Clone)]
struct StoredAdmission {
  source_scope: FactoryWorkSourceScope,
  external_identity: ExternalWorkIdentity,
  intent_digest: FactoryDigest,
  outcome: PublishedFactoryAdmission,
}

#[async_trait]
impl MutationEvidenceProbe for InMemoryFactoryConfigurationStore {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let state = self.lock().expect("in-memory Factory Configuration store is available");
    let count = state.evidence.len();
    MutationEvidenceCounts {
      idempotency: count,
      audit: state.audit.len(),
      outbox: count,
    }
  }
}

#[async_trait]
impl ManagementAuditProbe for InMemoryFactoryConfigurationStore {
  async fn management_audit_facts(&self) -> Vec<RecordedManagementAuditFact> {
    self
      .lock()
      .expect("in-memory Factory Configuration store is available")
      .audit
      .iter()
      .cloned()
      .collect()
  }
}

#[async_trait]
impl FactoryConfigurationStore for InMemoryFactoryConfigurationStore {
  async fn replay_factory_configuration_mutation(
    &self,
    request: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let scope = mutation_scope(&request.intent);
    let idempotency = audit.scoped_idempotency_key(&request.idempotency_key);
    let state = self.lock()?;
    replay(&state, scope, &idempotency, &request.intent)
  }

  async fn factory_configuration_availability(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains(&project_id) {
      return Err(not_found(EntityKind::Project));
    }
    Ok(state.choices.get(&project_id).cloned().map_or(
      FactoryConfigurationAvailability::Disabled,
      FactoryConfigurationAvailability::Available,
    ))
  }

  async fn create_factory_configuration(
    &self,
    request: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let scope = "create-factory-configuration";
    let idempotency = audit.scoped_idempotency_key(&request.idempotency_key);
    validate_configuration_intent(
      &request.intent,
      &request.configuration,
      None,
      StoreOperation::CreateFactoryConfiguration,
    )?;
    let reference = request.configuration.reference();
    let id = reference.id();
    let project_id = reference.project_id();
    let version = reference.version();
    let mut state = self.lock()?;
    if let Some(outcome) = replay(&state, scope, &idempotency, &request.intent)? {
      return Ok(outcome);
    }
    if !state.projects.contains(&project_id) {
      return Err(not_found(EntityKind::Project));
    }
    if !state.choices.contains_key(&project_id) {
      return Err(StoreError::Unavailable);
    }
    if version != FactoryConfigurationVersion::INITIAL || state.current_versions.contains_key(&id) {
      return Err(conflict());
    }
    let published = PublishedFactoryConfiguration {
      configuration: request.configuration,
      published_at: request.published_at,
    };
    state.current_versions.insert(id, version);
    state.versions.insert((id, version), published.clone());
    record(
      &mut state,
      scope,
      idempotency,
      request.intent,
      published.clone(),
      recorded_management_audit(
        &audit,
        StoreOperation::CreateFactoryConfiguration,
        EntityKind::FactoryConfiguration,
        id,
      ),
    );
    Ok(FactoryConfigurationMutationOutcome {
      disposition: MutationDisposition::Applied,
      configuration: published,
    })
  }

  async fn replace_factory_configuration(
    &self,
    request: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
    let (request, audit) = request.into_parts();
    let scope = "replace-factory-configuration";
    let idempotency = audit.scoped_idempotency_key(&request.idempotency_key);
    validate_configuration_intent(
      &request.intent,
      &request.configuration,
      Some(request.expected_current_version),
      StoreOperation::ReplaceFactoryConfiguration,
    )?;
    let reference = request.configuration.reference();
    let id = reference.id();
    let project_id = reference.project_id();
    let version = reference.version();
    let mut state = self.lock()?;
    if let Some(outcome) = replay(&state, scope, &idempotency, &request.intent)? {
      return Ok(outcome);
    }
    if !state.choices.contains_key(&project_id) {
      return Err(StoreError::Unavailable.into());
    }
    let current_version = state
      .current_versions
      .get(&id)
      .copied()
      .ok_or_else(|| ReplaceFactoryConfigurationError::Store(not_found(EntityKind::FactoryConfiguration)))?;
    if current_version != request.expected_current_version {
      return Err(ReplaceFactoryConfigurationError::PreconditionFailed);
    }
    let current = state
      .versions
      .get(&(id, current_version))
      .ok_or(StoreError::Unavailable)?;
    if current.configuration.reference().project_id() != project_id || request.published_at < current.published_at {
      return Err(conflict().into());
    }
    let expected_next = current_version
      .get()
      .checked_add(1)
      .and_then(|value| FactoryConfigurationVersion::new(value).ok())
      .ok_or(StoreError::Unavailable)?;
    if version != expected_next {
      return Err(conflict().into());
    }
    let published = PublishedFactoryConfiguration {
      configuration: request.configuration,
      published_at: request.published_at,
    };
    state.current_versions.insert(id, version);
    state.versions.insert((id, version), published.clone());
    record(
      &mut state,
      scope,
      idempotency,
      request.intent,
      published.clone(),
      recorded_management_audit(
        &audit,
        StoreOperation::ReplaceFactoryConfiguration,
        EntityKind::FactoryConfiguration,
        id,
      ),
    );
    Ok(FactoryConfigurationMutationOutcome {
      disposition: MutationDisposition::Applied,
      configuration: published,
    })
  }

  async fn factory_configuration_version(
    &self,
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    self
      .lock()?
      .versions
      .get(&(id, version))
      .cloned()
      .ok_or_else(|| not_found(EntityKind::FactoryConfiguration))
  }

  async fn current_factory_configuration(
    &self,
    id: FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    let state = self.lock()?;
    let version = state
      .current_versions
      .get(&id)
      .copied()
      .ok_or_else(|| not_found(EntityKind::FactoryConfiguration))?;
    state
      .versions
      .get(&(id, version))
      .cloned()
      .ok_or(StoreError::Unavailable)
  }
}

#[async_trait]
impl FactoryAdmissionStore for InMemoryFactoryConfigurationStore {
  async fn replay_factory_admission(
    &self,
    probe: &FactoryAdmissionProbe,
  ) -> Result<Option<FactoryAdmissionMutationOutcome>, StoreError> {
    let state = self.lock()?;
    replay_admission(&state, probe)
  }

  async fn factory_admission_context(
    &self,
    request: ReadFactoryAdmissionContext,
  ) -> Result<FactoryAdmissionContext, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains(&request.project_id) {
      return Err(not_found(EntityKind::Project));
    }
    let configuration = state
      .versions
      .get(&(request.configuration_id, request.configuration_version))
      .filter(|published| published.configuration.reference().project_id() == request.project_id)
      .cloned()
      .ok_or_else(|| not_found(EntityKind::FactoryConfiguration))?;
    let repository = state
      .repositories
      .get(&(request.repository_id, request.repository_version))
      .filter(|published| published.project_id == request.project_id)
      .cloned()
      .ok_or_else(|| not_found(EntityKind::Repository))?;
    Ok(FactoryAdmissionContext {
      configuration,
      repository,
    })
  }

  async fn admit_factory_work(
    &self,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    if audit.security_scope() != &request.probe.source_scope.security_scope {
      return Err(StoreError::invalid(
        StoreOperation::AdmitFactoryWork,
        crate::StoreInputError::InvalidMutationAuditContext,
      ));
    }
    let idempotency = audit.scoped_idempotency_key(&request.probe.idempotency_key);
    let mut state = self.lock()?;
    if let Some(outcome) = replay_admission(&state, &request.probe)? {
      return Ok(outcome);
    }
    let configuration = state
      .versions
      .get(&(
        request.work.configuration().id(),
        request.work.configuration().version(),
      ))
      .ok_or_else(|| not_found(EntityKind::FactoryConfiguration))?;
    let repository = state
      .repositories
      .get(&(request.work.subject().repository_id(), request.repository_version))
      .ok_or_else(|| not_found(EntityKind::Repository))?;
    if !configuration.configuration.is_enabled()
      || configuration.configuration.reference() != request.work.configuration()
      || repository.project_id != request.work.subject().project_id()
      || request.work.external_identity() != &request.probe.external_identity
      || request.run.configuration() != request.work.configuration()
      || request.run.work_id() != request.work.id()
      || request.run.subject() != request.work.subject()
    {
      return Err(StoreError::invalid(
        StoreOperation::AdmitFactoryWork,
        crate::StoreInputError::InvalidFactoryAdmission,
      ));
    }
    let active_runs = state
      .factory_runs
      .values()
      .filter(|stored| {
        stored.run().configuration() == request.work.configuration()
          && stored.run().state() != octacity_server_factory::FactoryRunState::Completed
      })
      .count();
    if active_runs >= configuration.configuration.wip_limits().max_active_runs() as usize {
      return Err(StoreError::Conflict {
        entity: EntityKind::FactoryRun,
      });
    }
    let outcome = PublishedFactoryAdmission {
      work: request.work,
      run: request.run,
      admitted_at: request.admitted_at,
    };
    if state.factory_runs.contains_key(&outcome.run.id()) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::FactoryRun,
      });
    }
    let stored_run = StoredFactoryRun::admitted(&outcome, request.probe.intent_digest, &audit)?;
    let stored = StoredAdmission {
      source_scope: request.probe.source_scope.clone(),
      external_identity: request.probe.external_identity.clone(),
      intent_digest: request.probe.intent_digest,
      outcome: outcome.clone(),
    };
    state.admission_mutations.insert(idempotency.clone(), stored.clone());
    state
      .admissions
      .insert((request.probe.source_scope, request.probe.external_identity), stored);
    state.factory_runs.insert(outcome.run.id(), stored_run);
    state
      .evidence
      .insert(format!("admit-factory-work:{}", idempotency.caller_key().as_str()));
    state.audit.insert(recorded_management_audit(
      &audit,
      StoreOperation::AdmitFactoryWork,
      EntityKind::FactoryRun,
      outcome.run.id(),
    ));
    Ok(FactoryAdmissionMutationOutcome {
      disposition: MutationDisposition::Applied,
      admission: outcome,
    })
  }
}

fn replay_admission(
  state: &FactoryConfigurationMemoryState,
  probe: &FactoryAdmissionProbe,
) -> Result<Option<FactoryAdmissionMutationOutcome>, StoreError> {
  let idempotency =
    ManagementIdempotencyKey::new(probe.source_scope.security_scope.clone(), probe.idempotency_key.clone());
  let stored = state.admission_mutations.get(&idempotency).or_else(|| {
    state
      .admissions
      .get(&(probe.source_scope.clone(), probe.external_identity.clone()))
  });
  let Some(stored) = stored else {
    return Ok(None);
  };
  if stored.source_scope != probe.source_scope
    || stored.external_identity != probe.external_identity
    || stored.intent_digest != probe.intent_digest
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::FactoryRun,
    });
  }
  Ok(Some(FactoryAdmissionMutationOutcome {
    disposition: MutationDisposition::Replayed,
    admission: stored.outcome.clone(),
  }))
}

fn replay(
  state: &FactoryConfigurationMemoryState,
  scope: &'static str,
  key: &ManagementIdempotencyKey,
  intent: &FactoryConfigurationMutationIntent,
) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.clone())) else {
    return Ok(None);
  };
  if &stored.intent != intent {
    return Err(conflict());
  }
  Ok(Some(FactoryConfigurationMutationOutcome {
    disposition: MutationDisposition::Replayed,
    configuration: stored.outcome.clone(),
  }))
}

fn record(
  state: &mut FactoryConfigurationMemoryState,
  scope: &'static str,
  key: ManagementIdempotencyKey,
  intent: FactoryConfigurationMutationIntent,
  outcome: PublishedFactoryConfiguration,
  audit: RecordedManagementAuditFact,
) {
  let evidence = format!("{scope}:{}", key.caller_key().as_str());
  state.mutations.insert((scope, key), StoredMutation { intent, outcome });
  state.evidence.insert(evidence);
  state.audit.insert(audit);
}

fn mutation_scope(intent: &FactoryConfigurationMutationIntent) -> &'static str {
  match intent {
    FactoryConfigurationMutationIntent::Create { .. } => "create-factory-configuration",
    FactoryConfigurationMutationIntent::Replace { .. } => "replace-factory-configuration",
  }
}

fn validate_configuration_intent(
  intent: &FactoryConfigurationMutationIntent,
  configuration: &FactoryConfiguration,
  expected_current_version: Option<FactoryConfigurationVersion>,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let reference = configuration.reference();
  let matches = match intent {
    FactoryConfigurationMutationIntent::Create {
      id,
      project_id,
      definition_digest,
      ..
    } => {
      expected_current_version.is_none()
        && reference.id() == *id
        && reference.project_id() == *project_id
        && reference.version() == FactoryConfigurationVersion::INITIAL
        && reference.definition_digest() == *definition_digest
    }
    FactoryConfigurationMutationIntent::Replace {
      id,
      expected_current_version: intent_version,
      definition_digest,
      ..
    } => {
      expected_current_version == Some(*intent_version)
        && reference.id() == *id
        && reference.version().get() == intent_version.get().saturating_add(1)
        && reference.definition_digest() == *definition_digest
    }
  };
  if matches {
    Ok(())
  } else {
    Err(StoreError::invalid(
      operation,
      crate::StoreInputError::InvalidFactoryConfiguration,
    ))
  }
}

fn not_found(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryConfiguration,
  }
}
