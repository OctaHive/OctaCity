//! Contextual installed profiles reuse ordinary authoring use cases and their authorization decorator.
use super::*;
use octacity_server_domain::ProjectId;
use std::sync::Arc;

/// Trusted composition port; browser metadata cannot install or alter a profile.
/// Each request resolves its actual Project, including Projects created after startup.
pub trait FactorySourceConfigurationProvider<S>: Send + Sync {
  /// Supplies the existing handlers with the immutable installed choices for this request.
  fn for_project(
    &self,
    project: ProjectId,
    context: &ManagementRequestContext,
  ) -> Result<FactorySourceConfigurationHandlers<S>, ApplicationError>;
}
/// Application dispatch over a trusted dynamic installation. Must be wrapped by
/// `AuthorizedQueryHandler` / `AuthorizedCommandHandler` at the composition root.
pub struct InstalledFactorySourceHandlers<S> {
  provider: Arc<dyn FactorySourceConfigurationProvider<S>>,
}
impl<S> InstalledFactorySourceHandlers<S> {
  /// Stores the trusted resolver without resolving a grant or contacting a provider.
  pub fn new(provider: Arc<dyn FactorySourceConfigurationProvider<S>>) -> Self {
    Self { provider }
  }
}

// All operations use the same immutable inventory. The actual Project comes from the
// authorized request, so a newly published Project/grant requires no server restart.
macro_rules! project_query {
  ($query:ty, $request:ident, $project:expr) => {
    #[async_trait::async_trait]
    impl<
      S: FactorySourceConfigurationStore
        + FactorySourceDiagnosticsStore
        + IntegrationProjectAccessStore
        + IntegrationConnectionStore
        + IntegrationAdministrationStore
        + 'static,
    > ManagementQueryUseCase<$query> for InstalledFactorySourceHandlers<S>
    {
      type Error = ApplicationError;
      async fn execute_management_query(
        &self,
        context: &ManagementRequestContext,
        grant: &ManagementAuthorizationGrant,
        $request: $query,
      ) -> Result<<$query as Query>::Outcome, ApplicationError> {
        self
          .provider
          .for_project($project, context)?
          .execute_management_query(context, grant, $request)
          .await
      }
    }
  };
}
project_query!(ListWorkSourceInputChoicesQuery, query, query.project_id);
project_query!(ValidateWorkSourceConfigurationQuery, query, query.intent.project_id);
project_query!(GetWorkSourceConfigurationQuery, query, query.project_id);
project_query!(ListWorkSourceConfigurationsQuery, query, query.project_id);
project_query!(PreviewWorkSourcesQuery, query, query.intent.project_id);
project_query!(GetWorkSourceDiagnosticsQuery, query, query.project_id);

#[async_trait::async_trait]
impl<
  S: FactorySourceConfigurationStore
    + FactorySourceDiagnosticsStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementCommandUseCase<PublishWorkSourceConfigurationCommand> for InstalledFactorySourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishWorkSourceConfigurationCommand,
  ) -> Result<<PublishWorkSourceConfigurationCommand as Command>::Outcome, ApplicationError> {
    self
      .provider
      .for_project(command.intent.project_id, context)?
      .execute_management_command(context, grant, command)
      .await
  }
}
