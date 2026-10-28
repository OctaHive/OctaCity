//! JSON-response Streamable HTTP, qualified for MCP 2025-11-25.
use super::{IssueMcpApplication, error, pending, tool_failure, tools};
use axum::{
  Json,
  extract::{Request, State},
  http::{Method, StatusCode},
  response::{IntoResponse, Response},
};
use octacity_server_application::{
  ApplicationError, GetIssueQuery, ListProjectIssuesQuery, ManagementQueryUseCase, ProviderCredentialValue,
};
use serde_json::{Value, json};
use std::sync::Arc;
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const REQUEST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

pub(super) async fn dispatch<
  H: ManagementQueryUseCase<ListProjectIssuesQuery, Error = ApplicationError>
    + ManagementQueryUseCase<GetIssueQuery, Error = ApplicationError>
    + 'static,
>(
  State(application): State<Arc<IssueMcpApplication<H>>>,
  request: Request,
) -> Response {
  let mut response = tokio::time::timeout(REQUEST_DEADLINE, dispatch_bounded(application, request))
    .await
    .unwrap_or_else(|_| StatusCode::GATEWAY_TIMEOUT.into_response());
  response
    .headers_mut()
    .insert("cache-control", axum::http::HeaderValue::from_static("no-store"));
  response
}

async fn dispatch_bounded<
  H: ManagementQueryUseCase<ListProjectIssuesQuery, Error = ApplicationError>
    + ManagementQueryUseCase<GetIssueQuery, Error = ApplicationError>
    + 'static,
>(
  application: Arc<IssueMcpApplication<H>>,
  request: Request,
) -> Response {
  // Native server-to-server clients send no browser Origin. No browser origin is installed.
  if request.headers().contains_key("origin") {
    return StatusCode::FORBIDDEN.into_response();
  }
  let Some(bearer) = request
    .headers()
    .get("authorization")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
  else {
    return StatusCode::UNAUTHORIZED.into_response();
  };
  let Ok(credential) = ProviderCredentialValue::new(bearer.as_bytes().to_vec()) else {
    return StatusCode::UNAUTHORIZED.into_response();
  };
  let principal = match application.authenticate(&credential).await {
    Ok(Some(principal)) => principal,
    Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
    Err(status) => return status.into_response(),
  };
  if request.method() != Method::DELETE
    && !request
      .headers()
      .get("content-type")
      .and_then(|value| value.to_str().ok())
      .is_some_and(|value| {
        value
          .split(';')
          .next()
          .unwrap_or_default()
          .trim()
          .eq_ignore_ascii_case("application/json")
      })
  {
    return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
  }
  if request
    .headers()
    .get("mcp-protocol-version")
    .is_some_and(|value| value != "2025-11-25")
  {
    return StatusCode::BAD_REQUEST.into_response();
  }
  let session = match request.headers().get("mcp-session-id") {
    Some(value) => match value.to_str() {
      Ok(value) if application.pending.accepts_session(principal.connection_scope(), value) => value.to_owned(),
      _ => return StatusCode::NOT_FOUND.into_response(),
    },
    None => String::new(), // A sessionless caller shares its verified connection's request namespace.
  };
  if request.method() == Method::DELETE {
    return if session.is_empty() {
      StatusCode::BAD_REQUEST.into_response()
    } else if application.pending.terminate(principal.connection_scope(), &session) {
      StatusCode::NO_CONTENT.into_response()
    } else {
      StatusCode::NOT_FOUND.into_response()
    };
  }
  let Ok(bytes) = axum::body::to_bytes(request.into_body(), MAX_REQUEST_BYTES).await else {
    return StatusCode::PAYLOAD_TOO_LARGE.into_response();
  };
  let Ok(message) = serde_json::from_slice::<Value>(&bytes) else {
    return error(Value::Null, -32700, "Invalid JSON");
  };
  let id = message.get("id").cloned().unwrap_or(Value::Null);
  let notification = message.get("id").is_none()
    && message["method"]
      .as_str()
      .is_some_and(|method| method.starts_with("notifications/"));
  let valid_id = match &id {
    Value::String(value) => !value.is_empty() && value.len() <= 256,
    Value::Number(value) => value.is_i64() || value.is_u64(),
    _ => false,
  };
  if message["jsonrpc"] != "2.0"
    || (!notification && !valid_id)
    || message["method"].as_str().is_none_or(|method| method.len() > 128)
    || (message.get("params").is_some() && !message["params"].is_object())
  {
    return error(if valid_id { id } else { Value::Null }, -32600, "Invalid request");
  }
  if message["method"] == "notifications/cancelled" {
    application
      .pending
      .cancel(principal.connection_scope(), &session, &message["params"]["requestId"]);
    return StatusCode::ACCEPTED.into_response();
  }
  if notification {
    return StatusCode::ACCEPTED.into_response();
  }
  let registration = if message["method"] == "tools/call" {
    match application
      .pending
      .register(principal.connection_scope(), &session, &id)
    {
      Ok(registration) => Some(registration),
      Err(pending::Refusal::Duplicate) => return error(id, -32600, "Request already active"),
      Err(pending::Refusal::Capacity) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
      Err(pending::Refusal::SessionUnavailable) => return StatusCode::NOT_FOUND.into_response(),
    }
  } else {
    None
  };
  let cancel_id = id.clone();
  let operation = async {
    match message["method"].as_str() {
      Some("ping") => Json(json!({"jsonrpc":"2.0","id":id,"result":{}})).into_response(),
      Some("tools/call") => tools::call(&application, &credential, &principal, id, &message["params"]).await,
      Some("tools/list") => {
        Json(json!({"jsonrpc":"2.0", "id":id, "result":{"tools":tools::catalog()}})).into_response()
      }
      Some("initialize") if message["params"]["protocolVersion"].is_string() => {
        let Some(session) = application.pending.initialize(principal.connection_scope()) else {
          return StatusCode::TOO_MANY_REQUESTS.into_response();
        };
        let mut response = Json(json!({
          "jsonrpc":"2.0", "id":id, "result":{
            "protocolVersion":"2025-11-25", "capabilities":{"tools":{}},
            "serverInfo":{"name":"octacity","version":env!("CARGO_PKG_VERSION")}
          }
        }))
        .into_response();
        response
          .headers_mut()
          .insert("mcp-session-id", session.parse().expect("UUID header"));
        response
      }
      Some("initialize") => error(id, -32602, "Invalid initialization parameters"),
      _ => error(id, -32601, "Method unavailable"),
    }
  };
  match registration {
    Some((_registration, cancelled)) => tokio::select! {
      biased;
      _ = cancelled => tool_failure(cancel_id, "cancelled"),
      response = operation => response,
    },
    None => operation.await,
  }
}
