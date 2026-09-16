//! Applies namespace metadata, cancellation, and deadlines to containerd gRPC calls.

use super::*;

/// Adds the configured containerd namespace to a request.
///
/// Namespace metadata is mandatory on every API call; omitting it could make
/// cleanup inspect or mutate resources outside the agent-owned namespace.
pub(super) fn namespaced<T>(value: T, namespace: &str) -> Result<Request<T>, ExecutionError> {
  let metadata = MetadataValue::try_from(namespace)
    .map_err(|_| invalid("containerd namespace cannot be represented as gRPC metadata"))?;
  let mut request = Request::new(value);
  request.metadata_mut().insert("containerd-namespace", metadata);
  Ok(request)
}

/// Converts monotonic elapsed time into the saturating wire representation.
pub(super) fn elapsed_millis(started: Instant) -> u64 {
  u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Creates the single absolute deadline shared by all steps of an operation.
pub(super) fn operation_deadline(duration: Duration) -> Result<Instant, ExecutionError> {
  Instant::now()
    .checked_add(duration)
    .ok_or_else(|| invalid("operation timeout is too large"))
}

/// Executes a containerd RPC within the job deadline and cancellation scope.
pub(super) async fn grpc_before<T>(
  deadline: Instant,
  cancellation: Option<&CancellationToken>,
  operation: &'static str,
  future: impl Future<Output = Result<T, Status>>,
) -> Result<T, ExecutionError> {
  before_deadline(deadline, cancellation, operation, future)
    .await?
    .map_err(|error| grpc(operation, error))
}

/// Applies an absolute deadline to a non-gRPC backend future.
pub(super) async fn before_deadline<T>(
  deadline: Instant,
  cancellation: Option<&CancellationToken>,
  operation: &'static str,
  future: impl Future<Output = T>,
) -> Result<T, ExecutionError> {
  if let Some(cancellation) = cancellation {
    tokio::select! {
      biased;
      () = cancellation.cancelled() => Err(ExecutionError::Cancelled),
      result = timeout_at(deadline, future) => {
        result.map_err(|_| ExecutionError::TimedOut { operation })
      }
    }
  } else {
    timeout_at(deadline, future)
      .await
      .map_err(|_| ExecutionError::TimedOut { operation })
  }
}

/// Adds operation context while preserving the containerd status code.
pub(super) fn grpc(operation: &str, error: Status) -> ExecutionError {
  ExecutionError::Backend(format!("{operation}: {error}"))
}
