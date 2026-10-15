//! Ordinary Build preparation from an exact installed command and retained inputs.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::ArtifactBytePublisher;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{AdmittedFlow, FactoryClaimOwnership, FlowBuildIntent, resolve_factory_permissions};
use octacity_server_store::{AcceptFactoryBuild, FactoryExecutionInputStore};
use std::{collections::BTreeMap, sync::Arc};

/// Uses the ordinary owner fence, Artifact publication and signed Build boundary.
pub struct FactoryCommandBuildMaterializer<C, S, B> {
  contexts: Arc<C>,
  publisher: Arc<FactoryExecutionInputPublisher<S, B>>,
  catalog: crate::factory_command_catalog::FactoryCommandCatalog,
  candidate_context: Option<Arc<dyn FactoryNodeCandidateContextSource>>,
  validation: Option<Arc<dyn FactoryProtectedValidationInputSource>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<C, S, B> FactoryCommandBuildMaterializer<C, S, B> {
  /// Installs exact immutable context resolution and finite operator profiles.
  pub fn new(
    contexts: Arc<C>,
    publisher: Arc<FactoryExecutionInputPublisher<S, B>>,
    profiles: Vec<FactoryCommandBuildProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    if profiles.is_empty()
      || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles.iter().enumerate().any(|(index, profile)| {
        profiles[..index]
          .iter()
          .any(|prior| prior.task_digest() == profile.task_digest())
      })
    {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(Self {
      contexts,
      publisher,
      catalog: crate::factory_command_catalog::FactoryCommandCatalog::builtin(profiles)?,
      candidate_context: None,
      validation: None,
      clock,
    })
  }
  /// Installs explicit deployment qualification and its trusted approved toolchain.
  /// The actual ordinary context must still match this snapshot before publication.
  pub fn for_deployment(
    contexts: Arc<C>,
    publisher: Arc<FactoryExecutionInputPublisher<S, B>>,
    profiles: Vec<FactoryDeploymentCommandProfile>,
    approved: JobSpecToolchainPolicy,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    Ok(Self {
      contexts,
      publisher,
      catalog: crate::factory_command_catalog::FactoryCommandCatalog::deployment(profiles, approved)?,
      candidate_context: None,
      validation: None,
      clock,
    })
  }
  /// Adds exact owner-fenced candidate reconstruction; absent by default.
  #[must_use]
  pub fn with_candidate_context_source(mut self, source: Arc<dyn FactoryNodeCandidateContextSource>) -> Self {
    self.candidate_context = Some(source);
    self
  }
  /// Adds the original private validation selection shared with result verification.
  #[must_use]
  pub fn with_protected_validation_input_source(
    mut self,
    source: Arc<dyn FactoryProtectedValidationInputSource>,
  ) -> Self {
    self.validation = Some(source);
    self
  }
  /// Selects the installed promotion role. Ordinary commands never infer it
  /// from a Flow kind, action name or untrusted request payload.
  #[must_use]
  pub fn with_promotion(mut self) -> Self {
    self.catalog = self.catalog.with_promotion();
    self
  }
}
#[async_trait]
impl<C: BuildExecutionContextSource, S: FactoryExecutionInputStore, B: ArtifactBytePublisher> FactoryBuildMaterializer
  for FactoryCommandBuildMaterializer<C, S, B>
{
  fn action_input(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
  ) -> Result<Option<serde_json::Value>, OrdinaryBuildApplicationError> {
    if !self.catalog.is_promotion() {
      return Ok(None);
    }
    if self.validation.is_some()
      || self
        .catalog
        .profiles()
        .filter(|profile| profile.matches_intent(intent))
        .count()
        != 1
    {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    let plan = octacity_server_factory::PromotionPlan::freeze(admitted, intent.input())
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    crate::factory_command::promotion_action_input(&plan).map(Some)
  }
  async fn materialize(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
    ownership: FactoryClaimOwnership,
    at: Timestamp,
  ) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::{Conflict, Invalid, Unavailable};
    if at >= intent.node().deadline() || intent.node().verify_observer(&ownership, at).is_err() {
      return Err(Invalid);
    }
    let profile = self.catalog.profile(intent)?;
    let effective = resolve_factory_permissions(
      &profile.project_permissions,
      admitted.limits().permissions(),
      intent.permissions(),
      &profile.local_permissions,
    )
    .map_err(|_| Invalid)?;
    if &effective != intent.permissions() {
      return Err(Invalid);
    }
    if self.catalog.is_promotion() && self.validation.is_some() {
      return Err(Invalid);
    }
    let mut compiled = self.catalog.compile(intent, admitted)?;
    let candidate_selected = intent
      .input()
      .context()
      .is_some_and(|context| context.subject().candidate().is_some());
    if candidate_selected && self.candidate_context.is_none() {
      return Err(Invalid);
    }
    let context = self
      .contexts
      .load(
        intent.input().subject().project_id(),
        profile.build_configuration.id(),
        profile.build_configuration.version(),
      )
      .await
      .map_err(|_| Unavailable)?;
    self.catalog.validate_dispatch(intent, &context.job_spec_toolchain)?;
    if context.pipeline.dag.nodes().len() != 1 || context.pipeline.dag.nodes()[0].id() != &profile.pipeline_node {
      return Err(Invalid);
    }
    let now = self.clock.now().map_err(|_| Unavailable)?;
    if now >= intent.node().deadline() || intent.node().verify_observer(&ownership, now).is_err() {
      return Err(Conflict);
    }
    let candidate = if candidate_selected {
      Some(
        self
          .candidate_context
          .as_ref()
          .ok_or(Invalid)?
          .read_candidate_context(intent, &ownership, now, intent.node().deadline())
          .await
          .map_err(|_| Invalid)?,
      )
    } else {
      None
    };
    if let Some(context) = &candidate {
      compiled = compiled.with_candidate(context.candidate(), now)?;
    }
    if let Some(source) = &self.validation {
      let validation = source.materialization(intent).await.map_err(|_| Invalid)?;
      compiled = compiled.with_protected_validation(&validation)?;
    }
    let mut manifest = self
      .publisher
      .publish(intent, &ownership, compiled.documents().to_vec())
      .await
      .map_err(|_| Unavailable)?;
    manifest.inputs.extend_from_slice(compiled.additional_inputs());
    manifest
      .inputs
      .sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    let program = compiled.managed_program(manifest)?;
    let command = compiled.command_proof(&program)?;
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
        command: Some(command),
        candidate: candidate.map(|context| context.candidate().clone()),
        capture: None,
      },
    )
  }
}
