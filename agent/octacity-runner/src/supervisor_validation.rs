//! Builds runner requests and validates protocol lifecycle values.

use octa_runner_protocol::{CacheSessionSpec, RemoteCacheSession, RunRequest, RunStatus};
use octacity_execution::{ExecutionExit, ExecutionPaths};
use octacity_protocol::ExecutionSpec;

use super::{RunnerCacheSession, RunnerSupervisionError, invalid, protocol};
use crate::protocol::{RunnerEvent, RunnerMessage};

/// Maps the signed, agent-level execution contract onto Octa's wire request.
/// Host paths come exclusively from the selected backend rather than from the
/// control-plane payload.
pub(super) fn build_run_request(
  spec: &ExecutionSpec,
  cache: Option<&RunnerCacheSession>,
  paths: &ExecutionPaths,
) -> Result<RunRequest, RunnerSupervisionError> {
  let cache = match (cache, paths.cache.as_ref()) {
    (None, None) => None,
    (Some(cache), Some(paths)) => {
      let remote = match (&cache.remote_endpoint, &paths.token_file) {
        (Some(endpoint), Some(token_file)) => Some(RemoteCacheSession {
          endpoint: endpoint.clone(),
          token_file: token_file.clone(),
          ca_certificate_file: paths.ca_certificate_file.clone(),
          request_timeout_seconds: cache.request_timeout_seconds,
          max_parallel_transfers: cache.max_parallel_transfers,
        }),
        (None, None) => None,
        _ => return invalid("cache endpoint and projected token path must be present together"),
      };
      Some(CacheSessionSpec {
        mode: cache.mode,
        namespace: cache.namespace.clone(),
        local_directory: paths.local_directory.clone(),
        local_capacity: cache.local_capacity,
        runtime: cache.runtime.clone(),
        remote,
      })
    }
    _ => return invalid("runner and backend cache projections must agree"),
  };
  Ok(RunRequest {
    workspace: paths.workspace.clone(),
    octafile: spec.octafile.as_ref().map(Into::into),
    data_dir: paths.data_dir.clone(),
    plugins_dir: paths.plugins_dir.clone(),
    plugin_lock: Some(paths.plugin_lock.clone()),
    secrets_profile: spec.secrets_profile.as_ref().map(Into::into),
    cache,
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
  })
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
pub(super) fn validate_event(
  event: &RunnerEvent,
  expected_schema: u16,
  previous: &mut Option<u64>,
) -> Result<(), RunnerSupervisionError> {
  if event.schema_version != expected_schema || event.timestamp.is_empty() {
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

#[cfg(test)]
mod tests {
  use std::{collections::BTreeMap, num::NonZeroUsize, path::PathBuf};

  use octa_cache_protocol::{
    CacheMode, Digest, DigestAlgorithm, LocalCacheCapacity, PlatformArchitecture, PlatformOs, RuntimeIdentity,
  };
  use octacity_execution::{ExecutionCachePaths, ExecutionPaths};

  use super::*;

  fn execution_spec() -> ExecutionSpec {
    ExecutionSpec {
      octafile: None,
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    }
  }

  fn paths() -> ExecutionPaths {
    ExecutionPaths {
      workspace: PathBuf::from("/workspace"),
      data_dir: PathBuf::from("/workspace/.octa"),
      plugins_dir: PathBuf::from("/opt/octa/plugins"),
      plugin_lock: PathBuf::from("/opt/octa/Octa.lock"),
      cache: Some(ExecutionCachePaths {
        local_directory: PathBuf::from("/var/cache/octa"),
        token_file: Some(PathBuf::from("/run/octa-cache/token")),
        ca_certificate_file: None,
      }),
    }
  }

  fn session() -> RunnerCacheSession {
    RunnerCacheSession {
      mode: CacheMode::ReadWrite,
      namespace: "project/main".to_owned(),
      local_capacity: LocalCacheCapacity::new(100, 90, 80).unwrap(),
      runtime: RuntimeIdentity::Native {
        os: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
        environment: Digest::from_hex(DigestAlgorithm::Blake3, &"a".repeat(64), 0).unwrap(),
      },
      remote_endpoint: Some("https://cache.example/v1".to_owned()),
      request_timeout_seconds: 10,
      max_parallel_transfers: NonZeroUsize::new(2).unwrap(),
    }
  }

  #[test]
  fn builds_one_local_and_remote_cache_session_without_copying_the_bearer() {
    let request = build_run_request(&execution_spec(), Some(&session()), &paths()).unwrap();
    assert!(!serde_json::to_string(&request).unwrap().contains("cache-secret"));
    let cache = request.cache.unwrap();
    assert_eq!(cache.local_directory, PathBuf::from("/var/cache/octa"));
    assert_eq!(cache.local_capacity.max_bytes, 100);
    let remote = cache.remote.unwrap();
    assert_eq!(remote.token_file, PathBuf::from("/run/octa-cache/token"));
  }

  #[test]
  fn rejects_disagreement_between_semantic_and_backend_cache_projections() {
    let mut projected = paths();
    projected.cache = None;
    assert!(build_run_request(&execution_spec(), Some(&session()), &projected).is_err());

    let mut projected = paths();
    projected.cache.as_mut().unwrap().token_file = None;
    assert!(build_run_request(&execution_spec(), Some(&session()), &projected).is_err());
  }
}
