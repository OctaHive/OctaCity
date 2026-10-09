use std::collections::{BTreeMap, HashSet};

use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId};
use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose, DecisionSignalRouteSet,
  FactoryChoiceKind, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion,
  FactoryCredentialProfiles, FactoryDigest, FactoryError, FactoryKey, FactoryStageKind, MAX_CRITERION_PACKS,
  MAX_EVALUATORS,
};

/// Maximum number of aliases accepted in one configuration choice category.
pub const MAX_CONFIGURATION_CHOICES_PER_KIND: usize = 256;
/// Maximum stage definitions in one immutable Factory Configuration.
pub const MAX_FACTORY_STAGES: usize = 32;
/// Maximum concurrently admitted runs under one Factory Configuration.
pub const MAX_FACTORY_ACTIVE_RUNS: u32 = 10_000;
/// Maximum concurrently active stage attempts under one Factory Configuration.
pub const MAX_FACTORY_ACTIVE_STAGES: u32 = 1_000;
/// Maximum rework cycles permitted by one immutable configuration.
pub const MAX_FACTORY_REWORK_CYCLES: u16 = 32;

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
}

impl FactoryConfigurationChoices {
  /// Constructs bounded catalogs and rejects duplicate aliases within a category.
  pub fn try_new(entries: FactoryConfigurationChoiceEntries) -> Result<Self, FactoryError> {
    Ok(Self {
      references: reference_choice_map(entries.references)?,
      build_configurations: choice_map(entries.build_configurations, FactoryChoiceKind::BuildConfiguration)?,
    })
  }

  fn resolve_build_configuration(&self, alias: &FactoryKey) -> Result<BuildConfigurationRef, FactoryError> {
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
#[serde(deny_unknown_fields)]
pub struct FactoryWipLimits {
  max_active_runs: u32,
  max_active_stages: u32,
}

impl FactoryWipLimits {
  /// Constructs positive WIP limits within product safety ceilings.
  pub const fn new(max_active_runs: u32, max_active_stages: u32) -> Result<Self, FactoryError> {
    if max_active_runs == 0 || max_active_runs > MAX_FACTORY_ACTIVE_RUNS {
      return Err(FactoryError::InvalidConfiguration {
        field: "max_active_runs",
      });
    }
    if max_active_stages == 0 || max_active_stages > MAX_FACTORY_ACTIVE_STAGES {
      return Err(FactoryError::InvalidConfiguration {
        field: "max_active_stages",
      });
    }
    Ok(Self {
      max_active_runs,
      max_active_stages,
    })
  }

  /// Returns the maximum concurrently admitted runs.
  #[must_use]
  pub const fn max_active_runs(self) -> u32 {
    self.max_active_runs
  }

  /// Returns the maximum concurrently active stage attempts.
  #[must_use]
  pub const fn max_active_stages(self) -> u32 {
    self.max_active_stages
  }
}

/// Unresolved stage selection supplied for configuration publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryStageDraft {
  /// Stable stage identity within the configuration version.
  pub key: FactoryKey,
  /// Program-owned stage kind.
  pub kind: FactoryStageKind,
  /// Alias of the selected exact Build Configuration.
  pub build_configuration: FactoryKey,
  /// Hard budget for one attempt of this stage.
  pub budget: BudgetLimit,
}

/// Immutable validated stage definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryStageDefinition {
  key: FactoryKey,
  kind: FactoryStageKind,
  build_configuration: BuildConfigurationRef,
  budget: BudgetLimit,
}

impl FactoryStageDefinition {
  /// Returns the stable stage key.
  #[must_use]
  pub const fn key(&self) -> &FactoryKey {
    &self.key
  }

  /// Returns the program-owned stage kind.
  #[must_use]
  pub const fn kind(&self) -> FactoryStageKind {
    self.kind
  }

  /// Returns the exact Build Configuration selected at publication.
  #[must_use]
  pub const fn build_configuration(&self) -> &BuildConfigurationRef {
    &self.build_configuration
  }

  /// Returns the immutable per-attempt budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
}

/// Optional unresolved Decision Signal profile selected for one purpose.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSignalProfileDraft {
  /// Purpose governed by this profile.
  pub purpose: DecisionSignalPurpose,
  /// Provider adapter alias.
  pub provider: FactoryKey,
  /// Exact protocol adapter alias.
  pub adapter: FactoryKey,
  /// Exact-model alias.
  pub model: FactoryKey,
  /// Versioned question-set alias.
  pub question_set: FactoryKey,
  /// Deterministic consumption-policy alias.
  pub policy: FactoryKey,
  /// Configured rollout authority.
  pub mode: DecisionSignalMode,
  /// Fail-closed fallback for an unavailable or unusable signal.
  pub fallback: DecisionSignalFallback,
  /// Hard budget for one signal request.
  pub budget: BudgetLimit,
  /// Finite outgoing routes for the immutable state governed by a routing profile.
  pub routes: Option<DecisionSignalRouteSet>,
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

/// Unresolved criterion and evaluator selection supplied at publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationPolicyDraft {
  /// Selected criterion-pack aliases.
  pub criterion_packs: Vec<FactoryKey>,
  /// Selected evaluator aliases.
  pub evaluators: Vec<FactoryKey>,
  /// Number of schema-valid evaluator branches required by deterministic policy.
  pub required_quorum: u16,
  /// Hard budget enclosing evaluation work.
  pub budget: BudgetLimit,
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

/// Unresolved bounded rework selection supplied at publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReworkPolicyDraft {
  /// Maximum number of rework cycles; zero disables rework.
  pub max_cycles: u16,
  /// Stage key used for rework when cycles are enabled.
  pub stage: Option<FactoryKey>,
}

/// Immutable bounded rework policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReworkPolicy {
  max_cycles: u16,
  stage: Option<FactoryKey>,
}

impl ReworkPolicy {
  /// Returns the maximum number of rework cycles.
  #[must_use]
  pub const fn max_cycles(&self) -> u16 {
    self.max_cycles
  }

  /// Returns the configured rework stage when rework is enabled.
  #[must_use]
  pub const fn stage(&self) -> Option<&FactoryKey> {
    self.stage.as_ref()
  }
}

/// Unresolved delivery-for-review selection supplied at publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryPolicyDraft {
  /// Delivery adapter alias.
  pub adapter: FactoryKey,
  /// Immutable delivery target-policy alias.
  pub policy: FactoryKey,
}

/// Exact delivery-for-human-review policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryPolicy {
  adapter: ImmutableReference,
  policy: ImmutableReference,
}

impl DeliveryPolicy {
  /// Returns the exact delivery adapter identity.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    &self.adapter
  }

  /// Returns the exact immutable target policy.
  #[must_use]
  pub const fn policy(&self) -> &ImmutableReference {
    &self.policy
  }
}

/// Complete unresolved definition submitted for immutable publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationDraft {
  /// Alias of the immutable admission policy governing later Work selection.
  pub admission_policy: FactoryKey,
  /// Stage definitions and Build Configuration aliases.
  pub stages: Vec<FactoryStageDraft>,
  /// Per-configuration work-in-progress limits.
  pub wip_limits: FactoryWipLimits,
  /// Hard budget enclosing every stage, evaluation, signal, and rework cycle.
  pub hard_budget: BudgetLimit,
  /// Alias of the deny-by-default permission ceiling selected for later intersection.
  pub permission_ceiling: FactoryKey,
  /// Distinct logical credentials scoped to trusted Factory consumers.
  pub credential_profiles: FactoryCredentialProfiles,
  /// Optional purpose-specific Decision Signal profiles.
  pub decision_signals: Vec<DecisionSignalProfileDraft>,
  /// Criterion packs, evaluators, quorum, and evaluation budget.
  pub evaluation: EvaluationPolicyDraft,
  /// Bounded rework selection.
  pub rework: ReworkPolicyDraft,
  /// Delivery-for-human-review selection.
  pub delivery: DeliveryPolicyDraft,
  /// Whether later Work admission may select this version.
  pub enabled: bool,
}

/// One validated immutable Factory Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfiguration {
  reference: FactoryConfigurationRef,
  admission_policy: ImmutableReference,
  stages: Vec<FactoryStageDefinition>,
  wip_limits: FactoryWipLimits,
  hard_budget: BudgetLimit,
  permission_ceiling: ImmutableReference,
  credential_profiles: FactoryCredentialProfiles,
  decision_signals: Vec<DecisionSignalProfile>,
  evaluation: EvaluationPolicy,
  rework: ReworkPolicy,
  delivery: DeliveryPolicy,
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
    let stages = resolve_stages(project_id, &draft, choices)?;
    let decision_signals = resolve_decision_signals(&draft, choices)?;
    let evaluation = resolve_evaluation(&draft, choices)?;
    let rework = resolve_rework(&draft.rework, &stages)?;
    let delivery = DeliveryPolicy {
      adapter: choices.resolve_reference(FactoryChoiceKind::DeliveryAdapter, &draft.delivery.adapter)?,
      policy: choices.resolve_reference(FactoryChoiceKind::DeliveryPolicy, &draft.delivery.policy)?,
    };
    let permission_ceiling =
      choices.resolve_reference(FactoryChoiceKind::PermissionCeiling, &draft.permission_ceiling)?;

    Ok(Self {
      reference: FactoryConfigurationRef::new(id, version, project_id, definition_digest),
      admission_policy,
      stages,
      wip_limits: draft.wip_limits,
      hard_budget: draft.hard_budget,
      permission_ceiling,
      credential_profiles: draft.credential_profiles,
      decision_signals,
      evaluation,
      rework,
      delivery,
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

  /// Returns the immutable stage definitions.
  #[must_use]
  pub fn stages(&self) -> &[FactoryStageDefinition] {
    &self.stages
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

  /// Returns the immutable stage-scoped logical credential profiles.
  #[must_use]
  pub const fn credential_profiles(&self) -> &FactoryCredentialProfiles {
    &self.credential_profiles
  }

  /// Returns all configured Decision Signal profiles.
  #[must_use]
  pub fn decision_signals(&self) -> &[DecisionSignalProfile] {
    &self.decision_signals
  }

  /// Returns a configured Decision Signal profile by purpose.
  #[must_use]
  pub fn decision_signal(&self, purpose: DecisionSignalPurpose) -> Option<&DecisionSignalProfile> {
    self.decision_signals.iter().find(|profile| profile.purpose == purpose)
  }

  /// Returns the immutable evaluation policy.
  #[must_use]
  pub const fn evaluation(&self) -> &EvaluationPolicy {
    &self.evaluation
  }

  /// Returns the immutable rework policy.
  #[must_use]
  pub const fn rework(&self) -> &ReworkPolicy {
    &self.rework
  }

  /// Returns the immutable delivery-for-human-review policy.
  #[must_use]
  pub const fn delivery(&self) -> &DeliveryPolicy {
    &self.delivery
  }

  /// Reports whether later Work admission may select this configuration version.
  #[must_use]
  pub const fn is_enabled(&self) -> bool {
    self.enabled
  }
}

fn resolve_stages(
  project_id: ProjectId,
  draft: &FactoryConfigurationDraft,
  choices: &FactoryConfigurationChoices,
) -> Result<Vec<FactoryStageDefinition>, FactoryError> {
  validate_count(&draft.stages, 1, MAX_FACTORY_STAGES, "configuration stages")?;
  let mut keys = HashSet::with_capacity(draft.stages.len());
  let mut kinds = HashSet::with_capacity(4);
  let mut stages = Vec::with_capacity(draft.stages.len());
  for stage in &draft.stages {
    if !keys.insert(stage.key.clone()) {
      return Err(FactoryError::InvalidConfiguration {
        field: "duplicate stage key",
      });
    }
    let build_configuration = choices.resolve_build_configuration(&stage.build_configuration)?;
    if build_configuration.project_id != project_id {
      return Err(FactoryError::InvalidConfiguration {
        field: "stage Build Configuration Project",
      });
    }
    if !stage.budget.fits_within(draft.hard_budget) {
      return Err(FactoryError::InvalidConfiguration { field: "stage budget" });
    }
    if !kinds.insert(stage.kind) {
      return Err(FactoryError::InvalidConfiguration {
        field: "duplicate stage kind",
      });
    }
    stages.push(FactoryStageDefinition {
      key: stage.key.clone(),
      kind: stage.kind,
      build_configuration,
      budget: stage.budget,
    });
  }
  if ![
    FactoryStageKind::Implementation,
    FactoryStageKind::Validation,
    FactoryStageKind::Evaluation,
  ]
  .into_iter()
  .all(|kind| kinds.contains(&kind))
  {
    return Err(FactoryError::InvalidConfiguration {
      field: "required stages",
    });
  }
  Ok(stages)
}

fn resolve_decision_signals(
  draft: &FactoryConfigurationDraft,
  choices: &FactoryConfigurationChoices,
) -> Result<Vec<DecisionSignalProfile>, FactoryError> {
  validate_count(&draft.decision_signals, 0, 2, "decision signal profiles")?;
  let mut purposes = HashSet::with_capacity(draft.decision_signals.len());
  let mut profiles = Vec::with_capacity(draft.decision_signals.len());
  for profile in &draft.decision_signals {
    if !purposes.insert(profile.purpose) {
      return Err(FactoryError::InvalidConfiguration {
        field: "duplicate Decision Signal purpose",
      });
    }
    if !profile.budget.fits_within(draft.hard_budget) {
      return Err(FactoryError::InvalidConfiguration {
        field: "Decision Signal budget",
      });
    }
    if matches!(profile.purpose, DecisionSignalPurpose::Routing) != profile.routes.is_some() {
      return Err(FactoryError::InvalidConfiguration {
        field: "Decision Signal routing routes",
      });
    }
    profiles.push(DecisionSignalProfile::from_resolved(DecisionSignalProfileDefinition {
      purpose: profile.purpose,
      provider: choices.resolve_reference(FactoryChoiceKind::DecisionSignalProvider, &profile.provider)?,
      adapter: choices.resolve_reference(FactoryChoiceKind::DecisionSignalAdapter, &profile.adapter)?,
      model: choices.resolve_reference(FactoryChoiceKind::DecisionSignalModel, &profile.model)?,
      question_set: choices.resolve_reference(FactoryChoiceKind::DecisionSignalQuestionSet, &profile.question_set)?,
      policy: choices.resolve_reference(FactoryChoiceKind::DecisionSignalPolicy, &profile.policy)?,
      mode: profile.mode,
      fallback: profile.fallback,
      budget: profile.budget,
      routes: profile.routes.clone(),
    }));
  }
  Ok(profiles)
}

fn resolve_evaluation(
  draft: &FactoryConfigurationDraft,
  choices: &FactoryConfigurationChoices,
) -> Result<EvaluationPolicy, FactoryError> {
  validate_unique_aliases(
    &draft.evaluation.criterion_packs,
    MAX_CRITERION_PACKS,
    "criterion packs",
  )?;
  validate_unique_aliases(&draft.evaluation.evaluators, MAX_EVALUATORS, "evaluators")?;
  if draft.evaluation.required_quorum == 0
    || usize::from(draft.evaluation.required_quorum) > draft.evaluation.evaluators.len()
  {
    return Err(FactoryError::InvalidConfiguration {
      field: "evaluator quorum",
    });
  }
  if !draft.evaluation.budget.fits_within(draft.hard_budget) {
    return Err(FactoryError::InvalidConfiguration {
      field: "evaluation budget",
    });
  }
  EvaluationPolicy::try_new(
    resolve_references(
      choices,
      FactoryChoiceKind::CriterionPack,
      &draft.evaluation.criterion_packs,
    )?,
    resolve_references(choices, FactoryChoiceKind::Evaluator, &draft.evaluation.evaluators)?,
    draft.evaluation.required_quorum,
    draft.evaluation.budget,
  )
}

fn resolve_rework(draft: &ReworkPolicyDraft, stages: &[FactoryStageDefinition]) -> Result<ReworkPolicy, FactoryError> {
  if draft.max_cycles > MAX_FACTORY_REWORK_CYCLES {
    return Err(FactoryError::InvalidConfiguration { field: "rework cycles" });
  }
  match (draft.max_cycles, draft.stage.as_ref()) {
    (0, None) => Ok(ReworkPolicy {
      max_cycles: 0,
      stage: None,
    }),
    (1.., Some(stage_key))
      if stages
        .iter()
        .any(|stage| stage.key == *stage_key && stage.kind == FactoryStageKind::Rework) =>
    {
      Ok(ReworkPolicy {
        max_cycles: draft.max_cycles,
        stage: Some(stage_key.clone()),
      })
    }
    _ => Err(FactoryError::InvalidConfiguration { field: "rework stage" }),
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

fn resolve_references(
  choices: &FactoryConfigurationChoices,
  kind: FactoryChoiceKind,
  aliases: &[FactoryKey],
) -> Result<Vec<ImmutableReference>, FactoryError> {
  aliases
    .iter()
    .map(|alias| choices.resolve_reference(kind, alias))
    .collect()
}

fn validate_unique_aliases(
  aliases: &[FactoryKey],
  maximum: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  validate_count(aliases, 1, maximum, collection)?;
  if aliases.iter().collect::<HashSet<_>>().len() != aliases.len() {
    return Err(FactoryError::InvalidConfiguration { field: collection });
  }
  Ok(())
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
