use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, BudgetUsage, FactoryClaimOwnership, FactoryConfiguration, FactoryDigest, FactoryError, FactoryKey,
  FactoryRun, FactoryRunId, FlowAdmissionLimits, FlowDefinition, FlowDefinitionRef, FlowNodeKind, FlowRunId,
  ImmutableReference, NodeAttemptId, NodeAttemptNumber, PinnedFlowDefinitionClosure, StageAttempt,
  StageAttemptCompletion, StageAttemptId, StageAttemptOutcome, WorkflowCycleId, WorkflowCycleNumber,
};

/// Immutable Flow Definition closure and initial execution records pinned at admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedFlow {
  closure: PinnedFlowDefinitionClosure,
  limits: FlowAdmissionLimits,
  root_run: FlowRun,
  initial_cycle: WorkflowCycle,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  triage: Option<crate::FactoryTriageConfiguration>,
}

impl AdmittedFlow {
  /// Binds a pinned closure to its root Flow Run and initial cycle.
  pub fn new(
    closure: PinnedFlowDefinitionClosure,
    limits: FlowAdmissionLimits,
    root_run: FlowRun,
    initial_cycle: WorkflowCycle,
  ) -> Result<Self, FactoryError> {
    closure.clone().validate(limits.clone())?;
    if root_run.definition() != closure.root()
      || root_run.parent().is_some()
      || initial_cycle.flow_run_id() != root_run.id()
      || initial_cycle.number() != WorkflowCycleNumber::INITIAL
      || initial_cycle.predecessor().is_some()
    {
      return Err(FactoryError::InvalidReference {
        relationship: "admitted Flow root",
      });
    }
    Ok(Self {
      closure,
      limits,
      root_run,
      initial_cycle,
      triage: None,
    })
  }

  /// Projects a fixed-stage configuration into the initial generic Flow records.
  pub fn from_stage_projection(configuration: &FactoryConfiguration, run: &FactoryRun) -> Result<Self, FactoryError> {
    if configuration.reference() != run.configuration() {
      return Err(FactoryError::InvalidReference {
        relationship: "Flow admission configuration",
      });
    }
    let closure = PinnedFlowDefinitionClosure::from_stage_projection(configuration)?;
    let root_definition = closure
      .definition(closure.root())
      .ok_or(FactoryError::InvalidReference {
        relationship: "admitted Flow root definition",
      })?;
    let limits = FlowAdmissionLimits::product_defaults(
      root_definition.execution().max_active_nodes(),
      root_definition.execution().budget(),
      root_definition.execution().permissions().clone(),
    )?;
    let root_run = FlowRun::root(run, closure.root())?;
    let initial_cycle = WorkflowCycle::initial(&root_run)?;
    Self::new(closure, limits, root_run, initial_cycle)
  }

  /// Admits the exact journey selected by the immutable configuration.
  pub fn from_configuration(configuration: &FactoryConfiguration, run: &FactoryRun) -> Result<Self, FactoryError> {
    if configuration.reference() != run.configuration() {
      return Err(FactoryError::InconsistentSubject);
    }
    let Some(flow) = configuration.flow() else {
      return Self::from_stage_projection(configuration, run);
    };
    flow.validate(
      configuration.reference(),
      configuration.hard_budget(),
      configuration.wip_limits(),
    )?;
    let root = FlowRun::root(run, flow.closure.root())?;
    let cycle = WorkflowCycle::initial(&root)?;
    let mut admitted = Self::new(flow.closure.clone(), flow.limits.clone(), root, cycle)?;
    admitted.triage = Some(flow.triage.clone());
    Ok(admitted)
  }

  /// Returns the pinned two-phase intake contract for a configured journey.
  #[must_use]
  pub const fn triage(&self) -> Option<&crate::FactoryTriageConfiguration> {
    self.triage.as_ref()
  }

  /// Returns the exact reachable definition closure.
  #[must_use]
  pub const fn closure(&self) -> &PinnedFlowDefinitionClosure {
    &self.closure
  }

  /// Returns the exact admission-wide ceilings retained with the Run.
  #[must_use]
  pub const fn limits(&self) -> &FlowAdmissionLimits {
    &self.limits
  }

  /// Revalidates the persisted closure against its original admission limits.
  pub fn validated(&self) -> Result<crate::ValidatedFlowDefinitionClosure, FactoryError> {
    self.closure.clone().validate(self.limits.clone())
  }

  /// Returns the root Flow Run.
  #[must_use]
  pub const fn root_run(&self) -> &FlowRun {
    &self.root_run
  }

  /// Returns the initial workflow cycle.
  #[must_use]
  pub const fn initial_cycle(&self) -> &WorkflowCycle {
    &self.initial_cycle
  }

  /// Projects one fixed-stage attempt into the pinned root Flow and cycle.
  pub fn project_stage(&self, stage: StageAttempt) -> Result<NodeAttempt, FactoryError> {
    let definition = self
      .closure
      .definitions()
      .iter()
      .find(|definition| definition.reference() == self.root_run.definition())
      .ok_or(FactoryError::InvalidReference {
        relationship: "root Flow Definition",
      })?;
    let node = definition
      .nodes()
      .iter()
      .find(|node| node.stage_projection() == Some(stage.kind()))
      .ok_or(FactoryError::InvalidReference {
        relationship: "stage projection node",
      })?;
    NodeAttempt::from_stage(stage, &self.root_run, &self.initial_cycle, node.key().clone())
  }

  /// Verifies the complete lossless generic projection of one fixed-stage attempt.
  #[must_use]
  pub fn matches_stage_projection(&self, stage: &StageAttempt, node: &NodeAttempt) -> bool {
    self
      .project_stage(stage.clone())
      .is_ok_and(|expected| expected == *node)
  }
}

/// Exact parent call that instantiated one nested Flow Run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRunParent {
  flow_run_id: FlowRunId,
  node_attempt_id: NodeAttemptId,
}

impl FlowRunParent {
  /// Constructs one exact parent-call relationship.
  #[must_use]
  pub const fn new(flow_run_id: FlowRunId, node_attempt_id: NodeAttemptId) -> Self {
    Self {
      flow_run_id,
      node_attempt_id,
    }
  }

  /// Returns the calling Flow Run.
  #[must_use]
  pub const fn flow_run_id(self) -> FlowRunId {
    self.flow_run_id
  }

  /// Returns the subflow-call Node Attempt.
  #[must_use]
  pub const fn node_attempt_id(self) -> NodeAttemptId {
    self.node_attempt_id
  }
}

/// One durable root or nested execution of an immutable Flow Definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRun {
  id: FlowRunId,
  factory_run_id: FactoryRunId,
  definition: FlowDefinitionRef,
  parent: Option<FlowRunParent>,
}

impl FlowRun {
  /// Creates the root Flow Run while retaining the Factory Run UUID.
  pub fn root(run: &FactoryRun, definition: FlowDefinitionRef) -> Result<Self, FactoryError> {
    Ok(Self {
      id: FlowRunId::from_uuid(run.id().as_uuid())?,
      factory_run_id: run.id(),
      definition,
      parent: None,
    })
  }

  /// Creates one nested Flow Run from an exact subflow call attempt.
  #[must_use]
  pub const fn nested(
    id: FlowRunId,
    factory_run_id: FactoryRunId,
    definition: FlowDefinitionRef,
    parent: FlowRunParent,
  ) -> Self {
    Self {
      id,
      factory_run_id,
      definition,
      parent: Some(parent),
    }
  }

  /// Returns this Flow Run identity.
  #[must_use]
  pub const fn id(&self) -> FlowRunId {
    self.id
  }

  /// Returns the enclosing Factory Run.
  #[must_use]
  pub const fn factory_run_id(&self) -> FactoryRunId {
    self.factory_run_id
  }

  /// Returns the exact pinned definition.
  #[must_use]
  pub const fn definition(&self) -> FlowDefinitionRef {
    self.definition
  }

  /// Returns the exact parent call for a nested run.
  #[must_use]
  pub const fn parent(&self) -> Option<FlowRunParent> {
    self.parent
  }
}

/// One append-only logical pass through a Flow Run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowCycle {
  id: WorkflowCycleId,
  flow_run_id: FlowRunId,
  number: WorkflowCycleNumber,
  predecessor: Option<WorkflowCycleId>,
}

impl WorkflowCycle {
  /// Creates the initial cycle while retaining the Flow Run UUID.
  pub fn initial(flow_run: &FlowRun) -> Result<Self, FactoryError> {
    Ok(Self {
      id: WorkflowCycleId::from_uuid(flow_run.id().as_uuid())?,
      flow_run_id: flow_run.id(),
      number: WorkflowCycleNumber::INITIAL,
      predecessor: None,
    })
  }

  /// Creates a later correction or bounded-repeat cycle.
  #[must_use]
  pub const fn next(
    id: WorkflowCycleId,
    flow_run_id: FlowRunId,
    number: WorkflowCycleNumber,
    predecessor: WorkflowCycleId,
  ) -> Self {
    Self {
      id,
      flow_run_id,
      number,
      predecessor: Some(predecessor),
    }
  }

  /// Returns the cycle identity.
  #[must_use]
  pub const fn id(&self) -> WorkflowCycleId {
    self.id
  }

  /// Returns the owning Flow Run.
  #[must_use]
  pub const fn flow_run_id(&self) -> FlowRunId {
    self.flow_run_id
  }

  /// Returns the monotonic cycle number.
  #[must_use]
  pub const fn number(&self) -> WorkflowCycleNumber {
    self.number
  }

  /// Returns the superseded cycle, when this is a later cycle.
  #[must_use]
  pub const fn predecessor(&self) -> Option<WorkflowCycleId> {
    self.predecessor
  }
}

/// Append-only execution identity for one node in one workflow cycle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeAttempt {
  id: NodeAttemptId,
  factory_run_id: FactoryRunId,
  flow_run_id: FlowRunId,
  workflow_cycle_id: WorkflowCycleId,
  node_key: FactoryKey,
  node_kind: FlowNodeKind,
  number: NodeAttemptNumber,
  input_digest: FactoryDigest,
  budget: BudgetLimit,
  deadline: Timestamp,
  execution: NodeExecutionIdentity,
  ownership: FactoryClaimOwnership,
  stage_projection_id: Option<StageAttemptId>,
}

/// Exact executor selected before one Node Attempt is dispatched.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "identity", deny_unknown_fields)]
pub enum NodeExecutionIdentity {
  /// Code-owned control evaluation without an external executor.
  BuiltIn,
  /// Exact immutable Build, reasoning, signal, or trusted-action provider identity.
  External(ImmutableReference),
  /// Lossless fixed-stage projection whose ordinary Build is linked separately.
  StageProjection(StageAttemptId),
}

impl NodeExecutionIdentity {
  pub(super) fn is_compatible(&self, kind: FlowNodeKind) -> bool {
    match kind {
      FlowNodeKind::BuildCommand => matches!(self, Self::External(_) | Self::StageProjection(_)),
      FlowNodeKind::Reasoning | FlowNodeKind::DecisionSignal | FlowNodeKind::TrustedAction => {
        matches!(self, Self::External(_))
      }
      FlowNodeKind::DeterministicGate
      | FlowNodeKind::FanOut
      | FlowNodeKind::Join
      | FlowNodeKind::HumanGate
      | FlowNodeKind::SubflowCall => matches!(self, Self::BuiltIn),
    }
  }
}

/// Immutable caller-selected fields for one generic Node Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeAttemptInput {
  /// Stable append-only attempt identity.
  pub id: NodeAttemptId,
  /// Node selected from the pinned definition.
  pub node_key: FactoryKey,
  /// Closed primitive kind repeated for indexed diagnostics.
  pub node_kind: FlowNodeKind,
  /// Positive attempt number for this node and cycle.
  pub number: NodeAttemptNumber,
  /// Digest of the complete immutable attempt input.
  pub input_digest: FactoryDigest,
  /// Hard budget inherited from the immutable node declaration.
  pub budget: BudgetLimit,
  /// Authoritative deadline after which execution must stop.
  pub deadline: Timestamp,
  /// Exact code-owned or external executor selected before dispatch.
  pub execution: NodeExecutionIdentity,
  /// Fenced owner that created this attempt.
  pub ownership: FactoryClaimOwnership,
}

impl NodeAttempt {
  /// Creates one generic attempt for a node declared by the pinned definition.
  pub fn new(
    flow_run: &FlowRun,
    cycle: &WorkflowCycle,
    definition: &FlowDefinition,
    input: NodeAttemptInput,
  ) -> Result<Self, FactoryError> {
    if flow_run.definition() != definition.reference()
      || cycle.flow_run_id() != flow_run.id()
      || definition
        .node(&input.node_key)
        .is_none_or(|node| node.kind() != input.node_kind || input.budget != node.budget())
      || input.deadline <= input.ownership.claim().claimed_at()
      || input.deadline > input.ownership.claim().expires_at()
      || !input.execution.is_compatible(input.node_kind)
    {
      return Err(FactoryError::InvalidReference {
        relationship: "Node Attempt definition",
      });
    }
    Ok(Self {
      id: input.id,
      factory_run_id: flow_run.factory_run_id(),
      flow_run_id: flow_run.id(),
      workflow_cycle_id: cycle.id(),
      node_key: input.node_key,
      node_kind: input.node_kind,
      number: input.number,
      input_digest: input.input_digest,
      budget: input.budget,
      deadline: input.deadline,
      execution: input.execution,
      ownership: input.ownership,
      stage_projection_id: None,
    })
  }

  /// Projects one Stage Attempt without changing its UUID, order, or payload.
  pub fn from_stage(
    stage: StageAttempt,
    flow_run: &FlowRun,
    cycle: &WorkflowCycle,
    node_key: FactoryKey,
  ) -> Result<Self, FactoryError> {
    if stage.run_id() != flow_run.factory_run_id() || cycle.flow_run_id() != flow_run.id() {
      return Err(FactoryError::InvalidReference {
        relationship: "Node Attempt flow",
      });
    }
    Ok(Self {
      id: NodeAttemptId::from_uuid(stage.id().as_uuid())?,
      factory_run_id: stage.run_id(),
      flow_run_id: flow_run.id(),
      workflow_cycle_id: cycle.id(),
      node_key,
      node_kind: FlowNodeKind::BuildCommand,
      number: NodeAttemptNumber::new(stage.number().get())?,
      input_digest: stage.input_digest(),
      budget: stage.budget(),
      deadline: stage.claim().expires_at(),
      execution: NodeExecutionIdentity::StageProjection(stage.id()),
      ownership: FactoryClaimOwnership::new(stage.owner().clone(), stage.claim()),
      stage_projection_id: Some(stage.id()),
    })
  }

  pub(super) fn accepts_completion_owner(&self, owner: &FactoryClaimOwnership) -> bool {
    (owner.owner() == self.owner() && owner.claim() == self.claim())
      || (self.stage_projection_id().is_none() && owner.claim().claimed_at() >= self.claim().expires_at())
  }

  /// Returns the losslessly preserved Node Attempt identity.
  #[must_use]
  pub const fn id(&self) -> NodeAttemptId {
    self.id
  }

  /// Returns the enclosing Factory Run.
  #[must_use]
  pub const fn factory_run_id(&self) -> FactoryRunId {
    self.factory_run_id
  }

  /// Returns the owning Flow Run.
  #[must_use]
  pub const fn flow_run_id(&self) -> FlowRunId {
    self.flow_run_id
  }

  /// Returns the owning workflow cycle.
  #[must_use]
  pub const fn workflow_cycle_id(&self) -> WorkflowCycleId {
    self.workflow_cycle_id
  }

  /// Returns the immutable node key selected by the pinned definition.
  #[must_use]
  pub const fn node_key(&self) -> &FactoryKey {
    &self.node_key
  }

  /// Returns the closed node kind.
  #[must_use]
  pub const fn node_kind(&self) -> FlowNodeKind {
    self.node_kind
  }

  /// Returns the append-only attempt number.
  #[must_use]
  pub const fn number(&self) -> NodeAttemptNumber {
    self.number
  }

  /// Returns the immutable input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the immutable node-attempt hard budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the immutable authoritative execution deadline.
  #[must_use]
  pub const fn deadline(&self) -> Timestamp {
    self.deadline
  }

  /// Returns the exact executor selected before dispatch.
  #[must_use]
  pub const fn execution(&self) -> &NodeExecutionIdentity {
    &self.execution
  }

  /// Returns the owner that created this attempt.
  #[must_use]
  pub const fn owner(&self) -> &FactoryKey {
    self.ownership.owner()
  }

  /// Returns the fenced claim under which this attempt was created.
  #[must_use]
  pub const fn claim(&self) -> crate::FactoryClaim {
    self.ownership.claim()
  }

  /// Returns the exact fixed-stage identity projected by this node, when any.
  #[must_use]
  pub const fn stage_projection_id(&self) -> Option<StageAttemptId> {
    self.stage_projection_id
  }
}

/// Immutable schema-validated terminal observation of one generic Node Attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeAttemptCompletion {
  id: FactoryDigest,
  node_attempt_id: NodeAttemptId,
  flow_run_id: FlowRunId,
  workflow_cycle_id: WorkflowCycleId,
  node_key: FactoryKey,
  outcome: FactoryKey,
  output_schema: ImmutableReference,
  output_digest: FactoryDigest,
  ownership: FactoryClaimOwnership,
  usage: BudgetUsage,
  observed_at: Timestamp,
}

/// Caller-owned fields for one immutable typed Node Attempt result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeAttemptCompletionInput {
  /// Finite outcome declared by the immutable node.
  pub outcome: FactoryKey,
  /// Exact schema used to validate the output.
  pub output_schema: ImmutableReference,
  /// Digest of the immutable output payload.
  pub output_digest: FactoryDigest,
  /// Fenced worker ownership that observed completion.
  pub ownership: FactoryClaimOwnership,
  /// Measured resource usage bounded by the node budget.
  pub usage: BudgetUsage,
  /// Authoritative observation time.
  pub observed_at: Timestamp,
}

impl NodeAttemptCompletion {
  /// Validates one typed outcome against the exact immutable node declaration.
  pub fn new(
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    input: NodeAttemptCompletionInput,
  ) -> Result<Self, FactoryError> {
    let NodeAttemptCompletionInput {
      outcome,
      output_schema,
      output_digest,
      ownership,
      usage,
      observed_at,
    } = input;
    let node = definition
      .node(attempt.node_key())
      .ok_or(FactoryError::InvalidReference {
        relationship: "Node Attempt completion node",
      })?;
    if node.kind() != attempt.node_kind()
      || node
        .outcome(&outcome)
        .is_none_or(|declared| declared.schema() != &output_schema)
      || !attempt.accepts_completion_owner(&ownership)
    {
      return Err(FactoryError::InvalidReference {
        relationship: "Node Attempt completion outcome",
      });
    }
    ownership.claim().verify_fence(ownership.claim().fence(), observed_at)?;
    usage.validate(attempt.budget())?;
    let id = FactoryDigest::sha256(
      "octacity.factory.node-attempt-completion.v1",
      &[
        attempt.id().as_uuid().as_bytes(),
        outcome.as_str().as_bytes(),
        &output_schema.digest().as_bytes(),
        &output_digest.as_bytes(),
        ownership.owner().as_str().as_bytes(),
        &ownership.claim().fence().digest().as_bytes(),
        &usage.attempts.to_be_bytes(),
        &usage.elapsed_millis.to_be_bytes(),
        &usage.tokens.to_be_bytes(),
        &usage.cost_micro_units.to_be_bytes(),
        &usage.output_bytes.to_be_bytes(),
        &observed_at.unix_millis().to_be_bytes(),
      ],
    );
    Ok(Self {
      id,
      node_attempt_id: attempt.id(),
      flow_run_id: attempt.flow_run_id(),
      workflow_cycle_id: attempt.workflow_cycle_id(),
      node_key: attempt.node_key().clone(),
      outcome,
      output_schema,
      output_digest,
      ownership,
      usage,
      observed_at,
    })
  }

  /// Projects one fixed-stage completion into its generic Node Attempt result.
  pub fn from_stage(
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    completion: &StageAttemptCompletion,
  ) -> Result<Self, FactoryError> {
    if attempt.stage_projection_id() != Some(completion.stage_attempt_id()) {
      return Err(FactoryError::InvalidReference {
        relationship: "Stage completion projection",
      });
    }
    let outcome = match completion.outcome() {
      StageAttemptOutcome::Succeeded => FactoryKey::new("succeeded")?,
      StageAttemptOutcome::Failed => FactoryKey::new("failed")?,
      StageAttemptOutcome::Cancelled => FactoryKey::new("cancelled")?,
    };
    let schema = definition
      .node(attempt.node_key())
      .and_then(|node| node.outcome(&outcome))
      .map(|outcome| outcome.schema().clone())
      .ok_or(FactoryError::InvalidReference {
        relationship: "Stage completion outcome",
      })?;
    Self::new(
      attempt,
      definition,
      NodeAttemptCompletionInput {
        outcome,
        output_schema: schema,
        output_digest: completion.id(),
        ownership: FactoryClaimOwnership::new(completion.owner().clone(), completion.claim()),
        usage: completion.usage(),
        observed_at: completion.observed_at(),
      },
    )
  }

  /// Returns the content-derived observation identity.
  #[must_use]
  pub const fn id(&self) -> FactoryDigest {
    self.id
  }
  /// Returns the completed generic attempt.
  #[must_use]
  pub const fn node_attempt_id(&self) -> NodeAttemptId {
    self.node_attempt_id
  }
  /// Returns the owning Flow Run.
  #[must_use]
  pub const fn flow_run_id(&self) -> FlowRunId {
    self.flow_run_id
  }
  /// Returns the owning workflow cycle.
  #[must_use]
  pub const fn workflow_cycle_id(&self) -> WorkflowCycleId {
    self.workflow_cycle_id
  }
  /// Returns the node key.
  #[must_use]
  pub const fn node_key(&self) -> &FactoryKey {
    &self.node_key
  }
  /// Returns the declared typed outcome.
  #[must_use]
  pub const fn outcome(&self) -> &FactoryKey {
    &self.outcome
  }
  /// Returns the exact output schema.
  #[must_use]
  pub const fn output_schema(&self) -> &ImmutableReference {
    &self.output_schema
  }
  /// Returns the immutable output digest.
  #[must_use]
  pub const fn output_digest(&self) -> FactoryDigest {
    self.output_digest
  }
  /// Returns the owner that observed this terminal result.
  #[must_use]
  pub const fn owner(&self) -> &FactoryKey {
    self.ownership.owner()
  }
  /// Returns the fenced claim that authorized this observation.
  #[must_use]
  pub const fn claim(&self) -> crate::FactoryClaim {
    self.ownership.claim()
  }
  /// Returns consumed budget.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }
  /// Returns the authoritative observation time.
  #[must_use]
  pub const fn observed_at(&self) -> Timestamp {
    self.observed_at
  }
}
