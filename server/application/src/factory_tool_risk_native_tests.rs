//! Lease-bound application behavior over the common journal and ordinary Job contracts.
use crate::factory_node_test_support::fixtures;
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_job::{JobSpecBuildSnapshot, JobState};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{testing::*, *};
use std::{
  collections::BTreeMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
};

fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(1011))
  }
}
struct NeverDispatch;
#[async_trait]
impl FactoryNodeExecutor for NeverDispatch {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("first step only")
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
}
struct QueryStore(Mutex<Queries>);
#[async_trait]
impl BuildQueryStore for QueryStore {
  async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
    let q = self.0.lock().unwrap();
    assert_eq!(id, q.build.build.id);
    Ok(q.build.clone())
  }
  async fn attempt(&self, id: AttemptId) -> Result<AttemptRecord, StoreError> {
    let q = self.0.lock().unwrap();
    assert_eq!(id, q.attempt.id);
    Ok(q.attempt.clone())
  }
  async fn latest_attempt(&self, id: BuildId) -> Result<AttemptRecord, StoreError> {
    let q = self.0.lock().unwrap();
    assert_eq!(id, q.build.build.id);
    Ok(q.attempt.clone())
  }
  async fn job(&self, id: JobId) -> Result<JobRecord, StoreError> {
    let q = self.0.lock().unwrap();
    assert_eq!(id, q.attempt.jobs[0].id());
    Ok(q.attempt.jobs[0].clone())
  }
}
struct Acceptance(FactoryDigest, AcceptedFactoryBuild);
#[async_trait]
impl FactoryBuildAcceptanceStore for Acceptance {
  async fn factory_build_for_operation(&self, id: FactoryDigest) -> Result<Option<AcceptedFactoryBuild>, StoreError> {
    Ok((id == self.0).then(|| self.1.clone()))
  }
  async fn accept_factory_build(&self, _: AcceptFactoryBuild) -> Result<AcceptedFactoryBuild, StoreError> {
    unreachable!()
  }
}
struct Leases {
  access: LeaseAccess,
  job: JobId,
  valid: AtomicBool,
}
#[async_trait]
impl JobExecutionStore for Leases {
  async fn authorize_tool_action_lease(
    &self,
    access: LeaseAccess,
    job: JobId,
    attempt: AttemptNumber,
    _: Timestamp,
  ) -> Result<(), StoreError> {
    if access == self.access && job == self.job && attempt == AttemptNumber::FIRST && self.valid.load(Ordering::SeqCst)
    {
      Ok(())
    } else {
      Err(StoreError::Unavailable)
    }
  }
  async fn claim_ready_job(&self, _: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    unreachable!()
  }
  async fn append_job_events(&self, _: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    unreachable!()
  }
  async fn complete_job(&self, _: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    unreachable!()
  }
  async fn fail_lease_assignment(&self, _: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    unreachable!()
  }
}
struct Provider {
  capability: DecisionSignalProviderCapability,
  calls: AtomicUsize,
  lose_lease: Mutex<Option<Arc<Leases>>>,
}
impl DecisionSignalProvider for Provider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }
  fn evaluate<'a>(&'a self, r: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    Box::pin(async move {
      self.calls.fetch_add(1, Ordering::SeqCst);
      if let Some(leases) = self.lose_lease.lock().unwrap().as_ref() {
        leases.valid.store(false, Ordering::SeqCst);
      }
      Ok(
        DecisionSignalProviderResult::new(
          r.id(),
          r.provider().clone(),
          r.adapter().clone(),
          r.model().clone(),
          r.probability_semantics().clone(),
          vec![DecisionSignalAnswer::new(
            r.input().questions()[0].reference().clone(),
            DecisionSignalAnswerValue::FiniteChoice {
              selected: FactoryKey::new("deny").unwrap(),
              selected_probability: DecisionSignalProbability::new(800_000).unwrap(),
              runner_up_probability: DecisionSignalProbability::new(200_000).unwrap(),
            },
          )],
          BudgetUsage {
            attempts: 1,
            elapsed_millis: 5,
            tokens: 10,
            cost_micro_units: 0,
            output_bytes: 20,
          },
        )
        .unwrap(),
      )
    })
  }
}
struct Fixture {
  runs: Arc<InMemoryFactoryConfigurationStore>,
  queries: Arc<QueryStore>,
  acceptance: Arc<Acceptance>,
  leases: Arc<Leases>,
  provider: Arc<Provider>,
  signals: Arc<Signals>,
  binding: FactoryToolRiskBinding,
  intent: FlowBuildIntent,
  base_task: FactoryDigest,
}
impl Fixture {
  fn authorizer(
    &self,
  ) -> FactoryToolRiskAuthorizer<InMemoryFactoryConfigurationStore, QueryStore, Acceptance, Leases, Signals> {
    FactoryToolRiskAuthorizer::new(
      self.runs.clone(),
      self.queries.clone(),
      self.acceptance.clone(),
      self.leases.clone(),
      self.signals.clone(),
      vec![FactoryToolRiskNodeProfile {
        provider: self.provider.clone(),
        binding: self.binding.clone(),
        base_task_digest: self.base_task,
        build_configuration: self.intent.profile().build_configuration.clone(),
      }],
      Arc::new(Clock),
    )
    .unwrap()
  }
  fn input(&self) -> LeaseBoundToolActionInput {
    LeaseBoundToolActionInput {
      lease: self.leases.access,
      job_id: self.leases.job,
      attempt: AttemptNumber::FIRST,
      observed_at: at(1011),
      proposal_sha256: FactoryDigest::from_bytes([91; 32]).to_string(),
      proposal_summary: octacity_protocol::FactoryToolActionSummaryV3 {
        argument_count: 1,
        read_path_count: 1,
        write_path_count: 0,
        network_host_count: 0,
        secret_profile_count: 0,
        workload_identity_profile_count: 0,
        descendants: 1,
        output_kind_count: 0,
      },
    }
  }
}
async fn fixture() -> Fixture {
  fixture_with_capability("fixture.blocking-tool-authorization.v1").await
}
async fn fixture_with_capability(capability: &str) -> Fixture {
  let work = fixtures::work();
  let mut binding = crate::factory_tool_risk_authorizer_tests::binding_fixture();
  binding.project_id = work.subject().project_id();
  binding.max_calls = 1;
  let grants = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![binding.plan.profile.adapter().clone()],
    network_hosts: vec![binding.network_host.clone()],
    secret_profiles: vec![binding.credential_profile.clone()],
    ..FactoryPermissionDraft::default()
  })
  .unwrap();
  binding.project_permissions = grants.clone();
  binding.local_permissions = LocalPermissionCeiling::fully_enforced(grants);
  let base_task = FactoryDigest::from_bytes([73; 32]);
  let mut selection = fixtures::binding(work.subject().project_id());
  selection.plugin = crate::factory_job_test_support::reference("codex", "0.5.0");
  selection.tool = crate::factory_job_test_support::reference("codex-cli", "0.116.0");
  selection.task_digest = factory_tool_risk_task_digest(base_task, binding.policy_digest().unwrap());
  let mut worker = serde_json::to_value(crate::factory_job_test_support::permissions(false)).unwrap();
  worker["tools"] = serde_json::json!([crate::factory_job_test_support::reference("read", "1.0.0")]);
  worker["resources"]["elapsed_millis"] = serde_json::json!(1000);
  let worker: FactoryPermissionSet = serde_json::from_value(worker).unwrap();
  let (_, previous, schema, previous_input, _) = fixtures::execution_fixture_with_permissions(
    FlowNodeKind::Reasoning,
    work.clone(),
    selection,
    None,
    worker.clone(),
  );
  let original = previous.closure().definition(previous.closure().root()).unwrap();
  let mut nodes = serde_json::to_value(original.nodes()).unwrap();
  nodes[0]["budget"] = serde_json::to_value(BudgetLimit::new(2, 1000, 100, 1000, 10000).unwrap()).unwrap();
  let mut combined = serde_json::to_value(&worker).unwrap();
  combined["plugins"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::to_value(binding.plan.profile.adapter()).unwrap());
  combined["network_hosts"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!(binding.network_host.as_str()));
  combined["secret_profiles"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!(binding.credential_profile.as_str()));
  let combined: FactoryPermissionSet = serde_json::from_value(combined).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: serde_json::from_value(nodes).unwrap(),
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: original.context_projections().to_vec(),
    data_projections: original.data_projections().to_vec(),
    execution: FlowExecutionPolicy::new(previous.limits().budget(), combined.clone(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(previous_input.factory_run_id(), &work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(4, previous.limits().budget(), combined).unwrap(),
    root,
    cycle,
    previous.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    previous_input.node().clone(),
    FlowPayload::new(&schema, serde_json::json!({"count":3})).unwrap(),
  )
  .unwrap()
  .with_context(
    ContextManifest::new(
      ContextManifestId::generate(),
      FactoryTaskSubject::Exact(work.subject().clone()),
      FactoryDigest::from_bytes([63; 32]),
      vec![],
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  let runs = Arc::new(InMemoryFactoryConfigurationStore::new());
  runs
    .seed_factory_run(
      PublishedFactoryAdmission {
        work: work.clone(),
        run: run.clone(),
        flow: admitted,
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([23; 32]),
      management_mutation_with_request((), "tool-risk-test").audit(),
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    FactoryKey::new("owner").unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([24; 32])),
      at(1000),
      at(5000),
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
        FactoryKey::new("factory.claimed").unwrap(),
        claim.id,
        FactoryKey::new("claimed").unwrap(),
        at(1000),
      ),
    })
    .await
    .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  FactoryNodeRunner::new(runs.clone(), Arc::new(NeverDispatch), Arc::new(Clock))
    .run_once(run.id(), claim.id, target, Some(input), at(1001))
    .await
    .unwrap();
  let intent = runs
    .factory_run_snapshot(run.id())
    .await
    .unwrap()
    .flow
    .data
    .build_intents[0]
    .clone();
  let base = authoritative_store_contract_fixture();
  let mut build = base.request.build.clone();
  build.project_id = work.subject().project_id();
  build.repository_id = work.subject().repository_id();
  build.configuration_id = intent.profile().build_configuration.id();
  build.configuration_version = intent.profile().build_configuration.version();
  build.immutable_revision = work.subject().base_revision().clone();
  let mut canonical = crate::factory_job_test_support::canonical_v3_spec();
  canonical.runtime.timeout_seconds = 1;
  let mut execution = canonical.execution.clone();
  execution.tool_control = Some(octacity_protocol::FactoryToolControlV3 {
    plugin: intent.profile().plugin.identity().to_string(),
    capability: capability.to_owned(),
    mode: octacity_protocol::FactoryToolControlModeV3::ToolRisk,
    response_limits: octacity_protocol::FactoryToolResponseLimitsV3 {
      per_call_bytes: 32,
      total_bytes: 64,
    },
  });
  let mut job = base.request.jobs[0].clone();
  job.job_spec_template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: JobSpecBuildSnapshot::new(
      build.id,
      build.immutable_revision.clone(),
      None,
      RepositoryLocator::new("https://example.test/repository.git").unwrap(),
      BTreeMap::new(),
    )
    .unwrap(),
    pipeline_node_id: job.pipeline_node_id.clone(),
    execution,
    protected_inputs: canonical.protected_inputs.clone(),
    causality: octacity_protocol::FactoryCausalityV3 {
      factory_run_id: run.id().to_string(),
      factory_configuration_id: intent.input().configuration().id().to_string(),
      factory_configuration_version: intent.input().configuration().version().get(),
      flow_run_id: intent.input().flow_run_id().to_string(),
      workflow_cycle_id: intent.input().cycle_id().to_string(),
      node_attempt_id: intent.node().id().to_string(),
      input_digest: intent.input().digest().unwrap().to_string(),
      subject_digest: work.subject().digest().to_string(),
      source_write_paths: None,
      change_set_capture: None,
      change_set_materialization: None,
    },
    policy_layers: FactoryBuildPolicyLayers {
      project: worker.clone(),
      configuration: worker.clone(),
      task: worker.clone(),
      local: LocalPermissionCeiling::fully_enforced(worker),
    },
    required_enforcement: canonical.required_enforcement.clone(),
    policy: crate::factory_job_test_support::policy(&canonical),
  })
  .unwrap()
  .with_execution_deadline(intent.node().deadline())
  .unwrap();
  build.priority = intent.priority();
  AcceptFactoryBuild {
    intent: intent.clone(),
    ownership: FactoryClaimOwnership::new(claim.owner, claim.claim),
    build: build.clone(),
    attempt_id: base.request.attempt_id,
    jobs: vec![job.clone()],
    accepted_at: at(1002),
  }
  .validate()
  .expect("ordinary acceptance bridge uses a valid managed DAG");
  let job_id = job.id;
  let queries = Arc::new(QueryStore(Mutex::new(Queries {
    build: BuildRecord {
      build: build.clone(),
      state: BuildState::Running,
      version: BuildVersion::new(1).unwrap(),
      origin: BuildOrigin::Factory(FactoryBuildOrigin::from_intent(&intent).unwrap()),
      created_at: at(1002),
      updated_at: at(1002),
    },
    attempt: AttemptRecord {
      id: base.request.attempt_id,
      build_id: build.id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state: AttemptState::Running,
      version: AttemptVersion::new(1).unwrap(),
      created_at: at(1002),
      updated_at: at(1002),
      jobs: vec![JobRecord {
        attempt_id: base.request.attempt_id,
        job,
        state: JobState::Running,
        version: JobVersion::new(1).unwrap(),
        created_at: at(1002),
        updated_at: at(1002),
        queue: None,
        assignment: Some(JobAssignmentRecord {
          pool_id: base.allowed_pool,
          agent_id: base.agent_id,
        }),
        terminal: None,
        event_cursor: 0,
      }],
    },
  })));
  let acceptance = Arc::new(Acceptance(
    intent.operation_id().unwrap(),
    AcceptedFactoryBuild {
      build_id: build.id,
      attempt_id: base.request.attempt_id,
      job_ids: vec![job_id],
      effective_policy_digest: intent.permissions().digest(),
    },
  ));
  let leases = Arc::new(Leases {
    access: LeaseAccess {
      lease_id: LeaseId::generate(),
      fence: LeaseFence::from_bytes([31; 32]),
      agent_id: base.agent_id,
      registration_epoch: base.registration_epoch,
    },
    job: job_id,
    valid: AtomicBool::new(true),
  });
  let provider = Arc::new(Provider {
    capability: binding.plan.capability.clone(),
    calls: AtomicUsize::new(0),
    lose_lease: Mutex::new(None),
  });
  Fixture {
    runs,
    queries,
    acceptance,
    leases,
    provider,
    signals: Arc::new(Signals::default()),
    binding,
    intent,
    base_task,
  }
}

#[test]
fn lease_bound_tool_risk_uses_the_signed_harness_selector_without_a_vendor_contract() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture_with_capability("qualified-reader.blocking-tool-authorization.v1").await;
    let decision = f
      .authorizer()
      .authorize(f.input())
      .await
      .expect("current signed tool-risk authority is independent of the harness vendor");
    assert_eq!(
      decision.disposition(),
      octacity_protocol::FactoryToolActionDispositionV3::Allow
    );
    assert_eq!(
      decision.source(),
      octacity_protocol::FactoryToolActionDecisionSourceV3::DecisionSignal
    );
    assert_eq!(f.provider.calls.load(Ordering::SeqCst), 1);
  });
}

#[test]
fn lease_bound_shadow_tool_risk_retains_denial_and_replays_without_another_provider_call() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    let authorizer = f.authorizer();
    let decision = authorizer.authorize(f.input()).await.unwrap();
    assert_eq!(
      decision.disposition(),
      octacity_protocol::FactoryToolActionDispositionV3::Allow
    );
    assert_eq!(
      decision.source(),
      octacity_protocol::FactoryToolActionDecisionSourceV3::DecisionSignal
    );
    let request = f
      .binding
      .prepare_request(
        f.intent.input(),
        f.intent.node(),
        FactoryDigest::from_bytes([91; 32]),
        &f.input().proposal_summary,
      )
      .unwrap();
    let receipt = f
      .signals
      .factory_decision_signal_receipt(&request)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(
      receipt.consumption().recommendation(),
      Some(&DecisionSignalDisposition::Deny)
    );
    assert_eq!(
      decision.decision_signal_receipt_sha256(),
      Some(receipt.receipt_digest().to_string().as_str())
    );
    let replay = authorizer.authorize(f.input()).await.unwrap();
    assert_eq!(replay, decision);
    assert_eq!(f.provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.usage().tokens, 10);
    let reserved = f.signals.reservations.lock().unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(
      reserved[0].reservation_limit,
      Some(f.binding.signal_allowance().unwrap())
    );
    assert_eq!(
      reserved[0].lease_guard,
      Some(FactoryDecisionSignalLeaseGuard {
        lease: f.leases.access,
        job_id: f.leases.job,
        attempt: AttemptNumber::FIRST
      })
    );
  });
}

#[test]
fn a_current_leased_job_can_authorize_its_first_hook_before_running_events_arrive() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    {
      let mut queries = f.queries.0.lock().unwrap();
      queries.build.state = BuildState::Queued;
      queries.attempt.state = AttemptState::Created;
      queries.attempt.jobs[0].state = JobState::Leased;
    }
    let authorizer = f.authorizer();
    let decision = authorizer.authorize(f.input()).await.unwrap();
    assert_eq!(
      decision.disposition(),
      octacity_protocol::FactoryToolActionDispositionV3::Allow
    );
    assert_eq!(f.provider.calls.load(Ordering::SeqCst), 1);
    {
      let mut queries = f.queries.0.lock().unwrap();
      queries.build.state = BuildState::Running;
      queries.attempt.state = AttemptState::Running;
      queries.attempt.jobs[0].state = JobState::Running;
    }
    assert_eq!(authorizer.authorize(f.input()).await.unwrap(), decision);
    f.leases.valid.store(false, Ordering::SeqCst);
    assert!(
      authorizer.authorize(f.input()).await.is_err(),
      "event delivery cannot revive revoked lease authority"
    );
    assert_eq!(f.provider.calls.load(Ordering::SeqCst), 1);
  });
}

#[test]
fn lease_bound_tool_risk_rejects_terminal_build_before_provider_dispatch() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    f.queries.0.lock().unwrap().build.state = BuildState::Cancelled;
    assert!(f.authorizer().authorize(f.input()).await.is_err());
    assert_eq!(f.provider.calls.load(Ordering::SeqCst), 0);
    assert!(f.signals.reservations.lock().unwrap().is_empty());
  });
}

#[test]
fn native_tool_risk_installation_rejects_cross_project_build_configuration() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    let configuration = &f.intent.profile().build_configuration;
    let other = BuildConfigurationRef::new(
      configuration.id(),
      configuration.version(),
      ProjectId::generate(),
      configuration.definition_digest(),
    );
    assert!(
      FactoryToolRiskAuthorizer::new(
        f.runs,
        f.queries,
        f.acceptance,
        f.leases,
        f.signals,
        vec![FactoryToolRiskNodeProfile {
          provider: f.provider,
          binding: f.binding,
          base_task_digest: f.base_task,
          build_configuration: other
        }],
        Arc::new(Clock)
      )
      .is_err()
    );
  });
}

#[test]
fn lease_bound_tool_risk_retains_measured_receipt_but_refuses_action_after_lease_loss() {
  crate::factory_admission_tests::run_ready(async {
    let f = fixture().await;
    *f.provider.lose_lease.lock().unwrap() = Some(f.leases.clone());
    assert!(f.authorizer().authorize(f.input()).await.is_err());
    assert_eq!(f.provider.calls.load(Ordering::SeqCst), 1);
    let receipts = f.signals.receipts.lock().unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].usage().tokens, 10);
  });
}

#[derive(Default)]
struct Signals {
  reservations: Mutex<Vec<ReserveFactoryDecisionSignalRequest>>,
  receipts: Mutex<Vec<DecisionSignalReceipt>>,
}
#[async_trait]
impl FactoryDecisionSignalRequestStore for Signals {
  async fn factory_decision_signal_request_reserved(
    &self,
    r: &DecisionSignalProviderRequest,
  ) -> Result<bool, StoreError> {
    Ok(self.reservations.lock().unwrap().iter().any(|v| v.request == *r))
  }
  async fn reserve_factory_decision_signal_request(
    &self,
    r: ReserveFactoryDecisionSignalRequest,
  ) -> Result<FactoryDecisionSignalRequestDisposition, StoreError> {
    self.reservations.lock().unwrap().push(r);
    Ok(FactoryDecisionSignalRequestDisposition::Reserved)
  }
}
#[async_trait]
impl FactoryDecisionSignalReceiptStore for Signals {
  async fn factory_decision_signal_receipt(
    &self,
    r: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, StoreError> {
    Ok(self.receipts.lock().unwrap().iter().find(|v| v.request() == r).cloned())
  }
  async fn publish_factory_decision_signal_receipt(
    &self,
    r: PublishFactoryDecisionSignalReceipt,
  ) -> Result<(), StoreError> {
    self.receipts.lock().unwrap().push(r.receipt);
    Ok(())
  }
}
