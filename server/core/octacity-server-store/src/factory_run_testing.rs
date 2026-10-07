use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use octacity_server_domain::{BuildId, EntityKind};
use octacity_server_factory::{
  Assessment, AssessmentId, ChangeSet, ChangeSetId, Decision, DecisionId, DecisionSignalReceipt,
  DecisionSignalReceiptId, DecisionSignalRequest, DecisionSignalRequestId, DeliveryAttempt, DeliveryAttemptId,
  DeliveryIntent, Escalation, EscalationId, EvaluationPlan, EvaluationPlanId, EvidenceManifest, EvidenceManifestId,
  FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryEscalationDisposition, FactoryKey, FactoryLifecycleProgress,
  FactoryRun, FactoryRunId, FactoryRunState, FactoryRunVersion, FactoryStageProgress, FactoryWipUsage, MacroCall,
  MacroCallId, ReportingAttempt, ReportingAttemptId, StageAttempt, StageAttemptCompletion, StageAttemptId,
  WorkEnvelope,
};

use crate::{
  ApplyFactoryRunControl, ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns,
  ClaimedFactoryOutbox, ClaimedFactoryRun, CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome,
  FactoryAuditFact, FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryOutboxRecord, FactoryOutboxSettlement,
  FactoryRunClaimRecord, FactoryRunControlIntent, FactoryRunControlOutcome, FactoryRunControlStore,
  FactoryRunCurrentProjection, FactoryRunDiagnosticKind, FactoryRunDiagnosticPage, FactoryRunDiagnosticRecord,
  FactoryRunHistoryAppend, FactoryRunSnapshot, FactoryRunStore, ListFactoryRunDiagnostics,
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
  build_observations: BTreeMap<FactoryDigest, crate::FactoryBuildObservationRecord>,
  candidates: BTreeMap<ChangeSetId, ChangeSet>,
  evidence: BTreeMap<EvidenceManifestId, EvidenceManifest>,
  evaluation_plans: BTreeMap<EvaluationPlanId, EvaluationPlan>,
  assessments: BTreeMap<AssessmentId, Assessment>,
  decisions: BTreeMap<DecisionId, Decision>,
  escalations: BTreeMap<EscalationId, Escalation>,
  delivery_attempts: BTreeMap<DeliveryAttemptId, DeliveryAttempt>,
  reporting_attempts: BTreeMap<ReportingAttemptId, ReportingAttempt>,
  audit: BTreeMap<FactoryDigest, FactoryAuditFact>,
  controls: BTreeMap<FactoryDigest, crate::FactoryRunControlRecord>,
  outbox: BTreeMap<FactoryDigest, FactoryOutboxRecord>,
  current: FactoryRunCurrentProjection,
}

impl StoredFactoryRun {
  pub(super) const fn run(&self) -> &FactoryRun {
    &self.run
  }

  pub(super) const fn work(&self) -> &WorkEnvelope {
    &self.work
  }

  pub(super) const fn admitted_at(&self) -> octacity_server_domain::Timestamp {
    self.admitted_at
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
      build_observations: BTreeMap::new(),
      candidates: BTreeMap::new(),
      evidence: BTreeMap::new(),
      evaluation_plans: BTreeMap::new(),
      assessments: BTreeMap::new(),
      decisions: BTreeMap::new(),
      escalations: BTreeMap::new(),
      delivery_attempts: BTreeMap::new(),
      reporting_attempts: BTreeMap::new(),
      audit: BTreeMap::from([(audit.id, audit)]),
      controls: BTreeMap::new(),
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
      build_observations: values(&self.build_observations),
      candidates: values(&self.candidates),
      evidence: values(&self.evidence),
      evaluation_plans: values(&self.evaluation_plans),
      assessments: values(&self.assessments),
      decisions: values(&self.decisions),
      escalations: values(&self.escalations),
      delivery_attempts: values(&self.delivery_attempts),
      reporting_attempts: values(&self.reporting_attempts),
      audit: values(&self.audit),
      controls: values(&self.controls),
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
            .verify_fence(completion.claim().fence(), completion.observed_at())
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
    let mut linked_stages = BTreeSet::new();
    for link in self.linked_builds.values() {
      let stage = self.stage_attempts.get(&link.stage_attempt_id);
      let unique_jobs = link.job_ids.iter().copied().collect::<BTreeSet<_>>();
      let parent_is_valid = match (&link.target, link.parent) {
        (octacity_server_factory::FactoryStageTarget::Implementation, None) => {
          link.exact_revision == *subject.base_revision()
        }
        (
          octacity_server_factory::FactoryStageTarget::Validation
          | octacity_server_factory::FactoryStageTarget::Evaluation(_),
          Some(crate::FactoryBuildParent::ChangeSet(id)),
        ) => self
          .candidates
          .get(&id)
          .is_some_and(|candidate| candidate.subject().candidate_revision() == &link.exact_revision),
        (octacity_server_factory::FactoryStageTarget::Rework, Some(crate::FactoryBuildParent::Decision(id))) => self
          .decisions
          .get(&id)
          .is_some_and(|decision| decision.subject().candidate_revision() == &link.exact_revision),
        _ => false,
      };
      require(
        link.run_id == run_id
          && link.factory_configuration == *self.run.configuration()
          && link.build_configuration.project_id() == subject.project_id()
          && !link.job_ids.is_empty()
          && link.job_ids.len() <= crate::MAX_MATERIALIZED_JOBS
          && unique_jobs.len() == link.job_ids.len()
          && linked_stages.insert(link.stage_attempt_id)
          && stage.is_some_and(|stage| {
            stage.run_id() == run_id
              && stage.kind() == link.stage_kind
              && stage.kind() == link.target.kind()
              && stage.number() == link.stage_attempt_number
              && stage.input_digest() == link.task_envelope_digest
          })
          && parent_is_valid,
      )?;
    }
    let mut observed_builds = BTreeSet::new();
    for observation in self.build_observations.values() {
      let link = self
        .linked_builds
        .get(&observation.build_id)
        .ok_or_else(|| invalid_transition(StoreOperation::ReadFactoryRunSnapshot))?;
      let canonical = crate::FactoryBuildObservationRecord::new(
        link,
        crate::FactoryBuildObservationInput {
          build_version: observation.build_version,
          attempt_id: observation.attempt_id,
          attempt_version: observation.attempt_version,
          job_ids: observation.job_ids.clone(),
          outputs: observation.outputs.clone(),
          state: observation.state,
          infrastructure_retry_eligible: observation.infrastructure_retry_eligible,
          observed_at: observation.observed_at,
        },
      )?;
      require(
        observation.run_id == run_id
          && observation.stage_attempt_id == link.stage_attempt_id
          && observation.target == link.target
          && observation.build_id == link.build_id
          && observed_builds.insert(observation.build_id)
          && observation.state.is_terminal()
          && observation == &canonical,
      )?;
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
      + self.build_observations.len()
      + self.candidates.len()
      + self.evidence.len()
      + self.evaluation_plans.len()
      + self.assessments.len()
      + self.decisions.len()
      + self.escalations.len()
      + self.delivery_attempts.len()
      + self.reporting_attempts.len()
      + self.audit.len()
      + self.controls.len()
      + self.outbox.len()
  }
}

fn diagnostic_records(snapshot: FactoryRunSnapshot, kind: FactoryRunDiagnosticKind) -> Vec<FactoryRunDiagnosticRecord> {
  match kind {
    FactoryRunDiagnosticKind::StageAttempt => snapshot
      .stage_attempts
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::StageAttempt(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::StageAttemptCompletion => snapshot
      .stage_attempt_completions
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::StageAttemptCompletion(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::MacroCall => snapshot
      .macro_calls
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::MacroCall(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::SignalRequest => snapshot
      .signal_requests
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::SignalRequest(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::SignalReceipt => snapshot
      .signal_receipts
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::SignalReceipt(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::BuildLink => snapshot
      .linked_builds
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::BuildLink(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::BuildObservation => snapshot
      .build_observations
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::BuildObservation(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::Candidate => snapshot
      .candidates
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::Candidate(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::Evidence => snapshot
      .evidence
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::Evidence(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::EvaluationPlan => snapshot
      .evaluation_plans
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::EvaluationPlan(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::Assessment => snapshot
      .assessments
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::Assessment(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::Decision => snapshot
      .decisions
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::Decision(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::Escalation => snapshot
      .escalations
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::Escalation(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::DeliveryAttempt => snapshot
      .delivery_attempts
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::DeliveryAttempt(Box::new(value)))
      .collect(),
    FactoryRunDiagnosticKind::ReportingAttempt => snapshot
      .reporting_attempts
      .into_iter()
      .map(|value| FactoryRunDiagnosticRecord::ReportingAttempt(Box::new(value)))
      .collect(),
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
    let snapshot = self
      .lock()?
      .factory_runs
      .get(&request.run_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::FactoryRun,
      })?
      .snapshot()?;
    let mut records = diagnostic_records(snapshot, request.kind);
    records.sort_unstable_by_key(FactoryRunDiagnosticRecord::cursor);
    if let Some(after) = &request.after {
      records.retain(|record| record.cursor() > *after);
    }
    let limit = usize::from(request.limit.get());
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
      if stored.request != request {
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
  let current_checkpoint = stored
    .lifecycle_checkpoints
    .get(&stored.current.lifecycle_checkpoint_id)
    .cloned()
    .ok_or(StoreError::Unavailable)?;
  let current_budget = stored
    .budgets
    .get(&stored.current.budget_id)
    .cloned()
    .ok_or(StoreError::Unavailable)?;
  if request.requested_at < current_checkpoint.recorded_at
    || stored
      .record_count()
      .checked_add(4)
      .is_none_or(|count| count > MAX_FACTORY_RUN_SNAPSHOT_RECORDS)
  {
    return Err(invalid_control());
  }
  let next_version = stored
    .run
    .version()
    .get()
    .checked_add(1)
    .and_then(|value| FactoryRunVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)?;
  let (next_state, next_progress, cancellation_requested, audit_outcome) = match &request.intent {
    FactoryRunControlIntent::Cancel => {
      if stored.run.state().is_terminal() || current_checkpoint.cancellation_requested {
        return Err(conflict());
      }
      (
        stored.run.state(),
        current_checkpoint.progress.clone(),
        true,
        key("cancel-requested"),
      )
    }
    FactoryRunControlIntent::RetryInfrastructure { stage_attempt_id } => {
      let stage = stored
        .stage_attempts
        .get(stage_attempt_id)
        .ok_or_else(invalid_control)?;
      if current_checkpoint.cancellation_requested || stored.current.stage_attempt_id != Some(*stage_attempt_id) {
        return Err(invalid_control());
      }
      let next_progress = request_retry(&current_checkpoint.progress, stage.target())?;
      (
        stored.run.state(),
        next_progress,
        current_checkpoint.cancellation_requested,
        key("retry-requested"),
      )
    }
    FactoryRunControlIntent::ResolveEscalation {
      escalation_id,
      disposition,
      ..
    } => {
      if stored.run.state() != FactoryRunState::Escalated
        || stored.current.escalation_id != Some(*escalation_id)
        || !stored.escalations.contains_key(escalation_id)
        || current_checkpoint.cancellation_requested
        || !escalation_accepts_disposition(&current_checkpoint.progress)
      {
        return Err(invalid_control());
      }
      match disposition {
        FactoryEscalationDisposition::Acknowledge => (
          FactoryRunState::Completed,
          FactoryLifecycleProgress::Completed,
          false,
          key("acknowledged"),
        ),
        FactoryEscalationDisposition::Cancel => (
          FactoryRunState::Escalated,
          current_checkpoint.progress.clone(),
          true,
          key("cancel-requested"),
        ),
      }
    }
    FactoryRunControlIntent::RequestDelivery {
      candidate_id,
      decision_id,
    } => {
      let candidate = stored.candidates.get(candidate_id).ok_or_else(invalid_control)?;
      let decision = stored.decisions.get(decision_id).ok_or_else(invalid_control)?;
      if stored.run.state() != FactoryRunState::ReadyForDelivery
        || current_checkpoint.cancellation_requested
        || current_checkpoint.progress != FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval)
        || stored.current.candidate_id != Some(*candidate_id)
        || stored.current.decision_id != Some(*decision_id)
        || decision.outcome() != octacity_server_factory::DecisionOutcome::Accept
        || decision.subject() != candidate.subject()
      {
        return Err(invalid_control());
      }
      (
        FactoryRunState::ReadyForDelivery,
        FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::Requested),
        false,
        key("delivery-requested"),
      )
    }
  };

  let next_run = FactoryRun::restore(
    stored.run.id(),
    stored.run.configuration().clone(),
    &stored.work,
    stored.run.subject().clone(),
    next_state,
    next_version,
  )
  .map_err(|_| invalid_control())?;
  let budget = FactoryBudgetRecord::new(
    stored.run.id(),
    next_version,
    current_budget.usage,
    request.requested_at,
  );
  let checkpoint = FactoryLifecycleCheckpoint::new(
    stored.run.id(),
    next_version,
    next_progress,
    current_checkpoint.signal,
    cancellation_requested,
    request.requested_at,
  );
  octacity_server_factory::validate_lifecycle_transition(
    stored.run.state(),
    &current_checkpoint.progress,
    next_state,
    &checkpoint.progress,
  )
  .map_err(|_| invalid_control())?;
  let control = crate::FactoryRunControlRecord::new(
    stored.run.id(),
    next_version,
    request.intent.clone(),
    request.requested_at,
  );
  let actor_identity_digest = context
    .actor()
    .identity
    .as_deref()
    .map(|identity| FactoryDigest::sha256("octacity.factory.audit-actor.v1", &[identity.as_bytes()]));
  let audit = FactoryAuditFact::new(
    stored.run.id(),
    context.actor().kind,
    actor_identity_digest,
    key(request.intent.operation()),
    FactoryDigest::sha256(
      "octacity.factory.audit-request.v1",
      &[context.request_identity().as_bytes()],
    ),
    audit_outcome,
    request.requested_at,
  );
  append_unique(&mut stored.budgets, budget.id, budget.clone())?;
  append_unique(&mut stored.lifecycle_checkpoints, checkpoint.id, checkpoint.clone())?;
  append_unique(&mut stored.controls, control.id, control)?;
  append_unique(&mut stored.audit, audit.id, audit)?;
  stored.run = next_run;
  stored.current_claim_id = None;
  stored.current.budget_id = budget.id;
  stored.current.lifecycle_checkpoint_id = checkpoint.id;
  stored.validate_integrity()?;
  Ok(FactoryRunControlOutcome {
    disposition: MutationDisposition::Applied,
    run_id: stored.run.id(),
    version: next_version,
    state: next_state,
    cancellation_requested,
  })
}

fn request_retry(
  progress: &FactoryLifecycleProgress,
  target: &octacity_server_factory::FactoryStageTarget,
) -> Result<FactoryLifecycleProgress, StoreError> {
  match progress {
    FactoryLifecycleProgress::Stage {
      target: current,
      progress: FactoryStageProgress::RetryableFailure,
    } if current == target => Ok(FactoryLifecycleProgress::Stage {
      target: current.clone(),
      progress: FactoryStageProgress::RetryRequested,
    }),
    FactoryLifecycleProgress::Evaluating(octacity_server_factory::EvaluationState::Branches(branches)) => {
      let octacity_server_factory::FactoryStageTarget::Evaluation(key) = target else {
        return Err(invalid_control());
      };
      Ok(FactoryLifecycleProgress::Evaluating(
        octacity_server_factory::EvaluationState::Branches(
          branches
            .advance_branch(key, octacity_server_factory::EvaluationBranchState::RetryRequested)
            .map_err(|_| invalid_control())?,
        ),
      ))
    }
    _ => Err(invalid_control()),
  }
}

fn escalation_accepts_disposition(progress: &FactoryLifecycleProgress) -> bool {
  matches!(
    progress,
    FactoryLifecycleProgress::Escalated(
      octacity_server_factory::ReportingProgress::Disabled
        | octacity_server_factory::ReportingProgress::Succeeded
        | octacity_server_factory::ReportingProgress::Exhausted
    )
  )
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
    .filter(|stored| stored.run.configuration() == configuration && stored.run.state().is_active())
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
      run: &stored.run,
      budget: current_budget,
      lifecycle: current_checkpoint,
      current: &stored.current,
      claim,
      stage_attempts: &stored.stage_attempts,
      stage_completions: &stored.stage_attempt_completions,
      record_count: stored.record_count(),
    },
    request,
  )
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
  for record in &append.build_observations {
    append_unique(&mut stored.build_observations, record.id, record.clone())?;
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
  let decision_matches_checkpoint = stored
    .lifecycle_checkpoints
    .get(&current.lifecycle_checkpoint_id)
    .is_some_and(|checkpoint| match &checkpoint.progress {
      octacity_server_factory::FactoryLifecycleProgress::Evaluating(
        octacity_server_factory::EvaluationState::DecisionRecorded { outcome, .. },
      ) => current.decision_id.is_some_and(|id| {
        stored.decisions.get(&id).is_some_and(|decision| {
          decision.outcome() == *outcome && Some(decision.plan_id()) == current.evaluation_plan_id
        })
      }),
      _ => true,
    });
  require(
    decision_matches_checkpoint
      && current
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

fn invalid_control() -> StoreError {
  StoreError::invalid(
    StoreOperation::ControlFactoryRun,
    StoreInputError::InvalidFactoryRunControl,
  )
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
    AssessmentOutcome, BudgetLimit, BudgetUsage, BuildConfigurationRef, CandidateSubject, DecisionEngineInput,
    DecisionOutcome, DecisionPolicy, DecisionPolicyDefinition, DecisionPolicyVersion, DeliveryAttemptNumber,
    DeliveryState, DeterministicGate, DeterministicGateOutcome, EvidenceItem, ExternalWorkIdentity, FactoryClaim,
    FactoryClaimFence, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryMetadata,
    FactoryRunState, FindingSeverity, IndeterminatePolicy, MacroCallKind, ReportingAttemptNumber, ReportingState,
    RiskClass, StageAttemptNumber, WorkArtifacts, WorkClassification, WorkEnvelopeId, WorkPriority, evaluate_decision,
  };

  use crate::test_support::{id, run_ready, time};
  use crate::{
    AuditActor, AuditActorKind, FactoryBuildLink, FactoryOutboxState, IdempotencyKey, ManagementSecurityScope,
    StoreError,
  };

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
      octacity_server_factory::FactoryStageTarget::Implementation,
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
    let build = FactoryBuildLink::new(
      &stage,
      crate::FactoryBuildLinkInput {
        build_id: id::<BuildId>(22),
        attempt_id: id::<octacity_server_domain::AttemptId>(22),
        job_ids: vec![id::<octacity_server_domain::JobId>(22)],
        factory_configuration: run.configuration().clone(),
        target: octacity_server_factory::FactoryStageTarget::Implementation,
        build_configuration: BuildConfigurationRef::new(
          id::<BuildConfigurationId>(22),
          BuildConfigurationVersion::INITIAL,
          run.subject().project_id(),
          digest(21),
        ),
        task_envelope_digest: digest(20),
        exact_revision: run.subject().base_revision().clone(),
        parent: None,
        effective_policy_digest: digest(22),
        input_digest: digest(23),
      },
    );
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

  async fn seed_full_history(fixture: &Fixture) {
    let claim = claim(fixture);
    fixture.store.claim_factory_run(claim.clone()).await.unwrap();
    fixture
      .store
      .commit_factory_run_transition(transition(fixture, &claim, full_history(&fixture.run)))
      .await
      .unwrap();
  }

  fn set_current_lifecycle(fixture: &Fixture, state: FactoryRunState, progress: FactoryLifecycleProgress) {
    let mut memory = fixture.store.lock().unwrap();
    let stored = memory.factory_runs.get_mut(&fixture.run.id()).unwrap();
    let version = FactoryRunVersion::new(stored.run.version().get() + 1).unwrap();
    let usage = stored.budgets.get(&stored.current.budget_id).unwrap().usage;
    let budget = FactoryBudgetRecord::new(stored.run.id(), version, usage, time(25));
    let checkpoint = FactoryLifecycleCheckpoint::new(
      stored.run.id(),
      version,
      progress,
      octacity_server_factory::DecisionSignalProgress::Disabled,
      false,
      time(25),
    );
    stored.run = FactoryRun::restore(
      stored.run.id(),
      stored.run.configuration().clone(),
      &stored.work,
      stored.run.subject().clone(),
      state,
      version,
    )
    .unwrap();
    stored.current_claim_id = None;
    stored.current.budget_id = budget.id;
    stored.current.lifecycle_checkpoint_id = checkpoint.id;
    stored.budgets.insert(budget.id, budget);
    stored.lifecycle_checkpoints.insert(checkpoint.id, checkpoint);
    stored.validate_integrity().unwrap();
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
  fn diagnostic_pages_are_bounded_stable_and_collection_typed() {
    run_ready(
      async {
        let fixture = fixture();
        seed_full_history(&fixture).await;
        let second = ReportingAttempt::new(
          id::<ReportingAttemptId>(34),
          &fixture.run,
          ReportingAttemptNumber::new(2).unwrap(),
          ReportingState::Succeeded,
          key("manual"),
        );
        fixture
          .store
          .lock()
          .unwrap()
          .factory_runs
          .get_mut(&fixture.run.id())
          .unwrap()
          .reporting_attempts
          .insert(second.id(), second.clone());

        let first = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(fixture.run.id(), FactoryRunDiagnosticKind::ReportingAttempt, None, 1)
              .unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(first.items.len(), 1);
        let cursor = first.next.expect("another reporting attempt exists");
        let final_page = fixture
          .store
          .list_factory_run_diagnostics(
            ListFactoryRunDiagnostics::new(
              fixture.run.id(),
              FactoryRunDiagnosticKind::ReportingAttempt,
              Some(cursor),
              1,
            )
            .unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(
          final_page.items,
          vec![FactoryRunDiagnosticRecord::ReportingAttempt(Box::new(second))]
        );
        assert!(final_page.next.is_none());
        assert!(
          ListFactoryRunDiagnostics::new(
            fixture.run.id(),
            FactoryRunDiagnosticKind::ReportingAttempt,
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
  fn build_links_reject_inconsistent_factory_causality_atomically() {
    run_ready(
      async {
        let fixture = fixture();
        let claim = claim(&fixture);
        fixture.store.claim_factory_run(claim.clone()).await.unwrap();
        let mut request = transition(&fixture, &claim, full_history(&fixture.run));
        request.append.linked_builds[0].task_envelope_digest = digest(99);
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

  #[test]
  fn cancellation_is_replayable_and_preserves_completed_factory_evidence() {
    run_ready(
      async {
        let fixture = fixture();
        seed_full_history(&fixture).await;
        let before = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let request = control_request(&fixture, FactoryRunControlIntent::Cancel, "cancel-1");

        let applied = fixture.store.apply_factory_run_control(request.clone()).await.unwrap();
        let replayed = fixture.store.apply_factory_run_control(request).await.unwrap();
        assert_eq!(applied.disposition, MutationDisposition::Applied);
        assert_eq!(replayed.disposition, MutationDisposition::Replayed);
        assert_eq!(applied.version, replayed.version);
        assert!(applied.cancellation_requested);

        let after = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        assert!(after.current_claim.is_none());
        assert_eq!(after.candidates, before.candidates);
        assert_eq!(after.evidence, before.evidence);
        assert_eq!(after.assessments, before.assessments);
        assert_eq!(after.decisions, before.decisions);
        assert_eq!(after.delivery_attempts, before.delivery_attempts);
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

  #[test]
  fn infrastructure_retry_requires_the_current_retryable_stage() {
    run_ready(
      async {
        let fixture = fixture();
        seed_full_history(&fixture).await;
        set_current_lifecycle(
          &fixture,
          FactoryRunState::Implementing,
          FactoryLifecycleProgress::Stage {
            target: octacity_server_factory::FactoryStageTarget::Implementation,
            progress: FactoryStageProgress::RetryableFailure,
          },
        );
        let stage_id = fixture
          .store
          .factory_run_snapshot(fixture.run.id())
          .await
          .unwrap()
          .current
          .stage_attempt_id
          .unwrap();
        let stale = control_request(
          &fixture,
          FactoryRunControlIntent::RetryInfrastructure {
            stage_attempt_id: id::<StageAttemptId>(999),
          },
          "retry-stale",
        );
        assert!(matches!(
          fixture.store.apply_factory_run_control(stale).await.unwrap_err(),
          StoreError::InvalidInput { .. }
        ));

        let accepted = control_request(
          &fixture,
          FactoryRunControlIntent::RetryInfrastructure {
            stage_attempt_id: stage_id,
          },
          "retry-current",
        );
        let outcome = fixture.store.apply_factory_run_control(accepted).await.unwrap();
        assert_eq!(outcome.state, FactoryRunState::Implementing);
        let snapshot = fixture.store.factory_run_snapshot(fixture.run.id()).await.unwrap();
        let checkpoint = snapshot
          .lifecycle_checkpoints
          .iter()
          .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
          .unwrap();
        assert!(matches!(
          checkpoint.progress,
          FactoryLifecycleProgress::Stage {
            progress: FactoryStageProgress::RetryRequested,
            ..
          }
        ));
        assert!(matches!(
          snapshot.controls.last().map(|record| &record.intent),
          Some(FactoryRunControlIntent::RetryInfrastructure { stage_attempt_id }) if *stage_attempt_id == stage_id
        ));
      },
      "Factory infrastructure retry control is ready",
    );
  }

  #[test]
  fn escalation_and_delivery_controls_accept_only_current_bounded_facts() {
    run_ready(
      async {
        let escalation_fixture = fixture();
        seed_full_history(&escalation_fixture).await;
        set_current_lifecycle(
          &escalation_fixture,
          FactoryRunState::Escalated,
          FactoryLifecycleProgress::Escalated(octacity_server_factory::ReportingProgress::Disabled),
        );
        let escalation_id = escalation_fixture
          .store
          .factory_run_snapshot(escalation_fixture.run.id())
          .await
          .unwrap()
          .current
          .escalation_id
          .unwrap();
        let resolution = control_request(
          &escalation_fixture,
          FactoryRunControlIntent::ResolveEscalation {
            escalation_id,
            disposition: FactoryEscalationDisposition::Acknowledge,
            reason: octacity_server_factory::FactoryText::new("reviewed by operator").unwrap(),
          },
          "resolve-1",
        );
        let resolved = escalation_fixture
          .store
          .apply_factory_run_control(resolution)
          .await
          .unwrap();
        assert_eq!(resolved.state, FactoryRunState::Completed);
        let snapshot = escalation_fixture
          .store
          .factory_run_snapshot(escalation_fixture.run.id())
          .await
          .unwrap();
        assert!(matches!(
          &snapshot.controls.last().unwrap().intent,
          FactoryRunControlIntent::ResolveEscalation { reason, .. } if reason.as_str() == "reviewed by operator"
        ));

        let delivery_fixture = fixture();
        seed_full_history(&delivery_fixture).await;
        set_current_lifecycle(
          &delivery_fixture,
          FactoryRunState::ReadyForDelivery,
          FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval),
        );
        let before = delivery_fixture
          .store
          .factory_run_snapshot(delivery_fixture.run.id())
          .await
          .unwrap();
        let stale = control_request(
          &delivery_fixture,
          FactoryRunControlIntent::RequestDelivery {
            candidate_id: id::<ChangeSetId>(999),
            decision_id: before.current.decision_id.unwrap(),
          },
          "delivery-stale",
        );
        assert!(matches!(
          delivery_fixture
            .store
            .apply_factory_run_control(stale)
            .await
            .unwrap_err(),
          StoreError::InvalidInput { .. }
        ));
        let request = control_request(
          &delivery_fixture,
          FactoryRunControlIntent::RequestDelivery {
            candidate_id: before.current.candidate_id.unwrap(),
            decision_id: before.current.decision_id.unwrap(),
          },
          "delivery-current",
        );
        delivery_fixture.store.apply_factory_run_control(request).await.unwrap();
        let after = delivery_fixture
          .store
          .factory_run_snapshot(delivery_fixture.run.id())
          .await
          .unwrap();
        assert_eq!(after.delivery_attempts, before.delivery_attempts);
        let checkpoint = after
          .lifecycle_checkpoints
          .iter()
          .find(|checkpoint| checkpoint.id == after.current.lifecycle_checkpoint_id)
          .unwrap();
        assert_eq!(
          checkpoint.progress,
          FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::Requested)
        );
      },
      "Factory escalation and delivery controls are ready",
    );
  }
}
