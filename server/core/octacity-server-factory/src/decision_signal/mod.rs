//! Provider-neutral Decision Signal contracts and deterministic consumption.

mod consumption;
mod contract;
mod provider;
mod question;

pub use consumption::*;
pub use contract::*;
pub use provider::{DecisionSignalProvider, DecisionSignalProviderFuture};
pub use question::*;

/// Maximum routing receipts that may be bound to one deterministic Decision.
pub const MAX_DECISION_SIGNAL_RECEIPTS: usize = 64;
