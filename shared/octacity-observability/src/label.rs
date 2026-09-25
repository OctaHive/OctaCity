use thiserror::Error;

/// Maximum low-cardinality labels accepted for one metric point.
pub const MAX_LABELS_PER_METRIC: usize = 4;

/// Stable metric label keys.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MetricLabelKey {
  /// Provider-neutral adapter family.
  Adapter,
  /// Bounded transfer or I/O direction.
  Direction,
  /// Stable error classification.
  ErrorClass,
  /// HTTP request method.
  HttpMethod,
  /// Coarse route group, never a raw path.
  HttpRoute,
  /// Execution isolation class.
  Isolation,
  /// Stable operation classification.
  Operation,
  /// Stable operation outcome.
  Outcome,
  /// Agent runtime family.
  Runtime,
  /// Build-log stream classification.
  Stream,
  /// Trigger source classification.
  TriggerKind,
  /// Durable worker classification.
  Worker,
}

impl MetricLabelKey {
  /// Returns the stable exporter spelling.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Adapter => "adapter",
      Self::Direction => "direction",
      Self::ErrorClass => "error.class",
      Self::HttpMethod => "http.request.method",
      Self::HttpRoute => "http.route.group",
      Self::Isolation => "isolation.kind",
      Self::Operation => "operation",
      Self::Outcome => "outcome",
      Self::Runtime => "runtime.kind",
      Self::Stream => "stream",
      Self::TriggerKind => "trigger.kind",
      Self::Worker => "worker.kind",
    }
  }

  /// Returns the maximum number of values admitted by this closed key.
  #[must_use]
  pub const fn value_budget(self) -> u16 {
    match self {
      Self::Adapter => AdapterKind::COUNT,
      Self::Direction => Direction::COUNT,
      Self::ErrorClass => ErrorClass::COUNT,
      Self::HttpMethod => HttpMethod::COUNT,
      Self::HttpRoute => HttpRoute::COUNT,
      Self::Isolation => IsolationKind::COUNT,
      Self::Operation => Operation::COUNT,
      Self::Outcome => Outcome::COUNT,
      Self::Runtime => RuntimeKind::COUNT,
      Self::Stream => StreamKind::COUNT,
      Self::TriggerKind => TriggerKind::COUNT,
      Self::Worker => WorkerKind::COUNT,
    }
  }
}

/// One validated low-cardinality metric label.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MetricLabel {
  key: MetricLabelKey,
  value: &'static str,
}

impl MetricLabel {
  /// Returns the stable label key.
  #[must_use]
  pub const fn key(self) -> MetricLabelKey {
    self.key
  }

  /// Returns the stable label value.
  #[must_use]
  pub const fn value(self) -> &'static str {
    self.value
  }
}

mod private {
  pub trait Sealed {}
}

/// A sealed value that can be inserted into a metric label set.
///
/// The trait is intentionally not implementable outside this crate. This keeps
/// identifiers, user input, paths, URLs, and secret wrappers out of labels.
pub trait MetricLabelValue: private::Sealed + Copy {
  /// Converts the closed vocabulary value into a key/value pair.
  fn into_metric_label(self) -> MetricLabel;
}

macro_rules! label_enum {
  (
    $(#[$meta:meta])*
    $name:ident, $key:ident {
      $($(#[$variant_meta:meta])* $variant:ident => $value:literal),+ $(,)?
    }
  ) => {
    $(#[$meta])*
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum $name {
      $($(#[$variant_meta])* $variant),+
    }

    impl $name {
      const COUNT: u16 = [$(Self::$variant),+].len() as u16;

      /// Returns the stable exporter spelling.
      #[must_use]
      pub const fn as_str(self) -> &'static str {
        match self {
          $(Self::$variant => $value),+
        }
      }
    }

    impl private::Sealed for $name {}

    impl MetricLabelValue for $name {
      fn into_metric_label(self) -> MetricLabel {
        MetricLabel {
          key: MetricLabelKey::$key,
          value: self.as_str(),
        }
      }
    }
  };
}

label_enum! {
  /// Provider-neutral adapter families permitted in metrics.
  AdapterKind, Adapter {
    /// Webhook provider adapter.
    Webhook => "webhook",
    /// Version-control adapter.
    Vcs => "vcs",
    /// Immutable object-storage adapter.
    ObjectStorage => "object_storage",
    /// Secret-provider adapter.
    SecretProvider => "secret_provider",
    /// Telemetry exporter adapter.
    Telemetry => "telemetry",
  }
}

label_enum! {
  /// Bounded byte or object movement directions.
  Direction, Direction {
    /// Read from a dependency.
    Read => "read",
    /// Write to a dependency.
    Write => "write",
    /// Upload from an Agent.
    Upload => "upload",
    /// Download to a client or Agent.
    Download => "download",
    /// Receive from the network.
    Receive => "receive",
    /// Transmit to the network.
    Transmit => "transmit",
  }
}

label_enum! {
  /// Stable, secret-free failure classifications.
  ErrorClass, ErrorClass {
    /// Operation was cancelled.
    Cancelled => "cancelled",
    /// Optimistic or state conflict.
    Conflict => "conflict",
    /// Authority or lease expired.
    Expired => "expired",
    /// Stale owner was fenced.
    Fenced => "fenced",
    /// Unexpected internal failure.
    Internal => "internal",
    /// Input was invalid.
    Invalid => "invalid",
    /// Peer violated a protocol.
    Protocol => "protocol",
    /// Configured rate limit rejected work.
    RateLimited => "rate_limited",
    /// Bounded operation timed out.
    Timeout => "timeout",
    /// Dependency was unavailable.
    Unavailable => "unavailable",
  }
}

label_enum! {
  /// HTTP methods supported by the stable metric contract.
  HttpMethod, HttpMethod {
    /// GET.
    Get => "GET",
    /// POST.
    Post => "POST",
    /// PUT.
    Put => "PUT",
    /// PATCH.
    Patch => "PATCH",
    /// DELETE.
    Delete => "DELETE",
  }
}

label_enum! {
  /// Coarse bounded route groups; raw paths and identifiers are forbidden.
  HttpRoute, HttpRoute {
    /// Liveness and readiness probes.
    Health => "health",
    /// Trusted-network management API.
    Management => "management",
    /// Authenticated Agent API.
    Agent => "agent",
    /// Authenticated webhook ingress.
    Webhook => "webhook",
    /// Authorized cache data plane.
    Cache => "cache",
  }
}

label_enum! {
  /// Supported execution-isolation classes.
  IsolationKind, Isolation {
    /// Native Linux process isolation.
    Native => "native",
    /// OCI process isolation.
    OciProcess => "oci_process",
    /// OCI hypervisor isolation.
    OciHypervisor => "oci_hypervisor",
  }
}

label_enum! {
  /// Stable operation classifications used across server and Agent metrics.
  Operation, Operation {
    /// Accept an inbound command or event.
    Accept => "accept",
    /// Authenticate a caller or delivery.
    Authenticate => "authenticate",
    /// Verify content or a signature.
    Verify => "verify",
    /// Normalize provider input.
    Normalize => "normalize",
    /// Resolve immutable source state.
    Resolve => "resolve",
    /// Evaluate a policy or Trigger.
    Evaluate => "evaluate",
    /// Enqueue durable work.
    Enqueue => "enqueue",
    /// Claim durable work.
    Claim => "claim",
    /// Renew authority.
    Renew => "renew",
    /// Append ordered data.
    Append => "append",
    /// Complete an operation.
    Complete => "complete",
    /// Cancel active work.
    Cancel => "cancel",
    /// Retry durable work.
    Retry => "retry",
    /// Search a projection.
    Search => "search",
    /// Upload immutable bytes.
    Upload => "upload",
    /// Download immutable bytes.
    Download => "download",
    /// Delete retained state.
    Delete => "delete",
    /// Rebuild a derived projection.
    Rebuild => "rebuild",
    /// Export diagnostic data.
    Export => "export",
  }
}

label_enum! {
  /// Stable operation outcomes.
  Outcome, Outcome {
    /// Operation completed successfully.
    Success => "success",
    /// Operation was rejected without retry.
    Rejected => "rejected",
    /// Operation remains scheduled for retry.
    Retry => "retry",
    /// Operation failed.
    Failure => "failure",
    /// Operation was cancelled.
    Cancelled => "cancelled",
    /// Diagnostic data was intentionally dropped.
    Dropped => "dropped",
  }
}

label_enum! {
  /// Stable Agent runtime families.
  RuntimeKind, Runtime {
    /// Direct native executor.
    Native => "native",
    /// Containerd-backed executor.
    Containerd => "containerd",
    /// Microsandbox-backed executor.
    Microsandbox => "microsandbox",
  }
}

label_enum! {
  /// Textual build-log streams.
  StreamKind, Stream {
    /// Standard output.
    Stdout => "stdout",
    /// Standard error.
    Stderr => "stderr",
  }
}

label_enum! {
  /// Stable Trigger kinds.
  TriggerKind, TriggerKind {
    /// Trusted-network manual Trigger.
    Manual => "manual",
    /// Durable schedule occurrence.
    Schedule => "schedule",
    /// Authenticated external event.
    External => "external",
    /// Server-generated internal event.
    Internal => "internal",
  }
}

label_enum! {
  /// Durable server worker kinds.
  WorkerKind, Worker {
    /// Lease-expiry recovery.
    LeaseExpiry => "lease_expiry",
    /// Schedule evaluation.
    Schedule => "schedule",
    /// Internal Trigger delivery.
    InternalTrigger => "internal_trigger",
    /// Trigger source evaluation.
    TriggerEvaluation => "trigger_evaluation",
    /// Webhook delivery and registration work.
    Webhook => "webhook",
    /// Build-log search indexing.
    LogIndex => "log_index",
    /// Build Result retention.
    Retention => "retention",
    /// Invisible orphan cleanup.
    OrphanCleanup => "orphan_cleanup",
    /// Pending Artifact cleanup.
    ArtifactCleanup => "artifact_cleanup",
    /// Cache retention.
    CacheRetention => "cache_retention",
    /// Transactional outbox delivery.
    Outbox => "outbox",
  }
}

/// A bounded set of closed-vocabulary metric labels.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MetricLabelSet {
  labels: Vec<MetricLabel>,
}

impl MetricLabelSet {
  /// Creates an empty label set.
  #[must_use]
  pub const fn new() -> Self {
    Self { labels: Vec::new() }
  }

  /// Inserts one allowed low-cardinality value.
  ///
  /// Arbitrary strings, correlation identifiers, and secret wrappers cannot
  /// satisfy the sealed [`MetricLabelValue`] bound.
  ///
  /// ```compile_fail
  /// use octacity_observability::MetricLabelSet;
  /// let mut labels = MetricLabelSet::new();
  /// labels.insert("agent-0195").unwrap();
  /// ```
  ///
  /// ```compile_fail
  /// use octacity_observability::{MetricLabelSet, Sensitive};
  /// let mut labels = MetricLabelSet::new();
  /// labels.insert(Sensitive::new("bearer-secret")).unwrap();
  /// ```
  ///
  /// ```compile_fail
  /// use octacity_observability::{CorrelationValue, MetricLabelSet};
  /// let mut labels = MetricLabelSet::new();
  /// let build_id = CorrelationValue::try_new("build-0195").unwrap();
  /// labels.insert(build_id).unwrap();
  /// ```
  pub fn insert<T: MetricLabelValue>(&mut self, value: T) -> Result<(), MetricLabelSetError> {
    let label = value.into_metric_label();
    if self.labels.iter().any(|existing| existing.key == label.key) {
      return Err(MetricLabelSetError::DuplicateKey(label.key));
    }
    if self.labels.len() == MAX_LABELS_PER_METRIC {
      return Err(MetricLabelSetError::TooManyLabels);
    }
    self.labels.push(label);
    self.labels.sort_unstable_by_key(|label| label.key);
    Ok(())
  }

  /// Returns the labels in deterministic key order.
  pub fn iter(&self) -> impl ExactSizeIterator<Item = MetricLabel> + '_ {
    self.labels.iter().copied()
  }

  /// Returns the number of labels.
  #[must_use]
  pub fn len(&self) -> usize {
    self.labels.len()
  }

  /// Returns whether the set has no labels.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.labels.is_empty()
  }
}

/// Rejection returned while building a metric label set.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MetricLabelSetError {
  /// A key may occur only once.
  #[error("duplicate metric label key: {0:?}")]
  DuplicateKey(MetricLabelKey),
  /// The process-wide per-point label limit was exceeded.
  #[error("too many metric labels")]
  TooManyLabels,
}
