use axum::http::StatusCode;
use octacity_server_application::{
  ApplicationError, ApplicationFailure, BuildLogSearchError, ManagementHandlerError, ManualTriggerError,
};

use super::{ApiError, ErrorCode, RequestId, application_error};
use crate::v1::MANAGEMENT_FORBIDDEN_MESSAGE;

pub(crate) trait ApplicationErrorClassification {
  fn classification(&self) -> ApplicationFailure;
}

impl ApplicationErrorClassification for ApplicationError {
  fn classification(&self) -> ApplicationFailure {
    ApplicationError::classification(self)
  }
}

impl ApplicationErrorClassification for ManualTriggerError {
  fn classification(&self) -> ApplicationFailure {
    ManualTriggerError::classification(self)
  }
}

impl ApplicationErrorClassification for BuildLogSearchError {
  fn classification(&self) -> ApplicationFailure {
    BuildLogSearchError::classification(self)
  }
}

pub(crate) fn authorized_handler_error<E: ApplicationErrorClassification>(
  error: ManagementHandlerError<E>,
  request_id: &RequestId,
) -> ApiError {
  match error {
    ManagementHandlerError::Forbidden(failure) => {
      let request_id = RequestId(failure.request_id().to_string());
      ApiError::new(
        StatusCode::FORBIDDEN,
        ErrorCode::Forbidden,
        MANAGEMENT_FORBIDDEN_MESSAGE,
        &request_id,
      )
    }
    ManagementHandlerError::Application(error) => application_error(error.classification(), request_id),
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  };

  use async_trait::async_trait;
  use axum::{
    Json,
    body::to_bytes,
    response::{IntoResponse, Response},
  };
  use octacity_server_application::{
    ApplicationError, AuthorizedCommandHandler, AuthorizedManagementCommandHandler, Command, ManagementAction,
    ManagementActor, ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationMapping,
    ManagementAuthorizationPolicy, ManagementAuthorizationTarget, ManagementCommandUseCase, ManagementIngress,
    ManagementRequestAttributes, ManagementRequestContext, ManagementRequestId, ManagementResource,
    ManagementResourceIdentity, ManagementResourceKind, ManagementSecurityScope, ManagementVisibility,
  };
  use serde_json::json;
  use uuid::Uuid;

  use super::*;
  use crate::v1::ErrorResponse;

  struct FixtureCommand;

  impl Command for FixtureCommand {
    type Outcome = &'static str;
  }

  impl ManagementAuthorizationTarget for FixtureCommand {
    const AUTHORIZATION: ManagementAuthorizationMapping =
      ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);

    fn management_resource(&self) -> Result<ManagementResource, octacity_server_application::ManagementSecurityError> {
      ManagementResource::instance(
        ManagementResourceKind::Project,
        ManagementResourceIdentity::new("protected-project")?,
      )
    }
  }

  struct FixturePolicy {
    allow: bool,
  }

  #[async_trait]
  impl ManagementAuthorizationPolicy for FixturePolicy {
    async fn authorize(
      &self,
      _context: &ManagementRequestContext,
      _action: ManagementAction,
      _resource: &ManagementResource,
    ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
      if self.allow {
        Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
      } else {
        Err(ManagementAuthorizationDenial::forbidden())
      }
    }
  }

  #[derive(Default)]
  struct FixtureUseCase {
    calls: AtomicUsize,
  }

  #[async_trait]
  impl ManagementCommandUseCase<FixtureCommand> for FixtureUseCase {
    type Error = ApplicationError;

    async fn execute_management_command(
      &self,
      _context: &ManagementRequestContext,
      _grant: &ManagementAuthorizationGrant,
      _command: FixtureCommand,
    ) -> Result<&'static str, Self::Error> {
      self.calls.fetch_add(1, Ordering::SeqCst);
      Ok("unchanged")
    }
  }

  type ErasedFixtureHandler = dyn AuthorizedManagementCommandHandler<FixtureCommand, Error = ApplicationError>;

  #[tokio::test]
  async fn denial_maps_to_an_opaque_forbidden_envelope_with_safe_correlation() {
    let use_case = Arc::new(FixtureUseCase::default());
    let handler: Arc<ErasedFixtureHandler> = Arc::new(AuthorizedCommandHandler::new(
      Arc::new(FixturePolicy { allow: false }),
      use_case.clone(),
    ));
    let context = private_context();
    let transport_request_id = RequestId(Uuid::new_v4().to_string());

    let error = handler
      .handle_authorized_command(&context, FixtureCommand)
      .await
      .unwrap_err();
    let response = authorized_handler_error(error, &transport_request_id).into_response();

    assert_eq!(use_case.calls.load(Ordering::SeqCst), 0);
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response_body(response).await;
    let error: ErrorResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(error.code, ErrorCode::Forbidden);
    assert_eq!(error.message, "management operation is forbidden");
    assert_eq!(error.request_id, context.request_id().to_string());
    let diagnostic = String::from_utf8(body).unwrap();
    for protected in ["private-operator", "private:operator", "protected-project", "policy"] {
      assert!(!diagnostic.contains(protected), "forbidden response leaked {protected}");
    }
  }

  #[tokio::test]
  async fn allowed_type_erased_fixture_retains_its_existing_response() {
    let use_case = Arc::new(FixtureUseCase::default());
    let handler: Arc<ErasedFixtureHandler> = Arc::new(AuthorizedCommandHandler::new(
      Arc::new(FixturePolicy { allow: true }),
      use_case.clone(),
    ));
    let context = private_context();

    let outcome = handler
      .handle_authorized_command(&context, FixtureCommand)
      .await
      .unwrap();
    let response = (StatusCode::OK, Json(json!({ "result": outcome }))).into_response();

    assert_eq!(use_case.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, br#"{"result":"unchanged"}"#);
  }

  fn private_context() -> ManagementRequestContext {
    ManagementRequestContext::new(
      ManagementActor::authenticated("private-operator").unwrap(),
      ManagementSecurityScope::new("private:operator").unwrap(),
      ManagementRequestId::new(Uuid::from_u128(0x12345678_1234_4234_8234_123456789abc)).unwrap(),
      ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
    )
    .unwrap()
  }

  async fn response_body(response: Response) -> Vec<u8> {
    to_bytes(response.into_body(), 1_024).await.unwrap().to_vec()
  }
}
