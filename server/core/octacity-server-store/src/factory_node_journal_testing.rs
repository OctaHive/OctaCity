use crate::*;
use factory::*;
use octacity_server_factory as factory;

#[path = "../../octacity-server-factory/src/flow_test_support.rs"]
mod fixtures;
use fixtures::*;

/// Immutable fixture shared by memory and PostgreSQL node-journal contracts.
#[derive(Clone)]
pub struct FactoryNodeJournalContractFixture {
  /// Actual ordinary execution identities provided by the Build boundary.
  pub execution: FlowBuildExecution,
  /// Immutable Work and initial generic Flow admission.
  pub admission: PublishedFactoryAdmission,
  /// Exact observation data contract.
  pub schema: FlowDataSchema,
  /// Frozen input whose identity the attempt consumes.
  pub input: FlowNodeInput,
  /// First command/reasoning attempt under a bounded owner claim.
  pub attempt: NodeAttempt,
}
/// Constructs an unknown named node without a dedicated phase type.
#[must_use]
pub fn factory_node_journal_contract_fixture() -> FactoryNodeJournalContractFixture {
  let (work, admitted, schema, input, attempt) = execution_fixture(FlowNodeKind::Reasoning);
  let admitted = admitted.with_data_schemas(vec![schema.clone()]).unwrap();
  let run = FactoryRun::admitted(admitted.root_run().factory_run_id(), &work);
  FactoryNodeJournalContractFixture {
    execution: FlowBuildExecution::new(
      &FlowBuildIntent::new(
        input.clone(),
        &admitted,
        attempt.clone(),
        BudgetUsage::default(),
        i64::from(work.priority().get()),
      )
      .unwrap(),
      octacity_server_domain::BuildId::generate(),
      octacity_server_domain::AttemptId::generate(),
      vec![octacity_server_domain::JobId::generate()],
    )
    .unwrap(),
    admission: PublishedFactoryAdmission {
      work,
      run,
      flow: admitted,
      admitted_at: time(1),
    },
    schema,
    input,
    attempt,
  }
}
/// Verifies atomic retained input/result commits and restart reads using the same public store contract.
pub async fn verify_factory_node_journal_contract<S: FactoryRunStore, R: FactoryRunStore>(
  store: &S,
  restarted: &R,
  fixture: &FactoryNodeJournalContractFixture,
) {
  let run = &fixture.admission.run;
  let admitted = &fixture.admission.flow;
  let schema = &fixture.schema;
  let input = &fixture.input;
  let attempt = &fixture.attempt;
  let claim = FactoryRunClaimRecord::new(run.id(), attempt.owner().clone(), attempt.claim());
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: fact(run.id(), "factory.claimed", 10),
    })
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
  let mut start = transition(&snapshot, &claim, 11);
  start.append.flow.attempts.push(attempt.clone());
  start.append.flow.data.inputs.push(input.clone());
  let intent = FlowBuildIntent::new(
    input.clone(),
    admitted,
    attempt.clone(),
    BudgetUsage::default(),
    i64::from(fixture.admission.work.priority().get()),
  )
  .unwrap();
  start.append.flow.data.build_intents.push(intent.clone());
  start.budget.usage.attempts = 1;
  start.budget = FactoryBudgetRecord::new(run.id(), start.next_run.version(), start.budget.usage, time(11));
  start.current.budget_id = start.budget.id;
  let mut uncharged = start.clone();
  uncharged.budget = FactoryBudgetRecord::new(run.id(), uncharged.next_run.version(), BudgetUsage::default(), time(11));
  uncharged.current.budget_id = uncharged.budget.id;
  assert!(store.commit_factory_run_transition(uncharged).await.is_err());
  for (priority, usage) in [
    (99, BudgetUsage::default()),
    (
      i64::from(fixture.admission.work.priority().get()),
      BudgetUsage {
        tokens: 1,
        ..Default::default()
      },
    ),
  ] {
    let mut forged = start.clone();
    forged.append.flow.data.build_intents =
      vec![FlowBuildIntent::new(input.clone(), admitted, attempt.clone(), usage, priority).unwrap()];
    assert!(
      store.commit_factory_run_transition(forged).await.is_err(),
      "node intent must freeze authoritative usage and Work priority"
    );
  }
  store.commit_factory_run_transition(start).await.unwrap();
  let snapshot = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(snapshot.flow.data.inputs, vec![(*input).clone()]);
  assert_eq!(snapshot.flow.data.build_intents, vec![intent.clone()]);
  assert_eq!(
    snapshot.flow.data.build_intents[0].operation_id().unwrap(),
    intent.operation_id().unwrap()
  );
  assert!(snapshot.flow.data.records.is_empty());
  assert!(snapshot.flow.data.build_executions.is_empty());
  let mut observed = transition(&snapshot, &claim, 12);
  observed
    .append
    .flow
    .data
    .build_executions
    .push(fixture.execution.clone());
  store.commit_factory_run_transition(observed).await.unwrap();
  let snapshot = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(snapshot.flow.data.build_executions, vec![fixture.execution.clone()]);
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  let profile = FlowBuildIntent::new(
    input.clone(),
    admitted,
    attempt.clone(),
    BudgetUsage::default(),
    i64::from(fixture.admission.work.priority().get()),
  )
  .unwrap()
  .profile()
  .clone();
  let artifact = FactoryArtifactReference::new(
    octacity_server_domain::ArtifactId::generate(),
    FactoryDigest::content_sha256(br#"{"count":9}"#),
    11,
  )
  .unwrap();
  let provenance = FlowBuildProvenance::new(
    profile.clone(),
    vec![FlowVerifiedBuildOutput {
      name: profile.result_output,
      output_kind: EvidenceOutputKind::Report,
      schema: schema.reference().clone(),
      artifact,
      producer: EvidenceProducer::new(
        fixture.execution.build_id(),
        fixture.execution.attempt_id(),
        fixture.execution.jobs()[0],
        profile.tool,
        profile.plugin,
      ),
      verified_at: time(20),
      fresh_until: time(1000),
      data: None,
    }],
  )
  .unwrap();
  let record = FlowNodeRecord::new(
    input,
    attempt,
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(schema, serde_json::json!({"count":9})).unwrap(),
      producer: attempt.execution().clone(),
      observed_at: time(20),
    },
  )
  .unwrap()
  .with_build_provenance(provenance, definition)
  .unwrap();
  let completion = record
    .completion(
      attempt,
      definition,
      FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
      BudgetUsage::default(),
      time(21),
    )
    .unwrap();
  let mut finish = transition(&snapshot, &claim, 21);
  finish.append.flow.completions.push(completion.clone());
  assert!(store.commit_factory_run_transition(finish.clone()).await.is_err());
  assert!(
    store
      .factory_run_snapshot(run.id())
      .await
      .unwrap()
      .flow
      .completions
      .is_empty()
  );
  finish.append.flow.data.records.push(record.clone());
  store.commit_factory_run_transition(finish).await.unwrap();
  let restored = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(restored.flow.data.records, vec![record]);
  assert_eq!(restored.flow.completions, vec![completion]);
}

#[cfg(test)]
#[test]
fn frozen_node_data_and_completions_are_committed_atomically_and_survive_snapshot_reads() {
  let fixture = factory_node_journal_contract_fixture();
  let store = crate::testing::InMemoryFactoryConfigurationStore::new();
  let audit = crate::testing::management_mutation_with_request((), "generic-node-admission")
    .audit()
    .clone();
  store
    .seed_factory_run(fixture.admission.clone(), digest(1), &audit)
    .unwrap();
  crate::test_support::run_ready(
    verify_factory_node_journal_contract(&store, &store, &fixture),
    "node journal contract yielded",
  );
}

#[cfg(test)]
#[test]
fn arbitrary_configured_pools_reconstruct_frozen_inputs_and_reject_caller_supplied_priority_facts() {
  crate::test_support::run_ready(
    async {
      let (work, admitted, schema, input, attempt) =
        execution_fixture_with_pool(FlowNodeKind::Reasoning, vec![], Some(key("audit.queue")));
      let policy = PhasePoolPolicy {
        phase: key("audit.queue"),
        order: vec![
          PhasePoolOrder::Severity,
          PhasePoolOrder::ProjectPriority,
          PhasePoolOrder::Age,
        ],
        max_wip: 2,
        max_project_wip: 1,
        budget: admitted.limits().budget(),
      };
      let settings = FlowPoolSettings {
        policy: policy.clone(),
        project_priority: 42,
        capabilities: Default::default(),
        selection: FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!({"severity":"high","dependencies":[]}),
        })
        .unwrap(),
      };
      let admitted = admitted
        .with_data_schemas(vec![schema])
        .unwrap()
        .with_pool_settings(std::collections::BTreeMap::from([(key("audit.queue"), settings)]))
        .unwrap();
      let run = FactoryRun::admitted(admitted.root_run().factory_run_id(), &work);
      let store = crate::testing::InMemoryFactoryConfigurationStore::new();
      let audit = crate::testing::management_mutation_with_request((), "pool-node-admission")
        .audit()
        .clone();
      store
        .seed_factory_run(
          PublishedFactoryAdmission {
            work,
            run: run.clone(),
            flow: admitted.clone(),
            admitted_at: time(1),
          },
          digest(1),
          &audit,
        )
        .unwrap();
      let claim = FactoryRunClaimRecord::new(run.id(), attempt.owner().clone(), attempt.claim());
      store
        .claim_factory_run(ClaimFactoryRun {
          run_id: run.id(),
          expected_version: run.version(),
          record: claim.clone(),
          audit: fact(run.id(), "factory.claimed", 10),
        })
        .await
        .unwrap();
      let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
      let mut prepare = transition(&snapshot, &claim, 11);
      prepare.append.flow.data.inputs.push(input.clone());
      store.commit_factory_run_transition(prepare).await.unwrap();
      let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
      let ready = PhasePoolInput {
        generation: 0,
        run_id: run.id(),
        run_version: snapshot.run.version(),
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
        severity: FindingSeverity::High,
        project_priority: 42,
        phase_input_digest: input.digest().unwrap(),
        capabilities: Default::default(),
        dependencies: vec![],
        reservation: BudgetUsage {
          attempts: 1,
          elapsed_millis: 1000,
          tokens: 100,
          cost_micro_units: 1000,
          output_bytes: 10000,
        },
      };
      assert_eq!(configured_phase_pool_input(&snapshot, &input).unwrap(), ready);
      let mut under_reserved = ready.clone();
      under_reserved.reservation = BudgetUsage {
        attempts: 1,
        ..Default::default()
      };
      assert!(store.publish_phase_ready(policy.clone(), under_reserved).await.is_err());
      let mut unselected = transition(&snapshot, &claim, 12);
      unselected.append.flow.attempts.push(attempt.clone());
      unselected.append.flow.data.build_intents.push(
        FlowBuildIntent::new(
          input.clone(),
          &admitted,
          attempt.clone(),
          BudgetUsage::default(),
          i64::from(snapshot.work.priority().get()),
        )
        .unwrap(),
      );
      unselected.budget = FactoryBudgetRecord::new(
        run.id(),
        unselected.next_run.version(),
        BudgetUsage {
          attempts: 1,
          ..Default::default()
        },
        time(12),
      );
      unselected.current.budget_id = unselected.budget.id;
      assert!(
        store.commit_factory_run_transition(unselected).await.is_err(),
        "a queued node cannot bypass pool selection"
      );
      let mut forged = ready.clone();
      forged.severity = FindingSeverity::Critical;
      assert!(store.publish_phase_ready(policy.clone(), forged).await.is_err());
      let mut forged = ready.clone();
      forged.project_priority = 43;
      assert!(store.publish_phase_ready(policy.clone(), forged).await.is_err());
      let mut forged = ready.clone();
      forged.phase_input_digest = digest(200);
      assert!(store.publish_phase_ready(policy.clone(), forged).await.is_err());
      let entry = store.publish_phase_ready(policy.clone(), ready).await.unwrap();
      assert_eq!(
        store.phase_ready_entries(policy.digest(), None, 10).await.unwrap(),
        vec![entry.clone()]
      );
      let selected = store
        .select_phase_ready(SelectPhasePool {
          policy_digest: policy.digest(),
          request_id: digest(100),
          owner: key("queue.worker"),
          observed_at: time(101),
          expires_at: time(201),
          capabilities: Default::default(),
          limit: 1,
        })
        .await
        .unwrap();
      assert_eq!(selected.len(), 1);
      assert_eq!(selected[0].entry, entry);
      assert_eq!(
        store
          .phase_pool_selection_for_claim(run.id(), selected[0].run_claim().id)
          .await
          .unwrap(),
        Some(selected[0].clone())
      );
      assert_eq!(selected[0].entry.input.project_priority, 42);
    },
    "configured pool contract yielded",
  );
}
fn time(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
fn fact(run: FactoryRunId, operation: &str, at: i64) -> FactoryAuditFact {
  FactoryAuditFact::new(
    run,
    AuditActorKind::Worker,
    None,
    key(operation),
    digest(1),
    key("accepted"),
    time(at),
  )
}
fn transition(snapshot: &FactoryRunSnapshot, claim: &FactoryRunClaimRecord, at: i64) -> CommitFactoryRunTransition {
  let version = FactoryRunVersion::new(snapshot.run.version().get() + 1).unwrap();
  let usage = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .unwrap()
    .usage;
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, usage, time(at));
  let lifecycle = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    false,
    time(at),
  );
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = lifecycle.id;
  CommitFactoryRunTransition {
    run_id: snapshot.run.id(),
    expected_version: snapshot.run.version(),
    claim_id: claim.id,
    owner: claim.owner.clone(),
    fence: claim.claim.fence(),
    committed_at: time(at),
    next_run: FactoryRun::restore(
      snapshot.run.id(),
      snapshot.run.configuration().clone(),
      &snapshot.work,
      snapshot.run.subject().clone(),
      FactoryRunState::Admitted,
      version,
    )
    .unwrap(),
    budget,
    lifecycle_checkpoint: lifecycle,
    append: FactoryRunHistoryAppend::default(),
    current,
    audit: fact(snapshot.run.id(), "factory.node", at),
    outbox: vec![],
  }
}
