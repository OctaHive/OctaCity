//! Exact retained candidate context with separate observation and execution gates.
use crate::{
  ApplicationError, FactoryCandidateContext, FactoryCandidateContextSource, FactoryExecutionClock,
  ReadFactoryCandidateContext,
};
use async_trait::async_trait;
use octacity_artifact_store::ArtifactByteReader;
use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryClaimOwnership, FactoryContextReference, FlowBuildIntent, ImmutableReference};
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore, FactoryRunSnapshot, FactoryRunStore};
use std::{collections::BTreeSet, sync::Arc};

/// Trusted execution context selection; caller/model producer facts are not accepted.
#[async_trait]
pub trait FactoryNodeCandidateContextSource: Send + Sync {
  /// Reads the exact frozen candidate while this consumer still owns execution.
  /// Retention must cover the original execution deadline, never a shorter window.
  async fn read_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    ownership: &FactoryClaimOwnership,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError>;
}

/// Readonly candidate recovery from current persisted lineage, without dispatch authority.
#[async_trait]
pub trait FactoryRetainedCandidateContextSource: Send + Sync {
  /// Verifies retained publications at actual time and exact current intent linkage.
  /// This operation neither requires nor renews an execution claim or deadline.
  async fn read_retained_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError>;
}

/// Joins exact persisted consumer lineage with retained capture publications.
/// Execution additionally requires current fenced authority; observation does not.
/// The installed capture reference is independent of logical Flow/node names.
pub struct FactoryStoredCandidateContextSource<R, A, Q, B> {
  runs: Arc<R>,
  source: Arc<FactoryCandidateContextSource<A, Q, B>>,
  capture_tool: ImmutableReference,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, A, Q, B> FactoryStoredCandidateContextSource<R, A, Q, B> {
  /// Installs authoritative journals, the existing Artifact reader and trusted time.
  pub fn new(
    runs: Arc<R>,
    source: Arc<FactoryCandidateContextSource<A, Q, B>>,
    capture_tool: ImmutableReference,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      runs,
      source,
      capture_tool,
      clock,
    }
  }
}
#[async_trait]
impl<R: FactoryRunStore, A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader>
  FactoryNodeCandidateContextSource for FactoryStoredCandidateContextSource<R, A, Q, B>
{
  async fn read_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    ownership: &FactoryClaimOwnership,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    let invalid = ApplicationError::invalid;
    if required_until != intent.node().deadline() {
      return Err(invalid());
    }
    let at = self.clock.now()?.max(observed_at);
    let snapshot = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    verify_current_consumer(&snapshot, intent, ownership, at)?;
    let result = self
      .read_selected_context(&snapshot, intent, at, required_until)
      .await?;
    let now = self.clock.now()?.max(at);
    let current = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    verify_current_consumer(&current, intent, ownership, now)?;
    Ok(result)
  }
}

#[async_trait]
impl<R: FactoryRunStore, A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader>
  FactoryRetainedCandidateContextSource for FactoryStoredCandidateContextSource<R, A, Q, B>
{
  async fn read_retained_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    let at = self.clock.now()?.max(observed_at);
    if required_until <= observed_at {
      return Err(ApplicationError::invalid());
    }
    // This is a readonly retention guarantee, not an execution deadline. A
    // caller's observation can precede this reader's actual entry under load.
    let read_point = at
      .unix_millis()
      .checked_add(1)
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(ApplicationError::invalid)?;
    let required_until = required_until.max(read_point);
    let snapshot = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    verify_retained_consumer(&snapshot, intent)?;
    let context = self
      .read_selected_context(&snapshot, intent, at, required_until)
      .await?;
    let now = self.clock.now()?.max(at);
    let current = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    verify_retained_consumer(&current, intent)?;
    // The requested retention window is a lower guarantee, not an I/O timeout.
    // Every publication's validated lifetime must still cover actual completion.
    if !context.is_retained_at(now) {
      return Err(ApplicationError::invalid());
    }
    Ok(context)
  }
}

impl<R, A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader>
  FactoryStoredCandidateContextSource<R, A, Q, B>
{
  async fn read_selected_context(
    &self,
    snapshot: &FactoryRunSnapshot,
    intent: &FlowBuildIntent,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let context = intent.input().context().ok_or_else(invalid)?;
    if context.subject().candidate().is_none() {
      return Err(invalid());
    }
    let selected = context
      .entries()
      .iter()
      .map(|entry| match entry.source() {
        FactoryContextReference::Artifact(artifact) => Ok(artifact.artifact_id()),
        _ => Err(invalid()),
      })
      .collect::<Result<BTreeSet<_>, _>>()?;
    let mut producers = snapshot.flow.data.records.iter().filter(|record| {
      record.build_provenance().is_some_and(|provenance| {
        [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
          .iter()
          .all(|name| {
            provenance
              .outputs()
              .iter()
              .any(|output| output.name.as_str() == *name && selected.contains(&output.artifact.artifact_id()))
          })
      })
    });
    let record = producers.next().ok_or_else(invalid)?;
    if producers.next().is_some() {
      return Err(invalid());
    }
    let mut intents = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .filter(|row| row.node().id() == record.node_attempt_id());
    let producer_intent = intents.next().ok_or_else(invalid)?;
    if intents.next().is_some() {
      return Err(invalid());
    }
    let manifest = record
      .build_provenance()
      .ok_or_else(invalid)?
      .outputs()
      .iter()
      .find(|output| output.name.as_str() == CHANGE_SET_MANIFEST_OUTPUT)
      .ok_or_else(invalid)?;
    let mut executions = snapshot.flow.data.build_executions.iter().filter(|row| {
      row.node_attempt_id() == record.node_attempt_id()
        && row.build_id() == manifest.producer.build_id()
        && row.attempt_id() == manifest.producer.attempt_id()
    });
    let execution = executions.next().ok_or_else(invalid)?;
    if executions.next().is_some() {
      return Err(invalid());
    }
    self
      .source
      .read(ReadFactoryCandidateContext {
        input: intent.input(),
        producer_record: record,
        producer_intent,
        producer_execution: execution,
        capture_tool: &self.capture_tool,
        observed_at,
        required_until,
      })
      .await
  }
}
fn verify_current_consumer(
  snapshot: &FactoryRunSnapshot,
  intent: &FlowBuildIntent,
  ownership: &FactoryClaimOwnership,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  verify_retained_consumer(snapshot, intent)?;
  let node = intent.node();
  if snapshot.run.state().is_terminal()
    || at >= node.deadline()
    || snapshot.current_claim.as_ref().is_none_or(|claim| {
      claim.owner != *ownership.owner()
        || claim.claim != ownership.claim()
        || claim.claim.verify_fence(ownership.claim().fence(), at).is_err()
    })
    || snapshot
      .flow
      .completions
      .iter()
      .any(|row| row.node_attempt_id() == node.id())
    || snapshot
      .flow
      .data
      .stops
      .iter()
      .any(|row| row.node_attempt_id() == node.id())
    || node.verify_observer(ownership, at).is_err()
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
pub(crate) fn verify_retained_consumer(
  snapshot: &FactoryRunSnapshot,
  intent: &FlowBuildIntent,
) -> Result<(), ApplicationError> {
  let input = intent.input();
  let node = intent.node();
  if snapshot.run.id() != input.factory_run_id()
    || snapshot.run.configuration() != input.configuration()
    || snapshot.work.subject() != input.subject()
    || snapshot.run.state() == octacity_server_factory::FactoryRunState::Cancelled
    || !snapshot.flow.runtime().is_current_cycle(input.cycle_id())
    || !snapshot.flow.data.inputs.contains(input)
    || !snapshot.flow.data.build_intents.contains(intent)
    || snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|row| row.cancellation_requested)
    || snapshot
      .flow
      .attempts
      .iter()
      .filter(|row| {
        row.flow_run_id() == node.flow_run_id()
          && row.workflow_cycle_id() == node.workflow_cycle_id()
          && row.node_key() == node.node_key()
      })
      .max_by_key(|row| row.number())
      .is_none_or(|row| row != node)
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
