use std::{collections::BTreeMap, str::FromStr, time::Duration};

use octacity_protocol::AgentCredentialToken;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, EnrollmentCredentialId, Timestamp, TriggerId, TriggerIdentity,
  TriggerVersion,
};
use octacity_server_pipeline::{CapabilityCatalog, ExecutionCapability, PipelineDag};
use octacity_server_secrets::AgentEnrollmentSecretKey;
use octacity_server_store::{
  AgentPlatform, ExpectedAgentPlatform, ManagedWebhookOperation, TriggerDefinitionRef, TriggerTarget,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
  AcceptManualTriggerCommand, AgentDrainMode, CancelBuildCommand, CreateAgentPoolCommand,
  CreateBuildConfigurationCommand, CreateInternalTriggerCommand, CreateManagedWebhookCommand, CreatePipelineCommand,
  CreateProjectCommand, CreateRepositoryCommand, CreateScheduleCommand, CreateTriggerDefinitionCommand,
  CreateUnmanagedWebhookCommand, DeleteAgentPoolCommand, DeleteProjectCommand, DrainAgentCommand, GetAgentPoolQuery,
  GetAgentQuery, GetAttemptQuery, GetBuildConfigurationQuery, GetBuildQuery, GetInternalTriggerQuery, GetJobQuery,
  GetPipelineQuery, GetProjectQuery, GetRepositoryQuery, GetScheduleQuery, InternalTriggerDefinition,
  InternalTriggerSourceStrategy, IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery,
  ListInternalTriggersQuery, ListProjectsQuery, MAX_JOB_EVENT_WAIT, ManageWebhookRegistrationCommand,
  ManualSourceSelection, ManualTriggerCommand, MoveProjectCommand, ProjectPolicyDefinition,
  PublishAgentPoolVersionCommand, PublishBuildConfigurationVersionCommand, PublishInternalTriggerVersionCommand,
  PublishPipelineVersionCommand, PublishProjectPolicyCommand, PublishRepositoryVersionCommand, ReadJobEventsQuery,
  ReassignAgentPoolCommand, RenameProjectCommand, RetryBuildCommand, ScheduledBuildDefinition,
};

mod agent;
mod configuration;
mod enrollment;
mod execution;
mod project;
mod trigger;

const AGENT_ENROLLMENT_NAMESPACE: Uuid = Uuid::from_u128(0x4ad5_41e3_c0f0_5ea2_87cb_1a2f_d458_9107);

/// Builds typed application commands and queries from transport-neutral wire primitives.
///
/// This keeps HTTP adapters from depending directly on server core crates while preserving
/// validation at the application boundary.
#[derive(Clone)]
pub struct ManagementInputFactory {
  pipeline_capabilities: CapabilityCatalog,
  agent_enrollment_lifetime: Duration,
  agent_enrollment_secret_key: AgentEnrollmentSecretKey,
}

/// Transport-neutral primitive input for one manual Trigger command.
pub struct ManualTriggerInput {
  /// Exact Trigger definition identity.
  pub trigger_id: String,
  /// Exact Trigger definition version.
  pub trigger_version: u64,
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact target Build Configuration version.
  pub configuration_version: u64,
  /// Stable source-scoped occurrence identity.
  pub deduplication_identity: String,
  /// Tagged manual source-selection document.
  pub source: Value,
  /// Caller-supplied parameter values.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
}

/// Transport-neutral primitive input for one manual Trigger definition.
pub struct ManualTriggerDefinitionInput {
  /// Stable identity selected once by the transport adapter.
  pub id: Uuid,
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether new occurrences may be accepted.
  pub enabled: bool,
  /// Kind-specific bounded definition document.
  pub definition: Value,
}

/// Transport-neutral primitive input for one scheduled Trigger definition.
pub struct ScheduledTriggerDefinitionInput {
  /// Stable identity selected once by the transport adapter.
  pub id: Uuid,
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether workers may evaluate occurrences.
  pub enabled: bool,
  /// Strict schedule definition document.
  pub schedule: Value,
  /// Strict per-occurrence Build input document.
  pub build: Value,
}

/// Transport-neutral primitive input for one internal Trigger definition version.
pub struct InternalTriggerDefinitionInput {
  /// Exact upstream Build Configuration identity.
  pub upstream_configuration_id: String,
  /// Exact upstream Build Configuration version.
  pub upstream_configuration_version: u64,
  /// Exact downstream Build Configuration identity.
  pub configuration_id: String,
  /// Exact downstream Build Configuration version.
  pub configuration_version: u64,
  /// Terminal Build event kind.
  pub event_kind: String,
  /// Tagged downstream source strategy.
  pub source: Value,
  /// Downstream Build parameters.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
  /// Whether matching source events may create occurrences.
  pub enabled: bool,
}

/// Transport-neutral primitive input for an unmanaged webhook integration.
pub struct UnmanagedWebhookInput {
  /// Stable integration identity selected once by the transport adapter.
  pub integration_id: Uuid,
  /// Stable external Trigger identity selected once by the transport adapter.
  pub trigger_id: Uuid,
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether deliveries may be accepted.
  pub enabled: bool,
  /// Installed provider adapter identity.
  pub adapter_id: String,
  /// Immutable executable digest.
  pub adapter_sha256: String,
  /// Logical protected verification-material handle.
  pub verification_material_handle: String,
  /// Sorted lowercase provider headers needed by verification.
  pub verification_headers: Vec<String>,
  /// Repository identity normalized deliveries must name.
  pub repository_id: String,
  /// Provider-neutral event classification to match.
  pub event_kind: String,
  /// Build parameters supplied to matching deliveries.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
}

/// Transport-neutral primitive input for a provider-managed webhook integration.
pub struct ManagedWebhookInput {
  /// Common delivery authentication and Trigger configuration.
  pub webhook: UnmanagedWebhookInput,
  /// Protected provider-administration credential handle.
  pub administration_credential_handle: String,
}

impl ManagementInputFactory {
  /// Validates the policy inputs needed by management adapters without requiring credential material.
  pub fn validate_policy<I, S>(
    pipeline_capabilities: I,
    agent_enrollment_lifetime: Duration,
  ) -> Result<(), ManagementInputError>
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    Self::parse_policy(pipeline_capabilities, agent_enrollment_lifetime).map(|_| ())
  }

  /// Creates a factory using the execution capabilities accepted for new Pipeline versions.
  pub fn new(
    pipeline_capabilities: impl IntoIterator<Item = String>,
    agent_enrollment_lifetime: Duration,
    agent_enrollment_secret_key: AgentEnrollmentSecretKey,
  ) -> Result<Self, ManagementInputError> {
    let (pipeline_capabilities, agent_enrollment_lifetime) =
      Self::parse_policy(pipeline_capabilities, agent_enrollment_lifetime)?;
    Ok(Self {
      pipeline_capabilities,
      agent_enrollment_lifetime,
      agent_enrollment_secret_key,
    })
  }

  fn parse_policy<I, S>(
    pipeline_capabilities: I,
    agent_enrollment_lifetime: Duration,
  ) -> Result<(CapabilityCatalog, Duration), ManagementInputError>
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    let pipeline_capabilities = pipeline_capabilities
      .into_iter()
      .map(|value| parse::<ExecutionCapability>(value.as_ref(), "pipeline capability"))
      .collect::<Result<Vec<_>, _>>()?;
    if agent_enrollment_lifetime.is_zero() || agent_enrollment_lifetime.as_millis() > i64::MAX as u128 {
      return Err(ManagementInputError::Invalid("agent enrollment lifetime"));
    }
    Ok((CapabilityCatalog::new(pipeline_capabilities), agent_enrollment_lifetime))
  }
}

/// Stable application-boundary classification for invalid command primitives.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ManagementInputError {
  /// One named input could not be parsed or validated.
  #[error("invalid {0}")]
  Invalid(&'static str),
}

fn parse<T>(value: &str, field: &'static str) -> Result<T, ManagementInputError>
where
  T: FromStr,
{
  value.parse().map_err(|_| ManagementInputError::Invalid(field))
}

fn optional_parse<T>(value: Option<&str>, field: &'static str) -> Result<Option<T>, ManagementInputError>
where
  T: FromStr,
{
  value.map(|value| parse(value, field)).transpose()
}

fn identifier<T>(value: Uuid, field: &'static str) -> Result<T, ManagementInputError>
where
  T: FromStr,
{
  parse(&value.hyphenated().to_string(), field)
}

fn version<T>(value: u64, field: &'static str) -> Result<T, ManagementInputError>
where
  T: DeserializeOwned,
{
  decode(Value::from(value), field)
}

fn timestamp(value: i64) -> Result<Timestamp, ManagementInputError> {
  Timestamp::from_unix_millis(value).map_err(|_| ManagementInputError::Invalid("timestamp"))
}

fn decode<T>(value: Value, field: &'static str) -> Result<T, ManagementInputError>
where
  T: DeserializeOwned,
{
  serde_json::from_value(value).map_err(|_| ManagementInputError::Invalid(field))
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn rejects_pipeline_capabilities_outside_the_configured_catalog() {
    let factory = ManagementInputFactory::new(
      ["native".to_owned()],
      Duration::from_secs(900),
      AgentEnrollmentSecretKey::new([7; 32]),
    )
    .unwrap();
    let dag = json!({
      "schema_version": 1,
      "nodes": [{
        "id": "build",
        "name": "Build",
        "dependency_policy": "all_succeeded",
        "required_capabilities": ["oci"],
        "template": {"commands": ["build"]}
      }],
      "edges": []
    });

    assert_eq!(
      factory.create_pipeline(
        Uuid::from_u128(1),
        "00000000-0000-0000-0000-000000000002",
        "Build".to_owned(),
        dag,
        "create-pipeline",
        1,
      ),
      Err(ManagementInputError::Invalid("pipeline DAG"))
    );
  }
}
