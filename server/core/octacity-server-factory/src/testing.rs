//! Explicit graph fixture for the existing candidate/evidence capability contracts.
//! This is test data, never a default graph used by admission or production workers.
use crate::*;
use serde::{Deserialize, Serialize};

/// Frozen generic producing input for pure candidate/evidence fixture contracts.
/// Runtime and store fixtures use their actual admitted input instead.
#[must_use]
pub fn candidate_node_fixture(work: &WorkEnvelope) -> (FlowNodeInput, NodeAttempt) {
  let (_, _, _, input, node) =
    crate::flow_test_support::execution_fixture_for_work(FlowNodeKind::BuildCommand, work.clone(), vec![], None);
  (input, node)
}

/// Configured reasoning attempt with the exact context selected by a capability fixture.
/// This creates a real admitted graph and reconstructs its attempt after freezing context.
pub fn capability_node_fixture(work: &WorkEnvelope, context: Option<ContextManifest>) -> (FlowNodeInput, NodeAttempt) {
  let (_, admitted, _, mut input, original) =
    crate::flow_test_support::execution_fixture_for_work(FlowNodeKind::Reasoning, work.clone(), vec![], None);
  if let Some(context) = context {
    input = input.with_context(context, vec![]).unwrap();
  }
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(input.definition()).unwrap(),
    NodeAttemptInput {
      id: original.id(),
      node_key: original.node_key().clone(),
      node_kind: original.node_kind(),
      number: original.number(),
      input_digest: input.digest().unwrap(),
      budget: original.budget(),
      started_at: original.started_at(),
      deadline: original.deadline(),
      execution: original.execution().clone(),
      ownership: FactoryClaimOwnership::new(original.owner().clone(), original.claim()),
    },
  )
  .unwrap();
  (input, attempt)
}

/// Literal roles in historical configuration fixtures; never runtime dispatch types.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryStageKind {
  /// First candidate fixture operation.
  Implementation,
  /// Deterministic evidence fixture operation.
  Validation,
  /// Independent review fixture operation.
  Evaluation,
  /// Candidate replacement fixture operation.
  Rework,
}

/// Literal candidate capability selection used only by historical contract fixtures.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryStageDraft {
  /// Stable stage identity within the configuration version.
  pub key: FactoryKey,
  /// Program-owned stage kind.
  pub kind: FactoryStageKind,
  /// Alias of the selected exact Build Configuration.
  pub build_configuration: FactoryKey,
  /// Hard budget for one attempt of this stage.
  pub budget: BudgetLimit,
}

/// Attaches the contract fixture's declared graph before publishing configuration.
/// Tests of other flows replace this fixture with their own exact definition.
#[must_use]
pub fn with_contract_flow(
  draft: FactoryConfigurationDraft,
  stages: Vec<FactoryStageDraft>,
  choices: &FactoryConfigurationChoices,
) -> FactoryConfigurationDraft {
  with_contract_flow_with_rework_limit(draft, stages, choices, 0)
}

/// Declares a literal bounded loop in the fixture graph, without a global lifecycle policy.
#[must_use]
pub fn with_contract_flow_with_rework_limit(
  mut draft: FactoryConfigurationDraft,
  stages: Vec<FactoryStageDraft>,
  choices: &FactoryConfigurationChoices,
  max_rework_cycles: u16,
) -> FactoryConfigurationDraft {
  if draft.flow.is_some() {
    return draft;
  }
  let key = |name: &str| FactoryKey::new(name).unwrap();
  let data_schema = FlowDataSchema::new(key("contract.stage-result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let schema = data_schema.reference().clone();
  let outcome = |name: &str, kind| FlowOutcomeDefinition::new(key(name), kind, schema.clone());
  let nodes = stages
    .iter()
    .map(|stage| {
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: stage.key.clone(),
        kind: FlowNodeKind::TrustedAction,
        input_schema: Some(schema.clone()),
        outcomes: vec![
          outcome("succeeded", FlowOutcomeKind::Success),
          outcome("failed", FlowOutcomeKind::Failure),
          outcome("cancelled", FlowOutcomeKind::Failure),
          outcome("retryable", FlowOutcomeKind::Failure),
        ],
        budget: stage.budget,
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_action(
        FlowActionBinding::new(
          ImmutableReference::new(
            key("contract.capability"),
            key("v1"),
            FactoryDigest::from_bytes([71; 32]),
          ),
          stage.key.clone(),
          serde_json::json!({
            "kind": stage.kind,
            "build_configuration": choices.resolve_build_configuration(&stage.build_configuration).unwrap(),
          }),
        )
        .unwrap(),
      )
      .unwrap()
      .with_input_binding(
        FlowInputBinding::new(
          Default::default(),
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!(true),
          })
          .unwrap(),
        )
        .unwrap(),
      )
      .unwrap()
    })
    .collect::<Vec<_>>();
  let select = |kind| {
    stages
      .iter()
      .find(|stage| stage.kind == kind)
      .map(|stage| stage.key.clone())
  };
  let mut transitions = Vec::new();
  // The literal graph is fixture policy. No production code interprets these roles as edges.
  for (from, result, to) in [
    (
      FactoryStageKind::Implementation,
      "succeeded",
      FactoryStageKind::Validation,
    ),
    (FactoryStageKind::Validation, "succeeded", FactoryStageKind::Evaluation),
    (FactoryStageKind::Validation, "failed", FactoryStageKind::Rework),
    (FactoryStageKind::Evaluation, "failed", FactoryStageKind::Rework),
  ] {
    if let (Some(from), Some(to)) = (select(from), select(to)) {
      transitions.push(FlowTransition::new(from, key(result), FlowTransitionTarget::Node(to)));
    }
  }
  if let (Some(from), Some(to)) = (select(FactoryStageKind::Rework), select(FactoryStageKind::Validation)) {
    transitions.push(
      FlowTransition::repeated(
        from,
        key("succeeded"),
        FlowTransitionTarget::Node(to),
        max_rework_cycles,
        key("failed"),
      )
      .unwrap(),
    );
  }
  for node in &nodes {
    transitions.push(
      if Some(node.key().clone()) == select(FactoryStageKind::Implementation) {
        FlowTransition::repeated(
          node.key().clone(),
          key("retryable"),
          FlowTransitionTarget::Node(node.key().clone()),
          10,
          key("failed"),
        )
        .unwrap()
      } else {
        FlowTransition::new(
          node.key().clone(),
          key("retryable"),
          FlowTransitionTarget::Terminal(key("failed")),
        )
      },
    );
  }
  for node in &nodes {
    for result in ["succeeded", "failed", "cancelled"] {
      if !transitions
        .iter()
        .any(|edge| edge.predecessor() == node.key() && edge.outcome() == &key(result))
      {
        transitions.push(FlowTransition::new(
          node.key().clone(),
          key(result),
          FlowTransitionTarget::Terminal(key(result)),
        ));
      }
    }
  }
  let identity = FactoryDigest::content_sha256(&serde_json::to_vec(&(&draft, &nodes, &transitions)).unwrap());
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(uuid::Uuid::from_bytes(identity.as_bytes()[..16].try_into().unwrap())).unwrap(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.clone()),
    entry: select(FactoryStageKind::Implementation).unwrap(),
    nodes,
    transitions,
    terminals: ["succeeded", "failed", "cancelled"]
      .into_iter()
      .map(|name| FlowTerminalDefinition::new(key(name), schema.clone()))
      .collect(),
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      draft.hard_budget,
      FactoryPermissionSet::deny_all(),
      draft.wip_limits.max_active_nodes() as u16,
    )
    .unwrap(),
  })
  .unwrap();
  draft.flow = Some(
    FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      FlowAdmissionLimits::product_defaults(
        draft.wip_limits.max_active_nodes() as u16,
        draft.hard_budget,
        FactoryPermissionSet::deny_all(),
      )
      .unwrap(),
      vec![data_schema],
      Default::default(),
    )
    .unwrap(),
  );
  draft
}

/// Prepares ordinary retained input and execution for an explicit fixture node.
/// All business-role selection remains with the test caller, never the runtime.
#[allow(clippy::too_many_arguments)] // Explicit creation time keeps fixture execution budgets independent of worker leases.
pub fn contract_node_attempt(
  work: &WorkEnvelope,
  admitted: &AdmittedFlow,
  history: FlowRuntimeHistory<'_>,
  data: &FlowDataHistory,
  node: FactoryKey,
  id: NodeAttemptId,
  ownership: FactoryClaimOwnership,
  started_at: octacity_server_domain::Timestamp,
) -> Result<(FlowNodeInput, NodeAttempt), FactoryError> {
  let definition = admitted.closure().definition(admitted.root_run().definition()).unwrap();
  let declared = definition.node(&node).unwrap();
  let schema = admitted.data_schema(declared.input_schema().unwrap()).unwrap();
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work,
    admitted,
    flow: admitted.root_run(),
    cycle: admitted.initial_cycle(),
    node: node.clone(),
    schema,
    history,
    records: &data.records,
    inputs: &data.inputs,
    incoming: data.incoming.first(),
  })?;
  let number = NodeAttemptNumber::new(history.attempts.iter().map(|row| row.number().get()).max().unwrap_or(0) + 1)?;
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      started_at,
      id,
      node_key: node,
      node_kind: declared.kind(),
      number,
      input_digest: input.digest()?,
      budget: declared.budget(),
      deadline: octacity_server_domain::Timestamp::from_unix_millis(
        started_at.unix_millis() + i64::try_from(declared.budget().max_elapsed_millis()).unwrap(),
      )
      .unwrap(),
      execution: NodeExecutionIdentity::External(declared.action().unwrap().plugin().clone()),
      ownership,
    },
  )?;
  Ok((input, attempt))
}

/// Retains a typed fixture observation through the same result/completion seam as adapters.
pub fn contract_node_result(
  admitted: &AdmittedFlow,
  input: &FlowNodeInput,
  attempt: &NodeAttempt,
  outcome: FactoryKey,
  ownership: FactoryClaimOwnership,
  at: octacity_server_domain::Timestamp,
) -> Result<(FlowNodeRecord, NodeAttemptCompletion), FactoryError> {
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let schema = admitted
    .data_schema(
      definition
        .node(input.node())
        .unwrap()
        .outcome(&outcome)
        .unwrap()
        .schema(),
    )
    .unwrap();
  let record = FlowNodeRecord::new(
    input,
    attempt,
    definition,
    FlowRecordObservation {
      outcome,
      payload: FlowPayload::new(schema, serde_json::json!(true))?,
      producer: attempt.execution().clone(),
      observed_at: at,
    },
  )?;
  let completion = record.completion(attempt, definition, ownership, BudgetUsage::default(), at)?;
  Ok((record, completion))
}

/// Declares a pool and reservation on the explicit capability contract fixture.
/// This helper changes test configuration before publication, never admitted history.
#[must_use]
pub fn with_contract_pool(
  mut draft: FactoryConfigurationDraft,
  settings: FlowPoolSettings,
  reservation: BudgetLimit,
) -> FactoryConfigurationDraft {
  let flow = draft.flow.take().expect("explicit contract graph");
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  let nodes: Vec<FlowNodeDefinition> = root
    .nodes()
    .iter()
    .map(|node| {
      let mut wire = serde_json::to_value(node.clone().with_phase_pool(settings.policy.phase.clone())).unwrap();
      wire["budget"] = serde_json::to_value(reservation).unwrap();
      serde_json::from_value(wire).unwrap()
    })
    .collect();
  let identity = FactoryDigest::content_sha256(&serde_json::to_vec(&(root.reference(), &nodes, &settings)).unwrap());
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(uuid::Uuid::from_bytes(identity.as_bytes()[..16].try_into().unwrap())).unwrap(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes,
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  draft.flow = Some(
    FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      flow.limits,
      flow.data_schemas,
      std::collections::BTreeMap::from([(settings.policy.phase.clone(), settings)]),
    )
    .unwrap(),
  );
  draft
}
