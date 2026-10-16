//! Independent OpenAI Decisions transport for shadow-only finite-choice signals.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod openai;
pub use openai::{OpenAiDecisionSignalError, OpenAiDecisionSignalProvider};
#[cfg(test)]
mod tests;
