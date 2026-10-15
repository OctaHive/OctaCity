use crate::factory_node_test_support::fixtures;
use crate::*;
use octacity_server_factory::*;
use std::collections::BTreeMap;

pub(crate) fn command_fixture() -> (FlowBuildIntent, AdmittedFlow, FactoryCommandBuildProfile) {
  let work = fixtures::work();
  let mut binding = fixtures::binding(work.subject().project_id());
  let (_, _, schema, _, _) = fixtures::execution_fixture(FlowNodeKind::BuildCommand);
  let profile = FactoryCommandBuildProfile {
    identity: binding.model_or_tool.clone(),
    plugin: binding.plugin.clone(),
    executor: binding.tool.clone(),
    build_configuration: binding.build_configuration.clone(),
    pipeline_node: octacity_server_domain::PipelineNodeId::new("installed-command").unwrap(),
    program: FactoryCommandProgram {
      octafile: FactoryCommandOctafile::new(serde_json::json!({"version":1,"tasks":{"observe":{"shell":"printf '{\"count\":9}' > output/audit.report.json","reports":[{"name":binding.result_output,"path":"output/audit.report.json","format":"json"}]}}}).to_string()).unwrap(),
      tasks: vec!["observe".into()], workspace_root: Some(octacity_protocol::FACTORY_WORKSPACE_ROOT.to_owned()), credential_profile: None,
    },
    input_destination: "/octacity/protected/command-input.json".into(),
    parameters: BTreeMap::new(),
    project_permissions: FactoryPermissionSet::deny_all(),
    local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
    candidate_capture_tool: None,
    outputs: vec![EvidenceRequirement::new(binding.result_output.clone(), binding.result_output_kind, schema.reference().clone(), binding.tool.clone(), binding.plugin.clone())],
  };
  binding.task_digest = profile.task_digest();
  let (_, admitted, _, input, node) = fixtures::execution_fixture_with_permissions(
    FlowNodeKind::BuildCommand,
    work,
    binding,
    None,
    FactoryPermissionSet::deny_all(),
  );
  (
    FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap(),
    admitted,
    profile,
  )
}

#[test]
fn installed_command_compilation_pins_program_and_frozen_input_without_stage_names() {
  let (intent, admitted, profile) = command_fixture();
  let compiled = compile_factory_command_task(&intent, &admitted, &profile).unwrap();
  assert_eq!(compiled.documents().len(), 2);
  let mut replaced = profile.clone();
  replaced.program.octafile = FactoryCommandOctafile::new(r#"{"version":1,"tasks":{}}"#).unwrap();
  assert_ne!(profile.task_digest(), replaced.task_digest());
  assert!(compile_factory_command_task(&intent, &admitted, &replaced).is_err());
}

#[test]
fn command_materializer_rejects_expired_owner_before_resolving_or_publishing() {
  use async_trait::async_trait;
  use octacity_artifact_store::*;
  use octacity_server_domain::*;
  use octacity_server_store::*;
  use std::sync::Arc;
  struct Contexts;
  #[async_trait]
  impl BuildExecutionContextSource for Contexts {
    async fn load(
      &self,
      _: ProjectId,
      _: BuildConfigurationId,
      _: BuildConfigurationVersion,
    ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
      panic!("expired authority must not resolve context")
    }
  }
  struct Inputs;
  #[async_trait]
  impl FactoryExecutionInputStore for Inputs {
    async fn reserve_factory_execution_inputs(
      &self,
      _: PublishFactoryExecutionInputs,
    ) -> Result<FactoryExecutionInputDisposition, StoreError> {
      panic!("expired authority must not publish")
    }
    async fn publish_factory_execution_inputs(&self, _: PublishFactoryExecutionInputs) -> Result<(), StoreError> {
      panic!("expired authority must not publish")
    }
  }
  struct Bytes;
  #[async_trait]
  impl ArtifactBytePublisher for Bytes {
    async fn publish_verified(&self, _: &ArtifactObject, _: Vec<u8>) -> Result<(), ArtifactStoreError> {
      panic!("expired authority must not publish")
    }
  }
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(Timestamp::from_unix_millis(100).unwrap())
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, profile) = command_fixture();
    let materializer = FactoryCommandBuildMaterializer::new(
      Arc::new(Contexts),
      Arc::new(FactoryExecutionInputPublisher::new(
        Arc::new(Inputs),
        Arc::new(Bytes),
        Arc::new(Clock),
      )),
      vec![profile],
      Arc::new(Clock),
    )
    .unwrap();
    let ownership = FactoryClaimOwnership::new(
      fixtures::key("worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([64; 32])),
        Timestamp::from_unix_millis(10).unwrap(),
        Timestamp::from_unix_millis(100).unwrap(),
      )
      .unwrap(),
    );
    assert!(matches!(
      materializer
        .materialize(&intent, &admitted, ownership, Timestamp::from_unix_millis(100).unwrap())
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
  });
}

#[test]
fn installed_command_capture_verifier_pin_changes_its_task_authority() {
  let (_, _, mut profile) = command_fixture();
  let original = profile.task_digest();
  profile.candidate_capture_tool = Some(fixtures::reference("trusted-candidate-capture"));
  assert_ne!(
    original,
    profile.task_digest(),
    "retained candidate verifier cannot change under an existing command pin"
  );
}

#[test]
fn validation_command_documents_include_the_independently_derived_report_contract() {
  use async_trait::async_trait;
  use octacity_artifact_store::*;
  use octacity_server_store::*;
  use std::sync::{Arc, Mutex};
  struct Inputs;
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
      request.validate()
    }
  }
  #[derive(Default)]
  struct Bytes(Mutex<Vec<Vec<u8>>>);
  #[async_trait]
  impl ArtifactBytePublisher for Bytes {
    async fn publish_verified(&self, _: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
      self.0.lock().unwrap().push(bytes);
      Ok(())
    }
  }
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
      Ok(octacity_server_domain::Timestamp::from_unix_millis(41).unwrap())
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let (_, _, mut profile) = command_fixture();
    let fixture = crate::factory_protected_validation_input_source_tests::retained_validation_fixture(
      false,
      6,
      |_, binding, permissions| {
        profile.identity = binding.model_or_tool.clone();
        profile.plugin = binding.plugin.clone();
        profile.executor = binding.tool.clone();
        profile.build_configuration = binding.build_configuration.clone();
        profile.project_permissions = permissions.clone();
        profile.local_permissions = LocalPermissionCeiling::fully_enforced(permissions.clone());
        binding.task_digest = profile.task_digest();
      },
    )
    .await;
    let compiled = compile_factory_command_task(&fixture.intent, &fixture.admitted, &profile)
      .unwrap()
      .with_protected_validation(&fixture.materialized)
      .unwrap();
    assert_eq!(
      compiled.documents().len(),
      3,
      "one trusted invocation descriptor supplements program and frozen operation"
    );
    let bytes = Arc::new(Bytes::default());
    let mut manifest = FactoryExecutionInputPublisher::new(Arc::new(Inputs), bytes.clone(), Arc::new(Clock))
      .publish(
        &fixture.intent,
        &FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim()),
        compiled.documents().to_vec(),
      )
      .await
      .unwrap();
    let published = bytes.0.lock().unwrap();
    let value: serde_json::Value = published
      .iter()
      .filter_map(|body| serde_json::from_slice::<serde_json::Value>(body).ok())
      .find(|value| value.get("input_digest").is_some() && value.get("classes").is_some())
      .expect("trusted validation metadata is actually published as a private document");
    assert_eq!(value.as_object().unwrap().len(), 5);
    let report = FactoryProtectedValidationReport {
      changeset_id: serde_json::from_value(value["changeset_id"].clone()).unwrap(),
      candidate: serde_json::from_value(value["candidate"].clone()).unwrap(),
      tests: serde_json::from_value(value["tests"].clone()).unwrap(),
      input_digest: serde_json::from_value(value["input_digest"].clone()).unwrap(),
      classes: serde_json::from_value::<Vec<FactoryKey>>(value["classes"].clone())
        .unwrap()
        .into_iter()
        .map(|class| FactoryProtectedValidationClassResult {
          class,
          outcome: DeterministicGateOutcome::Passed,
        })
        .collect(),
    };
    assert_eq!(
      fixture.materialized.verify_report(&report).unwrap(),
      DeterministicGateOutcome::Passed
    );
    assert!(
      published
        .iter()
        .all(|body| !String::from_utf8_lossy(body).contains("PRIVATE_EXPECTATION")),
      "private source bodies never enter generated metadata"
    );
    drop(published);
    manifest.inputs.extend_from_slice(compiled.additional_inputs());
    manifest
      .inputs
      .sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    assert!(
      compiled.managed_program(manifest).is_ok(),
      "the exact original inputs and descriptor form one closed signed program"
    );
  });
}

fn installation_fixture() -> (FactoryCommandBuildProfile, JobSpecToolchainPolicy, ImmutableReference) {
  let (_, _, mut profile) = command_fixture();
  crate::factory_command_output_source_tests::configure_profile(&mut profile, &fixtures::work());
  let additional = fixtures::reference("independent-installed-plugin");
  let canonical = crate::factory_job_test_support::canonical_v3_spec();
  let mut approved = JobSpecToolchainPolicy {
    source: octacity_server_job::SourcePluginPolicy::new("git", "0.1.0", canonical.source.plugin_sha256, "url")
      .unwrap(),
    octa: canonical.octa,
    validity: octacity_server_job::JobSpecValidity::new(3600).unwrap(),
  };
  approved.octa.plugin_digests = [profile.plugin.clone(), additional.clone()]
    .into_iter()
    .map(|plugin| (plugin.identity().to_string(), plugin.digest().to_string()))
    .collect();
  (profile, approved, additional)
}
fn set_installed_plugins(profile: &mut FactoryCommandBuildProfile, plugins: Vec<ImmutableReference>) {
  let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
  grants["plugins"] = serde_json::json!(plugins);
  profile.project_permissions = serde_json::from_value(grants).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
}
#[test]
fn builtin_command_qualifies_the_exact_installed_plugin_inventory_without_other_authority() {
  let (mut profile, approved, additional) = installation_fixture();
  let primary = profile.plugin.clone();
  let before = profile.task_digest();
  set_installed_plugins(&mut profile, vec![primary, additional]);
  assert_ne!(
    before,
    profile.task_digest(),
    "explicit operator inventory approval must repin the command"
  );
  assert!(profile.project_permissions.executables().len() == 0);
  assert!(profile.project_permissions.tools().len() == 0);
  assert!(profile.project_permissions.secret_profiles().len() == 0);
  profile
    .validate_installation(&approved)
    .expect("the immutable builtin program uses its primary plugin within an exactly approved runtime inventory");
}
#[test]
fn builtin_command_rejects_missing_extra_or_replaced_inventory_in_either_ceiling() {
  let (mut profile, approved, additional) = installation_fixture();
  assert!(
    profile.validate_installation(&approved).is_err(),
    "a signed subset cannot satisfy the installed Octa.lock"
  );
  let primary = profile.plugin.clone();
  set_installed_plugins(
    &mut profile,
    vec![
      primary.clone(),
      additional.clone(),
      fixtures::reference("unapproved-plugin"),
    ],
  );
  assert!(profile.validate_installation(&approved).is_err());
  let replaced = ImmutableReference::new(
    additional.identity().clone(),
    additional.version().clone(),
    FactoryDigest::content_sha256(b"replaced plugin bytes"),
  );
  set_installed_plugins(&mut profile, vec![primary.clone(), replaced]);
  assert!(profile.validate_installation(&approved).is_err());
  set_installed_plugins(&mut profile, vec![primary.clone(), additional]);
  let approved_local = profile.local_permissions.clone();
  let mut subset = serde_json::to_value(&profile.project_permissions).unwrap();
  subset["plugins"] = serde_json::json!([primary]);
  let subset = serde_json::from_value(subset).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(subset);
  assert!(
    profile.validate_installation(&approved).is_err(),
    "local inventory must independently match, even when project inventory is complete"
  );
  profile.local_permissions = approved_local;
  profile.validate_installation(&approved).unwrap();
}

#[test]
fn installed_command_program_accepts_bounded_source_without_changing_ordinary_factory_text() {
  let wire = |source: &str| {
    serde_json::json!({
      "octafile":source,"tasks":["deploy"],"workspace_root":null,"credential_profile":null
    })
  };
  let source = format!(
    "version: 1\ntasks:\n  deploy:\n    shell: true\n# {}\n",
    "x".repeat(4096)
  );
  let program: FactoryCommandProgram = serde_json::from_value(wire(&source))
    .expect("operator-installed source has its own bound and accepts YAML line breaks");
  assert_eq!(program.octafile.as_str(), source);
  assert_eq!(serde_json::to_value(&program).unwrap(), wire(&source));
  assert!(FactoryText::new("x".repeat(4097)).is_err());
  assert!(FactoryText::new("two\nlines").is_err());
  for source in ["x".repeat(65536), "é".repeat(32768)] {
    assert!(serde_json::from_value::<FactoryCommandProgram>(wire(&source)).is_ok());
  }
  for source in [
    String::new(),
    " \n\t".into(),
    "x".repeat(65537),
    "é".repeat(32769),
    "secret\0source".into(),
  ] {
    assert!(serde_json::from_value::<FactoryCommandProgram>(wire(&source)).is_err());
  }
  let original = wire(r#"{"version":1,"tasks":{"deploy":{"shell":"true"}}}"#);
  let program: FactoryCommandProgram = serde_json::from_value(original.clone()).unwrap();
  assert_eq!(serde_json::to_value(program).unwrap(), original);
}
