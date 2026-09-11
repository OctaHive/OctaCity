//! Builds runner requests and validates protocol lifecycle values.

use octa_runner_protocol::{RUNNER_EVENT_SCHEMA_VERSION, RunRequest, RunStatus};
use octacity_execution::{ExecutionExit, ExecutionPaths};
use octacity_protocol::ExecutionSpec;

use super::{RunnerSupervisionError, protocol};
use crate::protocol::{RunnerEvent, RunnerMessage};

/// Maps the signed, agent-level execution contract onto Octa's wire request.
/// Host paths come exclusively from the selected backend rather than from the
/// control-plane payload.
pub(super) fn build_run_request(spec: &ExecutionSpec, paths: &ExecutionPaths) -> RunRequest {
  RunRequest {
    workspace: paths.workspace.clone(),
    octafile: spec.octafile.as_ref().map(Into::into),
    data_dir: paths.data_dir.clone(),
    plugins_dir: paths.plugins_dir.clone(),
    plugin_lock: Some(paths.plugin_lock.clone()),
    secrets_profile: spec.secrets_profile.as_ref().map(Into::into),
    plugins: Vec::new(),
    default_plugin: None,
    commands: spec.commands.clone(),
    variables: spec.variables.clone(),
    arguments: spec.arguments.clone(),
    concurrency: spec.concurrency,
    parallel: spec.parallel,
    failfast: spec.failfast,
    dry: false,
    force: false,
    quiet: false,
    silence: None,
  }
}

/// Confirms that the live executable is the release already matched against
/// the signed job by `RunnerInstallation`.
pub(super) fn validate_hello(
  message: &RunnerMessage,
  expectation: &octacity_protocol::OctaSpec,
) -> Result<(), RunnerSupervisionError> {
  match message {
    RunnerMessage::Hello {
      protocol_version,
      octa_version,
      event_schema_version,
      plugin_protocol_version,
    } if *protocol_version == expectation.runner_protocol
      && octa_version == &expectation.version
      && *event_schema_version == expectation.event_schema
      && *plugin_protocol_version == expectation.plugin_protocol =>
    {
      Ok(())
    }
    RunnerMessage::Hello { .. } => Err(protocol("hello does not match the signed Octa requirement")),
    _ => Err(protocol("first message was not hello")),
  }
}

/// Enforces the event schema and a gap-free monotonic sequence before an event
/// can be forwarded to the server.
pub(super) fn validate_event(event: &RunnerEvent, previous: &mut Option<u64>) -> Result<(), RunnerSupervisionError> {
  if event.schema_version != RUNNER_EVENT_SCHEMA_VERSION || event.timestamp.is_empty() {
    return Err(protocol("runner event has an invalid schema version or timestamp"));
  }
  if !matches!(event.category.as_str(), "execution" | "diagnostic" | "document") {
    return Err(protocol("runner event has an unknown category"));
  }
  if let Some(previous_sequence) = *previous {
    let expected = previous_sequence
      .checked_add(1)
      .ok_or_else(|| protocol("runner event sequence overflowed"))?;
    if event.sequence != expected {
      return Err(protocol(format!(
        "runner event sequence {} followed {previous_sequence}, expected {expected}",
        event.sequence
      )));
    }
  }
  *previous = Some(event.sequence);
  Ok(())
}

/// Cross-checks the structured terminal status against the process exit code.
pub(super) fn validate_exit(status: RunStatus, exit: ExecutionExit) -> Result<(), RunnerSupervisionError> {
  let expected = match status {
    RunStatus::Succeeded => 0,
    RunStatus::Failed => 1,
    RunStatus::Cancelled => 130,
  };
  if exit.code == Some(expected) {
    Ok(())
  } else {
    Err(protocol(format!(
      "terminal status does not match process exit status {:?}",
      exit.code
    )))
  }
}
