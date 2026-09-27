use super::*;

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
