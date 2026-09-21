//! Placement of already-ready Jobs onto compatible accepting Agents.
//!
//! Trigger evaluation, DAG traversal, and agent-side execution are deliberately
//! outside this module.
//!
//! The **Placement Scheduler** consumes already Ready Jobs and may create one
//! fenced [`LeaseState`] for a compatible accepting Agent. Pool admission and
//! drain are represented by [`PoolDrainState`]. See the [canonical glossary]
//! and [server ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod lease;
mod placement;
mod pool;

pub use lease::{LeaseEvent, LeaseState};
pub use placement::is_compatible;
pub use pool::{PoolDrainEvent, PoolDrainState, PoolDrainTransitionError};
