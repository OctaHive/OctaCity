use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, IntegrationId, RepositoryId, SourceReference,
  Timestamp, TriggerId, TriggerIdentity, TriggerVersion,
};
use octacity_server_store::{
  CreateManagedWebhook, CreateTriggerDefinition, CreateUnmanagedWebhook, EnqueueManagedWebhookOperation,
  EnqueueWebhookDelivery, IdempotencyKey, ManagedWebhookDefinition, ManagedWebhookOperation,
  ManagedWebhookOperationStore, ManagedWebhookRegistration as StoredManagedWebhookRegistration,
  ManagedWebhookRegistrationStatus as StoredManagedWebhookRegistrationStatus, ManagedWebhookRegistrationStore,
  StoreError, TriggerDefinitionRef, TriggerEventKind, TriggerKind, UnmanagedWebhookDefinition,
  WebhookConfigurationStore, WebhookDeliveryAdmissionStore, WebhookDeliveryId, WebhookFailureCode,
  WebhookIntegrationReader, canonical_webhook_headers,
};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{ApplicationError, ApplicationFailure, Command, CommandHandler, MutationDisposition};

mod ingress;
mod management;
mod support;

pub use ingress::WebhookIngressService;
pub use management::WebhookManagementService;
pub(crate) use support::{durable_webhook_diagnostic, ensure_managed_result, webhook_failure_code};
use support::{ensure_callback, managed_provider_error, valid_webhook_callback_origin, validated_webhook_definition};

/// Typed management command for an operator-configured remote webhook.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateUnmanagedWebhookCommand {
  /// Server-selected integration identity used by the public callback.
  pub integration_id: IntegrationId,
  /// Server-selected immutable external Trigger identity.
  pub trigger_id: TriggerId,
  /// Target Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable target version.
  pub configuration_version: BuildConfigurationVersion,
  /// Whether deliveries may currently create Builds.
  pub enabled: bool,
  /// Operator-installed provider adapter identity.
  pub adapter_id: String,
  /// SHA-256 pin for the selected adapter executable.
  pub adapter_sha256: String,
  /// Logical verification-material handle, never returned to management clients.
  pub verification_material_handle: String,
  /// Lowercase headers the provider adapter requires for authentication.
  pub verification_headers: Vec<String>,
  /// Repository identity normalized deliveries must name.
  pub repository_id: RepositoryId,
  /// Normalized event kind accepted by this Trigger.
  pub event_kind: TriggerEventKind,
  /// Build parameters supplied for accepted deliveries.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
  /// Stable management replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

impl Command for CreateUnmanagedWebhookCommand {
  type Outcome = UnmanagedWebhookProjection;
}

/// Secret-free instructions needed to configure the remote webhook manually.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookVerificationRequirements {
  /// Installed adapter that defines provider-specific verification semantics.
  pub adapter_id: String,
  /// Exact immutable executable selected for verification.
  pub adapter_sha256: String,
  /// Header names the remote provider must send.
  pub required_headers: Vec<String>,
  /// Verification material must be configured through a protected logical handle.
  pub protected_material_required: bool,
}

/// Management projection for an unmanaged webhook integration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnmanagedWebhookProjection {
  /// Whether creation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Server-owned integration identity.
  pub integration_id: IntegrationId,
  /// Exact immutable external Trigger.
  pub trigger: TriggerDefinitionRef,
  /// Public URL the operator configures at the provider.
  pub callback_url: String,
  /// Secret-free provider verification requirements.
  pub verification: WebhookVerificationRequirements,
}

/// Typed management command for a provider-managed remote webhook.
#[derive(Clone, Eq, PartialEq)]
pub struct CreateManagedWebhookCommand {
  /// Server-selected integration identity used by the public callback.
  pub integration_id: IntegrationId,
  /// Server-selected immutable external Trigger identity.
  pub trigger_id: TriggerId,
  /// Target Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable target version.
  pub configuration_version: BuildConfigurationVersion,
  /// Whether deliveries may currently create Builds.
  pub enabled: bool,
  /// Operator-installed provider adapter identity.
  pub adapter_id: String,
  /// SHA-256 pin for the selected adapter executable.
  pub adapter_sha256: String,
  /// Logical verification-material handle used only for incoming delivery authentication.
  pub verification_material_handle: String,
  /// Lowercase headers the provider adapter requires for authentication.
  pub verification_headers: Vec<String>,
  /// Protected provider-administration credential handle used only for managed operations.
  pub administration_credential_handle: String,
  /// Repository identity normalized deliveries must name.
  pub repository_id: RepositoryId,
  /// Normalized event kind accepted by this Trigger.
  pub event_kind: TriggerEventKind,
  /// Build parameters supplied for accepted deliveries.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
  /// Stable management and provider replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative first-attempt time.
  pub created_at: Timestamp,
}

impl std::fmt::Debug for CreateManagedWebhookCommand {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("CreateManagedWebhookCommand")
      .field("integration_id", &self.integration_id)
      .field("trigger_id", &self.trigger_id)
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
      .field("idempotency_key", &self.idempotency_key)
      .field("created_at", &self.created_at)
      .finish()
  }
}

impl Command for CreateManagedWebhookCommand {
  type Outcome = ManagedWebhookProjection;
}

/// Typed command that synchronizes one existing managed registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManageWebhookRegistrationCommand {
  /// Server-owned managed integration identity.
  pub integration_id: IntegrationId,
  /// Provider lifecycle operation to execute.
  pub operation: ManagedWebhookOperation,
  /// Stable operation replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Server observation time.
  pub observed_at: Timestamp,
}

impl Command for ManageWebhookRegistrationCommand {
  type Outcome = ManagedWebhookProjection;
}

/// Secret-free managed registration status exposed by the application layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedWebhookRegistrationStatus {
  /// Remote registration exists and is enabled.
  Active,
  /// Remote registration exists but is disabled.
  Disabled,
  /// Remote registration is absent.
  Missing,
}

/// Secret-free normalized managed registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedWebhookRegistration {
  /// Opaque remote provider identity.
  pub registration_id: String,
  /// Current normalized remote state.
  pub status: ManagedWebhookRegistrationStatus,
  /// Callback URL observed by the provider.
  pub callback_url: String,
}

/// Management projection for a provider-managed webhook integration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedWebhookProjection {
  /// Whether the local lifecycle mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Server-owned integration identity.
  pub integration_id: IntegrationId,
  /// Exact immutable external Trigger.
  pub trigger: TriggerDefinitionRef,
  /// Public callback managed by the provider adapter.
  pub callback_url: String,
  /// Secret-free current remote state, absent while durable provider work is pending.
  pub registration: Option<ManagedWebhookRegistration>,
}

/// Exact raw delivery admitted by the public webhook HTTP adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptWebhookDeliveryCommand {
  /// Callback-scoped integration identity.
  pub integration_id: IntegrationId,
  /// Lowercase HTTP headers retained until the integration allowlist is loaded.
  pub headers: BTreeMap<String, String>,
  /// Exact request body bytes; authentication always precedes parsing by the Trigger Engine.
  pub body: Vec<u8>,
  /// Server observation time used when the provider supplies no timestamp.
  pub received_at: Timestamp,
}

impl AcceptWebhookDeliveryCommand {
  /// Validates transport primitives before they enter the application handler.
  pub fn from_transport(
    integration_id: &str,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    received_at_unix_ms: i64,
  ) -> Result<Self, WebhookDeliveryInputError> {
    Ok(Self {
      integration_id: integration_id
        .parse()
        .map_err(|_| WebhookDeliveryInputError::InvalidIntegrationId)?,
      headers,
      body,
      received_at: Timestamp::from_unix_millis(received_at_unix_ms)
        .map_err(|_| WebhookDeliveryInputError::InvalidReceivedAt)?,
    })
  }
}

/// Failure while converting public transport primitives into a delivery command.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WebhookDeliveryInputError {
  /// Callback path does not contain a valid integration identity.
  #[error("webhook integration identity is invalid")]
  InvalidIntegrationId,
  /// Server observation time cannot be represented by the domain clock.
  #[error("webhook receipt time is invalid")]
  InvalidReceivedAt,
}

impl Command for AcceptWebhookDeliveryCommand {
  type Outcome = WebhookDeliveryAccepted;
}

/// Durable admission result returned before adapter authentication begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebhookDeliveryAccepted {
  /// Server-owned receipt identity used for diagnostics and causality.
  pub delivery_id: WebhookDeliveryId,
  /// Whether this exact server receipt was newly stored or replayed.
  pub disposition: MutationDisposition,
}

/// Provider-neutral input passed to the selected verification adapter.
pub struct VerifyWebhookDelivery {
  /// Durable server receipt identity reused across transient attempts.
  pub delivery_id: WebhookDeliveryId,
  /// Integration being authenticated.
  pub integration_id: IntegrationId,
  /// Immutable adapter identity.
  pub adapter_id: String,
  /// Immutable adapter digest.
  pub adapter_sha256: String,
  /// Protected verification-material handle.
  pub verification_material_handle: String,
  /// Allowlisted transport headers only.
  pub headers: BTreeMap<String, String>,
  /// Exact request bytes.
  pub body: Vec<u8>,
}

impl std::fmt::Debug for VerifyWebhookDelivery {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("VerifyWebhookDelivery")
      .field("delivery_id", &self.delivery_id)
      .field("integration_id", &self.integration_id)
      .field("adapter_id", &self.adapter_id)
      .field("adapter_sha256", &self.adapter_sha256)
      .field("verification_material_handle", &"<redacted>")
      .field("header_names", &self.headers.keys().collect::<Vec<_>>())
      .field("body", &"<redacted>")
      .finish()
  }
}

/// Authenticated provider-neutral repository event returned by an adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedWebhookEvent {
  /// Integration echoed by the adapter.
  pub integration_id: IntegrationId,
  /// Provider delivery identity scoped to the integration.
  pub delivery_id: TriggerIdentity,
  /// Provider-neutral event kind.
  pub event_kind: TriggerEventKind,
  /// Repository named by the normalized event.
  pub repository_id: RepositoryId,
  /// Optional mutable reference reported by the provider.
  pub reference: Option<SourceReference>,
  /// Optional immutable revision reported by the provider.
  pub revision: Option<ImmutableRevision>,
  /// Optional provider observation time.
  pub provider_time: Option<Timestamp>,
  /// Optional display-only actor name retained as metadata.
  pub actor_display_name: Option<String>,
  /// Bounded provider metadata retained only after authentication.
  pub metadata: BTreeMap<String, String>,
}

/// Stable verifier failure vocabulary exposed to the application service.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WebhookVerificationError {
  /// Signature, token, or another provider proof did not authenticate.
  #[error("webhook delivery authentication failed")]
  AuthenticationFailed,
  /// Installed adapter or integration configuration is permanently invalid.
  #[error("webhook verifier configuration is invalid")]
  InvalidConfiguration,
  /// Selected adapter does not advertise the requested optional operation.
  #[error("webhook provider operation is unsupported")]
  Unsupported,
  /// Provider rejected the requested lifecycle change permanently.
  #[error("webhook provider rejected the operation")]
  Permanent,
  /// Provider operation was cooperatively cancelled.
  #[error("webhook provider operation was cancelled")]
  Cancelled,
  /// Verification dependency may succeed on a later attempt.
  #[error("webhook verifier is temporarily unavailable")]
  Unavailable,
  /// Provider adapter violated its normalized protocol.
  #[error("webhook verifier returned an invalid response")]
  InvalidResponse,
  /// Provider returned a validated safe failure with operation-specific diagnostics.
  #[error("webhook provider failure ({code}): {diagnostic}")]
  ProviderFailure {
    /// Application classification used for retry and transport mapping.
    classification: WebhookVerificationFailure,
    /// Stable provider-defined machine code.
    code: String,
    /// Bounded provider diagnostic validated by the wire protocol.
    diagnostic: String,
    /// Optional provider retry guidance retained for diagnostics.
    retry_after_milliseconds: Option<u64>,
  },
}

/// Stable application classification for webhook provider failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookVerificationFailure {
  /// Provider proof did not authenticate.
  AuthenticationFailed,
  /// Integration or installation configuration is invalid.
  InvalidConfiguration,
  /// Requested optional operation is unsupported.
  Unsupported,
  /// Provider permanently rejected the operation.
  Permanent,
  /// Provider operation was cooperatively cancelled.
  Cancelled,
  /// Operation may succeed on bounded retry.
  Unavailable,
  /// Adapter response violated the protocol.
  InvalidResponse,
}

impl WebhookVerificationError {
  /// Creates a provider-reported failure while retaining its safe diagnostics.
  #[must_use]
  pub fn provider(
    classification: WebhookVerificationFailure,
    code: String,
    diagnostic: String,
    retry_after_milliseconds: Option<u64>,
  ) -> Self {
    Self::ProviderFailure {
      classification,
      code,
      diagnostic,
      retry_after_milliseconds,
    }
  }

  /// Returns the stable behavior classification independent of diagnostic details.
  #[must_use]
  pub const fn classification(&self) -> WebhookVerificationFailure {
    match self {
      Self::AuthenticationFailed => WebhookVerificationFailure::AuthenticationFailed,
      Self::InvalidConfiguration => WebhookVerificationFailure::InvalidConfiguration,
      Self::Unsupported => WebhookVerificationFailure::Unsupported,
      Self::Permanent => WebhookVerificationFailure::Permanent,
      Self::Cancelled => WebhookVerificationFailure::Cancelled,
      Self::Unavailable => WebhookVerificationFailure::Unavailable,
      Self::InvalidResponse => WebhookVerificationFailure::InvalidResponse,
      Self::ProviderFailure { classification, .. } => *classification,
    }
  }

  /// Returns bounded provider retry guidance when the adapter supplied it.
  #[must_use]
  pub const fn retry_after_milliseconds(&self) -> Option<u64> {
    match self {
      Self::ProviderFailure {
        retry_after_milliseconds,
        ..
      } => *retry_after_milliseconds,
      _ => None,
    }
  }
}

/// Replaceable authentication and normalization port used before Trigger evaluation.
#[async_trait]
pub trait WebhookDeliveryVerifier: Send + Sync {
  /// Authenticates the exact body and returns one normalized event.
  async fn verify(
    &self,
    delivery: VerifyWebhookDelivery,
  ) -> Result<AuthenticatedWebhookEvent, WebhookVerificationError>;
}

/// Replaceable provider management port used by integration commands.
#[async_trait]
pub trait WebhookManagementProvider: Send + Sync {
  /// Verifies adapter installation and digest selection before configuration is committed.
  async fn validate_configuration(
    &self,
    adapter_id: &str,
    adapter_sha256: &str,
  ) -> Result<(), WebhookVerificationError>;

  /// Verifies that the selected adapter advertises one optional managed operation.
  async fn validate_managed_operation(
    &self,
    adapter_id: &str,
    adapter_sha256: &str,
    operation: ManagedWebhookOperation,
  ) -> Result<(), WebhookVerificationError>;

  /// Executes one idempotent managed-registration operation.
  async fn manage_registration(
    &self,
    request: ManagedWebhookRegistrationRequest,
  ) -> Result<ManagedWebhookRegistration, WebhookVerificationError>;
}

/// Protected provider-neutral request for one managed registration operation.
pub struct ManagedWebhookRegistrationRequest {
  /// Installed adapter identity.
  pub adapter_id: String,
  /// Immutable adapter executable digest.
  pub adapter_sha256: String,
  /// Requested provider lifecycle operation.
  pub operation: ManagedWebhookOperation,
  /// Server-owned integration identity.
  pub integration_id: IntegrationId,
  /// Stable operation replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Public callback URL.
  pub callback_url: String,
  /// Host-owned provider-administration credential handle.
  pub administration_credential_handle: String,
}

/// Validated public origin used to construct webhook callback URLs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookCallbackOrigin(String);

impl WebhookCallbackOrigin {
  /// Validates an HTTP(S) origin without path, query, fragment, or credentials.
  pub fn new(value: impl Into<String>) -> Result<Self, ApplicationError> {
    let value = value.into();
    if !valid_webhook_callback_origin(&value) {
      return Err(ApplicationError::invalid());
    }
    Ok(Self(value))
  }

  /// Constructs the callback for one server-owned integration identity.
  #[must_use]
  pub fn callback_url(&self, integration_id: IntegrationId) -> String {
    format!("{}/webhooks/v1/integrations/{integration_id}", self.0)
  }

  /// Borrows the canonical origin string.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl std::fmt::Debug for ManagedWebhookRegistrationRequest {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ManagedWebhookRegistrationRequest")
      .field("adapter_id", &self.adapter_id)
      .field("adapter_sha256", &self.adapter_sha256)
      .field("operation", &self.operation)
      .field("integration_id", &self.integration_id)
      .field("idempotency_key", &self.idempotency_key)
      .field("callback_url", &self.callback_url)
      .field("administration_credential_handle", &"<redacted>")
      .finish()
  }
}

/// Failure from authenticated public delivery processing.
#[derive(Debug, Error)]
pub enum WebhookDeliveryError {
  /// Required verification headers were absent.
  #[error("webhook delivery authentication failed")]
  AuthenticationFailed,
  /// Integration lookup failed.
  #[error("webhook integration lookup failed")]
  Store(#[from] StoreError),
}

impl WebhookDeliveryError {
  /// Returns a stable transport-neutral failure classification.
  #[must_use]
  pub const fn classification(&self) -> WebhookDeliveryFailure {
    match self {
      Self::AuthenticationFailed => WebhookDeliveryFailure::Unauthorized,
      Self::Store(StoreError::NotFound { .. }) => WebhookDeliveryFailure::NotFound,
      Self::Store(StoreError::Unavailable) => WebhookDeliveryFailure::Unavailable,
      Self::Store(_) => WebhookDeliveryFailure::Invalid,
    }
  }
}

/// Stable public webhook error classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookDeliveryFailure {
  /// Provider authentication failed without disclosing why.
  Unauthorized,
  /// Callback integration does not exist.
  NotFound,
  /// Request or authenticated normalized event is invalid.
  Invalid,
  /// A required dependency is temporarily unavailable.
  Unavailable,
}

struct WebhookDefinitionInput<'a> {
  adapter_id: &'a str,
  adapter_sha256: &'a str,
  verification_material_handle: &'a str,
  verification_headers: &'a [String],
  repository_id: RepositoryId,
  event_kind: &'a TriggerEventKind,
  parameters: &'a BTreeMap<String, Value>,
  priority: i64,
}
