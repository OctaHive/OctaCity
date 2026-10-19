//! Local Issue effects are idempotent commands, executed by the existing trusted-action owner.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryRunStore, IssueExecutionStore, IssuePipelineActionProvenance, IssuePipelineStatusSelection, IssueStore,
  RetainedIssueStatusCommand, TransitionIssueIntent,
};
use std::sync::Arc;

/// Installed provider-neutral action. The host supplies accepted attribution and current policy;
/// neither the binding nor projected input can select a principal, permission or target Issue.
pub struct FactoryIssueStatusAction<S, F> {
  identity: ImmutableReference,
  issues: Arc<S>,
  runs: Arc<F>,
  queries: AuthorizedQueryHandler<IssueHandlers<S>>,
  commands: AuthorizedCommandHandler<IssueHandlers<S>>,
  context: ManagementRequestContext,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S: IssueStore, F> FactoryIssueStatusAction<S, F> {
  /// Registers only an exact installed plugin; construction performs no dependency I/O.
  pub fn new(
    identity: ImmutableReference,
    issues: Arc<S>,
    runs: Arc<F>,
    context: ManagementRequestContext,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    let handlers = Arc::new(IssueHandlers::new(issues.clone(), policy.clone()));
    Self {
      identity,
      issues,
      runs,
      queries: AuthorizedQueryHandler::new(policy.clone(), handlers.clone()),
      commands: AuthorizedCommandHandler::new(policy, handlers),
      context,
      clock,
    }
  }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedEvidence {
  #[serde(default)]
  candidate: Option<CandidateSubject>,
  #[serde(default)]
  evidence: Vec<FactoryDigest>,
}
fn authorized_error(error: ManagementHandlerError<ApplicationError>) -> ApplicationError {
  match error {
    ManagementHandlerError::Application(error) => error,
    ManagementHandlerError::Forbidden(_) => ApplicationError::invalid(),
  }
}

#[async_trait::async_trait]
impl<S: IssueStore + IssueExecutionStore, F: FactoryRunStore> FactoryFlowActionPlugin
  for FactoryIssueStatusAction<S, F>
{
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    crate::factory_issue_status_authoring::issue_status_selection(binding, outcomes)?;
    if binding.plugin() != &self.identity {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    // Cancelling never replays a pending move or undoes an already committed business result.
    Ok(Some(BudgetUsage::default()))
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let snapshot = self.runs.factory_run_snapshot(request.input.factory_run_id()).await?;
    let attempt = snapshot
      .flow
      .attempts
      .iter()
      .find(|a| a.id() == request.node_attempt_id)
      .ok_or_else(invalid)?;
    if snapshot.work.id() != request.work_id
      || snapshot.work.subject() != request.subject
      || !snapshot.flow.data.inputs.contains(request.input)
      || attempt.input_digest() != request.input.digest().map_err(|_| invalid())?
      || attempt.flow_run_id() != request.input.flow_run_id()
      || attempt.workflow_cycle_id() != request.input.cycle_id()
    {
      return Err(invalid());
    }
    let selection: IssuePipelineStatusSelection =
      serde_json::from_value(request.binding.parameters().clone()).map_err(|_| invalid())?;
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(request.input.definition())
      .ok_or_else(invalid)?;
    let node = definition.node(request.input.node()).ok_or_else(invalid)?;
    self.validate(request.binding, node.outcomes())?;
    let outcome = node
      .outcomes()
      .iter()
      .find(|o| o.key() == &selection.outcome)
      .ok_or_else(invalid)?;
    let schema = request
      .schemas
      .iter()
      .find(|s| s.reference() == outcome.schema())
      .ok_or_else(invalid)?;
    // Validate the receipt before any business effect; the common owner retains this exact result.
    let payload =
      FlowPayload::new(schema, serde_json::json!({"receipt":request.operation_id})).map_err(|_| invalid())?;
    let output_bytes = serde_json::to_vec(&payload).map_err(|_| invalid())?.len() as u64;
    BudgetUsage {
      output_bytes,
      ..Default::default()
    }
    .validate(request.budget)
    .map_err(|_| invalid())?;
    let project = request.subject.project_id();
    let association = self
      .issues
      .issue_work_association(project, request.work_id)
      .await?
      .filter(|a| a.run_id == request.input.factory_run_id() && a.requirements_version.is_some())
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let issue = self
      .queries
      .handle_query(
        &self.context,
        GetIssueQuery {
          project_id: project,
          id: association.issue_id,
          version: None,
        },
      )
      .await
      .map_err(authorized_error)?;
    let retained = self
      .issues
      .issue_status_command(
        project,
        issue.id,
        &self.context.security_scope().to_store(),
        request.operation_id,
      )
      .await?;
    let at = self.clock.now()?.max(request.observed_at);
    let deadline = request.deadline.min(request.authority_expires_at);
    if at >= deadline {
      return Err(invalid());
    }
    let mutation = if let Some(retained) = retained {
      let p = &retained.provenance;
      if retained.selection != selection
        || p.work_id != request.work_id
        || p.run_id != request.input.factory_run_id()
        || p.flow_run_id != request.input.flow_run_id()
        || p.cycle_id != request.input.cycle_id()
        || p.node_attempt_id != request.node_attempt_id
      {
        return Err(invalid());
      }
      retained.restore(&snapshot)?
    } else {
      let selected: SelectedEvidence =
        serde_json::from_value(request.input.payload().value().clone()).map_err(|_| invalid())?;
      if selected
        .evidence
        .iter()
        .any(|digest| !request.input.sources().iter().any(|r| r.digest() == *digest))
      {
        return Err(invalid());
      }
      RetainedIssueStatusCommand {
        selection: selection.clone(),
        provenance: IssuePipelineActionProvenance {
          issue_id: issue.id,
          requirements_version: association.requirements_version.ok_or_else(invalid)?,
          work_id: request.work_id,
          run_id: request.input.factory_run_id(),
          flow_run_id: request.input.flow_run_id(),
          cycle_id: request.input.cycle_id(),
          node_attempt_id: request.node_attempt_id,
          operation_id: request.operation_id,
          candidate: selected.candidate,
          evidence: selected.evidence,
          outcome: selection.outcome.clone(),
        },
        intent: TransitionIssueIntent {
          project_id: project,
          id: issue.id,
          expected_version: issue.version,
          workflow_id: selection.workflow_id,
          workflow_version: selection.workflow_version,
          expected_status_id: issue.status_id,
          transition_id: selection.transition_id,
          reason: None,
        },
        moved_at: at,
      }
      .restore(&snapshot)?
    };
    self
      .commands
      .handle_command(
        &self.context,
        ApplyIssuePipelineStatusCommand {
          intent: mutation.intent.clone(),
          accepted: mutation.pipeline_action().ok_or_else(invalid)?.clone(),
          moved_at: mutation.moved_at,
        },
      )
      .await
      .map_err(authorized_error)?;
    let finished = self.clock.now()?.max(at);
    let usage = BudgetUsage {
      output_bytes,
      elapsed_millis: u64::try_from(
        finished
          .unix_millis()
          .checked_sub(request.started_at.unix_millis())
          .ok_or_else(invalid)?,
      )
      .map_err(|_| invalid())?,
      ..Default::default()
    };
    usage.validate(request.budget).map_err(|_| invalid())?;
    if finished >= deadline {
      return Err(invalid());
    }
    Ok(FactoryFlowActionObservation {
      discovery: None,
      outcome: selection.outcome,
      payload,
      usage,
      outbox: vec![],
    })
  }
}
