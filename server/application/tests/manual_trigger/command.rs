//! Installed static command programs create ordinary immutable Jobs without model credentials.
use super::*;
use factory::*;

#[test]
fn managed_program_preserves_exact_preincluded_candidate_inputs_without_duplication() {
  let (mut input, candidate, bundle, manifest) = factory_candidate_materialization_fixture();
  let proof = FactoryManagedCandidateProof::verify(
    input.intent.input(),
    candidate.clone(),
    bundle.clone(),
    manifest.clone(),
    time(12),
  )
  .unwrap();
  let original = factory_node_candidate_materialization(input.intent.input(), &candidate, &bundle, &manifest, time(12))
    .unwrap()
    .wire_inputs()
    .1;
  for program in input.programs.values_mut() {
    program.protected_inputs.inputs.extend(original.clone());
    program
      .protected_inputs
      .inputs
      .sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    program.protected_inputs.validate().unwrap();
  }
  let plan = FactoryManagedSourcePlan {
    command: None,
    candidate: Some(proof),
    capture: None,
  };
  let accepted = materialize_factory_managed_build_with_source(input.clone(), plan.clone())
    .expect("a complete compiler manifest may already contain the independently verified candidate inputs");
  accepted.validate_fresh().unwrap();
  for job in &accepted.jobs {
    let manifest = job.job_spec_template.managed_intent().unwrap().protected_inputs();
    for expected in &original {
      assert_eq!(manifest.inputs.iter().filter(|input| *input == expected).count(), 1);
    }
  }
  let mut changed = input;
  let program = changed.programs.values_mut().next().unwrap();
  let conflicting = program
    .protected_inputs
    .inputs
    .iter_mut()
    .find(|input| input.artifact_id == original[0].artifact_id)
    .unwrap();
  conflicting.sha256 = FactoryDigest::from_bytes([101; 32]).to_string();
  assert!(
    materialize_factory_managed_build_with_source(changed, plan).is_err(),
    "preincluded metadata cannot substitute the trusted candidate proof"
  );
}

#[test]
fn installed_command_materializes_a_builtin_plugin_job_without_phantom_external_executables() {
  run(async {
    let (base, _, work, mut binding) = factory_materialization_fixture();
    let mut context = base.context;
    let node = context.pipeline.dag.nodes()[0].clone();
    context.pipeline.dag = PublishablePipelineDag::new(
      vec![node.clone()],
      vec![],
      &CapabilityCatalog::new(node.required_capabilities().iter().cloned()),
    )
    .unwrap()
    .into_dag();
    context.configuration.definition.runtime.network = ConfigurationNetworkPolicy::Disabled;
    context.configuration.definition.secrets_profile = None;
    binding.plugin = managed_fixture::reference("shell", "1.0.0");
    binding.tool = flow_fixture::reference("installed-command");
    context
      .job_spec_toolchain
      .octa
      .plugin_digests
      .insert("shell".into(), binding.plugin.digest().to_string());
    let approved_plugins = context.job_spec_toolchain.octa.plugin_digests.clone();
    assert_eq!(
      approved_plugins,
      BTreeMap::from([
        (
          "codex".to_owned(),
          "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned()
        ),
        (
          "shell".to_owned(),
          "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned()
        ),
      ]),
      "this installation explicitly approves the existing Codex and primary shell runtime pins"
    );
    let runtime_plugins = approved_plugins
      .iter()
      .map(|(identity, digest)| {
        ImmutableReference::new(
          FactoryKey::new(identity).unwrap(),
          binding.plugin.version().clone(),
          digest.parse().unwrap(),
        )
      })
      .collect::<Vec<_>>();
    let mut permissions = serde_json::to_value(base.policy_layers.task).unwrap();
    permissions["plugins"] = json!(runtime_plugins);
    for category in [
      "executables",
      "commands",
      "tools",
      "network_hosts",
      "secret_profiles",
      "workload_identity_profiles",
    ] {
      permissions[category] = json!([]);
    }
    permissions["outputs"]["kinds"] = json!([binding.result_output.clone()]);
    for mount in permissions["mounts"].as_array_mut().unwrap() {
      if mount["root"] == "/workspace/source" {
        mount["mode"] = json!("read_only");
      }
    }
    let permissions: FactoryPermissionSet = serde_json::from_value(permissions).unwrap();
    let (_, _, schema, _, _) = flow_fixture::execution_fixture(FlowNodeKind::BuildCommand);
    let profile = FactoryCommandBuildProfile {
      identity: binding.model_or_tool.clone(),
      plugin: binding.plugin.clone(),
      executor: binding.tool.clone(),
      build_configuration: binding.build_configuration.clone(),
      pipeline_node: node.id().clone(),
      program: FactoryCommandProgram {
        octafile: FactoryCommandOctafile::new(
          serde_json::json!({"version":1,"tasks":{"observe":{
            "shell":"printf '{\"count\":9}' > output/audit.report.json",
            "reports":[{"name":binding.result_output.as_str(),"path":"output/audit.report.json","format":"json"}]
          }}})
          .to_string(),
        )
        .unwrap(),
        tasks: vec!["observe".into()],
        workspace_root: Some(octacity_protocol::FACTORY_WORKSPACE_ROOT.into()),
        credential_profile: None,
      },
      input_destination: "/octacity/protected/command-input.json".into(),
      parameters: base.parameters,
      project_permissions: permissions.clone(),
      local_permissions: LocalPermissionCeiling::fully_enforced(permissions.clone()),
      outputs: vec![EvidenceRequirement::new(
        binding.result_output.clone(),
        binding.result_output_kind,
        schema.reference().clone(),
        binding.tool.clone(),
        binding.plugin.clone(),
      )],
      candidate_capture_tool: None,
    };
    profile
      .validate_installation(&context.job_spec_toolchain)
      .expect("installed hermetic command plugin and grants qualify");
    binding.task_digest = profile.task_digest();
    let (_, admitted, _, input, previous) =
      flow_fixture::execution_fixture_with_permissions(FlowNodeKind::BuildCommand, work, binding, None, permissions);
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      admitted.closure().definition(input.definition()).unwrap(),
      NodeAttemptInput {
        id: previous.id(),
        node_key: input.node().clone(),
        node_kind: FlowNodeKind::BuildCommand,
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
    let inputs = Arc::new(codex_capture::Inputs::default());
    let clock = Arc::new(codex_capture::Clock);
    let publisher = Arc::new(FactoryExecutionInputPublisher::new(
      inputs.clone(),
      Arc::new(codex_capture::Bytes),
      clock.clone(),
    ));
    let compiler = FactoryCommandBuildMaterializer::new(
      Arc::new(codex_capture::Context(context)),
      publisher,
      vec![profile],
      clock,
    )
    .unwrap();
    let outcome = compiler.materialize(&intent, &admitted, base.ownership, time(12)).await;
    assert!(
      outcome.is_ok(),
      "materialization failed after {} publications: {:?}",
      inputs.0.lock().unwrap().len(),
      outcome.as_ref().err()
    );
    let accepted = outcome.unwrap();
    accepted.validate_fresh().unwrap();
    assert_eq!(accepted.jobs.len(), 1);
    assert_eq!(
      accepted.jobs[0]
        .job_spec_template
        .placement_policy()
        .octa
        .plugin_digests,
      approved_plugins,
      "the signed runtime inventory matches every explicitly approved plugin pin"
    );
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert!(managed.permissions().executables.is_empty());
    assert!(managed.permissions().secret_profiles.is_empty());
    assert_eq!(managed.execution().tasks, ["observe"]);
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
    assert_eq!(managed.protected_inputs().inputs.len(), 2);
  });
}
