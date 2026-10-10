//! Fenced two-phase manual intake over the configured nested Flow and ordinary execution ports.
use crate::ApplicationError;
pub(super) mod history;
mod transition;
use async_trait::async_trait;
use history::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;
pub(super) use transition::commit_intake;

/// Frozen server-owned request to execute or observe one exact intake subflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryTriagePhaseRequest {
  /// Current authoritative snapshot, including persisted dispatch intent and ownership.
  pub snapshot: FactoryRunSnapshot,
  /// Exact admitted phase execution.
  pub flow_run: FlowRun,
  /// Frozen typed phase input.
  pub input: FactoryTriagePhaseInput,
  /// Exact selected producer, model/tool, and task policy.
  pub execution: FactoryTriageExecutionProfile,
}

/// Provider-neutral typed inputs; classification is created only by deterministic eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryTriagePhaseInput {
  /// First-phase input after authorized discovery is frozen.
  Eligibility(Box<EligibilityInput>),
  /// Second-phase input with accepted first-phase provenance.
  Classification(Box<ClassificationInput>),
}

/// Observation returned only after its producing Node Attempt has completed durably.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryTriagePhaseResult {
  /// Non-authoritative eligibility observations.
  Eligibility(Box<EligibilityResult>),
  /// Non-authoritative classification observations.
  Classification(Box<TriageResult>),
}

/// Server-side execution seam over ordinary Builds and the pinned Flow interpreter.
///
/// Implementations observe the stable phase Flow Run before dispatch, commit its
/// Node Attempts/outbox before invoking external execution, and persist typed
/// completions and measured budgets under the current Run fence. Restart must
/// observe the same execution. This port never returns a route or a decision.
#[async_trait]
pub trait FactoryTriagePhaseExecutor: Send + Sync {
  /// Dispatches or observes one persisted phase; `None` means execution is pending.
  async fn observe_or_dispatch(
    &self,
    request: FactoryTriagePhaseRequest,
  ) -> Result<Option<FactoryTriagePhaseResult>, ApplicationError>;
}

/// Authorized immutable discovery performed once before eligibility dispatch.
#[async_trait]
pub trait FactoryTriageDiscovery: Send + Sync {
  /// Resolves observed dependencies to exact authorized Run identities and terminal requirements.
  async fn phase_dependencies(
    &self,
    work: &WorkEnvelope,
    dependencies: &[TriageDependency],
  ) -> Result<Vec<PhasePoolDependency>, ApplicationError>;
  /// Returns bounded same-Project candidate facts; hidden Work must be omitted.
  async fn duplicate_candidates(&self, work: &WorkEnvelope) -> Result<Vec<DuplicateCandidate>, ApplicationError>;
}

/// Trusted deterministic validation, separate from all phase/provider observation contracts.
#[async_trait]
pub trait FactoryTriageEvidenceValidator: Send + Sync {
  /// Accepts exact Project-fit and duplicate evidence for eligibility.
  async fn eligibility_evidence(
    &self,
    input: &EligibilityInput,
    result: &EligibilityResult,
  ) -> Result<Vec<AcceptedTriageEvidence>, ApplicationError>;
  /// Accepts exact reproduction or terminal evidence for classification.
  async fn classification_evidence(
    &self,
    input: &ClassificationInput,
    result: &TriageResult,
  ) -> Result<Vec<AcceptedTriageEvidence>, ApplicationError>;
}

/// Result of one bounded intake step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryTriageStep {
  /// A persisted phase is executing.
  Waiting,
  /// One new input, phase result, or deterministic gate was committed.
  Advanced,
  /// Work reached its durable declared successor pool.
  Ready(PhasePoolEntry),
  /// Work resolved without an implementation dispatch.
  Resolved(TriageDisposition),
}

/// Restart-safe intake coordinator; every call advances at most one phase.
pub struct FactoryTriageRunner<S> {
  store: Arc<S>,
  discovery: Arc<dyn FactoryTriageDiscovery>,
  execution: Arc<dyn FactoryTriagePhaseExecutor>,
  evidence: Arc<dyn FactoryTriageEvidenceValidator>,
}

impl<S> FactoryTriageRunner<S>
where
  S: FactoryRunStore + FactoryConfigurationStore + FactoryPhasePoolStore + 'static,
{
  /// Connects authoritative persistence and distinct discovery, execution, and validation ports.
  #[must_use]
  pub fn new(
    store: Arc<S>,
    discovery: Arc<dyn FactoryTriageDiscovery>,
    execution: Arc<dyn FactoryTriagePhaseExecutor>,
    evidence: Arc<dyn FactoryTriageEvidenceValidator>,
  ) -> Self {
    Self {
      store,
      discovery,
      execution,
      evidence,
    }
  }

  /// Advances one claimed Run; stale or expired ownership fails before execution.
  pub async fn run_once(
    &self,
    run_id: FactoryRunId,
    claim_id: FactoryDigest,
    at: Timestamp,
  ) -> Result<FactoryTriageStep, ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(run_id).await?;
    let claim = snapshot
      .current_claim
      .as_ref()
      .filter(|row| row.id == claim_id)
      .ok_or_else(ApplicationError::invalid)?;
    claim.claim.verify_fence(claim.claim.fence(), at).map_err(invalid)?;
    if cancellation(&snapshot)? {
      return Ok(FactoryTriageStep::Waiting);
    }
    let config = self
      .store
      .factory_configuration_version(
        snapshot.run.configuration().id(),
        snapshot.run.configuration().version(),
      )
      .await?
      .configuration;
    if config.reference() != snapshot.run.configuration() {
      return Err(ApplicationError::invalid());
    }
    let flow = config.flow().ok_or_else(ApplicationError::invalid)?;
    let policy = TriagePolicy::for_flow(
      config.reference().clone(),
      &snapshot.admitted_flow.validated().map_err(invalid)?,
      flow.triage.definition,
      flow.triage.policy.clone(),
    )
    .map_err(invalid)?;
    match snapshot.flow.triage.last() {
      None => {
        if snapshot.run.state() != FactoryRunState::Admitted || cancellation(&snapshot)? {
          return Ok(FactoryTriageStep::Waiting);
        }
        let candidates = self.discovery.duplicate_candidates(&snapshot.work).await?;
        let input =
          EligibilityInput::new(&snapshot.work, flow.triage.project_goals.clone(), candidates).map_err(invalid)?;
        let mut append = FactoryRunHistoryAppend::default();
        let mut history = snapshot.flow.clone();
        let root = snapshot.admitted_flow.root_run().clone();
        let cycle = snapshot.admitted_flow.initial_cycle().clone();
        let root_definition = snapshot
          .admitted_flow
          .closure()
          .definition(root.definition())
          .ok_or_else(ApplicationError::invalid)?;
        let triage = enter(
          &snapshot,
          &mut history,
          &root,
          &cycle,
          root_definition.entry(),
          input.digest().map_err(invalid)?,
        )?;
        let triage_cycle = cycle_for(&history, &triage)?;
        enter(
          &snapshot,
          &mut history,
          &triage,
          &triage_cycle,
          &TriageNode::Eligibility.key(),
          input.digest().map_err(invalid)?,
        )?;
        append.flow = difference(&snapshot.flow, &history);
        append.flow.triage.push(TriageJournalRecord::Prepared(Box::new(input)));
        self.commit(&snapshot, append, None, at).await?;
        Ok(FactoryTriageStep::Advanced)
      }
      Some(TriageJournalRecord::Prepared(input)) => {
        let input = input.as_ref().clone();
        let phase = phase_run(&snapshot, TriageNode::Eligibility)?;
        let result = self
          .execution
          .observe_or_dispatch(FactoryTriagePhaseRequest {
            snapshot: snapshot.clone(),
            flow_run: phase.clone(),
            input: FactoryTriagePhaseInput::Eligibility(Box::new(input.clone())),
            execution: flow.triage.eligibility.clone(),
          })
          .await?;
        let Some(FactoryTriagePhaseResult::Eligibility(result)) = result else {
          return if result.is_none() {
            Ok(FactoryTriageStep::Waiting)
          } else {
            Err(ApplicationError::invalid())
          };
        };
        let result = *result;
        let snapshot = self.store.factory_run_snapshot(run_id).await?;
        if snapshot.current_claim.as_ref().is_none_or(|row| row.id != claim_id) || cancellation(&snapshot)? {
          return Err(ApplicationError::invalid());
        }
        verify_observation(
          &snapshot,
          &phase,
          result.provenance(),
          result.digest().map_err(invalid)?,
        )?;
        let evidence = self.evidence.eligibility_evidence(&input, &result).await?;
        let usage = current_usage(&snapshot)?;
        let decision = policy.eligibility(&input, &result, &evidence, usage).map_err(invalid)?;
        let receipt = EligibilityReceipt {
          input,
          result,
          evidence,
          usage,
          decision,
        };
        let mut history = snapshot.flow.clone();
        complete_parent(
          &snapshot,
          &mut history,
          &phase,
          &key("observed"),
          receipt.result.digest().map_err(invalid)?,
          at,
        )?;
        let triage = triage_run(&snapshot)?;
        let cycle = cycle_for(&history, &triage)?;
        let disposition = TriageDisposition::Eligibility(receipt.decision.clone());
        let (schema, output_digest) = if receipt.decision.outcome == EligibilityOutcome::Classify {
          let next = policy
            .classification_input(
              receipt.input.clone(),
              receipt.result.clone(),
              &receipt.evidence,
              receipt.usage,
            )
            .map_err(invalid)?;
          (
            TriageSchema::ClassificationInput.reference().map_err(invalid)?,
            next.digest().map_err(invalid)?,
          )
        } else {
          (
            TriageSchema::Decision.reference().map_err(invalid)?,
            payload_digest(&disposition)?,
          )
        };
        gate(
          &snapshot,
          &mut history,
          &triage,
          &cycle,
          TriageGate {
            node: TriageNode::EligibilityPolicy,
            input: receipt.result.digest().map_err(invalid)?,
            output: TypedFlowOutput {
              outcome: key(receipt.decision.outcome.as_str()),
              schema,
              digest: output_digest,
              usage: BudgetUsage::default(),
            },
          },
          at,
        )?;
        let resolution = if receipt.decision.outcome == EligibilityOutcome::Classify {
          enter(
            &snapshot,
            &mut history,
            &triage,
            &cycle,
            &TriageNode::Classification.key(),
            output_digest,
          )?;
          None
        } else {
          complete_parent(
            &snapshot,
            &mut history,
            &triage,
            &key(receipt.decision.outcome.as_str()),
            output_digest,
            at,
          )?;
          Some(disposition)
        };
        let mut append = FactoryRunHistoryAppend {
          flow: difference(&snapshot.flow, &history),
          ..Default::default()
        };
        append
          .flow
          .triage
          .push(TriageJournalRecord::Eligibility(Box::new(receipt)));
        self.commit(&snapshot, append, resolution.as_ref(), at).await?;
        Ok(resolution.map_or(FactoryTriageStep::Advanced, FactoryTriageStep::Resolved))
      }
      Some(TriageJournalRecord::Eligibility(receipt)) if receipt.decision.outcome == EligibilityOutcome::Classify => {
        let receipt = receipt.as_ref().clone();
        let input = policy
          .classification_input(
            receipt.input.clone(),
            receipt.result.clone(),
            &receipt.evidence,
            receipt.usage,
          )
          .map_err(invalid)?;
        let phase = phase_run(&snapshot, TriageNode::Classification)?;
        let result = self
          .execution
          .observe_or_dispatch(FactoryTriagePhaseRequest {
            snapshot: snapshot.clone(),
            flow_run: phase.clone(),
            input: FactoryTriagePhaseInput::Classification(Box::new(input.clone())),
            execution: flow.triage.classification.clone(),
          })
          .await?;
        let Some(FactoryTriagePhaseResult::Classification(result)) = result else {
          return if result.is_none() {
            Ok(FactoryTriageStep::Waiting)
          } else {
            Err(ApplicationError::invalid())
          };
        };
        let result = *result;
        let snapshot = self.store.factory_run_snapshot(run_id).await?;
        if snapshot.current_claim.as_ref().is_none_or(|row| row.id != claim_id) || cancellation(&snapshot)? {
          return Err(ApplicationError::invalid());
        }
        verify_observation(
          &snapshot,
          &phase,
          result.provenance(),
          result.digest().map_err(invalid)?,
        )?;
        let evidence = self.evidence.classification_evidence(&input, &result).await?;
        let usage = current_usage(&snapshot)?;
        let decision = policy
          .route(&input, &result, &receipt.evidence, &evidence, usage)
          .map_err(invalid)?;
        let row = ClassificationReceipt {
          eligibility: receipt,
          result,
          evidence,
          usage,
          decision,
        };
        let disposition = TriageDisposition::Classification(row.decision.clone());
        let digest = payload_digest(&disposition)?;
        let mut history = snapshot.flow.clone();
        complete_parent(
          &snapshot,
          &mut history,
          &phase,
          &key("observed"),
          row.result.digest().map_err(invalid)?,
          at,
        )?;
        let triage = triage_run(&snapshot)?;
        let cycle = cycle_for(&history, &triage)?;
        gate(
          &snapshot,
          &mut history,
          &triage,
          &cycle,
          TriageGate {
            node: TriageNode::RoutingPolicy,
            input: row.result.digest().map_err(invalid)?,
            output: TypedFlowOutput {
              outcome: key(row.decision.route.as_str()),
              schema: TriageSchema::Decision.reference().map_err(invalid)?,
              digest,
              usage: BudgetUsage::default(),
            },
          },
          at,
        )?;
        complete_parent(
          &snapshot,
          &mut history,
          &triage,
          &key(row.decision.route.as_str()),
          digest,
          at,
        )?;
        let terminal = flow.successor(row.decision.route).is_none();
        let mut append = FactoryRunHistoryAppend {
          flow: difference(&snapshot.flow, &history),
          ..Default::default()
        };
        append
          .flow
          .triage
          .push(TriageJournalRecord::Classification(Box::new(row)));
        self
          .commit(&snapshot, append, terminal.then_some(&disposition), at)
          .await?;
        if terminal {
          Ok(FactoryTriageStep::Resolved(disposition))
        } else {
          self.publish_ready(run_id, flow).await
        }
      }
      Some(TriageJournalRecord::Eligibility(row)) => Ok(FactoryTriageStep::Resolved(TriageDisposition::Eligibility(
        row.decision.clone(),
      ))),
      Some(TriageJournalRecord::Classification(row)) => {
        if flow.successor(row.decision.route).is_some() {
          self.publish_ready(run_id, flow).await
        } else {
          Ok(FactoryTriageStep::Resolved(TriageDisposition::Classification(
            row.decision.clone(),
          )))
        }
      }
    }
  }

  async fn publish_ready(
    &self,
    run_id: FactoryRunId,
    configured: &FactoryFlowConfiguration,
  ) -> Result<FactoryTriageStep, ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(run_id).await?;
    if snapshot.run.state() != FactoryRunState::Admitted || cancellation(&snapshot)? {
      return Ok(FactoryTriageStep::Waiting);
    }
    let Some(TriageJournalRecord::Classification(row)) = snapshot.flow.triage.last() else {
      return Err(ApplicationError::invalid());
    };
    let route = row.decision.route;
    let node = configured
      .successor(route)
      .ok_or_else(ApplicationError::invalid)?
      .clone();
    let policy = configured
      .triage
      .pools
      .get(&route)
      .ok_or_else(ApplicationError::invalid)?
      .clone();
    let root = snapshot.admitted_flow.root_run();
    if snapshot
      .flow
      .attempts
      .iter()
      .any(|attempt| attempt.flow_run_id() == root.id() && attempt.node_key() == &node)
    {
      return Ok(FactoryTriageStep::Waiting);
    }
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(root.definition())
      .ok_or_else(ApplicationError::invalid)?;
    let budget = definition.node(&node).ok_or_else(ApplicationError::invalid)?.budget();
    let observed_dependencies = row
      .result
      .classification()
      .dependencies
      .iter()
      .map(|row| row.value().clone())
      .collect::<Vec<_>>();
    let mut dependencies = self
      .discovery
      .phase_dependencies(&snapshot.work, &observed_dependencies)
      .await?;
    dependencies.sort_by_key(|row| row.run_id);
    let mut observed = observed_dependencies
      .iter()
      .map(|row| (row.work_id, row.work_digest))
      .collect::<Vec<_>>();
    let mut resolved = dependencies
      .iter()
      .map(|row| (row.work_id, row.work_digest))
      .collect::<Vec<_>>();
    observed.sort();
    resolved.sort();
    if observed != resolved {
      return Err(ApplicationError::invalid());
    }
    for dependency in &dependencies {
      let target = self.store.factory_run_snapshot(dependency.run_id).await?;
      if target.work.id() != dependency.work_id
        || target.work.subject().project_id() != snapshot.work.subject().project_id()
        || phase_pool_work_digest(&target) != dependency.work_digest
      {
        return Err(ApplicationError::invalid());
      }
    }
    let input = PhasePoolInput {
      run_id,
      run_version: snapshot.run.version(),
      flow_run_id: root.id(),
      cycle_id: snapshot.admitted_flow.initial_cycle().id(),
      node,
      severity: *row.result.classification().severity.value(),
      project_priority: configured.triage.project_priority,
      phase_input_digest: payload_digest(&TriageDisposition::Classification(row.decision.clone()))?,
      capabilities: configured.triage.capabilities.get(&route).cloned().unwrap_or_default(),
      dependencies,
      reservation: BudgetUsage {
        attempts: budget.max_attempts(),
        elapsed_millis: budget.max_elapsed_millis(),
        tokens: budget.max_tokens(),
        cost_micro_units: budget.max_cost_micro_units(),
        output_bytes: budget.max_output_bytes(),
      },
    };
    self
      .store
      .publish_phase_ready(policy, input)
      .await
      .map(FactoryTriageStep::Ready)
      .map_err(Into::into)
  }

  pub(super) async fn commit(
    &self,
    snapshot: &FactoryRunSnapshot,
    append: FactoryRunHistoryAppend,
    resolution: Option<&TriageDisposition>,
    at: Timestamp,
  ) -> Result<(), ApplicationError> {
    commit_intake(self.store.as_ref(), snapshot, append, resolution, at).await
  }
}

/// Optional configured-Flow intake branch of the ordinary bounded Factory worker.
#[async_trait]
pub trait FactoryTriageCoordinator: Send + Sync {
  /// Advances one already-claimed Run through its pinned intake composition.
  async fn reconcile_intake(
    &self,
    run_id: FactoryRunId,
    claim_id: FactoryDigest,
    at: Timestamp,
  ) -> Result<FactoryTriageStep, ApplicationError>;
}

#[async_trait]
impl<S> FactoryTriageCoordinator for FactoryTriageRunner<S>
where
  S: FactoryRunStore + FactoryConfigurationStore + FactoryPhasePoolStore + 'static,
{
  async fn reconcile_intake(
    &self,
    run_id: FactoryRunId,
    claim_id: FactoryDigest,
    at: Timestamp,
  ) -> Result<FactoryTriageStep, ApplicationError> {
    self.run_once(run_id, claim_id, at).await
  }
}
