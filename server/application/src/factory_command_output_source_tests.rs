//! Retained ordinary command publications, independent of any model wire format.
use crate::factory_node_test_support::fixtures;
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_protocol::FactoryCausalityV3;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_job::{JobSpecBuildSnapshot, JobState};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{testing::*, *};
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};

fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
struct ReadClock(i64);
impl FactoryExecutionClock for ReadClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0))
  }
}
struct PreparationClock;
impl FactoryExecutionClock for PreparationClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(6))
  }
}
struct NeverDispatch;
#[async_trait]
impl FactoryNodeExecutor for NeverDispatch {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("fixture prepares only the persisted input and intent");
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    unreachable!()
  }
}
#[derive(Clone)]
struct Queries {
  build: BuildRecord,
  attempt: AttemptRecord,
  history: BTreeMap<AttemptId, AttemptRecord>,
}
#[async_trait]
impl BuildQueryStore for Queries {
  async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
    assert_eq!(id, self.build.build.id);
    Ok(self.build.clone())
  }
  async fn attempt(&self, id: AttemptId) -> Result<AttemptRecord, StoreError> {
    if id == self.attempt.id {
      Ok(self.attempt.clone())
    } else {
      self.history.get(&id).cloned().ok_or(StoreError::Unavailable)
    }
  }
  async fn latest_attempt(&self, id: BuildId) -> Result<AttemptRecord, StoreError> {
    assert_eq!(id, self.build.build.id);
    Ok(self.attempt.clone())
  }
  async fn job(&self, id: JobId) -> Result<JobRecord, StoreError> {
    self
      .attempt
      .jobs
      .iter()
      .chain(self.history.values().flat_map(|attempt| &attempt.jobs))
      .find(|job| job.id() == id)
      .cloned()
      .ok_or(StoreError::Unavailable)
  }
}
#[derive(Default)]
struct Bytes(Mutex<BTreeMap<ArtifactId, Vec<u8>>>);
#[async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.0.lock().unwrap().insert(object.artifact_id(), bytes);
    Ok(())
  }
}
#[async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    let bytes = self.0.lock().unwrap().get(&object.artifact_id()).unwrap().clone();
    assert!(bytes.len() as u64 <= maximum);
    Ok(bytes)
  }
}
struct Inputs;
#[async_trait]
impl FactoryExecutionInputStore for Inputs {
  async fn reserve_factory_execution_inputs(
    &self,
    request: PublishFactoryExecutionInputs,
  ) -> Result<FactoryExecutionInputDisposition, StoreError> {
    request.validate()?;
    Ok(FactoryExecutionInputDisposition::Pending)
  }
  async fn publish_factory_execution_inputs(&self, request: PublishFactoryExecutionInputs) -> Result<(), StoreError> {
    request.validate()
  }
}

struct Fixture {
  lease: LeaseAccess,
  runs: Arc<InMemoryFactoryConfigurationStore>,
  query: Queries,
  artifacts: Arc<InMemoryArtifactRecordStore>,
  bytes: Arc<Bytes>,
  profile: FactoryCommandBuildProfile,
  intent: FlowBuildIntent,
  clock: Arc<dyn FactoryExecutionClock>,
  candidate_context: Option<Arc<dyn FactoryRetainedCandidateContextSource>>,
  validation_source: Option<Arc<dyn FactoryProtectedValidationInputSource>>,
}

async fn observed_command_fixture(primary: PrimaryReport) -> Fixture {
  let (_, _, mut profile) = crate::factory_command_tests::command_fixture();
  let contract = crate::factory_command_observation_tests::contract();
  let schema = contract.report_schema().unwrap();
  let original = &profile.outputs[0];
  profile.outputs[0] = EvidenceRequirement::new(
    original.kind().clone(),
    original.output_kind(),
    schema.reference().clone(),
    original.tool().clone(),
    original.plugin().clone(),
  );
  profile.observation_contract = Some(contract);
  prepared_with_report(profile, "application/json", Some(schema), primary, |_| {}).await
}

#[test]
fn qualified_command_observations_retain_positive_negative_and_unresolved_reports() {
  crate::factory_admission_tests::run_ready(async {
    let cases: [(&str, PrimaryReport); 3] = [
      ("matched", |intent, profile| {
        Some(crate::factory_command_observation_tests::report(
          intent,
          profile.observation_contract.as_ref().unwrap(),
        ))
      }),
      ("not_matched", |intent, profile| {
        let mut value =
          crate::factory_command_observation_tests::report(intent, profile.observation_contract.as_ref().unwrap());
        value["observations"]["unexpected_statuses"] = serde_json::json!(0);
        value["finding"] = serde_json::json!("not_matched");
        Some(value)
      }),
      ("indeterminate", |intent, profile| {
        let mut value =
          crate::factory_command_observation_tests::report(intent, profile.observation_contract.as_ref().unwrap());
        value["status"] = serde_json::json!("environment_unavailable");
        value["finding"] = serde_json::json!("indeterminate");
        value.as_object_mut().unwrap().remove("observations");
        Some(value)
      }),
    ];
    for (expected, primary) in cases {
      let fixture = observed_command_fixture(primary).await;
      let FactoryNodeBuildOutputObservation::Verified(outputs) = fixture
        .source(fixture.query.clone())
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 4000)
        .await
        .unwrap()
      else {
        panic!("the completed qualified command must retain its independently evaluated report");
      };
      let document = &outputs.documents[0];
      let value = document.data.as_ref().unwrap().payload().value();
      assert_eq!(value["finding"], expected);
      assert_eq!(
        value["input_digest"],
        serde_json::json!(fixture.intent.input().digest().unwrap())
      );
      assert_eq!(document.record.identity().build_id, fixture.query.build.build.id);
      assert_eq!(document.record.identity().attempt_id, fixture.query.attempt.id);
      assert_eq!(document.record.identity().job_id, fixture.query.attempt.jobs[0].id());
      assert_eq!(document.tool, fixture.profile.executor);
      assert_eq!(outputs.execution.profile.task_digest, fixture.profile.task_digest());
      assert_eq!(
        document.schema,
        fixture
          .profile
          .observation_contract
          .as_ref()
          .unwrap()
          .report_schema()
          .unwrap()
          .reference()
          .clone()
      );
    }
  });
}

#[test]
fn failed_reproduction_commands_remain_technical_stops_even_when_a_report_was_published() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = observed_command_fixture(|intent, profile| {
      Some(crate::factory_command_observation_tests::report(
        intent,
        profile.observation_contract.as_ref().unwrap(),
      ))
    })
    .await;
    for timed_out in [true, false] {
      let mut query = fixture.query.clone();
      query.build.state = BuildState::Failed;
      query.attempt.state = AttemptState::Failed;
      query.attempt.jobs[0].state = JobState::Failed;
      let terminal = query.attempt.jobs[0].terminal.as_mut().unwrap();
      terminal.state = JobState::Failed;
      terminal.timed_out = timed_out;
      terminal.failure_class = Some(octacity_server_job::JobFailureClass::Execution);
      let observed = fixture
        .source(query)
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 4000)
        .await
        .unwrap();
      let FactoryNodeBuildOutputObservation::ExecutionStopped { usage } = observed else {
        panic!("timeout, checkout/dependency or generic execution failure cannot establish a symptom finding");
      };
      assert!(usage.elapsed_millis > 0);
      assert!(
        usage.output_bytes > 0,
        "retain the real published report cost without accepting its semantic claim"
      );
    }
  });
}

#[test]
fn configured_command_observation_cannot_accept_a_schema_valid_unbound_report() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = observed_command_fixture(|intent, profile| {
      let mut value =
        crate::factory_command_observation_tests::report(intent, profile.observation_contract.as_ref().unwrap());
      value["input_digest"] = serde_json::json!("ab".repeat(32));
      Some(value)
    })
    .await;
    assert!(
      matches!(
        fixture
          .source(fixture.query.clone())
          .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 4000)
          .await,
        Err(OrdinaryBuildApplicationError::Invalid)
      ),
      "ordinary publication/schema validity alone cannot qualify a bound command observation"
    );
  });
}

#[test]
fn command_success_with_terminal_failure_facts_cannot_supply_semantic_evidence() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared(profile).await;
    for timed_out in [true, false] {
      let mut query = fixture.query.clone();
      let terminal = query.attempt.jobs[0].terminal.as_mut().unwrap();
      terminal.timed_out = timed_out;
      if !timed_out {
        terminal.failure_class = Some(octacity_server_job::JobFailureClass::Execution);
      }
      assert!(
        matches!(
          fixture
            .source(query)
            .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
            .await,
          Err(OrdinaryBuildApplicationError::Invalid)
        ),
        "contradictory successful/failed terminal facts cannot qualify an observation"
      );
    }
  });
}

#[test]
fn retained_command_json_uses_installed_schema_and_charges_server_wall_time() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared(profile).await;
    let observation = fixture
      .source(fixture.query.clone())
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap();
    let FactoryNodeBuildOutputObservation::Verified(outputs) = observation else {
      panic!("ordinary successful command JSON must be verified");
    };
    assert_eq!(outputs.documents.len(), 1);
    assert_eq!(outputs.documents[0].bytes, br#"{"count":9}"#);
    assert_eq!(
      outputs.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert_eq!(outputs.documents[0].schema, fixture.profile.outputs[0].schema().clone());
    assert_eq!(outputs.execution.input_digest, fixture.intent.input().digest().unwrap());
    // Server time includes queueing and cleanup; provider JSON cannot understate it.
    assert_eq!(
      outputs.usage,
      BudgetUsage {
        attempts: 1,
        elapsed_millis: 18,
        tokens: 0,
        cost_micro_units: 0,
        output_bytes: 11
      }
    );
    assert_eq!(outputs.verified_at, at(32));
    assert_eq!(
      fixture
        .source(fixture.query.clone())
        .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
        .await
        .unwrap(),
      BudgetUsage {
        attempts: 0,
        elapsed_millis: 18,
        tokens: 0,
        cost_micro_units: 0,
        output_bytes: 11
      }
    );
  });
}
impl Fixture {
  fn source(
    &self,
    query: Queries,
  ) -> FactoryCommandBuildOutputSource<InMemoryFactoryConfigurationStore, Queries, InMemoryArtifactRecordStore, Bytes>
  {
    let source = FactoryCommandBuildOutputSource::new(
      self.runs.clone(),
      Arc::new(query),
      self.artifacts.clone(),
      self.bytes.clone(),
      vec![self.profile.clone()],
      self.clock.clone(),
    )
    .unwrap();
    let source = if let Some(context) = &self.candidate_context {
      source.with_retained_candidate_context_source(context.clone())
    } else {
      source
    };
    if let Some(validation) = &self.validation_source {
      source.with_protected_validation_input_source(validation.clone())
    } else {
      source
    }
  }
}

pub(crate) fn configure_profile(profile: &mut FactoryCommandBuildProfile, work: &WorkEnvelope) {
  let mut grants = serde_json::to_value(crate::factory_job_test_support::permissions(false)).unwrap();
  grants["plugins"] = serde_json::json!([profile.plugin]);
  grants["executables"] = serde_json::json!([]);
  grants["commands"] = serde_json::json!([]);
  grants["network_hosts"] = serde_json::json!([]);
  grants["secret_profiles"] = serde_json::json!([]);
  grants["outputs"]["kinds"] = serde_json::json!([profile.outputs[0].kind()]);
  for mount in grants["mounts"].as_array_mut().unwrap() {
    if mount["root"] == "/workspace/source" {
      mount["mode"] = serde_json::json!("read_only");
    }
  }
  profile.project_permissions = serde_json::from_value(grants).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
  profile.build_configuration = BuildConfigurationRef::new(
    profile.build_configuration.id(),
    profile.build_configuration.version(),
    work.subject().project_id(),
    profile.build_configuration.definition_digest(),
  );
}

async fn prepared(profile: FactoryCommandBuildProfile) -> Fixture {
  prepared_with_media(profile, "application/json").await
}
async fn prepared_with_media(profile: FactoryCommandBuildProfile, media_type: &str) -> Fixture {
  prepared_with_profile(profile, media_type, |_| {}).await
}
async fn prepared_with_profile(
  profile: FactoryCommandBuildProfile,
  media_type: &str,
  configure: impl FnOnce(&mut FactoryCommandBuildProfile),
) -> Fixture {
  prepared_with_report(profile, media_type, None, count_report, configure).await
}

type PrimaryReport = fn(&FlowBuildIntent, &FactoryCommandBuildProfile) -> Option<serde_json::Value>;
fn count_report(_: &FlowBuildIntent, _: &FactoryCommandBuildProfile) -> Option<serde_json::Value> {
  Some(serde_json::json!({"count":9}))
}

async fn prepared_with_report(
  mut profile: FactoryCommandBuildProfile,
  media_type: &str,
  output_schema: Option<FlowDataSchema>,
  primary: PrimaryReport,
  configure: impl FnOnce(&mut FactoryCommandBuildProfile),
) -> Fixture {
  let work = fixtures::work();
  configure_profile(&mut profile, &work);
  configure(&mut profile);
  let mut binding = fixtures::binding(work.subject().project_id());
  binding.tool = profile.executor.clone();
  binding.plugin = profile.plugin.clone();
  binding.model_or_tool = profile.identity.clone();
  binding.build_configuration = profile.build_configuration.clone();
  binding.task_digest = profile.task_digest();
  binding.result_output = profile.outputs[0].kind().clone();
  binding.evidence = profile.outputs[1..].to_vec();
  let (_, admitted, schema, input, _) = fixtures::execution_fixture_with_permissions(
    FlowNodeKind::BuildCommand,
    work.clone(),
    binding,
    output_schema,
    profile.project_permissions.clone(),
  );
  assert_eq!(schema.reference(), profile.outputs[0].schema());
  let runs = Arc::new(InMemoryFactoryConfigurationStore::new());
  let run = FactoryRun::admitted(input.factory_run_id(), &work);
  let audit = management_mutation_with_request((), "command-observation")
    .audit()
    .clone();
  runs
    .seed_factory_run(
      PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: admitted.clone(),
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([31; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    fixtures::key("ordinary-worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([32; 32])),
      at(5),
      at(500),
    )
    .unwrap(),
  );
  runs
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        fixtures::key("factory.claimed"),
        claim.id,
        fixtures::key("claimed"),
        at(5),
      ),
    })
    .await
    .unwrap();
  FactoryNodeRunner::new(runs.clone(), Arc::new(NeverDispatch), Arc::new(PreparationClock))
    .run_once(
      run.id(),
      claim.id,
      FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      },
      Some(input),
      at(6),
    )
    .await
    .unwrap();
  let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
  let intent = snapshot.flow.data.build_intents[0].clone();
  finish_prepared_with_primary(profile, runs, admitted, intent, None, media_type, primary).await
}

enum FixtureSource {
  Candidate(FactoryCandidateContext),
  Validation(Box<FactoryProtectedValidationMaterialization>),
  Promotion(FactoryCandidateContext),
  Deployment(
    Box<(
      FactoryCandidateContext,
      FactoryDeploymentCommandProfile,
      JobSpecToolchainPolicy,
    )>,
  ),
}

async fn finish_prepared(
  profile: FactoryCommandBuildProfile,
  runs: Arc<InMemoryFactoryConfigurationStore>,
  admitted: AdmittedFlow,
  intent: FlowBuildIntent,
  source: Option<FixtureSource>,
  media_type: &str,
) -> Fixture {
  finish_prepared_with_primary(profile, runs, admitted, intent, source, media_type, count_report).await
}
async fn finish_prepared_with_primary(
  profile: FactoryCommandBuildProfile,
  runs: Arc<InMemoryFactoryConfigurationStore>,
  admitted: AdmittedFlow,
  intent: FlowBuildIntent,
  source: Option<FixtureSource>,
  media_type: &str,
  primary: PrimaryReport,
) -> Fixture {
  let offset = intent.node().started_at().unix_millis() - 6;
  let mut compiled = match &source {
    Some(FixtureSource::Deployment(deployment)) => {
      compile_factory_deployment_command_task(&intent, &admitted, &deployment.1, &deployment.2).unwrap()
    }
    _ => compile_factory_command_task(&intent, &admitted, &profile).unwrap(),
  };
  compiled = match &source {
    Some(FixtureSource::Candidate(context)) => compiled
      .with_candidate(context.candidate(), intent.node().started_at())
      .unwrap(),
    Some(FixtureSource::Validation(validation)) => compiled.with_protected_validation(validation).unwrap(),
    Some(FixtureSource::Promotion(context)) => compiled
      .with_candidate(context.candidate(), intent.node().started_at())
      .unwrap()
      .with_promotion(&PromotionPlan::freeze(&admitted, intent.input()).unwrap(), &admitted)
      .unwrap(),
    Some(FixtureSource::Deployment(deployment)) => compiled
      .with_candidate(deployment.0.candidate(), intent.node().started_at())
      .unwrap(),
    None => compiled,
  };
  let bytes = Arc::new(Bytes::default());
  let mut manifest = FactoryExecutionInputPublisher::new(
    Arc::new(Inputs),
    bytes.clone(),
    Arc::new(ReadClock(intent.node().started_at().unix_millis())),
  )
  .publish(
    &intent,
    &FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
    compiled.documents().to_vec(),
  )
  .await
  .unwrap();
  manifest.inputs.extend_from_slice(compiled.additional_inputs());
  manifest.inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
  let program = compiled.managed_program(manifest).unwrap();
  let base = authoritative_store_contract_fixture();
  let mut build = base.request.build;
  build.project_id = intent.input().subject().project_id();
  build.repository_id = intent.input().subject().repository_id();
  build.configuration_id = profile.build_configuration.id();
  build.configuration_version = profile.build_configuration.version();
  build.immutable_revision = intent.input().subject().base_revision().clone();
  if let Some(FixtureSource::Deployment(deployment)) = &source {
    let context = crate::factory_command_materializer_tests::deployment_context(&profile, &intent, &deployment.2);
    build.effective_policy_snapshot = serde_json::to_value(crate::snapshots::BuildPolicySnapshot {
      project: context.effective_policy,
      job_spec_toolchain: context.job_spec_toolchain,
    })
    .unwrap();
  }
  let mut job = base.request.jobs[0].clone();
  job.pipeline_node_id = profile.pipeline_node.clone();
  let mut canonical = crate::factory_job_test_support::canonical_v3_spec();
  canonical.runtime.network = match &source {
    Some(FixtureSource::Deployment(deployment)) => {
      canonical.octa = deployment.2.octa.clone();
      octacity_protocol::NetworkPolicy::Restricted {
        allowed_hosts: profile
          .project_permissions
          .network_hosts()
          .map(|host| host.as_str().to_owned())
          .collect(),
      }
    }
    _ => octacity_protocol::NetworkPolicy::Disabled,
  };
  canonical.octa.plugin_digests = BTreeMap::from([(
    profile.plugin.identity().to_string(),
    profile.plugin.digest().to_string(),
  )]);
  job.job_spec_template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: JobSpecBuildSnapshot::new(
      build.id,
      build.immutable_revision.clone(),
      None,
      RepositoryLocator::new("https://example.test/ordinary.git").unwrap(),
      BTreeMap::new(),
    )
    .unwrap(),
    pipeline_node_id: profile.pipeline_node.clone(),
    execution: program.execution,
    protected_inputs: program.protected_inputs,
    required_enforcement: program.required_enforcement,
    causality: FactoryCausalityV3 {
      factory_run_id: intent.input().factory_run_id().to_string(),
      factory_configuration_id: intent.input().configuration().id().to_string(),
      factory_configuration_version: intent.input().configuration().version().get(),
      flow_run_id: intent.input().flow_run_id().to_string(),
      workflow_cycle_id: intent.input().cycle_id().to_string(),
      node_attempt_id: intent.node().id().to_string(),
      input_digest: intent.input().digest().unwrap().to_string(),
      subject_digest: intent.input().subject().digest().to_string(),
      source_write_paths: None,
      change_set_capture: None,
      change_set_materialization: match &source {
        Some(FixtureSource::Candidate(context) | FixtureSource::Promotion(context)) => Some(
          context
            .candidate()
            .materialization(intent.input(), intent.node().started_at())
            .unwrap()
            .wire_inputs()
            .0,
        ),
        Some(FixtureSource::Deployment(deployment)) => Some(
          deployment
            .0
            .candidate()
            .materialization(intent.input(), intent.node().started_at())
            .unwrap()
            .wire_inputs()
            .0,
        ),
        Some(FixtureSource::Validation(validation)) => Some(validation.wire_inputs(&[]).unwrap().0),
        None => None,
      },
    },
    policy_layers: FactoryBuildPolicyLayers {
      project: profile.project_permissions.clone(),
      configuration: profile.project_permissions.clone(),
      task: profile.project_permissions.clone(),
      local: profile.local_permissions.clone(),
    },
    policy: crate::factory_job_test_support::policy(&canonical),
  })
  .unwrap()
  .with_execution_deadline(intent.node().deadline())
  .unwrap();
  let job_id = job.id;
  let query = Queries {
    build: BuildRecord {
      build: build.clone(),
      state: BuildState::Succeeded,
      version: BuildVersion::new(1).unwrap(),
      origin: BuildOrigin::Factory(FactoryBuildOrigin::from_intent(&intent).unwrap()),
      created_at: at(10 + offset),
      updated_at: at(30 + offset),
    },
    attempt: AttemptRecord {
      id: base.request.attempt_id,
      build_id: build.id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state: AttemptState::Succeeded,
      version: AttemptVersion::new(1).unwrap(),
      created_at: at(10 + offset),
      updated_at: at(30 + offset),
      jobs: vec![JobRecord {
        attempt_id: base.request.attempt_id,
        job,
        state: JobState::Succeeded,
        version: JobVersion::new(1).unwrap(),
        created_at: at(12 + offset),
        updated_at: at(30 + offset),
        queue: None,
        assignment: Some(JobAssignmentRecord {
          pool_id: base.allowed_pool,
          agent_id: base.agent_id,
        }),
        terminal: Some(JobTerminalRecord {
          state: JobState::Succeeded,
          failure_class: None,
          timed_out: false,
          completed_at: at(30 + offset),
        }),
        event_cursor: 0,
      }],
    },
    history: BTreeMap::new(),
  };
  let lease = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([33; 32]),
    agent_id: base.agent_id,
    registration_epoch: base.registration_epoch,
  };
  let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access: lease,
    build_id: build.id,
    attempt_id: base.request.attempt_id,
    attempt: AttemptNumber::FIRST,
    job_id,
    expires_at: at(500),
  }));
  if let Some(value) = primary(&intent, &profile) {
    let content = serde_json::to_vec(&value).unwrap();
    let artifact_id = ArtifactId::generate();
    let upload = artifacts
      .begin_artifact_upload(BeginArtifactUpload {
        artifact_id,
        upload_id: ArtifactUploadId::generate(),
        idempotency_key: IdempotencyKey::new("ordinary-output").unwrap(),
        lease,
        job_id,
        attempt: AttemptNumber::FIRST,
        logical_name: ArtifactName::new(intent.profile().result_output.as_str()).unwrap(),
        producer_run_id: 1,
        producer_task_id: 1,
        artifact_type: ArtifactType::Report(ArtifactReportFormat::new("operator-json-v1").unwrap()),
        media_type: ArtifactMediaType::new(media_type).unwrap(),
        transport_media_type: ArtifactMediaType::new(media_type).unwrap(),
        size_bytes: content.len() as u64,
        digest: ArtifactContentDigest::from_lower_hex(&FactoryDigest::content_sha256(&content).to_string()).unwrap(),
        reserved_at: at(31 + offset),
        capability_expires_at: at(400),
      })
      .await
      .unwrap();
    let verify = VerifyArtifactUpload {
      upload_id: upload.upload.upload_id,
      lease,
      job_id,
      attempt: AttemptNumber::FIRST,
      observed_at: at(32 + offset),
    };
    artifacts.begin_artifact_verification(verify).await.unwrap();
    artifacts
      .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
      .await
      .unwrap();
    bytes.0.lock().unwrap().insert(artifact_id, content);
  }
  Fixture {
    lease,
    runs,
    query,
    artifacts,
    bytes,
    profile,
    intent,
    clock: Arc::new(ReadClock(50 + offset)),
    candidate_context: None,
    validation_source: None,
  }
}
#[test]
fn command_usage_includes_a_failed_retry_without_needing_a_json_report_from_it() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared(profile).await;
    let mut query = fixture.query.clone();
    query.build.created_at = at(6);
    let mut previous = query.attempt.clone();
    previous.id = AttemptId::generate();
    previous.state = AttemptState::Failed;
    previous.created_at = at(6);
    previous.updated_at = at(10);
    previous.jobs[0].attempt_id = previous.id;
    previous.jobs[0].job.id = JobId::generate();
    previous.jobs[0].created_at = at(6);
    previous.jobs[0].updated_at = at(10);
    previous.jobs[0].state = JobState::Failed;
    previous.jobs[0].terminal = Some(JobTerminalRecord {
      state: JobState::Failed,
      failure_class: Some(octacity_server_job::JobFailureClass::Execution),
      timed_out: false,
      completed_at: at(10),
    });
    query.attempt.number = AttemptNumber::new(2).unwrap();
    query.attempt.retry_of_attempt_id = Some(previous.id);
    query.history.insert(previous.id, previous);
    let source = fixture.source(query);
    let usage = source
      .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
      .await
      .unwrap();
    assert_eq!(
      usage,
      BudgetUsage {
        attempts: 0,
        elapsed_millis: 22,
        output_bytes: 11,
        tokens: 0,
        cost_micro_units: 0
      }
    );
    let FactoryNodeBuildOutputObservation::Verified(outputs) = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("successful latest retry retains the generic JSON result");
    };
    assert_eq!(outputs.usage.elapsed_millis, 22);
    assert_eq!(
      outputs.usage.attempts, 1,
      "ordinary retries must not double charge Node Attempt creation"
    );
  });
}
#[test]
fn cancellation_blocks_semantic_output_but_preserves_ordinary_terminal_consumption() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared(profile).await;
    let run_id = fixture.intent.input().factory_run_id();
    let snapshot = fixture.runs.factory_run_snapshot(run_id).await.unwrap();
    FactoryControlHandlers::new(fixture.runs.clone())
      .execute_management_command(
        &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
        &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
        CancelFactoryRunCommand {
          run_id,
          expected_version: snapshot.run.version(),
          idempotency_key: IdempotencyKey::new("command-cancelled-observation").unwrap(),
          requested_at: at(40),
        },
      )
      .await
      .unwrap();
    let source = fixture.source(fixture.query.clone());
    assert!(matches!(
      source
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
    assert_eq!(
      source
        .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
        .await
        .unwrap(),
      BudgetUsage {
        attempts: 0,
        elapsed_millis: 18,
        output_bytes: 11,
        tokens: 0,
        cost_micro_units: 0
      }
    );
  });
}
#[test]
fn cancellation_accepted_during_command_output_io_discards_completion_without_refunding_consumption() {
  struct CancelDuringRead {
    bytes: Arc<Bytes>,
    runs: Arc<InMemoryFactoryConfigurationStore>,
    run_id: FactoryRunId,
  }
  #[async_trait]
  impl ArtifactByteReader for CancelDuringRead {
    async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
      let bytes = self.bytes.read_published(object, maximum).await?;
      let snapshot = self.runs.factory_run_snapshot(self.run_id).await.unwrap();
      FactoryControlHandlers::new(self.runs.clone())
        .execute_management_command(
          &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
          &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
          CancelFactoryRunCommand {
            run_id: self.run_id,
            expected_version: snapshot.run.version(),
            idempotency_key: IdempotencyKey::new("cancel-during-command-output-read").unwrap(),
            requested_at: at(40),
          },
        )
        .await
        .unwrap();
      Ok(bytes)
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared(profile).await;
    let source = FactoryCommandBuildOutputSource::new(
      fixture.runs.clone(),
      Arc::new(fixture.query.clone()),
      fixture.artifacts.clone(),
      Arc::new(CancelDuringRead {
        bytes: fixture.bytes.clone(),
        runs: fixture.runs.clone(),
        run_id: fixture.intent.input().factory_run_id(),
      }),
      vec![fixture.profile.clone()],
      fixture.clock.clone(),
    )
    .unwrap();
    let error = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .err()
      .expect("cancellation accepted during Artifact I/O must stop semantic completion");
    assert_eq!(error, OrdinaryBuildApplicationError::Invalid);
    assert_eq!(error.to_string(), "ordinary Build request is invalid");
    assert_eq!(
      source
        .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
        .await
        .unwrap(),
      BudgetUsage {
        attempts: 0,
        elapsed_millis: 18,
        output_bytes: 11,
        tokens: 0,
        cost_micro_units: 0,
      }
    );
    assert_eq!(
      fixture
        .query
        .job(fixture.query.attempt.jobs[0].id())
        .await
        .unwrap()
        .state,
      JobState::Succeeded,
      "a discarded Factory completion cannot rewrite the authoritative ordinary Job outcome"
    );
  });
}

#[test]
fn retained_command_output_rejects_a_template_signed_for_another_ordinary_build() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared(profile).await;
    let mut query = fixture.query.clone();
    let mut template = serde_json::to_value(&query.attempt.jobs[0].job.job_spec_template).unwrap();
    template["build_id"] = serde_json::to_value(BuildId::generate()).unwrap();
    query.attempt.jobs[0].job.job_spec_template = serde_json::from_value(template).unwrap();
    assert!(matches!(
      fixture
        .source(query)
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
  });
}

#[test]
fn retained_command_candidate_outputs_match_the_exact_persisted_source_and_compiler_inputs() {
  crate::factory_admission_tests::run_ready(async {
    let work = fixtures::work();
    let (_, _, mut profile) = crate::factory_command_tests::command_fixture();
    configure_profile(&mut profile, &work);
    profile.candidate_capture_tool = Some(fixtures::reference("git.capture"));
    let mut binding = fixtures::binding(work.subject().project_id());
    binding.tool = profile.executor.clone();
    binding.plugin = profile.plugin.clone();
    binding.model_or_tool = profile.identity.clone();
    binding.build_configuration = profile.build_configuration.clone();
    binding.task_digest = profile.task_digest();
    binding.result_output = profile.outputs[0].kind().clone();
    let candidate = crate::factory_candidate_context_test_support::candidate_context_fixture_with_consumer(
      crate::factory_candidate_context_test_support::ReadonlyCandidateFixtureProfile {
        build_configuration: profile.build_configuration.clone(),
        work,
        permissions: profile.project_permissions.clone(),
        toolchain: FactoryTaskToolchain {
          plugin: profile.plugin.clone(),
          executable: profile.executor.clone(),
          model: profile.identity.clone(),
        },
        credentials: crate::factory_codex_tests::credential_profiles(),
        instructions: FactoryText::new("Only the declared retained candidate is selected.").unwrap(),
      },
      Some((FlowNodeKind::BuildCommand, binding)),
    )
    .await;
    let source = Arc::new(FactoryStoredCandidateContextSource::new(
      candidate.runs.clone(),
      candidate.source.clone(),
      profile.candidate_capture_tool.clone().unwrap(),
      Arc::new(ReadClock(76)),
    ));
    let mut fixture = finish_prepared(
      profile,
      candidate.runs,
      candidate.admitted,
      candidate.intent,
      Some(FixtureSource::Candidate(candidate.context)),
      "application/json",
    )
    .await;
    fixture.candidate_context = Some(source);
    let managed = fixture.query.attempt.jobs[0]
      .job
      .job_spec_template
      .managed_intent()
      .unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 4);
    assert_eq!(
      managed
        .factory()
        .change_set_materialization
        .as_ref()
        .unwrap()
        .candidate_revision,
      "0123456789abcdef0123456789abcdef0123456789"
    );
    let FactoryNodeBuildOutputObservation::Verified(outputs) = fixture
      .source(fixture.query.clone())
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("exact retained candidate command result must be independently verified");
    };
    assert_eq!(outputs.execution.input_digest, fixture.intent.input().digest().unwrap());
    assert_eq!(
      outputs.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert_eq!(outputs.usage.elapsed_millis, 18);
  });
}

#[test]
fn command_report_accepts_the_actual_ordinary_octet_stream_transport_with_installed_json_schema() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared_with_media(profile, "application/octet-stream").await;
    let FactoryNodeBuildOutputObservation::Verified(outputs) = fixture
      .source(fixture.query.clone())
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("ordinary Report transport must not select or replace the installed JSON contract");
    };
    assert_eq!(
      outputs.documents[0].record.identity().media_type.as_str(),
      "application/octet-stream"
    );
    assert_eq!(
      outputs.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert_eq!(outputs.documents[0].schema, fixture.profile.outputs[0].schema().clone());
  });
}

#[test]
fn private_credential_commands_cannot_claim_hermetic_zero_model_consumption() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, profile) = crate::factory_command_tests::command_fixture();
    let fixture = prepared_with_profile(profile, "application/json", |profile| {
      let purpose = FactoryKey::new("private-command").unwrap();
      profile.program.credential_profile = Some(purpose.clone());
      let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
      grants["secret_profiles"] = serde_json::json!([purpose]);
      profile.project_permissions = serde_json::from_value(grants).unwrap();
      profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
    })
    .await;
    assert!(matches!(
      fixture
        .source(fixture.query.clone())
        .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
  });
}

#[test]
fn retained_command_validation_outputs_verify_all_private_and_compiler_inputs_without_exposing_test_bodies() {
  crate::factory_admission_tests::run_ready(async {
    let configured = std::cell::RefCell::new(None);
    let validation = crate::factory_protected_validation_input_source_tests::retained_validation_fixture(
      false,
      6,
      |work, binding, permissions| {
        let (_, _, mut profile) = crate::factory_command_tests::command_fixture();
        configure_profile(&mut profile, work);
        profile.candidate_capture_tool = Some(fixtures::reference("git.capture"));
        binding.tool = profile.executor.clone();
        binding.plugin = profile.plugin.clone();
        binding.model_or_tool = profile.identity.clone();
        binding.build_configuration = profile.build_configuration.clone();
        binding.result_output = profile.outputs[0].kind().clone();
        binding.task_digest = profile.task_digest();
        *permissions = profile.project_permissions.clone();
        *configured.borrow_mut() = Some(profile);
      },
    )
    .await;
    let profile = configured.into_inner().unwrap();
    let mut fixture = finish_prepared(
      profile,
      validation.runs,
      validation.admitted,
      validation.intent,
      Some(FixtureSource::Validation(Box::new(validation.materialized))),
      "application/octet-stream",
    )
    .await;
    fixture.validation_source = Some(validation.source);
    let managed = fixture.query.attempt.jobs[0]
      .job
      .job_spec_template
      .managed_intent()
      .unwrap();
    assert_eq!(
      managed.protected_inputs().inputs.len(),
      7,
      "three immutable compiler documents plus the exact four retained source/test inputs"
    );
    assert!(
      managed
        .protected_inputs()
        .inputs
        .iter()
        .any(|input| input.artifact_id == validation.private_file.to_string())
    );
    assert!(
      fixture.bytes.0.lock().unwrap().values().all(|bytes| !bytes
        .windows(b"PRIVATE_EXPECTATION".len())
        .any(|part| part == b"PRIVATE_EXPECTATION")),
      "publishing immutable invocation metadata must not republish private test bodies"
    );
    let FactoryNodeBuildOutputObservation::Verified(outputs) = fixture
      .source(fixture.query.clone())
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("exact retained validation command output must be verified");
    };
    assert_eq!(
      outputs.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert_eq!(outputs.execution.input_digest, fixture.intent.input().digest().unwrap());
    let mut query = fixture.query.clone();
    let mut changed = serde_json::to_value(&query.attempt.jobs[0].job.job_spec_template).unwrap();
    let private_input = changed["execution"]["protected_inputs"]["inputs"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|input| input["artifact_id"] == validation.private_file.to_string())
      .unwrap();
    private_input["sha256"] = serde_json::json!(FactoryDigest::content_sha256(b"different private bytes").to_string());
    query.attempt.jobs[0].job.job_spec_template = serde_json::from_value(changed).unwrap();
    query.attempt.jobs[0].job.job_spec_template.validate().unwrap();
    assert!(matches!(
      fixture
        .source(query)
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
  });
}

/// The existing capture journal is reused; only the installed consumer and its
/// declared disposition criterion come from the public promotion contract.
pub(crate) async fn retained_promotion_command_fixture() -> (
  FactoryCommandBuildProfile,
  crate::factory_candidate_context_test_support::CandidateContextFixture,
) {
  retained_promotion_command_fixture_with_profile(|_, _| {}).await
}

pub(crate) async fn retained_promotion_command_fixture_with_profile(
  configure: impl FnOnce(&mut FactoryCommandBuildProfile, &mut FactoryPermissionSet),
) -> (
  FactoryCommandBuildProfile,
  crate::factory_candidate_context_test_support::CandidateContextFixture,
) {
  let work = fixtures::work();
  let (_, _, mut profile) = crate::factory_command_tests::command_fixture();
  profile.executor = profile.plugin.clone();
  profile.outputs = profile
    .outputs
    .iter()
    .map(|output| {
      EvidenceRequirement::new(
        output.kind().clone(),
        output.output_kind(),
        output.schema().clone(),
        profile.executor.clone(),
        profile.plugin.clone(),
      )
    })
    .collect();
  configure_profile(&mut profile, &work);
  let mut consumer_permissions = profile.project_permissions.clone();
  configure(&mut profile, &mut consumer_permissions);
  profile.candidate_capture_tool = Some(fixtures::reference("git.capture"));
  let mut binding = fixtures::binding(work.subject().project_id());
  binding.tool = profile.executor.clone();
  binding.plugin = profile.plugin.clone();
  binding.model_or_tool = profile.identity.clone();
  binding.build_configuration = profile.build_configuration.clone();
  binding.task_digest = profile.task_digest();
  binding.result_output = profile.outputs[0].kind().clone();
  let (_, promotion, _, input, _) = fixtures::promotion_execution_fixture();
  let definition = promotion.closure().definition(input.definition()).unwrap();
  let original_action = definition.node(input.node()).unwrap().action().unwrap();
  let action = FlowActionBinding::new(
    profile.executor.clone(),
    original_action.action().clone(),
    original_action.parameters().clone(),
  )
  .unwrap();
  let policy: PromotionPolicy = serde_json::from_value(action.parameters()["plan"].clone()).unwrap();
  let mut criterion = serde_json::to_value(definition.node(&policy.criteria).unwrap()).unwrap();
  criterion["permissions"] = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
  let criterion: FlowNodeDefinition = serde_json::from_value(criterion).unwrap();
  let candidate = crate::factory_candidate_context_test_support::candidate_context_fixture_with_definition(
    crate::factory_candidate_context_test_support::ReadonlyCandidateFixtureProfile {
      build_configuration: profile.build_configuration.clone(),
      work,
      permissions: consumer_permissions,
      toolchain: FactoryTaskToolchain {
        plugin: profile.plugin.clone(),
        executable: profile.executor.clone(),
        model: profile.identity.clone(),
      },
      credentials: crate::factory_codex_tests::credential_profiles(),
      instructions: FactoryText::new("Select only the retained exact promotion candidate.").unwrap(),
    },
    Some((FlowNodeKind::BuildCommand, binding)),
    |definition| {
      let mut consumer = serde_json::to_value(&definition.nodes[1]).unwrap();
      consumer["kind"] = serde_json::json!("trusted_action");
      consumer["action"] = serde_json::to_value(action).unwrap();
      let consumer: FlowNodeDefinition = serde_json::from_value(consumer).unwrap();
      let terminal = definition.terminals[0].key().clone();
      definition.transitions[1] = FlowTransition::new(
        consumer.key().clone(),
        consumer.outcomes()[0].key().clone(),
        FlowTransitionTarget::Node(criterion.key().clone()),
      );
      definition.transitions.push(FlowTransition::new(
        criterion.key().clone(),
        criterion.outcomes()[0].key().clone(),
        FlowTransitionTarget::Terminal(terminal),
      ));
      definition.nodes[1] = consumer;
      definition.nodes.push(criterion);
    },
  )
  .await;
  (profile, candidate)
}

struct ProjectionOnlyContext;
#[async_trait]
impl BuildExecutionContextSource for ProjectionOnlyContext {
  async fn load(
    &self,
    _: ProjectId,
    _: BuildConfigurationId,
    _: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
    panic!("trusted action projection must not load or mutate ordinary Build context");
  }
}

#[test]
fn explicit_promotion_commands_sign_and_verify_the_original_rollout_plan_with_retained_candidate_inputs() {
  crate::factory_admission_tests::run_ready(async {
    let (profile, candidate) = retained_promotion_command_fixture().await;
    let plan = PromotionPlan::freeze(&candidate.admitted, candidate.intent.input()).unwrap();
    let action_input = serde_json::json!({"plan":plan,"plan_digest":plan.digest().unwrap()});
    let compiler = FactoryCommandBuildMaterializer::new(
      Arc::new(ProjectionOnlyContext),
      Arc::new(FactoryExecutionInputPublisher::new(
        Arc::new(Inputs),
        Arc::new(Bytes::default()),
        Arc::new(ReadClock(32)),
      )),
      vec![profile.clone()],
      Arc::new(ReadClock(32)),
    )
    .unwrap();
    assert_eq!(
      compiler.action_input(&candidate.intent, &candidate.admitted).unwrap(),
      None
    );
    assert_eq!(
      compiler
        .with_promotion()
        .action_input(&candidate.intent, &candidate.admitted)
        .unwrap(),
      Some(action_input.clone())
    );
    let source = Arc::new(FactoryStoredCandidateContextSource::new(
      candidate.runs.clone(),
      candidate.source.clone(),
      profile.candidate_capture_tool.clone().unwrap(),
      Arc::new(ReadClock(76)),
    ));
    let mut fixture = finish_prepared(
      profile,
      candidate.runs,
      candidate.admitted,
      candidate.intent,
      Some(FixtureSource::Promotion(candidate.context)),
      "application/octet-stream",
    )
    .await;
    fixture.candidate_context = Some(source);
    let managed = fixture.query.attempt.jobs[0]
      .job
      .job_spec_template
      .managed_intent()
      .unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 5);
    let descriptor = managed
      .protected_inputs()
      .inputs
      .iter()
      .find(|input| input.destination == "/octacity/protected/promotion-input.json")
      .unwrap();
    let id: ArtifactId = descriptor.artifact_id.parse().unwrap();
    assert_eq!(
      serde_json::from_slice::<serde_json::Value>(&fixture.bytes.0.lock().unwrap()[&id]).unwrap(),
      action_input
    );
    assert!(matches!(
      fixture
        .source(fixture.query.clone())
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid)
    ));
    let FactoryNodeBuildOutputObservation::Verified(outputs) = fixture
      .source(fixture.query.clone())
      .with_promotion()
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("only the explicit promotion role accepts its original signed plan");
    };
    assert_eq!(outputs.execution.input_digest, fixture.intent.input().digest().unwrap());
  });
}

fn install_author_profile(command: &mut FactoryCommandBuildProfile) -> FactoryProtectedTestAuthoringProfile {
  let inventory = EvidenceRequirement::new(
    fixtures::key("private.inventory"),
    EvidenceOutputKind::Artifact,
    command.outputs[0].schema().clone(),
    command.executor.clone(),
    command.plugin.clone(),
  );
  if !command.outputs.contains(&inventory) {
    command.outputs.push(inventory.clone());
  }
  let mut grants = serde_json::to_value(&command.project_permissions).unwrap();
  grants["outputs"]["kinds"] = serde_json::json!([command.outputs[0].kind(), inventory.kind(), "hidden.contract"]);
  command.project_permissions = serde_json::from_value(grants).unwrap();
  command.local_permissions = LocalPermissionCeiling::fully_enforced(command.project_permissions.clone());
  let limits: FactoryProtectedTestPolicy = serde_json::from_value(serde_json::json!({
    "classes":["custom.check"], "max_files":4, "max_bytes":9000,
    "reserved_inputs":4, "retention_millis":1000
  }))
  .unwrap();
  let policy = FactoryProtectedTestAuthoringPolicy::new(
    limits,
    inventory,
    vec![FactoryProtectedTestFileOutput {
      output: fixtures::key("hidden.contract"),
      class: fixtures::key("custom.check"),
      path: FactoryRepositoryPath::new("contract.rs").unwrap(),
    }],
  )
  .unwrap();
  command.identity = ImmutableReference::new(
    command.identity.identity().clone(),
    command.identity.version().clone(),
    policy.digest().unwrap(),
  );
  FactoryProtectedTestAuthoringProfile::new(command.clone(), policy).unwrap()
}

async fn publish_author_file(fixture: &Fixture, name: &str, body: &[u8]) {
  let lease = fixture.lease;
  let job_id = fixture.query.attempt.jobs[0].id();
  let id = ArtifactId::generate();
  let reservation = fixture
    .artifacts
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: id,
      upload_id: ArtifactUploadId::generate(),
      idempotency_key: IdempotencyKey::new(name).unwrap(),
      lease,
      job_id,
      attempt: AttemptNumber::FIRST,
      logical_name: ArtifactName::new(name).unwrap(),
      producer_run_id: 1,
      producer_task_id: 2,
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      transport_media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      size_bytes: body.len() as u64,
      digest: ArtifactContentDigest::from_lower_hex(&FactoryDigest::content_sha256(body).to_string()).unwrap(),
      reserved_at: at(31),
      capability_expires_at: at(400),
    })
    .await
    .unwrap();
  let verification = VerifyArtifactUpload {
    upload_id: reservation.upload.upload_id,
    lease,
    job_id,
    attempt: AttemptNumber::FIRST,
    observed_at: at(32),
  };
  fixture
    .artifacts
    .begin_artifact_verification(verification)
    .await
    .unwrap();
  fixture
    .artifacts
    .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
  fixture.bytes.0.lock().unwrap().insert(id, body.to_vec());
}

#[test]
fn installed_author_source_retains_exact_private_files_as_opaque_artifacts() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, command) = crate::factory_command_tests::command_fixture();
    let fixture = prepared_with_profile(command, "application/json", |command| {
      install_author_profile(command);
    })
    .await;
    let mut command = fixture.profile.clone();
    let author = install_author_profile(&mut command);
    assert_eq!(command.task_digest(), fixture.profile.task_digest());
    publish_author_file(&fixture, "private.inventory", b"{\"opaque_inventory\":true}").await;
    publish_author_file(&fixture, "hidden.contract", b"PRIVATE_EXPECTATION\0\xff").await;
    assert!(
      fixture
        .source(fixture.query.clone())
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000,)
        .await
        .is_err(),
      "ordinary command observers never acquire private-file authority"
    );
    let source = fixture
      .source(fixture.query.clone())
      .with_protected_test_authoring(vec![author])
      .unwrap();
    let FactoryNodeBuildOutputObservation::Verified(outputs) = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("exact installed author outputs must verify");
    };
    assert_eq!(outputs.documents.len(), 3);
    for name in ["private.inventory", "hidden.contract"] {
      let document = outputs
        .documents
        .iter()
        .find(|document| document.record.identity().logical_name.as_str() == name)
        .unwrap();
      assert!(document.data.is_none(), "private artifacts never become Flow data");
      assert_eq!(document.tool, fixture.profile.executor);
    }
    let private = outputs
      .documents
      .iter()
      .find(|document| document.record.identity().logical_name.as_str() == "hidden.contract")
      .unwrap();
    assert_eq!(private.bytes, b"PRIVATE_EXPECTATION\0\xff");
    let public = outputs
      .documents
      .iter()
      .find(|document| document.record.identity().logical_name.as_str() == fixture.profile.outputs[0].kind().as_str())
      .unwrap();
    assert_eq!(
      public.data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert!(
      source
        .with_promotion()
        .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 1000,)
        .await
        .is_err(),
      "author output authority cannot combine with promotion"
    );
  });
}

// The authoritative accepted contract is prepared through the same common owner
// used in production; only ordinary Build records/object storage are port fakes.
async fn prepared_accepted_author(
  mut profile: FactoryCommandBuildProfile,
) -> (Fixture, FactoryProtectedTestAuthoringProfile, LeaseAccess) {
  let work = fixtures::work();
  configure_profile(&mut profile, &work);
  let public_schema = fixtures::supplied_base_flow_template()
    .data_schemas()
    .unwrap()
    .remove(&fixtures::key("protected_test_handoff"))
    .unwrap();
  let author = install_author_profile(&mut profile);
  let policy = FactoryProtectedTestAuthoringPolicy::new(
    author.policy().clone(),
    author.inventory().clone(),
    author.files().to_vec(),
  )
  .unwrap();
  profile.outputs[0] = EvidenceRequirement::new(
    profile.outputs[0].kind().clone(),
    EvidenceOutputKind::Report,
    public_schema.reference().clone(),
    profile.executor.clone(),
    profile.plugin.clone(),
  );
  profile.program.octafile = FactoryCommandOctafile::new(
    serde_json::json!({"version":1,"tasks":{"observe":{
      "shell":"printf 'private contract' > /workspace/output/contract.rs",
      "artifacts":[{"name":"hidden.contract","path":"/workspace/output/contract.rs"}]
    }}})
    .to_string(),
  )
  .unwrap();
  let author = FactoryProtectedTestAuthoringProfile::new(profile.clone(), policy).unwrap();
  let mut binding = fixtures::binding(work.subject().project_id());
  binding.tool = profile.executor.clone();
  binding.plugin = profile.plugin.clone();
  binding.model_or_tool = profile.identity.clone();
  binding.build_configuration = profile.build_configuration.clone();
  binding.task_digest = profile.task_digest();
  binding.result_output = profile.outputs[0].kind().clone();
  binding.evidence = vec![profile.outputs[1].clone()];
  let (_, base, schema, _, _) = fixtures::execution_fixture_with_permissions(
    FlowNodeKind::Reasoning,
    work.clone(),
    binding,
    Some(public_schema.clone()),
    profile.project_permissions.clone(),
  );
  let original = base.closure().definition(base.closure().root()).unwrap();
  let input_schema = base.data_schema(original.input_schema().unwrap()).unwrap().clone();
  let acceptance = fixtures::key("confirm_contract");
  let author_node = original.nodes()[0]
    .clone()
    .with_input_binding(
      FlowInputBinding::new(
        BTreeMap::from([(
          fixtures::key("accepted"),
          FlowInputSource::Record {
            node: acceptance.clone(),
            outcome: fixtures::key("accepted"),
            view: FlowRecordView::Payload,
            path: String::new(),
          },
        )]),
        FlowDataMapping::new(FlowValueMapping::Pointer {
          path: "/accepted".into(),
        })
        .unwrap(),
      )
      .unwrap(),
    )
    .unwrap();
  let accepted = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: acceptance.clone(),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(input_schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      fixtures::key("accepted"),
      FlowOutcomeKind::Success,
      input_schema.reference().clone(),
    )],
    budget: author_node.budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      BTreeMap::new(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!({"count":3}),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap()
  .with_gate(
    FlowGateProgram::new(vec![], fixtures::key("accepted"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
      .unwrap()
      .binding()
      .unwrap(),
  )
  .unwrap()
  .with_accepted_work_outcomes(vec![fixtures::key("accepted")])
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(input_schema.reference().clone()),
    entry: acceptance.clone(),
    nodes: vec![accepted, author_node.clone()],
    transitions: vec![
      FlowTransition::new(
        acceptance,
        fixtures::key("accepted"),
        FlowTransitionTarget::Node(author_node.key().clone()),
      ),
      FlowTransition::new(
        author_node.key().clone(),
        fixtures::key("observed"),
        FlowTransitionTarget::Terminal(fixtures::key("done")),
      ),
    ],
    terminals: vec![FlowTerminalDefinition::new(
      fixtures::key("done"),
      schema.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: original.execution().clone(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    base.limits().clone(),
    root,
    cycle,
    FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![input_schema, public_schema])
  .unwrap();
  let runs = Arc::new(InMemoryFactoryConfigurationStore::new());
  let audit = management_mutation_with_request((), "accepted-author").audit().clone();
  runs
    .seed_factory_run(
      PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: admitted.clone(),
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([31; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    fixtures::key("owner"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([32; 32])),
      at(5),
      at(500),
    )
    .unwrap(),
  );
  runs
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        fixtures::key("factory.claimed"),
        claim.id,
        fixtures::key("claimed"),
        at(5),
      ),
    })
    .await
    .unwrap();
  let owner = FactoryFlowRunner::new(
    runs.clone(),
    Arc::new(NeverDispatch),
    Arc::new(crate::factory_flow_tests::Inputs),
    Arc::new(PreparationClock),
  );
  let mut intent = None;
  for time in 6..16 {
    owner.reconcile_flow(run.id(), claim.id, at(time)).await.unwrap();
    let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
    if let Some(row) = snapshot.flow.data.build_intents.first() {
      intent = Some(row.clone());
      break;
    }
  }
  let intent = intent.expect("the configured accepted contract must reach the author");
  let mut f =
    finish_prepared_with_primary(profile, runs, admitted, intent, None, "application/json", |_, _| None).await;
  f.query.build.state = BuildState::Running;
  f.query.attempt.state = AttemptState::Running;
  f.query.attempt.jobs[0].state = JobState::Running;
  f.query.attempt.jobs[0].terminal = None;
  publish_author_file(&f, "hidden.contract", b"assert_eq!(actual, PRIVATE_EXPECTATION);").await;
  let lease = f.lease;
  (f, author, lease)
}

#[test]
fn protected_author_completion_resolves_the_actual_signed_job_before_publication() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, command) = crate::factory_command_tests::command_fixture();
    let (f, author, lease) = prepared_accepted_author(command).await;
    let completion = AgentOutputFinalization {
      lease,
      job_id: f.query.attempt.jobs[0].id(),
      attempt: AttemptNumber::FIRST,
      observed_at: at(50),
    };
    let original_writes = f.bytes.0.lock().unwrap().len();
    let make = |query| {
      FactoryProtectedTestOutputFinalizer::new(
        f.runs.clone(),
        Arc::new(query),
        f.artifacts.clone(),
        f.bytes.clone(),
        vec![author.clone()],
        f.clock.clone(),
      )
      .unwrap()
    };
    let mut other = f.query.clone();
    other.attempt.jobs[0].job.job_spec_template = job_spec_template(other.build.build.id, "substituted-command");
    assert!(
      make(other).finalize(completion).await.is_err(),
      "a matching Build origin cannot authorize a substituted signed Job"
    );
    assert_eq!(
      f.bytes.0.lock().unwrap().len(),
      original_writes,
      "qualification failure must not add output writes"
    );
    make(f.query.clone()).finalize(completion).await.unwrap();
    make(f.query.clone()).finalize(completion).await.unwrap();
    let records = f
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: f.query.build.build.id,
        attempt_id: Some(f.query.attempt.id),
        limit: MAX_ARTIFACT_PAGE_SIZE,
        visibility: ArtifactListVisibility::all(),
      })
      .await
      .unwrap();
    assert_eq!(
      records.len(),
      3,
      "actual private file, derived inventory and closed public handoff"
    );
    let public = records
      .iter()
      .find(|row| row.artifact.identity().logical_name.as_str() == author.command().outputs[0].kind().as_str())
      .unwrap();
    {
      let bytes = f.bytes.0.lock().unwrap();
      assert_eq!(
        bytes.len(),
        original_writes + 2,
        "completion retries do not create new identities or bytes"
      );
      let value: serde_json::Value = serde_json::from_slice(&bytes[&public.artifact.identity().artifact_id]).unwrap();
      assert_eq!(value["classes"], serde_json::json!(["custom.check"]));
      assert_eq!(value.as_object().unwrap().len(), 2);
      assert!(
        !String::from_utf8_lossy(&bytes[&public.artifact.identity().artifact_id]).contains("PRIVATE_EXPECTATION")
      );
    }
    let mut fenced = completion;
    fenced.lease.fence = LeaseFence::from_bytes([99; 32]);
    assert!(make(f.query.clone()).finalize(fenced).await.is_err());
    let mut terminal = f.query.clone();
    terminal.attempt.jobs[0].state = JobState::Succeeded;
    terminal.attempt.jobs[0].terminal = Some(JobTerminalRecord {
      state: JobState::Succeeded,
      failure_class: None,
      timed_out: false,
      completed_at: at(60),
    });
    let replay = FactoryProtectedTestOutputFinalizer::new(
      f.runs.clone(),
      Arc::new(terminal),
      f.artifacts.clone(),
      f.bytes.clone(),
      vec![author],
      Arc::new(ReadClock(1000)),
    )
    .unwrap();
    replay.finalize(completion).await.unwrap();
    assert_eq!(
      f.bytes.0.lock().unwrap().len(),
      original_writes + 2,
      "terminal ack replay performs no new writes after authority expiry"
    );
  });
}

#[test]
fn profiled_author_executor_completes_only_the_installed_public_handoff() {
  crate::factory_admission_tests::run_ready(async {
    let (_, _, command) = crate::factory_command_tests::command_fixture();
    let (mut f, author, lease) = prepared_accepted_author(command).await;
    let now = f.clock.now().unwrap();
    FactoryProtectedTestOutputFinalizer::new(
      f.runs.clone(),
      Arc::new(f.query.clone()),
      f.artifacts.clone(),
      f.bytes.clone(),
      vec![author.clone()],
      f.clock.clone(),
    )
    .unwrap()
    .finalize(AgentOutputFinalization {
      lease,
      job_id: f.query.attempt.jobs[0].id(),
      attempt: AttemptNumber::FIRST,
      observed_at: now,
    })
    .await
    .unwrap();
    f.query.build.state = BuildState::Succeeded;
    f.query.build.updated_at = now;
    f.query.attempt.state = AttemptState::Succeeded;
    f.query.attempt.updated_at = now;
    f.query.attempt.jobs[0].state = JobState::Succeeded;
    f.query.attempt.jobs[0].updated_at = now;
    f.query.attempt.jobs[0].terminal = Some(JobTerminalRecord {
      state: JobState::Succeeded,
      failure_class: None,
      timed_out: false,
      completed_at: now,
    });
    let outputs = Arc::new(
      f.source(f.query.clone())
        .with_protected_test_authoring(vec![author.clone()])
        .unwrap(),
    );
    let mut builds = crate::factory_node_test_support::Builds::new_at(f.query.build.created_at);
    builds.acceptance = FactoryBuildAcceptance {
      build_id: f.query.build.build.id,
      attempt_id: f.query.attempt.id,
      job_ids: vec![f.query.attempt.jobs[0].id()],
      effective_policy_digest: f.intent.permissions().digest(),
    };
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    *builds.latest.lock().unwrap() = Some(f.query.attempt.clone());
    let builds = Arc::new(builds);
    assert!(
      FactoryProtectedTestAuthoringExecutor::new(
        builds.clone(),
        outputs.clone(),
        f.runs.clone(),
        vec![author.clone(), author.clone()],
      )
      .is_err(),
      "an exact installed author selection must be unambiguous"
    );
    let executor =
      FactoryProtectedTestAuthoringExecutor::new(builds.clone(), outputs, f.runs.clone(), vec![author]).unwrap();
    let snapshot = f
      .runs
      .factory_run_snapshot(f.intent.input().factory_run_id())
      .await
      .unwrap();
    let FactoryNodeExecutionStep::Waiting {
      execution: Some(accepted),
    } = executor
      .observe(FactoryNodeExecutionRequest {
        run_id: snapshot.run.id(),
        admitted: &snapshot.admitted_flow,
        work: &snapshot.work,
        input: f.intent.input(),
        attempt: f.intent.node(),
        build: Some(&f.intent),
        ownership: FactoryClaimOwnership::new(f.intent.node().owner().clone(), f.intent.node().claim()),
        observed_at: now,
      })
      .await
      .unwrap()
    else {
      panic!("author admission retains its signed execution before completion");
    };
    assert_eq!(accepted.build_id(), builds.acceptance.build_id);
    assert_eq!(accepted.attempt_id(), builds.acceptance.attempt_id);
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
    let FactoryNodeExecutionStep::Completed { record, .. } = executor
      .observe(FactoryNodeExecutionRequest {
        run_id: snapshot.run.id(),
        admitted: &snapshot.admitted_flow,
        work: &snapshot.work,
        input: f.intent.input(),
        attempt: f.intent.node(),
        build: Some(&f.intent),
        ownership: FactoryClaimOwnership::new(f.intent.node().owner().clone(), f.intent.node().claim()),
        observed_at: now,
      })
      .await
      .unwrap()
    else {
      panic!("the ordinary signed author must complete through its selected policy");
    };
    assert_eq!(record.payload().value()["classes"], serde_json::json!(["custom.check"]));
    let public = serde_json::to_string(&record).unwrap();
    assert!(!public.contains("PRIVATE_EXPECTATION"));
    assert!(!public.contains("contract.rs"));
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn retained_deployment_reconstructs_the_qualified_target_and_plan_at_both_verification_boundaries() {
  crate::factory_admission_tests::run_ready(async {
    let (command, candidate, approved, authority) =
      crate::factory_deployment_command_tests::candidate_fixture("exact.environment", None).await;
    let deployment = FactoryDeploymentCommandProfile::new(command.clone(), authority).unwrap();
    let candidate_source = Arc::new(FactoryStoredCandidateContextSource::new(
      candidate.runs.clone(),
      candidate.source.clone(),
      command.candidate_capture_tool.clone().unwrap(),
      Arc::new(ReadClock(76)),
    ));
    let f = finish_prepared(
      command,
      candidate.runs,
      candidate.admitted,
      candidate.intent,
      Some(FixtureSource::Deployment(Box::new((
        candidate.context,
        deployment.clone(),
        approved.clone(),
      )))),
      "application/octet-stream",
    )
    .await;
    let source = FactoryCommandBuildOutputSource::for_deployment(
      f.runs.clone(),
      Arc::new(f.query.clone()),
      f.artifacts.clone(),
      f.bytes.clone(),
      vec![deployment.clone()],
      approved.clone(),
      f.clock.clone(),
    )
    .unwrap()
    .with_retained_candidate_context_source(candidate_source.clone());
    let FactoryNodeBuildOutputObservation::Verified(outputs) = source
      .published_outputs(f.query.build.build.id, f.query.attempt.id, 1000)
      .await
      .unwrap()
    else {
      panic!("qualified deployment must preserve the actual retained observation");
    };
    assert_eq!(outputs.documents[0].bytes, br#"{"count":9}"#);
    assert_eq!(outputs.execution.input_digest, f.intent.input().digest().unwrap());
    assert_eq!(outputs.usage.elapsed_millis, 18);
    assert_eq!(
      f.query.attempt.jobs[0]
        .job
        .job_spec_template
        .managed_intent()
        .unwrap()
        .protected_inputs()
        .inputs
        .len(),
      6
    );
    // Keep the actual signed program unchanged: a different frozen installation
    // snapshot must not acquire approval merely from matching its report bytes.
    let mut changed = f.query.clone();
    changed.build.build.effective_policy_snapshot["job_spec_toolchain"]["source"]["plugin_version"] =
      serde_json::json!("unapproved-version");
    let substituted = FactoryCommandBuildOutputSource::for_deployment(
      f.runs.clone(),
      Arc::new(changed),
      f.artifacts.clone(),
      f.bytes.clone(),
      vec![deployment.clone()],
      approved.clone(),
      f.clock.clone(),
    )
    .unwrap()
    .with_retained_candidate_context_source(candidate_source.clone());
    assert_eq!(
      substituted
        .terminal_usage(f.query.build.build.id, f.query.attempt.id)
        .await,
      Err(OrdinaryBuildApplicationError::Invalid),
      "a changed frozen source toolchain is not approved by the installation"
    );
    assert!(
      f.source(f.query.clone())
        .with_promotion()
        .terminal_usage(f.query.build.build.id, f.query.attempt.id)
        .await
        .is_err(),
      "an ordinary promotion observer cannot attribute a network/credential command"
    );
  });
}
