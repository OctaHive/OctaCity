use axum::{
  Json,
  http::StatusCode,
  response::{IntoResponse, Response},
};

use crate::RequestId;

use super::super::{ErrorCode, ErrorResponse};

#[derive(Debug)]
pub(super) struct ApiError {
  status: StatusCode,
  body: ErrorResponse,
}

impl ApiError {
  pub(super) fn new(status: StatusCode, code: ErrorCode, message: &str, request_id: &RequestId) -> Self {
    Self {
      status,
      body: ErrorResponse {
        code,
        message: message.to_owned(),
        request_id: request_id.0.clone(),
        diagnostics: None,
      },
    }
  }
  pub(super) fn with_factory_diagnostics(
    mut self,
    report: &octacity_server_application::FactoryValidationDiagnostics,
  ) -> Self {
    if let Ok(value) = serde_json::to_value(report) {
      self.body.diagnostics = Some(value);
    }
    self
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (self.status, Json(self.body)).into_response()
  }
}
