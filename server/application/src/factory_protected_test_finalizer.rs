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
  candidates: Option<Arc<dyn FactoryRetainedCandidateContextSource>>,
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
    if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS || unique.len() != profiles.len() {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      runs,
      builds,
      artifacts,
      bytes,
      profiles,
      candidates: None,
      clock,
    })
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
    let Some(author) = self
      .profiles
      .iter()
      .find(|profile| profile.command().matches_intent(intent))
    else {
      return Ok(());
    };
    let profile = author.command();
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
      || build.build.repository_id != intent.input().subject().repository_id()
      || build.build.configuration_id != profile.build_configuration.id()
      || build.build.configuration_version != profile.build_configuration.version()
      || build.build.immutable_revision != *intent.input().subject().base_revision()
      || attempt.id != job.attempt_id
      || attempt.build_id != build_id
      || attempt.number != completion.attempt
      || attempt.state.is_terminal()
      || attempt.updated_at > now
      || attempt.jobs.as_slice() != std::slice::from_ref(&job)
    {
      return Err(invalid());
    }
    crate::factory_stored_candidate_context::verify_retained_consumer(&snapshot, intent)?;
    let program = crate::factory_command_output_source::verify_command_job(
      intent,
      &snapshot.admitted_flow,
      profile,
      &job,
      build_id,
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
    let compiled = compile_factory_command_task(intent, &snapshot.admitted_flow, profile).map_err(|_| invalid())?;
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
        .change_set_materialization
        != materialization
    {
      return Err(invalid());
    }
    let plan =
      FactoryStoredProtectedTestPlanSource::new(self.runs.clone(), profile.identity.clone(), author.policy().clone())
        .frozen_plan(intent)
        .await?;
    let execution = FlowBuildExecution::new(intent, build_id, attempt.id, vec![job.id()]).map_err(|_| invalid())?;
    let current = self.runs.factory_run_snapshot(origin.run_id).await?;
    crate::factory_stored_candidate_context::verify_retained_consumer(&current, intent)?;
    FactoryProtectedTestInventoryPublisher::new(self.artifacts.clone(), self.bytes.clone(), self.clock.clone())
      .publish_outputs(FactoryProtectedTestInventoryPublication {
        admitted: &snapshot.admitted_flow,
        intent,
        execution: &execution,
        plan: &plan,
        inventory: author.inventory(),
        files: author.files(),
        completion,
      })
      .await
  }
}
