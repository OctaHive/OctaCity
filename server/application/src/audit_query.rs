use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{AuditFactId, Timestamp};
use octacity_server_store::{AuditActorKind, AuditCursor, AuditFactPage, AuditFactQuery, AuditFactStore, AuditOutcome};
use serde_json::Value;

use crate::{ApplicationError, Query, QueryHandler};

/// Transport-independent input for a bounded immutable audit read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFactQueryInput {
  /// Optional actor classification.
  pub actor_kind: Option<AuditActorKind>,
  /// Optional exact actor identity.
  pub actor_identity: Option<String>,
  /// Optional exact operation.
  pub operation: Option<String>,
  /// Optional exact target classification.
  pub target_kind: Option<String>,
  /// Optional exact target identity.
  pub target_identity: Option<String>,
  /// Optional exact request identity.
  pub request_identity: Option<String>,
  /// Inclusive lower commit time in Unix milliseconds.
  pub occurred_from_unix_ms: Option<i64>,
  /// Inclusive upper commit time in Unix milliseconds.
  pub occurred_through_unix_ms: Option<i64>,
  /// Optional exclusive continuation cursor.
  pub after: Option<AuditCursorInput>,
  /// Maximum number of facts returned.
  pub limit: u16,
}

/// Decoded continuation state at the application boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditCursorInput {
  /// Commit time of the preceding page's final fact.
  pub occurred_at_unix_ms: i64,
  /// Fact identity of the preceding page's final fact.
  pub id: String,
}

/// Typed application query for immutable audit facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListAuditFactsQuery {
  query: AuditFactQuery,
}

impl ListAuditFactsQuery {
  /// Parses cursor values and validates all backend-neutral bounds.
  pub fn try_from_input(input: AuditFactQueryInput) -> Result<Self, ApplicationError> {
    let query = AuditFactQuery {
      actor_kind: input.actor_kind,
      actor_identity: input.actor_identity,
      operation: input.operation,
      target_kind: input.target_kind,
      target_identity: input.target_identity,
      request_identity: input.request_identity,
      occurred_from: parse_time(input.occurred_from_unix_ms)?,
      occurred_through: parse_time(input.occurred_through_unix_ms)?,
      after: input
        .after
        .map(|cursor| {
          Ok::<_, ApplicationError>(AuditCursor {
            occurred_at: Timestamp::from_unix_millis(cursor.occurred_at_unix_ms)
              .map_err(|_| ApplicationError::invalid())?,
            id: cursor
              .id
              .parse::<AuditFactId>()
              .map_err(|_| ApplicationError::invalid())?,
          })
        })
        .transpose()?,
      limit: input.limit,
    };
    query.validate().map_err(|_| ApplicationError::invalid())?;
    Ok(Self { query })
  }
}

fn parse_time(value: Option<i64>) -> Result<Option<Timestamp>, ApplicationError> {
  value
    .map(|value| Timestamp::from_unix_millis(value).map_err(|_| ApplicationError::invalid()))
    .transpose()
}

impl Query for ListAuditFactsQuery {
  type Outcome = AuditFactPageProjection;
}

/// Honest actor data exposed by an audit read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditActorProjection {
  /// Stable actor classification.
  pub kind: AuditActorKind,
  /// Available authenticated or durable worker identity.
  pub identity: Option<String>,
}

/// Transport-independent projection of one immutable audit fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFactProjection {
  /// Stable fact identity.
  pub id: AuditFactId,
  /// Honest available actor information.
  pub actor: AuditActorProjection,
  /// Stable operation name.
  pub operation: String,
  /// Stable target classification.
  pub target_kind: String,
  /// Logical target identity.
  pub target_identity: String,
  /// Request identity when one was supplied.
  pub request_identity: Option<String>,
  /// Idempotency identity when supported by the mutation.
  pub idempotency_key: Option<String>,
  /// Stable committed outcome.
  pub outcome: AuditOutcome,
  /// Bounded explicitly selected non-sensitive metadata.
  pub metadata: Value,
  /// Authoritative commit time in Unix milliseconds.
  pub occurred_at_unix_ms: i64,
}

/// Opaque continuation state for audit pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditCursorProjection {
  /// Commit time of the page's final fact.
  pub occurred_at_unix_ms: i64,
  /// Stable fact tie-breaker.
  pub id: AuditFactId,
}

/// Deterministic bounded page of immutable audit facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFactPageProjection {
  /// Facts in stable newest-first order.
  pub items: Vec<AuditFactProjection>,
  /// Continuation state when another page exists.
  pub next_cursor: Option<AuditCursorProjection>,
}

/// Typed read-only audit query service.
pub struct AuditQueries<S> {
  store: Arc<S>,
}

impl<S> AuditQueries<S> {
  /// Creates a query service from the read-only authoritative port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> QueryHandler<ListAuditFactsQuery> for AuditQueries<S>
where
  S: AuditFactStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListAuditFactsQuery) -> Result<AuditFactPageProjection, Self::Error> {
    self
      .store
      .list_audit_facts(query.query)
      .await
      .map(project_page)
      .map_err(Into::into)
  }
}

fn project_page(page: AuditFactPage) -> AuditFactPageProjection {
  AuditFactPageProjection {
    items: page
      .items
      .into_iter()
      .map(|fact| AuditFactProjection {
        id: fact.id,
        actor: AuditActorProjection {
          kind: fact.actor.kind,
          identity: fact.actor.identity,
        },
        operation: fact.operation,
        target_kind: fact.target_kind,
        target_identity: fact.target_identity,
        request_identity: fact.request_identity,
        idempotency_key: fact.idempotency_key,
        outcome: fact.outcome,
        metadata: fact.metadata.into_value(),
        occurred_at_unix_ms: fact.occurred_at.unix_millis(),
      })
      .collect(),
    next_cursor: page.next_cursor.map(|cursor| AuditCursorProjection {
      occurred_at_unix_ms: cursor.occurred_at.unix_millis(),
      id: cursor.id,
    }),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_inverted_time_ranges_before_the_store() {
    let input = AuditFactQueryInput {
      actor_kind: None,
      actor_identity: None,
      operation: None,
      target_kind: None,
      target_identity: None,
      request_identity: None,
      occurred_from_unix_ms: Some(2),
      occurred_through_unix_ms: Some(1),
      after: None,
      limit: 1,
    };
    assert!(matches!(
      ListAuditFactsQuery::try_from_input(input),
      Err(ApplicationError::InvalidInput)
    ));
  }
}
