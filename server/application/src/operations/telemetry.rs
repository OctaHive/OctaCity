//! Application-layer telemetry classification without correctness coupling.

use std::future::Future;

use octacity_observability::{ErrorClass, Operation, Outcome, ServerOperationMetric, record_server_operation};

pub(crate) async fn observe<T, E>(
  metric: ServerOperationMetric,
  operation: Operation,
  future: impl Future<Output = Result<T, E>>,
  classify: impl FnOnce(&E) -> (Outcome, ErrorClass),
) -> Result<T, E> {
  observe_classified(metric, operation, future, |result| classify_result(result, classify)).await
}

pub(crate) async fn observe_classified<T, E>(
  metric: ServerOperationMetric,
  operation: Operation,
  future: impl Future<Output = Result<T, E>>,
  classify: impl FnOnce(&Result<T, E>) -> (Outcome, Option<ErrorClass>),
) -> Result<T, E> {
  let result = future.await;
  let (outcome, error_class) = classify(&result);
  record_server_operation(metric, operation, outcome, error_class);
  result
}

fn classify_result<T, E>(
  result: &Result<T, E>,
  classify: impl FnOnce(&E) -> (Outcome, ErrorClass),
) -> (Outcome, Option<ErrorClass>) {
  match result {
    Ok(_) => (Outcome::Success, None),
    Err(error) => {
      let (outcome, error_class) = classify(error);
      (outcome, Some(error_class))
    }
  }
}

pub(crate) const fn rejected(error: ErrorClass) -> (Outcome, ErrorClass) {
  (Outcome::Rejected, error)
}

pub(crate) const fn failed(error: ErrorClass) -> (Outcome, ErrorClass) {
  (Outcome::Failure, error)
}
