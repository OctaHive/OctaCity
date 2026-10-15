//! Native command preparation keeps the original private inputs and invocation contract together.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_pipeline::*;
use octacity_server_store::*;
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Arc, Mutex},
};

#[path = "../tests/support/manual_trigger_context.rs"]
mod ordinary_context;

pub(crate) struct Context(pub(crate) BuildExecutionContext);
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
pub(crate) struct Inputs(pub(crate) Mutex<Vec<PublishFactoryExecutionInputs>>);
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
    request.validate()?;
    self.0.lock().unwrap().push(request);
    Ok(())
  }
}
pub(crate) struct Bytes;
#[async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, _: &ArtifactObject, _: Vec<u8>) -> Result<(), ArtifactStoreError> {
    Ok(())
  }
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(41))
  }
}
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}

fn context(profile: &FactoryCommandBuildProfile, intent: &FlowBuildIntent) -> BuildExecutionContext {
  managed_context(
    &profile.build_configuration,
    &profile.pipeline_node,
    &profile.plugin,
    intent,
  )
}
/// Reuses the same published ordinary configuration for independently installed adapters.
pub(crate) fn managed_context(
  configuration: &BuildConfigurationRef,
  pipeline_node: &PipelineNodeId,
  plugin: &ImmutableReference,
  intent: &FlowBuildIntent,
) -> BuildExecutionContext {
  let (mut context, _) = ordinary_context::context();
  let project = intent.input().subject().project_id();
  let repository = intent.input().subject().repository_id();
  context.repository.id = repository;
  context.repository.project_id = project;
  context.pipeline.project_id = project;
  let node = PipelineNode::new(
    pipeline_node.clone(),
    JobName::new("Independent validation").unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![],
    serde_json::json!({}),
  )
  .unwrap();
  context.pipeline.dag = PublishablePipelineDag::new(vec![node], vec![], &CapabilityCatalog::new([]))
    .unwrap()
    .into_dag();
  context.configuration.id = configuration.id();
  context.configuration.version = configuration.version();
  context.configuration.project_id = project;
  let canonical = crate::factory_job_test_support::canonical_v3_spec();
  let definition = &mut context.configuration.definition;
  definition.repository_id = repository;
  definition.pipeline_id = context.pipeline.id;
  definition.pipeline_version = context.pipeline.version;
  definition.parameters = ParameterSchema {
    parameters: BTreeMap::new(),
    deny_unknown: true,
  };
  definition.secrets_profile = None;
  definition.runtime.class = RuntimeClass::Virtualization;
  definition.runtime.operating_system = canonical.runtime.target.target_platform.os;
  definition.runtime.architecture = canonical.runtime.target.target_platform.architecture;
  definition.runtime.host_platform = Some(canonical.runtime.target.host_platform);
  definition.runtime.required_guarantees = canonical.runtime.target.required_guarantees.iter().copied().collect();
  definition.runtime.immutable_image = canonical.runtime.target.immutable_image.clone();
  definition.runtime.memory_bytes = canonical.runtime.memory_bytes;
  definition.runtime.writable_disk_bytes = canonical.runtime.writable_disk_bytes;
  definition.runtime.timeout_seconds = 1;
  definition.runtime.network = ConfigurationNetworkPolicy::Disabled;
  definition.artifacts = ArtifactPolicy {
    artifact_count: 10,
    artifact_bytes: 1024,
    report_count: 10,
    report_bytes: 1024,
    single_output_bytes: 1024,
  };
  context.effective_policy.sources = vec![PolicySource {
    project_id: project,
    version: ProjectPolicyVersion::INITIAL,
  }];
  let policy = &mut context.effective_policy.policy;
  policy.repositories = BTreeSet::from([repository]);
  policy.secret_profiles.clear();
  policy.artifacts = definition.artifacts;
  policy.runtimes.insert(RuntimeClass::Virtualization);
  policy.execution_targets.insert(ProjectExecutionTarget {
    mode: octacity_protocol::ExecutionMode::Virtualization,
    host_platform: canonical.runtime.target.host_platform,
    target_platform: canonical.runtime.target.target_platform,
    required_guarantees: canonical.runtime.target.required_guarantees.into_iter().collect(),
  });
  context.job_spec_toolchain.octa = canonical.octa;
  context.job_spec_toolchain.octa.plugin_digests =
    BTreeMap::from([(plugin.identity().to_string(), plugin.digest().to_string())]);
  context
}

#[test]
fn validation_materializer_publishes_the_invocation_contract_and_signs_every_original_input_once() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, mut profile) = crate::factory_command_tests::command_fixture();
    let fixture = crate::factory_protected_validation_input_source_tests::retained_validation_fixture(
      false,
      6,
      |work, binding, permissions| {
        profile.identity = binding.model_or_tool.clone();
        profile.plugin = binding.plugin.clone();
        profile.executor = binding.tool.clone();
        profile.candidate_capture_tool = Some(crate::factory_node_test_support::fixtures::reference("git.capture"));
        crate::factory_command_output_source_tests::configure_profile(&mut profile, work);
        let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
        grants["resources"]["elapsed_millis"] = serde_json::json!(1000);
        profile.project_permissions = serde_json::from_value(grants).unwrap();
        profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
        *permissions = profile.project_permissions.clone();
        binding.build_configuration = profile.build_configuration.clone();
        binding.task_digest = profile.task_digest();
      },
    )
    .await;
    let context = context(&profile, &fixture.intent);
    profile.validate_installation(&context.job_spec_toolchain).unwrap();
    let inputs = Arc::new(Inputs::default());
    let compiler = FactoryCommandBuildMaterializer::new(
      Arc::new(Context(context)),
      Arc::new(FactoryExecutionInputPublisher::new(
        inputs.clone(),
        Arc::new(Bytes),
        Arc::new(Clock),
      )),
      vec![profile],
      Arc::new(Clock),
    )
    .unwrap()
    .with_candidate_context_source(fixture.candidate_source)
    .with_protected_validation_input_source(fixture.source);
    let owner = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
    let accepted = compiler
      .materialize(&fixture.intent, &fixture.admitted, owner, at(41))
      .await;
    let published = inputs.0.lock().unwrap();
    assert_eq!(
      published.len(),
      1,
      "one fenced publication must carry the whole command document set"
    );
    assert_eq!(
      published[0].inputs.len(),
      3,
      "the actual Job compiler must publish the independently derived invocation document"
    );
    let accepted = accepted.expect("the same complete command proof must survive ordinary Job signing");
    accepted.validate_fresh().unwrap();
    assert_eq!(accepted.jobs.len(), 1);
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 7);
    for original in fixture.materialized.wire_inputs(&[]).unwrap().1 {
      assert_eq!(
        managed
          .protected_inputs()
          .inputs
          .iter()
          .filter(|input| **input == original)
          .count(),
        1
      );
    }
    assert!(managed.factory().change_set_materialization.is_some());
    assert!(managed.factory().change_set_capture.is_none());
    assert!(managed.permissions().secret_profiles.is_empty());
  });
}

struct AdvancingClock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for AdvancingClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst)))
  }
}

#[test]
fn command_materialization_preserves_admission_time_while_rechecking_current_authority() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, mut profile) = crate::factory_command_tests::command_fixture();
    let fixture = crate::factory_protected_validation_input_source_tests::retained_validation_fixture(
      false,
      6,
      |work, binding, permissions| {
        profile.identity = binding.model_or_tool.clone();
        profile.plugin = binding.plugin.clone();
        profile.executor = binding.tool.clone();
        profile.candidate_capture_tool = Some(crate::factory_node_test_support::fixtures::reference("git.capture"));
        crate::factory_command_output_source_tests::configure_profile(&mut profile, work);
        let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
        grants["resources"]["elapsed_millis"] = serde_json::json!(1000);
        profile.project_permissions = serde_json::from_value(grants).unwrap();
        profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
        *permissions = profile.project_permissions.clone();
        binding.build_configuration = profile.build_configuration.clone();
        binding.task_digest = profile.task_digest();
      },
    )
    .await;
    let context = context(&profile, &fixture.intent);
    let inputs = Arc::new(Inputs::default());
    let clock = Arc::new(AdvancingClock(std::sync::atomic::AtomicI64::new(42)));
    let compiler = FactoryCommandBuildMaterializer::new(
      Arc::new(Context(context)),
      Arc::new(FactoryExecutionInputPublisher::new(
        inputs.clone(),
        Arc::new(Bytes),
        clock.clone(),
      )),
      vec![profile],
      clock.clone(),
    )
    .unwrap()
    .with_candidate_context_source(fixture.candidate_source)
    .with_protected_validation_input_source(fixture.source);
    let owner = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
    let prepared = compiler
      .materialize(&fixture.intent, &fixture.admitted, owner, at(41))
      .await
      .unwrap();
    assert_eq!(
      prepared.accepted_at,
      at(41),
      "ordinary admission compares the original preparation timestamp before its final fenced commit"
    );
    prepared.validate_fresh().unwrap();
    assert!(
      clock.0.load(std::sync::atomic::Ordering::SeqCst) > 43,
      "publication and final authority checks must observe the advancing clock"
    );
  });
}

#[test]
fn qualified_deployment_materializer_signs_the_exact_target_and_plan_without_builtin_authority() {
  crate::factory_admission_tests::run_ready(async {
    let (command, candidate, approved, authority) =
      crate::factory_deployment_command_tests::candidate_fixture("exact.environment", None).await;
    let deployment = FactoryDeploymentCommandProfile::new(command.clone(), authority).unwrap();
    let context = deployment_context(&command, &candidate.intent, &approved);
    let at = candidate.intent.node().started_at();
    struct DeploymentClock(Timestamp);
    impl FactoryExecutionClock for DeploymentClock {
      fn now(&self) -> Result<Timestamp, ApplicationError> {
        Ok(self.0)
      }
    }
    let clock = Arc::new(DeploymentClock(at));
    let source = Arc::new(FactoryStoredCandidateContextSource::new(
      candidate.runs.clone(),
      candidate.source,
      command.candidate_capture_tool.clone().unwrap(),
      clock.clone(),
    ));
    let inputs = Arc::new(Inputs::default());
    let compiler = FactoryCommandBuildMaterializer::for_deployment(
      Arc::new(Context(context)),
      Arc::new(FactoryExecutionInputPublisher::new(
        inputs.clone(),
        Arc::new(Bytes),
        clock.clone(),
      )),
      vec![deployment],
      approved,
      clock,
    )
    .unwrap()
    .with_candidate_context_source(source);
    let action = compiler
      .action_input(&candidate.intent, &candidate.admitted)
      .unwrap()
      .unwrap();
    assert!(action.get("plan_digest").is_some());
    let accepted = compiler
      .materialize(
        &candidate.intent,
        &candidate.admitted,
        FactoryClaimOwnership::new(candidate.intent.node().owner().clone(), candidate.intent.node().claim()),
        at,
      )
      .await;
    let published = inputs.0.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].inputs.len(), 4);
    let accepted = accepted.expect("the qualified deployment must pass ordinary signing");
    accepted.validate_fresh().unwrap();
    let managed = accepted.jobs[0].job_spec_template.managed_intent().unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 6);
    for path in [
      "/octacity/protected/deployment-target.json",
      "/octacity/protected/promotion-input.json",
    ] {
      assert_eq!(
        managed
          .protected_inputs()
          .inputs
          .iter()
          .filter(|input| input.destination == path)
          .count(),
        1
      );
    }
    assert_eq!(
      managed.execution().credential_profile.as_deref(),
      Some("staging-credential")
    );
    assert_eq!(managed.permissions().network_hosts, ["staging.example.test"]);
    assert!(managed.permissions().executables.is_empty());
    assert!(managed.permissions().tools.is_empty());
  });
}

/// The ordinary installed configuration used at both dispatch and retained read.
pub(crate) fn deployment_context(
  command: &FactoryCommandBuildProfile,
  intent: &FlowBuildIntent,
  approved: &JobSpecToolchainPolicy,
) -> BuildExecutionContext {
  let mut context = context(command, intent);
  context.job_spec_toolchain = approved.clone();
  context.configuration.definition.runtime.timeout_seconds =
    command.project_permissions.resources().elapsed_millis().div_ceil(1000);
  context.configuration.definition.runtime.network = ConfigurationNetworkPolicy::Restricted {
    allowed_hosts: BTreeSet::from([octacity_server_domain::NetworkHost::new("staging.example.test").unwrap()]),
  };
  context
    .effective_policy
    .policy
    .secret_profiles
    .insert(crate::SecretProfileName::new("staging-credential").unwrap());
  context
}
