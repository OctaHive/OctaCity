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
impl FactorySourceConfigurationProvider<PostgresStore> for SourceManagement {
  fn for_project(
    &self,
    project: ProjectId,
    context: &ManagementRequestContext,
  ) -> Result<FactorySourceConfigurationHandlers<PostgresStore>, ApplicationError> {
    // These are execution-input choices. Source-only readers remain installed for
    // task consumers, but cannot promise preparation for a Factory input.
    let prepared: Vec<_> = self
      .profiles
      .readers
      .iter()
      .filter(|r| r.preparation.is_some())
      .collect();
    let reads = prepared
      .iter()
      .map(|r| r.profile(project, context.clone()))
      .collect::<Result<Vec<_>, _>>()?;
    let reporters = self
      .profiles
      .reporters
      .iter()
      .map(|r| r.profile(project))
      .collect::<Result<Vec<_>, _>>()?;
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
    for (reader, source) in prepared.into_iter().zip(sources) {
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
    Ok(handler.with_preview(Arc::new(preview)))
  }
}
