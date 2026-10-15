//! Explicit installed deployment authority over the ordinary immutable command compiler.
use crate::factory_node_test_support::fixtures;
use crate::*;
use octacity_server_factory::*;

fn authority(environment: &str, endpoint: &str, credential: Option<&str>) -> FactoryDeploymentCommandAuthority {
  FactoryDeploymentCommandAuthority::new(
    fixtures::key("bounded-deployment-policy"),
    fixtures::key("v1"),
    fixtures::reference(environment),
    endpoint.to_owned(),
    credential.map(fixtures::key),
  )
  .unwrap()
}
async fn fixture() -> (
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCommandBuildProfile,
  JobSpecToolchainPolicy,
  FactoryDeploymentCommandAuthority,
) {
  fixture_for_environment("exact.environment").await
}
pub(crate) async fn fixture_for_environment(
  environment: &str,
) -> (
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCommandBuildProfile,
  JobSpecToolchainPolicy,
  FactoryDeploymentCommandAuthority,
) {
  fixture_with_extra_host(environment, None).await
}
async fn fixture_with_extra_host(
  environment: &str,
  extra_host: Option<&str>,
) -> (
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCommandBuildProfile,
  JobSpecToolchainPolicy,
  FactoryDeploymentCommandAuthority,
) {
  let (command, candidate, approved, authority) = candidate_fixture(environment, extra_host).await;
  (candidate.intent, candidate.admitted, command, approved, authority)
}

pub(crate) async fn candidate_fixture(
  environment: &str,
  extra_host: Option<&str>,
) -> (
  FactoryCommandBuildProfile,
  crate::factory_candidate_context_test_support::CandidateContextFixture,
  JobSpecToolchainPolicy,
  FactoryDeploymentCommandAuthority,
) {
  let authority = authority(
    environment,
    "https://staging.example.test/deploy",
    Some("staging-credential"),
  );
  let (command, candidate) =
    crate::factory_command_output_source_tests::retained_promotion_command_fixture_with_profile(|command, consumer| {
      command.identity = authority.reference().clone();
      command.program.credential_profile = Some(fixtures::key("staging-credential"));
      let mut grants = serde_json::to_value(&command.project_permissions).unwrap();
      grants["network_hosts"] = serde_json::json!(["staging.example.test"]);
      grants["secret_profiles"] = serde_json::json!(["staging-credential"]);
      grants["resources"]["elapsed_millis"] = serde_json::json!(1000);
      command.project_permissions = serde_json::from_value(grants.clone()).unwrap();
      command.local_permissions = LocalPermissionCeiling::fully_enforced(command.project_permissions.clone());
      if let Some(host) = extra_host {
        grants["network_hosts"] = serde_json::json!(["staging.example.test", host]);
      }
      *consumer = serde_json::from_value(grants).unwrap();
    })
    .await;
  let canonical = crate::factory_job_test_support::canonical_v3_spec();
  let approved = JobSpecToolchainPolicy {
    source: octacity_server_job::SourcePluginPolicy::new("git", "0.1.0", canonical.source.plugin_sha256, "url")
      .unwrap(),
    octa: octacity_protocol::OctaSpec {
      plugin_digests: std::collections::BTreeMap::from([(
        command.plugin.identity().to_string(),
        command.plugin.digest().to_string(),
      )]),
      ..canonical.octa
    },
    validity: octacity_server_job::JobSpecValidity::new(3600).unwrap(),
  };
  (command, candidate, approved, authority)
}

#[test]
fn explicitly_pinned_deployment_command_qualifies_without_widening_builtin_commands() {
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, command, approved, authority) = fixture().await;
    assert!(command.validate_installation(&approved).is_err());
    let ordinary = compile_factory_command_task(&intent, &admitted, &command).unwrap();
    let deployment = FactoryDeploymentCommandProfile::new(command, authority).unwrap();
    deployment.validate_installation(&approved).unwrap();
    let compiled = compile_factory_deployment_command_task(&intent, &admitted, &deployment, &approved).unwrap();
    assert_eq!(compiled.documents().len(), 4);
    // Publish through the existing input boundary; document internals stay opaque.
    {
      let bytes = std::sync::Arc::new(Bytes::default());
      let manifest = FactoryExecutionInputPublisher::new(
        std::sync::Arc::new(Inputs),
        bytes.clone(),
        std::sync::Arc::new(Clock(intent.node().started_at())),
      )
      .publish(
        &intent,
        &FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
        compiled.documents().to_vec(),
      )
      .await
      .unwrap();
      let published = bytes.0.lock().unwrap();
      let descriptor: serde_json::Value = published
        .iter()
        .filter_map(|body| serde_json::from_slice::<serde_json::Value>(body).ok())
        .find(|value| value.get("endpoint").is_some())
        .unwrap();
      assert_eq!(
        descriptor["environment"],
        serde_json::json!(fixtures::reference("exact.environment"))
      );
      assert_eq!(descriptor["endpoint"], "https://staging.example.test/deploy");
      assert_eq!(descriptor["credential_profile"], "staging-credential");
      drop(published);
      let program = compiled.managed_program(manifest.clone()).unwrap();
      assert!(compiled.command_proof(&program).is_ok());
      let mut base = manifest;
      base.inputs.retain(|input| {
        ![
          "/octacity/protected/deployment-target.json",
          "/octacity/protected/promotion-input.json",
        ]
        .contains(&input.destination.as_str())
      });
      let ordinary_program = ordinary.managed_program(base).unwrap();
      assert!(ordinary.command_proof(&ordinary_program).is_err());
    }
  });
}

#[test]
fn deployment_environment_endpoint_and_credential_replacements_require_an_explicit_policy_repin() {
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, command, approved, original) = fixture().await;
    for substituted in [
      authority(
        "other-environment",
        "https://staging.example.test/deploy",
        Some("staging-credential"),
      ),
      authority(
        "exact.environment",
        "https://other.example.test/deploy",
        Some("staging-credential"),
      ),
      authority(
        "exact.environment",
        "https://staging.example.test/deploy",
        Some("other-credential"),
      ),
    ] {
      assert_ne!(substituted.reference(), original.reference());
      assert!(FactoryDeploymentCommandProfile::new(command.clone(), substituted).is_err());
    }
    let mut unpinned = command.clone();
    unpinned.identity = fixtures::reference("unqualified-static-command");
    assert!(FactoryDeploymentCommandProfile::new(unpinned, original.clone()).is_err());
    let mut changed = command;
    let mut grants = serde_json::to_value(&changed.project_permissions).unwrap();
    grants["network_hosts"] = serde_json::json!(["other.example.test"]);
    changed.project_permissions = serde_json::from_value(grants).unwrap();
    changed.local_permissions = LocalPermissionCeiling::fully_enforced(changed.project_permissions.clone());
    let replacement = FactoryDeploymentCommandProfile::new(changed, original);
    if let Ok(replacement) = replacement {
      assert!(replacement.validate_installation(&approved).is_err());
      assert!(compile_factory_deployment_command_task(&intent, &admitted, &replacement, &approved).is_err());
    }
  });
}

struct Inputs;
#[async_trait::async_trait]
impl octacity_server_store::FactoryExecutionInputStore for Inputs {
  async fn reserve_factory_execution_inputs(
    &self,
    request: octacity_server_store::PublishFactoryExecutionInputs,
  ) -> Result<octacity_server_store::FactoryExecutionInputDisposition, octacity_server_store::StoreError> {
    request.validate()?;
    Ok(octacity_server_store::FactoryExecutionInputDisposition::Pending)
  }
  async fn publish_factory_execution_inputs(
    &self,
    request: octacity_server_store::PublishFactoryExecutionInputs,
  ) -> Result<(), octacity_server_store::StoreError> {
    request.validate()
  }
}
#[derive(Default)]
struct Bytes(std::sync::Mutex<Vec<Vec<u8>>>);
#[async_trait::async_trait]
impl octacity_artifact_store::ArtifactBytePublisher for Bytes {
  async fn publish_verified(
    &self,
    _: &octacity_artifact_store::ArtifactObject,
    bytes: Vec<u8>,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    self.0.lock().unwrap().push(bytes);
    Ok(())
  }
}
struct Clock(octacity_server_domain::Timestamp);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
    Ok(self.0)
  }
}

#[test]
fn deployment_compilation_requires_the_explicit_promotion_role_and_exact_single_environment() {
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, command, approved, authority) = fixture_for_environment("substituted.environment").await;
    let deployment = FactoryDeploymentCommandProfile::new(command, authority).unwrap();
    deployment.validate_installation(&approved).unwrap();
    assert!(
      compile_factory_deployment_command_task(&intent, &admitted, &deployment, &approved).is_err(),
      "a matching profile/task pin cannot substitute the environment bound by the Promotion Plan"
    );
    let (_, _, mut command, approved, authority) = fixture().await;
    let work = fixtures::work();
    command.build_configuration = BuildConfigurationRef::new(
      command.build_configuration.id(),
      command.build_configuration.version(),
      work.subject().project_id(),
      command.build_configuration.definition_digest(),
    );
    let mut binding = fixtures::binding(work.subject().project_id());
    binding.tool = command.executor.clone();
    binding.plugin = command.plugin.clone();
    binding.model_or_tool = command.identity.clone();
    binding.build_configuration = command.build_configuration.clone();
    binding.task_digest = command.task_digest();
    binding.result_output = command.outputs[0].kind().clone();
    binding.evidence = command.outputs[1..].to_vec();
    let (_, admitted, _, input, node) = fixtures::execution_fixture_with_permissions(
      FlowNodeKind::BuildCommand,
      work,
      binding,
      None,
      command.project_permissions.clone(),
    );
    let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
    assert!(compile_factory_command_task(&intent, &admitted, &command).is_ok());
    let deployment = FactoryDeploymentCommandProfile::new(command, authority).unwrap();
    deployment.validate_installation(&approved).unwrap();
    assert!(
      compile_factory_deployment_command_task(&intent, &admitted, &deployment, &approved).is_err(),
      "ordinary BuildCommand never gains deployment qualification"
    );
  });
}

#[test]
fn deployment_authority_rejects_noncanonical_or_credential_bearing_endpoints() {
  for endpoint in [
    "file:///tmp/target",
    "https://user:secret@staging.example.test/deploy",
    "https://staging.example.test/deploy?token=private",
    "https://staging.example.test/deploy#fragment",
    "https://STAGING.example.test/deploy",
    "https://staging.example.test:443/deploy",
  ] {
    assert!(
      FactoryDeploymentCommandAuthority::new(
        fixtures::key("bounded-deployment-policy"),
        fixtures::key("v1"),
        fixtures::reference("exact.environment"),
        endpoint.to_owned(),
        None
      )
      .is_err()
    );
  }
}

#[test]
fn deployment_compiler_rejects_broader_node_permissions_with_the_same_frozen_task_pin() {
  crate::factory_admission_tests::run_ready(async {
    let (intent, admitted, command, approved, authority) =
      fixture_with_extra_host("exact.environment", Some("unselected.example.test")).await;
    assert!(command.matches_intent(&intent));
    let deployment = FactoryDeploymentCommandProfile::new(command, authority).unwrap();
    deployment.validate_installation(&approved).unwrap();
    assert!(
      compile_factory_deployment_command_task(&intent, &admitted, &deployment, &approved).is_err(),
      "a valid profile/task pin must not qualify an admitted Node broader than either installed ceiling"
    );
  });
}
