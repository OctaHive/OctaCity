//! Compilation of operator-installed command programs, independent of Flow names.
use crate::{ApplicationError, FactoryExecutionDocument, FactoryManagedJobProgram, OrdinaryBuildApplicationError};
use octacity_protocol::{FactoryEnforcementCapabilityV3, ManagedOctaExecutionV3, ProtectedInputManifestV3};
use octacity_server_domain::PipelineNodeId;
use octacity_server_factory::{
  AdmittedFlow, BuildConfigurationRef, EvidenceRequirement, FactoryDigest, FactoryKey, FactoryPermissionSet,
  FlowBuildIntent, ImmutableReference, LocalPermissionCeiling, PermissionCategory,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const OCTAFILE_DESTINATION: &str = "/octacity/protected/command-Octafile.yml";

/// Maximum UTF-8 bytes in one operator-installed command Octafile.
/// This source bound is separate from human-readable Factory summaries and
/// remains below ordinary managed Octafile and protected-transfer limits.
pub const MAX_FACTORY_COMMAND_OCTAFILE_BYTES: usize = 64 * 1024;

/// Exact bounded operator-installed source. Serialized as an unchanged string;
/// line breaks and indentation are part of the immutable program identity.
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct FactoryCommandOctafile(String);
impl FactoryCommandOctafile {
  /// Accepts nonblank source without normalizing its bytes. Unsupported control
  /// characters and oversized source fail with source-free diagnostics.
  pub fn new(value: impl Into<String>) -> Result<Self, ApplicationError> {
    let value = value.into();
    if value.trim().is_empty()
      || value.len() > MAX_FACTORY_COMMAND_OCTAFILE_BYTES
      || value
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self(value))
  }
  /// Returns the exact installed source, including whitespace.
  pub fn as_str(&self) -> &str {
    &self.0
  }
}
impl<'de> Deserialize<'de> for FactoryCommandOctafile {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
  }
}

/// Static operator-owned program. Flow input cannot replace executable instructions.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCommandProgram {
  /// Immutable installed Octafile bytes.
  pub octafile: FactoryCommandOctafile,
  /// Finite installed task selection.
  pub tasks: Vec<String>,
  /// Optional qualified workspace anchor.
  pub workspace_root: Option<String>,
  /// Logical Agent-local credential selection; contains no credential material.
  pub credential_profile: Option<FactoryKey>,
}

/// Exact command installation and independent permission ceilings.
#[derive(Clone)]
pub struct FactoryCommandBuildProfile {
  /// Installed command configuration selected by the common binding.
  pub identity: ImmutableReference,
  /// Exact installed execution plugin.
  pub plugin: ImmutableReference,
  /// Exact installed command capability; this is not an external binary identity.
  pub executor: ImmutableReference,
  /// Immutable ordinary Build Configuration.
  pub build_configuration: BuildConfigurationRef,
  /// Explicit single-job Pipeline node.
  pub pipeline_node: PipelineNodeId,
  /// Operator-owned instructions and finite selection.
  pub program: FactoryCommandProgram,
  /// Reserved absolute destination of canonical frozen input JSON.
  pub input_destination: String,
  /// Installed ordinary Build parameters.
  pub parameters: BTreeMap<String, serde_json::Value>,
  /// Project installation ceiling.
  pub project_permissions: FactoryPermissionSet,
  /// Qualified Agent/backend ceiling.
  pub local_permissions: LocalPermissionCeiling,
  /// Exact trusted candidate capture verifier, absent without candidate authority.
  pub candidate_capture_tool: Option<ImmutableReference>,
  /// Complete installed primary and additional output contracts.
  pub outputs: Vec<EvidenceRequirement>,
  /// Optional independently evaluated observation criterion for this installed command.
  /// Absence preserves the legacy command task identity and output semantics.
  pub observation_contract: Option<crate::FactoryCommandObservationContract>,
}
impl FactoryCommandBuildProfile {
  /// Installation metadata shared with readiness and the materializer's shape check.
  pub fn pipeline_contract(&self) -> octacity_server_factory::FactoryBuildPipelineContract {
    octacity_server_factory::FactoryBuildPipelineContract {
      build_configuration: self.build_configuration.clone(),
      tool: self.executor.clone(),
      plugin: self.plugin.clone(),
      model_or_tool: self.identity.clone(),
      pipeline_node: self.pipeline_node.clone(),
      compilation: None,
    }
  }

  /// Returns finite primary reports compatible with this explicitly installed observation contract.
  /// No program/credential values or I/O are involved; plain reports advertise no such capability.
  pub fn observation_bindings(
    &self,
  ) -> Result<Vec<crate::FactoryCommandObservationBinding>, OrdinaryBuildApplicationError> {
    let invalid = || OrdinaryBuildApplicationError::Invalid;
    let Some(contract) = &self.observation_contract else {
      return Ok(vec![]);
    };
    contract.validate()?;
    if self.outputs.len() > octacity_server_factory::MAX_FLOW_SCHEMA_ENTRIES {
      return Err(invalid());
    }
    let schema = contract.report_schema()?;
    let task_digest = self.task_digest();
    let bindings: Vec<_> = self
      .outputs
      .iter()
      .filter(|output| {
        output.output_kind() == octacity_server_factory::EvidenceOutputKind::Report
          && output.schema() == schema.reference()
      })
      .map(|output| crate::FactoryCommandObservationBinding {
        command: self.identity.clone(),
        tool: self.executor.clone(),
        plugin: self.plugin.clone(),
        task_digest,
        build_configuration: self.build_configuration.clone(),
        result_output: output.kind().clone(),
        schema: schema.reference().clone(),
        environment: contract.environment.clone(),
        criterion: contract.criterion.clone(),
      })
      .collect();
    if bindings.is_empty() {
      return Err(invalid());
    }
    Ok(bindings)
  }

  /// Pins executable instructions, output contracts, parameters and every ceiling.
  pub fn task_digest(&self) -> FactoryDigest {
    let enforcement: Vec<_> = PermissionCategory::ALL
      .into_iter()
      .map(|category| self.local_permissions.can_enforce(category))
      .collect();
    let bytes = serde_json::to_vec(&(
      &self.identity,
      &self.plugin,
      &self.executor,
      &self.build_configuration,
      &self.pipeline_node,
      &self.program,
      &self.input_destination,
      &self.parameters,
      &self.project_permissions,
      self.local_permissions.grants(),
      enforcement,
      &self.outputs,
      &self.candidate_capture_tool,
    ))
    .expect("typed installed command profile serializes");
    let legacy = FactoryDigest::sha256("octacity.factory.command.task.v1", &[&bytes]);
    match &self.observation_contract {
      None => legacy,
      Some(contract) => FactoryDigest::sha256(
        "octacity.factory.command.observation-task.v1",
        &[
          &legacy.as_bytes(),
          &serde_json::to_vec(contract).expect("typed command observation contract serializes"),
        ],
      ),
    }
  }
  /// Selects only the exact immutable installed capability and command policy.
  pub fn matches_intent(&self, intent: &FlowBuildIntent) -> bool {
    let profile = intent.profile();
    profile.model_or_tool == self.identity
      && profile.plugin == self.plugin
      && profile.tool == self.executor
      && profile.build_configuration == self.build_configuration
      && profile.task_digest == self.task_digest()
  }
  /// Qualifies a static non-model command against the exact approved runtime plugin inventory.
  /// Agent grants and backend enforcement are still checked independently.
  pub fn validate_installation(
    &self,
    approved: &crate::JobSpecToolchainPolicy,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    self.validate_plugin_inventory(approved)?;
    if !self.builtin_authority() {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(())
  }
  pub(crate) fn validate_plugin_inventory(
    &self,
    approved: &crate::JobSpecToolchainPolicy,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    approved
      .validate()
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    let inventory_matches = |grants: &FactoryPermissionSet| {
      grants.plugins().len() == approved.octa.plugin_digests.len()
        && grants.plugins().all(|plugin| {
          approved.octa.plugin_digests.get(plugin.identity().as_str()) == Some(&plugin.digest().to_string())
        })
    };
    if approved.octa.plugin_digests.get(self.plugin.identity().as_str()) != Some(&self.plugin.digest().to_string())
      || !inventory_matches(&self.project_permissions)
      || !inventory_matches(self.local_permissions.grants())
    {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(())
  }
  fn builtin_authority(&self) -> bool {
    let safe = |grants: &FactoryPermissionSet| {
      grants.executables().len() == 0
        && grants.commands().len() == 0
        && grants.tools().len() == 0
        && grants.network_hosts().len() == 0
        && grants.secret_profiles().len() == 0
        && grants.workload_identity_profiles().len() == 0
        && grants.plugins().any(|plugin| plugin == &self.plugin)
        && grants.mounts().all(|mount| {
          mount.mode() == octacity_server_factory::MountMode::ReadOnly
            || [
              octacity_protocol::FACTORY_OUTPUT_ROOT,
              octacity_protocol::FACTORY_SCRATCH_ROOT,
            ]
            .contains(&mount.root().as_str())
        })
    };
    self.program.credential_profile.is_none()
      && !self.outputs.is_empty()
      && self.outputs.len() <= 64
      && safe(&self.project_permissions)
      && safe(self.local_permissions.grants())
      && self.outputs.iter().enumerate().all(|(index, output)| {
        output.plugin() == &self.plugin
          && output.tool() == &self.executor
          && !self.outputs[..index].iter().any(|other| other.kind() == output.kind())
          && self
            .project_permissions
            .outputs()
            .kinds()
            .any(|kind| kind == output.kind())
      })
  }
}

/// Bounded documents ready for fenced Artifact publication.
pub struct CompiledFactoryCommandTask {
  program: FactoryCommandProgram,
  documents: Vec<FactoryExecutionDocument>,
  expected: Vec<(String, String, Vec<u8>)>,
  intent_digest: FactoryDigest,
  input: octacity_server_factory::FlowNodeInput,
  additional_inputs: Vec<octacity_protocol::ProtectedInputV3>,
  validation: Option<crate::FactoryProtectedValidationMaterialization>,
  promotion: bool,
  authority: Option<StaticCommandAuthority>,
}
#[derive(Clone, Copy)]
enum StaticCommandAuthority {
  Builtin,
  Deployment,
}
impl CompiledFactoryCommandTask {
  pub(crate) fn with_deployment_authority(
    mut self,
    authority: &crate::FactoryDeploymentCommandAuthority,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    const DESTINATION: &str = "/octacity/protected/deployment-target.json";
    if !self.promotion
      || self.validation.is_some()
      || self
        .expected
        .iter()
        .any(|(destination, _, _)| destination == DESTINATION)
    {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    let bytes = serde_json::to_vec(authority).map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    self.documents.push(
      FactoryExecutionDocument::new(DESTINATION, "application/json", bytes.clone())
        .map_err(|_| OrdinaryBuildApplicationError::Invalid)?,
    );
    self
      .expected
      .push((DESTINATION.to_owned(), "application/json".to_owned(), bytes));
    self.authority = Some(StaticCommandAuthority::Deployment);
    Ok(self)
  }
  /// Exact operator program and frozen operation documents.
  pub fn documents(&self) -> &[FactoryExecutionDocument] {
    &self.documents
  }
  /// Adds independently verified candidate inputs for this exact frozen consumer.
  /// Publication still contains only newly generated compiler documents.
  pub fn with_candidate(
    mut self,
    candidate: &crate::FactoryManagedCandidateProof,
    at: octacity_server_domain::Timestamp,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    if !self.additional_inputs.is_empty() {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    self.additional_inputs = candidate.materialization(&self.input, at)?.wire_inputs().1;
    Ok(self)
  }
  /// Adds the original private input proof and an immutable report invocation.
  /// The descriptor contains verified identities and classes, never test bodies.
  pub fn with_protected_validation(
    mut self,
    validation: &crate::FactoryProtectedValidationMaterialization,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::Invalid;
    const DESTINATION: &str = "/octacity/protected/validation-input.json";
    if validation.input() != &self.input
      || self.validation.is_some()
      || self.promotion
      || self
        .expected
        .iter()
        .any(|(destination, _, _)| destination == DESTINATION)
    {
      return Err(Invalid);
    }
    let (_, additional) = validation.wire_inputs(&[]).map_err(|_| Invalid)?;
    if self.additional_inputs.iter().any(|input| !additional.contains(input)) {
      return Err(Invalid);
    }
    let bytes = validation.command_invocation_bytes().map_err(|_| Invalid)?;
    self
      .documents
      .push(FactoryExecutionDocument::new(DESTINATION, "application/json", bytes.clone()).map_err(|_| Invalid)?);
    self
      .expected
      .push((DESTINATION.to_owned(), "application/json".to_owned(), bytes));
    self.additional_inputs = additional;
    self.validation = Some(validation.clone());
    Ok(self)
  }
  /// Adds the exact independently frozen deployment plan selected by an
  /// explicitly installed promotion compiler. This document grants no readiness.
  pub fn with_promotion(
    mut self,
    plan: &octacity_server_factory::PromotionPlan,
    admitted: &AdmittedFlow,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::Invalid;
    const DESTINATION: &str = "/octacity/protected/promotion-input.json";
    if self.validation.is_some()
      || self.promotion
      || octacity_server_factory::PromotionPlan::freeze(admitted, &self.input).map_err(|_| Invalid)? != *plan
      || self
        .expected
        .iter()
        .any(|(destination, _, _)| destination == DESTINATION)
    {
      return Err(Invalid);
    }
    let bytes = serde_json::to_vec(&promotion_action_input(plan)?).map_err(|_| Invalid)?;
    self
      .documents
      .push(FactoryExecutionDocument::new(DESTINATION, "application/json", bytes.clone()).map_err(|_| Invalid)?);
    self
      .expected
      .push((DESTINATION.to_owned(), "application/json".to_owned(), bytes));
    self.promotion = true;
    Ok(self)
  }
  /// Exact retained inputs accepted by the typed compiler selection.
  pub fn additional_inputs(&self) -> &[octacity_protocol::ProtectedInputV3] {
    &self.additional_inputs
  }
  /// Proves that this exact published program came from the qualified installation.
  pub fn command_proof(
    &self,
    program: &FactoryManagedJobProgram,
  ) -> Result<FactoryManagedCommandProof, OrdinaryBuildApplicationError> {
    let expected = self.managed_program(program.protected_inputs.clone())?;
    if self.authority.is_none()
      || expected.execution != program.execution
      || expected.required_enforcement != program.required_enforcement
    {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(FactoryManagedCommandProof {
      intent_digest: self.intent_digest,
      program_digest: command_program_digest(program),
    })
  }
  /// Attribution proves all generated documents for the installed role. Retained
  /// candidate/private inputs are independently proved before semantic completion.
  pub(crate) fn attribution_program(
    &self,
    manifest: &octacity_protocol::ProtectedInputManifestV3,
  ) -> Result<FactoryManagedJobProgram, OrdinaryBuildApplicationError> {
    let generated = octacity_protocol::ProtectedInputManifestV3 {
      inputs: manifest
        .inputs
        .iter()
        .filter(|input| {
          self
            .expected
            .iter()
            .any(|(destination, _, _)| input.destination == *destination)
        })
        .cloned()
        .collect(),
    };
    self.managed_program(generated)
  }
  /// Accepts only the exact published document set, including executable bytes.
  pub fn managed_program(
    &self,
    manifest: ProtectedInputManifestV3,
  ) -> Result<FactoryManagedJobProgram, OrdinaryBuildApplicationError> {
    let invalid = || OrdinaryBuildApplicationError::Invalid;
    manifest.validate().map_err(|_| invalid())?;
    if manifest.inputs.len() != self.expected.len() + self.additional_inputs.len()
      || self
        .additional_inputs
        .iter()
        .any(|expected| !manifest.inputs.contains(expected))
    {
      return Err(invalid());
    }
    for (destination, media_type, bytes) in &self.expected {
      if !manifest.inputs.iter().any(|input| {
        input.destination == *destination
          && input.media_type == *media_type
          && input.size_bytes == bytes.len() as u64
          && input.sha256 == FactoryDigest::content_sha256(bytes).to_string()
      }) {
        return Err(invalid());
      }
    }
    if let Some(validation) = &self.validation {
      let generated = manifest
        .inputs
        .iter()
        .filter(|input| !self.additional_inputs.contains(input))
        .cloned()
        .collect::<Vec<_>>();
      let (_, expected) = validation.wire_inputs(&generated).map_err(|_| invalid())?;
      if expected != manifest.inputs {
        return Err(invalid());
      }
    }
    let octafile_input = manifest
      .inputs
      .iter()
      .find(|input| input.destination == OCTAFILE_DESTINATION)
      .ok_or_else(invalid)?
      .artifact_id
      .clone();
    Ok(FactoryManagedJobProgram {
      execution: ManagedOctaExecutionV3 {
        workspace_root: self.program.workspace_root.clone(),
        octafile_input,
        tasks: self.program.tasks.clone(),
        credential_profile: self
          .program
          .credential_profile
          .as_ref()
          .map(|key| key.as_str().to_owned()),
        mcp_credentials: Default::default(),
        tool_control: None,
      },
      protected_inputs: manifest,
      required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    })
  }
}

/// Freezes admitted operation data without accepting commands from Flow data.
/// Publication, current ownership and Build admission remain separate boundaries.
pub fn compile_factory_command_task(
  intent: &FlowBuildIntent,
  admitted: &AdmittedFlow,
  profile: &FactoryCommandBuildProfile,
) -> Result<CompiledFactoryCommandTask, OrdinaryBuildApplicationError> {
  let invalid = || OrdinaryBuildApplicationError::Invalid;
  let reconstructed = FlowBuildIntent::new(
    intent.input().clone(),
    admitted,
    intent.node().clone(),
    intent.usage_before(),
    intent.priority(),
  )
  .map_err(|_| invalid())?;
  if &reconstructed != intent {
    return Err(invalid());
  }
  if !profile.matches_intent(intent)
    || profile.input_destination == OCTAFILE_DESTINATION
    || profile.program.tasks.is_empty()
  {
    return Err(invalid());
  }
  if profile.observation_contract.is_some()
    && !profile
      .observation_bindings()?
      .iter()
      .any(|binding| binding.result_output == intent.profile().result_output)
  {
    return Err(invalid());
  }
  let binding = admitted
    .closure()
    .definition(intent.input().definition())
    .and_then(|definition| definition.node(intent.input().node()))
    .and_then(|node| node.build())
    .ok_or_else(invalid)?;
  let result = profile
    .outputs
    .iter()
    .find(|output| output.kind() == &binding.result_output)
    .ok_or_else(invalid)?;
  let node = admitted
    .closure()
    .definition(intent.input().definition())
    .and_then(|definition| definition.node(intent.input().node()))
    .ok_or_else(invalid)?;
  let outcome = node
    .outcomes()
    .iter()
    .find(|outcome| outcome.key() == &binding.result_outcome)
    .ok_or_else(invalid)?;
  if result.schema() != outcome.schema()
    || result.output_kind() != binding.result_output_kind
    || result.tool() != binding.result_tool.as_ref().unwrap_or(&binding.tool)
    || result.plugin() != &binding.plugin
    || profile.outputs.len() != binding.evidence.len() + 1
    || binding
      .evidence
      .iter()
      .any(|expected| !profile.outputs.contains(expected))
  {
    return Err(invalid());
  }
  let bytes = serde_json::to_vec(intent).map_err(|_| invalid())?;
  let expected = vec![
    (
      OCTAFILE_DESTINATION.to_owned(),
      "application/yaml".to_owned(),
      profile.program.octafile.as_str().as_bytes().to_vec(),
    ),
    (profile.input_destination.clone(), "application/json".to_owned(), bytes),
  ];
  let documents = expected
    .iter()
    .map(|(destination, media, bytes)| {
      FactoryExecutionDocument::new(destination.clone(), media.clone(), bytes.clone()).map_err(|_| invalid())
    })
    .collect::<Result<Vec<_>, _>>()?;
  Ok(CompiledFactoryCommandTask {
    input: intent.input().clone(),
    additional_inputs: Vec::new(),
    validation: None,
    promotion: false,
    intent_digest: FactoryDigest::content_sha256(&serde_json::to_vec(intent).map_err(|_| invalid())?),
    authority: (profile.builtin_authority() && intent.permissions().is_no_broader_than(&profile.project_permissions))
      .then_some(StaticCommandAuthority::Builtin),
    program: profile.program.clone(),
    documents,
    expected,
  })
}

pub(crate) fn promotion_action_input(
  plan: &octacity_server_factory::PromotionPlan,
) -> Result<serde_json::Value, OrdinaryBuildApplicationError> {
  let digest = plan.digest().map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
  Ok(serde_json::json!({"plan":plan,"plan_digest":digest}))
}

/// Opaque exact-program authority produced only by the installed command compiler.
#[derive(Clone)]
pub struct FactoryManagedCommandProof {
  intent_digest: FactoryDigest,
  program_digest: FactoryDigest,
}
impl FactoryManagedCommandProof {
  pub(crate) fn verify(
    &self,
    intent: &FlowBuildIntent,
    programs: &BTreeMap<PipelineNodeId, FactoryManagedJobProgram>,
  ) -> bool {
    serde_json::to_vec(intent).is_ok_and(|bytes| FactoryDigest::content_sha256(&bytes) == self.intent_digest)
      && programs.len() == 1
      && programs
        .values()
        .all(|program| command_program_digest(program) == self.program_digest)
  }
}
fn command_program_digest(program: &FactoryManagedJobProgram) -> FactoryDigest {
  FactoryDigest::content_sha256(
    &serde_json::to_vec(&(
      &program.execution,
      &program.protected_inputs,
      &program.required_enforcement,
    ))
    .expect("typed managed command program serializes"),
  )
}
