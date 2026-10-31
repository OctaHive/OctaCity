//! Deterministic ordinary Build DAG assembly for installed managed task compilers.
use crate::{
  BuildExecutionContext, FactoryBuildPolicyLayers, FactoryManagedJobSpecInput, OrdinaryBuildApplicationError,
  derive_factory_managed_job_spec_template,
};
use octacity_protocol::{
  ChangeSetCaptureV3, FactoryCausalityV3, FactoryEnforcementCapabilityV3, ManagedOctaExecutionV3,
  ProtectedInputManifestV3,
};
use octacity_server_artifacts::ArtifactRecord;
use octacity_server_domain::{AttemptId, BuildId, JobId, PipelineNodeId, Timestamp};
use octacity_server_factory::{ChangeSet, FactoryClaimOwnership, FactoryDigest, FlowBuildIntent, FlowNodeInput};
use octacity_server_job::JobSpecBuildSnapshot;
use octacity_server_store::{
  AcceptFactoryBuild, BuildRetentionDeadlines, ImmutableBuildInput, MaterializedJob, MaterializedJobPayload,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// One server-generated task program for an explicitly selected Pipeline node.
/// Protected inputs must already be published by the trusted input publisher.
#[derive(Clone)]
pub struct FactoryManagedJobProgram {
  /// Generated Octafile and finite task selection.
  pub execution: ManagedOctaExecutionV3,
  /// Complete protected inputs with immutable content identities.
  pub protected_inputs: ProtectedInputManifestV3,
  /// Qualified enforcement required by this installed capability.
  pub required_enforcement: Vec<FactoryEnforcementCapabilityV3>,
}
/// Frozen inputs to the common ordinary Build materialization boundary.
#[derive(Clone)]
pub struct FactoryManagedBuildInput {
  /// Exact server-owned Build Configuration closure and effective Project policy.
  pub context: BuildExecutionContext,
  /// Already persisted common node operation.
  pub intent: FlowBuildIntent,
  /// Current dispatch authority, independently checked by atomic acceptance.
  pub ownership: FactoryClaimOwnership,
  /// Installed task compiler's bounded ordinary Build parameters.
  pub parameters: BTreeMap<String, serde_json::Value>,
  /// Complete explicit program assignment to the published Pipeline DAG.
  pub programs: BTreeMap<PipelineNodeId, FactoryManagedJobProgram>,
  /// Immutable four-layer permission ceilings used again at signing.
  pub policy_layers: FactoryBuildPolicyLayers,
  /// Trusted preparation time; the caller rereads time before acceptance.
  pub accepted_at: Timestamp,
}

/// Verified retained candidate selected by one complete frozen node input.
/// Construction checks trusted Artifact records, never provider observations.
/// The records remain private so callers cannot replace their content identities.
#[derive(Clone)]
pub struct FactoryManagedCandidateProof {
  input_digest: FactoryDigest,
  candidate: ChangeSet,
  bundle: ArtifactRecord,
  manifest: ArtifactRecord,
}
impl FactoryManagedCandidateProof {
  /// Verifies explicit context selection, publication, provenance and retention.
  /// Records must come from the authoritative Artifact store. Actual downloaded
  /// bytes are subsequently checked against these signed protected identities.
  pub fn verify(
    input: &FlowNodeInput,
    candidate: ChangeSet,
    bundle: ArtifactRecord,
    manifest: ArtifactRecord,
    observed_at: Timestamp,
  ) -> Result<Self, crate::FactoryChangeSetError> {
    crate::factory_node_candidate_materialization(input, &candidate, &bundle, &manifest, observed_at)?;
    Ok(Self {
      input_digest: input
        .digest()
        .map_err(|_| crate::FactoryChangeSetError::InvalidEvidence)?,
      candidate,
      bundle,
      manifest,
    })
  }
  pub(crate) const fn changeset(&self) -> &ChangeSet {
    &self.candidate
  }
  pub(crate) fn materialization(
    &self,
    input: &FlowNodeInput,
    accepted_at: Timestamp,
  ) -> Result<crate::FactoryCandidateMaterialization, OrdinaryBuildApplicationError> {
    if input.digest().map_err(|_| OrdinaryBuildApplicationError::Invalid)? != self.input_digest {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    // A proof observed earlier must not bypass retention at actual acceptance.
    crate::factory_node_candidate_materialization(input, &self.candidate, &self.bundle, &self.manifest, accepted_at)
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)
  }
}

/// Explicit source operations supplied by an installed task compiler.
/// The default preserves ordinary readonly assembly and its stable identities.
#[derive(Clone, Default)]
pub struct FactoryManagedSourcePlan {
  /// Exact operator-installed builtin command authority; absent for external producers.
  pub command: Option<crate::FactoryManagedCommandProof>,
  /// Retained candidate verified for this exact frozen input.
  pub candidate: Option<FactoryManagedCandidateProof>,
  /// Trusted capture instruction; its authority comes from the frozen profile.
  pub capture: Option<ChangeSetCaptureV3>,
}

/// Checks the same frozen capture scope before publication and ordinary assembly.
pub(crate) fn factory_capture_source_paths(
  intent: &FlowBuildIntent,
  capture: Option<&ChangeSetCaptureV3>,
) -> Result<Option<Vec<String>>, OrdinaryBuildApplicationError> {
  use OrdinaryBuildApplicationError::Invalid;
  let profile = intent.profile();
  if profile.capture_write_paths.is_some() != capture.is_some()
    || (profile.capture_deliverables.is_some() && capture.is_none())
  {
    return Err(Invalid);
  }
  let Some(capture) = capture else { return Ok(None) };
  capture.validate().map_err(|_| Invalid)?;
  let paths: Vec<_> = profile
    .capture_write_paths
    .as_ref()
    .ok_or(Invalid)?
    .iter()
    .map(|path| path.as_str().to_owned())
    .collect();
  if capture.policy.allowed_path_prefixes != paths {
    return Err(Invalid);
  }
  if let Some(deliverables) = &profile.capture_deliverables {
    // Concrete output verification later enforces these exact frozen deliverables.
    deliverables.freeze(intent.input()).map_err(|_| Invalid)?;
  }
  Ok(Some(paths))
}

/// Compiles complete managed Job templates with stable ordinary identities.
/// This pure boundary does not create a Trigger, dispatch Jobs or commit the Run.
pub fn materialize_factory_managed_build(
  input: FactoryManagedBuildInput,
) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError> {
  materialize_factory_managed_build_with_source(input, FactoryManagedSourcePlan::default())
}

/// Compiles the published DAG with an explicitly verified source plan.
/// Checkout uses the selected node resource's exact base, independently of Work
/// correlation, and is absent for explicit no-code input. Candidate reconstruction
/// is signed separately and cannot be inferred from a mutable lifecycle pointer.
pub fn materialize_factory_managed_build_with_source(
  input: FactoryManagedBuildInput,
  source: FactoryManagedSourcePlan,
) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError> {
  use OrdinaryBuildApplicationError::Invalid;
  let context = &input.context;
  let repository = context.repository.as_ref();
  let configuration = &context.configuration;
  let definition = &configuration.definition;
  let work_subject = input.intent.input().subject();
  let project_id = work_subject.project_id();
  let resource = input.intent.input().exact_resource().ok();
  match (resource, repository, definition.source) {
    (Some(subject), Some(repository), None)
      if repository.id == subject.repository_id()
        && repository.project_id == project_id
        && Some(repository.id) == definition.repository_id
        && Some(repository.version) == definition.repository_version
        && repository.definition.selection.allow_exact_revision => {}
    (None, None, Some(octacity_server_domain::BuildSourceMode::SourceFreeV1))
      if matches!(
        input.intent.input().code(),
        Some(octacity_server_factory::FlowNodeCode::None)
      ) => {}
    _ => return Err(Invalid),
  }
  let profile = input.intent.profile();
  let permissions = input.intent.permissions();
  let selected = &input.intent.profile().build_configuration;
  let builtin_command = source
    .command
    .as_ref()
    .is_some_and(|proof| proof.verify(&input.intent, &input.programs));
  if source.command.is_some() && !builtin_command {
    return Err(Invalid);
  }
  if !definition.enabled
    || configuration.id != selected.id()
    || configuration.version != selected.version()
    || configuration.project_id != project_id
    || selected.project_id() != project_id
    || context.pipeline.id != definition.pipeline_id
    || context.pipeline.version != definition.pipeline_version
    || context.pipeline.project_id != project_id
    || context
      .effective_policy
      .sources
      .last()
      .is_none_or(|source| source.project_id != project_id)
    || input
      .intent
      .input()
      .context()
      .is_some_and(|context| context.subject().candidate().is_some())
      != source.candidate.is_some()
    || context.pipeline.dag.nodes().len() != input.programs.len()
    || context.pipeline.dag.nodes().len() > octacity_server_store::MAX_FACTORY_BUILD_JOBS
    || !permissions.plugins().any(|allowed| allowed == &profile.plugin)
    || (!builtin_command && !permissions.executables().any(|allowed| allowed == &profile.tool))
    || context
      .job_spec_toolchain
      .octa
      .plugin_digests
      .get(profile.plugin.identity().as_str())
      != Some(&profile.plugin.digest().to_string())
  {
    return Err(Invalid);
  }
  definition.validate().map_err(|_| Invalid)?;
  let source_write_paths = factory_capture_source_paths(&input.intent, source.capture.as_ref())?;
  let candidate = source
    .candidate
    .as_ref()
    .map(|proof| proof.materialization(input.intent.input(), input.accepted_at))
    .transpose()?;
  let (materialization, candidate_inputs) = candidate.map_or((None, Vec::new()), |candidate| {
    let (instruction, protected) = candidate.wire_inputs();
    (Some(instruction), protected)
  });
  crate::build_configuration_policy::validate_configuration_policy(context).map_err(|_| Invalid)?;
  let parameters = definition.parameters.resolve(&input.parameters).map_err(|_| Invalid)?;
  let allowed_pools: Vec<_> = definition
    .allowed_pools
    .intersection(&context.effective_policy.policy.pools)
    .copied()
    .collect();
  if allowed_pools.is_empty() {
    return Err(Invalid);
  }
  let operation = input.intent.operation_id().map_err(|_| Invalid)?;
  let namespace = Uuid::new_v5(&Uuid::NAMESPACE_OID, &operation.as_bytes());
  let build_id = BuildId::from_uuid(Uuid::new_v5(&namespace, b"ordinary-build:v1")).map_err(|_| Invalid)?;
  let attempt_id = AttemptId::from_uuid(Uuid::new_v5(&build_id.as_uuid(), b"attempt:1")).map_err(|_| Invalid)?;
  let identities: BTreeMap<_, _> = context
    .pipeline
    .dag
    .nodes()
    .iter()
    .map(|node| {
      JobId::from_uuid(Uuid::new_v5(&attempt_id.as_uuid(), node.id().as_str().as_bytes()))
        .map(|id| (node.id().clone(), id))
        .map_err(|_| Invalid)
    })
    .collect::<Result<_, _>>()?;
  let mut signing_context = context.clone();
  signing_context
    .job_spec_toolchain
    .octa
    .plugin_digests
    .retain(|identity, digest| {
      permissions
        .plugins()
        .any(|plugin| plugin.identity().as_str() == identity && &plugin.digest().to_string() == digest)
    });
  // Reserve only the output capacity actually authorized for this task. The
  // published Build Configuration is a ceiling shared by multiple producers.
  let requested = permissions.outputs();
  let outputs = &mut signing_context.configuration.definition.artifacts;
  outputs.artifact_count = outputs.artifact_count.min(requested.max_artifact_count());
  outputs.artifact_bytes = outputs.artifact_bytes.min(requested.max_artifact_bytes());
  outputs.report_count = outputs.report_count.min(requested.max_report_count());
  outputs.report_bytes = outputs.report_bytes.min(requested.max_report_bytes());
  let single_ceiling = match (outputs.artifact_count > 0, outputs.report_count > 0) {
    (true, true) => outputs.artifact_bytes.min(outputs.report_bytes),
    (true, false) => outputs.artifact_bytes,
    (false, true) => outputs.report_bytes,
    (false, false) => 0,
  };
  outputs.single_output_bytes = outputs.single_output_bytes.min(single_ceiling);
  let policy =
    crate::build_configuration_job_policy::configuration_job_policy(&signing_context, None).map_err(|_| Invalid)?;
  let build_snapshot = match (resource, repository) {
    (Some(subject), Some(repository)) => JobSpecBuildSnapshot::new(
      build_id,
      subject.base_revision().clone(),
      None,
      repository.definition.repository_locator.clone(),
      BTreeMap::new(),
    )
    .map_err(|_| Invalid)?,
    (None, None) => JobSpecBuildSnapshot::source_free(build_id, BTreeMap::new()),
    _ => return Err(Invalid),
  };
  let source_selection: octacity_server_domain::BuildSourceSelection = match resource {
    Some(subject) => crate::ManualSourceSelection::ExactRevision(subject.base_revision().clone()).into(),
    None => octacity_server_domain::BuildSourceMode::SourceFreeV1.into(),
  };
  let frozen = input.intent.input();
  let causality = FactoryCausalityV3 {
    factory_run_id: frozen.factory_run_id().to_string(),
    factory_configuration_id: frozen.configuration().id().to_string(),
    factory_configuration_version: frozen.configuration().version().get(),
    flow_run_id: frozen.flow_run_id().to_string(),
    workflow_cycle_id: frozen.cycle_id().to_string(),
    node_attempt_id: input.intent.node().id().to_string(),
    input_digest: frozen.digest().map_err(|_| Invalid)?.to_string(),
    subject_digest: work_subject.digest().to_string(),
    source_write_paths,
    change_set_capture: source.capture,
    change_set_materialization: materialization,
  };
  let mut jobs: Vec<MaterializedJob> = context
    .pipeline
    .dag
    .nodes()
    .iter()
    .map(|node| {
      let program = input.programs.get(node.id()).ok_or(Invalid)?;
      program.protected_inputs.validate().map_err(|_| Invalid)?;
      let mut protected_inputs = program.protected_inputs.clone();
      for input in &candidate_inputs {
        if !protected_inputs.inputs.contains(input) {
          protected_inputs.inputs.push(input.clone());
        }
      }
      protected_inputs
        .inputs
        .sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
      protected_inputs.validate().map_err(|_| Invalid)?;
      let template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
        build: build_snapshot.clone(),
        pipeline_node_id: node.id().clone(),
        execution: program.execution.clone(),
        causality: causality.clone(),
        protected_inputs,
        policy_layers: input.policy_layers.clone(),
        required_enforcement: program.required_enforcement.clone(),
        policy: policy.clone(),
      })
      .map_err(|_| Invalid)?
      .with_execution_deadline(input.intent.node().deadline())
      .map_err(|_| Invalid)?;
      let requirements = crate::build_configuration_job_policy::configuration_job_requirements(definition, node);
      let dependencies = context
        .pipeline
        .dag
        .edges()
        .iter()
        .filter(|edge| edge.dependent() == node.id())
        .map(|edge| identities.get(edge.predecessor()).copied().ok_or(Invalid))
        .collect::<Result<_, _>>()?;
      MaterializedJob::new(
        identities[node.id()],
        node.id().clone(),
        dependencies,
        node.dependency_policy(),
        allowed_pools.clone(),
        MaterializedJobPayload::new(requirements, template).map_err(|_| Invalid)?,
      )
      .map_err(|_| Invalid)
    })
    .collect::<Result<_, _>>()?;
  jobs.sort_unstable_by_key(|job| job.id);
  let deadline = |seconds: u64| -> Result<Timestamp, OrdinaryBuildApplicationError> {
    i64::try_from(seconds)
      .ok()
      .and_then(|value| value.checked_mul(1000))
      .and_then(|value| input.accepted_at.unix_millis().checked_add(value))
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or(Invalid)
  };
  let retention = context.effective_policy.policy.retention;
  let request = AcceptFactoryBuild {
    intent: input.intent.clone(),
    ownership: input.ownership,
    build: ImmutableBuildInput {
      id: build_id,
      project_id,
      configuration_id: configuration.id,
      configuration_version: configuration.version,
      pipeline_id: context.pipeline.id,
      pipeline_version: context.pipeline.version,
      repository_id: repository.map(|repository| repository.id),
      repository_version: repository.map(|repository| repository.version),
      immutable_revision: resource.map(|subject| subject.base_revision().clone()),
      source: source_selection.mode(),
      input_snapshot: serde_json::to_value(crate::snapshots::BuildInputSnapshot {
        parameters,
        source: source_selection,
      })
      .map_err(|_| Invalid)?,
      effective_policy_snapshot: serde_json::to_value(crate::snapshots::BuildPolicySnapshot {
        project: context.effective_policy.clone(),
        job_spec_toolchain: context.job_spec_toolchain.clone(),
      })
      .map_err(|_| Invalid)?,
      retention: BuildRetentionDeadlines {
        metadata: deadline(retention.build_seconds)?,
        logs: deadline(retention.log_seconds)?,
        artifacts: deadline(retention.artifact_seconds)?,
        reports: deadline(retention.artifact_seconds)?,
      },
      project_job_concurrency_limit: context.effective_policy.policy.concurrency.active_jobs,
      priority: input.intent.priority(),
    },
    attempt_id,
    jobs,
    accepted_at: input.accepted_at,
  };
  request.validate_fresh().map_err(|_| Invalid)?;
  Ok(request)
}
