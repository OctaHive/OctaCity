//! Trusted accepted producer data for the shared Issue transition; no wire command or Run owner.
use crate::*;
use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryRunId, FlowRunId, NodeAttemptId, WorkEnvelopeId, WorkflowCycleId,
};
use serde::{Deserialize, Serialize};

/// Safe immutable origin of one accepted, explicitly configured pipeline action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePipelineActionProvenance {
  /// Exact Issue selected by the retained Work association.
  pub issue_id: octacity_server_domain::IssueId,
  /// Original reviewed requirements, independent of later status-only revisions.
  pub requirements_version: octacity_server_domain::IssueVersion,
  /// Original immutable Work.
  pub work_id: WorkEnvelopeId,
  /// Original Run, never the Issue's latest execution.
  pub run_id: FactoryRunId,
  /// Exact nested Flow Run.
  pub flow_run_id: FlowRunId,
  /// Exact producing cycle.
  pub cycle_id: WorkflowCycleId,
  /// Exact accepted attempt.
  pub node_attempt_id: NodeAttemptId,
  /// Stable accepted action identity, reused on recovery.
  pub operation_id: FactoryDigest,
  /// Exact execution candidate selected by accepted input, absent for actions without evidence.
  pub candidate: Option<octacity_server_factory::CandidateSubject>,
  /// Explicitly selected retained evidence records; populated only by qualification.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub evidence: Vec<FactoryDigest>,
  /// Explicit declared outcome; its spelling has no business meaning.
  pub outcome: FactoryKey,
}
/// Operator-selected business effect for one declared outcome, independent of provider vocabulary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePipelineStatusSelection {
  /// Exact Project business workflow.
  pub workflow_id: octacity_server_domain::IssueWorkflowId,
  /// Immutable workflow publication.
  pub workflow_version: octacity_server_domain::IssueWorkflowVersion,
  /// One permitted direct edge.
  pub transition_id: octacity_server_domain::IssueTransitionId,
  /// Declared accepted outcome, not arbitrary prose or a phase label.
  pub outcome: FactoryKey,
}
/// Accepted internal producer input; intentionally not deserializable as a management body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedIssueStatusAction {
  provenance: IssuePipelineActionProvenance,
  selection: IssuePipelineStatusSelection,
  evidence: Option<IssuePipelineEvidence>,
}
impl AcceptedIssueStatusAction {
  /// Receives immutable facts from an already accepted trusted action, never model payloads.
  pub fn for_selection(
    selection: IssuePipelineStatusSelection,
    provenance: IssuePipelineActionProvenance,
  ) -> Result<Self, StoreError> {
    if !provenance.evidence.is_empty() || selection.outcome != provenance.outcome {
      return Err(StoreError::invalid(
        StoreOperation::TransitionIssue,
        StoreInputError::InvalidIssue,
      ));
    }
    Ok(Self {
      provenance,
      selection,
      evidence: None,
    })
  }
  /// Borrows the exact immutable operator selection.
  pub fn selection(&self) -> &IssuePipelineStatusSelection {
    &self.selection
  }
  pub(crate) fn matches(&self, intent: &TransitionIssueIntent) -> bool {
    self.selection.workflow_id == intent.workflow_id
      && self.selection.workflow_version == intent.workflow_version
      && self.selection.transition_id == intent.transition_id
  }
  /// Borrows metadata suitable for exact history and replay comparison.
  pub fn provenance(&self) -> &IssuePipelineActionProvenance {
    &self.provenance
  }
}
impl TransitionIssue {
  /// Constructs an ordinary manual command, with no caller-selectable pipeline authority.
  pub fn manual(
    intent: TransitionIssueIntent,
    idempotency_key: IdempotencyKey,
    moved_at: octacity_server_domain::Timestamp,
  ) -> Self {
    Self {
      intent,
      idempotency_key,
      moved_at,
      pipeline: None,
    }
  }
  /// Reuses the one transition boundary with a stable identity from accepted pipeline provenance.
  pub fn pipeline(
    intent: TransitionIssueIntent,
    accepted: AcceptedIssueStatusAction,
    moved_at: octacity_server_domain::Timestamp,
  ) -> Self {
    let idempotency_key = issue_status_operation_key(accepted.provenance.operation_id);
    Self {
      intent,
      idempotency_key,
      moved_at,
      pipeline: Some(accepted),
    }
  }
  /// Trusted pipeline origin; manual REST callers cannot supply this field.
  pub fn pipeline_action(&self) -> Option<&AcceptedIssueStatusAction> {
    self.pipeline.as_ref()
  }
  /// Stores original accepted metadata without serializing evidence authority flags.
  pub fn retained_pipeline_command(&self) -> Option<RetainedIssueStatusCommand> {
    self.pipeline_action().map(|action| RetainedIssueStatusCommand {
      intent: self.intent.clone(),
      selection: action.selection.clone(),
      provenance: action.provenance.clone(),
      moved_at: self.moved_at,
    })
  }
}

/// Stable replay namespace shared by command publication and recovery reads.
pub fn issue_status_operation_key(operation_id: FactoryDigest) -> IdempotencyKey {
  IdempotencyKey::new(format!("issue-action.{operation_id}")).expect("bounded action digest")
}

/// Sealed safe command metadata in the existing replay record, never a wire authority token.
/// Evidence is requalified against the original immutable journal on restore.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedIssueStatusCommand {
  /// Original Issue/status/workflow preconditions; recovery cannot select a new head.
  pub intent: TransitionIssueIntent,
  /// Exact operator-published direct edge and outcome.
  pub selection: IssuePipelineStatusSelection,
  /// Accepted producing operation and exact selected record references.
  pub provenance: IssuePipelineActionProvenance,
  /// Original trusted move time, independent of retry observation time.
  pub moved_at: octacity_server_domain::Timestamp,
}
impl RetainedIssueStatusCommand {
  /// Restores only the sealed command; current authority/applicability still cross the shared port.
  pub fn restore(&self, snapshot: &FactoryRunSnapshot) -> Result<TransitionIssue, StoreError> {
    if snapshot.work.id() != self.provenance.work_id
      || snapshot.run.id() != self.provenance.run_id
      || snapshot.work.subject().project_id() != self.intent.project_id
      || self.provenance.candidate.is_some() && self.provenance.evidence.is_empty()
    {
      return Err(StoreError::invalid(
        StoreOperation::TransitionIssue,
        StoreInputError::InvalidIssue,
      ));
    }
    let mut provenance = self.provenance.clone();
    let selected = std::mem::take(&mut provenance.evidence);
    let mut accepted = AcceptedIssueStatusAction::for_selection(self.selection.clone(), provenance)?;
    if !selected.is_empty() {
      accepted = accepted.with_evidence(snapshot, &selected)?;
    }
    Ok(TransitionIssue::pipeline(self.intent.clone(), accepted, self.moved_at))
  }
}

// Derived from exact accepted records, never caller booleans or model-shaped payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
struct IssuePipelineEvidence {
  satisfied: std::collections::BTreeSet<IssueTransitionEvidence>,
  observed_at: octacity_server_domain::Timestamp,
  expires_at: Option<octacity_server_domain::Timestamp>,
}
impl AcceptedIssueStatusAction {
  /// Qualifies only explicitly selected immutable records of this Work/Run/candidate.
  /// Typed discovery facts must match their canonical payloads and existing journal validation.
  pub fn with_evidence(
    mut self,
    snapshot: &FactoryRunSnapshot,
    selected: &[FactoryDigest],
  ) -> Result<Self, StoreError> {
    use octacity_server_factory::*;
    let invalid = || StoreError::invalid(StoreOperation::TransitionIssue, StoreInputError::InvalidIssue);
    let candidate = self.provenance.candidate.as_ref().ok_or_else(invalid)?;
    if snapshot.work.id() != self.provenance.work_id
      || snapshot.run.id() != self.provenance.run_id
      || snapshot.work.subject() != candidate.exact()
      || selected.is_empty()
      || selected.len() > MAX_EVIDENCE_ITEMS
      || selected.iter().collect::<std::collections::BTreeSet<_>>().len() != selected.len()
    {
      return Err(invalid());
    }
    snapshot
      .flow
      .data
      .validate(&snapshot.work, &snapshot.admitted_flow, snapshot.flow.runtime())
      .map_err(|_| invalid())?;
    let records = selected
      .iter()
      .map(|digest| {
        snapshot
          .flow
          .data
          .records
          .iter()
          .find(|r| r.digest().ok() == Some(*digest))
          .ok_or_else(invalid)
      })
      .collect::<Result<Vec<_>, _>>()?;
    let mut proof = IssuePipelineEvidence {
      satisfied: Default::default(),
      observed_at: records.first().ok_or_else(invalid)?.observed_at(),
      expires_at: None,
    };
    for record in records {
      let facts = record.discovery().ok_or_else(invalid)?;
      if record.input().factory_run_id() != self.provenance.run_id || facts.candidate_subject() != Some(candidate) {
        return Err(invalid());
      }
      proof.observed_at = proof.observed_at.max(record.observed_at());
      match facts.kind() {
        FlowDiscoveryKind::Decision => {
          let decision: Decision = serde_json::from_value(record.payload().value().clone()).map_err(|_| invalid())?;
          if facts != &FlowDiscoveryFacts::decision(&decision) || decision.subject() != candidate {
            return Err(invalid());
          }
          if decision.outcome() == DecisionOutcome::Accept {
            proof.satisfied.insert(IssueTransitionEvidence::AcceptedExecution);
          }
        }
        FlowDiscoveryKind::Evidence => {
          let manifest: EvidenceManifest =
            serde_json::from_value(record.payload().value().clone()).map_err(|_| invalid())?;
          if facts != &FlowDiscoveryFacts::evidence(&manifest).map_err(|_| invalid())?
            || manifest.subject() != candidate
          {
            return Err(invalid());
          }
          if !manifest.requirements().is_empty()
            && manifest
              .items()
              .iter()
              .all(|item| item.outcome() == DeterministicGateOutcome::Passed)
          {
            proof.satisfied.insert(IssueTransitionEvidence::PassedRequiredChecks);
          }
          for item in manifest.items() {
            proof.expires_at = Some(
              proof
                .expires_at
                .map_or(item.fresh_until(), |at| at.min(item.fresh_until())),
            );
          }
        }
        _ => return Err(invalid()),
      }
    }
    self.provenance.evidence = selected.to_vec();
    self.evidence = Some(proof);
    Ok(self)
  }
  pub(crate) fn satisfies(&self, requirement: &IssueTransitionEvidence, at: octacity_server_domain::Timestamp) -> bool {
    self.evidence.as_ref().is_some_and(|p| {
      p.satisfied.contains(requirement) && p.observed_at <= at && p.expires_at.is_none_or(|expires| at < expires)
    })
  }
}
