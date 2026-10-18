//! Memory parity for Project workflows; never invokes execution or providers.
use crate::*;
use octacity_server_domain::{
  EntityKind, IssueWorkflowId, IssueWorkflowSelectionVersion, IssueWorkflowVersion, ProjectId,
};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct IssueWorkflowMemoryState {
  current: BTreeMap<(ProjectId, IssueWorkflowId), IssueWorkflowVersion>,
  versions: BTreeMap<(ProjectId, IssueWorkflowId, IssueWorkflowVersion), PublishedIssueWorkflow>,
  publications: BTreeMap<ManagementIdempotencyKey, (PublishIssueWorkflowIntent, IssueWorkflowPublicationOutcome)>,
  defaults: BTreeMap<ProjectId, (IssueWorkflowSelectionVersion, IssueWorkflowId, IssueWorkflowVersion)>,
  selections: BTreeMap<ManagementIdempotencyKey, (SelectDefaultIssueWorkflowIntent, IssueWorkflowSelectionOutcome)>,
}
impl IssueWorkflowMemoryState {
  pub(super) fn mutation_count(&self) -> usize {
    self.publications.len() + self.selections.len()
  }
  fn default(&self, project: &Project) -> Result<IssueWorkflowDefault, StoreError> {
    match self.defaults.get(&project.id) {
      Some((version, id, workflow_version)) => Ok(IssueWorkflowDefault {
        version: *version,
        workflow: self.workflow(project, *id, Some(*workflow_version))?,
      }),
      None => Ok(IssueWorkflowDefault {
        version: IssueWorkflowSelectionVersion::INITIAL,
        workflow: initial_issue_workflow(project.id, project.created_at)?,
      }),
    }
  }
  fn workflow(
    &self,
    project: &Project,
    id: IssueWorkflowId,
    version: Option<IssueWorkflowVersion>,
  ) -> Result<PublishedIssueWorkflow, StoreError> {
    let initial = initial_issue_workflow(project.id, project.created_at)?;
    let version = version
      .or_else(|| self.current.get(&(project.id, id)).copied())
      .or_else(|| (id == initial.id).then_some(initial.version))
      .ok_or_else(not_found)?;
    self
      .versions
      .get(&(project.id, id, version))
      .cloned()
      .or_else(|| (id == initial.id && version == initial.version).then_some(initial))
      .ok_or_else(not_found)
  }
}
fn not_found() -> StoreError {
  StoreError::NotFound {
    entity: EntityKind::IssueWorkflow,
  }
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::IssueWorkflow,
  }
}

#[async_trait::async_trait]
impl IssueWorkflowStore for crate::testing::InMemoryProjectStore {
  async fn default_issue_workflow(&self, project_id: ProjectId) -> Result<IssueWorkflowDefault, StoreError> {
    let state = self.lock()?;
    let project = state.projects.get(&project_id).ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
    state.workflows.default(project)
  }
  async fn issue_workflow(
    &self,
    project_id: ProjectId,
    id: IssueWorkflowId,
    version: Option<IssueWorkflowVersion>,
  ) -> Result<PublishedIssueWorkflow, StoreError> {
    let state = self.lock()?;
    let project = state.projects.get(&project_id).ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
    state.workflows.workflow(project, id, version)
  }
  async fn publish_issue_workflow(
    &self,
    request: ManagementMutation<PublishIssueWorkflow>,
  ) -> Result<IssueWorkflowPublicationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.workflows.publications.get(&key) {
      if intent != &request.intent {
        return Err(conflict());
      }
      let mut replay = outcome.clone();
      replay.disposition = MutationDisposition::Replayed;
      return Ok(replay);
    }
    let intent = &request.intent;
    let project = state.projects.get(&intent.project_id).ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
    let current = match state.workflows.workflow(project, intent.id, None) {
      Ok(workflow) => Some(workflow),
      Err(StoreError::NotFound { .. }) => None,
      Err(error) => return Err(error),
    };
    if current.as_ref().map(|w| w.version) != intent.expected_current_version {
      return Err(conflict());
    }
    if request.published_at < project.created_at
      || current.as_ref().is_some_and(|w| request.published_at < w.published_at)
    {
      return Err(crate::issue_workflow::invalid_workflow());
    }
    intent.definition.validate()?;
    let version = intent
      .expected_current_version
      .map_or(Ok(IssueWorkflowVersion::INITIAL), |v| v.next())
      .map_err(|_| crate::issue_workflow::invalid_workflow())?;
    let workflow = PublishedIssueWorkflow {
      project_id: intent.project_id,
      id: intent.id,
      version,
      definition: intent.definition.clone(),
      published_at: request.published_at,
    };
    let outcome = IssueWorkflowPublicationOutcome {
      disposition: MutationDisposition::Applied,
      workflow: workflow.clone(),
    };
    state.workflows.current.insert((intent.project_id, intent.id), version);
    state
      .workflows
      .versions
      .insert((intent.project_id, intent.id, version), workflow);
    state
      .workflows
      .publications
      .insert(key.clone(), (intent.clone(), outcome.clone()));
    state.active_references.insert(intent.project_id);
    state.audit_facts.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::PublishIssueWorkflow,
      EntityKind::IssueWorkflow,
      intent.id,
    ));
    state.outbox_entries.insert(format!(
      "publish-issue-workflow:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    Ok(outcome)
  }
  async fn select_default_issue_workflow(
    &self,
    request: ManagementMutation<SelectDefaultIssueWorkflow>,
  ) -> Result<IssueWorkflowSelectionOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.workflows.selections.get(&key) {
      if intent != &request.intent {
        return Err(conflict());
      }
      let mut replay = outcome.clone();
      replay.disposition = MutationDisposition::Replayed;
      return Ok(replay);
    }
    let intent = &request.intent;
    let project = state.projects.get(&intent.project_id).ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
    let current = state.workflows.default(project)?;
    if current.version != intent.expected_version {
      return Err(conflict());
    }
    let workflow = state
      .workflows
      .workflow(project, intent.workflow_id, Some(intent.workflow_version))?;
    let version = current
      .version
      .next()
      .map_err(|_| crate::issue_workflow::invalid_workflow())?;
    let outcome = IssueWorkflowSelectionOutcome {
      disposition: MutationDisposition::Applied,
      default: IssueWorkflowDefault { version, workflow },
    };
    state.workflows.defaults.insert(
      intent.project_id,
      (version, intent.workflow_id, intent.workflow_version),
    );
    state
      .workflows
      .selections
      .insert(key.clone(), (intent.clone(), outcome.clone()));
    state.active_references.insert(intent.project_id);
    state.audit_facts.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::SelectDefaultIssueWorkflow,
      EntityKind::Project,
      intent.project_id,
    ));
    state.outbox_entries.insert(format!(
      "select-default-issue-workflow:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    Ok(outcome)
  }
}
