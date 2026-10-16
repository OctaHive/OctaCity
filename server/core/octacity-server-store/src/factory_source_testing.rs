use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;
use crate::{FactorySourceStore, PublishWorkSources, StoreError, source_conflict, validate_source_input_dependencies};
use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  WorkSourceGroup, WorkSourceGroupId, WorkSourceGroupVersion, WorkSourceInput, WorkSourceInputId,
  WorkSourceInputVersion,
};
use std::collections::BTreeMap;

#[derive(Default, Clone)]
pub(super) struct SourceMemoryState {
  pub(super) materializations: BTreeMap<octacity_server_factory::FactoryDigest, crate::SourceMaterialization>,
  pub(super) preparations: BTreeMap<octacity_server_factory::FactoryDigest, crate::WorkSourcePreparation>,
  pub(super) scans: BTreeMap<(&'static str, uuid::Uuid), crate::WorkSourceScan>,
  pub(super) inputs: BTreeMap<(WorkSourceInputId, WorkSourceInputVersion), WorkSourceInput>,
  pub(super) groups: BTreeMap<(WorkSourceGroupId, WorkSourceGroupVersion), WorkSourceGroup>,
}
#[async_trait]
impl FactorySourceStore for InMemoryFactoryConfigurationStore {
  async fn source_issue_correlations(
    &self,
    project: ProjectId,
    issue: octacity_server_factory::FactoryDigest,
    after: Option<WorkSourceInputId>,
    limit: u16,
  ) -> Result<Vec<crate::WorkSourceIssueAssociation>, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let mut rows = self
      .lock()?
      .sources
      .preparations
      .values()
      .filter(|v| {
        v.input.project_id() == project && v.issue == issue && after.is_none_or(|cursor| v.input.id() > cursor)
      })
      .filter_map(|v| {
        v.admission.clone().map(|admission| crate::WorkSourceIssueAssociation {
          input_id: v.input.id(),
          input_version: v.input.version(),
          admission,
        })
      })
      .collect::<Vec<_>>();
    rows.sort_by_key(|v| v.input_id);
    rows.truncate(usize::from(limit));
    Ok(rows)
  }
  async fn admit_prepared_source_work(
    &self,
    guard: crate::AdmitPreparedWorkSource,
    request: crate::ManagementMutation<crate::AdmitFactoryWork>,
  ) -> Result<crate::FactoryAdmissionMutationOutcome, StoreError> {
    let mut state = self.lock()?;
    state
      .sources
      .require_current(guard.claim.project_id, guard.claim.target)?;
    crate::require_source_scan(
      state
        .sources
        .scans
        .get(&guard.claim.target.identity())
        .ok_or_else(source_conflict)?,
      &guard.claim,
      guard.now,
    )?;
    let mut preparation = state
      .sources
      .preparations
      .get(&guard.preparation_id)
      .cloned()
      .ok_or_else(source_conflict)?;
    preparation.require_selected_owner(
      &guard.claim,
      &state.sources.selected_input(&guard.claim, preparation.input.id())?,
    )?;
    preparation.validate_admission(&guard, request.mutation())?;
    if crate::factory_configuration_testing::replay_admission(&state, &request.mutation().probe)?.is_none()
      && let Some(manifest) = state.sources.materializations.get(&guard.preparation_id)
    {
      manifest.require_admission(request.mutation().work.artifacts(), guard.now)?;
    }
    let outcome = crate::factory_configuration_testing::admit_factory_work_locked(&mut state, request)?;
    preparation.correlate(&outcome.admission);
    state.sources.preparations.insert(preparation.id, preparation);
    Ok(outcome)
  }
  async fn pending_source_preparations(
    &self,
    project: ProjectId,
    target: crate::WorkSourceScanTarget,
    after: Option<octacity_server_factory::FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<crate::WorkSourcePreparation>, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let state = self.lock()?;
    Ok(
      state
        .sources
        .preparations
        .values()
        .filter(|v| {
          v.input.project_id() == project
            && v.target.identity() == target.identity()
            && v.admission.is_none()
            && !v.snapshot.is_empty()
            && !state
              .sources
              .materializations
              .get(&v.id)
              .is_some_and(|m| m.rejection.is_some() || m.cleaned_at.is_some())
            && after.is_none_or(|cursor| v.id > cursor)
        })
        .take(usize::from(limit))
        .cloned()
        .collect(),
    )
  }
  async fn publish_source_preparation_intent(
    &self,
    request: crate::PublishSourcePreparationIntent,
  ) -> Result<crate::WorkSourcePreparation, StoreError> {
    let mut state = self.lock()?;
    let sources = &mut state.sources;
    sources.require_current(request.claim.project_id, request.claim.target)?;
    crate::require_source_scan(
      sources
        .scans
        .get(&request.claim.target.identity())
        .ok_or_else(source_conflict)?,
      &request.claim,
      request.now,
    )?;
    let preparation = sources
      .preparations
      .get(&request.preparation_id)
      .ok_or_else(source_conflict)?;
    preparation.require_selected_owner(
      &request.claim,
      &sources.selected_input(&request.claim, preparation.input.id())?,
    )?;
    let retained = preparation.with_intent(request)?;
    sources.preparations.insert(retained.id, retained.clone());
    Ok(retained)
  }
  async fn reserve_source_preparation(
    &self,
    request: crate::ReserveWorkSourcePreparation,
  ) -> Result<crate::WorkSourcePreparation, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    let sources = &mut state.sources;
    sources.require_current(request.claim.project_id, request.claim.target)?;
    crate::require_source_scan(
      sources
        .scans
        .get(&request.claim.target.identity())
        .ok_or_else(source_conflict)?,
      &request.claim,
      request.now,
    )?;
    let input = sources.selected_input(&request.claim, request.input_id)?;
    let preparation = request.freeze(input)?;
    Ok(
      sources
        .preparations
        .entry(preparation.id)
        .or_insert(preparation)
        .clone(),
    )
  }
  async fn source_preparation(
    &self,
    project: ProjectId,
    id: octacity_server_factory::FactoryDigest,
  ) -> Result<crate::WorkSourcePreparation, StoreError> {
    self
      .lock()?
      .sources
      .preparations
      .get(&id)
      .filter(|v| v.input.project_id() == project)
      .cloned()
      .ok_or_else(source_conflict)
  }
  async fn claim_source_scan(
    &self,
    request: crate::ClaimWorkSourceScan,
  ) -> Result<Option<crate::WorkSourceScan>, StoreError> {
    let mut state = self.lock()?;
    state.sources.require_current(request.project_id, request.target)?;
    let key = request.target.identity();
    let next = crate::next_source_scan(request, state.sources.scans.get(&key))?;
    if let Some(scan) = &next {
      state.sources.scans.insert(key, scan.clone());
    }
    Ok(next)
  }
  async fn checkpoint_source_scan(
    &self,
    claim: crate::WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    expected: Option<crate::WorkSourceProgress>,
    progress: crate::WorkSourceProgress,
  ) -> Result<(), StoreError> {
    progress.validate()?;
    let mut state = self.lock()?;
    state.sources.require_current(claim.project_id, claim.target)?;
    if state.sources.preparations.values().any(|v| {
      v.input.project_id() == claim.project_id
        && v.target.identity() == claim.target.identity()
        && v.admission.is_none()
        && !v.snapshot.is_empty()
        && !state
          .sources
          .materializations
          .get(&v.id)
          .is_some_and(|m| m.rejection.is_some() || m.cleaned_at.is_some())
    }) {
      return Err(source_conflict());
    }
    let scan = state
      .sources
      .scans
      .get_mut(&claim.target.identity())
      .ok_or_else(source_conflict)?;
    crate::require_source_scan(scan, &claim, now)?;
    if scan.progress != expected {
      return Err(source_conflict());
    }
    scan.progress = Some(progress);
    Ok(())
  }
  async fn publish_work_sources(&self, request: PublishWorkSources) -> Result<(), StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    if !state.projects.contains(&request.project_id) {
      return Err(source_conflict());
    }
    let mut sources = state.sources.clone();
    for change in request.inputs {
      let current = sources
        .inputs
        .values()
        .filter(|v| v.id() == change.input.id())
        .max_by_key(|v| v.version());
      if current.map(|v| v.version()) != change.expected_version
        || current.is_some_and(|v| v.project_id() != request.project_id)
      {
        return Err(source_conflict());
      }
      let definition = change.input.definition();
      let target = &state
        .versions
        .get(&(definition.target.id(), definition.target.version()))
        .ok_or_else(source_conflict)?
        .configuration;
      let repository = state
        .repositories
        .get(&(definition.repository.id, definition.repository.version))
        .ok_or_else(source_conflict)?;
      validate_source_input_dependencies(&change.input, target, repository)?;
      sources
        .inputs
        .insert((change.input.id(), change.input.version()), change.input);
    }
    if let Some(change) = request.group {
      let current = sources
        .groups
        .values()
        .filter(|v| v.id() == change.group.id())
        .max_by_key(|v| v.version());
      if current.map(|v| v.version()) != change.expected_version
        || current.is_some_and(|v| v.project_id() != request.project_id)
      {
        return Err(source_conflict());
      }
      for member in change.group.members() {
        let current = sources
          .inputs
          .values()
          .filter(|v| v.id() == member.id())
          .max_by_key(|v| v.version());
        if current != Some(member) {
          return Err(source_conflict());
        }
      }
      sources
        .groups
        .insert((change.group.id(), change.group.version()), change.group);
    }
    state.sources = sources;
    Ok(())
  }
  async fn source_input(
    &self,
    project: ProjectId,
    id: WorkSourceInputId,
    version: WorkSourceInputVersion,
  ) -> Result<WorkSourceInput, StoreError> {
    self
      .lock()?
      .sources
      .inputs
      .get(&(id, version))
      .filter(|v| v.project_id() == project)
      .cloned()
      .ok_or_else(source_conflict)
  }
  async fn source_group(
    &self,
    project: ProjectId,
    id: WorkSourceGroupId,
    version: WorkSourceGroupVersion,
  ) -> Result<WorkSourceGroup, StoreError> {
    self
      .lock()?
      .sources
      .groups
      .get(&(id, version))
      .filter(|v| v.project_id() == project)
      .cloned()
      .ok_or_else(source_conflict)
  }
}

impl SourceMemoryState {
  pub(super) fn selected_input(
    &self,
    claim: &crate::WorkSourceScanClaim,
    id: WorkSourceInputId,
  ) -> Result<WorkSourceInput, StoreError> {
    match claim.target {
      crate::WorkSourceScanTarget::Input { id: selected, version } if selected == id => {
        self.inputs.get(&(id, version)).cloned().ok_or_else(source_conflict)
      }
      crate::WorkSourceScanTarget::Group { id: group_id, version } => self
        .groups
        .get(&(group_id, version))
        .and_then(|g| g.members().iter().find(|v| v.id() == id))
        .cloned()
        .ok_or_else(source_conflict),
      _ => Err(source_conflict()),
    }
  }

  pub(super) fn require_current(
    &self,
    project: ProjectId,
    target: crate::WorkSourceScanTarget,
  ) -> Result<(), StoreError> {
    match target {
      crate::WorkSourceScanTarget::Input { id, version } => {
        let input = self
          .inputs
          .values()
          .filter(|v| v.id() == id)
          .max_by_key(|v| v.version())
          .ok_or_else(source_conflict)?;
        if input.project_id() != project || input.version() != version || !input.definition().enabled {
          return Err(source_conflict());
        }
      }
      crate::WorkSourceScanTarget::Group { id, version } => {
        let group = self
          .groups
          .values()
          .filter(|v| v.id() == id)
          .max_by_key(|v| v.version())
          .ok_or_else(source_conflict)?;
        if group.project_id() != project || group.version() != version {
          return Err(source_conflict());
        }
        let mut enabled = false;
        for member in group.members() {
          let current = self
            .inputs
            .values()
            .filter(|v| v.id() == member.id())
            .max_by_key(|v| v.version());
          if current != Some(member) {
            return Err(source_conflict());
          }
          enabled |= member.definition().enabled;
        }
        if !enabled {
          return Err(source_conflict());
        }
      }
    }
    Ok(())
  }
}
