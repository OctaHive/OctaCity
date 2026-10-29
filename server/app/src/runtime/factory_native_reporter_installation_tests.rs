//! The native producer is installed before authoring; no published Flow supplies its contract.
use super::*;
use crate::runtime::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::FactoryConfigurationAvailability;
use std::sync::Arc;

struct Ready;
#[async_trait::async_trait]
impl FactoryRuntimeReadiness for Ready {
  async fn available(&self) -> bool {
    true
  }
}

#[tokio::test]
async fn native_reporter_installation_declares_the_same_exact_producer_to_authoring_and_execution_without_a_flow() {
  let (root, installed, document) = installed_reporter_profile_fixture();
  let (_, config) = source_profile_config(root.path(), &document, std::slice::from_ref(&installed));
  let project = ProjectId::generate();
  let resources = crate::runtime::factory_tests::resources();
  let store = resources.runs.clone();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![],
    build_configurations: vec![],
  })
  .unwrap();
  let mut factory = factory(project, choices, resources).await;
  let integrations = crate::runtime::integration_management::install_with_factory(
    &config,
    store,
    tokio_util::sync::CancellationToken::new(),
    &mut crate::runtime::tests::healthy_checks(),
    Some(&mut factory),
  )
  .await
  .unwrap()
  .unwrap();
  let sources = integrations.source_management.unwrap();
  let writer = sources.reporter_profiles(project).unwrap().remove(0);
  let contract = sources.reporter_action_contract(&writer.binding().capability).unwrap();
  assert_eq!(contract.plugin, writer.binding().capability);
  assert_eq!(contract.action.as_str(), "set_status");
  assert_eq!(contract.outcome.key().as_str(), "queued");
  assert!(FlowPayload::new(&contract.output, serde_json::json!({"receipt":"ab".repeat(32)})).is_ok());
  assert_eq!(factory.executors.len(), 1);
  assert_eq!(factory.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(
    factory.executors[0].1,
    NodeExecutionIdentity::External(contract.plugin.clone())
  );
  let FactoryConfigurationAvailability::Available(choices) = factory.capabilities.availability(project).await.unwrap()
  else {
    panic!("the installed Project remains available")
  };
  assert!(
    choices
      .entries()
      .references
      .iter()
      .any(|choice| choice.kind == FactoryChoiceKind::NodeCapability && choice.reference == contract.plugin)
  );
  assert!(
    matches!(
      factory.capabilities.availability(ProjectId::generate()).await.unwrap(),
      FactoryConfigurationAvailability::Disabled
    ),
    "native actions cannot create a new Project execution inventory"
  );
}

async fn factory(
  project: ProjectId,
  choices: FactoryConfigurationChoices,
  resources: FactoryRuntimeResources,
) -> FactoryRuntimeInstallation {
  let catalog = FactoryRuntimeProfileCatalog {
    projects: vec![(project, choices)],
    codex: vec![],
    signals: vec![],
    profile_selectors: vec![],
    tool_risk: vec![],
    review_planning: vec![],
    executors: vec![],
  };
  FactoryProfileRuntimeInstaller::new(catalog, Arc::new(Ready), vec![])
    .unwrap()
    .install(resources)
    .await
    .unwrap()
}

#[tokio::test]
async fn native_reporter_alias_collision_preserves_existing_project_inventory() {
  let (root, installed, document) = installed_reporter_profile_fixture();
  let (_, config) = source_profile_config(root.path(), &document, std::slice::from_ref(&installed));
  let project = ProjectId::generate();
  let resources = crate::runtime::factory_tests::resources();
  let store = resources.runs.clone();
  let empty = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries::default()).unwrap();
  let mut initial = factory(project, empty, resources).await;
  crate::runtime::integration_management::install_with_factory(
    &config,
    store.clone(),
    tokio_util::sync::CancellationToken::new(),
    &mut crate::runtime::tests::healthy_checks(),
    Some(&mut initial),
  )
  .await
  .unwrap();
  let FactoryConfigurationAvailability::Available(observed) = initial.capabilities.availability(project).await.unwrap()
  else {
    panic!("installed Project")
  };
  let native = observed
    .entries()
    .references
    .into_iter()
    .find(|choice| choice.kind == FactoryChoiceKind::NodeCapability)
    .unwrap();
  let original = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: native.alias.clone(),
      reference: ImmutableReference::new(
        FactoryKey::new("manual-capability").unwrap(),
        FactoryKey::new("v1").unwrap(),
        FactoryDigest::from_bytes([93; 32]),
      ),
    }],
    build_configurations: vec![],
  })
  .unwrap()
  .with_choice_metadata(
    FactoryChoiceKind::NodeCapability,
    native.alias,
    FactoryChoiceMetadata {
      node_kinds: vec![FlowNodeKind::Reasoning],
      ..Default::default()
    },
  )
  .unwrap();
  let mut installed = factory(project, original.clone(), crate::runtime::factory_tests::resources()).await;
  crate::runtime::integration_management::install_with_factory(
    &config,
    store,
    tokio_util::sync::CancellationToken::new(),
    &mut crate::runtime::tests::healthy_checks(),
    Some(&mut installed),
  )
  .await
  .unwrap();
  assert_eq!(
    installed.capabilities.availability(project).await.unwrap(),
    FactoryConfigurationAvailability::Available(original),
    "optional Reporter metadata cannot replace or disable the existing inventory"
  );
}
