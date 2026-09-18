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
mod cancellation;
mod cycle;
mod reconcile;
mod retry;

pub use attempt::{AttemptEvent, AttemptState};
pub use build::{BuildEvent, BuildState};
pub use cancellation::{CancellationDecision, CancellationError, CancellationTransition, cancel_job_states};
pub use cycle::{OrchestrationEvent, OrchestrationState};
pub use reconcile::{
  JobGraphNode, JobGraphTransition, OrchestrationDecision, OrchestrationError, reconcile_cancelled_job_graph,
  reconcile_job_graph, validate_job_graph,
};
pub use retry::{RetryDecision, RetryDecisionError, decide_retry};
