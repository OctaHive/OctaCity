//! Build, Attempt, and DAG transition decisions.
//!
//! The orchestrator advances persisted graph state; it never executes
//! repository-controlled build code.
//!
//! The **Orchestrator** consumes persisted Job outcomes, advances
//! [`BuildState`] and [`AttemptState`], and makes dependency-unblocked Jobs
//! ready. It neither creates Builds from Triggers nor selects Agents. See the
//! [canonical glossary] and [server ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod attempt;
mod build;
mod cycle;
mod dag;

pub use attempt::{AttemptEvent, AttemptState};
pub use build::{BuildEvent, BuildState};
pub use cycle::{OrchestrationEvent, OrchestrationState};
pub use dag::{DagDecisionError, DependencyObservation, newly_ready_jobs};
