use super::*;
use factory::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
use octacity_protocol::{
  CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT, ChangeSetBinaryPolicyV3, ChangeSetCapturePolicyV3,
  ChangeSetCaptureV3, ChangeSetSecretPatternV3,
};
use octacity_server_store::{
  FactoryExecutionInputDisposition, FactoryExecutionInputStore, PublishFactoryExecutionInputs,
};

pub(super) struct Context(pub(super) BuildExecutionContext);
#[async_trait]
impl BuildExecutionContextSource for Context {
  async fn load(
    &self,
    project: ProjectId,
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
    assert_eq!(
      (project, id, version),
      (
        self.0.configuration.project_id,
        self.0.configuration.id,
        self.0.configuration.version
      )
    );
    Ok(self.0.clone())
  }
}

#[derive(Default)]
pub(super) struct Inputs(pub(super) Mutex<Vec<PublishFactoryExecutionInputs>>);
#[async_trait]
impl octacity_server_store::FactoryRoleInvocationStore for Inputs {}
#[async_trait]
impl FactoryExecutionInputStore for Inputs {
  async fn reserve_factory_execution_inputs(
    &self,
    request: PublishFactoryExecutionInputs,
  ) -> Result<FactoryExecutionInputDisposition, StoreError> {
    request.validate()?;
    Ok(FactoryExecutionInputDisposition::Pending)
  }
  async fn publish_factory_execution_inputs(&self, request: PublishFactoryExecutionInputs) -> Result<(), StoreError> {
    self.0.lock().unwrap().push(request);
    Ok(())
  }
}
pub(super) struct Bytes;
#[async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, _: &ArtifactObject, _: Vec<u8>) -> Result<(), ArtifactStoreError> {
    Ok(())
  }
}
pub(super) struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(time(12))
  }
}
struct Selected(octacity_server_domain::ArtifactId, Vec<u8>);
#[async_trait]
impl FactoryNodeContextSource for Selected {
  async fn read_context(
    &self,
    input: &FlowNodeInput,
    _: Timestamp,
    _: Timestamp,
  ) -> Result<BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>, ApplicationError> {
    assert_eq!(input.context().unwrap().entries().len(), 1);
    Ok(BTreeMap::from([(self.0, self.1.clone())]))
  }
}

#[test]
fn installed_codex_author_materializes_only_the_explicit_trusted_capture_scope_without_tools() {
  run(async {
    let (base, _, original, mut binding) = factory_materialization_fixture();
    let text = b"Produce the selected public specification.".to_vec();
    let artifact = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::content_sha256(&text),
      text.len() as u64,
    )
    .unwrap();
    let work = WorkEnvelope::new(
      original.id(),
      original.configuration().clone(),
      original.external_identity().clone(),
      original.subject().clone(),
      WorkArtifacts::new(artifact.clone(), original.artifacts().acceptance().clone(), vec![]).unwrap(),
      WorkClassification::new(original.priority(), original.risk(), original.metadata().clone()),
    )
    .unwrap();
    let mut context = base.context;
    let node = context.pipeline.dag.nodes()[0].clone();
    context.pipeline.dag = PublishablePipelineDag::new(
      vec![node.clone()],
      vec![],
      &CapabilityCatalog::new(node.required_capabilities().iter().cloned()),
    )
    .unwrap()
    .into_dag();
    let mut grants = serde_json::to_value(base.policy_layers.task).unwrap();
    grants["outputs"]["kinds"] = json!([
      "change-set.bundle",
      "change-set-manifest.json",
      "codex-run-provenance",
      "codex-run-result",
      "codex-run-trace"
    ]);
    let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
    let instructions = FactoryText::new("Write only the selected public specification.").unwrap();
    let credentials = FactoryCredentialProfiles::new(
      flow_fixture::key("model-coding"),
      flow_fixture::key("model-evaluation"),
      flow_fixture::key("source-read"),
      flow_fixture::key("delivery-write"),
    )
    .unwrap();
    binding.result_output = flow_fixture::key("codex-run-result");
    binding.capture_write_paths = Some(vec![FactoryRepositoryPath::new("public/approved").unwrap()]);
    let tool = flow_fixture::reference("git.capture");
    binding.evidence = [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
      .into_iter()
      .map(|name| {
        EvidenceRequirement::new(
          flow_fixture::key(name),
          EvidenceOutputKind::Artifact,
          flow_fixture::reference("capture.artifacts"),
          tool.clone(),
          binding.plugin.clone(),
        )
      })
      .collect();
    let capture = ChangeSetCaptureV3 {
      author_name: "Installed operator".to_owned(),
      author_email: "operator@example.test".to_owned(),
      committed_at: 1,
      patch_max_bytes: None,
      policy: ChangeSetCapturePolicyV3 {
        allowed_path_prefixes: vec!["public/approved".to_owned()],
        forbidden_control_paths: vec![".github/workflows".to_owned()],
        max_changed_paths: 2,
        max_file_bytes: 128,
        max_total_bytes: 256,
        allow_empty: false,
        binary_policy: ChangeSetBinaryPolicyV3::Reject,
        forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
      },

      observation: None,
    };
    let profile = FactoryCodexBuildProfile {
      mcp_configurations: vec![],
      toolchain: FactoryTaskToolchain {
        executable: binding.tool.clone(),
        plugin: binding.plugin.clone(),
        model: binding.model_or_tool.clone(),
      },
      build_configuration: binding.build_configuration.clone(),
      instructions,
      credentials,
      consumer: FactoryCredentialConsumer::CodingNode,
      usage_policy: FactoryCodexUsagePolicy::ReportedCost,
      tool_risk: None,
      tool_response_limits: None,
      reviewer: None,
      candidate_capture_tool: None,
      capture: Some(FactoryCodexCapturePolicy::new(tool.clone(), capture.clone()).unwrap()),
      pipeline_node: node.id().clone(),
      parameters: base.parameters,
      project_permissions: grants.clone(),
      local_permissions: LocalPermissionCeiling::fully_enforced(grants.clone()),
    };
    let mut changed_capture = capture.clone();
    changed_capture.committed_at = 2;
    let mut changed_profile = profile.clone();
    changed_profile.capture = Some(FactoryCodexCapturePolicy::new(tool, changed_capture).unwrap());
    assert_ne!(
      profile.task_digest(),
      changed_profile.task_digest(),
      "a changed trusted capture instruction must select a different immutable profile"
    );
    binding.task_digest = profile.task_digest();
    let (_, admitted, _, input, previous) =
      flow_fixture::execution_fixture_with_permissions(FlowNodeKind::Reasoning, work, binding, None, grants.clone());
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      admitted.closure().definition(input.definition()).unwrap(),
      NodeAttemptInput {
        id: previous.id(),
        node_key: input.node().clone(),
        node_kind: FlowNodeKind::Reasoning,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: previous.budget(),
        started_at: time(10),
        deadline: time(1010),
        execution: previous.execution().clone(),
        ownership: base.ownership.clone(),
      },
    )
    .unwrap();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 17).unwrap();
    let inputs = Arc::new(Inputs::default());
    let publisher = Arc::new(FactoryExecutionInputPublisher::new(
      inputs.clone(),
      Arc::new(Bytes),
      Arc::new(Clock),
    ));
    let materializer = FactoryCodexBuildMaterializer::new(
      Arc::new(Context(context)),
      publisher,
      vec![profile],
      Arc::new(Selected(artifact.artifact_id(), text)),
      Arc::new(Clock),
    )
    .unwrap();
    let accepted = materializer
      .materialize(&intent, &admitted, base.ownership, time(12))
      .await
      .unwrap();
    accepted.validate_fresh().unwrap();
    assert_eq!(accepted.jobs.len(), 1);
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert_eq!(
      managed.factory().source_write_paths.as_deref(),
      Some(["public/approved".to_owned()].as_slice())
    );
    assert_eq!(managed.factory().change_set_capture.as_ref(), Some(&capture));
    assert!(managed.factory().change_set_materialization.is_none());
    assert_eq!(managed.protected_inputs().inputs.len(), 4);
    assert_eq!(managed.permissions().secret_profiles, ["model-coding"]);
    assert!(managed.permissions().tools.is_empty());
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
  });
}
