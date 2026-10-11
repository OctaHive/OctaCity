use super::*;
use std::time::SystemTime;

const FACTORY_RUNNER_BYTES: &[u8] = b"factory runner fixture";

#[tokio::test]
async fn expired_managed_execution_stops_before_protected_inputs_or_source_materialization() {
  let root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let mut spec = factory_spec();
  spec.issued_at = 100;
  spec.expires_at = 200;
  spec.execution_deadline_unix_ms = 200_000;
  let executor = factory_executor(
    root.path(),
    source_calls.clone(),
    starts.clone(),
    &spec,
    FactoryEnforcementCapabilityV3::ALL,
  );
  let inputs = transfers(&spec.protected_inputs);
  let (events, _) = mpsc::channel(8);
  let failure = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: inputs,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();
  assert!(
    matches!(failure.error(), JobError::TimedOut),
    "the signed absolute ceiling must stop expired execution"
  );
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn reports_an_unqualified_mode_without_falling_back_to_a_legacy_backend() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let executor = executor(
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
  let spec = host_spec();
  let (events, _receiver) = mpsc::channel(1);

  let error = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: Vec::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    error.error(),
    JobError::ExecutionModeUnavailable(ExecutionMode::Host)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn records_provider_evidence_for_a_qualified_host_route() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let backend = Arc::new(FakeBackend {
    starts: starts.clone(),
    destroyed: Arc::new(AtomicBool::new(false)),
  });
  let spec = host_spec();
  let platform = spec.runtime.target.host_platform;
  let executor = host_executor(work_root.path(), source_calls.clone(), backend, platform);
  let (events, _receiver) = mpsc::channel(8);

  let completion = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: Vec::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap();

  let evidence = completion.execution().unwrap();
  assert_eq!(evidence.provider.as_str(), "host");
  assert_eq!(evidence.target.mode, ExecutionMode::Host);
  assert!(evidence.target.required_guarantees.is_empty());
  assert_eq!(source_calls.load(Ordering::SeqCst), 1);
  assert_eq!(starts.load(Ordering::SeqCst), 1);
  completion.cleanup().await.unwrap();
}

#[tokio::test]
async fn projects_provider_neutral_isolation_to_an_immutable_process_image() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let backend = Arc::new(FakeBackend {
    starts: starts.clone(),
    destroyed: Arc::new(AtomicBool::new(false)),
  });
  let spec = isolation_spec();
  let platform = spec.runtime.target.host_platform;
  let capability = ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new("containerd").unwrap(),
    mode: ExecutionMode::Isolation,
    host_platform: platform,
    target_platform: platform,
    guarantees: guarantees_for(ExecutionMode::Isolation),
    immutable_images: true,
  };
  let executor = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: source_calls.clone(),
      fail: false,
    }),
    None,
  )
  .with_execution_backends([ExecutionBackendRoute::new(
    capability,
    ExecutionEnvironmentId::new("containerd-fixture-v1").unwrap(),
    backend,
  )
  .unwrap()])
  .unwrap();
  let (events, _receiver) = mpsc::channel(8);

  let completion = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: Vec::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap();

  let evidence = completion.execution().unwrap();
  assert_eq!(evidence.provider.as_str(), "containerd");
  assert_eq!(evidence.target.mode, ExecutionMode::Isolation);
  assert_eq!(
    evidence.target.required_guarantees,
    guarantees_for(ExecutionMode::Isolation)
  );
  assert_eq!(source_calls.load(Ordering::SeqCst), 1);
  assert_eq!(starts.load(Ordering::SeqCst), 1);
  completion.cleanup().await.unwrap();
}

#[tokio::test]
async fn rejects_host_workload_identity_before_source_activity() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let backend = Arc::new(FakeBackend {
    starts: starts.clone(),
    destroyed: Arc::new(AtomicBool::new(false)),
  });
  let mut spec = host_spec();
  spec.runtime.workload_identity_profile = Some("ci".to_owned());
  let platform = spec.runtime.target.host_platform;
  let executor = host_executor(work_root.path(), source_calls.clone(), backend, platform);
  let (events, _receiver) = mpsc::channel(1);

  let error = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: Vec::new(),
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    error.error(),
    JobError::WorkloadIdentityUnsupported(ExecutionMode::Host)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn rejects_factory_grant_widening_before_source_or_spawn() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let spec = factory_spec();
  let mut executor = factory_executor(
    work_root.path(),
    source_calls.clone(),
    starts.clone(),
    &spec,
    FactoryEnforcementCapabilityV3::ALL,
  );
  let mut local_permissions = spec.permissions.clone();
  local_permissions.network_hosts.clear();
  executor.factory_permissions = Some(local_permissions);
  let transfers = transfers(&spec.protected_inputs);
  let (events, _receiver) = mpsc::channel(1);

  let failure = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: transfers,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    failure.error(),
    JobError::FactoryPreflight(FactoryPreflightError::Network)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn rejects_an_incomplete_factory_backend_before_source_or_spawn() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let spec = factory_spec();
  let enforcement = FactoryEnforcementCapabilityV3::ALL
    .into_iter()
    .filter(|capability| *capability != FactoryEnforcementCapabilityV3::NetworkHosts);
  let mut executor = factory_executor(
    work_root.path(),
    source_calls.clone(),
    starts.clone(),
    &spec,
    enforcement,
  );
  executor.factory_permissions = Some(spec.permissions.clone());
  let (events, _receiver) = mpsc::channel(1);

  let failure = executor
    .execute(
      ExecuteJobRequest {
        protected_inputs: transfers(&spec.protected_inputs),
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    failure.error(),
    JobError::FactoryPreflight(FactoryPreflightError::BackendCapability)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn stages_and_projects_an_admitted_factory_job() {
  let (completion, cancelled, _work_root) = captured_factory_job(None, false, &[]).await;
  let completion = completion.unwrap();
  assert!(!cancelled.load(Ordering::SeqCst));
  assert_eq!(
    completion.change_set().unwrap().manifest.node_attempt_id,
    "00000000-0000-0000-0000-000000000003"
  );
  assert!(!completion.workspace().parent().unwrap().join("protected").exists());
  completion.cleanup().await.unwrap();
}

#[tokio::test]
async fn capture_filters_preserve_file_and_overlapping_prefixes_without_mount_grants() {
  for prefixes in [
    &["Octafile.yml"][..],
    &["contracts/new.md"][..],
    &["docs", "docs/specs"][..],
  ] {
    let (completion, _, _root) = captured_factory_job(None, false, prefixes).await;
    let completion = completion.expect("a capture filter does not declare writable mount roots");
    assert!(completion.workspace().join("Octafile.yml").is_file());
    assert!(!completion.workspace().join("contracts/new.md").exists());
    assert!(!completion.workspace().join("docs").exists());
    completion.cleanup().await.unwrap();
  }
}

#[tokio::test]
async fn an_admitted_managed_execution_outlives_its_signature_validity() {
  let (completion, cancelled, _root) = captured_factory_job(Some(Duration::from_secs(3)), false, &[]).await;
  assert!(!cancelled.load(Ordering::SeqCst));
  completion
    .expect("admitted execution retains its own deadline")
    .cleanup()
    .await
    .unwrap();
}

#[tokio::test]
async fn managed_capture_obeys_the_absolute_deadline_and_drains_cancellation() {
  let (completion, cancelled, _work_root) = captured_factory_job(Some(Duration::from_millis(500)), false, &[]).await;
  let failure = completion.expect_err("capture cannot turn an expired execution into success");
  assert!(matches!(failure.error(), JobError::TimedOut));
  assert!(
    cancelled.load(Ordering::SeqCst),
    "the installed capturer must observe cancellation before return"
  );
  failure.cleanup().await.unwrap();
}

#[tokio::test]
async fn managed_materialization_obeys_the_deadline_before_starting_the_runner() {
  let (completion, cancelled, _root) = captured_factory_job(Some(Duration::from_millis(500)), true, &[]).await;
  let failure = completion.expect_err("expired preparation cannot execute the runner");
  assert!(matches!(failure.error(), JobError::TimedOut));
  assert!(
    cancelled.load(Ordering::SeqCst),
    "the installed materializer must observe deadline cancellation"
  );
  failure.cleanup().await.unwrap();
}

struct SlowChangeSetMaterializer {
  cancelled: Arc<AtomicBool>,
}

#[async_trait]
impl ChangeSetMaterializer for SlowChangeSetMaterializer {
  async fn materialize(
    &self,
    _request: MaterializationRequest,
    cancellation: CancellationToken,
  ) -> Result<octacity_changeset::MaterializedChangeSet, MaterializationError> {
    tokio::select! {
      () = tokio::time::sleep(Duration::from_secs(2)) => Err(MaterializationError::Cancelled),
      () = cancellation.cancelled() => {
        self.cancelled.store(true, Ordering::SeqCst);
        Err(MaterializationError::Cancelled)
      }
    }
  }
}

async fn captured_factory_job(
  deadline: Option<Duration>,
  materialization: bool,
  write_paths: &[&str],
) -> (Result<JobCompletion, JobFailure>, Arc<AtomicBool>, tempfile::TempDir) {
  captured_factory_job_with_source_scope(deadline, materialization, write_paths, None).await
}

async fn captured_factory_job_with_source_scope(
  deadline: Option<Duration>,
  materialization: bool,
  write_paths: &[&str],
  source_write_paths: Option<&[&str]>,
) -> (Result<JobCompletion, JobFailure>, Arc<AtomicBool>, tempfile::TempDir) {
  use sha2::Digest as _;
  use std::time::SystemTime;
  use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

  let octafile = b"version: 1\n\ntasks:\n  implement:\n    shell: echo managed\n";
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let server = tokio::spawn(async move {
    for _ in 0..if materialization { 3 } else { 1 } {
      let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
      let mut request = [0_u8; 4096];
      let count = socket.read(&mut request).await.unwrap();
      let request = std::str::from_utf8(&request[..count]).unwrap();
      let media_type = if request.contains("accepted-change-set-bundle") {
        octacity_protocol::CHANGE_SET_BUNDLE_MEDIA_TYPE
      } else if request.contains("accepted-change-set-manifest") {
        octacity_protocol::CHANGE_SET_MANIFEST_MEDIA_TYPE
      } else {
        "application/yaml"
      };
      let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {media_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        octafile.len()
      );
      socket.write_all(response.as_bytes()).await.unwrap();
      socket.write_all(octafile).await.unwrap();
    }
  });
  let origin = format!("http://{address}");
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let mut spec = factory_spec();
  spec.protected_inputs.inputs[0].size_bytes = octafile.len() as u64;
  spec.protected_inputs.inputs[0].sha256 = hex::encode(sha2::Sha256::digest(octafile));
  spec.protected_inputs.inputs[0].media_type = "application/yaml".to_owned();
  spec.octa.runner_sha256 = hex::encode(sha2::Sha256::digest(FACTORY_RUNNER_BYTES));
  spec.factory = Some(capture_causality());
  spec.factory.as_mut().unwrap().source_write_paths =
    source_write_paths.map(|paths| paths.iter().map(|path| (*path).to_owned()).collect());
  if !write_paths.is_empty() {
    spec
      .factory
      .as_mut()
      .unwrap()
      .change_set_capture
      .as_mut()
      .unwrap()
      .policy
      .allowed_path_prefixes = write_paths.iter().map(|path| (*path).to_owned()).collect();
  }
  spec.expires_at = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .unwrap()
    .as_secs()
    + 1;
  if materialization {
    for (id, destination, media_type) in [
      (
        "accepted-change-set-bundle",
        octacity_protocol::CHANGE_SET_BUNDLE_INPUT,
        octacity_protocol::CHANGE_SET_BUNDLE_MEDIA_TYPE,
      ),
      (
        "accepted-change-set-manifest",
        octacity_protocol::CHANGE_SET_MANIFEST_INPUT,
        octacity_protocol::CHANGE_SET_MANIFEST_MEDIA_TYPE,
      ),
    ] {
      spec.protected_inputs.inputs.push(ProtectedInputV3 {
        artifact_id: id.to_owned(),
        destination: destination.to_owned(),
        media_type: media_type.to_owned(),
        size_bytes: octafile.len() as u64,
        sha256: hex::encode(sha2::Sha256::digest(octafile)),
      });
    }
    spec
      .protected_inputs
      .inputs
      .sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    spec
      .permissions
      .mounts
      .iter_mut()
      .find(|mount| mount.root == FACTORY_SOURCE_ROOT)
      .unwrap()
      .mode = FactoryMountModeV3::ReadOnly;
    spec.factory.as_mut().unwrap().change_set_materialization = Some(octacity_protocol::ChangeSetMaterializationV3 {
      bundle_input: "accepted-change-set-bundle".to_owned(),
      manifest_input: "accepted-change-set-manifest".to_owned(),
      candidate_revision: "abcdef0123456789abcdef0123456789abcdef01".to_owned(),
    });
  }
  if let Some(duration) = deadline {
    spec.execution_deadline_unix_ms =
      (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap() + duration).as_millis() as u64;
  }
  let mut executor = factory_executor(
    work_root.path(),
    source_calls.clone(),
    starts.clone(),
    &spec,
    FactoryEnforcementCapabilityV3::ALL,
  );
  install_revalidatable_runner(&mut executor, work_root.path());
  executor.factory_permissions = Some(spec.permissions.clone());
  let stager = ProtectedInputStager::new(ProtectedInputStagerConfig {
    allowed_origins: vec![origin.clone()],
    download_timeout: Duration::from_secs(2),
  })
  .unwrap();
  let captured = Arc::new(AtomicBool::new(false));
  let cancelled = Arc::new(AtomicBool::new(false));
  let executor = executor
    .with_protected_input_stager(stager)
    .with_change_set_capturer(Arc::new(FakeChangeSetCapturer {
      called: captured.clone(),
      starts: starts.clone(),
      delay: deadline.map(|_| Duration::from_secs(2)),
      cancelled: cancelled.clone(),
    }));
  let executor = if materialization {
    executor.with_change_set_materializer(Arc::new(SlowChangeSetMaterializer {
      cancelled: cancelled.clone(),
    }))
  } else {
    executor
  };
  let mut protected_inputs = transfers(&spec.protected_inputs);
  for input in &mut protected_inputs {
    input.capability.url = format!("{origin}/{}", input.input.artifact_id);
    input.capability.expires_at_unix_ms = SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_millis()
      .saturating_add(60_000) as u64;
  }
  let (events, _receiver) = mpsc::channel(8);

  let completion = executor
    .execute(
      ExecuteJobRequest {
        protected_inputs,
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
      },
      CancellationToken::new(),
      &events,
    )
    .await;
  if let Err(failure) = &completion {
    assert!(
      !matches!(
        failure.error(),
        JobError::FactoryPreflight(_) | JobError::ProtectedInputs(_)
      ),
      "fixture admission failed: {failure:?}"
    );
  }
  server.await.unwrap();

  assert_eq!(source_calls.load(Ordering::SeqCst), 1);
  if !materialization {
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert!(captured.load(Ordering::SeqCst));
  } else {
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    assert!(!captured.load(Ordering::SeqCst));
  }
  (completion, cancelled, work_root)
}

#[tokio::test]
async fn rejects_requested_change_set_capture_before_source_when_unconfigured() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let mut spec = factory_spec();
  spec.factory = Some(capture_causality());
  let mut executor = factory_executor(
    work_root.path(),
    source_calls.clone(),
    starts.clone(),
    &spec,
    FactoryEnforcementCapabilityV3::ALL,
  );
  executor.factory_permissions = Some(spec.permissions.clone());
  let (events, _receiver) = mpsc::channel(1);

  let failure = executor
    .execute(
      ExecuteJobRequest {
        protected_inputs: transfers(&spec.protected_inputs),
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    failure.error(),
    JobError::FactoryPreflight(FactoryPreflightError::ChangeSetCaptureUnavailable)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rejects_requested_change_set_materialization_before_source_when_unconfigured() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let mut spec = factory_spec();
  let bundle_input = "accepted-change-set-bundle".to_owned();
  let manifest_input = "accepted-change-set-manifest".to_owned();
  spec.protected_inputs.inputs.extend([
    ProtectedInputV3 {
      artifact_id: bundle_input.clone(),
      size_bytes: 512,
      sha256: DIGEST.to_owned(),
      media_type: octacity_protocol::CHANGE_SET_BUNDLE_MEDIA_TYPE.to_owned(),
      destination: octacity_protocol::CHANGE_SET_BUNDLE_INPUT.to_owned(),
    },
    ProtectedInputV3 {
      artifact_id: manifest_input.clone(),
      size_bytes: 512,
      sha256: DIGEST.to_owned(),
      media_type: octacity_protocol::CHANGE_SET_MANIFEST_MEDIA_TYPE.to_owned(),
      destination: octacity_protocol::CHANGE_SET_MANIFEST_INPUT.to_owned(),
    },
  ]);
  spec
    .protected_inputs
    .inputs
    .sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
  spec
    .permissions
    .mounts
    .iter_mut()
    .find(|mount| mount.root == FACTORY_SOURCE_ROOT)
    .unwrap()
    .mode = FactoryMountModeV3::ReadOnly;
  let mut causality = capture_causality();
  causality.change_set_capture = None;
  causality.change_set_materialization = Some(octacity_protocol::ChangeSetMaterializationV3 {
    bundle_input,
    manifest_input,
    candidate_revision: "abcdef0123456789abcdef0123456789abcdef01".to_owned(),
  });
  spec.factory = Some(causality);
  spec
    .validate(&JobBinding {
      job_id: &spec.job_id,
      attempt: spec.attempt,
      now: spec.issued_at,
    })
    .unwrap();
  let mut executor = factory_executor(
    work_root.path(),
    source_calls.clone(),
    starts.clone(),
    &spec,
    FactoryEnforcementCapabilityV3::ALL,
  );
  executor.factory_permissions = Some(spec.permissions.clone());
  let (events, _receiver) = mpsc::channel(1);

  let failure = executor
    .execute(
      ExecuteJobRequest {
        protected_inputs: transfers(&spec.protected_inputs),
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    failure.error(),
    JobError::FactoryPreflight(FactoryPreflightError::ChangeSetMaterializationUnavailable)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
}

struct FakeChangeSetCapturer {
  called: Arc<AtomicBool>,
  starts: Arc<AtomicUsize>,
  delay: Option<Duration>,
  cancelled: Arc<AtomicBool>,
}

#[async_trait]
impl ChangeSetCapturer for FakeChangeSetCapturer {
  async fn capture(
    &self,
    request: CaptureRequest,
    cancellation: CancellationToken,
  ) -> Result<CapturedChangeSet, CaptureError> {
    assert_eq!(
      self.starts.load(Ordering::SeqCst),
      1,
      "capture must run after the runner"
    );
    assert!(request.workspace.join("Octafile.yml").is_file());
    assert!(request.destination.is_dir());
    fs::write(request.destination.join("change-set.bundle"), b"bundle").unwrap();
    self.called.store(true, Ordering::SeqCst);
    if let Some(delay) = self.delay {
      tokio::select! {
        () = tokio::time::sleep(delay) => {},
        () = cancellation.cancelled() => {
          self.cancelled.store(true, Ordering::SeqCst);
          return Err(CaptureError::Cancelled);
        }
      }
    }
    let bundle = CapturedFile {
      name: "change-set.bundle".to_owned(),
      size_bytes: 6,
      sha256: DIGEST.to_owned(),
    };
    Ok(CapturedChangeSet {
      root: request.destination,
      manifest: ChangeSetManifest {
        format_version: 1,
        base_revision: request.base_revision,
        candidate_revision: DIGEST.to_owned(),
        node_attempt_id: request.node_attempt_id,
        capture_tool: FactoryImmutableReferenceV3 {
          identity: "git".to_owned(),
          version: "2.0.0".to_owned(),
          sha256: DIGEST.to_owned(),
        },
        changed_paths: Vec::new(),
        bundle,
        patch: None,
      },
      manifest_file: CapturedFile {
        name: "change-set-manifest.json".to_owned(),
        size_bytes: 1,
        sha256: DIGEST.to_owned(),
      },
    })
  }
}

fn install_revalidatable_runner(executor: &mut JobExecutor, work_root: &Path) {
  let release = work_root.join(".factory-runner");
  octacity_private_fs::create_private_directory(&release).unwrap();
  let executable = release.join("octa-runner");
  fs::write(&executable, FACTORY_RUNNER_BYTES).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
  }
  let plugins = release.join("plugins");
  fs::create_dir(&plugins).unwrap();
  let lock = release.join("Octa.lock");
  fs::write(&lock, "version: 1\nplugins: {}\n").unwrap();
  executor.runner.root = release.canonicalize().unwrap();
  executor.runner.executable = executable.canonicalize().unwrap();
  executor.runner.plugins_dir = plugins.canonicalize().unwrap();
  executor.runner.default_plugin_lock = lock.canonicalize().unwrap();
  executor.runner.sha256 = hex::encode(sha2::Sha256::digest(FACTORY_RUNNER_BYTES));
  executor.runner.revalidate_files(&BTreeMap::new()).unwrap();
}

#[tokio::test]
async fn never_falls_back_to_host_for_a_factory_job() {
  let work_root = tempfile::tempdir().unwrap();
  let source_calls = Arc::new(AtomicUsize::new(0));
  let starts = Arc::new(AtomicUsize::new(0));
  let executor = executor(
    work_root.path(),
    Arc::new(FakeSource {
      calls: source_calls.clone(),
      fail: false,
    }),
    Some(Arc::new(FakeBackend {
      starts: starts.clone(),
      destroyed: Arc::new(AtomicBool::new(false)),
    })),
  );
  let spec = factory_spec();
  let (events, _receiver) = mpsc::channel(1);

  let failure = executor
    .execute(
      ExecuteJobRequest {
        protected_inputs: transfers(&spec.protected_inputs),
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
      },
      CancellationToken::new(),
      &events,
    )
    .await
    .unwrap_err();

  assert!(matches!(
    failure.error(),
    JobError::ExecutionModeUnavailable(ExecutionMode::Virtualization)
  ));
  assert_eq!(source_calls.load(Ordering::SeqCst), 0);
  assert_eq!(starts.load(Ordering::SeqCst), 0);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

fn host_executor(
  work_root: &Path,
  source_calls: Arc<AtomicUsize>,
  backend: Arc<FakeBackend>,
  platform: PlatformSpec,
) -> JobExecutor {
  let capability = ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new("host").unwrap(),
    mode: ExecutionMode::Host,
    host_platform: platform,
    target_platform: platform,
    guarantees: guarantees_for(ExecutionMode::Host),
    immutable_images: false,
  };
  executor(
    work_root,
    Arc::new(FakeSource {
      calls: source_calls,
      fail: false,
    }),
    None,
  )
  .with_execution_backends([ExecutionBackendRoute::new(
    capability,
    ExecutionEnvironmentId::new("host-fixture-v1").unwrap(),
    backend,
  )
  .unwrap()])
  .unwrap()
}

fn factory_executor(
  work_root: &Path,
  source_calls: Arc<AtomicUsize>,
  starts: Arc<AtomicUsize>,
  spec: &JobSpecV3,
  enforcement: impl IntoIterator<Item = FactoryEnforcementCapabilityV3>,
) -> JobExecutor {
  let capability = ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new("microsandbox").unwrap(),
    mode: ExecutionMode::Virtualization,
    host_platform: spec.runtime.target.host_platform,
    target_platform: spec.runtime.target.target_platform,
    guarantees: guarantees_for(ExecutionMode::Virtualization),
    immutable_images: true,
  };
  let backend = Arc::new(FakeBackend {
    starts,
    destroyed: Arc::new(AtomicBool::new(false)),
  });
  let mut executor = executor(
    work_root,
    Arc::new(FakeSource {
      calls: source_calls,
      fail: false,
    }),
    None,
  )
  .with_execution_backends([ExecutionBackendRoute::new(
    capability,
    ExecutionEnvironmentId::new("microsandbox-fixture-v1").unwrap(),
    backend,
  )
  .unwrap()
  .with_factory_enforcement(enforcement)
  .unwrap()])
  .unwrap();
  executor.allowed_network_hosts.insert("api.openai.com".to_owned());
  executor
}

fn host_spec() -> JobSpecV2 {
  let legacy = specification(RuntimeMode::Native);
  let platform = legacy.runtime.platform();
  JobSpecV2 {
    protocol_version: EXECUTION_CONTRACT_V2,
    job_id: legacy.job_id,
    attempt: legacy.attempt,
    issued_at: legacy.issued_at,
    expires_at: legacy.expires_at,
    source: legacy.source,
    octa: legacy.octa,
    execution: legacy.execution,
    runtime: RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode: ExecutionMode::Host,
        host_platform: platform,
        target_platform: platform,
        required_guarantees: guarantees_for(ExecutionMode::Host),
        immutable_image: None,
      },
      cpu_millis: legacy.runtime.cpu_millis,
      memory_bytes: legacy.runtime.memory_bytes,
      writable_disk_bytes: legacy.runtime.writable_disk_bytes,
      timeout_seconds: legacy.runtime.timeout_seconds,
      network: legacy.runtime.network,
      workload_identity_profile: legacy.runtime.workload_identity_profile,
    },
    cache: legacy.cache,
    outputs: legacy.outputs,
  }
}

fn isolation_spec() -> JobSpecV2 {
  let mut spec = host_spec();
  spec.job_id = "provider-neutral-isolation".to_owned();
  spec.runtime.target.mode = ExecutionMode::Isolation;
  spec.runtime.target.required_guarantees = guarantees_for(ExecutionMode::Isolation);
  spec.runtime.target.immutable_image = Some(format!("registry.example.com/build@sha256:{DIGEST}"));
  spec.runtime.network = NetworkPolicy::Disabled;
  spec
}

fn factory_spec() -> JobSpecV3 {
  let mut current = isolation_spec();
  current.issued_at = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .unwrap()
    .as_secs();
  current.expires_at = current.issued_at + current.runtime.timeout_seconds;
  current.job_id = "managed-factory-job".to_owned();
  current.runtime.target.mode = ExecutionMode::Virtualization;
  current.runtime.target.required_guarantees = guarantees_for(ExecutionMode::Virtualization);
  current.runtime.network = NetworkPolicy::Restricted {
    allowed_hosts: vec!["api.openai.com".to_owned()],
  };
  let protected_inputs = ProtectedInputManifestV3 {
    inputs: vec![ProtectedInputV3 {
      artifact_id: "managed-octafile".to_owned(),
      size_bytes: 128,
      sha256: DIGEST.to_owned(),
      media_type: "application/yaml".to_owned(),
      destination: "/octacity/protected/Octafile.yml".to_owned(),
    }],
  };
  let permissions = FactoryPermissionSetV3 {
    plugins: Vec::new(),
    executables: Vec::new(),
    tools: Vec::new(),
    commands: Vec::new(),
    max_descendants: 1,
    mounts: vec![
      FactoryMountPermissionV3 {
        root: "/octacity/protected".to_owned(),
        mode: FactoryMountModeV3::ReadOnly,
      },
      FactoryMountPermissionV3 {
        root: "/workspace/output".to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
      FactoryMountPermissionV3 {
        root: "/workspace/scratch".to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
      FactoryMountPermissionV3 {
        root: "/workspace/source".to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
    ],
    network_hosts: vec!["api.openai.com".to_owned()],
    secret_profiles: Vec::new(),
    workload_identity_profiles: Vec::new(),
    resources: FactoryResourceLimitsV3 {
      cpu_millis: current.runtime.cpu_millis,
      memory_bytes: current.runtime.memory_bytes,
      disk_bytes: current.runtime.writable_disk_bytes,
      process_count: 2,
      elapsed_millis: current.runtime.timeout_seconds * 1_000,
    },
    outputs: FactoryOutputPermissionsV3 {
      kinds: Vec::new(),
      max_artifact_count: 0,
      max_artifact_bytes: 0,
      max_report_count: 0,
      max_report_bytes: 0,
    },
  };
  let spec = JobSpecV3 {
    protocol_version: EXECUTION_CONTRACT_V3,
    job_id: current.job_id,
    attempt: current.attempt,
    issued_at: current.issued_at,
    expires_at: current.expires_at,
    execution_deadline_unix_ms: current.expires_at * 1_000,
    source: current.source,
    octa: current.octa,
    execution: ManagedOctaExecutionV3 {
      octafile_input: "managed-octafile".to_owned(),
      tasks: vec!["implement".to_owned()],
      credential_profile: None,
      tool_control: None,
    },
    runtime: current.runtime,
    cache: current.cache,
    outputs: current.outputs,
    factory: None,
    protected_inputs,
    permissions,
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
  };
  spec
    .validate(&JobBinding {
      job_id: &spec.job_id,
      attempt: spec.attempt,
      now: spec.issued_at,
    })
    .unwrap();
  spec
}

fn capture_causality() -> FactoryCausalityV3 {
  FactoryCausalityV3 {
    factory_run_id: "00000000-0000-0000-0000-000000000001".to_owned(),
    factory_configuration_id: "00000000-0000-0000-0000-000000000002".to_owned(),
    factory_configuration_version: 1,
    flow_run_id: "00000000-0000-0000-0000-000000000004".to_owned(),
    workflow_cycle_id: "00000000-0000-0000-0000-000000000005".to_owned(),
    node_attempt_id: "00000000-0000-0000-0000-000000000003".to_owned(),
    input_digest: DIGEST.to_owned(),
    subject_digest: DIGEST.to_owned(),
    source_write_paths: None,
    change_set_capture: Some(ChangeSetCaptureV3 {
      author_name: "OctaCity Factory".to_owned(),
      author_email: "factory@octacity.invalid".to_owned(),
      committed_at: 1_767_225_600,
      patch_max_bytes: Some(1024),
      policy: ChangeSetCapturePolicyV3 {
        allowed_path_prefixes: vec![".".to_owned()],
        forbidden_control_paths: vec![".github/workflows".to_owned()],
        max_changed_paths: 64,
        max_file_bytes: 1024 * 1024,
        max_total_bytes: 4 * 1024 * 1024,
        allow_empty: false,
        binary_policy: ChangeSetBinaryPolicyV3::Reject,
        forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
      },
    }),
    change_set_materialization: None,
  }
}

fn transfers(manifest: &ProtectedInputManifestV3) -> Vec<ProtectedInputTransferV3> {
  manifest
    .inputs
    .iter()
    .cloned()
    .map(|input| ProtectedInputTransferV3 {
      input,
      capability: ArtifactTransferCapability {
        url: "https://artifacts.invalid/protected".to_owned(),
        required_headers: BTreeMap::new(),
        expires_at_unix_ms: u64::MAX,
      },
    })
    .collect()
}

#[tokio::test]
async fn signed_source_write_roots_narrow_the_runner_write_projection_before_authoring() {
  let (completion, _, _root) =
    captured_factory_job_with_source_scope(None, false, &[], Some(&["contracts", "plans/changes"])).await;
  let completion = completion.unwrap();
  assert!(completion.workspace().join("contracts").is_dir());
  assert!(completion.workspace().join("plans/changes").is_dir());
  completion.cleanup().await.unwrap();
}
