//! Canonical review metadata shares the ordinary Issue transaction/replay publisher.
use super::*;
use octacity_server_factory::FactoryDigest;
#[derive(Clone, Default)]
pub(super) struct IssueCodeReviewMemoryState {
  rows: BTreeMap<(ManagementSecurityScope, IssueId, FactoryDigest), IssueCodeReviewAssociation>,
  observations: BTreeMap<(ManagementSecurityScope, FactoryDigest, u64), IssueCodeReviewObservationRecord>,
  observed_replays:
    BTreeMap<ManagementIdempotencyKey, (ObserveIssueCodeReviewIntent, IssueCodeReviewObservationOutcome)>,
  replays: BTreeMap<ManagementIdempotencyKey, (AttachIssueCodeReviewIntent, IssueCodeReviewOutcome)>,
}
impl IssueCodeReviewMemoryState {
  pub(super) fn mutation_count(&self) -> usize {
    self.replays.len() + self.observed_replays.len()
  }
}
#[async_trait::async_trait]
impl IssueCodeReviewStore for crate::testing::InMemoryProjectStore {
  async fn issue_code_review_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &AttachIssueCodeReviewIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueCodeReviewOutcome>, StoreError> {
    let state = self.lock()?;
    if !state.issues.current.contains_key(&(intent.project_id, intent.issue_id)) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    state
      .issues
      .code_reviews
      .replays
      .get(&ManagementIdempotencyKey::new(scope.clone(), key.clone()))
      .map(|(original, outcome)| {
        if original != intent {
          return Err(conflict());
        }
        Ok(IssueCodeReviewOutcome {
          disposition: MutationDisposition::Replayed,
          ..outcome.clone()
        })
      })
      .transpose()
  }
  async fn attach_issue_code_review(
    &self,
    request: ManagementMutation<AttachIssueCodeReview>,
  ) -> Result<IssueCodeReviewOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let mut state = self.lock()?;
    let issue = state
      .issues
      .current
      .get(&(intent.project_id, intent.issue_id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    if let Some((original, outcome)) = state.issues.code_reviews.replays.get(&key) {
      if original != intent {
        return Err(conflict());
      }
      return Ok(IssueCodeReviewOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    request.validate(issue)?;
    let id = intent.selection.identity(intent.project_id, intent.issue_id);
    let row_key = (audit.security_scope().clone(), intent.issue_id, id);
    let existing = state.issues.code_reviews.rows.get(&row_key);
    if existing.is_some_and(|row| row.binding != intent.binding) {
      return Err(conflict());
    }
    let sequence = state
      .issues
      .code_reviews
      .rows
      .iter()
      .filter(|((scope, issue, _), _)| scope == audit.security_scope() && *issue == intent.issue_id)
      .map(|(_, row)| row.sequence)
      .max()
      .unwrap_or(0)
      + 1;
    let association = existing.cloned().unwrap_or(IssueCodeReviewAssociation {
      id,
      sequence,
      project_id: intent.project_id,
      issue_id: intent.issue_id,
      selection: intent.selection.clone(),
      binding: intent.binding.clone(),
      provenance: request.provenance,
      attached_at: request.attached_at,
    });
    let outcome = IssueCodeReviewOutcome {
      disposition: if existing.is_some() {
        MutationDisposition::Replayed
      } else {
        MutationDisposition::Applied
      },
      association,
    };
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::AttachIssueCodeReview,
      intent.issue_id,
      |transaction| {
        let record = IssueCodeReviewObservationRecord::new(outcome.association.id, 1, request.observation)?;
        transaction
          .issues
          .code_reviews
          .observations
          .entry((audit.security_scope().clone(), outcome.association.id, 1))
          .or_insert(record);
        transaction
          .issues
          .code_reviews
          .rows
          .entry(row_key)
          .or_insert(outcome.association.clone());
        Ok(())
      },
      |memory| {
        memory
          .code_reviews
          .replays
          .insert(key.clone(), (intent.clone(), outcome.clone()));
      },
    )?;
    Ok(outcome)
  }
  async fn issue_code_review(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    id: FactoryDigest,
  ) -> Result<IssueCodeReviewAssociation, StoreError> {
    self.issue(project, issue, None).await?;
    self
      .lock()?
      .issues
      .code_reviews
      .rows
      .get(&(scope.clone(), issue, id))
      .filter(|row| row.project_id == project)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })
  }
  async fn issue_code_review_observation_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &ObserveIssueCodeReviewIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueCodeReviewObservationOutcome>, StoreError> {
    self
      .issue_code_review(intent.project_id, intent.issue_id, scope, intent.association_id)
      .await?;
    self
      .lock()?
      .issues
      .code_reviews
      .observed_replays
      .get(&ManagementIdempotencyKey::new(scope.clone(), key.clone()))
      .map(|(original, outcome)| {
        if original != intent {
          return Err(conflict());
        }
        Ok(IssueCodeReviewObservationOutcome {
          disposition: MutationDisposition::Replayed,
          ..outcome.clone()
        })
      })
      .transpose()
  }
  async fn observe_issue_code_review(
    &self,
    request: ManagementMutation<ObserveIssueCodeReview>,
  ) -> Result<IssueCodeReviewObservationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    self
      .issue_code_review(
        intent.project_id,
        intent.issue_id,
        audit.security_scope(),
        intent.association_id,
      )
      .await?;
    let mut state = self.lock()?;
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    if let Some((original, outcome)) = state.issues.code_reviews.observed_replays.get(&key) {
      if original != intent {
        return Err(conflict());
      }
      return Ok(IssueCodeReviewObservationOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    let row_key = (audit.security_scope().clone(), intent.issue_id, intent.association_id);
    let row = state.issues.code_reviews.rows.get(&row_key).ok_or_else(conflict)?;
    request.validate(row)?;
    let sequence = state
      .issues
      .code_reviews
      .observations
      .keys()
      .filter(|(scope, id, _)| scope == audit.security_scope() && *id == intent.association_id)
      .map(|(_, _, seq)| *seq)
      .max()
      .unwrap_or(0)
      + 1;
    let observation = IssueCodeReviewObservationRecord::new(intent.association_id, sequence, request.observation)?;
    let existing = state
      .issues
      .code_reviews
      .observations
      .iter()
      .filter(|((scope, _, _), _)| scope == audit.security_scope())
      .map(|(_, r)| r)
      .find(|r| r.id == observation.id);
    let outcome = IssueCodeReviewObservationOutcome {
      disposition: if existing.is_some() {
        MutationDisposition::Replayed
      } else {
        MutationDisposition::Applied
      },
      observation: existing.cloned().unwrap_or(observation),
    };
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::ObserveIssueCodeReview,
      intent.issue_id,
      |tx| {
        tx.issues
          .code_reviews
          .observations
          .entry((
            audit.security_scope().clone(),
            intent.association_id,
            outcome.observation.sequence,
          ))
          .or_insert(outcome.observation.clone());
        Ok(())
      },
      |memory| {
        memory
          .code_reviews
          .observed_replays
          .insert(key.clone(), (intent.clone(), outcome.clone()));
      },
    )?;
    Ok(outcome)
  }
  async fn issue_code_review_observations(
    &self,
    request: ReadIssueCodeReviews,
    association: FactoryDigest,
  ) -> Result<IssueCodeReviewStorePage<IssueCodeReviewObservationRecord>, StoreError> {
    request.validate()?;
    let selected = self
      .issue_code_review(request.project_id, request.issue_id, &request.scope, association)
      .await?;
    if !request.visibility.allows(&selected.selection.repository_identity()) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    let state = self.lock()?;
    let rows: Vec<_> = state
      .issues
      .code_reviews
      .observations
      .iter()
      .filter(|((scope, id, _), _)| scope == &request.scope && *id == association)
      .map(|(_, row)| row)
      .collect();
    let boundary = request.retained_boundary(rows.iter().map(|row| row.sequence).max().unwrap_or(0))?;
    let mut items: Vec<_> = rows
      .into_iter()
      .filter(|row| row.sequence <= boundary && request.after.is_none_or(|after| row.sequence > after))
      .take(usize::from(request.limit) + 1)
      .cloned()
      .collect();
    let more = items.len() > usize::from(request.limit);
    items.truncate(usize::from(request.limit));
    let next = more.then(|| items.last().unwrap().sequence);
    Ok(IssueCodeReviewStorePage { items, boundary, next })
  }
  async fn issue_code_reviews(&self, request: ReadIssueCodeReviews) -> Result<IssueCodeReviewStorePage, StoreError> {
    request.validate()?;
    self.issue(request.project_id, request.issue_id, None).await?;
    let state = self.lock()?;
    let rows: Vec<_> = state
      .issues
      .code_reviews
      .rows
      .iter()
      .filter(|((scope, issue, _), row)| {
        scope == &request.scope
          && *issue == request.issue_id
          && row.project_id == request.project_id
          && request.visibility.allows(&row.selection.repository_identity())
      })
      .map(|(_, row)| row)
      .collect();
    let boundary = request.retained_boundary(rows.iter().map(|row| row.sequence).max().unwrap_or(0))?;
    let mut items: Vec<_> = rows
      .into_iter()
      .filter(|row| row.sequence <= boundary && request.after.is_none_or(|after| row.sequence > after))
      .cloned()
      .collect();
    items.sort_by_key(|row| row.sequence);
    let more = items.len() > usize::from(request.limit);
    items.truncate(usize::from(request.limit));
    let next = more.then(|| items.last().unwrap().sequence);
    let items = items
      .into_iter()
      .map(|association| {
        let observation = state
          .issues
          .code_reviews
          .observations
          .iter()
          .filter(|((scope, id, _), _)| scope == &request.scope && *id == association.id)
          .map(|(_, record)| record)
          .max_by_key(|record| (record.observation.observed_at, record.sequence))
          .map(|record| record.observation.clone());
        IssueCodeReviewRow {
          association,
          observation,
        }
      })
      .collect();
    Ok(IssueCodeReviewStorePage { items, boundary, next })
  }
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
