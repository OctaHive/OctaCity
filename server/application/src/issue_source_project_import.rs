//! Reviewed import activation changes metadata only; consumers retain their existing Source journal.
use super::*;
use octacity_server_domain::IntegrationVersion;
use octacity_server_store::{
  IntegrationAdministrationStore, IntegrationConnectionStore, IntegrationProjectAccessStore, IssueSourceImportStore,
  PublishIssueSourceImport, ReplayIssueSourceImport,
};
pub use octacity_server_store::{
  IssueSourceImportDefinition, IssueSourceImportId, IssueSourceImportIntent, IssueSourceImportOutcome,
  IssueSourceImportPage, PublishedIssueSourceImport,
};

/// Publishes a reviewed Project import revision; it grants no Factory or push authority.
#[derive(Clone, Debug)]
pub struct PublishIssueSourceImportCommand {
  /// Exact reviewed selection, predicate and enablement.
  pub intent: IssueSourceImportIntent,
  /// Stable scoped mutation key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted acceptance time.
  pub published_at: Timestamp,
}
impl Command for PublishIssueSourceImportCommand {
  type Outcome = IssueSourceImportOutcome;
}
impl ManagementAuthorizationTarget for PublishIssueSourceImportCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
/// Exact or current retained import metadata, without provider calls.
#[derive(Clone, Debug)]
pub struct GetIssueSourceImportQuery {
  /// Visible owner.
  pub project_id: ProjectId,
  /// Stable import identity.
  pub id: IssueSourceImportId,
  /// Exact immutable revision; absence selects current.
  pub version: Option<IntegrationVersion>,
}
/// Bounded current import definitions, including disabled ones.
#[derive(Clone, Debug)]
pub struct ListIssueSourceImportsQuery {
  /// Visible owner.
  pub project_id: ProjectId,
  /// Exclusive stable position.
  pub after: Option<IssueSourceImportId>,
  /// Positive bounded page size.
  pub limit: u16,
}
impl Query for GetIssueSourceImportQuery {
  type Outcome = PublishedIssueSourceImport;
}
impl Query for ListIssueSourceImportsQuery {
  type Outcome = IssueSourceImportPage;
}
impl ManagementAuthorizationTarget for GetIssueSourceImportQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
impl ManagementAuthorizationTarget for ListIssueSourceImportsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueSourceImportQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
impl ManagementVisibilityTarget for ListIssueSourceImportsQuery {
  type Visibility = ProjectListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Project];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListProjectsQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}
fn visible_import_project(grant: &ManagementAuthorizationGrant, project: ProjectId) -> Result<(), ApplicationError> {
  if !grant
    .visibility_for::<ListProjectsQuery>()
    .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
    .allows(&project)
  {
    return Err(
      octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Project,
      }
      .into(),
    );
  }
  Ok(())
}
impl<S> IssueSourceHandlers<S> {
  async fn visible_import_connection(
    &self,
    context: &ManagementRequestContext,
    configuration: &PublishedIssueSourceImport,
  ) -> Result<(), ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::IntegrationConnection,
      configuration.definition.connection.id,
    )
    .await
  }
}
#[async_trait::async_trait]
impl<
  S: IssueSourceImportStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementCommandUseCase<PublishIssueSourceImportCommand> for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishIssueSourceImportCommand,
  ) -> Result<IssueSourceImportOutcome, ApplicationError> {
    visible_import_project(grant, command.intent.project_id)?;
    if let Some(outcome) = self
      .store
      .replay_issue_source_import(crate::management_security::audited_mutation(
        context,
        ReplayIssueSourceImport {
          intent: command.intent.clone(),
          idempotency_key: command.idempotency_key.clone(),
        },
      )?)
      .await?
    {
      self.visible_import_connection(context, &outcome.configuration).await?;
      return Ok(outcome);
    }
    command.intent.definition.validate()?;
    let connection = &command.intent.definition.connection;
    if command.intent.definition.enabled {
      let project = command.intent.project_id;
      let resource = crate::management_security::owned_collection_resource(
        ManagementResourceKind::Issue,
        ManagementResourceKind::Project,
        project,
      )
      .map_err(|_| ApplicationError::invalid())?;
      let create = self
        .policy
        .authorize(context, ManagementAction::Create, &resource)
        .await
        .map_err(|_| ApplicationError::invalid())?;
      if !create
        .visibility_for::<ListProjectsQuery>()
        .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
        .allows(&project)
      {
        return Err(ApplicationError::capability_unavailable());
      }
      let grant = self
        .policy
        .authorize(
          context,
          ManagementAction::View,
          &crate::management_security::instance_resource(ManagementResourceKind::Project, project)
            .map_err(|_| ApplicationError::invalid())?,
        )
        .await
        .map_err(|_| ApplicationError::invalid())?;
      let choices = self
        .execute_management_query(
          context,
          &grant,
          ListProjectIssueSourceChoicesQuery {
            selection: ListWorkSourceInputChoicesQuery {
              project_id: project,
              connection_id: connection.id,
              connection_version: connection.version,
            },
          },
        )
        .await?;
      if choices.access_version != connection.access_version {
        return Err(ApplicationError::PreconditionFailed);
      }
      let choice = choices
        .items
        .iter()
        .find(|c| {
          c.binding == command.intent.definition.source && c.connection_configuration == connection.source_configuration
        })
        .ok_or_else(|| FactoryValidationDiagnostics::unavailable("/source"))?;
      if let Some(predicate) = &command.intent.definition.predicate {
        let states = choice
          .predicate_states
          .as_ref()
          .ok_or_else(|| FactoryValidationDiagnostics::invalid("/predicate"))?;
        if predicate.state.as_ref().is_some_and(|s| !states.contains(s)) {
          return Err(FactoryValidationDiagnostics::invalid("/predicate/state"));
        }
      }
    }
    let publication = PublishIssueSourceImport {
      intent: command.intent,
      idempotency_key: command.idempotency_key,
      published_at: command.published_at,
    };
    let configuration = publication.resolve()?;
    self.visible_import_connection(context, &configuration).await?;
    Ok(
      self
        .store
        .publish_issue_source_import(crate::management_security::audited_mutation(context, publication)?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceImportStore + 'static> ManagementQueryUseCase<GetIssueSourceImportQuery> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueSourceImportQuery,
  ) -> Result<PublishedIssueSourceImport, ApplicationError> {
    visible_import_project(grant, query.project_id)?;
    let configuration = self
      .store
      .issue_source_import(
        query.project_id,
        &context.security_scope().to_store(),
        query.id,
        query.version,
      )
      .await?;
    self.visible_import_connection(context, &configuration).await?;
    Ok(configuration)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceImportStore + 'static> ManagementQueryUseCase<ListIssueSourceImportsQuery>
  for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueSourceImportsQuery,
  ) -> Result<IssueSourceImportPage, ApplicationError> {
    if !grant
      .visibility_for::<ListIssueSourceImportsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
      .allows(&query.project_id)
    {
      return Err(ApplicationError::invalid());
    }
    let visibility = self
      .policy
      .authorize(
        context,
        ManagementAction::View,
        &ManagementResource::collection(ManagementResourceKind::IntegrationConnection)
          .map_err(|_| ApplicationError::invalid())?,
      )
      .await
      .map_err(|_| ApplicationError::invalid())?
      .visibility_for::<ListIntegrationConnectionsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let page = self
      .store
      .list_issue_source_imports(
        query.project_id,
        &context.security_scope().to_store(),
        visibility,
        query.after,
        query.limit,
      )
      .await?;
    Ok(page)
  }
}
