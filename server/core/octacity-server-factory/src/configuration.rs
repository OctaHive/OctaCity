use std::{
  collections::{BTreeMap, HashSet},
  fmt,
  str::FromStr,
};

use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId};
use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose, DecisionSignalRouteSet,
  FactoryChoiceKind, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryDigest,
  FactoryError, FactoryKey, MAX_CRITERION_PACKS, MAX_EVALUATORS,
};

/// Maximum number of aliases accepted in one configuration choice category.
pub const MAX_CONFIGURATION_CHOICES_PER_KIND: usize = 256;
/// Maximum concurrently admitted runs under one Factory Configuration.
pub const MAX_FACTORY_ACTIVE_RUNS: u32 = 10_000;
/// Maximum concurrently active node attempts under one Factory Configuration.
pub const MAX_FACTORY_ACTIVE_NODES: u32 = 1_000;

/// Exact immutable identity selected from a configuration choice catalog.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImmutableReference {
  identity: FactoryKey,
  version: FactoryKey,
  digest: FactoryDigest,
}

impl ImmutableReference {
  /// Constructs a versioned, content-addressed reference.
  #[must_use]
  pub const fn new(identity: FactoryKey, version: FactoryKey, digest: FactoryDigest) -> Self {
    Self {
      identity,
      version,
      digest,
    }
  }

  /// Returns the provider-neutral logical identity.
  #[must_use]
  pub const fn identity(&self) -> &FactoryKey {
    &self.identity
  }

  /// Returns the exact immutable version selected at publication.
  #[must_use]
  pub const fn version(&self) -> &FactoryKey {
    &self.version
  }

  /// Returns the exact definition or capability digest.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }
}

/// Exact Project-owned Build Configuration version used by a Factory stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfigurationRef {
  id: BuildConfigurationId,
  version: BuildConfigurationVersion,
  project_id: ProjectId,
  definition_digest: FactoryDigest,
}

impl BuildConfigurationRef {
  /// Constructs an exact Build Configuration reference.
  #[must_use]
  pub const fn new(
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
    project_id: ProjectId,
    definition_digest: FactoryDigest,
  ) -> Self {
    Self {
      id,
      version,
      project_id,
      definition_digest,
    }
  }

  /// Returns the stable Build Configuration identity.
  #[must_use]
  pub const fn id(&self) -> BuildConfigurationId {
    self.id
  }

  /// Returns the exact immutable Build Configuration version.
  #[must_use]
  pub const fn version(&self) -> BuildConfigurationVersion {
    self.version
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the exact Build Configuration definition digest.
  #[must_use]
  pub const fn definition_digest(&self) -> FactoryDigest {
    self.definition_digest
  }
}

/// One typed alias for an exact immutable configuration reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryReferenceChoice {
  /// Reference category resolved by the configuration field.
  pub kind: FactoryChoiceKind,
  /// Stable operator-facing alias scoped to the category.
  pub alias: FactoryKey,
  /// Exact versioned identity selected by the alias.
  pub reference: ImmutableReference,
}

/// Project-scoped selectable alias; identical names in other Projects grant no access.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactoryChoiceIdentity {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Closed technical reference category.
  pub kind: FactoryChoiceKind,
  /// Operator-facing alias within this Project and category.
  pub alias: FactoryKey,
}

impl fmt::Display for FactoryChoiceIdentity {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}:{}:{}", self.project_id, self.kind.as_str(), self.alias)
  }
}

impl FromStr for FactoryChoiceIdentity {
  type Err = FactoryError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let invalid = || FactoryError::InvalidConfiguration {
      field: "configuration choice identity",
    };
    let mut parts = value.split(':');
    let (Some(project), Some(kind), Some(alias), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
      return Err(invalid());
    };
    let identity = Self {
      project_id: project.parse().map_err(|_| invalid())?,
      kind: serde_json::from_value(serde_json::Value::String(kind.to_owned())).map_err(|_| invalid())?,
      alias: alias.parse()?,
    };
    if identity.to_string() != value {
      return Err(invalid());
    }
    Ok(identity)
  }
}

/// Bounded aliases and exact authorized identities available during publication.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryConfigurationChoiceEntries {
  /// Selectable immutable references across all non-Build categories.
  pub references: Vec<FactoryReferenceChoice>,
  /// Selectable Project-owned Build Configuration versions.
  pub build_configurations: Vec<(FactoryKey, BuildConfigurationRef)>,
}

/// Validated bounded choice catalogs used to resolve a configuration draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationChoices {
  references: BTreeMap<(FactoryChoiceKind, FactoryKey), ImmutableReference>,
  build_configurations: BTreeMap<FactoryKey, BuildConfigurationRef>,
  metadata: BTreeMap<(FactoryChoiceKind, FactoryKey), FactoryChoiceMetadata>,
  role_profiles: BTreeMap<FactoryKey, crate::FactoryRoleProfileCapability>,
  runtime_settings: BTreeMap<FactoryKey, crate::FactoryRuntimeSettingsSupport>,
  excluded_roles: std::collections::BTreeSet<crate::FactoryRoleId>,
}

/// Safe deployment-declared capabilities. Absence means no declaration is available;
/// these hints never grant execution authority or replace exact contract validation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryChoiceMetadata {
  /// Technical node kinds supported by this installed choice.
  pub node_kinds: Vec<crate::FlowNodeKind>,
  /// Supported non-authoritative signal purposes.
  pub signal_purposes: Vec<crate::DecisionSignalPurpose>,
  /// Restrictions this installed capability can enforce.
  pub enforcement: Vec<crate::PermissionCategory>,
  /// Compatible aliases in the same Project catalog, filtered again for the reader.
  pub compatible_choices: Vec<FactoryChoiceCompatibility>,
  /// Bounded deployment-provided declarations for guided permission authoring.
  /// These selectable values are not an execution grant; publication and admission
  /// still validate exact references and intersect authoritative policy.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub permission_declarations: Option<crate::FactoryPermissionSet>,
  /// Read-only facts projected from an installed profile selector, never provider input or credentials.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub profile_selection: Option<FactoryProfileSelectorMetadata>,
}

/// Safe installation facts for Console profile-selection authoring.
/// The complete private plan remains in its existing installed owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProfileSelectorMetadata {
  /// Exact installed provider identity.
  pub provider: ImmutableReference,
  /// Exact installed protocol adapter.
  pub adapter: ImmutableReference,
  /// Exact installed model.
  pub model: ImmutableReference,
  /// Semantic criteria version; question text remains private plan content.
  pub question_set: ImmutableReference,
  /// Applicable calibration identity.
  pub calibration: ImmutableReference,
  /// Meaning of provider probabilities.
  pub probability_semantics: ImmutableReference,
  /// Installed rollout authority, independent of Flow authoring.
  pub mode: crate::DecisionSignalMode,
  /// Explicit candidate used by shadow/advisory.
  pub baseline: FactoryKey,
  /// Complete finite answer keys; Flow candidates bind profiles to these exact keys.
  pub candidates: crate::DecisionSignalChoices,
  /// Minimum selected-answer probability in millionths.
  pub minimum_probability: crate::DecisionSignalProbability,
  /// Minimum lead over the next answer in millionths.
  pub minimum_margin: crate::DecisionSignalProbability,
}

/// One typed compatibility relationship within a Project choice catalog.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryChoiceCompatibility {
  /// Closed technical category of the compatible choice.
  pub kind: FactoryChoiceKind,
  /// Selectable alias in that category.
  pub alias: FactoryKey,
}

impl FactoryConfigurationChoices {
  /// Constructs bounded catalogs and rejects duplicate aliases within a category.
  pub fn try_new(entries: FactoryConfigurationChoiceEntries) -> Result<Self, FactoryError> {
    Ok(Self {
      references: reference_choice_map(entries.references)?,
      build_configurations: choice_map(entries.build_configurations, FactoryChoiceKind::BuildConfiguration)?,
      metadata: BTreeMap::new(),
      role_profiles: BTreeMap::new(),
      runtime_settings: BTreeMap::new(),
      excluded_roles: Default::default(),
    })
  }

  /// Attaches a bounded declaration from the deployment owner to an installed alias.
  /// Unknown aliases and relationships cannot create new selectable choices.
  pub fn with_choice_metadata(
    mut self,
    kind: FactoryChoiceKind,
    alias: FactoryKey,
    mut metadata: FactoryChoiceMetadata,
  ) -> Result<Self, FactoryError> {
    let installed = |kind, alias: &FactoryKey| {
      if kind == FactoryChoiceKind::BuildConfiguration {
        self.build_configurations.contains_key(alias)
      } else {
        self.references.contains_key(&(kind, alias.clone()))
      }
    };
    if !installed(kind, &alias)
      || metadata
        .compatible_choices
        .iter()
        .any(|choice| !installed(choice.kind, &choice.alias))
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "choice metadata alias",
      });
    }
    if metadata.node_kinds.len() > crate::FlowNodeKind::VALUES.len()
      || metadata.signal_purposes.len() > crate::DecisionSignalPurpose::VALUES.len()
      || metadata.enforcement.len() > crate::PermissionCategory::ALL.len()
      || metadata.compatible_choices.len() > MAX_CONFIGURATION_CHOICES_PER_KIND
    {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "choice metadata",
      });
    }
    metadata.node_kinds.sort();
    metadata.node_kinds.dedup();
    metadata.signal_purposes.sort();
    metadata.signal_purposes.dedup();
    metadata.enforcement.sort();
    metadata.enforcement.dedup();
    metadata.compatible_choices.sort();
    metadata.compatible_choices.dedup();
    self.metadata.insert((kind, alias), metadata);
    Ok(self)
  }

  /// Declares private configuration support on an existing installed Runtime alias.
  /// Transport paths stay out of generic choice metadata and discovery responses.
  pub fn with_runtime_settings_support(
    mut self,
    alias: FactoryKey,
    support: crate::FactoryRuntimeSettingsSupport,
  ) -> Result<Self, FactoryError> {
    if !self.role_profiles.contains_key(&alias)
      || support.mcp_executables.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || support.mcp_executables.iter().any(|(path, reference)| {
        crate::FactoryPath::new(path).is_err()
          || !self
            .references
            .iter()
            .any(|((kind, _), value)| *kind == FactoryChoiceKind::NodeCapability && value == reference)
      })
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "Runtime settings support",
      });
    }
    self.runtime_settings.insert(alias, support);
    Ok(self)
  }
  /// Dedicated installed support used by authorized Runtime configuration.
  pub fn runtime_settings_support(&self, alias: &FactoryKey) -> Option<&crate::FactoryRuntimeSettingsSupport> {
    self.runtime_settings.get(alias)
  }

  /// Attaches a trusted role-profile declaration to an already installed runtime alias.
  /// This extends the same catalog; it cannot install an executable or grant credentials.
  pub fn with_role_profile_capability(
    mut self,
    alias: FactoryKey,
    capability: crate::FactoryRoleProfileCapability,
  ) -> Result<Self, FactoryError> {
    if self.references.get(&(FactoryChoiceKind::NodeCapability, alias.clone()))
      != Some(&capability.ceiling.toolchain.executable)
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "role profile capability alias",
      });
    }
    capability
      .ceiling
      .validate(capability.ceiling.build_configuration.project_id())?;
    if capability.public_environment.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "profile environment schema",
      });
    }
    self.role_profiles.insert(alias, capability);
    Ok(self)
  }
  /// Removes a deleted agent from all new candidate qualification, preserving retained content.
  pub fn exclude_role(&mut self, id: crate::FactoryRoleId) {
    self.excluded_roles.insert(id);
  }
  /// Keeps dynamic exclusions when an authorized subset rebuilds selectable entries.
  /// Intersection can only narrow role eligibility, never restore a deleted role.
  pub fn intersect_role_availability(&mut self, source: &Self) {
    self.excluded_roles.extend(source.excluded_roles.iter().copied());
  }
  /// Current eligibility is independent of immutable Role/Profile version reads.
  #[must_use]
  pub fn role_is_available(&self, id: crate::FactoryRoleId) -> bool {
    !self.excluded_roles.contains(&id)
  }
  /// Borrows exact trusted declarations; authorization filters their dependencies before disclosure.
  #[must_use]
  pub fn role_profile_capabilities(&self) -> &BTreeMap<FactoryKey, crate::FactoryRoleProfileCapability> {
    &self.role_profiles
  }

  /// Restricts an installed declaration to dependencies selectable in this catalog.
  /// Optional unavailable entries are removed from the ceiling, never from a submitted profile.
  /// Callers supply the authorized catalog when filtering operator-visible choices.
  #[must_use]
  pub fn available_role_profile_capability(
    &self,
    alias: &FactoryKey,
    capability: &crate::FactoryRoleProfileCapability,
  ) -> Option<crate::FactoryRoleProfileCapability> {
    let installed = |reference: &ImmutableReference| {
      self
        .references
        .iter()
        .any(|((kind, _), entry)| *kind == FactoryChoiceKind::NodeCapability && entry == reference)
    };
    let definition = &capability.ceiling;
    if !self
      .references
      .contains_key(&(FactoryChoiceKind::NodeCapability, alias.clone()))
      || !self
        .build_configurations
        .values()
        .any(|reference| reference == &definition.build_configuration)
      || ![
        &definition.toolchain.plugin,
        &definition.toolchain.executable,
        &definition.toolchain.model,
      ]
      .into_iter()
      .all(installed)
    {
      return None;
    }
    let named = |kind, name: &FactoryKey| {
      self
        .references
        .iter()
        .any(|((entry_kind, alias), reference)| *entry_kind == kind && (alias == name || reference.identity() == name))
    };
    let permissions = &definition.permissions;
    let mut available = capability.clone();
    available.ceiling.permissions = crate::FactoryPermissionSet::try_new(crate::FactoryPermissionDraft {
      plugins: permissions
        .plugins()
        .filter(|reference| installed(reference))
        .cloned()
        .collect(),
      executables: permissions
        .executables()
        .filter(|reference| installed(reference))
        .cloned()
        .collect(),
      tools: permissions
        .tools()
        .filter(|reference| installed(reference))
        .cloned()
        .collect(),
      commands: permissions
        .commands()
        .filter(|command| installed(command.executable()))
        .cloned()
        .collect(),
      secret_profiles: permissions
        .secret_profiles()
        .filter(|name| named(FactoryChoiceKind::SecretProfile, name))
        .cloned()
        .collect(),
      workload_identity_profiles: permissions
        .workload_identity_profiles()
        .filter(|name| named(FactoryChoiceKind::WorkloadIdentityProfile, name))
        .cloned()
        .collect(),
      max_descendants: permissions.max_descendants(),
      mounts: permissions.mounts().cloned().collect(),
      network_hosts: permissions.network_hosts().cloned().collect(),
      resources: permissions.resources(),
      outputs: permissions.outputs().clone(),
    })
    .ok()?;
    available.ceiling.skills.retain(|_, reference| installed(reference));
    available
      .ceiling
      .execution
      .mcp_servers
      .retain(|_, binding| installed(&binding.configuration));
    Some(available)
  }

  /// Returns a safe declaration for an exact installed category/alias.
  #[must_use]
  pub fn choice_metadata(&self, kind: FactoryChoiceKind, alias: &FactoryKey) -> Option<&FactoryChoiceMetadata> {
    self.metadata.get(&(kind, alias.clone()))
  }

  /// Borrows declared selector facts only for the same exact installed policy.
  pub(crate) fn profile_selector_metadata(
    &self,
    policy: &ImmutableReference,
  ) -> Option<&FactoryProfileSelectorMetadata> {
    self.references.iter().find_map(|((kind, alias), reference)| {
      if *kind == FactoryChoiceKind::DecisionSignalPolicy && reference == policy {
        self.choice_metadata(*kind, alias)?.profile_selection.as_ref()
      } else {
        None
      }
    })
  }

  /// Returns the bounded exact catalog in deterministic category/alias order.
  #[must_use]
  pub fn entries(&self) -> FactoryConfigurationChoiceEntries {
    FactoryConfigurationChoiceEntries {
      references: self
        .references
        .iter()
        .map(|((kind, alias), reference)| FactoryReferenceChoice {
          kind: *kind,
          alias: alias.clone(),
          reference: reference.clone(),
        })
        .collect(),
      build_configurations: self
        .build_configurations
        .iter()
        .map(|(alias, reference)| (alias.clone(), reference.clone()))
        .collect(),
    }
  }

  /// Checks every exact runtime, verifier, model and Build reference without consulting worker capacity.
  /// Role/profile settings and effective enforcement remain the profile capability validator's responsibility.
  #[must_use]
  pub fn supports_build_binding(&self, build: &crate::FlowBuildBinding) -> bool {
    let selectable = |kind, reference: &ImmutableReference| {
      self
        .references
        .iter()
        .any(|((entry_kind, _), entry)| *entry_kind == kind && entry == reference)
    };
    [&build.tool, build.result_tool(), &build.plugin]
      .into_iter()
      .all(|reference| selectable(FactoryChoiceKind::NodeCapability, reference))
      && (selectable(FactoryChoiceKind::NodeCapability, &build.model_or_tool)
        || selectable(FactoryChoiceKind::DecisionSignalModel, &build.model_or_tool))
      && self
        .build_configurations
        .values()
        .any(|reference| reference == &build.build_configuration)
      && build.evidence.iter().all(|evidence| {
        selectable(FactoryChoiceKind::NodeCapability, evidence.tool())
          && selectable(FactoryChoiceKind::NodeCapability, evidence.plugin())
      })
  }

  /// Resolves one explicitly selected immutable Build configuration alias.
  pub fn resolve_build_configuration(&self, alias: &FactoryKey) -> Result<BuildConfigurationRef, FactoryError> {
    resolve_choice(&self.build_configurations, alias, FactoryChoiceKind::BuildConfiguration)
  }

  fn resolve_reference(&self, kind: FactoryChoiceKind, alias: &FactoryKey) -> Result<ImmutableReference, FactoryError> {
    if kind == FactoryChoiceKind::BuildConfiguration {
      return Err(FactoryError::InvalidConfiguration {
        field: "choice category",
      });
    }
    self
      .references
      .get(&(kind, alias.clone()))
      .cloned()
      .ok_or(FactoryError::UnknownChoiceAlias { kind })
  }
}

/// Per-configuration work-in-progress ceilings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "FactoryWipLimitsWire")]
pub struct FactoryWipLimits {
  max_active_runs: u32,
  max_active_nodes: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryWipLimitsWire {
  max_active_runs: u32,
  max_active_nodes: u32,
}

impl TryFrom<FactoryWipLimitsWire> for FactoryWipLimits {
  type Error = FactoryError;

  fn try_from(wire: FactoryWipLimitsWire) -> Result<Self, Self::Error> {
    Self::new(wire.max_active_runs, wire.max_active_nodes)
  }
}

impl FactoryWipLimits {
  /// Constructs positive WIP limits within product safety ceilings.
  pub const fn new(max_active_runs: u32, max_active_nodes: u32) -> Result<Self, FactoryError> {
    if max_active_runs == 0 || max_active_runs > MAX_FACTORY_ACTIVE_RUNS {
      return Err(FactoryError::InvalidConfiguration {
        field: "max_active_runs",
      });
    }
    if max_active_nodes == 0 || max_active_nodes > MAX_FACTORY_ACTIVE_NODES {
      return Err(FactoryError::InvalidConfiguration {
        field: "max_active_nodes",
      });
    }
    Ok(Self {
      max_active_runs,
      max_active_nodes,
    })
  }

  /// Returns the maximum concurrently admitted runs.
  #[must_use]
  pub const fn max_active_runs(self) -> u32 {
    self.max_active_runs
  }

  /// Returns the maximum concurrently active node attempts.
  #[must_use]
  pub const fn max_active_nodes(self) -> u32 {
    self.max_active_nodes
  }
}

/// Immutable purpose-specific Decision Signal selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSignalProfile {
  purpose: DecisionSignalPurpose,
  provider: ImmutableReference,
  adapter: ImmutableReference,
  model: ImmutableReference,
  question_set: ImmutableReference,
  policy: ImmutableReference,
  mode: DecisionSignalMode,
  fallback: DecisionSignalFallback,
  budget: BudgetLimit,
  routes: Option<DecisionSignalRouteSet>,
}

/// Named exact inputs for one resolved Decision Signal profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSignalProfileDefinition {
  /// Purpose governed by this profile.
  pub purpose: DecisionSignalPurpose,
  /// Exact decision-service identity.
  pub provider: ImmutableReference,
  /// Exact protocol-adapter identity.
  pub adapter: ImmutableReference,
  /// Exact model identity.
  pub model: ImmutableReference,
  /// Exact versioned question-set identity.
  pub question_set: ImmutableReference,
  /// Exact deterministic consumption-policy identity.
  pub policy: ImmutableReference,
  /// Configured rollout authority.
  pub mode: DecisionSignalMode,
  /// Fail-closed fallback for an unavailable or unusable signal.
  pub fallback: DecisionSignalFallback,
  /// Hard budget for one signal request.
  pub budget: BudgetLimit,
  /// Finite outgoing routes for a routing profile.
  pub routes: Option<DecisionSignalRouteSet>,
}

impl DecisionSignalProfile {
  /// Constructs one exact resolved profile with purpose-compatible routing data.
  pub fn new(definition: DecisionSignalProfileDefinition) -> Result<Self, FactoryError> {
    if matches!(definition.purpose, DecisionSignalPurpose::Routing) != definition.routes.is_some() {
      return Err(FactoryError::InvalidConfiguration {
        field: "decision_signal_routes",
      });
    }
    Ok(Self::from_resolved(definition))
  }

  pub(crate) fn from_resolved(definition: DecisionSignalProfileDefinition) -> Self {
    Self {
      purpose: definition.purpose,
      provider: definition.provider,
      adapter: definition.adapter,
      model: definition.model,
      question_set: definition.question_set,
      policy: definition.policy,
      mode: definition.mode,
      fallback: definition.fallback,
      budget: definition.budget,
      routes: definition.routes,
    }
  }

  /// Returns the profile purpose.
  #[must_use]
  pub const fn purpose(&self) -> DecisionSignalPurpose {
    self.purpose
  }

  /// Returns the exact decision service identity.
  #[must_use]
  pub const fn provider(&self) -> &ImmutableReference {
    &self.provider
  }

  /// Returns the exact provider adapter identity.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    &self.adapter
  }

  /// Returns the exact model identity.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns the exact versioned question set.
  #[must_use]
  pub const fn question_set(&self) -> &ImmutableReference {
    &self.question_set
  }

  /// Returns the exact deterministic consumption policy.
  #[must_use]
  pub const fn policy(&self) -> &ImmutableReference {
    &self.policy
  }

  /// Returns the rollout authority.
  #[must_use]
  pub const fn mode(&self) -> DecisionSignalMode {
    self.mode
  }

  /// Returns the fail-closed fallback.
  #[must_use]
  pub const fn fallback(&self) -> DecisionSignalFallback {
    self.fallback
  }

  /// Returns the hard budget for one provider request.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the immutable outgoing route domain for a routing profile.
  #[must_use]
  pub const fn routes(&self) -> Option<&DecisionSignalRouteSet> {
    self.routes.as_ref()
  }
}

/// Exact immutable evaluation selection for one configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationPolicy {
  criterion_packs: Vec<ImmutableReference>,
  evaluators: Vec<ImmutableReference>,
  required_quorum: u16,
  budget: BudgetLimit,
}

impl EvaluationPolicy {
  /// Constructs a canonical immutable evaluation selection.
  pub fn try_new(
    mut criterion_packs: Vec<ImmutableReference>,
    mut evaluators: Vec<ImmutableReference>,
    required_quorum: u16,
    budget: BudgetLimit,
  ) -> Result<Self, FactoryError> {
    validate_unique_references(&criterion_packs, MAX_CRITERION_PACKS, "criterion packs")?;
    validate_unique_references(&evaluators, MAX_EVALUATORS, "evaluators")?;
    validate_unique_reference_identities(&criterion_packs, "criterion pack identities")?;
    validate_unique_reference_identities(&evaluators, "evaluator identities")?;
    if required_quorum == 0 || usize::from(required_quorum) > evaluators.len() {
      return Err(FactoryError::InvalidConfiguration {
        field: "evaluator quorum",
      });
    }
    criterion_packs.sort();
    evaluators.sort();
    Ok(Self {
      criterion_packs,
      evaluators,
      required_quorum,
      budget,
    })
  }

  /// Returns the exact selected criterion packs.
  #[must_use]
  pub fn criterion_packs(&self) -> &[ImmutableReference] {
    &self.criterion_packs
  }

  /// Returns the exact selected evaluator capabilities.
  #[must_use]
  pub fn evaluators(&self) -> &[ImmutableReference] {
    &self.evaluators
  }

  /// Returns the deterministic evaluator quorum.
  #[must_use]
  pub const fn required_quorum(&self) -> u16 {
    self.required_quorum
  }

  /// Returns the evaluation hard budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
}

/// Complete unresolved definition submitted for immutable publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationDraft {
  /// Alias of the immutable admission policy governing later Work selection.
  pub admission_policy: FactoryKey,
  /// Explicit pinned Flow journey; publication rejects an absent definition.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub flow: Option<crate::FactoryFlowConfiguration>,
  /// Per-configuration work-in-progress limits.
  pub wip_limits: FactoryWipLimits,
  /// Hard budget enclosing every configured node and nested Flow execution.
  pub hard_budget: BudgetLimit,
  /// Alias of the deny-by-default permission ceiling selected for later intersection.
  pub permission_ceiling: FactoryKey,
  /// Whether later Work admission may select this version.
  pub enabled: bool,
}

/// One validated immutable Factory Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfiguration {
  reference: FactoryConfigurationRef,
  admission_policy: ImmutableReference,
  flow: crate::FactoryFlowConfiguration,
  wip_limits: FactoryWipLimits,
  hard_budget: BudgetLimit,
  permission_ceiling: ImmutableReference,
  enabled: bool,
}

impl FactoryConfiguration {
  /// Resolves aliases and publishes one validated immutable configuration version.
  pub fn publish(
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
    project_id: ProjectId,
    definition_digest: FactoryDigest,
    draft: FactoryConfigurationDraft,
    choices: &FactoryConfigurationChoices,
  ) -> Result<Self, FactoryError> {
    let admission_policy = choices.resolve_reference(FactoryChoiceKind::AdmissionPolicy, &draft.admission_policy)?;
    let permission_ceiling =
      choices.resolve_reference(FactoryChoiceKind::PermissionCeiling, &draft.permission_ceiling)?;

    let flow = draft
      .flow
      .ok_or(FactoryError::InvalidConfiguration { field: "required Flow" })?;
    flow.validate(
      &FactoryConfigurationRef::new(id, version, project_id, definition_digest),
      draft.hard_budget,
      draft.wip_limits,
    )?;
    for definition in flow.closure.definitions() {
      for node in definition.nodes() {
        if let Some(selection) = node.profile_selection() {
          selection.validate_capabilities(choices)?;
        }
      }
    }
    Ok(Self {
      reference: FactoryConfigurationRef::new(id, version, project_id, definition_digest),
      admission_policy,
      flow,
      wip_limits: draft.wip_limits,
      hard_budget: draft.hard_budget,
      permission_ceiling,
      enabled: draft.enabled,
    })
  }

  /// Publishes the immediately following immutable version without mutating this version.
  pub fn replace(
    &self,
    version: FactoryConfigurationVersion,
    definition_digest: FactoryDigest,
    draft: FactoryConfigurationDraft,
    choices: &FactoryConfigurationChoices,
  ) -> Result<Self, FactoryError> {
    if self.reference.version().get().checked_add(1) != Some(version.get()) {
      return Err(FactoryError::InvalidConfiguration {
        field: "replacement version",
      });
    }
    Self::publish(
      self.reference.id(),
      version,
      self.reference.project_id(),
      definition_digest,
      draft,
      choices,
    )
  }

  /// Returns the exact version reference used when admitting Work.
  #[must_use]
  pub const fn reference(&self) -> &FactoryConfigurationRef {
    &self.reference
  }

  /// Returns the exact immutable admission policy.
  #[must_use]
  pub const fn admission_policy(&self) -> &ImmutableReference {
    &self.admission_policy
  }

  /// Returns the required exact immutable Flow journey.
  #[must_use]
  pub const fn flow(&self) -> &crate::FactoryFlowConfiguration {
    &self.flow
  }

  /// Returns the work-in-progress limits.
  #[must_use]
  pub const fn wip_limits(&self) -> FactoryWipLimits {
    self.wip_limits
  }

  /// Returns the configuration-wide hard budget.
  #[must_use]
  pub const fn hard_budget(&self) -> BudgetLimit {
    self.hard_budget
  }

  /// Returns the exact permission ceiling selected for later pure intersection.
  #[must_use]
  pub const fn permission_ceiling(&self) -> &ImmutableReference {
    &self.permission_ceiling
  }

  /// Reports whether later Work admission may select this configuration version.
  #[must_use]
  pub const fn is_enabled(&self) -> bool {
    self.enabled
  }
}

fn choice_map<T>(
  entries: Vec<(FactoryKey, T)>,
  kind: FactoryChoiceKind,
) -> Result<BTreeMap<FactoryKey, T>, FactoryError> {
  if entries.len() > MAX_CONFIGURATION_CHOICES_PER_KIND {
    return Err(FactoryError::CollectionLimitExceeded {
      collection: "configuration choices",
    });
  }
  let mut choices = BTreeMap::new();
  for (alias, value) in entries {
    if choices.insert(alias, value).is_some() {
      return Err(FactoryError::DuplicateChoiceAlias { kind });
    }
  }
  Ok(choices)
}

fn reference_choice_map(
  entries: Vec<FactoryReferenceChoice>,
) -> Result<BTreeMap<(FactoryChoiceKind, FactoryKey), ImmutableReference>, FactoryError> {
  let mut counts = BTreeMap::<FactoryChoiceKind, usize>::new();
  let mut choices = BTreeMap::new();
  for entry in entries {
    if entry.kind == FactoryChoiceKind::BuildConfiguration {
      return Err(FactoryError::InvalidConfiguration {
        field: "choice category",
      });
    }
    let count = counts.entry(entry.kind).or_default();
    *count += 1;
    if *count > MAX_CONFIGURATION_CHOICES_PER_KIND {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "configuration choices",
      });
    }
    if choices.insert((entry.kind, entry.alias), entry.reference).is_some() {
      return Err(FactoryError::DuplicateChoiceAlias { kind: entry.kind });
    }
  }
  Ok(choices)
}

fn resolve_choice<T: Clone>(
  choices: &BTreeMap<FactoryKey, T>,
  alias: &FactoryKey,
  kind: FactoryChoiceKind,
) -> Result<T, FactoryError> {
  choices
    .get(alias)
    .cloned()
    .ok_or(FactoryError::UnknownChoiceAlias { kind })
}

fn validate_unique_references(
  references: &[ImmutableReference],
  maximum: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  validate_count(references, 1, maximum, collection)?;
  if references.iter().collect::<HashSet<_>>().len() != references.len() {
    return Err(FactoryError::InvalidConfiguration { field: collection });
  }
  Ok(())
}

fn validate_unique_reference_identities(
  references: &[ImmutableReference],
  collection: &'static str,
) -> Result<(), FactoryError> {
  if references
    .iter()
    .map(ImmutableReference::identity)
    .collect::<HashSet<_>>()
    .len()
    != references.len()
  {
    return Err(FactoryError::InvalidConfiguration { field: collection });
  }
  Ok(())
}

fn validate_count<T>(
  values: &[T],
  minimum: usize,
  maximum: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  if values.len() < minimum || values.len() > maximum {
    return Err(FactoryError::CollectionLimitExceeded { collection });
  }
  Ok(())
}
