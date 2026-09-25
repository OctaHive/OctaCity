//! Verified adapter files and bounded one-operation process execution.
//!
//! Provider-specific registry manifests and wire protocols stay in their
//! owning adapters. This module owns the security-sensitive filesystem and
//! process mechanics that must remain identical for every hosted adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod process;
mod registry;

pub use process::{
  HostError, HostFailureClass, ProcessRequest, cancel_request_id, classify_host_telemetry, execute_process,
};
pub use registry::{
  AdapterRegistry, MAX_EXECUTABLE_BYTES, RegistryAdapter, RegistryError, VerifiedExecutable, read_manifest,
  verify_executable,
};
