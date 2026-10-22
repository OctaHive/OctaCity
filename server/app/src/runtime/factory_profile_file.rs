//! Strict deployment data, separate from provider transports and Flow definitions.
use super::factory_profiles::FactoryRuntimeProfileCatalog;
use super::{FactoryRuntimeInstallationError, FactoryRuntimeSignalConnector};
use octacity_server_application::{
  FactoryCodexBuildProfile, FactoryCodexCapturePolicy, FactoryCodexUsagePolicy, FactoryDecisionSignalNodeBinding,
  FactoryDecisionSignalNodeOutput, FactoryDecisionSignalRequestPlan, FactoryProtectedTestPolicy,
  FactoryProtectedValidationEvidenceProfile, FactoryReviewPlanningProfile, FactoryToolRiskBinding,
  FactoryToolRiskNodeProfile, MAX_FACTORY_EXECUTOR_BINDINGS,
};
use octacity_server_domain::{PipelineNodeId, ProjectId};
use octacity_server_factory::*;
use serde::Deserialize;
use std::collections::BTreeMap;

const MAX_PROFILE_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct FileProfileInstaller {
  catalog: FactoryRuntimeProfileCatalog,
  providers: Vec<FactoryRuntimeSignalConnector>,
  bindings: FileBindings,
}
struct FileBindings {
  protected_validation: Vec<ValidationCommandBinding>,
  promotions: Vec<PromotionCommandBinding>,
  commands: Vec<octacity_server_application::FactoryCommandBuildProfile>,
  authors: Vec<octacity_server_application::FactoryProtectedTestAuthoringProfile>,
  delivery: Vec<super::factory_delivery_file::FactoryFileDeliveryProfile>,
  protected_validation_evidence: Vec<ValidationEvidenceEntry>,
  evaluation_decisions: Vec<(
    ImmutableReference,
    octacity_server_application::FactoryEvaluationDecisionProfile,
  )>,
}
enum PromotionCommandBinding {
  Builtin {
    identity: ImmutableReference,
    profiles: Vec<octacity_server_application::FactoryCommandBuildProfile>,
  },
  Deployment {
    identity: ImmutableReference,
    profiles: Vec<octacity_server_application::FactoryDeploymentCommandProfile>,
  },
}
impl FileProfileInstaller {
  pub(super) async fn load(
    path: &std::path::Path,
    providers: &[FactoryRuntimeSignalConnector],
  ) -> Result<Self, FactoryRuntimeInstallationError> {
    let bytes = super::assembly::read_private_bytes("Factory profiles", path, MAX_PROFILE_BYTES as u64)
      .await
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    Self::parse_json(&bytes, providers)
  }
  pub(super) fn parse_json(
    bytes: &[u8],
    providers: &[FactoryRuntimeSignalConnector],
  ) -> Result<Self, FactoryRuntimeInstallationError> {
    let (catalog, bindings) = FactoryRuntimeProfileCatalog::parse_file_json(bytes, providers)?;
    Ok(Self {
      catalog,
      providers: providers.to_vec(),
      bindings,
    })
  }
}

struct NativeProfileReadiness {
  store: std::sync::Arc<octacity_server_store_postgres::PostgresStore>,
  objects: std::sync::Arc<octacity_artifact_s3::S3ArtifactStore>,
  projects: Vec<ProjectId>,
}
#[async_trait::async_trait]
impl super::FactoryRuntimeReadiness for NativeProfileReadiness {
  async fn available(&self) -> bool {
    use octacity_server_store::ProjectStore as _;
    for project in &self.projects {
      if self.store.project(*project).await.is_err() {
        return false;
      }
    }
    self.objects.health_check().await.is_ok()
  }
}
#[async_trait::async_trait]
impl super::FactoryRuntimeInstaller for FileProfileInstaller {
  async fn install(
    &self,
    resources: super::FactoryRuntimeResources,
  ) -> Result<super::FactoryRuntimeInstallation, FactoryRuntimeInstallationError> {
    let dependencies = std::sync::Arc::new(NativeProfileReadiness {
      store: resources.runs.clone(),
      objects: resources.objects.clone(),
      projects: self.catalog.projects.iter().map(|(project, _)| *project).collect(),
    });
    let readiness = std::sync::Arc::new(super::FactoryRuntimeSignalReadiness::new(
      dependencies,
      self.providers.clone(),
    ));
    let mut catalog = self.catalog.clone();
    for binding in &self.bindings.protected_validation_evidence {
      catalog.executors.push(resources.protected_validation_evidence_executor(
        binding.identity.clone(),
        binding.profile.native(),
        binding.author_profile.clone(),
        binding.policy.clone(),
        binding.inventory.clone(),
        binding.capture_tool.clone(),
      )?);
    }
    for (identity, profile) in &self.bindings.evaluation_decisions {
      catalog
        .executors
        .push(resources.evaluation_decision_executor(identity.clone(), profile.clone())?);
    }
    let mut commands = BTreeMap::<ImmutableReference, Vec<_>>::new();
    for profile in &self.bindings.commands {
      commands
        .entry(profile.executor.clone())
        .or_default()
        .push(profile.clone());
    }
    for profiles in commands.into_values() {
      catalog.executors.push(resources.command_executor(profiles)?);
    }
    for binding in &self.bindings.promotions {
      let installed = match binding {
        PromotionCommandBinding::Builtin { identity, profiles } => {
          resources.promotion_command_executor(identity.clone(), profiles.clone())?
        }
        PromotionCommandBinding::Deployment { identity, profiles } => {
          resources.deployment_command_executor(identity.clone(), profiles.clone())?
        }
      };
      catalog.executors.push(installed);
    }
    let mut authors = BTreeMap::<ImmutableReference, Vec<_>>::new();
    for profile in &self.bindings.authors {
      authors
        .entry(profile.command().executor.clone())
        .or_default()
        .push(profile.clone());
    }
    for profiles in authors.into_values() {
      catalog
        .executors
        .push(resources.protected_test_authoring_command_executor(profiles)?);
    }
    let output_finalizer = if self.bindings.authors.is_empty() {
      None
    } else {
      Some(resources.protected_test_output_finalizer(self.bindings.authors.clone())?)
    };
    for binding in &self.bindings.protected_validation {
      catalog.executors.push(resources.protected_validation_command_executor(
        binding.profiles.clone(),
        binding.inputs.clone(),
        binding.report.clone(),
        binding.facts.clone(),
      )?);
    }
    for binding in &self.bindings.delivery {
      catalog.executors.push(binding.install(&resources)?);
    }
    let mut installed = super::FactoryProfileRuntimeInstaller::new(catalog, readiness, vec![])?
      .install(resources)
      .await?;
    installed.output_finalizer = output_finalizer;
    Ok(installed)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogFile {
  version: u16,
  #[serde(default)]
  commands: Vec<CommandProfile>,
  #[serde(default)]
  protected_test_authors: Vec<AuthorEntry>,
  #[serde(default)]
  promotions: Vec<PromotionEntry>,
  #[serde(default)]
  protected_validation: Vec<ValidationCommandEntry>,
  projects: Vec<ProjectChoices>,
  codex: Vec<CodexProfile>,
  #[serde(default)]
  signals: Vec<RoutingProfile>,
  #[serde(default)]
  tool_risk: Vec<ToolRiskProfile>,
  #[serde(default)]
  review_planning: Vec<ReviewPlanningEntry>,
  #[serde(default)]
  evaluation_decisions: Vec<EvaluationDecisionEntry>,
  #[serde(default)]
  protected_validation_evidence: Vec<ValidationEvidenceEntry>,
  #[serde(default)]
  delivery: Vec<super::factory_delivery_file::FactoryFileDeliveryProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorEntry {
  command: CommandProfile,
  policy: octacity_server_application::FactoryProtectedTestAuthoringPolicy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromotionEntry {
  identity: ImmutableReference,
  profiles: Vec<CommandProfile>,
  #[serde(default)]
  deployment_authority: Option<DeploymentAuthorityEntry>,
}
/// One operator-installed target, distinct from a Flow-selected environment.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentAuthorityEntry {
  identity: FactoryKey,
  version: FactoryKey,
  environment: ImmutableReference,
  endpoint: String,
  #[serde(default)]
  credential_profile: Option<FactoryKey>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationCommandEntry {
  profiles: Vec<CommandProfile>,
  inputs: ValidationCommandInputs,
  report: EvidenceRequirement,
  facts: EvidenceRequirement,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationCommandInputs {
  author_profile: ImmutableReference,
  policy: FactoryProtectedTestPolicy,
  inventory: EvidenceRequirement,
  capture_tool: ImmutableReference,
}
struct ValidationCommandBinding {
  profiles: Vec<octacity_server_application::FactoryCommandBuildProfile>,
  inputs: super::FactoryRuntimeProtectedValidationProfile,
  report: EvidenceRequirement,
  facts: EvidenceRequirement,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandProfile {
  identity: ImmutableReference,
  plugin: ImmutableReference,
  executor: ImmutableReference,
  build_configuration: BuildConfigurationRef,
  pipeline_node: PipelineNodeId,
  program: octacity_server_application::FactoryCommandProgram,
  input_destination: String,
  parameters: BTreeMap<String, serde_json::Value>,
  project_permissions: FactoryPermissionSet,
  local_permissions: LocalPermissions,
  outputs: Vec<EvidenceRequirement>,
  #[serde(default)]
  candidate_capture_tool: Option<ImmutableReference>,
}
impl CommandProfile {
  fn native(self) -> Result<octacity_server_application::FactoryCommandBuildProfile, FactoryRuntimeInstallationError> {
    Ok(octacity_server_application::FactoryCommandBuildProfile {
      identity: self.identity,
      plugin: self.plugin,
      executor: self.executor,
      build_configuration: self.build_configuration,
      pipeline_node: self.pipeline_node,
      program: self.program,
      input_destination: self.input_destination,
      parameters: self.parameters,
      project_permissions: self.project_permissions,
      local_permissions: LocalPermissionCeiling::try_new(
        self.local_permissions.grants,
        self.local_permissions.enforceable,
      )
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?,
      outputs: self.outputs,
      candidate_capture_tool: self.candidate_capture_tool,
    })
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationEvidenceEntry {
  identity: ImmutableReference,
  profile: ValidationEvidencePolicy,
  author_profile: ImmutableReference,
  policy: FactoryProtectedTestPolicy,
  inventory: EvidenceRequirement,
  capture_tool: ImmutableReference,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationEvidencePolicy {
  action: FactoryKey,
  ready: FlowOutcomeDefinition,
  report: EvidenceRequirement,
  facts: EvidenceRequirement,
}
impl ValidationEvidencePolicy {
  fn native(&self) -> FactoryProtectedValidationEvidenceProfile {
    FactoryProtectedValidationEvidenceProfile {
      action: self.action.clone(),
      ready: self.ready.clone(),
      report: self.report.clone(),
      facts: self.facts.clone(),
    }
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewPlanningEntry {
  identity: ImmutableReference,
  profile: ReviewPlanningPolicy,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewPlanningPolicy {
  action: FactoryKey,
  ready: FlowOutcomeDefinition,
  escalation: FlowOutcomeDefinition,
  purpose: ReviewPurpose,
  criterion_packs: Vec<CriterionPack>,
  branches: Vec<ReviewBranch>,
  policy: EvaluationPolicy,
  data_handling: ImmutableReference,
  capabilities: Vec<ReviewEvaluatorCapability>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationDecisionEntry {
  identity: ImmutableReference,
  profile: EvaluationDecisionProfile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationDecisionProfile {
  action: FactoryKey,
  policy: EvaluationDecisionPolicy,
  outcomes: Vec<EvaluationDecisionOutcome>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationDecisionOutcome {
  disposition: DecisionOutcome,
  outcome: FlowOutcomeDefinition,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationDecisionPolicy {
  version: DecisionPolicyVersion,
  required_evidence: Vec<FactoryKey>,
  required_evaluators: Vec<FactoryKey>,
  quorum: u16,
  severity_threshold: FindingSeverity,
  indeterminate_policy: IndeterminatePolicy,
  failure_outcome: DecisionOutcome,
}
impl EvaluationDecisionEntry {
  fn native(
    self,
  ) -> Result<
    (
      ImmutableReference,
      octacity_server_application::FactoryEvaluationDecisionProfile,
    ),
    FactoryRuntimeInstallationError,
  > {
    use FactoryRuntimeInstallationError::Invalid;
    let rules = self.profile.policy;
    let profile = octacity_server_application::FactoryEvaluationDecisionProfile {
      action: self.profile.action,
      policy: DecisionPolicy::try_new(
        rules.version,
        DecisionPolicyDefinition {
          required_evidence: rules.required_evidence,
          required_evaluators: rules.required_evaluators,
          quorum: rules.quorum,
          severity_threshold: rules.severity_threshold,
          indeterminate_policy: rules.indeterminate_policy,
          failure_outcome: rules.failure_outcome,
        },
      )
      .map_err(|_| Invalid)?,
      outcomes: self
        .profile
        .outcomes
        .into_iter()
        .map(|entry| (entry.disposition, entry.outcome))
        .collect(),
    };
    if self.identity.digest() != profile.digest().map_err(|_| Invalid)? {
      return Err(Invalid);
    }
    Ok((self.identity, profile))
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolRiskProfile {
  project_id: ProjectId,
  plan: FactoryDecisionSignalRequestPlan,
  mapping: ToolRiskChoiceMapping,
  max_calls: u16,
  credential_profile: FactoryKey,
  network_host: String,
  project_permissions: FactoryPermissionSet,
  local_permissions: LocalPermissions,
  base_task_digest: FactoryDigest,
  build_configuration: BuildConfigurationRef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoutingProfile {
  action: FlowActionBinding,
  project_id: ProjectId,
  plan: FactoryDecisionSignalRequestPlan,
  baseline_route: FactoryKey,
  credential_profile: FactoryKey,
  network_host: String,
  project_permissions: FactoryPermissionSet,
  local_permissions: LocalPermissions,
  outputs: Vec<RoutingOutput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoutingOutput {
  disposition: DecisionSignalDisposition,
  outcome: FactoryKey,
  projection: FlowDataMapping,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectChoices {
  project_id: ProjectId,
  #[serde(default)]
  role_profiles: BTreeMap<FactoryKey, RoleProfileDeclaration>,
  references: Vec<ReferenceChoice>,
  build_configurations: Vec<BuildChoice>,
}
// Deployment-owned compatibility only; this does not install a runtime or executor.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleProfileDeclaration {
  ceiling: FactoryRoleProfileDefinition,
  parameters: FlowDataSchema,
  public_environment: BTreeMap<String, FlowDataSchema>,
  enforcement: Vec<PermissionCategory>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceChoice {
  kind: FactoryChoiceKind,
  alias: FactoryKey,
  reference: ImmutableReference,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildChoice {
  alias: FactoryKey,
  reference: BuildConfigurationRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Toolchain {
  plugin: ImmutableReference,
  executable: ImmutableReference,
  model: ImmutableReference,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalPermissions {
  grants: FactoryPermissionSet,
  enforceable: Vec<PermissionCategory>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureProfile {
  tool: ImmutableReference,
  instruction: octacity_protocol::ChangeSetCaptureV3,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CodexExecutionMode {
  ProtectedTools,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexProfile {
  toolchain: Toolchain,
  build_configuration: BuildConfigurationRef,
  instructions: FactoryText,
  credentials: FactoryCredentialProfiles,
  credential_purpose: FactoryCredentialPurpose,
  #[serde(default)]
  execution_mode: Option<CodexExecutionMode>,
  usage_policy: FactoryCodexUsagePolicy,
  #[serde(default)]
  tool_response_limits: Option<octacity_protocol::FactoryToolResponseLimitsV3>,
  #[serde(default)]
  candidate_capture_tool: Option<ImmutableReference>,
  #[serde(default)]
  reviewer: Option<octacity_server_application::FactoryCodexReviewerPolicy>,
  #[serde(default)]
  capture: Option<CaptureProfile>,
  pipeline_node: PipelineNodeId,
  parameters: BTreeMap<String, serde_json::Value>,
  project_permissions: FactoryPermissionSet,
  local_permissions: LocalPermissions,
}

impl FactoryRuntimeProfileCatalog {
  /// Reads a stable owner-only regular file without exposing its path or private
  /// content in diagnostics. The same bounded parser validates loaded bytes.
  pub async fn load(path: &std::path::Path) -> Result<Self, FactoryRuntimeInstallationError> {
    Self::load_with_signal_providers(path, &[]).await
  }
  /// Loads the same private catalog while resolving only trusted installed connectors.
  pub async fn load_with_signal_providers(
    path: &std::path::Path,
    providers: &[FactoryRuntimeSignalConnector],
  ) -> Result<Self, FactoryRuntimeInstallationError> {
    let bytes = super::assembly::read_private_bytes("Factory profiles", path, MAX_PROFILE_BYTES as u64)
      .await
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    Self::parse_json_with_signal_providers(&bytes, providers)
  }
  /// Parses bounded operator-owned JSON. Diagnostics never include private
  /// instructions, permission paths, credentials or decoder source text.
  /// Provider transports are installed separately through DecisionSignalProvider.
  pub fn parse_json(bytes: &[u8]) -> Result<Self, FactoryRuntimeInstallationError> {
    Self::parse_json_with_signal_providers(bytes, &[])
  }

  /// Resolves routing metadata against already installed exact connectors.
  /// This never constructs transports or resolves credentials from task data.
  pub fn parse_json_with_signal_providers(
    bytes: &[u8],
    providers: &[FactoryRuntimeSignalConnector],
  ) -> Result<Self, FactoryRuntimeInstallationError> {
    let (catalog, bindings) = Self::parse_file_json(bytes, providers)?;
    if !bindings.protected_validation.is_empty()
      || !bindings.promotions.is_empty()
      || !bindings.commands.is_empty()
      || !bindings.authors.is_empty()
      || !bindings.protected_validation_evidence.is_empty()
      || !bindings.evaluation_decisions.is_empty()
      || !bindings.delivery.is_empty()
    {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    Ok(catalog)
  }
  fn parse_file_json(
    bytes: &[u8],
    providers: &[FactoryRuntimeSignalConnector],
  ) -> Result<(Self, FileBindings), FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    if bytes.len() > MAX_PROFILE_BYTES || providers.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(Invalid);
    }
    for (index, provider) in providers.iter().enumerate() {
      if providers[..index].iter().any(|previous| {
        previous.capability() == provider.capability() && previous.credential_profile() == provider.credential_profile()
      }) {
        return Err(Invalid);
      }
    }
    let file: CatalogFile = serde_json::from_slice(bytes).map_err(|_| Invalid)?;
    if file.version != 1 {
      return Err(Invalid);
    }
    if file.signals.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.tool_risk.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.review_planning.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.evaluation_decisions.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.protected_validation_evidence.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.delivery.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.commands.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.protected_test_authors.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.promotions.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || file.protected_validation.len() > MAX_FACTORY_EXECUTOR_BINDINGS
    {
      return Err(Invalid);
    }
    let projects = file
      .projects
      .into_iter()
      .map(|project| {
        let mut choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
          references: project
            .references
            .into_iter()
            .map(|choice| FactoryReferenceChoice {
              kind: choice.kind,
              alias: choice.alias,
              reference: choice.reference,
            })
            .collect(),
          build_configurations: project
            .build_configurations
            .into_iter()
            .map(|choice| (choice.alias, choice.reference))
            .collect(),
        })
        .map_err(|_| Invalid)?;
        for (alias, declaration) in project.role_profiles {
          if declaration.ceiling.build_configuration.project_id() != project.project_id
            || !choices
              .entries()
              .build_configurations
              .iter()
              .any(|(_, reference)| reference == &declaration.ceiling.build_configuration)
          {
            return Err(Invalid);
          }
          let local_permissions =
            LocalPermissionCeiling::try_new(declaration.ceiling.permissions.clone(), declaration.enforcement)
              .map_err(|_| Invalid)?;
          choices = choices
            .with_role_profile_capability(
              alias,
              FactoryRoleProfileCapability {
                ceiling: declaration.ceiling,
                parameters: declaration.parameters,
                public_environment: declaration.public_environment,
                local_permissions,
              },
            )
            .map_err(|_| Invalid)?;
        }
        Ok((project.project_id, choices))
      })
      .collect::<Result<_, FactoryRuntimeInstallationError>>()?;
    let mut codex: Vec<FactoryCodexBuildProfile> = file
      .codex
      .into_iter()
      .map(|profile| {
        if let Some(limits) = profile.tool_response_limits {
          limits.validate().map_err(|_| Invalid)?;
        }
        let capture = profile
          .capture
          .map(|capture| FactoryCodexCapturePolicy::new(capture.tool, capture.instruction).map_err(|_| Invalid))
          .transpose()?;
        let consumer = match (profile.execution_mode, profile.credential_purpose, capture.is_some()) {
          (None, FactoryCredentialPurpose::Evaluator, false) => FactoryCredentialConsumer::ReadOnlyReview,
          (None, FactoryCredentialPurpose::Model, true) => FactoryCredentialConsumer::CodingNode,
          (Some(CodexExecutionMode::ProtectedTools), FactoryCredentialPurpose::Model, false)
            if profile.candidate_capture_tool.is_none() && profile.tool_response_limits.is_some() =>
          {
            FactoryCredentialConsumer::ToolReasoning
          }
          _ => return Err(Invalid),
        };
        Ok(FactoryCodexBuildProfile {
          toolchain: FactoryTaskToolchain {
            plugin: profile.toolchain.plugin,
            executable: profile.toolchain.executable,
            model: profile.toolchain.model,
          },
          build_configuration: profile.build_configuration,
          instructions: profile.instructions,
          credentials: profile.credentials,
          consumer,
          usage_policy: profile.usage_policy,
          tool_risk: None,
          tool_response_limits: profile.tool_response_limits,
          reviewer: profile.reviewer,
          candidate_capture_tool: profile.candidate_capture_tool,
          capture,
          pipeline_node: profile.pipeline_node,
          parameters: profile.parameters,
          project_permissions: profile.project_permissions,
          local_permissions: LocalPermissionCeiling::try_new(
            profile.local_permissions.grants,
            profile.local_permissions.enforceable,
          )
          .map_err(|_| Invalid)?,
        })
      })
      .collect::<Result<_, FactoryRuntimeInstallationError>>()?;
    let signals = file
      .signals
      .into_iter()
      .map(|profile| {
        let provider = providers
          .iter()
          .find(|provider| {
            provider.capability() == &profile.plan.capability
              && provider.credential_profile() == &profile.credential_profile
          })
          .ok_or(Invalid)?
          .provider()
          .clone();
        let binding = FactoryDecisionSignalNodeBinding {
          action: profile.action,
          project_id: profile.project_id,
          plan: profile.plan,
          baseline_route: profile.baseline_route,
          credential_profile: profile.credential_profile,
          network_host: NetworkHost::new(profile.network_host).map_err(|_| Invalid)?,
          project_permissions: profile.project_permissions,
          local_permissions: LocalPermissionCeiling::try_new(
            profile.local_permissions.grants,
            profile.local_permissions.enforceable,
          )
          .map_err(|_| Invalid)?,
          outputs: profile
            .outputs
            .into_iter()
            .map(|output| FactoryDecisionSignalNodeOutput {
              disposition: output.disposition,
              outcome: output.outcome,
              projection: output.projection,
            })
            .collect(),
        }
        .pin_policy()
        .map_err(|_| Invalid)?;
        Ok(super::FactoryRuntimeSignalProfile {
          provider,
          bindings: vec![binding],
        })
      })
      .collect::<Result<_, FactoryRuntimeInstallationError>>()?;
    let tool_risk: Vec<FactoryToolRiskNodeProfile> = file
      .tool_risk
      .into_iter()
      .map(|profile| {
        let provider = providers
          .iter()
          .find(|provider| {
            provider.capability() == &profile.plan.capability
              && provider.credential_profile() == &profile.credential_profile
          })
          .ok_or(Invalid)?
          .provider()
          .clone();
        let binding = FactoryToolRiskBinding {
          project_id: profile.project_id,
          plan: profile.plan,
          mapping: profile.mapping,
          max_calls: profile.max_calls,
          credential_profile: profile.credential_profile,
          network_host: NetworkHost::new(profile.network_host).map_err(|_| Invalid)?,
          project_permissions: profile.project_permissions,
          local_permissions: LocalPermissionCeiling::try_new(
            profile.local_permissions.grants,
            profile.local_permissions.enforceable,
          )
          .map_err(|_| Invalid)?,
        };
        binding.policy_digest().map_err(|_| Invalid)?;
        Ok(FactoryToolRiskNodeProfile {
          provider,
          binding,
          base_task_digest: profile.base_task_digest,
          build_configuration: profile.build_configuration,
        })
      })
      .collect::<Result<_, FactoryRuntimeInstallationError>>()?;
    for profile in &mut codex {
      if profile.consumer != FactoryCredentialConsumer::ToolReasoning {
        continue;
      }
      let base = profile.base_task_digest();
      let mut policies = tool_risk.iter().filter(|policy| {
        policy.base_task_digest == base
          && policy.build_configuration == profile.build_configuration
          && policy.binding.project_id == profile.build_configuration.project_id()
      });
      let policy = policies.next().ok_or(Invalid)?;
      if policies.next().is_some() {
        return Err(Invalid);
      }
      profile.tool_risk = Some(
        octacity_server_application::FactoryCodexToolRiskPolicy::from_binding(&policy.binding).map_err(|_| Invalid)?,
      );
    }
    let catalog = Self {
      projects,
      codex,
      signals,
      tool_risk,
      review_planning: file
        .review_planning
        .into_iter()
        .map(|entry| {
          let profile = entry.profile;
          (
            entry.identity,
            FactoryReviewPlanningProfile {
              action: profile.action,
              ready: profile.ready,
              escalation: profile.escalation,
              purpose: profile.purpose,
              criterion_packs: profile.criterion_packs,
              branches: profile.branches,
              policy: profile.policy,
              data_handling: profile.data_handling,
              capabilities: profile.capabilities,
            },
          )
        })
        .collect(),
      executors: vec![],
    };
    catalog.validate(0)?;
    let commands = file
      .commands
      .into_iter()
      .map(CommandProfile::native)
      .collect::<Result<Vec<_>, _>>()?;
    validate_command_choices(&catalog, &commands)?;
    let authors = file
      .protected_test_authors
      .into_iter()
      .map(|entry| {
        let command = entry.command.native()?;
        validate_command_choices(&catalog, std::slice::from_ref(&command))?;
        octacity_server_application::FactoryProtectedTestAuthoringProfile::new(command, entry.policy)
          .map_err(|_| Invalid)
      })
      .collect::<Result<Vec<_>, _>>()?;
    let promotions = file
      .promotions
      .into_iter()
      .map(|entry| {
        let profiles = entry
          .profiles
          .into_iter()
          .map(CommandProfile::native)
          .collect::<Result<Vec<_>, _>>()?;
        if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
          return Err(Invalid);
        }
        validate_command_choices(&catalog, &profiles)?;
        if let Some(authority) = entry.deployment_authority {
          let authority = octacity_server_application::FactoryDeploymentCommandAuthority::new(
            authority.identity,
            authority.version,
            authority.environment,
            authority.endpoint,
            authority.credential_profile,
          )
          .map_err(|_| Invalid)?;
          let profiles = profiles
            .into_iter()
            .map(|command| {
              octacity_server_application::FactoryDeploymentCommandProfile::new(command, authority.clone())
                .map_err(|_| Invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
          Ok(PromotionCommandBinding::Deployment {
            identity: entry.identity,
            profiles,
          })
        } else {
          Ok(PromotionCommandBinding::Builtin {
            identity: entry.identity,
            profiles,
          })
        }
      })
      .collect::<Result<Vec<_>, FactoryRuntimeInstallationError>>()?;
    let protected_validation = file
      .protected_validation
      .into_iter()
      .map(|entry| {
        let profiles = entry
          .profiles
          .into_iter()
          .map(CommandProfile::native)
          .collect::<Result<Vec<_>, _>>()?;
        if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
          return Err(Invalid);
        }
        validate_command_choices(&catalog, &profiles)?;
        Ok(ValidationCommandBinding {
          profiles,
          inputs: super::FactoryRuntimeProtectedValidationProfile {
            author_profile: entry.inputs.author_profile,
            policy: entry.inputs.policy,
            inventory: entry.inputs.inventory,
            capture_tool: entry.inputs.capture_tool,
          },
          report: entry.report,
          facts: entry.facts,
        })
      })
      .collect::<Result<Vec<_>, FactoryRuntimeInstallationError>>()?;
    Ok((
      catalog,
      FileBindings {
        protected_validation,
        promotions,
        commands,
        authors,
        protected_validation_evidence: file.protected_validation_evidence,
        evaluation_decisions: file
          .evaluation_decisions
          .into_iter()
          .map(EvaluationDecisionEntry::native)
          .collect::<Result<_, _>>()?,
        delivery: file.delivery,
      },
    ))
  }
}

fn validate_command_choices(
  catalog: &FactoryRuntimeProfileCatalog,
  profiles: &[octacity_server_application::FactoryCommandBuildProfile],
) -> Result<(), FactoryRuntimeInstallationError> {
  if profiles.iter().any(|profile| {
    !catalog.projects.iter().any(|(project, choices)| {
      *project == profile.build_configuration.project_id()
        && choices
          .entries()
          .build_configurations
          .iter()
          .any(|(_, reference)| reference == &profile.build_configuration)
    })
  }) {
    return Err(FactoryRuntimeInstallationError::Invalid);
  }
  Ok(())
}
