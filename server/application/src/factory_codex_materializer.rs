//! Qualified Codex compilation into the existing ordinary Build boundary.
use crate::{
  BuildExecutionContextSource, FactoryBuildMaterializer, FactoryBuildPolicyLayers, FactoryCodexFlowCompilationOptions,
  FactoryExecutionClock, FactoryExecutionDocument, FactoryExecutionInputPublisher, FactoryManagedBuildInput,
  FactoryManagedSourcePlan, FactoryNodeCandidateContextSource, FactoryNodeContextSource, OrdinaryBuildApplicationError,
  compile_codex_flow_task_with_options, materialize_factory_managed_build_with_source,
};
use async_trait::async_trait;
use octacity_artifact_store::ArtifactBytePublisher;
use octacity_server_domain::{PipelineNodeId, Timestamp};
use octacity_server_factory::{
  AdmittedFlow, BuildConfigurationRef, FactoryClaimOwnership, FactoryCredentialConsumer, FactoryCredentialProfiles,
  FactoryPermissionSet, FactoryTaskToolchain, FactoryText, FlowBuildIntent, LocalPermissionCeiling,
};
use octacity_server_store::{AcceptFactoryBuild, FactoryExecutionInputStore};
use std::{collections::BTreeMap, sync::Arc};

/// Installed trusted producer capture, independent of candidate consumer pins.
/// This policy grants no model command or protected-tool authority and does not
/// qualify a backend. Placement separately requires a qualified workspace route.
#[derive(Clone)]
pub struct FactoryCodexCapturePolicy {
  tool: octacity_server_factory::ImmutableReference,
  instruction: octacity_protocol::ChangeSetCaptureV3,
}
impl FactoryCodexCapturePolicy {
  /// Freezes the independently installed implementation and deterministic limits.
  pub fn new(
    tool: octacity_server_factory::ImmutableReference,
    instruction: octacity_protocol::ChangeSetCaptureV3,
  ) -> Result<Self, crate::CodexCompilationError> {
    use crate::factory_codex_contract::{CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE};
    instruction
      .validate()
      .map_err(|_| crate::CodexCompilationError::CandidateWriteAccess)?;
    if instruction.observation.as_ref().is_some_and(|observation| {
      [CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE].contains(&observation.name.as_str())
    }) {
      return Err(crate::CodexCompilationError::MissingAuthority);
    }
    Ok(Self { tool, instruction })
  }
  /// Implementation whose exact pin must match declared trusted capture evidence.
  pub fn tool(&self) -> &octacity_server_factory::ImmutableReference {
    &self.tool
  }
  /// Validated instruction later signed with the exact admitted source paths.
  pub fn instruction(&self) -> &octacity_protocol::ChangeSetCaptureV3 {
    &self.instruction
  }
  pub(crate) fn task_digest(
    &self,
    base: octacity_server_factory::FactoryDigest,
  ) -> octacity_server_factory::FactoryDigest {
    let bytes = serde_json::to_vec(&(&self.tool, &self.instruction))
      .expect("validated capture policy has a canonical JSON encoding");
    octacity_server_factory::FactoryDigest::sha256(
      "octacity.factory.codex.capture-policy.v1",
      &[&base.as_bytes(), &bytes],
    )
  }
  pub(crate) fn validate_intent(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
  ) -> Result<(), crate::CodexCompilationError> {
    use crate::CodexCompilationError::MissingAuthority;
    use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT, CHANGE_SET_PATCH_OUTPUT};
    crate::factory_build_materialization::factory_capture_source_paths(intent, Some(&self.instruction))
      .map_err(|_| MissingAuthority)?;
    let definition = admitted
      .closure()
      .definition(intent.input().definition())
      .ok_or(MissingAuthority)?;
    let binding = intent.profile().binding(definition).map_err(|_| MissingAuthority)?;
    let mut required = vec![CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT];
    if self.instruction.patch_max_bytes.is_some() {
      required.push(CHANGE_SET_PATCH_OUTPUT);
    }
    if required.into_iter().any(|name| {
      !binding.evidence.iter().any(|evidence| {
        evidence.kind().as_str() == name
          && evidence.output_kind() == octacity_server_factory::EvidenceOutputKind::Artifact
          && evidence.tool() == &self.tool
          && evidence.plugin() == &intent.profile().plugin
      })
    }) {
      return Err(MissingAuthority);
    }
    if let Some(observation) = &self.instruction.observation
      && !binding.evidence.iter().any(|evidence| {
        evidence.kind().as_str() == observation.name
          && evidence.output_kind() == octacity_server_factory::EvidenceOutputKind::Report
          && evidence.schema().identity().as_str() == observation.schema.identity
          && evidence.schema().version().as_str() == observation.schema.version
          && evidence.schema().digest().to_string() == observation.schema.sha256
          && admitted.data_schema(evidence.schema()).is_some()
          && evidence.tool() == &self.tool
          && evidence.plugin() == &intent.profile().plugin
      })
    {
      return Err(MissingAuthority);
    }
    Ok(())
  }
}

/// Exact independent evaluator policy selected by the operator installation.
/// Its task pin is separate from ordinary readonly model observations.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCodexReviewerPolicy {
  /// Immutable evaluator identity selected by the frozen Evaluation Plan.
  pub evaluator: octacity_server_factory::ImmutableReference,
}
impl FactoryCodexReviewerPolicy {
  pub(crate) fn task_digest(
    &self,
    base: octacity_server_factory::FactoryDigest,
  ) -> octacity_server_factory::FactoryDigest {
    let bytes = serde_json::to_vec(self).expect("typed evaluator policy has a canonical JSON encoding");
    octacity_server_factory::FactoryDigest::sha256(
      "octacity.factory.codex.reviewer-policy.v1",
      &[&base.as_bytes(), &bytes],
    )
  }
}

/// Operator-installed execution identity and independent permission ceilings.
/// No logical Flow or node name selects these instructions or credential purpose.
#[derive(Clone)]
pub struct FactoryCodexBuildProfile {
  /// Exact installed executable, plugin and model, independent of Flow names.
  pub toolchain: FactoryTaskToolchain,
  /// Exact immutable ordinary Build Configuration authorized for this capability.
  pub build_configuration: BuildConfigurationRef,
  /// Installed instructions with the exact declared task digest.
  pub instructions: FactoryText,
  /// Separate credential purposes; contains no credential material.
  pub credentials: FactoryCredentialProfiles,
  /// Trusted purpose assigned by the installation.
  pub consumer: FactoryCredentialConsumer,
  /// Explicit cost basis frozen in the protected invocation documents.
  pub usage_policy: crate::FactoryCodexUsagePolicy,
  /// Exact server-only tool-risk pin and finite provider resource reservation.
  /// Provider credentials and API data never enter the ordinary Job profile.
  pub tool_risk: Option<crate::FactoryCodexToolRiskPolicy>,
  /// Explicit signed reader byte quotas; absent on tool-free profiles.
  pub tool_response_limits: Option<octacity_protocol::FactoryToolResponseLimitsV3>,
  /// Trusted capture implementation accepted for this consumer's retained source.
  /// This deployment metadata grants no capture or model command authority.
  pub candidate_capture_tool: Option<octacity_server_factory::ImmutableReference>,
  /// Explicit independent review role; ordinary readonly tasks retain `None`.
  pub reviewer: Option<FactoryCodexReviewerPolicy>,
  /// Independent trusted producer capture policy; absent on readonly profiles.
  pub capture: Option<FactoryCodexCapturePolicy>,
  /// Explicit qualified single-job Pipeline node; any valid identifier is accepted.
  pub pipeline_node: PipelineNodeId,
  /// Bounded ordinary Build parameters selected by the installation.
  pub parameters: BTreeMap<String, serde_json::Value>,
  /// Installed Project-owned Factory permission ceiling.
  pub project_permissions: FactoryPermissionSet,
  /// Qualified backend's enforceable permission ceiling.
  pub local_permissions: LocalPermissionCeiling,
}

impl FactoryCodexBuildProfile {
  /// Exact installed compilation policy retained alongside older policy versions.
  pub fn task_digest(&self) -> octacity_server_factory::FactoryDigest {
    let base = self.base_task_digest();
    let selected = self.tool_risk.map_or(base, |policy| policy.task_digest(base));
    let selected = self
      .capture
      .as_ref()
      .map_or(selected, |capture| capture.task_digest(selected));
    self
      .reviewer
      .as_ref()
      .map_or(selected, |reviewer| reviewer.task_digest(selected))
  }
  /// Installed model task before the separately pinned provider allocation.
  pub fn base_task_digest(&self) -> octacity_server_factory::FactoryDigest {
    crate::factory_codex::codex_response_policy_digest(
      crate::codex_flow_task_digest(
        &self.instructions,
        self.credentials.profile(self.consumer.purpose()),
        self.consumer,
        &self.usage_policy,
      ),
      self.tool_response_limits,
    )
  }
  /// Selects the complete immutable installed profile for one frozen Build intent.
  /// Sharing an executable does not share model instructions or capture authority.
  pub fn matches_intent(&self, intent: &FlowBuildIntent) -> bool {
    self.matches_role_intent(intent, None)
  }
  /// Matches the same installed adapter under an exact role/profile task identity.
  pub fn matches_role_intent(
    &self,
    intent: &FlowBuildIntent,
    role: Option<&octacity_server_factory::FactoryRoleProfile>,
  ) -> bool {
    intent.profile().tool == self.toolchain.executable
      && intent.profile().plugin == self.toolchain.plugin
      && intent.profile().model_or_tool == self.toolchain.model
      && intent.profile().build_configuration == self.build_configuration
      && intent.profile().task_digest
        == role.map_or_else(
          || self.task_digest(),
          |role| crate::factory_role_task_digest(self.task_digest(), role),
        )
  }
  /// Rejects unqualified package selections and authority before installing an executor.
  /// Actual immutable configuration and current owner checks still run per operation.
  pub fn validate_installation(
    &self,
    approved: &crate::JobSpecToolchainPolicy,
  ) -> Result<(), crate::CodexCompilationError> {
    use crate::CodexCompilationError;
    use crate::factory_codex_contract::{
      CODEX_EXECUTABLE_IDENTITY, CODEX_PLUGIN_IDENTITY, CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE,
    };
    approved
      .validate()
      .map_err(|_| CodexCompilationError::UnsupportedToolchain)?;
    if self.toolchain.plugin.identity().as_str() != CODEX_PLUGIN_IDENTITY
      || !self.usage_policy.matches_model(&self.toolchain.model)
      || self.toolchain.executable.identity().as_str() != CODEX_EXECUTABLE_IDENTITY
      || approved.octa.plugin_digests.get(CODEX_PLUGIN_IDENTITY) != Some(&self.toolchain.plugin.digest().to_string())
    {
      return Err(CodexCompilationError::UnsupportedToolchain);
    }
    let permissions = &self.project_permissions;
    if self.reviewer.is_some()
      && (self.consumer != FactoryCredentialConsumer::ReadOnlyReview
        || self.candidate_capture_tool.is_none()
        || self.capture.is_some()
        || self.tool_risk.is_some()
        || self.tool_response_limits.is_some()
        || permissions.tools().next().is_some())
    {
      return Err(CodexCompilationError::ReviewerBindings);
    }
    let protected_tools = self.consumer == FactoryCredentialConsumer::ToolReasoning;
    let author = match (self.consumer, self.capture.as_ref()) {
      (FactoryCredentialConsumer::ReadOnlyReview | FactoryCredentialConsumer::ToolReasoning, None) => false,
      (FactoryCredentialConsumer::CodingNode, Some(_)) => true,
      _ => return Err(CodexCompilationError::CredentialProfile),
    };
    if (self.tool_risk.is_some() && !protected_tools)
      || permissions.secret_profiles().collect::<Vec<_>>() != [self.credentials.profile(self.consumer.purpose())]
    {
      return Err(CodexCompilationError::CredentialProfile);
    }
    if protected_tools {
      self
        .tool_response_limits
        .ok_or(CodexCompilationError::MissingAuthority)?
        .validate()
        .map_err(|_| CodexCompilationError::MissingAuthority)?;
      if self.candidate_capture_tool.is_some() {
        return Err(CodexCompilationError::MissingAuthority);
      }
    }
    let mut expected_outputs =
      std::collections::BTreeSet::from([CODEX_RUN_TRACE, CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT]);
    if let Some(capture) = &self.capture {
      expected_outputs.extend([
        octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT,
        octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT,
      ]);
      if capture.instruction().patch_max_bytes.is_some() {
        expected_outputs.insert(octacity_protocol::CHANGE_SET_PATCH_OUTPUT);
      }
      if let Some(observation) = &capture.instruction().observation {
        expected_outputs.insert(observation.name.as_str());
      }
      if !permissions.mounts().any(|mount| {
        mount.root().as_str() == octacity_protocol::FACTORY_SOURCE_ROOT
          && mount.mode() == octacity_server_factory::MountMode::ReadWrite
      }) {
        return Err(CodexCompilationError::MissingAuthority);
      }
    }
    if !permissions
      .plugins()
      .any(|reference| reference == &self.toolchain.plugin)
      || !permissions
        .executables()
        .any(|reference| reference == &self.toolchain.executable)
      || !permissions
        .commands()
        .any(|command| command.executable() == &self.toolchain.executable)
      || permissions.tools().next().is_some() != protected_tools
      || permissions.mounts().any(|mount| {
        !(author
          && mount.root().as_str() == octacity_protocol::FACTORY_SOURCE_ROOT
          && mount.mode() == octacity_server_factory::MountMode::ReadWrite)
          && !matches!(
            (mount.root().as_str(), mount.mode()),
            (
              octacity_protocol::FACTORY_SOURCE_ROOT | octacity_protocol::PROTECTED_INPUT_ROOT,
              octacity_server_factory::MountMode::ReadOnly
            ) | (
              octacity_protocol::FACTORY_OUTPUT_ROOT | octacity_protocol::FACTORY_SCRATCH_ROOT,
              octacity_server_factory::MountMode::ReadWrite
            )
          )
      })
      || permissions
        .outputs()
        .kinds()
        .map(|kind| kind.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        != expected_outputs
    {
      return Err(CodexCompilationError::MissingAuthority);
    }
    let effective = octacity_server_factory::resolve_factory_permissions(
      permissions,
      permissions,
      permissions,
      &self.local_permissions,
    )
    .map_err(|_| CodexCompilationError::MissingAuthority)?;
    if &effective != permissions {
      return Err(CodexCompilationError::MissingAuthority);
    }
    Ok(())
  }
}

/// Publishes protected compiler documents before preparing ordinary Job templates.
/// Readonly jobs use explicit Artifact context; author capture requires an
/// independent producer policy. This boundary does not qualify native execution.
pub struct FactoryCodexBuildMaterializer<C, S, B> {
  contexts: Arc<C>,
  publisher: Arc<FactoryExecutionInputPublisher<S, B>>,
  profiles: Vec<FactoryCodexBuildProfile>,
  context_bytes: Arc<dyn FactoryNodeContextSource>,
  candidate_context: Option<Arc<dyn FactoryNodeCandidateContextSource>>,
  review: Option<Arc<dyn crate::FactoryReviewInvocationSource>>,
  role: Option<Arc<dyn crate::FactoryRoleInvocationSource>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<C, S, B> FactoryCodexBuildMaterializer<C, S, B> {
  /// Installs exact configuration resolution, fenced publication and capability policy.
  pub fn new(
    contexts: Arc<C>,
    publisher: Arc<FactoryExecutionInputPublisher<S, B>>,
    profiles: Vec<FactoryCodexBuildProfile>,
    context_bytes: Arc<dyn FactoryNodeContextSource>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, crate::CodexCompilationError> {
    if profiles.is_empty()
      || profiles.len() > crate::MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles.iter().enumerate().any(|(index, profile)| {
        profiles[..index].iter().any(|other| {
          profile.toolchain == other.toolchain
            && profile.build_configuration == other.build_configuration
            && profile.task_digest() == other.task_digest()
        })
      })
    {
      return Err(crate::CodexCompilationError::NodeBindings);
    }
    Ok(Self {
      contexts,
      publisher,
      profiles,
      context_bytes,
      candidate_context: None,
      review: None,
      role: None,
      clock,
    })
  }

  /// Adds immutable Project role content without changing static executor registration.
  #[must_use]
  pub fn with_role_invocation_source(mut self, source: Arc<dyn crate::FactoryRoleInvocationSource>) -> Self {
    self.role = Some(source);
    self
  }

  /// Installs exact whole-plan/evidence qualification for independent readonly reviews.
  #[must_use]
  pub fn with_review_invocation_source(mut self, source: Arc<dyn crate::FactoryReviewInvocationSource>) -> Self {
    self.review = Some(source);
    self
  }

  /// Installs owner-fenced recovery for exact readonly candidate consumers.
  /// Default installations retain their earlier candidate rejection; capture
  /// and writer/tool authority remain unsupported by this qualified adapter.
  #[must_use]
  pub fn with_candidate_context_source(mut self, source: Arc<dyn FactoryNodeCandidateContextSource>) -> Self {
    self.candidate_context = Some(source);
    self
  }
}
#[async_trait]
impl<
  C: BuildExecutionContextSource,
  S: FactoryExecutionInputStore + octacity_server_store::FactoryRoleInvocationStore,
  B: ArtifactBytePublisher,
> FactoryBuildMaterializer for FactoryCodexBuildMaterializer<C, S, B>
{
  async fn materialize(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
    ownership: FactoryClaimOwnership,
    at: Timestamp,
  ) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::{Conflict, Invalid, Unavailable};
    let is_candidate = intent
      .input()
      .context()
      .is_some_and(|context| context.subject().candidate().is_some());
    if is_candidate && self.candidate_context.is_none() {
      return Err(Invalid);
    }
    if at >= intent.node().deadline() || intent.node().verify_observer(&ownership, at).is_err() {
      return Err(Invalid);
    }
    let (profile, role) = if intent.selected_profile().is_none()
      && let Some(profile) = self.profiles.iter().find(|profile| profile.matches_intent(intent))
    {
      (profile, None)
    } else {
      let role = if let Some(role) = self.publisher.role_invocation(intent).await.map_err(|_| Unavailable)? {
        role
      } else {
        self
          .role
          .as_ref()
          .ok_or(Invalid)?
          .invocation(intent, at, intent.node().deadline())
          .await
          .map_err(|_| Unavailable)?
          .ok_or(Invalid)?
      };
      let profile = self
        .profiles
        .iter()
        .find(|profile| profile.matches_role_intent(intent, Some(&role.package().inputs().profile)))
        .ok_or(Invalid)?;
      (profile, Some(role))
    };
    if let Some(capture) = &profile.capture {
      if profile.consumer != FactoryCredentialConsumer::CodingNode
        || profile.tool_risk.is_some()
        || profile.tool_response_limits.is_some()
        || intent.permissions().tools().next().is_some()
      {
        return Err(Invalid);
      }
      capture.validate_intent(intent, admitted).map_err(|_| Invalid)?;
    } else {
      crate::factory_build_materialization::factory_capture_source_paths(intent, None)?;
    }
    if is_candidate
      && (profile.consumer != FactoryCredentialConsumer::ReadOnlyReview
        || profile.tool_risk.is_some()
        || intent.permissions().tools().next().is_some())
    {
      return Err(Invalid);
    }
    let effective = octacity_server_factory::resolve_factory_permissions(
      &profile.project_permissions,
      admitted.limits().permissions(),
      intent.permissions(),
      &profile.local_permissions,
    )
    .map_err(|_| Invalid)?;
    if &effective != intent.permissions() {
      return Err(Invalid);
    }
    let context = self
      .contexts
      .load(
        intent.input().subject().project_id(),
        intent.profile().build_configuration.id(),
        intent.profile().build_configuration.version(),
      )
      .await
      .map_err(|_| Unavailable)?;
    if context.pipeline.dag.nodes().len() != 1 || context.pipeline.dag.nodes()[0].id() != &profile.pipeline_node {
      return Err(Invalid);
    }
    let observed_at = self.clock.now().map_err(|_| Unavailable)?;
    if observed_at >= intent.node().deadline() || intent.node().verify_observer(&ownership, observed_at).is_err() {
      return Err(Conflict);
    }
    let candidate = if is_candidate {
      Some(
        self
          .candidate_context
          .as_ref()
          .ok_or(Invalid)?
          .read_candidate_context(intent, &ownership, observed_at, intent.node().deadline())
          .await
          .map_err(|_| Unavailable)?,
      )
    } else {
      None
    };
    let selected = if candidate.is_none() {
      Some(
        self
          .context_bytes
          .read_context(intent.input(), observed_at, intent.node().deadline())
          .await
          .map_err(|_| Unavailable)?,
      )
    } else {
      None
    };
    let review = if let Some(policy) = &profile.reviewer {
      Some(
        self
          .review
          .as_ref()
          .ok_or(Invalid)?
          .invocation(intent.input(), &policy.evaluator, observed_at)
          .await
          .map_err(|_| Invalid)?,
      )
    } else {
      None
    };
    let review = profile
      .reviewer
      .as_ref()
      .zip(review.as_ref())
      .map(|(policy, invocation)| crate::FactoryCodexReviewCompilation { policy, invocation });
    let compiled = compile_codex_flow_task_with_options(
      intent,
      admitted,
      &profile.instructions,
      &profile.credentials,
      profile.consumer,
      &profile.usage_policy,
      FactoryCodexFlowCompilationOptions {
        role: role.as_ref(),
        context: selected.as_ref(),
        candidate_context: candidate.as_ref(),
        tool_risk: profile.tool_risk,
        tool_response_limits: profile.tool_response_limits,
        capture: profile.capture.as_ref(),
        reviewer: review,
      },
    )
    .map_err(|_| Invalid)?;
    if let Some(role) = &role {
      self
        .publisher
        .retain_role_invocation(intent, &ownership, role)
        .await
        .map_err(|_| Unavailable)?;
    }
    let documents = compiled
      .documents()
      .iter()
      .map(|document| {
        FactoryExecutionDocument::new(document.destination(), document.media_type(), document.bytes().to_vec())
          .map_err(|_| Invalid)
      })
      .collect::<Result<Vec<_>, _>>()?;
    let mut manifest = self
      .publisher
      .publish(intent, &ownership, documents)
      .await
      .map_err(|_| Unavailable)?;
    manifest.inputs.extend_from_slice(compiled.additional_inputs());
    manifest.inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    let program = compiled.managed_program(manifest).map_err(|_| Invalid)?;
    let now = self.clock.now().map_err(|_| Unavailable)?;
    if now >= intent.node().deadline() || intent.node().verify_observer(&ownership, now).is_err() {
      return Err(Conflict);
    }
    materialize_factory_managed_build_with_source(
      FactoryManagedBuildInput {
        context,
        intent: intent.clone(),
        ownership,
        parameters: profile.parameters.clone(),
        programs: BTreeMap::from([(profile.pipeline_node.clone(), program)]),
        policy_layers: FactoryBuildPolicyLayers {
          project: profile.project_permissions.clone(),
          configuration: admitted.limits().permissions().clone(),
          task: intent.permissions().clone(),
          local: profile.local_permissions.clone(),
        },
        accepted_at: at,
      },
      FactoryManagedSourcePlan {
        command: None,
        candidate: candidate.map(|context| context.candidate().clone()),
        capture: profile.capture.as_ref().map(|capture| capture.instruction().clone()),
      },
    )
  }
}
