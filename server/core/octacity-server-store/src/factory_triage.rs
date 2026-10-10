use crate::{StoreError, StoreInputError, StoreOperation};
use octacity_server_factory::{
  AdmittedFlow, EligibilityOutcome, FactoryRunState, FlowRuntimeHistory, TriageJournalRecord, TriageRoute, WorkEnvelope,
};

/// Revalidates append-only intake order, exact Work, decisions, and producing Node Attempts.
pub fn validate_factory_triage_history(
  work: &WorkEnvelope,
  admitted: &AdmittedFlow,
  records: &[TriageJournalRecord],
  history: FlowRuntimeHistory<'_>,
) -> Result<(), StoreError> {
  let attempts = history.attempts;
  let completions = history.completions;
  let flows = history.flow_runs;
  let invalid = || {
    StoreError::invalid(
      StoreOperation::CommitFactoryRunTransition,
      StoreInputError::InvalidFactoryRunTransition,
    )
  };
  if records.len() > 3 {
    return Err(invalid());
  }
  for (index, record) in records.iter().enumerate() {
    if usize::from(record.phase()) != index
      || record.validate(work, admitted).is_err()
      || (index > 0 && record.input() != records[0].input())
    {
      return Err(invalid());
    }
    let observations = match record {
      TriageJournalRecord::Prepared(_) => Vec::new(),
      TriageJournalRecord::Eligibility(row) => {
        vec![(row.result.provenance(), row.result.digest().map_err(|_| invalid())?)]
      }
      TriageJournalRecord::Classification(row) => {
        if records.get(1) != Some(&TriageJournalRecord::Eligibility(Box::new(row.eligibility.clone()))) {
          return Err(invalid());
        }
        vec![(row.result.provenance(), row.result.digest().map_err(|_| invalid())?)]
      }
    };
    for (provenance, digest) in observations {
      let role = if index == 1 {
        octacity_server_factory::TriageNode::Eligibility
      } else {
        octacity_server_factory::TriageNode::Classification
      };
      let settings = admitted.triage().ok_or_else(invalid)?;
      let phase = flows
        .iter()
        .find(|run| {
          run.parent().is_some_and(|parent| {
            attempts
              .iter()
              .find(|attempt| attempt.id() == parent.node_attempt_id())
              .is_some_and(|call| {
                call.node_key() == &role.key()
                  && flows.iter().any(|composition| {
                    composition.id() == parent.flow_run_id() && composition.definition() == settings.definition
                  })
              })
          })
        })
        .ok_or_else(invalid)?;
      octacity_server_factory::validate_triage_phase_observation(admitted, phase, provenance, digest, history)
        .map_err(|_| invalid())?;
    }
  }
  if let Some(settings) = admitted.triage() {
    let composition = admitted.closure().definition(settings.definition).ok_or_else(invalid)?;
    let triage_runs = flows
      .iter()
      .filter(|run| run.definition() == settings.definition)
      .collect::<Vec<_>>();
    if !records.is_empty() && triage_runs.len() != 1 {
      return Err(invalid());
    }
    for run in triage_runs {
      let parent = run.parent().ok_or_else(invalid)?;
      let root_attempt = attempts
        .iter()
        .find(|row| row.id() == parent.node_attempt_id())
        .ok_or_else(invalid)?;
      if root_attempt.input_digest()
        != records
          .first()
          .ok_or_else(invalid)?
          .input()
          .digest()
          .map_err(|_| invalid())?
      {
        return Err(invalid());
      }
      for (node, phase) in [
        (octacity_server_factory::TriageNode::EligibilityPolicy, 1),
        (octacity_server_factory::TriageNode::RoutingPolicy, 2),
      ] {
        if records.get(phase).is_some()
          && !completions
            .iter()
            .any(|row| row.flow_run_id() == run.id() && row.node_key().as_str() == node.as_str())
        {
          return Err(invalid());
        }
        for result in completions
          .iter()
          .filter(|row| row.flow_run_id() == run.id() && row.node_key().as_str() == node.as_str())
        {
          let record = records.get(phase).ok_or_else(invalid)?;
          let (outcome, output) = match record {
            TriageJournalRecord::Eligibility(row) => {
              if row.decision.outcome == EligibilityOutcome::Classify {
                let policy = octacity_server_factory::TriagePolicy::for_flow(
                  work.configuration().clone(),
                  &admitted.validated().map_err(|_| invalid())?,
                  settings.definition,
                  settings.policy.clone(),
                )
                .map_err(|_| invalid())?;
                let input = policy
                  .classification_input(row.input.clone(), row.result.clone(), &row.evidence, row.usage)
                  .map_err(|_| invalid())?;
                (row.decision.outcome.as_str(), input.digest().map_err(|_| invalid())?)
              } else {
                (
                  row.decision.outcome.as_str(),
                  octacity_server_factory::TriageDisposition::Eligibility(row.decision.clone())
                    .digest()
                    .map_err(|_| invalid())?,
                )
              }
            }
            TriageJournalRecord::Classification(row) => (
              row.decision.route.as_str(),
              octacity_server_factory::TriageDisposition::Classification(row.decision.clone())
                .digest()
                .map_err(|_| invalid())?,
            ),
            _ => return Err(invalid()),
          };
          if result.outcome().as_str() != outcome
            || result.output_digest() != output
            || composition.node(result.node_key()).is_none()
          {
            return Err(invalid());
          }
        }
      }
      let root_completion = completions
        .iter()
        .find(|row| row.node_attempt_id() == root_attempt.id());
      let resolved = matches!(records.last(), Some(TriageJournalRecord::Classification(_)))
        || matches!(records.last(), Some(TriageJournalRecord::Eligibility(row)) if row.decision.outcome != EligibilityOutcome::Classify);
      if resolved && root_completion.is_none() {
        return Err(invalid());
      }
      if let Some(result) = root_completion {
        let disposition = match records.last() {
          Some(TriageJournalRecord::Eligibility(row)) if row.decision.outcome != EligibilityOutcome::Classify => {
            octacity_server_factory::TriageDisposition::Eligibility(row.decision.clone())
          }
          Some(TriageJournalRecord::Classification(row)) => {
            octacity_server_factory::TriageDisposition::Classification(row.decision.clone())
          }
          _ => return Err(invalid()),
        };
        let outcome = match &disposition {
          octacity_server_factory::TriageDisposition::Eligibility(row) => row.outcome.as_str(),
          octacity_server_factory::TriageDisposition::Classification(row) => row.route.as_str(),
        };
        if result.outcome().as_str() != outcome
          || result.output_digest() != disposition.digest().map_err(|_| invalid())?
        {
          return Err(invalid());
        }
      }
    }
  }
  Ok(())
}

pub(crate) fn triage_terminal_transition(
  admitted: &AdmittedFlow,
  records: &[TriageJournalRecord],
  previous: FactoryRunState,
  next: FactoryRunState,
) -> bool {
  if admitted.triage().is_none() || previous != FactoryRunState::Admitted {
    return false;
  }
  match records.last() {
    Some(TriageJournalRecord::Eligibility(row)) => match row.decision.outcome {
      EligibilityOutcome::Rejection => next == FactoryRunState::Rejected,
      EligibilityOutcome::Escalation => next == FactoryRunState::Escalated,
      _ => false,
    },
    Some(TriageJournalRecord::Classification(row)) => match row.decision.route {
      TriageRoute::Rejection => next == FactoryRunState::Rejected,
      TriageRoute::Escalation => next == FactoryRunState::Escalated,
      TriageRoute::AlreadyFixed => next == FactoryRunState::Completed,
      _ => false,
    },
    _ => false,
  }
}
