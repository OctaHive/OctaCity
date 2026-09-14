//! Selects an OCI engine by guest platform and effective isolation tier.
//!
//! Job orchestration sees one `OciBackend`. Concrete engines such as
//! containerd and Microsandbox remain replaceable details and cannot weaken a
//! signed `hypervisor` request to process isolation.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_execution::{
  ExecutionBackend, ExecutionError, ExecutionPlatform, ExecutionTarget, OciIsolation, RunnerProgram, RunningExecution,
  StartExecution,
};
use tokio_util::sync::CancellationToken;

/// Exact guest platform and isolation boundary supplied by one OCI engine.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OciCapability {
  /// Exact guest platform supplied by the engine.
  pub platform: ExecutionPlatform,
  /// Effective isolation boundary supplied by the engine.
  pub isolation: OciIsolation,
}

/// Concrete OCI lifecycle implementation hidden behind [`OciBackend`].
#[async_trait]
pub trait OciEngine: Send + Sync {
  /// Returns the exact combinations this configured engine can enforce.
  fn capabilities(&self) -> Vec<OciCapability>;

  /// Starts an OCI execution or rolls back every partially created resource.
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError>;

  /// Removes abandoned resources owned by this agent and engine.
  async fn cleanup_orphans(&self) -> Result<(), ExecutionError>;
}

/// Top-level OCI backend that performs deterministic, fail-closed dispatch.
pub struct OciBackend {
  engines: Vec<Arc<dyn OciEngine>>,
  routes: BTreeMap<OciCapability, usize>,
}

impl OciBackend {
  /// Builds deterministic routes and rejects missing or overlapping engines.
  pub fn new(engines: Vec<Arc<dyn OciEngine>>) -> Result<Self, ExecutionError> {
    if engines.is_empty() {
      return Err(ExecutionError::Invalid(
        "OCI backend requires at least one configured engine".to_owned(),
      ));
    }

    let mut routes = BTreeMap::new();
    for (index, engine) in engines.iter().enumerate() {
      let capabilities = engine.capabilities();
      if capabilities.is_empty() {
        return Err(ExecutionError::Invalid(
          "an OCI engine must advertise at least one capability".to_owned(),
        ));
      }
      for capability in capabilities {
        if routes.insert(capability, index).is_some() {
          return Err(ExecutionError::Invalid(format!(
            "multiple OCI engines advertise {capability:?}"
          )));
        }
      }
    }
    Ok(Self { engines, routes })
  }

  /// Returns every platform and isolation pair accepted by this backend.
  pub fn capabilities(&self) -> impl Iterator<Item = OciCapability> + '_ {
    self.routes.keys().copied()
  }
}

#[async_trait]
impl ExecutionBackend for OciBackend {
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    request.validate()?;
    let ExecutionTarget::Oci {
      platform, isolation, ..
    } = &request.root
    else {
      return Err(ExecutionError::Invalid(
        "OCI backend requires an OCI execution root".to_owned(),
      ));
    };
    let capability = OciCapability {
      platform: *platform,
      isolation: *isolation,
    };
    let engine = self
      .routes
      .get(&capability)
      .and_then(|index| self.engines.get(*index))
      .ok_or_else(|| ExecutionError::Unavailable(format!("no OCI engine provides {capability:?}")))?;
    engine.start(runner, request, cancellation).await
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    let mut failures = Vec::new();
    for engine in &self.engines {
      if let Err(error) = engine.cleanup_orphans().await {
        failures.push(error.to_string());
      }
    }
    if failures.is_empty() {
      Ok(())
    } else {
      Err(ExecutionError::Backend(failures.join("; ")))
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
  };

  use octacity_execution::{ExecutionArchitecture, ExecutionOs, NetworkAccess};

  use super::*;

  struct FailingEngine {
    capability: OciCapability,
    starts: AtomicUsize,
    cleanup_error: Option<&'static str>,
  }

  #[async_trait]
  impl OciEngine for FailingEngine {
    fn capabilities(&self) -> Vec<OciCapability> {
      vec![self.capability]
    }

    async fn start(
      &self,
      _runner: &RunnerProgram,
      _request: StartExecution,
      _cancellation: CancellationToken,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      self.starts.fetch_add(1, Ordering::SeqCst);
      Err(ExecutionError::Backend("selected engine".to_owned()))
    }

    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      match self.cleanup_error {
        Some(message) => Err(ExecutionError::Backend(message.to_owned())),
        None => Ok(()),
      }
    }
  }

  struct EmptyEngine;

  #[async_trait]
  impl OciEngine for EmptyEngine {
    fn capabilities(&self) -> Vec<OciCapability> {
      Vec::new()
    }

    async fn start(
      &self,
      _runner: &RunnerProgram,
      _request: StartExecution,
      _cancellation: CancellationToken,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      panic!("an engine without capabilities must never be started")
    }

    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      Ok(())
    }
  }

  fn capability(isolation: OciIsolation) -> OciCapability {
    OciCapability {
      platform: ExecutionPlatform {
        os: ExecutionOs::Linux,
        architecture: ExecutionArchitecture::Amd64,
      },
      isolation,
    }
  }

  fn execution_request(isolation: OciIsolation) -> (tempfile::TempDir, StartExecution, RunnerProgram) {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::create_dir(temporary.path().join("data")).unwrap();
    let workspace = temporary.path().canonicalize().unwrap();
    let request = StartExecution {
      execution_id: "job-1".to_owned(),
      workspace_root: workspace.clone(),
      workspace: workspace.clone(),
      data_dir: workspace.join("data"),
      workload_identity: None,
      cpu_millis: 1000,
      memory_bytes: 1024,
      writable_disk_bytes: 1024,
      max_duration: Duration::from_secs(1),
      root: ExecutionTarget::Oci {
        reference: format!("example/build@sha256:{}", "0".repeat(64)),
        platform: capability(isolation).platform,
        isolation,
      },
      network: NetworkAccess::Disabled,
    };
    let runner = RunnerProgram {
      release_root: PathBuf::from("/release"),
      executable: PathBuf::from("/release/octa-runner"),
      plugins_dir: PathBuf::from("/release/plugins"),
      plugin_lock: PathBuf::from("/release/plugins.lock"),
    };
    (temporary, request, runner)
  }

  #[test]
  fn rejects_missing_and_empty_engines() {
    assert!(OciBackend::new(Vec::new()).is_err());
    assert!(OciBackend::new(vec![Arc::new(EmptyEngine)]).is_err());
  }

  #[test]
  fn rejects_ambiguous_routes() {
    let first = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Process),
      starts: AtomicUsize::new(0),
      cleanup_error: None,
    });
    let second = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Process),
      starts: AtomicUsize::new(0),
      cleanup_error: None,
    });

    assert!(OciBackend::new(vec![first, second]).is_err());
  }

  #[tokio::test]
  async fn dispatches_only_to_the_exact_isolation_tier() {
    let engine = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Hypervisor),
      starts: AtomicUsize::new(0),
      cleanup_error: None,
    });
    let backend = OciBackend::new(vec![engine.clone()]).unwrap();
    let (_temporary, request, runner) = execution_request(OciIsolation::Process);

    let error = backend
      .start(&runner, request, CancellationToken::new())
      .await
      .err()
      .unwrap();
    assert!(matches!(error, ExecutionError::Unavailable(_)));
    assert_eq!(engine.starts.load(Ordering::SeqCst), 0);

    let (_temporary, request, runner) = execution_request(OciIsolation::Hypervisor);
    let error = backend
      .start(&runner, request, CancellationToken::new())
      .await
      .err()
      .unwrap();
    assert!(matches!(error, ExecutionError::Backend(_)));
    assert_eq!(engine.starts.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn rejects_host_roots_and_aggregates_cleanup_failures() {
    let first = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Process),
      starts: AtomicUsize::new(0),
      cleanup_error: Some("process cleanup"),
    });
    let second = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Hypervisor),
      starts: AtomicUsize::new(0),
      cleanup_error: Some("hypervisor cleanup"),
    });
    let backend = OciBackend::new(vec![first, second]).unwrap();
    assert_eq!(backend.capabilities().count(), 2);

    let (_temporary, mut request, runner) = execution_request(OciIsolation::Process);
    request.root = ExecutionTarget::Native {
      platform: capability(OciIsolation::Process).platform,
    };
    assert!(matches!(
      backend.start(&runner, request, CancellationToken::new()).await,
      Err(ExecutionError::Invalid(_))
    ));

    let error = backend.cleanup_orphans().await.unwrap_err().to_string();
    assert!(error.contains("process cleanup"));
    assert!(error.contains("hypervisor cleanup"));
  }
}
