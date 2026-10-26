//! Safe installed input choices; repository selection uses the existing independent Repository query.
use super::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId};
pub use octacity_server_factory::{
  MAX_SOURCE_REPORTING_CHOICES, MAX_SOURCE_REPORTING_EFFECT_LABELS, MAX_SOURCE_REPORTING_STATUSES,
  MAX_SOURCE_REPORTING_TEXT_BYTES, MAX_WORK_SOURCE_PREDICATE_LABEL_BYTES as MAX_SOURCE_SELECTION_LABEL_BYTES,
  MAX_WORK_SOURCE_PREDICATE_LABELS as MAX_SOURCE_SELECTION_LABELS,
};
use serde::Serialize;

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
  /// Exact installed effect presentation; absence means unknown, never editable.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub reporting: Option<WorkSourceReportingChoice>,
  /// Explicit editable normalized states over an unrestricted installed read profile.
  /// Absence keeps legacy installed-only selection; clients cannot infer editing support.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub predicate_states: Option<Vec<FactoryKey>>,
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
/// Adapter-validated installed effects and optional explicit editing ceiling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceReportingChoice {
  /// Existing immutable default mapping, with no privileged status names.
  pub mapping: WorkSourceStatusMapping,
  /// None keeps installed-only selection; read-only checks do not imply write readiness.
  pub editable: Option<WorkSourceReportingPolicy>,
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
  /// Attaches bounded presentation verified by the trusted installed Reporter owner.
  /// This does not install a writer or grant any Project/native resource authority.
  pub fn with_reporting_mapping(
    mut self,
    capability: WorkSourceCapability,
    mapping: WorkSourceStatusMapping,
    editable: Option<WorkSourceReportingPolicy>,
  ) -> Result<Self, ApplicationError> {
    mapping.validate().map_err(|_| ApplicationError::invalid())?;
    if let Some(policy) = &editable {
      policy
        .validate_mapping(&mapping)
        .map_err(|_| ApplicationError::invalid())?;
    }
    if capability.role != WorkSourceCapabilityRole::Reporter
      || !self.capabilities.contains(&capability)
      || self.reporting_mappings.iter().any(|(c, _)| c == &capability)
    {
      return Err(ApplicationError::invalid());
    }
    self
      .reporting_mappings
      .push((capability, WorkSourceReportingChoice { mapping, editable }));
    Ok(self)
  }
  /// Declares normalized narrowing support over an adapter-owned unrestricted base.
  /// The trusted installation owner supplies its native unrestricted state key and permitted states.
  /// This cannot change transport, credentials, resources, mapping or network authority.
  pub fn with_predicate_states(
    mut self,
    capability: WorkSourceCapability,
    unrestricted: FactoryKey,
    states: Vec<FactoryKey>,
  ) -> Result<Self, ApplicationError> {
    let base = self
      .selection_labels
      .iter()
      .find(|(c, _)| c == &capability)
      .map(|(_, s)| s)
      .ok_or_else(ApplicationError::invalid)?;
    if capability.role != WorkSourceCapabilityRole::Source
      || base.state != unrestricted
      || !base.all_of.is_empty()
      || !base.any_of.is_empty()
      || !base.none_of.is_empty()
      || states.len() > MAX_SOURCE_SELECTION_LABELS
      || states.iter().collect::<std::collections::BTreeSet<_>>().len() != states.len()
      || self.predicate_states.iter().any(|(c, _)| c == &capability)
    {
      return Err(ApplicationError::invalid());
    }
    self.predicate_states.push((capability, states));
    Ok(self)
  }

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
impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore>
  ManagementQueryUseCase<ListWorkSourceInputChoicesQuery> for FactorySourceConfigurationHandlers<S>
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
        reporting: self
          .reporting_mappings
          .iter()
          .find(|(c, _)| c == capability)
          .map(|(_, choice)| choice.clone()),
        predicate_states: self
          .predicate_states
          .iter()
          .find(|(c, _)| c == capability)
          .map(|(_, states)| states.clone()),
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
