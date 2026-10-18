//! Operator-owned Flow authoring uses the same graph grammar as admission.
use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, Query, management_security::owned_collection_resource,
};
use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  FactoryFlowConfiguration, FactoryKey, FlowAdmissionLimits, FlowContextProjection, FlowDataProjection, FlowDataSchema,
  FlowDefinition, FlowDefinitionId, FlowDefinitionInput, FlowDefinitionVersion, FlowExecutionPolicy,
  FlowNodeDefinition, FlowPoolSettings, FlowTerminalDefinition, FlowTransition, ImmutableReference,
  PinnedFlowDefinitionClosure,
};
use octacity_server_store::FactoryFlowDefinitionStore;
use serde::{Deserialize, Serialize};
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

/// Exact native gate policies recognized in an already validated published closure.
/// This discovery fact grants no execution authority and requires no deployment alias.
#[must_use]
pub fn factory_native_gate_policies(flow: &FactoryFlowConfiguration) -> Vec<ImmutableReference> {
  let mut policies = BTreeSet::new();
  for node in flow
    .closure
    .definitions()
    .iter()
    .flat_map(|definition| definition.nodes())
  {
    if octacity_server_factory::FlowGateProgram::from_node(node).is_ok()
      && let Some(gate) = node.gate()
    {
      policies.insert(gate.policy().clone());
    }
  }
  policies.into_iter().collect()
}

/// Typed root authoring grammar. Identity, version and digest are server-owned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryFlowRootDraft {
  /// Exact input contract for the entry.
  pub input_schema: Option<ImmutableReference>,
  /// Caller-selected entry node.
  pub entry: FactoryKey,
  /// Closed primitive declarations with arbitrary logical keys.
  pub nodes: Vec<FlowNodeDefinition>,
  /// Finite outcome-selected routes.
  pub transitions: Vec<FlowTransition>,
  /// Finite terminal contract.
  pub terminals: Vec<FlowTerminalDefinition>,
  /// Explicit context visibility, independent of ordering.
  pub context_projections: Vec<FlowContextProjection>,
  /// Explicit typed data transfer.
  pub data_projections: Vec<FlowDataProjection>,
  /// Root resource and authority ceilings.
  pub execution: FlowExecutionPolicy,
}
/// Root content and bounded executable contracts; subflows resolve from published exact references.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryFlowDefinitionDraft {
  /// Root content without a caller-computed digest.
  pub root: FactoryFlowRootDraft,
  /// Admission ceilings for the entire reachable closure.
  pub limits: FlowAdmissionLimits,
  /// Explicit typed input and output contracts.
  pub data_schemas: Vec<FlowDataSchema>,
  /// Named phase policies; node names carry no runtime meaning.
  #[serde(default)]
  pub pools: BTreeMap<FactoryKey, FlowPoolSettings>,
}

/// Non-mutating schema normalization with the same Project-scoped authoring authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidateFactoryFlowDataSchemaQuery {
  /// Owning visible Project.
  pub project_id: ProjectId,
  /// Operator-owned logical contract name.
  pub identity: FactoryKey,
  /// Explicit immutable contract version.
  pub version: FactoryKey,
  /// Closed bounded data grammar, without a caller-computed digest.
  pub shape: octacity_server_factory::FlowValueSchema,
}
impl Query for ValidateFactoryFlowDataSchemaQuery {
  type Outcome = FlowDataSchema;
}
impl ManagementAuthorizationTarget for ValidateFactoryFlowDataSchemaQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ValidateFactoryFlowDefinitionQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FlowDefinition,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

#[async_trait]
impl<S: octacity_server_store::FactoryConfigurationStore + 'static>
  crate::ManagementQueryUseCase<ValidateFactoryFlowDataSchemaQuery> for FactoryFlowDefinitionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: ValidateFactoryFlowDataSchemaQuery,
  ) -> Result<FlowDataSchema, ApplicationError> {
    crate::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      query.project_id,
    )
    .await?;
    FlowDataSchema::new(query.identity, query.version, query.shape)
      .map_err(|_| crate::FactoryValidationDiagnostics::invalid("/shape"))
  }
}
impl FactoryFlowDefinitionDraft {
  fn root(&self, id: FlowDefinitionId, version: FlowDefinitionVersion) -> Result<FlowDefinition, ApplicationError> {
    let root = &self.root;
    FlowDefinition::new(FlowDefinitionInput {
      id,
      version,
      input_schema: root.input_schema.clone(),
      entry: root.entry.clone(),
      nodes: root.nodes.clone(),
      transitions: root.transitions.clone(),
      terminals: root.terminals.clone(),
      context_projections: root.context_projections.clone(),
      data_projections: root.data_projections.clone(),
      execution: root.execution.clone(),
    })
    .map_err(|_| crate::FactoryValidationDiagnostics::invalid("/root"))
  }
}
/// Non-mutating authoritative graph and exact closure validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidateFactoryFlowDefinitionQuery {
  /// Prospective stable root identity.
  pub id: FlowDefinitionId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Explicit caller graph.
  pub draft: FactoryFlowDefinitionDraft,
}
impl Query for ValidateFactoryFlowDefinitionQuery {
  type Outcome = FactoryFlowConfiguration;
}
impl ManagementAuthorizationTarget for ValidateFactoryFlowDefinitionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::FlowDefinition,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FlowDefinition,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}
/// Exact immutable Flow authoring over the existing authorization and store ports.
pub struct FactoryFlowDefinitionHandlers<S> {
  store: Arc<S>,
  policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
  source_actions: Option<Arc<dyn crate::FactoryStageSourceAuthorizer>>,
}
impl<S> FactoryFlowDefinitionHandlers<S> {
  /// Binds the same authorization policy used by the management decorators.
  #[must_use]
  pub fn new(store: Arc<S>, policy: Arc<dyn crate::ManagementAuthorizationPolicy>) -> Self {
    Self {
      store,
      policy,
      source_actions: None,
    }
  }
  /// Shares installed source authorization with Factory Configuration publication.
  #[must_use]
  pub fn with_source_actions(mut self, authorizer: Arc<dyn crate::FactoryStageSourceAuthorizer>) -> Self {
    self.source_actions = Some(authorizer);
    self
  }
}
impl<S: FactoryFlowDefinitionStore + octacity_server_store::FactoryConfigurationStore>
  FactoryFlowDefinitionHandlers<S>
{
  async fn resolve(
    &self,
    context: &crate::ManagementRequestContext,
    id: FlowDefinitionId,
    version: FlowDefinitionVersion,
    project: ProjectId,
    draft: FactoryFlowDefinitionDraft,
  ) -> Result<FactoryFlowConfiguration, ApplicationError> {
    let choices = crate::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      project,
    )
    .await?;
    let root = draft.root(id, version)?;
    let root_ref = root.reference();
    let mut pending = root
      .nodes()
      .iter()
      .filter_map(FlowNodeDefinition::subflow_definition)
      .collect::<Vec<_>>();
    let mut seen = BTreeSet::from([root_ref]);
    let mut definitions = vec![root];
    while let Some(reference) = pending.pop() {
      if !seen.insert(reference) {
        continue;
      }
      if seen.len() > octacity_server_factory::MAX_FLOW_DEFINITION_CLOSURE {
        return Err(ApplicationError::invalid());
      }
      crate::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::FlowDefinition,
        reference.id(),
      )
      .await?;
      let published = self
        .store
        .factory_flow_definition(reference.id(), Some(reference.version()))
        .await?;
      if published.project_id != project || published.flow.closure.root() != reference {
        return Err(ApplicationError::invalid());
      }
      for definition in published.flow.closure.definitions() {
        if definition.reference() != reference {
          pending.push(definition.reference());
        }
      }
      definitions.push(
        published
          .flow
          .closure
          .definition(reference)
          .ok_or_else(ApplicationError::invalid)?
          .clone(),
      );
    }
    let flow = FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(root_ref, definitions).map_err(|_| ApplicationError::invalid())?,
      draft.limits,
      draft.data_schemas,
      draft.pools,
    )
    .map_err(|_| ApplicationError::invalid())?;
    crate::factory_configuration_choices::validate_flow_choices(&flow, &choices, "")?;
    crate::factory_stage_source_update::validate_flow_source_updates(
      &flow,
      self.source_actions.as_deref(),
      context,
      project,
    )
    .await?;
    Ok(flow)
  }
}
#[async_trait]
impl<S: FactoryFlowDefinitionStore + octacity_server_store::FactoryConfigurationStore + 'static>
  crate::ManagementQueryUseCase<ValidateFactoryFlowDefinitionQuery> for FactoryFlowDefinitionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: ValidateFactoryFlowDefinitionQuery,
  ) -> Result<FactoryFlowConfiguration, ApplicationError> {
    self
      .resolve(
        context,
        query.id,
        FlowDefinitionVersion::INITIAL,
        query.project_id,
        query.draft,
      )
      .await
  }
}

/// Resolved immutable graph preview, without publication or mutation.
pub type FactoryFlowDefinitionValidation = FactoryFlowConfiguration;

/// Initial immutable Flow publication with a caller-owned replay identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFactoryFlowDefinitionCommand {
  /// Stable root identity.
  pub id: FlowDefinitionId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Complete root draft.
  pub draft: FactoryFlowDefinitionDraft,
  /// Stable transport identity.
  pub idempotency_key: octacity_server_store::IdempotencyKey,
  /// Authoritative server time.
  pub published_at: octacity_server_domain::Timestamp,
}
/// Append the next immutable version of one Flow identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceFactoryFlowDefinitionCommand {
  /// Existing identity.
  pub id: FlowDefinitionId,
  /// Version that must still be current.
  pub expected_current_version: FlowDefinitionVersion,
  /// Complete replacement content.
  pub draft: FactoryFlowDefinitionDraft,
  /// Stable transport identity.
  pub idempotency_key: octacity_server_store::IdempotencyKey,
  /// Authoritative server time.
  pub published_at: octacity_server_domain::Timestamp,
}
/// Exact immutable publication outcome.
pub type FactoryFlowDefinitionCommandOutcome = octacity_server_store::FactoryFlowDefinitionMutationOutcome;
impl crate::Command for CreateFactoryFlowDefinitionCommand {
  type Outcome = FactoryFlowDefinitionCommandOutcome;
}
impl crate::Command for ReplaceFactoryFlowDefinitionCommand {
  type Outcome = FactoryFlowDefinitionCommandOutcome;
}
impl ManagementAuthorizationTarget for CreateFactoryFlowDefinitionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ValidateFactoryFlowDefinitionQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FlowDefinition,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}
impl ManagementAuthorizationTarget for ReplaceFactoryFlowDefinitionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::FlowDefinition);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::FlowDefinition, self.id)
  }
}
/// Reads the current or one exact immutable Flow version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetFactoryFlowDefinitionQuery {
  /// Stable root identity.
  pub id: FlowDefinitionId,
  /// Exact version, or the current version when absent.
  pub version: Option<FlowDefinitionVersion>,
}
impl Query for GetFactoryFlowDefinitionQuery {
  type Outcome = octacity_server_store::PublishedFactoryFlowDefinition;
}
impl ManagementAuthorizationTarget for GetFactoryFlowDefinitionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::FlowDefinition);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::FlowDefinition, self.id)
  }
}
/// Bounded visible Flow definitions, also used as exact subflow choices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectFactoryFlowDefinitionsQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exclusive composite creation position.
  pub after: Option<octacity_server_store::FlowDefinitionPagePosition>,
  /// Positive bounded page size.
  pub limit: u16,
}
impl Query for ListProjectFactoryFlowDefinitionsQuery {
  type Outcome = octacity_server_store::FactoryFlowDefinitionPage;
}
impl ManagementAuthorizationTarget for ListProjectFactoryFlowDefinitionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::FlowDefinition,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FlowDefinition,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}
fn draft_digest(
  draft: &FactoryFlowDefinitionDraft,
) -> Result<octacity_server_factory::FactoryDigest, ApplicationError> {
  Ok(octacity_server_factory::FactoryDigest::sha256(
    "octacity.factory.flow-draft.v1",
    &[&serde_json::to_vec(draft).map_err(|_| ApplicationError::invalid())?],
  ))
}
#[async_trait]
impl<S: FactoryFlowDefinitionStore + octacity_server_store::FactoryConfigurationStore + 'static>
  crate::ManagementCommandUseCase<CreateFactoryFlowDefinitionCommand> for FactoryFlowDefinitionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: CreateFactoryFlowDefinitionCommand,
  ) -> Result<FactoryFlowDefinitionCommandOutcome, ApplicationError> {
    let digest = draft_digest(&command.draft)?;
    let probe = octacity_server_store::ReplayFactoryFlowDefinition {
      id: command.id,
      project_id: Some(command.project_id),
      expected_current_version: None,
      intent_digest: digest,
      idempotency_key: command.idempotency_key.clone(),
    };
    if let Some(outcome) = self
      .store
      .replay_factory_flow_definition(crate::management_security::audited_mutation(context, probe)?)
      .await?
    {
      return Ok(outcome);
    }
    let flow = self
      .resolve(
        context,
        command.id,
        FlowDefinitionVersion::INITIAL,
        command.project_id,
        command.draft,
      )
      .await?;
    Ok(
      self
        .store
        .publish_factory_flow_definition(crate::management_security::audited_mutation(
          context,
          octacity_server_store::PublishFactoryFlowDefinition {
            intent_digest: digest,
            publication: octacity_server_store::PublishedFactoryFlowDefinition {
              project_id: command.project_id,
              flow,
              published_at: command.published_at,
            },
            expected_current_version: None,
            idempotency_key: command.idempotency_key,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait]
impl<S: FactoryFlowDefinitionStore + octacity_server_store::FactoryConfigurationStore + 'static>
  crate::ManagementCommandUseCase<ReplaceFactoryFlowDefinitionCommand> for FactoryFlowDefinitionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: ReplaceFactoryFlowDefinitionCommand,
  ) -> Result<FactoryFlowDefinitionCommandOutcome, ApplicationError> {
    let digest = draft_digest(&command.draft)?;
    let probe = octacity_server_store::ReplayFactoryFlowDefinition {
      id: command.id,
      project_id: None,
      expected_current_version: Some(command.expected_current_version),
      intent_digest: digest,
      idempotency_key: command.idempotency_key.clone(),
    };
    if let Some(outcome) = self
      .store
      .replay_factory_flow_definition(crate::management_security::audited_mutation(context, probe)?)
      .await?
    {
      return Ok(outcome);
    }
    let current = self.store.factory_flow_definition(command.id, None).await?;
    if current.flow.closure.root().version() != command.expected_current_version {
      return Err(ApplicationError::PreconditionFailed);
    }
    let version = FlowDefinitionVersion::new(
      command
        .expected_current_version
        .get()
        .checked_add(1)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let flow = self
      .resolve(context, command.id, version, current.project_id, command.draft)
      .await?;
    Ok(
      self
        .store
        .publish_factory_flow_definition(crate::management_security::audited_mutation(
          context,
          octacity_server_store::PublishFactoryFlowDefinition {
            intent_digest: digest,
            publication: octacity_server_store::PublishedFactoryFlowDefinition {
              project_id: current.project_id,
              flow,
              published_at: command.published_at,
            },
            expected_current_version: Some(command.expected_current_version),
            idempotency_key: command.idempotency_key,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait]
impl<S: FactoryFlowDefinitionStore + octacity_server_store::FactoryConfigurationStore + 'static>
  crate::ManagementQueryUseCase<GetFactoryFlowDefinitionQuery> for FactoryFlowDefinitionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetFactoryFlowDefinitionQuery,
  ) -> Result<octacity_server_store::PublishedFactoryFlowDefinition, ApplicationError> {
    Ok(self.store.factory_flow_definition(query.id, query.version).await?)
  }
}
#[async_trait]
impl<S: FactoryFlowDefinitionStore + octacity_server_store::FactoryConfigurationStore + 'static>
  crate::ManagementQueryUseCase<ListProjectFactoryFlowDefinitionsQuery> for FactoryFlowDefinitionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListProjectFactoryFlowDefinitionsQuery,
  ) -> Result<octacity_server_store::FactoryFlowDefinitionPage, ApplicationError> {
    if query.limit == 0 || query.limit > octacity_server_store::MAX_FACTORY_CONFIGURATION_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    Ok(
      self
        .store
        .list_factory_flow_definitions(
          query.project_id,
          query.after,
          query.limit,
          grant
            .visibility_for::<ListProjectFactoryFlowDefinitionsQuery>()
            .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?,
        )
        .await?,
    )
  }
}
