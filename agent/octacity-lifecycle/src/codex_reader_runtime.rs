//! Concrete CodeX MCP reader composition; neutral Job execution knows no provider format.
use crate::{
  AgentToolActionBroker, ConfinedProtectedReadBackend, ProtectedReadChannel, ProtectedReadDispatcher,
  ProtectedReadInventory, ProtectedReadRecipe,
};
use async_trait::async_trait;
use octacity_coordinator::{Registration, ToolActionCoordinator};
use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionError, ExecutionOs, ExecutionProgram, ExecutionTarget,
  FactoryExecutionLayout, FactoryExecutionPartition, NetworkAccess, RunningExecution, StartExecution,
};
use octacity_job::{FactoryPreflightError, FactoryToolExecutionOwner, FactoryToolExecutionScope};
use octacity_protocol::{FACTORY_SOURCE_ROOT, FactoryPermissionSetV3, LeaseAssignment};
use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};
use tokio_util::sync::CancellationToken;
const SELECTOR: &str = "OCTA_CODEX_TOOL_AUTHORIZER";
const CAPABILITY: &str = "codex.blocking-pre-tool-authorization.v1";
const PORT: u32 = 5001;
const SOCKET_NAME: &str = "r";
fn unavailable() -> FactoryPreflightError {
  FactoryPreflightError::ToolAuthorizationUnavailable
}
fn execution_error() -> ExecutionError {
  ExecutionError::Unavailable("controlled reader execution is unavailable".to_owned())
}

/// Concrete immutable worker and CodeX bridge pins, independently installed by the operator.
#[derive(Clone)]
pub struct FactoryProtectedReadInstallation {
  /// Actual standalone confined worker, verified before each physical use.
  pub worker: ExecutionProgram,
  /// Actual provider-local MCP/hook executable, never selected by model input.
  pub bridge: ExecutionProgram,
  /// Exact bridge bytes frozen at installation.
  pub bridge_sha256: String,
  /// Actual guest binary platform, not the Agent host platform.
  pub platform: String,
  /// Exact installed MCP read hook, including the configured server name.
  pub hook_tool_name: String,
  /// Immutable read-only operation and its separate physical allocation.
  pub recipe: ProtectedReadRecipe,
}
impl ProtectedReadInventory for FactoryProtectedReadInstallation {
  fn installed_rooted_reader(&self) -> Option<ProtectedReadRecipe> {
    Some(self.recipe.clone())
  }
  fn installed_rooted_reader_program(&self) -> Option<ExecutionProgram> {
    Some(self.worker.clone())
  }
}
impl FactoryProtectedReadInstallation {
  fn verify(&self) -> Result<(), FactoryPreflightError> {
    let template = self
      .recipe
      .action(FACTORY_SOURCE_ROOT)
      .map_err(|_| unavailable())?
      .into_proposal();
    let server = self
      .hook_tool_name
      .strip_prefix("mcp__")
      .and_then(|name| name.strip_suffix("__read_file"))
      .filter(|name| !name.is_empty() && !name.chars().any(char::is_control))
      .ok_or_else(unavailable)?;
    if server.len() > 128
      || self.worker.executable == self.bridge.executable
      || self.worker.plugin_projection.is_some()
      || self.bridge.plugin_projection.is_some()
      || !self.worker.external_executables.is_empty()
      || !self.bridge.external_executables.is_empty()
    {
      return Err(unavailable());
    }
    for (program, digest) in [
      (&self.worker, &template.executable.sha256),
      (&self.bridge, &self.bridge_sha256),
    ] {
      let entry = program
        .executable
        .strip_prefix(&program.release_root)
        .map_err(|_| unavailable())?;
      let verified = octacity_runner::verify_standalone_program(&program.release_root, entry, digest, &self.platform)
        .map_err(|_| unavailable())?;
      if verified != *program {
        return Err(unavailable());
      }
    }
    Ok(())
  }
}

/// Installed CodeX reader wiring without any active lease or provider credential.
#[derive(Clone)]
pub struct FactoryProtectedReadRuntime {
  installation: FactoryProtectedReadInstallation,
  local: FactoryPermissionSetV3,
  state_root: PathBuf,
}
impl FactoryProtectedReadRuntime {
  /// Freezes an independently verified installation and local permission ceiling.
  pub fn new(
    installation: FactoryProtectedReadInstallation,
    local: FactoryPermissionSetV3,
    state_root: PathBuf,
  ) -> Result<Self, FactoryPreflightError> {
    installation.verify()?;
    local.validate().map_err(|_| unavailable())?;
    if !state_root.is_absolute()
      || state_root.canonicalize().map_err(|_| unavailable())? != state_root
      || !socket_address_fits(&state_root)
    {
      return Err(unavailable());
    }
    octacity_private_fs::validate_private_access(&state_root).map_err(|_| unavailable())?;
    octacity_private_fs::validate_trusted_directory_chain(&state_root).map_err(|_| unavailable())?;
    Ok(Self {
      installation,
      local,
      state_root,
    })
  }
  /// Selects the concrete coordinator once; lease binding remains in-memory.
  pub fn with_coordinator(
    self,
    coordinator: Arc<dyn ToolActionCoordinator>,
  ) -> Arc<dyn crate::FactoryToolExecutionProvider> {
    Arc::new(ReaderProvider {
      runtime: self,
      coordinator,
    })
  }
  /// Binds one current registration/lease without publishing its authority to the harness.
  pub fn bind(
    &self,
    coordinator: Arc<dyn ToolActionCoordinator>,
    registration: Registration,
    lease: LeaseAssignment,
  ) -> Result<Arc<dyn FactoryToolExecutionOwner>, FactoryPreflightError> {
    registration.validate().map_err(|_| unavailable())?;
    if lease.job_id.is_empty() || lease.attempt == 0 || lease.lease_id.is_empty() || lease.fencing_token.is_empty() {
      return Err(unavailable());
    }
    Ok(Arc::new(LeasedReader {
      runtime: self.clone(),
      coordinator,
      registration,
      lease,
    }))
  }
}
struct ReaderProvider {
  runtime: FactoryProtectedReadRuntime,
  coordinator: Arc<dyn ToolActionCoordinator>,
}
impl crate::FactoryToolExecutionProvider for ReaderProvider {
  fn bind(&self, registration: &Registration, lease: &LeaseAssignment) -> Arc<dyn FactoryToolExecutionOwner> {
    Arc::new(LeasedReader {
      runtime: self.runtime.clone(),
      coordinator: self.coordinator.clone(),
      registration: registration.clone(),
      lease: lease.clone(),
    })
  }
}
struct LeasedReader {
  runtime: FactoryProtectedReadRuntime,
  coordinator: Arc<dyn ToolActionCoordinator>,
  registration: Registration,
  lease: LeaseAssignment,
}
impl FactoryToolExecutionOwner for LeasedReader {
  fn prepare(&self, scope: FactoryToolExecutionScope) -> Result<Arc<dyn ExecutionBackend>, FactoryPreflightError> {
    self.registration.validate().map_err(|_| unavailable())?;
    if self.lease.lease_id.is_empty() || self.lease.fencing_token.is_empty() {
      return Err(unavailable());
    }
    self.runtime.installation.verify()?;
    if scope.job_id != self.lease.job_id
      || scope.attempt != self.lease.attempt
      || scope.control.plugin != "codex"
      || scope.control.capability != CAPABILITY
      || scope.cancellation.is_cancelled()
    {
      return Err(unavailable());
    }
    let ExecutionTarget::Oci { platform, .. } = &scope.target else {
      return Err(FactoryPreflightError::BackendCapability);
    };
    let expected = match (platform.os, platform.architecture) {
      (ExecutionOs::Linux, ExecutionArchitecture::Arm64) => "linux-aarch64",
      (ExecutionOs::Linux, ExecutionArchitecture::Amd64) => "linux-x86_64",
      _ => return Err(FactoryPreflightError::BackendCapability),
    };
    if expected != self.runtime.installation.platform {
      return Err(FactoryPreflightError::BackendCapability);
    }
    let tools_root = scope
      .backend
      .factory_tool_projection(&scope.target)
      .ok_or(FactoryPreflightError::BackendCapability)?;
    // Concrete proposals receive their exact signed and local checks in the
    // broker. A synthetic root read would require authority the real operation
    // may deliberately lack (for example, an exact single-file command grant).
    let remaining = scope.deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
      return Err(unavailable());
    }
    let broker = Arc::new(
      AgentToolActionBroker::new(
        self.coordinator.clone(),
        self.registration.clone(),
        self.lease.clone(),
        scope.permissions,
        self.runtime.local.clone(),
        remaining,
        scope.cancellation.clone(),
      )
      .map_err(|_| unavailable())?,
    );
    scope.control.response_limits.validate().map_err(|_| unavailable())?;
    Ok(Arc::new(ControlledBackend {
      installation: self.runtime.installation.clone(),
      state_root: self.runtime.state_root.clone(),
      inner: scope.backend,
      tools_root,
      deadline: scope.deadline,
      cancellation: scope.cancellation,
      broker,
      limits: scope.control.response_limits,
    }))
  }
}
struct ControlledBackend {
  installation: FactoryProtectedReadInstallation,
  state_root: PathBuf,
  inner: Arc<dyn ExecutionBackend>,
  tools_root: PathBuf,
  deadline: tokio::time::Instant,
  cancellation: CancellationToken,
  broker: Arc<AgentToolActionBroker>,
  limits: octacity_protocol::FactoryToolResponseLimitsV3,
}
#[async_trait]
impl ExecutionBackend for ControlledBackend {
  async fn start(
    &self,
    program: &ExecutionProgram,
    mut main: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    self.installation.verify().map_err(|_| execution_error())?;
    if self.cancellation.is_cancelled() || cancellation.is_cancelled() {
      return Err(ExecutionError::Cancelled);
    }
    let source = main.factory.as_ref().ok_or_else(execution_error)?.source.clone();
    let template = self
      .installation
      .recipe
      .action(FACTORY_SOURCE_ROOT)
      .map_err(|_| execution_error())?
      .into_proposal();
    let main_layout = main.factory.as_mut().ok_or_else(execution_error)?;
    if !main_layout.source_writable_paths.is_empty() {
      return Err(execution_error());
    }
    main_layout.source_visible = false;
    main_layout.source_read_only = true;
    let job = source.parent().ok_or_else(execution_error)?;
    let mut helper = main.clone();
    helper.execution_id.push_str("-reader");
    helper.workload_identity = None;
    helper.cache = None;
    helper.network = NetworkAccess::Disabled;
    helper.cpu_millis = template.resources.cpu_millis;
    helper.memory_bytes = template.resources.memory_bytes;
    helper.writable_disk_bytes = template.resources.disk_bytes;
    helper.max_duration = Duration::from_millis(template.resources.elapsed_millis);
    let roots = [
      job.join("controlled-reader-scratch"),
      job.join("controlled-reader-output"),
      job.join("controlled-reader-protected"),
    ];
    for root in &roots {
      octacity_private_fs::create_private_directory(root).map_err(|_| execution_error())?;
    }
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&roots[2], std::fs::Permissions::from_mode(0o500)).map_err(|_| execution_error())?;
    helper.data_dir = roots[0].join("data");
    octacity_private_fs::create_private_directory(&helper.data_dir).map_err(|_| execution_error())?;
    helper.factory = Some(FactoryExecutionLayout {
      workspace_anchor: false,
      disk_accounting: octacity_execution::ExecutionDiskAccounting::WholeJob,
      protected_inputs: roots[2].clone(),
      source,
      source_visible: true,
      source_read_only: true,
      source_writable_paths: vec![],
      tool_channel: None,
      scratch: roots[0].clone(),
      output: roots[1].clone(),
      process_limit: template.resources.process_count,
    });
    let partition = FactoryExecutionPartition::new(main, helper)?;
    let (mut main, reader) =
      ConfinedProtectedReadBackend::prepare(&self.installation, self.inner.clone(), partition, self.deadline)
        .map_err(|_| execution_error())?;
    let reader = Arc::new(reader);
    let dispatcher = Arc::new(
      ProtectedReadDispatcher::new(
        self.broker.clone(),
        reader.clone(),
        self.limits.per_call_bytes,
        self.limits,
      )
      .map_err(|_| execution_error())?,
    );
    let control_root = self.state_root.join(uuid::Uuid::new_v4().simple().to_string());
    octacity_private_fs::create_private_directory(&control_root).map_err(|_| execution_error())?;
    let mut directory = OwnedDirectory(Some(control_root.clone()));
    let channel = ProtectedReadChannel::bind(dispatcher, control_root.join(SOCKET_NAME), PORT)?.with_owned_parent();
    directory.0 = None;
    main.factory.as_mut().ok_or_else(execution_error)?.tool_channel = Some(channel.endpoint().clone());
    let prepared = (|| {
      let mut program = program.clone();
      if program
        .external_executables
        .insert(SELECTOR.to_owned(), self.installation.bridge.executable.clone())
        .is_some()
      {
        return Err(execution_error());
      }
      let guest = program
        .external_executable_projections()
        .into_iter()
        .find(|item| item.selector == SELECTOR)
        .ok_or_else(execution_error)?
        .destination(&self.tools_root);
      let scratch = &main.factory.as_ref().ok_or_else(execution_error)?.scratch;
      let data = scratch.join(".octacity");
      if data.exists() {
        let metadata = std::fs::symlink_metadata(&data).map_err(|_| execution_error())?;
        if !metadata.file_type().is_dir() || data.canonicalize().map_err(|_| execution_error())? != data {
          return Err(execution_error());
        }
        octacity_private_fs::validate_private_access(&data).map_err(|_| execution_error())?;
        octacity_private_fs::validate_trusted_owner(&data).map_err(|_| execution_error())?;
      } else {
        octacity_private_fs::create_private_directory(&data).map_err(|_| execution_error())?;
      }
      let mut recipe = template.clone();
      recipe.arguments.clear();
      recipe.paths.clear();
      let config = serde_json::to_vec(
        &serde_json::json!({"port":PORT,"hook_tool_name":self.installation.hook_tool_name,"recipe":recipe}),
      )
      .map_err(|_| execution_error())?;
      write_new(&data.join("controlled-reader.json"), &config)?;
      let home = scratch.join(".codex");
      octacity_private_fs::create_private_directory(&home).map_err(|_| execution_error())?;
      let server = self
        .installation
        .hook_tool_name
        .strip_prefix("mcp__")
        .and_then(|value| value.strip_suffix("__read_file"))
        .ok_or_else(execution_error)?;
      let config = format!(
        "[mcp_servers.{}]\ncommand = {}\nargs = [\"mcp\"]\n",
        serde_json::to_string(server).map_err(|_| execution_error())?,
        serde_json::to_string(guest.to_str().ok_or_else(execution_error)?).map_err(|_| execution_error())?
      );
      write_new(&home.join("config.toml"), config.as_bytes())?;
      Ok(program)
    })();
    let started = match prepared {
      Ok(program) => self.inner.start(&program, main, cancellation).await,
      Err(error) => Err(error),
    };
    match started {
      Ok(main) => Ok(channel.attach(reader.attach(main))),
      Err(error) => {
        let channel_cleanup = channel.shutdown().await;
        let reader_cleanup = reader.shutdown().await;
        match (channel_cleanup, reader_cleanup) {
          (Ok(()), Ok(())) => Err(error),
          (Err(cleanup), _) | (_, Err(cleanup)) => Err(error.with_cleanup(cleanup)),
        }
      }
    }
  }
  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    self.inner.cleanup_orphans().await
  }
}
fn socket_address_fits(root: &Path) -> bool {
  let capacity = std::mem::size_of::<libc::sockaddr_un>() - std::mem::offset_of!(libc::sockaddr_un, sun_path);
  // Every execution adds one compact UUID directory and one fixed socket name.
  // Leave the terminating NUL required by the host Unix address ABI.
  root
    .join(uuid::Uuid::nil().simple().to_string())
    .join(SOCKET_NAME)
    .as_os_str()
    .as_encoded_bytes()
    .len()
    < capacity
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ExecutionError> {
  use std::io::Write as _;
  use std::os::unix::fs::OpenOptionsExt as _;
  let mut file = std::fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .open(path)
    .map_err(|_| execution_error())?;
  file.write_all(bytes).map_err(|_| execution_error())
}
struct OwnedDirectory(Option<PathBuf>);
impl Drop for OwnedDirectory {
  fn drop(&mut self) {
    if let Some(path) = &self.0 {
      let _ = std::fs::remove_dir(path);
    }
  }
}
