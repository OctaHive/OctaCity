//! Real Octa runner and lifecycle fixtures for the Phase 6 service contract.
//!
//! The host backend is intentionally test-only: it exercises the production
//! runner protocol while the separate privileged contract covers actual
//! Native and microVM isolation.

use super::*;

pub(super) struct Phase6Source {
  pub(super) vault_endpoint: String,
}

#[async_trait]
impl SourceMaterializer for Phase6Source {
  async fn materialize(
    &self,
    requirement: &SourceSpec,
    request: SourceMaterializationRequest,
    _operation_timeout: Duration,
    _cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceError> {
    if cancellation.is_cancelled() {
      return Err(
        SourceHostError::Cancelled {
          plugin: requirement.provider.clone(),
        }
        .into(),
      );
    }
    let identity = request
      .destination
      .parent()
      .expect("the lifecycle workspace always has a job root")
      .join("identity/token");
    fs::write(
      request.destination.join("secrets.yml"),
      format!(
        "version: 1\nproviders:\n  application:\n    type: vault\n    address: {}\n    mount: secret\n    kv_version: 2\n    auth:\n      type: jwt\n      role: octacity-ci\n      jwt_path: {}\n      mount: auth/octacity-jwt\n",
        serde_json::to_string(&self.vault_endpoint).unwrap(),
        serde_json::to_string(&identity).unwrap(),
      ),
    )
    .map_err(|source| SourceHostError::Io {
      plugin: requirement.provider.clone(),
      source,
    })?;
    fs::write(
      request.destination.join("Octafile.yml"),
      r#"version: 1
vars:
  VAULT_SECRET:
    secret:
      provider: application
      key: phase6
      field: token
tasks:
  build:
    shell: mkdir -p dist reports && printf application-bytes > dist/application.bin && printf opaque-report-bytes > reports/analysis.acme && cat ../identity/token && echo "{{ VAULT_SECRET }}"
    artifacts:
      - name: application
        path: dist/application.bin
        content_type: application/octet-stream
    reports:
      - name: analysis
        path: reports/analysis.acme
        format: vendor/acme-v7
"#,
    )
    .map_err(|source| SourceHostError::Io {
      plugin: requirement.provider.clone(),
      source,
    })?;
    Ok(MaterializedSource {
      revision: requirement.revision.clone(),
      provenance: BTreeMap::new(),
      progress: Vec::new(),
      diagnostics: Vec::new(),
    })
  }
}

/// Test-only Native adapter that exercises the real runner without requiring
/// cgroup delegation in the service-backed contract workflow.
pub(super) struct HostRunnerBackend {
  pub(super) captured: Arc<Mutex<Vec<u8>>>,
  pub(super) allowed_network_host: String,
}

struct HostRunnerExecution {
  child: Child,
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  reaped: bool,
}

#[async_trait]
impl ExecutionBackend for HostRunnerBackend {
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    runner.validate()?;
    request.validate()?;
    if cancellation.is_cancelled() {
      return Err(ExecutionError::Cancelled);
    }
    if !matches!(request.root, ExecutionTarget::Native { .. }) {
      return Err(ExecutionError::Invalid(
        "phase-six host backend requires Native execution".to_owned(),
      ));
    }
    if !matches!(
      &request.network,
      octacity_execution::NetworkAccess::Restricted { allowed_hosts }
        if allowed_hosts == std::slice::from_ref(&self.allowed_network_host)
    ) {
      return Err(ExecutionError::Invalid(
        "phase-six contract requires a restricted single-host Vault policy".to_owned(),
      ));
    }
    let mut child = Command::new(&runner.executable)
      .current_dir(&request.workspace)
      // The workflow environment contains Vault and MinIO administrator
      // credentials. The job must prove it can obtain only its application
      // secret through the mounted workload identity, never ambient CI state.
      .env_clear()
      .env("PATH", required("PATH"))
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true)
      .spawn()
      .map_err(ExecutionError::Io)?;
    let stdout = capture_reader(
      child.stdout.take().ok_or(ExecutionError::IoTaken)?,
      self.captured.clone(),
    );
    let stderr = capture_reader(
      child.stderr.take().ok_or(ExecutionError::IoTaken)?,
      self.captured.clone(),
    );
    let io = ExecutionIo {
      stdin: Box::pin(child.stdin.take().ok_or(ExecutionError::IoTaken)?) as ExecutionWriter,
      stdout: Box::pin(stdout) as ExecutionReader,
      stderr: Box::pin(stderr) as ExecutionReader,
    };
    Ok(Box::new(HostRunnerExecution {
      child,
      io: Some(io),
      paths: ExecutionPaths {
        workspace: request.workspace,
        data_dir: request.data_dir,
        plugins_dir: runner.plugins_dir.clone(),
        plugin_lock: runner.plugin_lock.clone(),
        cache: None,
      },
      reaped: false,
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    Ok(())
  }
}

fn capture_reader(
  mut input: impl AsyncRead + Unpin + Send + 'static,
  captured: Arc<Mutex<Vec<u8>>>,
) -> tokio::io::DuplexStream {
  let (mut output, reader) = tokio::io::duplex(64 * 1024);
  tokio::spawn(async move {
    let mut buffer = [0_u8; 8192];
    loop {
      let read = match input.read(&mut buffer).await {
        Ok(0) | Err(_) => return,
        Ok(read) => read,
      };
      captured.lock().unwrap().extend_from_slice(&buffer[..read]);
      if output.write_all(&buffer[..read]).await.is_err() {
        return;
      }
    }
  });
  reader
}

#[async_trait]
impl RunningExecution for HostRunnerExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    Ok(ResourceUsage::default())
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    if self.reaped {
      return Err(ExecutionError::Reaped);
    }
    let status = self.child.wait().await.map_err(ExecutionError::Io)?;
    self.reaped = true;
    Ok(ExecutionExit { code: status.code() })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    if !self.reaped {
      self.child.kill().await.map_err(ExecutionError::Io)?;
    }
    Ok(())
  }

  async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
    if !self.reaped {
      self.child.kill().await.map_err(ExecutionError::Io)?;
      self.child.wait().await.map_err(ExecutionError::Io)?;
      self.reaped = true;
    }
    Ok(())
  }
}

#[derive(Default)]
pub(super) struct Phase6Coordinator {
  pub(super) events: Mutex<Vec<AttemptEventEnvelope>>,
  pub(super) completions: Mutex<Vec<CompleteLeaseRequest>>,
}

#[async_trait]
impl CoordinatorClient for Phase6Coordinator {
  async fn register(
    &self,
    _inventory: &AgentInventory,
    _cancellation: CancellationToken,
  ) -> Result<Registration, CoordinatorError> {
    unreachable!()
  }

  async fn acquire_lease(
    &self,
    _registration: &Registration,
    _wait: Duration,
    _lease_safety_margin: Duration,
    _accept_jobs: bool,
    _cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError> {
    unreachable!()
  }

  async fn heartbeat(
    &self,
    _registration: &Registration,
    lease: &LeaseAssignment,
    _snapshot: &HostSnapshot,
    _capacity: &HostCapacity,
    _lease_safety_margin: Duration,
    _cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError> {
    Ok(HeartbeatDirective::Continue {
      expires_at: lease.expires_at,
    })
  }

  async fn append_events(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    events: &[AttemptEventEnvelope],
    _cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError> {
    self.events.lock().unwrap().extend_from_slice(events);
    Ok(AppendEventsResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "phase6-events".to_owned(),
      acknowledged_sequence: events.last().expect("event batches are non-empty").stream_sequence,
    })
  }

  async fn complete_lease(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    completion: &CompleteLeaseRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    self.completions.lock().unwrap().push(completion.clone());
    Ok(())
  }
}

#[async_trait]
impl octacity_coordinator::CacheSessionCoordinator for Phase6Coordinator {
  async fn begin_cache_session(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _request: &octacity_protocol::BeginCacheSessionRequest,
    _cancellation: CancellationToken,
  ) -> Result<octacity_protocol::BeginCacheSessionResponse, CoordinatorError> {
    unreachable!("phase-six fixture does not enable caching")
  }

  async fn revoke_cache_session(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _request: &octacity_protocol::RevokeCacheSessionRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    unreachable!("phase-six fixture does not enable caching")
  }
}

pub(super) struct InspectingPublisher {
  pub(super) inner: Arc<dyn OutputPublisher>,
  pub(super) state_root: PathBuf,
  pub(super) work_root: PathBuf,
  pub(super) forbidden: Vec<String>,
  pub(super) inspected: Arc<AtomicBool>,
}

#[async_trait]
impl OutputPublisher for InspectingPublisher {
  async fn freeze(
    &self,
    request: FreezeOutputs<'_>,
    cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, octacity_output::OutputError> {
    let forbidden = self.forbidden.iter().map(String::as_str).collect::<Vec<_>>();
    let identity = request.workspace.parent().unwrap().join("identity");
    assert!(
      !identity.exists(),
      "workload identity remained during output publication"
    );
    assert_secret_absent(
      "runner terminal results",
      &serde_json::to_vec(request.results).unwrap(),
      &forbidden,
    );
    assert_tree_excludes(&self.state_root, &forbidden);
    assert_tree_excludes(&self.work_root, &forbidden);
    let frozen = self.inner.freeze(request, cancellation).await?;
    assert_tree_excludes(&self.state_root, &forbidden);
    assert_tree_excludes(&self.work_root, &forbidden);
    Ok(frozen)
  }

  async fn publish(
    &self,
    request: PublishOutputs<'_>,
    cancellation: CancellationToken,
  ) -> Result<(), octacity_output::OutputError> {
    self.inner.publish(request, cancellation).await?;
    let forbidden = self.forbidden.iter().map(String::as_str).collect::<Vec<_>>();
    assert_tree_excludes(&self.state_root, &forbidden);
    assert_tree_excludes(&self.work_root, &forbidden);
    self.inspected.store(true, Ordering::SeqCst);
    Ok(())
  }
}

pub(super) fn phase6_installation(root: &FilePath) -> RunnerInstallation {
  let source_runner = required_path("OCTACITY_PHASE6_OCTA_RUNNER");
  let source_plugins = required_path("OCTACITY_PHASE6_OCTA_PLUGINS_DIR");
  let source_shell = required_plugin(&source_plugins, "shell");
  let source_tpl = required_plugin(&source_plugins, "tpl");
  let release = root.join("release");
  let plugins = release.join("plugins");
  fs::create_dir_all(&plugins).unwrap();
  let runner = release.join(source_runner.file_name().unwrap());
  let shell = plugins.join(source_shell.file_name().unwrap());
  let tpl = plugins.join(source_tpl.file_name().unwrap());
  copy_release_file(&source_runner, &runner);
  copy_release_file(&source_shell, &shell);
  copy_release_file(&source_tpl, &tpl);
  let runner_digest = file_sha256(&runner);
  let shell_digest = file_sha256(&shell);
  let tpl_digest = file_sha256(&tpl);
  let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
  let capabilities = runner_capabilities(&runner, &platform);
  let plugin_protocol = capabilities.plugin_protocols[0];
  let plugin_version = capabilities.octa_version.clone();
  let lock = release.join("Octa.lock");
  fs::write(
    &lock,
    format!(
      "version: 1\nplugins:\n  shell:\n    version: '{}'\n    protocol: {plugin_protocol}\n    platforms: [{platform}]\n    entrypoint: {}\n    sha256: {shell_digest}\n    capabilities: [shell]\n    source: phase6\n  tpl:\n    version: '{}'\n    protocol: {plugin_protocol}\n    platforms: [{platform}]\n    entrypoint: {}\n    sha256: {tpl_digest}\n    capabilities: []\n    source: phase6\n",
      capabilities.octa_version,
      shell.file_name().unwrap().to_string_lossy(),
      capabilities.octa_version,
      tpl.file_name().unwrap().to_string_lossy(),
    ),
  )
  .unwrap();
  let plugins_inventory = BTreeMap::from([
    (
      "shell".to_owned(),
      RunnerPlugin {
        version: plugin_version.clone(),
        protocol: plugin_protocol,
        platforms: vec![platform.clone()],
        executable: shell,
        sha256: shell_digest,
        capabilities: vec!["shell".to_owned()],
      },
    ),
    (
      "tpl".to_owned(),
      RunnerPlugin {
        version: plugin_version,
        protocol: plugin_protocol,
        platforms: vec![platform],
        executable: tpl,
        sha256: tpl_digest,
        capabilities: Vec::new(),
      },
    ),
  ]);
  RunnerInstallation {
    root: release,
    executable: runner,
    plugins_dir: plugins,
    default_plugin_lock: lock,
    sha256: runner_digest,
    capabilities,
    plugins: plugins_inventory,
  }
}

fn required_plugin(directory: &FilePath, name: &str) -> PathBuf {
  [
    format!("octa_plugin_{name}"),
    format!("octa-plugin-{name}"),
    format!("octa_plugin_{name}.exe"),
    format!("octa-plugin-{name}.exe"),
  ]
  .into_iter()
  .map(|candidate| directory.join(candidate))
  .find(|path| path.is_file())
  .unwrap_or_else(|| panic!("the phase-six {name} plugin must exist in OCTACITY_PHASE6_OCTA_PLUGINS_DIR"))
}

fn runner_capabilities(runner: &FilePath, platform: &str) -> RunnerCapabilities {
  let output = std::process::Command::new(runner).output().unwrap();
  let hello: serde_json::Value = serde_json::from_slice(
    output
      .stdout
      .split(|byte| *byte == b'\n')
      .find(|line| !line.is_empty())
      .expect("octa-runner must emit its hello before reading input"),
  )
  .unwrap();
  assert_eq!(hello["type"], "hello");
  RunnerCapabilities {
    octa_version: hello["octa_version"].as_str().unwrap().to_owned(),
    runner_protocols: vec![hello["protocol_version"].as_u64().unwrap() as u16],
    event_schemas: vec![hello["event_schema_version"].as_u64().unwrap() as u16],
    plugin_protocols: vec![hello["plugin_protocol_version"].as_u64().unwrap() as u16],
    octafile_versions: vec![1],
    platform: platform.to_owned(),
    features: Vec::new(),
    build_commit: None,
  }
}

fn copy_release_file(source: &FilePath, destination: &FilePath) {
  if fs::hard_link(source, destination).is_err() {
    fs::copy(source, destination).unwrap();
  }
}

fn file_sha256(path: &FilePath) -> String {
  let mut file = fs::File::open(path).unwrap();
  let mut digest = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let read = file.read(&mut buffer).unwrap();
    if read == 0 {
      return format!("{:x}", digest.finalize());
    }
    digest.update(&buffer[..read]);
  }
}

pub(super) fn phase6_spec(installation: &RunnerInstallation, vault_host: &str) -> JobSpecV1 {
  let now = unix_now();
  let digest = "0".repeat(64);
  let runner_protocol = installation.capabilities.runner_protocols[0];
  let event_schema = installation.capabilities.event_schemas[0];
  let plugin_protocol = installation.capabilities.plugin_protocols[0];
  JobSpecV1 {
    protocol_version: AGENT_PROTOCOL_VERSION,
    job_id: "job-1".to_owned(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 300,
    source: SourceSpec {
      provider: "phase6".to_owned(),
      plugin_version: "1.0.0".to_owned(),
      plugin_sha256: digest,
      revision: "phase6-revision".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
    },
    octa: OctaSpec {
      version: installation.capabilities.octa_version.clone(),
      runner_sha256: installation.sha256.clone(),
      runner_protocol,
      event_schema,
      plugin_protocol,
      plugin_digests: installation
        .plugins
        .iter()
        .map(|(name, plugin)| (name.clone(), plugin.sha256.clone()))
        .collect(),
    },
    execution: ExecutionSpec {
      octafile: Some("Octafile.yml".to_owned()),
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: Some("secrets.yml".to_owned()),
    },
    runtime: RuntimeSpec {
      target: RuntimeTarget::Native {
        platform: host_platform(),
      },
      cpu_millis: 1000,
      memory_bytes: 256 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024,
      timeout_seconds: 60,
      network: NetworkPolicy::Restricted {
        allowed_hosts: vec![vault_host.to_owned()],
      },
      workload_identity_profile: Some("ci".to_owned()),
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 1,
      artifact_bytes: 1024,
      report_count: 1,
      report_bytes: 1024,
      single_output_bytes: 1024,
    },
  }
}

fn host_platform() -> PlatformSpec {
  PlatformSpec {
    os: match std::env::consts::OS {
      "linux" => PlatformOs::Linux,
      "windows" => PlatformOs::Windows,
      "macos" => PlatformOs::Macos,
      other => panic!("unsupported phase-six host OS {other}"),
    },
    architecture: match std::env::consts::ARCH {
      "x86_64" => PlatformArchitecture::Amd64,
      "aarch64" => PlatformArchitecture::Arm64,
      other => panic!("unsupported phase-six host architecture {other}"),
    },
  }
}

pub(super) fn phase6_capacity() -> HostCapacity {
  HostCapacity {
    logical_cpu_count: 2,
    total_memory_bytes: 1024 * 1024 * 1024,
    work_disk_total_bytes: 4 * 1024 * 1024,
    state_disk_total_bytes: 4 * 1024 * 1024,
    virtualization_available: false,
  }
}

pub(super) fn phase6_snapshot(capacity: &HostCapacity) -> HostSnapshot {
  HostSnapshot {
    available_cpu_millis: 2000,
    available_memory_bytes: capacity.total_memory_bytes,
    work_disk_free_bytes: capacity.work_disk_total_bytes,
    state_disk_free_bytes: capacity.state_disk_total_bytes,
    active_job: None,
    backends: Vec::new(),
  }
}
