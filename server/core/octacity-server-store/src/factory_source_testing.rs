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
  pub(super) mutations: BTreeMap<
    crate::ManagementIdempotencyKey,
    (
      crate::WorkSourceConfigurationIntent,
      crate::WorkSourceConfigurationOutcome,
    ),
  >,
  pub(super) materializations: BTreeMap<octacity_server_factory::FactoryDigest, crate::SourceMaterialization>,
  pub(super) preparations: BTreeMap<octacity_server_factory::FactoryDigest, crate::WorkSourcePreparation>,
  pub(super) scans: BTreeMap<(&'static str, uuid::Uuid), crate::WorkSourceScan>,
  pub(super) inputs: BTreeMap<(WorkSourceInputId, WorkSourceInputVersion), WorkSourceInput>,
  pub(super) groups: BTreeMap<(WorkSourceGroupId, WorkSourceGroupVersion), WorkSourceGroup>,
  pub(super) projection_sequences: BTreeMap<(ProjectId, octacity_server_factory::FactoryDigest), u64>,
  pub(super) projection_intents: BTreeMap<
    (
      ProjectId,
      octacity_server_factory::FactoryDigest,
      octacity_server_factory::FactoryDigest,
    ),
    crate::SourceProjectionOrder,
  >,
  pub(super) projections: BTreeMap<(ProjectId, octacity_server_factory::FactoryDigest), crate::SourceProjectionJournal>,
}
#[async_trait]
impl FactorySourceStore for InMemoryFactoryConfigurationStore {
  async fn coordinate_source_projection(
    &self,
    request: crate::SourceProjectionRequest,
  ) -> Result<crate::SourceProjectionState, StoreError> {
    let mut state = self.lock()?;
    let preparation = state
      .sources
      .preparations
      .values()
      .find(|p| {
        p.input.project_id() == request.project_id && p.admission.as_ref().is_some_and(|a| a.work_id == request.work_id)
      })
      .ok_or_else(source_conflict)?;
    let issue = preparation.projection_item();
    let association = preparation.admission.as_ref().ok_or_else(source_conflict)?;
    let reporter = preparation
      .input
      .definition()
      .reporter
      .as_ref()
      .ok_or_else(source_conflict)?;
    let snapshots = vec![
      state
        .factory_runs
        .get(&association.run_id)
        .ok_or_else(source_conflict)?
        .snapshot()?,
    ];
    let candidates = crate::source_projection_candidates(&snapshots)?;
    if !candidates
      .iter()
      .any(|c| c.operation_id == request.operation_id && c.capability == reporter.capability)
    {
      return Err(source_conflict());
    }
    let key = (request.project_id, issue, request.operation_id);
    let mut order = state
      .sources
      .projection_intents
      .get(&key)
      .cloned()
      .ok_or_else(source_conflict)?;
    let newer_unknown = state
      .sources
      .projection_intents
      .iter()
      .any(|((project, item, _), other)| {
        *project == request.project_id && *item == issue && other.sequence > order.sequence && other.fields.is_none()
      });
    let mut journal = state
      .sources
      .projections
      .get(&(request.project_id, issue))
      .cloned()
      .unwrap_or_default();
    let result = journal.coordinate(&request, &candidates, &mut order, newer_unknown)?;
    state.sources.projection_intents.insert(key, order);
    state.sources.projections.insert((request.project_id, issue), journal);
    Ok(result)
  }
  async fn source_preparation_for_work(
    &self,
    project: ProjectId,
    work: octacity_server_factory::WorkEnvelopeId,
  ) -> Result<Option<crate::WorkSourcePreparation>, StoreError> {
    let state = self.lock()?;
    let mut matching = state
      .sources
      .preparations
      .values()
      .filter(|p| p.input.project_id() == project && p.admission.as_ref().is_some_and(|a| a.work_id == work));
    let result = matching.next().cloned();
    if matching.next().is_some() {
      return Err(source_conflict());
    }
    Ok(result)
  }
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
    state.sources.require_current(project, target)?;
    Ok(
      state
        .sources
        .pending_preparations(project, target)
        .filter(|v| after.is_none_or(|cursor| v.id > cursor))
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
  async fn source_scan_members(
    &self,
    claim: crate::WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
  ) -> Result<Vec<WorkSourceInput>, StoreError> {
    let state = self.lock()?;
    state.sources.require_current(claim.project_id, claim.target)?;
    crate::require_source_scan(
      state
        .sources
        .scans
        .get(&claim.target.identity())
        .ok_or_else(source_conflict)?,
      &claim,
      now,
    )?;
    match claim.target {
      crate::WorkSourceScanTarget::Input { id, .. } => Ok(vec![state.sources.selected_input(&claim, id)?]),
      crate::WorkSourceScanTarget::Group { id, version } => Ok(
        state
          .sources
          .groups
          .get(&(id, version))
          .ok_or_else(source_conflict)?
          .members()
          .to_vec(),
      ),
    }
  }
  async fn select_source_input(
    &self,
    claim: crate::WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    item: crate::WorkSourceItem,
    matched: Vec<WorkSourceInputId>,
  ) -> Result<octacity_server_factory::WorkSourceSelection, StoreError> {
    let mut state = self.lock()?;
    state.sources.require_current(claim.project_id, claim.target)?;
    let members = match claim.target {
      crate::WorkSourceScanTarget::Input { id, .. } => vec![state.sources.selected_input(&claim, id)?],
      crate::WorkSourceScanTarget::Group { id, version } => state
        .sources
        .groups
        .get(&(id, version))
        .ok_or_else(source_conflict)?
        .members()
        .to_vec(),
    };
    let scan = state
      .sources
      .scans
      .get_mut(&claim.target.identity())
      .ok_or_else(source_conflict)?;
    crate::record_source_selection(scan, &claim, now, members, item, matched)
  }
  async fn finish_source_scan(
    &self,
    claim: crate::WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    schedule: crate::WorkSourceSchedule,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    state.sources.require_current(claim.project_id, claim.target)?;
    let scan = state
      .sources
      .scans
      .get_mut(&claim.target.identity())
      .ok_or_else(source_conflict)?;
    crate::require_source_scan(scan, &claim, now)?;
    scan.schedule = Some(schedule);
    scan.claim.until = now;
    Ok(())
  }
  async fn source_scan_schedule(
    &self,
    project: ProjectId,
    target: crate::WorkSourceScanTarget,
  ) -> Result<Option<crate::WorkSourceSchedule>, StoreError> {
    Ok(
      self
        .lock()?
        .sources
        .scans
        .get(&target.identity())
        .filter(|s| s.claim.project_id == project && s.claim.target == target)
        .and_then(|s| s.schedule.clone()),
    )
  }
  async fn source_scan_targets(
    &self,
    project: ProjectId,
    after: Option<crate::WorkSourceScanTarget>,
    limit: u16,
  ) -> Result<Vec<crate::WorkSourceScanTarget>, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let state = self.lock()?;
    let sources = &state.sources;
    let mut inputs = BTreeMap::new();
    let mut groups = BTreeMap::new();
    for input in sources.inputs.values().filter(|v| v.project_id() == project) {
      inputs.insert(input.id(), input);
    }
    for group in sources.groups.values().filter(|v| v.project_id() == project) {
      groups.insert(group.id(), group);
    }
    let mut targets = Vec::new();
    for input in inputs.values() {
      if input.definition().enabled
        && !groups
          .values()
          .any(|g| g.members().iter().any(|m| m.id() == input.id()))
      {
        targets.push(crate::WorkSourceScanTarget::Input {
          id: input.id(),
          version: input.version(),
        });
      }
    }
    for group in groups.values() {
      let target = crate::WorkSourceScanTarget::Group {
        id: group.id(),
        version: group.version(),
      };
      if sources.require_current(project, target).is_ok() {
        targets.push(target);
      }
    }
    targets.sort_by_key(|v| v.identity());
    Ok(
      targets
        .into_iter()
        .filter(|v| after.is_none_or(|a| v.identity() > a.identity()))
        .take(usize::from(limit))
        .collect(),
    )
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
    if state
      .sources
      .pending_preparations(claim.project_id, claim.target)
      .next()
      .is_some()
    {
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
  async fn expire_source_poll_pages(
    &self,
    now: octacity_server_domain::Timestamp,
    limit: u16,
  ) -> Result<usize, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let mut state = self.lock()?;
    let mut count = 0;
    for scan in state.sources.scans.values_mut() {
      if count == usize::from(limit) {
        break;
      }
      if scan
        .progress
        .as_ref()
        .and_then(|p| p.polling.as_ref())
        .and_then(|p| p.pending_page.as_ref())
        .is_some_and(|p| p.expires_at <= now)
      {
        scan.progress = None;
        count += 1;
      }
    }
    Ok(count)
  }
  async fn publish_work_sources(&self, request: PublishWorkSources) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    state.sources = resolved_publication(&state, request)?;
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
  fn pending_preparations(
    &self,
    project: ProjectId,
    target: crate::WorkSourceScanTarget,
  ) -> impl Iterator<Item = &crate::WorkSourcePreparation> {
    self.preparations.values().filter(move |preparation| {
      let member = match target {
        crate::WorkSourceScanTarget::Input { id, version } if id == preparation.input.id() => {
          self.inputs.get(&(id, version))
        }
        crate::WorkSourceScanTarget::Group { id, version } => self
          .groups
          .get(&(id, version))
          .and_then(|group| group.members().iter().find(|m| m.id() == preparation.input.id())),
        _ => None,
      };
      preparation.input.project_id() == project
        && member.is_some_and(|m| m.definition().enabled)
        && preparation.admission.is_none()
        && !preparation.snapshot.is_empty()
        && !self
          .materializations
          .get(&preparation.id)
          .is_some_and(|m| m.rejection.is_some() || m.cleaned_at.is_some())
    })
  }

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
        let mut groups = BTreeMap::new();
        for group in self.groups.values().filter(|g| g.project_id() == project) {
          groups.insert(group.id(), group);
        }
        if groups.values().any(|g| g.members().iter().any(|m| m.id() == id)) {
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

/// Called inside the same memory transaction that publishes accepted Flow/outbox data.
pub(super) fn record_projection_intents(
  state: &mut crate::factory_configuration_testing::FactoryConfigurationMemoryState,
  next: &crate::factory_run_testing::StoredFactoryRun,
  request: &crate::CommitFactoryRunTransition,
) -> Result<(), StoreError> {
  if !request
    .outbox
    .iter()
    .any(|row| row.kind.as_str() == "work.status.projection")
  {
    return Ok(());
  }
  let Some(preparation) = state
    .sources
    .preparations
    .values()
    .find(|p| p.admission.as_ref().is_some_and(|a| a.run_id == request.run_id))
  else {
    return Ok(());
  };
  let Some(reporter) = preparation.input.definition().reporter.as_ref() else {
    return Ok(());
  };
  let key = (preparation.input.project_id(), preparation.projection_item());
  let candidates = crate::source_projection_candidates(&[next.snapshot()?])?;
  let mut sequence = state.sources.projection_sequences.get(&key).copied().unwrap_or(0);
  let mut staged = Vec::new();
  for row in &request.outbox {
    if !candidates
      .iter()
      .any(|c| c.operation_id == row.operation_id && c.capability == reporter.capability)
    {
      continue;
    }
    let id = (key.0, key.1, row.operation_id);
    if state.sources.projection_intents.contains_key(&id) {
      continue;
    }
    sequence = sequence
      .checked_add(1)
      .filter(|v| *v <= i64::MAX as u64)
      .ok_or_else(source_conflict)?;
    staged.push((id, crate::SourceProjectionOrder { sequence, fields: None }));
  }
  state.sources.projection_sequences.insert(key, sequence);
  state.sources.projection_intents.extend(staged);
  Ok(())
}

// Prepare the complete next source state before committing any part of it.
fn resolved_publication(
  state: &crate::factory_configuration_testing::FactoryConfigurationMemoryState,
  request: PublishWorkSources,
) -> Result<SourceMemoryState, StoreError> {
  request.validate()?;
  if !state.projects.contains(&request.project_id) {
    return Err(source_conflict());
  }
  let mut sources = state.sources.clone();
  for change in request.inputs {
    state.integrations.validate_source_input(&change.input)?;
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
      state.integrations.validate_source_input(member)?;
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
  Ok(sources)
}
#[async_trait]
impl crate::FactorySourceConfigurationStore for InMemoryFactoryConfigurationStore {
  async fn list_work_source_configurations(
    &self,
    request: crate::ListWorkSourceConfigurations,
  ) -> Result<crate::WorkSourceConfigurationPage, StoreError> {
    request.validate()?;
    let state = self.lock()?;
    if !state.projects.contains(&request.project_id) {
      return Err(source_conflict());
    }
    let mut inputs = BTreeMap::new();
    for input in state
      .sources
      .inputs
      .values()
      .filter(|i| i.project_id() == request.project_id)
    {
      inputs.insert(input.id(), input);
    }
    let mut groups = BTreeMap::new();
    for group in state
      .sources
      .groups
      .values()
      .filter(|g| g.project_id() == request.project_id)
    {
      groups.insert(group.id(), group);
    }
    let mut items = inputs
      .into_values()
      .map(|i| crate::WorkSourceConfigurationRecord::Input(Box::new(i.clone())))
      .chain(
        groups
          .into_values()
          .map(|g| crate::WorkSourceConfigurationRecord::Group(g.clone())),
      )
      .filter(|r| r.inputs().iter().all(|i| request.visibility.allows(i)))
      .filter(|r| {
        request
          .after
          .is_none_or(|after| r.target().identity() > after.identity())
      })
      .collect::<Vec<_>>();
    items.sort_by_key(|r| r.target().identity());
    let more = items.len() > usize::from(request.limit);
    items.truncate(usize::from(request.limit));
    let next_cursor = more.then(|| items.last().map(|r| r.target())).flatten();
    Ok(crate::WorkSourceConfigurationPage { items, next_cursor })
  }
  async fn replay_work_source_configuration(
    &self,
    request: crate::ManagementMutation<crate::ReplayWorkSourceConfiguration>,
  ) -> Result<Option<crate::WorkSourceConfigurationOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let state = self.lock()?;
    replay_configuration(
      &state.sources,
      &audit.scoped_idempotency_key(&request.idempotency_key),
      &request.intent,
    )
  }
  async fn publish_work_source_configuration(
    &self,
    request: crate::ManagementMutation<crate::PublishWorkSourceConfiguration>,
  ) -> Result<crate::WorkSourceConfigurationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    request.validate()?;
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(prior) = replay_configuration(&state.sources, &key, &request.intent)? {
      return Ok(prior);
    }
    let mut sources = resolved_publication(&state, request.publication.clone())?;
    let outcome = request.outcome();
    sources.mutations.insert(key.clone(), (request.intent, outcome.clone()));
    state.sources = sources;
    state.evidence.insert(format!(
      "publish-work-source-configuration:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(crate::testing::recorded_management_audit(
      &audit,
      crate::StoreOperation::PublishWorkSourceConfiguration,
      octacity_server_domain::EntityKind::Project,
      outcome.publication.project_id,
    ));
    Ok(outcome)
  }
}
fn replay_configuration(
  sources: &SourceMemoryState,
  key: &crate::ManagementIdempotencyKey,
  intent: &crate::WorkSourceConfigurationIntent,
) -> Result<Option<crate::WorkSourceConfigurationOutcome>, StoreError> {
  let Some((original, outcome)) = sources.mutations.get(key) else {
    return Ok(None);
  };
  if original != intent {
    return Err(source_conflict());
  }
  let mut replay = outcome.clone();
  replay.disposition = crate::MutationDisposition::Replayed;
  Ok(Some(replay))
}
#[async_trait]
impl crate::FactorySourceDiagnosticsStore for InMemoryFactoryConfigurationStore {
  async fn source_reporting_diagnostics(
    &self,
    project: ProjectId,
    association: &crate::WorkSourceDiagnosticAssociation,
    limit: u16,
  ) -> Result<crate::WorkSourceReportingPage, StoreError> {
    if !(1..=crate::MAX_WORK_SOURCE_REPORTING_OPERATIONS).contains(&limit) {
      return Err(source_conflict());
    }
    let state = self.lock()?;
    let preparation = state
      .sources
      .preparations
      .get(&association.id)
      .filter(|p| {
        p.input.project_id() == project
          && p.input.id() == association.input_id
          && p.input.version() == association.input_version
          && p
            .admission
            .as_ref()
            .is_some_and(|a| a.work_id == association.work_id && a.run_id == association.run_id)
      })
      .ok_or_else(source_conflict)?;
    // This registry is written with accepted Flow/outbox publication after checking the original Reporter.
    let issue = preparation.projection_item();
    let accepted = state
      .sources
      .projection_intents
      .keys()
      .filter(|(p, i, _)| *p == project && *i == issue)
      .map(|(_, _, operation)| *operation)
      .collect();
    let run = state
      .factory_runs
      .get(&association.run_id)
      .ok_or_else(source_conflict)?;
    Ok(run.source_reporting_diagnostics(&accepted, limit))
  }
  async fn source_scan_diagnostics(
    &self,
    project: ProjectId,
    target: crate::WorkSourceScanTarget,
  ) -> Result<Option<crate::WorkSourceScanDiagnostics>, StoreError> {
    Ok(
      self
        .lock()?
        .sources
        .scans
        .get(&target.identity())
        .filter(|s| s.claim.project_id == project && s.claim.target == target)
        .cloned()
        .map(crate::WorkSourceScanDiagnostics::from_scan),
    )
  }
  async fn source_diagnostic_correlations(
    &self,
    request: crate::ReadWorkSourceCorrelations,
  ) -> Result<crate::WorkSourceDiagnosticPage, StoreError> {
    request.validate()?;
    let state = self.lock()?;
    let mut items = vec![];
    for p in state.sources.preparations.values() {
      let Some(admission) = &p.admission else { continue };
      if p.input.project_id() != request.project_id
        || request.after.is_some_and(|after| p.id <= after)
        || !request.visibility.allows(&p.input)
        || !request.runs.allows(&admission.run_id)
      {
        continue;
      }
      let partition = crate::factory_configuration_testing::source_admission_partition(&state, admission.work_id)
        .ok_or(StoreError::Unavailable)?;
      let matches = match &request.filter {
        crate::WorkSourceDiagnosticFilter::Inputs(ids) => ids.contains(&p.input.id()),
        crate::WorkSourceDiagnosticFilter::Native { issues, security_scope } => {
          issues.contains(&p.issue) && partition == *security_scope
        }
      };
      if !matches {
        continue;
      }
      items.push(crate::WorkSourceDiagnosticAssociation {
        id: p.id,
        native_correlation: crate::source_native_correlation(p.issue, partition),
        input_id: p.input.id(),
        input_version: p.input.version(),
        work_id: admission.work_id,
        run_id: admission.run_id,
      });
      if items.len() > usize::from(request.limit) {
        break;
      }
    }
    let next_cursor = (items.len() > usize::from(request.limit)).then(|| items[usize::from(request.limit) - 1].id);
    items.truncate(usize::from(request.limit));
    Ok(crate::WorkSourceDiagnosticPage { items, next_cursor })
  }
}
