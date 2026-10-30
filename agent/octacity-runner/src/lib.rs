//! Verifies, starts, and supervises the `octa-runner` process for OctaCity.
//!
//! The crate owns Octa release inventory, bounded async protocol framing, and
//! the runner lifecycle state machine. It depends only on the backend-neutral
//! execution port and wire contracts; it has no knowledge of agent
//! configuration, source acquisition, coordinator transport, or concrete
//! execution backends.

#![warn(missing_docs)]

mod installation;
mod private_variables;
mod protocol;
mod supervisor;

mod private_files;
pub use private_files::{RunnerPrivateCredentialGuard, RunnerPrivateJsonFile, RunnerSensitiveValues};

pub use private_variables::{AuthorizedRunnerPrivateVariables, MAX_MANAGED_OCTAFILE_BYTES, RunnerPrivateVariables};

pub use installation::{
  ConfiguredExternalExecutable, RunnerCapabilities, RunnerInstallation, RunnerInstallationError, RunnerPlugin,
  VerifiedExternalExecutable, verify_standalone_program,
};
pub use octa_runner_protocol::{RUNNER_EVENT_SCHEMA_VERSION, RUNNER_PROTOCOL_VERSION, RunStatus};
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub use protocol::{MAX_RUNNER_OUTPUT_FRAME_BYTES, decode_message_frame};
pub use protocol::{RunnerEvent, RunnerMessage, RunnerProtocolError};
pub use supervisor::{
  RunnerCacheSession, RunnerCompletion, RunnerJobRequest, RunnerRedactions, RunnerStreamItem, RunnerSupervisionError,
  RunnerSupervisionPolicy, TerminationReason, supervise, supervise_with_private_variables,
};
