use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use octacity_server_factory::{FactoryDigest, MAX_PHASE_POOL_BATCH, PhasePoolPolicy};

use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;
use crate::factory_phase_pool::{digest, pool_conflict, pool_invalid};
use crate::{
  FactoryPhasePoolStore, PhasePoolEntry, PhasePoolInput, PhasePoolSelection, SelectPhasePool, StoreError,
  compare_phase_pool_entries, derive_phase_pool_entry, observe_phase_pool_candidate, phase_pool_selection_active,
  plan_phase_pool_selection,
};

#[derive(Clone, Default)]
pub(super) struct PhasePoolMemoryState {
  policies: BTreeMap<FactoryDigest, PhasePoolPolicy>,
  entries: BTreeMap<FactoryDigest, PhasePoolEntry>,
  pub(super) selections: BTreeMap<FactoryDigest, PhasePoolSelection>,
  passes: BTreeMap<FactoryDigest, (SelectPhasePool, Vec<PhasePoolSelection>)>,
  scan_after: BTreeMap<FactoryDigest, (FactoryDigest, FactoryDigest)>,
}

#[async_trait]
impl FactoryPhasePoolStore for InMemoryFactoryConfigurationStore {
  async fn phase_pool_selection_for_claim(
    &self,
    run: octacity_server_factory::FactoryRunId,
    claim: FactoryDigest,
  ) -> Result<Option<PhasePoolSelection>, StoreError> {
    let state = self.lock()?;
    Ok(
      state
        .phase_pools
        .selections
        .values()
        .find(|selection| selection.entry.input.run_id == run && selection.run_claim().id == claim)
        .cloned(),
    )
  }

  async fn publish_phase_ready(
    &self,
    policy: PhasePoolPolicy,
    input: PhasePoolInput,
  ) -> Result<PhasePoolEntry, StoreError> {
    let mut state = self.lock()?;
    let snapshot = state
      .factory_runs
      .get(&input.run_id)
      .ok_or_else(pool_conflict)?
      .snapshot()?;
    let entry = derive_phase_pool_entry(&policy, input, &snapshot)?;
    if let Some(existing) = state.phase_pools.entries.get(&entry.id) {
      return Ok(existing.clone());
    }
    if state.phase_pools.entries.values().any(|existing| {
      existing.policy_digest == entry.policy_digest
        && existing.input.run_version == entry.input.run_version
        && same_target(existing, &entry)
    }) {
      return Err(pool_conflict());
    }
    state.phase_pools.policies.insert(policy.digest(), policy);
    state.phase_pools.entries.insert(entry.id, entry.clone());
    Ok(entry)
  }

  async fn phase_ready_entries(
    &self,
    policy: FactoryDigest,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<PhasePoolEntry>, StoreError> {
    if limit == 0 || limit > MAX_PHASE_POOL_BATCH {
      return Err(pool_invalid());
    }
    let state = self.lock()?;
    let policy = state.phase_pools.policies.get(&policy).ok_or_else(pool_conflict)?;
    let after = after
      .map(|id| {
        state
          .phase_pools
          .entries
          .get(&id)
          .filter(|row| row.policy_digest == policy.digest())
          .ok_or_else(pool_invalid)
      })
      .transpose()?;
    let mut entries = Vec::new();
    for entry in state.phase_pools.entries.values().filter(|row| {
      row.policy_digest == policy.digest()
        && !state
          .phase_pools
          .selections
          .values()
          .any(|selected| same_target(row, &selected.entry))
    }) {
      let snapshot = state
        .factory_runs
        .get(&entry.input.run_id)
        .ok_or(StoreError::Unavailable)?
        .snapshot()?;
      if derive_phase_pool_entry(policy, entry.input.clone(), &snapshot).is_ok_and(|current| current == *entry)
        && after.is_none_or(|cursor| compare_phase_pool_entries(policy, entry, cursor).is_gt())
      {
        entries.push(entry.clone());
      }
    }
    entries.sort_by(|left, right| compare_phase_pool_entries(policy, left, right));
    entries.truncate(usize::from(limit));
    Ok(entries)
  }

  async fn select_phase_ready(&self, request: SelectPhasePool) -> Result<Vec<PhasePoolSelection>, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    if let Some((existing, selected)) = state.phase_pools.passes.get(&request.request_id) {
      if existing != &request {
        return Err(pool_conflict());
      }
      return Ok(selected.clone());
    }
    let policy = state
      .phase_pools
      .policies
      .get(&request.policy_digest)
      .ok_or_else(pool_conflict)?;
    let mut entries = state
      .phase_pools
      .entries
      .values()
      .filter(|row| {
        row.policy_digest == policy.digest()
          && !state
            .phase_pools
            .selections
            .values()
            .any(|selected| same_target(row, &selected.entry))
      })
      .filter(|entry| {
        state
          .factory_runs
          .get(&entry.input.run_id)
          .is_some_and(|run| run.run().version() == entry.input.run_version)
      })
      .cloned()
      .collect::<Vec<_>>();
    entries.sort_by(|left, right| compare_phase_pool_entries(policy, left, right));
    if let Some(cursor) = state
      .phase_pools
      .scan_after
      .get(&policy.digest())
      .and_then(|(id, guard)| {
        let cursor = state.phase_pools.entries.get(id)?;
        (*guard == scan_guard(&state, policy, &request, cursor)).then_some(cursor)
      })
    {
      entries.retain(|entry| compare_phase_pool_entries(policy, entry, cursor).is_gt());
    }
    entries.truncate(usize::from(MAX_PHASE_POOL_BATCH));
    let next_scan = if entries.len() == usize::from(MAX_PHASE_POOL_BATCH) {
      entries.last().map(|entry| entry.id)
    } else {
      None
    };
    let mut candidates = Vec::new();
    for entry in entries {
      let snapshot = state
        .factory_runs
        .get(&entry.input.run_id)
        .ok_or(StoreError::Unavailable)?
        .snapshot()?;
      let dependencies = entry
        .input
        .dependencies
        .iter()
        .map(|dep| {
          state
            .factory_runs
            .get(&dep.run_id)
            .map(|run| run.snapshot())
            .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
      candidates.push(observe_phase_pool_candidate(
        policy,
        &entry,
        &snapshot,
        &dependencies,
        request.observed_at,
      ));
    }
    let mut active = Vec::new();
    for selection in state
      .phase_pools
      .selections
      .values()
      .filter(|row| row.entry.policy_digest == policy.digest())
    {
      let snapshot = state
        .factory_runs
        .get(&selection.entry.input.run_id)
        .ok_or(StoreError::Unavailable)?
        .snapshot()?;
      if phase_pool_selection_active(selection, &snapshot, request.observed_at) {
        active.push(selection.clone());
      }
    }
    active.sort_by_key(|row| row.entry.id);
    let selected = plan_phase_pool_selection(policy, &request, &candidates, &active)?;
    let policy_id = policy.digest();
    let next_scan = if selected.is_empty() && active.len() < usize::from(policy.max_wip) {
      next_scan
    } else {
      None
    };
    let next_scan = next_scan.map(|id| {
      let cursor = state.phase_pools.entries.get(&id).expect("inspected entry exists");
      (id, scan_guard(&state, policy, &request, cursor))
    });
    for selection in &selected {
      let run = state
        .factory_runs
        .get_mut(&selection.entry.input.run_id)
        .ok_or(StoreError::Unavailable)?;
      let claim = selection.run_claim();
      let audit = crate::FactoryAuditFact::new(
        run.run().id(),
        crate::AuditActorKind::Worker,
        Some(digest("phase-pool-owner", &selection.owner)),
        octacity_server_factory::FactoryKey::new("factory.phase_selected").expect("static key"),
        selection.input_digest,
        octacity_server_factory::FactoryKey::new("accepted").expect("static key"),
        request.observed_at,
      );
      run.current_claim_id = Some(claim.id);
      run.claims.insert(claim.id, claim);
      run.audit.insert(audit.id, audit);
      state
        .phase_pools
        .selections
        .insert(selection.entry.id, selection.clone());
    }
    state
      .phase_pools
      .passes
      .insert(request.request_id, (request, selected.clone()));
    if let Some(cursor) = next_scan {
      state.phase_pools.scan_after.insert(policy_id, cursor);
    } else {
      state.phase_pools.scan_after.remove(&policy_id);
    }
    Ok(selected)
  }
}

// Guard only the skipped prefix and reserved capacity. Lower-ranked unrelated
// publications must not reset progress behind a permanently blocked prefix.
fn scan_guard(
  state: &crate::factory_configuration_testing::FactoryConfigurationMemoryState,
  policy: &PhasePoolPolicy,
  request: &SelectPhasePool,
  cursor: &PhasePoolEntry,
) -> FactoryDigest {
  let mut run_ids = BTreeSet::new();
  let mut entry_ids = Vec::new();
  for entry in state
    .phase_pools
    .entries
    .values()
    .filter(|row| row.policy_digest == policy.digest() && !compare_phase_pool_entries(policy, row, cursor).is_gt())
  {
    entry_ids.push(entry.id);
    run_ids.insert(entry.input.run_id);
    run_ids.extend(entry.input.dependencies.iter().map(|dep| dep.run_id));
  }
  for selection in state
    .phase_pools
    .selections
    .values()
    .filter(|row| row.entry.policy_digest == policy.digest())
  {
    run_ids.insert(selection.entry.input.run_id);
  }
  let observations = run_ids
    .into_iter()
    .map(|id| {
      (
        id,
        state
          .factory_runs
          .get(&id)
          .map(|run| run.phase_pool_scan_digest(request.observed_at)),
      )
    })
    .collect::<Vec<_>>();
  digest("phase-pool-scan", &(&request.capabilities, entry_ids, observations))
}

fn same_target(left: &PhasePoolEntry, right: &PhasePoolEntry) -> bool {
  (
    left.input.run_id,
    left.input.flow_run_id,
    left.input.cycle_id,
    &left.input.node,
    left.input.generation,
  ) == (
    right.input.run_id,
    right.input.flow_run_id,
    right.input.cycle_id,
    &right.input.node,
    right.input.generation,
  )
}
