//! Verifies, starts, and supervises the `octa-runner` process for OctaCity.
//!
//! The crate owns Octa release inventory, bounded async protocol framing, and
//! the runner lifecycle state machine. It depends only on the backend-neutral
//! execution port and wire contracts; it has no knowledge of agent
//! configuration, source acquisition, coordinator transport, or concrete
//! execution backends.

mod installation;
mod protocol;
mod supervisor;

pub use installation::{RunnerCapabilities, RunnerInstallation, RunnerInstallationError, RunnerPlugin};
pub use octa_runner_protocol::RunStatus;
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub use protocol::{MAX_RUNNER_OUTPUT_FRAME_BYTES, decode_message_frame};
pub use protocol::{RunnerEvent, RunnerMessage, RunnerProtocolError};
pub use supervisor::{
  RunnerCacheSession, RunnerCompletion, RunnerJobRequest, RunnerRedactions, RunnerStreamItem, RunnerSupervisionError,
  RunnerSupervisionPolicy, TerminationReason, supervise,
};
