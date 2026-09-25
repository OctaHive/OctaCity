//! PostgreSQL operation telemetry at the store-port boundary.

use std::{future::Future, time::Instant};

use octacity_observability::{ErrorClass, Operation, Outcome, TraceSpan, record_store_operation};
use octacity_server_store::StoreError;
use tracing::Instrument as _;

pub(crate) async fn observe<T>(
  operation: Operation,
  future: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, StoreError> {
  let span = tracing::info_span!(
    TraceSpan::ServerStoreOperation.as_str(),
    component = "postgres",
    operation = operation.as_str(),
  );
  let started = Instant::now();
  let result = future.instrument(span.clone()).await;
  let _entered = span.enter();
  let (outcome, error_class) = match &result {
    Ok(_) => (Outcome::Success, None),
    Err(error) => classify(error),
  };
  record_store_operation(operation, outcome, error_class, started.elapsed());
  result
}

fn classify(error: &StoreError) -> (Outcome, Option<ErrorClass>) {
  match error {
    StoreError::InvalidInput { .. } | StoreError::CredentialRejected | StoreError::NotFound { .. } => {
      (Outcome::Rejected, Some(ErrorClass::Invalid))
    }
    StoreError::Conflict { .. } | StoreError::Duplicate { .. } => (Outcome::Rejected, Some(ErrorClass::Conflict)),
    StoreError::Fenced { .. } => (Outcome::Rejected, Some(ErrorClass::Fenced)),
    StoreError::Expired { .. } => (Outcome::Rejected, Some(ErrorClass::Expired)),
    StoreError::EventGap { .. } | StoreError::EventsMissing { .. } => (Outcome::Rejected, Some(ErrorClass::Conflict)),
    StoreError::Unavailable => (Outcome::Failure, Some(ErrorClass::Unavailable)),
  }
}
