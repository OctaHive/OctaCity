//! Explicit resolution reuses authorized mapping publication and the existing Reporter owner.
use super::*;
pub use octacity_server_store::{IssueSourceResolutionReview, ResolveIssueSourceConflictIntent};
use octacity_server_store::{IssueSourceResolutionStore, ResolveIssueSourceConflict};

/// Reviewed current policy and exact terminal receipt, with no unconditional overwrite control.
#[derive(Clone, Debug)]
pub struct ResolveIssueSourceConflictCommand {
  /// Closed review of Issue/mapping versions, failed receipt and supported desired policy.
  pub intent: ResolveIssueSourceConflictIntent,
  /// Scoped operation identity; replay publishes no additional intent.
  pub idempotency_key: IdempotencyKey,
  /// Trusted original resolution time; later dispatch cannot renew its bounds.
  pub resolved_at: Timestamp,
}
impl Command for ResolveIssueSourceConflictCommand {
  type Outcome = octacity_server_store::IssueSourceMappingOutcome;
}
impl ManagementAuthorizationTarget for ResolveIssueSourceConflictCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.mapping.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceResolutionStore> ManagementCommandUseCase<ResolveIssueSourceConflictCommand>
  for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: ResolveIssueSourceConflictCommand,
  ) -> Result<octacity_server_store::IssueSourceMappingOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.intent.mapping.issue_id)?;
    self.visible(context, command.intent.mapping.issue_id).await?;
    if let Some(replay) = self
      .store
      .issue_source_resolution_replay(
        &context.security_scope().to_store(),
        &command.intent,
        &command.idempotency_key,
      )
      .await?
    {
      return Ok(replay);
    }
    let publication = self
      .authorized_mapping_publication(
        context,
        PublishIssueSourceMappingCommand {
          intent: command.intent.mapping,
          idempotency_key: command.idempotency_key,
          published_at: command.resolved_at,
        },
      )
      .await?;
    Ok(
      self
        .store
        .resolve_issue_source_conflict(crate::management_security::audited_mutation(
          context,
          ResolveIssueSourceConflict {
            publication,
            review: command.intent.review,
          },
        )?)
        .await?,
    )
  }
}
