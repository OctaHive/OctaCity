//! Selects an OCI engine by guest platform and effective isolation tier.
//!
//! Job orchestration sees one `OciBackend`. Concrete engines such as
//! containerd and Microsandbox remain replaceable details and cannot weaken a
//! signed `hypervisor` request to process isolation.

#![warn(missing_docs)]

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
  /// Read-only root used for independently verified operator executables.
  /// Absence disables the controlled reader composition for this engine.
  fn factory_tool_projection_root(&self) -> Option<std::path::PathBuf> {
    None
  }

  /// Returns the exact combinations this configured engine can enforce.
  fn capabilities(&self) -> Vec<OciCapability>;

  /// Whether this engine projects the trusted private guest-to-host stream.
  /// This transport capability does not qualify controlled tool execution.
  fn supports_private_tool_channel(&self) -> bool {
    false
  }

  /// Whether admission, enforcement and sampling use this execution's actual
  /// writable mounts when a Factory partition requests that technical scope.
  /// This resource capability does not qualify controlled tool execution.
  fn supports_owned_writable_mount_accounting(&self) -> bool {
    false
  }

  /// Whether this engine provides the empty read-only parent workspace while
  /// exposing only individually admitted child mounts.
  fn supports_factory_workspace_anchor(&self) -> bool {
    false
  }

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

/// Returns the fixed executable projection only when this exact engine provides
/// both the private channel and independent writable-allocation accounting.
/// Installation of a lease-owned tool controller remains a separate requirement.
pub fn qualified_factory_tool_projection(engine: &dyn OciEngine) -> Option<std::path::PathBuf> {
  if !engine.supports_private_tool_channel() || !engine.supports_owned_writable_mount_accounting() {
    return None;
  }
  engine.factory_tool_projection_root().filter(|path| path.is_absolute())
}

#[async_trait]
impl ExecutionBackend for OciBackend {
  fn factory_tool_projection(&self, target: &ExecutionTarget) -> Option<std::path::PathBuf> {
    let ExecutionTarget::Oci {
      platform,
      isolation: OciIsolation::Hypervisor,
      ..
    } = target
    else {
      return None;
    };
    let engine = self.engines.get(*self.routes.get(&OciCapability {
      platform: *platform,
      isolation: OciIsolation::Hypervisor,
    })?)?;
    qualified_factory_tool_projection(engine.as_ref())
  }

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
    if request
      .factory
      .as_ref()
      .is_some_and(|factory| factory.tool_channel.is_some())
      && !engine.supports_private_tool_channel()
    {
      return Err(ExecutionError::Unavailable(
        "OCI engine does not provide a private tool-action stream".to_owned(),
      ));
    }
    if request.factory.as_ref().is_some_and(|factory| {
      factory.disk_accounting == octacity_execution::ExecutionDiskAccounting::OwnedWritableMounts
    }) && !engine.supports_owned_writable_mount_accounting()
    {
      return Err(ExecutionError::Unavailable(
        "OCI engine does not provide owned writable-mount accounting".to_owned(),
      ));
    }
    if !engine.supports_factory_workspace_anchor() {
      request.require_unanchored_factory_workspace()?;
    }
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
      cache: None,
      factory: None,
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
      plugin_projection: Some(octacity_execution::ExecutionPluginProjection {
        directory: PathBuf::from("/release/plugins"),
        lock: PathBuf::from("/release/plugins.lock"),
      }),
      external_executable_names: std::collections::BTreeMap::new(),
      external_executables: std::collections::BTreeMap::new(),
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

  #[cfg(unix)]
  fn factory_execution_request(
    disk_accounting: octacity_execution::ExecutionDiskAccounting,
  ) -> (tempfile::TempDir, StartExecution, RunnerProgram) {
    use std::os::unix::fs::PermissionsExt as _;
    let (temporary, mut request, runner) = execution_request(OciIsolation::Hypervisor);
    std::fs::set_permissions(&request.workspace_root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let job = request.workspace_root.join("job");
    std::fs::create_dir(&job).unwrap();
    let source = job.join("source");
    let scratch = job.join("scratch");
    let output = job.join("output");
    let protected = job.join("protected");
    for path in [&source, &scratch, &output, &protected] {
      std::fs::create_dir(path).unwrap();
    }
    std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o500)).unwrap();
    request.workspace = source.clone();
    request.data_dir = scratch.clone();
    request.factory = Some(octacity_execution::FactoryExecutionLayout {
      workspace_anchor: false,
      disk_accounting,
      protected_inputs: protected,
      source,
      source_visible: false,
      source_read_only: true,
      source_writable_paths: vec![],
      tool_channel: None,
      scratch,
      output,
      process_limit: 4,
    });
    (temporary, request, runner)
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn refuses_a_private_tool_channel_before_starting_an_unqualified_engine() {
    use std::os::unix::{fs::PermissionsExt as _, net::UnixListener};
    let engine = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Hypervisor),
      starts: AtomicUsize::new(0),
      cleanup_error: None,
    });
    let backend = OciBackend::new(vec![engine.clone()]).unwrap();
    let (_temporary, mut request, runner) =
      factory_execution_request(octacity_execution::ExecutionDiskAccounting::WholeJob);
    let socket = request.workspace_root.join("tool.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    request.factory.as_mut().unwrap().tool_channel =
      Some(octacity_execution::FactoryToolChannel::new(socket, 5001).unwrap());
    request
      .validate()
      .expect("valid channel reaches the engine capability gate");
    let error = backend
      .start(&runner, request, CancellationToken::new())
      .await
      .err()
      .unwrap();
    assert!(matches!(error, ExecutionError::Unavailable(_)));
    assert_eq!(engine.starts.load(Ordering::SeqCst), 0);
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn refuses_owned_writable_accounting_before_starting_an_unqualified_engine() {
    let engine = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Hypervisor),
      starts: AtomicUsize::new(0),
      cleanup_error: None,
    });
    let backend = OciBackend::new(vec![engine.clone()]).unwrap();
    let (_temporary, request, runner) =
      factory_execution_request(octacity_execution::ExecutionDiskAccounting::OwnedWritableMounts);
    request.validate().unwrap();
    let error = backend
      .start(&runner, request, CancellationToken::new())
      .await
      .err()
      .unwrap();
    assert!(matches!(error, ExecutionError::Unavailable(_)), "{error:?}");
    assert_eq!(engine.starts.load(Ordering::SeqCst), 0);
  }
  #[cfg(unix)]
  #[tokio::test]
  async fn refuses_an_anchored_workspace_before_starting_an_unqualified_engine() {
    let engine = Arc::new(FailingEngine {
      capability: capability(OciIsolation::Hypervisor),
      starts: AtomicUsize::new(0),
      cleanup_error: None,
    });
    let backend = OciBackend::new(vec![engine.clone()]).unwrap();
    let (_temporary, mut request, runner) =
      factory_execution_request(octacity_execution::ExecutionDiskAccounting::WholeJob);
    request.factory.as_mut().unwrap().workspace_anchor = true;
    assert!(matches!(
      backend.start(&runner, request, CancellationToken::new()).await,
      Err(ExecutionError::Unavailable(_))
    ));
    assert_eq!(engine.starts.load(Ordering::SeqCst), 0);
  }
}

#[cfg(test)]
mod controlled_projection_tests {
  use super::*;
  use octacity_execution::{ExecutionArchitecture, ExecutionOs};
  use std::path::PathBuf;
  struct ReaderEngine {
    platform: ExecutionPlatform,
    enabled: bool,
    projection_root: PathBuf,
  }
  #[async_trait]
  impl OciEngine for ReaderEngine {
    fn capabilities(&self) -> Vec<OciCapability> {
      vec![OciCapability {
        platform: self.platform,
        isolation: OciIsolation::Hypervisor,
      }]
    }
    fn supports_private_tool_channel(&self) -> bool {
      self.enabled
    }
    fn supports_owned_writable_mount_accounting(&self) -> bool {
      self.enabled
    }
    fn factory_tool_projection_root(&self) -> Option<PathBuf> {
      Some(self.projection_root.clone())
    }
    async fn start(
      &self,
      _: &RunnerProgram,
      _: StartExecution,
      _: CancellationToken,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      panic!("qualification must precede execution")
    }
    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      Ok(())
    }
  }
  #[test]
  fn controlled_tool_projection_requires_the_exact_selected_route_and_all_physical_controls() {
    let temporary = tempfile::tempdir().unwrap();
    let projection_root = temporary.path().join("readonly-tools");
    assert!(projection_root.is_absolute());
    let qualified = ExecutionPlatform {
      os: ExecutionOs::Linux,
      architecture: ExecutionArchitecture::Arm64,
    };
    let unqualified = ExecutionPlatform {
      os: ExecutionOs::Linux,
      architecture: ExecutionArchitecture::Amd64,
    };
    let backend = OciBackend::new(vec![
      Arc::new(ReaderEngine {
        platform: qualified,
        enabled: true,
        projection_root: projection_root.clone(),
      }),
      Arc::new(ReaderEngine {
        platform: unqualified,
        enabled: false,
        projection_root: projection_root.clone(),
      }),
    ])
    .unwrap();
    let target = |platform, isolation| ExecutionTarget::Oci {
      reference: format!("image@sha256:{}", "ab".repeat(32)),
      platform,
      isolation,
    };
    assert_eq!(
      backend.factory_tool_projection(&target(qualified, OciIsolation::Hypervisor)),
      Some(projection_root)
    );
    assert_eq!(
      backend.factory_tool_projection(&target(unqualified, OciIsolation::Hypervisor)),
      None
    );
    assert_eq!(
      backend.factory_tool_projection(&target(qualified, OciIsolation::Process)),
      None
    );
  }
}
