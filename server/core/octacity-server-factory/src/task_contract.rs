//! Versioned immutable contracts exchanged around one Factory model call.
//!
//! This facade preserves one public interface while keeping validation logic
//! local to the context, envelope, result, and handoff records it protects.

mod common;
mod completion;
mod context;
mod envelope;
mod handoff;
mod result;
mod retrieval;

pub use common::*;
pub use completion::*;
pub use context::*;
pub use envelope::*;
pub use handoff::*;
pub use result::*;
pub use retrieval::*;
