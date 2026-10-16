//! Independent JEV transport for the provider-neutral Decision Signal contract.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod jev;
pub use jev::{JevAdapterError, JevApiKey, JevBillingPolicy, JevDecisionSignalProvider};
#[cfg(test)]
mod tests;
