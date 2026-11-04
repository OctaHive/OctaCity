//! Local status authoring uses visible immutable workflows and the ordinary Flow publication path.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{FactoryFlowConfiguration, FlowActionBinding, FlowOutcomeDefinition};
use octacity_server_store::IssuePipelineStatusSelection;

/// Resolves an exact local transition under the author's current workflow read rights.
/// This check performs no move, admission or provider I/O.
#[async_trait::async_trait]
pub trait FactoryIssueStatusAuthorizer: Send + Sync {
  /// Validates the pinned workflow/edge; execution rechecks Issue applicability and move authority.
  async fn validate_issue_status(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssuePipelineStatusSelection,
  ) -> Result<(), ApplicationError>;
}

pub(crate) fn issue_status_selection(
  binding: &FlowActionBinding,
  outcomes: &[FlowOutcomeDefinition],
) -> Result<IssuePipelineStatusSelection, ApplicationError> {
  let selection: IssuePipelineStatusSelection = serde_json::from_value(binding.parameters().clone())
    .map_err(|_| FactoryValidationDiagnostics::invalid("/action/parameters"))?;
  if binding.action().as_str() != "transition_issue"
    || !outcomes.iter().any(|outcome| outcome.key() == &selection.outcome)
  {
    return Err(FactoryValidationDiagnostics::invalid("/action/parameters/outcome"));
  }
  Ok(selection)
}

pub(crate) async fn validate_flow_issue_status_actions(
  flow: &FactoryFlowConfiguration,
  authorizer: Option<&dyn FactoryIssueStatusAuthorizer>,
  context: &ManagementRequestContext,
  project: ProjectId,
) -> Result<(), ApplicationError> {
  for selection in flow_issue_status_selections(flow) {
    authorizer
      .ok_or_else(ApplicationError::capability_unavailable)?
      .validate_issue_status(context, project, &selection?)
      .await?;
  }
  Ok(())
}

/// Traverses the same pinned closure for publication and known execution-target checks.
pub(crate) fn flow_issue_status_selections(
  flow: &FactoryFlowConfiguration,
) -> impl Iterator<Item = Result<IssuePipelineStatusSelection, ApplicationError>> + '_ {
  flow
    .closure
    .definitions()
    .iter()
    .flat_map(|definition| definition.nodes())
    .filter_map(|node| {
      node
        .action()
        .filter(|binding| binding.action().as_str() == "transition_issue")
        .map(|binding| issue_status_selection(binding, node.outcomes()))
    })
}

#[async_trait::async_trait]
impl<S: IssueWorkflowStore + 'static> FactoryIssueStatusAuthorizer
  for AuthorizedQueryHandler<IssueWorkflowHandlers<S>>
{
  async fn validate_issue_status(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssuePipelineStatusSelection,
  ) -> Result<(), ApplicationError> {
    let workflow = self
      .handle_query(
        context,
        GetIssueWorkflowQuery {
          project_id: project,
          id: selection.workflow_id,
          version: Some(selection.workflow_version),
        },
      )
      .await
      .map_err(|error| match error {
        ManagementHandlerError::Application(error) => error,
        ManagementHandlerError::Forbidden(_) => ApplicationError::invalid(),
      })?;
    if !workflow.definition.transitions.iter().any(|edge| {
      edge.id == selection.transition_id
        && edge
          .origins
          .contains(&octacity_server_store::IssueTransitionOrigin::Pipeline)
    }) {
      return Err(FactoryValidationDiagnostics::invalid(
        "/action/parameters/transition_id",
      ));
    }
    Ok(())
  }
}
