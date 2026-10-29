//! One bootstrap declaration supplies both the native executor and its read-only authoring contract.
use super::{factory::FactoryRuntimeInstallation, source_management::SourceManagement};
use octacity_server_application::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::{FactoryCapabilitySource, FactoryConfigurationAvailability, StoreError};
use std::{collections::BTreeSet, sync::Arc};

pub(super) fn contracts(
  profiles: &super::source_profiles_file::SourceProfiles,
  factory: &FactoryRuntimeInstallation,
) -> Result<Vec<FactoryWorkStatusActionContract>, ApplicationError> {
  let identities: BTreeSet<_> = profiles
    .reporters
    .iter()
    .map(|reporter| reporter.write.capability.capability.clone())
    .collect();
  let selected: Vec<_> = identities
    .into_iter()
    .filter(|identity| {
      !factory.executors.iter().any(|(kind, installed, _)| {
        *kind == FlowNodeKind::TrustedAction && installed == &NodeExecutionIdentity::External(identity.clone())
      })
    })
    .collect();
  if selected.len() + factory.executors.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
    // Optional producers cannot evict an installed executor or break ordinary Factory startup.
    return Ok(vec![]);
  }
  selected.into_iter().map(FactoryWorkStatusActionContract::new).collect()
}
pub(super) fn install(
  sources: &Arc<SourceManagement>,
  factory: &mut FactoryRuntimeInstallation,
) -> Result<(), ApplicationError> {
  for contract in &sources.reporter_actions {
    let action = FactoryWorkStatusAction::installed(contract.plugin.clone(), sources.clone())?;
    factory.executors.push((
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(contract.plugin.clone()),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
    ));
  }
  if !sources.reporter_actions.is_empty() {
    factory.capabilities = Arc::new(ReporterCapabilities {
      base: factory.capabilities.clone(),
      contracts: sources.reporter_actions.clone(),
    });
  }
  Ok(())
}
struct ReporterCapabilities {
  base: Arc<dyn FactoryCapabilitySource>,
  contracts: Vec<FactoryWorkStatusActionContract>,
}
#[async_trait::async_trait]
impl FactoryCapabilitySource for ReporterCapabilities {
  async fn availability(&self, project: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    let available = self.base.availability(project).await?;
    let FactoryConfigurationAvailability::Available(mut choices) = available else {
      return Ok(available);
    };
    let mut aliases: BTreeSet<_> = choices
      .entries()
      .references
      .iter()
      .filter(|choice| choice.kind == FactoryChoiceKind::NodeCapability)
      .map(|choice| choice.alias.clone())
      .collect();
    for contract in &self.contracts {
      if aliases.len() == MAX_CONFIGURATION_CHOICES_PER_KIND {
        break;
      }
      let plugin = &contract.plugin;
      let digest = FactoryDigest::sha256(
        "octacity.source-reporter.action-choice.v1",
        &[
          plugin.identity().as_str().as_bytes(),
          plugin.version().as_str().as_bytes(),
          &plugin.digest().as_bytes(),
        ],
      );
      let alias = FactoryKey::new(format!("work-reporter-{digest}")).map_err(|_| StoreError::Unavailable)?;
      // Optional presentation cannot replace an existing Project choice, even when its alias collides.
      if !aliases.insert(alias.clone()) {
        continue;
      }
      choices = choices
        .with_reference_choice(FactoryReferenceChoice {
          kind: FactoryChoiceKind::NodeCapability,
          alias: alias.clone(),
          reference: plugin.clone(),
        })
        .and_then(|choices| {
          choices.with_choice_metadata(
            FactoryChoiceKind::NodeCapability,
            alias,
            FactoryChoiceMetadata {
              node_kinds: vec![FlowNodeKind::TrustedAction],
              ..Default::default()
            },
          )
        })
        .map_err(|_| StoreError::Unavailable)?;
    }
    Ok(FactoryConfigurationAvailability::Available(choices))
  }
  async fn refresh_role_models(&self, project: ProjectId, alias: &FactoryKey) -> Result<(), StoreError> {
    self.base.refresh_role_models(project, alias).await
  }
}
