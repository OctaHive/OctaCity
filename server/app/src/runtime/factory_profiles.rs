//! Native composition of exact installed profiles and Project-scoped choices.
use super::factory::*;
use async_trait::async_trait;
use octacity_server_application::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{DecisionSignalProvider, FactoryChoiceKind, FactoryConfigurationChoices};
use octacity_server_store::{FactoryConfigurationAvailability, StoreError};
use octacity_server_store_postgres::FactoryCapabilitySource;
use std::sync::Arc;

/// One installed provider with exact routing operations and independent authority.
#[derive(Clone)]
pub struct FactoryRuntimeSignalProfile {
  /// Credential-resolved provider selected by the trusted deployment.
  pub provider: Arc<dyn DecisionSignalProvider>,
  /// Bounded Project-owned actions; logical node names never select them.
  pub bindings: Vec<FactoryDecisionSignalNodeBinding>,
}
/// Deployment-owned catalog; contains no Flow or stage routing rules.
#[derive(Clone)]
pub struct FactoryRuntimeProfileCatalog {
  /// Exact selectable references, scoped independently for each Project.
  pub projects: Vec<(ProjectId, FactoryConfigurationChoices)>,
  /// Qualified ordinary Build compilers and retained output verification profiles.
  pub codex: Vec<FactoryCodexBuildProfile>,
  /// Separate installed routing providers; tool-risk uses its own authorization seam.
  pub signals: Vec<FactoryRuntimeSignalProfile>,
  /// Separate pre-dispatch policies using the same installed connector registry.
  pub profile_selectors: Vec<FactoryProfileSelector>,
  /// Exact workload-bound tool-risk connectors; credentials stay server-owned.
  pub tool_risk: Vec<FactoryToolRiskNodeProfile>,
  /// Exact installed review policies, executed against the native fenced journal.
  pub review_planning: Vec<(
    octacity_server_factory::ImmutableReference,
    FactoryReviewPlanningProfile,
  )>,
  /// Other qualified technical executors, including trusted delivery and promotion.
  pub executors: Vec<FactoryNodeExecutorBinding>,
}
/// Installs native journals, selected Artifact context and ordinary Build ports.
/// Dependencies are not probed here; the scoped readiness monitor owns availability.
pub struct FactoryProfileRuntimeInstaller {
  catalog: FactoryRuntimeProfileCatalog,
  readiness: Arc<dyn FactoryRuntimeReadiness>,
  outbox: Vec<Arc<dyn FactoryOutboxDispatcher>>,
  work_sources: Option<Arc<dyn super::FactorySourceRuntime>>,
}
impl FactoryProfileRuntimeInstaller {
  /// Enables independent Source polling; availability never gates manual Factory admission.
  pub fn with_work_sources(mut self, worker: Arc<dyn super::FactorySourceRuntime>) -> Self {
    self.work_sources = Some(worker);
    self
  }
  /// Rejects ambiguous Projects, foreign Build choices and profiles missing
  /// their exact Project-owned Build choice before I/O.
  pub fn new(
    catalog: FactoryRuntimeProfileCatalog,
    readiness: Arc<dyn FactoryRuntimeReadiness>,
    outbox: Vec<Arc<dyn FactoryOutboxDispatcher>>,
  ) -> Result<Self, FactoryRuntimeInstallationError> {
    catalog.validate(outbox.len())?;
    Ok(Self {
      catalog,
      readiness,
      outbox,
      work_sources: None,
    })
  }
}
impl FactoryRuntimeProfileCatalog {
  pub(super) fn validate(&self, outbox_count: usize) -> Result<(), FactoryRuntimeInstallationError> {
    let catalog = self;
    use FactoryRuntimeInstallationError::Invalid;
    if catalog.projects.is_empty()
      || catalog.projects.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog.codex.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog.signals.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog.profile_selectors.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog.tool_risk.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog.review_planning.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog.executors.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || outbox_count > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog
        .signals
        .iter()
        .any(|profile| profile.bindings.len() > MAX_FACTORY_EXECUTOR_BINDINGS)
    {
      return Err(Invalid);
    }
    for (index, (project, choices)) in catalog.projects.iter().enumerate() {
      if catalog.projects[..index]
        .iter()
        .any(|(previous, _)| previous == project)
        || choices
          .entries()
          .build_configurations
          .iter()
          .any(|(_, config)| config.project_id() != *project)
      {
        return Err(Invalid);
      }
    }
    for profile in &catalog.codex {
      if !catalog.projects.iter().any(|(project, choices)| {
        *project == profile.build_configuration.project_id()
          && choices
            .entries()
            .build_configurations
            .iter()
            .any(|(_, config)| config == &profile.build_configuration)
      }) {
        return Err(Invalid);
      }
    }
    if catalog
      .signals
      .iter()
      .flat_map(|profile| &profile.bindings)
      .any(|binding| {
        !catalog
          .projects
          .iter()
          .any(|(project, _)| *project == binding.project_id)
      })
    {
      return Err(Invalid);
    }
    for profile in &catalog.profile_selectors {
      profile.validate().map_err(|_| Invalid)?;
      if !catalog
        .projects
        .iter()
        .any(|(project, _)| *project == profile.project_id)
      {
        return Err(Invalid);
      }
    }
    if catalog.tool_risk.iter().any(|profile| {
      !catalog.projects.iter().any(|(project, choices)| {
        *project == profile.binding.project_id
          && choices
            .entries()
            .build_configurations
            .iter()
            .any(|(_, config)| config == &profile.build_configuration)
      })
    }) {
      return Err(Invalid);
    }
    if catalog.review_planning.iter().any(|(identity, profile)| {
      !profile.digest().is_ok_and(|digest| identity.digest() == digest)
        || profile.criterion_packs.iter().any(|pack| {
          !catalog
            .projects
            .iter()
            .any(|(project, _)| *project == pack.project_id())
        })
    }) {
      return Err(Invalid);
    }
    Ok(())
  }
}
struct ProjectCatalog(Vec<(ProjectId, FactoryConfigurationChoices)>);
#[async_trait]
impl FactoryCapabilitySource for ProjectCatalog {
  async fn availability(&self, project: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(
      self
        .0
        .iter()
        .find(|(owner, _)| *owner == project)
        .map_or(FactoryConfigurationAvailability::Disabled, |(_, choices)| {
          FactoryConfigurationAvailability::Available(choices.clone())
        }),
    )
  }
}
#[async_trait]
impl FactoryRuntimeInstaller for FactoryProfileRuntimeInstaller {
  async fn install(
    &self,
    resources: FactoryRuntimeResources,
  ) -> Result<FactoryRuntimeInstallation, FactoryRuntimeInstallationError> {
    let mut executors = self.catalog.executors.clone();
    for (identity, profile) in &self.catalog.review_planning {
      let action = FactoryReviewPlanningAction::new(
        identity.clone(),
        resources.runs.clone(),
        profile.clone(),
        Arc::new(SystemFactoryExecutionClock),
      );
      executors.push((
        octacity_server_factory::FlowNodeKind::TrustedAction,
        octacity_server_factory::NodeExecutionIdentity::External(identity.clone()),
        Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
      ));
    }
    let mut remaining = self.catalog.codex.clone();
    while let Some(first) = remaining.pop() {
      let mut profiles = vec![first];
      let mut index = 0;
      while index < remaining.len() {
        if remaining[index].toolchain.executable == profiles[0].toolchain.executable {
          profiles.push(remaining.remove(index));
        } else {
          index += 1;
        }
      }
      executors.push(resources.codex_executor(profiles)?);
    }
    let mut remaining = self
      .catalog
      .signals
      .iter()
      .flat_map(|profile| {
        profile.bindings.iter().map(|binding| FactoryDecisionSignalNodeProfile {
          provider: profile.provider.clone(),
          binding: binding.clone(),
        })
      })
      .collect::<Vec<_>>();
    while let Some(first) = remaining.pop() {
      let mut profiles = vec![first];
      let mut index = 0;
      while index < remaining.len() {
        if remaining[index].binding.action.plugin() == profiles[0].binding.action.plugin() {
          profiles.push(remaining.remove(index));
        } else {
          index += 1;
        }
      }
      executors.push(resources.decision_signal_executor(profiles)?);
    }
    FactoryNodeExecutorRegistry::new(executors.clone()).map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    let tool_authorizer = if self.catalog.tool_risk.is_empty() {
      None
    } else {
      Some(resources.tool_risk_authorizer(self.catalog.tool_risk.clone())?)
    };
    let reader = resources.retained_artifact_reader();
    let profile_selectors = if self.catalog.profile_selectors.is_empty() {
      None
    } else {
      Some(resources.profile_selectors(self.catalog.profile_selectors.clone())?)
    };
    let mut projects = self.catalog.projects.clone();
    for selector in &self.catalog.profile_selectors {
      for (project, choices) in &mut projects {
        if *project != selector.project_id {
          continue;
        }
        for entry in choices.entries().references {
          if entry.kind == FactoryChoiceKind::DecisionSignalPolicy && entry.reference == selector.selection_policy {
            let mut metadata = choices
              .choice_metadata(entry.kind, &entry.alias)
              .cloned()
              .unwrap_or_default();
            let installed = selector
              .choice_metadata()
              .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
            metadata.node_kinds.extend(installed.node_kinds);
            metadata.signal_purposes.extend(installed.signal_purposes);
            metadata.profile_selection = installed.profile_selection;
            *choices = choices
              .clone()
              .with_choice_metadata(entry.kind, entry.alias, metadata)
              .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
          }
        }
      }
    }
    Ok(FactoryRuntimeInstallation {
      capabilities: Arc::new(ProjectCatalog(projects)),
      executors,
      inputs: Arc::new(FactoryArtifactFlowInputs::new(
        reader,
        Arc::new(SystemFactoryExecutionClock),
      )),
      readiness: self.readiness.clone(),
      outbox_dispatchers: self.outbox.clone(),
      tool_authorizer,
      output_finalizer: None,
      work_sources: self.work_sources.clone(),
      profile_selectors,
    })
  }
}
