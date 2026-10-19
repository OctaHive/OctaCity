//! Shared pure task policy: adapters retain transactions, not competing rule sets.
use crate::*;
use octacity_server_domain::EntityKind;

impl IssueRevision {
  /// Validates an exact local edit and returns its immutable next revision.
  pub fn edited(&self, request: &EditIssue, policy: &IssueFieldPolicy) -> Result<Self, StoreError> {
    let intent = &request.intent;
    if self.id != intent.id
      || self.project_id != intent.project_id
      || self.version != intent.expected_version
      || !intent
        .content
        .changed_fields(&self.content)
        .is_disjoint(&policy.source_owned)
    {
      return Err(conflict());
    }
    intent.content.validate()?;
    if request.edited_at < self.updated_at {
      return Err(crate::issue::invalid_issue(StoreOperation::EditIssue));
    }
    Ok(Self {
      version: self
        .version
        .next()
        .map_err(|_| crate::issue::invalid_issue(StoreOperation::EditIssue))?,
      applicability_version: if intent
        .content
        .changed_fields(&self.content)
        .iter()
        .any(|f| matches!(f, IssueField::Title | IssueField::Description | IssueField::Acceptance))
      {
        self
          .version
          .next()
          .map_err(|_| crate::issue::invalid_issue(StoreOperation::EditIssue))?
      } else {
        self.applicability_version
      },
      content: intent.content.clone(),
      updated_at: request.edited_at,
      ..self.clone()
    })
  }
  /// Evaluates one explicit manual edge; caller owns authorization and atomic publication.
  pub fn transitioned(
    &self,
    request: &TransitionIssue,
    workflow: &PublishedIssueWorkflow,
    policy: &IssueFieldPolicy,
    actor: &AuditActor,
  ) -> Result<IssueTransitionOutcome, StoreError> {
    let intent = &request.intent;
    if self.id != intent.id
      || self.project_id != intent.project_id
      || self.version != intent.expected_version
      || self.status_id != intent.expected_status_id
      || self.workflow_id != intent.workflow_id
      || self.workflow_version != intent.workflow_version
      || workflow.project_id != self.project_id
      || workflow.id != self.workflow_id
      || workflow.version != self.workflow_version
    {
      return Err(conflict());
    }
    if request.moved_at < self.updated_at
      || intent.reason.as_ref().is_some_and(|s| {
        s.trim().is_empty() || s.trim() != s || s.len() > MAX_ISSUE_TITLE_BYTES || s.chars().any(char::is_control)
      })
    {
      return Err(crate::issue::invalid_issue(StoreOperation::TransitionIssue));
    }
    if request.pipeline_action().is_some_and(|a| !a.matches(intent)) {
      return Err(conflict());
    }
    let origin = if request.pipeline_action().is_some() {
      IssueTransitionOrigin::Pipeline
    } else {
      IssueTransitionOrigin::Manual
    };
    let edge = workflow
      .definition
      .transitions
      .iter()
      .find(|e| {
        e.id == intent.transition_id
          && e.from == self.status_id
          && e
            .move_block_reason(policy, origin, |requirement| {
              request
                .pipeline_action()
                .is_some_and(|action| action.satisfies(requirement, request.moved_at))
            })
            .is_none()
      })
      .ok_or_else(conflict)?;
    let issue = Self {
      version: self
        .version
        .next()
        .map_err(|_| crate::issue::invalid_issue(StoreOperation::TransitionIssue))?,
      applicability_version: self
        .version
        .next()
        .map_err(|_| crate::issue::invalid_issue(StoreOperation::TransitionIssue))?,
      status_id: edge.to,
      updated_at: request.moved_at,
      ..self.clone()
    };
    let transition = IssueStatusHistoryEntry {
      issue_id: issue.id,
      version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      from: Some(self.status_id),
      to: issue.status_id,
      transition_id: Some(edge.id),
      previous_workflow: None,
      origin,
      pipeline: request.pipeline_action().map(|a| a.provenance().clone()),
      reason: intent.reason.clone(),
      actor: actor.clone(),
      occurred_at: request.moved_at,
    };
    Ok(IssueTransitionOutcome {
      disposition: MutationDisposition::Applied,
      issue,
      transition,
    })
  }
}
impl IssueFieldPolicy {
  /// Retains only changed authorized push fields under the original immutable mapping.
  pub fn push_intent(&self, previous: &IssueRevision, issue: &IssueRevision) -> Option<IssuePushIntent> {
    let push = self.push.as_ref()?;
    let mut changed = issue.content.changed_fields(&previous.content);
    if issue.status_id != previous.status_id {
      changed.insert(IssueField::Status);
    }
    let fields: std::collections::BTreeSet<_> = changed.intersection(&push.fields).copied().collect();
    (!fields.is_empty()).then(|| IssuePushIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      version: issue.version,
      mapping: push.mapping.clone(),
      fields,
    })
  }
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}

impl IssueRevision {
  /// Validates explicit reviewed adoption; no transition path, projection or execution is inferred.
  pub fn adopted(
    &self,
    request: &AdoptIssueWorkflow,
    source: &PublishedIssueWorkflow,
    target: &PublishedIssueWorkflow,
    policy: &IssueFieldPolicy,
    actor: &AuditActor,
  ) -> Result<IssueTransitionOutcome, StoreError> {
    let intent = &request.intent;
    if self.id != intent.id
      || self.project_id != intent.project_id
      || self.version != intent.expected_version
      || self.workflow_id != intent.expected_workflow_id
      || self.workflow_version != intent.expected_workflow_version
      || self.status_id != intent.expected_status_id
      || source.project_id != self.project_id
      || source.id != self.workflow_id
      || source.version != self.workflow_version
      || target.project_id != self.project_id
      || target.id != intent.target_workflow_id
      || target.version != intent.target_workflow_version
      || (target.id == self.workflow_id && target.version == self.workflow_version)
      || policy != &intent.expected_field_policy
      || policy.source_owned.contains(&IssueField::Status)
    {
      return Err(conflict());
    }
    let pin = IssueWorkflowPin {
      id: target.id,
      version: target.version,
    };
    if (policy.status_mapping_workflow.is_some()
      || policy
        .push
        .as_ref()
        .is_some_and(|p| p.fields.contains(&IssueField::Status)))
      && policy.status_mapping_workflow.as_ref() != Some(&pin)
    {
      return Err(conflict());
    }
    if request.adopted_at < self.updated_at
      || request.adopted_at < target.published_at
      || intent.status_correspondence.is_empty()
      || intent.status_correspondence.len() > MAX_ISSUE_WORKFLOW_STATUSES
      || intent.status_correspondence.iter().any(|(from, to)| {
        !source.definition.statuses.iter().any(|s| s.id == *from)
          || !target.definition.statuses.iter().any(|s| s.id == *to)
      })
    {
      return Err(crate::issue::invalid_issue(StoreOperation::AdoptIssueWorkflow));
    }
    let status_id = *intent
      .status_correspondence
      .get(&self.status_id)
      .ok_or_else(|| crate::issue::invalid_issue(StoreOperation::AdoptIssueWorkflow))?;
    let version = self
      .version
      .next()
      .map_err(|_| crate::issue::invalid_issue(StoreOperation::AdoptIssueWorkflow))?;
    let issue = Self {
      version,
      applicability_version: version,
      workflow_id: target.id,
      workflow_version: target.version,
      status_id,
      updated_at: request.adopted_at,
      ..self.clone()
    };
    let transition = IssueStatusHistoryEntry {
      issue_id: self.id,
      version,
      workflow_id: target.id,
      workflow_version: target.version,
      from: Some(self.status_id),
      to: status_id,
      transition_id: None,
      previous_workflow: Some(IssueWorkflowPin {
        id: self.workflow_id,
        version: self.workflow_version,
      }),
      origin: IssueTransitionOrigin::Manual,
      pipeline: None,
      reason: None,
      actor: actor.clone(),
      occurred_at: request.adopted_at,
    };
    Ok(IssueTransitionOutcome {
      disposition: MutationDisposition::Applied,
      issue,
      transition,
    })
  }
}

impl IssueWorkflowTransition {
  /// Shared manual-edge eligibility for observation and authoritative mutation.
  /// Identity/from/version checks and permission authorization remain with their callers.
  pub fn manual_move_block_reason(&self, policy: &IssueFieldPolicy) -> Option<IssueMoveBlockReason> {
    self.move_block_reason(policy, IssueTransitionOrigin::Manual, |_| false)
  }

  fn move_block_reason(
    &self,
    policy: &IssueFieldPolicy,
    origin: IssueTransitionOrigin,
    satisfies: impl Fn(&IssueTransitionEvidence) -> bool,
  ) -> Option<IssueMoveBlockReason> {
    if policy.source_owned.contains(&IssueField::Status) {
      Some(IssueMoveBlockReason::SourceOwned)
    } else if !self.origins.contains(&origin) {
      Some(IssueMoveBlockReason::OriginNotAllowed)
    } else if !self.evidence.iter().all(satisfies) {
      Some(IssueMoveBlockReason::EvidenceRequired)
    } else {
      None
    }
  }
}
