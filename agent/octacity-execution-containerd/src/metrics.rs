//! Decodes containerd cgroup-v2 resource-accounting payloads.

use super::*;

/// Subset of containerd's cgroup-v2 metrics used by the agent wire model.
#[derive(Clone, PartialEq, Message)]
pub(super) struct CgroupV2Metrics {
  #[prost(message, optional, tag = "2")]
  pub(super) cpu: Option<CpuStat>,
  #[prost(message, optional, tag = "4")]
  pub(super) memory: Option<MemoryStat>,
  #[prost(message, optional, tag = "6")]
  pub(super) io: Option<IoStat>,
}

/// Cumulative CPU accounting reported by cgroup v2 in microseconds.
#[derive(Clone, PartialEq, Message)]
pub(super) struct CpuStat {
  #[prost(uint64, tag = "1")]
  pub(super) usage_usec: u64,
}

/// Current and peak resident memory for the complete cgroup.
#[derive(Clone, PartialEq, Message)]
pub(super) struct MemoryStat {
  #[prost(uint64, tag = "32")]
  pub(super) usage: u64,
  #[prost(uint64, tag = "36")]
  pub(super) max_usage: u64,
}

/// Per-device block I/O accounting.
#[derive(Clone, PartialEq, Message)]
pub(super) struct IoStat {
  #[prost(message, repeated, tag = "1")]
  pub(super) usage: Vec<IoEntry>,
}

/// Read and written bytes for one block-device major/minor pair.
#[derive(Clone, PartialEq, Message)]
pub(super) struct IoEntry {
  #[prost(uint64, tag = "3")]
  pub(super) rbytes: u64,
  #[prost(uint64, tag = "4")]
  pub(super) wbytes: u64,
}

/// Rejects metrics payloads from an unexpected containerd runtime type.
pub(super) fn decode_cgroup_v2_metrics(value: &Any) -> Result<CgroupV2Metrics, ExecutionError> {
  if !value.type_url.ends_with(CGROUP_V2_METRICS_SUFFIX) {
    return Err(unavailable(format!(
      "containerd returned unsupported metrics type '{}'",
      value.type_url
    )));
  }
  CgroupV2Metrics::decode(value.value.as_slice())
    .map_err(|error| backend(format!("decode containerd cgroup-v2 metrics: {error}")))
}
