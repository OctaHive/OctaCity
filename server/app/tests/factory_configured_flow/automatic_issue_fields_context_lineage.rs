//! An accepted producer's original Context remains a dependency across later Issue baselines.
use super::*;

fn append_consumer_with_producer(fixture: &mut AutomaticFixture, producer_writes: bool) {
  let original = &fixture.admission.flow;
  let root = original.closure().definition(original.closure().root()).unwrap();
  let producer = &root.nodes()[0];
  let single = original.limits().budget();
  let budget = BudgetLimit::new(
    single.max_attempts() * 2,
    single.max_elapsed_millis() * 2,
    single.max_tokens() * 2,
    single.max_cost_micro_units() * 2,
    single.max_output_bytes() * 2,
  )
  .unwrap();
  let outcome = producer.outcomes()[0].key().clone();
  let mut wire = serde_json::to_value(producer).unwrap();
  wire["key"] = json!("summarize_analysis");
  wire["input_schema"] = json!(fixture.schema.reference());
  wire["input_binding"] = json!({"sources":{"analysis":{"source":"record","node":producer.key(),
    "outcome":outcome,"view":"payload","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/analysis"}}});
  let consumer: FlowNodeDefinition = serde_json::from_value(wire).unwrap();
  let mut producer_wire = serde_json::to_value(producer).unwrap();
  if !producer_writes {
    producer_wire["outcomes"][0]
      .as_object_mut()
      .unwrap()
      .remove("issue_fields");
    producer_wire["permissions"] = json!(FactoryPermissionSet::deny_all());
  }
  let producer: FlowNodeDefinition = serde_json::from_value(producer_wire).unwrap();

  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes: vec![producer.clone(), consumer.clone()],
    transitions: vec![
      FlowTransition::new(
        producer.key().clone(),
        outcome.clone(),
        FlowTransitionTarget::Node(consumer.key().clone()),
      ),
      FlowTransition::new(
        consumer.key().clone(),
        outcome,
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: FlowExecutionPolicy::new(
      budget,
      root.execution().permissions().clone(),
      root.execution().max_active_nodes(),
    )
    .unwrap(),
  })
  .unwrap();
  let run = FlowRun::root(&fixture.admission.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&run).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(
      original.limits().max_wip(),
      budget,
      original.limits().permissions().clone(),
    )
    .unwrap(),
    run,
    cycle,
    original.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(original.data_schemas().to_vec())
  .unwrap()
  .with_pool_settings(original.pool_settings().clone())
  .unwrap();
  fixture.input = FlowNodeInput::new(
    &fixture.admission.work,
    &flow,
    flow.root_run(),
    flow.initial_cycle(),
    fixture.input.node().clone(),
    fixture.input.payload().clone(),
  )
  .unwrap()
  .with_context(fixture.input.context().unwrap().clone(), vec![])
  .unwrap();
  fixture.admission.flow = flow;
}

// The child result returns through a real call record, whose own input contains no document.
fn nest_producer(fixture: &mut AutomaticFixture) {
  let original = &fixture.admission.flow;
  let root = original.closure().definition(original.closure().root()).unwrap();
  let producer = &root.nodes()[0];
  let returned = key("returned");
  let child = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: producer.input_schema().cloned(),
    entry: producer.key().clone(),
    nodes: vec![producer.clone()],
    transitions: vec![FlowTransition::new(
      producer.key().clone(),
      producer.outcomes()[0].key().clone(),
      FlowTransitionTarget::Terminal(returned.clone()),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      returned.clone(),
      fixture.schema.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      root.execution().budget(),
      FactoryPermissionSet::deny_all(),
      root.execution().max_active_nodes(),
    )
    .unwrap(),
  })
  .unwrap();
  let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("analyze_in_subflow"),
    kind: FlowNodeKind::SubflowCall,
    input_schema: producer.input_schema().cloned(),
    outcomes: vec![FlowOutcomeDefinition::new(
      returned.clone(),
      FlowOutcomeKind::Success,
      fixture.schema.reference().clone(),
    )],
    budget: root.execution().budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: Some(child.reference()),
  })
  .unwrap();
  let mut wire = serde_json::to_value(&root.nodes()[1]).unwrap();
  wire["input_binding"]["sources"]["analysis"]["node"] = json!(call.key());
  wire["input_binding"]["sources"]["analysis"]["outcome"] = json!(returned);
  let consumer: FlowNodeDefinition = serde_json::from_value(wire).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: call.key().clone(),
    nodes: vec![call.clone(), consumer.clone()],
    transitions: vec![
      FlowTransition::new(
        call.key().clone(),
        returned,
        FlowTransitionTarget::Node(consumer.key().clone()),
      ),
      FlowTransition::new(
        consumer.key().clone(),
        consumer.outcomes()[0].key().clone(),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let run = FlowRun::root(&fixture.admission.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&run).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition, child]).unwrap(),
    original.limits().clone(),
    run,
    cycle,
    original.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(original.data_schemas().to_vec())
  .unwrap()
  .with_pool_settings(original.pool_settings().clone())
  .unwrap();
  fixture.input = FlowNodeInput::new(
    &fixture.admission.work,
    &flow,
    flow.root_run(),
    flow.initial_cycle(),
    call.key().clone(),
    fixture.input.payload().clone(),
  )
  .unwrap()
  .with_context(fixture.input.context().unwrap().clone(), vec![])
  .unwrap();
  fixture.admission.flow = flow;
}

#[derive(Clone, Copy)]
enum Producer {
  Writing,
  ReadOnly,
  Subflow,
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_consumer_rejects_a_producers_context_removed_between_invocations() {
  assert_context_lineage(Producer::Writing).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_consumer_retains_context_from_a_non_writing_producer() {
  assert_context_lineage(Producer::ReadOnly).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_consumer_retains_context_returned_from_a_subflow() {
  assert_context_lineage(Producer::Subflow).await;
}

async fn assert_context_lineage(layout: Producer) {
  let producer_writes = matches!(layout, Producer::Writing);
  let nested = matches!(layout, Producer::Subflow);
  let (db, store, mut fixture, issue, claim) = fixture_options(|fixture| {
    append_consumer_with_producer(fixture, producer_writes);
    if nested {
      nest_producer(fixture);
    }
  })
  .await;
  let clock = Arc::new(FactoryTestClock(std::sync::atomic::AtomicI64::new(11)));
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let documents = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(
      ProjectTextIntake::new(
        store.clone(),
        Arc::new(DocumentBytes::default()),
        Arc::new(ExactRevisionResolver),
        policy,
        clock.clone(),
      )
      .with_issue_documents(store.clone()),
    ),
  );
  let document = documents
    .handle_command(
      &context(),
      PublishIssueDocumentCommand {
        submission: IssueDocumentSubmission {
          project_id: issue.project_id,
          issue_id: issue.id,
          name: "Producer context".parse().unwrap(),
          content: "The producer analyzes this exact document.".into(),
          idempotency_key: "producer-document".parse().unwrap(),
        },
      },
    )
    .await
    .unwrap();
  let reference = IssueMaterialReference::IssueDocument {
    project_id: issue.project_id,
    issue_id: issue.id,
    artifact: document.artifact.clone(),
  };
  let approved = store
    .edit_issue_materials(management_mutation(EditIssueMaterials {
      intent: EditIssueMaterialsIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        references: vec![reference.clone()],
      },
      idempotency_key: "producer-approved".parse().unwrap(),
      edited_at: at(11),
    }))
    .await
    .unwrap()
    .issue;
  let consumer_context = fixture.input.context().unwrap().clone();
  let root_target = FactoryNodeTarget {
    flow_run_id: fixture.admission.flow.root_run().id(),
    cycle_id: fixture.admission.flow.initial_cycle().id(),
    node: fixture.input.node().clone(),
  };
  let call_runner = FactoryNodeRunner::new(store.clone(), Arc::new(flow_contract::FakePending), clock.clone());
  if nested {
    let call_input = FlowNodeInput::new(
      &fixture.admission.work,
      &fixture.admission.flow,
      fixture.admission.flow.root_run(),
      fixture.admission.flow.initial_cycle(),
      fixture.input.node().clone(),
      fixture.input.payload().clone(),
    )
    .unwrap();
    assert_eq!(
      call_runner
        .run_once(
          fixture.admission.run.id(),
          claim.id,
          root_target.clone(),
          Some(call_input),
          at(11)
        )
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let snapshot = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    let child = snapshot.flow.runs.iter().find(|flow| flow.parent().is_some()).unwrap();
    let cycle = snapshot
      .flow
      .cycles
      .iter()
      .find(|cycle| cycle.flow_run_id() == child.id())
      .unwrap();
    let definition = snapshot.admitted_flow.closure().definition(child.definition()).unwrap();
    fixture.input = FlowNodeInput::new(
      &fixture.admission.work,
      &fixture.admission.flow,
      child,
      cycle,
      definition.entry().clone(),
      fixture.input.payload().clone(),
    )
    .unwrap()
    .with_context(consumer_context.clone(), vec![])
    .unwrap();
  }
  let subject = fixture.input.task_subject().unwrap();
  let mut entries = consumer_context.entries().to_vec();
  entries.push(
    ContextManifestEntry::new(
      ContextSourceKind::Specification,
      key("producer-document"),
      subject.clone(),
      FactoryContextReference::Artifact(document.artifact.clone()),
      document.artifact.content_digest(),
      document.artifact.encoded_size(),
      FactorySafeText::new("Explicit producer Context").unwrap(),
      FactoryDigest::from_bytes([220; 32]),
    )
    .unwrap(),
  );
  fixture.input = fixture
    .input
    .with_context(
      ContextManifest::new(
        ContextManifestId::generate(),
        subject,
        FactoryDigest::from_bytes([220; 32]),
        entries,
      )
      .unwrap(),
      vec![],
    )
    .unwrap();
  clock.0.store(12, std::sync::atomic::Ordering::SeqCst);
  let producer = analyze(&db, &store, &fixture, &claim, &clock).await;
  let after_producer = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
  if !producer_writes {
    assert!(
      !after_producer
        .outbox
        .iter()
        .any(|row| row.kind.as_str().starts_with("issue.fields.")),
      "read provenance cannot enable an Issue field effect"
    );
    let node = after_producer
      .admitted_flow
      .closure()
      .definition(producer.input().definition())
      .unwrap()
      .node(producer.input().node())
      .unwrap();
    assert_eq!(node.permissions(), &FactoryPermissionSet::deny_all());
    assert!(node.outcomes().iter().all(|outcome| outcome.issue_fields().is_none()));
  }

  let returned = if nested {
    clock.0.store(17, std::sync::atomic::Ordering::SeqCst);
    let FactoryNodeStep::Completed(record) = call_runner
      .run_once(fixture.admission.run.id(), claim.id, root_target.clone(), None, at(17))
      .await
      .unwrap()
    else {
      panic!("the child result must return before the document is removed");
    };
    assert_eq!(record.subflow_source().unwrap().digest(), producer.digest().unwrap());
    assert_eq!(record.payload(), producer.payload());
    assert!(record.input().context().is_none());
    Some(record)
  } else {
    None
  };

  let removed = store
    .edit_issue_materials(management_mutation(EditIssueMaterials {
      intent: EditIssueMaterialsIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: approved.version,
        references: vec![],
      },
      idempotency_key: "remove-before-consumer".parse().unwrap(),
      edited_at: at(18),
    }))
    .await
    .unwrap()
    .issue;
  let snapshot = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
  fixture.input = prepare_factory_flow_input(
    &snapshot,
    &FactoryNodeTarget {
      flow_run_id: root_target.flow_run_id,
      cycle_id: root_target.cycle_id,
      node: key("summarize_analysis"),
    },
  )
  .unwrap()
  .with_context(consumer_context, vec![])
  .unwrap();
  clock.0.store(20, std::sync::atomic::Ordering::SeqCst);
  let consumer_writer = writer(store.clone(), &fixture, clock.clone());
  let consumer = analyze(&db, &store, &fixture, &claim, &clock).await;
  assert_eq!(
    consumer.input().issue_field_baseline().unwrap().version,
    removed.version
  );
  assert_eq!(
    consumer.input().sources()[0].digest(),
    returned.as_deref().unwrap_or(&producer).digest().unwrap()
  );
  let summary = consumer_writer.dispatch_once(at(26)).await.unwrap();
  assert_eq!(
    (summary.delivered, summary.failed),
    (0, if producer_writes { 2 } else { 1 }),
    "B must retain A's missing document dependency, even though B captured a newer Issue baseline"
  );
  assert_eq!(
    producer.input().issue_field_baseline().unwrap().version,
    approved.version
  );
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), removed);
  let retained = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
  assert_eq!(retained.flow.data.records.len(), if nested { 3 } else { 2 });
  if let Some(returned) = returned {
    assert!(retained.flow.data.records.contains(returned.as_ref()));
  }
  assert!(retained.flow.data.records.contains(producer.as_ref()));
  assert!(retained.flow.data.records.contains(consumer.as_ref()));
  assert_eq!(retained.flow.data.build_intents.len(), 2);
  assert_eq!(
    retained
      .outbox
      .iter()
      .filter(|row| row.state == FactoryOutboxState::Failed)
      .filter(|row| row.failure == Some(FactoryOutboxFailure::Conflict))
      .count(),
    if producer_writes { 2 } else { 1 }
  );
  assert_eq!(consumer_writer.dispatch_once(at(27)).await.unwrap().claimed, 0);
  db.cleanup().await;
}
