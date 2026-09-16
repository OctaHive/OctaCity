//! Materializes one workspace through a previously verified source plugin.
//!
//! This module owns the dynamic side of source acquisition. It receives an
//! [`InstalledSourcePlugin`] selected by `SourcePluginRegistry`, starts a fresh
//! process with a cleared environment, checks
//! its `Hello` identity, sends one `Materialize` request, validates the ordered
//! JSONL lifecycle, enforces output and time limits, propagates cancellation,
//! and reaps the plugin and its descendants. A successful result is accepted
//! only when the plugin returns the exact requested immutable revision.
//!
//! Registry discovery, manifest trust, and executable digest verification stay
//! in `SourcePluginRegistry`. Creation and final cleanup of the workspace and
//! credential files belong to the higher-level job owner; this module only
//! passes their already resolved paths to the plugin and supervises their use.

use std::{collections::BTreeMap, path::PathBuf, process::Stdio, time::Duration};

use octacity_source_plugin::{
  MAX_SOURCE_FRAME_BYTES, MaterializeRequest, SOURCE_PLUGIN_PROTOCOL_VERSION, SourceCommand, SourceMessage,
  validate_request_id,
};
use processkit::ProcessGroup;
use thiserror::Error;
use tokio::{
  io::BufReader,
  process::Command,
  time::{Instant, sleep_until, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::registry::InstalledSourcePlugin;

mod process;

use process::*;

const MAX_PLUGIN_STDERR_BYTES: usize = 64 * 1024;
const MAX_LIFECYCLE_MESSAGES: usize = 4096;
const MAX_LIFECYCLE_BYTES: usize = MAX_SOURCE_FRAME_BYTES;

/// Host-side request with agent-resolved credential paths and destination.
#[derive(Debug)]
pub struct SourceMaterializationRequest {
  /// Unique identifier used to correlate all protocol frames.
  pub request_id: String,
  /// Existing empty directory the plugin may populate.
  pub destination: PathBuf,
  /// Exact immutable revision that must be returned by the plugin.
  pub revision: String,
  /// Optional human-readable branch or tag associated with the revision.
  pub reference: Option<String>,
  /// Provider-specific values copied from the signed job.
  pub parameters: BTreeMap<String, serde_json::Value>,
  /// Agent-created credential files keyed by logical handle.
  pub credential_files: BTreeMap<String, PathBuf>,
  /// Maximum workspace size the plugin must enforce while materializing.
  pub max_workspace_bytes: u64,
}

/// Verified terminal data returned by a source plugin.
#[derive(Debug, Eq, PartialEq)]
pub struct MaterializedSource {
  /// Exact immutable revision confirmed by the plugin.
  pub revision: String,
  /// Provider-specific provenance suitable for audit records.
  pub provenance: BTreeMap<String, String>,
  /// Ordered progress messages emitted during materialization.
  pub progress: Vec<String>,
  /// Ordered diagnostics emitted during materialization.
  pub diagnostics: Vec<String>,
}

#[derive(Debug, Error)]
/// Failure while validating or supervising a source-plugin process.
pub enum SourceHostError {
  #[error("invalid source-plugin request: {0}")]
  /// The host-side request is invalid before the plugin is started.
  Invalid(String),
  /// The source-plugin process could not be started.
  #[error("failed to start source plugin '{plugin}': {source}")]
  Spawn {
    /// Logical plugin name.
    plugin: String,
    /// Underlying process error.
    source: std::io::Error,
  },
  /// Communication with the running source plugin failed.
  #[error("source plugin '{plugin}' I/O failed: {source}")]
  Io {
    /// Logical plugin name.
    plugin: String,
    /// Underlying stream error.
    source: std::io::Error,
  },
  /// The source plugin emitted malformed JSON.
  #[error("source plugin '{plugin}' sent invalid JSON: {source}")]
  Json {
    /// Logical plugin name.
    plugin: String,
    /// JSON decoding error.
    source: Box<serde_json::Error>,
  },
  /// The source plugin violated message identity or lifecycle ordering.
  #[error("source plugin '{plugin}' violated its protocol: {message}")]
  Protocol {
    /// Logical plugin name.
    plugin: String,
    /// Description of the lifecycle violation.
    message: String,
  },
  /// The source plugin reported a provider-specific failure.
  #[error("source plugin '{plugin}' failed: {message}")]
  Plugin {
    /// Logical plugin name.
    plugin: String,
    /// Provider-reported failure message.
    message: String,
  },
  /// Materialization was cancelled by the agent.
  #[error("source plugin '{plugin}' was cancelled")]
  Cancelled {
    /// Logical plugin name.
    plugin: String,
  },
  /// Materialization exceeded its absolute deadline.
  #[error("source plugin '{plugin}' exceeded its execution timeout")]
  TimedOut {
    /// Logical plugin name.
    plugin: String,
  },
}

impl InstalledSourcePlugin {
  /// Materializes one exact source revision through this verified plugin.
  pub async fn materialize(
    &self,
    request: SourceMaterializationRequest,
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceHostError> {
    let plugin = self.manifest.name.clone();
    if operation_timeout.is_zero() || cancellation_grace.is_zero() {
      return Err(SourceHostError::Invalid(
        "operation and cancellation timeouts must be greater than zero".to_owned(),
      ));
    }
    let deadline = Instant::now()
      .checked_add(operation_timeout)
      .ok_or_else(|| SourceHostError::Invalid("operation timeout is too large".to_owned()))?;
    validate_request_id(&request.request_id).map_err(SourceHostError::Invalid)?;
    let wire_request = self.wire_request(&request)?;
    wire_request.validate().map_err(SourceHostError::Invalid)?;
    info!(plugin = %plugin, request_id = %request.request_id, "starting source materialization");

    // Plugins receive only explicit request fields and credential handles. In
    // particular, agent or service credentials must not leak through env vars.
    let mut command = Command::new(&self.executable);
    command
      .env_clear()
      .current_dir(
        self
          .executable
          .parent()
          .ok_or_else(|| SourceHostError::Invalid("source-plugin executable has no parent".to_owned()))?,
      )
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());
    let process_group = ProcessGroup::new().map_err(|source| SourceHostError::Spawn {
      plugin: plugin.clone(),
      source: std::io::Error::other(source),
    })?;
    let child = process_group.spawn(command).map_err(|source| SourceHostError::Spawn {
      plugin: plugin.clone(),
      source: std::io::Error::other(source),
    })?;
    let mut child = ChildGuard::new(child, process_group);
    let Some(mut stdin) = child.child.stdin.take() else {
      child.force_kill_and_wait().await;
      return Err(protocol(&plugin, "stdin was not piped"));
    };
    let Some(stdout) = child.child.stdout.take() else {
      child.force_kill_and_wait().await;
      return Err(protocol(&plugin, "stdout was not piped"));
    };
    let Some(stderr) = child.child.stderr.take() else {
      child.force_kill_and_wait().await;
      return Err(protocol(&plugin, "stderr was not piped"));
    };
    let mut stdout = BufReader::new(stdout);
    let mut stderr_task = Some(tokio::spawn(read_bounded(stderr, MAX_PLUGIN_STDERR_BYTES)));

    let result = async {
      let hello = tokio::select! {
        () = cancellation.cancelled() => return Err(SourceHostError::Cancelled { plugin: plugin.clone() }),
        result = tokio::time::timeout_at(deadline, read_message(&plugin, &mut stdout)) => {
          result
            .map_err(|_| SourceHostError::TimedOut { plugin: plugin.clone() })??
            .ok_or_else(|| protocol(&plugin, "stdout closed before hello"))?
        }
      };
    match hello {
      SourceMessage::Hello {
        protocol_version,
        plugin_name,
        plugin_version,
      } if protocol_version == SOURCE_PLUGIN_PROTOCOL_VERSION
        && plugin_name == self.manifest.name
        && plugin_version == self.manifest.version => {}
      _ => return Err(protocol(&plugin, "hello does not match the verified manifest")),
    }
    debug!(plugin = %plugin, request_id = %request.request_id, "validated source-plugin hello");

    let materialize = SourceCommand::Materialize {
      protocol_version: SOURCE_PLUGIN_PROTOCOL_VERSION,
      request_id: request.request_id.clone(),
      request: wire_request,
    };
    tokio::select! {
      () = cancellation.cancelled() => return Err(SourceHostError::Cancelled { plugin: plugin.clone() }),
      result = tokio::time::timeout_at(deadline, write_command(&plugin, &mut stdin, &materialize)) => {
        result.map_err(|_| SourceHostError::TimedOut { plugin: plugin.clone() })??;
      }
    }

    let operation_deadline = sleep_until(deadline);
    tokio::pin!(operation_deadline);
    let mut cancellation_deadline = None;
    let mut stop_reason = None;
    let mut accepted = false;
    let mut progress = Vec::new();
    let mut diagnostics = Vec::new();
    // Progress and diagnostic messages are bounded in aggregate as well as per
    // frame so a valid-looking plugin cannot exhaust agent memory over time.
    let mut lifecycle_messages = 0_usize;
    let mut lifecycle_bytes = 0_usize;

    // Once cancellation or timeout begins, keep reading only long enough for
    // the plugin to acknowledge it; the original stop reason remains final.
    loop {
      tokio::select! {
        message = read_message(&plugin, &mut stdout) => {
          let message = match message {
            Ok(Some(message)) => message,
            Ok(None) => {
              if let Some(reason) = stop_reason {
                child.force_kill_and_wait().await;
                return Err(stop_error(&plugin, reason));
              }
              return Err(protocol(&plugin, "stdout closed before a terminal message"));
            }
            Err(error) => {
              if let Some(reason) = stop_reason {
                child.force_kill_and_wait().await;
                return Err(stop_error(&plugin, reason));
              }
              return Err(error);
            }
          };
          match message {
            SourceMessage::Accepted { request_id } if request_id == request.request_id && !accepted => {
              accepted = true;
              debug!(plugin = %plugin, request_id = %request.request_id, "source plugin accepted request");
            },
            SourceMessage::Progress { request_id, message } if request_id == request.request_id && accepted => {
              record_lifecycle_message(&plugin, &message, &mut lifecycle_messages, &mut lifecycle_bytes)?;
              progress.push(message);
            }
            SourceMessage::Diagnostic { request_id, message } if request_id == request.request_id && accepted => {
              record_lifecycle_message(&plugin, &message, &mut lifecycle_messages, &mut lifecycle_bytes)?;
              diagnostics.push(message);
            }
            SourceMessage::Finished { request_id, revision, provenance }
              if request_id == request.request_id && accepted => {
                if let Some(reason) = stop_reason {
                  child.force_kill_and_wait().await;
                  return Err(stop_error(&plugin, reason));
                }
                if revision != request.revision {
                  return Err(protocol(&plugin, "terminal revision differs from the requested revision"));
                }
                finish_process(&plugin, &mut child, &mut stderr_task, cancellation_grace).await?;
                info!(plugin = %plugin, request_id = %request.request_id, revision = %revision, "source materialization finished");
                return Ok(MaterializedSource { revision, provenance, progress, diagnostics });
              }
            SourceMessage::Cancelled { request_id } if request_id == request.request_id && accepted => {
              if let Some(reason) = stop_reason {
                child.force_kill_and_wait().await;
                return Err(stop_error(&plugin, reason));
              }
              finish_process(&plugin, &mut child, &mut stderr_task, cancellation_grace).await?;
              return Err(stop_error(&plugin, StopReason::Cancelled));
            }
            SourceMessage::Error { request_id, message }
              if request_id.as_deref().is_none_or(|id| id == request.request_id) => {
                if let Some(reason) = stop_reason {
                  child.force_kill_and_wait().await;
                  return Err(stop_error(&plugin, reason));
                }
                finish_process(&plugin, &mut child, &mut stderr_task, cancellation_grace).await?;
                return Err(SourceHostError::Plugin { plugin, message });
              }
            _ => return Err(protocol(&plugin, "unexpected, duplicate, or incorrectly correlated message")),
          }
        }
        () = cancellation.cancelled(), if stop_reason.is_none() => {
          warn!(plugin = %plugin, request_id = %request.request_id, "cancelling source materialization");
          stop_reason = Some(StopReason::Cancelled);
          let deadline = Instant::now() + cancellation_grace;
          cancellation_deadline = Some(deadline);
          send_cancel_before(&plugin, &mut stdin, &request.request_id, deadline).await;
        }
        () = &mut operation_deadline, if stop_reason.is_none() => {
          warn!(plugin = %plugin, request_id = %request.request_id, "source materialization timed out");
          stop_reason = Some(StopReason::TimedOut);
          let deadline = Instant::now() + cancellation_grace;
          cancellation_deadline = Some(deadline);
          send_cancel_before(&plugin, &mut stdin, &request.request_id, deadline).await;
        }
        () = wait_for_deadline(cancellation_deadline), if stop_reason.is_some() => {
          if let Some(reason) = stop_reason {
            child.force_kill_and_wait().await;
            return Err(stop_error(&plugin, reason));
          }
        }
      }
      }
    }
    .await;
    child.force_kill_and_wait().await;
    if let Some(stderr_task) = stderr_task {
      let _ = timeout(cancellation_grace, stderr_task).await;
    }
    result
  }

  fn wire_request(&self, request: &SourceMaterializationRequest) -> Result<MaterializeRequest, SourceHostError> {
    let destination = utf8_path("destination", &request.destination)?;
    let credential_files = request
      .credential_files
      .iter()
      .map(|(name, path)| Ok((name.clone(), utf8_path("credential file", path)?)))
      .collect::<Result<_, SourceHostError>>()?;
    Ok(MaterializeRequest {
      destination,
      revision: request.revision.clone(),
      reference: request.reference.clone(),
      parameters: request.parameters.clone(),
      settings: self.manifest.settings.clone(),
      credential_files,
      max_workspace_bytes: request.max_workspace_bytes,
    })
  }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
