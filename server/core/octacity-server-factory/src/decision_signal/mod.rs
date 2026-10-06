//! Provider-neutral Decision Signal contracts and deterministic consumption.

mod consumption;
mod contract;
mod provider;
mod question;

pub use consumption::*;
pub use contract::*;
pub use provider::{DecisionSignalProvider, DecisionSignalProviderFuture};
pub use question::*;
