//! Starts and owns one containerd task after its rootfs snapshot is prepared.

use super::*;

/// Fully resolved inputs required to create a container and its initial task.
///
/// Image resolution and snapshot preparation happen before this boundary, so
/// this stage owns only container/task creation and their subsequent cleanup.
pub(super) struct PreparedExecution<'a> {
  pub client: &'a Client,
  pub config: &'a ContainerdEngineConfig,
  pub runner: &'a RunnerProgram,
  pub request: &'a StartExecution,
  pub id: &'a str,
  pub snapshot_key: &'a str,
  pub labels: HashMap<String, String>,
  pub rootfs: Vec<containerd_client::types::Mount>,
  pub image_environment: &'a [String],
  pub io_directory: &'a Path,
  pub deadline: Instant,
  pub cancellation: &'a CancellationToken,
}

/// Creates and starts one task without resetting the caller's absolute deadline.
pub(super) async fn start_prepared(prepared: PreparedExecution<'_>) -> Result<ContainerdExecution, ExecutionError> {
  let PreparedExecution {
    client,
    config,
    runner,
    request,
    id,
    snapshot_key,
    labels,
    rootfs,
    image_environment,
    io_directory,
    deadline,
    cancellation,
  } = prepared;
  let guest_paths = guest_paths(runner, request)?;
  let spec = oci_spec(config, runner, request, &guest_paths, image_environment, id)?;
  let container = Container {
    id: id.to_owned(),
    labels,
    image: match &request.root {
      ExecutionTarget::Oci { reference, .. } => reference.clone(),
      ExecutionTarget::Native { .. } => return Err(invalid("containerd requires an OCI execution target")),
    },
    runtime: Some(Runtime {
      name: config.runtime.clone(),
      options: None,
    }),
    spec: Some(Any {
      type_url: OCI_SPEC_TYPE_URL.to_owned(),
      value: serde_json::to_vec(&spec).map_err(|error| backend(format!("serialize OCI runtime spec: {error}")))?,
    }),
    snapshotter: config.snapshotter.clone(),
    snapshot_key: snapshot_key.to_owned(),
    ..Default::default()
  };
  // Create resources from longest-lived to shortest-lived. The caller owns
  // rollback until this function returns a complete execution handle.
  grpc_before(
    deadline,
    Some(cancellation),
    "create containerd container",
    client.containers().create(namespaced(
      CreateContainerRequest {
        container: Some(container),
      },
      &config.namespace,
    )?),
  )
  .await?;

  let io = ContainerIo::create(io_directory)?;
  grpc_before(
    deadline,
    Some(cancellation),
    "create containerd task",
    client.tasks().create(namespaced(
      CreateTaskRequest {
        container_id: id.to_owned(),
        rootfs,
        stdin: io.stdin_path.to_string_lossy().into_owned(),
        stdout: io.stdout_path.to_string_lossy().into_owned(),
        stderr: io.stderr_path.to_string_lossy().into_owned(),
        terminal: false,
        checkpoint: None,
        options: None,
        runtime_path: String::new(),
      },
      &config.namespace,
    )?),
  )
  .await?;
  grpc_before(
    deadline,
    Some(cancellation),
    "start containerd task",
    client.tasks().start(namespaced(
      StartRequest {
        container_id: id.to_owned(),
        exec_id: String::new(),
      },
      &config.namespace,
    )?),
  )
  .await?;

  info!(container_id = id, "started containerd OCI process execution");
  Ok(ContainerdExecution {
    channel: client.channel(),
    namespace: config.namespace.clone(),
    snapshotter: config.snapshotter.clone(),
    container_id: id.to_owned(),
    snapshot_key: snapshot_key.to_owned(),
    io_directory: io_directory.to_owned(),
    io: Some(io.into_execution_io()?),
    paths: guest_paths,
    filesystem_root: request.workspace_root.clone(),
    started: Instant::now(),
    disk_peak_bytes: filesystem_usage(&request.workspace_root)?,
    cleanup_timeout: config.cleanup_timeout,
    reaped: false,
  })
}

/// Exclusive owner of all containerd and filesystem resources for one job.
pub(super) struct ContainerdExecution {
  channel: Channel,
  namespace: String,
  snapshotter: String,
  container_id: String,
  snapshot_key: String,
  io_directory: PathBuf,
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  filesystem_root: PathBuf,
  started: Instant,
  disk_peak_bytes: u64,
  cleanup_timeout: Duration,
  reaped: bool,
}

#[async_trait]
impl RunningExecution for ContainerdExecution {
  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    let response = Client::from(self.channel.clone())
      .tasks()
      .metrics(namespaced(
        MetricsRequest {
          filters: vec![format!("id=={}", self.container_id)],
        },
        &self.namespace,
      )?)
      .await
      .map_err(|error| grpc("read containerd task metrics", error))?
      .into_inner();
    let metric = response
      .metrics
      .into_iter()
      .find(|metric| metric.id == self.container_id)
      .ok_or_else(|| backend("containerd returned no metrics for the running task"))?;
    let data = metric
      .data
      .ok_or_else(|| backend("containerd returned empty task metrics"))?;
    let metrics = decode_cgroup_v2_metrics(&data)?;
    let disk_current_bytes = filesystem_usage(&self.filesystem_root)?;
    self.disk_peak_bytes = self.disk_peak_bytes.max(disk_current_bytes);
    Ok(ResourceUsage {
      elapsed_ms: elapsed_millis(self.started),
      cpu_time_ms: metrics.cpu.as_ref().map_or(0, |cpu| cpu.usage_usec / 1000),
      memory_current_bytes: metrics.memory.as_ref().map_or(0, |memory| memory.usage),
      memory_peak_bytes: metrics.memory.as_ref().map_or(0, |memory| memory.max_usage),
      disk_current_bytes,
      disk_peak_bytes: self.disk_peak_bytes,
      io_read_bytes: metrics
        .io
        .as_ref()
        .map_or(0, |io| io.usage.iter().map(|entry| entry.rbytes).sum()),
      io_written_bytes: metrics
        .io
        .as_ref()
        .map_or(0, |io| io.usage.iter().map(|entry| entry.wbytes).sum()),
      network_received_bytes: None,
      network_transmitted_bytes: None,
    })
  }

  async fn close_input(&mut self) -> Result<(), ExecutionError> {
    Client::from(self.channel.clone())
      .tasks()
      .close_io(namespaced(
        CloseIoRequest {
          container_id: self.container_id.clone(),
          exec_id: String::new(),
          stdin: true,
        },
        &self.namespace,
      )?)
      .await
      .map_err(|error| grpc("close containerd task input", error))?;
    Ok(())
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    if self.reaped {
      return Err(ExecutionError::Reaped);
    }
    let response = Client::from(self.channel.clone())
      .tasks()
      .wait(namespaced(
        WaitRequest {
          container_id: self.container_id.clone(),
          exec_id: String::new(),
        },
        &self.namespace,
      )?)
      .await
      .map_err(|error| grpc("wait for containerd task", error))?
      .into_inner();
    self.reaped = true;
    Ok(ExecutionExit {
      code: i32::try_from(response.exit_status).ok(),
    })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    kill_task(&Client::from(self.channel.clone()), &self.namespace, &self.container_id).await
  }

  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError> {
    destroy_resources(
      &Client::from(self.channel.clone()),
      &self.namespace,
      &self.snapshotter,
      &self.container_id,
      &self.snapshot_key,
      &self.io_directory,
      self.cleanup_timeout,
    )
    .await
  }
}

async fn kill_task(client: &Client, namespace: &str, id: &str) -> Result<(), ExecutionError> {
  match client
    .tasks()
    .kill(namespaced(
      KillRequest {
        container_id: id.to_owned(),
        exec_id: String::new(),
        signal: libc::SIGKILL as u32,
        all: true,
      },
      namespace,
    )?)
    .await
  {
    Ok(_) => Ok(()),
    Err(error) if matches!(error.code(), Code::NotFound | Code::FailedPrecondition) => Ok(()),
    Err(error) => Err(grpc("kill containerd task", error)),
  }
}

pub(super) async fn destroy_resources(
  client: &Client,
  namespace: &str,
  snapshotter: &str,
  id: &str,
  snapshot_key: &str,
  io_directory: &Path,
  cleanup_timeout: Duration,
) -> Result<(), ExecutionError> {
  // Cleanup is deliberately exhaustive and idempotent: later resources are
  // still removed when an earlier API call fails, and NotFound means the
  // desired postcondition has already been reached.
  timeout(cleanup_timeout, async {
    let mut failures = Vec::new();
    if let Err(error) = kill_task(client, namespace, id).await {
      failures.push(error.to_string());
    }
    if let Err(error) = client
      .tasks()
      .wait(namespaced(
        WaitRequest {
          container_id: id.to_owned(),
          exec_id: String::new(),
        },
        namespace,
      )?)
      .await
      && error.code() != Code::NotFound
    {
      failures.push(grpc("wait for containerd task cleanup", error).to_string());
    }
    if let Err(error) = client
      .tasks()
      .delete(namespaced(
        DeleteTaskRequest {
          container_id: id.to_owned(),
        },
        namespace,
      )?)
      .await
      && error.code() != Code::NotFound
    {
      failures.push(grpc("delete containerd task", error).to_string());
    }
    if let Err(error) = client
      .containers()
      .delete(namespaced(DeleteContainerRequest { id: id.to_owned() }, namespace)?)
      .await
      && error.code() != Code::NotFound
    {
      failures.push(grpc("delete containerd container", error).to_string());
    }
    if !snapshot_key.is_empty()
      && let Err(error) = client
        .snapshots()
        .remove(namespaced(
          RemoveSnapshotRequest {
            snapshotter: snapshotter.to_owned(),
            key: snapshot_key.to_owned(),
          },
          namespace,
        )?)
        .await
      && error.code() != Code::NotFound
    {
      failures.push(grpc("remove containerd snapshot", error).to_string());
    }
    if let Err(error) = fs::remove_dir_all(io_directory)
      && error.kind() != std::io::ErrorKind::NotFound
    {
      failures.push(format!(
        "remove containerd I/O directory '{}': {error}",
        io_directory.display()
      ));
    }
    if failures.is_empty() {
      Ok(())
    } else {
      Err(ExecutionError::Backend(failures.join("; ")))
    }
  })
  .await
  .map_err(|_| backend("containerd cleanup timed out"))?
}
