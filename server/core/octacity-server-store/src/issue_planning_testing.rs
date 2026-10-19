//! Planning uses the same Project lock, revision publisher and evidence authority.
use super::*;
pub(super) fn definitions(
  state: &crate::project_testing::ProjectMemoryState,
  project_id: ProjectId,
  version: Option<u64>,
) -> Result<PublishedIssueProperties, StoreError> {
  let project = state.projects.get(&project_id).ok_or(StoreError::NotFound {
    entity: EntityKind::Project,
  })?;
  let latest = state
    .issues
    .property_versions
    .range((project_id, 0)..=(project_id, u64::MAX))
    .next_back()
    .map(|(_, p)| p);
  let version = version.unwrap_or_else(|| latest.map_or(0, |p| p.version));
  if version == 0 {
    return Ok(PublishedIssueProperties {
      project_id,
      version,
      definition: IssuePropertyDefinitionSet::default(),
      published_at: project.created_at,
    });
  }
  state
    .issues
    .property_versions
    .get(&(project_id, version))
    .cloned()
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })
}
#[async_trait::async_trait]
impl IssuePropertyStore for crate::testing::InMemoryProjectStore {
  async fn issue_children(&self, request: ListProjectIssues, parent_id: IssueId) -> Result<IssuePage, StoreError> {
    let state = self.lock()?;
    if !state.issues.current.contains_key(&(request.project_id(), parent_id)) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    let eligible = |i: &&IssueRevision| {
      i.project_id == request.project_id()
        && i.planning.parent_id == Some(parent_id)
        && request.visibility().allows(&i.id)
    };
    if request
      .after()
      .is_some_and(|after| !state.issues.current.values().filter(eligible).any(|i| i.id == after))
    {
      return Err(crate::issue::invalid_issue(StoreOperation::ListProjectIssues));
    }
    let mut items: Vec<_> = state
      .issues
      .current
      .values()
      .filter(eligible)
      .filter(|i| request.after().is_none_or(|a| i.id > a))
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

  async fn edit_issue_planning(
    &self,
    request: ManagementMutation<EditIssuePlanning>,
  ) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.issues.planning_edits.get(&key) {
      if intent != &request.intent {
        return Err(crate::issue_planning::planning_conflict());
      }
      return Ok(IssueMutationOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    let i = &request.intent;
    let current = state
      .issues
      .current
      .get(&(i.project_id, i.id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    validate_parent(&state, i.project_id, i.id, i.planning.parent_id)?;
    let catalog = definitions(&state, i.project_id, None)?;
    let issue = current.planned(
      &request,
      &state.issues.policies.get(&i.id).cloned().unwrap_or_default(),
      &catalog,
    )?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue,
    };
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::EditIssuePlanning,
      &outcome.issue,
      None,
      |memory| {
        memory
          .planning_edits
          .insert(key.clone(), (request.intent, outcome.clone()));
      },
    )?;
    Ok(outcome)
  }

  async fn issue_properties(
    &self,
    project_id: ProjectId,
    version: Option<u64>,
  ) -> Result<PublishedIssueProperties, StoreError> {
    definitions(&*self.lock()?, project_id, version)
  }
  async fn publish_issue_properties(
    &self,
    request: ManagementMutation<PublishIssueProperties>,
  ) -> Result<IssuePropertiesOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.issues.property_publications.get(&key) {
      if intent != &request.intent {
        return Err(crate::issue_planning::planning_conflict());
      }
      return Ok(IssuePropertiesOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      });
    }
    let intent = &request.intent;
    let current = definitions(&state, intent.project_id, None)?;
    if intent.expected_version != current.version {
      return Err(crate::issue_planning::planning_conflict());
    }
    intent.definition.validate_replacement(&current.definition)?;
    if request.published_at < current.published_at {
      return Err(crate::issue::invalid_issue(StoreOperation::PublishIssueProperties));
    }
    let resource = PublishedIssueProperties {
      project_id: intent.project_id,
      version: current
        .version
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(crate::issue_planning::planning_conflict)?,
      definition: intent.definition.clone(),
      published_at: request.published_at,
    };
    let outcome = IssuePropertiesOutcome {
      disposition: MutationDisposition::Applied,
      resource,
    };
    use crate::{project_testing::inject_failure, testing::MutationFailurePoint};
    let failure = state.issues.next_failure.take();
    let mut tx = state.clone();
    tx.issues
      .property_versions
      .insert((intent.project_id, outcome.resource.version), outcome.resource.clone());
    inject_failure(failure, MutationFailurePoint::DomainState)?;
    tx.issues
      .property_publications
      .insert(key.clone(), (intent.clone(), outcome.clone()));
    inject_failure(failure, MutationFailurePoint::IdempotencyOutcome)?;
    tx.active_references.insert(intent.project_id);
    tx.audit_facts.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::PublishIssueProperties,
      EntityKind::Project,
      intent.project_id,
    ));
    inject_failure(failure, MutationFailurePoint::AuditFact)?;
    tx.outbox_entries.insert(crate::testing::management_evidence_identity(
      "PublishIssueProperties",
      &key,
    ));
    inject_failure(failure, MutationFailurePoint::OutboxEntry)?;
    inject_failure(failure, MutationFailurePoint::Commit)?;
    *state = tx;
    Ok(outcome)
  }
}

// Walk authoritative current parent links under the same mutation lock.
pub(super) fn validate_parent(
  state: &crate::project_testing::ProjectMemoryState,
  project_id: ProjectId,
  id: IssueId,
  parent: Option<IssueId>,
) -> Result<(), StoreError> {
  let mut cursor = parent;
  let mut seen = std::collections::BTreeSet::new();
  while let Some(parent) = cursor {
    if parent == id || !seen.insert(parent) {
      return Err(crate::issue_planning::planning_conflict());
    }
    cursor = state
      .issues
      .current
      .get(&(project_id, parent))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?
      .planning
      .parent_id;
  }
  Ok(())
}
