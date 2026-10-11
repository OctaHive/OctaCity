//! Versioned immutable contracts exchanged around one Factory model call.
//!
//! This facade preserves one public interface while keeping validation logic
//! local to the context, envelope and result records it protects.

mod common;
mod context;
mod envelope;
mod result;
mod retrieval;

pub use common::*;
pub use context::*;
pub use envelope::*;
pub use result::*;
pub use retrieval::*;
