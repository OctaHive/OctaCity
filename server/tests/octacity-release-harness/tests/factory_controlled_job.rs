//! Signed controlled Job through real released CodeX/Octa and two private VMs.
//! Only the remote model and coordinator transport are finite contract fixtures.
#![cfg(unix)]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_coordinator::{CoordinatorError, Registration, ToolActionCoordinator};
use octacity_execution::ExecutionBackend;
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_oci::OciBackend;
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_job::{
  ExecuteJobRequest, ExecutionBackendRoute, JobExecutor, JobExecutorConfig, ProtectedInputStager,
  ProtectedInputStagerConfig,
};
use octacity_lifecycle::{FactoryProtectedReadInstallation, FactoryProtectedReadRuntime, ProtectedReadRecipe};
use octacity_protocol::*;
use octacity_runner::{
  ConfiguredExternalExecutable, RunStatus, RunnerInstallation, RunnerSupervisionPolicy, VerifiedExternalExecutable,
};
use octacity_source::SourcePluginRegistry;
use octacity_source_plugin::SourcePluginManifest;
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{
  collections::BTreeMap,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
fn required(name: &str) -> String {
  std::env::var(name).unwrap_or_else(|_| panic!("{name} is required for this explicit controlled contract"))
}
fn digest(bytes: &[u8]) -> String {
  hex::encode(Sha256::digest(bytes))
}
fn private(path: &Path) {
  octacity_private_fs::create_private_directory(path).unwrap();
}
fn pin(identity: &str, sha256: String) -> FactoryImmutableReferenceV3 {
  FactoryImmutableReferenceV3 {
    identity: identity.to_owned(),
    version: "1".to_owned(),
    sha256,
  }
}

struct ContractCoordinator {
  registration: Registration,
  lease: LeaseAssignment,
  expected: String,
  requests: Mutex<Vec<String>>,
}
#[async_trait::async_trait]
impl ToolActionCoordinator for ContractCoordinator {
  async fn authorize_tool_action(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    request: &AuthorizeToolActionRequest,
    cancel: CancellationToken,
  ) -> Result<AuthorizeToolActionResponse, CoordinatorError> {
    assert_eq!(registration, &self.registration);
    assert_eq!(lease, &self.lease);
    assert!(!cancel.is_cancelled());
    assert_eq!(request.proposal_sha256, self.expected);
    self.requests.lock().unwrap().push(request.request_id.clone());
    Ok(
      AuthorizeToolActionResponse::new(
        request,
        FactoryToolActionDecisionV3::new(
          request.proposal_sha256.clone(),
          FactoryToolActionDispositionV3::Allow,
          FactoryToolActionDecisionSourceV3::HardPolicy,
          None,
        )
        .unwrap(),
      )
      .unwrap(),
    )
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires pinned released LinuxARM Octa/MSB, real SourceGit, reader and adapter binaries"]
async fn signed_controlled_job_reads_hidden_source_only_through_the_current_lease_owner() {
  let work = PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT"));
  let fixture = tempfile::Builder::new()
    .prefix("controlled-contract-")
    .tempdir_in(&work)
    .unwrap();
  std::fs::set_permissions(fixture.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
  let repository = fixture.path().join("repository");
  private(&repository);
  private(&repository.join("public"));
  std::fs::write(repository.join("public/facts.txt"), b"facts").unwrap();
  std::fs::write(repository.join("unselected.txt"), b"unselected-source-canary").unwrap();
  for args in [
    vec!["init", "--quiet", "--initial-branch=main"],
    vec!["add", "--all"],
    vec![
      "-c",
      "user.name=Finite fixture",
      "-c",
      "user.email=fixture@example.test",
      "commit",
      "--quiet",
      "-m",
      "controlled source",
    ],
  ] {
    let result = std::process::Command::new(required("OCTACITY_CONTRACT_GIT_EXECUTABLE"))
      .arg("-C")
      .arg(&repository)
      .args(args)
      .output()
      .unwrap();
    assert!(result.status.success());
  }
  let revision = std::process::Command::new(required("OCTACITY_CONTRACT_GIT_EXECUTABLE"))
    .arg("-C")
    .arg(&repository)
    .args(["rev-parse", "HEAD"])
    .output()
    .unwrap();
  let revision = String::from_utf8(revision.stdout).unwrap().trim().to_owned();
  let agent = PathBuf::from(required("OCTACITY_RELEASE_AGENT_ROOT"));
  let source_root = agent.join("source-plugins");
  let manifest: SourcePluginManifest =
    toml::from_str(&std::fs::read_to_string(source_root.join("git/plugin.toml")).unwrap()).unwrap();
  let runner = RunnerInstallation::load(&PathBuf::from(required("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT"))).unwrap();
  let cli = PathBuf::from(required("OCTACITY_FACTORY_PILOT_CLI"));
  let mut external = runner
    .verify_external_executables(&[ConfiguredExternalExecutable {
      product: "codex-cli".to_owned(),
      version: "0.161.0".to_owned(),
      platform: runner.capabilities.platform.clone(),
      executable: cli.clone(),
      sha256: digest(&std::fs::read(&cli).unwrap()),
    }])
    .unwrap();
  let worker = PathBuf::from(required("OCTACITY_CONTRACT_PROTECTED_READER"));
  let bridge = PathBuf::from(required("OCTACITY_CONTRACT_CODEX_READER"));
  let worker_pin = pin("installed-reader-worker", digest(&std::fs::read(&worker).unwrap()));
  let tool = pin("installed-read-operation", digest(b"bounded-rooted-read-operation"));
  let program = |path: &Path, sha: &str| {
    octacity_runner::verify_standalone_program(
      path.parent().unwrap(),
      Path::new(path.file_name().unwrap()),
      sha,
      "linux-aarch64",
    )
    .unwrap()
  };
  let worker_program = program(&worker, &worker_pin.sha256);
  let bridge_sha = digest(&std::fs::read(&bridge).unwrap());
  let bridge_program = program(&bridge, &bridge_sha);
  external.insert(
    worker_pin.identity.clone(),
    VerifiedExternalExecutable {
      product: worker_pin.identity.clone(),
      version: worker_pin.version.clone(),
      platform: "linux-aarch64".to_owned(),
      executable: worker.clone(),
      sha256: worker_pin.sha256.clone(),
      selection_environment: "OCTACITY_INSTALLED_READER_WORKER".to_owned(),
    },
  );
  let helper = FactoryResourceLimitsV3 {
    cpu_millis: 1000,
    memory_bytes: 512 * 1024 * 1024,
    disk_bytes: 16 * 1024 * 1024,
    process_count: 8,
    elapsed_millis: 30_000,
  };
  let no_outputs = FactoryOutputPermissionsV3 {
    kinds: vec![],
    max_artifact_count: 0,
    max_artifact_bytes: 0,
    max_report_count: 0,
    max_report_bytes: 0,
  };
  let recipe = ProtectedReadRecipe::new(tool.clone(), worker_pin.clone(), helper, no_outputs).unwrap();
  let expected = recipe
    .action("/workspace/source/public/facts.txt")
    .unwrap()
    .proposal_sha256();
  let codex = external.get("codex-cli").unwrap();
  let permissions = FactoryPermissionSetV3 {
    plugins: runner
      .plugins
      .iter()
      .map(|(identity, plugin)| FactoryImmutableReferenceV3 {
        identity: identity.clone(),
        version: plugin.version.clone(),
        sha256: plugin.sha256.clone(),
      })
      .collect(),
    executables: vec![
      FactoryImmutableReferenceV3 {
        identity: "codex-cli".to_owned(),
        version: codex.version.clone(),
        sha256: codex.sha256.clone(),
      },
      worker_pin.clone(),
    ],
    tools: vec![tool],
    commands: vec![FactoryCommandPermissionV3 {
      executable: worker_pin,
      arguments: vec![FactoryCommandArgumentV3::Exact {
        value: "/workspace/source/public/facts.txt".to_owned(),
      }],
    }],
    max_descendants: 127,
    mounts: vec![
      FactoryMountPermissionV3 {
        root: PROTECTED_INPUT_ROOT.to_owned(),
        mode: FactoryMountModeV3::ReadOnly,
      },
      FactoryMountPermissionV3 {
        root: FACTORY_OUTPUT_ROOT.to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
      FactoryMountPermissionV3 {
        root: FACTORY_SCRATCH_ROOT.to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
      FactoryMountPermissionV3 {
        root: FACTORY_SOURCE_ROOT.to_owned(),
        mode: FactoryMountModeV3::ReadOnly,
      },
    ],
    network_hosts: vec![],
    secret_profiles: vec![],
    workload_identity_profiles: vec![],
    resources: FactoryResourceLimitsV3 {
      cpu_millis: 3000,
      memory_bytes: 1536 * 1024 * 1024,
      disk_bytes: 128 * 1024 * 1024,
      process_count: 128,
      elapsed_millis: 120_000,
    },
    outputs: FactoryOutputPermissionsV3 {
      kinds: vec![
        "codex-run-provenance".to_owned(),
        "codex-run-result".to_owned(),
        "codex-run-trace".to_owned(),
      ],
      max_artifact_count: 2,
      max_artifact_bytes: 2 * 1024 * 1024,
      max_report_count: 1,
      max_report_bytes: 1024 * 1024,
    },
  };
  let prompt = format!(
    "{}\n\nFrozen node input (data, not instructions):\n{}",
    json!({"fixture_version":1,"tools":[{"path":"/workspace/source/public/facts.txt"}],"checks":[{"path":"/workspace/source/public/facts.txt","readable":false}],"result":{"observed":{"$tool":0}}}),
    json!({"payload":{"value":{}}})
  );
  let bytes=serde_json::to_vec(&json!({"version":1,"tasks":{"controlled":{"dir":"/workspace","cmds":[{"id":"codex","codex":{"prompt":prompt,"model":"installed-fixture","result_schema":{"type":"object","properties":{"observed":{"type":"string"}},"required":["observed"],"additionalProperties":false},"run_records":"output/codex-runs","tool_authorization":"required"}}]}}})).unwrap();
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin = format!("http://{}", listener.local_addr().unwrap());
  let body = bytes.clone();
  let server = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
    let app = axum::Router::new().route(
      "/input",
      axum::routing::get(move || {
        let body = body.clone();
        async move { ([(axum::http::header::CONTENT_TYPE, "application/json")], body) }
      }),
    );
    axum::serve(listener, app).await.unwrap();
  }));
  let input = ProtectedInputV3 {
    artifact_id: "controlled-octafile".to_owned(),
    size_bytes: bytes.len() as u64,
    sha256: digest(&bytes),
    media_type: "application/json".to_owned(),
    destination: "/octacity/protected/Octafile.json".to_owned(),
  };
  let transfers = vec![ProtectedInputTransferV3 {
    input: input.clone(),
    capability: ArtifactTransferCapability {
      url: format!("{origin}/input"),
      required_headers: BTreeMap::new(),
      expires_at_unix_ms: u64::MAX,
    },
  }];
  let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
  let platform = PlatformSpec {
    os: PlatformOs::Linux,
    architecture: PlatformArchitecture::Arm64,
  };
  let host = PlatformSpec {
    os: PlatformOs::Macos,
    architecture: PlatformArchitecture::Arm64,
  };
  let spec = JobSpecV3 {
    protocol_version: 3,
    job_id: uuid::Uuid::new_v4().to_string(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 120,
    execution_deadline_unix_ms: (now + 120) * 1000,
    source: SourceSpec {
      provider: manifest.name,
      plugin_version: manifest.version,
      plugin_sha256: manifest.sha256,
      revision,
      reference: None,
      parameters: BTreeMap::from([("url".to_owned(), json!(repository.canonicalize().unwrap()))]),
    },
    octa: OctaSpec {
      version: runner.capabilities.octa_version.clone(),
      runner_sha256: runner.sha256.clone(),
      runner_protocol: *runner.capabilities.runner_protocols.iter().max().unwrap(),
      event_schema: *runner.capabilities.event_schemas.iter().max().unwrap(),
      plugin_protocol: *runner.capabilities.plugin_protocols.iter().max().unwrap(),
      plugin_digests: runner
        .plugins
        .iter()
        .map(|(name, plugin)| (name.clone(), plugin.sha256.clone()))
        .collect(),
    },
    execution: ManagedOctaExecutionV3 {
      workspace_root: Some(FACTORY_WORKSPACE_ROOT.to_owned()),
      octafile_input: input.artifact_id.clone(),
      tasks: vec!["controlled".to_owned()],
      credential_profile: None,
      tool_control: Some(FactoryToolControlV3 {
        plugin: "codex".to_owned(),
        capability: "codex.blocking-pre-tool-authorization.v1".to_owned(),
        mode: FactoryToolControlModeV3::ToolRisk,
        response_limits: FactoryToolResponseLimitsV3 {
          per_call_bytes: 5,
          total_bytes: 5,
        },
      }),
    },
    runtime: RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode: ExecutionMode::Virtualization,
        host_platform: host,
        target_platform: platform,
        required_guarantees: guarantees_for(ExecutionMode::Virtualization),
        immutable_image: Some(required("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE")),
      },
      cpu_millis: 3000,
      memory_bytes: 1536 * 1024 * 1024,
      writable_disk_bytes: 128 * 1024 * 1024,
      timeout_seconds: 120,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 2,
      artifact_bytes: 2 * 1024 * 1024,
      report_count: 1,
      report_bytes: 1024 * 1024,
      single_output_bytes: 1024 * 1024,
    },
    factory: None,
    protected_inputs: ProtectedInputManifestV3 { inputs: vec![input] },
    permissions: permissions.clone(),
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
  };
  let signing = SigningKey::from_bytes(&[43; 32]);
  let payload = serde_json::to_vec(&spec).unwrap();
  let envelope = SignedEnvelope {
    key_id: "controlled-key".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: STANDARD.encode(&payload),
    signature: STANDARD.encode(signing.sign(&payload).to_bytes()),
  };
  let verified = verify_compatible_job_spec(
    &envelope,
    &BTreeMap::from([("controlled-key".to_owned(), signing.verifying_key())]),
    JobBinding {
      job_id: &spec.job_id,
      attempt: 1,
      now,
    },
  )
  .unwrap();
  let lease = LeaseAssignment {
    lease_id: "controlled-lease".to_owned(),
    job_id: spec.job_id.clone(),
    attempt: 1,
    fencing_token: "controlled-fence".to_owned(),
    issued_at: now,
    expires_at: now + 120,
    signed_job_spec: envelope,
    protected_inputs: transfers.clone(),
  };
  let registration = Registration {
    agent_id: "controlled-agent".to_owned(),
    registration_id: "controlled-registration".to_owned(),
    execution_contract_version: 3,
    max_retry_delay: Duration::from_secs(1),
  };
  let coordinator = Arc::new(ContractCoordinator {
    registration: registration.clone(),
    lease: lease.clone(),
    expected,
    requests: Mutex::new(vec![]),
  });
  let control_fixture = tempfile::Builder::new()
    .prefix("c-")
    .tempdir_in(work.parent().unwrap())
    .unwrap();
  let control = control_fixture.path().canonicalize().unwrap();
  std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
  let runtime = FactoryProtectedReadRuntime::new(
    FactoryProtectedReadInstallation {
      worker: worker_program,
      bridge: bridge_program,
      bridge_sha256: bridge_sha,
      platform: "linux-aarch64".to_owned(),
      hook_tool_name: "mcp__qualified_fs__read_file".to_owned(),
      recipe,
    },
    permissions.clone(),
    control.clone(),
  )
  .unwrap();
  let owner = runtime.bind(coordinator.clone(), registration, lease).unwrap();
  let engine = MicrosandboxEngine::new(MicrosandboxEngineConfig {
    agent_id: "controlled-signed-contract".to_owned(),
    state_root: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT")),
    work_root: work.clone(),
    runner_platform: runner.capabilities.platform.clone(),
    executable: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE")),
    libkrunfw: PathBuf::from(required("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW")),
    cleanup_timeout: Duration::from_secs(10),
    metrics_sample_interval: Duration::from_secs(1),
  })
  .unwrap();
  let backend: Arc<dyn ExecutionBackend> = Arc::new(OciBackend::new(vec![Arc::new(engine)]).unwrap());
  let executor = JobExecutor::new(
    runner,
    Arc::new(SourcePluginRegistry::discover(&source_root).unwrap()),
    Arc::new(FileWorkloadIdentityProvider::default()),
    BTreeMap::new(),
    JobExecutorConfig {
      work_root: work,
      max_workspace_bytes: 128 * 1024 * 1024,
      allow_unrestricted_network: false,
      allowed_network_hosts: vec![],
      max_output_limits: spec.outputs.clone(),
      cancellation_grace: Duration::from_secs(10),
      runner_supervision: RunnerSupervisionPolicy {
        hello_timeout: Duration::from_secs(15),
        resource_sample_interval: Duration::from_secs(1),
        resource_sample_timeout: Duration::from_secs(15),
        max_accounting_failures: 3,
      },
      external_executables: external,
      factory_permissions: Some(permissions),
    },
  )
  .unwrap()
  .with_execution_backends([ExecutionBackendRoute::new(
    ExecutionCapabilityV2 {
      provider: ExecutionProviderId::new("microsandbox").unwrap(),
      mode: ExecutionMode::Virtualization,
      host_platform: host,
      target_platform: platform,
      guarantees: guarantees_for(ExecutionMode::Virtualization),
      immutable_images: true,
    },
    ExecutionEnvironmentId::new("controlled-signed-contract").unwrap(),
    backend,
  )
  .unwrap()
  .with_factory_enforcement(FactoryEnforcementCapabilityV3::ALL)
  .unwrap()])
  .unwrap()
  .with_protected_input_stager(
    ProtectedInputStager::new(ProtectedInputStagerConfig {
      allowed_origins: vec![origin],
      download_timeout: Duration::from_secs(10),
    })
    .unwrap(),
  );
  let (events, mut receiver) = tokio::sync::mpsc::channel(128);
  let drain = tokio::spawn(async move {
    let mut items = Vec::new();
    while let Some(event) = receiver.recv().await {
      items.push(event);
    }
    items
  });
  let completion = executor
    .execute_with_factory_tools(
      ExecuteJobRequest {
        spec: verified,
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: transfers,
      },
      owner,
      CancellationToken::new(),
      &events,
    )
    .await;
  let completion = match completion {
    Ok(value) => value,
    Err(failure) => {
      failure.cleanup().await.unwrap();
      panic!("actual controlled Job failed; private details are withheld")
    }
  };
  let result = completion.runner().clone();
  let published = completion.publication_workspace().to_owned();
  let records = read_files(&published);
  completion.cleanup().await.unwrap();
  drop(events);
  let events = drain.await.unwrap();
  drop(server);
  assert_eq!(result.status, RunStatus::Succeeded);
  let observed = records.iter().find_map(|bytes| {
    serde_json::from_slice::<serde_json::Value>(bytes)
      .ok()?
      .get("structured_result")
      .cloned()
  });
  assert_eq!(observed, Some(json!({"observed":"facts"})));
  assert!(!format!("{result:?}{events:?}").contains("unselected-source-canary"));
  for bytes in records {
    assert!(!String::from_utf8_lossy(&bytes).contains("unselected-source-canary"));
  }
  let requests = coordinator.requests.lock().unwrap();
  assert_eq!(
    requests.len(),
    2,
    "hook and physical call each consume one independent bounded authorization"
  );
  assert_ne!(requests[0], requests[1]);
  assert_eq!(
    std::fs::read_dir(&control).unwrap().count(),
    0,
    "the execution owner removes its private endpoint and directory"
  );
}
fn read_files(root: &Path) -> Vec<Vec<u8>> {
  let mut files = Vec::new();
  for entry in std::fs::read_dir(root).unwrap() {
    let entry = entry.unwrap();
    if entry.file_type().unwrap().is_dir() {
      files.extend(read_files(&entry.path()));
    } else {
      files.push(std::fs::read(entry.path()).unwrap());
    }
  }
  files
}
