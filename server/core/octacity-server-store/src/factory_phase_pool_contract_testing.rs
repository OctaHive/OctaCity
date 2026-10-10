use std::collections::BTreeSet;

use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, FactoryDigest, FactoryKey, FactoryRunId, FindingSeverity, ImmutableReference,
  PhasePoolOrder, PhasePoolPolicy,
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

/// Constructs a bounded fixture for the currently ready entry of a fresh admitted Flow.
#[must_use]
pub fn phase_pool_contract_input(snapshot: &FactoryRunSnapshot, severity: FindingSeverity) -> PhasePoolInput {
  PhasePoolInput {
    run_id: snapshot.run.id(),
    run_version: snapshot.run.version(),
    flow_run_id: snapshot.admitted_flow.root_run().id(),
    cycle_id: snapshot.admitted_flow.initial_cycle().id(),
    node: snapshot
      .admitted_flow
      .closure()
      .definition(snapshot.admitted_flow.closure().root())
      .unwrap()
      .entry()
      .clone(),
    severity,
    project_priority: 10,
    phase_input_digest: digest(1),
    capabilities: BTreeSet::new(),
    dependencies: Vec::new(),
    reservation: BudgetUsage {
      attempts: 1,
      tokens: 1,
      ..BudgetUsage::default()
    },
  }
}

async fn retire_selected_work<S: FactoryRunStore>(store: &S, selection: &crate::PhasePoolSelection) {
  use crate::{
    AuditActorKind, ClaimFactoryRun, CommitFactoryRunTransition, FactoryAuditFact, FactoryBudgetRecord,
    FactoryLifecycleCheckpoint, FactoryRunClaimRecord, FactoryRunHistoryAppend,
  };
  use octacity_server_factory::{
    DecisionSignalProgress, FactoryClaim, FactoryClaimFence, FactoryLifecycleProgress, FactoryRun, FactoryRunState,
    FactoryRunVersion, ReportingProgress,
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
  let version = FactoryRunVersion::new(snapshot.run.version().get() + 1).unwrap();
  let usage = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .unwrap()
    .usage;
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, usage, time(202));
  let checkpoint = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    FactoryLifecycleProgress::Cancelled(ReportingProgress::Ready),
    DecisionSignalProgress::Disabled,
    true,
    time(202),
  );
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = checkpoint.id;
  let next_run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    FactoryRunState::Cancelled,
    version,
  )
  .unwrap();
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner,
      fence: claim.claim.fence(),
      committed_at: time(202),
      next_run,
      budget,
      lifecycle_checkpoint: checkpoint,
      append: FactoryRunHistoryAppend::default(),
      current,
      audit: audit("factory.cancelled", 202),
      outbox: Vec::new(),
    })
    .await
    .unwrap();
}

/// Verifies deterministic ordering, exact capability and dependency gates, atomic
/// capacity/budget reservation, frozen replay, expiry, and no repeated selection.
/// The adapter must contain five fresh Runs in the same Project and no prior pool history.
pub async fn verify_factory_phase_pool_contract<S: FactoryPhasePoolStore + FactoryRunStore>(
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
    let snapshot = store.factory_run_snapshot(*run).await.unwrap();
    let severity = match index {
      0 | 1 => FindingSeverity::Critical,
      2 | 3 => FindingSeverity::High,
      _ => FindingSeverity::Low,
    };
    let mut input = phase_pool_contract_input(&snapshot, severity);
    if index == 0 {
      input.dependencies.push(PhasePoolDependency {
        run_id: runs[4],
        work_id: store.factory_run_snapshot(runs[4]).await.unwrap().work.id(),
        work_digest: crate::phase_pool_work_digest(&store.factory_run_snapshot(runs[4]).await.unwrap()),
        terminal: key("accepted"),
      });
    }
    if index == 1 {
      input.capabilities.insert(capability.clone());
    }
    if index == 2 {
      input.reservation.tokens = 9;
    }
    if index == 3 {
      input.reservation.tokens = 5;
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
    assert!(snapshot.audit.iter().any(
      |fact| fact.operation.as_str() == "factory.phase_selected" && fact.request_identity_digest == row.input_digest
    ));
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
  let ready = store.factory_run_snapshot(runs[100]).await.unwrap();
  for (index, run) in runs.iter().enumerate() {
    let snapshot = store.factory_run_snapshot(*run).await.unwrap();
    let mut input = phase_pool_contract_input(
      &snapshot,
      if index < 100 {
        FindingSeverity::Critical
      } else {
        FindingSeverity::Low
      },
    );
    if index < 100 {
      if index % 2 == 0 {
        input
          .capabilities
          .insert(ImmutableReference::new(key("unavailable.tool"), key("v1"), digest(90)));
      } else {
        input.dependencies.push(PhasePoolDependency {
          run_id: ready.run.id(),
          work_id: ready.work.id(),
          work_digest: crate::phase_pool_work_digest(&ready),
          terminal: key("accepted"),
        });
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
    let snapshot = store.factory_run_snapshot(*run).await.unwrap();
    let mut input = phase_pool_contract_input(
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
        input.capabilities.insert(capability.clone());
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
