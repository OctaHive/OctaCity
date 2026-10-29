use octacity_server_domain::ProjectId;
use octacity_server_factory::{BuildConfigurationRef, FactoryChoiceKind, ImmutableReference};
pub use octacity_server_factory::{FactoryChoiceIdentity, FactoryChoiceMetadata};
use std::num::NonZeroU16;

/// Published technical categories; logical plugin and policy aliases remain configurable.
pub fn factory_configuration_choice_kinds() -> impl Iterator<Item = &'static str> {
  FactoryChoiceKind::ALL.into_iter().map(FactoryChoiceKind::as_str)
}

/// Closed technical node primitives accepted by the common interpreter contract.
pub fn factory_flow_node_kinds() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::FlowNodeKind::VALUES
    .iter()
    .map(|kind| kind.as_str())
}

/// Closed purposes published by installed Decision Signal capabilities.
pub fn factory_signal_purposes() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::DecisionSignalPurpose::VALUES
    .iter()
    .map(|purpose| purpose.as_str())
}

/// Closed rollout modes published by installed Decision Signal capabilities.
pub fn factory_signal_modes() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::DecisionSignalMode::VALUES
    .iter()
    .map(|mode| mode.as_str())
}

/// Closed fail-closed fallbacks published by installed Decision Signal capabilities.
pub fn factory_signal_fallbacks() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::DecisionSignalFallback::VALUES
    .iter()
    .map(|fallback| fallback.as_str())
}

/// Closed enforcement categories published by installed capabilities.
pub fn factory_permission_categories() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::PermissionCategory::ALL
    .into_iter()
    .map(|category| category.as_str())
}

/// Domain-owned cardinality and budget bounds used by management schemas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationDocumentBounds {
  /// Maximum complete immutable journal records, bounding linked diagnostic histories.
  pub journal_records: usize,
  /// Maximum combined argv bytes.
  pub argument_bytes: usize,
  /// Maximum role/profile collection entries.
  pub role_entries: usize,
  /// Combined instruction bytes in one role or profile.
  pub instruction_bytes: usize,
  /// Encoded role/profile definition bytes.
  pub role_definition_bytes: usize,
  /// Maximum inline encoded avatar metadata.
  pub role_avatar_bytes: usize,
  /// Bounded UTF-8 display/environment value bytes.
  pub text_bytes: usize,
  /// Imported skill file count.
  pub skill_files: usize,
  /// Normalized imported path bytes.
  pub skill_path_bytes: usize,
  /// Maximum bytes of one imported file.
  pub skill_file_bytes: u64,
  /// Maximum aggregate imported file bytes.
  pub skill_total_bytes: u64,
  /// Maximum admitted per-stage override entries.
  pub profile_overrides: usize,
  /// Maximum typed questions and answers retained by one Decision Signal.
  pub signal_questions: usize,
  /// Maximum finite answers consumed by one profile selector.
  pub signal_choices: usize,
  /// Fixed-point denominator for probability and confidence values.
  pub signal_probability_scale: u32,
  /// Maximum aliases or compatibility relationships in one technical category.
  pub choice_entries: usize,
  /// Maximum safe Work metadata entries.
  pub work_metadata_entries: usize,
  /// Maximum safe metadata key bytes.
  pub work_metadata_key_bytes: usize,
  /// Maximum safe metadata value bytes.
  pub work_metadata_value_bytes: usize,
  /// Maximum source-scoped external identity bytes.
  pub external_work_identity_bytes: usize,

  /// Maximum references in one permission category.
  pub permission_entries: usize,
  /// Maximum command arguments.
  pub command_arguments: usize,
  /// Maximum CPU millicores.
  pub cpu_millis: u32,
  /// Maximum memory bytes.
  pub memory_bytes: u64,
  /// Maximum temporary disk bytes.
  pub disk_bytes: u64,
  /// Maximum processes or descendants.
  pub processes: u32,
  /// Maximum artifacts or reports.
  pub output_count: u32,
  /// Maximum required evidence outputs.
  pub evidence_items: usize,
  /// Maximum nodes in one immutable definition.
  pub nodes: usize,
  /// Maximum transitions in one immutable definition.
  pub transitions: usize,
  /// Maximum reachable immutable definitions in one closure.
  pub definitions: usize,
  /// Maximum declared schema fields or local named policies.
  pub schema_entries: usize,
  /// Maximum UTF-8 bytes in one externally tagged data variant name.
  pub schema_variant_name_bytes: usize,
  /// Maximum declared schema array items.
  pub schema_items: u16,
  /// Maximum configured data schema and mapping nesting depth.
  pub schema_depth: usize,
  /// Maximum retained string value bytes.
  pub value_bytes: usize,
  /// Maximum stable key bytes.
  pub key_bytes: usize,
  /// Maximum nesting depth.
  pub depth: u16,
  /// Maximum expanded node executions.
  pub expanded_nodes: u32,
  /// Maximum repeat count.
  pub repeats: u16,
  /// Maximum successor fan-out.
  pub fan_out: u16,
  /// Maximum concurrently admitted runs.
  pub active_runs: u32,
  /// Maximum concurrently active nodes.
  pub active_nodes: u32,
  /// Maximum permitted attempts.
  pub attempts: u32,
  /// Maximum elapsed milliseconds.
  pub elapsed_millis: u64,
  /// Maximum model tokens.
  pub tokens: u64,
  /// Maximum cost in micro-units.
  pub cost_micro_units: u64,
  /// Maximum published output bytes.
  pub output_bytes: u64,
}

/// Returns construction bounds from the core rather than duplicating safety parameters.
#[must_use]
pub const fn factory_configuration_document_bounds() -> FactoryConfigurationDocumentBounds {
  use octacity_server_factory as factory;
  FactoryConfigurationDocumentBounds {
    journal_records: octacity_server_store::MAX_FACTORY_RUN_SNAPSHOT_RECORDS,
    argument_bytes: factory::MAX_COMMAND_ARGUMENT_BYTES,
    role_entries: factory::MAX_FACTORY_ROLE_PROFILE_ENTRIES,
    instruction_bytes: factory::MAX_FACTORY_ROLE_INSTRUCTION_BYTES,
    role_definition_bytes: factory::MAX_FACTORY_ROLE_DEFINITION_BYTES,
    role_avatar_bytes: factory::MAX_FACTORY_ROLE_AVATAR_BYTES,
    text_bytes: factory::MAX_FACTORY_TEXT_BYTES,
    skill_files: factory::MAX_CONTEXT_MANIFEST_ENTRIES,
    skill_path_bytes: factory::MAX_FACTORY_REPOSITORY_PATH_BYTES,
    skill_file_bytes: factory::MAX_CONTEXT_ENTRY_BYTES,
    skill_total_bytes: factory::MAX_CONTEXT_MANIFEST_BYTES,
    profile_overrides: factory::MAX_FLOW_DEFINITION_NODES,
    signal_questions: factory::MAX_DECISION_SIGNAL_QUESTIONS,
    signal_choices: factory::MAX_DECISION_SIGNAL_CHOICES,
    signal_probability_scale: factory::DECISION_SIGNAL_PROBABILITY_SCALE,
    choice_entries: factory::MAX_CONFIGURATION_CHOICES_PER_KIND,
    work_metadata_entries: factory::MAX_FACTORY_METADATA_ENTRIES,
    work_metadata_key_bytes: factory::MAX_FACTORY_METADATA_KEY_BYTES,
    work_metadata_value_bytes: factory::MAX_FACTORY_METADATA_VALUE_BYTES,
    external_work_identity_bytes: factory::MAX_EXTERNAL_WORK_IDENTITY_BYTES,

    permission_entries: factory::MAX_PERMISSION_ENTRIES_PER_CATEGORY,
    command_arguments: factory::MAX_COMMAND_ARGUMENTS,
    cpu_millis: factory::MAX_FACTORY_CPU_MILLIS,
    memory_bytes: factory::MAX_FACTORY_MEMORY_BYTES,
    disk_bytes: factory::MAX_FACTORY_DISK_BYTES,
    processes: factory::MAX_FACTORY_PROCESS_COUNT,
    output_count: factory::MAX_FACTORY_OUTPUT_COUNT,
    evidence_items: factory::MAX_EVIDENCE_ITEMS,
    nodes: factory::MAX_FLOW_DEFINITION_NODES,
    transitions: factory::MAX_FLOW_DEFINITION_EDGES,
    definitions: factory::MAX_FLOW_DEFINITION_CLOSURE,
    schema_entries: factory::MAX_FLOW_SCHEMA_ENTRIES,
    schema_variant_name_bytes: factory::MAX_FLOW_VARIANT_NAME_BYTES,
    schema_items: factory::MAX_FLOW_SCHEMA_ITEMS,
    schema_depth: factory::MAX_FLOW_SCHEMA_DEPTH,
    value_bytes: factory::MAX_FLOW_DATA_BYTES,
    key_bytes: factory::MAX_FACTORY_KEY_BYTES,
    depth: factory::MAX_FLOW_NESTING_DEPTH,
    expanded_nodes: factory::MAX_FLOW_EXPANDED_NODES,
    repeats: factory::MAX_FLOW_REPEAT_COUNT,
    fan_out: factory::MAX_FLOW_FAN_OUT,
    active_runs: factory::MAX_FACTORY_ACTIVE_RUNS,
    active_nodes: factory::MAX_FACTORY_ACTIVE_NODES,
    attempts: factory::MAX_FACTORY_ATTEMPTS,
    elapsed_millis: factory::MAX_FACTORY_ELAPSED_MILLIS,
    tokens: factory::MAX_FACTORY_TOKENS,
    cost_micro_units: factory::MAX_FACTORY_COST_MICRO_UNITS,
    output_bytes: factory::MAX_FACTORY_OUTPUT_BYTES,
  }
}

use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, Query, management_security::owned_collection_resource,
};

/// A bounded page of choices selectable for one Project and deployment policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryConfigurationChoicesQuery {
  project_id: ProjectId,
  kind: Option<FactoryChoiceKind>,
  after: Option<FactoryChoiceIdentity>,
  limit: NonZeroU16,
}

impl ListFactoryConfigurationChoicesQuery {
  /// Validates category, Project-scoped continuation, and page bounds.
  pub fn try_new(
    project_id: ProjectId,
    kind: Option<FactoryChoiceKind>,
    after: Option<FactoryChoiceIdentity>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= octacity_server_store::MAX_FACTORY_CONFIGURATION_PAGE_SIZE)
      .ok_or_else(ApplicationError::invalid)?;
    if after.as_ref().is_some_and(|position| position.project_id != project_id) {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      project_id,
      kind,
      after,
      limit,
    })
  }
  /// Returns the catalog's owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }
  /// Returns the optional exact technical category filter.
  #[must_use]
  pub const fn kind(&self) -> Option<FactoryChoiceKind> {
    self.kind
  }
  /// Borrows the exclusive continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<&FactoryChoiceIdentity> {
    self.after.as_ref()
  }
  /// Returns the validated positive page bound.
  #[must_use]
  pub const fn limit(&self) -> u16 {
    self.limit.get()
  }
}

impl Query for ListFactoryConfigurationChoicesQuery {
  type Outcome = FactoryConfigurationChoicePageProjection;
}

impl ManagementAuthorizationTarget for ListFactoryConfigurationChoicesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::FactoryChoice,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryChoice,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Exact public reference facts, with no credential material or provider payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryChoiceReferenceProjection {
  /// Logical immutable policy or installed capability reference.
  Immutable(ImmutableReference),
  /// Exact Project-owned Build Configuration.
  BuildConfiguration(BuildConfigurationRef),
}

/// One visible selectable alias and its exact frozen reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationChoiceProjection {
  /// Authorization and continuation identity scoped to this Project.
  pub identity: FactoryChoiceIdentity,
  /// Exact selectable reference facts.
  pub reference: FactoryChoiceReferenceProjection,
  /// Optional deployment-declared hints, with hidden compatible aliases removed.
  pub metadata: Option<octacity_server_factory::FactoryChoiceMetadata>,
}

/// Visible count and deterministic bounded choices, derived after authorization.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryConfigurationChoicePageProjection {
  /// Matching visible count before continuation.
  pub total: u64,
  /// Choices ordered by category and alias.
  pub items: Vec<FactoryConfigurationChoiceProjection>,
  /// Exclusive position when more matching visible choices remain.
  pub next_cursor: Option<FactoryChoiceIdentity>,
}

pub(super) async fn visible_factory_choices<S: octacity_server_store::FactoryConfigurationStore>(
  store: &S,
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  project: ProjectId,
) -> Result<octacity_server_factory::FactoryConfigurationChoices, ApplicationError> {
  visible_factory_choices_from(store, policy, context, project, ChoiceSource::Configured).await
}

pub(super) async fn visible_installed_factory_choices<S: octacity_server_store::FactoryConfigurationStore>(
  store: &S,
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  project: ProjectId,
) -> Result<octacity_server_factory::FactoryConfigurationChoices, ApplicationError> {
  visible_factory_choices_from(store, policy, context, project, ChoiceSource::Installation).await
}

enum ChoiceSource {
  Installation,
  Configured,
}

async fn visible_factory_choices_from<S: octacity_server_store::FactoryConfigurationStore>(
  store: &S,
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  project: ProjectId,
  source: ChoiceSource,
) -> Result<octacity_server_factory::FactoryConfigurationChoices, ApplicationError> {
  let resource = crate::management_security::owned_collection_resource(
    ManagementResourceKind::FactoryChoice,
    ManagementResourceKind::Project,
    project,
  )
  .map_err(|_| ApplicationError::invalid())?;
  let grant = policy
    .authorize(context, ManagementAction::View, &resource)
    .await
    .map_err(|_| ApplicationError::invalid())?;
  let visibility = grant
    .visibility_for::<ListFactoryConfigurationChoicesQuery>()
    .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
  if visibility.kind() == octacity_server_store::ReadVisibilityKind::None {
    return octacity_server_factory::FactoryConfigurationChoices::try_new(Default::default())
      .map_err(|_| ApplicationError::invalid());
  }
  let availability = match source {
    ChoiceSource::Installation => store.factory_configuration_installation(project).await?,
    ChoiceSource::Configured => store.factory_configuration_availability(project).await?,
  };
  let octacity_server_store::FactoryConfigurationAvailability::Available(choices) = availability else {
    return Err(ApplicationError::capability_unavailable());
  };
  let entries = choices.entries();
  let identity = |kind, alias| FactoryChoiceIdentity {
    project_id: project,
    kind,
    alias,
  };
  let mut visible = octacity_server_factory::FactoryConfigurationChoices::try_new(
    octacity_server_factory::FactoryConfigurationChoiceEntries {
      references: entries
        .references
        .into_iter()
        .filter(|entry| visibility.allows(&identity(entry.kind, entry.alias.clone())))
        .collect(),
      build_configurations: entries
        .build_configurations
        .into_iter()
        .filter(|(alias, reference)| {
          reference.project_id() == project
            && visibility.allows(&identity(
              octacity_server_factory::FactoryChoiceKind::BuildConfiguration,
              alias.clone(),
            ))
        })
        .collect(),
    },
  )
  .map_err(|_| ApplicationError::invalid())?;
  for contract in choices.build_pipeline_contracts() {
    if let Ok(qualified) = visible.clone().with_build_pipeline_contract(contract.clone()) {
      visible = qualified;
    }
  }
  visible.intersect_role_availability(&choices);
  for (alias, cap) in choices.role_profile_capabilities() {
    if let Some(cap) = visible.available_role_profile_capability(alias, cap) {
      visible = visible
        .with_role_profile_capability(alias.clone(), cap)
        .map_err(|_| ApplicationError::invalid())?;
      if let Some(mut support) = choices.runtime_settings_support(alias).cloned() {
        let references = visible.entries().references;
        support.mcp_executables.retain(|_, reference| {
          references
            .iter()
            .any(|r| r.kind == octacity_server_factory::FactoryChoiceKind::NodeCapability && &r.reference == reference)
        });
        visible = visible
          .with_runtime_settings_support(alias.clone(), support)
          .map_err(|_| ApplicationError::invalid())?;
      }
    }
  }
  // Retain selector constraints for validation of visible policies. This is not a
  // discovery response; its separate projection still filters private dependencies.
  for entry in visible.entries().references {
    if let Some(selector) = choices
      .choice_metadata(entry.kind, &entry.alias)
      .and_then(|metadata| metadata.profile_selection.as_ref())
    {
      visible = visible
        .with_choice_metadata(
          entry.kind,
          entry.alias,
          octacity_server_factory::FactoryChoiceMetadata {
            profile_selection: Some(selector.clone()),
            ..Default::default()
          },
        )
        .map_err(|_| ApplicationError::invalid())?;
    }
  }
  Ok(visible)
}

/// Exact capability and credential references selectable in a permission declaration.
pub(super) fn permissions_selectable(
  permissions: &octacity_server_factory::FactoryPermissionSet,
  references: &[octacity_server_factory::FactoryReferenceChoice],
) -> bool {
  permissions
    .plugins()
    .chain(permissions.executables())
    .chain(permissions.tools())
    .all(|reference| {
      references
        .iter()
        .any(|entry| entry.kind == FactoryChoiceKind::NodeCapability && &entry.reference == reference)
    })
    && [
      (
        FactoryChoiceKind::SecretProfile,
        permissions.secret_profiles().collect::<Vec<_>>(),
      ),
      (
        FactoryChoiceKind::WorkloadIdentityProfile,
        permissions.workload_identity_profiles().collect(),
      ),
    ]
    .into_iter()
    .all(|(kind, names)| {
      names.into_iter().all(|name| {
        references
          .iter()
          .any(|entry| entry.kind == kind && (&entry.alias == name || entry.reference.identity() == name))
      })
    })
}

/// Checks installed identities throughout a caller-owned graph without interpreting logical stages.
pub(super) fn validate_flow_choices(
  flow: &octacity_server_factory::FactoryFlowConfiguration,
  choices: &octacity_server_factory::FactoryConfigurationChoices,
  prefix: &str,
) -> Result<(), ApplicationError> {
  use octacity_server_factory::{FactoryChoiceKind, FlowGateProgram};
  let entries = choices.entries();
  let selectable = |kind, reference: &ImmutableReference| {
    entries
      .references
      .iter()
      .any(|entry| entry.kind == kind && &entry.reference == reference)
  };
  if !permissions_selectable(flow.limits.permissions(), &entries.references) {
    return Err(crate::FactoryValidationDiagnostics::unavailable(format!(
      "{prefix}/limits/permissions"
    )));
  }
  for definition in flow.closure.definitions() {
    if definition
      .execution()
      .verification()
      .is_some_and(|policy| !selectable(FactoryChoiceKind::ExecutionPolicy, &policy.data_handling))
    {
      return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
        definition.reference(),
        None,
        "/execution/verification/data_handling",
      ));
    }
    if !permissions_selectable(definition.execution().permissions(), &entries.references) {
      return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
        definition.reference(),
        None,
        "/execution/permissions",
      ));
    }
    for node in definition.nodes() {
      if !permissions_selectable(node.permissions(), &entries.references) {
        return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
          definition.reference(),
          Some(node.key()),
          "/permissions",
        ));
      }
      if node
        .action()
        .is_some_and(|action| !selectable(FactoryChoiceKind::NodeCapability, action.plugin()))
      {
        return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
          definition.reference(),
          Some(node.key()),
          "/action/plugin",
        ));
      }
      if node.gate().is_some_and(|gate| {
        FlowGateProgram::from_node(node).is_err() && !selectable(FactoryChoiceKind::ExecutionPolicy, gate.policy())
      }) {
        return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
          definition.reference(),
          Some(node.key()),
          "/gate/policy",
        ));
      }
      if node
        .profile_selection()
        .is_some_and(|selection| selection.validate_capabilities(choices).is_err())
      {
        return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
          definition.reference(),
          Some(node.key()),
          "/profile_selection",
        ));
      }
      if node
        .build_bindings()
        .any(|build| !choices.supports_build_binding(build))
      {
        return Err(crate::FactoryValidationDiagnostics::unavailable_binding(
          definition.reference(),
          Some(node.key()),
          "/build",
        ));
      }
    }
  }
  // Extension parameters are bounded data owned by the exact plugin/policy contract.
  // They never acquire authority from their JSON field names. Typed Build bindings,
  // capabilities and permission ceilings above carry the graph's selectable authority.
  Ok(())
}

pub(super) async fn require_visible_reference(
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  kind: ManagementResourceKind,
  id: impl ToString,
) -> Result<(), ApplicationError> {
  require_authorized_reference(policy, context, ManagementAction::View, kind, id).await
}
pub(super) async fn require_authorized_reference(
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  action: ManagementAction,
  kind: ManagementResourceKind,
  id: impl ToString,
) -> Result<(), ApplicationError> {
  let resource =
    crate::management_security::instance_resource(kind, id.to_string()).map_err(|_| ApplicationError::invalid())?;
  let grant = policy
    .authorize(context, action, &resource)
    .await
    .map_err(|_| ApplicationError::invalid())?;
  match grant.visibility().view() {
    crate::ManagementVisibilityView::All => Ok(()),
    crate::ManagementVisibilityView::Restricted(resources) if resources.contains(&resource) => Ok(()),
    _ => Err(ApplicationError::invalid()),
  }
}

/// Closed severity vocabulary for public discovery and evidence contracts.
pub fn factory_finding_severities() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::FindingSeverity::VALUES
    .iter()
    .map(|value| value.as_str())
}

/// Closed authoritative disposition values accepted by discovery.
pub fn factory_decision_outcomes() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::DecisionOutcome::VALUES
    .iter()
    .copied()
    .map(|value| value.as_str())
}
/// Closed delivery classifications accepted by discovery.
pub fn factory_delivery_states() -> impl Iterator<Item = &'static str> {
  octacity_server_factory::DeliveryState::VALUES
    .iter()
    .copied()
    .map(|value| value.as_str())
}
