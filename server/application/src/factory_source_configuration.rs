//! Automatic input authoring selects installed settings; it creates no task or execution.
use crate::management_security::instance_resource;
use crate::*;
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::*;
pub use octacity_server_store::{
  MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE, PublishWorkSources, WorkSourceConfigurationIntent,
  WorkSourceConfigurationOutcome, WorkSourceConfigurationPage, WorkSourceConfigurationRecord, WorkSourceGroupIntent,
  WorkSourceInputIntent, WorkSourceMemberRef, WorkSourceScanTarget,
};
use std::sync::Arc;
#[path = "factory_source_actions.rs"]
mod actions;
#[path = "factory_source_installation.rs"]
mod installation;
pub use installation::*;
#[path = "factory_source_configuration_choices.rs"]
mod choices;
pub use choices::*;
#[path = "factory_source_configuration_reads.rs"]
mod reads;
pub use reads::*;
#[path = "factory_source_preview.rs"]
mod preview;
pub use preview::*;

/// Non-mutating validation of an exact, credential-free publication intent.
#[derive(Clone, Debug)]
pub struct ValidateWorkSourceConfigurationQuery {
  /// Current selections to resolve and validate without starting discovery.
  pub intent: WorkSourceConfigurationIntent,
}
/// Publishes reviewed automatic inputs; it grants no integration or Issue authority.
#[derive(Clone, Debug)]
pub struct PublishWorkSourceConfigurationCommand {
  /// Original immutable selections and current-version preconditions.
  pub intent: WorkSourceConfigurationIntent,
  /// Scoped stable mutation key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first acceptance time.
  pub published_at: octacity_server_domain::Timestamp,
}
impl Command for PublishWorkSourceConfigurationCommand {
  type Outcome = WorkSourceConfigurationOutcome;
}
impl ManagementAuthorizationTarget for PublishWorkSourceConfigurationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
impl Query for ValidateWorkSourceConfigurationQuery {
  type Outcome = PublishWorkSources;
}
impl ManagementAuthorizationTarget for ValidateWorkSourceConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
struct InstalledSourcePredicate {
  capability: WorkSourceCapability,
  states: Vec<FactoryKey>,
  label_presence: bool,
}

/// Trusted installed Source/Reporter choices, independent of provider names and private files.
pub struct FactorySourceConfigurationHandlers<S> {
  store: Arc<S>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  capabilities: Vec<WorkSourceCapability>,
  configuration_parents: Vec<(WorkSourceCapability, ImmutableReference)>,
  selection_labels: Vec<(WorkSourceCapability, WorkSourceSelectionLabels)>,
  predicate_support: Vec<InstalledSourcePredicate>,
  reporting_mappings: Vec<(WorkSourceCapability, WorkSourceReportingChoice)>,
  preparation_mappings: Vec<(WorkSourceCapability, SourceAcceptanceMapping)>,
  preview: Option<Arc<dyn FactoryWorkSourcePreview>>,
  issue_links: Option<Arc<dyn FactoryWorkSourceIssueLinks>>,
}
impl<S> FactorySourceConfigurationHandlers<S> {
  /// Installs exact reviewed profiles; ambiguous declarations fail closed.
  pub fn new(
    store: Arc<S>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    capabilities: Vec<WorkSourceCapability>,
  ) -> Result<Self, ApplicationError> {
    if capabilities.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || capabilities.iter().enumerate().any(|(i, c)| {
        capabilities[..i]
          .iter()
          .any(|p| p.project_id == c.project_id && p.role == c.role && p.binding == c.binding)
      })
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      store,
      policy,
      capabilities,
      configuration_parents: vec![],
      selection_labels: vec![],
      predicate_support: vec![],
      reporting_mappings: vec![],
      preparation_mappings: vec![],
      preview: None,
      issue_links: None,
    })
  }
  /// Records a trusted installed input profile derived from an exact connection profile.
  /// Clients cannot declare this relationship or turn arbitrary settings into an installed capability.
  pub fn with_connection_configuration(
    mut self,
    capability: WorkSourceCapability,
    connection_configuration: ImmutableReference,
  ) -> Result<Self, ApplicationError> {
    if !self.capabilities.contains(&capability) || self.configuration_parents.iter().any(|(c, _)| *c == capability) {
      return Err(ApplicationError::invalid());
    }
    self.configuration_parents.push((capability, connection_configuration));
    Ok(self)
  }
  fn connection_configuration<'a>(&'a self, capability: &'a WorkSourceCapability) -> &'a ImmutableReference {
    self
      .configuration_parents
      .iter()
      .find(|(c, _)| c == capability)
      .map_or(&capability.binding.configuration, |(_, configuration)| configuration)
  }
  async fn require(
    &self,
    context: &ManagementRequestContext,
    kind: ManagementResourceKind,
    id: impl ToString,
    action: ManagementAction,
  ) -> Result<(), ApplicationError> {
    let resource = instance_resource(kind, id.to_string()).map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, action, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    match grant.visibility().view() {
      ManagementVisibilityView::All => Ok(()),
      ManagementVisibilityView::Restricted(resources) if resources.contains(&resource) => Ok(()),
      _ => Err(ApplicationError::invalid()),
    }
  }
}
impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> FactorySourceConfigurationHandlers<S>
{
  async fn require_input_visibility(
    &self,
    context: &ManagementRequestContext,
    definition: &WorkSourceInputDraft,
  ) -> Result<(), ApplicationError> {
    if let Some(connection) = &definition.connection {
      self
        .require(
          context,
          ManagementResourceKind::IntegrationConnection,
          connection.id,
          ManagementAction::View,
        )
        .await?;
    }
    self
      .require(
        context,
        ManagementResourceKind::FactoryConfiguration,
        definition.target.id(),
        ManagementAction::View,
      )
      .await?;
    self
      .require(
        context,
        ManagementResourceKind::Repository,
        definition.repository.id,
        ManagementAction::View,
      )
      .await?;
    Ok(())
  }
  async fn require_input_use(
    &self,
    context: &ManagementRequestContext,
    definition: &WorkSourceInputDraft,
  ) -> Result<(), ApplicationError> {
    self.require_input_visibility(context, definition).await?;
    if let Some(mapping) = &definition.reporting {
      let policy = self
        .reporting_mappings
        .iter()
        .find(|(c, _)| {
          c.project_id == definition.target.project_id() && Some(&c.binding) == definition.reporter.as_ref()
        })
        .and_then(|(_, choice)| choice.editable.as_ref());
      choices::validate_source_reporting(mapping, policy)?;
    }
    if let Some(predicate) = &definition.predicate {
      let support = self.predicate_support.iter().find(|s| {
        s.capability.project_id == definition.target.project_id() && s.capability.binding == definition.source
      });
      choices::validate_source_predicate(
        predicate,
        support.map(|s| s.states.as_slice()),
        support.is_some_and(|s| s.label_presence),
      )?;
    }
    let connection = definition
      .connection
      .as_ref()
      .ok_or_else(|| FactoryValidationDiagnostics::invalid("/connection"))?;
    self
      .require(
        context,
        ManagementResourceKind::IntegrationConnection,
        connection.id,
        ManagementAction::Execute,
      )
      .await
  }
  async fn resolve_input(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    intent: &WorkSourceInputIntent,
  ) -> Result<WorkSourceInput, ApplicationError> {
    let definition = &intent.definition;
    self.require_input_use(context, definition).await?;
    let connection = definition
      .connection
      .as_ref()
      .ok_or_else(|| FactoryValidationDiagnostics::invalid("/connection"))?;
    let grant = self
      .store
      .integration_project_access(connection.id, project)
      .await?
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if grant.version != connection.access_version {
      return Err(ApplicationError::PreconditionFailed);
    }
    let metadata = self.store.integration_connection(connection.id, None).await?;
    if metadata.version != connection.version {
      return Err(ApplicationError::PreconditionFailed);
    }
    if definition.reporter.as_ref().is_some_and(|reporter| {
      !grant.resources.iter().any(|resource| {
        resource.reference == reporter.resource && resource.purposes.contains(&IntegrationPurpose::TaskUpdate)
      })
    }) {
      return Err(FactoryValidationDiagnostics::unavailable("/reporter"));
    }
    let administration = self.store.integration_administration(connection.id).await?;
    let selected = self
      .store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: definition.target.id(),
        configuration_version: definition.target.version(),
        repository_id: Some(definition.repository.id),

        repository_version: Some(definition.repository.version),
      })
      .await?;
    let selected_repository = selected.repository.as_ref().ok_or_else(ApplicationError::invalid)?;
    let repository = WorkSourceRepositoryRef {
      project_id: selected_repository.project_id,
      id: selected_repository.id,
      version: selected_repository.version,
      definition_digest: FactoryDigest::content_sha256(
        &serde_json::to_vec(&selected_repository.definition).map_err(|_| ApplicationError::invalid())?,
      ),
    };
    let find = |binding: &WorkSourceBinding, role| {
      self
        .capabilities
        .iter()
        .find(|c| c.project_id == project && c.role == role && c.binding == *binding)
    };
    let source = find(&definition.source, WorkSourceCapabilityRole::Source)
      .ok_or_else(|| FactoryValidationDiagnostics::unavailable("/source"))?;
    let reporter = definition
      .reporter
      .as_ref()
      .and_then(|r| find(r, WorkSourceCapabilityRole::Reporter));
    if self.connection_configuration(source) != &connection.source_configuration
      || reporter.map(|r| self.connection_configuration(r)) != connection.reporter_configuration.as_ref()
    {
      return Err(FactoryValidationDiagnostics::unavailable("/connection"));
    }
    let publication = WorkSourceInputContext {
      target: &selected.configuration.configuration,
      repository: &repository,
      source: Some(source),
      reporter,
    };
    let input = if let Some(version) = intent.expected_version {
      let prior = self.store.source_input(project, intent.id, version).await?;
      prior.replace(
        WorkSourceInputVersion::new(version.get().checked_add(1).ok_or_else(ApplicationError::invalid)?)
          .map_err(|_| ApplicationError::invalid())?,
        definition.clone(),
        publication,
      )
    } else {
      WorkSourceInput::publish(intent.id, project, definition.clone(), publication)
    }
    .map_err(|error| {
      let field = match error {
        FactoryError::InvalidConfiguration {
          field: "source input target" | "source input enabled target",
        } => "/target",
        FactoryError::InvalidConfiguration {
          field: "source input repository",
        } => "/repository",
        FactoryError::InvalidConfiguration {
          field: "source input reporter",
        } => "/reporter",
        _ => "/source",
      };
      FactoryValidationDiagnostics::invalid(field)
    })?;
    validate_managed_work_source(&input, &metadata, &grant, &administration)?;
    validate_source_input_dependencies(&input, &selected.configuration.configuration, selected_repository)?;
    // This intake cannot supply a reviewed Issue revision/association. Disabled drafts remain repairable.
    if input.definition().enabled
      && crate::factory_issue_status_authoring::flow_issue_status_selections(
        selected.configuration.configuration.flow(),
      )
      .next()
      .is_some()
    {
      return Err(FactoryValidationDiagnostics::invalid("/target"));
    }
    crate::factory_stage_source_update::validate_input_source_updates(
      input.definition(),
      selected.configuration.configuration.flow(),
    )
    .map_err(|_| FactoryValidationDiagnostics::invalid("/target"))?;
    Ok(input)
  }
  async fn resolve(
    &self,
    context: &ManagementRequestContext,
    intent: &WorkSourceConfigurationIntent,
  ) -> Result<PublishWorkSources, ApplicationError> {
    if intent.inputs.len() > 64 || (intent.inputs.is_empty() && intent.group.is_none()) {
      return Err(ApplicationError::invalid());
    }
    self
      .require(
        context,
        ManagementResourceKind::Project,
        intent.project_id,
        ManagementAction::View,
      )
      .await?;
    let mut inputs = vec![];
    for (index, input) in intent.inputs.iter().enumerate() {
      let resolved = self
        .resolve_input(context, intent.project_id, input)
        .await
        .map_err(|mut error| {
          if let ApplicationError::FactoryValidation(report) = &mut error {
            for issue in &mut report.errors {
              issue.field = format!("/inputs/{index}/definition{}", issue.field);
            }
            for field in &mut report.unavailable_references {
              *field = format!("/inputs/{index}/definition{field}");
            }
          }
          error
        })?;
      inputs.push(WorkSourceInputChange {
        expected_version: input.expected_version,
        input: resolved,
      });
    }
    let group = if let Some(group) = &intent.group {
      if group.members.is_empty() || group.members.len() > MAX_WORK_SOURCE_GROUP_MEMBERS {
        return Err(FactoryValidationDiagnostics::invalid("/group/members"));
      }
      let mut members = vec![];
      for member in &group.members {
        let input = if let Some(change) = inputs.iter().find(|change| change.input.id() == member.id) {
          if change.input.version() != member.version {
            return Err(ApplicationError::PreconditionFailed);
          }
          change.input.clone()
        } else {
          let input = self
            .store
            .source_input(intent.project_id, member.id, member.version)
            .await?;
          self
            .resolve_input(
              context,
              intent.project_id,
              &WorkSourceInputIntent {
                id: input.id(),
                expected_version: None,
                definition: input.definition().clone(),
              },
            )
            .await?;
          input
        };
        members.push(input);
      }
      let resolved = if let Some(version) = group.expected_version {
        let prior = self.store.source_group(intent.project_id, group.id, version).await?;
        prior.replace(
          WorkSourceGroupVersion::new(version.get().checked_add(1).ok_or_else(ApplicationError::invalid)?)
            .map_err(|_| ApplicationError::invalid())?,
          members,
        )
      } else {
        WorkSourceGroup::publish(group.id, intent.project_id, members)
      }
      .map_err(FactoryValidationDiagnostics::from_factory_error)?;
      Some(WorkSourceGroupChange {
        expected_version: group.expected_version,
        group: resolved,
      })
    } else {
      None
    };
    let resolved = PublishWorkSources {
      project_id: intent.project_id,
      inputs,
      group,
    };
    resolved.validate()?;
    Ok(resolved)
  }
}
#[async_trait]
impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> ManagementQueryUseCase<ValidateWorkSourceConfigurationQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ValidateWorkSourceConfigurationQuery,
  ) -> Result<PublishWorkSources, ApplicationError> {
    self.resolve(context, &query.intent).await
  }
}
#[async_trait]
impl<
  S: FactorySourceConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> ManagementCommandUseCase<PublishWorkSourceConfigurationCommand> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishWorkSourceConfigurationCommand,
  ) -> Result<WorkSourceConfigurationOutcome, ApplicationError> {
    self
      .require(
        context,
        ManagementResourceKind::Project,
        command.intent.project_id,
        ManagementAction::View,
      )
      .await?;
    if let Some(outcome) = self
      .store
      .replay_work_source_configuration(crate::management_security::audited_mutation(
        context,
        ReplayWorkSourceConfiguration {
          intent: command.intent.clone(),
          idempotency_key: command.idempotency_key.clone(),
        },
      )?)
      .await?
    {
      for input in outcome
        .publication
        .inputs
        .iter()
        .map(|c| &c.input)
        .chain(outcome.publication.group.iter().flat_map(|c| c.group.members()))
      {
        self.require_input_use(context, input.definition()).await?;
      }
      return Ok(outcome);
    }
    let publication = self.resolve(context, &command.intent).await?;
    Ok(
      self
        .store
        .publish_work_source_configuration(crate::management_security::audited_mutation(
          context,
          PublishWorkSourceConfiguration {
            intent: command.intent,
            publication,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}
#[path = "factory_source_diagnostics.rs"]
mod diagnostics;
pub use diagnostics::*;
