use crate::factory_node_test_support::fixtures;
use crate::factory_node_test_support::{Builds, document};
use crate::{
  ApplicationError, CreateFactoryBuild, FactoryBuildAcceptance, FactoryFlowCoordinator, FactoryFlowInputPreparer,
  FactoryFlowRunner, FactoryFlowStep, FactoryNodeBuildAdapter, FactoryNodeBuildExecutionFacts,
  FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs, FactoryNodeExecutionRequest, FactoryNodeExecutionStep,
  FactoryNodeExecutor, FactoryNodeRunner, FactoryNodeTarget, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use fixtures::{key, reference};
use octacity_server_factory::*;
use octacity_server_store::testing as store_testing;
use octacity_server_store::*;
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}

pub(crate) struct Inputs;
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    let input = FactoryNodeRunner::<store_testing::InMemoryFactoryConfigurationStore, FakePending>::prepare_input(
      snapshot, target,
    )?;
    let subject = FactoryTaskSubject::Exact(snapshot.work.subject().clone());
    let task = snapshot.work.artifacts().task().clone();
    let manifest = ContextManifest::new(
      ContextManifestId::generate(),
      subject.clone(),
      FactoryDigest::from_bytes([4; 32]),
      vec![
        ContextManifestEntry::new(
          ContextSourceKind::Task,
          key("task"),
          subject,
          FactoryContextReference::Artifact(task.clone()),
          task.content_digest(),
          task.encoded_size(),
          FactorySafeText::new("Configured task projection").unwrap(),
          FactoryDigest::from_bytes([4; 32]),
        )
        .unwrap(),
      ],
    )
    .unwrap();
    input
      .with_context(manifest, vec![])
      .map_err(|_| ApplicationError::invalid())
  }
}
struct FakePending;
#[async_trait]
impl FactoryNodeExecutor for FakePending {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    Ok(FactoryNodeExecutionStep::Waiting { execution: None })
  }
}

pub(crate) struct Execution {
  clock: std::sync::atomic::AtomicI64,
  pub(crate) seeder: Option<Arc<dyn FactoryTestBuildSeeder>>,
  builds: Mutex<BTreeMap<FactoryDigest, Arc<Builds>>>,
  requirements: BTreeMap<FactoryKey, Vec<EvidenceRequirement>>,
  result_schemas: BTreeMap<FactoryKey, ImmutableReference>,
  reports: BTreeMap<FactoryKey, serde_json::Value>,
  facts: BTreeMap<FactoryKey, serde_json::Value>,
}
impl Execution {
  fn by_build(&self, id: octacity_server_domain::BuildId) -> Arc<Builds> {
    self
      .builds
      .lock()
      .unwrap()
      .values()
      .find(|build| build.acceptance.build_id == id)
      .unwrap()
      .clone()
  }
}
#[async_trait]
impl OrdinaryBuildApplication for Execution {
  async fn factory_build_for_operation(
    &self,
    operation_id: FactoryDigest,
  ) -> Result<Option<FactoryBuildAcceptance>, OrdinaryBuildApplicationError> {
    Ok(
      self
        .builds
        .lock()
        .unwrap()
        .get(&operation_id)
        .map(|build| build.acceptance.clone()),
    )
  }
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    let builds = {
      let mut map = self.builds.lock().unwrap();
      map
        .entry(request.operation_id)
        .or_insert_with(|| {
          let builds = Arc::new(Builds::new_at(at(self.clock.load(std::sync::atomic::Ordering::SeqCst))));
          *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
          builds
        })
        .clone()
    };
    let accepted = builds.create_factory_build(request.clone()).await?;
    if let Some(seeder) = &self.seeder {
      seeder.seed(&request, &accepted).await;
    }
    Ok(accepted)
  }
}
#[async_trait]
impl BuildQueryStore for Execution {
  async fn build(&self, id: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    self.by_build(id).build(id).await
  }
  async fn latest_attempt(&self, id: octacity_server_domain::BuildId) -> Result<AttemptRecord, StoreError> {
    self.by_build(id).latest_attempt(id).await
  }
  async fn attempt(&self, _: octacity_server_domain::AttemptId) -> Result<AttemptRecord, StoreError> {
    Err(StoreError::Unavailable)
  }
  async fn job(&self, _: octacity_server_domain::JobId) -> Result<JobRecord, StoreError> {
    Err(StoreError::Unavailable)
  }
}
#[async_trait]
impl FactoryNodeBuildOutputSource for Execution {
  async fn published_outputs(
    &self,
    build: octacity_server_domain::BuildId,
    _: octacity_server_domain::AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError> {
    let builds = self.by_build(build);
    let request = builds.requests.lock().unwrap()[0].clone();
    let causality = request.causality.node().unwrap();
    let profile = &causality.profile;
    let wire: serde_json::Value = serde_json::from_slice(&causality.input).unwrap();
    let report = self
      .reports
      .get(&profile.node)
      .cloned()
      .unwrap_or_else(|| wire["payload"]["value"].clone());
    let mut observed = document(
      &builds,
      &profile.result_output,
      &self.result_schema(profile.node.clone()),
      serde_json::to_vec(&report).unwrap(),
      profile.tool.clone(),
    );
    observed.plugin = profile.plugin.clone();
    let mut documents = vec![observed];
    for requirement in &self.requirements[&profile.node] {
      let mut value = if requirement.output_kind() == EvidenceOutputKind::Artifact {
        serde_json::json!({"problem":"bounded feature proposal","sources":["admitted.task"],"assumptions":[],"alternatives":["existing contract"],"unresolved_questions":[]})
      } else if requirement.kind() == &key("terminal_resolution") {
        serde_json::json!({"accepted":false,"outcome":"unresolved"})
      } else {
        self.facts[&profile.node].clone()
      };
      value["input_digest"] = serde_json::json!(causality.input_digest);
      let mut proof = crate::factory_node_test_support::document_kind(
        &builds,
        requirement.kind(),
        requirement.schema(),
        serde_json::to_vec(&value).unwrap(),
        requirement.tool().clone(),
        requirement.output_kind(),
      );
      proof.plugin = requirement.plugin().clone();
      documents.push(proof);
    }
    let output_bytes = documents.iter().map(|doc| doc.record.identity().size_bytes).sum();
    assert!(output_bytes <= max_bytes);
    Ok(FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: profile.clone(),
        input_digest: causality.input_digest,
        permissions: request.effective_permissions,
        budget: request.budget,
        deadline: request.deadline,
      },
      documents,
      usage: BudgetUsage {
        attempts: 1,
        output_bytes,
        ..Default::default()
      },
      verified_at: builds.created_at,
      fresh_until: at(10000),
    })
  }
}
impl Execution {
  fn result_schema(&self, node: FactoryKey) -> ImmutableReference {
    self.result_schemas[&node].clone()
  }
}

pub(crate) fn configured_fixture() -> (PublishedFactoryAdmission, Arc<Execution>) {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let mut template: FlowDefinitionTemplate =
    serde_json::from_str(include_str!("../../../config/factory/flows/base-intake.json")).unwrap();
  let mut audit = template
    .nodes
    .iter()
    .find(|node| node.key == key("protected_tests"))
    .unwrap()
    .clone();
  audit.key = key("accessibility_audit");
  audit.pool = None;
  audit.build_profile = Some(key("accessibility.profile"));
  template.nodes.push(audit);
  template.transitions = template
    .transitions
    .iter()
    .map(|edge| {
      if edge.predecessor() == &key("requirements_needed") && edge.outcome() == &key("bypass") {
        FlowTransition::new(
          key("requirements_needed"),
          key("bypass"),
          FlowTransitionTarget::Node(key("accessibility_audit")),
        )
      } else {
        edge.clone()
      }
    })
    .collect();
  template.transitions.push(FlowTransition::new(
    key("accessibility_audit"),
    key("observed"),
    FlowTransitionTarget::Node(key("protected_tests")),
  ));
  let schemas = template
    .schemas
    .iter()
    .map(|(key, schema)| {
      (
        key.clone(),
        FlowDataSchema::new(schema.identity.clone(), schema.version.clone(), schema.shape.clone()).unwrap(),
      )
    })
    .collect::<BTreeMap<_, _>>();
  let fact_schemas = BTreeMap::from([
    (key("eligibility"), key("eligibility_facts")),
    (key("classification"), key("classification_facts")),
    (key("defect_research"), key("defect_facts")),
    (key("feature_research"), key("feature_facts")),
  ]);
  let mut profiles = BTreeMap::new();
  let mut requirements = BTreeMap::new();
  let mut result_schemas = BTreeMap::new();
  for node in &template.nodes {
    if let Some(choice) = &node.build_profile {
      let mut profile = fixtures::binding(work.subject().project_id());
      profile.result_output = key("observations");
      if let Some(facts) = fact_schemas.get(&node.key) {
        profile.evidence = vec![EvidenceRequirement::new(
          key("facts"),
          EvidenceOutputKind::Report,
          schemas[facts].reference().clone(),
          reference("independent.verifier"),
          reference("verifier.plugin"),
        )];
      }
      if node.key == key("feature_research") {
        profile.evidence.push(EvidenceRequirement::new(
          key("proposal"),
          EvidenceOutputKind::Artifact,
          reference("proposal.verified"),
          reference("proposal.verifier"),
          reference("verifier.plugin"),
        ));
      }
      if matches!(node.key.as_str(), "defect_research" | "feature_research") {
        profile.evidence.push(EvidenceRequirement::new(
          key("terminal_resolution"),
          EvidenceOutputKind::Report,
          schemas[&key("terminal_facts")].reference().clone(),
          reference("terminal.verifier"),
          reference("verifier.plugin"),
        ));
      }
      requirements.insert(node.key.clone(), profile.evidence.clone());
      result_schemas.insert(node.key.clone(), schemas[&node.outcomes[0].schema].reference().clone());
      profiles.insert(choice.clone(), profile);
    }
  }
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, published.definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let limits =
    FlowAdmissionLimits::product_defaults(4, template.execution.budget(), FactoryPermissionSet::deny_all()).unwrap();
  let policy = PhasePoolPolicy {
    phase: key("development.ready"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 4,
    max_project_wip: 1,
    budget: limits.budget(),
  };
  let pools = BTreeMap::from([(
    policy.phase.clone(),
    FlowPoolSettings {
      policy,
      project_priority: 4,
      capabilities: Default::default(),
      selection: FlowDataMapping::new(FlowValueMapping::Object {
        fields: BTreeMap::from([
          (
            key("severity"),
            FlowValueMapping::Pointer {
              path: "/severity".into(),
            },
          ),
          (
            key("dependencies"),
            FlowValueMapping::Pointer {
              path: "/dependencies".into(),
            },
          ),
        ]),
      })
      .unwrap(),
    },
  )]);
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(published.definition.reference(), vec![published.definition]).unwrap(),
    limits,
    flow,
    cycle,
  )
  .unwrap()
  .with_pool_settings(pools)
  .unwrap()
  .with_data_schemas(published.schemas)
  .unwrap();
  let execution = Execution {
    clock: std::sync::atomic::AtomicI64::new(30),
    seeder: None,
    builds: Mutex::new(BTreeMap::new()),
    requirements,
    result_schemas,
    reports: BTreeMap::from([
      (key("eligibility"), serde_json::json!({"fit":"fits","duplicate":false})),
      (
        key("classification"),
        serde_json::json!({"work_kind":"defect","size":"small","risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","reproducibility":"not_reproduced"}),
      ),
      (
        key("defect_research"),
        serde_json::json!({"outcome":"reproduced","summary":"Independent bounded reproduction"}),
      ),
    ]),
    facts: BTreeMap::from([
      (key("eligibility"), serde_json::json!({"fit":"fits","duplicate":false})),
      (
        key("classification"),
        serde_json::json!({"work_kind":"defect","size":"small","risk":"low","component":"api","severity":"high","dependencies":[],"reproduced":false,"already_fixed":false,"verification_only":false}),
      ),
      (key("defect_research"), serde_json::json!({"outcome":"reproduced"})),
    ]),
  };
  (
    PublishedFactoryAdmission {
      work,
      run,
      flow: admitted,
      admitted_at: at(1),
    },
    Arc::new(execution),
  )
}
pub(crate) async fn verify_configured_owner_journey<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
) -> Box<PhasePoolEntry> {
  verify_owner_journey(
    store,
    restarted,
    admission,
    execution,
    "defect_research",
    "defect_research_gate",
  )
  .await
}
async fn verify_owner_journey<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
  research: &str,
  gate: &str,
) -> Box<PhasePoolEntry> {
  let claim = FactoryRunClaimRecord::new(
    admission.run.id(),
    key("flow.worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: admission.run.id(),
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        admission.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut ready = None;
  for time in 31..75 {
    let adapter = Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()));
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      adapter,
      Arc::new(Inputs),
    );
    match owner
      .reconcile_flow(admission.run.id(), claim.id, at(time))
      .await
      .unwrap()
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Ready(entry) => {
        ready = Some(entry);
        break;
      }
      other => panic!("expected configured progress, got {other:?}"),
    }
  }
  let ready = ready.expect("configured queue must be reached");
  assert_eq!(ready.input.node.as_str(), "protected_tests");
  assert_eq!(ready.input.severity, FindingSeverity::High);
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let mut records = snapshot
    .flow
    .data
    .records
    .iter()
    .map(|row| {
      (
        snapshot
          .flow
          .attempts
          .iter()
          .find(|attempt| attempt.id() == row.node_attempt_id())
          .unwrap()
          .number(),
        row.input().node().as_str(),
      )
    })
    .collect::<Vec<_>>();
  records.sort();
  assert_eq!(
    records.into_iter().map(|(_, name)| name).collect::<Vec<_>>(),
    vec![
      "eligibility",
      "eligibility_gate",
      "classification",
      "classification_gate",
      research,
      gate,
      "requirements_needed",
      "accessibility_audit"
    ]
  );
  assert_eq!(snapshot.flow.data.build_intents.len(), 4);
  assert_eq!(snapshot.flow.data.build_executions.len(), 4);
  let accepted = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|record| record.input().node() == &key("requirements_needed"))
    .unwrap();
  let contract = AcceptedWorkContract::freeze(
    &snapshot.work,
    &snapshot.admitted_flow,
    &snapshot.flow.data,
    FlowRuntimeHistory {
      flow_runs: &snapshot.flow.runs,
      cycles: &snapshot.flow.cycles,
      attempts: &snapshot.flow.attempts,
      completions: &snapshot.flow.completions,
    },
    accepted.node_attempt_id(),
  )
  .unwrap();
  assert_eq!(
    contract.work().artifacts().acceptance(),
    admission.work.artifacts().acceptance()
  );
  assert_eq!(contract.records().len(), 7);
  assert!(
    !snapshot
      .flow
      .attempts
      .iter()
      .any(|attempt| attempt.node_key() == &key("protected_tests"))
  );
  let policy = snapshot.admitted_flow.pool_settings()[&key("development.ready")]
    .policy
    .clone();
  let selected = store
    .select_phase_ready(SelectPhasePool {
      policy_digest: policy.digest(),
      request_id: FactoryDigest::from_bytes([99; 32]),
      owner: key("pool.worker"),
      observed_at: at(901),
      expires_at: at(1800),
      capabilities: Default::default(),
      limit: 1,
    })
    .await
    .unwrap();
  assert_eq!(selected.len(), 1);
  assert_eq!(&selected[0].entry, ready.as_ref());
  let selected_claim = selected[0].run_claim();
  let mut terminal = None;
  for time in 902..940 {
    execution.clock.store(time, std::sync::atomic::Ordering::SeqCst);
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
      Arc::new(Inputs),
    );
    match owner
      .reconcile_flow(admission.run.id(), selected_claim.id, at(time))
      .await
      .unwrap()
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        terminal = Some(outcome);
        break;
      }
      other => panic!("unexpected selected progress {other:?}"),
    }
  }
  assert_eq!(terminal, Some(key("verified_candidate")));
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let mut records = snapshot
    .flow
    .data
    .records
    .iter()
    .map(|row| {
      (
        snapshot
          .flow
          .attempts
          .iter()
          .find(|attempt| attempt.id() == row.node_attempt_id())
          .unwrap()
          .number(),
        row.input().node().as_str(),
      )
    })
    .collect::<Vec<_>>();
  records.sort();
  assert_eq!(
    records.into_iter().map(|(_, name)| name).collect::<Vec<_>>(),
    vec![
      "eligibility",
      "eligibility_gate",
      "classification",
      "classification_gate",
      research,
      gate,
      "requirements_needed",
      "accessibility_audit",
      "protected_tests",
      "implementation",
      "review",
      "verification"
    ]
  );
  assert_eq!(snapshot.flow.data.build_intents.len(), 8);
  ready
}
#[test]
fn configured_intake_research_unknown_step_and_pool_share_one_owner_interface_across_restart() {
  let (admission, execution) = configured_fixture();
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "configured-flow")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(verify_configured_owner_journey(
    store.clone(),
    store,
    &admission,
    execution,
  ));
}

#[test]
fn model_small_work_claim_cannot_bypass_independently_verified_large_work() {
  let (admission, execution) = configured_fixture();
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  execution.reports.get_mut(&key("classification")).unwrap()["reproducibility"] = serde_json::json!("reproduced");
  execution.facts.get_mut(&key("classification")).unwrap()["reproduced"] = serde_json::json!(true);
  execution.facts.get_mut(&key("classification")).unwrap()["size"] = serde_json::json!("large");
  let execution = Arc::new(execution);
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "untrusted-size")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(async {
    let claim = FactoryRunClaimRecord::new(
      admission.run.id(),
      key("flow.worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
        at(10),
        at(900),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: admission.run.id(),
        expected_version: admission.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          admission.run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([1; 32]),
          key("accepted"),
          at(10),
        ),
      })
      .await
      .unwrap();
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution)),
      Arc::new(Inputs),
    );
    let mut terminal = None;
    for time in 31..65 {
      match owner
        .reconcile_flow(admission.run.id(), claim.id, at(time))
        .await
        .unwrap()
      {
        FactoryFlowStep::Advanced => {}
        FactoryFlowStep::Resolved(outcome) => {
          terminal = Some(outcome);
          break;
        }
        other => panic!("unexpected readiness: {other:?}"),
      }
    }
    assert_eq!(terminal, Some(key("specification_required")));
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    assert!(!snapshot.flow.attempts.iter().any(
      |attempt| attempt.node_key() == &key("accessibility_audit") || attempt.node_key() == &key("protected_tests")
    ));
  });
}

#[async_trait]
pub(crate) trait FactoryTestBuildSeeder: Send + Sync {
  async fn seed(&self, request: &CreateFactoryBuild, accepted: &FactoryBuildAcceptance);
}
fn run_ready<T>(future: impl std::future::Future<Output = T>) -> T {
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    std::task::Poll::Ready(value) => value,
    std::task::Poll::Pending => panic!("deterministic memory contract yielded"),
  }
}

#[test]
fn a_configured_bounded_cycle_executes_fresh_attempts_and_exhausts_its_declared_route() {
  verify_bounded_cycle(false, false);
}

#[test]
fn configured_nested_calls_freeze_caller_input_and_return_the_exact_child_result_after_restart() {
  run_ready(async {
    let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
    let admission = nested_fixture();
    let audit = store_testing::management_mutation_with_request((), "nested-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_nested_owner(store.clone(), store, &admission).await;
  });
}

pub(crate) fn nested_fixture() -> PublishedFactoryAdmission {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let schema = FlowDataSchema::new(key("nested.value"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let budget = BudgetLimit::new(16, 10000, 1000, 10000, 10000).unwrap();
  let child_node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("inspect_input"),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      BTreeMap::from([(key("caller"), FlowInputSource::CallerInput { path: String::new() })]),
      FlowDataMapping::new(FlowValueMapping::Pointer { path: "/caller".into() }).unwrap(),
    )
    .unwrap(),
  )
  .unwrap()
  .with_gate(
    FlowGateProgram::new(vec![], key("observed"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
      .unwrap()
      .binding()
      .unwrap(),
  )
  .unwrap()
  .with_accepted_work_outcomes(vec![key("observed")])
  .unwrap();
  let child = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("inspect_input"),
    nodes: vec![child_node],
    transitions: vec![FlowTransition::new(
      key("inspect_input"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("returned")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("returned"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("configured_call"),
    kind: FlowNodeKind::SubflowCall,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("returned"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: Some(child.reference()),
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("configured_call"),
    nodes: vec![call],
    transitions: vec![FlowTransition::new(
      key("configured_call"),
      key("returned"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, root.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(root.reference(), vec![root, child]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    flow,
    cycle,
  )
  .unwrap()
  .with_data_schemas(vec![schema])
  .unwrap();
  PublishedFactoryAdmission {
    work,
    run,
    flow: admitted,
    admitted_at: at(1),
  }
}

pub(crate) async fn verify_nested_owner<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
) {
  let run = &admission.run;
  let mut claim = FactoryRunClaimRecord::new(
    run.id(),
    key("nested.worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut resolved = None;
  let mut time = 11;
  let mut took_over = false;
  for _ in 0..14 {
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FakePending),
      Arc::new(Inputs),
    );
    match owner.reconcile_flow(run.id(), claim.id, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        resolved = Some(outcome);
        break;
      }
      other => panic!("nested flow stalled: {other:?}"),
    }
    let snapshot = restarted
      .factory_run_snapshot(run.id())
      .await
      .unwrap_or_else(|error| panic!("nested snapshot at {time}, takeover {took_over}: {error:?}"));
    if !took_over && snapshot.flow.data.records.len() == 1 {
      time = claim.claim.expires_at().unix_millis() + 1;
      claim = FactoryRunClaimRecord::new(
        run.id(),
        key("nested.successor"),
        FactoryClaim::new(
          FactoryClaimFence::new(FactoryDigest::from_bytes([4; 32])),
          at(time),
          at(time + 900),
        )
        .unwrap(),
      );
      restarted
        .claim_factory_run(ClaimFactoryRun {
          run_id: run.id(),
          expected_version: snapshot.run.version(),
          record: claim.clone(),
          audit: FactoryAuditFact::new(
            run.id(),
            AuditActorKind::Worker,
            None,
            key("factory.claimed"),
            FactoryDigest::from_bytes([1; 32]),
            key("accepted"),
            at(time),
          ),
        })
        .await
        .unwrap();
      took_over = true;
    }
    time += 1;
  }
  assert!(took_over);
  assert_eq!(resolved, Some(key("done")));
  let snapshot = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(snapshot.flow.runs.len(), 2);
  assert_eq!(snapshot.flow.attempts.len(), 2);
  assert_eq!(snapshot.flow.data.records.len(), 2);
  let returned = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|row| row.input().node() == &key("configured_call"))
    .unwrap();
  assert_eq!(returned.payload().value(), &serde_json::json!(true));
  let mut missing_child_proof = snapshot.flow.data.clone();
  missing_child_proof
    .records
    .retain(|row| row.input().node() == &key("configured_call"));
  assert!(
    missing_child_proof
      .validate(
        &snapshot.work,
        &snapshot.admitted_flow,
        FlowRuntimeHistory {
          flow_runs: &snapshot.flow.runs,
          cycles: &snapshot.flow.cycles,
          attempts: &snapshot.flow.attempts,
          completions: &snapshot.flow.completions,
        }
      )
      .is_err()
  );

  assert_eq!(returned.metadata().unwrap()["outcome"], serde_json::json!("returned"));
  let child_result = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|row| row.input().node() == &key("inspect_input"))
    .unwrap();
  let accepted = AcceptedWorkContract::freeze(
    &snapshot.work,
    &snapshot.admitted_flow,
    &snapshot.flow.data,
    FlowRuntimeHistory {
      flow_runs: &snapshot.flow.runs,
      cycles: &snapshot.flow.cycles,
      attempts: &snapshot.flow.attempts,
      completions: &snapshot.flow.completions,
    },
    child_result.node_attempt_id(),
  )
  .unwrap();
  assert_eq!(
    serde_json::to_value(accepted).unwrap()["caller_inputs"]
      .as_array()
      .unwrap()
      .len(),
    1
  );
}

#[test]
fn frozen_predecessor_inputs_survive_a_later_different_predecessor_in_a_bounded_cycle() {
  verify_bounded_cycle(true, false);
}

#[test]
fn a_pooled_bounded_repeat_requires_a_fresh_selection_and_reserves_each_execution() {
  verify_bounded_cycle(false, true);
}
pub(crate) fn bounded_fixture(project_predecessor: bool, pooled: bool) -> PublishedFactoryAdmission {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let schema = FlowDataSchema::new(key("cycle.value"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let budget = BudgetLimit::new(16, 10000, 1000, 10000, 10000).unwrap();
  let program = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_output_mapping(
      key("observed"),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap();
  let binding = FlowInputBinding::new(
    Default::default(),
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!(true),
    })
    .unwrap(),
  )
  .unwrap();
  let projected = FlowInputBinding::new(
    BTreeMap::from([(
      key("previous"),
      FlowInputSource::Predecessor {
        view: FlowRecordView::Payload,
        path: String::new(),
      },
    )]),
    FlowDataMapping::new(FlowValueMapping::Pointer {
      path: "/previous".into(),
    })
    .unwrap(),
  )
  .unwrap();
  let node = |name| {
    let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind: FlowNodeKind::DeterministicGate,
      input_schema: Some(schema.reference().clone()),
      outcomes: vec![FlowOutcomeDefinition::new(
        key("observed"),
        FlowOutcomeKind::Success,
        schema.reference().clone(),
      )],
      budget: BudgetLimit::new(1, 1000, 100, 1000, 1000).unwrap(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_input_binding(if project_predecessor && name != "a" {
      projected.clone()
    } else {
      binding.clone()
    })
    .unwrap()
    .with_gate(program.binding().unwrap())
    .unwrap();
    if pooled && name == "b" {
      node.with_phase_pool(key("repeat.queue"))
    } else {
      node
    }
  };
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("a"),
    nodes: if project_predecessor {
      vec![node("a"), node("b"), node("c")]
    } else {
      vec![node("a"), node("b")]
    },
    transitions: if project_predecessor {
      vec![
        FlowTransition::new(key("a"), key("observed"), FlowTransitionTarget::Node(key("b"))),
        FlowTransition::new(key("b"), key("observed"), FlowTransitionTarget::Node(key("c"))),
        FlowTransition::repeated(
          key("c"),
          key("observed"),
          FlowTransitionTarget::Node(key("b")),
          1,
          key("exhausted"),
        )
        .unwrap(),
      ]
    } else {
      vec![
        FlowTransition::new(key("a"), key("observed"), FlowTransitionTarget::Node(key("b"))),
        FlowTransition::repeated(
          key("b"),
          key("observed"),
          FlowTransitionTarget::Node(key("a")),
          1,
          key("exhausted"),
        )
        .unwrap(),
      ]
    },
    terminals: vec![FlowTerminalDefinition::new(
      key("exhausted"),
      schema.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let mut admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    flow,
    cycle,
  )
  .unwrap()
  .with_data_schemas(vec![schema])
  .unwrap();
  let pool_policy = PhasePoolPolicy {
    phase: key("repeat.queue"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 1,
    max_project_wip: 1,
    budget,
  };
  if pooled {
    admitted = admitted
      .with_pool_settings(BTreeMap::from([(
        pool_policy.phase.clone(),
        FlowPoolSettings {
          policy: pool_policy.clone(),
          project_priority: 0,
          capabilities: Default::default(),
          selection: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!({"severity":"low","dependencies":[]}),
          })
          .unwrap(),
        },
      )]))
      .unwrap();
  }
  PublishedFactoryAdmission {
    work,
    run,
    flow: admitted,
    admitted_at: at(1),
  }
}
fn verify_bounded_cycle(project_predecessor: bool, pooled: bool) {
  let admission = bounded_fixture(project_predecessor, pooled);
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "bounded-cycle")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(verify_bounded_owner(
    store.clone(),
    store,
    &admission,
    project_predecessor,
    pooled,
  ));
}
pub(crate) async fn verify_bounded_owner<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  project_predecessor: bool,
  pooled: bool,
) {
  let run = &admission.run;
  let pool_policy = admission
    .flow
    .pool_settings()
    .values()
    .next()
    .map(|settings| settings.policy.clone());
  let mut claim = FactoryRunClaimRecord::new(
    run.id(),
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut terminal = None;
  let mut time = 11;
  let mut selections = Vec::new();
  for _ in 0..40 {
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FakePending),
      Arc::new(Inputs),
    );
    match owner.reconcile_flow(run.id(), claim.id, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        terminal = Some(outcome);
        break;
      }
      FactoryFlowStep::Ready(entry) if pooled => {
        time = claim.claim.expires_at().unix_millis() + 1;
        let selected = store
          .select_phase_ready(SelectPhasePool {
            policy_digest: pool_policy.as_ref().unwrap().digest(),
            request_id: FactoryDigest::content_sha256(&time.to_be_bytes()),
            owner: key("repeat.worker"),
            observed_at: at(time),
            expires_at: at(time + 30),
            capabilities: Default::default(),
            limit: 1,
          })
          .await
          .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].entry, *entry);
        assert_eq!(selected[0].entry.input.generation as usize, selections.len());
        let selected_snapshot = restarted.factory_run_snapshot(run.id()).await.unwrap();
        assert!(phase_pool_selection_active(&selected[0], &selected_snapshot, at(time)));
        for previous in &selections {
          assert!(!phase_pool_selection_active(previous, &selected_snapshot, at(time)));
        }

        claim = selected[0].run_claim();
        selections.push(selected[0].clone());
      }
      other => panic!("unexpected cycle state {other:?}"),
    }
    time += 1;
  }
  if pooled {
    assert_eq!(
      selections.len(),
      2,
      "every bounded repeat must acquire its own capacity"
    );
    assert_ne!(selections[0].entry.id, selections[1].entry.id);
  }
  assert_eq!(terminal, Some(key("exhausted")));
  let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(snapshot.flow.attempts.len(), if project_predecessor { 5 } else { 4 });
  assert_eq!(
    snapshot.flow.data.records.len(),
    if project_predecessor { 5 } else { 4 }
  );
  assert_eq!(snapshot.flow.data.inputs.len(), if project_predecessor { 5 } else { 4 });
}

#[test]
fn configured_feature_research_retains_an_actual_proposal_artifact_before_bypass() {
  let (admission, execution) = configured_fixture();
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  let report = execution.reports.get_mut(&key("classification")).unwrap();
  report["work_kind"] = serde_json::json!("feature_request");
  report["recommended_route"] = serde_json::json!("research");
  report["reproducibility"] = serde_json::json!("not_applicable");
  execution.facts.get_mut(&key("classification")).unwrap()["work_kind"] = serde_json::json!("feature_request");
  execution.reports.insert(key("feature_research"),serde_json::json!({"outcome":"proposed","problem":"Add an operator-configured check","acceptance_boundary":"Declared contract passes","out_of_scope":"Deployment"}));
  execution.facts.insert(
    key("feature_research"),
    serde_json::json!({"proposal_verified":true,"already_fixed":false}),
  );
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "configured-feature")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(async {
    verify_owner_journey(
      store.clone(),
      store.clone(),
      &admission,
      Arc::new(execution),
      "feature_research",
      "feature_research_gate",
    )
    .await;
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let result = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("feature_research"))
      .unwrap();
    let outputs = result.build_provenance().unwrap().outputs();
    assert_eq!(outputs.len(), 4);
    let proposal = outputs.iter().find(|output| output.name == key("proposal")).unwrap();
    assert_eq!(proposal.output_kind, EvidenceOutputKind::Artifact);
    assert_eq!(proposal.producer.tool().identity().as_str(), "proposal.verifier");
    assert!(
      snapshot
        .flow
        .data
        .build_executions
        .iter()
        .any(|execution| execution.build_id() == proposal.producer.build_id()
          && execution.attempt_id() == proposal.producer.attempt_id()
          && execution.jobs().contains(&proposal.producer.job_id()))
    );
  });
}
