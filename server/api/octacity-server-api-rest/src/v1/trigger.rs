use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::MutationDisposition;

/// Request body for publishing an immutable Project policy version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishProjectPolicyRequest {
  /// Complete strict policy-directive document.
  pub policy: Value,
}

/// REST result of a Project policy publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPolicyResource {
  /// Owning Project identity.
  pub project_id: String,
  /// Exact immutable policy version.
  pub version: u64,
}

/// Request body for creating a manual Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateManualTriggerDefinitionRequest {
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether occurrences may be accepted.
  pub enabled: bool,
  /// Manual Trigger-specific bounded definition document.
  pub definition: Value,
}

/// Explicit policy for schedule occurrences missed while no worker owned them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MissedRunPolicy {
  /// Evaluate the oldest due occurrence once and advance past the remaining gap.
  RunOnce,
  /// Evaluate oldest occurrences in bounded batches.
  CatchUp {
    /// Positive per-claim limit, bounded by the server contract.
    maximum_occurrences: u16,
  },
}

/// Calendar definition for a scheduled Trigger.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleDefinition {
  /// Seven-field cron expression including seconds and year.
  pub expression: String,
  /// IANA timezone name.
  pub timezone: String,
  /// Explicit restart and downtime behavior.
  pub missed_run_policy: MissedRunPolicy,
}

/// Build input evaluated for every scheduled occurrence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledBuildDefinition {
  /// Source expression resolved for each occurrence.
  pub source: ManualSource,
  /// Parameter values resolved against the immutable configuration.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
}

/// Request body for creating one durable scheduled Trigger.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateScheduledTriggerDefinitionRequest {
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether workers may evaluate occurrences.
  pub enabled: bool,
  /// Calendar and missed-run behavior.
  pub schedule: ScheduleDefinition,
  /// Build input shared by the schedule occurrences.
  pub build: ScheduledBuildDefinition,
}

/// Management representation of one durable schedule.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleResource {
  /// Exact owning Trigger identity.
  pub trigger_id: String,
  /// Exact immutable Trigger version.
  pub trigger_version: u64,
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether workers may evaluate occurrences.
  pub enabled: bool,
  /// Calendar and missed-run behavior.
  pub schedule: ScheduleDefinition,
  /// First occurrence not yet completed, as Unix milliseconds.
  pub next_occurrence_at_unix_ms: i64,
  /// Build input shared by the schedule occurrences.
  pub build: ScheduledBuildDefinition,
}

/// REST result of creating a Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDefinitionResource {
  /// Stable Trigger identity.
  pub id: String,
  /// Exact immutable Trigger version.
  pub version: u64,
}

/// Request body for creating an operator-managed remote webhook.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUnmanagedWebhookRequest {
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether authenticated deliveries may be evaluated.
  pub enabled: bool,
  /// Operator-installed provider adapter identity.
  pub adapter_id: String,
  /// Lowercase SHA-256 pin for the provider adapter executable.
  pub adapter_sha256: String,
  /// Logical protected verification-material handle.
  pub verification_material_handle: String,
  /// Sorted lowercase headers required for provider authentication.
  pub verification_headers: Vec<String>,
  /// Repository identity normalized deliveries must name.
  pub repository_id: String,
  /// Provider-neutral event kind to match.
  pub event_kind: String,
  /// Build parameters supplied to matching deliveries.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
}

/// Secret-free verification instructions for manual provider configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookVerificationRequirements {
  /// Provider adapter that owns verification semantics.
  pub adapter_id: String,
  /// Exact immutable adapter executable digest.
  pub adapter_sha256: String,
  /// Provider headers that must be delivered to the callback.
  pub required_headers: Vec<String>,
  /// Verification material is supplied through a protected logical handle.
  pub protected_material_required: bool,
}

/// Management representation returned after unmanaged webhook creation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnmanagedWebhookResource {
  /// Whether the mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Server-owned integration identity.
  pub integration_id: String,
  /// Exact immutable external Trigger identity.
  pub trigger: TriggerDefinitionResource,
  /// Public callback URL to configure at the provider.
  pub callback_url: String,
  /// Secret-free provider verification requirements.
  pub verification: WebhookVerificationRequirements,
}

/// Request body for creating a provider-managed remote webhook.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateManagedWebhookRequest {
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether authenticated deliveries may be evaluated.
  pub enabled: bool,
  /// Operator-installed provider adapter identity.
  pub adapter_id: String,
  /// Lowercase SHA-256 pin for the provider adapter executable.
  pub adapter_sha256: String,
  /// Logical protected verification-material handle.
  pub verification_material_handle: String,
  /// Sorted lowercase headers required for provider authentication.
  pub verification_headers: Vec<String>,
  /// Protected provider-administration credential handle.
  pub administration_credential_handle: String,
  /// Repository identity normalized deliveries must name.
  pub repository_id: String,
  /// Provider-neutral event kind to match.
  pub event_kind: String,
  /// Build parameters supplied to matching deliveries.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
}

impl std::fmt::Debug for CreateManagedWebhookRequest {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("CreateManagedWebhookRequest")
      .field("configuration_id", &self.configuration_id)
      .field("configuration_version", &self.configuration_version)
      .field("enabled", &self.enabled)
      .field("adapter_id", &self.adapter_id)
      .field("adapter_sha256", &self.adapter_sha256)
      .field("verification_material_handle", &"<redacted>")
      .field("verification_headers", &self.verification_headers)
      .field("administration_credential_handle", &"<redacted>")
      .field("repository_id", &self.repository_id)
      .field("event_kind", &self.event_kind)
      .field("parameters", &self.parameters)
      .field("priority", &self.priority)
      .finish()
  }
}

/// Provider-neutral remote registration state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedWebhookRegistrationStatus {
  /// Remote registration exists and is enabled.
  Active,
  /// Remote registration exists but is disabled.
  Disabled,
  /// Remote registration is absent.
  Missing,
}

/// Secret-free remote registration representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedWebhookRegistrationResource {
  /// Opaque remote provider identity.
  pub registration_id: String,
  /// Current normalized lifecycle state.
  pub status: ManagedWebhookRegistrationStatus,
  /// Callback URL observed by the provider.
  pub callback_url: String,
}

/// Management representation of one managed webhook lifecycle result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedWebhookResource {
  /// Whether the local mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Server-owned integration identity.
  pub integration_id: String,
  /// Exact immutable external Trigger identity.
  pub trigger: TriggerDefinitionResource,
  /// Public callback URL managed by the provider adapter.
  pub callback_url: String,
  /// Secret-free current remote registration, absent while provider work is pending.
  pub registration: Option<ManagedWebhookRegistrationResource>,
}

/// Source expression supplied by a manual Build request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManualSource {
  /// Resolve the Repository version's configured default reference.
  DefaultReference,
  /// Resolve one explicitly allowed mutable branch or tag.
  Reference(String),
  /// Verify and use an explicitly supplied immutable revision.
  ExactRevision(String),
}

/// Request body for accepting one manual Trigger occurrence.
///
/// Transport observation and acceptance times are server-owned. Replay
/// identity is carried by `Idempotency-Key`, while the deduplication identity
/// remains part of the normalized Trigger intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptManualTriggerRequest {
  /// Exact immutable Trigger definition identity.
  pub trigger_id: String,
  /// Exact immutable Trigger definition version.
  pub trigger_version: u64,
  /// Exact Build Configuration target identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Stable source-scoped occurrence identity.
  pub deduplication_identity: String,
  /// Allowed source expression to resolve once.
  pub source: ManualSource,
  /// Primitive parameter values supplied before defaults are applied.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority copied to root Jobs.
  pub priority: i64,
}

/// REST result of one accepted or intentionally suppressed Trigger evaluation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerEvaluationResponse {
  /// Evaluation created one Build and materialized its first Attempt.
  Accepted {
    /// Whether this request applied or replayed the evaluation.
    disposition: MutationDisposition,
    /// Stable normalized Trigger occurrence identity.
    trigger_occurrence_id: String,
    /// Build associated with the occurrence.
    build_id: String,
    /// First Attempt associated with the Build.
    attempt_id: String,
    /// Root Jobs inserted into the global ready queue.
    ready_job_ids: Vec<String>,
  },
  /// Policy intentionally created no Build or queued work.
  Suppressed {
    /// Whether this request applied or replayed the evaluation.
    disposition: MutationDisposition,
    /// Stable normalized Trigger occurrence identity.
    trigger_occurrence_id: String,
  },
}
