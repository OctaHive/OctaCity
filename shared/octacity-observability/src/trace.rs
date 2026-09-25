use thiserror::Error;

/// Maximum UTF-8 bytes in one high-cardinality correlation value.
pub const MAX_CORRELATION_VALUE_BYTES: usize = 128;

/// Stable structured span names.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TraceSpan {
  /// One inbound server HTTP request.
  ServerHttpRequest,
  /// One authoritative store operation.
  ServerStoreOperation,
  /// One external adapter operation.
  ServerAdapterOperation,
  /// One durable worker pass.
  ServerWorkerRun,
  /// One Agent coordination request.
  AgentCoordination,
  /// One Agent Job lifecycle.
  AgentJob,
}

impl TraceSpan {
  /// Returns the stable span name.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::ServerHttpRequest => "octacity.server.http.request",
      Self::ServerStoreOperation => "octacity.server.store.operation",
      Self::ServerAdapterOperation => "octacity.server.adapter.operation",
      Self::ServerWorkerRun => "octacity.server.worker.run",
      Self::AgentCoordination => "octacity.agent.coordination",
      Self::AgentJob => "octacity.agent.job",
    }
  }
}

/// Stable structured event names.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TraceEvent {
  /// HTTP request completed.
  ServerRequestCompleted,
  /// Operation was rejected at a boundary.
  OperationRejected,
  /// Durable retry was scheduled.
  RetryScheduled,
  /// Telemetry was dropped at a bounded boundary.
  TelemetryDropped,
  /// Top-level Agent command failed.
  AgentCommandFailed,
}

impl TraceEvent {
  /// Returns the stable event name.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::ServerRequestCompleted => "octacity.server.http.request.completed",
      Self::OperationRejected => "octacity.operation.rejected",
      Self::RetryScheduled => "octacity.retry.scheduled",
      Self::TelemetryDropped => "octacity.telemetry.dropped",
      Self::AgentCommandFailed => "octacity.agent.command.failed",
    }
  }
}

/// Cardinality and safety class for a trace field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TraceFieldClass {
  /// Closed classification also suitable for a declared metric label.
  Classification,
  /// Bounded high-cardinality identifier for trace correlation only.
  Correlation,
  /// Numeric measurement.
  Measurement,
  /// Stable safe diagnostic code, never a raw error body.
  Diagnostic,
}

/// Stable structured trace fields.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TraceField {
  /// Stable event name.
  EventName,
  /// Stable component name.
  Component,
  /// Stable operation classification.
  Operation,
  /// Stable outcome classification.
  Outcome,
  /// Stable error classification.
  ErrorClass,
  /// Request correlation identity.
  RequestId,
  /// Build correlation identity.
  BuildId,
  /// Attempt correlation identity.
  AttemptId,
  /// Job correlation identity.
  JobId,
  /// Agent correlation identity.
  AgentId,
  /// Lease correlation identity.
  LeaseId,
  /// Trigger-occurrence correlation identity.
  TriggerOccurrenceId,
  /// Integration correlation identity.
  IntegrationId,
  /// Stable HTTP method.
  HttpMethod,
  /// Registered route template, never a raw URI.
  HttpRoute,
  /// HTTP response status.
  HttpStatusCode,
  /// Elapsed operation time in milliseconds.
  DurationMilliseconds,
  /// Bounded batch item count.
  BatchSize,
  /// Stable worker kind.
  WorkerKind,
  /// Stable adapter kind.
  AdapterKind,
  /// Stable diagnostic code.
  DiagnosticCode,
}

const ALL_TRACE_FIELDS: &[TraceField] = &[
  TraceField::EventName,
  TraceField::Component,
  TraceField::Operation,
  TraceField::Outcome,
  TraceField::ErrorClass,
  TraceField::RequestId,
  TraceField::BuildId,
  TraceField::AttemptId,
  TraceField::JobId,
  TraceField::AgentId,
  TraceField::LeaseId,
  TraceField::TriggerOccurrenceId,
  TraceField::IntegrationId,
  TraceField::HttpMethod,
  TraceField::HttpRoute,
  TraceField::HttpStatusCode,
  TraceField::DurationMilliseconds,
  TraceField::BatchSize,
  TraceField::WorkerKind,
  TraceField::AdapterKind,
  TraceField::DiagnosticCode,
];

impl TraceField {
  /// Returns every stable trace field.
  #[must_use]
  pub const fn all() -> &'static [Self] {
    ALL_TRACE_FIELDS
  }

  /// Returns the stable structured field spelling.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::EventName => "event.name",
      Self::Component => "component",
      Self::Operation => "operation",
      Self::Outcome => "outcome",
      Self::ErrorClass => "error.class",
      Self::RequestId => "request.id",
      Self::BuildId => "build.id",
      Self::AttemptId => "attempt.id",
      Self::JobId => "job.id",
      Self::AgentId => "agent.id",
      Self::LeaseId => "lease.id",
      Self::TriggerOccurrenceId => "trigger.occurrence.id",
      Self::IntegrationId => "integration.id",
      Self::HttpMethod => "http.request.method",
      Self::HttpRoute => "http.route",
      Self::HttpStatusCode => "http.response.status_code",
      Self::DurationMilliseconds => "duration.milliseconds",
      Self::BatchSize => "batch.size",
      Self::WorkerKind => "worker.kind",
      Self::AdapterKind => "adapter.kind",
      Self::DiagnosticCode => "diagnostic.code",
    }
  }

  /// Returns the field's safety and cardinality class.
  #[must_use]
  pub const fn class(self) -> TraceFieldClass {
    match self {
      Self::RequestId
      | Self::BuildId
      | Self::AttemptId
      | Self::JobId
      | Self::AgentId
      | Self::LeaseId
      | Self::TriggerOccurrenceId
      | Self::IntegrationId => TraceFieldClass::Correlation,
      Self::HttpStatusCode | Self::DurationMilliseconds | Self::BatchSize => TraceFieldClass::Measurement,
      Self::DiagnosticCode => TraceFieldClass::Diagnostic,
      Self::EventName
      | Self::Component
      | Self::Operation
      | Self::Outcome
      | Self::ErrorClass
      | Self::HttpMethod
      | Self::HttpRoute
      | Self::WorkerKind
      | Self::AdapterKind => TraceFieldClass::Classification,
    }
  }
}

/// Validated high-cardinality identity used only for trace correlation.
///
/// This type intentionally does not implement the sealed metric-label trait.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CorrelationValue(String);

impl CorrelationValue {
  /// Validates a bounded opaque identity.
  pub fn try_new(value: impl Into<String>) -> Result<Self, TraceContractError> {
    let value = value.into();
    if value.is_empty()
      || value.len() > MAX_CORRELATION_VALUE_BYTES
      || value.trim() != value
      || value.chars().any(|character| character.is_control())
    {
      return Err(TraceContractError::InvalidCorrelationValue);
    }
    Ok(Self(value))
  }

  /// Borrows the validated correlation value.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Trace vocabulary validation failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TraceContractError {
  /// Correlation value was empty, unbounded, padded, or contained controls.
  #[error("invalid trace correlation value")]
  InvalidCorrelationValue,
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use super::*;

  #[test]
  fn field_names_are_unique_and_correlation_values_are_explicit() {
    let mut names = HashSet::new();
    for field in TraceField::all() {
      assert!(
        names.insert(field.as_str()),
        "duplicate trace field: {}",
        field.as_str()
      );
      if field.as_str().ends_with(".id") {
        assert_eq!(field.class(), TraceFieldClass::Correlation);
      }
    }
  }

  #[test]
  fn correlation_values_are_bounded_but_not_metric_labels() {
    assert!(CorrelationValue::try_new("0195f0c2-opaque").is_ok());
    assert_eq!(
      CorrelationValue::try_new("x".repeat(MAX_CORRELATION_VALUE_BYTES + 1)),
      Err(TraceContractError::InvalidCorrelationValue)
    );
  }
}
