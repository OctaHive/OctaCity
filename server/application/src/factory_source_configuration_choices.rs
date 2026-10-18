//! Safe installed input choices; repository selection uses the existing independent Repository query.
use super::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId};
use serde::Serialize;
/// Presentation ceiling; an adapter can impose tighter selector limits.
pub const MAX_SOURCE_SELECTION_LABELS: usize = 64;
/// UTF-8 ceiling shared with normalized task labels.
pub const MAX_SOURCE_SELECTION_LABEL_BYTES: usize = 256;

/// Provider-normalized labels for an already validated immutable selector, never Flow instructions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceSelectionLabels {
  /// Opaque installed state key; no provider's state vocabulary is built in.
  pub state: FactoryKey,
  /// Required literal labels.
  pub all_of: Vec<String>,
  /// Literal alternatives.
  pub any_of: Vec<String>,
  /// Excluded literal labels.
  pub none_of: Vec<String>,
}
/// One usable installed profile; private role handles and credential references are absent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceInputChoice {
  /// Exact input selector/mapping profile.
  pub binding: WorkSourceBinding,
  /// Connection profile from which this input configuration was installed.
  pub connection_configuration: ImmutableReference,
  /// Independent task read or task update purpose.
  pub purpose: IntegrationPurpose,
  /// Stable native task container, independent of the code Repository.
  pub native_id: ExternalWorkIdentity,
  /// Safe human context from authorized metadata.
  pub locator: ExternalWorkIdentity,
  /// Optional provider-validated literal selector presentation.
  pub selection: Option<WorkSourceSelectionLabels>,
}
/// Current safe choices and reviewed version preconditions; this is not an execution grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceInputChoices {
  /// Selected reusable connection.
  pub connection_id: IntegrationId,
  /// Current exact metadata version.
  pub connection_version: IntegrationVersion,
  /// Current Project grant revision to review at publication.
  pub access_version: IntegrationVersion,
  /// Installed and currently granted profiles only.
  pub items: Vec<WorkSourceInputChoice>,
}
/// Bounded installed choice discovery, without provider reads, task import or input publication.
#[derive(Clone, Debug)]
pub struct ListWorkSourceInputChoicesQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Separately visible and usable connection.
  pub connection_id: IntegrationId,
  /// Exact reviewed metadata; stale selections do not follow a replacement.
  pub connection_version: IntegrationVersion,
}
impl Query for ListWorkSourceInputChoicesQuery {
  type Outcome = WorkSourceInputChoices;
}
impl ManagementAuthorizationTarget for ListWorkSourceInputChoicesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
impl<S> FactorySourceConfigurationHandlers<S> {
  /// Attaches bounded presentation from the trusted owner of an exact, adapter-validated selector.
  /// It cannot install another profile, change executable settings or widen a grant.
  pub fn with_selection_labels(
    mut self,
    capability: WorkSourceCapability,
    selection: WorkSourceSelectionLabels,
  ) -> Result<Self, ApplicationError> {
    if capability.role != WorkSourceCapabilityRole::Source
      || !self.capabilities.contains(&capability)
      || self.selection_labels.iter().any(|(c, _)| c == &capability)
      || [&selection.all_of, &selection.any_of, &selection.none_of]
        .iter()
        .any(|labels| {
          labels.len() > MAX_SOURCE_SELECTION_LABELS
            || labels.iter().any(|label| {
              label.trim().is_empty()
                || label.len() > MAX_SOURCE_SELECTION_LABEL_BYTES
                || label.chars().any(char::is_control)
            })
        })
    {
      return Err(ApplicationError::invalid());
    }
    self.selection_labels.push((capability, selection));
    Ok(self)
  }
}
#[async_trait]
impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> ManagementQueryUseCase<ListWorkSourceInputChoicesQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListWorkSourceInputChoicesQuery,
  ) -> Result<WorkSourceInputChoices, ApplicationError> {
    if !grant
      .visibility_for::<ListWorkSourceInputChoicesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
      .allows(&query.project_id)
    {
      return Err(ApplicationError::invalid());
    }
    self
      .require(
        context,
        ManagementResourceKind::Project,
        query.project_id,
        ManagementAction::View,
      )
      .await?;
    self
      .require(
        context,
        ManagementResourceKind::IntegrationConnection,
        query.connection_id,
        ManagementAction::View,
      )
      .await?;
    self
      .require(
        context,
        ManagementResourceKind::IntegrationConnection,
        query.connection_id,
        ManagementAction::Execute,
      )
      .await?;
    let metadata = self.store.integration_connection(query.connection_id, None).await?;
    if metadata.version != query.connection_version {
      return Err(ApplicationError::PreconditionFailed);
    }
    let access = self
      .store
      .integration_project_access(query.connection_id, query.project_id)
      .await?
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let administration = self.store.integration_administration(query.connection_id).await?;
    if access.connection_version != query.connection_version
      || !administration.enabled
      || administration.active_version != Some(query.connection_version)
    {
      return Err(ApplicationError::capability_unavailable());
    }
    let mut items = vec![];
    for capability in self
      .capabilities
      .iter()
      .filter(|c| c.project_id == query.project_id && c.binding.installation == metadata.definition.installation)
    {
      let purpose = match capability.role {
        WorkSourceCapabilityRole::Source => IntegrationPurpose::TaskRead,
        WorkSourceCapabilityRole::Reporter => IntegrationPurpose::TaskUpdate,
      };
      let binding = &capability.binding;
      let Some(resource) = metadata
        .definition
        .resources
        .iter()
        .find(|r| r.reference == binding.resource)
      else {
        continue;
      };
      let Some(installed) = resource.capabilities.iter().find(|c| c.purpose == purpose) else {
        continue;
      };
      if installed.capability != binding.capability
        || installed.configuration != *self.connection_configuration(capability)
        || !access
          .resources
          .iter()
          .any(|r| r.reference == binding.resource && r.purposes.contains(&purpose))
        || !administration
          .credentials
          .iter()
          .any(|c| c.reference == installed.credential && c.configured)
      {
        continue;
      }
      items.push(WorkSourceInputChoice {
        binding: binding.clone(),
        connection_configuration: installed.configuration.clone(),
        purpose,
        native_id: resource.native_id.clone(),
        locator: resource.locator.clone(),
        selection: self
          .selection_labels
          .iter()
          .find(|(c, _)| c == capability)
          .map(|(_, s)| s.clone()),
      });
    }
    Ok(WorkSourceInputChoices {
      connection_id: query.connection_id,
      connection_version: query.connection_version,
      access_version: access.version,
      items,
    })
  }
}

impl ManagementVisibilityTarget for ListWorkSourceInputChoicesQuery {
  type Visibility = ProjectListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Project];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListProjectsQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}
