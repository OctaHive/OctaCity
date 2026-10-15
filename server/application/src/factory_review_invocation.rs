//! Whole typed review inputs qualified against explicitly selected owner journal records.
use crate::ApplicationError;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  EvaluationPlan, EvidenceManifest, FactoryDigest, FlowDiscoveryFacts, FlowNodeInput, FlowNodeRecord,
  ImmutableReference,
};
use octacity_server_store::{FactoryRunSnapshot, FactoryRunStore};
use std::sync::Arc;

/// Qualified immutable metadata for one installed evaluator. This grants no execution authority.
#[derive(Clone)]
pub struct FactoryReviewInvocation {
  input_digest: FactoryDigest,
  evaluator: ImmutableReference,
  plan: EvaluationPlan,
  evidence: EvidenceManifest,
}
impl FactoryReviewInvocation {
  /// Complete owner-frozen consumer input covered by this qualification.
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
  /// Exact installed evaluator selected by the plan.
  pub const fn evaluator(&self) -> &ImmutableReference {
    &self.evaluator
  }
  /// Whole immutable plan explicitly selected in the consumer payload.
  pub const fn plan(&self) -> &EvaluationPlan {
    &self.plan
  }
  /// Whole immutable evidence explicitly selected in the consumer payload.
  pub const fn evidence(&self) -> &EvidenceManifest {
    &self.evidence
  }
}
/// Resolves review metadata without contacting an evaluator or reading private artifacts.
#[async_trait]
pub trait FactoryReviewInvocationSource: Send + Sync {
  /// Qualifies whole projected values and exact installed evaluator against retained completions.
  async fn invocation(
    &self,
    input: &FlowNodeInput,
    evaluator: &ImmutableReference,
    observed_at: Timestamp,
  ) -> Result<FactoryReviewInvocation, ApplicationError>;
}
/// Existing owner journal is the only source of plan/evidence publication authority.
pub struct FactoryStoredReviewInvocationSource<S> {
  store: Arc<S>,
}
impl<S> FactoryStoredReviewInvocationSource<S> {
  /// Installs the authoritative common Run journal; no separate result store is created.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}
#[async_trait]
impl<S: FactoryRunStore> FactoryReviewInvocationSource for FactoryStoredReviewInvocationSource<S> {
  async fn invocation(
    &self,
    input: &FlowNodeInput,
    evaluator: &ImmutableReference,
    observed_at: Timestamp,
  ) -> Result<FactoryReviewInvocation, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let snapshot = self.store.factory_run_snapshot(input.factory_run_id()).await?;
    if snapshot.work.id() != input.work_id()
      || snapshot.work.subject() != input.subject()
      || !snapshot.flow.data.inputs.contains(input)
    {
      return Err(invalid());
    }
    let payload = input.payload().value();
    let plan: EvaluationPlan =
      serde_json::from_value(payload.get("plan").cloned().ok_or_else(invalid)?).map_err(|_| invalid())?;
    let evidence: EvidenceManifest =
      serde_json::from_value(payload.get("evidence").cloned().ok_or_else(invalid)?).map_err(|_| invalid())?;
    let selected: ImmutableReference =
      serde_json::from_value(payload.get("evaluator").cloned().ok_or_else(invalid)?).map_err(|_| invalid())?;
    if &selected != evaluator
      || !plan.has_evaluator(evaluator)
      || plan.subject().exact() != input.subject()
      || plan.subject() != evidence.subject()
      || plan.evidence_id() != evidence.id()
      || plan.created_at() > observed_at
      || observed_at >= plan.deadline()
      || evidence.constructed_at() > plan.created_at()
      || evidence.items().iter().any(|item| observed_at >= item.fresh_until())
    {
      return Err(invalid());
    }
    let records = selected_records(&snapshot, input, observed_at)?;
    let plan_record = whole_record(
      &records,
      &plan,
      FlowDiscoveryFacts::review_plan(&plan).map_err(|_| invalid())?,
    )?;
    let evidence_record = whole_record(
      &records,
      &evidence,
      FlowDiscoveryFacts::evidence(&evidence).map_err(|_| invalid())?,
    )?;
    if plan_record.input().payload().value() != evidence_record.payload().value()
      || !plan_record.input().sources().iter().any(|source| {
        source.node_attempt_id() == evidence_record.node_attempt_id()
          && evidence_record.digest().is_ok_and(|digest| digest == source.digest())
      })
    {
      return Err(invalid());
    }
    Ok(FactoryReviewInvocation {
      input_digest: input.digest().map_err(|_| invalid())?,
      evaluator: selected,
      plan,
      evidence,
    })
  }
}
fn selected_records<'a>(
  snapshot: &'a FactoryRunSnapshot,
  input: &FlowNodeInput,
  at: Timestamp,
) -> Result<Vec<&'a FlowNodeRecord>, ApplicationError> {
  let invalid = ApplicationError::invalid;
  input
    .sources()
    .iter()
    .map(|source| {
      let record = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|record| record.node_attempt_id() == source.node_attempt_id())
        .ok_or_else(invalid)?;
      let attempt = snapshot
        .flow
        .attempts
        .iter()
        .find(|attempt| attempt.id() == source.node_attempt_id())
        .ok_or_else(invalid)?;
      let completion = snapshot
        .flow
        .completions
        .iter()
        .find(|completion| completion.node_attempt_id() == source.node_attempt_id())
        .ok_or_else(invalid)?;
      let definition = snapshot
        .admitted_flow
        .closure()
        .definition(record.input().definition())
        .ok_or_else(invalid)?;
      let schema = snapshot
        .admitted_flow
        .data_schema(record.payload().schema())
        .ok_or_else(invalid)?;
      if record.digest().map_err(|_| invalid())? != source.digest()
        || completion.output_digest() != source.digest()
        || record.input().factory_run_id() != input.factory_run_id()
        || record.input().work_id() != input.work_id()
        || !snapshot.flow.data.inputs.contains(record.input())
        || record.observed_at() > at
        || completion.observed_at() > at
      {
        return Err(invalid());
      }
      let bytes = serde_json::to_vec(record).map_err(|_| invalid())?;
      FlowNodeRecord::restore(&bytes, record.input(), attempt, definition, schema, completion)
        .map_err(|_| invalid())?;
      Ok(record)
    })
    .collect()
}
fn whole_record<'a, T: serde::Serialize>(
  records: &[&'a FlowNodeRecord],
  value: &T,
  discovery: FlowDiscoveryFacts,
) -> Result<&'a FlowNodeRecord, ApplicationError> {
  let value = serde_json::to_value(value).map_err(|_| ApplicationError::invalid())?;
  let mut matching = records
    .iter()
    .filter(|record| record.payload().value() == &value && record.discovery() == Some(&discovery));
  let selected = *matching.next().ok_or_else(ApplicationError::invalid)?;
  if matching.next().is_some() {
    return Err(ApplicationError::invalid());
  }
  Ok(selected)
}
