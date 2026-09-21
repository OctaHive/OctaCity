use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, ProjectId, ProjectPolicyVersion, Timestamp, TriggerId,
  TriggerVersion,
};
use octacity_server_trigger::TriggerKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{IdempotencyKey, MutationDisposition, StoreError, StoreOperation, model::require_bounded_json_object};

/// Publishes the next immutable policy document for one Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishProjectPolicy {
  /// Project that owns the policy lineage.
  pub project_id: ProjectId,
  /// Current version expected by the caller, or `None` for the initial policy.
  pub expected_current_version: Option<ProjectPolicyVersion>,
  /// Application-validated policy-directive document.
  pub policy: Value,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl PublishProjectPolicy {
  /// Revalidates the generic persistence bounds at the adapter seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    require_bounded_json_object(&self.policy).map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::PublishProjectPolicy,
      source,
    })
  }
}

/// Durable result of publishing one Project policy version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectPolicyMutationOutcome {
  /// Whether the mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Project that owns the published policy.
  pub project_id: ProjectId,
  /// Exact immutable version selected by the server.
  pub version: ProjectPolicyVersion,
}

/// Creates one immutable Trigger definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateTriggerDefinition {
  /// Stable Trigger identity selected once by the caller.
  pub id: TriggerId,
  /// Initial Trigger version. Creation currently accepts version one only.
  pub version: TriggerVersion,
  /// Build Configuration selected by this Trigger.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Normalized Trigger origin.
  pub kind: TriggerKind,
  /// Whether new occurrences may be accepted.
  pub enabled: bool,
  /// Kind-specific bounded definition document.
  pub definition: Value,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

impl CreateTriggerDefinition {
  /// Revalidates invariants at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.version != TriggerVersion::INITIAL {
      return Err(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Trigger,
      });
    }
    require_bounded_json_object(&self.definition).map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::CreateTriggerDefinition,
      source,
    })
  }
}

/// Durable result of creating one Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TriggerDefinitionMutationOutcome {
  /// Whether the mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Stable Trigger identity.
  pub trigger_id: TriggerId,
  /// Exact immutable Trigger version.
  pub version: TriggerVersion,
}
