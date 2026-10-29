//! Stage authoring reuses installed Source choices and current independent Project grants.
use super::*;

#[async_trait]
impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> FactoryStageSourceAuthorizer for FactorySourceConfigurationHandlers<S>
{
  async fn validate_source_update(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    update: &FactoryStageSourceUpdate,
  ) -> Result<(), ApplicationError> {
    update.validate()?;
    let connection = update.connection.as_ref().ok_or_else(ApplicationError::invalid)?;
    let query = ListWorkSourceInputChoicesQuery {
      project_id: project,
      connection_id: connection.id,
      connection_version: connection.version,
    };
    let resource = query.management_resource().map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, ManagementAction::View, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    let choices = self.execute_management_query(context, &grant, query).await?;
    validate_update_choices(update, &choices)
  }
}

// The same metadata-only authority can be composed by preset and ordinary Flow publication.
#[async_trait]
impl<H: ManagementQueryUseCase<ListWorkSourceInputChoicesQuery, Error = ApplicationError> + 'static>
  FactoryStageSourceAuthorizer for AuthorizedQueryHandler<H>
{
  async fn validate_source_update(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    update: &FactoryStageSourceUpdate,
  ) -> Result<(), ApplicationError> {
    update.validate()?;
    let connection = update.connection.as_ref().ok_or_else(ApplicationError::invalid)?;
    let choices = self
      .handle_query(
        context,
        ListWorkSourceInputChoicesQuery {
          project_id: project,
          connection_id: connection.id,
          connection_version: connection.version,
        },
      )
      .await
      .map_err(|_| ApplicationError::capability_unavailable())?;
    validate_update_choices(update, &choices)
  }
}
fn validate_update_choices(
  update: &FactoryStageSourceUpdate,
  choices: &WorkSourceInputChoices,
) -> Result<(), ApplicationError> {
  let connection = update.connection.as_ref().ok_or_else(ApplicationError::invalid)?;
  if choices.access_version != connection.access_version {
    return Err(ApplicationError::PreconditionFailed);
  }
  let source = choices
    .items
    .iter()
    .find(|c| c.purpose == IntegrationPurpose::TaskRead && c.binding == update.source);
  let writer = choices
    .items
    .iter()
    .find(|c| c.purpose == IntegrationPurpose::TaskUpdate && c.binding == update.reporter);
  let (Some(source), Some(writer)) = (source, writer) else {
    return Err(ApplicationError::capability_unavailable());
  };
  if source.connection_configuration != connection.source_configuration
    || Some(&writer.connection_configuration) != connection.reporter_configuration.as_ref()
  {
    return Err(ApplicationError::invalid());
  }
  let policy = writer
    .reporting
    .as_ref()
    .and_then(|r| r.editable.as_ref())
    .ok_or_else(ApplicationError::capability_unavailable)?;
  update.validate_policy(policy)
}
