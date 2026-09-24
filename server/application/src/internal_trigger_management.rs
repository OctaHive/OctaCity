use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{RepositoryId, Timestamp, TriggerId, TriggerVersion};
use octacity_server_store::{
  ConfigurationStore, CreateInternalTriggerDefinition, CreateTriggerDefinition, IdempotencyKey,
  InternalTriggerDefinitionRecord, InternalTriggerDefinitionStore, ListInternalTriggerDefinitions,
  PublishInternalTriggerVersion, RepositorySelectionPolicy, TerminalBuildEvent, TriggerDefinitionRef, TriggerKind,
  TriggerTarget,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
  ApplicationError, Command, CommandHandler, ManualSourceSelection, MutationDisposition, ProjectionError, Query,
  QueryHandler,
};

/// Explicit source strategy for a downstream Build created by an internal Trigger.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum InternalTriggerSourceStrategy {
  /// Reuse the exact immutable revision built by the upstream Build.
  InheritRevision,
  /// Resolve source independently using the downstream Repository policy.
  ResolveTarget(ManualSourceSelection),
}

/// Strict immutable definition matched against one exact upstream Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InternalTriggerDefinition {
  /// Exact upstream Build Configuration version.
  pub upstream: TriggerTarget,
  /// Terminal upstream Build outcome.
  pub event_kind: TerminalBuildEvent,
  /// Explicit downstream source-selection behavior.
  pub source: InternalTriggerSourceStrategy,
  /// Parameter values resolved against the downstream Build Configuration.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority copied to root Jobs.
  pub priority: i64,
}

/// Creates version one of a source-scoped internal Trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateInternalTriggerCommand {
  /// Stable Trigger identity selected once by the transport.
  pub id: TriggerId,
  /// Exact immutable downstream Build Configuration.
  pub target: TriggerTarget,
  /// Complete strict source-scoped definition.
  pub definition: InternalTriggerDefinition,
  /// Whether matching events may create occurrences.
  pub enabled: bool,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

/// Publishes the next immutable version of a source-scoped internal Trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishInternalTriggerVersionCommand {
  /// Stable Trigger identity.
  pub id: TriggerId,
  /// Version that must still be current.
  pub expected_current_version: TriggerVersion,
  /// Exact immutable downstream Build Configuration.
  pub target: TriggerTarget,
  /// Complete strict source-scoped definition.
  pub definition: InternalTriggerDefinition,
  /// Whether matching events may create occurrences.
  pub enabled: bool,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Reads one exact immutable internal Trigger version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetInternalTriggerQuery {
  /// Stable Trigger identity.
  pub trigger_id: TriggerId,
  /// Exact immutable version.
  pub version: TriggerVersion,
}

/// Lists current internal Trigger versions in stable identity order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListInternalTriggersQuery {
  /// Exclusive stable-identity cursor.
  pub after: Option<TriggerId>,
  /// Positive bounded page size.
  pub limit: u16,
}

/// Safe application projection of one internal Trigger definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerProjection {
  /// Exact immutable Trigger identity and version.
  pub trigger: TriggerDefinitionRef,
  /// Exact downstream Build Configuration.
  pub target: TriggerTarget,
  /// Whether new matching events may create occurrences.
  pub enabled: bool,
  /// Strict source-scoped definition.
  pub definition: InternalTriggerDefinition,
  /// Authoritative publication time.
  pub created_at: Timestamp,
}

/// Result of creating or publishing an internal Trigger version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerCommandOutcome {
  /// Whether the mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Complete immutable definition returned by the mutation.
  pub trigger: InternalTriggerProjection,
}

/// One deterministic page of current internal Trigger versions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerPageProjection {
  /// Current definitions ordered by stable Trigger identity.
  pub items: Vec<InternalTriggerProjection>,
  /// Exclusive cursor for the following page.
  pub next_after: Option<TriggerId>,
}

impl Command for CreateInternalTriggerCommand {
  type Outcome = InternalTriggerCommandOutcome;
}

impl Command for PublishInternalTriggerVersionCommand {
  type Outcome = InternalTriggerCommandOutcome;
}

impl Query for GetInternalTriggerQuery {
  type Outcome = InternalTriggerProjection;
}

impl Query for ListInternalTriggersQuery {
  type Outcome = InternalTriggerPageProjection;
}

/// Typed internal Trigger management handlers backed by narrow store ports.
pub struct InternalTriggerHandlers<S> {
  store: Arc<S>,
}

impl<S> InternalTriggerHandlers<S> {
  /// Creates handlers from durable definition and configuration stores.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandHandler<CreateInternalTriggerCommand> for InternalTriggerHandlers<S>
where
  S: InternalTriggerDefinitionStore + ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateInternalTriggerCommand,
  ) -> Result<InternalTriggerCommandOutcome, Self::Error> {
    let definition = command.definition.clone();
    validate_definition(self.store.as_ref(), command.target, &definition).await?;
    let encoded = serde_json::to_value(&definition).map_err(|_| ApplicationError::invalid())?;
    let outcome = self
      .store
      .create_internal_trigger_definition(CreateInternalTriggerDefinition {
        trigger: CreateTriggerDefinition {
          id: command.id,
          version: TriggerVersion::INITIAL,
          configuration_id: command.target.configuration_id,
          configuration_version: command.target.configuration_version,
          kind: TriggerKind::Internal,
          enabled: command.enabled,
          definition: encoded,
          idempotency_key: command.idempotency_key,
          created_at: command.created_at,
        },
      })
      .await?;
    let trigger = project(
      self
        .store
        .internal_trigger_definition(outcome.trigger_id, outcome.version)
        .await?,
    )?;
    Ok(InternalTriggerCommandOutcome {
      disposition: outcome.disposition.into(),
      trigger,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<PublishInternalTriggerVersionCommand> for InternalTriggerHandlers<S>
where
  S: InternalTriggerDefinitionStore + ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PublishInternalTriggerVersionCommand,
  ) -> Result<InternalTriggerCommandOutcome, Self::Error> {
    let definition = command.definition.clone();
    validate_definition(self.store.as_ref(), command.target, &definition).await?;
    let encoded = serde_json::to_value(&definition).map_err(|_| ApplicationError::invalid())?;
    let outcome = self
      .store
      .publish_internal_trigger_version(PublishInternalTriggerVersion {
        id: command.id,
        expected_current_version: command.expected_current_version,
        target: command.target,
        enabled: command.enabled,
        definition: encoded,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    let trigger = project(
      self
        .store
        .internal_trigger_definition(outcome.trigger_id, outcome.version)
        .await?,
    )?;
    Ok(InternalTriggerCommandOutcome {
      disposition: outcome.disposition.into(),
      trigger,
    })
  }
}

#[async_trait]
impl<S> QueryHandler<GetInternalTriggerQuery> for InternalTriggerHandlers<S>
where
  S: InternalTriggerDefinitionStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetInternalTriggerQuery) -> Result<InternalTriggerProjection, Self::Error> {
    project(
      self
        .store
        .internal_trigger_definition(query.trigger_id, query.version)
        .await?,
    )
  }
}

#[async_trait]
impl<S> QueryHandler<ListInternalTriggersQuery> for InternalTriggerHandlers<S>
where
  S: InternalTriggerDefinitionStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListInternalTriggersQuery) -> Result<InternalTriggerPageProjection, Self::Error> {
    let page = self
      .store
      .list_internal_trigger_definitions(ListInternalTriggerDefinitions::new(query.after, query.limit)?)
      .await?;
    Ok(InternalTriggerPageProjection {
      items: page.items.into_iter().map(project).collect::<Result<_, _>>()?,
      next_after: page.next_after,
    })
  }
}

async fn validate_definition<S>(
  store: &S,
  target: TriggerTarget,
  definition: &InternalTriggerDefinition,
) -> Result<(), ApplicationError>
where
  S: ConfigurationStore,
{
  if !definition_shape_is_valid(target, definition) {
    return Err(ApplicationError::invalid());
  }
  let upstream = store
    .build_configuration_version(
      definition.upstream.configuration_id,
      definition.upstream.configuration_version,
    )
    .await?;
  let downstream = store
    .build_configuration_version(target.configuration_id, target.configuration_version)
    .await?;
  if !downstream.definition.triggers.allowed.contains(&TriggerKind::Internal)
    || downstream
      .definition
      .parameters
      .resolve(&definition.parameters)
      .is_err()
  {
    return Err(ApplicationError::invalid());
  }
  let repository = store
    .repository_version(
      downstream.definition.repository_id,
      downstream.definition.repository_version,
    )
    .await?;
  let source_is_valid = source_strategy_is_valid(
    upstream.definition.repository_id,
    downstream.definition.repository_id,
    &repository.definition.selection,
    &definition.source,
  );
  if source_is_valid {
    Ok(())
  } else {
    Err(ApplicationError::invalid())
  }
}

fn definition_shape_is_valid(target: TriggerTarget, definition: &InternalTriggerDefinition) -> bool {
  definition.upstream != target
}

fn source_strategy_is_valid(
  upstream_repository_id: RepositoryId,
  downstream_repository_id: RepositoryId,
  downstream_policy: &RepositorySelectionPolicy,
  strategy: &InternalTriggerSourceStrategy,
) -> bool {
  match strategy {
    InternalTriggerSourceStrategy::InheritRevision => {
      upstream_repository_id == downstream_repository_id && downstream_policy.allow_exact_revision
    }
    InternalTriggerSourceStrategy::ResolveTarget(source) => match source {
      ManualSourceSelection::DefaultReference => downstream_policy.default_reference.is_some(),
      ManualSourceSelection::Reference(reference) => downstream_policy.allowed_references.contains(reference),
      ManualSourceSelection::ExactRevision(_) => downstream_policy.allow_exact_revision,
    },
  }
}

fn project(record: InternalTriggerDefinitionRecord) -> Result<InternalTriggerProjection, ApplicationError> {
  let definition = serde_json::from_value(record.definition)
    .map_err(|_| ApplicationError::Projection(ProjectionError::InvalidInternalTriggerSnapshot))?;
  Ok(InternalTriggerProjection {
    trigger: record.trigger,
    target: record.target,
    enabled: record.enabled,
    definition,
    created_at: record.created_at,
  })
}

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, BTreeSet};

  use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, SourceReference};

  use super::*;

  #[test]
  fn rejects_direct_self_dependency() {
    let downstream = target(1);
    let self_dependency = definition(
      downstream,
      TerminalBuildEvent::Succeeded,
      InternalTriggerSourceStrategy::InheritRevision,
    );
    assert!(!definition_shape_is_valid(downstream, &self_dependency));
  }

  #[test]
  fn revision_inheritance_requires_the_same_repository_and_exact_revision_policy() {
    let policy = repository_policy(true);
    assert!(!source_strategy_is_valid(
      id(1),
      id(2),
      &policy,
      &InternalTriggerSourceStrategy::InheritRevision,
    ));
    assert!(!source_strategy_is_valid(
      id(1),
      id(1),
      &repository_policy(false),
      &InternalTriggerSourceStrategy::InheritRevision,
    ));
    assert!(source_strategy_is_valid(
      id(1),
      id(1),
      &policy,
      &InternalTriggerSourceStrategy::InheritRevision,
    ));
  }

  #[test]
  fn independent_resolution_obeys_the_downstream_repository_policy() {
    let policy = repository_policy(false);
    assert!(source_strategy_is_valid(
      id(1),
      id(2),
      &policy,
      &InternalTriggerSourceStrategy::ResolveTarget(ManualSourceSelection::DefaultReference),
    ));
    assert!(!source_strategy_is_valid(
      id(1),
      id(2),
      &policy,
      &InternalTriggerSourceStrategy::ResolveTarget(ManualSourceSelection::ExactRevision(
        ImmutableRevision::new("abc").unwrap(),
      )),
    ));
  }

  fn definition(
    upstream: TriggerTarget,
    event_kind: TerminalBuildEvent,
    source: InternalTriggerSourceStrategy,
  ) -> InternalTriggerDefinition {
    InternalTriggerDefinition {
      upstream,
      event_kind,
      source,
      parameters: BTreeMap::new(),
      priority: 0,
    }
  }

  fn repository_policy(allow_exact_revision: bool) -> RepositorySelectionPolicy {
    let main = SourceReference::new("refs/heads/main").unwrap();
    RepositorySelectionPolicy {
      allowed_references: BTreeSet::from([main.clone()]),
      default_reference: Some(main),
      allow_exact_revision,
    }
  }

  fn target(value: u128) -> TriggerTarget {
    TriggerTarget {
      configuration_id: BuildConfigurationId::from_uuid(uuid::Uuid::from_u128(value)).unwrap(),
      configuration_version: BuildConfigurationVersion::INITIAL,
    }
  }

  fn id(value: u128) -> RepositoryId {
    RepositoryId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
  }
}
