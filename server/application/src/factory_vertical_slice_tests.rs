use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  sync::{Arc, Mutex},
};

use async_trait::async_trait;
use octacity_protocol::{
  CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, CHANGE_SET_MANIFEST_OUTPUT,
  CapturedChangeSetFileV1, CapturedChangeSetManifestV1, FactoryImmutableReferenceV3, PlatformArchitecture, PlatformOs,
};
use octacity_server_artifacts::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactRetentionPolicy, ArtifactType,
};
use octacity_server_domain::{
  ArtifactId, ArtifactName, AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, JobId, JobVersion,
  LeaseId, PipelineId, PipelineNodeId, PipelineVersion, PoolId, RepositoryVersion, RuntimeClass, TriggerId,
  TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_factory::{
  Assessment, AssessmentId, AssessmentInput, AssessmentOutcome, BoundedSummary, BudgetLimit, CriterionPack,
  DecisionEngineInput, DecisionId, DecisionOutcome, DecisionPolicy, DecisionPolicyDefinition, DecisionPolicyVersion,
  DeliveryIntent, DeterministicGateOutcome, EvaluationBranch, EvaluationBranchState, EvaluationPlanDefinition,
  EvaluationPlanId, EvaluationPolicy, EvaluationProgress, EvaluationState, EvidenceItem, EvidenceItemInput,
  EvidenceManifest, EvidenceManifestId, EvidenceOutputKind, EvidenceProducer, EvidenceRequirement,
  FactoryArtifactReference, FactoryDigest, FactoryLifecycleProgress, FactoryOutputPermissions, FactoryPermissionDraft,
  FactoryPermissionSet, FactoryResourceLimits, FactoryRun, FactoryRunState, FactoryRunVersion, FactorySafeText,
  FactoryStageProgress, FactoryStageTarget, FactoryTaskSubject, FindingSeverity, ImmutableReference,
  IndeterminatePolicy, LocalPermissionCeiling, ReviewBranch, ReviewEvaluatorCapability, ReviewPlanPreparation,
  ReviewPurpose, evaluate_decision, prepare_evaluation_plan,
};
use octacity_server_job::JobFailureClass;
use octacity_server_job::JobRequirements;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_store::{
  ApplyFactoryRunControl, AttemptRecord, AuditActor, AuditActorKind, BuildQueryStore, BuildRecord,
  BuildRetentionDeadlines, ClaimFactoryOutbox, ClaimFactoryRuns, ClaimedFactoryOutbox, CommitFactoryRunTransition,
  FactoryAuditFact, FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryOutboxSettlement, FactoryRunControlIntent,
  FactoryRunControlStore, FactoryRunHistoryAppend, FactoryRunStore, IdempotencyKey, ImmutableBuildInput,
  ManagementMutation, ManagementSecurityScope, MutationAuditContext, NormalizedTriggerOccurrence, SettleFactoryOutbox,
  StoreError, TriggerCause, TriggerDefinitionRef, TriggerMetadata, TriggerOccurrenceState, TriggerTarget,
};

use crate::factory_build_bridge::FactoryBuildClock;
use crate::{
  CreateFactoryBuild, FactoryAdmissionHandlers, FactoryBuildAcceptance, FactoryBuildBridge, FactoryBuildOutputSource,
  FactoryBuildPolicyLayers, FactoryBuildPolicyRequest, FactoryBuildPolicySource, FactoryBuildPolicySourceError,
  FactoryReconciliationShutdown, MutationDisposition, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
  factory_admission_tests::{RecordingResolver, admit, command, repository, seeded_store, time},
  factory_reconciliation_tests::{key, reconciler, run_to_completion},
};

struct FixedBuildClock;

impl FactoryBuildClock for FixedBuildClock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::FactoryBuildBridgeError> {
    Ok(time(11))
  }
}

#[derive(Clone)]
struct AcceptedBuild {
  request: CreateFactoryBuild,
  attempt_id: AttemptId,
  job_ids: Vec<JobId>,
  outputs: Vec<ArtifactIdentity>,
  documents: Vec<crate::VerifiedFactoryArtifact>,
  state: BuildState,
}

#[derive(Clone, Copy, Default)]
enum BuildFixtureOutcome {
  #[default]
  Succeeded,
  PartialUploadFailure,
}

#[derive(Default)]
struct SucceedingBuildApplication {
  builds: Mutex<BTreeMap<BuildId, AcceptedBuild>>,
  outcomes: Mutex<VecDeque<BuildFixtureOutcome>>,
  requests: Mutex<Vec<CreateFactoryBuild>>,
}

impl SucceedingBuildApplication {
  fn count(&self) -> usize {
    self.builds.lock().unwrap().len()
  }

  fn fail_next_after_partial_upload(&self) {
    self
      .outcomes
      .lock()
      .unwrap()
      .push_back(BuildFixtureOutcome::PartialUploadFailure);
  }

  fn requests(&self) -> Vec<CreateFactoryBuild> {
    self.requests.lock().unwrap().clone()
  }

  fn accepted(&self, build_id: BuildId) -> Result<AcceptedBuild, StoreError> {
    self
      .builds
      .lock()
      .unwrap()
      .get(&build_id)
      .cloned()
      .ok_or(StoreError::Unavailable)
  }

  fn documents(&self, build_id: BuildId) -> Vec<crate::VerifiedFactoryArtifact> {
    self.accepted(build_id).unwrap().documents
  }

  fn tampered_documents(&self, build_id: BuildId) -> Vec<crate::VerifiedFactoryArtifact> {
    let accepted = self.accepted(build_id).unwrap();
    capture_outputs_with_candidate(
      &accepted.request,
      build_id,
      accepted.attempt_id,
      accepted.job_ids[0],
      "tampered-candidate",
    )
    .1
  }
}

#[async_trait]
impl OrdinaryBuildApplication for SucceedingBuildApplication {
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    let build_id = BuildId::generate();
    let attempt_id = AttemptId::generate();
    let job_ids = vec![JobId::generate()];
    let (mut outputs, mut documents) = capture_outputs(&request, build_id, attempt_id, job_ids[0]);
    let outcome = self.outcomes.lock().unwrap().pop_front().unwrap_or_default();
    let state = match outcome {
      BuildFixtureOutcome::Succeeded => BuildState::Succeeded,
      BuildFixtureOutcome::PartialUploadFailure => {
        outputs.truncate(1);
        documents.truncate(1);
        BuildState::Failed
      }
    };
    self.requests.lock().unwrap().push(request.clone());
    self.builds.lock().unwrap().insert(
      build_id,
      AcceptedBuild {
        request: request.clone(),
        attempt_id,
        job_ids: job_ids.clone(),
        outputs,
        documents,
        state,
      },
    );
    Ok(FactoryBuildAcceptance {
      build_id,
      attempt_id,
      job_ids,
      effective_policy_digest: request.effective_permissions.digest(),
    })
  }
}

#[async_trait]
impl FactoryBuildOutputSource for SucceedingBuildApplication {
  async fn published_outputs(
    &self,
    build_id: BuildId,
    attempt_id: AttemptId,
  ) -> Result<Vec<octacity_server_artifacts::ArtifactIdentity>, OrdinaryBuildApplicationError> {
    let accepted = self
      .accepted(build_id)
      .map_err(|_| OrdinaryBuildApplicationError::Unavailable)?;
    if accepted.attempt_id != attempt_id {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(accepted.outputs)
  }
}

#[async_trait]
impl BuildQueryStore for SucceedingBuildApplication {
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError> {
    let accepted = self.accepted(build_id)?;
    let trigger = NormalizedTriggerOccurrence::root(
      TriggerOccurrenceId::generate(),
      TriggerDefinitionRef {
        id: TriggerId::generate(),
        version: TriggerVersion::INITIAL,
      },
      TriggerTarget {
        configuration_id: accepted.request.build_configuration.id(),
        configuration_version: accepted.request.build_configuration.version(),
      },
      TriggerIdentity::new("factory-vertical-slice").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      time(11),
    )
    .unwrap();
    Ok(BuildRecord {
      build: ImmutableBuildInput {
        id: build_id,
        project_id: accepted.request.project_id,
        configuration_id: accepted.request.build_configuration.id(),
        configuration_version: accepted.request.build_configuration.version(),
        pipeline_id: PipelineId::generate(),
        pipeline_version: PipelineVersion::INITIAL,
        repository_id: accepted.request.repository_id,
        repository_version: RepositoryVersion::INITIAL,
        immutable_revision: accepted.request.immutable_revision,
        input_snapshot: serde_json::json!({}),
        effective_policy_snapshot: serde_json::json!({}),
        retention: BuildRetentionDeadlines {
          metadata: time(1_000),
          logs: time(1_000),
          artifacts: time(1_000),
          reports: time(1_000),
        },
        project_job_concurrency_limit: 1,
        priority: accepted.request.priority,
      },
      state: accepted.state,
      version: BuildVersion::INITIAL,
      trigger,
      trigger_state: TriggerOccurrenceState::Accepted,
      trigger_created_at: time(11),
      trigger_updated_at: time(11),
      created_at: time(11),
      updated_at: time(11),
    })
  }

  async fn attempt(&self, attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
    let build_id = self
      .builds
      .lock()
      .unwrap()
      .iter()
      .find_map(|(build_id, accepted)| (accepted.attempt_id == attempt_id).then_some(*build_id))
      .ok_or(StoreError::Unavailable)?;
    self.latest_attempt(build_id).await
  }

  async fn job(&self, _job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn latest_attempt(&self, build_id: BuildId) -> Result<AttemptRecord, StoreError> {
    let accepted = self.accepted(build_id)?;
    Ok(AttemptRecord {
      id: accepted.attempt_id,
      build_id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state: match accepted.state {
        BuildState::Succeeded => AttemptState::Succeeded,
        BuildState::Failed => AttemptState::Failed,
        _ => unreachable!("fixture creates only terminal Builds"),
      },
      version: AttemptVersion::INITIAL,
      created_at: time(11),
      updated_at: time(11),
      jobs: accepted
        .job_ids
        .iter()
        .map(|job_id| octacity_server_store::JobRecord {
          attempt_id: accepted.attempt_id,
          job: octacity_server_store::MaterializedJob::new(
            *job_id,
            PipelineNodeId::new("factory").unwrap(),
            Vec::new(),
            DependencyPolicy::AllSucceeded,
            vec![PoolId::generate()],
            octacity_server_store::MaterializedJobPayload::new(
              JobRequirements {
                capabilities: BTreeSet::new(),
                labels: BTreeMap::new(),
                minimum_cpu_millis: 0,
                minimum_memory_bytes: 0,
                minimum_disk_bytes: 0,
                runtime_class: RuntimeClass::Native,
                operating_system: PlatformOs::Linux,
                architecture: PlatformArchitecture::Amd64,
                host_platform: None,
                required_guarantees: BTreeSet::new(),
              },
              octacity_server_store::testing::job_spec_template(build_id, "factory"),
            )
            .unwrap(),
          )
          .unwrap(),
          state: match accepted.state {
            BuildState::Succeeded => octacity_server_job::JobState::Succeeded,
            BuildState::Failed => octacity_server_job::JobState::Failed,
            _ => unreachable!("fixture creates only terminal Builds"),
          },
          version: JobVersion::INITIAL,
          created_at: time(11),
          updated_at: time(11),
          queue: None,
          assignment: None,
          terminal: Some(octacity_server_store::JobTerminalRecord {
            state: match accepted.state {
              BuildState::Succeeded => octacity_server_job::JobState::Succeeded,
              BuildState::Failed => octacity_server_job::JobState::Failed,
              _ => unreachable!("fixture creates only terminal Builds"),
            },
            failure_class: (accepted.state == BuildState::Failed).then_some(JobFailureClass::Infrastructure),
            timed_out: false,
            completed_at: time(11),
          }),
          event_cursor: 1,
        })
        .collect(),
    })
  }
}

fn capture_outputs(
  request: &CreateFactoryBuild,
  build_id: BuildId,
  attempt_id: AttemptId,
  job_id: JobId,
) -> (Vec<ArtifactIdentity>, Vec<crate::VerifiedFactoryArtifact>) {
  capture_outputs_with_candidate(request, build_id, attempt_id, job_id, "candidate-revision")
}

fn capture_outputs_with_candidate(
  request: &CreateFactoryBuild,
  build_id: BuildId,
  attempt_id: AttemptId,
  job_id: JobId,
  candidate_revision: &str,
) -> (Vec<ArtifactIdentity>, Vec<crate::VerifiedFactoryArtifact>) {
  if !matches!(
    request.causality.target,
    FactoryStageTarget::Implementation | FactoryStageTarget::Rework
  ) {
    return (Vec::new(), Vec::new());
  }
  let bundle_bytes = b"fixture git bundle\n".to_vec();
  let bundle_digest = FactoryDigest::content_sha256(&bundle_bytes);
  let bundle_file = CapturedChangeSetFileV1 {
    name: CHANGE_SET_BUNDLE_OUTPUT.to_owned(),
    size_bytes: bundle_bytes.len() as u64,
    sha256: bundle_digest.to_string(),
  };
  let capture_base_revision = request
    .causality
    .candidate
    .as_ref()
    .map_or(request.immutable_revision.as_str(), |candidate| {
      candidate.candidate_revision.as_str()
    });
  let manifest = CapturedChangeSetManifestV1 {
    format_version: 1,
    base_revision: capture_base_revision.to_owned(),
    candidate_revision: candidate_revision.to_owned(),
    stage_attempt_id: request.causality.stage_attempt_id.to_string(),
    capture_tool: FactoryImmutableReferenceV3 {
      identity: "git".to_owned(),
      version: "2.0.0".to_owned(),
      sha256: "11".repeat(32),
    },
    changed_paths: Vec::new(),
    bundle: bundle_file,
    patch: None,
  };
  let mut manifest_bytes = serde_json::to_vec(&manifest).unwrap();
  manifest_bytes.push(b'\n');
  let lease_id = LeaseId::generate();
  let outputs = vec![
    artifact_identity(
      build_id,
      attempt_id,
      job_id,
      lease_id,
      CHANGE_SET_BUNDLE_OUTPUT,
      CHANGE_SET_BUNDLE_MEDIA_TYPE,
      &bundle_bytes,
    ),
    artifact_identity(
      build_id,
      attempt_id,
      job_id,
      lease_id,
      CHANGE_SET_MANIFEST_OUTPUT,
      CHANGE_SET_MANIFEST_MEDIA_TYPE,
      &manifest_bytes,
    ),
  ];
  let documents = outputs
    .iter()
    .cloned()
    .zip([bundle_bytes, manifest_bytes])
    .map(|(identity, bytes)| crate::VerifiedFactoryArtifact::new(identity, bytes).unwrap())
    .collect();
  (outputs, documents)
}

fn artifact_identity(
  build_id: BuildId,
  attempt_id: AttemptId,
  job_id: JobId,
  lease_id: LeaseId,
  name: &str,
  media_type: &str,
  bytes: &[u8],
) -> ArtifactIdentity {
  ArtifactIdentity {
    artifact_id: ArtifactId::generate(),
    build_id,
    attempt_id,
    job_id,
    lease_id,
    logical_name: ArtifactName::new(name).unwrap(),
    artifact_type: ArtifactType::Artifact,
    media_type: ArtifactMediaType::new(media_type).unwrap(),
    size_bytes: bytes.len() as u64,
    digest: ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(bytes).as_bytes()),
    retention: ArtifactRetentionPolicy::Keep,
  }
}

struct FixedPolicies;

#[async_trait]
impl FactoryBuildPolicySource for FixedPolicies {
  async fn policy_layers(
    &self,
    _request: FactoryBuildPolicyRequest,
  ) -> Result<FactoryBuildPolicyLayers, FactoryBuildPolicySourceError> {
    let permissions = permission_set();
    Ok(FactoryBuildPolicyLayers {
      project: permissions.clone(),
      configuration: permissions.clone(),
      task: permissions.clone(),
      local: LocalPermissionCeiling::fully_enforced(permissions),
    })
  }
}

fn permission_set() -> FactoryPermissionSet {
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    resources: FactoryResourceLimits::new(100, 100, 100, 10, 100).unwrap(),
    outputs: FactoryOutputPermissions::try_new(Vec::new(), 10, 100, 10, 100).unwrap(),
    ..FactoryPermissionDraft::default()
  })
  .unwrap()
}

fn retry_infrastructure(
  run_id: octacity_server_factory::FactoryRunId,
  version: FactoryRunVersion,
  stage_attempt_id: octacity_server_factory::StageAttemptId,
  identity: &str,
) -> ManagementMutation<ApplyFactoryRunControl> {
  ManagementMutation::new(
    ApplyFactoryRunControl {
      run_id,
      expected_version: version,
      idempotency_key: IdempotencyKey::new(identity).unwrap(),
      intent: FactoryRunControlIntent::RetryInfrastructure { stage_attempt_id },
      requested_at: time(11),
    },
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some("operator-1".to_owned()),
      },
      ManagementSecurityScope::trusted_network(),
      identity,
    )
    .unwrap(),
  )
}

#[test]
fn partial_capture_retry_restarts_from_the_exact_base_and_accepted_candidate_survives_restart() {
  run_to_completion(async {
    let project_id = octacity_server_domain::ProjectId::generate();
    let configuration_id = octacity_server_factory::FactoryConfigurationId::generate();
    let repository_id = octacity_server_domain::RepositoryId::generate();
    let main = octacity_server_domain::SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let admission = FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default()));
    let admitted = admit(&admission, command(project_id, configuration_id, repository_id))
      .await
      .unwrap();
    let builds = Arc::new(SucceedingBuildApplication::default());
    builds.fail_next_after_partial_upload();
    let first_bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      Arc::new(FixedPolicies),
      Arc::new(FixedBuildClock),
    );
    let shutdown = FactoryReconciliationShutdown::new();
    let first_worker = reconciler(store.clone(), "factory.slice", 1, 1, 11);

    reconcile_named(&first_worker, &shutdown, "create initial implementation attempt").await;
    reconcile_named(&first_worker, &shutdown, "schedule initial implementation Build").await;
    let failed = run_current_build(&store, &first_bridge).await;
    assert_eq!(failed.state(), BuildState::Failed);
    assert!(failed.infrastructure_retry_eligible());
    assert_eq!(failed.outputs().len(), 1, "fixture must model a partial upload");

    let failed_snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    let failed_stage_id = failed_snapshot.current.stage_attempt_id.unwrap();
    assert!(failed_snapshot.candidates.is_empty());
    assert_eq!(failed_snapshot.stage_attempts.len(), 1);
    assert_eq!(builds.requests()[0].immutable_revision, admitted.base_revision);
    assert!(matches!(
      current_checkpoint(&failed_snapshot).progress,
      FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Implementation,
        progress: FactoryStageProgress::RetryableFailure,
      }
    ));

    let retry = retry_infrastructure(
      admitted.factory_run_id,
      failed_snapshot.run.version(),
      failed_stage_id,
      "retry-partial-capture",
    );
    store.apply_factory_run_control(retry).await.unwrap();

    reconcile_named(&first_worker, &shutdown, "create replacement implementation attempt").await;
    reconcile_named(&first_worker, &shutdown, "schedule replacement implementation Build").await;
    let retry_snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    let retry_stage_id = retry_snapshot.current.stage_attempt_id.unwrap();
    assert_ne!(retry_stage_id, failed_stage_id);
    assert_eq!(retry_snapshot.stage_attempts.len(), 2);
    let mut attempt_numbers = retry_snapshot
      .stage_attempts
      .iter()
      .map(|attempt| attempt.number().get())
      .collect::<Vec<_>>();
    attempt_numbers.sort_unstable();
    assert_eq!(attempt_numbers, [1, 2]);
    assert!(retry_snapshot.candidates.is_empty());

    // New process-local bridge state models losing the first Agent and its
    // worker. Durable server-adapter reconstruction is covered separately by
    // the PostgreSQL Factory contract; this slice verifies that no bridge-local
    // state is needed to resume from immutable Build inputs.
    let restarted_bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      Arc::new(FixedPolicies),
      Arc::new(FixedBuildClock),
    );
    let succeeded = run_current_build(&store, &restarted_bridge).await;
    assert_eq!(succeeded.state(), BuildState::Succeeded);
    let requests = builds.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].immutable_revision, admitted.base_revision);
    assert_eq!(requests[1].immutable_revision, admitted.base_revision);
    assert_eq!(requests[0].causality.target, FactoryStageTarget::Implementation);
    assert_eq!(requests[1].causality.target, FactoryStageTarget::Implementation);
    assert_ne!(
      requests[0].causality.stage_attempt_id,
      requests[1].causality.stage_attempt_id
    );

    reconcile_named(&first_worker, &shutdown, "request successful candidate capture").await;
    let capture = claim_outbox(&store, "candidate.capture").await;
    let implementation_build_id = store
      .factory_run_snapshot(admitted.factory_run_id)
      .await
      .unwrap()
      .current
      .build_id
      .unwrap();
    let accepted =
      crate::accept_factory_change_set(&*store, capture, builds.documents(implementation_build_id), time(11))
        .await
        .unwrap()
        .change_set;

    // Recreate both workers after acceptance. Recovery must continue with
    // validation from retained artifacts, never schedule implementation again.
    let recovered_worker = reconciler(store.clone(), "factory.slice", 1, 1, 11);
    let recovered_bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      Arc::new(FixedPolicies),
      Arc::new(FixedBuildClock),
    );
    reconcile_named(
      &recovered_worker,
      &shutdown,
      "continue accepted candidate with validation",
    )
    .await;
    reconcile_named(&recovered_worker, &shutdown, "schedule validation after restart").await;
    let validation = run_current_build(&store, &recovered_bridge).await;
    assert_eq!(validation.state(), BuildState::Succeeded);

    let recovered = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    assert_eq!(recovered.current.candidate_id, Some(accepted.id()));
    assert!(recovered.candidates.contains(&accepted));
    let requests = builds.requests();
    assert_eq!(requests.len(), 3, "recovery must not rerun implementation");
    assert_eq!(requests[2].causality.target, FactoryStageTarget::Validation);
    assert_eq!(requests[2].immutable_revision, admitted.base_revision);
    let materialization = requests[2].causality.candidate.as_ref().unwrap();
    assert_eq!(materialization.change_set_id, accepted.id());
    assert_eq!(materialization.base_revision, admitted.base_revision);
    assert_eq!(
      materialization.candidate_revision,
      *accepted.subject().candidate_revision()
    );
  });
}

#[test]
fn infrastructure_retry_stops_at_the_factory_attempt_budget() {
  run_to_completion(async {
    let project_id = octacity_server_domain::ProjectId::generate();
    let configuration_id = octacity_server_factory::FactoryConfigurationId::generate();
    let repository_id = octacity_server_domain::RepositoryId::generate();
    let main = octacity_server_domain::SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let admission = FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default()));
    let admitted = admit(&admission, command(project_id, configuration_id, repository_id))
      .await
      .unwrap();
    let builds = Arc::new(SucceedingBuildApplication::default());
    let bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      Arc::new(FixedPolicies),
      Arc::new(FixedBuildClock),
    );
    let shutdown = FactoryReconciliationShutdown::new();
    let worker = reconciler(store.clone(), "factory.slice", 1, 1, 11);
    reconcile_named(&worker, &shutdown, "create initial implementation attempt").await;
    reconcile_named(&worker, &shutdown, "schedule initial implementation Build").await;

    let maximum_attempts = 10;
    for attempt in 1..=maximum_attempts {
      builds.fail_next_after_partial_upload();
      let failed = run_current_build(&store, &bridge).await;
      assert_eq!(failed.state(), BuildState::Failed);
      let snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
      let stage_attempt_id = snapshot.current.stage_attempt_id.unwrap();
      let retry = retry_infrastructure(
        admitted.factory_run_id,
        snapshot.run.version(),
        stage_attempt_id,
        &format!("retry-budget-{attempt}"),
      );
      let outcome = store.apply_factory_run_control(retry).await;
      if attempt == maximum_attempts {
        assert!(
          outcome.is_err(),
          "retry beyond the configured attempt budget was accepted"
        );
      } else {
        outcome.unwrap();
        reconcile_named(&worker, &shutdown, "create bounded replacement attempt").await;
        reconcile_named(&worker, &shutdown, "schedule bounded replacement Build").await;
      }
    }

    let exhausted = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    assert_eq!(exhausted.stage_attempts.len(), maximum_attempts);
    assert_eq!(builds.count(), maximum_attempts);
    assert!(exhausted.candidates.is_empty());
  });
}

#[test]
fn manual_admission_reaches_delivery_approval_without_a_provider_or_delivery_dispatch() {
  run_to_completion(async {
    let project_id = octacity_server_domain::ProjectId::generate();
    let configuration_id = octacity_server_factory::FactoryConfigurationId::generate();
    let repository_id = octacity_server_domain::RepositoryId::generate();
    let main = octacity_server_domain::SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let admission = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let admitted = admit(&admission, command(project_id, configuration_id, repository_id))
      .await
      .unwrap();
    let builds = Arc::new(SucceedingBuildApplication::default());
    let bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      Arc::new(FixedPolicies),
      Arc::new(FixedBuildClock),
    );
    let shutdown = FactoryReconciliationShutdown::new();
    let worker = reconciler(store.clone(), "factory.slice", 1, 1, 11);

    reconcile(&worker, &shutdown).await;
    reconcile(&worker, &shutdown).await;
    run_build(&store, &bridge).await;

    reconcile(&worker, &shutdown).await;
    let candidate_outbox = claim_outbox(&store, "candidate.capture").await;
    let replayed_candidate_outbox = candidate_outbox.clone();
    let build_id = store
      .factory_run_snapshot(admitted.factory_run_id)
      .await
      .unwrap()
      .current
      .build_id
      .unwrap();
    let accepted = crate::accept_factory_change_set(&*store, candidate_outbox, builds.documents(build_id), time(11))
      .await
      .unwrap();
    assert_eq!(
      accepted.disposition,
      octacity_server_store::MutationDisposition::Applied
    );
    let candidate = accepted.change_set;
    let replayed = crate::accept_factory_change_set(
      &*store,
      replayed_candidate_outbox.clone(),
      builds.documents(build_id),
      time(11),
    )
    .await
    .unwrap();
    assert_eq!(
      replayed.disposition,
      octacity_server_store::MutationDisposition::Replayed
    );
    assert_eq!(replayed.change_set, candidate);
    assert!(matches!(
      crate::accept_factory_change_set(
        &*store,
        replayed_candidate_outbox.clone(),
        builds.tampered_documents(build_id),
        time(11),
      )
      .await,
      Err(crate::FactoryChangeSetError::InvalidEvidence)
    ));
    let mut duplicate = builds.documents(build_id);
    duplicate.push(duplicate[0].clone());
    assert!(matches!(
      crate::accept_factory_change_set(&*store, replayed_candidate_outbox.clone(), duplicate, time(11)).await,
      Err(crate::FactoryChangeSetError::InvalidEvidence)
    ));

    reconcile(&worker, &shutdown).await;
    reconcile(&worker, &shutdown).await;
    run_build(&store, &bridge).await;
    let validation_snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    let validation_request = builds
      .accepted(validation_snapshot.current.build_id.unwrap())
      .unwrap()
      .request;
    assert_eq!(
      validation_request.immutable_revision,
      *candidate.subject().exact().base_revision()
    );
    let materialization = validation_request.causality.candidate.as_ref().unwrap();
    assert_eq!(materialization.change_set_id, candidate.id());
    assert_eq!(
      materialization.candidate_revision,
      *candidate.subject().candidate_revision()
    );
    let (instruction, inputs) = materialization.wire_inputs();
    assert_eq!(
      instruction.candidate_revision,
      candidate.subject().candidate_revision().as_str()
    );
    assert_eq!(inputs.len(), 2);
    assert!(matches!(
      crate::accept_factory_change_set(&*store, replayed_candidate_outbox, builds.documents(build_id), time(11),).await,
      Err(crate::FactoryChangeSetError::InvalidEvidence)
    ));

    reconcile(&worker, &shutdown).await;
    let evidence_outbox = claim_outbox(&store, "evidence.construct").await;
    let schema = ImmutableReference::new(key("test-schema"), key("v1"), digest(20));
    let tool = ImmutableReference::new(key("test-tool"), key("v1"), digest(21));
    let plugin = ImmutableReference::new(key("test-plugin"), key("v1"), digest(22));
    let evidence_item = EvidenceItem::new(EvidenceItemInput {
      subject: candidate.subject().clone(),
      kind: key("tests"),
      output_kind: EvidenceOutputKind::Report,
      artifact: FactoryArtifactReference::new(ArtifactId::generate(), digest(2), 1).unwrap(),
      schema: schema.clone(),
      producer: EvidenceProducer::new(
        BuildId::generate(),
        AttemptId::generate(),
        JobId::generate(),
        tool.clone(),
        plugin.clone(),
      ),
      outcome: DeterministicGateOutcome::Passed,
      published_at: time(9),
      fresh_until: time(11),
    });
    let evidence = EvidenceManifest::new(
      EvidenceManifestId::generate(),
      &candidate,
      candidate.subject().clone(),
      time(10),
      vec![EvidenceRequirement::new(
        key("tests"),
        EvidenceOutputKind::Report,
        schema,
        tool,
        plugin,
      )],
      vec![evidence_item.clone()],
    )
    .unwrap();
    commit_worker_result(
      &store,
      evidence_outbox,
      FactoryRunState::Validating,
      FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Validation,
        progress: FactoryStageProgress::EvidenceConstructed,
      },
      FactoryRunHistoryAppend {
        evidence: vec![evidence.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.evidence_id = Some(evidence.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    let plan_outbox = claim_outbox(&store, "evaluation.plan").await;
    let budget = BudgetLimit::new(1, 1, 1, 1, 1).unwrap();
    let pack_reference = ImmutableReference::new(key("quality"), key("v1"), digest(23));
    let pack = CriterionPack::new(
      evidence.subject().exact().project_id(),
      pack_reference.clone(),
      ImmutableReference::new(key("criterion-schema"), key("v1"), digest(24)),
      FactoryArtifactReference::new(ArtifactId::generate(), digest(23), 1).unwrap(),
    )
    .unwrap();
    let evaluator = ImmutableReference::new(key("review"), key("v1"), digest(25));
    let data_handling = ImmutableReference::new(key("restricted-source"), key("v1"), digest(26));
    let evaluation_policy =
      EvaluationPolicy::try_new(vec![pack_reference], vec![evaluator.clone()], 1, budget).unwrap();
    let definition = EvaluationPlanDefinition {
      id: EvaluationPlanId::generate(),
      purpose: ReviewPurpose::Implementation,
      subject: evidence.subject().clone(),
      criterion_packs: vec![pack],
      branches: vec![ReviewBranch::new(key("review"), evaluator.clone(), true)],
      budget,
      created_at: time(10),
      deadline: time(11),
      data_handling: data_handling.clone(),
    };
    let capabilities = [ReviewEvaluatorCapability::try_new(
      evaluator.clone(),
      vec![ReviewPurpose::Implementation],
      vec![data_handling],
      budget,
    )
    .unwrap()];
    let plan = match prepare_evaluation_plan(definition, &evidence, &evaluation_policy, &capabilities).unwrap() {
      ReviewPlanPreparation::Ready(plan) => *plan,
      ReviewPlanPreparation::Escalate(reason) => panic!("fixture review plan escalated: {reason:?}"),
    };
    let branches = EvaluationProgress::try_new(
      vec![EvaluationBranch::new(
        key("review"),
        true,
        EvaluationBranchState::Pending,
      )],
      1,
    )
    .unwrap();
    commit_worker_result(
      &store,
      plan_outbox,
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)),
      FactoryRunHistoryAppend {
        evaluation_plans: vec![plan.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.evaluation_plan_id = Some(plan.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    reconcile(&worker, &shutdown).await;
    run_build(&store, &bridge).await;

    reconcile(&worker, &shutdown).await;
    let decision_outbox = claim_outbox(&store, "decision.compute").await;
    let assessment = Assessment::new(
      AssessmentId::generate(),
      &plan,
      AssessmentInput {
        subject: plan.subject().clone(),
        evaluator,
        outcome: AssessmentOutcome::Satisfied,
        summary: BoundedSummary::new(
          FactoryTaskSubject::Candidate(plan.subject().clone()),
          FactorySafeText::new("Independent review satisfied the selected criteria").unwrap(),
          digest(27),
        ),
        findings: Vec::new(),
        model: ImmutableReference::new(key("review-model"), key("v1"), digest(28)),
        prompt_digest: digest(29),
        result: FactoryArtifactReference::new(ArtifactId::generate(), digest(30), 1).unwrap(),
        provenance: FactoryArtifactReference::new(ArtifactId::generate(), digest(31), 1).unwrap(),
      },
    )
    .unwrap();
    let policy = DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![key("tests")],
        required_evaluators: vec![key("review")],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: DecisionOutcome::Escalate,
      },
    )
    .unwrap();
    let assessments = [assessment.clone()];
    let decision = evaluate_decision(
      DecisionId::generate(),
      DecisionEngineInput::new(&evidence, &plan, &policy, &assessments),
    )
    .unwrap();
    assert_eq!(decision.outcome(), DecisionOutcome::Accept);
    commit_worker_result(
      &store,
      decision_outbox,
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
        outcome: decision.outcome(),
        completed_rework_cycles: 0,
        max_rework_cycles: 0,
      }),
      FactoryRunHistoryAppend {
        assessments: vec![assessment],
        decisions: vec![decision.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.decision_id = Some(decision.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    let waiting = worker.run_once(time(10), time(100), &shutdown).await.unwrap();
    assert_eq!(waiting.waiting, 1);

    let final_snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    assert_eq!(final_snapshot.run.state(), FactoryRunState::ReadyForDelivery);
    assert!(matches!(
      current_checkpoint(&final_snapshot).progress,
      FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval)
    ));
    assert_eq!(builds.count(), 3);
    assert_eq!(resolver.call_count(), 1);
    assert!(final_snapshot.signal_requests.is_empty());
    assert!(final_snapshot.signal_receipts.is_empty());
    assert!(final_snapshot.delivery_attempts.is_empty());
    assert!(
      !final_snapshot
        .outbox
        .iter()
        .any(|record| record.kind == key("delivery.request"))
    );
  });
}

async fn reconcile<S>(worker: &crate::FactoryReconciler<S>, shutdown: &FactoryReconciliationShutdown)
where
  S: FactoryRunStore + octacity_server_store::FactoryConfigurationStore + 'static,
{
  let outcome = worker.run_once(time(10), time(100), shutdown).await.unwrap();
  assert_eq!(outcome.actions_committed, 1);
}

async fn reconcile_named<S>(
  worker: &crate::FactoryReconciler<S>,
  shutdown: &FactoryReconciliationShutdown,
  operation: &str,
) where
  S: FactoryRunStore + octacity_server_store::FactoryConfigurationStore + 'static,
{
  let outcome = worker.run_once(time(10), time(100), shutdown).await.unwrap();
  assert_eq!(outcome.actions_committed, 1, "{operation}");
}

async fn run_build(
  store: &Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  bridge: &FactoryBuildBridge<
    octacity_server_store::testing::InMemoryFactoryConfigurationStore,
    SucceedingBuildApplication,
    FixedPolicies,
  >,
) {
  let observation = run_current_build(store, bridge).await;
  assert_eq!(observation.state(), BuildState::Succeeded);
  assert_eq!(observation.disposition(), Some(MutationDisposition::Applied));
}

async fn run_current_build(
  store: &Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  bridge: &FactoryBuildBridge<
    octacity_server_store::testing::InMemoryFactoryConfigurationStore,
    SucceedingBuildApplication,
    FixedPolicies,
  >,
) -> crate::FactoryBuildObservation {
  let outbox = claim_outbox(store, "build.create").await;
  bridge.dispatch(outbox).await.unwrap();
  let claimed = store
    .claim_factory_runs(ClaimFactoryRuns::new(key("factory.slice"), time(10), time(100), 1).unwrap())
    .await
    .unwrap()
    .pop()
    .unwrap();
  let observation = bridge.observe(&claimed).await.unwrap();
  assert_eq!(observation.disposition(), Some(MutationDisposition::Applied));
  observation
}

async fn claim_outbox(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  kind: &str,
) -> ClaimedFactoryOutbox {
  let claimed = store
    .claim_factory_outbox(ClaimFactoryOutbox::new(key("factory.slice"), time(11), time(100), 32).unwrap())
    .await
    .unwrap();
  let mut selected = None;
  for record in claimed {
    if record.record.kind.as_str() == kind {
      selected = Some(record);
    } else {
      settle_outbox(store, &record).await;
    }
  }
  selected.unwrap_or_else(|| panic!("expected pending {kind} operation"))
}

async fn settle_outbox(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  claimed: &ClaimedFactoryOutbox,
) {
  store
    .settle_factory_outbox(SettleFactoryOutbox {
      operation_id: claimed.record.operation_id,
      owner: claimed.record.owner.clone().unwrap(),
      fence: claimed.record.claim.unwrap().fence(),
      observed_at: time(11),
      settlement: FactoryOutboxSettlement::Delivered,
    })
    .await
    .unwrap();
}

async fn commit_worker_result(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  outbox: ClaimedFactoryOutbox,
  state: FactoryRunState,
  progress: FactoryLifecycleProgress,
  append: FactoryRunHistoryAppend,
  update_current: impl FnOnce(&mut octacity_server_store::FactoryRunCurrentProjection),
) {
  let operation_kind = outbox.record.kind.clone();
  let snapshot = store.factory_run_snapshot(outbox.record.run_id).await.unwrap();
  let claim = snapshot.current_claim.clone().unwrap();
  let current_budget = snapshot
    .budgets
    .iter()
    .find(|budget| budget.id == snapshot.current.budget_id)
    .unwrap();
  let checkpoint = current_checkpoint(&snapshot);
  let version = FactoryRunVersion::new(snapshot.run.version().get() + 1).unwrap();
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, current_budget.usage, time(11));
  let lifecycle = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    progress,
    checkpoint.signal,
    checkpoint.cancellation_requested,
    time(11),
  );
  let next_progress = lifecycle.progress.clone();
  let previous_progress = checkpoint.progress.clone();
  let mut current = snapshot.current;
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = lifecycle.id;
  update_current(&mut current);
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at: time(11),
      next_run: FactoryRun::restore(
        snapshot.run.id(),
        snapshot.run.configuration().clone(),
        &snapshot.work,
        snapshot.run.subject().clone(),
        state,
        version,
      )
      .unwrap(),
      budget,
      lifecycle_checkpoint: lifecycle,
      append,
      current,
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        Some(digest(9)),
        key("factory.fixture.completed"),
        outbox.record.operation_id,
        key("accepted"),
        time(11),
      ),
      outbox: Vec::new(),
    })
    .await
    .unwrap_or_else(|error| {
      panic!(
        "{operation_kind:?} worker transition failed: {error:?}; previous={previous_progress:?} next={:?}",
        next_progress
      )
    });
  settle_outbox(store, &outbox).await;
}

fn current_checkpoint(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
) -> &octacity_server_store::FactoryLifecycleCheckpoint {
  snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
    .unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
