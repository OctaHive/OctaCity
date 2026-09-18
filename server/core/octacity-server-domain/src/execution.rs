use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Runtime and isolation class shared by Project policy and Build configuration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeClass {
  /// Execute directly on the Agent host.
  Native,
  /// Execute in an OCI container sharing the Agent kernel.
  OciProcess,
  /// Execute an OCI workload behind a hypervisor boundary.
  OciHypervisor,
}

/// Aggregate artifact and report ceilings shared by policy and configuration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPolicy {
  /// Maximum number of artifacts produced by one Job.
  pub artifact_count: u32,
  /// Maximum aggregate artifact bytes produced by one Job.
  pub artifact_bytes: u64,
  /// Maximum number of reports produced by one Job.
  pub report_count: u32,
  /// Maximum aggregate report bytes produced by one Job.
  pub report_bytes: u64,
  /// Maximum bytes in one artifact or report.
  pub single_output_bytes: u64,
}

impl ArtifactPolicy {
  /// Verifies that counts, aggregate byte ceilings, and the per-output ceiling agree.
  pub fn validate(self) -> Result<(), ArtifactPolicyError> {
    let artifact_shape = (self.artifact_count == 0) == (self.artifact_bytes == 0);
    let report_shape = (self.report_count == 0) == (self.report_bytes == 0);
    if !artifact_shape
      || !report_shape
      || (self.artifact_count > 0 || self.report_count > 0) && self.single_output_bytes == 0
      || self.artifact_count > 0 && self.single_output_bytes > self.artifact_bytes
      || self.report_count > 0 && self.single_output_bytes > self.report_bytes
    {
      return Err(ArtifactPolicyError);
    }
    Ok(())
  }
}

/// The fields of an artifact policy do not form consistent ceilings.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("artifact and report ceilings are inconsistent")]
pub struct ArtifactPolicyError;
