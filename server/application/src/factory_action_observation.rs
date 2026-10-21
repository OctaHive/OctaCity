//! Exact authoritative action observation; index catch-up and command acceptance are independent.
use crate::*;
use octacity_server_factory::{FactoryRunId, FactoryRunVersion, NodeAttemptId};
use octacity_server_store::{FactoryDiscoveryStore, FactoryRunControlIntent, FactoryRunStore};
use serde::Serialize;
/// Reads only one retained original attempt; no Issue revision or latest execution substitution.
#[derive(Clone, Debug)]
pub struct GetFactoryNodeActionQuery {
  /// Exact original Run.
  pub run_id: FactoryRunId,
  /// Exact original attempt in that Run.
  pub node_attempt_id: NodeAttemptId,
  /// Optional selected Project; exact target ownership must match before projection.
  pub project_id: Option<octacity_server_domain::ProjectId>,
  /// Optional selected card, checked through its original Work association and independent visibility.
  pub issue_id: Option<octacity_server_domain::IssueId>,
}
impl Query for GetFactoryNodeActionQuery {
  type Outcome = FactoryNodeActionObservation;
}
impl ManagementAuthorizationTarget for GetFactoryNodeActionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRunQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}
impl ManagementVisibilityTarget for GetFactoryNodeActionQuery {
  type Visibility = octacity_server_store::FactoryRunListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::FactoryRun];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    grant.visibility_for::<GetFactoryRunQuery>()
  }
}
/// These states describe an exact guarded action, never acceptance of the whole task.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryNodeActionState {
  /// Currently eligible according to the original response/retry policy.
  Waiting,
  /// Accepted intent, not yet consumed by the execution owner.
  Recorded,
  /// The owner recorded completion or the corresponding new retry attempt.
  Consumed,
  /// Expired/cancelled/superseded or otherwise no longer actionable retained target.
  Historical,
}
/// Safe original control provenance without private response values or reasons.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryNodeActionObservation {
  /// Exact original Run, including across later Issue edits.
  pub run_id: FactoryRunId,
  /// Exact original target.
  pub node_attempt_id: NodeAttemptId,
  /// Independently current Run version, for a fresh reviewed command only.
  pub run_version: FactoryRunVersion,
  /// Current original-target processing state.
  pub state: FactoryNodeActionState,
  /// Present only for a currently eligible exact response/retry.
  pub description: Option<FactoryNodeActionDescription>,
  /// Existing exact-current-candidate Decision fact; does not grant delivery/merge permission.
  pub accepted_candidate: bool,
  /// Original accepted intent version, never replaced by a later Run version.
  pub recorded_version: Option<FactoryRunVersion>,
  /// Original acceptance time, without the accepted response body.
  pub recorded_at: Option<Timestamp>,
  /// Trusted current read time used for eligibility/expiry.
  pub observed_at: Timestamp,
}
#[async_trait::async_trait]
impl<S: FactoryRunStore + FactoryDiscoveryStore + 'static> ManagementQueryUseCase<GetFactoryNodeActionQuery>
  for FactoryActionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryNodeActionQuery,
  ) -> Result<FactoryNodeActionObservation, ApplicationError> {
    let visibility = grant
      .visibility_for::<GetFactoryRunQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    FactoryDiscoveryHandlers::with_policy(self.store.clone(), self.policy.clone())
      .visible_run_summary(context, query.run_id, visibility)
      .await?;
    let snapshot = self.store.factory_run_snapshot(query.run_id).await?;
    if !snapshot.flow.attempts.iter().any(|a| a.id() == query.node_attempt_id) {
      return Err(ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::FactoryRun,
      }));
    }
    let not_found = || {
      ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::FactoryRun,
      })
    };
    if query
      .project_id
      .is_some_and(|p| p != snapshot.work.subject().project_id())
    {
      return Err(not_found());
    }
    if let Some(issue) = query.issue_id {
      let project = query.project_id.ok_or_else(ApplicationError::invalid)?;
      let resource = crate::management_security::instance_resource(ManagementResourceKind::Issue, issue)
        .map_err(|_| ApplicationError::invalid())?;
      let issue_grant = self
        .policy
        .authorize(context, ManagementAction::View, &resource)
        .await
        .map_err(|_| not_found())?;
      crate::issue::visible_issue(&issue_grant, issue)?;
      let association = self
        .associations
        .as_ref()
        .ok_or_else(ApplicationError::unavailable)?
        .issue_work_association(project, snapshot.work.id())
        .await?
        .ok_or_else(not_found)?;
      if association.issue_id != issue || association.run_id != query.run_id || association.project_id != project {
        return Err(not_found());
      }
    }
    let accepted_candidate = octacity_server_store::FactoryRunSummaryDiagnostics::from_snapshot(&snapshot)?
      .latest_decision
      .is_some_and(|d| d.outcome == octacity_server_factory::DecisionOutcome::Accept);
    let observed_at = self.clock.now()?;
    let description = FactoryNodeActionDescription::from_snapshot(&snapshot, query.node_attempt_id, observed_at);
    let control = snapshot.controls.iter().find(|control| {
      matches!(
        control.intent,
        FactoryRunControlIntent::ResolveNode {node_attempt_id, ..}
        | FactoryRunControlIntent::RetryNode {node_attempt_id}
          if node_attempt_id == query.node_attempt_id
      )
    });
    let consumed = control.is_some_and(|control| match control.intent {
      FactoryRunControlIntent::ResolveNode { .. } => snapshot
        .flow
        .completions
        .iter()
        .any(|c| c.node_attempt_id() == query.node_attempt_id),
      FactoryRunControlIntent::RetryNode { .. } => snapshot
        .flow
        .attempts
        .iter()
        .any(|a| a.retry_of() == Some(query.node_attempt_id)),
      FactoryRunControlIntent::Cancel => false,
    });
    let state = if consumed {
      FactoryNodeActionState::Consumed
    } else if control.is_some() && !snapshot.run.state().is_terminal() {
      FactoryNodeActionState::Recorded
    } else if description.is_some() {
      FactoryNodeActionState::Waiting
    } else {
      FactoryNodeActionState::Historical
    };
    Ok(FactoryNodeActionObservation {
      run_id: query.run_id,
      node_attempt_id: query.node_attempt_id,
      run_version: snapshot.run.version(),
      state,
      description,
      accepted_candidate,
      recorded_version: control.map(|c| c.run_version),
      recorded_at: control.map(|c| c.recorded_at),
      observed_at,
    })
  }
}
