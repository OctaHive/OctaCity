use std::collections::BTreeSet;

use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  BudgetLimit, FactoryDigest, FactoryKey, FactoryRunId, FindingSeverity, ImmutableReference, PhasePoolOrder,
  PhasePoolPolicy,
};

use crate::{
  FactoryPhasePoolStore, FactoryRunSnapshot, FactoryRunStore, PhasePoolDependency, PhasePoolInput, SelectPhasePool,
  compare_phase_pool_entries,
};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

/// Queue scenarios shared by adapter contract fixtures; these names never reach runtime dispatch.
#[derive(Clone, Copy)]
pub enum FactoryPhasePoolContractScenario {
  /// Ordering, capability/dependency gates, reservations and replay across five Runs.
  Contract,
  /// Progress after one hundred blocked entries across store restart.
  Progress,
  /// Reconsideration of a blocked prefix after capability or lease changes.
  ScanInvalidation {
    /// Whether ordinary leases, rather than missing capabilities, block the prefix.
    lease_blocked: bool,
  },
}
impl FactoryPhasePoolContractScenario {
  /// Publishes the exact policy before any Work is admitted.
  pub fn policy(self) -> PhasePoolPolicy {
    PhasePoolPolicy {
      phase: key(match self {
        Self::Contract => "research.v1",
        Self::Progress => "research.progress",
        Self::ScanInvalidation { .. } => "research.invalidation",
      }),
      order: vec![
        PhasePoolOrder::Severity,
        PhasePoolOrder::ProjectPriority,
        PhasePoolOrder::Age,
      ],
      max_wip: if matches!(self, Self::Contract) { 2 } else { 1 },
      max_project_wip: if matches!(self, Self::Contract) { 2 } else { 1 },
      budget: BudgetLimit::new(10, 1000, 10, 1000, 1000).unwrap(),
    }
  }
  /// Freezes one fixture's selection and reservation, including forward Work dependencies.
  pub fn settings(
    self,
    index: usize,
    dependency: Option<&PhasePoolDependency>,
  ) -> (octacity_server_factory::FlowPoolSettings, BudgetLimit) {
    let mut capabilities = BTreeSet::new();
    let mut dependencies = vec![];
    let severity = match self {
      Self::Contract => match index {
        0 | 1 => FindingSeverity::Critical,
        2 | 3 => FindingSeverity::High,
        _ => FindingSeverity::Low,
      },
      _ => {
        if index < 100 {
          FindingSeverity::Critical
        } else {
          FindingSeverity::Low
        }
      }
    };
    match self {
      Self::Contract if index == 0 => dependencies.push(dependency.unwrap().clone()),
      Self::Contract if index == 1 => {
        capabilities.insert(ImmutableReference::new(key("research.tool"), key("v1"), digest(2)));
      }
      Self::Progress if index < 100 && index.is_multiple_of(2) => {
        capabilities.insert(ImmutableReference::new(key("unavailable.tool"), key("v1"), digest(90)));
      }
      Self::Progress if index < 100 => dependencies.push(dependency.unwrap().clone()),
      Self::ScanInvalidation { lease_blocked: false } if index < 100 => {
        capabilities.insert(ImmutableReference::new(key("research.tool"), key("v1"), digest(94)));
      }
      _ => (),
    }
    let tokens = match (self, index) {
      (Self::Contract, 2) => 9,
      (Self::Contract, 3) => 5,
      _ => 1,
    };
    let settings = octacity_server_factory::FlowPoolSettings {
      policy: self.policy(),
      project_priority: 10,
      capabilities,
      selection: octacity_server_factory::FlowDataMapping::new(octacity_server_factory::FlowValueMapping::Constant {
        value: serde_json::json!({"severity": severity, "dependencies": dependencies}),
      })
      .unwrap(),
    };
    (settings, BudgetLimit::new(1, 1, tokens, 1, 1).unwrap())
  }
}

/// Constructs a bounded fixture for the currently ready entry of a fresh admitted Flow.
#[must_use]
pub fn phase_pool_contract_input(snapshot: &FactoryRunSnapshot, severity: FindingSeverity) -> PhasePoolInput {
  let input = crate::configured_phase_pool_input(
    snapshot,
    snapshot
      .flow
      .data
      .inputs
      .last()
      .expect("queue input is frozen before publication"),
  )
  .unwrap();
  assert_eq!(input.severity, severity, "fixture carries its declared severity");
  input
}

/// Freezes the configured input through the ordinary fenced journal before queue publication.
pub async fn freeze_phase_pool_contract_input<S: FactoryRunStore>(store: &S, run: FactoryRunId) -> FactoryRunSnapshot {
  use crate::*;
  use octacity_server_factory::*;
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  if !snapshot.flow.data.inputs.is_empty() {
    return snapshot;
  }
  let claim = FactoryRunClaimRecord::new(
    run,
    key("pool.input-owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(200)), time(2), time(3)).unwrap(),
  );
  let audit = |operation| {
    FactoryAuditFact::new(
      run,
      AuditActorKind::Worker,
      None,
      key(operation),
      claim.id,
      key("accepted"),
      time(2),
    )
  };
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: audit("factory.claimed"),
    })
    .await
    .unwrap();
  let node = snapshot
    .admitted_flow
    .closure()
    .definition(snapshot.admitted_flow.closure().root())
    .unwrap()
    .entry()
    .clone();
  let declared = snapshot
    .admitted_flow
    .closure()
    .definition(snapshot.admitted_flow.closure().root())
    .unwrap()
    .node(&node)
    .unwrap();
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work: &snapshot.work,
    admitted: &snapshot.admitted_flow,
    flow: snapshot.admitted_flow.root_run(),
    cycle: snapshot.admitted_flow.initial_cycle(),
    node,
    schema: snapshot
      .admitted_flow
      .data_schema(declared.input_schema().unwrap())
      .unwrap(),
    history: FlowRuntimeHistory {
      stops: &snapshot.flow.data.stops,
      flow_runs: &snapshot.flow.runs,
      cycles: &snapshot.flow.cycles,
      attempts: &snapshot.flow.attempts,
      completions: &snapshot.flow.completions,
    },
    records: &snapshot.flow.data.records,
    inputs: &snapshot.flow.data.inputs,
    incoming: None,
  })
  .unwrap();
  let mut transition = crate::testing::factory_flow_contract_transition(&snapshot, &claim, 2);
  transition.append.flow.data.inputs.push(input);
  store.commit_factory_run_transition(transition).await.unwrap();
  store.factory_run_snapshot(run).await.unwrap()
}

async fn retire_selected_work<S: FactoryRunStore + crate::FactoryRunControlStore>(
  store: &S,
  selection: &crate::PhasePoolSelection,
) {
  use crate::{AuditActorKind, ClaimFactoryRun, FactoryAuditFact, FactoryLifecycleCheckpoint, FactoryRunClaimRecord};
  use octacity_server_factory::{
    FactoryClaim, FactoryClaimFence, FactoryLifecycleProgress, FactoryRun, FactoryRunState,
  };
  let snapshot = store.factory_run_snapshot(selection.entry.input.run_id).await.unwrap();
  let claim = FactoryRunClaimRecord::new(
    snapshot.run.id(),
    key("pool.recovery"),
    FactoryClaim::new(FactoryClaimFence::new(selection.entry.id), time(201), time(301)).unwrap(),
  );
  let audit = |operation: &str, at: i64| {
    FactoryAuditFact::new(
      snapshot.run.id(),
      AuditActorKind::Worker,
      None,
      key(operation),
      selection.input_digest,
      key("accepted"),
      time(at),
    )
  };
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: audit("factory.claimed", 201),
    })
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
  store
    .apply_factory_run_control(crate::testing::management_mutation_with_request(
      crate::ApplyFactoryRunControl {
        run_id: snapshot.run.id(),
        expected_version: snapshot.run.version(),
        idempotency_key: crate::IdempotencyKey::new(format!("retire-pool-work-{}", snapshot.run.id())).unwrap(),
        intent: crate::FactoryRunControlIntent::Cancel,
        requested_at: time(202),
      },
      "retire-pool-work",
    ))
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
  let successor = FactoryRunClaimRecord::new(
    snapshot.run.id(),
    claim.owner,
    FactoryClaim::new(FactoryClaimFence::new(digest(202)), time(203), time(303)).unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      record: successor.clone(),
      audit: audit("factory.claimed", 203),
    })
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
  let mut finish = crate::testing::factory_flow_contract_transition(&snapshot, &successor, 204);
  finish.next_run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    FactoryRunState::Cancelled,
    finish.next_run.version(),
  )
  .unwrap();
  finish.lifecycle_checkpoint = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    finish.next_run.version(),
    FactoryLifecycleProgress::FlowCancelled {
      root: snapshot.admitted_flow.root_run().id(),
    },
    true,
    time(204),
  );
  finish.current.lifecycle_checkpoint_id = finish.lifecycle_checkpoint.id;
  store.commit_factory_run_transition(finish).await.unwrap();
}

/// Verifies deterministic ordering, exact capability and dependency gates, atomic
/// capacity/budget reservation, frozen replay, expiry, and no repeated selection.
/// The adapter must contain five fresh Runs in the same Project and no prior pool history.
pub async fn verify_factory_phase_pool_contract<
  S: FactoryPhasePoolStore + FactoryRunStore + crate::FactoryRunControlStore,
>(
  store: &S,
  runs: &[FactoryRunId],
) {
  assert_eq!(runs.len(), 5);
  let policy = PhasePoolPolicy {
    phase: key("research.v1"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 2,
    max_project_wip: 2,
    budget: BudgetLimit::new(10, 1_000, 10, 1_000, 1_000).unwrap(),
  };
  let capability = ImmutableReference::new(key("research.tool"), key("v1"), digest(2));
  let mut entries = Vec::new();
  for (index, run) in runs.iter().enumerate() {
    let snapshot = freeze_phase_pool_contract_input(store, *run).await;
    let severity = match index {
      0 | 1 => FindingSeverity::Critical,
      2 | 3 => FindingSeverity::High,
      _ => FindingSeverity::Low,
    };
    let input = phase_pool_contract_input(&snapshot, severity);
    if index == 0 {
      assert_eq!(input.dependencies.len(), 1);
      assert_eq!(input.dependencies[0].run_id, runs[4]);
    }
    if index == 1 {
      assert_eq!(input.capabilities, BTreeSet::from([capability.clone()]));
    }
    if index == 2 {
      assert_eq!(input.reservation.tokens, 9);
    }
    if index == 3 {
      assert_eq!(input.reservation.tokens, 5);
    }
    let entry = store.publish_phase_ready(policy.clone(), input.clone()).await.unwrap();
    assert_eq!(store.publish_phase_ready(policy.clone(), input).await.unwrap(), entry);
    entries.push(entry);
  }
  let mut expected = entries.clone();
  expected.sort_by(|left, right| compare_phase_pool_entries(&policy, left, right));
  assert_eq!(
    store.phase_ready_entries(policy.digest(), None, 100).await.unwrap(),
    expected
  );
  let first_page = store.phase_ready_entries(policy.digest(), None, 2).await.unwrap();
  let rest = store
    .phase_ready_entries(policy.digest(), Some(first_page[1].id), 100)
    .await
    .unwrap();
  assert_eq!([first_page, rest].concat(), expected);
  assert!(
    store
      .phase_ready_entries(policy.digest(), Some(digest(254)), 1)
      .await
      .is_err()
  );
  let request = SelectPhasePool {
    policy_digest: policy.digest(),
    request_id: digest(10),
    owner: key("worker.one"),
    observed_at: time(100),
    expires_at: time(200),
    capabilities: BTreeSet::new(),
    limit: 2,
  };
  let selected = store.select_phase_ready(request.clone()).await.unwrap();
  assert_eq!(
    selected.iter().map(|row| row.entry.input.run_id).collect::<Vec<_>>(),
    [runs[2], runs[4]]
  );
  assert_eq!(selected[0].input_digest, selected[1].input_digest);
  for row in &selected {
    assert_eq!(row.entry.policy_digest, policy.digest());
    let snapshot = store.factory_run_snapshot(row.entry.input.run_id).await.unwrap();
    assert_eq!(snapshot.current_claim, Some(row.run_claim()));
    let audits = store
      .list_factory_run_diagnostics(
        crate::ListFactoryRunDiagnostics::new(snapshot.run.id(), crate::FactoryRunDiagnosticKind::Audit, None, 100)
          .unwrap(),
      )
      .await
      .unwrap();
    assert!(
      audits
        .items
        .iter()
        .any(|record| matches!(record, crate::FactoryRunDiagnosticRecord::Audit(fact)
      if fact.operation.as_str() == "factory.phase_selected" && fact.request_identity_digest == row.input_digest))
    );
  }
  assert_eq!(store.select_phase_ready(request.clone()).await.unwrap(), selected);
  let mut altered = request.clone();
  altered.owner = key("worker.substitution");
  assert!(store.select_phase_ready(altered).await.is_err());
  let mut next = request.clone();
  next.request_id = digest(11);
  next.owner = key("worker.two");
  next.capabilities.insert(capability.clone());
  assert!(store.select_phase_ready(next.clone()).await.unwrap().is_empty());
  // Even an empty pass replays unchanged after capacity becomes available.
  assert!(store.select_phase_ready(next.clone()).await.unwrap().is_empty());
  let mut after_expiry = request;
  after_expiry.request_id = digest(12);
  after_expiry.owner = key("worker.three");
  after_expiry.observed_at = time(200);
  after_expiry.expires_at = time(300);
  after_expiry.capabilities.insert(capability);
  assert!(store.select_phase_ready(after_expiry.clone()).await.unwrap().is_empty());
  // Lease expiry cannot create capacity for new Work while recovery still owns the logical selections.
  for selection in &selected {
    retire_selected_work(store, selection).await;
  }
  assert!(store.select_phase_ready(next).await.unwrap().is_empty());
  after_expiry.request_id = digest(14);
  after_expiry.observed_at = time(203);
  let selected_later = store.select_phase_ready(after_expiry.clone()).await.unwrap();
  assert_eq!(
    selected_later
      .iter()
      .map(|row| row.entry.input.run_id)
      .collect::<Vec<_>>(),
    [runs[1], runs[3]]
  );
  assert_eq!(
    store.select_phase_ready(after_expiry.clone()).await.unwrap(),
    selected_later
  );
  after_expiry.request_id = digest(13);
  after_expiry.observed_at = time(300);
  after_expiry.expires_at = time(400);
  assert!(store.select_phase_ready(after_expiry).await.unwrap().is_empty());
  assert_eq!(
    store.phase_ready_entries(policy.digest(), None, 100).await.unwrap(),
    vec![entries[0].clone()]
  );
}

/// Verifies bounded progress past a full blocked window, including restart and
/// exact empty-pass replay. Both handles must address the same authoritative store.
/// The fixture contains 101 fresh Runs in one Project.
pub async fn verify_factory_phase_pool_progress<S: FactoryPhasePoolStore + FactoryRunStore>(
  store: &S,
  restarted: &S,
  runs: &[FactoryRunId],
) {
  assert_eq!(runs.len(), 101);
  let policy = PhasePoolPolicy {
    phase: key("research.progress"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 1,
    max_project_wip: 1,
    budget: BudgetLimit::new(10, 1000, 10, 1000, 1000).unwrap(),
  };
  let ready = freeze_phase_pool_contract_input(store, runs[100]).await;
  for (index, run) in runs.iter().enumerate() {
    let snapshot = freeze_phase_pool_contract_input(store, *run).await;
    let input = phase_pool_contract_input(
      &snapshot,
      if index < 100 {
        FindingSeverity::Critical
      } else {
        FindingSeverity::Low
      },
    );
    if index < 100 {
      if index % 2 == 0 {
        assert!(
          input
            .capabilities
            .contains(&ImmutableReference::new(key("unavailable.tool"), key("v1"), digest(90)))
        );
      } else {
        assert_eq!(input.dependencies.len(), 1);
        assert_eq!(input.dependencies[0].run_id, ready.run.id());
      }
    }
    if index < 100 {
      store.publish_phase_ready(policy.clone(), input).await.unwrap();
    }
  }
  let request = SelectPhasePool {
    policy_digest: policy.digest(),
    request_id: digest(91),
    owner: key("progress.worker"),
    observed_at: time(200),
    expires_at: time(300),
    capabilities: BTreeSet::new(),
    limit: 1,
  };
  assert!(store.select_phase_ready(request.clone()).await.unwrap().is_empty());
  assert!(restarted.select_phase_ready(request.clone()).await.unwrap().is_empty());
  // A new lower-ranked entry must not invalidate the already skipped prefix.
  store
    .publish_phase_ready(policy.clone(), phase_pool_contract_input(&ready, FindingSeverity::Low))
    .await
    .unwrap();
  let mut next = request.clone();
  next.request_id = digest(92);
  let selected = restarted.select_phase_ready(next.clone()).await.unwrap();
  assert_eq!(selected.len(), 1);
  assert_eq!(selected[0].entry.input.run_id, ready.run.id());
  assert_eq!(store.select_phase_ready(next).await.unwrap(), selected);
  assert!(store.select_phase_ready(request).await.unwrap().is_empty());
}

/// Verifies that changed capabilities or expired ordinary claims recheck the
/// highest-ranked blocked prefix before selecting lower-ranked Work.
/// Each invocation needs 101 fresh Runs; `lease_blocked` selects the lease case.
pub async fn verify_factory_phase_pool_scan_invalidation<S: FactoryPhasePoolStore + FactoryRunStore>(
  store: &S,
  restarted: &S,
  runs: &[FactoryRunId],
  lease_blocked: bool,
) {
  use crate::{AuditActorKind, ClaimFactoryRun, FactoryAuditFact, FactoryRunClaimRecord};
  use octacity_server_factory::{FactoryClaim, FactoryClaimFence};
  assert_eq!(runs.len(), 101);
  let policy = PhasePoolPolicy {
    phase: key("research.invalidation"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 1,
    max_project_wip: 1,
    budget: BudgetLimit::new(10, 1000, 10, 1000, 1000).unwrap(),
  };
  let capability = ImmutableReference::new(key("research.tool"), key("v1"), digest(94));
  for (index, run) in runs.iter().enumerate() {
    let snapshot = freeze_phase_pool_contract_input(store, *run).await;
    let input = phase_pool_contract_input(
      &snapshot,
      if index < 100 {
        FindingSeverity::Critical
      } else {
        FindingSeverity::Low
      },
    );
    if index < 100 {
      if lease_blocked {
        let claim = FactoryRunClaimRecord::new(
          *run,
          key("existing.worker"),
          FactoryClaim::new(FactoryClaimFence::new(digest(95)), time(150), time(250)).unwrap(),
        );
        store
          .claim_factory_run(ClaimFactoryRun {
            run_id: *run,
            expected_version: snapshot.run.version(),
            record: claim.clone(),
            audit: FactoryAuditFact::new(
              *run,
              AuditActorKind::Worker,
              None,
              key("factory.claimed"),
              claim.id,
              key("accepted"),
              time(150),
            ),
          })
          .await
          .unwrap();
      } else {
        assert!(input.capabilities.contains(&capability));
      }
    }
    store.publish_phase_ready(policy.clone(), input).await.unwrap();
  }
  let request = SelectPhasePool {
    policy_digest: policy.digest(),
    request_id: digest(96),
    owner: key("invalidation.worker"),
    observed_at: time(200),
    expires_at: time(300),
    capabilities: BTreeSet::new(),
    limit: 1,
  };
  assert!(store.select_phase_ready(request.clone()).await.unwrap().is_empty());
  let mut next = request.clone();
  next.request_id = digest(97);
  if lease_blocked {
    next.observed_at = time(250);
  } else {
    next.capabilities.insert(capability);
  }
  let selected = restarted.select_phase_ready(next.clone()).await.unwrap();
  assert_eq!(selected.len(), 1);
  assert!(runs[..100].contains(&selected[0].entry.input.run_id));
  assert_eq!(selected[0].entry.input.severity, FindingSeverity::Critical);
  assert_eq!(store.select_phase_ready(next).await.unwrap(), selected);
  assert!(store.select_phase_ready(request).await.unwrap().is_empty());
}
