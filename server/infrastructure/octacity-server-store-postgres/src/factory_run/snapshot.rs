use std::collections::{HashMap, HashSet};

use octacity_server_factory::{
  AdmittedFlow, BudgetUsage, EvaluationState, FactoryConfiguration, FactoryLifecycleProgress, FactoryRunId,
  FactoryRunVersion, FlowDefinition, FlowDefinitionId, FlowDefinitionVersion, FlowRun, FlowRunId, FlowRunParent,
  MacroCall, NodeAttemptId, PinnedFlowDefinitionClosure, WorkflowCycle, WorkflowCycleId, WorkflowCycleNumber,
};
use octacity_server_store::{
  FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryRunHistoryAppend, FactoryRunSnapshot,
  MAX_FACTORY_RUN_SNAPSHOT_RECORDS, StoreError,
};
use sqlx::{PgPool, types::Json};

use super::{
  FactoryAuditRow, LifecycleDocument, OutboxRow, decode_audit_row, decode_claim, decode_outbox, digest, history_rows,
  lock_run,
};
use crate::{
  database::unavailable,
  discovery::{positive, timestamp},
};

pub(crate) async fn read_snapshot(pool: &PgPool, run_id: FactoryRunId) -> Result<FactoryRunSnapshot, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let snapshot = read_snapshot_in_transaction(&mut transaction, run_id).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(snapshot)
}

pub(super) async fn read_snapshot_in_transaction(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
) -> Result<FactoryRunSnapshot, StoreError> {
  let locked = lock_run(transaction, run_id).await?;
  let admitted_flow = read_admitted_flow(transaction, &locked).await?;
  let flow_runs = read_flow_runs(transaction, run_id, &admitted_flow).await?;
  let workflow_cycles = read_workflow_cycles(transaction, run_id, &flow_runs).await?;
  let (record_count, control_count) = snapshot_record_counts(transaction, run_id).await?;
  if record_count > MAX_FACTORY_RUN_SNAPSHOT_RECORDS || control_count != 0 {
    return Err(StoreError::Unavailable);
  }
  let claims = sqlx::query_as::<_, (uuid::Uuid, String, Vec<u8>, i64, i64)>(
    "SELECT run_id, owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT, \
            FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT \
     FROM factory_run_claims WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(decode_claim)
  .collect::<Result<Vec<_>, _>>()?;
  let budgets = sqlx::query_as::<_, (Vec<u8>, i64, Json<BudgetUsage>, i64)>(
    "SELECT id, run_version, usage, FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT \
     FROM factory_run_budgets WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(|(id, version, Json(usage), recorded_at)| {
    Ok(FactoryBudgetRecord {
      id: digest(&id)?,
      run_version: FactoryRunVersion::new(positive(version)?).map_err(|_| StoreError::Unavailable)?,
      usage,
      recorded_at: timestamp(recorded_at)?,
    })
  })
  .collect::<Result<Vec<_>, StoreError>>()?;
  let lifecycle_checkpoints = sqlx::query_as::<_, (Vec<u8>, i64, Json<LifecycleDocument>, bool, i64)>(
    "SELECT id, run_version, lifecycle, cancellation_requested, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT \
     FROM factory_lifecycle_checkpoints WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(|(id, version, Json(document), cancellation_requested, recorded_at)| {
    Ok(FactoryLifecycleCheckpoint {
      id: digest(&id)?,
      run_id,
      run_version: FactoryRunVersion::new(positive(version)?).map_err(|_| StoreError::Unavailable)?,
      progress: document.progress,
      signal: document.signal,
      cancellation_requested,
      recorded_at: timestamp(recorded_at)?,
    })
  })
  .collect::<Result<Vec<_>, StoreError>>()?;
  let audit = sqlx::query_as::<_, FactoryAuditRow>(
    "SELECT id, run_id, actor_kind, actor_identity_digest, operation, request_identity_digest, outcome, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis \
     FROM factory_audit_links WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(decode_audit_row)
  .collect::<Result<Vec<_>, _>>()?;
  let outbox = outbox_rows(transaction, run_id).await?;
  let current_claim = locked
    .claim_id
    .map(|id| {
      claims
        .iter()
        .find(|record| record.id == id)
        .cloned()
        .ok_or(StoreError::Unavailable)
    })
    .transpose()?;
  let history = history_rows(
    transaction,
    run_id,
    &admitted_flow,
    &flow_runs,
    &workflow_cycles,
    &locked.work,
  )
  .await?;
  validate_history(run_id, &admitted_flow, &history)?;
  validate_evaluation_progress(&locked.lifecycle, &locked.current, &history)?;
  Ok(FactoryRunSnapshot {
    work: locked.work,
    run: locked.run,
    flow: octacity_server_store::FactoryFlowHistory {
      runs: flow_runs,
      cycles: workflow_cycles,
      attempts: history.flow.attempts,
      completions: history.flow.completions,
      data: history.flow.data,
    },
    admitted_flow,
    current_claim,
    claims,
    budgets,
    lifecycle_checkpoints,
    stage_attempts: history.stage_attempts,
    stage_attempt_completions: history.stage_attempt_completions,
    stage_handoffs: history.stage_handoffs,
    context_manifests: history.context_manifests,
    macro_calls: history.macro_calls,
    macro_call_completions: history.macro_call_completions,
    signal_requests: history.signal_requests,
    signal_receipts: history.signal_receipts,
    linked_builds: history.linked_builds,
    build_observations: history.build_observations,
    candidates: history.candidates,
    evidence: history.evidence,
    evaluation_plans: history.evaluation_plans,
    assessments: history.assessments,
    decisions: history.decisions,
    escalations: history.escalations,
    delivery_attempts: history.delivery_attempts,
    reporting_attempts: history.reporting_attempts,
    audit,
    controls: Vec::new(),
    outbox,
    current: locked.current,
  })
}

pub(super) async fn read_admitted_flow(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  locked: &super::LockedRun,
) -> Result<AdmittedFlow, StoreError> {
  let Json(configuration): Json<FactoryConfiguration> = sqlx::query_scalar(
    "SELECT definition FROM factory_configuration_versions \
     WHERE factory_configuration_id = $1 AND version = $2",
  )
  .bind(locked.run.configuration().id().as_uuid())
  .bind(i64::try_from(locked.run.configuration().version().get()).map_err(|_| StoreError::Unavailable)?)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let rows = sqlx::query_as::<_, (Json<FlowDefinition>, bool)>(
    "SELECT definition.definition, link.is_root \
     FROM factory_configuration_flow_definitions AS link \
     JOIN factory_flow_definition_versions AS definition \
       ON definition.id = link.flow_definition_id AND definition.version = link.flow_definition_version \
     WHERE link.factory_configuration_id = $1 AND link.factory_configuration_version = $2 \
     ORDER BY link.ordinal",
  )
  .bind(locked.run.configuration().id().as_uuid())
  .bind(i64::try_from(locked.run.configuration().version().get()).map_err(|_| StoreError::Unavailable)?)
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let roots = rows
    .iter()
    .filter(|(_, is_root)| *is_root)
    .map(|(definition, _)| definition.reference())
    .collect::<Vec<_>>();
  let [root] = roots.as_slice() else {
    return Err(StoreError::Unavailable);
  };
  let closure = PinnedFlowDefinitionClosure::new(
    *root,
    rows.into_iter().map(|(Json(definition), _)| definition).collect(),
  )
  .map_err(|_| StoreError::Unavailable)?;
  let expected = match configuration.flow() {
    Some(flow) => flow.closure.clone(),
    None => PinnedFlowDefinitionClosure::from_stage_projection(&configuration).map_err(|_| StoreError::Unavailable)?,
  };
  if closure != expected {
    return Err(StoreError::Unavailable);
  }
  let Json(limits): Json<octacity_server_factory::FlowAdmissionLimits> =
    sqlx::query_scalar("SELECT flow_admission_limits FROM factory_runs WHERE id = $1")
      .bind(locked.run.id().as_uuid())
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let admitted_root = FlowRun::root(&locked.run, closure.root()).map_err(|_| StoreError::Unavailable)?;
  let admitted_cycle = WorkflowCycle::initial(&admitted_root).map_err(|_| StoreError::Unavailable)?;
  let admitted = AdmittedFlow::from_configuration(&configuration, &locked.run).map_err(|_| StoreError::Unavailable)?;
  if admitted.closure() != &closure
    || admitted.limits() != &limits
    || admitted.root_run() != &admitted_root
    || admitted.initial_cycle() != &admitted_cycle
  {
    return Err(StoreError::Unavailable);
  }
  let root = admitted.root_run();
  let definition = root.definition();
  let cycle = admitted.initial_cycle();
  let persisted: (uuid::Uuid, uuid::Uuid, i64, uuid::Uuid, i64) = sqlx::query_as(
    "SELECT flow.id, flow.flow_definition_id, flow.flow_definition_version, cycle.id, cycle.cycle_number \
     FROM factory_flow_runs AS flow \
     JOIN factory_workflow_cycles AS cycle ON cycle.flow_run_id = flow.id \
     WHERE flow.factory_run_id = $1 AND flow.parent_flow_run_id IS NULL AND cycle.predecessor_id IS NULL",
  )
  .bind(locked.run.id().as_uuid())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if persisted.0 != root.id().as_uuid()
    || persisted.1 != definition.id().as_uuid()
    || i64::try_from(definition.version().get()) != Ok(persisted.2)
    || persisted.3 != cycle.id().as_uuid()
    || i64::try_from(cycle.number().get()) != Ok(persisted.4)
  {
    return Err(StoreError::Unavailable);
  }
  Ok(admitted)
}

pub(super) async fn read_flow_runs(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
  admitted: &AdmittedFlow,
) -> Result<Vec<FlowRun>, StoreError> {
  let mut pending = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, i64, Option<uuid::Uuid>, Option<uuid::Uuid>)>(
    "SELECT id, flow_definition_id, flow_definition_version, parent_flow_run_id, parent_node_attempt_id \
     FROM factory_flow_runs WHERE factory_run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let root = admitted.root_run().clone();
  let root_position = pending
    .iter()
    .position(|row| row.0 == root.id().as_uuid() && row.3.is_none() && row.4.is_none())
    .ok_or(StoreError::Unavailable)?;
  pending.remove(root_position);
  if pending.iter().any(|row| row.3.is_none() || row.4.is_none()) {
    return Err(StoreError::Unavailable);
  }
  let mut resolved = vec![root];
  while !pending.is_empty() {
    let Some(position) = pending.iter().position(|row| {
      row
        .3
        .is_some_and(|parent| resolved.iter().any(|flow_run| flow_run.id().as_uuid() == parent))
    }) else {
      return Err(StoreError::Unavailable);
    };
    let (id, definition_id, definition_version, parent_flow_id, parent_node_id) = pending.remove(position);
    let definition_id = FlowDefinitionId::from_uuid(definition_id).map_err(|_| StoreError::Unavailable)?;
    let definition_version =
      FlowDefinitionVersion::new(positive(definition_version)?).map_err(|_| StoreError::Unavailable)?;
    let definition = admitted
      .closure()
      .definitions()
      .iter()
      .find(|definition| {
        definition.reference().id() == definition_id && definition.reference().version() == definition_version
      })
      .ok_or(StoreError::Unavailable)?
      .reference();
    resolved.push(FlowRun::nested(
      FlowRunId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
      run_id,
      definition,
      FlowRunParent::new(
        FlowRunId::from_uuid(parent_flow_id.ok_or(StoreError::Unavailable)?).map_err(|_| StoreError::Unavailable)?,
        NodeAttemptId::from_uuid(parent_node_id.ok_or(StoreError::Unavailable)?)
          .map_err(|_| StoreError::Unavailable)?,
      ),
    ));
  }
  Ok(resolved)
}

pub(super) async fn read_workflow_cycles(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
  flow_runs: &[FlowRun],
) -> Result<Vec<WorkflowCycle>, StoreError> {
  let rows = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, i64, Option<uuid::Uuid>)>(
    "SELECT id, flow_run_id, cycle_number, predecessor_id FROM factory_workflow_cycles \
     WHERE factory_run_id = $1 ORDER BY cycle_number, id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let mut cycles = Vec::with_capacity(rows.len());
  for (id, flow_run_id, number, predecessor_id) in rows {
    let flow_run_id = FlowRunId::from_uuid(flow_run_id).map_err(|_| StoreError::Unavailable)?;
    let flow_run = flow_runs
      .iter()
      .find(|flow_run| flow_run.id() == flow_run_id)
      .ok_or(StoreError::Unavailable)?;
    let number = WorkflowCycleNumber::new(positive(number)?).map_err(|_| StoreError::Unavailable)?;
    let cycle = match predecessor_id {
      None if number == WorkflowCycleNumber::INITIAL => {
        let cycle = WorkflowCycle::initial(flow_run).map_err(|_| StoreError::Unavailable)?;
        (cycle.id().as_uuid() == id)
          .then_some(cycle)
          .ok_or(StoreError::Unavailable)?
      }
      Some(predecessor_id) if number != WorkflowCycleNumber::INITIAL => {
        let predecessor = WorkflowCycleId::from_uuid(predecessor_id).map_err(|_| StoreError::Unavailable)?;
        let previous = cycles
          .iter()
          .find(|cycle: &&WorkflowCycle| cycle.id() == predecessor && cycle.flow_run_id() == flow_run_id)
          .ok_or(StoreError::Unavailable)?;
        if previous.number().get().checked_add(1) != Some(number.get()) {
          return Err(StoreError::Unavailable);
        }
        WorkflowCycle::next(
          WorkflowCycleId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
          flow_run_id,
          number,
          predecessor,
        )
      }
      _ => return Err(StoreError::Unavailable),
    };
    cycles.push(cycle);
  }
  if flow_runs
    .iter()
    .any(|flow_run| !cycles.iter().any(|cycle| cycle.flow_run_id() == flow_run.id()))
  {
    return Err(StoreError::Unavailable);
  }
  Ok(cycles)
}

fn validate_evaluation_progress(
  lifecycle: &FactoryLifecycleCheckpoint,
  current: &octacity_server_store::FactoryRunCurrentProjection,
  history: &FactoryRunHistoryAppend,
) -> Result<(), StoreError> {
  let FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(progress)) = &lifecycle.progress else {
    return Ok(());
  };
  if !progress.is_plan_bound() {
    return Ok(());
  }
  let plan = current
    .evaluation_plan_id
    .and_then(|id| history.evaluation_plans.iter().find(|plan| plan.id() == id))
    .ok_or(StoreError::Unavailable)?;
  progress
    .validate_bindings(
      plan,
      history.stage_attempts.iter(),
      history.macro_calls.iter(),
      history.assessments.iter(),
    )
    .map_err(|_| StoreError::Unavailable)
}

pub(super) async fn snapshot_record_counts(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
) -> Result<(usize, usize), StoreError> {
  let (record_count, control_count): (i64, i64) = sqlx::query_as(
    "SELECT \
       (SELECT COUNT(*) FROM factory_run_claims WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_run_budgets WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_lifecycle_checkpoints WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_flow_runs WHERE factory_run_id = $1) + \
       (SELECT COUNT(*) FROM factory_workflow_cycles WHERE factory_run_id = $1) + \
       (SELECT COUNT(*) FROM factory_node_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_flow_build_executions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_flow_build_intents WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_flow_incoming WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_flow_inputs WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_flow_records WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_node_attempt_completions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_stage_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_stage_attempt_completions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_stage_handoffs WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_context_manifests WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_call_nodes WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_call_completions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_decision_signal_requests WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_decision_signal_receipts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_build_links WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_build_observations WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_changesets WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_evidence_manifests WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_evaluation_plans WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_assessments WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_decisions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_escalations WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_delivery_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_reporting_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_audit_links WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_outbox_records WHERE run_id = $1), \
       (SELECT COUNT(*) FROM factory_run_controls WHERE run_id = $1)",
  )
  .bind(run_id.as_uuid())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok((
    usize::try_from(record_count).map_err(|_| StoreError::Unavailable)?,
    usize::try_from(control_count).map_err(|_| StoreError::Unavailable)?,
  ))
}

fn validate_history(
  run_id: FactoryRunId,
  admitted_flow: &AdmittedFlow,
  history: &octacity_server_store::FactoryRunHistoryAppend,
) -> Result<(), StoreError> {
  let stages = history
    .stage_attempts
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let nodes = history
    .flow
    .attempts
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let calls = history
    .macro_calls
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let call_records = history
    .macro_calls
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let handoffs = history
    .stage_handoffs
    .iter()
    .map(|record| (record.stage_attempt_id(), record))
    .collect::<HashMap<_, _>>();
  let manifests = history
    .context_manifests
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let requests = history
    .signal_requests
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let receipts = history
    .signal_receipts
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let builds = history
    .linked_builds
    .iter()
    .map(|record| (record.build_id, record))
    .collect::<HashMap<_, _>>();
  let candidates = history
    .candidates
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let evidence = history
    .evidence
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let plans = history
    .evaluation_plans
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let assessments = history
    .assessments
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let decisions = history
    .decisions
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let invalid = history.stage_attempts.iter().any(|record| {
    record.run_id() != run_id
      || nodes
        .values()
        .all(|node| !admitted_flow.matches_stage_projection(record, node))
  }) || history.flow.attempts.iter().any(|node| {
    node.stage_projection_id().is_some_and(|stage_id| {
      history
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == stage_id)
        .is_none_or(|stage| !admitted_flow.matches_stage_projection(stage, node))
    })
  }) || history.flow.completions.iter().any(|completion| {
    nodes.get(&completion.node_attempt_id()).is_none_or(|node| {
      node.factory_run_id() != run_id
        || node.flow_run_id() != completion.flow_run_id()
        || node.workflow_cycle_id() != completion.workflow_cycle_id()
        || node.node_key() != completion.node_key()
        || node
          .verify_observer(
            &octacity_server_factory::FactoryClaimOwnership::new(completion.owner().clone(), completion.claim()),
            completion.observed_at(),
          )
          .is_err()
        || completion.usage().validate(node.budget()).is_err()
    })
  }) || history
    .stage_attempt_completions
    .iter()
    .any(|record| record.run_id() != run_id || !stages.contains(&record.stage_attempt_id()))
    || history.stage_handoffs.iter().any(|record| {
      history
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == record.stage_attempt_id())
        .is_none_or(|stage| record.subject().exact() != stage.subject())
    })
    || history.macro_calls.iter().any(|record| {
      record.run_id() != run_id
        || !stages.contains(&record.stage_attempt_id())
        || record.parent_id().is_some_and(|id| !calls.contains(&id))
        || record.stage_dependencies().iter().any(|id| !handoffs.contains_key(id))
        || record.call_dependencies().iter().any(|id| !calls.contains(id))
        || record.depth()
          != record
            .call_dependencies()
            .iter()
            .filter_map(|id| history.macro_calls.iter().find(|call| call.id() == *id))
            .map(MacroCall::depth)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
        || manifests.get(&record.context_manifest_id()).is_none_or(|manifest| {
          manifest.subject() != record.subject() || manifest.digest().ok() != Some(record.context_digest())
        })
    })
    || history.macro_call_completions.iter().any(|completion| {
      call_records.get(&completion.call_id()).is_none_or(|call| {
        completion.subject() != call.subject()
          || !history
            .stage_attempts
            .iter()
            .find(|stage| stage.id() == call.stage_attempt_id())
            .is_some_and(|stage| completion.task_envelope_digest() == stage.input_digest())
          || !completion.usage_is_valid_for(call.budget())
      })
    })
    || history
      .signal_requests
      .iter()
      .any(|record| record.run_id() != run_id || !stages.contains(&record.stage_attempt_id()))
    || history
      .signal_receipts
      .iter()
      .any(|record| record.run_id() != run_id || !requests.contains(&record.request_id()))
    || history
      .linked_builds
      .iter()
      .any(|record| record.run_id != run_id || !stages.contains(&record.stage_attempt_id))
    || history.build_observations.iter().any(|record| {
      record.run_id != run_id
        || builds
          .get(&record.build_id)
          .is_none_or(|link| link.stage_attempt_id != record.stage_attempt_id || link.target != record.target)
    })
    || history
      .candidates
      .iter()
      .any(|record| !stages.contains(&record.stage_attempt_id()))
    || history
      .evidence
      .iter()
      .any(|record| !candidates.contains(&record.changeset_id()))
    || history
      .evaluation_plans
      .iter()
      .any(|record| !evidence.contains_key(&record.evidence_id()))
    || history.assessments.iter().any(|record| {
      plans.get(&record.plan_id()).is_none_or(|plan| {
        evidence
          .get(&plan.evidence_id())
          .is_none_or(|manifest| record.validate_bindings(plan, manifest).is_err())
      })
    })
    || history.decisions.iter().any(|record| {
      plans.get(&record.plan_id()).is_none_or(|plan| {
        record
          .validate_bindings(
            plan,
            history.stage_attempts.iter(),
            assessments.values().copied(),
            receipts.values().copied(),
          )
          .is_err()
      })
    })
    || history
      .escalations
      .iter()
      .any(|record| record.run_id() != run_id || record.decision_id().is_some_and(|id| !decisions.contains(&id)))
    || history
      .delivery_attempts
      .iter()
      .any(|record| !decisions.contains(&record.decision_id()))
    || history
      .reporting_attempts
      .iter()
      .any(|record| record.run_id() != run_id);
  if invalid {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn outbox_rows(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
) -> Result<Vec<octacity_server_store::FactoryOutboxRecord>, StoreError> {
  sqlx::query_as::<_, OutboxRow>(
    "SELECT id, operation_id, run_id, kind, input_digest, state, attempt, \
            FLOOR(EXTRACT(EPOCH FROM available_at) * 1000)::BIGINT AS available_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis, \
            owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT AS claimed_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM factory_outbox_records WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(decode_outbox)
  .collect()
}
