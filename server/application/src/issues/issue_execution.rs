//! Authorized execution reservation, separate from manual Issue creation and ordinary admission.
use crate::*;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use octacity_server_store::{
  FactoryAdmissionStore, FactoryRunStore, IdempotencyKey, IssueExecutionStore, RequestIssueExecution,
};
pub use octacity_server_store::{
  IssueExecutionMapping, IssueExecutionOutcome, IssueExecutionRequest, IssueExecutionTarget,
  RequestIssueExecutionIntent,
};
use std::sync::Arc;

#[path = "issue_execution_read.rs"]
mod reads;
pub use reads::*;

/// Explicit reviewed request; saving an Issue never invokes this command.
#[derive(Clone, Debug)]
pub struct RequestIssueExecutionCommand {
  /// Frozen Issue revision and target.
  pub intent: RequestIssueExecutionIntent,
  /// Scoped transport replay key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted request time.
  pub requested_at: Timestamp,
}
impl Command for RequestIssueExecutionCommand {
  type Outcome = IssueExecutionOutcome;
}
impl ManagementAuthorizationTarget for RequestIssueExecutionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Execute, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.issue_id)
  }
}
/// Dedicated read of exact request lineage; never performs publication or admission.
#[derive(Clone, Debug)]
pub struct GetIssueExecutionQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible owning Issue.
  pub issue_id: IssueId,
  /// Exact retained request.
  pub request_id: octacity_server_domain::IssueExecutionRequestId,
}
impl Query for GetIssueExecutionQuery {
  type Outcome = IssueExecutionRequest;
}
impl ManagementAuthorizationTarget for GetIssueExecutionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Request-only composition; publication/admission remains the existing intake's responsibility.
pub struct IssueExecutionHandlers<S, F> {
  store: Arc<S>,
  factory: Arc<F>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S, F> IssueExecutionHandlers<S, F> {
  /// Keeps optional execution dependencies outside manual Issue handlers.
  pub fn new(store: Arc<S>, factory: Arc<F>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self { store, factory, policy }
  }
}
#[async_trait::async_trait]
impl<
  S: IssueExecutionStore + octacity_server_store::IssueStore + octacity_server_store::IssuePropertyStore,
  F: FactoryAdmissionStore + FactoryRunStore,
> ManagementCommandUseCase<RequestIssueExecutionCommand> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: RequestIssueExecutionCommand,
  ) -> Result<IssueExecutionOutcome, ApplicationError> {
    crate::issues::issue::visible_issue(grant, command.intent.issue_id)?;
    crate::factory::admission::factory_admission::authorize_project_admission(
      self.policy.as_ref(),
      context,
      command.intent.project_id,
    )
    .await?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      command.intent.issue_id,
    )
    .await?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::FactoryConfiguration,
      command.intent.target.configuration_id,
    )
    .await?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      command.intent.target.repository_id,
    )
    .await?;
    let request = crate::management_security::audited_mutation(
      context,
      RequestIssueExecution {
        admission_classification: None,
        snapshot_digest: None,
        id: octacity_server_domain::IssueExecutionRequestId::generate(),
        intent: command.intent,
        idempotency_key: command.idempotency_key,
        requested_at: command.requested_at,
      },
    )?;
    if let Some(replay) = self.store.replay_issue_execution(&request).await? {
      return Ok(replay);
    }
    let intent = &request.mutation().intent;
    let target = &intent.target;
    let selected = self
      .factory
      .factory_admission_context(octacity_server_store::ReadFactoryAdmissionContext {
        project_id: intent.project_id,
        configuration_id: target.configuration_id,
        configuration_version: target.configuration_version,
        repository_id: Some(target.repository_id),

        repository_version: Some(target.repository_version),
      })
      .await?;
    crate::factory::admission::factory_admission::validate_admission_project(&selected, intent.project_id)?;
    crate::factory::admission::factory_admission::validate_launch_policy(
      &selected,
      octacity_server_factory::FactoryAdmissionOrigin::Manual,
    )?;
    if selected.configuration.configuration.flow().issue_snapshot.is_none() {
      resolve_issue_predecessor(
        self.store.as_ref(),
        self.factory.as_ref(),
        self.policy.as_ref(),
        context,
        intent,
        selected.configuration.configuration.flow(),
      )
      .await?;
    }
    let snapshot = if selected.configuration.configuration.flow().issue_snapshot.is_some() {
      let issue = self.store.issue(intent.project_id, intent.issue_id, None).await?;
      if issue.version != intent.expected_version {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        );
      }
      Some(
        crate::issues::issue_execution_snapshot::select(
          self.store.as_ref(),
          self.policy.as_ref(),
          context,
          &issue,
          target,
          selected.configuration.configuration.flow(),
          None,
        )
        .await?,
      )
    } else {
      None
    };
    let mut actions = crate::factory::reporting::factory_issue_status_authoring::flow_issue_status_selections(
      selected.configuration.configuration.flow(),
    )
    .peekable();
    if actions.peek().is_some() {
      let issue = self.store.issue(intent.project_id, intent.issue_id, None).await?;
      if issue.version != intent.expected_version {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        );
      }

      for action in actions {
        let action = action?;
        if action.workflow_id != issue.workflow_id || action.workflow_version != issue.workflow_version {
          return Err(FactoryValidationDiagnostics::invalid("/target"));
        }
      }
    }

    crate::triggers::manual_trigger::select_source(
      &target.source,
      selected.repository.as_ref().ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::InvalidInput)?;
    let mutation = RequestIssueExecution {
      admission_classification: snapshot.as_ref().and_then(|s| s.admission_classification.clone()),
      snapshot_digest: snapshot.as_ref().map(|s| s.digest()).transpose()?,
      ..request.mutation().clone()
    };
    Ok(
      self
        .store
        .request_issue_execution(crate::management_security::audited_mutation(context, mutation)?)
        .await?,
    )
  }
}

pub(crate) async fn resolve_issue_predecessor(
  issues: &(impl IssueExecutionStore + octacity_server_store::IssueStore),
  runs: &impl FactoryRunStore,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  intent: &RequestIssueExecutionIntent,
  target_flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> Result<Option<FactorySourceHandoff>, ApplicationError> {
  let Some(run_id) = intent.target.predecessor_run else {
    crate::factory::admission::factory_admission::reject_missing_incoming_result(target_flow)?;
    return Ok(None);
  };
  resolve_associated_issue_predecessor(
    issues,
    runs,
    policy,
    context,
    IssuePredecessorRequest {
      project: intent.project_id,
      issue: intent.issue_id,
      expected_version: intent.expected_version,
      repository: intent.target.repository_id,
      run: run_id,
    },
    target_flow,
  )
  .await
  .map(Some)
}

pub(crate) struct IssuePredecessorRequest {
  pub project: ProjectId,
  pub issue: IssueId,
  pub expected_version: octacity_server_domain::IssueVersion,
  pub repository: octacity_server_domain::RepositoryId,
  pub run: octacity_server_factory::FactoryRunId,
}

pub(crate) async fn resolve_associated_issue_predecessor(
  issues: &(impl IssueExecutionStore + octacity_server_store::IssueStore),
  runs: &impl FactoryRunStore,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  request: IssuePredecessorRequest,
  target_flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> Result<FactorySourceHandoff, ApplicationError> {
  let diagnostic = |code| FactoryValidationDiagnostics::unavailable_with_code("/incoming_result", code);
  let predecessor = target_flow
    .accepted_predecessor
    .as_ref()
    .ok_or_else(|| diagnostic("unexpected_incoming_result"))?;
  crate::factory::authoring::factory_configuration_choices::require_visible_reference(
    policy,
    context,
    ManagementResourceKind::FactoryRun,
    request.run,
  )
  .await
  .map_err(|_| diagnostic("incoming_result_inaccessible"))?;
  let snapshot = runs
    .factory_run_snapshot(request.run)
    .await
    .map_err(|error| match error {
      octacity_server_store::StoreError::NotFound { .. } => diagnostic("incoming_result_inaccessible"),
      other => other.into(),
    })?;
  let association = issues
    .issue_work_association(request.project, snapshot.work.id())
    .await?
    .filter(|link| link.issue_id == request.issue && link.run_id == request.run)
    .ok_or_else(|| diagnostic("incoming_result_wrong_issue"))?;
  let version = association
    .requirements_version
    .ok_or_else(|| diagnostic("incoming_result_stale"))?;
  let original = issues.issue(request.project, request.issue, Some(version)).await?;
  let reviewed = issues
    .issue(request.project, request.issue, Some(request.expected_version))
    .await?;
  let current = issues.issue(request.project, request.issue, None).await?;
  if !original.content_available
    || !reviewed.content_available
    || original.applicability_version != reviewed.applicability_version
    || reviewed.applicability_version != current.applicability_version
  {
    return Err(diagnostic("incoming_result_stale"));
  }
  if snapshot.work.subject().repository().is_err()
    || snapshot
      .work
      .subject()
      .repository()
      .is_ok_and(|subject| subject.repository_id() != request.repository)
  {
    return Err(diagnostic("incoming_result_wrong_subject"));
  }
  let record = snapshot
    .accepted_terminal_result(predecessor.definition, &predecessor.terminal, &predecessor.outcomes)?
    .ok_or_else(|| diagnostic("incoming_result_not_accepted"))?;
  if target_flow.incoming_schemas().next() != Some(record.payload().schema()) {
    return Err(diagnostic("incoming_result_incompatible"));
  }
  let mut handoff = FactorySourceHandoff::new(record.clone(), snapshot.work.artifacts().clone())
    .with_issue_text_evidence(octacity_server_store::WorkSourceIssueText {
      title: &original.content.title,
      description: &original.content.description,
    });
  if association.request_id.is_none() {
    handoff.issue_text = Some(
      issue_execution_documents(&original.content, &IssueExecutionMapping::TitleDescriptionAcceptanceV1)
        .map_err(|_| diagnostic("incoming_result_missing_implementation_acceptance"))?,
    );
  }
  Ok(handoff)
}

pub(crate) fn issue_execution_documents(
  content: &octacity_server_store::IssueContent,
  mapping: &IssueExecutionMapping,
) -> Result<octacity_server_store::ProjectTextDocuments, ApplicationError> {
  Ok(octacity_server_store::ProjectTextDocuments::try_new(
    String::from_utf8(mapping.task_bytes(content)).map_err(|_| ApplicationError::invalid())?,
    content.acceptance.as_deref().ok_or_else(ApplicationError::invalid)?,
  )?)
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore, F: Send + Sync> ManagementQueryUseCase<GetIssueExecutionQuery>
  for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionQuery,
  ) -> Result<IssueExecutionRequest, ApplicationError> {
    crate::issues::issue::visible_issue(grant, query.issue_id)?;
    Ok(
      self
        .store
        .issue_execution(query.project_id, query.issue_id, query.request_id)
        .await?,
    )
  }
}

#[path = "issue_execution_history.rs"]
mod history;
pub use history::*;

impl<S: octacity_server_store::IssueLegacyStore, F> IssueExecutionHandlers<S, F> {
  pub(crate) async fn legacy_work(
    &self,
    project: ProjectId,
    issue: IssueId,
    after: Option<octacity_server_factory::WorkEnvelopeId>,
    observation: Option<octacity_server_factory::FactoryDigest>,
    limit: u16,
  ) -> Result<octacity_server_store::IssueLegacyWorkPage, ApplicationError> {
    Ok(
      self
        .store
        .legacy_issue_work(project, issue, after, observation, limit)
        .await?,
    )
  }
}
