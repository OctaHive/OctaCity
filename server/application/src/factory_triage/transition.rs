use super::history::{current_usage, invalid, key};
use crate::ApplicationError;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
pub(crate) async fn commit_intake<S: FactoryRunStore>(
  store: &S,
  snapshot: &FactoryRunSnapshot,
  append: FactoryRunHistoryAppend,
  resolution: Option<&TriageDisposition>,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
  claim.claim.verify_fence(claim.claim.fence(), at).map_err(invalid)?;
  let version = FactoryRunVersion::new(
    snapshot
      .run
      .version()
      .get()
      .checked_add(1)
      .ok_or_else(ApplicationError::invalid)?,
  )
  .map_err(invalid)?;
  let mut usage = current_usage(snapshot)?;
  usage.attempts = usage
    .attempts
    .checked_add(u32::try_from(append.flow.attempts.len()).map_err(|_| ApplicationError::invalid())?)
    .ok_or_else(ApplicationError::invalid)?;
  for result in &append.flow.completions {
    // Attempts are charged when created, not again on completion.
    usage = usage
      .checked_add(BudgetUsage {
        attempts: 0,
        ..result.usage()
      })
      .ok_or_else(ApplicationError::invalid)?;
  }
  usage
    .validate(snapshot.admitted_flow.limits().budget())
    .map_err(invalid)?;
  let (state, progress) = match resolution {
    Some(TriageDisposition::Eligibility(row)) if row.outcome == EligibilityOutcome::Rejection => (
      FactoryRunState::Rejected,
      FactoryLifecycleProgress::Rejected(ReportingProgress::Disabled),
    ),
    Some(TriageDisposition::Classification(row)) if row.route == TriageRoute::Rejection => (
      FactoryRunState::Rejected,
      FactoryLifecycleProgress::Rejected(ReportingProgress::Disabled),
    ),
    Some(TriageDisposition::Classification(row)) if row.route == TriageRoute::AlreadyFixed => {
      (FactoryRunState::Completed, FactoryLifecycleProgress::Completed)
    }
    Some(_) => (
      FactoryRunState::Escalated,
      FactoryLifecycleProgress::Escalated(ReportingProgress::Disabled),
    ),
    None => (
      snapshot.run.state(),
      snapshot
        .lifecycle_checkpoints
        .iter()
        .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
        .ok_or_else(ApplicationError::invalid)?
        .progress
        .clone(),
    ),
  };
  let run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    state,
    version,
  )
  .map_err(invalid)?;
  let budget = FactoryBudgetRecord::new(run.id(), version, usage, at);
  let checkpoint =
    FactoryLifecycleCheckpoint::new(run.id(), version, progress, DecisionSignalProgress::Disabled, false, at);
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = checkpoint.id;
  let material = serde_json::to_vec(&append.flow.triage).map_err(|_| ApplicationError::invalid())?;
  let input = FactoryDigest::sha256(
    "octacity.factory.intake-transition.v1",
    &[&material, &snapshot.run.version().get().to_be_bytes()],
  );
  let outbox = append
    .flow
    .runs
    .iter()
    .filter(|run| {
      run.definition()
        != snapshot
          .admitted_flow
          .triage()
          .map_or(snapshot.admitted_flow.closure().root(), |triage| triage.definition)
    })
    .map(|phase| {
      let parent = phase.parent().expect("new intake phase has a parent");
      let attempt = append
        .flow
        .attempts
        .iter()
        .find(|attempt| attempt.id() == parent.node_attempt_id())
        .expect("phase call is in the same append");
      FactoryOutboxRecord::pending(
        FactoryDigest::sha256("octacity.factory.intake-phase.v1", &[phase.id().as_uuid().as_bytes()]),
        run.id(),
        key("factory.triage.execute"),
        attempt.input_digest(),
        at,
        at,
      )
    })
    .collect();
  let audit = FactoryAuditFact::new(
    run.id(),
    AuditActorKind::Worker,
    None,
    key("factory.triage.advance"),
    input,
    key(state.as_str()),
    at,
  );
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at: at,
      next_run: run,
      budget,
      lifecycle_checkpoint: checkpoint,
      append,
      audit,
      outbox,
      current,
    })
    .await?;
  Ok(())
}
