use std::fmt;

use octacity_server_application::{ListOperatorAttentionQuery, OperatorAttentionCursor as ApplicationCursor};
use serde::de::{Error as _, MapAccess, Visitor};

use super::*;

pub(super) struct OperatorAttentionParameters {
  build_ids: Vec<String>,
  agent_ids: Vec<String>,
  pool_ids: Vec<String>,
  include_critical_conditions: bool,
  occurred_from_unix_ms: Option<i64>,
  occurred_through_unix_ms: Option<i64>,
  after: Option<OperatorAttentionCursor>,
  limit: u16,
}

impl<'de> Deserialize<'de> for OperatorAttentionParameters {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserializer.deserialize_map(OperatorAttentionParametersVisitor)
  }
}

struct OperatorAttentionParametersVisitor;

impl<'de> Visitor<'de> for OperatorAttentionParametersVisitor {
  type Value = OperatorAttentionParameters;

  fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("bounded operator-attention query parameters")
  }

  fn visit_map<M>(self, mut parameters: M) -> Result<Self::Value, M::Error>
  where
    M: MapAccess<'de>,
  {
    let mut build_ids = Vec::new();
    let mut agent_ids = Vec::new();
    let mut pool_ids = Vec::new();
    let mut include_critical_conditions = None;
    let mut occurred_from_unix_ms = None;
    let mut occurred_through_unix_ms = None;
    let mut after = None;
    let mut limit = None;
    while let Some(name) = parameters.next_key::<String>()? {
      match name.as_str() {
        "build_ids" => append_id_filter(
          &mut build_ids,
          parameters.next_value()?,
          agent_ids.len().saturating_add(pool_ids.len()),
        )?,
        "agent_ids" => append_id_filter(
          &mut agent_ids,
          parameters.next_value()?,
          build_ids.len().saturating_add(pool_ids.len()),
        )?,
        "pool_ids" => append_id_filter(
          &mut pool_ids,
          parameters.next_value()?,
          build_ids.len().saturating_add(agent_ids.len()),
        )?,
        "include_critical_conditions" => set_once(
          &mut include_critical_conditions,
          parameters.next_value()?,
          "include_critical_conditions",
        )?,
        "occurred_from_unix_ms" => set_once(
          &mut occurred_from_unix_ms,
          parameters.next_value()?,
          "occurred_from_unix_ms",
        )?,
        "occurred_through_unix_ms" => set_once(
          &mut occurred_through_unix_ms,
          parameters.next_value()?,
          "occurred_through_unix_ms",
        )?,
        "after" => set_once(&mut after, parameters.next_value()?, "after")?,
        "limit" => set_once(&mut limit, parameters.next_value()?, "limit")?,
        _ => {
          return Err(M::Error::unknown_field(
            &name,
            &[
              "build_ids",
              "agent_ids",
              "pool_ids",
              "include_critical_conditions",
              "occurred_from_unix_ms",
              "occurred_through_unix_ms",
              "after",
              "limit",
            ],
          ));
        }
      }
    }
    Ok(OperatorAttentionParameters {
      build_ids,
      agent_ids,
      pool_ids,
      include_critical_conditions: include_critical_conditions.unwrap_or(false),
      occurred_from_unix_ms,
      occurred_through_unix_ms,
      after,
      limit: limit.unwrap_or(DEFAULT_PAGE_LIMIT),
    })
  }
}

fn append_id_filter<E>(destination: &mut Vec<String>, value: String, other_count: usize) -> Result<(), E>
where
  E: serde::de::Error,
{
  for identity in value.split(',') {
    if identity.len() != 36
      || other_count.saturating_add(destination.len()).saturating_add(1)
        > octacity_server_application::MAX_OPERATOR_ATTENTION_SCOPE_TARGETS
    {
      return Err(E::custom("operator attention scope is invalid"));
    }
    destination.push(identity.to_owned());
  }
  Ok(())
}

pub(super) async fn list_operator_attention(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  parameters: Result<Query<OperatorAttentionParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let scope = application
    .inputs
    .operator_attention_scope(
      &parameters.build_ids,
      &parameters.agent_ids,
      &parameters.pool_ids,
      parameters.include_critical_conditions,
      parameters.occurred_from_unix_ms,
      parameters.occurred_through_unix_ms,
    )
    .map_err(|error| invalid_operator_attention(&error.to_string(), &request_id))?;
  let after = parameters
    .after
    .map(|cursor| ApplicationCursor::decode(cursor.as_str()))
    .transpose()
    .map_err(|error| invalid_operator_attention(&error.to_string(), &request_id))?;
  let query = ListOperatorAttentionQuery::try_new(scope, after, parameters.limit)
    .map_err(|error| invalid_operator_attention(&error.to_string(), &request_id))?;
  let page = application
    .operator_attention
    .0
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(operator_attention_page(page, &request_id)?)))
}

fn invalid_operator_attention(message: &str, request_id: &RequestId) -> ApiError {
  ApiError::new(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, message, request_id)
}
