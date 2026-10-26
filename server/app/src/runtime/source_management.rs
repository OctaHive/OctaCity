//! Adapts one deployment inventory to existing per-Project Source use cases.
//! Resolving a Project is pure; current connection/grant checks remain in application handlers.
use super::source_profiles_file::SourceProfiles;
use octacity_server_application::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{WorkSourceCapability, WorkSourceCapabilityRole};
use octacity_server_store_postgres::PostgresStore;
use octacity_server_work_source::WorkAdapterRegistry;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) struct SourceManagement {
  pub(super) store: Arc<PostgresStore>,
  pub(super) policy: Arc<dyn ManagementAuthorizationPolicy>,
  pub(super) profiles: SourceProfiles,
  pub(super) registry: Arc<WorkAdapterRegistry>,
  pub(super) timeout: Duration,
  pub(super) grace: Duration,
  pub(super) cancellation: CancellationToken,
}
impl SourceManagement {
  /// Pair a task reader only with independently installed delivery on the same native parent.
  /// The returned access pin grants nothing; mapping choices/publication recheck TaskUpdate.
  fn issue_writer(
    &self,
    project: ProjectId,
    reader: &super::source_profiles_file::SourceReadDeclaration,
    connection: &octacity_server_factory::WorkSourceConnectionRef,
  ) -> Result<Option<InstalledIssueSourceWriter>, ApplicationError> {
    let mut compatible = self.profiles.reporters.iter().filter(|r| {
      r.issue_delivery.is_some()
        && r.write.installation == reader.read.installation
        && r.write.plugin == reader.read.plugin
        && r.write.resource_reference == reader.read.resource_reference
        && r.write.resource.resource_id == reader.read.resource.resource_id
    });
    let Some(reporter) = compatible.next() else {
      return Ok(None);
    };
    if compatible.next().is_some() {
      return Err(ApplicationError::capability_unavailable());
    }
    let profile = reporter.profile(project)?;
    let binding = profile.binding().clone();
    let pin = octacity_server_store::IssueSourceWriterAccess {
      connection_id: connection.id,
      connection_version: connection.version,
      configuration: Some(reporter.write.capability.configuration.clone()),
    };
    Ok(Some(InstalledIssueSourceWriter {
      access: ManagedIntegrationConnections::new(self.store.clone()).for_issue_writer(&pin, &binding),
      binding,
      plugin: reporter.write.plugin.clone(),
      effects: reporter
        .reporting
        .as_ref()
        .and_then(|r| r.editable.clone())
        .ok_or_else(ApplicationError::capability_unavailable)?,
    }))
  }
  /// One inventory supplies the exact Project-scoped writer settings to authoring and dispatch.
  pub(super) fn reporter_profiles(
    &self,
    project: ProjectId,
  ) -> Result<Vec<super::FactoryReporterProfile>, ApplicationError> {
    self.profiles.reporters.iter().map(|r| r.profile(project)).collect()
  }
  pub(super) fn reporter_projector(
    &self,
    project: ProjectId,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<super::HostedFactoryWorkStatusProjector<PostgresStore>, ApplicationError> {
    Ok(
      super::HostedFactoryWorkStatusProjector::new(
        self.store.clone(),
        self.registry.clone(),
        self.reporter_profiles(project)?,
        clock,
        self.timeout,
        self.grace,
        self.cancellation.clone(),
      )?
      .with_managed_connections(ManagedIntegrationConnections::new(self.store.clone())),
    )
  }
  fn handlers(
    &self,
    project: ProjectId,
    profiles: Vec<(
      &super::source_profiles_file::SourceReadDeclaration,
      super::WorkSourceReadProfile,
    )>,
  ) -> Result<FactorySourceConfigurationHandlers<PostgresStore>, ApplicationError> {
    let (declarations, reads): (Vec<_>, Vec<_>) = profiles.into_iter().unzip();
    let reporters = self.reporter_profiles(project)?;
    let capability = |role, binding| WorkSourceCapability {
      project_id: project,
      role,
      binding,
    };
    let sources: Vec<_> = reads
      .iter()
      .map(|r| capability(WorkSourceCapabilityRole::Source, r.binding.clone()))
      .collect();
    let writers: Vec<_> = reporters
      .iter()
      .map(|r| capability(WorkSourceCapabilityRole::Reporter, r.binding().clone()))
      .collect();
    let mut handler = FactorySourceConfigurationHandlers::new(
      self.store.clone(),
      self.policy.clone(),
      sources.iter().chain(&writers).cloned().collect(),
    )?;
    for (reader, source) in declarations.into_iter().zip(sources) {
      handler = handler.with_connection_configuration(source.clone(), reader.read.capability.configuration.clone())?;
      if let Some(selection) = &reader.selection_labels {
        let state = selection.state.clone().ok_or_else(ApplicationError::invalid)?;
        handler = handler.with_selection_labels(
          source.clone(),
          WorkSourceSelectionLabels {
            state: state.clone(),
            all_of: selection.all_of.clone(),
            any_of: selection.any_of.clone(),
            none_of: selection.none_of.clone(),
          },
        )?;
        if let Some(states) = &reader.predicate_states {
          handler = handler.with_predicate_states(source, state, states.clone())?;
        }
      }
    }
    for (reporter, writer) in self.profiles.reporters.iter().zip(writers) {
      handler =
        handler.with_connection_configuration(writer.clone(), reporter.write.capability.configuration.clone())?;
      if let Some(reporting) = &reporter.reporting {
        handler = handler.with_reporting_mapping(writer, reporting.mapping.clone(), reporting.editable.clone())?;
      }
    }
    let preview = super::HostedFactoryWorkSourcePreview::new(
      self.registry.clone(),
      reads,
      self.timeout,
      self.grace,
      self.cancellation.clone(),
    )?
    .with_managed_connections(ManagedIntegrationConnections::new(self.store.clone()));
    Ok(
      handler
        .with_preview(Arc::new(preview))
        .with_issue_links(Arc::new(IssueHandlers::new(self.store.clone(), self.policy.clone()))),
    )
  }
}

impl FactorySourceConfigurationProvider<PostgresStore> for SourceManagement {
  fn for_project(
    &self,
    project: ProjectId,
    context: &ManagementRequestContext,
  ) -> Result<FactorySourceConfigurationHandlers<PostgresStore>, ApplicationError> {
    // Execution inputs can only select readers with actual preparation support.
    self.handlers(
      project,
      self
        .profiles
        .readers
        .iter()
        .filter(|r| r.preparation.is_some())
        .map(|r| Ok((r, r.profile(project, context.clone())?)))
        .collect::<Result<Vec<_>, ApplicationError>>()?,
    )
  }
}

/// Reuses the installed inventory for task discovery, including observation-only readers.
/// Only the Project task-choice query is exposed through this adapter.
pub(super) struct IssueSourceChoiceProvider(pub(super) Arc<SourceManagement>);
impl FactorySourceConfigurationProvider<PostgresStore> for IssueSourceChoiceProvider {
  fn for_project(
    &self,
    project: ProjectId,
    context: &ManagementRequestContext,
  ) -> Result<FactorySourceConfigurationHandlers<PostgresStore>, ApplicationError> {
    self.0.handlers(
      project,
      self
        .0
        .profiles
        .readers
        .iter()
        .map(|r| Ok((r, r.observation_profile(project, context.clone())?)))
        .collect::<Result<Vec<_>, ApplicationError>>()?,
    )
  }
}

impl SourceManagement {
  pub(super) async fn observation_profile(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: &octacity_server_factory::WorkSourceBinding,
    connection: &octacity_server_factory::WorkSourceConnectionRef,
    deadline: tokio::time::Instant,
  ) -> Result<super::WorkSourceReadProfile, ApplicationError> {
    self
      .observation_profiles(context, project, connection, deadline)
      .await?
      .into_iter()
      .find(|(_, profile)| profile.binding == *source)
      .map(|(_, profile)| profile)
      .ok_or_else(ApplicationError::capability_unavailable)
  }
  async fn observation_profiles(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    connection: &octacity_server_factory::WorkSourceConnectionRef,
    deadline: tokio::time::Instant,
  ) -> Result<
    Vec<(
      &super::source_profiles_file::SourceReadDeclaration,
      super::WorkSourceReadProfile,
    )>,
    ApplicationError,
  > {
    use octacity_server_store::IntegrationConnectionStore;
    let dependency = |class| ApplicationError::SourceDependency {
      class,
      retry_after_millis: None,
    };
    let metadata = tokio::select! {
      biased;
      () = self.cancellation.cancelled() => return Err(dependency(WorkSourceFailure::Cancelled)),
      result = tokio::time::timeout_at(deadline, self.store.integration_connection(connection.id, None)) =>
        result.map_err(|_| dependency(WorkSourceFailure::Timeout))??,
    };
    if metadata.version != connection.version {
      return Err(ApplicationError::capability_unavailable());
    }
    let mut selected = Vec::new();
    for declaration in &self.profiles.readers {
      if declaration.read.installation != metadata.definition.installation
        || declaration.read.plugin != metadata.definition.adapter
        || declaration.read.capability.configuration != connection.source_configuration
        || !metadata.definition.resources.iter().any(|r| {
          r.reference == declaration.read.resource_reference
            && r.native_id.as_str() == declaration.read.resource.resource_id
            && r.capabilities.contains(&declaration.read.capability)
        })
      {
        continue;
      }
      let mut profile = declaration.observation_profile(project, context.clone())?;
      profile.binding = project_issue_source_binding(profile.binding, project, connection.id, connection.version)?;
      selected.push((declaration, profile));
    }
    Ok(selected)
  }
  async fn observation_reader(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: &octacity_server_factory::WorkSourceBinding,
    connection: &octacity_server_factory::WorkSourceConnectionRef,
    deadline: tokio::time::Instant,
  ) -> Result<super::HostedWorkSourceObservations, ApplicationError> {
    let profile = self
      .observation_profile(context, project, source, connection, deadline)
      .await?;
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
      return Err(ApplicationError::SourceDependency {
        class: WorkSourceFailure::Timeout,
        retry_after_millis: None,
      });
    }
    Ok(
      super::HostedWorkSourceObservations::new(
        self.registry.clone(),
        vec![profile],
        Arc::new(SystemFactoryExecutionClock),
        remaining,
        self.grace,
        self.cancellation.clone(),
      )?
      .with_managed_connections(ManagedIntegrationConnections::new(self.store.clone())),
    )
  }
}
#[async_trait::async_trait]
impl WorkSourceObservations for SourceManagement {
  async fn authorize_connection(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: octacity_server_factory::WorkSourceBinding,
    connection: Option<&octacity_server_factory::WorkSourceConnectionRef>,
  ) -> Result<(), ApplicationError> {
    let connection = connection.ok_or_else(ApplicationError::capability_unavailable)?;
    self
      .observation_reader(
        context,
        project,
        &source,
        connection,
        tokio::time::Instant::now() + self.timeout,
      )
      .await?
      .authorize_connection(context, project, source, Some(connection))
      .await
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let connection = request
      .connection
      .as_ref()
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let reader = self
      .observation_reader(
        context,
        request.project_id,
        &request.source,
        connection,
        tokio::time::Instant::now() + self.timeout,
      )
      .await?;
    reader.read(context, request).await
  }
}

#[async_trait::async_trait]
impl IssueSourceProfileProvider for IssueSourceChoiceProvider {
  async fn refresh_policy(
    &self,
    context: &ManagementRequestContext,
    selection: &IssueSourceSelection,
  ) -> Result<IssueSourceRefreshPolicy, ApplicationError> {
    let connection = selection
      .connection
      .as_ref()
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let profile = self
      .0
      .observation_profile(
        context,
        selection.project_id,
        &selection.source,
        connection,
        tokio::time::Instant::now() + self.0.timeout,
      )
      .await?;
    Ok(IssueSourceRefreshPolicy {
      read: self.0.profiles.consumer.read_policy(profile.polling)?,
      claim_lifetime: self.0.profiles.consumer.worker().claim_lifetime(),
    })
  }
  async fn for_connection(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    connection: &octacity_server_factory::WorkSourceConnectionRef,
  ) -> Result<Vec<InstalledIssueSourceMapping>, ApplicationError> {
    let mut mappings = Vec::new();
    for (declaration, profile) in self
      .0
      .observation_profiles(
        context,
        project,
        connection,
        tokio::time::Instant::now() + self.0.timeout,
      )
      .await?
    {
      mappings.push(InstalledIssueSourceMapping {
        project_id: project,
        source: profile.binding,
        native_resource: octacity_server_factory::ExternalWorkIdentity::new(profile.resource.resource_id)
          .map_err(|_| ApplicationError::invalid())?,
        readable: declaration.readable.clone(),
        writer: self.0.issue_writer(project, declaration, connection)?,
      });
    }
    Ok(mappings)
  }
}
/// Advertised Source intent remains unavailable, without falling back to legacy profiles.
pub(super) struct UnavailableIssueSourceProfiles;
#[async_trait::async_trait]
impl IssueSourceProfileProvider for UnavailableIssueSourceProfiles {
  async fn refresh_policy(
    &self,
    _: &ManagementRequestContext,
    _: &IssueSourceSelection,
  ) -> Result<IssueSourceRefreshPolicy, ApplicationError> {
    Err(ApplicationError::unavailable())
  }
  async fn for_connection(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: &octacity_server_factory::WorkSourceConnectionRef,
  ) -> Result<Vec<InstalledIssueSourceMapping>, ApplicationError> {
    Err(ApplicationError::unavailable())
  }
}
