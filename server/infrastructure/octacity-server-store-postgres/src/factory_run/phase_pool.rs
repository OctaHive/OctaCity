use std::collections::{BTreeMap, BTreeSet};

use octacity_server_domain::EntityKind;
use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryRunId, MAX_PHASE_POOL_BATCH, MAX_PHASE_POOL_WIP, PhasePoolOrder, PhasePoolPolicy,
};
use octacity_server_store::{
  FactoryAuditFact, FactoryRunSnapshot, PhasePoolEntry, PhasePoolInput, PhasePoolSelection, SelectPhasePool,
  StoreError, StoreInputError, StoreOperation, derive_phase_pool_entry, observe_phase_pool_candidate,
  phase_pool_selection_active, plan_phase_pool_selection,
};
use sqlx::{PgPool, Postgres, Transaction, types::Json};

use super::{conflict, insert_audit, insert_claim, snapshot::read_snapshot_in_transaction};
use crate::database::unavailable;

// Pool operations serialize capacity reservations before acquiring Run locks.
// Waiting preserves the total order; SKIP LOCKED could select a lower-ranked Work.
async fn lock_capacity(transaction: &mut Transaction<'_, Postgres>) -> Result<(), StoreError> {
  sqlx::query("SELECT pg_advisory_xact_lock(739450)")
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}

pub(crate) async fn publish_phase_ready(
  pool: &PgPool,
  policy: PhasePoolPolicy,
  input: PhasePoolInput,
) -> Result<PhasePoolEntry, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  lock_capacity(&mut transaction).await?;
  let snapshot = read_snapshot_in_transaction(&mut transaction, input.run_id).await?;
  let entry = derive_phase_pool_entry(&policy, input, &snapshot)?;
  let existing: Option<Json<PhasePoolEntry>> =
    sqlx::query_scalar("SELECT entry FROM factory_phase_pool_entries WHERE id = $1")
      .bind(entry.id.as_bytes().as_slice())
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?;
  if let Some(Json(existing)) = existing {
    if existing != entry {
      return Err(conflict());
    }
    transaction.commit().await.map_err(unavailable)?;
    return Ok(existing);
  }
  let occupied: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM factory_phase_pool_entries WHERE policy_id = $1 AND run_id = $2 AND run_version = $3 AND flow_run_id = $4 AND cycle_id = $5 AND node_key = $6)")
    .bind(entry.policy_digest.as_bytes().as_slice()).bind(entry.input.run_id.as_uuid()).bind(i64::try_from(entry.input.run_version.get()).map_err(|_| invalid())?)
    .bind(entry.input.flow_run_id.as_uuid()).bind(entry.input.cycle_id.as_uuid()).bind(entry.input.node.as_str()).fetch_one(&mut *transaction).await.map_err(unavailable)?;
  if occupied {
    return Err(conflict());
  }
  sqlx::query("INSERT INTO factory_phase_pool_policies (id, policy) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
    .bind(policy.digest().as_bytes().as_slice())
    .bind(Json(&policy))
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let rank = rank(&policy, &entry);
  sqlx::query("INSERT INTO factory_phase_pool_entries (id, policy_id, run_id, run_version, flow_run_id, cycle_id, node_key, entry, rank_one, rank_two, rank_three, work_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
    .bind(entry.id.as_bytes().as_slice()).bind(entry.policy_digest.as_bytes().as_slice()).bind(entry.input.run_id.as_uuid())
    .bind(i64::try_from(entry.input.run_version.get()).map_err(|_| invalid())?).bind(entry.input.flow_run_id.as_uuid())
    .bind(entry.input.cycle_id.as_uuid()).bind(entry.input.node.as_str()).bind(Json(&entry))
    .bind(rank[0]).bind(rank[1]).bind(rank[2]).bind(entry.work_id.as_uuid())
    .execute(&mut *transaction).await.map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(entry)
}

pub(crate) async fn phase_ready_entries(
  pool: &PgPool,
  policy: FactoryDigest,
  after: Option<FactoryDigest>,
  limit: u16,
) -> Result<Vec<PhasePoolEntry>, StoreError> {
  if limit == 0 || limit > MAX_PHASE_POOL_BATCH {
    return Err(invalid());
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  lock_capacity(&mut transaction).await?;
  let policy = read_policy(&mut transaction, policy).await?;
  if let Some(after) = after {
    let exists: bool =
      sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM factory_phase_pool_entries WHERE id = $1 AND policy_id = $2)")
        .bind(after.as_bytes().as_slice())
        .bind(policy.digest().as_bytes().as_slice())
        .fetch_one(&mut *transaction)
        .await
        .map_err(unavailable)?;
    if !exists {
      return Err(invalid());
    }
  }
  let entries = ordered_entries(&mut transaction, policy.digest(), after, limit).await?;
  let mut current = Vec::new();
  for entry in entries {
    let snapshot = read_snapshot_in_transaction(&mut transaction, entry.input.run_id).await?;
    if derive_phase_pool_entry(&policy, entry.input.clone(), &snapshot).is_ok_and(|derived| derived == entry) {
      current.push(entry);
    }
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(current)
}

pub(crate) async fn select_phase_ready(
  pool: &PgPool,
  request: SelectPhasePool,
) -> Result<Vec<PhasePoolSelection>, StoreError> {
  request.validate()?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  lock_capacity(&mut transaction).await?;
  let existing: Option<(Json<SelectPhasePool>, Json<Vec<PhasePoolSelection>>)> =
    sqlx::query_as("SELECT request, selections FROM factory_phase_pool_passes WHERE id = $1")
      .bind(request.request_id.as_bytes().as_slice())
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?;
  if let Some((Json(existing), Json(selections))) = existing {
    if existing != request {
      return Err(conflict());
    }
    transaction.commit().await.map_err(unavailable)?;
    return Ok(selections);
  }
  let policy = read_policy(&mut transaction, request.policy_digest).await?;
  let (scan_after, guard): (Option<Vec<u8>>, Option<Vec<u8>>) =
    sqlx::query_as("SELECT scan_after, scan_guard FROM factory_phase_pool_policies WHERE id = $1")
      .bind(policy.digest().as_bytes().as_slice())
      .fetch_one(&mut *transaction)
      .await
      .map_err(unavailable)?;
  let scan_after = scan_after
    .map(|bytes| {
      bytes
        .try_into()
        .map(FactoryDigest::from_bytes)
        .map_err(|_| StoreError::Unavailable)
    })
    .transpose()?;
  let scan_after = if let Some(cursor) = scan_after {
    let current = scan_guard(&mut transaction, &request, cursor).await?;
    (guard.as_deref() == Some(current.as_bytes().as_slice())).then_some(cursor)
  } else {
    None
  };
  let entries = ordered_entries(&mut transaction, policy.digest(), scan_after, MAX_PHASE_POOL_BATCH).await?;
  let next_scan = if entries.len() == usize::from(MAX_PHASE_POOL_BATCH) {
    entries.last().map(|entry| entry.id)
  } else {
    None
  };
  let selections: Vec<Json<PhasePoolSelection>> = sqlx::query_scalar(
    "SELECT selected.selection FROM factory_phase_pool_selections selected \
     JOIN factory_runs run ON run.id = selected.run_id \
     WHERE selected.policy_id = $1 AND run.visible AND run.state NOT IN ('rejected','cancelled','completed') \
       AND ((NOT EXISTS (SELECT 1 FROM factory_node_attempts node WHERE node.run_id = selected.run_id AND node.workflow_cycle_id = selected.cycle_id AND node.node_key = selected.node_key) AND selected.cycle_id = (SELECT id FROM factory_workflow_cycles WHERE flow_run_id = selected.flow_run_id ORDER BY cycle_number DESC LIMIT 1)) OR EXISTS ( \
         SELECT 1 FROM factory_node_attempts node WHERE node.run_id = selected.run_id \
           AND node.workflow_cycle_id = selected.cycle_id AND node.node_key = selected.node_key \
           AND NOT EXISTS (SELECT 1 FROM factory_node_attempt_completions result WHERE result.node_attempt_id = node.id))) \
     ORDER BY selected.entry_id LIMIT $2")
    .bind(policy.digest().as_bytes().as_slice()).bind(i64::from(MAX_PHASE_POOL_WIP) + 1).fetch_all(&mut *transaction).await.map_err(unavailable)?;
  if selections.len() > usize::from(MAX_PHASE_POOL_WIP) {
    return Err(invalid());
  }
  let mut run_ids = BTreeSet::new();
  for entry in &entries {
    run_ids.insert(entry.input.run_id);
    run_ids.extend(entry.input.dependencies.iter().map(|dep| dep.run_id));
  }
  run_ids.extend(selections.iter().map(|Json(row)| row.entry.input.run_id));
  let raw_ids = run_ids.iter().map(|id| id.as_uuid()).collect::<Vec<_>>();
  sqlx::query("SELECT id FROM factory_runs WHERE id = ANY($1) ORDER BY id FOR UPDATE")
    .bind(raw_ids)
    .fetch_all(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let mut snapshots = BTreeMap::<FactoryRunId, FactoryRunSnapshot>::new();
  for run_id in run_ids {
    match read_snapshot_in_transaction(&mut transaction, run_id).await {
      Ok(snapshot) => {
        snapshots.insert(run_id, snapshot);
      }
      Err(StoreError::NotFound { .. }) => {}
      Err(error) => return Err(error),
    }
  }
  let mut candidates = Vec::new();
  for entry in entries {
    if let Some(snapshot) = snapshots.get(&entry.input.run_id) {
      let dependencies = entry
        .input
        .dependencies
        .iter()
        .map(|dep| snapshots.get(&dep.run_id).cloned())
        .collect::<Vec<_>>();
      candidates.push(observe_phase_pool_candidate(
        &policy,
        &entry,
        snapshot,
        &dependencies,
        request.observed_at,
      ));
    }
  }
  let active = selections
    .into_iter()
    .map(|Json(row)| row)
    .filter(|row| {
      snapshots
        .get(&row.entry.input.run_id)
        .is_some_and(|snapshot| phase_pool_selection_active(row, snapshot, request.observed_at))
    })
    .collect::<Vec<_>>();
  let selected = plan_phase_pool_selection(&policy, &request, &candidates, &active)?;
  let next_scan = if selected.is_empty() && active.len() < usize::from(policy.max_wip) {
    next_scan
  } else {
    None
  };
  let next_guard = if let Some(cursor) = next_scan {
    Some(scan_guard(&mut transaction, &request, cursor).await?)
  } else {
    None
  };
  // Candidate and dependency Runs are locked. Earlier skipped Runs need only
  // this final optimistic recheck: concurrent changes discard the pass rather
  // than select lower-ranked Work or freeze unchecked facts into continuation.
  if let Some(cursor) = scan_after {
    let current = scan_guard(&mut transaction, &request, cursor).await?;
    if guard.as_deref() != Some(current.as_bytes().as_slice()) {
      return Err(conflict());
    }
  }
  sqlx::query("UPDATE factory_phase_pool_policies SET scan_after = $2, scan_guard = $3 WHERE id = $1")
    .bind(policy.digest().as_bytes().as_slice())
    .bind(next_scan.map(|id| id.as_bytes().to_vec()))
    .bind(next_guard.map(|id| id.as_bytes().to_vec()))
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  sqlx::query("INSERT INTO factory_phase_pool_passes (id, policy_id, request, selections) VALUES ($1,$2,$3,$4)")
    .bind(request.request_id.as_bytes().as_slice())
    .bind(request.policy_digest.as_bytes().as_slice())
    .bind(Json(&request))
    .bind(Json(&selected))
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  for selection in &selected {
    let entry = &selection.entry;
    let claim = selection.run_claim();
    insert_claim(&mut transaction, entry.input.run_id, &claim).await?;
    let audit = FactoryAuditFact::new(
      entry.input.run_id,
      octacity_server_store::AuditActorKind::Worker,
      Some(FactoryDigest::sha256(
        "phase-pool-owner",
        &[&serde_json::to_vec(&selection.owner).map_err(|_| invalid())?],
      )),
      FactoryKey::new("factory.phase_selected").expect("static key"),
      selection.input_digest,
      FactoryKey::new("accepted").expect("static key"),
      request.observed_at,
    );
    insert_audit(&mut transaction, &audit).await?;
    sqlx::query("UPDATE factory_run_current SET claim_id = $1 WHERE run_id = $2")
      .bind(claim.id.as_bytes().as_slice())
      .bind(entry.input.run_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    sqlx::query("INSERT INTO factory_phase_pool_selections (entry_id,pass_id,policy_id,run_id,flow_run_id,cycle_id,node_key,claim_id,selection) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
      .bind(entry.id.as_bytes().as_slice()).bind(request.request_id.as_bytes().as_slice()).bind(request.policy_digest.as_bytes().as_slice())
      .bind(entry.input.run_id.as_uuid()).bind(entry.input.flow_run_id.as_uuid()).bind(entry.input.cycle_id.as_uuid()).bind(entry.input.node.as_str())
      .bind(claim.id.as_bytes().as_slice()).bind(Json(selection)).execute(&mut *transaction).await.map_err(unavailable)?;
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(selected)
}

// Only the skipped prefix and reserved capacity contribute lightweight metadata.
// Changes that can make higher-ranked Work eligible invalidate continuation;
// unrelated lower-ranked publications cannot starve Work behind the prefix.
async fn scan_guard(
  transaction: &mut Transaction<'_, Postgres>,
  request: &SelectPhasePool,
  cursor: FactoryDigest,
) -> Result<FactoryDigest, StoreError> {
  let metadata: Vec<u8> = sqlx::query_scalar(
    "WITH entries AS (SELECT entry.id, entry.run_id, entry.entry FROM factory_phase_pool_entries entry \
       JOIN factory_phase_pool_entries cursor ON cursor.id = $3 WHERE entry.policy_id = $1 AND \
         (entry.rank_one,entry.rank_two,entry.rank_three,entry.work_id,entry.run_id,entry.flow_run_id,entry.cycle_id,entry.node_key,entry.id) <= \
         (cursor.rank_one,cursor.rank_two,cursor.rank_three,cursor.work_id,cursor.run_id,cursor.flow_run_id,cursor.cycle_id,cursor.node_key,cursor.id)), \
     run_ids AS (SELECT run_id FROM entries UNION \
       SELECT (dep->>'run_id')::uuid FROM entries, LATERAL jsonb_array_elements(entry->'input'->'dependencies') dep UNION \
       SELECT run_id FROM factory_phase_pool_selections WHERE policy_id = $1), \
     observations AS (SELECT jsonb_build_array('entry', encode(id, 'hex')) AS fact FROM entries UNION ALL \
       SELECT jsonb_build_array('run', run.id, run.version, run.visible, run.state, to_jsonb(current), \
         claim.expires_at > to_timestamp($2::double precision / 1000.0), \
         run.admitted_at <= to_timestamp($2::double precision / 1000.0)) \
       FROM run_ids JOIN factory_runs run ON run.id = run_ids.run_id \
       JOIN factory_run_current current ON current.run_id = run.id \
       LEFT JOIN factory_run_claims claim ON claim.id = current.claim_id) \
     SELECT sha256(convert_to(COALESCE(jsonb_agg(fact ORDER BY fact), '[]'::jsonb)::text, 'UTF8')) FROM observations",
  )
  .bind(request.policy_digest.as_bytes().as_slice())
  .bind(request.observed_at.unix_millis())
  .bind(cursor.as_bytes().as_slice())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(FactoryDigest::sha256(
    "phase-pool-scan",
    &[
      &metadata,
      &serde_json::to_vec(&request.capabilities).map_err(|_| invalid())?,
    ],
  ))
}

async fn read_policy(
  transaction: &mut Transaction<'_, Postgres>,
  id: FactoryDigest,
) -> Result<PhasePoolPolicy, StoreError> {
  let Json(policy): Json<PhasePoolPolicy> =
    sqlx::query_scalar("SELECT policy FROM factory_phase_pool_policies WHERE id = $1")
      .bind(id.as_bytes().as_slice())
      .fetch_optional(&mut **transaction)
      .await
      .map_err(unavailable)?
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?;
  policy.validate().map_err(|_| StoreError::Unavailable)?;
  if policy.digest() != id {
    return Err(StoreError::Unavailable);
  }
  Ok(policy)
}

async fn ordered_entries(
  transaction: &mut Transaction<'_, Postgres>,
  policy: FactoryDigest,
  after: Option<FactoryDigest>,
  limit: u16,
) -> Result<Vec<PhasePoolEntry>, StoreError> {
  let rows: Vec<Json<PhasePoolEntry>> = sqlx::query_scalar(
    "SELECT entry.entry FROM factory_phase_pool_entries entry JOIN factory_runs run ON run.id = entry.run_id \
     LEFT JOIN factory_phase_pool_entries cursor ON cursor.id = $2 \
     WHERE entry.policy_id = $1 AND run.visible AND run.version = entry.run_version \
       AND run.state NOT IN ('rejected','cancelled','completed') \
       AND NOT EXISTS (SELECT 1 FROM factory_phase_pool_selections selected WHERE selected.run_id = entry.run_id \
         AND selected.flow_run_id = entry.flow_run_id AND selected.cycle_id = entry.cycle_id AND selected.node_key = entry.node_key) \
       AND NOT EXISTS (SELECT 1 FROM factory_node_attempts node WHERE node.run_id = entry.run_id \
         AND node.workflow_cycle_id = entry.cycle_id AND node.node_key = entry.node_key) \
       AND ($2::bytea IS NULL OR (entry.rank_one,entry.rank_two,entry.rank_three,entry.work_id,entry.run_id,entry.flow_run_id,entry.cycle_id,entry.node_key,entry.id) > \
         (cursor.rank_one,cursor.rank_two,cursor.rank_three,cursor.work_id,cursor.run_id,cursor.flow_run_id,cursor.cycle_id,cursor.node_key,cursor.id)) \
     ORDER BY entry.rank_one,entry.rank_two,entry.rank_three,entry.work_id,entry.run_id,entry.flow_run_id,entry.cycle_id,entry.node_key,entry.id LIMIT $3")
    .bind(policy.as_bytes().as_slice()).bind(after.map(|id| id.as_bytes().to_vec())).bind(i64::from(limit))
    .fetch_all(&mut **transaction).await.map_err(unavailable)?;
  Ok(rows.into_iter().map(|Json(row)| row).collect())
}

fn rank(policy: &PhasePoolPolicy, entry: &PhasePoolEntry) -> [i64; 3] {
  std::array::from_fn(|index| match policy.order[index] {
    PhasePoolOrder::Severity => -(entry.input.severity as i64),
    PhasePoolOrder::ProjectPriority => -i64::from(entry.input.project_priority),
    PhasePoolOrder::Age => entry.admitted_at.unix_millis(),
  })
}

fn invalid() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::SelectFactoryPhasePool,
    source: StoreInputError::InvalidFactoryPhasePool,
  }
}
