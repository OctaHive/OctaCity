//! Composition root and process lifecycle for the OctaCity server.
//!
//! This crate is the only server crate allowed to select concrete adapters.
//! The initial runtime exposes bounded health probes and operational metadata;
//! it deliberately registers no domain command or query routes.
//!
//! Crate ownership, dependency direction, and public protocol seams are mapped
//! in the [server architecture guide].
//!
//! [server architecture guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]

mod config;
mod readiness;
mod runtime;

pub use config::{ServerConfig, ServerConfigError};
pub use runtime::{
  DurableWorkerError, LogSearchMaintenanceError, RuntimeAssemblyError, ServerRuntime, ServerRuntimeError,
  rebuild_log_search,
};
