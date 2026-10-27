//! Live managed input discovery reuses the installed Source inventory and existing polling/admission journals.
use super::{source_management::SourceManagement, work_source::*};
use octacity_server_application::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::WorkSourceInput;
use octacity_server_store::{FactorySourceStore, WorkSourceScanTarget};
use std::sync::Arc;

pub(super) struct ManagedFactorySource {
  sources: Arc<SourceManagement>,
  context: ManagementRequestContext,
  cursor: tokio::sync::Mutex<Cursor>,
}
#[derive(Default)]
struct Cursor {
  after_project: Option<ProjectId>,
  project: Option<ProjectId>,
  target: Option<WorkSourceScanTarget>,
}
impl ManagedFactorySource {
  pub(super) fn new(sources: Arc<SourceManagement>, principal: &str) -> Self {
    Self {
      sources,
      context: super::integration_management::request_context(principal),
      cursor: tokio::sync::Mutex::new(Cursor::default()),
    }
  }
  pub(super) fn worker<
    B: octacity_artifact_store::ArtifactBytePublisher + octacity_artifact_store::ArtifactByteReader + 'static,
  >(
    sources: Arc<SourceManagement>,
    bytes: Arc<B>,
    revisions: Arc<dyn RevisionResolver>,
    principal: &str,
  ) -> Result<Arc<dyn FactorySourceRuntime>, ApplicationError> {
    let consumer = sources.profiles.consumer;
    let worker = FactorySourceWorker::new(
      FactorySourceWorkerDependencies {
        store: sources.store.clone(),
        project_inputs: Some(Arc::new(FactoryProjectInputReader::new(
          sources.store.clone(),
          bytes.clone(),
        ))),
        bytes,
        revisions,
        authorization: sources.policy.clone(),
        registry: sources.registry.clone(),
        clock: Arc::new(SystemFactoryExecutionClock),
      },
      FactorySourceInstallation {
        projects: vec![],
        profiles: vec![],
        settings: FactorySourceWorkerSettings {
          worker: crate::FactoryRuntimeConfig::source_consumer(
            consumer.worker(),
            consumer.max_attempts,
            consumer.maximum_retry_millis,
          ),
          max_attempts: consumer.max_attempts,
          target_retry: Some(consumer.retry_policy()?),
          operation_timeout: sources.timeout,
          cancellation_grace: sources.grace,
        },
      },
    )?
    .with_managed_connections(ManagedIntegrationConnections::new(sources.store.clone()))
    .with_profile_provider(Arc::new(Self::new(sources, principal)));
    Ok(Arc::new(worker))
  }
}
#[async_trait::async_trait]
impl super::work_source::FactorySourceProfileProvider for ManagedFactorySource {
  async fn targets(&self, limit: u16) -> Result<Vec<(ProjectId, WorkSourceScanTarget)>, ApplicationError> {
    let mut cursor = self.cursor.lock().await;
    let project = match cursor.project {
      Some(project) => project,
      None => {
        let projects = self.sources.store.source_projects(cursor.after_project, 1).await?;
        let Some(project) = projects.first().copied() else {
          cursor.after_project = None;
          return Ok(vec![]);
        };
        cursor.project = Some(project);
        project
      }
    };
    let targets = self
      .sources
      .store
      .source_scan_targets(project, cursor.target, limit)
      .await?;
    if targets.len() == usize::from(limit) {
      cursor.target = targets.last().copied();
    } else {
      cursor.project = None;
      cursor.target = None;
      cursor.after_project = Some(project);
    }
    // Route only managed active members before claims; explicit legacy owners keep
    // their journal untouched. The cursor already covers the original unfiltered page.
    let mut managed = Vec::new();
    for target in targets {
      let members = match target {
        WorkSourceScanTarget::Input { id, version } => {
          vec![self.sources.store.source_input(project, id, version).await?]
        }
        WorkSourceScanTarget::Group { id, version } => self
          .sources
          .store
          .source_group(project, id, version)
          .await?
          .members()
          .to_vec(),
      };
      if members
        .iter()
        .filter(|i| i.definition().enabled)
        .all(|i| i.definition().connection.is_some())
      {
        managed.push((project, target));
      }
    }
    Ok(managed)
  }
  async fn profiles(&self, members: &[WorkSourceInput]) -> Result<Vec<FactorySourceProfile>, ApplicationError> {
    let mut profiles: Vec<FactorySourceProfile> = vec![];
    for input in members.iter().filter(|i| i.definition().enabled) {
      let Some(connection) = input.definition().connection.as_ref() else {
        return Err(ApplicationError::capability_unavailable());
      };
      if profiles
        .iter()
        .any(|p| p.read.project_id == input.project_id() && p.read.binding == input.definition().source)
      {
        continue;
      }
      let mut selected = None;
      for declaration in &self.sources.profiles.readers {
        let Some(preparation) = &declaration.preparation else {
          continue;
        };
        let read = declaration.profile(input.project_id(), self.context.clone())?;
        if read.binding != input.definition().source
          || declaration.read.capability.configuration != connection.source_configuration
        {
          continue;
        }
        let mut preparation = preparation.settings();
        preparation.mapping.source = read.binding.clone();
        selected = Some(FactorySourceProfile { read, preparation });
        break;
      }
      profiles.push(selected.ok_or_else(ApplicationError::capability_unavailable)?);
    }
    Ok(profiles)
  }
}
