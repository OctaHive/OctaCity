//! Closed read-only preset parameters and ordinary draft outcomes.
use crate::*;
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};

/// Bounded discovery in one visible Project, available before Factory execution is enabled.
#[derive(Clone, Debug)]
pub struct ListFactoryPresetsQuery {
  /// Owning Project and exclusive release identity cursor.
  pub page: CurrentDefinitionPageInput<FactoryKey>,
}
impl Query for ListFactoryPresetsQuery {
  type Outcome = FactoryPresetPage;
}
impl ManagementAuthorizationTarget for ListFactoryPresetsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Credential-free release metadata; aliases require independent exact Project profile selections.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryPresetSummary {
  /// Release content identity, including the complete template and process-completion criteria.
  pub reference: ImmutableReference,
  /// Ordinary template profile parameters, with no model or privilege defaults.
  pub profile_aliases: Vec<FactoryKey>,
  /// Typed ordinary authoring parameters; no model or credential values are supplied.
  pub parameters: Vec<FactoryPresetParameter>,
  /// Technical executor categories required by this release, not execution readiness.
  pub required_node_kinds: Vec<FlowNodeKind>,
}
/// One ordinary authoring selection advertised by the bundled release.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryPresetParameter {
  /// Closed request-field path, including independently selected stage aliases.
  pub path: String,
  /// Existing public contract category used by the authoring editor.
  pub kind: FactoryPresetParameterKind,
  /// Whether omission prevents a complete draft.
  pub required: bool,
}
/// Typed parameter categories; each uses its existing management contract.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryPresetParameterKind {
  /// Caller-selected publication identity.
  Identity,
  /// Exact published Role/Profile and Build binding.
  RoleProfile,
  /// Exact same-Project Repository version.
  Repository,
  /// Exact independently selected Project documents.
  ProjectContext,
  /// Root resource and permission ceilings.
  ExecutionPolicy,
  /// Installed admission and permission policy selections.
  ConfigurationPolicy,
  /// Optional disabled ordinary Source input and literal reporting selections.
  SourceInput,
  /// Finite accepted-gate effects compiled by the ordinary reporting composer.
  ReportingEffects,
}
/// Ordinary bounded discovery envelope, without private profile content or installation paths.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryPresetPage {
  /// Visible release count before pagination.
  pub total: u64,
  /// Release summaries in identity order.
  pub items: Vec<FactoryPresetSummary>,
  /// Exclusive continuation identity, absent at the end.
  pub next_cursor: Option<FactoryKey>,
}
/// Closed preset selection; dependency choices remain explicit ordinary authoring parameters.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPresetResolutionRequest {
  /// Exact release selected through discovery, never a mutable name alone.
  pub preset: ImmutableReference,
  /// Caller-owned prospective root identity; no version or identity is reserved by resolution.
  pub flow_id: Option<FlowDefinitionId>,
  /// Caller-owned prospective Configuration identity for ordinary publication.
  pub configuration_id: Option<FactoryConfigurationId>,
  /// Independently reviewed exact stage profiles and ordinary Build result bindings.
  #[serde(default)]
  pub profiles: std::collections::BTreeMap<FactoryKey, FactoryPresetProfileSelection>,
  /// Independently selected code Repository, never inferred from a ticket provider.
  pub repository: Option<WorkSourceRepositoryRef>,
  /// Explicit per-stage Project context; no preceding stage's private Context is inherited.
  #[serde(default)]
  pub context: std::collections::BTreeMap<FactoryKey, TriageProjectContextDraft>,
  /// Explicit root resource and authority ceilings.
  pub execution: Option<FlowExecutionPolicy>,
  /// Ordinary Configuration policy choices; publication and activation remain explicit.
  pub configuration: Option<FactoryPresetConfigurationParameters>,
  /// Optional ordinary intake settings; resolution never enables or publishes them.
  pub input: Option<FactoryPresetInputSelection>,
  /// Optional accepted-gate effects with caller-owned prospective child Flow identities.
  #[serde(default)]
  pub effects: std::collections::BTreeMap<FactoryKey, FactoryPresetEffectSelection>,
}
/// A reviewed finite reporting composition, published through ordinary Flow authoring.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPresetEffectSelection {
  /// Caller-owned child Flow identity; no identity is reserved by resolution.
  pub id: FlowDefinitionId,
  /// Existing finite outcome/dimension reporting compiler parameters.
  pub definition: FactoryResultReportingDraft,
}
/// One ordinary child draft that must precede standalone root Flow publication.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryPresetFlowDependency {
  /// Caller-selected publication identity.
  pub id: FlowDefinitionId,
  /// Ordinary draft for the existing Flow validate/publish commands.
  pub definition: FactoryFlowDefinitionDraft,
}
/// Ordinary Source selections without an independently supplied target or activation flag.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPresetInputSelection {
  /// Caller-owned prospective input identity.
  pub id: WorkSourceInputId,
  /// Exact reviewed connection metadata and Project access version.
  pub connection: WorkSourceConnectionRef,
  /// Exact installed read profile, selected independently of the code Repository.
  pub source: WorkSourceBinding,
  /// Optional independently installed writer profile.
  pub reporter: Option<WorkSourceBinding>,
  /// Optional finite literal status effects, subject to the installed editing ceiling.
  pub reporting: Option<WorkSourceStatusMapping>,
  /// Optional supported narrowing of the installed read profile.
  pub predicate: Option<WorkSourcePredicate>,
}
/// Exact independently selected role/profile and its existing Build contract.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPresetProfileSelection {
  /// Exact reviewed responsibility version.
  pub role: FactoryRoleRef,
  /// Exact published Project profile version.
  pub profile: FactoryRoleProfileRef,
  /// Ordinary stage result/capture and installed Build bindings.
  pub build: FlowBuildBinding,
}
/// Configuration choices without a caller-substituted Flow or automatic activation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPresetConfigurationParameters {
  /// Existing installed admission policy alias.
  pub admission_policy: FactoryKey,
  /// Explicit concurrency ceilings.
  pub wip_limits: FactoryWipLimits,
  /// Hard budget enclosing the complete resolved closure.
  pub hard_budget: BudgetLimit,
  /// Existing installed Project permission ceiling alias.
  pub permission_ceiling: FactoryKey,
}
/// Read-only review under the selected Project's current visibility.
#[derive(Clone, Debug)]
pub struct ResolveFactoryPresetQuery {
  /// Owning Project, obtained from the management resource path.
  pub project_id: octacity_server_domain::ProjectId,
  /// Exact selected release and its reviewed parameter choices.
  pub selection: FactoryPresetResolutionRequest,
}
impl Query for ResolveFactoryPresetQuery {
  type Outcome = FactoryPresetResolution;
}
impl ManagementAuthorizationTarget for ResolveFactoryPresetQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ListFactoryPresetsQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Closed safe dependency reason, independent from private installation errors.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryPresetDependencyReason {
  /// An explicit reviewed selection is required before an ordinary draft can be produced.
  Required,
  /// The exact selected dependency cannot currently be read in this Project.
  Unavailable,
}
/// Actionable parameter path; unavailable references and private paths are never echoed.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryPresetDependencyProblem {
  /// Closed request-field location for remediation.
  pub path: String,
  /// Safe code for clients to localize.
  pub reason: FactoryPresetDependencyReason,
}
/// Review result, never a published version, execution or readiness certificate.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryPresetResolution {
  /// Exact release used for this review.
  pub preset: ImmutableReference,
  /// Reviewable process-completion draft; no managed document is written by resolution.
  pub completion: SourceCompletionDocument,
  /// Missing or incompatible dependencies, without hidden entity details.
  pub diagnostics: Vec<FactoryPresetDependencyProblem>,
  /// Ordinary Flow draft when all required selections can be resolved.
  pub flow: Option<FactoryFlowDefinitionDraft>,
  /// Ordinary Configuration draft, independently validated and published by existing commands.
  pub configuration: Option<FactoryConfigurationDraft>,
  /// Ordinary disabled input intent; publish only after its exact target Configuration exists.
  pub input: Option<WorkSourceInputIntent>,
  /// Ordinary child Flow drafts in dependency publication order, never receipts.
  pub dependencies: Vec<FactoryPresetFlowDependency>,
}
impl FactoryPresetResolution {
  pub(super) fn unavailable(mut self, path: impl Into<String>) -> Self {
    self.diagnostics.push(FactoryPresetDependencyProblem {
      path: path.into(),
      reason: FactoryPresetDependencyReason::Unavailable,
    });
    self
  }
}
