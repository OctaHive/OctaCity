use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use octacity_server_domain::EntityKind;
use octacity_server_factory::{
  FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryLifecycleProgress, FactoryRun, FactoryRunId,
  FactoryRunState, FactoryRunVersion, FactoryWipUsage, FlowRun, FlowRunId, FlowRuntimeHistory, NodeAttempt,
  NodeAttemptCompletion, NodeAttemptId, WorkEnvelope, WorkflowCycle, WorkflowCycleId, validate_flow_runtime_history,
};

use crate::{
  ApplyFactoryRunControl, ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns,
  ClaimedFactoryOutbox, ClaimedFactoryRun, CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome,
  FactoryAuditFact, FactoryBudgetRecord, FactoryFlowHistory, FactoryLifecycleCheckpoint, FactoryOutboxRecord,
  FactoryOutboxSettlement, FactoryRunClaimRecord, FactoryRunControlIntent, FactoryRunControlOutcome,
  FactoryRunControlStore, FactoryRunCurrentProjection, FactoryRunDiagnosticKind, FactoryRunDiagnosticPage,
  FactoryRunDiagnosticRecord, FactoryRunHistoryAppend, FactoryRunSnapshot, FactoryRunStore, ListFactoryRunDiagnostics,
  MAX_FACTORY_RUN_SNAPSHOT_RECORDS, ManagementMutation, MutationAuditContext, MutationDisposition,
  PublishedFactoryAdmission, SettleFactoryOutbox, StoreError, StoreInputError, StoreOperation,
};

use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;

#[derive(Clone)]
pub(super) struct StoredFactoryRunControl {
  request: ApplyFactoryRunControl,
  outcome: FactoryRunControlOutcome,
}

#[derive(Clone)]
pub(super) struct StoredFactoryRun {
  work: WorkEnvelope,
  run: FactoryRun,
  admitted_flow: octacity_server_factory::AdmittedFlow,
  flow_runs: BTreeMap<FlowRunId, FlowRun>,
  workflow_cycles: BTreeMap<WorkflowCycleId, WorkflowCycle>,
  node_attempts: BTreeMap<NodeAttemptId, NodeAttempt>,
  node_attempt_completions: BTreeMap<NodeAttemptId, NodeAttemptCompletion>,
  node_inputs: BTreeMap<FactoryDigest, octacity_server_factory::FlowNodeInput>,
  node_stops: BTreeMap<NodeAttemptId, octacity_server_factory::FlowNodeStop>,
  node_build_intents: BTreeMap<NodeAttemptId, octacity_server_factory::FlowBuildIntent>,
  node_build_executions: BTreeMap<FactoryDigest, octacity_server_factory::FlowBuildExecution>,
  node_records: BTreeMap<NodeAttemptId, octacity_server_factory::FlowNodeRecord>,
  incoming_data: BTreeMap<FactoryDigest, octacity_server_factory::FlowIncomingData>,
  admitted_at: octacity_server_domain::Timestamp,
  pub(super) current_claim_id: Option<FactoryDigest>,
  pub(super) claims: BTreeMap<FactoryDigest, FactoryRunClaimRecord>,
  budgets: BTreeMap<FactoryDigest, FactoryBudgetRecord>,
  lifecycle_checkpoints: BTreeMap<FactoryDigest, FactoryLifecycleCheckpoint>,
  pub(super) audit: BTreeMap<FactoryDigest, FactoryAuditFact>,
  controls: BTreeMap<FactoryDigest, crate::FactoryRunControlRecord>,
  outbox: BTreeMap<FactoryDigest, FactoryOutboxRecord>,
  current: FactoryRunCurrentProjection,
}

impl StoredFactoryRun {
  pub(super) fn source_reporting_diagnostics(
    &self,
    accepted: &BTreeSet<FactoryDigest>,
    limit: u16,
  ) -> crate::WorkSourceReportingPage {
    let mut items = latest_outbox_records(self)
      .into_iter()
      .filter(|row| row.kind.as_str() == "work.status.projection" && accepted.contains(&row.operation_id))
      .take(usize::from(limit) + 1)
      .map(|row| crate::WorkSourceReportingDiagnostic {
        operation_id: row.operation_id,
        state: row.state.into(),
        attempt: row.attempt,
        available_at: row.available_at,
        origin: self
          .node_records
          .values()
          .find(|record| record.digest().ok() == Some(row.input_digest))
          .and_then(|record| {
            Some(crate::WorkSourceReportingOrigin {
              action: crate::WorkSourceReportingRecord::from_record(record)?,
              stages: record
                .input()
                .sources()
                .iter()
                .filter_map(|source| self.node_records.get(&source.node_attempt_id()))
                .filter_map(crate::WorkSourceReportingRecord::from_record)
                .collect(),
            })
          }),
      })
      .collect::<Vec<_>>();
    let truncated = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    crate::WorkSourceReportingPage { items, truncated }
  }
  pub(super) const fn run(&self) -> &FactoryRun {
    &self.run
  }

  pub(super) const fn work(&self) -> &WorkEnvelope {
    &self.work
  }

  pub(super) const fn admitted_at(&self) -> octacity_server_domain::Timestamp {
    self.admitted_at
  }

  // Version/current pointers cover committed history, budget and cancellation;
  // claims and time-based readiness can change without a Run version change.
  pub(super) fn phase_pool_scan_digest(&self, at: octacity_server_domain::Timestamp) -> FactoryDigest {
    crate::factory_phase_pool::digest(
      "phase-pool-scan-run",
      &(
        &self.run,
        self.current.budget_id,
        self.current.lifecycle_checkpoint_id,
        self.current_claim_id,
        self
          .current_claim_id
          .and_then(|id| self.claims.get(&id))
          .map(|row| row.claim.expires_at() > at),
        self.admitted_at <= at,
      ),
    )
  }

  pub(super) fn updated_at(&self) -> Result<octacity_server_domain::Timestamp, StoreError> {
    self
      .lifecycle_checkpoints
      .get(&self.current.lifecycle_checkpoint_id)
      .map(|checkpoint| checkpoint.recorded_at)
      .ok_or(StoreError::Unavailable)
  }

  pub(super) fn admitted(
    admission: &PublishedFactoryAdmission,
    intent_digest: FactoryDigest,
    context: &MutationAuditContext,
  ) -> Result<Self, StoreError> {
    let run = &admission.run;
    if run.state() != FactoryRunState::Admitted
      || run.version() != FactoryRunVersion::INITIAL
      || run.work_id() != admission.work.id()
      || run.configuration() != admission.work.configuration()
      || run.subject() != admission.work.subject()
    {
      return Err(invalid_transition(StoreOperation::AdmitFactoryWork));
    }

    let budget = FactoryBudgetRecord::new(
      run.id(),
      FactoryRunVersion::INITIAL,
      octacity_server_factory::BudgetUsage::default(),
      admission.admitted_at,
    );
    let lifecycle_checkpoint = FactoryLifecycleCheckpoint::new(
      run.id(),
      FactoryRunVersion::INITIAL,
      FactoryLifecycleProgress::Admitted,
      false,
      admission.admitted_at,
    );
    let actor_identity_digest = context
      .actor()
      .identity
      .as_deref()
      .map(|identity| FactoryDigest::sha256("octacity.factory.audit-actor.v1", &[identity.as_bytes()]));
    let request_identity_digest = FactoryDigest::sha256(
      "octacity.factory.audit-request.v1",
      &[context.request_identity().as_bytes()],
    );
    let audit = FactoryAuditFact::new(
      run.id(),
      context.actor().kind,
      actor_identity_digest,
      key("work.admitted"),
      request_identity_digest,
      key("accepted"),
      admission.admitted_at,
    );
    let operation_id = FactoryDigest::sha256(
      "octacity.factory.admission-outbox-operation.v1",
      &[run.id().as_uuid().as_bytes(), &intent_digest.as_bytes()],
    );
    let outbox = FactoryOutboxRecord::pending(
      operation_id,
      run.id(),
      key("factory.run.admitted"),
      intent_digest,
      admission.admitted_at,
      admission.admitted_at,
    );
    let current = FactoryRunCurrentProjection {
      budget_id: budget.id,
      lifecycle_checkpoint_id: lifecycle_checkpoint.id,
    };
    Ok(Self {
      work: admission.work.clone(),
      run: admission.run.clone(),
      admitted_flow: admission.flow.clone(),
      flow_runs: BTreeMap::from([(admission.flow.root_run().id(), admission.flow.root_run().clone())]),
      workflow_cycles: BTreeMap::from([(
        admission.flow.initial_cycle().id(),
        admission.flow.initial_cycle().clone(),
      )]),
      node_attempts: BTreeMap::new(),
      node_attempt_completions: BTreeMap::new(),
      node_inputs: BTreeMap::new(),
      node_stops: BTreeMap::new(),
      node_build_intents: BTreeMap::new(),
      node_build_executions: BTreeMap::new(),
      node_records: BTreeMap::new(),
      incoming_data: BTreeMap::new(),
      admitted_at: admission.admitted_at,
      current_claim_id: None,
      claims: BTreeMap::new(),
      budgets: BTreeMap::from([(budget.id, budget)]),
      lifecycle_checkpoints: BTreeMap::from([(lifecycle_checkpoint.id, lifecycle_checkpoint)]),
      audit: BTreeMap::from([(audit.id, audit)]),
      controls: BTreeMap::new(),
      outbox: BTreeMap::from([(outbox.id, outbox)]),
      current,
    })
  }

  pub(super) fn snapshot(&self) -> Result<FactoryRunSnapshot, StoreError> {
    self.validate_integrity()?;
    require_with(
      self.record_count() <= MAX_FACTORY_RUN_SNAPSHOT_RECORDS,
      StoreOperation::ReadFactoryRunSnapshot,
      StoreInputError::InvalidFactoryRunTransition,
    )?;
    let snapshot = FactoryRunSnapshot {
      work: self.work.clone(),
      run: self.run.clone(),
      admitted_flow: self.admitted_flow.clone(),
      flow: FactoryFlowHistory {
        runs: values(&self.flow_runs),
        cycles: values(&self.workflow_cycles),
        attempts: values(&self.node_attempts),
        completions: values(&self.node_attempt_completions),
        data: self.data_history(),
      },
      current_claim: self
        .current_claim_id
        .map(|id| self.claims.get(&id).cloned().ok_or(StoreError::Unavailable))
        .transpose()?,
      budgets: values(&self.budgets),
      lifecycle_checkpoints: values(&self.lifecycle_checkpoints),
      controls: values(&self.controls),
      outbox: values(&self.outbox),
      current: self.current.clone(),
    };
    crate::validate_factory_node_control_history(&snapshot)?;
    Ok(snapshot)
  }

  fn append_transition(&mut self, request: &CommitFactoryRunTransition) -> Result<(), StoreError> {
    append_unique(&mut self.budgets, request.budget.id, request.budget.clone())?;
    append_unique(
      &mut self.lifecycle_checkpoints,
      request.lifecycle_checkpoint.id,
      request.lifecycle_checkpoint.clone(),
    )?;
    append_history(self, &request.append)?;
    append_unique(&mut self.audit, request.audit.id, request.audit.clone())?;
    for record in &request.outbox {
      append_unique(&mut self.outbox, record.id, record.clone())?;
    }
    self.run = request.next_run.clone();
    self.current = request.current.clone();
    self.validate_integrity()
  }

  fn validate_integrity(&self) -> Result<(), StoreError> {
    let run_id = self.run.id();
    let subject = self.run.subject();
    if self.run.work_id() != self.work.id()
      || self.run.configuration() != self.work.configuration()
      || subject != self.work.subject()
      || self.admitted_flow.root_run().factory_run_id() != run_id
      || self.admitted_flow.root_run().definition() != self.admitted_flow.closure().root()
      || self.admitted_flow.initial_cycle().flow_run_id() != self.admitted_flow.root_run().id()
      || self.current_claim_id.is_some_and(|id| !self.claims.contains_key(&id))
      || !self
        .budgets
        .get(&self.current.budget_id)
        .is_some_and(|budget| budget.run_version == self.run.version())
      || !self
        .lifecycle_checkpoints
        .get(&self.current.lifecycle_checkpoint_id)
        .is_some_and(|checkpoint| checkpoint.run_version == self.run.version())
    {
      return Err(invalid_transition(StoreOperation::ReadFactoryRunSnapshot));
    }
    if self
      .current_claim_id
      .and_then(|id| self.claims.get(&id))
      .is_some_and(|record| record != &FactoryRunClaimRecord::new(run_id, record.owner.clone(), record.claim))
      || self.budgets.values().any(|record| {
        record.run_version > self.run.version()
          || record != &FactoryBudgetRecord::new(run_id, record.run_version, record.usage, record.recorded_at)
      })
      || self
        .budgets
        .values()
        .map(|record| record.run_version)
        .collect::<BTreeSet<_>>()
        .len()
        != self.budgets.len()
      || self.lifecycle_checkpoints.values().any(|checkpoint| {
        checkpoint.run_id != run_id
          || checkpoint.run_version > self.run.version()
          || checkpoint
            != &FactoryLifecycleCheckpoint::new(
              checkpoint.run_id,
              checkpoint.run_version,
              checkpoint.progress.clone(),
              checkpoint.cancellation_requested,
              checkpoint.recorded_at,
            )
          || (checkpoint.run_version == self.run.version()
            && octacity_server_factory::validate_lifecycle_progress(self.run.state(), &checkpoint.progress).is_err())
      })
      || self
        .lifecycle_checkpoints
        .values()
        .map(|checkpoint| checkpoint.run_version)
        .collect::<BTreeSet<_>>()
        .len()
        != self.lifecycle_checkpoints.len()
    {
      return Err(invalid_transition(StoreOperation::ReadFactoryRunSnapshot));
    }
    let flow_runs = values(&self.flow_runs);
    let cycles = values(&self.workflow_cycles);
    let attempts = values(&self.node_attempts);
    let completions = values(&self.node_attempt_completions);
    let stops = values(&self.node_stops);
    self
      .data_history()
      .validate(
        &self.work,
        &self.admitted_flow,
        FlowRuntimeHistory {
          stops: &stops,
          flow_runs: &flow_runs,
          cycles: &cycles,
          attempts: &attempts,
          completions: &completions,
        },
      )
      .map_err(|_| invalid_transition(StoreOperation::ReadFactoryRunSnapshot))?;
    validate_flow_runtime_history(
      &self.admitted_flow,
      FlowRuntimeHistory {
        stops: &stops,
        flow_runs: &flow_runs,
        cycles: &cycles,
        attempts: &attempts,
        completions: &completions,
      },
    )
    .map_err(|_| invalid_transition(StoreOperation::ReadFactoryRunSnapshot))?;
    require(self.controls.values().all(|record| {
      record.run_id == run_id
        && record.run_version <= self.run.version()
        && record
          == &crate::FactoryRunControlRecord::new(
            record.run_id,
            record.run_version,
            record.intent.clone(),
            record.recorded_at,
          )
    }))?;
    require(
      self
        .controls
        .values()
        .map(|record| record.run_version)
        .collect::<BTreeSet<_>>()
        .len()
        == self.controls.len(),
    )?;
    require(
      self
        .outbox
        .values()
        .all(|record| record.subject == crate::FactoryOutboxSubject::Run(run_id) && record.is_canonical()),
    )?;
    validate_outbox(self)?;
    Ok(())
  }

  fn record_count(&self) -> usize {
    usize::from(self.current_claim_id.is_some())
      + self.budgets.len()
      + self.lifecycle_checkpoints.len()
      + self.flow_runs.len()
      + self.workflow_cycles.len()
      + self.node_attempts.len()
      + self.node_attempt_completions.len()
      + self.node_inputs.len()
      + self.node_stops.len()
      + self.node_build_intents.len()
      + self.node_build_executions.len()
      + self.node_records.len()
      + self.incoming_data.len()
      + self.controls.len()
      + self.outbox.len()
  }
  fn data_history(&self) -> octacity_server_factory::FlowDataHistory {
    octacity_server_factory::FlowDataHistory {
      incoming: values(&self.incoming_data),
      inputs: values(&self.node_inputs),
      stops: values(&self.node_stops),
      build_intents: values(&self.node_build_intents),
      build_executions: values(&self.node_build_executions),
      records: values(&self.node_records),
    }
  }
}

// The memory backend indexes completions by producing node; the public cursor
// belongs to the immutable observation itself, independently of producer order.
fn ordered_diagnostics(
  rows: impl Iterator<Item = FactoryRunDiagnosticRecord>,
) -> Box<dyn Iterator<Item = FactoryRunDiagnosticRecord>> {
  let mut rows = rows.collect::<Vec<_>>();
  rows.sort_by_key(FactoryRunDiagnosticRecord::cursor);
  Box::new(rows.into_iter())
}

fn diagnostic_records(
  stored: &StoredFactoryRun,
  kind: FactoryRunDiagnosticKind,
) -> Box<dyn Iterator<Item = FactoryRunDiagnosticRecord> + '_> {
  match kind {
    FactoryRunDiagnosticKind::NodeStop => ordered_diagnostics(
      stored
        .node_stops
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::NodeStop(Box::new(row))),
    ),

    FactoryRunDiagnosticKind::Work => Box::new(std::iter::once(FactoryRunDiagnosticRecord::Work(Box::new(
      stored.work.clone(),
    )))),
    FactoryRunDiagnosticKind::Call => Box::new(
      stored
        .node_attempts
        .values()
        .filter(|node| {
          matches!(
            node.execution(),
            octacity_server_factory::NodeExecutionIdentity::External(_)
          )
        })
        .map(|node| FactoryRunDiagnosticRecord::Call {
          node: Box::new(node.clone()),
          profile: stored.node_build_intents.get(&node.id()).map(|intent| {
            Box::new(
              crate::FactoryCallProfileDiagnostic::try_from(intent)
                .expect("stored frozen intents are verified before commit"),
            )
          }),
        }),
    ),
    FactoryRunDiagnosticKind::DomainFact(kind) => {
      let mut records = stored
        .node_records
        .values()
        .filter(|row| row.discovery().is_some_and(|facts| facts.kind() == kind))
        .map(|row| {
          FactoryRunDiagnosticRecord::DomainFact(Box::new(
            crate::FactoryDomainDiagnostic::try_from(row).expect("stored node records are verified before commit"),
          ))
        })
        .collect::<Vec<_>>();
      records.sort_by_key(FactoryRunDiagnosticRecord::cursor);
      Box::new(records.into_iter())
    }
    FactoryRunDiagnosticKind::BuildExecution => Box::new(
      stored
        .node_build_executions
        .values()
        .map(|row| FactoryRunDiagnosticRecord::BuildExecution(Box::new(row.into()))),
    ),
    FactoryRunDiagnosticKind::Budget => Box::new(
      stored
        .budgets
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::Budget(Box::new(row))),
    ),
    FactoryRunDiagnosticKind::WorkflowCycle => Box::new(
      stored
        .workflow_cycles
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::WorkflowCycle(Box::new(row))),
    ),
    FactoryRunDiagnosticKind::FlowRun => Box::new(
      stored
        .flow_runs
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::FlowRun(Box::new(row))),
    ),
    FactoryRunDiagnosticKind::Claim => Box::new(
      stored
        .claims
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::Claim(Box::new(row))),
    ),
    FactoryRunDiagnosticKind::Audit => Box::new(
      stored
        .audit
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::Audit(Box::new(row))),
    ),
    FactoryRunDiagnosticKind::NodeAttempt => Box::new(
      stored
        .node_attempts
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::NodeAttempt(Box::new(row))),
    ),
    FactoryRunDiagnosticKind::NodeAttemptCompletion => ordered_diagnostics(
      stored
        .node_attempt_completions
        .values()
        .cloned()
        .map(|row| FactoryRunDiagnosticRecord::NodeAttemptCompletion(Box::new(row))),
    ),
  }
}

#[async_trait]
impl FactoryRunStore for InMemoryFactoryConfigurationStore {
  async fn factory_run_snapshot(&self, run_id: FactoryRunId) -> Result<FactoryRunSnapshot, StoreError> {
    self
      .lock()?
      .factory_runs
      .get(&run_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?
      .snapshot()
  }

  async fn list_factory_run_diagnostics(
    &self,
    request: ListFactoryRunDiagnostics,
  ) -> Result<FactoryRunDiagnosticPage, StoreError> {
    let state = self.lock()?;
    let stored = state.factory_runs.get(&request.run_id).ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?;
    request.validate()?;
    let limit = usize::from(request.limit.get());
    let mut records: Vec<_> = diagnostic_records(stored, request.kind)
      .filter(|record| request.after.as_ref().is_none_or(|after| record.cursor() > *after))
      .take(limit + 1)
      .collect();
    let has_next = records.len() > limit;
    records.truncate(limit);
    let next = has_next.then(|| records.last().expect("non-zero page limit").cursor());
    Ok(FactoryRunDiagnosticPage { items: records, next })
  }

  async fn claim_factory_run(&self, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
    let mut state = self.lock()?;
    let stored = state
      .factory_runs
      .get_mut(&request.run_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?;
    if let Some(existing) = stored.claims.get(&request.record.id) {
      if existing != &request.record || stored.audit.get(&request.audit.id) != Some(&request.audit) {
        return Err(conflict());
      }
      return Ok(ClaimFactoryRunOutcome {
        disposition: MutationDisposition::Replayed,
        record: existing.clone(),
      });
    }
    validate_claim(stored, &request)?;
    let mut next = stored.clone();
    append_unique(&mut next.claims, request.record.id, request.record.clone())?;
    append_unique(&mut next.audit, request.audit.id, request.audit)?;
    next.current_claim_id = Some(request.record.id);
    next.validate_integrity()?;
    *stored = next;
    Ok(ClaimFactoryRunOutcome {
      disposition: MutationDisposition::Applied,
      record: request.record,
    })
  }

  async fn claim_factory_runs(&self, request: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
    let mut state = self.lock()?;
    let mut staged_runs = state.factory_runs.clone();
    let run_ids = staged_runs
      .iter()
      .filter(|(_, stored)| stored.run.state().is_active())
      .filter(|(_, stored)| claim_is_selectable(stored, &request))
      .map(|(run_id, _)| *run_id)
      .take(usize::from(request.limit.get()))
      .collect::<Vec<_>>();
    let mut claims = Vec::with_capacity(run_ids.len());
    for run_id in run_ids {
      let stored = staged_runs.get(&run_id).ok_or(StoreError::Unavailable)?;
      let expected_version = stored.run.version();
      let wip_usage = configuration_wip_usage(&staged_runs, stored.run.configuration());
      if let Some(existing) = exact_claim_replay(stored, &request) {
        claims.push(ClaimedFactoryRun {
          run_id,
          disposition: MutationDisposition::Replayed,
          expected_version,
          record: existing,
          wip_usage,
        });
        continue;
      }

      let claim = batch_claim(run_id, expected_version, &request)?;
      let audit = batch_claim_audit(run_id, &request, claim.id);
      let claim_request = ClaimFactoryRun {
        run_id,
        expected_version,
        record: claim.clone(),
        audit: audit.clone(),
      };
      let stored = staged_runs.get_mut(&run_id).ok_or(StoreError::Unavailable)?;
      validate_claim(stored, &claim_request)?;
      let mut next = stored.clone();
      append_unique(&mut next.claims, claim.id, claim.clone())?;
      append_unique(&mut next.audit, audit.id, audit)?;
      next.current_claim_id = Some(claim.id);
      next.validate_integrity()?;
      *stored = next;
      claims.push(ClaimedFactoryRun {
        run_id,
        disposition: MutationDisposition::Applied,
        expected_version,
        record: claim,
        wip_usage,
      });
    }
    state.factory_runs = staged_runs;
    Ok(claims)
  }

  async fn commit_factory_run_transition(
    &self,
    request: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
    let mut state = self.lock()?;
    let selection =
      crate::factory_phase_pool_testing::retained_selection_for_claim(&state, request.run_id, request.claim_id)?;
    let configuration = state
      .factory_runs
      .get(&request.run_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?
      .run
      .configuration();
    let configuration_wip_usage = configuration_wip_usage(&state.factory_runs, configuration);
    let stored = state.factory_runs.get(&request.run_id).ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?;
    validate_transition(stored, &request, selection.as_ref(), configuration_wip_usage)?;
    let mut next = stored.clone();
    next.append_transition(&request)?;
    let outcome = CommitFactoryRunTransitionOutcome {
      version: next.run.version(),
      current: next.current.clone(),
    };
    crate::factory_source_testing::record_projection_intents(&mut state, &next, &request)?;
    state.factory_runs.insert(request.run_id, next);
    Ok(outcome)
  }

  async fn claim_factory_outbox(&self, request: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
    let mut state = self.lock()?;
    let mut due = state
      .factory_runs
      .iter()
      .flat_map(|(run_id, stored)| {
        latest_outbox_records(stored)
          .into_iter()
          .filter(|record| request.kinds.is_empty() || request.kinds.contains(&record.kind))
          .filter(|record| match record.state {
            crate::FactoryOutboxState::Pending => record.available_at <= request.observed_at,
            crate::FactoryOutboxState::Claimed => record
              .claim
              .is_some_and(|claim| claim.expires_at() <= request.observed_at),
            crate::FactoryOutboxState::Delivered | crate::FactoryOutboxState::Failed => false,
          })
          .map(|record| (*run_id, record.clone()))
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>();
    due.sort_by_key(|(_, record)| (record.available_at, record.operation_id));
    due.truncate(usize::from(request.limit.get()));

    let mut staged_runs = state.factory_runs.clone();
    let mut claimed = Vec::with_capacity(due.len());
    for (run_id, current) in due {
      let stored = staged_runs.get_mut(&run_id).ok_or(StoreError::Unavailable)?;
      let pending = if current.state == crate::FactoryOutboxState::Claimed {
        let retry = FactoryOutboxRecord::retry(&current, request.observed_at, request.observed_at)
          .ok_or_else(|| invalid_transition(StoreOperation::ClaimFactoryOutbox))?;
        append_unique(&mut stored.outbox, retry.id, retry.clone())?;
        retry
      } else {
        current
      };
      let fence = FactoryClaimFence::new(FactoryDigest::sha256(
        "octacity.factory.outbox-fence.v1",
        &[
          &pending.operation_id.as_bytes(),
          request.owner.as_str().as_bytes(),
          &pending.attempt.to_be_bytes(),
          &request.observed_at.unix_millis().to_be_bytes(),
          &request.claim_expires_at.unix_millis().to_be_bytes(),
        ],
      ));
      let claim = FactoryClaim::new(fence, request.observed_at, request.claim_expires_at)
        .map_err(|_| invalid_claim(StoreOperation::ClaimFactoryOutbox))?;
      let record = FactoryOutboxRecord::claimed(
        &pending,
        request.owner.clone(),
        claim,
        pending.attempt,
        request.observed_at,
      );
      append_unique(&mut stored.outbox, record.id, record.clone())?;
      let audit = FactoryAuditFact::new(
        run_id,
        crate::AuditActorKind::Worker,
        Some(FactoryDigest::sha256(
          "octacity.factory.audit-worker.v1",
          &[request.owner.as_str().as_bytes()],
        )),
        key("factory.outbox.claimed"),
        record.operation_id,
        key("accepted"),
        request.observed_at,
      );
      append_unique(&mut stored.audit, audit.id, audit)?;
      stored.validate_integrity()?;
      claimed.push(ClaimedFactoryOutbox { record });
    }
    state.factory_runs = staged_runs;
    Ok(claimed)
  }

  async fn settle_factory_outbox(&self, request: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError> {
    let mut state = self.lock()?;
    let run_id = state
      .factory_runs
      .iter()
      .find(|(_, stored)| {
        stored
          .outbox
          .values()
          .any(|record| record.operation_id == request.operation_id)
      })
      .map(|(run_id, _)| *run_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?;
    let stored = state.factory_runs.get(&run_id).ok_or(StoreError::Unavailable)?;
    let current = latest_outbox_record(stored, request.operation_id)
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    if matches!(
      (current.state, request.settlement),
      (crate::FactoryOutboxState::Delivered, FactoryOutboxSettlement::Delivered)
        | (crate::FactoryOutboxState::Failed, FactoryOutboxSettlement::Failed)
    ) && current.owner.as_ref() == Some(&request.owner)
      && current.claim.is_some_and(|claim| claim.fence() == request.fence)
    {
      return Ok(current);
    }
    if current.state != crate::FactoryOutboxState::Claimed || current.owner.as_ref() != Some(&request.owner) {
      return Err(conflict());
    }
    current
      .claim
      .ok_or_else(conflict)?
      .verify_fence(request.fence, request.observed_at)
      .map_err(|_| conflict())?;
    let next = match request.settlement {
      FactoryOutboxSettlement::Delivered => {
        FactoryOutboxRecord::terminal(&current, crate::FactoryOutboxState::Delivered, request.observed_at)
      }
      FactoryOutboxSettlement::Failed => {
        FactoryOutboxRecord::terminal(&current, crate::FactoryOutboxState::Failed, request.observed_at)
      }
      FactoryOutboxSettlement::RetryAt(available_at) if available_at >= request.observed_at => {
        FactoryOutboxRecord::retry(&current, available_at, request.observed_at)
          .ok_or_else(|| invalid_transition(StoreOperation::SettleFactoryOutbox))?
      }
      FactoryOutboxSettlement::RetryAt(_) => {
        return Err(invalid_transition(StoreOperation::SettleFactoryOutbox));
      }
    };
    let outcome = if next.state == crate::FactoryOutboxState::Delivered {
      "delivered"
    } else if next.state == crate::FactoryOutboxState::Failed {
      "failed"
    } else {
      "retry_scheduled"
    };
    let mut staged = stored.clone();
    append_unique(&mut staged.outbox, next.id, next.clone())?;
    let audit = FactoryAuditFact::new(
      run_id,
      crate::AuditActorKind::Worker,
      Some(FactoryDigest::sha256(
        "octacity.factory.audit-worker.v1",
        &[request.owner.as_str().as_bytes()],
      )),
      key("factory.outbox.settled"),
      request.operation_id,
      key(outcome),
      request.observed_at,
    );
    append_unique(&mut staged.audit, audit.id, audit)?;
    staged.validate_integrity()?;
    state.factory_runs.insert(run_id, staged);
    Ok(next)
  }
}

#[async_trait]
impl FactoryRunControlStore for InMemoryFactoryConfigurationStore {
  async fn apply_factory_run_control(
    &self,
    request: ManagementMutation<ApplyFactoryRunControl>,
  ) -> Result<FactoryRunControlOutcome, StoreError> {
    let (request, audit_context) = request.into_parts();
    let operation = request.intent.operation();
    let idempotency = audit_context.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(stored) = state.factory_run_controls.get(&(operation, idempotency.clone())) {
      if stored.request.replay_intent() != request.replay_intent() {
        return Err(conflict());
      }
      let mut outcome = stored.outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }

    let stored = state.factory_runs.get(&request.run_id).ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?;
    if stored.run.version() != request.expected_version {
      return Err(conflict());
    }
    let mut next = stored.clone();
    let outcome = apply_control(&mut next, &request, &audit_context)?;
    state.factory_run_controls.insert(
      (operation, idempotency),
      StoredFactoryRunControl {
        request: request.clone(),
        outcome: outcome.clone(),
      },
    );
    state.factory_runs.insert(request.run_id, next);
    Ok(outcome)
  }
}

fn apply_control(
  stored: &mut StoredFactoryRun,
  request: &ApplyFactoryRunControl,
  context: &MutationAuditContext,
) -> Result<FactoryRunControlOutcome, StoreError> {
  let change = crate::prepare_factory_run_control(&stored.snapshot()?, request, context, stored.record_count())?;
  append_unique(&mut stored.budgets, change.budget.id, change.budget.clone())?;
  append_unique(
    &mut stored.lifecycle_checkpoints,
    change.lifecycle.id,
    change.lifecycle.clone(),
  )?;
  append_unique(&mut stored.controls, change.control.id, change.control)?;
  append_unique(&mut stored.audit, change.audit.id, change.audit)?;
  stored.run = change.run;
  stored.current_claim_id = None;
  stored.current.budget_id = change.budget.id;
  stored.current.lifecycle_checkpoint_id = change.lifecycle.id;
  stored.validate_integrity()?;
  Ok(change.outcome)
}

fn claim_is_selectable(stored: &StoredFactoryRun, request: &ClaimFactoryRuns) -> bool {
  stored.current_claim_id.is_none_or(|id| {
    stored.claims.get(&id).is_none_or(|current| {
      current.claim.expires_at() <= request.observed_at
        || (current.owner == request.owner
          && current.claim.claimed_at() == request.observed_at
          && current.claim.expires_at() == request.claim_expires_at)
        || (current.owner == request.owner
          && current.claim.claimed_at() <= request.observed_at
          && stored
            .updated_at()
            .is_ok_and(|at| at >= current.claim.claimed_at() && at <= request.observed_at))
    })
  })
}

fn exact_claim_replay(stored: &StoredFactoryRun, request: &ClaimFactoryRuns) -> Option<FactoryRunClaimRecord> {
  stored
    .current_claim_id
    .and_then(|id| stored.claims.get(&id))
    .filter(|current| {
      current.owner == request.owner
        && current.claim.claimed_at() == request.observed_at
        && current.claim.expires_at() == request.claim_expires_at
    })
    .cloned()
}

fn batch_claim(
  run_id: FactoryRunId,
  version: FactoryRunVersion,
  request: &ClaimFactoryRuns,
) -> Result<FactoryRunClaimRecord, StoreError> {
  let version = version.get().to_be_bytes();
  let observed = request.observed_at.unix_millis().to_be_bytes();
  let expires = request.claim_expires_at.unix_millis().to_be_bytes();
  let fence = FactoryClaimFence::new(FactoryDigest::sha256(
    "octacity.factory.reconciliation-fence.v1",
    &[
      run_id.as_uuid().as_bytes(),
      request.owner.as_str().as_bytes(),
      &version,
      &observed,
      &expires,
    ],
  ));
  let claim = FactoryClaim::new(fence, request.observed_at, request.claim_expires_at)
    .map_err(|_| invalid_claim(StoreOperation::ClaimFactoryRuns))?;
  Ok(FactoryRunClaimRecord::new(run_id, request.owner.clone(), claim))
}

fn batch_claim_audit(run_id: FactoryRunId, request: &ClaimFactoryRuns, claim_id: FactoryDigest) -> FactoryAuditFact {
  FactoryAuditFact::new(
    run_id,
    crate::AuditActorKind::Worker,
    Some(FactoryDigest::sha256(
      "octacity.factory.audit-worker.v1",
      &[request.owner.as_str().as_bytes()],
    )),
    key("factory.claimed"),
    claim_id,
    key("accepted"),
    request.observed_at,
  )
}

fn configuration_wip_usage(
  runs: &BTreeMap<FactoryRunId, StoredFactoryRun>,
  configuration: &octacity_server_factory::FactoryConfigurationRef,
) -> FactoryWipUsage {
  let mut active_runs = 0_u32;
  let mut active_nodes = 0_u32;
  for stored in runs
    .values()
    .filter(|stored| stored.run.configuration() == configuration && stored.run.state().is_active())
  {
    active_runs = active_runs.saturating_add(1);
    let attempts = stored.node_attempts.values().cloned().collect::<Vec<_>>();
    let completions = stored.node_attempt_completions.values().cloned().collect::<Vec<_>>();
    let stops = values(&stored.node_stops);
    let count = FlowRuntimeHistory {
      stops: &stops,
      flow_runs: &[],
      cycles: &[],
      attempts: &attempts,
      completions: &completions,
    }
    .active_node_count();
    active_nodes = active_nodes.saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
  }
  FactoryWipUsage::new(active_runs, active_nodes)
}

fn latest_outbox_records(stored: &StoredFactoryRun) -> Vec<&FactoryOutboxRecord> {
  let mut latest = BTreeMap::new();
  for record in stored.outbox.values() {
    let replace = latest
      .get(&record.operation_id)
      .is_none_or(|current: &&FactoryOutboxRecord| outbox_order(record) > outbox_order(current));
    if replace {
      latest.insert(record.operation_id, record);
    }
  }
  latest.into_values().collect()
}

fn latest_outbox_record(stored: &StoredFactoryRun, operation_id: FactoryDigest) -> Option<&FactoryOutboxRecord> {
  stored
    .outbox
    .values()
    .filter(|record| record.operation_id == operation_id)
    .max_by_key(|record| outbox_order(record))
}

fn outbox_order(record: &FactoryOutboxRecord) -> (u16, u8, octacity_server_domain::Timestamp, FactoryDigest) {
  record.order_key()
}

fn validate_claim(stored: &StoredFactoryRun, request: &ClaimFactoryRun) -> Result<(), StoreError> {
  if request.expected_version != stored.run.version() {
    return Err(conflict());
  }
  if stored.current_claim_id.is_some_and(|id| {
    stored.claims.get(&id).is_none_or(|current| {
      current.claim.expires_at() > request.record.claim.claimed_at()
        && !(current.owner == request.record.owner
          && current.claim.claimed_at() <= request.record.claim.claimed_at()
          && stored
            .updated_at()
            .is_ok_and(|at| at >= current.claim.claimed_at() && at <= request.record.claim.claimed_at()))
    })
  }) {
    return Err(conflict());
  }
  if stored
    .record_count()
    .checked_add(2)
    .is_none_or(|count| count > MAX_FACTORY_RUN_SNAPSHOT_RECORDS)
  {
    return Err(invalid_claim(StoreOperation::ClaimFactoryRun));
  }
  require_with(
    request.record == FactoryRunClaimRecord::new(request.run_id, request.record.owner.clone(), request.record.claim)
      && request.record.claim.claimed_at() >= stored.admitted_at
      && request.audit.run_id == request.run_id
      && request.audit.recorded_at == request.record.claim.claimed_at()
      && request.audit
        == FactoryAuditFact::new(
          request.audit.run_id,
          request.audit.actor_kind,
          request.audit.actor_identity_digest,
          request.audit.operation.clone(),
          request.audit.request_identity_digest,
          request.audit.outcome.clone(),
          request.audit.recorded_at,
        ),
    StoreOperation::ClaimFactoryRun,
    StoreInputError::InvalidFactoryRunClaim,
  )
}

fn validate_transition(
  stored: &StoredFactoryRun,
  request: &CommitFactoryRunTransition,
  selection: Option<&crate::PhasePoolSelection>,
  configuration_wip_usage: FactoryWipUsage,
) -> Result<(), StoreError> {
  let claim = stored
    .current_claim_id
    .filter(|id| *id == request.claim_id)
    .and_then(|id| stored.claims.get(&id))
    .ok_or_else(conflict)?;
  let current_budget = stored
    .budgets
    .get(&stored.current.budget_id)
    .ok_or(StoreError::Unavailable)?;
  let current_checkpoint = stored
    .lifecycle_checkpoints
    .get(&stored.current.lifecycle_checkpoint_id)
    .ok_or(StoreError::Unavailable)?;
  crate::validate_factory_transition(
    &crate::FactoryTransitionBaseline {
      configuration_wip_usage,
      run: &stored.run,
      pool_selection: selection,
      budget: current_budget,
      cancellation_requested_at: stored
        .controls
        .values()
        .filter(|row| row.intent == FactoryRunControlIntent::Cancel)
        .map(|row| row.recorded_at)
        .min(),
      lifecycle: current_checkpoint,
      retry_requests: &stored
        .controls
        .values()
        .filter_map(|row| match row.intent {
          FactoryRunControlIntent::RetryNode { node_attempt_id } => Some((node_attempt_id, row.recorded_at)),
          _ => None,
        })
        .collect(),
      current: &stored.current,
      claim,
      admitted_flow: &stored.admitted_flow,
      work: &stored.work,
      data: &stored.data_history(),
      node_attempts: &stored.node_attempts,
      node_completions: &stored.node_attempt_completions,
      flow_runs: &stored.flow_runs,
      workflow_cycles: &stored.workflow_cycles,
      record_count: stored.record_count(),
    },
    request,
  )
}

fn append_history(stored: &mut StoredFactoryRun, append: &FactoryRunHistoryAppend) -> Result<(), StoreError> {
  for receipt in &append.flow.data.stops {
    append_unique(&mut stored.node_stops, receipt.node_attempt_id(), receipt.clone())?;
  }
  for execution in &append.flow.data.build_executions {
    append_unique(
      &mut stored.node_build_executions,
      execution.digest().map_err(|_| StoreError::Unavailable)?,
      execution.clone(),
    )?;
  }
  for intent in &append.flow.data.build_intents {
    append_unique(&mut stored.node_build_intents, intent.node().id(), intent.clone())?;
  }
  for input in &append.flow.data.inputs {
    append_unique(
      &mut stored.node_inputs,
      input.digest().map_err(|_| StoreError::Unavailable)?,
      input.clone(),
    )?;
  }
  for record in &append.flow.data.records {
    append_unique(&mut stored.node_records, record.node_attempt_id(), record.clone())?;
  }
  for incoming in &append.flow.data.incoming {
    append_unique(
      &mut stored.incoming_data,
      incoming.digest().map_err(|_| StoreError::Unavailable)?,
      incoming.clone(),
    )?;
  }
  for record in &append.flow.runs {
    append_unique(&mut stored.flow_runs, record.id(), record.clone())?;
  }
  for record in &append.flow.cycles {
    append_unique(&mut stored.workflow_cycles, record.id(), record.clone())?;
  }
  for record in &append.flow.attempts {
    append_unique(&mut stored.node_attempts, record.id(), record.clone())?;
  }
  for record in &append.flow.completions {
    append_unique(
      &mut stored.node_attempt_completions,
      record.node_attempt_id(),
      record.clone(),
    )?;
  }
  Ok(())
}

fn validate_outbox(stored: &StoredFactoryRun) -> Result<(), StoreError> {
  let mut operations: BTreeMap<FactoryDigest, Vec<&FactoryOutboxRecord>> = BTreeMap::new();
  for record in stored.outbox.values() {
    operations.entry(record.operation_id).or_default().push(record);
  }
  for records in operations.values_mut() {
    records.sort_by_key(|record| outbox_order(record));
    let Some(first) = records.first() else {
      continue;
    };
    if first.state != crate::FactoryOutboxState::Pending || first.attempt != 0 {
      return Err(invalid_transition(StoreOperation::ReadFactoryRunSnapshot));
    }
    for pair in records.windows(2) {
      let [previous, next] = pair else {
        unreachable!("outbox windows have two rows")
      };
      let same_input =
        previous.subject == next.subject && previous.kind == next.kind && previous.input_digest == next.input_digest;
      let valid_state = match (previous.state, next.state) {
        (crate::FactoryOutboxState::Pending, crate::FactoryOutboxState::Claimed) => {
          previous.attempt == next.attempt && next.owner.is_some() && next.claim.is_some()
        }
        (
          crate::FactoryOutboxState::Claimed,
          crate::FactoryOutboxState::Delivered | crate::FactoryOutboxState::Failed,
        ) => previous.attempt == next.attempt && previous.owner == next.owner && previous.claim == next.claim,
        (crate::FactoryOutboxState::Claimed, crate::FactoryOutboxState::Pending) => {
          previous.attempt.checked_add(1) == Some(next.attempt) && next.owner.is_none() && next.claim.is_none()
        }
        _ => false,
      };
      if !same_input || !valid_state || next.recorded_at < previous.recorded_at {
        return Err(invalid_transition(StoreOperation::ReadFactoryRunSnapshot));
      }
    }
  }
  Ok(())
}

fn append_unique<K, V>(map: &mut BTreeMap<K, V>, key: K, value: V) -> Result<(), StoreError>
where
  K: Ord,
{
  if map.insert(key, value).is_some() {
    return Err(conflict());
  }
  Ok(())
}

fn values<K, V: Clone>(map: &BTreeMap<K, V>) -> Vec<V> {
  map.values().cloned().collect()
}

fn require(valid: bool) -> Result<(), StoreError> {
  require_with(
    valid,
    StoreOperation::CommitFactoryRunTransition,
    StoreInputError::InvalidFactoryRunTransition,
  )
}

fn require_with(valid: bool, operation: StoreOperation, source: StoreInputError) -> Result<(), StoreError> {
  if valid {
    Ok(())
  } else {
    Err(StoreError::invalid(operation, source))
  }
}

fn invalid_transition(operation: StoreOperation) -> StoreError {
  StoreError::invalid(operation, StoreInputError::InvalidFactoryRunTransition)
}

fn invalid_claim(operation: StoreOperation) -> StoreError {
  StoreError::invalid(operation, StoreInputError::InvalidFactoryRunClaim)
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryRun,
  }
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("static Factory key is valid")
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use octacity_server_domain::{
    ArtifactId, BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, ProjectId, RepositoryId,
  };
  use octacity_server_factory::{
    AdmittedFlow, BudgetLimit, BudgetUsage, BuildConfigurationRef, ExternalWorkIdentity, FactoryArtifactReference,
    FactoryChoiceKind, FactoryConfiguration, FactoryConfigurationChoiceEntries, FactoryConfigurationChoices,
    FactoryConfigurationDraft, FactoryConfigurationId, FactoryConfigurationVersion, FactoryMetadata,
    FactoryReferenceChoice, FactoryStageKind, FactoryWipLimits, FindingSeverity, ImmutableReference,
    NodeAttemptCompletionInput, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelopeId, WorkPriority,
  };

  use crate::test_support::{id, run_ready, time};
  use crate::{AuditActor, AuditActorKind, FactoryOutboxState, IdempotencyKey, ManagementSecurityScope, StoreError};

  use super::*;

  struct Fixture {
    store: Arc<InMemoryFactoryConfigurationStore>,
    work: WorkEnvelope,
    run: FactoryRun,
  }
  struct FullHistory {
    append: FactoryRunHistoryAppend,
    current: FactoryRunCurrentProjection,
  }

  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn artifact(identity: u64, digest_byte: u8) -> FactoryArtifactReference {
    FactoryArtifactReference::new(id::<ArtifactId>(identity), digest(digest_byte), 1).unwrap()
  }

  fn reference(identity: &str, byte: u8) -> ImmutableReference {
    ImmutableReference::new(key(identity), key("v1"), digest(byte))
  }

  fn projected_configuration(project_id: ProjectId, configuration_id: FactoryConfigurationId) -> FactoryConfiguration {
    projected_configuration_for_pool(project_id, configuration_id, None)
  }

  fn projected_configuration_for_pool(
    project_id: ProjectId,
    configuration_id: FactoryConfigurationId,
    pool: Option<(octacity_server_factory::FlowPoolSettings, BudgetLimit)>,
  ) -> FactoryConfiguration {
    let build = BuildConfigurationRef::new(
      id::<BuildConfigurationId>(90),
      BuildConfigurationVersion::INITIAL,
      project_id,
      digest(90),
    );
    let exact = |identity, byte| ImmutableReference::new(key(identity), key("v1"), digest(byte));
    let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references: vec![
        FactoryReferenceChoice {
          kind: FactoryChoiceKind::AdmissionPolicy,
          alias: key("admission"),
          reference: exact("manual.medium", 91),
        },
        FactoryReferenceChoice {
          kind: FactoryChoiceKind::PermissionCeiling,
          alias: key("permissions"),
          reference: exact("restricted", 92),
        },
        FactoryReferenceChoice {
          kind: FactoryChoiceKind::CriterionPack,
          alias: key("criteria"),
          reference: exact("quality", 93),
        },
        FactoryReferenceChoice {
          kind: FactoryChoiceKind::Evaluator,
          alias: key("evaluator"),
          reference: exact("review", 94),
        },
        FactoryReferenceChoice {
          kind: FactoryChoiceKind::DeliveryAdapter,
          alias: key("delivery-adapter"),
          reference: exact("github", 95),
        },
        FactoryReferenceChoice {
          kind: FactoryChoiceKind::DeliveryPolicy,
          alias: key("delivery-policy"),
          reference: exact("human-review", 96),
        },
      ],
      build_configurations: vec![(key("build"), build)],
    })
    .unwrap();
    let stage = |name, kind| octacity_server_factory::testing::FactoryStageDraft {
      key: key(name),
      kind,
      build_configuration: key("build"),
      budget: budget(),
    };
    FactoryConfiguration::publish(
      configuration_id,
      FactoryConfigurationVersion::INITIAL,
      project_id,
      digest(97),
      {
        let draft = octacity_server_factory::testing::with_contract_flow(
          FactoryConfigurationDraft {
            flow: None,
            admission_policy: key("admission"),
            wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
            hard_budget: budget(),
            permission_ceiling: key("permissions"),
            enabled: true,
          },
          vec![
            stage("implement", FactoryStageKind::Implementation),
            stage("validate", FactoryStageKind::Validation),
            stage("evaluate", FactoryStageKind::Evaluation),
          ],
          &choices,
        );
        if let Some((settings, reservation)) = pool {
          octacity_server_factory::testing::with_contract_pool(draft, settings, reservation)
        } else {
          draft
        }
      },
      &choices,
    )
    .unwrap()
  }

  fn admitted_flow(run: &FactoryRun) -> AdmittedFlow {
    let configuration = projected_configuration(run.subject().project_id(), run.configuration().id());
    assert_eq!(configuration.reference(), run.configuration());
    AdmittedFlow::from_configuration(&configuration, run).unwrap()
  }

  fn fixture() -> Fixture {
    fixture_with_pool(None)
  }

  fn fixture_with_pool(pool: Option<octacity_server_factory::FlowPoolSettings>) -> Fixture {
    let project_id = id::<ProjectId>(1);
    let subject = octacity_server_factory::ExactSubject::new(
      project_id,
      id::<RepositoryId>(2),
      ImmutableRevision::new("base-revision").unwrap(),
    );
    let configuration = projected_configuration_for_pool(
      project_id,
      id::<FactoryConfigurationId>(3),
      pool.map(|settings| (settings, BudgetLimit::new(1, 1, 1, 1, 1).unwrap())),
    );
    let work = WorkEnvelope::new(
      id::<WorkEnvelopeId>(4),
      configuration.reference().clone(),
      ExternalWorkIdentity::new("manual/work-1").unwrap(),
      subject,
      WorkArtifacts::new(artifact(5, 5), artifact(6, 6), Vec::new()).unwrap(),
      WorkClassification::new(
        WorkPriority::new(10).unwrap(),
        RiskClass::Medium,
        FactoryMetadata::default(),
      ),
    )
    .unwrap();
    let run = FactoryRun::admitted(id::<FactoryRunId>(7), &work);
    let admission = PublishedFactoryAdmission {
      work: work.clone(),
      run: run.clone(),
      flow: AdmittedFlow::from_configuration(&configuration, &run).unwrap(),
      admitted_at: time(1),
    };
    let audit = MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::UnauthenticatedManagement,
        identity: None,
      },
      ManagementSecurityScope::trusted_network(),
      "admit-work-1",
    )
    .unwrap();
    let stored = StoredFactoryRun::admitted(&admission, digest(2), &audit).unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.lock().unwrap().factory_runs.insert(run.id(), stored);
    Fixture { store, work, run }
  }

  fn seed_pool_runs(fixture: &Fixture, count: u64) -> Vec<FactoryRunId> {
    let mut runs = vec![fixture.run.id()];
    for index in 1..count {
      let work = WorkEnvelope::new(
        id::<WorkEnvelopeId>(400 + index),
        fixture.work.configuration().clone(),
        ExternalWorkIdentity::new(format!("manual/pool-{index}")).unwrap(),
        fixture.work.subject().clone(),
        fixture.work.artifacts().clone(),
        WorkClassification::new(fixture.work.priority(), fixture.work.risk(), FactoryMetadata::default()),
      )
      .unwrap();
      let run = FactoryRun::admitted(id::<FactoryRunId>(500 + index), &work);
      let admission = PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: {
          let template = fixture
            .store
            .lock()
            .unwrap()
            .factory_runs
            .get(&fixture.run.id())
            .unwrap()
            .admitted_flow
            .clone();
          let root = octacity_server_factory::FlowRun::root(&run, template.closure().root()).unwrap();
          let cycle = octacity_server_factory::WorkflowCycle::initial(&root).unwrap();
          AdmittedFlow::new(
            template.closure().clone(),
            template.limits().clone(),
            root,
            cycle,
            template.wip_limits(),
          )
          .unwrap()
          .with_pool_settings(template.pool_settings().clone())
          .unwrap()
          .with_data_schemas(template.data_schemas().to_vec())
          .unwrap()
        },
        admitted_at: time(1),
      };
      let context = crate::testing::management_mutation_with_request((), format!("pool-admission-{index}"))
        .audit()
        .clone();
      fixture
        .store
        .seed_factory_run(admission, digest(u8::try_from(index + 20).unwrap()), &context)
        .unwrap();
      runs.push(run.id());
    }
    runs
  }

  fn seed_configured_pool_runs(
    fixture: &Fixture,
    count: usize,
    scenario: crate::testing::FactoryPhasePoolContractScenario,
  ) -> Vec<FactoryRunId> {
    let runs = (0..count)
      .map(|index| id::<FactoryRunId>(500 + index as u64))
      .collect::<Vec<_>>();
    let mut dependency = None;
    for index in std::iter::once(count - 1).chain(0..count - 1) {
      let configuration = projected_configuration_for_pool(
        fixture.work.subject().project_id(),
        id::<FactoryConfigurationId>(4000 + index as u64),
        Some(scenario.settings(index, dependency.as_ref())),
      );
      let work = WorkEnvelope::new(
        id::<WorkEnvelopeId>(400 + index as u64),
        configuration.reference().clone(),
        ExternalWorkIdentity::new(format!("manual/configured-pool-{index}")).unwrap(),
        fixture.work.subject().clone(),
        fixture.work.artifacts().clone(),
        WorkClassification::new(fixture.work.priority(), fixture.work.risk(), FactoryMetadata::default()),
      )
      .unwrap();
      let run = FactoryRun::admitted(runs[index], &work);
      let admission = PublishedFactoryAdmission {
        work,
        flow: AdmittedFlow::from_configuration(&configuration, &run).unwrap(),
        run,
        admitted_at: time(1),
      };
      let context = MutationAuditContext::try_new(
        AuditActor {
          kind: AuditActorKind::UnauthenticatedManagement,
          identity: None,
        },
        ManagementSecurityScope::trusted_network(),
        format!("configured-pool-{index}"),
      )
      .unwrap();
      fixture
        .store
        .seed_factory_run(admission, digest(20 + index as u8), &context)
        .unwrap();
      if index == count - 1 {
        run_ready(
          async {
            let snapshot = fixture.store.factory_run_snapshot(runs[index]).await.unwrap();
            dependency = Some(crate::PhasePoolDependency {
              run_id: snapshot.run.id(),
              work_id: snapshot.work.id(),
              work_digest: crate::phase_pool_work_digest(&snapshot),
              terminal: key("accepted"),
            });
          },
          "dependency fixture read yielded",
        );
      }
    }
    runs
  }

  fn configured_pool_settings(
    policy: octacity_server_factory::PhasePoolPolicy,
    severity: FindingSeverity,
  ) -> octacity_server_factory::FlowPoolSettings {
    octacity_server_factory::FlowPoolSettings {
      policy,
      project_priority: 10,
      capabilities: Default::default(),
      selection: octacity_server_factory::FlowDataMapping::new(octacity_server_factory::FlowValueMapping::Constant {
        value: serde_json::json!({"severity": severity, "dependencies": []}),
      })
      .unwrap(),
    }
  }

  #[test]
  fn a_capability_node_cannot_bypass_its_pinned_pool_input() {
    use crate::FactoryPhasePoolStore;
    let fixture = fixture_with_pool(Some(configured_pool_settings(pool_policy(), FindingSeverity::Medium)));
    run_ready(
      async {
        let snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let input = crate::PhasePoolInput {
          run_id: snapshot.run.id(),
          run_version: snapshot.run.version(),
          flow_run_id: snapshot.admitted_flow.root_run().id(),
          cycle_id: snapshot.admitted_flow.initial_cycle().id(),
          node: key("implement"),
          generation: 0,
          severity: FindingSeverity::Medium,
          project_priority: 10,
          phase_input_digest: digest(1),
          capabilities: Default::default(),
          dependencies: vec![],
          reservation: BudgetUsage {
            attempts: 1,
            elapsed_millis: 1,
            tokens: 1,
            cost_micro_units: 1,
            output_bytes: 1,
          },
        };
        assert!(
          fixture.store.publish_phase_ready(pool_policy(), input).await.is_err(),
          "a configured operation needs its durable frozen input before queue publication"
        );
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          snapshot
        );
        let prepared = crate::testing::freeze_phase_pool_contract_input(fixture.store.as_ref(), fixture.run.id()).await;
        let input = crate::testing::phase_pool_contract_input(&prepared, FindingSeverity::Medium);
        assert_eq!(
          input.reservation,
          BudgetUsage {
            attempts: 1,
            elapsed_millis: 1,
            tokens: 1,
            cost_micro_units: 1,
            output_bytes: 1
          }
        );
        let published = fixture
          .store
          .publish_phase_ready(pool_policy(), input.clone())
          .await
          .unwrap();
        assert_eq!(published.input, input);
      },
      "configured input test yielded",
    );
  }

  #[test]
  fn durable_phase_pool_contract() {
    let fixture = fixture();
    let runs = seed_configured_pool_runs(&fixture, 5, crate::testing::FactoryPhasePoolContractScenario::Contract);
    run_ready(
      crate::testing::verify_factory_phase_pool_contract(fixture.store.as_ref(), &runs),
      "pool contract yielded",
    );
  }

  #[test]
  fn phase_pool_progresses_past_blocked_window_without_changing_replay() {
    let fixture = fixture();
    let runs = seed_configured_pool_runs(
      &fixture,
      101,
      crate::testing::FactoryPhasePoolContractScenario::Progress,
    );
    run_ready(
      crate::testing::verify_factory_phase_pool_progress(fixture.store.as_ref(), fixture.store.as_ref(), &runs),
      "pool progress yielded",
    );
  }

  #[test]
  fn phase_pool_rechecks_priority_after_capability_or_lease_changes() {
    for lease_blocked in [false, true] {
      let fixture = fixture();
      let runs = seed_configured_pool_runs(
        &fixture,
        101,
        crate::testing::FactoryPhasePoolContractScenario::ScanInvalidation { lease_blocked },
      );
      run_ready(
        crate::testing::verify_factory_phase_pool_scan_invalidation(
          fixture.store.as_ref(),
          fixture.store.as_ref(),
          &runs,
          lease_blocked,
        ),
        "pool invalidation contract yielded",
      );
    }
  }

  fn pool_policy() -> octacity_server_factory::PhasePoolPolicy {
    octacity_server_factory::PhasePoolPolicy {
      phase: key("development.v1"),
      order: vec![
        octacity_server_factory::PhasePoolOrder::Severity,
        octacity_server_factory::PhasePoolOrder::ProjectPriority,
        octacity_server_factory::PhasePoolOrder::Age,
      ],
      max_wip: 1,
      max_project_wip: 1,
      budget: budget(),
    }
  }

  fn pool_request(policy: &octacity_server_factory::PhasePoolPolicy, byte: u8) -> crate::SelectPhasePool {
    crate::SelectPhasePool {
      policy_digest: policy.digest(),
      request_id: digest(byte),
      owner: key(&format!("pool.worker.{byte}")),
      observed_at: time(10),
      expires_at: time(100),
      capabilities: BTreeSet::new(),
      limit: 1,
    }
  }

  #[test]
  fn phase_pool_concurrent_publications_preserve_ties_and_workers_cannot_overbook() {
    use crate::FactoryPhasePoolStore;
    let fixture = fixture_with_pool(Some(configured_pool_settings(pool_policy(), FindingSeverity::Medium)));
    let runs = seed_pool_runs(&fixture, 5);
    std::thread::scope(|scope| {
      for run in runs.iter().rev() {
        let store = fixture.store.clone();
        let run = *run;
        scope.spawn(move || {
          run_ready(
            async {
              let snapshot = crate::testing::freeze_phase_pool_contract_input(store.as_ref(), run).await;
              let input = crate::testing::phase_pool_contract_input(&snapshot, FindingSeverity::Medium);
              store.publish_phase_ready(pool_policy(), input).await.unwrap();
            },
            "concurrent publication yielded",
          )
        });
      }
    });
    run_ready(
      async {
        let entries = fixture
          .store
          .phase_ready_entries(pool_policy().digest(), None, 100)
          .await
          .unwrap();
        let mut expected = runs.clone();
        expected.sort_by_key(|run| run.as_uuid().as_u128());
        assert_eq!(
          entries.iter().map(|entry| entry.input.run_id).collect::<Vec<_>>(),
          expected
        );
      },
      "ordered pool read yielded",
    );
    let selected = std::thread::scope(|scope| {
      let workers = (110..112)
        .map(|byte| {
          let store = fixture.store.clone();
          scope.spawn(move || {
            let mut selected = Vec::new();
            run_ready(
              async {
                selected = store
                  .select_phase_ready(pool_request(&pool_policy(), byte))
                  .await
                  .unwrap();
              },
              "selection yielded",
            );
            selected
          })
        })
        .collect::<Vec<_>>();
      workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>()
    });
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].entry.input.run_id, runs[0]);
  }

  #[test]
  fn phase_pool_rejects_unreachable_rewritten_and_stale_candidates() {
    use crate::FactoryPhasePoolStore;
    let fixture = fixture_with_pool(Some(configured_pool_settings(pool_policy(), FindingSeverity::Medium)));
    run_ready(
      async {
        let snapshot = crate::testing::freeze_phase_pool_contract_input(fixture.store.as_ref(), fixture.run.id()).await;
        let input = crate::testing::phase_pool_contract_input(&snapshot, FindingSeverity::Medium);
        let entry = fixture
          .store
          .publish_phase_ready(pool_policy(), input.clone())
          .await
          .unwrap();
        let mut unreachable = input.clone();
        unreachable.node = key("evaluate");
        assert!(
          fixture
            .store
            .publish_phase_ready(pool_policy(), unreachable)
            .await
            .is_err()
        );
        let mut rewritten = input.clone();
        rewritten.project_priority += 1;
        assert!(
          fixture
            .store
            .publish_phase_ready(pool_policy(), rewritten)
            .await
            .is_err()
        );
        let mut forged = input.clone();
        forged.dependencies.push(crate::PhasePoolDependency {
          run_id: input.run_id,
          work_id: snapshot.work.id(),
          work_digest: crate::phase_pool_work_digest(&snapshot),
          terminal: key("succeeded"),
        });
        assert!(fixture.store.publish_phase_ready(pool_policy(), forged).await.is_err());
        let mut claim = claim(&fixture);
        claim.expected_version = snapshot.run.version();
        fixture.store.claim_factory_run(claim.clone()).await.unwrap();
        fixture
          .store
          .commit_factory_run_transition(crate::testing::factory_flow_contract_transition(
            &snapshot,
            &claim.record,
            20,
          ))
          .await
          .unwrap();
        assert!(fixture.store.publish_phase_ready(pool_policy(), input).await.is_err());
        assert!(
          fixture
            .store
            .phase_ready_entries(pool_policy().digest(), None, 100)
            .await
            .unwrap()
            .is_empty()
        );
        assert!(
          fixture
            .store
            .select_phase_ready(pool_request(&pool_policy(), 120))
            .await
            .unwrap()
            .is_empty()
        );
        assert_eq!(entry.input.run_version.get(), 2);
      },
      "stale pool test yielded",
    );
  }

  #[test]
  fn phase_pool_checks_every_remaining_budget_and_preserves_running_reservations_after_expiry() {
    use crate::FactoryPhasePoolStore;
    use octacity_server_factory::{NodeAttemptInput, NodeAttemptNumber, NodeExecutionIdentity};
    let fixture = fixture_with_pool(Some(configured_pool_settings(pool_policy(), FindingSeverity::Medium)));
    run_ready(
      async {
        let snapshot = crate::testing::freeze_phase_pool_contract_input(fixture.store.as_ref(), fixture.run.id()).await;
        let input = crate::testing::phase_pool_contract_input(&snapshot, FindingSeverity::Medium);
        let entry = fixture.store.publish_phase_ready(pool_policy(), input).await.unwrap();
        let limit = snapshot.admitted_flow.limits().budget();
        for reserve in [
          BudgetUsage {
            attempts: limit.max_attempts(),
            ..entry.input.reservation
          },
          BudgetUsage {
            elapsed_millis: limit.max_elapsed_millis(),
            ..entry.input.reservation
          },
          BudgetUsage {
            tokens: limit.max_tokens(),
            ..entry.input.reservation
          },
          BudgetUsage {
            cost_micro_units: limit.max_cost_micro_units(),
            ..entry.input.reservation
          },
          BudgetUsage {
            output_bytes: limit.max_output_bytes(),
            ..entry.input.reservation
          },
        ] {
          let mut exhausted = snapshot.clone();
          exhausted
            .budgets
            .iter_mut()
            .find(|row| row.id == exhausted.current.budget_id)
            .unwrap()
            .usage = reserve;
          assert!(!crate::phase_pool_run_budget_ready(&entry, &exhausted));
        }
        let selection = fixture
          .store
          .select_phase_ready(pool_request(&pool_policy(), 130))
          .await
          .unwrap()
          .pop()
          .unwrap();
        let definition = snapshot
          .admitted_flow
          .closure()
          .definition(snapshot.admitted_flow.closure().root())
          .unwrap();
        let node = definition.node(&entry.input.node).unwrap();
        let attempt = NodeAttempt::new(
          snapshot.admitted_flow.root_run(),
          snapshot.admitted_flow.initial_cycle(),
          definition,
          NodeAttemptInput {
            started_at: time(20),
            id: id(700),
            node_key: entry.input.node.clone(),
            node_kind: node.kind(),
            number: NodeAttemptNumber::INITIAL,
            input_digest: entry.input.phase_input_digest,
            budget: node.budget(),
            deadline: time(21),
            execution: NodeExecutionIdentity::External(reference("pool.executor", 132)),
            ownership: octacity_server_factory::FactoryClaimOwnership::new(selection.owner.clone(), selection.claim),
          },
        )
        .unwrap();
        let mut running = snapshot.clone();
        running.flow.attempts.push(attempt.clone());
        assert!(crate::phase_pool_selection_active(&selection, &running, time(101)));
        assert!(crate::phase_pool_selection_active(&selection, &snapshot, time(101)));
        assert!(
          running
            .flow
            .data
            .inputs
            .iter()
            .any(|input| input.digest().unwrap() == attempt.input_digest())
        );
        let mut cancelling = running.clone();
        cancelling
          .lifecycle_checkpoints
          .iter_mut()
          .find(|row| row.id == cancelling.current.lifecycle_checkpoint_id)
          .unwrap()
          .cancellation_requested = true;
        assert!(
          crate::phase_pool_selection_active(&selection, &cancelling, time(30)),
          "requesting cancellation must retain the pool reservation while the operation is stopping"
        );
        cancelling.flow.data.stops.push(
          octacity_server_factory::FlowNodeStop::new(
            &attempt,
            octacity_server_factory::FactoryClaimOwnership::new(selection.owner.clone(), selection.claim),
            octacity_server_factory::FlowNodeStopReason::Cancelled { requested_at: time(25) },
            time(30),
            BudgetUsage::default(),
            None,
          )
          .unwrap(),
        );
        assert!(
          !crate::phase_pool_selection_active(&selection, &cancelling, time(30)),
          "a confirmed stop releases the pool reservation before root finalization"
        );
        let completion = NodeAttemptCompletion::new(
          &attempt,
          definition,
          NodeAttemptCompletionInput {
            outcome: key("succeeded"),
            output_schema: node.outcome(&key("succeeded")).unwrap().schema().clone(),
            output_digest: digest(133),
            ownership: octacity_server_factory::FactoryClaimOwnership::new(selection.owner.clone(), selection.claim),
            usage: BudgetUsage::default(),
            observed_at: time(20),
          },
        )
        .unwrap();
        running.flow.completions.push(completion);
        assert!(!crate::phase_pool_selection_active(&selection, &running, time(30)));
      },
      "budget pool test yielded",
    );
  }

  #[test]
  fn phase_pool_enforces_project_wip_independently_of_pool_capacity() {
    use crate::FactoryPhasePoolStore;
    let mut configured = pool_policy();
    configured.max_wip = 2;
    let fixture = fixture_with_pool(Some(configured_pool_settings(configured, FindingSeverity::Medium)));
    let runs = seed_pool_runs(&fixture, 2);
    run_ready(
      async {
        let mut policy = pool_policy();
        policy.max_wip = 2;
        for run in &runs {
          let snapshot = crate::testing::freeze_phase_pool_contract_input(fixture.store.as_ref(), *run).await;
          fixture
            .store
            .publish_phase_ready(
              policy.clone(),
              crate::testing::phase_pool_contract_input(&snapshot, FindingSeverity::Medium),
            )
            .await
            .unwrap();
        }
        let mut request = pool_request(&policy, 140);
        request.limit = 2;
        assert_eq!(fixture.store.select_phase_ready(request).await.unwrap().len(), 1);
      },
      "project WIP test yielded",
    );
  }

  #[test]
  fn phase_pool_dependencies_require_the_exact_completed_root_resolution() {
    let fixture = fixture_with_pool(Some(configured_pool_settings(pool_policy(), FindingSeverity::Low)));
    run_ready(
      async {
        let mut snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let mut dependency = crate::PhasePoolDependency {
          run_id: fixture.run.id(),
          work_id: fixture.work.id(),
          work_digest: crate::phase_pool_work_digest(&snapshot),
          terminal: key("succeeded"),
        };
        let project = fixture.run.subject().project_id();
        assert!(!crate::phase_pool_dependency_ready(&dependency, project, &snapshot));
        let definition = snapshot
          .admitted_flow
          .closure()
          .definition(snapshot.admitted_flow.closure().root())
          .unwrap()
          .clone();
        let ownership = octacity_server_factory::FactoryClaimOwnership::new(
          key("dependency.worker"),
          FactoryClaim::new(FactoryClaimFence::new(digest(150)), time(10), time(100)).unwrap(),
        );
        for (index, node_key) in ["implement", "validate", "evaluate"].into_iter().enumerate() {
          let (input, attempt) = octacity_server_factory::testing::contract_node_attempt(
            &snapshot.work,
            &snapshot.admitted_flow,
            FlowRuntimeHistory {
              stops: &snapshot.flow.data.stops,
              flow_runs: &snapshot.flow.runs,
              cycles: &snapshot.flow.cycles,
              attempts: &snapshot.flow.attempts,
              completions: &snapshot.flow.completions,
            },
            &snapshot.flow.data,
            key(node_key),
            id(800 + index as u64),
            ownership.clone(),
            time(19 + index as i64),
          )
          .unwrap();
          let (record, completion) = octacity_server_factory::testing::contract_node_result(
            &snapshot.admitted_flow,
            &input,
            &attempt,
            key("succeeded"),
            ownership.clone(),
            time(20 + index as i64),
          )
          .unwrap();
          if !snapshot.flow.data.inputs.contains(&input) {
            snapshot.flow.data.inputs.push(input);
          }
          snapshot.flow.data.records.push(record);
          snapshot.flow.attempts.push(attempt);
          snapshot.flow.completions.push(completion);
          validate_flow_runtime_history(
            &snapshot.admitted_flow,
            FlowRuntimeHistory {
              stops: &snapshot.flow.data.stops,
              flow_runs: &snapshot.flow.runs,
              cycles: &snapshot.flow.cycles,
              attempts: &snapshot.flow.attempts,
              completions: &snapshot.flow.completions,
            },
          )
          .unwrap();
          assert_eq!(
            crate::phase_pool_dependency_ready(&dependency, project, &snapshot),
            node_key == "evaluate"
          );
          if node_key == "implement" {
            let successor_input =
              octacity_server_factory::FlowNodeInput::prepare(octacity_server_factory::FlowInputPreparation {
                work: &snapshot.work,
                admitted: &snapshot.admitted_flow,
                flow: snapshot.admitted_flow.root_run(),
                cycle: snapshot.admitted_flow.initial_cycle(),
                node: key("validate"),
                schema: snapshot
                  .admitted_flow
                  .data_schema(definition.node(&key("validate")).unwrap().input_schema().unwrap())
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
            snapshot.flow.data.inputs.push(successor_input);
            let successor = crate::testing::phase_pool_contract_input(&snapshot, FindingSeverity::Low);
            assert!(crate::derive_phase_pool_entry(&pool_policy(), successor, &snapshot).is_ok());
          }
        }
        dependency.work_digest = digest(154);
        assert!(!crate::phase_pool_dependency_ready(&dependency, project, &snapshot));
        dependency.work_digest = crate::phase_pool_work_digest(&snapshot);
        dependency.terminal = key("failed");
        assert!(!crate::phase_pool_dependency_ready(&dependency, project, &snapshot));
        dependency.terminal = key("succeeded");
        assert!(!crate::phase_pool_dependency_ready(
          &dependency,
          id::<ProjectId>(999),
          &snapshot
        ));
      },
      "dependency resolution test yielded",
    );
  }

  fn audit(run_id: FactoryRunId, operation: &str, byte: u8, recorded_at: i64) -> FactoryAuditFact {
    FactoryAuditFact::new(
      run_id,
      AuditActorKind::Worker,
      Some(digest(byte)),
      key(operation),
      digest(byte.wrapping_add(1)),
      key("accepted"),
      time(recorded_at),
    )
  }

  fn claim(fixture: &Fixture) -> ClaimFactoryRun {
    let claim = FactoryClaim::new(FactoryClaimFence::new(digest(10)), time(10), time(100)).unwrap();
    ClaimFactoryRun {
      run_id: fixture.run.id(),
      expected_version: FactoryRunVersion::INITIAL,
      record: FactoryRunClaimRecord::new(fixture.run.id(), key("worker.one"), claim),
      audit: audit(fixture.run.id(), "factory.claimed", 11, 10),
    }
  }

  fn budget() -> BudgetLimit {
    BudgetLimit::new(10, 10_000, 10_000, 10_000, 10_000).unwrap()
  }

  fn full_history(work: &WorkEnvelope, run: &FactoryRun) -> FullHistory {
    let admitted = admitted_flow(run);
    let root = admitted.root_run();
    let cycle = admitted.initial_cycle();
    let ownership = octacity_server_factory::FactoryClaimOwnership::new(
      key("worker.one"),
      FactoryClaim::new(FactoryClaimFence::new(digest(10)), time(10), time(100)).unwrap(),
    );
    let mut flow = FactoryFlowHistory::default();
    let mut node_key = admitted
      .closure()
      .definition(root.definition())
      .unwrap()
      .entry()
      .clone();
    for number in 0..2 {
      let history = FlowRuntimeHistory {
        stops: &[],
        flow_runs: std::slice::from_ref(root),
        cycles: std::slice::from_ref(cycle),
        attempts: &flow.attempts,
        completions: &flow.completions,
      };
      let (input, attempt) = octacity_server_factory::testing::contract_node_attempt(
        work,
        &admitted,
        history,
        &flow.data,
        node_key.clone(),
        id::<NodeAttemptId>(20 + number),
        ownership.clone(),
        time(10 + i64::try_from(number).unwrap() * 10),
      )
      .unwrap();
      let (record, completion) = octacity_server_factory::testing::contract_node_result(
        &admitted,
        &input,
        &attempt,
        key("succeeded"),
        ownership.clone(),
        time(20),
      )
      .unwrap();
      flow.data.inputs.push(input);
      flow.data.records.push(record);
      flow.attempts.push(attempt);
      flow.completions.push(completion);
      if number == 0 {
        let history = FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(root),
          cycles: std::slice::from_ref(cycle),
          attempts: &flow.attempts,
          completions: &flow.completions,
        };
        let next = octacity_server_factory::FlowInterpreter::new(&admitted.validated().unwrap())
          .advance(root, cycle, &flow.attempts[0], &flow.completions[0], history)
          .unwrap();
        let octacity_server_factory::FlowDirective::ExecuteNode { node, .. } = &next[0] else {
          panic!("fixture next node");
        };
        node_key = node.clone();
      }
    }
    FullHistory {
      append: FactoryRunHistoryAppend { flow },
      current: FactoryRunCurrentProjection {
        budget_id: digest(0),
        lifecycle_checkpoint_id: digest(0),
      },
    }
  }

  fn transition(fixture: &Fixture, claim: &ClaimFactoryRun, history: FullHistory) -> CommitFactoryRunTransition {
    let version = FactoryRunVersion::new(2).unwrap();
    let budget = FactoryBudgetRecord::new(
      fixture.run.id(),
      version,
      BudgetUsage {
        attempts: 2,
        elapsed_millis: 20,
        tokens: 30,
        cost_micro_units: 40,
        output_bytes: 50,
      },
      time(20),
    );
    let mut current = history.current;
    current.budget_id = budget.id;
    let lifecycle_checkpoint = FactoryLifecycleCheckpoint::new(
      fixture.run.id(),
      version,
      FactoryLifecycleProgress::FlowRunning {
        root: admitted_flow(&fixture.run).root_run().id(),
      },
      false,
      time(20),
    );
    current.lifecycle_checkpoint_id = lifecycle_checkpoint.id;
    CommitFactoryRunTransition {
      run_id: fixture.run.id(),
      expected_version: FactoryRunVersion::INITIAL,
      claim_id: claim.record.id,
      owner: claim.record.owner.clone(),
      fence: claim.record.claim.fence(),
      committed_at: time(20),
      next_run: FactoryRun::restore(
        fixture.run.id(),
        fixture.run.configuration().clone(),
        &fixture.work,
        fixture.run.subject().clone(),
        FactoryRunState::Running,
        version,
      )
      .unwrap(),
      budget,
      lifecycle_checkpoint,
      append: history.append,
      current,
      audit: audit(fixture.run.id(), "factory.transition", 40, 20),
      outbox: vec![FactoryOutboxRecord::pending(
        digest(41),
        fixture.run.id(),
        key("delivery.request"),
        digest(42),
        time(20),
        time(20),
      )],
    }
  }

  async fn seed_full_history(fixture: &Fixture) {
    let claim = claim(fixture);
    fixture.store.claim_factory_run(claim.clone()).await.unwrap();
    fixture
      .store
      .commit_factory_run_transition(transition(fixture, &claim, full_history(&fixture.work, &fixture.run)))
      .await
      .unwrap();
  }

  fn control_context(request_identity: &str) -> MutationAuditContext {
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some("operator-1".to_owned()),
      },
      ManagementSecurityScope::trusted_network(),
      request_identity,
    )
    .unwrap()
  }

  fn control_request(
    fixture: &Fixture,
    intent: FactoryRunControlIntent,
    key: &str,
  ) -> ManagementMutation<ApplyFactoryRunControl> {
    let version = fixture
      .store
      .lock()
      .unwrap()
      .factory_runs
      .get(&fixture.run.id())
      .unwrap()
      .run
      .version();
    ManagementMutation::new(
      ApplyFactoryRunControl {
        run_id: fixture.run.id(),
        expected_version: version,
        idempotency_key: IdempotencyKey::new(key).unwrap(),
        intent,
        requested_at: time(30),
      },
      control_context(&format!("request-{key}")),
    )
  }

  #[test]
  fn admitted_snapshot_and_claim_history_are_immutable_and_fenced() {
    run_ready(
      async {
        let fixture = fixture();
        let initial = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert_eq!(initial.run, fixture.run);
        assert_eq!(initial.budgets.len(), 1);
        assert_eq!(initial.lifecycle_checkpoints.len(), 1);
        let initial_audit_page = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(initial.run.id(), FactoryRunDiagnosticKind::Audit, None, 100).unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(initial_audit_page.items.len(), 1);
        assert_eq!(initial.outbox.len(), 1);
        assert!(initial.current_claim.is_none());

        let request = claim(&fixture);
        let applied = fixture.store.claim_factory_run(request.clone()).await.unwrap();
        assert_eq!(applied.disposition, MutationDisposition::Applied);
        let replayed = fixture.store.claim_factory_run(request.clone()).await.unwrap();
        assert_eq!(replayed.disposition, MutationDisposition::Replayed);

        let overlapping_claim = FactoryClaim::new(FactoryClaimFence::new(digest(12)), time(50), time(150)).unwrap();
        let rejected = fixture
          .store
          .claim_factory_run(ClaimFactoryRun {
            run_id: fixture.run.id(),
            expected_version: FactoryRunVersion::INITIAL,
            record: FactoryRunClaimRecord::new(fixture.run.id(), key("worker.two"), overlapping_claim),
            audit: audit(fixture.run.id(), "factory.claimed", 13, 50),
          })
          .await
          .unwrap_err();
        assert_eq!(rejected, conflict());
        let takeover = FactoryClaim::new(FactoryClaimFence::new(digest(14)), time(100), time(200)).unwrap();
        let takeover = FactoryRunClaimRecord::new(fixture.run.id(), key("worker.two"), takeover);
        fixture
          .store
          .claim_factory_run(ClaimFactoryRun {
            run_id: fixture.run.id(),
            expected_version: FactoryRunVersion::INITIAL,
            record: takeover.clone(),
            audit: audit(fixture.run.id(), "factory.claimed", 15, 100),
          })
          .await
          .unwrap();
        let snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let claims = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(fixture.run.id(), FactoryRunDiagnosticKind::Claim, None, 100).unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(claims.items.len(), 2);
        assert!(
          claims
            .items
            .contains(&FactoryRunDiagnosticRecord::Claim(Box::new(request.record.clone())))
        );
        assert!(
          claims
            .items
            .contains(&FactoryRunDiagnosticRecord::Claim(Box::new(takeover.clone())))
        );
        assert_eq!(snapshot.current_claim, Some(takeover));
      },
      "Factory claim contract is ready",
    );
  }

  #[test]
  fn completion_pages_follow_completion_identity_order_across_continuations() {
    run_ready(
      async {
        let fixture = fixture();
        seed_full_history(&fixture).await;
        let snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let expected_minimum = snapshot
          .flow
          .completions
          .iter()
          .map(NodeAttemptCompletion::id)
          .min()
          .unwrap();
        let expected_maximum = snapshot
          .flow
          .completions
          .iter()
          .map(NodeAttemptCompletion::id)
          .max()
          .unwrap();
        assert_ne!(expected_minimum, expected_maximum);
        let first = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(
              fixture.run.id(),
              FactoryRunDiagnosticKind::NodeAttemptCompletion,
              None,
              1,
            )
            .unwrap(),
          )
          .await
          .unwrap();
        let FactoryRunDiagnosticRecord::NodeAttemptCompletion(row) = &first.items[0] else {
          panic!("completion collection")
        };
        assert_eq!(row.id(), expected_minimum);
        let last = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(
              fixture.run.id(),
              FactoryRunDiagnosticKind::NodeAttemptCompletion,
              first.next,
              1,
            )
            .unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(last.items.len(), 1);
        let FactoryRunDiagnosticRecord::NodeAttemptCompletion(row) = &last.items[0] else {
          panic!("completion collection")
        };
        assert_eq!(row.id(), expected_maximum);
        assert!(last.next.is_none());
      },
      "completion pagination is ready",
    );
  }

  #[test]
  fn diagnostic_pages_are_bounded_stable_and_collection_typed() {
    run_ready(
      async {
        let fixture = fixture();
        seed_full_history(&fixture).await;
        let second = fixture
          .store
          .factory_run_snapshot(fixture.run.id())
          .await
          .unwrap()
          .flow
          .attempts[1]
          .clone();

        let first = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(fixture.run.id(), FactoryRunDiagnosticKind::NodeAttempt, None, 1).unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(first.items.len(), 1);
        let cursor = first.next.expect("another node attempt exists");
        let final_page = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(fixture.run.id(), FactoryRunDiagnosticKind::NodeAttempt, Some(cursor), 1)
              .unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(
          final_page.items,
          vec![FactoryRunDiagnosticRecord::NodeAttempt(Box::new(second))]
        );
        assert!(final_page.next.is_none());
        assert!(
          ListFactoryRunDiagnostics::new(
            fixture.run.id(),
            FactoryRunDiagnosticKind::NodeAttempt,
            None,
            crate::MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE + 1,
          )
          .is_err()
        );
      },
      "Factory diagnostic pagination contract is ready",
    );
  }

  #[test]
  fn batch_claim_takeover_rejects_the_previous_owners_completion_atomically() {
    run_ready(
      async {
        let fixture = fixture();
        let first = fixture
          .store
          .claim_factory_runs(ClaimFactoryRuns::new(key("worker.one"), time(10), time(20), 1).unwrap())
          .await
          .unwrap()
          .pop()
          .expect("fixture Run is claimed");
        let stale_claim = ClaimFactoryRun {
          run_id: first.run_id,
          expected_version: first.expected_version,
          record: first.record,
          audit: audit(fixture.run.id(), "factory.claimed", 60, 10),
        };
        let stale_completion = transition(&fixture, &stale_claim, full_history(&fixture.work, &fixture.run));

        let replacement = fixture
          .store
          .claim_factory_runs(ClaimFactoryRuns::new(key("worker.two"), time(20), time(30), 1).unwrap())
          .await
          .unwrap();
        assert_eq!(replacement.len(), 1);
        let after_takeover = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();

        assert_eq!(
          fixture
            .store
            .commit_factory_run_transition(stale_completion)
            .await
            .unwrap_err(),
          conflict()
        );
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          after_takeover
        );
      },
      "stale Factory completion rejection is ready",
    );
  }

  #[test]
  fn batch_claim_failure_does_not_publish_an_earlier_staged_claim() {
    run_ready(
      async {
        let fixture = fixture();
        let second_run = FactoryRun::admitted(id::<FactoryRunId>(8), &fixture.work);
        let audit_context = MutationAuditContext::try_new(
          AuditActor {
            kind: AuditActorKind::UnauthenticatedManagement,
            identity: None,
          },
          ManagementSecurityScope::trusted_network(),
          "admit-work-2",
        )
        .unwrap();
        let mut second = StoredFactoryRun::admitted(
          &PublishedFactoryAdmission {
            work: fixture.work.clone(),
            run: second_run.clone(),
            flow: admitted_flow(&second_run),
            admitted_at: time(1),
          },
          digest(62),
          &audit_context,
        )
        .unwrap();
        second.current_claim_id = Some(digest(63));
        fixture
          .store
          .lock()
          .unwrap()
          .factory_runs
          .insert(second_run.id(), second);

        assert_eq!(
          fixture
            .store
            .claim_factory_runs(ClaimFactoryRuns::new(key("worker.batch"), time(10), time(20), 2).unwrap())
            .await
            .unwrap_err(),
          conflict()
        );
        let first = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert!(first.current_claim.is_none());
        let claims = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(first.run.id(), FactoryRunDiagnosticKind::Claim, None, 100).unwrap(),
          )
          .await
          .unwrap();
        assert!(claims.items.is_empty());
      },
      "Factory batch claim remains atomic",
    );
  }

  #[test]
  fn outbox_workers_claim_only_their_selected_effect_kind() {
    run_ready(
      async {
        let fixture = fixture();
        let before = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let unrelated = fixture
          .store
          .claim_factory_outbox(
            ClaimFactoryOutbox::new(key("reporter"), time(10), time(20), 1)
              .unwrap()
              .only_kind(key("work.status.projection")),
          )
          .await
          .unwrap();
        assert!(
          unrelated.is_empty(),
          "a reporter cannot consume the admission worker's event"
        );
        assert_eq!(
          fixture
            .store
            .factory_run_snapshot(fixture.run.id())
            .await
            .unwrap()
            .outbox,
          before.outbox
        );
        let matching = fixture
          .store
          .claim_factory_outbox(
            ClaimFactoryOutbox::new(key("admission"), time(10), time(20), 1)
              .unwrap()
              .only_kind(before.outbox[0].kind.clone()),
          )
          .await
          .unwrap();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].record.operation_id, before.outbox[0].operation_id);
      },
      "outbox kind selection is isolated",
    );
  }

  #[test]
  fn outbox_unknown_response_is_reclaimed_and_settled_under_one_operation_identity() {
    run_ready(
      async {
        let fixture = fixture();
        let first = fixture
          .store
          .claim_factory_outbox(ClaimFactoryOutbox::new(key("dispatcher.one"), time(10), time(20), 1).unwrap())
          .await
          .unwrap()
          .pop()
          .expect("admission outbox is due");
        assert_eq!(first.record.attempt, 0);
        let operation_id = first.record.operation_id;

        let replacement = fixture
          .store
          .claim_factory_outbox(ClaimFactoryOutbox::new(key("dispatcher.two"), time(20), time(30), 1).unwrap())
          .await
          .unwrap()
          .pop()
          .expect("expired unknown outcome is reclaimed");
        assert_eq!(replacement.record.operation_id, operation_id);
        assert_eq!(replacement.record.attempt, 1);
        assert_eq!(replacement.record.owner.as_ref(), Some(&key("dispatcher.two")));

        let stale = fixture
          .store
          .settle_factory_outbox(SettleFactoryOutbox {
            operation_id,
            owner: key("dispatcher.one"),
            fence: first.record.claim.unwrap().fence(),
            observed_at: time(21),
            settlement: FactoryOutboxSettlement::Delivered,
          })
          .await
          .unwrap_err();
        assert_eq!(stale, conflict());

        let settlement = SettleFactoryOutbox {
          operation_id,
          owner: key("dispatcher.two"),
          fence: replacement.record.claim.unwrap().fence(),
          observed_at: time(21),
          settlement: FactoryOutboxSettlement::Delivered,
        };
        let delivered = fixture.store.settle_factory_outbox(settlement.clone()).await.unwrap();
        assert_eq!(delivered.state, FactoryOutboxState::Delivered);
        assert_eq!(
          fixture.store.settle_factory_outbox(settlement).await.unwrap(),
          delivered
        );
        assert!(
          fixture
            .store
            .claim_factory_outbox(ClaimFactoryOutbox::new(key("dispatcher.three"), time(30), time(40), 1).unwrap())
            .await
            .unwrap()
            .is_empty()
        );
      },
      "Factory outbox recovery contract is ready",
    );
  }

  #[test]
  fn transition_appends_complete_history_and_projects_only_immutable_rows() {
    run_ready(
      async {
        let fixture = fixture();
        let claim = claim(&fixture);
        fixture.store.claim_factory_run(claim.clone()).await.unwrap();
        let request = transition(&fixture, &claim, full_history(&fixture.work, &fixture.run));
        let expected_current = request.current.clone();

        let outcome = fixture.store.commit_factory_run_transition(request).await.unwrap();
        assert_eq!(outcome.version, FactoryRunVersion::new(2).unwrap());
        assert_eq!(outcome.current, expected_current);
        let snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert_eq!(snapshot.run.version(), FactoryRunVersion::new(2).unwrap());
        assert_eq!(snapshot.budgets.len(), 2);
        assert_eq!(snapshot.lifecycle_checkpoints.len(), 2);
        assert_eq!(snapshot.flow.completions.len(), 2);
        assert_eq!(snapshot.flow.data.records.len(), 2);
        assert_eq!(snapshot.flow.data.inputs.len(), 2);
        let audits = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(fixture.run.id(), FactoryRunDiagnosticKind::Audit, None, 100).unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(audits.items.len(), 3);
        assert_eq!(snapshot.outbox.len(), 2);
        assert_eq!(snapshot.current, expected_current);
      },
      "Factory transition contract is ready",
    );
  }

  #[test]
  fn common_results_reject_inconsistent_execution_causality_atomically() {
    run_ready(
      async {
        let fixture = fixture();
        let claim = claim(&fixture);
        fixture.store.claim_factory_run(claim.clone()).await.unwrap();
        let mut request = transition(&fixture, &claim, full_history(&fixture.work, &fixture.run));
        let flow = admitted_flow(&fixture.run);
        let node = &request.append.flow.attempts[0];
        let old = &request.append.flow.completions[0];
        request.append.flow.completions[0] = NodeAttemptCompletion::new(
          node,
          flow.closure().definition(flow.root_run().definition()).unwrap(),
          NodeAttemptCompletionInput {
            outcome: old.outcome().clone(),
            output_schema: old.output_schema().clone(),
            output_digest: digest(99),
            ownership: octacity_server_factory::FactoryClaimOwnership::new(old.owner().clone(), old.claim()),
            usage: old.usage(),
            observed_at: old.observed_at(),
          },
        )
        .unwrap();
        let before = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();

        assert!(matches!(
          fixture.store.commit_factory_run_transition(request).await.unwrap_err(),
          StoreError::InvalidInput { .. }
        ));
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          before
        );
      },
      "Factory Build causality validation is ready",
    );
  }

  #[test]
  fn rejected_rewrites_missing_projections_and_stale_fences_are_atomic() {
    run_ready(
      async {
        let fixture = fixture();
        let claim = claim(&fixture);
        fixture.store.claim_factory_run(claim.clone()).await.unwrap();
        let first = transition(&fixture, &claim, full_history(&fixture.work, &fixture.run));
        fixture
          .store
          .commit_factory_run_transition(first.clone())
          .await
          .unwrap();
        let committed = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();

        let version = FactoryRunVersion::new(3).unwrap();
        let budget = FactoryBudgetRecord::new(fixture.run.id(), version, first.budget.usage, time(30));
        let next_run = FactoryRun::restore(
          fixture.run.id(),
          fixture.run.configuration().clone(),
          &fixture.work,
          fixture.run.subject().clone(),
          FactoryRunState::Completed,
          version,
        )
        .unwrap();
        let mut duplicate = first.clone();
        duplicate.expected_version = FactoryRunVersion::new(2).unwrap();
        duplicate.committed_at = time(30);
        duplicate.next_run = next_run.clone();
        duplicate.budget = budget.clone();
        duplicate.lifecycle_checkpoint = FactoryLifecycleCheckpoint::new(
          fixture.run.id(),
          version,
          FactoryLifecycleProgress::FlowResolved {
            root: admitted_flow(&fixture.run).root_run().id(),
            terminal: key("succeeded"),
          },
          false,
          time(30),
        );
        duplicate.current.budget_id = budget.id;
        duplicate.current.lifecycle_checkpoint_id = duplicate.lifecycle_checkpoint.id;
        duplicate.audit = audit(fixture.run.id(), "factory.complete", 50, 30);
        duplicate.outbox.clear();
        assert_eq!(
          fixture
            .store
            .commit_factory_run_transition(duplicate)
            .await
            .unwrap_err(),
          invalid_transition(StoreOperation::CommitFactoryRunTransition)
        );
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          committed
        );

        let mut missing_projection = CommitFactoryRunTransition {
          run_id: fixture.run.id(),
          expected_version: FactoryRunVersion::new(2).unwrap(),
          claim_id: claim.record.id,
          owner: claim.record.owner.clone(),
          fence: claim.record.claim.fence(),
          committed_at: time(30),
          next_run,
          budget: budget.clone(),
          lifecycle_checkpoint: FactoryLifecycleCheckpoint::new(
            fixture.run.id(),
            version,
            FactoryLifecycleProgress::FlowResolved {
              root: admitted_flow(&fixture.run).root_run().id(),
              terminal: key("succeeded"),
            },
            false,
            time(30),
          ),
          append: FactoryRunHistoryAppend::default(),
          current: committed.current.clone(),
          audit: audit(fixture.run.id(), "factory.complete", 51, 30),
          outbox: Vec::new(),
        };
        missing_projection.current.budget_id = budget.id;
        missing_projection.current.lifecycle_checkpoint_id = missing_projection.lifecycle_checkpoint.id;
        missing_projection.current.budget_id = digest(99);
        assert!(matches!(
          fixture
            .store
            .commit_factory_run_transition(missing_projection.clone())
            .await
            .unwrap_err(),
          StoreError::InvalidInput { .. }
        ));
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          committed
        );

        missing_projection.current.budget_id = budget.id;
        missing_projection.fence = FactoryClaimFence::new(digest(99));
        assert_eq!(
          fixture
            .store
            .commit_factory_run_transition(missing_projection)
            .await
            .unwrap_err(),
          conflict()
        );
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          committed
        );

        let mut decreasing = CommitFactoryRunTransition {
          run_id: fixture.run.id(),
          expected_version: FactoryRunVersion::new(2).unwrap(),
          claim_id: claim.record.id,
          owner: claim.record.owner,
          fence: claim.record.claim.fence(),
          committed_at: time(30),
          next_run: FactoryRun::restore(
            fixture.run.id(),
            fixture.run.configuration().clone(),
            &fixture.work,
            fixture.run.subject().clone(),
            FactoryRunState::Completed,
            version,
          )
          .unwrap(),
          budget: FactoryBudgetRecord::new(fixture.run.id(), version, BudgetUsage::default(), time(30)),
          lifecycle_checkpoint: FactoryLifecycleCheckpoint::new(
            fixture.run.id(),
            version,
            FactoryLifecycleProgress::FlowResolved {
              root: admitted_flow(&fixture.run).root_run().id(),
              terminal: key("succeeded"),
            },
            false,
            time(30),
          ),
          append: FactoryRunHistoryAppend::default(),
          current: committed.current.clone(),
          audit: audit(fixture.run.id(), "factory.complete", 52, 30),
          outbox: Vec::new(),
        };
        decreasing.current.budget_id = decreasing.budget.id;
        decreasing.current.lifecycle_checkpoint_id = decreasing.lifecycle_checkpoint.id;
        assert!(matches!(
          fixture
            .store
            .commit_factory_run_transition(decreasing)
            .await
            .unwrap_err(),
          StoreError::InvalidInput { .. }
        ));
        assert_eq!(
          fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap(),
          committed
        );
      },
      "Factory history rewrite rejection is ready",
    );
  }

  #[test]
  fn cancellation_is_replayable_and_preserves_completed_factory_evidence() {
    run_ready(
      async {
        let fixture = fixture();
        seed_full_history(&fixture).await;
        let before = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let request = control_request(&fixture, FactoryRunControlIntent::Cancel, "cancel-1");

        let applied = fixture.store.apply_factory_run_control(request.clone()).await.unwrap();
        let (mut replay, _) = request.into_parts();
        replay.requested_at = time(200);
        let replayed = fixture
          .store
          .apply_factory_run_control(ManagementMutation::new(
            replay,
            control_context("lost-response-follow-up"),
          ))
          .await
          .unwrap();
        assert_eq!(applied.disposition, MutationDisposition::Applied);
        assert_eq!(replayed.disposition, MutationDisposition::Replayed);
        assert_eq!(applied.version, replayed.version);
        assert!(applied.cancellation_requested);

        let after = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert!(after.current_claim.is_none());
        assert_eq!(after.flow, before.flow);
        assert_eq!(after.controls.len(), 1);
        assert!(matches!(after.controls[0].intent, FactoryRunControlIntent::Cancel));

        let conflicting = control_request(&fixture, FactoryRunControlIntent::Cancel, "cancel-1");
        assert_eq!(
          fixture.store.apply_factory_run_control(conflicting).await.unwrap_err(),
          conflict()
        );
      },
      "Factory cancellation control is ready",
    );
  }
}
