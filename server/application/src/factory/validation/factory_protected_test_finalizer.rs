//! Fenced protected-test output derivation from the actual installed command Job.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::ArtifactBytePublisher;
use octacity_server_factory::*;
use octacity_server_job::JobState;
use octacity_server_store::*;
use std::{collections::BTreeSet, sync::Arc};

/// Resolves ordinary Job, immutable Factory origin and installed author policy
/// before synthesizing inventory or public handoff bytes. Other Jobs are left
/// to their existing completion path; this component grants no execution rights.
pub struct FactoryProtectedTestOutputFinalizer<R, Q, A, B> {
  runs: Arc<R>,
  builds: Arc<Q>,
  artifacts: Arc<A>,
  bytes: Arc<B>,
  profiles: Vec<FactoryProtectedTestAuthoringProfile>,
  codex_profiles: Vec<FactoryCodexBuildProfile>,
  roles: Option<Arc<dyn FactoryRoleInvocationStore>>,
  candidates: Option<Arc<dyn FactoryRetainedCandidateContextSource>>,
  verification: Option<Arc<FactoryProjectInputReader>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, Q, A, B> FactoryProtectedTestOutputFinalizer<R, Q, A, B> {
  /// Installs a finite exact catalog; duplicate command policies are ambiguous.
  pub fn new(
    runs: Arc<R>,
    builds: Arc<Q>,
    artifacts: Arc<A>,
    bytes: Arc<B>,
    profiles: Vec<FactoryProtectedTestAuthoringProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, ApplicationError> {
    let unique = profiles
      .iter()
      .map(|profile| profile.command().task_digest())
      .collect::<BTreeSet<_>>();
    if profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS || unique.len() != profiles.len() {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      runs,
      builds,
      artifacts,
      bytes,
      profiles,
      codex_profiles: Vec::new(),
      roles: None,
      candidates: None,
      verification: None,
      clock,
    })
  }
  /// Reuses the author inventory publisher for declared Codex deliverables.
  pub fn with_codex_profiles(
    mut self,
    profiles: Vec<FactoryCodexBuildProfile>,
    roles: Arc<dyn FactoryRoleInvocationStore>,
  ) -> Result<Self, ApplicationError> {
    if profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles.iter().any(|profile| profile.protected_tests.is_none())
    {
      return Err(ApplicationError::invalid());
    }
    self.codex_profiles = profiles;
    self.roles = Some(roles);
    Ok(self)
  }
  /// Installs the same readonly retained-candidate proof as the command compiler.
  #[must_use]
  pub fn with_retained_candidate_context_source(
    mut self,
    source: Arc<dyn FactoryRetainedCandidateContextSource>,
  ) -> Self {
    self.candidates = Some(source);
    self
  }
  /// Installs the private Project attachment reader for test preparation.
  #[must_use]
  pub fn with_verification_inputs(mut self, reader: Arc<FactoryProjectInputReader>) -> Self {
    self.verification = Some(reader);
    self
  }
}
#[async_trait]
impl<R: FactoryRunStore, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactBytePublisher> AgentOutputFinalizer
  for FactoryProtectedTestOutputFinalizer<R, Q, A, B>
{
  async fn finalize(&self, completion: AgentOutputFinalization) -> Result<(), AgentExecutionError> {
    self.publish(completion).await.map_err(|error| match error {
      ApplicationError::Store(error) => error.into(),
      ApplicationError::InvalidInput | ApplicationError::FactoryValidation(_) => AgentExecutionError::InvalidRequest,
      _ => AgentExecutionError::Unavailable,
    })
  }
}
impl<R: FactoryRunStore, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactBytePublisher>
  FactoryProtectedTestOutputFinalizer<R, Q, A, B>
{
  async fn publish(&self, completion: AgentOutputFinalization) -> Result<(), ApplicationError> {
    let invalid = ApplicationError::invalid;
    let job = self.builds.job(completion.job_id).await?;
    if job.id() != completion.job_id {
      return Err(invalid());
    }
    // Lost terminal acknowledgement must use the ordinary completion ledger.
    // Do not attempt new writes under an expired, already-settled Lease.
    if job.state.is_terminal() {
      return Ok(());
    }
    let build_id = job.job.job_spec_template.build_id();
    let build = self.builds.build(build_id).await?;
    let BuildOrigin::Factory(origin) = &build.origin else {
      return Ok(());
    };
    let snapshot = self.runs.factory_run_snapshot(origin.run_id).await?;
    let mut intents = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .filter(|intent| intent.node().id() == origin.node_attempt_id);
    let intent = intents.next().ok_or_else(invalid)?;
    if intents.next().is_some() || FactoryBuildOrigin::from_intent(intent).map_err(|_| invalid())? != *origin {
      return Err(invalid());
    }
    let command = self
      .profiles
      .iter()
      .find(|profile| profile.command().matches_intent(intent));
    let codex = if command.is_none() {
      self.codex_profile(intent).await?
    } else {
      None
    };
    if command.is_none() && codex.is_none() {
      return Ok(());
    }
    let configuration = &intent.profile().build_configuration;
    let now = self.clock.now()?.max(completion.observed_at);
    let attempt = self.builds.latest_attempt(build_id).await?;
    if now >= intent.node().deadline()
      || !matches!(job.state, JobState::Leased | JobState::Running)
      || job.terminal.is_some()
      || job.updated_at > now
      || job.created_at < intent.node().started_at()
      || job
        .assignment
        .as_ref()
        .is_none_or(|assignment| assignment.agent_id != completion.lease.agent_id)
      || build.build.id != build_id
      || build.state.is_terminal()
      || build.updated_at > now
      || build.build.project_id != intent.input().subject().project_id()
      || build.build.repository_id != Some(intent.input().exact_resource().map_err(|_| invalid())?.repository_id())
      || build.build.configuration_id != configuration.id()
      || build.build.configuration_version != configuration.version()
      || build.build.immutable_revision.as_ref()
        != Some(intent.input().exact_resource().map_err(|_| invalid())?.base_revision())
      || attempt.id != job.attempt_id
      || attempt.build_id != build_id
      || attempt.number != completion.attempt
      || attempt.state.is_terminal()
      || attempt.updated_at > now
      || attempt.jobs.as_slice() != std::slice::from_ref(&job)
    {
      return Err(invalid());
    }
    crate::factory::artifacts::factory_stored_candidate_context::verify_retained_consumer(&snapshot, intent)?;
    let (profile, policy, inventory, files, transcript) = if let Some(author) = command {
      self
        .verify_command(intent, &snapshot.admitted_flow, author.command(), &job, build_id, now)
        .await?;
      (
        &author.command().identity,
        author.policy(),
        author.inventory(),
        author.files(),
        author.transcript(),
      )
    } else {
      let profile = codex.ok_or_else(invalid)?;
      let policy = profile.protected_tests.as_ref().ok_or_else(invalid)?;
      self.verify_codex_job(intent, &job, profile)?;
      (
        &profile.toolchain.model,
        policy.policy(),
        policy.inventory(),
        policy.files(),
        policy.transcript(),
      )
    };
    let plan = FactoryStoredProtectedTestPlanSource::new(self.runs.clone(), profile.clone(), policy.clone())
      .frozen_plan(intent)
      .await?;
    let supplement = if let Some(reference) = plan.supplement() {
      let retained = self
        .verification
        .as_ref()
        .ok_or_else(invalid)?
        .read_verification(ReadFactoryArtifact {
          project_id: intent.input().subject().project_id(),
          artifact: reference.clone(),
          maximum_bytes: MAX_PROTECTED_VERIFICATION_BYTES,
          observed_at: now,
          required_until: intent.node().deadline(),
        })
        .await?;
      Some(serde_json::from_slice(retained.bytes()).map_err(|_| invalid())?)
    } else {
      None
    };
    let execution = FlowBuildExecution::new(intent, build_id, attempt.id, vec![job.id()]).map_err(|_| invalid())?;
    let current = self.runs.factory_run_snapshot(origin.run_id).await?;
    crate::factory::artifacts::factory_stored_candidate_context::verify_retained_consumer(&current, intent)?;
    FactoryProtectedTestInventoryPublisher::new(self.artifacts.clone(), self.bytes.clone(), self.clock.clone())
      .publish_outputs(FactoryProtectedTestInventoryPublication {
        admitted: &snapshot.admitted_flow,
        intent,
        execution: &execution,
        plan: &plan,
        inventory,
        files,
        transcript,
        supplement: supplement.as_ref(),
        completion,
      })
      .await
  }

  async fn codex_profile(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<Option<&FactoryCodexBuildProfile>, ApplicationError> {
    if !self.codex_profiles.iter().any(|profile| {
      profile.toolchain.model == intent.profile().model_or_tool
        && profile.build_configuration == intent.profile().build_configuration
        && profile.toolchain.executable == intent.profile().tool
        && profile.toolchain.plugin == intent.profile().plugin
    }) {
      return Ok(None);
    }
    let role = if intent.selected_profile().is_some() {
      let retained = self
        .roles
        .as_ref()
        .ok_or_else(ApplicationError::invalid)?
        .factory_role_invocation(intent.operation_id().map_err(|_| ApplicationError::invalid())?)
        .await?
        .ok_or_else(ApplicationError::invalid)?;
      let role = FactoryRoleInvocation::restore(retained).map_err(|_| ApplicationError::invalid())?;
      role.verify_intent(intent).map_err(|_| ApplicationError::invalid())?;
      Some(role)
    } else {
      None
    };
    FactoryCodexBuildProfile::select_for_intent(
      &self.codex_profiles,
      intent,
      role.as_ref().map(|role| &role.package().inputs().profile),
      None,
    )
    .await
  }

  fn verify_codex_job(
    &self,
    intent: &FlowBuildIntent,
    job: &JobRecord,
    profile: &FactoryCodexBuildProfile,
  ) -> Result<(), ApplicationError> {
    let invalid = ApplicationError::invalid;
    let template = &job.job.job_spec_template;
    template.validate().map_err(|_| invalid())?;
    let managed = template.managed_intent().ok_or_else(invalid)?;
    let causality = managed.factory().ok_or_else(invalid)?;
    // The terminal observer still reconstructs and verifies the exact compiler
    // bytes and model provenance before this inventory can be used in the Flow.
    if template.execution_deadline() != Some(intent.node().deadline())
      || template.pipeline_node_id() != &profile.pipeline_node
      || template.source_revision() != Some(intent.input().exact_resource().map_err(|_| invalid())?.base_revision())
      || managed.permissions() != &factory_job_permissions(intent.permissions()).map_err(|_| invalid())?
      || causality.factory_run_id != intent.input().factory_run_id().to_string()
      || causality.node_attempt_id != intent.node().id().to_string()
      || causality.input_digest != intent.input().digest().map_err(|_| invalid())?.to_string()
      || causality.source_scope
        != factory_job_source_scope(intent.input(), intent.permissions()).map_err(|_| invalid())?
      || causality.source_write_paths.is_some()
      || causality.change_set_capture.is_some()
    {
      return Err(invalid());
    }
    Ok(())
  }

  async fn verify_command(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
    profile: &FactoryCommandBuildProfile,
    job: &JobRecord,
    build_id: octacity_server_domain::BuildId,
    now: octacity_server_domain::Timestamp,
  ) -> Result<(), ApplicationError> {
    let invalid = ApplicationError::invalid;
    let program = crate::factory::commands::factory_command_output_source::verify_command_job(
      intent, admitted, profile, job, build_id,
    )
    .map_err(|_| invalid())?;
    let candidate = if intent
      .input()
      .context()
      .is_some_and(|context| context.subject().candidate().is_some())
    {
      if profile.candidate_capture_tool.is_none() {
        return Err(invalid());
      }
      Some(
        self
          .candidates
          .as_ref()
          .ok_or_else(invalid)?
          .read_retained_candidate_context(intent, now, intent.node().deadline())
          .await?,
      )
    } else {
      None
    };
    let compiled = compile_factory_command_task(intent, admitted, profile).map_err(|_| invalid())?;
    let compiled = if let Some(candidate) = &candidate {
      compiled
        .with_candidate(candidate.candidate(), now)
        .map_err(|_| invalid())?
    } else {
      compiled
    };
    let expected = compiled
      .managed_program(program.protected_inputs.clone())
      .map_err(|_| invalid())?;
    let materialization = candidate
      .as_ref()
      .map(|candidate| candidate.candidate().materialization(intent.input(), now))
      .transpose()
      .map_err(|_| invalid())?
      .map(|proof| proof.wire_inputs().0);
    if expected.execution != program.execution
      || expected.required_enforcement != program.required_enforcement
      || job
        .job
        .job_spec_template
        .managed_intent()
        .ok_or_else(invalid)?
        .factory()
        .ok_or_else(invalid)?
        .change_set_materialization
        != materialization
    {
      return Err(invalid());
    }
    Ok(())
  }
}
