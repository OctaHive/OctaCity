use std::sync::Arc;

use axum::{
  Extension, Json,
  extract::{Query, State, rejection::QueryRejection},
};
use octacity_server_application::{
  AuditActorKind as ApplicationActorKind, AuditCursorInput, AuditFactPageProjection, AuditFactQueryInput,
  AuditOutcome as ApplicationAuditOutcome, ListAuditFactsQuery,
};
use serde::Deserialize;

use super::{ApiError, ManagementApplication, application_error, query_error};
use crate::{
  RequestId,
  v1::{AuditActor, AuditActorKind, AuditFactPage, AuditFactResource, AuditOutcome, Cursor, ErrorCode},
};

pub(crate) const DEFAULT_AUDIT_LIMIT: u16 = 50;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditFactParameters {
  actor_kind: Option<AuditActorKind>,
  actor_identity: Option<String>,
  operation: Option<String>,
  target_kind: Option<String>,
  target_identity: Option<String>,
  request_identity: Option<String>,
  occurred_from_unix_ms: Option<i64>,
  occurred_through_unix_ms: Option<i64>,
  after: Option<Cursor>,
  #[serde(default = "default_limit")]
  limit: u16,
}

const fn default_limit() -> u16 {
  DEFAULT_AUDIT_LIMIT
}

pub(super) async fn list_audit_facts(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  parameters: Result<Query<AuditFactParameters>, QueryRejection>,
) -> Result<Json<AuditFactPage>, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = ListAuditFactsQuery::try_from_input(AuditFactQueryInput {
    actor_kind: parameters.actor_kind.map(actor_kind),
    actor_identity: parameters.actor_identity,
    operation: parameters.operation,
    target_kind: parameters.target_kind,
    target_identity: parameters.target_identity,
    request_identity: parameters.request_identity,
    occurred_from_unix_ms: parameters.occurred_from_unix_ms,
    occurred_through_unix_ms: parameters.occurred_through_unix_ms,
    after: parameters
      .after
      .map(|cursor| decode_cursor(&cursor, &request_id))
      .transpose()?,
    limit: parameters.limit,
  })
  .map_err(|error| application_error(error.classification(), &request_id))?;
  let page = application
    .audit
    .0
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  project_page(page, &request_id).map(Json)
}

fn actor_kind(kind: AuditActorKind) -> ApplicationActorKind {
  match kind {
    AuditActorKind::UnauthenticatedManagement => ApplicationActorKind::UnauthenticatedManagement,
    AuditActorKind::Agent => ApplicationActorKind::Agent,
    AuditActorKind::Trigger => ApplicationActorKind::Trigger,
    AuditActorKind::Orchestrator => ApplicationActorKind::Orchestrator,
    AuditActorKind::Adapter => ApplicationActorKind::Adapter,
    AuditActorKind::Worker => ApplicationActorKind::Worker,
  }
}

fn decode_cursor(cursor: &Cursor, request_id: &RequestId) -> Result<AuditCursorInput, ApiError> {
  let (occurred_at, id) = cursor
    .as_str()
    .split_once('.')
    .ok_or_else(|| invalid_cursor(request_id))?;
  if id.is_empty() {
    return Err(invalid_cursor(request_id));
  }
  Ok(AuditCursorInput {
    occurred_at_unix_ms: occurred_at.parse().map_err(|_| invalid_cursor(request_id))?,
    id: id.to_owned(),
  })
}

fn project_page(page: AuditFactPageProjection, request_id: &RequestId) -> Result<AuditFactPage, ApiError> {
  Ok(AuditFactPage {
    items: page
      .items
      .into_iter()
      .map(|fact| AuditFactResource {
        id: fact.id.to_string(),
        actor: AuditActor {
          kind: match fact.actor.kind {
            ApplicationActorKind::UnauthenticatedManagement => AuditActorKind::UnauthenticatedManagement,
            ApplicationActorKind::Agent => AuditActorKind::Agent,
            ApplicationActorKind::Trigger => AuditActorKind::Trigger,
            ApplicationActorKind::Orchestrator => AuditActorKind::Orchestrator,
            ApplicationActorKind::Adapter => AuditActorKind::Adapter,
            ApplicationActorKind::Worker => AuditActorKind::Worker,
          },
          identity: fact.actor.identity,
        },
        operation: fact.operation,
        target_kind: fact.target_kind,
        target_identity: fact.target_identity,
        request_identity: fact.request_identity,
        idempotency_key: fact.idempotency_key,
        outcome: match fact.outcome {
          ApplicationAuditOutcome::Accepted => AuditOutcome::Accepted,
        },
        metadata: fact.metadata,
        occurred_at_unix_ms: fact.occurred_at_unix_ms,
      })
      .collect(),
    next_cursor: page
      .next_cursor
      .map(|cursor| Cursor::new(format!("{}.{}", cursor.occurred_at_unix_ms, cursor.id)))
      .transpose()
      .map_err(|_| internal_cursor(request_id))?,
  })
}

fn invalid_cursor(request_id: &RequestId) -> ApiError {
  ApiError::new(
    axum::http::StatusCode::BAD_REQUEST,
    ErrorCode::InvalidRequest,
    "audit cursor is invalid",
    request_id,
  )
}

fn internal_cursor(request_id: &RequestId) -> ApiError {
  ApiError::new(
    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    ErrorCode::Internal,
    "audit cursor could not be represented",
    request_id,
  )
}
