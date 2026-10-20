//! Discussion commits under the existing Issue/Project transaction and replay boundary.
use super::*;
use octacity_server_domain::IssueCommentId;
use octacity_server_factory::FactoryDigest;
type Fingerprint = (ProjectId, IssueId, Option<IssueCommentId>, FactoryDigest);
#[derive(Clone, Default)]
pub(super) struct IssueDiscussionMemoryState {
  comments: BTreeMap<(IssueId, u64), (IssueCommentMetadata, IssueCommentBody)>,
  lifetime: BTreeMap<IssueId, u64>,
  replays: BTreeMap<ManagementIdempotencyKey, (Fingerprint, IssueCommentMetadata)>,
}
impl IssueDiscussionMemoryState {
  pub(super) fn mutation_count(&self) -> usize {
    self.replays.len()
  }
}
#[async_trait::async_trait]
impl IssueDiscussionStore for crate::testing::InMemoryProjectStore {
  async fn issue_comments(
    &self,
    project: ProjectId,
    issue: IssueId,
    after: Option<u64>,
    limit: u16,
  ) -> Result<IssueCommentPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(crate::issue_discussion::invalid_issue_discussion());
    }
    let state = self.lock()?;
    state
      .issues
      .current
      .get(&(project, issue))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let mut items: Vec<_> = state
      .issues
      .discussion
      .comments
      .values()
      .filter(|(m, _)| m.issue_id == issue && after.is_none_or(|s| m.sequence > s))
      .take(usize::from(limit) + 1)
      .map(|(m, _)| m.clone())
      .collect();
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next_sequence = more.then(|| items.last().unwrap().sequence);
    Ok(IssueCommentPage { items, next_sequence })
  }
  async fn append_issue_comment(
    &self,
    request: ManagementMutation<AppendIssueComment>,
  ) -> Result<IssueCommentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let mut state = self.lock()?;
    state
      .issues
      .current
      .get(&(intent.project_id, intent.issue_id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let digest = FactoryDigest::content_sha256(intent.body.as_str().as_bytes());
    let fingerprint = (intent.project_id, intent.issue_id, intent.parent_id, digest);
    if let Some((original, comment)) = state.issues.discussion.replays.get(&key) {
      if *original != fingerprint {
        return Err(StoreError::Conflict {
          entity: EntityKind::Issue,
        });
      }
      return Ok(IssueCommentOutcome {
        disposition: MutationDisposition::Replayed,
        comment: comment.clone(),
      });
    }
    if intent.body.as_str().len() > request.limits.body_bytes() {
      return Err(crate::issue_discussion::invalid_issue_discussion());
    }
    let previous = state
      .issues
      .discussion
      .lifetime
      .get(&intent.issue_id)
      .copied()
      .unwrap_or(0);
    if previous >= u64::from(request.limits.lifetime_comments()) {
      return Err(StoreError::invalid(
        StoreOperation::AppendIssueComment,
        StoreInputError::IssueDiscussionQuotaReached,
      ));
    }
    if let Some(parent) = intent.parent_id
      && !state
        .issues
        .discussion
        .comments
        .values()
        .any(|(m, _)| m.issue_id == intent.issue_id && m.id == parent && m.parent_id.is_none())
    {
      return Err(crate::issue_discussion::invalid_issue_discussion());
    }
    let sequence = previous + 1;
    let comment = IssueCommentMetadata {
      id: request.id,
      project_id: intent.project_id,
      issue_id: intent.issue_id,
      sequence,
      parent_id: intent.parent_id,
      actor: audit.actor().clone(),
      created_at: request.created_at,
      body_digest: digest,
      body_bytes: intent.body.as_str().len() as u32,
    };
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::AppendIssueComment,
      intent.issue_id,
      |transaction| {
        transaction
          .issues
          .discussion
          .comments
          .insert((intent.issue_id, sequence), (comment.clone(), intent.body.clone()));
        transaction.issues.discussion.lifetime.insert(intent.issue_id, sequence);
        Ok(())
      },
      |memory| {
        memory
          .discussion
          .replays
          .insert(key.clone(), (fingerprint, comment.clone()));
      },
    )?;
    Ok(IssueCommentOutcome {
      disposition: MutationDisposition::Applied,
      comment,
    })
  }
  async fn issue_comment_body(
    &self,
    project: ProjectId,
    issue: IssueId,
    id: IssueCommentId,
  ) -> Result<IssueCommentContent, StoreError> {
    let state = self.lock()?;
    state
      .issues
      .current
      .get(&(project, issue))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    state
      .issues
      .discussion
      .comments
      .values()
      .find(|(m, _)| m.issue_id == issue && m.id == id)
      .map(|(comment, body)| IssueCommentContent {
        comment: comment.clone(),
        body: body.clone(),
      })
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })
  }
}
