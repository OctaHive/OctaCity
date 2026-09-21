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
      },
    }
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (self.status, Json(self.body)).into_response()
  }
}
