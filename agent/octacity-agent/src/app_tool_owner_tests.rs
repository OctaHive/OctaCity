#[cfg(all(unix, any(target_os = "macos", target_os = "linux")))]
mod tool_owner {
  use super::*;
  use base64::{Engine as _, engine::general_purpose::STANDARD};
  use ed25519_dalek::{Signer as _, SigningKey};
  use octacity_lifecycle::FactoryToolExecutionProvider;
  use octacity_protocol::*;
  use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
  };
  struct OwnerProvider(Mutex<Vec<(String, String, u32, String)>>);
  impl FactoryToolExecutionProvider for OwnerProvider {
    fn bind(
      &self,
      registration: &octacity_coordinator::Registration,
      lease: &LeaseAssignment,
    ) -> Arc<dyn octacity_job::FactoryToolExecutionOwner> {
      self.0.lock().unwrap().push((
        registration.registration_id.clone(),
        lease.job_id.clone(),
        lease.attempt,
        lease.fencing_token.clone(),
      ));
      Arc::new(OrdinaryOwner)
    }
  }
  struct OrdinaryOwner;
  impl octacity_job::FactoryToolExecutionOwner for OrdinaryOwner {
    fn prepare(
      &self,
      _: octacity_job::FactoryToolExecutionScope,
    ) -> Result<Arc<dyn octacity_execution::ExecutionBackend>, octacity_job::FactoryPreflightError> {
      panic!("ordinary Jobs never start controlled execution")
    }
  }
  #[tokio::test]
  async fn normal_daemon_hands_its_current_lease_to_the_installed_tool_provider() {
    let _guard = crate::composition::tests::component_graph_guard().await;
    let fixture = crate::composition::tests::inventory_only_agent_fixture("https://coordinator.example");
    let mut components = Components::load(&fixture.config).await.unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let spec = JobSpecV1 {
      protocol_version: AGENT_PROTOCOL_VERSION,
      job_id: "ordinary-provider-handoff".to_owned(),
      attempt: 1,
      issued_at: now.saturating_sub(1),
      expires_at: now + 60,
      source: SourceSpec {
        provider: "installed-source".to_owned(),
        plugin_version: "1".to_owned(),
        plugin_sha256: "a".repeat(64),
        revision: "1".to_owned(),
        reference: None,
        parameters: BTreeMap::new(),
      },
      octa: OctaSpec {
        version: components.runner.capabilities.octa_version.clone(),
        runner_sha256: components.runner.sha256.clone(),
        runner_protocol: *components.runner.capabilities.runner_protocols.iter().max().unwrap(),
        event_schema: *components.runner.capabilities.event_schemas.iter().max().unwrap(),
        plugin_protocol: *components.runner.capabilities.plugin_protocols.iter().max().unwrap(),
        plugin_digests: BTreeMap::new(),
      },
      execution: ExecutionSpec {
        octafile: Some("Octafile.yml".to_owned()),
        commands: vec!["installed-task".to_owned()],
        variables: BTreeMap::new(),
        arguments: Vec::new(),
        concurrency: None,
        parallel: false,
        failfast: true,
        secrets_profile: None,
      },
      runtime: RuntimeSpec {
        target: RuntimeTarget::Native {
          platform: PlatformSpec {
            os: PlatformOs::Linux,
            architecture: PlatformArchitecture::Arm64,
          },
        },
        cpu_millis: 1000,
        memory_bytes: 268435456,
        writable_disk_bytes: 1048576,
        timeout_seconds: 30,
        network: NetworkPolicy::Disabled,
        workload_identity_profile: None,
      },
      cache: None,
      outputs: OutputLimits {
        artifact_count: 0,
        artifact_bytes: 0,
        report_count: 0,
        report_bytes: 0,
        single_output_bytes: 0,
      },
    };
    let signing = SigningKey::from_bytes(&[17; 32]);
    let payload = serde_json::to_vec(&spec).unwrap();
    components
      .validated
      .signing_keys
      .insert("tool-handoff".to_owned(), signing.verifying_key());
    let lease = LeaseAssignment {
      lease_id: "normal-tool-lease".to_owned(),
      job_id: spec.job_id.clone(),
      attempt: 1,
      fencing_token: "current-fence".to_owned(),
      issued_at: now,
      expires_at: now + 60,
      signed_job_spec: SignedEnvelope {
        key_id: "tool-handoff".to_owned(),
        algorithm: SIGNATURE_ALGORITHM.to_owned(),
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(signing.sign(&payload).to_bytes()),
      },
      protected_inputs: Vec::new(),
    };
    let provider = Arc::new(OwnerProvider(Mutex::new(Vec::new())));
    let expected = (
      "registration-coverage".to_owned(),
      lease.job_id.clone(),
      1,
      lease.fencing_token.clone(),
    );
    let coordinator = Arc::new(installed_agent::DrainCoordinator {
      assignment: Mutex::new(Some(lease)),
      ..Default::default()
    });
    components.coordinator = coordinator.clone();
    components.factory_tools = Some(provider.clone());
    run_loaded(&mut components, CancellationToken::new()).await.unwrap();
    assert_eq!(*provider.0.lock().unwrap(), vec![expected]);
    let completed = coordinator.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(
      completed[0].status,
      JobCompletionStatus::InfrastructureFailed,
      "the fixture deliberately has no Native backend and grants no execution"
    );
  }
}
