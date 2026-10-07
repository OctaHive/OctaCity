use std::{
  collections::VecDeque,
  sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
  },
};

use async_trait::async_trait;
use octacity_server_domain::{
  AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, PipelineId, PipelineVersion, RepositoryVersion,
  TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_factory::{
  ChangeSetId, DecisionId, FactoryKey, FactoryOutputPermissions, FactoryPermissionDraft, FactoryPermissionSet,
  FactoryResourceLimits, FactoryStageTarget, LocalPermissionCeiling,
};
use octacity_server_job::JobFailureClass;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  AttemptRecord, BuildQueryStore, BuildRecord, BuildRetentionDeadlines, ClaimFactoryOutbox, ClaimFactoryRuns,
  FactoryOutboxState, FactoryRunStore, ImmutableBuildInput, NormalizedTriggerOccurrence, StoreError, TriggerCause,
  TriggerDefinitionRef, TriggerMetadata, TriggerOccurrenceState, TriggerTarget,
  testing::InMemoryFactoryConfigurationStore,
};

use crate::{
  CreateFactoryBuild, FactoryBuildAcceptance, FactoryBuildBridge, FactoryBuildOutputSource, FactoryBuildPolicyLayers,
  FactoryBuildPolicyRequest, FactoryBuildPolicySource, FactoryBuildPolicySourceError, FactoryReconciliationShutdown,
  MutationDisposition, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
  factory_build_bridge::{FactoryBuildClock, failed_job_classes_are_retryable, selected_build_subject},
  factory_reconciliation_tests::{key, reconciler, run_to_completion, seeded_store, time},
};

#[test]
fn only_authoritative_infrastructure_failures_are_retryable() {
  assert!(failed_job_classes_are_retryable([
    Some(JobFailureClass::Infrastructure),
    Some(JobFailureClass::Infrastructure),
  ]));
  assert!(!failed_job_classes_are_retryable([
    Some(JobFailureClass::Infrastructure),
    Some(JobFailureClass::Execution),
  ]));
  assert!(!failed_job_classes_are_retryable([None]));
  assert!(!failed_job_classes_are_retryable([]));
}

struct TestClock(AtomicI64);

impl TestClock {
  fn new(value: i64) -> Self {
    Self(AtomicI64::new(value))
  }

  fn set(&self, value: i64) {
    self.0.store(value, Ordering::Release);
  }
}

impl FactoryBuildClock for TestClock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::FactoryBuildBridgeError> {
    Ok(time(self.0.load(Ordering::Acquire)))
  }
}

struct SequenceClock(Mutex<VecDeque<i64>>);

impl SequenceClock {
  fn new(values: impl IntoIterator<Item = i64>) -> Self {
    Self(Mutex::new(values.into_iter().collect()))
  }
}

impl FactoryBuildClock for SequenceClock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::FactoryBuildBridgeError> {
    let value = self
      .0
      .lock()
      .unwrap()
      .pop_front()
      .expect("fixture clock must provide every requested instant");
    Ok(time(value))
  }
}

#[test]
fn every_factory_build_stage_selects_the_exact_revision_and_parent() {
  let base = octacity_server_domain::ImmutableRevision::new("base").unwrap();
  let candidate = octacity_server_domain::ImmutableRevision::new("candidate").unwrap();
  let change_set = ChangeSetId::generate();
  let decision = DecisionId::generate();
  let evaluation = FactoryStageTarget::Evaluation(FactoryKey::new("security").unwrap());

  for target in [FactoryStageTarget::Validation, evaluation] {
    assert_eq!(
      selected_build_subject(&target, base.clone(), Some((change_set, candidate.clone())), None,).unwrap(),
      (
        candidate.clone(),
        Some(octacity_server_store::FactoryBuildParent::ChangeSet(change_set))
      )
    );
  }
  assert_eq!(
    selected_build_subject(&FactoryStageTarget::Implementation, base.clone(), None, None).unwrap(),
    (base, None)
  );
  assert_eq!(
    selected_build_subject(
      &FactoryStageTarget::Rework,
      octacity_server_domain::ImmutableRevision::new("unused-base").unwrap(),
      Some((change_set, candidate.clone())),
      Some((decision, candidate.clone())),
    )
    .unwrap(),
    (
      candidate,
      Some(octacity_server_store::FactoryBuildParent::Decision(decision))
    )
  );
}

#[derive(Default)]
struct FixedPolicies {
  requests: Mutex<Vec<FactoryBuildPolicyRequest>>,
}

impl FixedPolicies {
  fn layers() -> FactoryBuildPolicyLayers {
    FactoryBuildPolicyLayers {
      project: permission_set(800),
      configuration: permission_set(600),
      task: permission_set(400),
      local: LocalPermissionCeiling::fully_enforced(permission_set(200)),
    }
  }
}

#[async_trait]
impl FactoryBuildPolicySource for FixedPolicies {
  async fn policy_layers(
    &self,
    request: FactoryBuildPolicyRequest,
  ) -> Result<FactoryBuildPolicyLayers, FactoryBuildPolicySourceError> {
    self.requests.lock().unwrap().push(request);
    Ok(Self::layers())
  }
}

fn permission_set(limit: u32) -> FactoryPermissionSet {
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    resources: FactoryResourceLimits::new(limit, u64::from(limit), u64::from(limit), limit, u64::from(limit)).unwrap(),
    outputs: FactoryOutputPermissions::try_new(Vec::new(), limit, u64::from(limit), limit, u64::from(limit)).unwrap(),
    ..FactoryPermissionDraft::default()
  })
  .unwrap()
}

struct RecordingBuildApplication {
  requests: Mutex<Vec<CreateFactoryBuild>>,
  acceptance: FactoryBuildAcceptance,
  state: Mutex<BuildState>,
  latest_attempt_id: Mutex<AttemptId>,
  latest_attempt_reads: Mutex<usize>,
}

impl RecordingBuildApplication {
  fn new() -> Self {
    let attempt_id = AttemptId::generate();
    Self {
      requests: Mutex::new(Vec::new()),
      acceptance: FactoryBuildAcceptance {
        build_id: BuildId::generate(),
        attempt_id,
        job_ids: vec![octacity_server_domain::JobId::generate()],
        effective_policy_digest: permission_set(400).digest(),
      },
      state: Mutex::new(BuildState::Queued),
      latest_attempt_id: Mutex::new(attempt_id),
      latest_attempt_reads: Mutex::new(0),
    }
  }

  fn request(&self) -> CreateFactoryBuild {
    self.requests.lock().unwrap()[0].clone()
  }

  fn set_state(&self, state: BuildState) {
    *self.state.lock().unwrap() = state;
  }

  fn set_latest_attempt(&self, attempt_id: AttemptId) {
    *self.latest_attempt_id.lock().unwrap() = attempt_id;
  }
}

#[async_trait]
impl OrdinaryBuildApplication for RecordingBuildApplication {
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    let mut acceptance = self.acceptance.clone();
    acceptance.effective_policy_digest = request.effective_permissions.digest();
    self.requests.lock().unwrap().push(request);
    Ok(acceptance)
  }
}

#[async_trait]
impl FactoryBuildOutputSource for RecordingBuildApplication {
  async fn published_outputs(
    &self,
    _build_id: BuildId,
    _attempt_id: AttemptId,
  ) -> Result<Vec<octacity_server_artifacts::ArtifactIdentity>, OrdinaryBuildApplicationError> {
    Ok(Vec::new())
  }
}

#[async_trait]
impl BuildQueryStore for RecordingBuildApplication {
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError> {
    let request = self.request();
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    let state = *self.state.lock().unwrap();
    let trigger = NormalizedTriggerOccurrence::root(
      TriggerOccurrenceId::generate(),
      TriggerDefinitionRef {
        id: TriggerId::generate(),
        version: TriggerVersion::INITIAL,
      },
      TriggerTarget {
        configuration_id: request.build_configuration.id(),
        configuration_version: request.build_configuration.version(),
      },
      TriggerIdentity::new("factory-build-fixture").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      time(12),
    )
    .unwrap();
    Ok(BuildRecord {
      build: ImmutableBuildInput {
        id: build_id,
        project_id: request.project_id,
        configuration_id: request.build_configuration.id(),
        configuration_version: request.build_configuration.version(),
        pipeline_id: PipelineId::generate(),
        pipeline_version: PipelineVersion::INITIAL,
        repository_id: request.repository_id,
        repository_version: RepositoryVersion::INITIAL,
        immutable_revision: request.immutable_revision,
        input_snapshot: serde_json::json!({}),
        effective_policy_snapshot: serde_json::json!({}),
        retention: BuildRetentionDeadlines {
          metadata: time(1_000),
          logs: time(1_000),
          artifacts: time(1_000),
          reports: time(1_000),
        },
        project_job_concurrency_limit: 1,
        priority: request.priority,
      },
      state,
      version: BuildVersion::INITIAL,
      trigger,
      trigger_state: TriggerOccurrenceState::Accepted,
      trigger_created_at: time(12),
      trigger_updated_at: time(12),
      created_at: time(12),
      updated_at: time(12),
    })
  }

  async fn attempt(&self, _attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
    self.latest_attempt(self.acceptance.build_id).await
  }

  async fn job(&self, _job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn latest_attempt(&self, build_id: BuildId) -> Result<AttemptRecord, StoreError> {
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    *self.latest_attempt_reads.lock().unwrap() += 1;
    let state = match *self.state.lock().unwrap() {
      BuildState::Queued => AttemptState::Created,
      BuildState::Running => AttemptState::Running,
      BuildState::Succeeded => AttemptState::Succeeded,
      BuildState::Failed => AttemptState::Failed,
      BuildState::Cancelled => AttemptState::Cancelled,
    };
    Ok(AttemptRecord {
      id: *self.latest_attempt_id.lock().unwrap(),
      build_id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state,
      version: AttemptVersion::INITIAL,
      created_at: time(12),
      updated_at: time(12),
      jobs: Vec::new(),
    })
  }
}

#[test]
fn dispatch_links_one_ordinary_build_with_narrowed_external_causality() {
  run_to_completion(async {
    let store = run_until_build_dispatch();
    let builds = Arc::new(RecordingBuildApplication::new());
    let policies = Arc::new(FixedPolicies::default());
    let claimed = claimed_build_dispatch(&store).await;
    let bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      policies.clone(),
      Arc::new(TestClock::new(13)),
    );

    let outcome = bridge.dispatch(claimed).await.unwrap();

    assert_eq!(outcome.disposition, MutationDisposition::Applied);
    assert_eq!(outcome.build_id, builds.acceptance.build_id);
    let request = builds.request();
    let layers = FixedPolicies::layers();
    assert!(request.effective_permissions.is_no_broader_than(&layers.project));
    assert!(request.effective_permissions.is_no_broader_than(&layers.configuration));
    assert!(request.effective_permissions.is_no_broader_than(&layers.task));
    assert!(request.effective_permissions.is_no_broader_than(layers.local.grants()));
    assert_eq!(request.effective_permissions.resources().cpu_millis(), 200);
    assert_eq!(policies.requests.lock().unwrap().len(), 1);

    let run_id = request.causality.run_id;
    let snapshot = store.factory_run_snapshot(run_id).await.unwrap();
    assert_eq!(snapshot.linked_builds.len(), 1);
    let link = &snapshot.linked_builds[0];
    assert_eq!(link.build_id, outcome.build_id);
    assert_eq!(link.attempt_id, builds.acceptance.attempt_id);
    assert_eq!(link.job_ids, builds.acceptance.job_ids);
    assert_eq!(link.stage_attempt_id, request.causality.stage_attempt_id);
    assert_eq!(link.task_envelope_digest, request.causality.task_envelope_digest);
    assert_eq!(link.effective_policy_digest, request.effective_permissions.digest());
    assert_eq!(snapshot.current.build_id, Some(outcome.build_id));
    assert!(
      snapshot
        .outbox
        .iter()
        .any(|record| record.kind == key("build.create") && record.state == FactoryOutboxState::Delivered)
    );
  });
}

#[test]
fn dispatch_revalidates_claims_with_fresh_time_after_async_build_creation() {
  run_to_completion(async {
    let store = run_until_build_dispatch();
    let builds = Arc::new(RecordingBuildApplication::new());
    let claimed = claimed_build_dispatch(&store).await;
    let run_id = claimed.record.run_id;
    let bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds,
      Arc::new(FixedPolicies::default()),
      Arc::new(SequenceClock::new([13, 101])),
    );

    let failure = bridge.dispatch(claimed).await.unwrap_err();

    assert!(matches!(failure, crate::FactoryBuildBridgeError::Store(_)));
    assert!(
      store
        .factory_run_snapshot(run_id)
        .await
        .unwrap()
        .linked_builds
        .is_empty()
    );
  });
}

#[test]
fn authoritative_build_cancellation_advances_only_factory_observation() {
  run_to_completion(async {
    let store = run_until_build_dispatch();
    let builds = Arc::new(RecordingBuildApplication::new());
    let policies = Arc::new(FixedPolicies::default());
    let clock = Arc::new(TestClock::new(13));
    let bridge = FactoryBuildBridge::new_with_clock(store.clone(), builds.clone(), policies, clock.clone());
    let claimed = claimed_build_dispatch(&store).await;
    let dispatched = bridge.dispatch(claimed).await.unwrap();
    let retried_attempt = AttemptId::generate();
    builds.set_latest_attempt(retried_attempt);
    builds.set_state(BuildState::Cancelled);

    let claimed_run = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("worker.two"), time(20), time(30), 1).unwrap())
      .await
      .unwrap()
      .pop()
      .expect("linked Run is eligible for observation");
    clock.set(21);
    let observation = bridge.observe(&claimed_run).await.unwrap();

    assert_eq!(observation.build_id, dispatched.build_id);
    assert_eq!(observation.state, BuildState::Cancelled);
    assert_eq!(observation.attempt_id, retried_attempt);
    assert_ne!(observation.attempt_id, dispatched.attempt_id);
    assert_eq!(observation.disposition, Some(MutationDisposition::Applied));
    assert!(observation.job_ids.is_empty());
    assert!(observation.outputs.is_empty());
    assert_eq!(*builds.latest_attempt_reads.lock().unwrap(), 1);
    let snapshot = store.factory_run_snapshot(claimed_run.run_id).await.unwrap();
    assert_eq!(snapshot.build_observations.len(), 1);
    let terminal = &snapshot.build_observations[0];
    assert_eq!(terminal.build_id, dispatched.build_id);
    assert_eq!(terminal.attempt_id, retried_attempt);
    assert_eq!(terminal.target, snapshot.linked_builds[0].target);
    assert_eq!(terminal.state, BuildState::Cancelled);
    assert!(matches!(
      snapshot
        .lifecycle_checkpoints
        .iter()
        .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
        .unwrap()
        .progress,
      octacity_server_factory::FactoryLifecycleProgress::Stage {
        progress: octacity_server_factory::FactoryStageProgress::Cancelled,
        ..
      }
    ));
    assert_eq!(
      builds.build(dispatched.build_id).await.unwrap().state,
      BuildState::Cancelled
    );
  });
}

fn run_until_build_dispatch() -> Arc<InMemoryFactoryConfigurationStore> {
  let store = seeded_store(1);
  let worker = reconciler(store.clone(), "worker.one", 1, 1, 11);
  let shutdown = FactoryReconciliationShutdown::new();
  run_to_completion(worker.run_once(time(10), time(20), &shutdown)).unwrap();
  run_to_completion(worker.run_once(time(10), time(20), &shutdown)).unwrap();
  store
}

async fn claimed_build_dispatch(
  store: &InMemoryFactoryConfigurationStore,
) -> octacity_server_store::ClaimedFactoryOutbox {
  store
    .claim_factory_outbox(ClaimFactoryOutbox::new(key("worker.one"), time(12), time(19), 10).unwrap())
    .await
    .unwrap()
    .into_iter()
    .find(|claimed| claimed.record.kind == key("build.create"))
    .expect("Build dispatch is durably pending")
}
