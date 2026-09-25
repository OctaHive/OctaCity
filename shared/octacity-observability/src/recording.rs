//! Safe emission through replaceable tracing and metrics facades.

use std::time::Duration;

use metrics::Label;

use crate::{
  AdapterKind, ErrorClass, HttpMethod, HttpRoute, MetricKind, MetricLabelSet, MetricLabelValue, MetricName,
  MetricScope, MetricUnit, Operation, Outcome, TraceEvent, TriggerKind, WorkerKind,
};

/// Server operation counters that share the closed operation label contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerOperationMetric {
  /// Build, Attempt, and Job graph transitions.
  Orchestration,
  /// Placement and Lease lifecycle operations.
  Lease,
  /// Remote-cache authority and data-plane operations.
  Cache,
  /// Artifact and immutable-log operations.
  Artifact,
}

impl ServerOperationMetric {
  const fn metric(self) -> MetricName {
    match self {
      Self::Orchestration => MetricName::ServerOrchestratorTransitions,
      Self::Lease => MetricName::ServerLeaseOperations,
      Self::Cache => MetricName::ServerCacheOperations,
      Self::Artifact => MetricName::ServerArtifactOperations,
    }
  }
}

/// Describes every metric owned by one product area to the installed recorder.
///
/// With no recorder installed this is a bounded no-op. Recorder or exporter
/// failure is deliberately outside the correctness path.
pub fn describe_metrics(scope: MetricScope) {
  for metric in MetricName::all() {
    let descriptor = metric.descriptor();
    if descriptor.scope != scope {
      continue;
    }
    let unit = metric_unit(descriptor.unit);
    match descriptor.kind {
      MetricKind::Counter => metrics::describe_counter!(descriptor.name, unit, descriptor.name),
      MetricKind::Histogram => metrics::describe_histogram!(descriptor.name, unit, descriptor.name),
      MetricKind::Gauge | MetricKind::UpDownCounter => {
        metrics::describe_gauge!(descriptor.name, unit, descriptor.name)
      }
    }
  }
}

/// Records one classified server operation and a safe structured event.
pub fn record_server_operation(
  metric: ServerOperationMetric,
  operation: Operation,
  outcome: Outcome,
  error_class: Option<ErrorClass>,
) {
  let labels = operation_labels(operation, outcome, error_class);
  increment(metric.metric(), &labels, 1);
  trace_operation(operation, outcome, error_class);
}

/// Records an authoritative sample of the current durable ready-Job count.
pub fn record_ready_jobs(count: u64) {
  set_gauge(MetricName::ServerReadyJobs, &MetricLabelSet::new(), count as f64);
  tracing::info!(
    event.name = TraceEvent::OperationCompleted.as_str(),
    component = "ready_queue",
    operation = Operation::Read.as_str(),
    outcome = Outcome::Success.as_str(),
    batch.size = count,
    "ready-Job queue sampled"
  );
}

/// Records one authoritative store operation and its elapsed time.
pub fn record_store_operation(
  operation: Operation,
  outcome: Outcome,
  error_class: Option<ErrorClass>,
  elapsed: Duration,
) {
  let labels = operation_labels(operation, outcome, error_class);
  increment(MetricName::ServerStoreOperations, &labels, 1);
  histogram(MetricName::ServerStoreOperationDuration, &labels, elapsed.as_secs_f64());
  trace_operation(operation, outcome, error_class);
}

/// Records one provider-neutral adapter operation.
pub fn record_adapter_operation(
  adapter: AdapterKind,
  operation: Operation,
  outcome: Outcome,
  error_class: Option<ErrorClass>,
) {
  let labels = labels([
    adapter.into_metric_label(),
    operation.into_metric_label(),
    outcome.into_metric_label(),
  ]);
  increment(MetricName::ServerAdapterOperations, &labels, 1);
  tracing::info!(
    event.name = TraceEvent::OperationCompleted.as_str(),
    adapter.kind = adapter.as_str(),
    operation = operation.as_str(),
    outcome = outcome.as_str(),
    error.class = error_class.map_or("", ErrorClass::as_str),
    "adapter operation completed"
  );
}

/// Records one bounded HTTP request using a coarse route label.
pub fn record_http_request(
  method: HttpMethod,
  route: HttpRoute,
  outcome: Outcome,
  status_code: u16,
  elapsed: Duration,
) {
  let labels = labels([
    method.into_metric_label(),
    route.into_metric_label(),
    outcome.into_metric_label(),
  ]);
  increment(MetricName::ServerHttpRequests, &labels, 1);
  histogram(MetricName::ServerHttpRequestDuration, &labels, elapsed.as_secs_f64());
  let elapsed_milliseconds = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
  tracing::info!(
    event.name = TraceEvent::ServerRequestCompleted.as_str(),
    http.response.status_code = status_code,
    duration.milliseconds = elapsed_milliseconds,
    outcome = outcome.as_str(),
    "HTTP request completed"
  );
}

/// Classifies an HTTP response without exposing a raw route or error body.
#[must_use]
pub const fn classify_http_response(status_code: u16) -> Outcome {
  match status_code {
    200..=399 => Outcome::Success,
    400..=499 => Outcome::Rejected,
    _ => Outcome::Failure,
  }
}

/// Records one normalized Trigger decision.
pub fn record_trigger_decision(kind: TriggerKind, outcome: Outcome, error_class: Option<ErrorClass>) {
  let labels = labels([kind.into_metric_label(), outcome.into_metric_label()]);
  increment(MetricName::ServerTriggerDecisions, &labels, 1);
  tracing::info!(
    event.name = TraceEvent::OperationCompleted.as_str(),
    component = "trigger",
    operation = Operation::Evaluate.as_str(),
    outcome = outcome.as_str(),
    error.class = error_class.map_or("", ErrorClass::as_str),
    "Trigger decision completed"
  );
}

/// Records one durable worker pass and its elapsed time.
pub fn record_worker_run(worker: WorkerKind, outcome: Outcome, error_class: Option<ErrorClass>, elapsed: Duration) {
  let mut labels = MetricLabelSet::new();
  insert(&mut labels, worker);
  insert(&mut labels, outcome);
  if let Some(error_class) = error_class {
    insert(&mut labels, error_class);
  }
  increment(MetricName::ServerWorkerRuns, &labels, 1);
  histogram(MetricName::ServerWorkerRunDuration, &labels, elapsed.as_secs_f64());
  tracing::info!(
    event.name = TraceEvent::OperationCompleted.as_str(),
    worker.kind = worker.as_str(),
    operation = Operation::Complete.as_str(),
    outcome = outcome.as_str(),
    error.class = error_class.map_or("", ErrorClass::as_str),
    duration.milliseconds = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
    "durable worker pass completed"
  );
}

/// Records that a durable worker retained work for bounded retry.
pub fn record_worker_retries(worker: WorkerKind, count: usize) {
  if count == 0 {
    return;
  }
  let labels = labels([worker.into_metric_label()]);
  increment(
    MetricName::ServerWorkerRetries,
    &labels,
    u64::try_from(count).unwrap_or(u64::MAX),
  );
  tracing::info!(
    event.name = TraceEvent::RetryScheduled.as_str(),
    worker.kind = worker.as_str(),
    operation = Operation::Retry.as_str(),
    outcome = Outcome::Retry.as_str(),
    batch.size = count,
    "durable retry scheduled"
  );
}

fn operation_labels(operation: Operation, outcome: Outcome, error_class: Option<ErrorClass>) -> MetricLabelSet {
  let mut labels = MetricLabelSet::new();
  insert(&mut labels, operation);
  insert(&mut labels, outcome);
  if let Some(error_class) = error_class {
    insert(&mut labels, error_class);
  }
  labels
}

fn labels<const N: usize>(values: [crate::MetricLabel; N]) -> MetricLabelSet {
  let mut labels = MetricLabelSet::new();
  for value in values {
    labels
      .insert_label(value)
      .expect("closed observability labels must form a valid set");
  }
  labels
}

fn insert<T: MetricLabelValue>(labels: &mut MetricLabelSet, value: T) {
  labels
    .insert(value)
    .expect("closed observability labels must form a valid set");
}

fn increment(metric: MetricName, labels: &MetricLabelSet, value: u64) {
  let descriptor = metric.descriptor();
  if descriptor.kind != MetricKind::Counter || descriptor.validate(labels).is_err() {
    tracing::error!(metric.name = descriptor.name, "invalid counter emission was dropped");
    return;
  }
  metrics::counter!(descriptor.name, metric_labels(labels)).increment(value);
}

fn histogram(metric: MetricName, labels: &MetricLabelSet, value: f64) {
  let descriptor = metric.descriptor();
  if descriptor.kind != MetricKind::Histogram || descriptor.validate(labels).is_err() {
    tracing::error!(metric.name = descriptor.name, "invalid histogram emission was dropped");
    return;
  }
  metrics::histogram!(descriptor.name, metric_labels(labels)).record(value);
}

fn set_gauge(metric: MetricName, labels: &MetricLabelSet, value: f64) {
  let descriptor = metric.descriptor();
  if !matches!(descriptor.kind, MetricKind::Gauge | MetricKind::UpDownCounter) || descriptor.validate(labels).is_err() {
    tracing::error!(metric.name = descriptor.name, "invalid gauge emission was dropped");
    return;
  }
  metrics::gauge!(descriptor.name, metric_labels(labels)).set(value);
}

fn metric_labels(labels: &MetricLabelSet) -> Vec<Label> {
  labels
    .iter()
    .map(|label| Label::from_static_parts(label.key().as_str(), label.value()))
    .collect()
}

fn trace_operation(operation: Operation, outcome: Outcome, error_class: Option<ErrorClass>) {
  tracing::info!(
    event.name = TraceEvent::OperationCompleted.as_str(),
    operation = operation.as_str(),
    outcome = outcome.as_str(),
    error.class = error_class.map_or("", ErrorClass::as_str),
    "server operation completed"
  );
}

const fn metric_unit(unit: MetricUnit) -> metrics::Unit {
  match unit {
    MetricUnit::One => metrics::Unit::Count,
    MetricUnit::Seconds => metrics::Unit::Seconds,
    MetricUnit::Bytes => metrics::Unit::Bytes,
  }
}

#[cfg(test)]
mod tests {
  use std::{
    io,
    sync::{
      Arc, Mutex,
      atomic::{AtomicU64, Ordering},
    },
  };

  use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit, with_local_recorder};
  use tracing_subscriber::fmt::MakeWriter;

  use super::*;
  use crate::TraceSpan;

  #[derive(Default)]
  struct CapturedMetrics {
    keys: Mutex<Vec<Key>>,
    ready_jobs: Arc<AtomicU64>,
    worker_retries: Arc<AtomicU64>,
  }

  impl CapturedMetrics {
    fn capture(&self, key: &Key) {
      self.keys.lock().unwrap().push(key.clone());
    }
  }

  impl Recorder for CapturedMetrics {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
      self.capture(key);
      if key.name() == MetricName::ServerWorkerRetries.descriptor().name {
        Counter::from_arc(self.worker_retries.clone())
      } else {
        Counter::noop()
      }
    }

    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
      self.capture(key);
      if key.name() == MetricName::ServerReadyJobs.descriptor().name {
        Gauge::from_arc(self.ready_jobs.clone())
      } else {
        Gauge::noop()
      }
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
      self.capture(key);
      Histogram::noop()
    }
  }

  #[test]
  fn representative_components_emit_bounded_outcome_metrics() {
    let recorder = CapturedMetrics::default();
    with_local_recorder(&recorder, || {
      record_http_request(
        HttpMethod::Post,
        HttpRoute::Management,
        Outcome::Success,
        202,
        Duration::from_millis(4),
      );
      record_adapter_operation(
        AdapterKind::Webhook,
        Operation::Verify,
        Outcome::Rejected,
        Some(ErrorClass::Invalid),
      );
      record_adapter_operation(
        AdapterKind::Vcs,
        Operation::Resolve,
        Outcome::Failure,
        Some(ErrorClass::Unavailable),
      );
      record_trigger_decision(TriggerKind::Manual, Outcome::Success, None);
      record_server_operation(
        ServerOperationMetric::Orchestration,
        Operation::Complete,
        Outcome::Success,
        None,
      );
      record_server_operation(
        ServerOperationMetric::Lease,
        Operation::Claim,
        Outcome::Rejected,
        Some(ErrorClass::Fenced),
      );
      record_ready_jobs(7);
      record_store_operation(
        Operation::Append,
        Outcome::Failure,
        Some(ErrorClass::Unavailable),
        Duration::from_millis(2),
      );
      record_server_operation(
        ServerOperationMetric::Cache,
        Operation::Download,
        Outcome::Success,
        None,
      );
      record_server_operation(
        ServerOperationMetric::Artifact,
        Operation::Upload,
        Outcome::Rejected,
        Some(ErrorClass::Protocol),
      );
      record_worker_run(WorkerKind::Outbox, Outcome::Success, None, Duration::from_millis(1));
      record_worker_run(
        WorkerKind::Retention,
        Outcome::Failure,
        Some(ErrorClass::Unavailable),
        Duration::from_millis(3),
      );
      record_worker_retries(WorkerKind::Webhook, 2);
    });

    assert_eq!(f64::from_bits(recorder.ready_jobs.load(Ordering::Acquire)), 7.0);
    assert_eq!(recorder.worker_retries.load(Ordering::Acquire), 2);
    let keys = recorder.keys.lock().unwrap();
    for required in [
      MetricName::ServerHttpRequests,
      MetricName::ServerHttpRequestDuration,
      MetricName::ServerAdapterOperations,
      MetricName::ServerTriggerDecisions,
      MetricName::ServerOrchestratorTransitions,
      MetricName::ServerLeaseOperations,
      MetricName::ServerReadyJobs,
      MetricName::ServerStoreOperations,
      MetricName::ServerStoreOperationDuration,
      MetricName::ServerCacheOperations,
      MetricName::ServerArtifactOperations,
      MetricName::ServerWorkerRuns,
      MetricName::ServerWorkerRunDuration,
      MetricName::ServerWorkerRetries,
    ] {
      assert!(
        keys.iter().any(|key| key.name() == required.descriptor().name),
        "missing representative metric {}",
        required.descriptor().name
      );
    }
    let outcomes: Vec<_> = keys
      .iter()
      .flat_map(Key::labels)
      .filter(|label| label.key() == "outcome")
      .map(metrics::Label::value)
      .collect();
    for expected in ["success", "rejected", "failure"] {
      assert!(outcomes.contains(&expected), "missing {expected} outcome");
    }
    assert!(keys.iter().flat_map(Key::labels).all(|label| {
      !label.key().ends_with(".id") && !label.value().contains("secret") && !label.value().contains("?")
    }));
  }

  #[test]
  fn trace_events_inherit_safe_request_correlation_without_diagnostics() {
    let output = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
      .json()
      .with_ansi(false)
      .with_writer(output.clone())
      .finish();
    tracing::subscriber::with_default(subscriber, || {
      let span = tracing::info_span!(
        TraceSpan::ServerHttpRequest.as_str(),
        request.id = "request-correlation-42",
        http.route = "/api/v1/artifacts/{artifact_id}:download",
      );
      let _entered = span.enter();
      record_server_operation(
        ServerOperationMetric::Artifact,
        Operation::Download,
        Outcome::Failure,
        Some(ErrorClass::Unavailable),
      );
    });

    let output = output.text();
    assert!(output.contains("request-correlation-42"), "captured trace: {output}");
    assert!(output.contains("octacity.operation.completed"));
    assert!(output.contains("unavailable"));
    assert!(!output.contains("X-Amz-Credential"));
    assert!(!output.contains("query-secret"));
  }

  #[derive(Clone, Default)]
  struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

  impl CapturedLogs {
    fn text(&self) -> String {
      String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
  }

  struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

  impl io::Write for CapturedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
      self.0.lock().unwrap().extend_from_slice(buffer);
      Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
      Ok(())
    }
  }

  impl<'writer> MakeWriter<'writer> for CapturedLogs {
    type Writer = CapturedWriter;

    fn make_writer(&'writer self) -> Self::Writer {
      CapturedWriter(self.0.clone())
    }
  }
}
