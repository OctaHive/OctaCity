use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use octacity_server_domain::{BuildId, EntityKind};
use octacity_server_factory::{
  Assessment, AssessmentId, ChangeSet, ChangeSetId, Decision, DecisionId, DecisionSignalReceipt,
  DecisionSignalReceiptId, DecisionSignalRequest, DecisionSignalRequestId, DeliveryAttempt, DeliveryAttemptId,
  Escalation, EscalationId, EvaluationPlan, EvaluationPlanId, EvidenceManifest, EvidenceManifestId, FactoryClaim,
  FactoryClaimFence, FactoryDigest, FactoryKey, FactoryLifecycleProgress, FactoryRun, FactoryRunId, FactoryRunState,
  FactoryRunVersion, FactoryWipUsage, MacroCall, MacroCallId, ReportingAttempt, ReportingAttemptId, StageAttempt,
  StageAttemptCompletion, StageAttemptId, WorkEnvelope,
};

use crate::{
  ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns, ClaimedFactoryOutbox,
  ClaimedFactoryRun, CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome, FactoryAuditFact,
  FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryOutboxRecord, FactoryOutboxSettlement, FactoryRunClaimRecord,
  FactoryRunCurrentProjection, FactoryRunHistoryAppend, FactoryRunSnapshot, FactoryRunStore,
  MAX_FACTORY_RUN_SNAPSHOT_RECORDS, MAX_FACTORY_TRANSITION_OUTBOX_RECORDS, MAX_FACTORY_TRANSITION_RECORDS,
  MutationAuditContext, MutationDisposition, PublishedFactoryAdmission, SettleFactoryOutbox, StoreError,
  StoreInputError, StoreOperation,
};

use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;

#[derive(Clone)]
pub(super) struct StoredFactoryRun {
  work: WorkEnvelope,
  run: FactoryRun,
  admitted_at: octacity_server_domain::Timestamp,
  current_claim_id: Option<FactoryDigest>,
  claims: BTreeMap<FactoryDigest, FactoryRunClaimRecord>,
  budgets: BTreeMap<FactoryDigest, FactoryBudgetRecord>,
  lifecycle_checkpoints: BTreeMap<FactoryDigest, FactoryLifecycleCheckpoint>,
  stage_attempts: BTreeMap<StageAttemptId, StageAttempt>,
  stage_attempt_completions: BTreeMap<FactoryDigest, StageAttemptCompletion>,
  macro_calls: BTreeMap<MacroCallId, MacroCall>,
  signal_requests: BTreeMap<DecisionSignalRequestId, DecisionSignalRequest>,
  signal_receipts: BTreeMap<DecisionSignalReceiptId, DecisionSignalReceipt>,
  linked_builds: BTreeMap<BuildId, crate::FactoryBuildLink>,
  candidates: BTreeMap<ChangeSetId, ChangeSet>,
  evidence: BTreeMap<EvidenceManifestId, EvidenceManifest>,
  evaluation_plans: BTreeMap<EvaluationPlanId, EvaluationPlan>,
  assessments: BTreeMap<AssessmentId, Assessment>,
  decisions: BTreeMap<DecisionId, Decision>,
  escalations: BTreeMap<EscalationId, Escalation>,
  delivery_attempts: BTreeMap<DeliveryAttemptId, DeliveryAttempt>,
  reporting_attempts: BTreeMap<ReportingAttemptId, ReportingAttempt>,
  audit: BTreeMap<FactoryDigest, FactoryAuditFact>,
  outbox: BTreeMap<FactoryDigest, FactoryOutboxRecord>,
  current: FactoryRunCurrentProjection,
}

impl StoredFactoryRun {
  pub(super) const fn run(&self) -> &FactoryRun {
    &self.run
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
      octacity_server_factory::DecisionSignalProgress::Disabled,
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
      stage_attempt_id: None,
      macro_call_id: None,
      signal_request_id: None,
      signal_receipt_id: None,
      build_id: None,
      candidate_id: None,
      evidence_id: None,
      evaluation_plan_id: None,
      decision_id: None,
      escalation_id: None,
      delivery_attempt_id: None,
      reporting_attempt_id: None,
    };
    Ok(Self {
      work: admission.work.clone(),
      run: admission.run.clone(),
      admitted_at: admission.admitted_at,
      current_claim_id: None,
      claims: BTreeMap::new(),
      budgets: BTreeMap::from([(budget.id, budget)]),
      lifecycle_checkpoints: BTreeMap::from([(lifecycle_checkpoint.id, lifecycle_checkpoint)]),
      stage_attempts: BTreeMap::new(),
      stage_attempt_completions: BTreeMap::new(),
      macro_calls: BTreeMap::new(),
      signal_requests: BTreeMap::new(),
      signal_receipts: BTreeMap::new(),
      linked_builds: BTreeMap::new(),
      candidates: BTreeMap::new(),
      evidence: BTreeMap::new(),
      evaluation_plans: BTreeMap::new(),
      assessments: BTreeMap::new(),
      decisions: BTreeMap::new(),
      escalations: BTreeMap::new(),
      delivery_attempts: BTreeMap::new(),
      reporting_attempts: BTreeMap::new(),
      audit: BTreeMap::from([(audit.id, audit)]),
      outbox: BTreeMap::from([(outbox.id, outbox)]),
      current,
    })
  }

  fn snapshot(&self) -> Result<FactoryRunSnapshot, StoreError> {
    self.validate_integrity()?;
    require_with(
      self.record_count() <= MAX_FACTORY_RUN_SNAPSHOT_RECORDS,
      StoreOperation::ReadFactoryRunSnapshot,
      StoreInputError::InvalidFactoryRunTransition,
    )?;
    Ok(FactoryRunSnapshot {
      work: self.work.clone(),
      run: self.run.clone(),
      current_claim: self
        .current_claim_id
        .map(|id| self.claims.get(&id).cloned().ok_or(StoreError::Unavailable))
        .transpose()?,
      claims: values(&self.claims),
      budgets: values(&self.budgets),
      lifecycle_checkpoints: values(&self.lifecycle_checkpoints),
      stage_attempts: values(&self.stage_attempts),
      stage_attempt_completions: values(&self.stage_attempt_completions),
      macro_calls: values(&self.macro_calls),
      signal_requests: values(&self.signal_requests),
      signal_receipts: values(&self.signal_receipts),
      linked_builds: values(&self.linked_builds),
      candidates: values(&self.candidates),
      evidence: values(&self.evidence),
      evaluation_plans: values(&self.evaluation_plans),
      assessments: values(&self.assessments),
      decisions: values(&self.decisions),
      escalations: values(&self.escalations),
      delivery_attempts: values(&self.delivery_attempts),
      reporting_attempts: values(&self.reporting_attempts),
      audit: values(&self.audit),
      outbox: values(&self.outbox),
      current: self.current.clone(),
    })
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
      .claims
      .values()
      .any(|record| record != &FactoryRunClaimRecord::new(run_id, record.owner.clone(), record.claim))
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
              checkpoint.signal,
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
    for stage in self.stage_attempts.values() {
      require(
        stage.run_id() == run_id
          && stage.subject() == subject
          && self
            .claims
            .values()
            .any(|claim| claim.owner == *stage.owner() && claim.claim == stage.claim()),
      )?;
    }
    for completion in self.stage_attempt_completions.values() {
      let stage = self.stage_attempts.get(&completion.stage_attempt_id());
      require(
        completion.run_id() == run_id
          && completion
            .claim()
            .authorize(completion.claim().fence(), completion.observed_at())
            .is_ok()
          && self
            .claims
            .values()
            .any(|claim| claim.owner == *completion.owner() && claim.claim == completion.claim())
          && stage.is_some_and(|stage| completion.usage().validate(stage.budget()).is_ok()),
      )?;
    }
    require(
      self
        .stage_attempt_completions
        .values()
        .map(StageAttemptCompletion::stage_attempt_id)
        .collect::<BTreeSet<_>>()
        .len()
        == self.stage_attempt_completions.len(),
    )?;
    let attempt_numbers = self
      .stage_attempts
      .values()
      .map(|stage| stage.number().get())
      .collect::<BTreeSet<_>>();
    require(
      attempt_numbers.len() == self.stage_attempts.len()
        && attempt_numbers
          .iter()
          .copied()
          .eq(1..=u64::try_from(attempt_numbers.len()).unwrap_or(u64::MAX)),
    )?;
    for call in self.macro_calls.values() {
      let stage = self.stage_attempts.get(&call.stage_attempt_id());
      require(
        call.run_id() == run_id
          && call.subject() == subject
          && stage.is_some_and(|stage| stage.id() == call.stage_attempt_id())
          && call.parent_id().is_none_or(|parent| {
            self
              .macro_calls
              .get(&parent)
              .is_some_and(|value| value.stage_attempt_id() == call.stage_attempt_id())
          }),
      )?;
    }
    for request in self.signal_requests.values() {
      require(
        request.run_id() == run_id
          && request.subject() == subject
          && self.stage_attempts.contains_key(&request.stage_attempt_id()),
      )?;
    }
    for receipt in self.signal_receipts.values() {
      require(
        receipt.run_id() == run_id
          && receipt.subject() == subject
          && self.signal_requests.contains_key(&receipt.request_id()),
      )?;
    }
    for link in self.linked_builds.values() {
      require(link.run_id == run_id && self.stage_attempts.contains_key(&link.stage_attempt_id))?;
    }
    for candidate in self.candidates.values() {
      require(
        candidate.subject().exact() == subject && self.stage_attempts.contains_key(&candidate.stage_attempt_id()),
      )?;
    }
    for evidence in self.evidence.values() {
      require(
        evidence.subject().exact() == subject
          && self
            .candidates
            .get(&evidence.changeset_id())
            .is_some_and(|candidate| candidate.subject() == evidence.subject()),
      )?;
    }
    for plan in self.evaluation_plans.values() {
      require(
        plan.subject().exact() == subject
          && self
            .evidence
            .get(&plan.evidence_id())
            .is_some_and(|evidence| evidence.subject() == plan.subject()),
      )?;
    }
    for assessment in self.assessments.values() {
      require(
        assessment.subject().exact() == subject
          && self
            .evaluation_plans
            .get(&assessment.plan_id())
            .is_some_and(|plan| plan.subject() == assessment.subject()),
      )?;
    }
    for decision in self.decisions.values() {
      require(
        decision.subject().exact() == subject
          && self
            .evaluation_plans
            .get(&decision.plan_id())
            .is_some_and(|plan| plan.subject() == decision.subject())
          && decision.assessment_ids().iter().all(|id| {
            self.assessments.get(id).is_some_and(|assessment| {
              assessment.plan_id() == decision.plan_id() && assessment.subject() == decision.subject()
            })
          }),
      )?;
    }
    for escalation in self.escalations.values() {
      require(
        escalation.run_id() == run_id
          && escalation.subject() == subject
          && escalation
            .decision_id()
            .is_none_or(|id| self.decisions.contains_key(&id)),
      )?;
    }
    for delivery in self.delivery_attempts.values() {
      require(delivery.subject().exact() == subject && self.decisions.contains_key(&delivery.decision_id()))?;
    }
    for reporting in self.reporting_attempts.values() {
      require(reporting.run_id() == run_id && reporting.subject() == subject)?;
    }
    require(self.audit.values().all(|fact| {
      fact.run_id == run_id
        && fact
          == &FactoryAuditFact::new(
            fact.run_id,
            fact.actor_kind,
            fact.actor_identity_digest,
            fact.operation.clone(),
            fact.request_identity_digest,
            fact.outcome.clone(),
            fact.recorded_at,
          )
    }))?;
    require(
      self
        .outbox
        .values()
        .all(|record| record.run_id == run_id && record.is_canonical()),
    )?;
    validate_outbox(self)?;
    validate_projection(self)
  }

  fn record_count(&self) -> usize {
    self.claims.len()
      + self.budgets.len()
      + self.lifecycle_checkpoints.len()
      + self.stage_attempts.len()
      + self.stage_attempt_completions.len()
      + self.macro_calls.len()
      + self.signal_requests.len()
      + self.signal_receipts.len()
      + self.linked_builds.len()
      + self.candidates.len()
      + self.evidence.len()
      + self.evaluation_plans.len()
      + self.assessments.len()
      + self.decisions.len()
      + self.escalations.len()
      + self.delivery_attempts.len()
      + self.reporting_attempts.len()
      + self.audit.len()
      + self.outbox.len()
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
      .filter(|(_, stored)| stored.run.state() != FactoryRunState::Completed)
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
    let stored = state
      .factory_runs
      .get_mut(&request.run_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?;
    validate_transition(stored, &request)?;
    let mut next = stored.clone();
    next.append_transition(&request)?;
    let outcome = CommitFactoryRunTransitionOutcome {
      version: next.run.version(),
      current: next.current.clone(),
    };
    *stored = next;
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
      .authorize(request.fence, request.observed_at)
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

fn claim_is_selectable(stored: &StoredFactoryRun, request: &ClaimFactoryRuns) -> bool {
  stored.current_claim_id.is_none_or(|id| {
    stored.claims.get(&id).is_none_or(|current| {
      current.claim.expires_at() <= request.observed_at
        || (current.owner == request.owner
          && current.claim.claimed_at() == request.observed_at
          && current.claim.expires_at() == request.claim_expires_at)
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
  let mut active_stages = 0_u32;
  for stored in runs
    .values()
    .filter(|stored| stored.run.configuration() == configuration && stored.run.state() != FactoryRunState::Completed)
  {
    active_runs = active_runs.saturating_add(1);
    if let Some(checkpoint) = stored
      .lifecycle_checkpoints
      .get(&stored.current.lifecycle_checkpoint_id)
    {
      active_stages = active_stages.saturating_add(checkpoint.progress.active_stage_count());
    }
  }
  FactoryWipUsage::new(active_runs, active_stages)
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
  let state = match record.state {
    crate::FactoryOutboxState::Pending => 0,
    crate::FactoryOutboxState::Claimed => 1,
    crate::FactoryOutboxState::Delivered | crate::FactoryOutboxState::Failed => 2,
  };
  (record.attempt, state, record.recorded_at, record.id)
}

fn validate_claim(stored: &StoredFactoryRun, request: &ClaimFactoryRun) -> Result<(), StoreError> {
  if request.expected_version != stored.run.version() {
    return Err(conflict());
  }
  if stored.current_claim_id.is_some_and(|id| {
    stored
      .claims
      .get(&id)
      .is_none_or(|current| current.claim.expires_at() > request.record.claim.claimed_at())
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

fn validate_transition(stored: &StoredFactoryRun, request: &CommitFactoryRunTransition) -> Result<(), StoreError> {
  if request.append.record_count() > MAX_FACTORY_TRANSITION_RECORDS
    || request.outbox.len() > MAX_FACTORY_TRANSITION_OUTBOX_RECORDS
  {
    return Err(invalid_transition(StoreOperation::CommitFactoryRunTransition));
  }
  let added = request
    .append
    .record_count()
    .checked_add(request.outbox.len())
    .and_then(|count| count.checked_add(3))
    .ok_or_else(|| invalid_transition(StoreOperation::CommitFactoryRunTransition))?;
  if stored
    .record_count()
    .checked_add(added)
    .is_none_or(|count| count > MAX_FACTORY_RUN_SNAPSHOT_RECORDS)
  {
    return Err(invalid_transition(StoreOperation::CommitFactoryRunTransition));
  }
  if request.expected_version != stored.run.version() {
    return Err(conflict());
  }
  let next_version = stored
    .run
    .version()
    .get()
    .checked_add(1)
    .and_then(|value| FactoryRunVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)?;
  let claim = stored
    .current_claim_id
    .filter(|id| *id == request.claim_id)
    .and_then(|id| stored.claims.get(&id))
    .ok_or_else(conflict)?;
  if claim.owner != request.owner {
    return Err(conflict());
  }
  claim
    .claim
    .authorize(request.fence, request.committed_at)
    .map_err(|_| conflict())?;
  let current_budget = stored
    .budgets
    .get(&stored.current.budget_id)
    .ok_or(StoreError::Unavailable)?;
  let current_checkpoint = stored
    .lifecycle_checkpoints
    .get(&stored.current.lifecycle_checkpoint_id)
    .ok_or(StoreError::Unavailable)?;
  let mut expected_attempt_number = u64::try_from(stored.stage_attempts.len())
    .ok()
    .and_then(|value| value.checked_add(1))
    .ok_or(StoreError::Unavailable)?;
  let appended_attempts_are_valid = request.append.stage_attempts.iter().all(|stage| {
    if let Some(existing) = stored.stage_attempts.get(&stage.id()) {
      return existing == stage;
    }
    let valid = stage.run_id() == request.run_id
      && stage.subject() == stored.run.subject()
      && stage.owner() == &request.owner
      && stage.claim() == claim.claim
      && stage.number().get() == expected_attempt_number;
    expected_attempt_number = expected_attempt_number.saturating_add(1);
    valid
  });
  let appended_completions_are_valid = request.append.stage_attempt_completions.iter().all(|completion| {
    let stage = stored.stage_attempts.get(&completion.stage_attempt_id()).or_else(|| {
      request
        .append
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == completion.stage_attempt_id())
    });
    completion.run_id() == request.run_id
      && completion.owner() == &request.owner
      && completion.claim() == claim.claim
      && !stored
        .stage_attempt_completions
        .values()
        .any(|current| current.stage_attempt_id() == completion.stage_attempt_id())
      && stage.is_some_and(|stage| completion.usage().validate(stage.budget()).is_ok())
  });
  require_with(
    request.next_run.id() == stored.run.id()
      && request.next_run.work_id() == stored.run.work_id()
      && request.next_run.configuration() == stored.run.configuration()
      && request.next_run.subject() == stored.run.subject()
      && request.next_run.version() == next_version
      && request.budget
        == FactoryBudgetRecord::new(request.run_id, next_version, request.budget.usage, request.committed_at)
      && budget_is_monotonic(current_budget.usage, request.budget.usage)
      && request.current.budget_id == request.budget.id
      && request.lifecycle_checkpoint
        == FactoryLifecycleCheckpoint::new(
          request.run_id,
          next_version,
          request.lifecycle_checkpoint.progress.clone(),
          request.lifecycle_checkpoint.signal,
          request.lifecycle_checkpoint.cancellation_requested,
          request.committed_at,
        )
      && octacity_server_factory::validate_lifecycle_progress(
        request.next_run.state(),
        &request.lifecycle_checkpoint.progress,
      )
      .is_ok()
      && octacity_server_factory::validate_lifecycle_transition(
        stored.run.state(),
        &current_checkpoint.progress,
        request.next_run.state(),
        &request.lifecycle_checkpoint.progress,
      )
      .is_ok()
      && appended_attempts_are_valid
      && appended_completions_are_valid
      && completion_budget_is_covered(
        current_budget.usage,
        request.budget.usage,
        &request.append.stage_attempt_completions,
      )
      && request.current.lifecycle_checkpoint_id == request.lifecycle_checkpoint.id
      && request.audit.run_id == request.run_id
      && request.audit.recorded_at == request.committed_at
      && request.audit
        == FactoryAuditFact::new(
          request.audit.run_id,
          request.audit.actor_kind,
          request.audit.actor_identity_digest,
          request.audit.operation.clone(),
          request.audit.request_identity_digest,
          request.audit.outcome.clone(),
          request.audit.recorded_at,
        )
      && request.outbox.iter().all(|record| {
        record.run_id == request.run_id
          && record.recorded_at == request.committed_at
          && record.available_at >= request.committed_at
          && record.state == crate::FactoryOutboxState::Pending
          && record.attempt == 0
          && record.is_canonical()
      }),
    StoreOperation::CommitFactoryRunTransition,
    StoreInputError::InvalidFactoryRunTransition,
  )
}

fn completion_budget_is_covered(
  current: octacity_server_factory::BudgetUsage,
  next: octacity_server_factory::BudgetUsage,
  completions: &[StageAttemptCompletion],
) -> bool {
  let Some(elapsed_millis) = completions
    .iter()
    .try_fold(current.elapsed_millis, |total, completion| {
      total.checked_add(completion.usage().elapsed_millis)
    })
  else {
    return false;
  };
  let Some(tokens) = completions.iter().try_fold(current.tokens, |total, completion| {
    total.checked_add(completion.usage().tokens)
  }) else {
    return false;
  };
  let Some(cost_micro_units) = completions
    .iter()
    .try_fold(current.cost_micro_units, |total, completion| {
      total.checked_add(completion.usage().cost_micro_units)
    })
  else {
    return false;
  };
  let Some(output_bytes) = completions.iter().try_fold(current.output_bytes, |total, completion| {
    total.checked_add(completion.usage().output_bytes)
  }) else {
    return false;
  };
  next.elapsed_millis >= elapsed_millis
    && next.tokens >= tokens
    && next.cost_micro_units >= cost_micro_units
    && next.output_bytes >= output_bytes
}

fn append_history(stored: &mut StoredFactoryRun, append: &FactoryRunHistoryAppend) -> Result<(), StoreError> {
  for record in &append.stage_attempts {
    append_unique(&mut stored.stage_attempts, record.id(), record.clone())?;
  }
  for record in &append.stage_attempt_completions {
    append_unique(&mut stored.stage_attempt_completions, record.id(), record.clone())?;
  }
  for record in &append.macro_calls {
    append_unique(&mut stored.macro_calls, record.id(), record.clone())?;
  }
  for record in &append.signal_requests {
    append_unique(&mut stored.signal_requests, record.id(), record.clone())?;
  }
  for record in &append.signal_receipts {
    append_unique(&mut stored.signal_receipts, record.id(), record.clone())?;
  }
  for record in &append.linked_builds {
    append_unique(&mut stored.linked_builds, record.build_id, record.clone())?;
  }
  for record in &append.candidates {
    append_unique(&mut stored.candidates, record.id(), record.clone())?;
  }
  for record in &append.evidence {
    append_unique(&mut stored.evidence, record.id(), record.clone())?;
  }
  for record in &append.evaluation_plans {
    append_unique(&mut stored.evaluation_plans, record.id(), record.clone())?;
  }
  for record in &append.assessments {
    append_unique(&mut stored.assessments, record.id(), record.clone())?;
  }
  for record in &append.decisions {
    append_unique(&mut stored.decisions, record.id(), record.clone())?;
  }
  for record in &append.escalations {
    append_unique(&mut stored.escalations, record.id(), record.clone())?;
  }
  for record in &append.delivery_attempts {
    append_unique(&mut stored.delivery_attempts, record.id(), record.clone())?;
  }
  for record in &append.reporting_attempts {
    append_unique(&mut stored.reporting_attempts, record.id(), record.clone())?;
  }
  Ok(())
}

fn validate_projection(stored: &StoredFactoryRun) -> Result<(), StoreError> {
  let current = &stored.current;
  require(
    current
      .stage_attempt_id
      .is_none_or(|id| stored.stage_attempts.contains_key(&id))
      && current
        .macro_call_id
        .is_none_or(|id| stored.macro_calls.contains_key(&id))
      && current
        .signal_request_id
        .is_none_or(|id| stored.signal_requests.contains_key(&id))
      && current
        .signal_receipt_id
        .is_none_or(|id| stored.signal_receipts.contains_key(&id))
      && current.build_id.is_none_or(|id| stored.linked_builds.contains_key(&id))
      && current
        .candidate_id
        .is_none_or(|id| stored.candidates.contains_key(&id))
      && current.evidence_id.is_none_or(|id| stored.evidence.contains_key(&id))
      && current
        .evaluation_plan_id
        .is_none_or(|id| stored.evaluation_plans.contains_key(&id))
      && current.decision_id.is_none_or(|id| stored.decisions.contains_key(&id))
      && current
        .escalation_id
        .is_none_or(|id| stored.escalations.contains_key(&id))
      && current
        .delivery_attempt_id
        .is_none_or(|id| stored.delivery_attempts.contains_key(&id))
      && current
        .reporting_attempt_id
        .is_none_or(|id| stored.reporting_attempts.contains_key(&id)),
  )
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
        previous.run_id == next.run_id && previous.kind == next.kind && previous.input_digest == next.input_digest;
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

const fn budget_is_monotonic(
  current: octacity_server_factory::BudgetUsage,
  next: octacity_server_factory::BudgetUsage,
) -> bool {
  next.attempts >= current.attempts
    && next.elapsed_millis >= current.elapsed_millis
    && next.tokens >= current.tokens
    && next.cost_micro_units >= current.cost_micro_units
    && next.output_bytes >= current.output_bytes
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

  use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};
  use octacity_server_factory::{
    AssessmentOutcome, BudgetLimit, BudgetUsage, CandidateSubject, DecisionEngineInput, DecisionOutcome,
    DecisionPolicy, DecisionPolicyDefinition, DecisionPolicyVersion, DeliveryAttemptNumber, DeliveryState,
    DeterministicGate, DeterministicGateOutcome, EvidenceItem, ExternalWorkIdentity, FactoryClaim, FactoryClaimFence,
    FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryMetadata, FactoryRunState,
    FactoryStageKind, FindingSeverity, IndeterminatePolicy, MacroCallKind, ReportingAttemptNumber, ReportingState,
    RiskClass, StageAttemptNumber, WorkArtifacts, WorkClassification, WorkEnvelopeId, WorkPriority, evaluate_decision,
  };

  use crate::test_support::{id, run_ready, time};
  use crate::{AuditActor, AuditActorKind, FactoryBuildLink, FactoryOutboxState, ManagementSecurityScope, StoreError};

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

  fn fixture() -> Fixture {
    let project_id = id::<ProjectId>(1);
    let subject = octacity_server_factory::ExactSubject::new(
      project_id,
      id::<RepositoryId>(2),
      ImmutableRevision::new("base-revision").unwrap(),
    );
    let configuration = FactoryConfigurationRef::new(
      id::<FactoryConfigurationId>(3),
      FactoryConfigurationVersion::INITIAL,
      project_id,
      digest(1),
    );
    let work = WorkEnvelope::new(
      id::<WorkEnvelopeId>(4),
      configuration,
      ExternalWorkIdentity::new("manual/work-1").unwrap(),
      subject,
      WorkArtifacts::new(id::<ArtifactId>(5), id::<ArtifactId>(6), Vec::new()).unwrap(),
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

  fn full_history(run: &FactoryRun) -> FullHistory {
    let stage = StageAttempt::new(
      id::<StageAttemptId>(20),
      run,
      StageAttemptNumber::INITIAL,
      FactoryStageKind::Implementation,
      budget(),
      digest(20),
      octacity_server_factory::FactoryClaimOwnership::new(
        key("worker.one"),
        FactoryClaim::new(FactoryClaimFence::new(digest(10)), time(10), time(100)).unwrap(),
      ),
    );
    let call = MacroCall::new(
      id::<MacroCallId>(21),
      &stage,
      MacroCallKind::Implement,
      digest(21),
      budget(),
      None,
    )
    .unwrap();
    let build = FactoryBuildLink::new(&stage, id::<BuildId>(22), digest(22));
    let candidate_subject = CandidateSubject::new(
      run.subject().clone(),
      ImmutableRevision::new("candidate-revision").unwrap(),
      digest(23),
    );
    let candidate = ChangeSet::new(
      id::<ChangeSetId>(23),
      &stage,
      candidate_subject.clone(),
      id::<ArtifactId>(24),
      id::<ArtifactId>(25),
    )
    .unwrap();
    let evidence = EvidenceManifest::new(
      id::<EvidenceManifestId>(26),
      &candidate,
      candidate_subject.clone(),
      vec![EvidenceItem::new(key("tests"), id::<ArtifactId>(27), digest(27))],
    )
    .unwrap();
    let plan = EvaluationPlan::new(
      id::<EvaluationPlanId>(28),
      &evidence,
      candidate_subject,
      vec![key("quality")],
      vec![key("reviewer")],
    )
    .unwrap();
    let assessment = Assessment::new(
      id::<AssessmentId>(29),
      &plan,
      plan.subject().clone(),
      key("reviewer"),
      AssessmentOutcome::Satisfied,
      Vec::new(),
    )
    .unwrap();
    let policy = DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![key("tests")],
        required_evaluators: vec![key("reviewer")],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: DecisionOutcome::Escalate,
      },
    )
    .unwrap();
    let gates = [DeterministicGate::new(
      key("tests"),
      digest(27),
      DeterministicGateOutcome::Passed,
    )];
    let assessments = [assessment.clone()];
    let decision = evaluate_decision(
      id::<DecisionId>(30),
      DecisionEngineInput::new(&evidence, &plan, &policy, &gates, &assessments),
    )
    .unwrap();
    let escalation = Escalation::new(
      id::<EscalationId>(31),
      run,
      None,
      octacity_server_factory::FactoryText::new("operator review").unwrap(),
    )
    .unwrap();
    let delivery = DeliveryAttempt::new(
      id::<DeliveryAttemptId>(32),
      &decision,
      DeliveryAttemptNumber::INITIAL,
      DeliveryState::Succeeded,
      key("github"),
    )
    .unwrap();
    let reporting = ReportingAttempt::new(
      id::<ReportingAttemptId>(33),
      run,
      ReportingAttemptNumber::INITIAL,
      ReportingState::Succeeded,
      key("manual"),
    );
    let current = FactoryRunCurrentProjection {
      budget_id: digest(0),
      lifecycle_checkpoint_id: digest(0),
      stage_attempt_id: Some(stage.id()),
      macro_call_id: Some(call.id()),
      signal_request_id: None,
      signal_receipt_id: None,
      build_id: Some(build.build_id),
      candidate_id: Some(candidate.id()),
      evidence_id: Some(evidence.id()),
      evaluation_plan_id: Some(plan.id()),
      decision_id: Some(decision.id()),
      escalation_id: Some(escalation.id()),
      delivery_attempt_id: Some(delivery.id()),
      reporting_attempt_id: Some(reporting.id()),
    };
    FullHistory {
      append: FactoryRunHistoryAppend {
        stage_attempts: vec![stage],
        macro_calls: vec![call],
        linked_builds: vec![build],
        candidates: vec![candidate],
        evidence: vec![evidence],
        evaluation_plans: vec![plan],
        assessments: vec![assessment],
        decisions: vec![decision],
        escalations: vec![escalation],
        delivery_attempts: vec![delivery],
        reporting_attempts: vec![reporting],
        ..FactoryRunHistoryAppend::default()
      },
      current,
    }
  }

  fn transition(fixture: &Fixture, claim: &ClaimFactoryRun, history: FullHistory) -> CommitFactoryRunTransition {
    let version = FactoryRunVersion::new(2).unwrap();
    let budget = FactoryBudgetRecord::new(
      fixture.run.id(),
      version,
      BudgetUsage {
        attempts: 1,
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
      FactoryLifecycleProgress::Stage {
        target: octacity_server_factory::FactoryStageTarget::Implementation,
        progress: octacity_server_factory::FactoryStageProgress::AttemptCreated,
      },
      octacity_server_factory::DecisionSignalProgress::Disabled,
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
        FactoryRunState::Implementing,
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

  #[test]
  fn admitted_snapshot_and_claim_history_are_immutable_and_fenced() {
    run_ready(
      async {
        let fixture = fixture();
        let initial = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert_eq!(initial.run, fixture.run);
        assert_eq!(initial.budgets.len(), 1);
        assert_eq!(initial.lifecycle_checkpoints.len(), 1);
        assert_eq!(initial.audit.len(), 1);
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
        assert_eq!(snapshot.claims.len(), 2);
        assert!(snapshot.claims.contains(&request.record));
        assert!(snapshot.claims.contains(&takeover));
        assert_eq!(snapshot.current_claim, Some(takeover));
      },
      "Factory claim contract is ready",
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
        let stale_completion = transition(&fixture, &stale_claim, full_history(&fixture.run));

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
        assert!(first.claims.is_empty());
      },
      "Factory batch claim remains atomic",
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
        let request = transition(&fixture, &claim, full_history(&fixture.run));
        let expected_current = request.current.clone();

        let outcome = fixture.store.commit_factory_run_transition(request).await.unwrap();
        assert_eq!(outcome.version, FactoryRunVersion::new(2).unwrap());
        assert_eq!(outcome.current, expected_current);
        let snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert_eq!(snapshot.run.version(), FactoryRunVersion::new(2).unwrap());
        assert_eq!(snapshot.budgets.len(), 2);
        assert_eq!(snapshot.lifecycle_checkpoints.len(), 2);
        assert_eq!(snapshot.stage_attempts.len(), 1);
        assert_eq!(snapshot.macro_calls.len(), 1);
        assert_eq!(snapshot.linked_builds.len(), 1);
        assert_eq!(snapshot.candidates.len(), 1);
        assert_eq!(snapshot.evidence.len(), 1);
        assert_eq!(snapshot.evaluation_plans.len(), 1);
        assert_eq!(snapshot.assessments.len(), 1);
        assert_eq!(snapshot.decisions.len(), 1);
        assert_eq!(snapshot.escalations.len(), 1);
        assert_eq!(snapshot.delivery_attempts.len(), 1);
        assert_eq!(snapshot.reporting_attempts.len(), 1);
        assert_eq!(snapshot.audit.len(), 3);
        assert_eq!(snapshot.outbox.len(), 2);
        assert_eq!(snapshot.current, expected_current);
      },
      "Factory transition contract is ready",
    );
  }

  #[test]
  fn rejected_rewrites_missing_projections_and_stale_fences_are_atomic() {
    run_ready(
      async {
        let fixture = fixture();
        let claim = claim(&fixture);
        fixture.store.claim_factory_run(claim.clone()).await.unwrap();
        let first = transition(&fixture, &claim, full_history(&fixture.run));
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
          FactoryLifecycleProgress::Completed,
          octacity_server_factory::DecisionSignalProgress::Disabled,
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
            FactoryLifecycleProgress::Completed,
            octacity_server_factory::DecisionSignalProgress::Disabled,
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
        missing_projection.current.candidate_id = Some(id::<ChangeSetId>(999));
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

        missing_projection.current.candidate_id = committed.current.candidate_id;
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
            FactoryLifecycleProgress::Completed,
            octacity_server_factory::DecisionSignalProgress::Disabled,
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
}
