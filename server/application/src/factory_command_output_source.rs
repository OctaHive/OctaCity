//! Retained JSON observations from exact operator-installed ordinary command Jobs.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_server_domain::{AttemptId, AttemptNumber, BuildId, Timestamp};
use octacity_server_factory::*;
use octacity_server_job::JobState;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::*;
use std::{collections::BTreeSet, sync::Arc};

/// Matches the immutable executed program and owner intent before interpreting
/// retained JSON. Schemas and producer pins come only from installed profiles.
/// Command profiles carry no model accounting: elapsed consumption is a
/// conservative server wall-time bound, including queueing and cleanup.
pub struct FactoryCommandBuildOutputSource<R, Q, A, B> {
  runs: Arc<R>,
  builds: Arc<Q>,
  artifacts: Arc<A>,
  reader: FactoryArtifactReader<A, Q, B>,
  catalog: crate::factory_command_catalog::FactoryCommandCatalog,
  candidate_context: Option<Arc<dyn FactoryRetainedCandidateContextSource>>,
  validation_input: Option<Arc<dyn FactoryProtectedValidationInputSource>>,
  authors: Vec<FactoryProtectedTestAuthoringProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactByteReader> FactoryCommandBuildOutputSource<R, Q, A, B> {
  /// Installs authoritative ordinary records, retained byte access and finite
  /// command profiles. JSON cannot select an alternative verifier or schema.
  pub fn new(
    runs: Arc<R>,
    builds: Arc<Q>,
    artifacts: Arc<A>,
    bytes: Arc<B>,
    profiles: Vec<FactoryCommandBuildProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(Self {
      runs,
      reader: FactoryArtifactReader::new(artifacts.clone(), builds.clone(), bytes),
      builds,
      artifacts,
      catalog: crate::factory_command_catalog::FactoryCommandCatalog::builtin(profiles)?,
      candidate_context: None,
      validation_input: None,
      authors: vec![],
      clock,
    })
  }

  /// Installs the same bounded deployment role and approved toolchain as dispatch.
  /// Approval is operator-owned and is never reconstructed from Job/report data.
  pub fn for_deployment(
    runs: Arc<R>,
    builds: Arc<Q>,
    artifacts: Arc<A>,
    bytes: Arc<B>,
    profiles: Vec<FactoryDeploymentCommandProfile>,
    approved: JobSpecToolchainPolicy,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    Ok(Self {
      runs,
      reader: FactoryArtifactReader::new(artifacts.clone(), builds.clone(), bytes),
      builds,
      artifacts,
      catalog: crate::factory_command_catalog::FactoryCommandCatalog::deployment(profiles, approved)?,
      candidate_context: None,
      validation_input: None,
      authors: vec![],
      clock,
    })
  }

  /// Installs the exact private output policy for protected-test authors.
  /// Files and inventory remain opaque Artifacts; only the public Report is
  /// interpreted as Flow data. The author adapter separately verifies the bundle.
  pub fn with_protected_test_authoring(
    mut self,
    authors: Vec<FactoryProtectedTestAuthoringProfile>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::Invalid;
    let mut seen = BTreeSet::new();
    if authors.len() != self.catalog.profiles().count()
      || authors.iter().any(|author| {
        !seen.insert(author.command().task_digest())
          || author.files().len() + author.command().outputs.len() >= usize::from(MAX_ARTIFACT_PAGE_SIZE)
          || !self
            .catalog
            .profiles()
            .any(|profile| profile.task_digest() == author.command().task_digest())
      })
    {
      return Err(Invalid);
    }
    self.authors = authors;
    Ok(self)
  }

  fn author(&self, intent: &FlowBuildIntent) -> Option<&FactoryProtectedTestAuthoringProfile> {
    self
      .authors
      .iter()
      .find(|author| author.command().matches_intent(intent))
  }

  /// Installs exact retained candidate recovery for readonly command consumers.
  /// No candidate input is accepted through the default observer.
  #[must_use]
  pub fn with_retained_candidate_context_source(
    mut self,
    source: Arc<dyn FactoryRetainedCandidateContextSource>,
  ) -> Self {
    self.candidate_context = Some(source);
    self
  }

  /// Installs the same exact private input proof used by the command compiler.
  /// Its closed retained identities and generated invocation metadata must all
  /// match the signed Job before a JSON result can become a semantic observation.
  #[must_use]
  pub fn with_protected_validation_input_source(
    mut self,
    source: Arc<dyn FactoryProtectedValidationInputSource>,
  ) -> Self {
    self.validation_input = Some(source);
    self
  }

  /// Selects the installed promotion role explicitly. Its immutable plan is
  /// rederived from the persisted admission and input, never from report JSON.
  #[must_use]
  pub fn with_promotion(mut self) -> Self {
    self.catalog = self.catalog.with_promotion();
    self
  }
}

struct InspectedAttempt {
  snapshot: FactoryRunSnapshot,
  intent: FlowBuildIntent,
  profile: FactoryCommandBuildProfile,
  uploads: Vec<ArtifactUploadRecord>,
  usage: BudgetUsage,
  succeeded: bool,
  observed_at: Timestamp,
  program: FactoryManagedJobProgram,
  candidate_instruction: Option<octacity_protocol::ChangeSetMaterializationV3>,
}

impl<R: FactoryRunStore, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactByteReader>
  FactoryCommandBuildOutputSource<R, Q, A, B>
{
  async fn inspect(
    &self,
    build_id: BuildId,
    attempt_id: AttemptId,
  ) -> Result<InspectedAttempt, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::{Invalid, Unavailable};
    let now = self.clock.now().map_err(|_| Unavailable)?;
    let build = self.builds.build(build_id).await.map_err(classify)?;
    let attempt = self.builds.attempt(attempt_id).await.map_err(classify)?;
    if build.build.id != build_id
      || !build.state.is_terminal()
      || build.updated_at > now
      || attempt.id != attempt_id
      || attempt.build_id != build_id
      || !attempt.state.is_terminal()
      || attempt.updated_at > now
      || attempt.jobs.len() != 1
    {
      return Err(Invalid);
    }
    let BuildOrigin::Factory(origin) = &build.origin else {
      return Err(Invalid);
    };
    let snapshot = self.runs.factory_run_snapshot(origin.run_id).await.map_err(classify)?;
    let mut intents = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .filter(|intent| intent.node().id() == origin.node_attempt_id);
    let intent = intents.next().ok_or(Invalid)?.clone();
    if intents.next().is_some()
      || FactoryBuildOrigin::from_intent(&intent).map_err(|_| Invalid)? != *origin
      || build.build.project_id != intent.input().subject().project_id()
      || build.build.repository_id != intent.input().subject().repository_id()
      || build.build.configuration_id != intent.profile().build_configuration.id()
      || build.build.configuration_version != intent.profile().build_configuration.version()
      || build.build.immutable_revision != *intent.input().subject().base_revision()
    {
      return Err(Invalid);
    }
    let profile = self.catalog.profile(&intent)?;
    let job = &attempt.jobs[0];
    let terminal = job.terminal.as_ref().ok_or(Unavailable)?;
    if job.attempt_id != attempt_id
      || !job.state.is_terminal()
      || job.assignment.is_none()
      || terminal.state != job.state
      || terminal.completed_at > now
      || job.updated_at > now
      || job.created_at < attempt.created_at
      || job.created_at < build.created_at
      || job.created_at < intent.node().started_at()
      || terminal.completed_at < job.created_at
    {
      return Err(Invalid);
    }
    self
      .catalog
      .validate_observed_toolchain(&build.build, &job.job.job_spec_template)?;
    let compiled = self.catalog.compile(&intent, &snapshot.admitted_flow)?;
    let program = verify_compiled_command_job(&intent, &snapshot.admitted_flow, profile, job, build_id, &compiled)?;
    let managed = job.job.job_spec_template.managed_intent().ok_or(Invalid)?;
    let uploads = self
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id,
        attempt_id: Some(attempt_id),
        limit: MAX_ARTIFACT_PAGE_SIZE,
        visibility: ArtifactListVisibility::all(),
      })
      .await
      .map_err(classify)?;
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for upload in &uploads {
      let identity = upload.artifact.identity();
      if identity.build_id != build_id
        || identity.attempt_id != attempt_id
        || identity.job_id != job.id()
        || !names.insert(identity.logical_name.as_str())
        || !profile
          .outputs
          .iter()
          .any(|output| output.kind().as_str() == identity.logical_name.as_str())
          && !self.author(&intent).is_some_and(|author| {
            author
              .files()
              .iter()
              .any(|file| file.output.as_str() == identity.logical_name.as_str())
          })
      {
        return Err(Invalid);
      }
      total = total.checked_add(identity.size_bytes).ok_or(Invalid)?;
    }
    // This is an upper bound over ordinary server-owned timestamps, not an
    // incomplete periodic Agent sample or self-reported command JSON usage.
    let elapsed_millis = u64::try_from(
      terminal
        .completed_at
        .unix_millis()
        .checked_sub(job.created_at.unix_millis())
        .ok_or(Invalid)?,
    )
    .map_err(|_| Invalid)?;
    Ok(InspectedAttempt {
      intent,
      profile: profile.clone(),
      uploads,
      usage: BudgetUsage {
        attempts: 0,
        elapsed_millis,
        output_bytes: total,
        tokens: 0,
        cost_micro_units: 0,
      },
      succeeded: build.state == BuildState::Succeeded
        && attempt.state == AttemptState::Succeeded
        && job.state == JobState::Succeeded,
      snapshot,
      observed_at: now,
      program,
      candidate_instruction: managed.factory().change_set_materialization.clone(),
    })
  }

  async fn current(
    &self,
    build: BuildId,
    attempt: AttemptId,
  ) -> Result<InspectedAttempt, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::Invalid;
    let latest = self.builds.latest_attempt(build).await.map_err(classify)?;
    if latest.id != attempt || latest.build_id != build {
      return Err(Invalid);
    }
    let mut inspected = self.inspect(build, attempt).await?;
    let mut current = latest;
    let mut seen = BTreeSet::from([attempt]);
    for _ in 0..MAX_FACTORY_ATTEMPTS {
      let Some(previous) = current.retry_of_attempt_id else {
        if current.number != AttemptNumber::FIRST {
          return Err(Invalid);
        }
        return Ok(inspected);
      };
      if !seen.insert(previous) {
        return Err(Invalid);
      }
      let predecessor = self.builds.attempt(previous).await.map_err(classify)?;
      if predecessor.id != previous
        || predecessor.build_id != build
        || predecessor.number.get().checked_add(1) != Some(current.number.get())
      {
        return Err(Invalid);
      }
      let spent = self.inspect(build, previous).await?;
      inspected.usage = inspected.usage.checked_add(spent.usage).ok_or(Invalid)?;
      current = predecessor;
    }
    Err(Invalid)
  }

  async fn outputs(
    &self,
    inspected: InspectedAttempt,
    maximum: u64,
  ) -> Result<FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::{Invalid, Unavailable};
    // Terminal resource facts survive cancellation. Only semantic completion
    // requires the input to remain the current uncancelled consumer.
    crate::factory_stored_candidate_context::verify_retained_consumer(&inspected.snapshot, &inspected.intent)
      .map_err(|_| Invalid)?;
    if !inspected.succeeded {
      return Ok(FactoryNodeBuildOutputObservation::ExecutionStopped { usage: inspected.usage });
    }
    let InspectedAttempt {
      snapshot,
      intent,
      profile,
      uploads,
      usage,
      observed_at: now,
      program,
      candidate_instruction,
      ..
    } = inspected;
    if maximum > MAX_ARTIFACT_READ_BYTES
      || usage.output_bytes > maximum
      || usage.output_bytes > intent.budget().max_output_bytes()
    {
      return Err(Invalid);
    }
    let author = self.author(&intent);
    if uploads.len() != profile.outputs.len() + author.map_or(0, |author| author.files().len()) {
      return Err(Unavailable);
    }
    if (self.catalog.is_promotion() && self.validation_input.is_some())
      || (author.is_some() && (self.catalog.is_promotion() || self.validation_input.is_some()))
    {
      return Err(Invalid);
    }
    let required_until =
      Timestamp::from_unix_millis(now.unix_millis().checked_add(1).ok_or(Invalid)?).map_err(|_| Invalid)?;
    let has_candidate = intent
      .input()
      .context()
      .is_some_and(|context| context.subject().candidate().is_some());
    if has_candidate && profile.candidate_capture_tool.is_none() {
      return Err(Invalid);
    }
    let validation = if let Some(source) = &self.validation_input {
      if !has_candidate {
        return Err(Invalid);
      }
      let materialized = source.materialization(&intent).await.map_err(|_| Unavailable)?;
      if materialized.input() != intent.input() || now >= materialized.retained_until() {
        return Err(Invalid);
      }
      Some(materialized)
    } else {
      None
    };
    let candidate = if has_candidate && validation.is_none() {
      Some(
        self
          .candidate_context
          .as_ref()
          .ok_or(Invalid)?
          .read_retained_candidate_context(&intent, now, intent.node().deadline().max(required_until))
          .await
          .map_err(|_| Unavailable)?,
      )
    } else {
      None
    };
    let compiled = self.catalog.compile(&intent, &snapshot.admitted_flow)?;
    let compiled = if let Some(validation) = &validation {
      compiled.with_protected_validation(validation)?
    } else if let Some(candidate) = &candidate {
      compiled.with_candidate(candidate.candidate(), now)?
    } else {
      compiled
    };
    if self.catalog.is_promotion() {
      PromotionPlan::freeze(&snapshot.admitted_flow, intent.input())
        .map_err(|_| Invalid)?
        .authorize(&snapshot.work, &snapshot.admitted_flow, intent.input())
        .map_err(|_| Invalid)?;
    }
    let expected = compiled.managed_program(program.protected_inputs.clone())?;
    let expected_materialization = if let Some(validation) = &validation {
      Some(validation.wire_inputs(&[]).map_err(|_| Invalid)?.0)
    } else {
      candidate
        .as_ref()
        .map(|candidate| candidate.candidate().materialization(intent.input(), now))
        .transpose()?
        .map(|source| source.wire_inputs().0)
    };
    if expected.execution != program.execution
      || expected.required_enforcement != program.required_enforcement
      || candidate_instruction != expected_materialization
    {
      return Err(Invalid);
    }
    let fresh_until = uploads
      .iter()
      .filter_map(|upload| upload.artifact.identity().retention.delete_after())
      .fold(
        validation.as_ref().map_or(intent.node().deadline(), |proof| {
          intent.node().deadline().min(proof.retained_until())
        }),
        Timestamp::min,
      );
    let verified_at = uploads
      .iter()
      .filter_map(|upload| upload.artifact.published_at())
      .max()
      .ok_or(Unavailable)?;
    if verified_at >= fresh_until {
      return Err(Invalid);
    }
    let mut documents = Vec::with_capacity(uploads.len());
    for upload in &uploads {
      let identity = upload.artifact.identity();
      let installed = profile
        .outputs
        .iter()
        .find(|output| output.kind().as_str() == identity.logical_name.as_str())
        .or_else(|| {
          author
            .filter(|author| {
              author
                .files()
                .iter()
                .any(|file| file.output.as_str() == identity.logical_name.as_str())
            })
            .map(|author| author.inventory())
        })
        .ok_or(Invalid)?;
      let kind = match identity.artifact_type {
        ArtifactType::Artifact => EvidenceOutputKind::Artifact,
        ArtifactType::Report(_) => EvidenceOutputKind::Report,
      };
      // Ordinary Report publication preserves the Agent's binary transfer type.
      // The installed schema, not this transport label, still validates JSON.
      let json_transport = identity.media_type.as_str() == "application/json"
        || (kind == EvidenceOutputKind::Report && identity.media_type.as_str() == "application/octet-stream");
      let opaque = author.is_some() && kind == EvidenceOutputKind::Artifact;
      if kind != installed.output_kind() || (!opaque && !json_transport) {
        return Err(Invalid);
      }
      let retained = self
        .reader
        .read(ReadFactoryArtifact {
          project_id: intent.input().subject().project_id(),
          artifact: FactoryArtifactReference::new(
            identity.artifact_id,
            FactoryDigest::from_bytes(identity.digest.as_bytes()),
            identity.size_bytes,
          )
          .map_err(|_| Invalid)?,
          maximum_bytes: maximum,
          observed_at: now,
          required_until: required_until.max(fresh_until),
        })
        .await
        .map_err(|_| Unavailable)?;
      let data = if opaque {
        None
      } else {
        let schema = snapshot.admitted_flow.data_schema(installed.schema()).ok_or(Invalid)?;
        let value = serde_json::from_slice(retained.bytes()).map_err(|_| Invalid)?;
        Some(FlowVerifiedBuildData::new(schema.clone(), value).map_err(|_| Invalid)?)
      };
      documents.push(FactoryNodeBuildOutputDocument {
        record: retained.record().clone(),
        bytes: retained.bytes().to_vec(),
        data,
        schema: installed.schema().clone(),
        tool: installed.tool().clone(),
        plugin: installed.plugin().clone(),
      });
    }
    let current = self
      .runs
      .factory_run_snapshot(intent.input().factory_run_id())
      .await
      .map_err(classify)?;
    crate::factory_stored_candidate_context::verify_retained_consumer(&current, &intent).map_err(|_| Invalid)?;
    let actual_now = self.clock.now().map_err(|_| Unavailable)?.max(now);
    if candidate
      .as_ref()
      .is_some_and(|context| !context.is_retained_at(actual_now))
      || validation
        .as_ref()
        .is_some_and(|proof| actual_now >= proof.retained_until())
      || uploads.iter().any(|upload| {
        upload
          .artifact
          .identity()
          .retention
          .delete_after()
          .is_some_and(|deadline| actual_now >= deadline)
      })
    {
      return Err(Invalid);
    }
    Ok(FactoryNodeBuildOutputObservation::Verified(Box::new(
      FactoryNodeBuildOutputs {
        execution: FactoryNodeBuildExecutionFacts {
          profile: intent.profile().clone(),
          input_digest: intent.input().digest().map_err(|_| Invalid)?,
          permissions: intent.permissions().clone(),
          budget: intent.budget(),
          deadline: intent.node().deadline(),
        },
        documents,
        usage: BudgetUsage { attempts: 1, ..usage },
        verified_at,
        fresh_until,
      },
    )))
  }
}

#[async_trait]
impl<R: FactoryRunStore, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactByteReader> FactoryNodeBuildOutputSource
  for FactoryCommandBuildOutputSource<R, Q, A, B>
{
  async fn terminal_usage(
    &self,
    build: BuildId,
    attempt: AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
    Ok(self.current(build, attempt).await?.usage)
  }
  async fn published_outputs(
    &self,
    build: BuildId,
    attempt: AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError> {
    self.outputs(self.current(build, attempt).await?, max_bytes).await
  }
}
fn classify(error: StoreError) -> OrdinaryBuildApplicationError {
  match error {
    StoreError::Unavailable => OrdinaryBuildApplicationError::Unavailable,
    _ => OrdinaryBuildApplicationError::Invalid,
  }
}

/// Verifies the installed command and its signed immutable Job inputs without
/// requiring terminal state. Callers independently establish current Job/Lease
/// ownership and prove any additional retained candidate or validation inputs.
pub(crate) fn verify_command_job(
  intent: &FlowBuildIntent,
  admitted: &AdmittedFlow,
  profile: &FactoryCommandBuildProfile,
  job: &JobRecord,
  build_id: BuildId,
) -> Result<FactoryManagedJobProgram, OrdinaryBuildApplicationError> {
  let compiled = compile_factory_command_task(intent, admitted, profile)?;
  verify_compiled_command_job(intent, admitted, profile, job, build_id, &compiled)
}
fn verify_compiled_command_job(
  intent: &FlowBuildIntent,
  admitted: &AdmittedFlow,
  profile: &FactoryCommandBuildProfile,
  job: &JobRecord,
  build_id: BuildId,
  compiled: &CompiledFactoryCommandTask,
) -> Result<FactoryManagedJobProgram, OrdinaryBuildApplicationError> {
  use OrdinaryBuildApplicationError::Invalid;
  let permissions = resolve_factory_permissions(
    &profile.project_permissions,
    admitted.limits().permissions(),
    intent.permissions(),
    &profile.local_permissions,
  )
  .map_err(|_| Invalid)?;
  if &permissions != intent.permissions() {
    return Err(Invalid);
  }
  let template = &job.job.job_spec_template;
  let managed = template.managed_intent().ok_or(Invalid)?;
  let causality = managed.factory();
  let origin = FactoryBuildOrigin::from_intent(intent).map_err(|_| Invalid)?;
  if template.build_id() != build_id
    || template.source_revision() != intent.input().subject().base_revision()
    || template.execution_deadline() != Some(intent.node().deadline())
    || template.pipeline_node_id() != &profile.pipeline_node
    || managed.permissions() != &factory_job_permissions(&permissions).map_err(|_| Invalid)?
    || causality.factory_run_id != origin.run_id.to_string()
    || causality.node_attempt_id != origin.node_attempt_id.to_string()
    || causality.input_digest != origin.input_digest.to_string()
    || causality.subject_digest != intent.input().subject().digest().to_string()
    || causality.factory_configuration_id != intent.input().configuration().id().to_string()
    || causality.factory_configuration_version != intent.input().configuration().version().get()
    || causality.flow_run_id != intent.input().flow_run_id().to_string()
    || causality.workflow_cycle_id != intent.input().cycle_id().to_string()
    || causality.source_write_paths.is_some()
    || causality.change_set_capture.is_some()
    || template
      .placement_policy()
      .octa
      .plugin_digests
      .get(profile.plugin.identity().as_str())
      != Some(&profile.plugin.digest().to_string())
  {
    return Err(Invalid);
  }
  template.validate().map_err(|_| Invalid)?;
  // Every generated document for this installed role participates in attribution.
  // Candidate/private retained inputs still require their separate opaque proof.
  let program = compiled.attribution_program(managed.protected_inputs())?;
  compiled.command_proof(&program)?;
  if managed.execution() != &program.execution
    || template.required_factory_enforcement() != Some(program.required_enforcement.as_slice())
  {
    return Err(Invalid);
  }
  Ok(FactoryManagedJobProgram {
    execution: managed.execution().clone(),
    protected_inputs: managed.protected_inputs().clone(),
    required_enforcement: program.required_enforcement,
  })
}
