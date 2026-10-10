use crate::*;
use std::collections::BTreeMap;

fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
}
fn history<'a>(
  admitted: &'a AdmittedFlow,
  attempts: &'a [NodeAttempt],
  completions: &'a [NodeAttemptCompletion],
) -> FlowRuntimeHistory<'a> {
  FlowRuntimeHistory {
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts,
    completions,
  }
}

#[test]
fn configured_node_contract_accepts_a_new_stage_shape_without_a_rust_stage_type() {
  let schema = FlowDataSchema::new(
    key("accessibility.audit"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (key("passed"), FlowFieldSchema::required(FlowValueSchema::Boolean)),
        (
          key("violations"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        ),
        (
          key("summary"),
          FlowFieldSchema::optional(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 200,
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let payload = FlowPayload::new(&schema, serde_json::json!({"passed": false, "violations": 3})).unwrap();
  assert_eq!(payload.schema(), schema.reference());
  assert_eq!(payload.value()["violations"], 3);
  for invalid in [
    serde_json::json!({"passed": true}),
    serde_json::json!({"passed": true, "violations": -1}),
    serde_json::json!({"passed": true, "violations": "0"}),
    serde_json::json!({"passed": true, "violations": 0, "accepted": true}),
    serde_json::json!({"passed": true, "violations": 0, "summary": "x".repeat(201)}),
  ] {
    assert!(FlowPayload::new(&schema, invalid).is_err());
  }
}

#[test]
fn configured_data_contract_bounds_and_identity_survive_restoration() {
  let shape = FlowValueSchema::Array {
    item: Box::new(FlowValueSchema::Enum {
      values: vec![serde_json::json!("pass"), serde_json::json!("needs_review")],
    }),
    min_items: 1,
    max_items: 3,
  };
  let schema = FlowDataSchema::new(key("team.findings"), key("v1"), shape.clone()).unwrap();
  let restored: FlowDataSchema = serde_json::from_slice(&serde_json::to_vec(&schema).unwrap()).unwrap();
  assert_eq!(restored, schema);
  let payload = FlowPayload::new(&schema, serde_json::json!(["needs_review", "pass"])).unwrap();
  let bytes = serde_json::to_vec(&payload).unwrap();
  assert_eq!(FlowPayload::restore(&bytes, &restored).unwrap(), payload);
  for value in [
    serde_json::json!([]),
    serde_json::json!(["pass", "pass", "pass", "pass"]),
    serde_json::json!(["accepted"]),
  ] {
    assert!(FlowPayload::new(&schema, value).is_err());
  }
  for invalid in [
    FlowValueSchema::Integer { minimum: 2, maximum: 1 },
    FlowValueSchema::String {
      min_bytes: 2,
      max_bytes: 1,
    },
    FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: MAX_FLOW_DATA_BYTES + 1,
    },
    FlowValueSchema::Array {
      item: Box::new(FlowValueSchema::Boolean),
      min_items: 0,
      max_items: MAX_FLOW_SCHEMA_ITEMS + 1,
    },
    FlowValueSchema::Enum {
      values: vec![serde_json::json!("pass"), serde_json::json!("pass")],
    },
    FlowValueSchema::Enum {
      values: vec![serde_json::json!({"unbounded": true})],
    },
  ] {
    assert!(FlowDataSchema::new(key("team.invalid"), key("v1"), invalid).is_err());
  }
  let mut deep = FlowValueSchema::Boolean;
  for _ in 0..=MAX_FLOW_SCHEMA_DEPTH {
    deep = FlowValueSchema::Array {
      item: Box::new(deep),
      min_items: 0,
      max_items: 1,
    };
  }
  assert!(FlowDataSchema::new(key("team.deep"), key("v1"), deep).is_err());
  let other = FlowDataSchema::new(key("team.other"), key("v1"), shape).unwrap();
  assert!(FlowPayload::restore(&bytes, &other).is_err());
  let mut changed = serde_json::to_value(&schema).unwrap();
  changed["shape"]["max_items"] = serde_json::json!(4);
  assert!(serde_json::from_value::<FlowDataSchema>(changed).is_err());
  assert!(FlowPayload::restore(&vec![b' '; MAX_FLOW_DATA_BYTES + 1], &schema).is_err());
}

pub(crate) fn node_fixture() -> (WorkEnvelope, AdmittedFlow, FlowDataSchema) {
  node_fixture_with_gate(None)
}
fn node_fixture_with_gate(binding: Option<FlowGateBinding>) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema) {
  node_fixture_with_bindings(binding, None)
}
pub(crate) fn node_fixture_with_bindings(
  binding: Option<FlowGateBinding>,
  input_binding: Option<FlowInputBinding>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema) {
  let work = crate::flow_test_support::work();
  let budget = BudgetLimit::new(16, 10000, 1000, 10000, 10000).unwrap();
  let limits = FlowAdmissionLimits::product_defaults(4, budget, FactoryPermissionSet::deny_all()).unwrap();
  let schema = FlowDataSchema::new(
    key("custom.observations"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("count"),
        FlowFieldSchema::required(FlowValueSchema::Integer {
          minimum: 0,
          maximum: 100,
        }),
      )]),
    },
  )
  .unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("accessibility_audit"),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap();
  let node = binding
    .map_or_else(|| Ok(node.clone()), |binding| node.clone().with_gate(binding))
    .unwrap();
  let node = input_binding
    .map_or_else(|| Ok(node.clone()), |binding| node.clone().with_input_binding(binding))
    .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: Some(schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("accessibility_audit"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    limits,
    root,
    cycle,
  )
  .unwrap();
  (work, admitted, schema)
}

#[test]
fn generic_node_input_freezes_work_schema_and_execution_identity() {
  let (work, admitted, schema) = node_fixture();
  let payload = FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    payload.clone(),
  )
  .unwrap();
  assert_eq!(input.work_id(), work.id());
  assert_eq!(input.subject(), work.subject());
  assert_eq!(input.configuration(), work.configuration());
  assert_eq!(input.payload(), &payload);
  let bytes = serde_json::to_vec(&input).unwrap();
  assert_eq!(
    FlowNodeInput::restore(
      &bytes,
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      input.digest().unwrap()
    )
    .unwrap(),
    input
  );
  let other_run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let other_flow = FlowRun::root(&other_run, admitted.closure().root()).unwrap();
  assert!(
    FlowNodeInput::new(
      &work,
      &admitted,
      &other_flow,
      admitted.initial_cycle(),
      key("accessibility_audit"),
      payload.clone()
    )
    .is_err()
  );
  assert!(
    FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      key("invented_stage"),
      payload
    )
    .is_err()
  );
  let mut altered = serde_json::to_value(&input).unwrap();
  altered["subject"]["base_revision"] = serde_json::json!("different-base");
  assert!(
    FlowNodeInput::restore(
      &serde_json::to_vec(&altered).unwrap(),
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      input.digest().unwrap()
    )
    .is_err()
  );
  altered = serde_json::to_value(&input).unwrap();
  altered["payload"]["value"]["count"] = serde_json::json!(4);
  assert!(
    FlowNodeInput::restore(
      &serde_json::to_vec(&altered).unwrap(),
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      input.digest().unwrap()
    )
    .is_err()
  );
  let again = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap();
  assert_eq!(input.digest().unwrap(), again.digest().unwrap());
  let limits = admitted.limits();
  let changed_limits = FlowAdmissionLimits::new(
    limits.max_depth(),
    limits.max_expanded_nodes(),
    limits.max_repeat_count(),
    limits.max_fan_out(),
    limits.max_wip() + 1,
    limits.budget(),
    limits.permissions().clone(),
  )
  .unwrap();
  let changed = AdmittedFlow::new(
    admitted.closure().clone(),
    changed_limits,
    admitted.root_run().clone(),
    admitted.initial_cycle().clone(),
  )
  .unwrap();
  assert!(
    FlowNodeInput::restore(
      &bytes,
      &work,
      &changed,
      changed.root_run(),
      changed.initial_cycle(),
      &schema,
      input.digest().unwrap()
    )
    .is_err()
  );
}

#[test]
fn generic_node_record_rejects_substituted_attempts_schemas_and_retained_bytes() {
  let (work, admitted, schema) = node_fixture();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
  let owner = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([81; 32])),
      at,
      octacity_server_domain::Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  );
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  let attempt_for = |input_digest| {
    NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: key("accessibility_audit"),
        node_kind: FlowNodeKind::DeterministicGate,
        number: NodeAttemptNumber::new(1).unwrap(),
        input_digest,
        budget: admitted.limits().budget(),
        deadline: owner.claim().expires_at(),
        ownership: owner.clone(),
        execution: NodeExecutionIdentity::BuiltIn,
      },
    )
    .unwrap()
  };
  let attempt = attempt_for(input.digest().unwrap());
  let observation = FlowRecordObservation {
    outcome: key("observed"),
    payload: FlowPayload::new(&schema, serde_json::json!({"count": 1})).unwrap(),
    producer: NodeExecutionIdentity::BuiltIn,
    observed_at: at,
  };
  let record = FlowNodeRecord::new(&input, &attempt, definition, observation.clone()).unwrap();
  assert_eq!(record.node_attempt_id(), attempt.id());
  assert_eq!(record.input(), &input);
  assert_eq!(record.payload().value()["count"], 1);
  let completion = record
    .completion(&attempt, definition, owner.clone(), BudgetUsage::default(), at)
    .unwrap();
  assert_eq!(completion.output_digest(), record.digest().unwrap());
  assert_eq!(completion.output_schema(), schema.reference());
  let bytes = serde_json::to_vec(&record).unwrap();
  assert_eq!(
    FlowNodeRecord::restore(&bytes, &input, &attempt, definition, &schema, &completion).unwrap(),
    record
  );
  assert!(
    FlowNodeRecord::new(
      &input,
      &attempt_for(FactoryDigest::from_bytes([82; 32])),
      definition,
      observation.clone()
    )
    .is_err()
  );
  let mut unknown = observation;
  unknown.outcome = key("invented_outcome");
  assert!(FlowNodeRecord::new(&input, &attempt, definition, unknown).is_err());
  let mut altered = serde_json::to_value(&record).unwrap();
  altered["payload"]["value"]["count"] = serde_json::json!(2);
  assert!(
    FlowNodeRecord::restore(
      &serde_json::to_vec(&altered).unwrap(),
      &input,
      &attempt,
      definition,
      &schema,
      &completion
    )
    .is_err()
  );
  assert!(
    FlowNodeRecord::restore(
      &bytes,
      &input,
      &attempt_for(input.digest().unwrap()),
      definition,
      &schema,
      &completion
    )
    .is_err()
  );
  for (field, value) in [
    ("flow_run_id", serde_json::to_value(FlowRunId::generate()).unwrap()),
    ("observed_at", serde_json::json!(11)),
  ] {
    let mut corrupted = serde_json::to_value(&completion).unwrap();
    corrupted[field] = value;
    let completion: NodeAttemptCompletion = serde_json::from_value(corrupted).unwrap();
    assert!(FlowNodeRecord::restore(&bytes, &input, &attempt, definition, &schema, &completion).is_err());
  }
}

#[test]
fn configured_gate_executes_to_a_fenced_generic_record_with_the_frozen_output_mapping() {
  let program = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_output_mapping(
      key("observed"),
      FlowDataMapping::new(FlowValueMapping::Object {
        fields: BTreeMap::from([(
          key("count"),
          FlowValueMapping::Constant {
            value: serde_json::json!(7),
          },
        )]),
      })
      .unwrap(),
    )
    .unwrap();
  let (work, admitted, schema) = node_fixture_with_gate(Some(program.binding().unwrap()));
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
  let owner = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([81; 32])),
      at,
      octacity_server_domain::Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  );
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("accessibility_audit"),
      node_kind: FlowNodeKind::DeterministicGate,
      number: NodeAttemptNumber::new(1).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: admitted.limits().budget(),
      deadline: owner.claim().expires_at(),
      ownership: owner.clone(),
      execution: NodeExecutionIdentity::BuiltIn,
    },
  )
  .unwrap();
  let record = program.record(&input, &attempt, definition, &schema, at).unwrap();
  assert_eq!(record.payload().value(), &serde_json::json!({"count": 7}));
  let completion = record
    .completion(&attempt, definition, owner, BudgetUsage::default(), at)
    .unwrap();
  assert_eq!(completion.outcome(), &key("observed"));
  assert_eq!(completion.output_digest(), record.digest().unwrap());
  let replay = FlowGateProgram::from_node(definition.node(&key("accessibility_audit")).unwrap()).unwrap();
  assert_eq!(
    replay.record(&input, &attempt, definition, &schema, at).unwrap(),
    record
  );
  let different = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_output_mapping(
      key("observed"),
      FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap(),
    )
    .unwrap();
  assert!(different.record(&input, &attempt, definition, &schema, at).is_err());
  assert!(
    FlowGateProgram::new(vec![], key("observed"))
      .unwrap()
      .record(&input, &attempt, definition, &schema, at)
      .is_err()
  );
}

#[test]
fn generic_node_input_freezes_subject_bound_context_without_forwarding_all_work_data() {
  let (work, admitted, schema) = node_fixture();
  let context_for = |subject: ExactSubject| {
    let subject = FactoryTaskSubject::Exact(subject);
    let artifact = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([85; 32]),
      20,
    )
    .unwrap();
    let entry = ContextManifestEntry::new(
      ContextSourceKind::Task,
      key("selected_input"),
      subject.clone(),
      FactoryContextReference::Artifact(artifact.clone()),
      artifact.content_digest(),
      artifact.encoded_size(),
      FactorySafeText::new("Configured node selection").unwrap(),
      FactoryDigest::from_bytes([86; 32]),
    )
    .unwrap();
    ContextManifest::new(
      ContextManifestId::generate(),
      subject,
      FactoryDigest::from_bytes([87; 32]),
      vec![entry],
    )
    .unwrap()
  };
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap();
  let context = context_for(work.subject().clone());
  let selected = input.clone().with_context(context.clone(), vec![]).unwrap();
  assert_eq!(selected.context(), Some(&context));
  assert_ne!(selected.digest().unwrap(), input.digest().unwrap());
  let bytes = serde_json::to_vec(&selected).unwrap();
  assert_eq!(
    FlowNodeInput::restore(
      &bytes,
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      selected.digest().unwrap()
    )
    .unwrap(),
    selected
  );
  assert!(
    FlowNodeInput::restore(
      &bytes,
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      input.digest().unwrap()
    )
    .is_err()
  );
  let foreign = ExactSubject::new(
    work.subject().project_id(),
    work.subject().repository_id(),
    octacity_server_domain::ImmutableRevision::new("other-base").unwrap(),
  );
  assert!(input.with_context(context_for(foreign), vec![]).is_err());
}

#[test]
fn generic_node_context_requires_exact_receipts_for_selected_repository_fragments() {
  let (work, admitted, schema) = node_fixture();
  let subject = FactoryTaskSubject::Exact(work.subject().clone());
  let reference = |name| ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([90; 32]));
  let range = FactoryRepositoryRange::new(
    work.subject().repository_id(),
    work.subject().base_revision().clone(),
    FactoryRepositoryPath::new("src/lib.rs").unwrap(),
    1,
    2,
  )
  .unwrap();
  let artifact = FactoryArtifactReference::new(
    octacity_server_domain::ArtifactId::generate(),
    FactoryDigest::from_bytes([91; 32]),
    20,
  )
  .unwrap();
  let receipt_for = |id| {
    RetrievalReceipt::new(
      id,
      subject.clone(),
      work.subject().base_revision().clone(),
      reference("index"),
      reference("embedding"),
      FactorySafeText::new("node context").unwrap(),
      reference("retrieval"),
      vec![RepositoryFragment::new(1, range.clone(), artifact.clone()).unwrap()],
      FactoryDigest::from_bytes([92; 32]),
    )
    .unwrap()
  };
  let receipt = receipt_for(RetrievalReceiptId::generate());
  let fragment = receipt.fragment_reference(1).unwrap();
  let selected = ContextManifestEntry::new(
    ContextSourceKind::RepositoryFragment,
    key("source"),
    subject.clone(),
    FactoryContextReference::repository_fragment(fragment.clone()),
    artifact.content_digest(),
    artifact.encoded_size(),
    FactorySafeText::new("Configured repository fragment").unwrap(),
    receipt.digest().unwrap(),
  )
  .unwrap();
  let task = work.artifacts().task();
  let task_entry = ContextManifestEntry::new(
    ContextSourceKind::Task,
    key("task"),
    subject.clone(),
    FactoryContextReference::Artifact(task.clone()),
    task.content_digest(),
    task.encoded_size(),
    FactorySafeText::new("Work task").unwrap(),
    FactoryDigest::from_bytes([93; 32]),
  )
  .unwrap();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject.clone(),
    FactoryDigest::from_bytes([94; 32]),
    vec![task_entry.clone(), selected],
  )
  .unwrap();
  let task_only = ContextManifest::new(
    ContextManifestId::generate(),
    subject.clone(),
    FactoryDigest::from_bytes([94; 32]),
    vec![task_entry],
  )
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap();
  let accepted = input
    .clone()
    .with_context(context.clone(), vec![receipt.clone()])
    .unwrap();
  let bytes = serde_json::to_vec(&accepted).unwrap();
  assert_eq!(
    FlowNodeInput::restore(
      &bytes,
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      accepted.digest().unwrap()
    )
    .unwrap(),
    accepted
  );
  assert!(input.clone().with_context(context.clone(), vec![]).is_err());
  assert!(
    input
      .clone()
      .with_context(context.clone(), vec![receipt.clone(), receipt.clone()])
      .is_err()
  );
  assert!(
    input
      .clone()
      .with_context(context, vec![receipt_for(RetrievalReceiptId::generate())])
      .is_err()
  );
  assert!(input.with_context(task_only, vec![receipt]).is_err());
}

#[test]
fn configured_node_prepares_its_input_from_explicit_work_sources_and_mapping() {
  let binding = FlowInputBinding::new(
    BTreeMap::from([(
      key("selected"),
      FlowInputSource::Work {
        path: "/priority".into(),
      },
    )]),
    FlowDataMapping::new(FlowValueMapping::Object {
      fields: BTreeMap::from([(
        key("count"),
        FlowValueMapping::Pointer {
          path: "/selected".into(),
        },
      )]),
    })
    .unwrap(),
  )
  .unwrap();
  let (work, admitted, schema) = node_fixture_with_bindings(None, Some(binding.clone()));
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work: &work,
    admitted: &admitted,
    flow: admitted.root_run(),
    cycle: admitted.initial_cycle(),
    node: key("accessibility_audit"),
    schema: &schema,
    history: history(&admitted, &[], &[]),
    records: &[],
    inputs: &[],
    incoming: None,
  })
  .unwrap();
  assert_eq!(input.payload().value(), &serde_json::json!({"count": 1}));
  assert_eq!(input.work_id(), work.id());
  let node = admitted
    .closure()
    .definition(admitted.closure().root())
    .unwrap()
    .node(input.node())
    .unwrap();
  assert_eq!(node.input_binding(), Some(&binding));
  let restored: FlowInputBinding = serde_json::from_slice(&serde_json::to_vec(&binding).unwrap()).unwrap();
  assert_eq!(restored, binding);
  let other_schema = FlowDataSchema::new(
    key("wrong_input"),
    key("v1"),
    FlowValueSchema::Integer {
      minimum: 0,
      maximum: 100,
    },
  )
  .unwrap();
  assert!(
    FlowNodeInput::prepare(FlowInputPreparation {
      work: &work,
      admitted: &admitted,
      flow: admitted.root_run(),
      cycle: admitted.initial_cycle(),
      node: key("accessibility_audit"),
      schema: &other_schema,
      history: history(&admitted, &[], &[]),
      records: &[],
      inputs: &[],
      incoming: None,
    })
    .is_err()
  );
}

#[test]
fn configured_node_can_select_arbitrary_incoming_data_with_an_exact_retained_identity() {
  let incoming_schema = FlowDataSchema::new(
    key("external.intake"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("score"),
        FlowFieldSchema::required(FlowValueSchema::Integer {
          minimum: 0,
          maximum: 100,
        }),
      )]),
    },
  )
  .unwrap();
  let binding = FlowInputBinding::new(
    BTreeMap::from([(
      key("event"),
      FlowInputSource::Incoming {
        schema: incoming_schema.reference().clone(),
        path: "/score".into(),
      },
    )]),
    FlowDataMapping::new(FlowValueMapping::Object {
      fields: BTreeMap::from([(key("count"), FlowValueMapping::Pointer { path: "/event".into() })]),
    })
    .unwrap(),
  )
  .unwrap();
  let (work, admitted, schema) = node_fixture_with_bindings(None, Some(binding));
  let incoming = FlowIncomingData::new(
    &work,
    &admitted,
    FlowPayload::new(&incoming_schema, serde_json::json!({"score":42})).unwrap(),
  )
  .unwrap();
  let prepare = |data| {
    FlowNodeInput::prepare(FlowInputPreparation {
      work: &work,
      admitted: &admitted,
      flow: admitted.root_run(),
      cycle: admitted.initial_cycle(),
      node: key("accessibility_audit"),
      schema: &schema,
      history: history(&admitted, &[], &[]),
      records: &[],
      inputs: &[],
      incoming: data,
    })
  };
  let input = prepare(Some(&incoming)).unwrap();
  assert_eq!(input.payload().value(), &serde_json::json!({"count":42}));
  assert_eq!(input.incoming_digest(), Some(incoming.digest().unwrap()));
  assert!(prepare(None).is_err());
  let other_work = crate::flow_test_support::work();
  let unrelated = FlowIncomingData::new(&other_work, &admitted, incoming.payload().clone()).unwrap();
  assert!(prepare(Some(&unrelated)).is_err());
  let bytes = serde_json::to_vec(&incoming).unwrap();
  assert_eq!(
    FlowIncomingData::restore(&bytes, &work, &admitted, &incoming_schema, incoming.digest().unwrap()).unwrap(),
    incoming
  );
  let mut changed = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
  changed["payload"]["value"]["score"] = serde_json::json!(43);
  assert!(
    FlowIncomingData::restore(
      &serde_json::to_vec(&changed).unwrap(),
      &work,
      &admitted,
      &incoming_schema,
      incoming.digest().unwrap()
    )
    .is_err()
  );
}

#[test]
fn admitted_data_contracts_survive_restart_and_require_the_complete_exact_schema_set() {
  let (_, admitted, schema) = node_fixture();
  let admitted = admitted.with_data_schemas(vec![schema.clone()]).unwrap();
  assert_eq!(admitted.data_schema(schema.reference()), Some(&schema));
  let restored: AdmittedFlow = serde_json::from_slice(&serde_json::to_vec(&admitted).unwrap()).unwrap();
  restored.validated().unwrap();
  assert_eq!(restored.data_schema(schema.reference()), Some(&schema));
  assert!(
    admitted
      .clone()
      .with_data_schemas(vec![schema.clone(), schema])
      .is_err()
  );
  let wrong = FlowDataSchema::new(key("another.schema"), key("v1"), FlowValueSchema::Boolean).unwrap();
  assert!(admitted.with_data_schemas(vec![wrong]).is_err());
}

#[test]
fn a_configured_factory_journey_has_no_mandatory_intake_stage_and_freezes_its_data_catalogue() {
  let (work, admitted, schema) = node_fixture();
  let configured = FactoryFlowConfiguration::new(
    admitted.closure().clone(),
    admitted.limits().clone(),
    vec![schema.clone()],
    BTreeMap::new(),
  )
  .unwrap();
  configured
    .validate(
      work.configuration(),
      admitted.limits().budget(),
      FactoryWipLimits::new(4, 4).unwrap(),
    )
    .unwrap();
  let restored: FactoryFlowConfiguration = serde_json::from_slice(&serde_json::to_vec(&configured).unwrap()).unwrap();
  assert_eq!(restored, configured);
  assert_eq!(restored.data_schemas, vec![schema.clone()]);
  assert_eq!(
    restored
      .closure
      .definition(restored.closure.root())
      .unwrap()
      .entry()
      .as_str(),
    "accessibility_audit"
  );
  assert!(
    FactoryFlowConfiguration::new(
      admitted.closure().clone(),
      admitted.limits().clone(),
      vec![],
      BTreeMap::new()
    )
    .is_err()
  );
}

#[test]
fn configured_input_selects_only_committed_records_and_freezes_their_exact_references() {
  verify_record_source(
    FlowInputSource::Record {
      node: key("accessibility_audit"),
      outcome: key("observed"),
      path: "/count".into(),
    },
    9,
  );
}
#[test]
fn configured_input_can_follow_the_declared_predecessor_without_naming_a_business_phase() {
  verify_record_source(
    FlowInputSource::Predecessor {
      view: FlowRecordView::Payload,
      path: "/count".into(),
    },
    9,
  );
}

#[test]
fn configured_predecessor_metadata_exposes_owner_facts_separately_from_provider_fields() {
  verify_record_source(
    FlowInputSource::Predecessor {
      view: FlowRecordView::Metadata,
      path: "/observed_at".into(),
    },
    10,
  );
}
fn verify_record_source(source: FlowInputSource, expected: i64) {
  let (work, prior, schema) = node_fixture();
  let source_definition = prior.closure().definition(prior.closure().root()).unwrap();
  let source_node = source_definition.nodes()[0].clone();
  let binding = FlowInputBinding::new(
    BTreeMap::from([(key("selected"), source)]),
    FlowDataMapping::new(FlowValueMapping::Object {
      fields: BTreeMap::from([(
        key("count"),
        FlowValueMapping::Pointer {
          path: "/selected".into(),
        },
      )]),
    })
    .unwrap(),
  )
  .unwrap();
  let next = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("custom_follow_up"),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: source_node.outcomes().to_vec(),
    budget: source_node.budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(binding)
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: source_definition.reference().id(),
    version: source_definition.reference().version(),
    input_schema: source_definition.input_schema().cloned(),
    entry: source_node.key().clone(),
    nodes: vec![source_node.clone(), next.clone()],
    transitions: vec![
      FlowTransition::new(
        source_node.key().clone(),
        key("observed"),
        FlowTransitionTarget::Node(next.key().clone()),
      ),
      FlowTransition::new(
        next.key().clone(),
        key("observed"),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: source_definition.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: source_definition.execution().clone(),
  })
  .unwrap();
  let root = FlowRun::root(
    &FactoryRun::admitted(prior.root_run().factory_run_id(), &work),
    definition.reference(),
  )
  .unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    prior.limits().clone(),
    root,
    cycle,
  )
  .unwrap();
  let source_input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    source_node.key().clone(),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
  let owner = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([81; 32])),
      at,
      octacity_server_domain::Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  );
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    &definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: source_node.key().clone(),
      node_kind: source_node.kind(),
      number: NodeAttemptNumber::new(1).unwrap(),
      input_digest: source_input.digest().unwrap(),
      budget: source_node.budget(),
      deadline: owner.claim().expires_at(),
      ownership: owner.clone(),
      execution: NodeExecutionIdentity::BuiltIn,
    },
  )
  .unwrap();
  let record = FlowNodeRecord::new(
    &source_input,
    &attempt,
    &definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(&schema, serde_json::json!({"count": 9})).unwrap(),
      producer: NodeExecutionIdentity::BuiltIn,
      observed_at: at,
    },
  )
  .unwrap();
  let completion = record
    .completion(&attempt, &definition, owner, BudgetUsage::default(), at)
    .unwrap();
  let attempts = [attempt];
  let completions = [completion];
  let records = [record];
  let prepare = |completions: &[NodeAttemptCompletion], records: &[FlowNodeRecord]| {
    FlowNodeInput::prepare(FlowInputPreparation {
      work: &work,
      admitted: &admitted,
      flow: admitted.root_run(),
      cycle: admitted.initial_cycle(),
      node: next.key().clone(),
      schema: &schema,
      history: history(&admitted, &attempts, completions),
      records,
      inputs: &[],
      incoming: None,
    })
  };
  let input = prepare(&completions, &records).unwrap();
  assert_eq!(input.payload().value(), &serde_json::json!({"count": expected}));
  assert!(records[0].metadata().unwrap().get("input").is_none());
  assert!(records[0].metadata().unwrap().get("payload").is_none());
  assert_eq!(input.sources().len(), 1);
  assert_eq!(input.sources()[0].node_attempt_id(), attempts[0].id());
  assert_eq!(input.sources()[0].digest(), records[0].digest().unwrap());
  let bytes = serde_json::to_vec(&input).unwrap();
  assert_eq!(
    FlowNodeInput::restore(
      &bytes,
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      &schema,
      input.digest().unwrap()
    )
    .unwrap(),
    input
  );
  assert!(prepare(&[], &records).is_err());
  assert!(prepare(&completions, &[]).is_err());
  assert!(prepare(&completions, &[records[0].clone(), records[0].clone()]).is_err());
}

#[test]
fn configured_record_source_must_name_a_declared_node_and_outcome() {
  let (_, admitted, _) = node_fixture();
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  for (node, outcome) in [("unknown_node", "observed"), ("accessibility_audit", "unknown_outcome")] {
    let binding = FlowInputBinding::new(
      BTreeMap::from([(
        key("selected"),
        FlowInputSource::Record {
          node: key(node),
          outcome: key(outcome),
          path: String::new(),
        },
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/selected".into(),
      })
      .unwrap(),
    )
    .unwrap();
    let configured = definition.nodes()[0].clone().with_input_binding(binding).unwrap();
    assert!(
      FlowDefinition::new(FlowDefinitionInput {
        id: definition.reference().id(),
        version: definition.reference().version(),
        input_schema: definition.input_schema().cloned(),
        entry: definition.entry().clone(),
        nodes: vec![configured],
        transitions: definition.transitions().to_vec(),
        terminals: definition.terminals().to_vec(),
        context_projections: vec![],
        data_projections: vec![],
        execution: definition.execution().clone(),
      })
      .is_err()
    );
  }
}
