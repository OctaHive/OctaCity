use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, BudgetUsage, FactoryClaimOwnership, FactoryConfiguration, FactoryDigest, FactoryError, FactoryKey,
  FactoryRun, FactoryRunId, FlowAdmissionLimits, FlowDefinition, FlowDefinitionRef, FlowNodeKind, FlowRunId,
  ImmutableReference, NodeAttemptId, NodeAttemptNumber, PinnedFlowDefinitionClosure, WorkflowCycleId,
  WorkflowCycleNumber,
};

/// Immutable Flow Definition closure and initial execution records pinned at admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedFlow {
  closure: PinnedFlowDefinitionClosure,
  limits: FlowAdmissionLimits,
  wip_limits: crate::FactoryWipLimits,
  root_run: FlowRun,
  initial_cycle: WorkflowCycle,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  data_schemas: Vec<crate::FlowDataSchema>,
  #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pools: std::collections::BTreeMap<FactoryKey, crate::FlowPoolSettings>,
}

impl AdmittedFlow {
  /// Binds a pinned closure to its root Flow Run and initial cycle.
  pub fn new(
    closure: PinnedFlowDefinitionClosure,
    limits: FlowAdmissionLimits,
    root_run: FlowRun,
    initial_cycle: WorkflowCycle,
    wip_limits: crate::FactoryWipLimits,
  ) -> Result<Self, FactoryError> {
    validate_wip_limits(&limits, wip_limits)?;
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
      wip_limits,
      root_run,
      initial_cycle,
      data_schemas: vec![],
      pools: Default::default(),
    })
  }

  /// Admits the exact journey selected by the immutable configuration.
  pub fn from_configuration(configuration: &FactoryConfiguration, run: &FactoryRun) -> Result<Self, FactoryError> {
    if configuration.reference() != run.configuration() {
      return Err(FactoryError::InconsistentSubject);
    }
    let flow = configuration.flow();
    flow.validate(
      configuration.reference(),
      configuration.hard_budget(),
      configuration.wip_limits(),
    )?;
    let root = FlowRun::root(run, flow.closure.root())?;
    let cycle = WorkflowCycle::initial(&root)?;
    let mut admitted = Self::new(
      flow.closure.clone(),
      flow.limits.clone(),
      root,
      cycle,
      configuration.wip_limits(),
    )?;
    if !flow.pools.is_empty() {
      admitted = admitted.with_pool_settings(flow.pools.clone())?;
    }
    if !flow.data_schemas.is_empty() {
      admitted = admitted.with_data_schemas(flow.data_schemas.clone())?;
    }
    Ok(admitted)
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

  /// Returns the configuration-wide capacity ceilings pinned at admission.
  #[must_use]
  pub const fn wip_limits(&self) -> crate::FactoryWipLimits {
    self.wip_limits
  }

  /// Revalidates the persisted closure against its original admission limits.
  pub fn validated(&self) -> Result<crate::ValidatedFlowDefinitionClosure, FactoryError> {
    validate_wip_limits(&self.limits, self.wip_limits)?;
    self.validate_data_schemas()?;
    crate::phase_pool::validate_pool_settings(&self.closure, &self.limits, &self.pools)?;
    self.closure.clone().validate(self.limits.clone())
  }

  /// Freezes the complete configured data catalogue before publishing admission.
  pub fn with_data_schemas(mut self, mut schemas: Vec<crate::FlowDataSchema>) -> Result<Self, FactoryError> {
    schemas.sort_by(|left, right| left.reference().cmp(right.reference()));
    self.data_schemas = schemas;
    self.validate_data_schemas()?;
    Ok(self)
  }
  /// Looks up an exact data contract without schema-name or version fallbacks.
  #[must_use]
  pub fn data_schema(&self, reference: &ImmutableReference) -> Option<&crate::FlowDataSchema> {
    self.data_schemas.iter().find(|schema| schema.reference() == reference)
  }
  /// Returns the immutable data catalogue selected before admission publication.
  #[must_use]
  pub fn data_schemas(&self) -> &[crate::FlowDataSchema] {
    &self.data_schemas
  }
  /// Freezes arbitrary pool policies and input mappings before admission publication.
  pub fn with_pool_settings(
    mut self,
    pools: std::collections::BTreeMap<FactoryKey, crate::FlowPoolSettings>,
  ) -> Result<Self, FactoryError> {
    crate::phase_pool::validate_pool_settings(&self.closure, &self.limits, &pools)?;
    self.pools = pools;
    Ok(self)
  }
  /// Returns exact configured pools without business-route dispatch.
  #[must_use]
  pub fn pool_settings(&self) -> &std::collections::BTreeMap<FactoryKey, crate::FlowPoolSettings> {
    &self.pools
  }
  fn validate_data_schemas(&self) -> Result<(), FactoryError> {
    validate_data_catalogue(&self.closure, &self.data_schemas)
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
  started_at: Timestamp,
  deadline: Timestamp,
  execution: NodeExecutionIdentity,
  ownership: FactoryClaimOwnership,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  retry_of: Option<NodeAttemptId>,
}

/// Exact executor selected before one Node Attempt is dispatched.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "identity", deny_unknown_fields)]
pub enum NodeExecutionIdentity {
  /// Code-owned control evaluation without an external executor.
  BuiltIn,
  /// Exact immutable Build, reasoning, signal, or trusted-action provider identity.
  External(ImmutableReference),
}

impl NodeExecutionIdentity {
  pub(super) fn is_compatible(&self, kind: FlowNodeKind) -> bool {
    match kind {
      FlowNodeKind::BuildCommand => matches!(self, Self::External(_)),
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
  /// Original start time, fenced at creation and independent of later worker leases.
  pub started_at: Timestamp,
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
      || !input.execution.is_compatible(input.node_kind)
    {
      return Err(FactoryError::InvalidReference {
        relationship: "Node Attempt definition",
      });
    }
    let attempt = Self {
      id: input.id,
      factory_run_id: flow_run.factory_run_id(),
      flow_run_id: flow_run.id(),
      workflow_cycle_id: cycle.id(),
      node_key: input.node_key,
      node_kind: input.node_kind,
      number: input.number,
      input_digest: input.input_digest,
      budget: input.budget,
      started_at: input.started_at,
      deadline: input.deadline,
      execution: input.execution,
      ownership: input.ownership,
      retry_of: None,
    };
    attempt.validate_execution_window()?;
    Ok(attempt)
  }

  /// Binds a new technical execution to the exact failed predecessor.
  /// Stop, policy and operator-intent checks belong to the complete journal transaction.
  pub fn with_retry_of(mut self, previous: &Self) -> Result<Self, FactoryError> {
    if self.factory_run_id != previous.factory_run_id
      || self.flow_run_id != previous.flow_run_id
      || self.workflow_cycle_id != previous.workflow_cycle_id
      || self.node_key != previous.node_key
      || self.node_kind != previous.node_kind
      || self.execution != previous.execution
      || self.budget != previous.budget
      || self.number <= previous.number
      || self.started_at < previous.started_at
    {
      return Err(FactoryError::InvalidReference {
        relationship: "Node execution retry predecessor",
      });
    }
    self.retry_of = Some(previous.id);
    Ok(self)
  }
  /// Failed execution retried by this attempt, independently of logical graph repeats.
  #[must_use]
  pub const fn retry_of(&self) -> Option<NodeAttemptId> {
    self.retry_of
  }

  pub(crate) fn validate_execution_window(&self) -> Result<(), FactoryError> {
    self.claim().verify_fence(self.claim().fence(), self.started_at)?;
    if self.deadline <= self.started_at
      || self
        .deadline
        .unix_millis()
        .checked_sub(self.started_at.unix_millis())
        .and_then(|elapsed| u64::try_from(elapsed).ok())
        .is_none_or(|elapsed| elapsed > self.budget.max_elapsed_millis())
    {
      return Err(FactoryError::InvalidReference {
        relationship: "Node Attempt execution window",
      });
    }
    Ok(())
  }

  pub(super) fn accepts_completion_owner(&self, owner: &FactoryClaimOwnership) -> bool {
    (owner.owner() == self.owner() && owner.claim().claimed_at() >= self.claim().claimed_at())
      || owner.claim().claimed_at() >= self.claim().expires_at()
  }

  /// Verifies a replacement fence for the same worker, or a successor after lease expiry.
  ///
  /// The durable Flow owner must also compare this claim with the current stored
  /// claim. This check prevents an unrelated overlapping lease from dispatching
  /// or observing this attempt; it does not grant readiness or renew its deadline.
  pub fn verify_observer(&self, owner: &FactoryClaimOwnership, at: Timestamp) -> Result<(), FactoryError> {
    if !self.accepts_completion_owner(owner) {
      return Err(FactoryError::InvalidReference {
        relationship: "Node Attempt observer",
      });
    }
    owner.claim().verify_fence(owner.claim().fence(), at)
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

  /// Original execution start, retained unchanged across successor observations.
  #[must_use]
  pub const fn started_at(&self) -> Timestamp {
    self.started_at
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

pub(crate) fn validate_data_catalogue(
  closure: &PinnedFlowDefinitionClosure,
  schemas: &[crate::FlowDataSchema],
) -> Result<(), FactoryError> {
  let invalid = || FactoryError::InvalidConfiguration {
    field: "admitted Flow data schemas",
  };
  if schemas.is_empty()
    || schemas.len() > crate::MAX_FLOW_SCHEMA_ENTRIES
    || schemas
      .windows(2)
      .any(|pair| pair[0].reference() >= pair[1].reference())
  {
    return Err(invalid());
  }
  for definition in closure.definitions() {
    for reference in definition
      .input_schema()
      .into_iter()
      .chain(definition.nodes().iter().flat_map(|node| {
        node
          .input_schema()
          .into_iter()
          .chain(node.outcomes().iter().map(|outcome| outcome.schema()))
      }))
      .chain(definition.terminals().iter().map(|terminal| terminal.schema()))
    {
      if !schemas.iter().any(|schema| schema.reference() == reference) {
        return Err(invalid());
      }
    }
  }
  Ok(())
}

fn validate_wip_limits(limits: &FlowAdmissionLimits, wip: crate::FactoryWipLimits) -> Result<(), FactoryError> {
  crate::FactoryWipLimits::new(wip.max_active_runs(), wip.max_active_nodes())?;
  if u32::from(limits.max_wip()) > wip.max_active_nodes() {
    return Err(FactoryError::InvalidConfiguration {
      field: "admitted Flow configuration WIP",
    });
  }
  Ok(())
}
