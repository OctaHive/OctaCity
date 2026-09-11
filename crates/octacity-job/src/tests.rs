//! Job orchestration tests using backend-neutral fakes.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_execution::{
  ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ExecutionReader, ExecutionWriter, ResourceUsage,
  RunnerProgram, RunningExecution,
};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, ExecutionSpec, NetworkPolicy, OctaSpec, OutputLimits, RuntimeSpec, SIGNATURE_ALGORITHM,
  SourceSpec,
};
use octacity_runner::{RunStatus, RunnerCapabilities};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

use super::*;

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct FakeSource {
  calls: Arc<AtomicUsize>,
  fail: bool,
}

#[async_trait]
impl SourceMaterializer for FakeSource {
  async fn materialize(
    &self,
    requirement: &SourceSpec,
    request: SourceMaterializationRequest,
    _operation_timeout: Duration,
    _cancellation_grace: Duration,
    _cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceError> {
    self.calls.fetch_add(1, Ordering::SeqCst);
    assert!(request.destination.is_dir());
    assert_eq!(request.revision, requirement.revision);
    if self.fail {
      return Err(SourceError::Host(octacity_source::SourceHostError::Invalid(
        "fixture failure".to_owned(),
      )));
    }
    fs::write(request.destination.join("Octafile.yml"), "version: 1\n").unwrap();
    Ok(MaterializedSource {
      revision: requirement.revision.clone(),
      provenance: BTreeMap::from([("commit".to_owned(), requirement.revision.clone())]),
      progress: Vec::new(),
      diagnostics: Vec::new(),
    })
  }
}

struct FakeBackend {
  starts: Arc<AtomicUsize>,
  destroyed: Arc<AtomicBool>,
}

struct FakeExecution {
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  destroyed: Arc<AtomicBool>,
}

#[async_trait]
impl ExecutionBackend for FakeBackend {
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    _cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    self.starts.fetch_add(1, Ordering::SeqCst);
    assert_eq!(
      request.root,
      ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      }
    );
    assert_eq!(request.network, NetworkAccess::Unrestricted);
    let request_id = request.execution_id.clone();
    let (agent_stdin, runner_input) = tokio::io::duplex(4096);
    let (runner_output, agent_stdout) = tokio::io::duplex(4096);
    let (agent_stderr, runner_stderr) = tokio::io::duplex(64);
    drop(agent_stderr);
    tokio::spawn(fake_runner(request_id, runner_input, runner_output));
    Ok(Box::new(FakeExecution {
      io: Some(ExecutionIo {
        stdin: Box::pin(agent_stdin) as ExecutionWriter,
        stdout: Box::pin(agent_stdout) as ExecutionReader,
        stderr: Box::pin(runner_stderr) as ExecutionReader,
      }),
      paths: ExecutionPaths {
        workspace: request.workspace,
        data_dir: request.data_dir,
        plugins_dir: runner.plugins_dir.clone(),
        plugin_lock: runner.plugin_lock.clone(),
      },
      destroyed: self.destroyed.clone(),
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    Ok(())
  }
}

#[async_trait]
impl RunningExecution for FakeExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    Ok(ResourceUsage {
      elapsed_ms: 10,
      cpu_time_ms: 2,
      memory_current_bytes: 1024,
      memory_peak_bytes: 2048,
      disk_current_bytes: 4096,
      disk_peak_bytes: 4096,
      io_read_bytes: 12,
      io_written_bytes: 34,
      network_received_bytes: None,
      network_transmitted_bytes: None,
    })
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    Ok(ExecutionExit { code: Some(0) })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    Ok(())
  }

  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError> {
    self.destroyed.store(true, Ordering::SeqCst);
    Ok(())
  }
}

async fn fake_runner(request_id: String, input: tokio::io::DuplexStream, mut output: tokio::io::DuplexStream) {
  output
      .write_all(
        b"{\"type\":\"hello\",\"protocol_version\":1,\"octa_version\":\"0.3.0\",\"event_schema_version\":3,\"plugin_protocol_version\":1}\n",
      )
      .await
      .unwrap();
  let mut input = BufReader::new(input);
  let mut command = String::new();
  input.read_line(&mut command).await.unwrap();
  assert!(command.contains(&request_id));
  output
      .write_all(
        format!(
          "{{\"type\":\"accepted\",\"request_id\":{id}}}\n{{\"type\":\"finished\",\"request_id\":{id},\"status\":\"succeeded\",\"results\":[]}}\n",
          id = serde_json::to_string(&request_id).unwrap()
        )
        .as_bytes(),
      )
      .await
      .unwrap();
}

fn specification(mode: RuntimeMode) -> JobSpecV1 {
  JobSpecV1 {
    protocol_version: AGENT_PROTOCOL_VERSION,
    job_id: "job/with/untrusted/path".to_owned(),
    attempt: 1,
    issued_at: 100,
    expires_at: 200,
    source: SourceSpec {
      provider: "git".to_owned(),
      plugin_version: "0.1.0".to_owned(),
      plugin_sha256: DIGEST.to_owned(),
      revision: "abcdef".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
    },
    octa: OctaSpec {
      version: "0.3.0".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 1,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    execution: ExecutionSpec {
      octafile: Some("Octafile.yml".to_owned()),
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    runtime: RuntimeSpec {
      target: match mode {
        RuntimeMode::Native => RuntimeTarget::Native {
          platform: octacity_protocol::PlatformSpec {
            os: PlatformOs::Linux,
            architecture: PlatformArchitecture::Amd64,
          },
        },
        RuntimeMode::Oci => RuntimeTarget::Oci {
          platform: octacity_protocol::PlatformSpec {
            os: PlatformOs::Linux,
            architecture: PlatformArchitecture::Amd64,
          },
          isolation: ProtocolOciIsolation::Hypervisor,
          image: format!("registry.example.com/build@sha256:{DIGEST}"),
        },
      },
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      timeout_seconds: 30,
      network: match mode {
        RuntimeMode::Native => NetworkPolicy::Unrestricted,
        RuntimeMode::Oci => NetworkPolicy::Disabled,
      },
      workload_identity_profile: None,
    },
    outputs: OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
    },
  }
}

fn envelope(spec: &JobSpecV1, key: &SigningKey) -> SignedEnvelope {
  let payload = serde_json::to_vec(spec).unwrap();
  SignedEnvelope {
    key_id: "test-key".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: BASE64.encode(&payload),
    signature: BASE64.encode(key.sign(&payload).to_bytes()),
  }
}

fn runner() -> RunnerInstallation {
  RunnerInstallation {
    root: PathBuf::from("/opt/octa"),
    executable: PathBuf::from("/opt/octa/octa-runner"),
    plugins_dir: PathBuf::from("/opt/octa/plugins"),
    default_plugin_lock: PathBuf::from("/opt/octa/Octa.lock"),
    sha256: DIGEST.to_owned(),
    capabilities: RunnerCapabilities {
      octa_version: "0.3.0".to_owned(),
      runner_protocols: vec![1],
      event_schemas: vec![3],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      platform: "any".to_owned(),
      features: Vec::new(),
      build_commit: None,
    },
    plugins: BTreeMap::new(),
  }
}

fn executor(
  work_root: &Path,
  source: Arc<dyn SourceMaterializer>,
  backend: Option<Arc<dyn ExecutionBackend>>,
) -> (JobExecutor, SigningKey) {
  let key = SigningKey::from_bytes(&[7; 32]);
  let backends = backend
    .map(|backend| BTreeMap::from([(RuntimeMode::Native, backend)]))
    .unwrap_or_else(|| {
      BTreeMap::from([(
        RuntimeMode::Oci,
        Arc::new(FakeBackend {
          starts: Arc::new(AtomicUsize::new(0)),
          destroyed: Arc::new(AtomicBool::new(false)),
        }) as Arc<dyn ExecutionBackend>,
      )])
    });
  (
    JobExecutor::new(
      BTreeMap::from([("test-key".to_owned(), key.verifying_key())]),
      runner(),
      source,
      backends,
      JobExecutorConfig {
        work_root: work_root.to_owned(),
        max_workspace_bytes: 2 * 1024 * 1024 * 1024,
        cancellation_grace: Duration::from_secs(1),
        runner_supervision: RunnerSupervisionPolicy::default(),
      },
    )
    .unwrap(),
    key,
  )
}

#[tokio::test]
async fn runs_a_signed_job_through_the_selected_backend_and_cleans_up() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let destroyed = Arc::new(AtomicBool::new(false));
  let (executor, key) = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: source_calls.clone(),
      fail: false,
    }),
    Some(Arc::new(FakeBackend {
      starts: starts.clone(),
      destroyed: destroyed.clone(),
    })),
  );
  let spec = specification(RuntimeMode::Native);
  let (events, _receiver) = mpsc::channel(8);
  let completion = executor
    .execute(
      ExecuteJobRequest {
        envelope: envelope(&spec, &key),
        job_id: spec.job_id.clone(),
        attempt: spec.attempt,
        now: 150,
        source_credentials: BTreeMap::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap();

  assert_eq!(completion.runner().status, RunStatus::Succeeded);
  assert_eq!(completion.output_limits(), &spec.outputs);
  assert_eq!(source_calls.load(Ordering::SeqCst), 1);
  assert_eq!(starts.load(Ordering::SeqCst), 1);
  assert!(destroyed.load(Ordering::SeqCst));
  assert!(completion.workspace().join("Octafile.yml").is_file());
  completion.cleanup().await.unwrap();
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn rejects_unavailable_workload_identity_before_source_activity() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let (executor, key) = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: source_calls.clone(),
      fail: false,
    }),
    Some(Arc::new(FakeBackend {
      starts: Arc::new(AtomicUsize::new(0)),
      destroyed: Arc::new(AtomicBool::new(false)),
    })),
  );
  let mut spec = specification(RuntimeMode::Native);
  spec.runtime.workload_identity_profile = Some("ci".to_owned());
  let (events, _receiver) = mpsc::channel(1);
  let error = executor
    .execute(
      ExecuteJobRequest {
        envelope: envelope(&spec, &key),
        job_id: spec.job_id.clone(),
        attempt: spec.attempt,
        now: 150,
        source_credentials: BTreeMap::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(error, JobError::WorkloadIdentityUnavailable));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn rejects_an_unavailable_backend_without_materializing_source() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let (executor, key) = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: source_calls.clone(),
      fail: false,
    }),
    None,
  );
  let spec = specification(RuntimeMode::Native);
  let (events, _receiver) = mpsc::channel(1);
  let error = executor
    .execute(
      ExecuteJobRequest {
        envelope: envelope(&spec, &key),
        job_id: spec.job_id.clone(),
        attempt: spec.attempt,
        now: 150,
        source_credentials: BTreeMap::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();
  assert!(matches!(error, JobError::RuntimeUnavailable(RuntimeMode::Native)));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn removes_the_workspace_after_source_failure() {
  let work_root = tempfile::tempdir().unwrap();
  let (executor, key) = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: Arc::new(AtomicUsize::new(0)),
      fail: true,
    }),
    Some(Arc::new(FakeBackend {
      starts: Arc::new(AtomicUsize::new(0)),
      destroyed: Arc::new(AtomicBool::new(false)),
    })),
  );
  let spec = specification(RuntimeMode::Native);
  let (events, _receiver) = mpsc::channel(1);
  assert!(
    executor
      .execute(
        ExecuteJobRequest {
          envelope: envelope(&spec, &key),
          job_id: spec.job_id.clone(),
          attempt: spec.attempt,
          now: 150,
          source_credentials: BTreeMap::new(),
        },
        CancellationToken::new(),
        &events,
      )
      .await
      .is_err()
  );
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn startup_cleanup_removes_only_owned_job_directories() {
  let work_root = tempfile::tempdir().unwrap();
  let owned = work_root
    .path()
    .join("job-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
  let operator_file = work_root.path().join("README");
  fs::create_dir(&owned).unwrap();
  fs::write(owned.join("partial"), "data").unwrap();
  fs::write(&operator_file, "do not remove").unwrap();
  let (executor, _) = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: Arc::new(AtomicUsize::new(0)),
      fail: false,
    }),
    Some(Arc::new(FakeBackend {
      starts: Arc::new(AtomicUsize::new(0)),
      destroyed: Arc::new(AtomicBool::new(false)),
    })),
  );

  executor.cleanup_orphans().await.unwrap();

  assert!(!owned.exists());
  assert!(operator_file.exists());
}

#[test]
fn preserves_backend_cancellation_and_timeout_as_job_outcomes() {
  assert!(matches!(
    map_runner_error(RunnerSupervisionError::Execution(ExecutionError::Cancelled)),
    JobError::Cancelled
  ));
  assert!(matches!(
    map_runner_error(RunnerSupervisionError::Execution(ExecutionError::TimedOut {
      operation: "fixture startup",
    })),
    JobError::TimedOut
  ));
  assert!(matches!(
    map_runner_error(RunnerSupervisionError::StartupTimeout),
    JobError::TimedOut
  ));
}
