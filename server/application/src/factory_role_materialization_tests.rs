use crate::factory_codex_tests::{credential_profiles, permissions, reference};
use crate::factory_command_materializer_tests::{Bytes, Context, Inputs, managed_context};
use crate::factory_node_test_support::fixtures;
use crate::*;
use octacity_server_domain::{ArtifactId, PipelineNodeId, Timestamp};
use octacity_server_factory::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn fixture_for_work(
  original: WorkEnvelope,
  edit: impl FnOnce(&mut FactoryRoleProfileDefinition),
  skills: BTreeMap<FactoryKey, FactoryRoleSkillBundle>,
  bind_task: fn(FactoryDigest, &FactoryRoleProfile) -> FactoryDigest,
) -> (
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCodexBuildProfile,
  FactoryRoleExecutionPackage,
  BTreeMap<ArtifactId, Vec<u8>>,
) {
  let task_bytes = b"Measure the selected code.".to_vec();
  let task = FactoryArtifactReference::new(
    ArtifactId::generate(),
    FactoryDigest::content_sha256(&task_bytes),
    task_bytes.len() as u64,
  )
  .unwrap();
  let work = WorkEnvelope::new(
    original.id(),
    original.configuration().clone(),
    original.external_identity().clone(),
    original.subject().clone(),
    WorkArtifacts::new(task.clone(), original.artifacts().acceptance().clone(), vec![]).unwrap(),
    WorkClassification::new(original.priority(), original.risk(), original.metadata().clone()),
  )
  .unwrap();
  let toolchain = FactoryTaskToolchain {
    plugin: reference("codex", 10),
    executable: reference("codex-cli", 11),
    model: reference("exact-model", 12),
  };
  let mut grants = serde_json::to_value(permissions(
    FactoryTaskMode::Evaluate,
    toolchain.plugin.clone(),
    toolchain.executable.clone(),
    true,
  ))
  .unwrap();
  grants["resources"]["elapsed_millis"] = json!(1000);
  grants["outputs"]["max_artifact_count"] = json!(8);
  grants["outputs"]["max_artifact_bytes"] = json!(1024);
  grants["outputs"]["max_report_count"] = json!(4);
  grants["outputs"]["max_report_bytes"] = json!(1024);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  let mut binding = fixtures::binding(work.subject().project_id());
  binding.plugin = toolchain.plugin.clone();
  binding.tool = toolchain.executable.clone();
  binding.model_or_tool = toolchain.model.clone();
  binding.result_output = key("codex-run-result");
  let installed = FactoryCodexBuildProfile {
    toolchain: toolchain.clone(),
    build_configuration: binding.build_configuration.clone(),
    instructions: FactoryText::new("Return the stage result.").unwrap(),
    credentials: credential_profiles(),
    consumer: FactoryCredentialConsumer::ReadOnlyReview,
    usage_policy: FactoryCodexUsagePolicy::ReportedCost,
    tool_risk: None,
    tool_response_limits: None,
    candidate_capture_tool: None,
    reviewer: None,
    capture: None,
    pipeline_node: PipelineNodeId::new("model-stage").unwrap(),
    parameters: BTreeMap::new(),
    project_permissions: grants.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(grants.clone()),
  };
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    work.subject().project_id(),
    serde_json::from_value(
      json!({"name":"Custom reviewer", "instructions":["Shared responsibility.", "Shared second rule."]}),
    )
    .unwrap(),
  )
  .unwrap();
  let mut definition = octacity_server_store::testing::role_profile_contract_definition(work.subject().project_id());
  definition.toolchain = toolchain.clone();
  definition.build_configuration = binding.build_configuration.clone();
  definition.permissions = grants.clone();
  definition.budget = BudgetLimit::new(1, 1000, 100, 1000, 10000).unwrap();
  definition.credential_purpose = installed.consumer.purpose();
  definition.skills.clear();
  definition.context_requirements.clear();
  definition.execution = FactoryRoleProfileExecution::default();
  definition.execution.environment.insert(
    "LANGUAGE".into(),
    FactoryRoleEnvironmentValue::Public { value: "ru".into() },
  );
  definition.model_parameters = [(key("reasoning_effort"), json!("high"))].into();
  definition.instructions = FactoryInstructions::new(vec!["Profile instructions.".into()]).unwrap();
  definition.skills = skills
    .iter()
    .map(|(alias, bundle)| (alias.clone(), bundle.reference().clone()))
    .collect();
  edit(&mut definition);
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    work.subject().project_id(),
    &role,
    definition,
  )
  .unwrap();
  binding.task_digest = bind_task(installed.task_digest(), &profile);
  let (_, admitted, _, input, node) =
    fixtures::execution_fixture_with_permissions(FlowNodeKind::Reasoning, work.clone(), binding, None, grants.clone());
  let rules = b"Project rules control all additions. Use TDD.".to_vec();
  let rules_ref = FactoryArtifactReference::new(
    ArtifactId::generate(),
    FactoryDigest::content_sha256(&rules),
    rules.len() as u64,
  )
  .unwrap();
  let old_context = input.context().unwrap();
  let mut entries = old_context.entries().to_vec();
  entries.push(
    ContextManifestEntry::new(
      ContextSourceKind::Policy,
      key("project-rules"),
      old_context.subject().clone(),
      FactoryContextReference::Artifact(rules_ref.clone()),
      rules_ref.content_digest(),
      rules_ref.encoded_size(),
      FactorySafeText::new("Project instructions").unwrap(),
      FactoryDigest::from_bytes([5; 32]),
    )
    .unwrap(),
  );
  let context = ContextManifest::new(
    old_context.id(),
    old_context.subject().clone(),
    FactoryDigest::from_bytes([63; 32]),
    entries,
  )
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    input.node().clone(),
    input.payload().clone(),
  )
  .unwrap()
  .with_context(context.clone(), vec![])
  .unwrap();
  let mut node_json = serde_json::to_value(node).unwrap();
  node_json["input_digest"] = json!(input.digest().unwrap());
  node_json["deadline"] = json!(1000);
  let node = serde_json::from_value(node_json).unwrap();
  let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
  let envelope = FactoryTaskEnvelope::new(
    FactoryTaskDeclaration {
      id: TaskEnvelopeId::generate(),
      node_attempt_id: intent.node().id(),
      subject: context.subject().clone(),
      mode: FactoryTaskMode::Observe,
    },
    FactoryTaskDefinition {
      task,
      specifications: vec![],
      prior_findings: vec![],
      permissions: grants,
      budget: intent.budget(),
      result_schema: FactoryTaskResultSchema::FlowV1,
      deliverables: vec![],
    },
    &context,
    toolchain,
    FactoryTaskControlDigests {
      policy: FactoryDigest::from_bytes([1; 32]),
      prompt: FactoryDigest::from_bytes([2; 32]),
      prompt_provenance: FactoryDigest::from_bytes([3; 32]),
      provenance: FactoryDigest::from_bytes([4; 32]),
    },
  )
  .unwrap();
  let package = FactoryRoleExecutionPackage::new(FactoryRoleExecutionPackageInput {
    project_rules: vec![FactoryRoleProjectRule {
      source: key("project-rules"),
      instructions: String::from_utf8(rules.clone()).unwrap(),
    }],
    role,
    profile,
    skills,
    task: envelope,
    context,
  })
  .unwrap();
  let bytes = [
    (rules_ref.artifact_id(), rules),
    (
      intent
        .input()
        .context()
        .unwrap()
        .entries()
        .iter()
        .find(|entry| entry.source_kind() == ContextSourceKind::Task)
        .and_then(|entry| match entry.source() {
          FactoryContextReference::Artifact(file) => Some(file.artifact_id()),
          _ => None,
        })
        .unwrap(),
      task_bytes,
    ),
  ]
  .into();
  (intent, admitted, installed, package, bytes)
}
fn fixture_with(
  edit: impl FnOnce(&mut FactoryRoleProfileDefinition),
  skills: BTreeMap<FactoryKey, FactoryRoleSkillBundle>,
) -> (
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCodexBuildProfile,
  FactoryRoleExecutionPackage,
  BTreeMap<ArtifactId, Vec<u8>>,
) {
  fixture_for_work(fixtures::work(), edit, skills, factory_role_task_digest)
}
fn fixture() -> (
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCodexBuildProfile,
  FactoryRoleExecutionPackage,
  BTreeMap<ArtifactId, Vec<u8>>,
) {
  fixture_with(|_| {}, BTreeMap::new())
}
#[test]
fn role_package_compiles_ordered_instructions_model_settings_and_exact_provenance() {
  let (intent, admitted, installed, package, bytes) = fixture();
  let role = FactoryRoleInvocation::new(package.clone(), None, BTreeMap::new()).unwrap();
  let compiled = compile_codex_flow_task_with_options(
    &intent,
    &admitted,
    &installed.instructions,
    &installed.credentials,
    installed.consumer,
    &installed.usage_policy,
    FactoryCodexFlowCompilationOptions {
      context: Some(&bytes),
      role: Some(&role),
      ..Default::default()
    },
  )
  .unwrap();
  let prompt = std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).unwrap();
  let layers = [
    "Project rules control",
    "Shared responsibility.",
    "Shared second rule.",
    "Profile instructions.",
    "Return the stage result.",
  ];
  let positions: Vec<_> = layers.into_iter().map(|layer| prompt.find(layer).unwrap()).collect();
  assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
  let octafile: Value =
    serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::Octafile).bytes()).unwrap();
  let task = &octafile["tasks"]["observe"]["cmds"][0]["codex"];
  assert_eq!(task["model"], "exact-model");
  assert_eq!(task["reasoning_effort"], "high");
  assert_eq!(task["environment"]["public"]["LANGUAGE"], "OCTACITY_ROLE_LANGUAGE");
  assert_eq!(octafile["vars"]["OCTACITY_ROLE_LANGUAGE"], "ru");
  let retained = compiled.document(FactoryProtectedDocumentKind::RoleExecutionPackage);
  assert_eq!(retained.bytes(), package.canonical_bytes().unwrap());
  let envelope: Value =
    serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::TaskEnvelope).bytes()).unwrap();
  assert_eq!(envelope["role_package_digest"], json!(package.digest().unwrap()));
}

struct RoleSource(FactoryRoleInvocation);
#[async_trait::async_trait]
impl FactoryRoleInvocationSource for RoleSource {
  async fn invocation(
    &self,
    _: &FlowBuildIntent,
    _: Timestamp,
    _: Timestamp,
  ) -> Result<Option<FactoryRoleInvocation>, ApplicationError> {
    Ok(Some(self.0.clone()))
  }
}
struct ContextBytes(BTreeMap<ArtifactId, Vec<u8>>);
#[async_trait::async_trait]
impl FactoryNodeContextSource for ContextBytes {
  async fn read_context(
    &self,
    _: &FlowNodeInput,
    _: Timestamp,
    _: Timestamp,
  ) -> Result<BTreeMap<ArtifactId, Vec<u8>>, ApplicationError> {
    Ok(self.0.clone())
  }
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(41).unwrap())
  }
}
type TestMaterializer = FactoryCodexBuildMaterializer<Context, Inputs, Bytes>;
fn materializer_fixture(
  intent: &FlowBuildIntent,
  installed: &FactoryCodexBuildProfile,
  bytes: BTreeMap<ArtifactId, Vec<u8>>,
) -> (TestMaterializer, Arc<Inputs>) {
  let mut context = managed_context(
    &installed.build_configuration,
    &installed.pipeline_node,
    &installed.toolchain.plugin,
    intent,
  );
  context.configuration.definition.runtime.cpu_millis = 2000;
  context.configuration.definition.runtime.memory_bytes = 2 * 1024 * 1024 * 1024;
  context.configuration.definition.runtime.writable_disk_bytes = 4 * 1024 * 1024 * 1024;
  context.configuration.definition.runtime.timeout_seconds = 1;
  context.configuration.definition.artifacts = octacity_server_store::ArtifactPolicy {
    artifact_count: 8,
    artifact_bytes: 1024,
    report_count: 4,
    report_bytes: 1024,
    single_output_bytes: 1024,
  };
  context.effective_policy.policy.artifacts = context.configuration.definition.artifacts;
  context
    .effective_policy
    .policy
    .secret_profiles
    .insert(SecretProfileName::new("model-evaluation").unwrap());
  let inputs = Arc::new(Inputs::default());
  let materializer = FactoryCodexBuildMaterializer::new(
    Arc::new(Context(context)),
    Arc::new(FactoryExecutionInputPublisher::new(
      inputs.clone(),
      Arc::new(Bytes),
      Arc::new(Clock),
    )),
    vec![installed.clone()],
    Arc::new(ContextBytes(bytes)),
    Arc::new(Clock),
  )
  .unwrap();
  (materializer, inputs)
}
#[test]
fn role_package_materializer_uses_the_existing_publisher_and_ordinary_job_contract() {
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, installed, package, bytes) = fixture();
    let (materializer, inputs) = materializer_fixture(&intent, &installed, bytes);
    let ownership = FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim());
    assert!(
      materializer
        .materialize(
          &intent,
          &admitted,
          ownership.clone(),
          Timestamp::from_unix_millis(41).unwrap()
        )
        .await
        .is_err(),
      "a role binding cannot fall back to the old static invocation"
    );
    let role = FactoryRoleInvocation::new(package, None, BTreeMap::new()).unwrap();
    let materializer = materializer.with_role_invocation_source(Arc::new(RoleSource(role)));
    let result = materializer
      .materialize(&intent, &admitted, ownership, Timestamp::from_unix_millis(41).unwrap())
      .await;
    assert!(
      result.is_ok(),
      "publication count: {}, error: {:?}",
      inputs.0.lock().unwrap().len(),
      result.as_ref().err()
    );
    let accepted = result.unwrap();
    accepted.validate_fresh().unwrap();
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 5);
    assert!(
      managed
        .protected_inputs()
        .inputs
        .iter()
        .any(|input| input.destination == "/octacity/protected/role-execution-package.json")
    );
    assert_eq!(managed.factory().node_attempt_id, intent.node().id().to_string());
    assert_eq!(
      managed.execution().credential_profile.as_deref(),
      Some("model-evaluation")
    );
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
  });
}

struct UnavailableRoleSource(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl FactoryRoleInvocationSource for UnavailableRoleSource {
  async fn invocation(
    &self,
    _: &FlowBuildIntent,
    _: Timestamp,
    _: Timestamp,
  ) -> Result<Option<FactoryRoleInvocation>, ApplicationError> {
    self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Err(ApplicationError::unavailable())
  }
}

#[test]
fn role_package_static_materialization_does_not_require_role_storage() {
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, installed, _, bytes) =
      fixture_for_work(fixtures::work(), |_| {}, BTreeMap::new(), |base, _| base);
    let (materializer, inputs) = materializer_fixture(&intent, &installed, bytes);
    let source = Arc::new(UnavailableRoleSource(std::sync::atomic::AtomicUsize::new(0)));
    let ownership = FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim());
    let accepted = materializer
      .with_role_invocation_source(source.clone())
      .materialize(&intent, &admitted, ownership, Timestamp::from_unix_millis(41).unwrap())
      .await
      .expect("static Builds must remain available when role storage is unavailable");
    accepted.validate_fresh().unwrap();
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 4);
    assert_eq!(source.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(inputs.0.lock().unwrap().len(), 1);
  });
}

#[test]
fn role_package_rejects_settings_the_pinned_adapter_cannot_enforce() {
  let edits: [fn(&mut FactoryRoleProfileDefinition); 7] = [
    |profile| {
      profile.model_parameters.insert(key("unknown_parameter"), json!(true));
    },
    |profile| {
      profile
        .model_parameters
        .insert(key("reasoning_effort"), json!("unqualified-effort"));
    },
    |profile| {
      profile.execution.cli_arguments = vec!["--arbitrary-option".into()];
    },
    |profile| {
      profile.execution.mcp_servers.insert(
        key("tools"),
        FactoryRoleMcpBinding {
          configuration: reference("mcp.tools", 5),
          integration: None,
        },
      );
    },
    |profile| {
      profile.execution.integrations =
        octacity_server_store::testing::role_profile_contract_definition(profile.build_configuration.project_id())
          .execution
          .integrations;
    },
    |profile| {
      profile.execution.environment.insert(
        "HOME".into(),
        FactoryRoleEnvironmentValue::Public {
          value: "/ambient/home".into(),
        },
      );
    },
    |profile| {
      profile.execution.environment.insert(
        "MODEL_AUTH".into(),
        FactoryRoleEnvironmentValue::Secret {
          reference: octacity_server_secrets::LogicalSecretReference::new(
            octacity_server_secrets::SecretProviderId::new("operator").unwrap(),
            "model-auth",
          )
          .unwrap(),
        },
      );
    },
  ];
  for edit in edits {
    let (intent, admitted, installed, package, bytes) = fixture_with(edit, BTreeMap::new());
    let role = FactoryRoleInvocation::new(package, None, BTreeMap::new()).unwrap();
    assert!(
      compile_codex_flow_task_with_options(
        &intent,
        &admitted,
        &installed.instructions,
        &installed.credentials,
        installed.consumer,
        &installed.usage_policy,
        FactoryCodexFlowCompilationOptions {
          context: Some(&bytes),
          role: Some(&role),
          ..Default::default()
        }
      )
      .is_err(),
      "unsupported requested settings cannot be silently dropped"
    );
  }
}

#[test]
fn role_package_materializes_imported_skill_closure_through_the_existing_input_transfer() {
  crate::factory_admission_tests::run_ready(async {
    let work = fixtures::work();
    let project = work.subject().project_id();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::default());
    store.seed_project(project).unwrap();
    let import = ProjectSkillImport::new(
      store,
      Arc::new(crate::factory_skill_import_tests::Bytes::default()),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let imported = import
      .import(
        &crate::factory_admission_tests::context(),
        ImportProjectSkill {
          project_id: project,
          identity: key("testing"),
          idempotency_key: "skill-import".parse().unwrap(),
          entries: [
            ("SKILL.md", b"# Write a failing test first".as_slice()),
            ("examples/test.txt", b"retained example".as_slice()),
          ]
          .into_iter()
          .map(|(path, bytes)| ReviewedSkillEntry::File {
            path: path.into(),
            sha256: FactoryDigest::content_sha256(bytes),
            bytes: bytes.to_vec(),
          })
          .collect(),
        },
      )
      .await
      .unwrap();
    let (intent, admitted, installed, package, bytes) = fixture_for_work(
      work,
      |_| {},
      [(key("testing"), imported.intent.bundle)].into(),
      factory_role_task_digest,
    );
    assert!(
      FactoryRoleInvocation::new(package.clone(), None, BTreeMap::new()).is_err(),
      "selected dependencies cannot disappear"
    );
    let role = import
      .prepare_invocation(package.clone(), BTreeMap::new())
      .await
      .unwrap();
    let compiled = compile_codex_flow_task_with_options(
      &intent,
      &admitted,
      &installed.instructions,
      &installed.credentials,
      installed.consumer,
      &installed.usage_policy,
      FactoryCodexFlowCompilationOptions {
        context: Some(&bytes),
        role: Some(&role),
        ..Default::default()
      },
    )
    .unwrap();
    let prompt = std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).unwrap();
    assert!(prompt.contains("/octacity/protected/skills/testing/SKILL.md"));
    let mut manifest = octacity_protocol::ProtectedInputManifestV3 {
      inputs: compiled
        .documents()
        .iter()
        .map(|document| octacity_protocol::ProtectedInputV3 {
          artifact_id: ArtifactId::generate().to_string(),
          destination: document.destination().into(),
          size_bytes: document.bytes().len() as u64,
          sha256: document.content_digest().to_string(),
          media_type: document.media_type().into(),
        })
        .collect(),
    };
    manifest.inputs.extend(role.skills().unwrap().inputs.clone());
    manifest.inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    let (materializer, published) = materializer_fixture(&intent, &installed, bytes.clone());
    let ownership = FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim());
    let accepted = materializer
      .with_role_invocation_source(Arc::new(RoleSource(role.clone())))
      .materialize(&intent, &admitted, ownership, Timestamp::from_unix_millis(41).unwrap())
      .await
      .unwrap();
    assert_eq!(
      accepted.jobs[0]
        .job_spec_template
        .managed_intent()
        .unwrap()
        .protected_inputs()
        .inputs
        .len(),
      7
    );
    assert_eq!(
      published.0.lock().unwrap()[0].inputs.len(),
      5,
      "existing skill objects are transferred without republishing bytes"
    );
    let program = compiled.managed_program(manifest.clone()).unwrap();
    assert_eq!(program.protected_inputs.inputs.len(), 7);
    assert!(
      program
        .protected_inputs
        .inputs
        .iter()
        .any(|file| file.destination == "/octacity/protected/skills/testing/examples/test.txt")
    );
    let file = manifest
      .inputs
      .iter_mut()
      .find(|file| file.destination.ends_with("/SKILL.md"))
      .unwrap();
    file.sha256 = FactoryDigest::from_bytes([88; 32]).to_string();
    assert!(compiled.managed_program(manifest).is_err());
    let mut substituted = role.skills().unwrap().clone();
    substituted.inputs[0].sha256 = FactoryDigest::from_bytes([99; 32]).to_string();
    assert!(FactoryRoleInvocation::new(package, Some(substituted), BTreeMap::new()).is_err());
  });
}

fn logical_secret(name: &str) -> octacity_server_secrets::LogicalSecretReference {
  octacity_server_secrets::LogicalSecretReference::new(
    octacity_server_secrets::SecretProviderId::new("operator-secrets").unwrap(),
    name,
  )
  .unwrap()
}
#[test]
fn role_package_maps_only_selected_qualified_secret_references_without_materializing_values() {
  let reference = logical_secret("project/model-auth");
  let (intent, admitted, installed, package, bytes) = fixture_with(
    |profile| {
      profile.execution.environment.insert(
        "MODEL_AUTH".into(),
        FactoryRoleEnvironmentValue::Secret {
          reference: reference.clone(),
        },
      );
    },
    BTreeMap::new(),
  );
  let variables = BTreeMap::from([("OCTACITY_CODEX_AUTH".into(), reference)]);
  let role = FactoryRoleInvocation::new(package.clone(), None, variables.clone()).unwrap();
  let compiled = compile_codex_flow_task_with_options(
    &intent,
    &admitted,
    &installed.instructions,
    &installed.credentials,
    installed.consumer,
    &installed.usage_policy,
    FactoryCodexFlowCompilationOptions {
      context: Some(&bytes),
      role: Some(&role),
      ..Default::default()
    },
  )
  .unwrap();
  let octafile: Value =
    serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::Octafile).bytes()).unwrap();
  assert_eq!(
    octafile["tasks"]["observe"]["cmds"][0]["codex"]["environment"]["secret"]["MODEL_AUTH"],
    "OCTACITY_CODEX_AUTH"
  );
  assert_eq!(
    octafile["vars"]["OCTACITY_CODEX_AUTH"],
    json!({"required":true,"secret":true})
  );
  assert!(!octafile.to_string().contains("project/model-auth"));
  assert!(!format!("{compiled:?}").contains("project/model-auth"));
  let mut extra = variables;
  extra.insert("UNSELECTED_CREDENTIAL".into(), logical_secret("project/other-auth"));
  assert!(
    FactoryRoleInvocation::new(package, None, extra).is_err(),
    "an invocation must not carry unselected credential bindings"
  );
}
#[test]
fn role_package_substitution_cannot_reuse_another_profile_or_node_invocation() {
  let (intent, admitted, installed, package, bytes) = fixture();
  let mut inputs = package.inputs().clone();
  let mut changed = inputs.profile.definition().clone();
  changed
    .model_parameters
    .insert(key("reasoning_effort"), json!("medium"));
  inputs.profile = inputs
    .profile
    .replace(FactoryRoleProfileVersion::new(2).unwrap(), &inputs.role, changed)
    .unwrap();
  assert_ne!(
    factory_role_task_digest(installed.task_digest(), &inputs.profile),
    intent.profile().task_digest
  );
  assert!(!installed.matches_role_intent(&intent, Some(&inputs.profile)));
  let substituted =
    FactoryRoleInvocation::new(FactoryRoleExecutionPackage::new(inputs).unwrap(), None, BTreeMap::new()).unwrap();
  assert!(
    compile_codex_flow_task_with_options(
      &intent,
      &admitted,
      &installed.instructions,
      &installed.credentials,
      installed.consumer,
      &installed.usage_policy,
      FactoryCodexFlowCompilationOptions {
        context: Some(&bytes),
        role: Some(&substituted),
        ..Default::default()
      }
    )
    .is_err()
  );
  let mut foreign: Value = serde_json::to_value(package).unwrap();
  foreign["inputs"]["task"]["node_attempt_id"] = json!(NodeAttemptId::generate());
  let foreign = FactoryRoleInvocation::new(serde_json::from_value(foreign).unwrap(), None, BTreeMap::new()).unwrap();
  assert!(foreign.verify_intent(&intent).is_err());
}

#[test]
fn role_package_rejects_a_skill_set_that_leaves_no_room_for_compiler_inputs_before_publication() {
  crate::factory_admission_tests::run_ready(async {
    let files: BTreeMap<_, _> = std::iter::once("SKILL.md".to_owned())
      .chain((0..255).map(|n| format!("support/f{n:03}.txt")))
      .map(|path| {
        (
          FactoryRepositoryPath::new(path).unwrap(),
          FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::content_sha256(b"x"), 1).unwrap(),
        )
      })
      .collect();
    let bundle = FactoryRoleSkillBundle::new(key("large"), key("v1"), files).unwrap();
    let manifest = octacity_protocol::ProtectedInputManifestV3 {
      inputs: bundle
        .files()
        .iter()
        .map(|(path, file)| octacity_protocol::ProtectedInputV3 {
          artifact_id: file.artifact_id().to_string(),
          size_bytes: 1,
          sha256: file.content_digest().to_string(),
          media_type: "application/octet-stream".into(),
          destination: format!("/octacity/protected/skills/large/{}", path.as_str()),
        })
        .collect(),
    };
    let mut manifest = manifest;
    manifest.inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    let (intent, admitted, installed, package, bytes) = fixture_with(|_| {}, [(key("large"), bundle)].into());
    let role = FactoryRoleInvocation::new(package, Some(manifest), BTreeMap::new()).unwrap();
    let (materializer, published) = materializer_fixture(&intent, &installed, bytes);
    let ownership = FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim());
    assert!(
      materializer
        .with_role_invocation_source(Arc::new(RoleSource(role)))
        .materialize(&intent, &admitted, ownership, Timestamp::from_unix_millis(41).unwrap())
        .await
        .is_err()
    );
    assert!(
      published.0.lock().unwrap().is_empty(),
      "the full transfer must be representable before compiler documents are published"
    );
  });
}
