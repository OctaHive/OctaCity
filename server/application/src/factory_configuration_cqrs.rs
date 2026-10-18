use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryConfiguration, FactoryConfigurationDraft, FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest,
};
use octacity_server_store::{
  CreateFactoryConfiguration, FactoryConfigurationAvailability, FactoryConfigurationMutationIntent,
  FactoryConfigurationStore, IdempotencyKey, ReplaceFactoryConfiguration, ReplayFactoryConfigurationMutation,
};

pub use octacity_server_store::{
  CurrentFactoryConfigurationPage as FactoryConfigurationPageProjection, FactoryConfigurationPagePosition,
};

use crate::{
  ApplicationError, Command, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, MutationDisposition, Query,
  management_security::{audited_mutation, instance_resource, owned_collection_resource},
};

/// Lists visible current configurations in newest-first composite order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectFactoryConfigurationsQuery {
  page: octacity_server_store::ListProjectFactoryConfigurations,
}

impl ListProjectFactoryConfigurationsQuery {
  /// Validates pagination before authorization and store dispatch.
  pub fn try_new(
    project_id: ProjectId,
    after: Option<FactoryConfigurationPagePosition>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    Ok(Self {
      page: octacity_server_store::ListProjectFactoryConfigurations::new(
        project_id,
        after,
        limit,
        octacity_server_store::FactoryConfigurationListVisibility::none(),
      )?,
    })
  }

  /// Borrows the validated page input; the handler supplies authorization visibility.
  #[must_use]
  pub const fn page(&self) -> &octacity_server_store::ListProjectFactoryConfigurations {
    &self.page
  }
}

impl Query for ListProjectFactoryConfigurationsQuery {
  type Outcome = FactoryConfigurationPageProjection;
}

impl ManagementAuthorizationTarget for ListProjectFactoryConfigurationsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::FactoryConfiguration,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Project,
      self.page.project_id(),
    )
  }
}

/// Creates one immutable Factory Configuration identity and version one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFactoryConfigurationCommand {
  /// Stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Digest of the canonical submitted definition.
  pub definition_digest: FactoryDigest,
  /// Complete unresolved initial definition.
  pub draft: FactoryConfigurationDraft,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for CreateFactoryConfigurationCommand {
  type Outcome = FactoryConfigurationCommandOutcome;
}

impl ManagementAuthorizationTarget for CreateFactoryConfigurationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::FactoryConfiguration,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Publishes the immediately following immutable Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceFactoryConfigurationCommand {
  /// Existing Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Version that must still be current.
  pub expected_current_version: FactoryConfigurationVersion,
  /// Digest of the canonical submitted definition.
  pub definition_digest: FactoryDigest,
  /// Complete unresolved replacement definition.
  pub draft: FactoryConfigurationDraft,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for ReplaceFactoryConfigurationCommand {
  type Outcome = FactoryConfigurationCommandOutcome;
}

impl ManagementAuthorizationTarget for ReplaceFactoryConfigurationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::FactoryConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}

/// Validates and resolves one candidate without allocating or publishing a version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidateFactoryConfigurationQuery {
  /// Prospective stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Digest of the canonical candidate definition.
  pub definition_digest: FactoryDigest,
  /// Complete unresolved candidate.
  pub draft: FactoryConfigurationDraft,
}

impl Query for ValidateFactoryConfigurationQuery {
  type Outcome = FactoryConfigurationValidation;
}

impl ManagementAuthorizationTarget for ValidateFactoryConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::FactoryConfiguration,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Reads one exact immutable Factory Configuration version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetFactoryConfigurationQuery {
  /// Stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Exact immutable version.
  pub version: FactoryConfigurationVersion,
}

impl Query for GetFactoryConfigurationQuery {
  type Outcome = FactoryConfigurationProjection;
}

impl ManagementAuthorizationTarget for GetFactoryConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::FactoryConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}

/// Discovers the current immutable version of one Factory Configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetCurrentFactoryConfigurationQuery {
  /// Stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
}

impl Query for GetCurrentFactoryConfigurationQuery {
  type Outcome = FactoryConfigurationProjection;
}

impl ManagementAuthorizationTarget for GetCurrentFactoryConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::FactoryConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}

/// Safe application projection of one immutable Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationProjection {
  /// Fully resolved immutable configuration.
  pub configuration: FactoryConfiguration,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Safe validation diagnostics shared by previews and rejected publications.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct FactoryValidationDiagnostics {
  /// Normalized field errors; values and hidden catalogue identities are omitted.
  pub errors: Vec<FactoryValidationIssue>,
  /// Fields whose references cannot be selected under the current grant.
  pub unavailable_references: Vec<String>,
  /// Exact required execution contracts selected from visible choices.
  pub required_node_capabilities: Vec<octacity_server_factory::ImmutableReference>,
  /// Exact tool/plugin contracts needed by ordinary Build execution.
  pub required_agent_capabilities: Vec<octacity_server_factory::ImmutableReference>,
  /// Semantic categories the backend must enforce, including denied categories.
  pub required_backend_capabilities: Vec<octacity_server_factory::PermissionCategory>,
  /// Definition/node ceilings after intersecting the enclosing Flow permissions.
  pub effective_permissions: Vec<FactoryEffectiveNodePermissions>,
  /// Stable warnings that contain no provider data or credential identities.
  pub warnings: Vec<&'static str>,
}
/// Effective authoring ceiling; Agent-local grants can narrow it again at dispatch.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryEffectiveNodePermissions {
  /// Exact immutable definition owning the node.
  pub definition: octacity_server_factory::FlowDefinitionRef,
  /// Arbitrary caller-selected logical node key.
  pub node: octacity_server_factory::FactoryKey,
  /// Effective Flow/definition/node permission intersection.
  pub permissions: octacity_server_factory::FactoryPermissionSet,
}
/// One stable field-associated validation failure.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryValidationIssue {
  /// JSON pointer into the submitted draft, or relative to the exact binding below.
  pub field: String,
  /// Stable safe classification, without provider errors or submitted values.
  pub code: &'static str,
  /// Stable enclosing definition; node fields use paths relative to this binding.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub definition: Option<octacity_server_factory::FlowDefinitionRef>,
  /// Stable caller node key, independent of canonical ordering.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub node: Option<octacity_server_factory::FactoryKey>,
}
impl FactoryValidationDiagnostics {
  /// Builds safe diagnostics from an already resolved and validated graph.
  #[must_use]
  pub fn for_flow(flow: &octacity_server_factory::FactoryFlowConfiguration) -> Self {
    let mut nodes = std::collections::BTreeSet::new();
    let mut agents = std::collections::BTreeSet::new();
    let mut effective_permissions = vec![];
    for definition in flow.closure.definitions() {
      let ceiling = flow
        .limits
        .permissions()
        .intersection(definition.execution().permissions());
      for node in definition.nodes() {
        effective_permissions.push(FactoryEffectiveNodePermissions {
          definition: definition.reference(),
          node: node.key().clone(),
          permissions: ceiling.intersection(node.permissions()),
        });
        nodes.extend(
          node
            .permissions()
            .plugins()
            .chain(node.permissions().executables())
            .chain(node.permissions().tools())
            .cloned(),
        );
        if let Some(action) = node.action() {
          nodes.insert(action.plugin().clone());
        }
        if let Some(build) = node.build() {
          agents.extend([build.tool.clone(), build.plugin.clone(), build.model_or_tool.clone()]);
          agents.insert(build.result_tool().clone());
          for evidence in &build.evidence {
            agents.extend([evidence.tool().clone(), evidence.plugin().clone()]);
          }
        }
      }
    }
    nodes.extend(agents.iter().cloned());
    Self {
      required_node_capabilities: nodes.into_iter().collect(),
      required_agent_capabilities: agents.into_iter().collect(),
      required_backend_capabilities: octacity_server_factory::PermissionCategory::ALL.to_vec(),
      effective_permissions,
      warnings: vec!["agent_local_permissions_resolved_at_dispatch"],
      ..Self::default()
    }
  }
  pub(crate) fn invalid(field: impl Into<String>) -> ApplicationError {
    ApplicationError::FactoryValidation(Box::new(Self {
      errors: vec![FactoryValidationIssue {
        field: field.into(),
        code: "invalid_contract",
        definition: None,
        node: None,
      }],
      ..Self::default()
    }))
  }
  pub(crate) fn unavailable_binding(
    definition: octacity_server_factory::FlowDefinitionRef,
    node: Option<&octacity_server_factory::FactoryKey>,
    field: &str,
  ) -> ApplicationError {
    let ApplicationError::FactoryValidation(mut report) = Self::unavailable(field) else {
      unreachable!()
    };
    report.errors[0].definition = Some(definition);
    report.errors[0].node = node.cloned();
    ApplicationError::FactoryValidation(report)
  }
  pub(crate) fn unavailable(field: impl Into<String>) -> ApplicationError {
    let field = field.into();
    ApplicationError::FactoryValidation(Box::new(Self {
      errors: vec![FactoryValidationIssue {
        field: field.clone(),
        code: "unavailable_reference",
        definition: None,
        node: None,
      }],
      unavailable_references: vec![field],
      ..Self::default()
    }))
  }
  pub(crate) fn from_factory_error(error: octacity_server_factory::FactoryError) -> ApplicationError {
    use octacity_server_factory::{FactoryChoiceKind, FactoryError};
    if let FactoryError::UnknownChoiceAlias { kind } = error {
      let field = match kind {
        FactoryChoiceKind::AdmissionPolicy => "/admission_policy",
        FactoryChoiceKind::PermissionCeiling => "/permission_ceiling",
        _ => "/flow",
      };
      return Self::unavailable(field);
    }
    ApplicationError::FactoryValidation(Box::new(Self {
      errors: vec![FactoryValidationIssue {
        field: "/flow".to_owned(),
        code: "invalid_contract",
        definition: None,
        node: None,
      }],
      ..Self::default()
    }))
  }
}

/// Successful non-mutating validation and exact alias-resolution preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationValidation {
  /// Prospective resolved configuration; it has not been persisted.
  pub configuration: FactoryConfiguration,
  /// Safe field diagnostics, effective ceilings and required capabilities.
  pub diagnostics: FactoryValidationDiagnostics,
}

/// Result of creating or replacing one Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Immutable configuration committed by the original command.
  pub configuration: FactoryConfigurationProjection,
}

/// Typed Factory Configuration command and query handlers backed by one narrow port.
pub struct FactoryConfigurationHandlers<S> {
  store: Arc<S>,
  policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
  source_actions: Option<Arc<dyn crate::FactoryStageSourceAuthorizer>>,
}

impl<S> FactoryConfigurationHandlers<S> {
  /// Shares the management policy for dependency visibility before alias resolution.
  #[must_use]
  pub fn with_policy(store: Arc<S>, policy: Arc<dyn crate::ManagementAuthorizationPolicy>) -> Self {
    Self {
      store,
      policy,
      source_actions: None,
    }
  }

  /// Creates handlers from a backend-neutral Factory Configuration port.
  #[must_use]
  pub fn new(store: Arc<S>) -> Self {
    Self::with_policy(store, Arc::new(crate::TrustedNetworkManagementPolicy))
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<ListProjectFactoryConfigurationsQuery> for FactoryConfigurationHandlers<S>
where
  S: octacity_server_store::FactoryDiscoveryStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListProjectFactoryConfigurationsQuery,
  ) -> Result<FactoryConfigurationPageProjection, Self::Error> {
    Ok(
      self
        .store
        .list_project_factory_configurations(octacity_server_store::ListProjectFactoryConfigurations::new(
          query.page.project_id(),
          query.page.after(),
          query.page.limit().get(),
          grant
            .visibility_for::<ListProjectFactoryConfigurationsQuery>()
            .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?,
        )?)
        .await?,
    )
  }
}

impl<S> FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore,
{
  /// Enables typed stage effects using current installed Source/Reporter authorization.
  #[must_use]
  pub fn with_source_actions(mut self, authorizer: Arc<dyn crate::FactoryStageSourceAuthorizer>) -> Self {
    self.source_actions = Some(authorizer);
    self
  }
  async fn resolve(
    &self,
    context: &crate::ManagementRequestContext,
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
    project_id: ProjectId,
    definition_digest: FactoryDigest,
    draft: FactoryConfigurationDraft,
  ) -> Result<FactoryConfiguration, ApplicationError> {
    let choices = crate::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      project_id,
    )
    .await?;
    if let Some(flow) = &draft.flow {
      crate::factory_configuration_choices::validate_flow_choices(flow, &choices, "/flow")?;
      crate::factory_stage_source_update::validate_flow_source_updates(
        flow,
        self.source_actions.as_deref(),
        context,
        project_id,
      )
      .await?;
    }
    FactoryConfiguration::publish(id, version, project_id, definition_digest, draft, &choices)
      .map_err(FactoryValidationDiagnostics::from_factory_error)
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<crate::ListFactoryConfigurationChoicesQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: crate::ListFactoryConfigurationChoicesQuery,
  ) -> Result<crate::FactoryConfigurationChoicePageProjection, Self::Error> {
    use crate::{
      FactoryChoiceIdentity, FactoryChoiceReferenceProjection, FactoryConfigurationChoicePageProjection,
      FactoryConfigurationChoiceProjection,
    };
    let visibility = grant
      .visibility_for::<crate::ListFactoryConfigurationChoicesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    if visibility.kind() == octacity_server_store::ReadVisibilityKind::None {
      return Ok(FactoryConfigurationChoicePageProjection::default());
    }
    let FactoryConfigurationAvailability::Available(choices) = self
      .store
      .factory_configuration_availability(query.project_id())
      .await?
    else {
      return Err(ApplicationError::capability_unavailable());
    };
    let entries = choices.entries();
    let visible_references = entries
      .references
      .iter()
      .filter(|entry| {
        visibility.allows(&FactoryChoiceIdentity {
          project_id: query.project_id(),
          kind: entry.kind,
          alias: entry.alias.clone(),
        })
      })
      .cloned()
      .collect::<Vec<_>>();

    let mut items = entries
      .references
      .into_iter()
      .map(|entry| FactoryConfigurationChoiceProjection {
        metadata: choices.choice_metadata(entry.kind, &entry.alias).cloned(),
        identity: FactoryChoiceIdentity {
          project_id: query.project_id(),
          kind: entry.kind,
          alias: entry.alias,
        },
        reference: FactoryChoiceReferenceProjection::Immutable(entry.reference),
      })
      .chain(
        entries
          .build_configurations
          .into_iter()
          .filter(|(_, reference)| reference.project_id() == query.project_id())
          .map(|(alias, reference)| FactoryConfigurationChoiceProjection {
            metadata: choices
              .choice_metadata(octacity_server_factory::FactoryChoiceKind::BuildConfiguration, &alias)
              .cloned(),
            identity: FactoryChoiceIdentity {
              project_id: query.project_id(),
              kind: octacity_server_factory::FactoryChoiceKind::BuildConfiguration,
              alias,
            },
            reference: FactoryChoiceReferenceProjection::BuildConfiguration(reference),
          }),
      )
      .filter(|item| visibility.allows(&item.identity))
      .filter(|item| query.kind().is_none_or(|kind| item.identity.kind == kind))
      .collect::<Vec<_>>();
    for item in &mut items {
      if let Some(metadata) = &mut item.metadata {
        if metadata.permission_declarations.as_ref().is_some_and(|permissions| {
          !crate::factory_configuration_choices::permissions_selectable(permissions, &visible_references)
        }) {
          metadata.permission_declarations = None;
        }

        metadata.compatible_choices.retain(|choice| {
          (choice.kind != octacity_server_factory::FactoryChoiceKind::BuildConfiguration
            || choices
              .resolve_build_configuration(&choice.alias)
              .is_ok_and(|reference| reference.project_id() == query.project_id()))
            && visibility.allows(&FactoryChoiceIdentity {
              project_id: query.project_id(),
              kind: choice.kind,
              alias: choice.alias.clone(),
            })
        });
      }
    }
    let total = items.len() as u64;
    items.sort_by(|left, right| left.identity.cmp(&right.identity));
    items.retain(|item| query.after().is_none_or(|after| &item.identity > after));
    let has_more = items.len() > usize::from(query.limit());
    items.truncate(usize::from(query.limit()));
    let next_cursor = has_more
      .then(|| items.last().map(|item| item.identity.clone()))
      .flatten();
    Ok(FactoryConfigurationChoicePageProjection {
      total,
      items,
      next_cursor,
    })
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<CreateFactoryConfigurationCommand> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: CreateFactoryConfigurationCommand,
  ) -> Result<FactoryConfigurationCommandOutcome, Self::Error> {
    let intent = FactoryConfigurationMutationIntent::Create {
      id: command.id,
      project_id: command.project_id,
      definition_digest: command.definition_digest,
      draft: command.draft.clone(),
    };
    if let Some(outcome) = self
      .store
      .replay_factory_configuration_mutation(audited_mutation(
        context,
        ReplayFactoryConfigurationMutation {
          idempotency_key: command.idempotency_key.clone(),
          intent: intent.clone(),
        },
      )?)
      .await?
    {
      return Ok(outcome.into());
    }
    let configuration = self
      .resolve(
        context,
        command.id,
        FactoryConfigurationVersion::INITIAL,
        command.project_id,
        command.definition_digest,
        command.draft,
      )
      .await?;
    let outcome = self
      .store
      .create_factory_configuration(audited_mutation(
        context,
        CreateFactoryConfiguration {
          configuration,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
          intent,
        },
      )?)
      .await?;
    Ok(outcome.into())
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<ReplaceFactoryConfigurationCommand> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: ReplaceFactoryConfigurationCommand,
  ) -> Result<FactoryConfigurationCommandOutcome, Self::Error> {
    let intent = FactoryConfigurationMutationIntent::Replace {
      id: command.id,
      expected_current_version: command.expected_current_version,
      definition_digest: command.definition_digest,
      draft: command.draft.clone(),
    };
    if let Some(outcome) = self
      .store
      .replay_factory_configuration_mutation(audited_mutation(
        context,
        ReplayFactoryConfigurationMutation {
          idempotency_key: command.idempotency_key.clone(),
          intent: intent.clone(),
        },
      )?)
      .await?
    {
      return Ok(outcome.into());
    }
    let current = self.store.current_factory_configuration(command.id).await?;
    if current.configuration.reference().version() != command.expected_current_version {
      return Err(ApplicationError::PreconditionFailed);
    }

    let next_version = FactoryConfigurationVersion::new(
      command
        .expected_current_version
        .get()
        .checked_add(1)
        .ok_or_else(ApplicationError::unavailable)?,
    )
    .map_err(|_| ApplicationError::unavailable())?;
    let configuration = self
      .resolve(
        context,
        command.id,
        next_version,
        current.configuration.reference().project_id(),
        command.definition_digest,
        command.draft,
      )
      .await?;
    let outcome = self
      .store
      .replace_factory_configuration(audited_mutation(
        context,
        ReplaceFactoryConfiguration {
          expected_current_version: command.expected_current_version,
          configuration,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
          intent,
        },
      )?)
      .await?;
    Ok(outcome.into())
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<ValidateFactoryConfigurationQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: ValidateFactoryConfigurationQuery,
  ) -> Result<FactoryConfigurationValidation, Self::Error> {
    let configuration = self
      .resolve(
        context,
        query.id,
        FactoryConfigurationVersion::INITIAL,
        query.project_id,
        query.definition_digest,
        query.draft,
      )
      .await?;
    let diagnostics = FactoryValidationDiagnostics::for_flow(configuration.flow());
    Ok(FactoryConfigurationValidation {
      configuration,
      diagnostics,
    })
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<GetFactoryConfigurationQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetFactoryConfigurationQuery,
  ) -> Result<FactoryConfigurationProjection, Self::Error> {
    self
      .store
      .factory_configuration_version(query.id, query.version)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<GetCurrentFactoryConfigurationQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetCurrentFactoryConfigurationQuery,
  ) -> Result<FactoryConfigurationProjection, Self::Error> {
    self
      .store
      .current_factory_configuration(query.id)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

impl From<octacity_server_store::PublishedFactoryConfiguration> for FactoryConfigurationProjection {
  fn from(value: octacity_server_store::PublishedFactoryConfiguration) -> Self {
    Self {
      configuration: value.configuration,
      published_at: value.published_at,
    }
  }
}

impl From<octacity_server_store::FactoryConfigurationMutationOutcome> for FactoryConfigurationCommandOutcome {
  fn from(value: octacity_server_store::FactoryConfigurationMutationOutcome) -> Self {
    Self {
      disposition: value.disposition.into(),
      configuration: value.configuration.into(),
    }
  }
}
