//! Safe paged read projections of the common Factory execution journal.
use super::*;
use octacity_server_factory::{FactoryKey, FactoryRunId};
use octacity_server_store::{
  FactoryRunDiagnosticKind, FactoryRunDiagnosticRecord, FactoryRunStore, ListFactoryRunDiagnostics,
};
use serde::{Deserialize, Serialize};

/// Maximum child records published by one management read.
pub const MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE: u16 = octacity_server_store::MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE;
/// Closed technical collections; logical node keys never select a read implementation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryDiagnosticCollection {
  /// Root and nested Flow executions.
  FlowRuns,
  /// Immutable cycle identities and causal predecessor links.
  WorkflowCycles,
  /// Safe identities of every configured node execution.
  NodeAttempts,
  /// Ordinary Builds, attempts and Jobs linked to configured execution.
  Builds,
  /// Authoritative versioned aggregate consumption.
  Budgets,
  /// Safe request and actor correlation committed with each mutation.
  Audit,
  /// External invocations with safe frozen task and Context provenance.
  Calls,
  /// Safe immutable admission identity.
  Work,
  /// Verified configured outcomes.
  NodeCompletions,
  /// Verified technical stop receipts.
  NodeStops,

  /// Advisory receipts with separate deterministic consumption.
  DecisionSignals,
  /// Original trusted Candidate and ChangeSet publications.
  Candidates,
  /// Original ChangeSet publications only.
  #[serde(rename = "changesets")]
  ChangeSets,
  /// Verified manifest identities.
  Evidence,
  /// Frozen evaluation plans.
  ReviewPlans,
  /// Safe exact evaluator outcomes.
  Assessments,
  /// Authoritative dispositions.
  Decisions,
  /// Bounded escalation identities.
  Escalations,
  /// Verified delivery receipts.
  Delivery,
  /// Verified rollout observations.
  Promotion,
  /// Reporter outcomes.
  Reporting,
}
impl FactoryDiagnosticCollection {
  /// Stable path component and cursor scope.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::FlowRuns => "flow-runs",
      Self::WorkflowCycles => "workflow-cycles",
      Self::NodeAttempts => "node-attempts",
      Self::Builds => "builds",
      Self::Budgets => "budgets",
      Self::Audit => "audit",
      Self::Calls => "calls",
      Self::Work => "work",
      Self::NodeCompletions => "node-completions",
      Self::NodeStops => "node-stops",

      Self::DecisionSignals => "decision-signals",
      Self::Candidates => "candidates",
      Self::ChangeSets => "changesets",
      Self::Evidence => "evidence",
      Self::ReviewPlans => "review-plans",
      Self::Assessments => "assessments",
      Self::Decisions => "decisions",
      Self::Escalations => "escalations",
      Self::Delivery => "delivery",
      Self::Promotion => "promotion",
      Self::Reporting => "reporting",
    }
  }
  fn kind(self) -> FactoryRunDiagnosticKind {
    match self {
      Self::FlowRuns => FactoryRunDiagnosticKind::FlowRun,
      Self::WorkflowCycles => FactoryRunDiagnosticKind::WorkflowCycle,
      Self::NodeAttempts => FactoryRunDiagnosticKind::NodeAttempt,
      Self::Builds => FactoryRunDiagnosticKind::BuildExecution,
      Self::Budgets => FactoryRunDiagnosticKind::Budget,
      Self::Audit => FactoryRunDiagnosticKind::Audit,
      Self::Calls => FactoryRunDiagnosticKind::Call,
      Self::Work => FactoryRunDiagnosticKind::Work,
      Self::NodeCompletions => FactoryRunDiagnosticKind::NodeAttemptCompletion,
      Self::NodeStops => FactoryRunDiagnosticKind::NodeStop,

      Self::DecisionSignals => {
        FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::DecisionSignal)
      }
      Self::Candidates => FactoryRunDiagnosticKind::CandidatePublication,
      Self::ChangeSets => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::ChangeSet),
      Self::Evidence => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Evidence),
      Self::ReviewPlans => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::ReviewPlan),
      Self::Assessments => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Assessment),
      Self::Decisions => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Decision),
      Self::Escalations => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Escalation),
      Self::Delivery => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Delivery),
      Self::Promotion => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Promotion),
      Self::Reporting => FactoryRunDiagnosticKind::DomainFact(octacity_server_factory::FlowDiscoveryKind::Reporting),
    }
  }
  /// Closed values published in OpenAPI.
  pub const ALL: [Self; 21] = [
    Self::FlowRuns,
    Self::WorkflowCycles,
    Self::NodeAttempts,
    Self::Builds,
    Self::Budgets,
    Self::Audit,
    Self::Calls,
    Self::Work,
    Self::NodeCompletions,
    Self::NodeStops,
    Self::DecisionSignals,
    Self::Candidates,
    Self::ChangeSets,
    Self::Evidence,
    Self::ReviewPlans,
    Self::Assessments,
    Self::Decisions,
    Self::Escalations,
    Self::Delivery,
    Self::Promotion,
    Self::Reporting,
  ];
}
/// Safe typed facts that cannot contain execution inputs or provider output payloads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryDiagnosticRecord {
  /// Exact identity, definition and parent-call correlation.
  FlowRun(octacity_server_factory::FlowRun),
  /// Exact cycle identity and causal predecessor.
  WorkflowCycle(octacity_server_factory::WorkflowCycle),
  /// Exact execution correlation without frozen inputs or ownership credentials.
  NodeAttempt(FactoryNodeAttemptProjection),
  /// Ordinary execution identities, without executable material.
  Build(octacity_server_store::FactoryBuildExecutionDiagnostic),
  /// Authoritative budget usage at an exact aggregate version.
  Budget(octacity_server_store::FactoryBudgetRecord),
  /// Trusted mutation correlation with hashed identities.
  Audit(FactoryAuditProjection),
  /// Typed safe owner facts under the exact producing record and input identities.
  DomainFact(Box<octacity_server_store::FactoryDomainDiagnostic>),
  /// Exact invocation and safe task metadata without the task itself.
  Call(Box<FactoryCallProjection>),
  /// Immutable Work authority without task references or metadata prose.
  Work(FactoryWorkProjection),
  /// Configured completion without provider output or ownership credentials.
  NodeCompletion(FactoryNodeCompletionProjection),
  /// Confirmed stopped execution without a manufactured business outcome.
  NodeStop(FactoryNodeStopProjection),
}
impl TryFrom<FactoryRunDiagnosticRecord> for FactoryDiagnosticRecord {
  type Error = ApplicationError;
  fn try_from(record: FactoryRunDiagnosticRecord) -> Result<Self, Self::Error> {
    match record {
      FactoryRunDiagnosticRecord::FlowRun(flow) => Ok(FactoryDiagnosticRecord::FlowRun(*flow)),
      FactoryRunDiagnosticRecord::WorkflowCycle(cycle) => Ok(FactoryDiagnosticRecord::WorkflowCycle(*cycle)),
      FactoryRunDiagnosticRecord::NodeAttempt(node) => Ok(FactoryDiagnosticRecord::NodeAttempt((&*node).into())),
      FactoryRunDiagnosticRecord::BuildExecution(build) => Ok(FactoryDiagnosticRecord::Build(*build)),
      FactoryRunDiagnosticRecord::Budget(budget) => Ok(FactoryDiagnosticRecord::Budget(*budget)),
      FactoryRunDiagnosticRecord::Audit(audit) => Ok(FactoryDiagnosticRecord::Audit((&*audit).into())),
      FactoryRunDiagnosticRecord::DomainFact(fact) => Ok(FactoryDiagnosticRecord::DomainFact(fact)),
      FactoryRunDiagnosticRecord::Call { node, profile } => {
        Ok(FactoryDiagnosticRecord::Call(Box::new(FactoryCallProjection {
          node: (&*node).into(),
          profile: profile.map(|profile| *profile),
        })))
      }
      FactoryRunDiagnosticRecord::Work(work) => Ok(FactoryDiagnosticRecord::Work((&*work).into())),
      FactoryRunDiagnosticRecord::NodeAttemptCompletion(completion) => {
        Ok(FactoryDiagnosticRecord::NodeCompletion((&*completion).into()))
      }
      FactoryRunDiagnosticRecord::NodeStop(stop) => Ok(FactoryDiagnosticRecord::NodeStop((&*stop).try_into()?)),
      _ => Err(ApplicationError::invalid()),
    }
  }
}
/// Allowlisted immutable facts; protected material and worker ownership remain private.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryWorkProjection {
  /// Immutable admission identity.
  pub id: octacity_server_factory::WorkEnvelopeId,
  /// Exact immutable controlling configuration.
  pub configuration: octacity_server_factory::FactoryConfigurationRef,
  /// Scoped public Work identity.
  pub external_identity: octacity_server_factory::ExternalWorkIdentity,
  /// Exact Project, Repository and admitted base.
  pub subject: octacity_server_factory::ExactSubject,
  /// Bounded admitted priority.
  pub priority: octacity_server_factory::WorkPriority,
  /// Admitted risk classification.
  pub risk: octacity_server_factory::RiskClass,
}
/// Allowlisted immutable facts; protected material and worker ownership remain private.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryNodeCompletionProjection {
  /// Canonical completion identity.
  pub id: octacity_server_factory::FactoryDigest,
  /// Exact producing attempt.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Exact root or nested execution.
  pub flow_run_id: octacity_server_factory::FlowRunId,
  /// Exact producing cycle.
  pub cycle_id: octacity_server_factory::WorkflowCycleId,
  /// Caller-configured logical node key.
  pub node_key: FactoryKey,
  /// Verified finite configured outcome.
  pub outcome: FactoryKey,
  /// Exact declared result schema.
  pub output_schema: octacity_server_factory::ImmutableReference,
  /// Complete retained record identity, without its bytes.
  pub output_digest: octacity_server_factory::FactoryDigest,
  /// Measured bounded resource consumption.
  pub usage: octacity_server_factory::BudgetUsage,
  /// Trusted completion time.
  pub observed_at: octacity_server_domain::Timestamp,
}
/// Allowlisted immutable facts; protected material and worker ownership remain private.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryNodeStopProjection {
  /// Canonical stop receipt identity.
  pub id: octacity_server_factory::FactoryDigest,
  /// Exact stopped execution.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Code-owned technical cause, without prose.
  pub reason: octacity_server_factory::FlowNodeStopReason,
  /// Measured bounded resource consumption.
  pub usage: octacity_server_factory::BudgetUsage,
  /// Exact ordinary execution link when present.
  pub execution: Option<octacity_server_factory::FactoryDigest>,
  /// Trusted stopped observation time.
  pub observed_at: octacity_server_domain::Timestamp,
}
impl From<&octacity_server_factory::WorkEnvelope> for FactoryWorkProjection {
  fn from(work: &octacity_server_factory::WorkEnvelope) -> Self {
    Self {
      id: work.id(),
      configuration: work.configuration().clone(),
      external_identity: work.external_identity().clone(),
      subject: work.subject().clone(),
      priority: work.priority(),
      risk: work.risk(),
    }
  }
}
impl From<&octacity_server_factory::NodeAttemptCompletion> for FactoryNodeCompletionProjection {
  fn from(row: &octacity_server_factory::NodeAttemptCompletion) -> Self {
    Self {
      id: row.id(),
      node_attempt_id: row.node_attempt_id(),
      flow_run_id: row.flow_run_id(),
      cycle_id: row.workflow_cycle_id(),
      node_key: row.node_key().clone(),
      outcome: row.outcome().clone(),
      output_schema: row.output_schema().clone(),
      output_digest: row.output_digest(),
      usage: row.usage(),
      observed_at: row.observed_at(),
    }
  }
}
impl TryFrom<&octacity_server_factory::FlowNodeStop> for FactoryNodeStopProjection {
  type Error = ApplicationError;
  fn try_from(row: &octacity_server_factory::FlowNodeStop) -> Result<Self, Self::Error> {
    Ok(Self {
      id: row.digest().map_err(|_| ApplicationError::invalid())?,
      node_attempt_id: row.node_attempt_id(),
      reason: row.reason(),
      usage: row.usage(),
      execution: row.execution(),
      observed_at: row.observed_at(),
    })
  }
}
/// External node invocation and its allowlisted frozen task provenance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCallProjection {
  /// Exact persisted execution identity, without ownership credentials.
  pub node: FactoryNodeAttemptProjection,
  /// Safe Build-backed task metadata; direct trusted plugins have no Build profile.
  pub profile: Option<octacity_server_store::FactoryCallProfileDiagnostic>,
}
/// Explicit allowlist of immutable node execution facts exposed to operators.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryNodeAttemptProjection {
  /// Immutable execution identity.
  pub id: octacity_server_factory::NodeAttemptId,
  /// Owning Factory Run.
  pub factory_run_id: FactoryRunId,
  /// Exact root or nested Flow Run.
  pub flow_run_id: octacity_server_factory::FlowRunId,
  /// Exact workflow cycle.
  pub cycle_id: octacity_server_factory::WorkflowCycleId,
  /// Caller-configured logical key, never interpreted by this projection.
  pub node_key: FactoryKey,
  /// Code-owned execution primitive.
  pub node_kind: octacity_server_factory::FlowNodeKind,
  /// Positive execution sequence for this node and cycle.
  pub number: octacity_server_factory::NodeAttemptNumber,
  /// Content identity of the frozen input, without the input itself.
  pub input_digest: octacity_server_factory::FactoryDigest,
  /// Exact declared executor or built-in execution marker.
  pub execution: octacity_server_factory::NodeExecutionIdentity,
  /// Authoritative start time.
  pub started_at: octacity_server_domain::Timestamp,
  /// Original bounded execution deadline.
  pub deadline: octacity_server_domain::Timestamp,
  /// Exact failed execution that an eligible retry replaces.
  pub retry_of: Option<octacity_server_factory::NodeAttemptId>,
  /// Current eligibility, present only in NodeAttempts reads backed by a snapshot.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub retryable: Option<bool>,
  /// Current activity, absent from immutable Call identities rather than guessed.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub active: Option<bool>,
}
impl From<&octacity_server_factory::NodeAttempt> for FactoryNodeAttemptProjection {
  fn from(node: &octacity_server_factory::NodeAttempt) -> Self {
    Self {
      id: node.id(),
      factory_run_id: node.factory_run_id(),
      flow_run_id: node.flow_run_id(),
      cycle_id: node.workflow_cycle_id(),
      node_key: node.node_key().clone(),
      node_kind: node.node_kind(),
      number: node.number(),
      input_digest: node.input_digest(),
      execution: node.execution().clone(),
      started_at: node.started_at(),
      deadline: node.deadline(),
      retry_of: node.retry_of(),
      retryable: None,
      active: None,
    }
  }
}
impl FactoryNodeAttemptProjection {
  /// Projects immutable facts and derives retry eligibility through the common control validator.
  pub fn from_snapshot(
    node: &octacity_server_factory::NodeAttempt,
    snapshot: &octacity_server_store::FactoryRunSnapshot,
  ) -> Self {
    let mut projection = Self::from(node);
    let history = snapshot.flow.runtime();
    projection.active = Some(
      !snapshot.run.state().is_terminal()
        && history.is_current_cycle(node.workflow_cycle_id())
        && history.active_nodes().any(|attempt| attempt.id() == node.id()),
    );
    projection.retryable = Some(
      snapshot
        .budgets
        .iter()
        .find(|budget| budget.id == snapshot.current.budget_id)
        .is_some_and(|budget| {
          octacity_server_store::validate_factory_node_retry(snapshot, node.id(), budget.recorded_at).is_ok()
        }),
    );
    projection
  }
}

/// Safe immutable request correlation, excluding raw actor identifiers and prose.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryAuditProjection {
  /// Content identity of the committed fact.
  pub id: octacity_server_factory::FactoryDigest,
  /// Owning Run.
  pub run_id: FactoryRunId,
  /// Stable code-owned actor classification.
  pub actor_kind: String,
  /// Accepted actor identity digest, when authenticated.
  pub actor_identity_digest: Option<octacity_server_factory::FactoryDigest>,
  /// Stable operation classification.
  pub operation: FactoryKey,
  /// Digest of the accepted request identity.
  pub request_identity_digest: octacity_server_factory::FactoryDigest,
  /// Stable operation outcome, independent of provider prose.
  pub outcome: FactoryKey,
  /// Authoritative commit time.
  pub recorded_at: octacity_server_domain::Timestamp,
}
impl From<&octacity_server_store::FactoryAuditFact> for FactoryAuditProjection {
  fn from(fact: &octacity_server_store::FactoryAuditFact) -> Self {
    Self {
      id: fact.id,
      run_id: fact.run_id,
      actor_kind: fact.actor_kind.as_str().to_owned(),
      actor_identity_digest: fact.actor_identity_digest,
      operation: fact.operation.clone(),
      request_identity_digest: fact.request_identity_digest,
      outcome: fact.outcome.clone(),
      recorded_at: fact.recorded_at,
    }
  }
}
/// Bounded child collection with a continuation scoped to this Run and collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryDiagnosticPage {
  /// Safe immutable records in ascending identity order.
  pub items: Vec<FactoryDiagnosticRecord>,
  /// Exclusive continuation, absent at the end.
  pub next_cursor: Option<String>,
}
/// One bounded read of a typed common journal collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryDiagnosticsQuery {
  page: ListFactoryRunDiagnostics,
  collection: FactoryDiagnosticCollection,
}
impl ListFactoryDiagnosticsQuery {
  /// Validates cursor binding and page limits before persistence access.
  pub fn try_new(
    run_id: FactoryRunId,
    collection: FactoryDiagnosticCollection,
    after: Option<&str>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    let after = after
      .map(|value| {
        if value.len() > 256 {
          return Err(ApplicationError::invalid());
        }
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "1" || parts[1] != run_id.to_string() || parts[2] != collection.as_str() {
          return Err(ApplicationError::invalid());
        }
        parts[3].parse::<FactoryKey>().map_err(|_| ApplicationError::invalid())
      })
      .transpose()?;
    Ok(Self {
      page: ListFactoryRunDiagnostics::new(run_id, collection.kind(), after, limit)?,
      collection,
    })
  }
  /// Owning Run identity.
  pub const fn run_id(&self) -> FactoryRunId {
    self.page.run_id
  }
  /// Exact technical collection selected by the caller.
  pub const fn collection(&self) -> FactoryDiagnosticCollection {
    self.collection
  }
  /// Validated positive page size.
  pub const fn limit(&self) -> u16 {
    self.page.limit.get()
  }
}
impl Query for ListFactoryDiagnosticsQuery {
  type Outcome = FactoryDiagnosticPage;
}
impl ManagementAuthorizationTarget for ListFactoryDiagnosticsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRunQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::FactoryRun, self.run_id())
  }
}
impl crate::ManagementVisibilityTarget for ListFactoryDiagnosticsQuery {
  type Visibility = FactoryRunListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] =
    <GetFactoryRunQuery as crate::ManagementVisibilityTarget>::RESTRICTED_RESOURCE_KINDS;
  fn visibility_from(
    grant: &crate::ManagementAuthorizationGrant,
  ) -> Result<Self::Visibility, crate::ManagementVisibilityError> {
    <GetFactoryRunQuery as crate::ManagementVisibilityTarget>::visibility_from(grant)
  }
}
#[async_trait]
impl<S: FactoryDiscoveryStore + FactoryRunStore + 'static> crate::ManagementQueryUseCase<ListFactoryDiagnosticsQuery>
  for FactoryDiscoveryHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListFactoryDiagnosticsQuery,
  ) -> Result<FactoryDiagnosticPage, ApplicationError> {
    let visibility = grant
      .visibility_for::<ListFactoryDiagnosticsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    self.visible_run_summary(context, query.run_id(), visibility).await?;
    let page = self.store.list_factory_run_diagnostics(query.page.clone()).await?;
    let mut items: Vec<FactoryDiagnosticRecord> = page
      .items
      .into_iter()
      .map(FactoryDiagnosticRecord::try_from)
      .collect::<Result<_, _>>()?;
    if query.collection() == FactoryDiagnosticCollection::NodeAttempts {
      let snapshot = self.store.factory_run_snapshot(query.run_id()).await?;
      for item in &mut items {
        if let FactoryDiagnosticRecord::NodeAttempt(projection) = item {
          let node = snapshot
            .flow
            .attempts
            .iter()
            .find(|node| node.id() == projection.id)
            .ok_or(ApplicationError::Store(octacity_server_store::StoreError::Unavailable))?;
          *projection = FactoryNodeAttemptProjection::from_snapshot(node, &snapshot);
        }
      }
    }
    Ok(FactoryDiagnosticPage {
      items,
      next_cursor: page
        .next
        .map(|key| format!("1:{}:{}:{}", query.run_id(), query.collection.as_str(), key)),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn domain_collections_publish_verified_dispositions_separately_from_signals() {
    crate::factory_admission_tests::run_ready(async {
      use octacity_server_store::testing::*;
      let mut fixture = factory_node_journal_contract_fixture();
      fixture.discovery = Some(serde_json::from_value(serde_json::json!({"kind":"decision",
        "subject":octacity_server_factory::CandidateSubject::new(fixture.admission.work.subject().clone(),
          octacity_server_domain::ImmutableRevision::new("verified-candidate").unwrap(), octacity_server_factory::FactoryDigest::from_bytes([91;32])),
        "id":"88888888-8888-4888-8888-888888888888", "outcome":"accept"})).unwrap());
      let store = Arc::new(InMemoryFactoryConfigurationStore::new());
      store
        .seed_factory_run(
          fixture.admission.clone(),
          octacity_server_factory::FactoryDigest::from_bytes([1; 32]),
          management_mutation_with_request((), "diagnostic-admission").audit(),
        )
        .unwrap();
      verify_factory_node_journal_contract(&*store, &*store, &fixture).await;
      let work = store
        .list_factory_run_diagnostics(
          ListFactoryDiagnosticsQuery::try_new(fixture.admission.run.id(), FactoryDiagnosticCollection::Work, None, 1)
            .unwrap()
            .page,
        )
        .await
        .unwrap();
      let work = serde_json::to_value(FactoryDiagnosticRecord::try_from(work.items[0].clone()).unwrap()).unwrap();
      assert_eq!(work["record"]["id"], fixture.admission.work.id().to_string());
      assert!(work["record"].get("artifacts").is_none());
      assert!(work["record"].get("metadata").is_none());
      let completed = store
        .list_factory_run_diagnostics(
          ListFactoryDiagnosticsQuery::try_new(
            fixture.admission.run.id(),
            FactoryDiagnosticCollection::NodeCompletions,
            None,
            1,
          )
          .unwrap()
          .page,
        )
        .await
        .unwrap();
      let completed =
        serde_json::to_value(FactoryDiagnosticRecord::try_from(completed.items[0].clone()).unwrap()).unwrap();
      assert_eq!(completed["record"]["node_attempt_id"], fixture.attempt.id().to_string());
      assert!(completed["record"].get("ownership").is_none());
      let stop = octacity_server_factory::FlowNodeStop::new(
        &fixture.attempt,
        octacity_server_factory::FactoryClaimOwnership::new(fixture.attempt.owner().clone(), fixture.attempt.claim()),
        octacity_server_factory::FlowNodeStopReason::ExecutionFailed,
        crate::factory_admission_tests::time(20),
        octacity_server_factory::BudgetUsage::default(),
        None,
      )
      .unwrap();
      let stopped = serde_json::to_value(
        FactoryDiagnosticRecord::try_from(FactoryRunDiagnosticRecord::NodeStop(Box::new(stop))).unwrap(),
      )
      .unwrap();
      assert_eq!(stopped["record"]["reason"]["kind"], "execution_failed");
      assert!(stopped["record"].get("ownership").is_none());
      let stop_query = ListFactoryDiagnosticsQuery::try_new(
        fixture.admission.run.id(),
        FactoryDiagnosticCollection::NodeStops,
        None,
        1,
      )
      .unwrap();
      assert!(
        store
          .list_factory_run_diagnostics(stop_query.page)
          .await
          .unwrap()
          .items
          .is_empty()
      );

      let run_id = fixture.admission.run.id();
      let query =
        ListFactoryDiagnosticsQuery::try_new(run_id, FactoryDiagnosticCollection::Decisions, None, 1).unwrap();
      let page = store.list_factory_run_diagnostics(query.page).await.unwrap();
      let decisions = FactoryDiagnosticPage {
        items: page
          .items
          .into_iter()
          .map(FactoryDiagnosticRecord::try_from)
          .collect::<Result<Vec<_>, _>>()
          .unwrap(),
        next_cursor: None,
      };
      let value = serde_json::to_value(decisions).unwrap();
      assert_eq!(value["items"][0]["record"]["facts"]["kind"], "decision");
      assert_eq!(value["items"][0]["record"]["facts"]["outcome"], "accept");
      assert_eq!(
        value["items"][0]["record"]["node_attempt_id"],
        fixture.attempt.id().to_string()
      );
      let calls = store
        .list_factory_run_diagnostics(
          ListFactoryDiagnosticsQuery::try_new(run_id, FactoryDiagnosticCollection::Calls, None, 1)
            .unwrap()
            .page,
        )
        .await
        .unwrap();
      let call = FactoryDiagnosticRecord::try_from(calls.items[0].clone()).unwrap();
      let call = serde_json::to_value(call).unwrap();
      assert_eq!(call["kind"], "call");
      assert_eq!(call["record"]["node"]["id"], fixture.attempt.id().to_string());
      for current_state in ["active", "retryable"] {
        assert!(
          call["record"]["node"].get(current_state).is_none(),
          "Calls publish immutable node identity, not invented {current_state}"
        );
      }
      assert_eq!(
        call["record"]["profile"]["context_digest"],
        fixture.input.context().unwrap().digest().unwrap().to_string()
      );
      for private in ["input", "payload", "context", "permissions", "commands"] {
        assert!(call["record"]["profile"].get(private).is_none(), "{private}");
      }
      for collection in [
        FactoryDiagnosticCollection::DecisionSignals,
        FactoryDiagnosticCollection::ChangeSets,
        FactoryDiagnosticCollection::Evidence,
        FactoryDiagnosticCollection::ReviewPlans,
        FactoryDiagnosticCollection::Assessments,
        FactoryDiagnosticCollection::Escalations,
        FactoryDiagnosticCollection::Delivery,
        FactoryDiagnosticCollection::Promotion,
        FactoryDiagnosticCollection::Reporting,
      ] {
        let query = ListFactoryDiagnosticsQuery::try_new(run_id, collection, None, 1).unwrap();
        let page = store.list_factory_run_diagnostics(query.page).await.unwrap();
        assert!(page.items.is_empty(), "{collection:?}");
        assert!(page.next.is_none());
      }
    });
  }
  #[test]
  fn node_projection_publishes_exact_execution_identity_without_worker_ownership() {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
    let projection = FactoryNodeAttemptProjection::from(&fixture.attempt);
    let value = serde_json::to_value(projection).unwrap();
    assert_eq!(value["id"], fixture.attempt.id().to_string());
    assert_eq!(value["flow_run_id"], fixture.attempt.flow_run_id().to_string());
    assert_eq!(value["cycle_id"], fixture.attempt.workflow_cycle_id().to_string());
    assert_eq!(value["input_digest"], fixture.input.digest().unwrap().to_string());
    assert_eq!(
      value["execution"],
      serde_json::to_value(fixture.attempt.execution()).unwrap()
    );
    for private in ["input", "ownership", "owner", "claim", "payload", "commands"] {
      assert!(value.get(private).is_none(), "{private}");
    }
  }
}
