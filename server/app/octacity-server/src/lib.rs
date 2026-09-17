//! Composition root and process lifecycle for the OctaCity server.
//!
//! This crate is the only server crate allowed to select concrete adapters.
//! The initial runtime exposes only bounded liveness and readiness probes; it
//! deliberately registers no domain command or query routes.
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
pub use readiness::ReadinessSetupError;
pub use runtime::{ServerRuntime, ServerRuntimeError};
