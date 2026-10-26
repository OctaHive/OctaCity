//! Uses the same Project lock and atomic mutation publisher as manual Issues.
use super::*;
#[path = "issue_source_mapping_testing.rs"]
mod mapping;
#[derive(Clone, Default)]
pub(super) struct IssueSourceMemoryState {
  writers: BTreeMap<(IssueId, octacity_server_domain::IssueSourceMappingVersion), IssueSourceWriterAccess>,
  pub(super) mappings:
    BTreeMap<(IssueId, octacity_server_domain::IssueSourceMappingVersion), PublishedIssueSourceMapping>,
  mapping_publications:
    BTreeMap<ManagementIdempotencyKey, (PublishIssueSourceMappingIntent, IssueSourceMappingOutcome)>,
  pulls: BTreeMap<ManagementIdempotencyKey, SourcePullReplay>,
  scans: BTreeMap<octacity_server_factory::FactoryDigest, IssueSourceReadScan>,
  refreshes: BTreeMap<(bool, ManagementIdempotencyKey), SourceRefreshReplay>,
  pub(super) states: BTreeMap<IssueId, IssueSourceRefreshState>,
  pub(super) observations: BTreeMap<(IssueId, octacity_server_factory::FactoryDigest), IssueSourceObservation>,
  pub(super) links: BTreeMap<IssueId, (ManagementSecurityScope, IssueSourceLink)>,
  native: BTreeMap<octacity_server_factory::FactoryDigest, IssueId>,
  attaches: BTreeMap<ManagementIdempotencyKey, (AttachIssueSourceIntent, IssueSourceLink)>,
  imports: BTreeMap<ManagementIdempotencyKey, (IssueSourceSelection, IssueSourceLink)>,
}
#[derive(Clone)]
struct SourceRefreshReplay {
  project_id: ProjectId,
  issue_id: IssueId,
  explicit_read: Option<octacity_server_factory::FactoryDigest>,
  intent: serde_json::Value,
  outcome: IssueSourceRefreshOutcome,
}
#[derive(Clone)]
struct SourcePullReplay {
  intent: (ProjectId, IssueId, u64, bool),
  outcome: IssueSourcePullOutcome,
}
impl IssueSourceMemoryState {
  pub(super) fn desired_push(
    &self,
    push: &IssuePushIntent,
    issue: &IssueRevision,
    audit: &MutationAuditContext,
    key: &IdempotencyKey,
    history: Option<&IssueStatusHistoryEntry>,
  ) -> Result<Option<IssueSourceDesiredState>, StoreError> {
    let Some((_, link)) = self
      .links
      .get(&issue.id)
      .filter(|(scope, _)| scope == audit.security_scope())
    else {
      return Ok(None);
    };
    let Some(mapping) = self
      .mappings
      .values()
      .find(|m| m.issue_id == issue.id && m.reference() == push.mapping)
    else {
      return Ok(None);
    };
    let mut desired = push.desired_state(issue, link, mapping, audit, key, history)?;
    desired.writer_access = self.writers.get(&(issue.id, mapping.version)).cloned();
    Ok(Some(desired))
  }

  fn update_read(
    &mut self,
    scope: &ManagementSecurityScope,
    claim: &IssueSourceReadClaim,
    at: octacity_server_domain::Timestamp,
    change: impl FnOnce(&mut IssueSourceReadScan) -> Result<(), StoreError>,
  ) -> Result<(), StoreError> {
    self.require_read_target(claim.project_id, scope, &claim.target)?;
    let key = claim.target.identity(claim.project_id, scope)?;
    let scan = self.scans.get_mut(&key).ok_or_else(crate::source_conflict)?;
    require_source_scan(scan, claim, at)?;
    change(scan)
  }
  fn read_generation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    target: &IssueSourceReadTarget,
  ) -> Result<Option<u64>, StoreError> {
    let Some(issue) = target.issue_id else {
      return Ok(Some(0));
    };
    let (_, link) = self
      .links
      .get(&issue)
      .filter(|(owner, link)| owner == scope && link.selection.project_id == project)
      .ok_or_else(crate::source_conflict)?;
    if link.selection.source.installation.identity() != target.source.installation.identity() {
      return Err(crate::source_conflict());
    }
    let state = self.states.get(&issue).ok_or(StoreError::Unavailable)?;
    Ok(state.enabled.then_some(state.generation))
  }
  fn require_read_target(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    target: &IssueSourceReadTarget,
  ) -> Result<(), StoreError> {
    if self.read_generation(project, scope, target)? != Some(target.pull_generation) {
      return Err(crate::source_conflict());
    }
    Ok(())
  }
  fn require_read_claim(
    &self,
    scope: &ManagementSecurityScope,
    claim: &IssueSourceReadClaim,
    selection: &IssueSourceSelection,
    issue: Option<IssueId>,
    at: octacity_server_domain::Timestamp,
  ) -> Result<(), StoreError> {
    if claim.project_id != selection.project_id
      || claim.target.source != selection.source
      || claim.target.issue_id.is_some_and(|id| Some(id) != issue)
    {
      return Err(crate::source_conflict());
    }
    self.require_read_target(claim.project_id, scope, &claim.target)?;
    let key = claim.target.identity(claim.project_id, scope)?;
    require_source_scan(self.scans.get(&key).ok_or_else(crate::source_conflict)?, claim, at)
  }
  pub(super) fn mutation_count(&self) -> usize {
    self.attaches.len() + self.imports.len() + self.refreshes.len() + self.pulls.len() + self.mapping_publications.len()
  }
}
#[async_trait::async_trait]
impl IssueSourceStore for crate::testing::InMemoryProjectStore {
  async fn native_issue_source_link(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    native: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<IssueSourceLink>, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains_key(&project) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    Ok(
      state
        .issues
        .sources
        .native
        .get(&native)
        .and_then(|id| state.issues.sources.links.get(id))
        .filter(|(owner, link)| owner == scope && link.selection.project_id == project)
        .map(|(_, link)| link.clone()),
    )
  }

  async fn issue_source_attachment_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &AttachIssueSourceIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceOutcome>, StoreError> {
    self
      .lock()?
      .issues
      .sources
      .attaches
      .get(&ManagementIdempotencyKey::new(scope.clone(), key.clone()))
      .map(|(original, link)| {
        if original != intent {
          return Err(crate::issue_source_mapping::mapping_conflict());
        }
        Ok(IssueSourceOutcome::Linked {
          link: link.clone(),
          disposition: MutationDisposition::Replayed,
        })
      })
      .transpose()
  }

  async fn issue_source_observation(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    id: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<IssueSourceObservation>, StoreError> {
    if self.issue_source_link(project, issue, scope).await?.is_none() {
      return Ok(None);
    }
    Ok(self.lock()?.issues.sources.observations.get(&(issue, id)).cloned())
  }
  async fn import_issue_source(
    &self,
    request: ManagementMutation<ImportIssueSource>,
  ) -> Result<IssueSourceOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((original, link)) = state.issues.sources.imports.get(&key) {
      if !request.visibility.allows(&link.issue_id) {
        return Err(StoreError::NotFound {
          entity: EntityKind::Issue,
        });
      }
      if original != &request.selection {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      return Ok(IssueSourceOutcome::Linked {
        link: link.clone(),
        disposition: MutationDisposition::Replayed,
      });
    }
    if let Some(claim) = &request.read_claim {
      state.issues.sources.require_read_claim(
        audit.security_scope(),
        claim,
        &request.selection,
        None,
        request.imported_at,
      )?;
    }
    let native = request.selection.native_correlation(audit.security_scope());
    request.observation.validate(
      &request.selection,
      request.imported_at,
      StoreOperation::ImportIssueSource,
    )?;
    let observation_id = request.observation.id(native)?;
    if let Some(id) = state.issues.sources.native.get(&native) {
      if !request.visibility.allows(id) {
        return Err(StoreError::NotFound {
          entity: EntityKind::Issue,
        });
      }
      let link = state
        .issues
        .sources
        .links
        .get(id)
        .expect("atomic native index")
        .1
        .clone();
      commit_issue_change(
        &mut state,
        &audit,
        &key,
        StoreOperation::ImportIssueSource,
        link.issue_id,
        |_| Ok(()),
        |memory| {
          memory
            .sources
            .imports
            .insert(key.clone(), (request.selection, link.clone()));
          memory
            .sources
            .states
            .entry(link.issue_id)
            .or_insert(IssueSourceRefreshState::new(request.observation.clone()));
          memory
            .sources
            .observations
            .insert((link.issue_id, observation_id), request.observation);
        },
      )?;
      return Ok(IssueSourceOutcome::Linked {
        link,
        disposition: MutationDisposition::Replayed,
      });
    }
    if !request.visibility.allows(&request.id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    let create = CreateIssue {
      id: request.id,
      intent: CreateIssueIntent {
        project_id: request.selection.project_id,
        content: request.content,
        planning: Default::default(),
      },
      assignees: Default::default(),
      idempotency_key: request.idempotency_key,
      created_at: request.imported_at,
    };
    let (issue, mut initial) = prepare_issue_creation(&state, &create, &audit)?;
    initial.origin = IssueTransitionOrigin::Source;
    let link = IssueSourceLink {
      display_id: request.observation.display_id.clone(),
      issue_id: issue.id,
      selection: request.selection.clone(),
      native_correlation: native,
      linked_at: request.imported_at,
    };
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::ImportIssueSource,
      &issue,
      Some(initial),
      |memory| {
        memory.numbers.insert(issue.project_id, issue.number);
        memory
          .sources
          .states
          .insert(issue.id, IssueSourceRefreshState::new(request.observation.clone()));
        memory
          .sources
          .observations
          .insert((issue.id, observation_id), request.observation);
        memory.sources.native.insert(native, issue.id);
        memory
          .sources
          .links
          .insert(issue.id, (audit.security_scope().clone(), link.clone()));
        memory
          .sources
          .imports
          .insert(key.clone(), (request.selection, link.clone()));
      },
    )?;
    Ok(IssueSourceOutcome::Linked {
      link,
      disposition: MutationDisposition::Applied,
    })
  }
  async fn issue_source_link(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
  ) -> Result<Option<IssueSourceLink>, StoreError> {
    let state = self.lock()?;
    if !state.issues.current.contains_key(&(project, issue)) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    Ok(
      state
        .issues
        .sources
        .links
        .get(&issue)
        .filter(|(owner, link)| owner == scope && link.selection.project_id == project)
        .map(|(_, link)| link.clone()),
    )
  }
  async fn attach_issue_source(
    &self,
    request: ManagementMutation<AttachIssueSource>,
  ) -> Result<IssueSourceOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    let intent = &request.intent;
    if let Some((original, link)) = state.issues.sources.attaches.get(&key) {
      if original != intent {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      return Ok(IssueSourceOutcome::Linked {
        link: link.clone(),
        disposition: MutationDisposition::Replayed,
      });
    }
    let issue = state
      .issues
      .current
      .get(&(intent.selection.project_id, intent.issue_id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    request.observation.validate(
      &intent.selection,
      request.attached_at,
      StoreOperation::AttachIssueSource,
    )?;
    if issue.version != intent.expected_version
      || state.issues.policies.get(&issue.id).cloned().unwrap_or_default() != intent.expected_field_policy
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      });
    }
    if request.attached_at < issue.updated_at {
      return Err(crate::issue::invalid_issue(StoreOperation::AttachIssueSource));
    }
    let native = intent.selection.native_correlation(audit.security_scope());
    let observation_id = request.observation.id(native)?;
    if let Some(id) = state.issues.sources.native.get(&native) {
      return Ok(IssueSourceOutcome::Conflict {
        canonical: state.issues.sources.links.get(id).map(|(_, link)| link.clone()),
      });
    }
    if state.issues.sources.links.contains_key(&issue.id) {
      return Ok(IssueSourceOutcome::Conflict { canonical: None });
    }
    let link = IssueSourceLink {
      display_id: request.observation.display_id.clone(),
      issue_id: issue.id,
      selection: intent.selection.clone(),
      native_correlation: native,
      linked_at: request.attached_at,
    };
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::AttachIssueSource,
      link.issue_id,
      |transaction| {
        transaction.issues.sources.native.insert(native, link.issue_id);
        transaction
          .issues
          .sources
          .links
          .insert(link.issue_id, (audit.security_scope().clone(), link.clone()));
        Ok(())
      },
      |memory| {
        memory
          .sources
          .states
          .insert(link.issue_id, IssueSourceRefreshState::new(request.observation.clone()));
        memory
          .sources
          .observations
          .insert((link.issue_id, observation_id), request.observation.clone());
        memory
          .sources
          .attaches
          .insert(key.clone(), (request.intent, link.clone()));
      },
    )?;
    Ok(IssueSourceOutcome::Linked {
      link,
      disposition: MutationDisposition::Applied,
    })
  }
}

#[async_trait::async_trait]
impl IssueSourceRefreshStore for crate::testing::InMemoryProjectStore {
  async fn issue_source_refresh_replay(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    explicit_read: octacity_server_factory::FactoryDigest,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceRefreshOutcome>, StoreError> {
    let state = self.lock()?;
    if !state
      .issues
      .sources
      .links
      .get(&issue)
      .is_some_and(|(s, l)| s == scope && l.selection.project_id == project)
    {
      return Err(crate::source_conflict());
    }
    state
      .issues
      .sources
      .refreshes
      .get(&(true, ManagementIdempotencyKey::new(scope.clone(), key.clone())))
      .map(|r| {
        if r.project_id != project || r.issue_id != issue || r.explicit_read != Some(explicit_read) {
          return Err(crate::source_conflict());
        }
        Ok(r.outcome.clone())
      })
      .transpose()
  }
  async fn issue_source_refresh_state(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
  ) -> Result<Option<IssueSourceRefreshState>, StoreError> {
    if self.issue_source_link(project, issue, scope).await?.is_none() {
      return Ok(None);
    }
    Ok(self.lock()?.issues.sources.states.get(&issue).cloned())
  }
  async fn set_issue_source_pull(
    &self,
    request: ManagementMutation<SetIssueSourcePull>,
  ) -> Result<IssueSourcePullOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    let intent = (
      request.project_id,
      request.issue_id,
      request.expected_generation,
      request.enabled,
    );
    if let Some(replay) = state.issues.sources.pulls.get(&key) {
      if replay.intent != intent {
        return Err(crate::source_conflict());
      }
      return Ok(replay.outcome.clone());
    }
    let link = state
      .issues
      .sources
      .links
      .get(&request.issue_id)
      .filter(|(scope, link)| scope == audit.security_scope() && link.selection.project_id == request.project_id)
      .ok_or_else(crate::source_conflict)?;
    let mut control = state
      .issues
      .sources
      .states
      .get(&link.1.issue_id)
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    if control.generation != request.expected_generation {
      return Err(crate::source_conflict());
    }
    control.generation = control.generation.checked_add(1).ok_or_else(crate::source_conflict)?;
    control.enabled = request.enabled;
    let outcome = IssueSourcePullOutcome {
      enabled: control.enabled,
      generation: control.generation,
    };
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::SetIssueSourcePull,
      request.issue_id,
      |_| Ok(()),
      |memory| {
        memory.sources.states.insert(request.issue_id, control);
        memory.sources.pulls.insert(
          key.clone(),
          SourcePullReplay {
            intent,
            outcome: outcome.clone(),
          },
        );
      },
    )?;
    Ok(outcome)
  }

  async fn refresh_issue_source(
    &self,
    request: ManagementMutation<RefreshIssueSource>,
  ) -> Result<IssueSourceRefreshOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let replay_key = (request.explicit_read.is_some(), key.clone());
    let intent = serde_json::to_value(request.replay_intent()).map_err(|_| StoreError::Unavailable)?;
    let mut state = self.lock()?;
    if let Some(replay) = state.issues.sources.refreshes.get(&replay_key) {
      if replay.intent != intent {
        return Err(crate::source_conflict());
      }
      return Ok(replay.outcome.clone());
    }
    if let Some(claim) = &request.read_claim {
      state.issues.sources.require_read_claim(
        audit.security_scope(),
        claim,
        &request.selection,
        Some(request.issue_id),
        request.refreshed_at,
      )?;
    }
    let link = state
      .issues
      .sources
      .links
      .get(&request.issue_id)
      .filter(|(scope, link)| {
        scope == audit.security_scope() && link.native_correlation == request.selection.native_correlation(scope)
      })
      .map(|(_, link)| link.clone())
      .ok_or_else(crate::source_conflict)?;
    request.observation.validate(
      &request.selection,
      request.refreshed_at,
      StoreOperation::RefreshIssueSource,
    )?;
    request.content.validate()?;
    let mut refresh = state
      .issues
      .sources
      .states
      .get(&link.issue_id)
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    let current = state
      .issues
      .current
      .get(&(link.selection.project_id, link.issue_id))
      .ok_or_else(crate::source_conflict)?;
    let mapping = state
      .issues
      .sources
      .mappings
      .values()
      .rev()
      .find(|m| m.issue_id == current.id);
    let project = state
      .projects
      .get(&current.project_id)
      .ok_or_else(crate::source_conflict)?;
    let workflow = state
      .workflows
      .workflow(project, current.workflow_id, Some(current.workflow_version))?;
    let plan = refresh.plan(
      current,
      &request,
      &state.issues.policies.get(&link.issue_id).cloned().unwrap_or_default(),
      mapping.map(|mapping| IssueSourceMappingContext {
        mapping,
        workflow: &workflow,
        native_correlation: link.native_correlation,
      }),
      audit.actor(),
    )?;
    let outcome = plan.outcome;
    let id = request.observation.id(link.native_correlation)?;
    let record = |memory: &mut IssueMemoryState| {
      memory.sources.states.insert(link.issue_id, refresh);
      memory
        .sources
        .observations
        .insert((link.issue_id, id), request.observation.clone());
      memory.sources.refreshes.insert(
        replay_key,
        SourceRefreshReplay {
          project_id: request.selection.project_id,
          issue_id: request.issue_id,
          explicit_read: request.explicit_read,
          intent,
          outcome: outcome.clone(),
        },
      );
    };
    if let Some(next) = plan.revision {
      commit_issue_revision(
        &mut state,
        &audit,
        &key,
        StoreOperation::RefreshIssueSource,
        &next,
        plan.transition,
        record,
      )?;
    } else {
      commit_issue_change(
        &mut state,
        &audit,
        &key,
        StoreOperation::RefreshIssueSource,
        link.issue_id,
        |_| Ok(()),
        record,
      )?;
    }
    Ok(outcome)
  }
}

#[async_trait::async_trait]
impl IssueSourceReadStore for crate::testing::InMemoryProjectStore {
  async fn claim_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    mut request: ClaimWorkSourceScan<IssueSourceReadTarget>,
  ) -> Result<Option<IssueSourceReadScan>, StoreError> {
    let mut state = self.lock()?;
    if !state.projects.contains_key(&request.project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    let Some(generation) = state
      .issues
      .sources
      .read_generation(request.project_id, scope, &request.target)?
    else {
      return Ok(None);
    };
    request.target.pull_generation = generation;
    let key = request.target.identity(request.project_id, scope)?;
    let scan = next_source_scan(request, state.issues.sources.scans.get(&key))?;
    if let Some(scan) = &scan {
      state.issues.sources.scans.insert(key, scan.clone());
    }
    Ok(scan)
  }
  async fn checkpoint_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    claim: IssueSourceReadClaim,
    at: octacity_server_domain::Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), StoreError> {
    self.lock()?.issues.sources.update_read(scope, &claim, at, |scan| {
      progress.validate()?;
      if scan.progress != expected {
        return Err(crate::source_conflict());
      }
      scan.progress = Some(progress);
      Ok(())
    })
  }
  async fn finish_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    claim: IssueSourceReadClaim,
    at: octacity_server_domain::Timestamp,
    schedule: WorkSourceSchedule,
  ) -> Result<(), StoreError> {
    self.lock()?.issues.sources.update_read(scope, &claim, at, |scan| {
      scan.schedule = Some(schedule);
      scan.claim.until = at;
      Ok(())
    })
  }
}
