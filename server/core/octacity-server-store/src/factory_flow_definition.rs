use crate::{FlowDefinitionListVisibility, IdempotencyKey, ManagementMutation, MutationDisposition, StoreError};
use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryFlowConfiguration, FlowDefinitionId, FlowDefinitionVersion};
use serde::{Deserialize, Serialize};

/// Immutable published root closure, data catalogue and admission ceilings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedFactoryFlowDefinition {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact complete reachable closure and executable contracts.
  pub flow: FactoryFlowConfiguration,
  /// Original version publication time.
  pub published_at: Timestamp,
}
/// Complete publication intent, including its strong baseline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishFactoryFlowDefinition {
  /// Canonical caller-owned authoring intent, independent of later resolution.
  pub intent_digest: FactoryDigest,
  /// Immutable prospective version.
  pub publication: PublishedFactoryFlowDefinition,
  /// Absent for initial publication; otherwise must remain current.
  pub expected_current_version: Option<FlowDefinitionVersion>,
  /// Security-scoped transport replay identity.
  pub idempotency_key: IdempotencyKey,
}
/// Replay probe that requires no current graph or capability lookup.
#[derive(Clone, Debug, Serialize)]
pub struct ReplayFactoryFlowDefinition {
  /// Stable root identity.
  pub id: FlowDefinitionId,
  /// Owning Project for creation; absent for replacement.
  pub project_id: Option<ProjectId>,
  /// Current-version baseline for replacement.
  pub expected_current_version: Option<FlowDefinitionVersion>,
  /// Exact submitted draft identity.
  pub intent_digest: FactoryDigest,
  /// Security-scoped transport replay identity.
  pub idempotency_key: IdempotencyKey,
}
impl PublishFactoryFlowDefinition {
  /// Constructs the immutable pre-resolution replay probe.
  #[must_use]
  pub fn replay_probe(&self) -> ReplayFactoryFlowDefinition {
    ReplayFactoryFlowDefinition {
      id: self.publication.flow.closure.root().id(),
      project_id: self
        .expected_current_version
        .is_none()
        .then_some(self.publication.project_id),
      expected_current_version: self.expected_current_version,
      intent_digest: self.intent_digest,
      idempotency_key: self.idempotency_key.clone(),
    }
  }
}
/// Durable applied or replayed publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactoryFlowDefinitionMutationOutcome {
  /// Whether a version was newly committed.
  pub disposition: MutationDisposition,
  /// Original immutable result.
  pub publication: PublishedFactoryFlowDefinition,
}
/// Exclusive newest-first Flow identity creation position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlowDefinitionPagePosition {
  /// Identity creation time.
  pub created_at: Timestamp,
  /// Stable tie breaker.
  pub id: FlowDefinitionId,
}
/// Bounded visible current root definitions and exact subflow choices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryFlowDefinitionPage {
  /// Visible count before continuation.
  pub total: u64,
  /// Exact current definitions ordered newest first.
  pub items: Vec<PublishedFactoryFlowDefinition>,
  /// Last visible returned creation position if more remain.
  pub next_cursor: Option<FlowDefinitionPagePosition>,
}
/// Atomic Flow publication and discovery, with no logical node names.
#[async_trait]
pub trait FactoryFlowDefinitionStore: Send + Sync {
  /// Reads the original outcome before resolving mutable references.
  async fn replay_factory_flow_definition(
    &self,
    request: ManagementMutation<ReplayFactoryFlowDefinition>,
  ) -> Result<Option<FactoryFlowDefinitionMutationOutcome>, StoreError>;
  /// Appends an immutable version, replay, audit and outbox atomically.
  async fn publish_factory_flow_definition(
    &self,
    request: ManagementMutation<PublishFactoryFlowDefinition>,
  ) -> Result<FactoryFlowDefinitionMutationOutcome, crate::ReplaceFactoryConfigurationError>;
  /// Reads an exact version or the current version when absent.
  async fn factory_flow_definition(
    &self,
    id: FlowDefinitionId,
    version: Option<FlowDefinitionVersion>,
  ) -> Result<PublishedFactoryFlowDefinition, StoreError>;
  /// Applies visibility before newest-first pagination and counts.
  async fn list_factory_flow_definitions(
    &self,
    project: ProjectId,
    after: Option<FlowDefinitionPagePosition>,
    limit: u16,
    visibility: FlowDefinitionListVisibility,
  ) -> Result<FactoryFlowDefinitionPage, StoreError>;
}
