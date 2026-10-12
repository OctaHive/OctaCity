use serde::{Deserialize, Serialize};

use super::Cursor;
/// Closed request interpreted only by the exact retained HumanGate binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryNodeOperationRequest {
  /// Escalation, delivery or promotion purpose.
  pub operation: String,
  /// Bounded disposition valid for the chosen purpose.
  pub disposition: String,
  /// Bounded human reason; retained privately rather than echoed in diagnostics.
  pub reason: String,
}
/// Observations for one exact pending HumanGate. Its schema is server-selected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryNodeResponseRequest {
  /// One finite outcome from the pinned node declaration.
  pub outcome: String,
  /// Bounded observations checked against the retained outcome schema.
  pub value: serde_json::Value,
}
/// Safe receipt of a bounded, preconditioned operator intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunControlResponse {
  /// Applied or replayed original intent.
  pub disposition: super::MutationDisposition,
  /// Exact controlled Run.
  pub run_id: String,
  /// Aggregate version produced by the original intent.
  pub version: u64,
  /// Code-owned lifecycle state after accepting the intent.
  pub state: String,
  /// Durable cancellation flag consumed by the common owner.
  pub cancellation_requested: bool,
}

/// Initial immutable configuration publication with a stable replay identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFactoryConfigurationRequest {
  /// Client-selected non-nil UUID, retained across unknown responses.
  pub id: String,
  /// Existing owning Project.
  pub project_id: String,
  /// Closed provider-neutral configuration document, decoded by the application.
  pub definition: serde_json::Value,
}

/// Complete replacement definition; `If-Match` carries the current immutable version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceFactoryConfigurationRequest {
  /// Closed configuration grammar, validated by the application before publication.
  pub definition: serde_json::Value,
}

/// Exact immutable configuration document and authoritative publication time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationDetails {
  /// Resolved configuration grammar described by the management schema.
  pub configuration: serde_json::Value,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Durable publication result; an exact replay returns the original resource.
pub type FactoryConfigurationMutationResponse = super::MutationResponse<FactoryConfigurationDetails>;

/// Safe current immutable Factory Configuration facts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationSummaryResource {
  /// Stable configuration identity.
  pub id: String,
  /// Owning Project.
  pub project_id: String,
  /// Current immutable version.
  pub version: u64,
  /// Whether new Work can select this version.
  pub enabled: bool,
  /// Identity creation time in Unix milliseconds.
  pub created_at_unix_ms: i64,
  /// Current-version publication time in Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Visible count and a bounded newest-first configuration page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationSummaryPage {
  /// Visible matching count before cursor pagination.
  pub total: u64,
  /// Safe visible summaries.
  pub items: Vec<FactoryConfigurationSummaryResource>,
  /// Exclusive composite continuation position.
  pub next_cursor: Option<Cursor>,
}

/// Exact credential-free logical capability or policy reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryImmutableReferenceResource {
  /// Logical installed capability or policy identity.
  pub identity: String,
  /// Immutable version.
  pub version: String,
  /// Canonical content digest.
  pub digest: String,
}

/// Exact Project-owned Build Configuration selected at publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryBuildConfigurationReferenceResource {
  /// Stable Build Configuration identity.
  pub id: String,
  /// Exact immutable version.
  pub version: u64,
  /// Owning Project.
  pub project_id: String,
  /// Exact definition digest.
  pub definition_digest: String,
}

/// Closed shape of a selectable reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "reference", deny_unknown_fields)]
pub enum FactoryChoiceReferenceResource {
  /// Logical installed capability or immutable policy.
  Immutable(FactoryImmutableReferenceResource),
  /// Exact Project-owned Build Configuration.
  BuildConfiguration(FactoryBuildConfigurationReferenceResource),
}

/// One visible selectable alias with its exact resolved identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationChoiceResource {
  /// Project-scoped authorization identity.
  pub identity: String,
  /// Closed technical reference category.
  pub kind: String,
  /// Operator-facing alias selectable in configuration fields.
  pub alias: String,
  /// Exact immutable reference facts, without provider credentials.
  pub reference: FactoryChoiceReferenceResource,
  /// Safe deployment declaration; absence means compatibility is not declared.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub metadata: Option<octacity_server_application::FactoryChoiceMetadata>,
}

/// Visible matching count and a bounded exact-reference page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationChoicePage {
  /// Count after visibility and category selection, before continuation.
  pub total: u64,
  /// Choices ordered by technical category and alias.
  pub items: Vec<FactoryConfigurationChoiceResource>,
  /// Exclusive Project-scoped continuation identity.
  pub next_cursor: Option<Cursor>,
}

/// Typed root graph and exact nested references for validation or first publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFactoryFlowDefinitionRequest {
  /// Client-owned stable non-nil identity.
  pub id: String,
  /// Existing owning Project.
  pub project_id: String,
  /// Closed root graph grammar decoded by the application.
  pub definition: serde_json::Value,
}

/// Complete next immutable root content; `If-Match` selects the baseline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceFactoryFlowDefinitionRequest {
  /// Closed graph authoring grammar.
  pub definition: serde_json::Value,
}

/// Bounded provider-neutral manual Work submission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmitManualFactoryWorkRequest {
  /// Visible exact configuration.
  pub configuration_id: String,
  /// Immutable configuration version.
  pub configuration_version: u64,
  /// Visible exact Repository.
  pub repository_id: String,
  /// Immutable Repository version.
  pub repository_version: u64,
  /// Source-scoped stable external Work identity.
  pub external_identity: String,
  /// Closed source selection shared with ordinary CI/CD.
  pub source: serde_json::Value,
  /// Exact task, acceptance and optional specification artifacts.
  pub artifacts: serde_json::Value,
  /// Bounded priority, risk and safe metadata.
  pub classification: serde_json::Value,
}

/// Exact immutable root publication and its complete validated closure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryFlowDefinitionDetails {
  /// Owning Project.
  pub project_id: String,
  /// Closed immutable graph and contract grammar.
  pub flow: serde_json::Value,
  /// Original publication time.
  pub published_at_unix_ms: i64,
}
/// Idempotent immutable Flow publication result.
pub type FactoryFlowDefinitionMutationResponse = super::MutationResponse<FactoryFlowDefinitionDetails>;
/// Bounded visible root definitions, also usable as exact subflow choices.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryFlowDefinitionPage {
  /// Visible count before continuation.
  pub total: u64,
  /// Exact visible definitions in newest-first identity order.
  pub items: Vec<FactoryFlowDefinitionDetails>,
  /// Self-contained exclusive composite continuation.
  pub next_cursor: Option<Cursor>,
}
/// Safe provider-neutral manual admission result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryAdmissionResponse {
  /// Applied or replayed original admission.
  pub disposition: super::MutationDisposition,
  /// Immutable admitted Work identity.
  pub work_envelope_id: String,
  /// Durable Factory Run identity.
  pub factory_run_id: String,
  /// Exact originally resolved subject revision.
  pub base_revision: String,
  /// Technical Run state, independent of logical phases.
  pub state: String,
  /// Original admission time.
  pub admitted_at_unix_ms: i64,
}
/// Safe current Run and immutable Work facts for discovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunSummaryResource {
  /// Safe exact subject, current attempts, budget and authoritative disposition facts.
  pub diagnostics: serde_json::Value,
  /// Stable Run identity.
  pub id: String,
  /// Owning Project.
  pub project_id: String,
  /// Immutable Work identity.
  pub work_id: String,
  /// Selected Factory Configuration.
  pub configuration_id: String,
  /// Exact configuration version pinned by admission.
  pub configuration_version: u64,
  /// Provider-neutral source kind.
  pub source: String,
  /// Scoped external Work identity.
  pub external_identity: String,
  /// Authoritative technical state.
  pub state: String,
  /// Optimistic aggregate version.
  pub version: u64,
  /// Original admission time.
  pub admitted_at_unix_ms: i64,
  /// Last authoritative update time.
  pub updated_at_unix_ms: i64,
}
/// Bounded authorization-filtered Run summaries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRunSummaryPage {
  /// Visible matching count before continuation.
  pub total: u64,
  /// Summaries ordered newest first with stable identity ties.
  pub items: Vec<FactoryRunSummaryResource>,
  /// Exclusive admission-time and identity position.
  pub next_cursor: Option<Cursor>,
}
/// Safe phase-ready facts; frozen inputs and dependency payloads are omitted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPhaseReadyResource {
  /// Exact entry identity.
  pub id: String,
  /// Immutable pool policy identity.
  pub policy_digest: String,
  /// Immutable Work identity.
  pub work_id: String,
  /// Owning Project.
  pub project_id: String,
  /// Current Run identity.
  pub run_id: String,
  /// Exact readiness version.
  pub run_version: u64,
  /// Owning nested or root Flow Run.
  pub flow_run_id: String,
  /// Exact current workflow cycle.
  pub cycle_id: String,
  /// Operator-defined node key.
  pub node: String,
  /// Zero-based execution generation.
  pub generation: u32,
  /// Trusted severity ordering observation.
  pub severity: String,
  /// Frozen operator-defined Project priority.
  pub project_priority: i32,
  /// Original admission time; refresh does not change age.
  pub admitted_at_unix_ms: i64,
}
/// Visible current phase entries in exact immutable policy order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPhaseReadyPage {
  /// Matching visible count before continuation.
  pub total: u64,
  /// Safe current phase projections.
  pub items: Vec<FactoryPhaseReadyResource>,
  /// Complete exclusive order tuple without an anchor lookup.
  pub next_cursor: Option<Cursor>,
}
