//! Deterministic task adapter sharing Project/workflow transactional authority.
use crate::*;
use octacity_server_domain::{EntityKind, IssueId, IssueVersion, ProjectId};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct IssueMemoryState {
  executions: execution::IssueExecutionMemoryState,
  property_versions: BTreeMap<(ProjectId, u64), PublishedIssueProperties>,
  property_publications: BTreeMap<ManagementIdempotencyKey, (PublishIssuePropertiesIntent, IssuePropertiesOutcome)>,
  planning_edits: BTreeMap<ManagementIdempotencyKey, (EditIssuePlanningIntent, IssueMutationOutcome)>,
  next_failure: Option<crate::testing::MutationFailurePoint>,
  current: BTreeMap<(ProjectId, IssueId), IssueRevision>,
  revisions: BTreeMap<(IssueId, IssueVersion), IssueRevision>,
  numbers: BTreeMap<ProjectId, u64>,
  pushes: BTreeMap<(IssueId, IssueVersion), IssuePushIntent>,
  policies: BTreeMap<IssueId, IssueFieldPolicy>,
  adoptions: BTreeMap<ManagementIdempotencyKey, (AdoptIssueWorkflowIntent, IssueTransitionOutcome)>,
  moves: BTreeMap<ManagementIdempotencyKey, IssueMoveReplay>,
  history: BTreeMap<(IssueId, IssueVersion), IssueStatusHistoryEntry>,
  edits: BTreeMap<ManagementIdempotencyKey, (EditIssueIntent, IssueMutationOutcome)>,
  creates: BTreeMap<ManagementIdempotencyKey, (CreateIssueIntent, IssueMutationOutcome)>,
}
// Accepted pipeline commands seal their original review before the atomic business move.
// A transient store failure leaves it pending; definitive business rejection stays historical.
#[derive(Clone)]
struct IssueMoveReplay {
  request: TransitionIssue,
  result: Option<Result<IssueTransitionOutcome, StoreError>>,
}
impl IssueMemoryState {
  pub(super) fn mutation_count(&self) -> usize {
    self.executions.mutation_count()
      + self.creates.len()
      + self.edits.len()
      + self.moves.values().filter(|r| matches!(r.result, Some(Ok(_)))).count()
      + self.adoptions.len()
      + self.property_publications.len()
      + self.planning_edits.len()
  }
}
#[async_trait::async_trait]
impl IssueStore for crate::testing::InMemoryProjectStore {
  async fn issue_status_command(
    &self,
    project_id: ProjectId,
    id: IssueId,
    scope: &ManagementSecurityScope,
    operation_id: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<RetainedIssueStatusCommand>, StoreError> {
    let key = ManagementIdempotencyKey::new(scope.clone(), issue_status_operation_key(operation_id));
    Ok(
      self
        .lock()?
        .issues
        .moves
        .get(&key)
        .filter(|r| r.request.intent.project_id == project_id && r.request.intent.id == id)
        .and_then(|r| r.request.retained_pipeline_command()),
    )
  }
  async fn issue_field_policy(&self, project_id: ProjectId, id: IssueId) -> Result<IssueFieldPolicy, StoreError> {
    let state = self.lock()?;
    if !state.issues.current.contains_key(&(project_id, id)) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    Ok(state.issues.policies.get(&id).cloned().unwrap_or_default())
  }

  async fn issue_revisions(
    &self,
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<IssueRevisionPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(crate::issue::invalid_issue(StoreOperation::ListIssueRevisions));
    }
    self.issue(project_id, id, None).await?;
    let state = self.lock()?;
    let mut items: Vec<_> = state
      .issues
      .revisions
      .values()
      .filter(|i| i.id == id && after.is_none_or(|v| i.version > v))
      .take(usize::from(limit) + 1)
      .map(IssueRevisionHeader::from)
      .collect();
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next_cursor = more.then(|| items.last().unwrap().version);
    Ok(IssueRevisionPage { items, next_cursor })
  }

  async fn list_issues(&self, request: ListProjectIssues) -> Result<IssuePage, StoreError> {
    let state = self.lock()?;
    if request
      .after()
      .is_some_and(|id| !state.issues.current.contains_key(&(request.project_id(), id)))
    {
      return Err(crate::issue::invalid_issue(StoreOperation::ListProjectIssues));
    }
    let mut items: Vec<_> = state
      .issues
      .current
      .values()
      .filter(|i| {
        i.project_id == request.project_id()
          && request.visibility().allows(&i.id)
          && request.after().is_none_or(|a| i.id > a)
      })
      .take(usize::from(request.limit()) + 1)
      .map(IssueSummary::from)
      .collect();
    let more = items.len() > usize::from(request.limit());
    if more {
      items.pop();
    }
    let next_cursor = more.then(|| items.last().unwrap().id);
    Ok(IssuePage { items, next_cursor })
  }

  async fn adopt_issue_workflow(
    &self,
    request: ManagementMutation<AdoptIssueWorkflow>,
  ) -> Result<IssueTransitionOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.issues.adoptions.get(&key) {
      if intent != &request.intent {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      return Ok(IssueTransitionOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    let intent = &request.intent;
    let current = state
      .issues
      .current
      .get(&(intent.project_id, intent.id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let project = state.projects.get(&intent.project_id).ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
    let source = state
      .workflows
      .workflow(project, current.workflow_id, Some(current.workflow_version))?;
    let target = state
      .workflows
      .workflow(project, intent.target_workflow_id, Some(intent.target_workflow_version))?;
    let outcome = current.adopted(
      &request,
      &source,
      &target,
      &state.issues.policies.get(&intent.id).cloned().unwrap_or_default(),
      audit.actor(),
    )?;
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::AdoptIssueWorkflow,
      &outcome.issue,
      Some(outcome.transition.clone()),
      |memory| {
        memory.adoptions.insert(key.clone(), (request.intent, outcome.clone()));
      },
    )?;
    Ok(outcome)
  }
  async fn issue(
    &self,
    project_id: ProjectId,
    id: IssueId,
    version: Option<IssueVersion>,
  ) -> Result<IssueRevision, StoreError> {
    let state = self.lock()?;
    let current = state
      .issues
      .current
      .get(&(project_id, id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    Ok(match version {
      None => current.clone(),
      Some(version) => state
        .issues
        .revisions
        .get(&(id, version))
        .cloned()
        .ok_or(StoreError::NotFound {
          entity: EntityKind::Issue,
        })?,
    })
  }
  async fn issue_status_history(
    &self,
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<IssueStatusHistoryPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(crate::issue::invalid_issue(StoreOperation::TransitionIssue));
    }
    let state = self.lock()?;
    state
      .issues
      .current
      .get(&(project_id, id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let mut items: Vec<_> = state
      .issues
      .history
      .values()
      .filter(|e| e.issue_id == id && after.is_none_or(|v| e.version > v))
      .take(usize::from(limit) + 1)
      .cloned()
      .collect();
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next_cursor = more.then(|| items.last().unwrap().version);
    Ok(IssueStatusHistoryPage { items, next_cursor })
  }
  async fn transition_issue(
    &self,
    request: ManagementMutation<TransitionIssue>,
  ) -> Result<IssueTransitionOutcome, StoreError> {
    let (mut request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(replay) = state.issues.moves.get(&key) {
      if replay.request.intent != request.intent || replay.request.pipeline_action() != request.pipeline_action() {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      if let Some(result) = &replay.result {
        return result.clone().map(|outcome| IssueTransitionOutcome {
          disposition: MutationDisposition::Replayed,
          ..outcome
        });
      }
      if request.pipeline_action().is_some() {
        request.moved_at = replay.request.moved_at;
      }
    } else if request.pipeline_action().is_some() {
      state.issues.moves.insert(
        key.clone(),
        IssueMoveReplay {
          request: request.clone(),
          result: None,
        },
      );
    }
    let evaluated = (|| {
      let intent = &request.intent;
      let current = state
        .issues
        .current
        .get(&(intent.project_id, intent.id))
        .ok_or(StoreError::NotFound {
          entity: EntityKind::Issue,
        })?;
      if let Some(action) = request.pipeline_action() {
        state.issues.executions.guard_action(action, current)?;
      }
      let project = state.projects.get(&intent.project_id).ok_or(StoreError::NotFound {
        entity: EntityKind::Project,
      })?;
      let workflow = state
        .workflows
        .workflow(project, current.workflow_id, Some(current.workflow_version))?;
      current.transitioned(
        &request,
        &workflow,
        &state.issues.policies.get(&intent.id).cloned().unwrap_or_default(),
        audit.actor(),
      )
    })();
    let outcome = match evaluated {
      Ok(outcome) => outcome,
      Err(error) => {
        if request.pipeline_action().is_some() && error != StoreError::Unavailable {
          state
            .issues
            .moves
            .get_mut(&key)
            .expect("sealed accepted command")
            .result = Some(Err(error.clone()));
        }
        return Err(error);
      }
    };
    let issue = outcome.issue.clone();
    let transition = outcome.transition.clone();
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::TransitionIssue,
      &issue,
      Some(transition),
      |memory| {
        if let Some(action) = request.pipeline_action() {
          memory.executions.advance_action(action, issue.applicability_version);
        }
        memory.moves.insert(
          key.clone(),
          IssueMoveReplay {
            request,
            result: Some(Ok(outcome.clone())),
          },
        );
      },
    )?;
    Ok(outcome)
  }
  async fn edit_issue(&self, request: ManagementMutation<EditIssue>) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.issues.edits.get(&key) {
      if intent != &request.intent {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      return Ok(IssueMutationOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    let intent = &request.intent;
    let current = state
      .issues
      .current
      .get(&(intent.project_id, intent.id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let issue = current.edited(
      &request,
      &state.issues.policies.get(&intent.id).cloned().unwrap_or_default(),
    )?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue: issue.clone(),
    };
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::EditIssue,
      &issue,
      None,
      |memory| {
        memory.edits.insert(key.clone(), (request.intent, outcome.clone()));
      },
    )?;
    Ok(outcome)
  }
  async fn create_issue(&self, request: ManagementMutation<CreateIssue>) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.issues.creates.get(&key) {
      if intent != &request.intent {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      return Ok(IssueMutationOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    planning::validate_parent(
      &state,
      request.intent.project_id,
      request.id,
      request.intent.planning.parent_id,
    )?;
    request.intent.content.validate()?;
    request.intent.planning.validate(
      &planning::definitions(&state, request.intent.project_id, None)?,
      None,
      &request.assignees,
    )?;
    let project = state
      .projects
      .get(&request.intent.project_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Project,
      })?;
    if request.created_at < project.created_at {
      return Err(crate::issue::invalid_issue(StoreOperation::CreateIssue));
    }
    let workflow = state.workflows.default(project)?.workflow;
    let number = state
      .issues
      .numbers
      .get(&project.id)
      .copied()
      .unwrap_or(0)
      .checked_add(1)
      .ok_or_else(|| crate::issue::invalid_issue(StoreOperation::CreateIssue))?;
    if state.issues.current.keys().any(|(_, id)| *id == request.id) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::Issue,
      });
    }
    let issue = IssueRevision {
      id: request.id,
      project_id: project.id,
      number,
      version: IssueVersion::INITIAL,
      applicability_version: IssueVersion::INITIAL,
      content: request.intent.content.clone(),
      planning: request.intent.planning.clone(),
      workflow_id: workflow.id,
      workflow_version: workflow.version,
      status_id: workflow.definition.initial_status_id,
      created_at: request.created_at,
      updated_at: request.created_at,
    };
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue: issue.clone(),
    };
    let initial = IssueStatusHistoryEntry {
      issue_id: issue.id,
      version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      from: None,
      to: issue.status_id,
      transition_id: None,
      previous_workflow: None,
      origin: IssueTransitionOrigin::Manual,
      pipeline: None,
      reason: None,
      actor: audit.actor().clone(),
      occurred_at: issue.created_at,
    };
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::CreateIssue,
      &issue,
      Some(initial),
      |memory| {
        memory.numbers.insert(issue.project_id, number);
        memory.creates.insert(key.clone(), (request.intent, outcome.clone()));
      },
    )?;
    Ok(outcome)
  }
}

impl crate::testing::InMemoryProjectStore {
  /// Models a separately authorized retained source association for task-command tests.
  /// No ordinary application edit/create/move accepts this policy or native credentials.
  pub fn seed_issue_field_policy(
    &self,
    project_id: ProjectId,
    id: IssueId,
    policy: IssueFieldPolicy,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    state
      .issues
      .current
      .get(&(project_id, id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    if policy
      .push
      .as_ref()
      .is_some_and(|push| push.fields.is_empty() || !push.fields.is_disjoint(&policy.source_owned))
    {
      return Err(crate::issue::invalid_issue(StoreOperation::TransitionIssue));
    }
    state.issues.policies.insert(id, policy);
    Ok(())
  }
}

impl crate::testing::InMemoryProjectStore {
  /// Observes typed retained projection evidence without dispatching to a provider.
  pub fn retained_issue_push_intents(&self) -> Result<Vec<IssuePushIntent>, StoreError> {
    Ok(self.lock()?.issues.pushes.values().cloned().collect())
  }
}

// One publication point keeps task revisions, history, replay and projection evidence atomic.
fn commit_issue_revision(
  state: &mut crate::project_testing::ProjectMemoryState,
  audit: &MutationAuditContext,
  key: &ManagementIdempotencyKey,
  operation: StoreOperation,
  issue: &IssueRevision,
  history: Option<IssueStatusHistoryEntry>,
  record_replay: impl FnOnce(&mut IssueMemoryState),
) -> Result<(), StoreError> {
  commit_issue_change(
    state,
    audit,
    key,
    operation,
    issue.id,
    |transaction| {
      let push = transaction
        .issues
        .current
        .get(&(issue.project_id, issue.id))
        .and_then(|previous| {
          transaction
            .issues
            .policies
            .get(&issue.id)
            .and_then(|policy| policy.push_intent(previous, issue))
        });
      transaction
        .issues
        .current
        .insert((issue.project_id, issue.id), issue.clone());
      transaction
        .issues
        .revisions
        .insert((issue.id, issue.version), issue.clone());
      if let Some(history) = history {
        transaction.issues.history.insert((issue.id, issue.version), history);
      }
      transaction.active_references.insert(issue.project_id);
      if let Some(push) = push.filter(|_| operation != StoreOperation::AdoptIssueWorkflow) {
        transaction.issues.pushes.insert((issue.id, issue.version), push);
      }
      Ok(())
    },
    |memory| {
      record_replay(memory);
    },
  )
}

// Shared atomic publication of domain state, replay and safe mutation evidence.
fn commit_issue_change(
  state: &mut crate::project_testing::ProjectMemoryState,
  audit: &MutationAuditContext,
  key: &ManagementIdempotencyKey,
  operation: StoreOperation,
  issue_id: IssueId,
  change: impl FnOnce(&mut crate::project_testing::ProjectMemoryState) -> Result<(), StoreError>,
  record_replay: impl FnOnce(&mut IssueMemoryState),
) -> Result<(), StoreError> {
  use crate::{project_testing::inject_failure, testing::MutationFailurePoint};
  let failure = state.issues.next_failure.take();
  let mut transaction = state.clone();
  change(&mut transaction)?;
  inject_failure(failure, MutationFailurePoint::DomainState)?;
  record_replay(&mut transaction.issues);
  inject_failure(failure, MutationFailurePoint::IdempotencyOutcome)?;
  transaction
    .audit_facts
    .insert(crate::testing::recorded_management_audit(
      audit,
      operation,
      EntityKind::Issue,
      issue_id,
    ));
  inject_failure(failure, MutationFailurePoint::AuditFact)?;
  transaction
    .outbox_entries
    .insert(crate::testing::management_evidence_identity(
      &format!("{operation:?}"),
      key,
    ));
  inject_failure(failure, MutationFailurePoint::OutboxEntry)?;
  inject_failure(failure, MutationFailurePoint::Commit)?;
  *state = transaction;
  Ok(())
}

impl crate::testing::InMemoryProjectStore {
  /// Injects a one-shot transactional failure without committing task/history/replay/projection state.
  pub fn fail_next_issue_mutation_at(&self, point: crate::testing::MutationFailurePoint) -> Result<(), StoreError> {
    self.lock()?.issues.next_failure = Some(point);
    Ok(())
  }
}

#[path = "issue_planning_testing.rs"]
mod planning;

#[path = "issue_execution_testing.rs"]
mod execution;
