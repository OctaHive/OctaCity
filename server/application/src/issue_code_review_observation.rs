//! Explicit read-only refresh and retained per-head history, without another worker/poller.
use super::*;
use octacity_server_store::{
  IssueCodeReviewObservationOutcome, IssueCodeReviewObservationRecord, ObserveIssueCodeReview,
  ObserveIssueCodeReviewIntent,
};
/// Explicit bounded refresh of one retained review; it cannot change its native target.
#[derive(Clone, Debug)]
pub struct RefreshIssueCodeReviewCommand {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible owning Issue.
  pub issue_id: IssueId,
  /// Original canonical association.
  pub association_id: FactoryDigest,
  /// Original scoped retry identity.
  pub idempotency_key: IdempotencyKey,
}
impl Command for RefreshIssueCodeReviewCommand {
  type Outcome = IssueCodeReviewObservationOutcome;
}
impl ManagementAuthorizationTarget for RefreshIssueCodeReviewCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = AttachIssueCodeReviewCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Bounded original per-head history, separate from association membership.
#[derive(Clone, Debug)]
pub struct ListIssueCodeReviewObservationsQuery {
  selection: ListIssueCodeReviewsQuery,
  association_id: FactoryDigest,
}
impl ListIssueCodeReviewObservationsQuery {
  /// Uses the shared page/cursor bounds; association is part of the observation security binding.
  pub fn try_new(
    project: ProjectId,
    issue: IssueId,
    association_id: FactoryDigest,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    Ok(Self {
      selection: ListIssueCodeReviewsQuery::try_new(project, issue, after, limit)?,
      association_id,
    })
  }
}
impl Query for ListIssueCodeReviewObservationsQuery {
  type Outcome = IssueCodeReviewObservationPage;
}
impl ManagementAuthorizationTarget for ListIssueCodeReviewObservationsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.selection.management_resource()
  }
}
/// Retained immutable observations survive revoked reads and provider outage.
#[derive(Clone, Debug, Serialize)]
pub struct IssueCodeReviewObservationPage {
  /// Exact original association; never rewritten from a newer remote head.
  pub association: IssueCodeReviewAssociation,
  /// Original sequence-bearing normalized observations.
  pub items: Vec<IssueCodeReviewObservationRecord>,
  /// Independent current read authority, never an acceptance signal.
  pub availability: IssueCodeReviewAvailability,
  /// Opaque original membership/association/security continuation.
  pub next_cursor: Option<String>,
}
#[async_trait::async_trait]
impl<S: IssueCodeReviewStore> ManagementCommandUseCase<RefreshIssueCodeReviewCommand> for IssueCodeReviewHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: RefreshIssueCodeReviewCommand,
  ) -> Result<IssueCodeReviewObservationOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.issue_id)?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      command.issue_id,
    )
    .await?;
    let intent = ObserveIssueCodeReviewIntent {
      project_id: command.project_id,
      issue_id: command.issue_id,
      association_id: command.association_id,
    };
    let scope = context.security_scope().to_store();
    let association = self
      .store
      .issue_code_review(intent.project_id, intent.issue_id, &scope, intent.association_id)
      .await?;
    self
      .visible_repository(context, intent.project_id, &association.selection)
      .await?;
    if let Some(original) = self
      .store
      .issue_code_review_observation_replay(&scope, &intent, &command.idempotency_key)
      .await?
    {
      return Ok(original);
    }
    self
      .reader
      .authorize(context, intent.project_id, &association.selection)
      .await?;
    let observation = self
      .reader
      .read(context, intent.project_id, &association.selection)
      .await?;
    self
      .reader
      .authorize(context, intent.project_id, &association.selection)
      .await?;
    Ok(
      self
        .store
        .observe_issue_code_review(crate::management_security::audited_mutation(
          context,
          ObserveIssueCodeReview {
            intent,
            observation,
            idempotency_key: command.idempotency_key,
            recorded_at: self.clock.now()?,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueCodeReviewStore> ManagementQueryUseCase<ListIssueCodeReviewObservationsQuery>
  for IssueCodeReviewHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueCodeReviewObservationsQuery,
  ) -> Result<IssueCodeReviewObservationPage, ApplicationError> {
    let mut page = self.read_observation_page(context, grant, query).await?;
    for item in &mut page.items {
      item.observation.check_details = None;
    }
    Ok(page)
  }
}
impl<S: IssueCodeReviewStore> IssueCodeReviewHandlers<S> {
  pub(super) async fn read_observation_page(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueCodeReviewObservationsQuery,
  ) -> Result<IssueCodeReviewObservationPage, ApplicationError> {
    let selected = query.selection;
    crate::issue::visible_issue(grant, selected.issue_id)?;
    let (scope, visibility) = self
      .read_scope(
        context,
        grant,
        selected.project_id,
        selected.issue_id,
        Some(query.association_id),
      )
      .await?;
    if selected.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let security = context.security_scope().to_store();
    let association = self
      .store
      .issue_code_review(selected.project_id, selected.issue_id, &security, query.association_id)
      .await?;
    if !visibility.allows(&association.selection.repository_identity()) {
      return Err(
        octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Issue,
        }
        .into(),
      );
    }
    let availability = self
      .availability(context, selected.project_id, &association.selection)
      .await;
    let page = self
      .store
      .issue_code_review_observations(
        ReadIssueCodeReviews {
          project_id: selected.project_id,
          issue_id: selected.issue_id,
          scope: security,
          boundary: selected.after.as_ref().map(|c| c.boundary),
          after: selected.after.map(|c| c.after),
          limit: selected.limit,
          visibility,
        },
        query.association_id,
      )
      .await?;
    Ok(IssueCodeReviewObservationPage {
      association,
      items: page.items,
      availability,
      next_cursor: page.next.map(|after| {
        crate::opaque_cursor::encode(&Cursor {
          scope,
          boundary: page.boundary,
          after,
        })
      }),
    })
  }
}
