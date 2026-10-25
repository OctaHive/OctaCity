//! Qualified retained Codex observations over ordinary immutable Job facts.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::ArtifactByteReader;
use octacity_server_domain::{AttemptId, BuildId, Timestamp};
use octacity_server_factory::*;
use octacity_server_job::JobState;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::*;
use std::sync::Arc;

/// Reads ordinary publications only after matching their executed signed-ready
/// Job program to the exact installed compiler and frozen common input.
/// Wire formats and harness statuses stay inside this Codex adapter.
pub struct FactoryCodexBuildOutputSource<R, Q, A, B> {
  runs: Arc<R>,
  builds: Arc<Q>,
  artifacts: Arc<A>,
  reader: FactoryArtifactReader<A, Q, B>,
  profiles: Vec<FactoryCodexBuildProfile>,
  context: Arc<dyn FactoryNodeContextSource>,
  candidate_context: Option<Arc<dyn FactoryRetainedCandidateContextSource>>,
  review: Option<Arc<dyn FactoryReviewInvocationSource>>,
  role: Option<Arc<dyn FactoryRoleInvocationStore>>,
  runtime_settings: Option<Arc<dyn FactoryRuntimeSettingsStore>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactByteReader> FactoryCodexBuildOutputSource<R, Q, A, B> {
  /// Connects bounded authoritative records, retained bytes and installed profiles.
  pub fn new(
    runs: Arc<R>,
    builds: Arc<Q>,
    artifacts: Arc<A>,
    bytes: Arc<B>,
    profiles: Vec<FactoryCodexBuildProfile>,
    context: Arc<dyn FactoryNodeContextSource>,
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
      profiles,
      context,
      candidate_context: None,
      review: None,
      role: None,
      runtime_settings: None,
      clock,
    })
  }

  /// Reads the original private package through the existing publication store.
  /// Static tasks do not consult this optional dependency or a mutable role catalog.
  #[must_use]
  pub fn with_role_invocation_store(mut self, store: Arc<dyn FactoryRoleInvocationStore>) -> Self {
    self.role = Some(store);
    self
  }

  /// Reconstructs original compiler documents from exact retained Project MCP pins.
  #[must_use]
  pub fn with_runtime_settings_store(mut self, store: Arc<dyn FactoryRuntimeSettingsStore>) -> Self {
    self.runtime_settings = Some(store);
    self
  }

  /// Installs journal qualification for explicitly selected independent review plans.
  /// Retained observation reconstructs the original invocation at its frozen start.
  #[must_use]
  pub fn with_review_invocation_source(mut self, source: Arc<dyn FactoryReviewInvocationSource>) -> Self {
    self.review = Some(source);
    self
  }

  /// Installs readonly recovery for exact candidate inputs and retained lineage.
  /// The default rejects candidate observations; capture and write roles remain unsupported.
  #[must_use]
  pub fn with_retained_candidate_context_source(
    mut self,
    source: Arc<dyn FactoryRetainedCandidateContextSource>,
  ) -> Self {
    self.candidate_context = Some(source);
    self
  }
}
struct Observation {
  outputs: FactoryNodeBuildOutputObservation,
  usage: BudgetUsage,
}
impl<R: FactoryRunStore, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactByteReader>
  FactoryCodexBuildOutputSource<R, Q, A, B>
{
  async fn observe_history(
    &self,
    build: BuildId,
    attempt: AttemptId,
    maximum: u64,
  ) -> Result<Observation, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::Invalid;
    let current = self.builds.latest_attempt(build).await.map_err(classify)?;
    if current.build_id != build || current.id != attempt {
      return Err(Invalid);
    }
    let mut observed = self.observe_attempt(build, attempt, maximum).await?;
    let mut current = self.builds.attempt(attempt).await.map_err(classify)?;
    let mut seen = std::collections::BTreeSet::from([attempt]);
    let mut usage = BudgetUsage {
      attempts: 0,
      ..observed.usage
    };
    for _ in 0..MAX_FACTORY_ATTEMPTS {
      let Some(previous) = current.retry_of_attempt_id else {
        if current.number != octacity_server_domain::AttemptNumber::FIRST {
          return Err(Invalid);
        }
        observed.usage = usage;
        match &mut observed.outputs {
          FactoryNodeBuildOutputObservation::Verified(outputs) => outputs.usage = BudgetUsage { attempts: 1, ..usage },
          FactoryNodeBuildOutputObservation::ExecutionStopped { usage: stored } => *stored = usage,
        }
        return Ok(observed);
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
      let spent = self.observe_attempt(build, previous, maximum).await?.usage;
      usage = usage.checked_add(BudgetUsage { attempts: 0, ..spent }).ok_or(Invalid)?;
      current = predecessor;
    }
    Err(Invalid)
  }
  async fn observe_attempt(
    &self,
    build_id: BuildId,
    attempt_id: AttemptId,
    max_bytes: u64,
  ) -> Result<Observation, OrdinaryBuildApplicationError> {
    use OrdinaryBuildApplicationError::{Invalid, Unavailable};
    let now = self.clock.now().map_err(|_| Unavailable)?;
    // Reading retained facts grants no execution authority. After a restart,
    // settlement may outlive the signed deadline, but retention must still
    // cover the actual observation rather than a fabricated historical time.
    let observation_until =
      Timestamp::from_unix_millis(now.unix_millis().checked_add(1).ok_or(Invalid)?).map_err(|_| Invalid)?;
    let build = self.builds.build(build_id).await.map_err(classify)?;
    let attempt = self.builds.attempt(attempt_id).await.map_err(classify)?;
    if build.build.id != build_id
      || !build.state.is_terminal()
      || attempt.id != attempt_id
      || attempt.build_id != build_id
      || !attempt.state.is_terminal()
      || attempt.jobs.len() != 1
      || attempt.updated_at > now
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
    let intent = intents.next().ok_or(Invalid)?;
    let has_candidate = intent
      .input()
      .context()
      .is_some_and(|context| context.subject().candidate().is_some());
    if intents.next().is_some()
      || FactoryBuildOrigin::from_intent(intent).map_err(|_| Invalid)? != *origin
      || build.build.project_id != intent.input().subject().project_id()
      || build.build.repository_id != intent.input().subject().repository_id()
      || build.build.configuration_id != intent.profile().build_configuration.id()
      || build.build.configuration_version != intent.profile().build_configuration.version()
      || build.build.immutable_revision != *intent.input().subject().base_revision()
      || (has_candidate && self.candidate_context.is_none())
    {
      return Err(Invalid);
    }
    let role =
      if intent.selected_profile().is_some() || !self.profiles.iter().any(|profile| profile.matches_intent(intent)) {
        let retained = self
          .role
          .as_ref()
          .ok_or(Invalid)?
          .factory_role_invocation(intent.operation_id().map_err(|_| Invalid)?)
          .await
          .map_err(classify)?
          .ok_or(Invalid)?;
        let role = FactoryRoleInvocation::restore(retained).map_err(|_| Invalid)?;
        role.verify_intent(intent).map_err(|_| Invalid)?;
        Some(role)
      } else {
        None
      };
    let mut profiles = self.profiles.iter().filter(|profile| {
      profile.matches_role_intent(intent, role.as_ref().map(|role| &role.package().inputs().profile))
    });
    let profile = profiles.next().ok_or(Invalid)?;
    if profiles.next().is_some() {
      return Err(Invalid);
    }
    let capture = profile.capture.as_ref();
    let write_paths = crate::factory_build_materialization::factory_capture_source_paths(
      intent,
      capture.map(FactoryCodexCapturePolicy::instruction),
    )?;
    if let Some(capture) = capture {
      if profile.consumer != FactoryCredentialConsumer::CodingNode
        || profile.tool_risk.is_some()
        || profile.tool_response_limits.is_some()
        || intent.permissions().tools().next().is_some()
      {
        return Err(Invalid);
      }
      capture
        .validate_intent(intent, &snapshot.admitted_flow)
        .map_err(|_| Invalid)?;
      crate::factory_stored_candidate_context::verify_retained_consumer(&snapshot, intent).map_err(|_| Invalid)?;
    }
    let permissions = resolve_factory_permissions(
      &profile.project_permissions,
      snapshot.admitted_flow.limits().permissions(),
      intent.permissions(),
      &profile.local_permissions,
    )
    .map_err(|_| Invalid)?;
    if &permissions != intent.permissions() {
      return Err(Invalid);
    }
    if has_candidate
      && (profile.consumer != FactoryCredentialConsumer::ReadOnlyReview
        || profile.tool_risk.is_some()
        || permissions.tools().next().is_some())
    {
      return Err(Invalid);
    }
    let job = &attempt.jobs[0];
    let template = &job.job.job_spec_template;
    let managed = template.managed_intent().ok_or(Invalid)?;
    let terminal = job.terminal.as_ref().ok_or(Invalid)?;
    let causality = managed.factory();
    if job.attempt_id != attempt_id
      || !job.state.is_terminal()
      || job.assignment.is_none()
      || job.updated_at > now
      || terminal.state != job.state
      || terminal.completed_at > now
      || template.source_revision() != &build.build.immutable_revision
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
      || causality.source_write_paths != write_paths
      || causality.change_set_capture.as_ref() != capture.map(FactoryCodexCapturePolicy::instruction)
      || causality.change_set_materialization.is_some() != has_candidate
      || template
        .placement_policy()
        .octa
        .plugin_digests
        .get(profile.toolchain.plugin.identity().as_str())
        != Some(&profile.toolchain.plugin.digest().to_string())
    {
      return Err(Invalid);
    }
    template.validate().map_err(|_| Invalid)?;
    let retained_until = intent.node().deadline().max(observation_until);
    let candidate = if has_candidate {
      Some(
        self
          .candidate_context
          .as_ref()
          .ok_or(Invalid)?
          .read_retained_candidate_context(intent, now, retained_until)
          .await
          .map_err(|_| Unavailable)?,
      )
    } else {
      None
    };
    let selected = if has_candidate {
      None
    } else {
      Some(
        self
          .context
          .read_context(intent.input(), now, retained_until)
          .await
          .map_err(|_| Unavailable)?,
      )
    };
    let review = if let Some(policy) = &profile.reviewer {
      Some(
        self
          .review
          .as_ref()
          .ok_or(Invalid)?
          .invocation(intent.input(), &policy.evaluator, intent.node().started_at())
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
      .map(|(policy, invocation)| FactoryCodexReviewCompilation { policy, invocation });
    let mcp_configurations = crate::factory_codex_mcp::configurations(
      &profile.mcp_configurations,
      role.as_ref(),
      self.runtime_settings.as_deref(),
    )
    .await
    .map_err(|_| Invalid)?;
    let compiled = compile_codex_flow_task_with_options(
      intent,
      &snapshot.admitted_flow,
      &profile.instructions,
      &profile.credentials,
      profile.consumer,
      &profile.usage_policy,
      FactoryCodexFlowCompilationOptions {
        mcp_configurations: Some(&mcp_configurations),
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
    let materialization = candidate
      .as_ref()
      .map(|context| context.candidate().materialization(intent.input(), now))
      .transpose()?;
    let mut compiler_inputs = managed.protected_inputs().clone();
    if let Some(materialization) = &materialization {
      let (instruction, expected_inputs) = materialization.wire_inputs();
      if causality.change_set_materialization.as_ref() != Some(&instruction) {
        return Err(Invalid);
      }
      let is_source = |input: &octacity_protocol::ProtectedInputV3| {
        expected_inputs
          .iter()
          .any(|expected| expected.artifact_id == input.artifact_id)
      };
      let actual_inputs = compiler_inputs
        .inputs
        .iter()
        .filter(|input| is_source(input))
        .cloned()
        .collect::<Vec<_>>();
      if actual_inputs != expected_inputs {
        return Err(Invalid);
      }
      compiler_inputs.inputs.retain(|input| !is_source(input));
    }
    let program = compiled.managed_program(compiler_inputs).map_err(|_| Invalid)?;
    if managed.execution() != &program.execution
      || template.required_factory_enforcement() != Some(program.required_enforcement.as_slice())
    {
      return Err(Invalid);
    }
    let schema = snapshot
      .admitted_flow
      .closure()
      .definition(intent.input().definition())
      .and_then(|definition| definition.node(intent.input().node()))
      .and_then(|node| node.outcome(intent.result_outcome()))
      .and_then(|outcome| snapshot.admitted_flow.data_schema(outcome.schema()))
      .ok_or(Invalid)?;
    use crate::factory_codex_contract::{CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE};
    use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT, CHANGE_SET_PATCH_OUTPUT};
    let model_names = std::collections::BTreeSet::from([CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE]);
    let mut expected_names = model_names.clone();
    if let Some(capture) = capture {
      expected_names.extend([CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]);
      if capture.instruction().patch_max_bytes.is_some() {
        expected_names.insert(CHANGE_SET_PATCH_OUTPUT);
      }
      if let Some(observation) = &capture.instruction().observation {
        expected_names.insert(observation.name.as_str());
      }
    }
    let all = self
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id,
        attempt_id: Some(attempt_id),
        limit: u16::try_from(expected_names.len() + 1).map_err(|_| Invalid)?,
        visibility: ArtifactListVisibility::all(),
      })
      .await
      .map_err(classify)?;
    let uploads = all
      .into_iter()
      .filter(|upload| upload.artifact.identity().attempt_id == attempt_id)
      .collect::<Vec<_>>();
    let names = uploads
      .iter()
      .map(|upload| upload.artifact.identity().logical_name.as_str())
      .collect::<std::collections::BTreeSet<_>>();
    // Failed ordinary execution can still retain real model consumption before
    // capture. Missing candidate outputs never become an acceptance result.
    let failed_before_capture = job.state != JobState::Succeeded && names == model_names;
    if uploads.len() != names.len() || (names != expected_names && !failed_before_capture) {
      return Err(Unavailable);
    }
    let total = uploads
      .iter()
      .try_fold(0_u64, |total, upload| {
        total.checked_add(upload.artifact.identity().size_bytes)
      })
      .ok_or(Invalid)?;
    if total > max_bytes || total > intent.budget().max_output_bytes() {
      return Err(Invalid);
    }
    let fresh_until = uploads
      .iter()
      .filter_map(|upload| upload.artifact.identity().retention.delete_after())
      .fold(intent.node().deadline(), Timestamp::min);
    let verified_at = uploads
      .iter()
      .filter_map(|upload| upload.artifact.published_at())
      .max()
      .ok_or(Invalid)?;
    let mut documents = Vec::with_capacity(model_names.len());
    let mut capture_documents = Vec::new();
    for upload in &uploads {
      let identity = upload.artifact.identity();
      if identity.job_id != job.id() {
        return Err(Invalid);
      }
      let retained = self
        .reader
        .read(ReadFactoryArtifact {
          project_id: build.build.project_id,
          artifact: FactoryArtifactReference::new(
            identity.artifact_id,
            FactoryDigest::from_bytes(identity.digest.as_bytes()),
            identity.size_bytes,
          )
          .map_err(|_| Invalid)?,
          maximum_bytes: max_bytes,
          observed_at: now,
          required_until: fresh_until.max(observation_until),
        })
        .await
        .map_err(|_| Unavailable)?;
      let document = FactoryCodexOutputDocument {
        identity: retained.record().identity().clone(),
        bytes: retained.bytes().to_vec(),
      };
      if model_names.contains(document.identity.logical_name.as_str()) {
        documents.push(document);
      } else {
        capture_documents.push((retained.record().clone(), document.bytes));
      }
    }
    let prompt =
      std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).map_err(|_| Invalid)?;
    let contract = FactoryCodexNodeRunContract {
      model_identifier: role
        .as_ref()
        .and_then(|role| role.package().inputs().profile.definition().model_identifier.as_deref()),
      usage_policy: &profile.usage_policy,
      profile: intent.profile(),
      schema,
      prompt_digest: codex_prompt_provenance_digest(prompt),
      source_revision: materialization
        .as_ref()
        .map_or(template.source_revision(), |source| &source.candidate_revision),
      budget: intent.budget(),
      execution_window: job.created_at..=terminal.completed_at.min(intent.node().deadline()),
    };
    let observed = if let Some(review) = review {
      verify_codex_review_node_run(contract, intent, review, &compiled, &documents)
    } else {
      verify_codex_node_run(contract, &documents)
    }
    .map_err(|error| match error {
      ApplicationError::Store(StoreError::Unavailable) => Unavailable,
      _ => Invalid,
    })?;
    if candidate.is_some() || capture.is_some() {
      // Output I/O follows source recovery. A cancellation or superseded cycle
      // during either read cannot turn retained ordinary results into authority.
      let current = self.runs.factory_run_snapshot(origin.run_id).await.map_err(classify)?;
      crate::factory_stored_candidate_context::verify_retained_consumer(&current, intent).map_err(|_| Invalid)?;
      let actual_now = self.clock.now().map_err(|_| Unavailable)?.max(now);
      if candidate
        .as_ref()
        .is_some_and(|candidate| !candidate.is_retained_at(actual_now))
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
    }
    let usage = BudgetUsage {
      output_bytes: total,
      ..observed.usage
    };
    let Some(data) = observed.data else {
      return Ok(Observation {
        outputs: FactoryNodeBuildOutputObservation::ExecutionStopped {
          usage: BudgetUsage { attempts: 0, ..usage },
        },
        usage,
      });
    };
    if build.state != BuildState::Succeeded
      || attempt.state != AttemptState::Succeeded
      || job.state != JobState::Succeeded
    {
      return Ok(Observation {
        outputs: FactoryNodeBuildOutputObservation::ExecutionStopped {
          usage: BudgetUsage { attempts: 0, ..usage },
        },
        usage,
      });
    }
    let index = documents
      .iter()
      .position(|document| document.identity.logical_name.as_str() == intent.profile().result_output.as_str())
      .ok_or(Invalid)?;
    let record = uploads
      .iter()
      .find(|upload| upload.artifact.identity().artifact_id == documents[index].identity.artifact_id)
      .ok_or(Invalid)?
      .artifact
      .clone();
    let document = documents.swap_remove(index);
    let mut outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: intent.input().digest().map_err(|_| Invalid)?,
        permissions,
        budget: intent.budget(),
        deadline: intent.node().deadline(),
      },
      documents: vec![FactoryNodeBuildOutputDocument {
        record,
        bytes: document.bytes,
        data: Some(data),
        schema: schema.reference().clone(),
        tool: profile.toolchain.executable.clone(),
        plugin: profile.toolchain.plugin.clone(),
      }],
      usage,
      verified_at,
      fresh_until,
    };
    if let Some(capture) = capture {
      let mut capture_observation = None;
      let definition = snapshot
        .admitted_flow
        .closure()
        .definition(intent.input().definition())
        .ok_or(Invalid)?;
      let binding = intent.profile().binding(definition).map_err(|_| Invalid)?;
      for (record, bytes) in capture_documents {
        let evidence = binding
          .evidence
          .iter()
          .find(|evidence| evidence.kind().as_str() == record.identity().logical_name.as_str())
          .ok_or(Invalid)?;
        let data = if let Some(observation) = capture
          .instruction()
          .observation
          .as_ref()
          .filter(|observation| observation.name == record.identity().logical_name.as_str())
        {
          let identity = record.identity();
          let upload = uploads
            .iter()
            .find(|upload| upload.artifact.identity().artifact_id == identity.artifact_id)
            .ok_or(Invalid)?;
          if !matches!(&identity.artifact_type, octacity_server_artifacts::ArtifactType::Report(format)
            if format.as_str() == observation.schema.identity)
            || !matches!(
              identity.media_type.as_str(),
              "application/json" | "application/octet-stream"
            )
            || upload.producer_run_id != octacity_protocol::CHANGE_SET_CAPTURE_PRODUCER_ID
            || upload.producer_task_id != octacity_protocol::CHANGE_SET_CAPTURE_PRODUCER_ID
          {
            return Err(Invalid);
          }
          let schema = snapshot.admitted_flow.data_schema(evidence.schema()).ok_or(Invalid)?;
          let facts: octacity_protocol::CapturedChangeSetObservationV1 =
            serde_json::from_slice(&bytes).map_err(|_| Invalid)?;
          let data = FlowVerifiedBuildData::new(schema.clone(), serde_json::to_value(&facts).map_err(|_| Invalid)?)
            .map_err(|_| Invalid)?;
          capture_observation = Some(facts);
          Some(data)
        } else {
          None
        };
        outputs.documents.push(FactoryNodeBuildOutputDocument {
          record,
          bytes,
          data,
          schema: evidence.schema().clone(),
          tool: capture.tool().clone(),
          plugin: profile.toolchain.plugin.clone(),
        });
      }
      let execution = FlowBuildExecution::new(intent, build_id, attempt_id, vec![job.id()]).map_err(|_| Invalid)?;
      let (candidate, manifest) = crate::factory_node_capture::verify_factory_node_change_set_manifest(
        intent,
        &execution,
        &outputs,
        capture.tool(),
        now,
      )
      .map_err(|_| Invalid)?;
      if let Some(facts) = capture_observation {
        let input_digest = intent.input().digest().map_err(|_| Invalid)?.to_string();
        if facts.candidate_revision != candidate.subject().candidate_revision().as_str()
          || facts.candidate_digest != candidate.subject().changeset_digest().to_string()
          || facts.input_digest != input_digest
          || facts.fresh_until != intent.node().deadline().unix_millis()
          || facts.fresh_until > fresh_until.unix_millis()
          || facts.fresh_until <= verified_at.unix_millis()
        {
          return Err(Invalid);
        }
      }
      let policy = &capture.instruction().policy;
      if (manifest.changed_paths.is_empty() && !policy.allow_empty)
        || manifest.changed_paths.len() > policy.max_changed_paths as usize
      {
        return Err(Invalid);
      }
    }
    Ok(Observation {
      outputs: FactoryNodeBuildOutputObservation::Verified(Box::new(outputs)),
      usage,
    })
  }
}
#[async_trait]
impl<R: FactoryRunStore, Q: BuildQueryStore, A: ArtifactRecordStore, B: ArtifactByteReader> FactoryNodeBuildOutputSource
  for FactoryCodexBuildOutputSource<R, Q, A, B>
{
  async fn published_outputs(
    &self,
    build: BuildId,
    attempt: AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError> {
    Ok(self.observe_history(build, attempt, max_bytes).await?.outputs)
  }
  async fn terminal_usage(
    &self,
    build: BuildId,
    attempt: AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
    let usage = self
      .observe_history(build, attempt, octacity_artifact_store::MAX_ARTIFACT_READ_BYTES)
      .await?
      .usage;
    Ok(BudgetUsage { attempts: 0, ..usage })
  }
}
fn classify(error: StoreError) -> OrdinaryBuildApplicationError {
  match error {
    StoreError::Unavailable => OrdinaryBuildApplicationError::Unavailable,
    _ => OrdinaryBuildApplicationError::Invalid,
  }
}
