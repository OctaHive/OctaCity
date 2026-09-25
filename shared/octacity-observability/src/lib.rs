//! Stable, bounded observability vocabulary shared by the server and Agent.
//!
//! This crate deliberately contains no exporter, recorder, subscriber, queue,
//! or transport implementation. It owns the compatibility contract that those
//! implementations consume:
//!
//! - metric names, kinds, units, permitted labels, and series budgets;
//! - a closed low-cardinality label vocabulary;
//! - stable span, event, and trace-field names;
//! - trace correlation bounds and diagnostic redaction rules.
//!
//! Correctness never depends on this crate or on a telemetry backend. Task 8.3
//! adds bounded Agent ingestion and task 8.4 instruments the production paths.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod label;
mod metric;
mod redaction;
mod trace;

pub use label::{
  AdapterKind, Direction, ErrorClass, HttpMethod, HttpRoute, IsolationKind, MAX_LABELS_PER_METRIC, MetricLabel,
  MetricLabelKey, MetricLabelSet, MetricLabelSetError, MetricLabelValue, Operation, Outcome, RuntimeKind, StreamKind,
  TriggerKind, WorkerKind,
};
pub use metric::{
  MAX_SERIES_PER_METRIC, MetricCardinalityBudget, MetricContractError, MetricDescriptor, MetricKind, MetricName,
  MetricScope, MetricUnit,
};
pub use redaction::{MAX_DIAGNOSTIC_BYTES, REDACTED, RedactionAction, Sensitive, field_redaction, sanitize_diagnostic};
pub use trace::{
  CorrelationValue, MAX_CORRELATION_VALUE_BYTES, TraceContractError, TraceEvent, TraceField, TraceFieldClass, TraceSpan,
};
