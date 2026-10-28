//! Read-only Issue MCP protocol over independently authenticated application reads.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use axum::{
  Json, Router,
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::post,
};
use octacity_server_application::{
  ApplicationError, FactoryExecutionClock, GetIssueQuery, IssueReadPrincipal, ListProjectIssuesQuery,
  ManagementQueryUseCase, ProviderCredentialValue, SystemFactoryExecutionClock, Timestamp,
};
use serde_json::{Value, json};
use std::sync::Arc;

mod pending;
mod protocol;
mod tools;

/// Authentication supplied by the existing deployment/secret owner, before application reads.
#[async_trait::async_trait]
pub trait IssueMcpAuthority: Send + Sync {
  /// Verifies current independent connection credentials, expiry and read scope.
  async fn authenticate(&self, credential: &ProviderCredentialValue, now: Timestamp) -> Option<IssueReadPrincipal>;
}

/// Explicit composition of scoped identity and existing Issue query handlers.
pub struct IssueMcpApplication<H> {
  authority: Arc<dyn IssueMcpAuthority>,
  queries: Arc<H>,
  pending: pending::PendingReads,
}
impl<H> IssueMcpApplication<H> {
  /// Installs no ambient management identity or credentials.
  #[must_use]
  pub fn new(authority: Arc<dyn IssueMcpAuthority>, queries: Arc<H>) -> Self {
    Self {
      authority,
      queries,
      pending: Default::default(),
    }
  }

  async fn authenticate(&self, credential: &ProviderCredentialValue) -> Result<Option<IssueReadPrincipal>, StatusCode> {
    let before = SystemFactoryExecutionClock
      .now()
      .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let principal = self.authority.authenticate(credential, before).await;
    let after = SystemFactoryExecutionClock
      .now()
      .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(principal.filter(|principal| principal.is_current_at(after)))
  }

  async fn recheck(
    &self,
    credential: &ProviderCredentialValue,
    original: &IssueReadPrincipal,
  ) -> Result<(), StatusCode> {
    match self.authenticate(credential).await {
      Ok(Some(current)) if &current == original => Ok(()),
      Ok(_) => Err(StatusCode::UNAUTHORIZED),
      Err(status) => Err(status),
    }
  }
}

/// Builds the independently authenticated MCP endpoint; ordinary management is separate.
pub fn issue_mcp_router<
  H: ManagementQueryUseCase<ListProjectIssuesQuery, Error = ApplicationError>
    + ManagementQueryUseCase<GetIssueQuery, Error = ApplicationError>
    + 'static,
>(
  application: IssueMcpApplication<H>,
) -> Router {
  Router::new()
    .route(
      "/mcp/issues",
      post(protocol::dispatch::<H>).delete(protocol::dispatch::<H>),
    )
    .with_state(Arc::new(application))
}

fn error(id: Value, code: i64, message: &str) -> Response {
  Json(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})).into_response()
}

fn tool_failure(id: Value, status: &str) -> Response {
  Json(json!({"jsonrpc":"2.0", "id":id, "result":{
    "isError":true, "structuredContent":{"status":status},
    "content":[{"type":"text","text":status}]
  }}))
  .into_response()
}
