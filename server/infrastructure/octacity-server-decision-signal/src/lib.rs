//! Provider-neutral exact Decision Signal discovery.
//! Transport implementations are separate crates selected by the server composition root.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod registry;
pub use registry::{DecisionSignalProviderRegistry, DecisionSignalRegistryError};
#[cfg(test)]
mod tests;
