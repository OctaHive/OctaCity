//! Exact provider discovery and transport adapters for Decision Signals.
//!
//! The registry exposes only provider-neutral Factory contracts. JEV HTTP
//! payloads and credentials remain private to this infrastructure crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod jev;
mod registry;

pub use jev::{JevAdapterError, JevApiKey, JevBillingPolicy, JevDecisionSignalProvider};
pub use registry::{DecisionSignalProviderRegistry, DecisionSignalRegistryError};

#[cfg(test)]
mod tests;
