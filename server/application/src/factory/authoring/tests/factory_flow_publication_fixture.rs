//! Standalone Flow validation and publication fixture shared by both test binaries.
use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::*;

// Standalone roots may read installed choices but must never read a subflow.
// Only command tests opt in to the publication boundary.
pub(crate) struct PublicationChoices {
  choices: std::sync::Mutex<FactoryConfigurationChoices>,
  publishes: bool,
  predecessor: Option<PublishedFactoryFlowDefinition>,
}
impl PublicationChoices {
  pub(crate) fn for_validation(choices: FactoryConfigurationChoices) -> Self {
    Self {
      choices: std::sync::Mutex::new(choices),
      publishes: false,
      predecessor: None,
    }
  }

  pub(crate) fn for_publication(choices: FactoryConfigurationChoices) -> Self {
    Self {
      publishes: true,
      ..Self::for_validation(choices)
    }
  }

  pub(crate) fn with_predecessor(mut self, predecessor: PublishedFactoryFlowDefinition) -> Self {
    self.predecessor = Some(predecessor);
    self
  }

  pub(crate) fn replace_choices(&self, choices: FactoryConfigurationChoices) {
    *self.choices.lock().unwrap() = choices;
  }
}

#[async_trait]
impl FactoryConfigurationStore for PublicationChoices {
  async fn factory_configuration_availability(
    &self,
    _: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(FactoryConfigurationAvailability::Available(
      self.choices.lock().unwrap().clone(),
    ))
  }
  async fn replay_factory_configuration_mutation(
    &self,
    _: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
    unreachable!("validation cannot replay a mutation")
  }
  async fn create_factory_configuration(
    &self,
    _: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
    unreachable!("validation cannot create a configuration")
  }
  async fn replace_factory_configuration(
    &self,
    _: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
    unreachable!("validation cannot replace a configuration")
  }
  async fn factory_configuration_version(
    &self,
    _: FactoryConfigurationId,
    _: FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    unreachable!("root validation needs no configuration read")
  }
  async fn current_factory_configuration(
    &self,
    _: FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    unreachable!("root validation needs no configuration read")
  }
}
#[async_trait]
impl FactoryFlowDefinitionStore for PublicationChoices {
  async fn replay_factory_flow_definition(
    &self,
    _: ManagementMutation<ReplayFactoryFlowDefinition>,
  ) -> Result<Option<FactoryFlowDefinitionMutationOutcome>, StoreError> {
    assert!(self.publishes, "validation cannot replay a mutation");
    Ok(None)
  }
  async fn publish_factory_flow_definition(
    &self,
    request: ManagementMutation<PublishFactoryFlowDefinition>,
  ) -> Result<FactoryFlowDefinitionMutationOutcome, ReplaceFactoryConfigurationError> {
    assert!(self.publishes, "validation cannot publish");
    Ok(FactoryFlowDefinitionMutationOutcome {
      disposition: octacity_server_store::MutationDisposition::Applied,
      publication: request.into_parts().0.publication,
    })
  }
  async fn factory_flow_definition(
    &self,
    id: FlowDefinitionId,
    version: Option<FlowDefinitionVersion>,
  ) -> Result<PublishedFactoryFlowDefinition, StoreError> {
    self
      .predecessor
      .clone()
      .filter(|publication| {
        let reference = publication.flow.closure.root();
        reference.id() == id && Some(reference.version()) == version
      })
      .ok_or(StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::FlowDefinition,
      })
  }
  async fn list_factory_flow_definitions(
    &self,
    _: ProjectId,
    _: Option<FlowDefinitionPagePosition>,
    _: u16,
    _: FlowDefinitionListVisibility,
  ) -> Result<FactoryFlowDefinitionPage, StoreError> {
    unreachable!("validation cannot list definitions")
  }
}
