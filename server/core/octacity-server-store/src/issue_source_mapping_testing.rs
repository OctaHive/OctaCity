//! Mapping metadata shares the ordinary Issue atomic mutation publisher.
use super::*;
#[async_trait::async_trait]
impl IssueSourceMappingStore for crate::testing::InMemoryProjectStore {
  async fn issue_source_mapping(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    version: Option<octacity_server_domain::IssueSourceMappingVersion>,
  ) -> Result<Option<PublishedIssueSourceMapping>, StoreError> {
    let state = self.lock()?;
    if !state
      .issues
      .sources
      .links
      .get(&issue)
      .is_some_and(|(s, l)| s == scope && l.selection.project_id == project)
    {
      return Err(crate::issue_source_mapping::mapping_conflict());
    }
    Ok(
      state
        .issues
        .sources
        .mappings
        .values()
        .rev()
        .find(|m| m.issue_id == issue && version.is_none_or(|v| m.version == v))
        .cloned(),
    )
  }
  async fn issue_source_mapping_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &PublishIssueSourceMappingIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceMappingOutcome>, StoreError> {
    mapping_replay(
      &self.lock()?.issues.sources,
      &ManagementIdempotencyKey::new(scope.clone(), key.clone()),
      intent,
    )
  }
  async fn publish_issue_source_mapping(
    &self,
    request: ManagementMutation<PublishIssueSourceMapping>,
  ) -> Result<IssueSourceMappingOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(replay) = mapping_replay(&state.issues.sources, &key, &request.intent)? {
      return Ok(replay);
    }
    let i = &request.intent;
    let (_, link) = state
      .issues
      .sources
      .links
      .get(&i.issue_id)
      .filter(|(s, l)| s == audit.security_scope() && l.selection.project_id == i.project_id)
      .ok_or_else(crate::issue_source_mapping::mapping_conflict)?;
    let current = state
      .issues
      .current
      .get(&(i.project_id, i.issue_id))
      .ok_or_else(crate::issue_source_mapping::mapping_conflict)?;
    let previous = state
      .issues
      .sources
      .mappings
      .values()
      .rev()
      .find(|m| m.issue_id == i.issue_id);
    let project = state
      .projects
      .get(&i.project_id)
      .ok_or_else(crate::issue_source_mapping::mapping_conflict)?;
    let workflow = state
      .workflows
      .workflow(project, current.workflow_id, Some(current.workflow_version))?;
    let mapping = request.resolve(current, link, previous, &workflow)?;
    let outcome = IssueSourceMappingOutcome {
      disposition: MutationDisposition::Applied,
      mapping: mapping.clone(),
    };
    commit_issue_change(
      &mut state,
      &audit,
      &key,
      StoreOperation::PublishIssueSourceMapping,
      i.issue_id,
      |tx| {
        tx.issues
          .sources
          .states
          .get_mut(&i.issue_id)
          .ok_or(StoreError::Unavailable)?
          .mapping_published()?;
        tx.issues.policies.insert(i.issue_id, mapping.field_policy());
        Ok(())
      },
      |memory| {
        memory
          .sources
          .mappings
          .insert((i.issue_id, mapping.version), mapping.clone());
        memory
          .sources
          .mapping_publications
          .insert(key.clone(), (i.clone(), outcome.clone()));
      },
    )?;
    Ok(outcome)
  }
}
fn mapping_replay(
  state: &IssueSourceMemoryState,
  key: &ManagementIdempotencyKey,
  intent: &PublishIssueSourceMappingIntent,
) -> Result<Option<IssueSourceMappingOutcome>, StoreError> {
  state
    .mapping_publications
    .get(key)
    .map(|(original, outcome)| {
      if original != intent {
        return Err(crate::issue_source_mapping::mapping_conflict());
      }
      Ok(IssueSourceMappingOutcome {
        disposition: MutationDisposition::Replayed,
        ..outcome.clone()
      })
    })
    .transpose()
}
