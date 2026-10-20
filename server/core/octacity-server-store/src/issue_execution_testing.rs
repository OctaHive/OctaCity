//! In-memory reservation shares the Issue revision lock and mutation evidence publisher.
use super::*;
#[derive(Clone, Default)]
pub(super) struct IssueExecutionMemoryState {
  associations: BTreeMap<octacity_server_factory::WorkEnvelopeId, IssueWorkAssociation>,
  pub(super) requests:
    BTreeMap<octacity_server_domain::IssueExecutionRequestId, (ManagementSecurityScope, u64, IssueExecutionRequest)>,
  replay: BTreeMap<ManagementIdempotencyKey, IssueExecutionOutcome>,
  markers: BTreeMap<octacity_server_factory::WorkEnvelopeId, octacity_server_domain::IssueVersion>,
}
impl IssueExecutionMemoryState {
  pub(super) fn guard_action(
    &self,
    action: &AcceptedIssueStatusAction,
    issue: &IssueRevision,
  ) -> Result<(), StoreError> {
    let p = action.provenance();
    let association = self.associations.get(&p.work_id).ok_or_else(conflict)?;
    if association.project_id != issue.project_id
      || association.issue_id != issue.id
      || p.issue_id != issue.id
      || association.run_id != p.run_id
      || association.requirements_version != Some(p.requirements_version)
      || self.markers.get(&p.work_id) != Some(&issue.applicability_version)
    {
      return Err(conflict());
    }
    Ok(())
  }
  pub(super) fn advance_action(
    &mut self,
    action: &AcceptedIssueStatusAction,
    version: octacity_server_domain::IssueVersion,
  ) {
    self.markers.insert(action.provenance().work_id, version);
  }
  pub(super) fn mutation_count(&self) -> usize {
    self.replay.len() + self.associations.len()
  }
  fn replay(
    &self,
    request: &ManagementMutation<RequestIssueExecution>,
  ) -> Result<Option<IssueExecutionOutcome>, StoreError> {
    let key = request
      .audit()
      .scoped_idempotency_key(&request.mutation().idempotency_key);
    self
      .replay
      .get(&key)
      .map(|original| {
        if original.request.intent != request.mutation().intent {
          return Err(conflict());
        }
        Ok(IssueExecutionOutcome {
          disposition: MutationDisposition::Replayed,
          ..original.clone()
        })
      })
      .transpose()
  }
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
#[async_trait::async_trait]
impl IssueExecutionStore for crate::testing::InMemoryProjectStore {
  async fn issue_execution_for_admission(
    &self,
    project_id: ProjectId,
    issue_id: IssueId,
    request_id: octacity_server_domain::IssueExecutionRequestId,
    scope: &ManagementSecurityScope,
  ) -> Result<IssueExecutionRequest, StoreError> {
    self
      .lock()?
      .issues
      .executions
      .requests
      .get(&request_id)
      .filter(|(owner, _, r)| owner == scope && r.intent.project_id == project_id && r.intent.issue_id == issue_id)
      .map(|(_, _, r)| r.clone())
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })
  }
  async fn issue_work_association(
    &self,
    project_id: ProjectId,
    work_id: octacity_server_factory::WorkEnvelopeId,
  ) -> Result<Option<IssueWorkAssociation>, StoreError> {
    Ok(
      self
        .lock()?
        .issues
        .executions
        .associations
        .get(&work_id)
        .filter(|a| a.project_id == project_id)
        .cloned(),
    )
  }
  async fn associate_issue_work(
    &self,
    request: ManagementMutation<AssociateIssueWork>,
  ) -> Result<IssueWorkAssociationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let mut state = self.lock()?;
    if !state
      .issues
      .current
      .contains_key(&(request.project_id, request.issue_id))
    {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    let reviewed = request
      .request_id
      .map(|id| {
        state
          .issues
          .executions
          .requests
          .get(&id)
          .map(|(_, _, r)| r)
          .filter(|r| r.intent.project_id == request.project_id && r.intent.issue_id == request.issue_id)
          .ok_or_else(conflict)
      })
      .transpose()?;
    let association = request.association(reviewed)?;
    if let Some(previous) = state.issues.executions.associations.get(&association.work_id) {
      if previous != &association {
        return Err(conflict());
      }
      return Ok(IssueWorkAssociationOutcome {
        disposition: MutationDisposition::Replayed,
        association: previous.clone(),
      });
    }
    let marker = association
      .requirements_version
      .and_then(|version| state.issues.revisions.get(&(association.issue_id, version)))
      .map(|revision| revision.applicability_version);
    let key = audit.scoped_idempotency_key(
      &IdempotencyKey::new(format!("issue-work.{}", association.work_id)).expect("bounded Work identity"),
    );
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::AssociateIssueWork,
      request.issue_id,
      |transaction| {
        transaction
          .issues
          .executions
          .associations
          .insert(association.work_id, association.clone());
        if let Some(marker) = marker {
          transaction
            .issues
            .executions
            .markers
            .insert(association.work_id, marker);
        }
        if let Some(id) = association.request_id {
          transaction
            .issues
            .executions
            .requests
            .get_mut(&id)
            .ok_or_else(conflict)?
            .2
            .association = Some(association.clone());
        }
        Ok(())
      },
      |_| {},
    )?;
    Ok(IssueWorkAssociationOutcome {
      disposition: MutationDisposition::Applied,
      association,
    })
  }

  async fn replay_issue_execution(
    &self,
    request: &ManagementMutation<RequestIssueExecution>,
  ) -> Result<Option<IssueExecutionOutcome>, StoreError> {
    self.lock()?.issues.executions.replay(request)
  }
  async fn request_issue_execution(
    &self,
    request: ManagementMutation<RequestIssueExecution>,
  ) -> Result<IssueExecutionOutcome, StoreError> {
    let mut state = self.lock()?;
    if let Some(replay) = state.issues.executions.replay(&request)? {
      return Ok(replay);
    }
    let (request, audit) = request.into_parts();
    let i = &request.intent;
    let issue = state
      .issues
      .current
      .get(&(i.project_id, i.issue_id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let outcome = request.reserve(issue)?;
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::RequestIssueExecution,
      i.issue_id,
      |transaction| {
        let sequence = transaction
          .issues
          .executions
          .requests
          .values()
          .filter(|(_, _, r)| r.intent.project_id == i.project_id && r.intent.issue_id == i.issue_id)
          .count() as u64
          + 1;
        transaction.issues.executions.requests.insert(
          request.id,
          (audit.security_scope().clone(), sequence, outcome.request.clone()),
        );
        Ok(())
      },
      |memory| {
        memory.executions.replay.insert(key.clone(), outcome.clone());
      },
    )?;
    Ok(outcome)
  }
  async fn issue_executions(
    &self,
    project_id: ProjectId,
    issue_id: IssueId,
    after: Option<IssueExecutionPosition>,
    observation: Option<IssueExecutionObservation>,
    limit: u16,
  ) -> Result<IssueExecutionReadPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(StoreError::invalid(
        StoreOperation::ListIssueExecutions,
        StoreInputError::InvalidIssueExecutionPage,
      ));
    }
    let state = self.lock()?;
    let issue = state
      .issues
      .current
      .get(&(project_id, issue_id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let requests = &state.issues.executions.requests;
    let maximum = requests
      .values()
      .filter(|(_, _, r)| r.intent.project_id == project_id && r.intent.issue_id == issue_id)
      .map(|(_, sequence, _)| *sequence)
      .max()
      .unwrap_or(0);
    let observation = observation.unwrap_or(IssueExecutionObservation {
      watermark: maximum,
      issue_version: issue.version,
    });
    if observation.watermark > maximum || observation.issue_version > issue.version {
      return Err(StoreError::invalid(
        StoreOperation::ListIssueExecutions,
        StoreInputError::InvalidIssueExecutionPage,
      ));
    }
    let mut items: Vec<_> = requests
      .values()
      .filter(|(_, sequence, r)| {
        r.intent.project_id == project_id
          && r.intent.issue_id == issue_id
          && *sequence <= observation.watermark
          && after.is_none_or(|p| r.position() > p)
      })
      .map(|(_, _, r)| r.clone())
      .collect();
    items.sort_by_key(IssueExecutionRequest::position);
    items.truncate(usize::from(limit) + 1);
    IssueExecutionReadPage::bounded(items, observation, limit)
  }
  async fn issue_execution(
    &self,
    project_id: ProjectId,
    issue_id: IssueId,
    request_id: octacity_server_domain::IssueExecutionRequestId,
  ) -> Result<IssueExecutionRequest, StoreError> {
    self
      .lock()?
      .issues
      .executions
      .requests
      .get(&request_id)
      .map(|(_, _, r)| r)
      .filter(|r| r.intent.project_id == project_id && r.intent.issue_id == issue_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })
  }
}
