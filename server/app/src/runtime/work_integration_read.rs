//! Shared verified read dispatch and closed failure mapping; no retries or role inference.
use octacity_server_application::{ApplicationError, WorkSourceFailure};
use octacity_server_factory::ImmutableReference;
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::{Request, Response, review, source};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(super) async fn execute_read(
  registry: &WorkAdapterRegistry,
  plugin: &ImmutableReference,
  wire: Request,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
) -> Result<Response, ApplicationError> {
  let adapter = registry
    .resolve(plugin.identity().as_str(), &plugin.digest().to_string())
    .map_err(|_| ApplicationError::capability_unavailable())?;
  let response = adapter
    .execute(wire, timeout, grace, cancellation)
    .await
    .map_err(|error| {
      use octacity_server_work_source::{HostError, HostFailureClass};
      let class = match error {
        HostError::TimedOut { .. } => WorkSourceFailure::Timeout,
        _ => match error.class() {
          HostFailureClass::Cancelled => WorkSourceFailure::Cancelled,
          HostFailureClass::Unsupported => WorkSourceFailure::Unavailable,
          HostFailureClass::ProtocolFault => WorkSourceFailure::ProtocolFault,
          HostFailureClass::Permanent | HostFailureClass::InvalidRequest => WorkSourceFailure::InvalidConfiguration,
          HostFailureClass::Transient => WorkSourceFailure::Transient,
        },
      };
      ApplicationError::SourceDependency {
        class,
        retry_after_millis: None,
      }
    })?;
  if let Response::Source(source::Response {
    outcome: source::Outcome::Failure(failure),
    ..
  })
  | Response::Review(review::Response {
    outcome: review::Outcome::Failure(failure),
    ..
  }) = &response
  {
    use octacity_work_source_protocol::FailureClass;
    let class = match failure.class {
      FailureClass::Transient if failure.retry_after_millis.is_some() => WorkSourceFailure::RateLimited,
      FailureClass::Transient => WorkSourceFailure::Transient,
      FailureClass::Unsupported => WorkSourceFailure::Unavailable,
      FailureClass::Conflict => WorkSourceFailure::Conflict,
      FailureClass::Cancelled => WorkSourceFailure::Cancelled,
      FailureClass::ProtocolFault => WorkSourceFailure::ProtocolFault,
      FailureClass::Permanent | FailureClass::InvalidRequest => WorkSourceFailure::InvalidConfiguration,
    };
    return Err(ApplicationError::SourceDependency {
      class,
      retry_after_millis: failure.retry_after_millis,
    });
  }
  Ok(response)
}
