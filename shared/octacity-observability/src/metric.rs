use thiserror::Error;

use crate::{MAX_LABELS_PER_METRIC, MetricLabelKey, MetricLabelSet};

/// Absolute upper bound for one metric instrument's permitted time series.
pub const MAX_SERIES_PER_METRIC: u16 = 2_048;

/// Product area that owns a metric.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricScope {
  /// Headless coordinator server.
  Server,
  /// Outbound build Agent.
  Agent,
}

/// Exporter-neutral metric aggregation kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricKind {
  /// Monotonic accumulated value.
  Counter,
  /// Distribution of recorded observations.
  Histogram,
  /// Value that may increase or decrease.
  UpDownCounter,
  /// Current sampled value.
  Gauge,
}

/// Stable metric units using UCUM-compatible spellings.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricUnit {
  /// Dimensionless count.
  One,
  /// Seconds.
  Seconds,
  /// Bytes.
  Bytes,
}

impl MetricUnit {
  /// Returns the stable exporter spelling.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::One => "1",
      Self::Seconds => "s",
      Self::Bytes => "By",
    }
  }
}

/// Explicit per-instrument cardinality budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricCardinalityBudget {
  /// Maximum labels on one data point.
  pub max_labels: u8,
  /// Maximum time series exported for the instrument.
  pub max_series: u16,
}

/// Stable server and Agent metric instruments.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricName {
  /// Accepted server HTTP requests.
  ServerHttpRequests,
  /// Server HTTP request latency.
  ServerHttpRequestDuration,
  /// Authoritative store operations.
  ServerStoreOperations,
  /// Authoritative store operation latency.
  ServerStoreOperationDuration,
  /// Current number of durable ready Jobs.
  ServerReadyJobs,
  /// Trigger decisions.
  ServerTriggerDecisions,
  /// Orchestrator state transitions.
  ServerOrchestratorTransitions,
  /// Lease lifecycle operations.
  ServerLeaseOperations,
  /// Durable worker passes.
  ServerWorkerRuns,
  /// Durable worker pass latency.
  ServerWorkerRunDuration,
  /// Provider-neutral adapter operations.
  ServerAdapterOperations,
  /// Cache authority and data-plane operations.
  ServerCacheOperations,
  /// Artifact and immutable-log operations.
  ServerArtifactOperations,
  /// Server telemetry points dropped at a bounded boundary.
  ServerTelemetryDropped,
  /// Agent coordination requests.
  AgentCoordinationRequests,
  /// Agent heartbeat latency.
  AgentHeartbeatDuration,
  /// Agent Job outcomes.
  AgentJobs,
  /// Agent Job execution latency.
  AgentJobDuration,
  /// Agent build events delivered to the server.
  AgentEvents,
  /// Agent telemetry samples accepted locally for export.
  AgentTelemetrySamples,
  /// Agent telemetry samples dropped at a bounded boundary.
  AgentTelemetryDropped,
  /// Agent executor CPU time.
  AgentCpuTime,
  /// Current Agent executor memory usage.
  AgentMemoryUsage,
  /// Agent executor I/O bytes.
  AgentIo,
}

/// Complete exporter-neutral contract for one metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricDescriptor {
  /// Stable metric identity.
  pub metric: MetricName,
  /// Stable exported name.
  pub name: &'static str,
  /// Owning product area.
  pub scope: MetricScope,
  /// Aggregation kind.
  pub kind: MetricKind,
  /// Stable unit.
  pub unit: MetricUnit,
  /// Labels permitted on this instrument.
  pub labels: &'static [MetricLabelKey],
  /// Explicit per-instrument cardinality budget.
  pub cardinality: MetricCardinalityBudget,
}

const HTTP: &[MetricLabelKey] = &[
  MetricLabelKey::HttpMethod,
  MetricLabelKey::HttpRoute,
  MetricLabelKey::Outcome,
];
const OPERATION: &[MetricLabelKey] = &[
  MetricLabelKey::Operation,
  MetricLabelKey::Outcome,
  MetricLabelKey::ErrorClass,
];
const OUTCOME: &[MetricLabelKey] = &[MetricLabelKey::Outcome];
const TRIGGER: &[MetricLabelKey] = &[MetricLabelKey::TriggerKind, MetricLabelKey::Outcome];
const WORKER: &[MetricLabelKey] = &[
  MetricLabelKey::Worker,
  MetricLabelKey::Outcome,
  MetricLabelKey::ErrorClass,
];
const ADAPTER: &[MetricLabelKey] = &[
  MetricLabelKey::Adapter,
  MetricLabelKey::Operation,
  MetricLabelKey::Outcome,
];
const EXECUTION: &[MetricLabelKey] = &[
  MetricLabelKey::Runtime,
  MetricLabelKey::Isolation,
  MetricLabelKey::Outcome,
];
const RESOURCE: &[MetricLabelKey] = &[MetricLabelKey::Runtime, MetricLabelKey::Isolation];
const IO: &[MetricLabelKey] = &[
  MetricLabelKey::Runtime,
  MetricLabelKey::Isolation,
  MetricLabelKey::Direction,
];
const EVENT: &[MetricLabelKey] = &[MetricLabelKey::Stream, MetricLabelKey::Outcome];
const NONE: &[MetricLabelKey] = &[];

const ALL_METRICS: &[MetricName] = &[
  MetricName::ServerHttpRequests,
  MetricName::ServerHttpRequestDuration,
  MetricName::ServerStoreOperations,
  MetricName::ServerStoreOperationDuration,
  MetricName::ServerReadyJobs,
  MetricName::ServerTriggerDecisions,
  MetricName::ServerOrchestratorTransitions,
  MetricName::ServerLeaseOperations,
  MetricName::ServerWorkerRuns,
  MetricName::ServerWorkerRunDuration,
  MetricName::ServerAdapterOperations,
  MetricName::ServerCacheOperations,
  MetricName::ServerArtifactOperations,
  MetricName::ServerTelemetryDropped,
  MetricName::AgentCoordinationRequests,
  MetricName::AgentHeartbeatDuration,
  MetricName::AgentJobs,
  MetricName::AgentJobDuration,
  MetricName::AgentEvents,
  MetricName::AgentTelemetrySamples,
  MetricName::AgentTelemetryDropped,
  MetricName::AgentCpuTime,
  MetricName::AgentMemoryUsage,
  MetricName::AgentIo,
];

impl MetricName {
  /// Returns every metric in stable declaration order.
  #[must_use]
  pub const fn all() -> &'static [Self] {
    ALL_METRICS
  }

  /// Returns the complete metric contract.
  #[must_use]
  pub const fn descriptor(self) -> MetricDescriptor {
    use MetricKind::{Counter, Gauge, Histogram, UpDownCounter};
    use MetricScope::{Agent, Server};
    use MetricUnit::{Bytes, One, Seconds};

    let (name, scope, kind, unit, labels, max_series) = match self {
      Self::ServerHttpRequests => ("octacity.server.http.requests", Server, Counter, One, HTTP, 256),
      Self::ServerHttpRequestDuration => (
        "octacity.server.http.request.duration",
        Server,
        Histogram,
        Seconds,
        HTTP,
        256,
      ),
      Self::ServerStoreOperations => (
        "octacity.server.store.operations",
        Server,
        Counter,
        One,
        OPERATION,
        2_048,
      ),
      Self::ServerStoreOperationDuration => (
        "octacity.server.store.operation.duration",
        Server,
        Histogram,
        Seconds,
        OPERATION,
        2_048,
      ),
      Self::ServerReadyJobs => ("octacity.server.queue.ready.jobs", Server, UpDownCounter, One, NONE, 1),
      Self::ServerTriggerDecisions => ("octacity.server.trigger.decisions", Server, Counter, One, TRIGGER, 32),
      Self::ServerOrchestratorTransitions => (
        "octacity.server.orchestrator.transitions",
        Server,
        Counter,
        One,
        OPERATION,
        2_048,
      ),
      Self::ServerLeaseOperations => (
        "octacity.server.lease.operations",
        Server,
        Counter,
        One,
        OPERATION,
        2_048,
      ),
      Self::ServerWorkerRuns => ("octacity.server.worker.runs", Server, Counter, One, WORKER, 1_024),
      Self::ServerWorkerRunDuration => (
        "octacity.server.worker.run.duration",
        Server,
        Histogram,
        Seconds,
        WORKER,
        1_024,
      ),
      Self::ServerAdapterOperations => (
        "octacity.server.adapter.operations",
        Server,
        Counter,
        One,
        ADAPTER,
        1_024,
      ),
      Self::ServerCacheOperations => (
        "octacity.server.cache.operations",
        Server,
        Counter,
        One,
        OPERATION,
        2_048,
      ),
      Self::ServerArtifactOperations => (
        "octacity.server.artifact.operations",
        Server,
        Counter,
        One,
        OPERATION,
        2_048,
      ),
      Self::ServerTelemetryDropped => ("octacity.server.telemetry.dropped", Server, Counter, One, OUTCOME, 8),
      Self::AgentCoordinationRequests => (
        "octacity.agent.coordination.requests",
        Agent,
        Counter,
        One,
        OPERATION,
        2_048,
      ),
      Self::AgentHeartbeatDuration => (
        "octacity.agent.heartbeat.duration",
        Agent,
        Histogram,
        Seconds,
        OUTCOME,
        8,
      ),
      Self::AgentJobs => ("octacity.agent.jobs", Agent, Counter, One, EXECUTION, 64),
      Self::AgentJobDuration => ("octacity.agent.job.duration", Agent, Histogram, Seconds, EXECUTION, 64),
      Self::AgentEvents => ("octacity.agent.events", Agent, Counter, One, EVENT, 16),
      Self::AgentTelemetrySamples => ("octacity.agent.telemetry.samples", Agent, Counter, One, OUTCOME, 8),
      Self::AgentTelemetryDropped => ("octacity.agent.telemetry.dropped", Agent, Counter, One, OUTCOME, 8),
      Self::AgentCpuTime => ("octacity.agent.cpu.time", Agent, Counter, Seconds, RESOURCE, 16),
      Self::AgentMemoryUsage => ("octacity.agent.memory.usage", Agent, Gauge, Bytes, RESOURCE, 16),
      Self::AgentIo => ("octacity.agent.io", Agent, Counter, Bytes, IO, 128),
    };
    MetricDescriptor {
      metric: self,
      name,
      scope,
      kind,
      unit,
      labels,
      cardinality: MetricCardinalityBudget {
        max_labels: labels.len() as u8,
        max_series,
      },
    }
  }
}

impl MetricDescriptor {
  /// Computes the maximum series reachable through the closed label enums.
  #[must_use]
  pub fn theoretical_series(self) -> u16 {
    self
      .labels
      .iter()
      .fold(1_u16, |total, label| total.saturating_mul(label.value_budget()))
  }

  /// Validates a concrete label set against this instrument contract.
  pub fn validate(self, labels: &MetricLabelSet) -> Result<(), MetricContractError> {
    if labels.len() > usize::from(self.cardinality.max_labels) {
      return Err(MetricContractError::TooManyLabels);
    }
    for label in labels.iter() {
      if !self.labels.contains(&label.key()) {
        return Err(MetricContractError::LabelNotAllowed(label.key()));
      }
    }
    Ok(())
  }

  /// Validates the descriptor's declared budget against global limits.
  pub fn validate_budget(self) -> Result<(), MetricContractError> {
    if usize::from(self.cardinality.max_labels) > MAX_LABELS_PER_METRIC
      || self.labels.len() > usize::from(self.cardinality.max_labels)
      || self.cardinality.max_series == 0
      || self.cardinality.max_series > MAX_SERIES_PER_METRIC
      || self.theoretical_series() > self.cardinality.max_series
    {
      return Err(MetricContractError::InvalidCardinalityBudget);
    }
    Ok(())
  }
}

/// Metric contract validation failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MetricContractError {
  /// A supplied label is not allowed on the selected instrument.
  #[error("metric label is not allowed: {0:?}")]
  LabelNotAllowed(MetricLabelKey),
  /// The instrument-specific label count was exceeded.
  #[error("too many labels for metric")]
  TooManyLabels,
  /// The declared series or label budget is inconsistent.
  #[error("invalid metric cardinality budget")]
  InvalidCardinalityBudget,
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use crate::{HttpMethod, MetricLabelSet, RuntimeKind};

  use super::*;

  #[test]
  fn every_metric_name_and_budget_is_stable_and_bounded() {
    let mut names = HashSet::new();
    for metric in MetricName::all() {
      let descriptor = metric.descriptor();
      assert!(
        names.insert(descriptor.name),
        "duplicate metric name: {}",
        descriptor.name
      );
      assert!(descriptor.name.starts_with("octacity."));
      assert!(
        descriptor.validate_budget().is_ok(),
        "{} exceeds its budget",
        descriptor.name
      );
    }
  }

  #[test]
  fn a_metric_rejects_labels_from_another_contract() {
    let mut labels = MetricLabelSet::new();
    labels.insert(RuntimeKind::Native).unwrap();
    assert_eq!(
      MetricName::ServerHttpRequests.descriptor().validate(&labels),
      Err(MetricContractError::LabelNotAllowed(MetricLabelKey::Runtime))
    );

    let mut valid = MetricLabelSet::new();
    valid.insert(HttpMethod::Get).unwrap();
    assert!(MetricName::ServerHttpRequests.descriptor().validate(&valid).is_ok());
  }
}
