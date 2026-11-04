//! Result reporting compiles to ordinary independent gates/actions and exact caller inputs.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn draft() -> FactoryResultReportingDraft {
  let source = WorkSourceBinding {
    installation: reference("tracker"),
    capability: reference("source"),
    configuration: reference("reader"),
    resource: reference("tickets"),
  };
  let reporter = WorkSourceBinding {
    capability: reference("reporter"),
    configuration: reference("writer"),
    ..source.clone()
  };
  FactoryResultReportingDraft {
    target: FactoryReportingTarget {
      source, reporter,
      connection: serde_json::from_value(serde_json::json!({"id":octacity_server_domain::IntegrationId::generate(),
        "version":1,"access_version":1,"source_configuration":reference("reader"),"reporter_configuration":reference("writer")})).unwrap(),
      policy: WorkSourceReportingPolicy { ownership: None, labels: vec!["triage:ready".into(),"triage:rejected".into()], states:vec!["open".into(),"closed".into()] },
    },
    dimensions:vec![],
    outcomes: BTreeMap::from([
      (key("eligible"),Some(WorkSourceStatusEffects { status:"reviewed".into(),add:vec!["triage:ready".into()],remove:vec![],state:Some("open".into()) })),
      (key("rejection"),Some(WorkSourceStatusEffects { status:"rejected".into(),add:vec!["triage:rejected".into()],remove:vec![],state:Some("closed".into()) })),
    ]),
    receipt: FlowDataSchema::new(key("report.receipt"),key("v1"),FlowValueSchema::Object {fields:BTreeMap::from([
      (key("receipt"),FlowFieldSchema::required(FlowValueSchema::String {min_bytes:64,max_bytes:64}))
    ])}).unwrap(),
    action_outcome:key("queued"),
    budget:BudgetLimit::new(1,1000,0,0,1024).unwrap(),
    execution:FlowExecutionPolicy::new(BudgetLimit::new(64,60000,0,0,65536).unwrap(),permissions("reporter"),1).unwrap(),
    retry:FactoryReporterRetryPolicy {max_attempts:2,retry_delay_millis:5},
  }
}

#[test]
fn finite_result_reporting_compiles_reviewed_literal_effects_to_ordinary_actions() {
  let draft = draft();
  let schema = FlowDataSchema::new(key("result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  let published = draft
    .compile(
      &schema,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .unwrap();
  assert_eq!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|n| n.kind() == FlowNodeKind::TrustedAction)
      .count(),
    2
  );
  let plugin = FactoryWorkStatusAction::new(
    reference("reporter"),
    reporter(),
    FlowOutcomeDefinition::new(
      key("queued"),
      FlowOutcomeKind::Success,
      draft.receipt.reference().clone(),
    ),
  );
  for node in published
    .definition
    .nodes()
    .iter()
    .filter(|n| n.kind() == FlowNodeKind::TrustedAction)
  {
    let action = node.action().unwrap();
    plugin.validate(action, node.outcomes()).unwrap();
    let update: &serde_json::Value = &action.parameters()["source_update"];
    assert_eq!(update["reporter"], serde_json::json!(draft.target.reporter));
    let status = action.parameters()["status"].as_str().unwrap();
    let expected = draft.outcomes.values().flatten().find(|e| e.status == status).unwrap();
    assert_eq!(update["effects"], serde_json::json!(expected));
    let binding = serde_json::to_value(node.input_binding().unwrap()).unwrap();
    assert_eq!(binding["sources"]["accepted"]["source"], "caller_input");
    assert!(action.parameters().get("item_id").is_none());
  }
}

#[test]
fn reporting_rejects_effects_outside_the_installed_label_and_state_choices() {
  let schema = FlowDataSchema::new(key("result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  for field in ["label", "state"] {
    let mut draft = draft();
    let effects = draft.outcomes.get_mut(&key("eligible")).unwrap().as_mut().unwrap();
    if field == "label" {
      effects.add = vec!["not-installed".into()];
    } else {
      effects.state = Some("not-installed".into());
    }
    assert!(
      draft
        .compile(
          &schema,
          &outcomes,
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL
        )
        .is_err(),
      "missing installed {field} must block composition, never create labels or widen scope"
    );
  }
}

#[test]
fn reporting_requires_explicit_complete_outcome_coverage() {
  let schema = FlowDataSchema::new(key("result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  for change in ["missing", "unknown"] {
    let mut draft = draft();
    let effect = draft.outcomes.remove(&key("rejection")).unwrap();
    if change == "unknown" {
      draft.outcomes.insert(key("model-route"), effect);
    }
    assert!(
      draft
        .compile(
          &schema,
          &outcomes,
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL
        )
        .is_err(),
      "{change} outcome mapping cannot fall through silently or accept a model route"
    );
  }
}

#[test]
fn explicit_no_effect_outcome_continues_without_a_status_action() {
  let mut draft = draft();
  draft.outcomes.insert(key("rejection"), None);
  let result = FlowDataSchema::new(key("result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  let published = draft
    .compile(
      &result,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .unwrap();
  assert_eq!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|n| n.kind() == FlowNodeKind::TrustedAction)
      .count(),
    1
  );
  let entry = published.definition.node(published.definition.entry()).unwrap();
  let schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == entry.input_schema())
    .unwrap();
  let input = FlowPayload::new(schema, serde_json::json!({"outcome":"rejection","result":true})).unwrap();
  let outcome = FlowGateProgram::from_node(entry).unwrap().select(&input).unwrap();
  let destination = published
    .definition
    .transitions()
    .iter()
    .find(|edge| edge.predecessor() == entry.key() && edge.outcome() == &outcome)
    .unwrap()
    .target();
  assert_eq!(destination, &FlowTransitionTarget::Node(key("finish")));
}

fn dimensional_draft() -> FactoryResultReportingDraft {
  let mut draft = draft();
  for (name, values) in [
    (
      "component",
      vec!["server", "console", "agent", "infrastructure", "documentation", "other"],
    ),
    ("size", vec!["small", "medium", "large"]),
  ] {
    let cases = values
      .iter()
      .map(|value| {
        let label = format!("{name}:{value}");
        draft.target.policy.labels.push(label.clone());
        FactoryReportingCase {
          selection: FactoryReportingSelection::Literal {
            value: serde_json::json!(value),
          },
          effects: Some(WorkSourceStatusEffects {
            status: label.clone(),
            add: vec![label],
            remove: vec![],
            state: None,
          }),
        }
      })
      .collect();
    draft.dimensions.push(FactoryReportingDimension {
      key: key(name),
      path: vec![key("assessment"), key(name)],
      outcomes: BTreeSet::from([key("eligible")]),
      cases,
    });
  }
  let cases = [(0, 49, "priority:low"), (50, 100, "priority:high")]
    .into_iter()
    .map(|(minimum, maximum, label)| {
      draft.target.policy.labels.push(label.into());
      FactoryReportingCase {
        selection: FactoryReportingSelection::IntegerRange { minimum, maximum },
        effects: Some(WorkSourceStatusEffects {
          status: label.into(),
          add: vec![label.into()],
          remove: vec![],
          state: None,
        }),
      }
    })
    .collect();
  draft.dimensions.push(FactoryReportingDimension {
    key: key("priority"),
    path: vec![key("assessment"), key("priority_assessment")],
    outcomes: BTreeSet::from([key("eligible")]),
    cases,
  });
  // This installed test provider explicitly owns one scope per literal label.
  draft.target.policy.ownership = Some(octacity_server_factory::WorkSourceReportingOwnership {
    labels: draft
      .target
      .policy
      .labels
      .iter()
      .map(|label| (label.clone(), BTreeSet::from([key(&label.replace(':', "-"))])))
      .collect(),
    state: BTreeSet::from([key("native-state")]),
  });
  draft
}
fn classified_result() -> (FlowDataSchema, serde_json::Value) {
  let schema = crate::factory_node_test_support::fixtures::supplied_triage_template()
    .unwrap()
    .data_schemas()
    .unwrap()
    .remove(&key("summary"))
    .unwrap();
  let value = serde_json::json!({"assessment":{"priority_assessment":75,"work_kind":"feature_request","severity":"advisory",
    "size":"large","component":"server","risk":"low","dependencies":[],"recommended_route":"requirements","reproducibility":"not_applicable"},
    "admission_priority":20,"admission_risk":"low","accepted":true,"evidence":"absent",
    "conclusion":"A large server change needs requirements.","decision":"eligible","duplicate_coverage":"complete",
    "priority":75,"size":"large","component":"server"});
  (schema, value)
}
#[test]
fn reporting_rejects_distinct_labels_that_replace_the_same_installed_native_group() {
  let mut draft = dimensional_draft();
  draft.target.policy.ownership = Some(octacity_server_factory::WorkSourceReportingOwnership {
    labels: draft
      .target
      .policy
      .labels
      .iter()
      .map(|label| {
        let scope = if label.starts_with("component:") || label.starts_with("size:") {
          "shared-group".to_owned()
        } else {
          label.replace(':', "-")
        };
        (label.clone(), BTreeSet::from([key(&scope)]))
      })
      .collect(),
    state: BTreeSet::from([key("native-state")]),
  });
  let (result, _) = classified_result();
  assert!(
    draft
      .compile(
        &result,
        &BTreeSet::from([key("eligible"), key("rejection")]),
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL
      )
      .is_err(),
    "component and size labels replace one native group even though their literal names differ"
  );
}
#[test]
fn independent_reporting_dimensions_grow_linearly_and_select_only_reviewed_effects() {
  let draft = dimensional_draft();
  let (result, value) = classified_result();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  let published = draft
    .compile(
      &result,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .unwrap();
  assert_eq!(
    published.definition.nodes().len(),
    18,
    "one outcome gate, three dimension gates, thirteen finite actions and one finish"
  );
  let schema = published
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == published.definition.input_schema())
    .unwrap();
  for (name, label) in [
    ("component", "component:server"),
    ("size", "size:large"),
    ("priority", "priority:high"),
  ] {
    let gate = published.definition.node(&key(&format!("dimension-{name}"))).unwrap();
    let program = FlowGateProgram::from_node(gate).unwrap();
    let input = FlowPayload::new(schema, serde_json::json!({"outcome":"eligible","result":value})).unwrap();
    let selected = program.select(&input).unwrap();
    let edge = published
      .definition
      .transitions()
      .iter()
      .find(|e| e.predecessor() == gate.key() && e.outcome() == &selected)
      .unwrap();
    let FlowTransitionTarget::Node(action) = edge.target() else {
      panic!("a selected finite effect must name its declared action");
    };
    assert_eq!(
      published
        .definition
        .node(action)
        .unwrap()
        .action()
        .unwrap()
        .parameters()["source_update"]["effects"]["add"],
      serde_json::json!([label])
    );
    let rejected = FlowPayload::new(schema, serde_json::json!({"outcome":"rejection","result":value})).unwrap();
    assert_eq!(
      program.select(&rejected).unwrap(),
      key("skip"),
      "dimensions are limited to reviewed source outcomes"
    );
  }
}

#[test]
fn reporting_dimensions_require_complete_nonoverlapping_typed_partitions() {
  let (schema, _) = classified_result();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  for change in [
    "missing",
    "duplicate",
    "foreign_value",
    "gap",
    "overlap",
    "range",
    "type",
    "outcome",
    "path",
  ] {
    let mut draft = dimensional_draft();
    match change {
      "missing" => {
        draft.dimensions[0].cases.pop();
      }
      "duplicate" => {
        draft.dimensions[0].cases[1].selection = draft.dimensions[0].cases[0].selection.clone();
      }
      "foreign_value" => {
        draft.dimensions[0].cases[0].selection = FactoryReportingSelection::Literal {
          value: serde_json::json!("model-component"),
        }
      }
      "gap" => {
        draft.dimensions[2].cases[1].selection = FactoryReportingSelection::IntegerRange {
          minimum: 51,
          maximum: 100,
        }
      }
      "overlap" => {
        draft.dimensions[2].cases[1].selection = FactoryReportingSelection::IntegerRange {
          minimum: 49,
          maximum: 100,
        }
      }
      "range" => {
        draft.dimensions[2].cases[1].selection = FactoryReportingSelection::IntegerRange {
          minimum: 50,
          maximum: 101,
        }
      }
      "type" => {
        draft.dimensions[2].cases[1].selection = FactoryReportingSelection::Literal {
          value: serde_json::json!("high"),
        }
      }
      "outcome" => {
        draft.dimensions[0].outcomes.insert(key("unreviewed"));
      }
      _ => draft.dimensions[0].path = vec![key("private-context")],
    }
    assert!(
      draft
        .compile(
          &schema,
          &outcomes,
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL
        )
        .is_err(),
      "{change} cannot produce a partial or ambiguous dimension"
    );
  }
}

#[test]
fn reporting_configuration_blocks_missing_writer_authority_scope_and_budget() {
  let result = FlowDataSchema::new(key("result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  for change in ["permission", "budget", "scope"] {
    let mut draft = draft();
    match change {
      "permission" => {
        draft.execution =
          FlowExecutionPolicy::new(draft.execution.budget(), FactoryPermissionSet::deny_all(), 1).unwrap()
      }
      "budget" => draft.budget = BudgetLimit::new(1, 60001, 0, 0, 1024).unwrap(),
      _ => draft.target.reporter.resource = reference("another-repository"),
    }
    assert!(
      draft
        .compile(
          &result,
          &outcomes,
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL
        )
        .is_err(),
      "{change} must block configuration before execution"
    );
  }
}

fn source_template() -> FlowDefinitionTemplate {
  let mut template = crate::factory_node_test_support::fixtures::supplied_triage_template().unwrap();
  let (_, value) = classified_result();
  template.nodes.retain(|node| node.key == key("classification_gate"));
  let source = &mut template.nodes[0];
  source.key = key("reviewed-source");
  source.input_schema = key("source-input");
  source.input_binding = FlowInputBinding::new(
    BTreeMap::new(),
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!(true),
    })
    .unwrap(),
  )
  .unwrap();
  source.gate = Some(
    FlowGateProgram::new(vec![], key("eligible"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant { value }).unwrap())
      .unwrap(),
  );
  source.outcomes = ["eligible", "rejection"]
    .into_iter()
    .map(|name| FlowTemplateOutcome {
      key: key(name),
      kind: FlowOutcomeKind::Success,
      schema: key("summary"),
      public_result: true,
      issue_presentation: None,
    })
    .collect();
  source.budget = BudgetLimit::new(1, 1000, 0, 0, 1024).unwrap();
  template.schemas.insert(
    key("source-input"),
    FlowTemplateSchema {
      identity: key("source-input"),
      version: key("v1"),
      shape: FlowValueSchema::Boolean,
    },
  );
  template.entry = key("reviewed-source");
  template.input_schema = key("source-input");
  template.transitions = ["eligible", "rejection"]
    .into_iter()
    .map(|name| {
      FlowTransition::new(
        key("reviewed-source"),
        key(name),
        FlowTransitionTarget::Terminal(key("done")),
      )
    })
    .collect();
  template.terminals = BTreeMap::from([(key("done"), key("summary"))]);
  template.execution = FlowExecutionPolicy::new(
    BudgetLimit::new(256, 120000, 0, 0, 262144).unwrap(),
    permissions("reporter"),
    1,
  )
  .unwrap();
  template
}

#[test]
fn reporting_binds_every_action_chain_to_the_exact_source_record_and_preserves_results() {
  let draft = dimensional_draft();
  let template = source_template();
  let (result, _) = classified_result();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  let child = draft
    .compile(
      &result,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .unwrap();
  let bound = draft
    .bind(&template, &key("reviewed-source"), &child, &key("report-selection"))
    .unwrap();
  assert_eq!(
    template.nodes.len(),
    1,
    "pure draft composition cannot mutate its parent"
  );
  let published = bound
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::new(),
    )
    .unwrap();
  assert_eq!(
    published.definition.nodes().len(),
    3,
    "one exact caller per source outcome shares the same reporting graph"
  );
  for (outcome, index) in [("eligible", 0), ("rejection", 1)] {
    let call = published
      .definition
      .node(&key(&format!("report-selection-{index}")))
      .unwrap();
    assert_eq!(call.kind(), FlowNodeKind::SubflowCall);
    assert_eq!(call.subflow_definition(), Some(child.definition.reference()));
    let binding = serde_json::to_value(call.input_binding().unwrap()).unwrap();
    assert_eq!(binding["sources"]["result"]["source"], "record");
    assert_eq!(binding["sources"]["result"]["node"], "reviewed-source");
    assert_eq!(binding["sources"]["result"]["outcome"], outcome);
    assert_eq!(call.outcomes()[0].schema(), result.reference());
  }
  let mut schemas = published.schemas;
  for schema in child.schemas {
    if !schemas.iter().any(|s| s.reference() == schema.reference()) {
      schemas.push(schema);
    }
  }
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(
      published.definition.reference(),
      vec![published.definition, child.definition],
    )
    .unwrap(),
    FlowAdmissionLimits::product_defaults(1, bound.execution.budget(), bound.execution.permissions().clone()).unwrap(),
    schemas,
    BTreeMap::new(),
  )
  .unwrap();
}

#[test]
fn reporting_binding_refuses_parent_collisions_unsupported_routes_and_authority_expansion() {
  let draft = dimensional_draft();
  let (result, _) = classified_result();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  let child = draft
    .compile(
      &result,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .unwrap();
  for change in [
    "schema_collision",
    "node_collision",
    "fanout",
    "repeat",
    "permissions",
    "budget",
  ] {
    let mut template = source_template();
    match change {
      "schema_collision" => {
        let selected = template.schemas[&key("source-input")].clone();
        template.schemas.insert(key("report-selection-input"), selected);
      }
      "node_collision" => {
        let mut node = template.nodes[0].clone();
        node.key = key("report-selection-0");
        template.nodes.push(node);
      }
      "fanout" => {
        template.transitions.push(template.transitions[0].clone());
      }
      "repeat" => {
        template.transitions[0] = FlowTransition::repeated(
          key("reviewed-source"),
          key("eligible"),
          FlowTransitionTarget::Node(key("reviewed-source")),
          1,
          key("done"),
        )
        .unwrap()
      }
      "permissions" => {
        template.execution =
          FlowExecutionPolicy::new(template.execution.budget(), FactoryPermissionSet::deny_all(), 1).unwrap()
      }
      _ => {
        template.execution = FlowExecutionPolicy::new(
          BudgetLimit::new(1, 1000, 0, 0, 1024).unwrap(),
          permissions("reporter"),
          1,
        )
        .unwrap()
      }
    }
    let before = serde_json::to_vec(&template).unwrap();
    assert!(
      draft
        .bind(&template, &key("reviewed-source"), &child, &key("report-selection"))
        .is_err(),
      "{change} requires explicit review rather than overwriting or broadening the parent"
    );
    assert_eq!(serde_json::to_vec(&template).unwrap(), before);
  }
}

#[test]
fn reporting_execution_queues_independent_receipts_and_retains_exact_result_provenance() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let draft = dimensional_draft();
    let template = source_template();
    let (result, value) = classified_result();
    let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
    let child = draft
      .compile(
        &result,
        &outcomes,
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
      )
      .unwrap();
    let bound = draft
      .bind(&template, &key("reviewed-source"), &child, &key("report-selection"))
      .unwrap();
    let published = bound
      .instantiate(
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
        &BTreeMap::new(),
      )
      .unwrap();
    let mut schemas = published.schemas;
    for schema in child.schemas {
      if !schemas.iter().any(|s| s.reference() == schema.reference()) {
        schemas.push(schema);
      }
    }
    let (store, run, claim) = super::stage_reporting::support::admitted_closure_with_schemas(
      vec![published.definition, child.definition],
      schemas,
    )
    .await;
    let reporter = reporter();
    let action = FlowOutcomeDefinition::new(
      key("queued"),
      FlowOutcomeKind::Success,
      draft.receipt.reference().clone(),
    );
    let executor = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("reporter"),
      reporter.clone(),
      action,
    ))));
    let initial = store.factory_run_snapshot(run).await.unwrap();
    let mut finished = false;
    for tick in 11..160 {
      let owner = FactoryFlowRunner::new(
        store.clone(),
        executor.clone(),
        Arc::new(crate::factory_flow_tests::Inputs),
        crate::factory_node_test_support::clock(),
      );
      if owner.reconcile_flow(run, claim, at(tick)).await.unwrap() == FactoryFlowStep::Resolved(key("done")) {
        finished = true;
        break;
      }
    }
    assert!(
      finished,
      "the ordinary nested reporting graph must finish without contacting a provider"
    );
    assert!(reporter.requests.lock().unwrap().is_empty());
    let queued = store.factory_run_snapshot(run).await.unwrap();
    let source = queued
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("reviewed-source"))
      .unwrap();
    assert_eq!(source.payload().value(), &value);
    let caller = queued
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.node() == &key("report-selection-0"))
      .unwrap();
    assert_eq!(caller.sources().len(), 1);
    assert_eq!(caller.sources()[0].digest(), source.digest().unwrap());
    let actions: Vec<_> = queued
      .flow
      .data
      .records
      .iter()
      .filter(|record| {
        queued
          .admitted_flow
          .closure()
          .definition(record.input().definition())
          .unwrap()
          .node(record.input().node())
          .unwrap()
          .kind()
          == FlowNodeKind::TrustedAction
      })
      .collect();
    assert_eq!(
      actions.len(),
      4,
      "outcome, component, size and priority each enqueue a separate logical effect"
    );
    for action in &actions {
      assert_eq!(action.input().caller_input_digest(), Some(caller.digest().unwrap()));
      assert_eq!(action.input().payload().value()["result"], value);
      assert_eq!(action.outcome(), &key("queued"));
    }
    let operations: BTreeSet<_> = queued
      .outbox
      .iter()
      .filter(|row| row.kind == key("work.status.projection"))
      .map(|row| row.operation_id)
      .collect();
    assert_eq!(operations.len(), 4);
    let worker = FactoryWorkStatusOutboxWorker::new(
      store.clone(),
      BTreeMap::from([(
        reference("reporter"),
        reporter.clone() as Arc<dyn FactoryWorkStatusProjector>,
      )]),
      FactoryReporterWorkerSettings::new(key("result-reporter"), 100, 1).unwrap(),
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    assert_eq!(
      worker.run_once(at(200)).await.unwrap().delivered,
      1,
      "delivery is not atomic across independent effects"
    );
    let partial = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(partial.work, initial.work);
    assert_eq!(
      partial.flow.data.records, queued.flow.data.records,
      "partial delivery cannot rewrite an accepted result"
    );
    assert_eq!(
      partial
        .outbox
        .iter()
        .filter(|row| row.state == FactoryOutboxState::Delivered && operations.contains(&row.operation_id))
        .count(),
      1
    );
    assert_eq!(reporter.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn reporting_rejects_coactive_field_conflicts_but_allows_exclusive_effect_choices() {
  let (schema, _) = classified_result();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  for conflict in ["outcome_label", "dimension_label", "outcome_state", "dimension_state"] {
    let mut draft = dimensional_draft();
    match conflict {
      "outcome_label" => draft.dimensions[0].cases[0]
        .effects
        .as_mut()
        .unwrap()
        .remove
        .push("triage:ready".into()),
      "dimension_label" => draft.dimensions[1].cases[0]
        .effects
        .as_mut()
        .unwrap()
        .remove
        .push("component:server".into()),
      "outcome_state" => draft.dimensions[0].cases[0].effects.as_mut().unwrap().state = Some("closed".into()),
      _ => {
        for effect in draft.outcomes.values_mut().flatten() {
          effect.state = None;
        }
        draft.dimensions[0].cases[0].effects.as_mut().unwrap().state = Some("closed".into());
        draft.dimensions[1].cases[0].effects.as_mut().unwrap().state = Some("open".into());
      }
    }
    assert!(
      draft
        .compile(
          &schema,
          &outcomes,
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL
        )
        .is_err(),
      "{conflict} cannot create ordering-dependent effects"
    );
  }
  let mut exclusive = dimensional_draft();
  exclusive
    .outcomes
    .get_mut(&key("eligible"))
    .unwrap()
    .as_mut()
    .unwrap()
    .state = None;
  for case in &mut exclusive.dimensions[0].cases {
    case.effects.as_mut().unwrap().state = Some("open".into());
  }
  exclusive.dimensions[0].cases[0]
    .effects
    .as_mut()
    .unwrap()
    .remove
    .push("triage:rejected".into());
  exclusive
    .compile(
      &schema,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .expect("effects from exclusive source outcomes or alternatives may share managed fields");
}

#[test]
fn reporting_rejects_incompatible_receipts_and_graphs_above_native_limits() {
  let (schema, _) = classified_result();
  let outcomes = BTreeSet::from([key("eligible"), key("rejection")]);
  for shape in [
    FlowValueSchema::Boolean,
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("receipt"),
        FlowFieldSchema::required(FlowValueSchema::Enum {
          values: vec![serde_json::json!("0".repeat(64))],
        }),
      )]),
    },
  ] {
    let mut draft = dimensional_draft();
    draft.receipt = FlowDataSchema::new(key("incompatible-receipt"), key("v1"), shape).unwrap();
    assert!(
      draft
        .compile(
          &schema,
          &outcomes,
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL
        )
        .is_err(),
      "the configured receipt must accept every ordinary operation digest, not just a sample"
    );
  }
  let mut draft = dimensional_draft();
  let mut dimension = draft.dimensions[0].clone();
  for case in &mut dimension.cases {
    case.effects = None;
  }
  draft.dimensions = (0..MAX_FLOW_DEFINITION_NODES)
    .map(|index| {
      let mut selected = dimension.clone();
      selected.key = key(&format!("dimension-{index}"));
      selected
    })
    .collect();
  assert!(
    draft
      .compile(
        &schema,
        &outcomes,
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL
      )
      .is_err(),
    "linear expansion still obeys native graph bounds"
  );
}

#[test]
fn supplied_two_stage_triage_publishes_with_independently_bound_reporting_flows() {
  let mut template = crate::factory_node_test_support::fixtures::supplied_triage_template().unwrap();
  template.execution = FlowExecutionPolicy::new(
    BudgetLimit::new(1000, 360000, 40000, 4000000, 16777216).unwrap(),
    permissions("reporter"),
    1,
  )
  .unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let profiles = ["eligibility", "classification"]
    .into_iter()
    .map(|name| {
      let mut profile = crate::factory_node_test_support::fixtures::binding(project);
      profile.result_outcome = key("observed");
      profile.result_output = key("codex-run-result");
      (key(name), profile)
    })
    .collect::<BTreeMap<_, _>>();
  let mut children = vec![];
  for source in ["eligibility_gate", "classification_gate"] {
    let node = template.nodes.iter().find(|n| n.key == key(source)).unwrap();
    let schema = template
      .data_schemas()
      .unwrap()
      .remove(&node.outcomes[0].schema)
      .unwrap();
    let outcomes = node.outcomes.iter().map(|o| o.key.clone()).collect::<BTreeSet<_>>();
    let mut reporting = if source == "classification_gate" {
      dimensional_draft()
    } else {
      draft()
    };
    reporting.outcomes = outcomes
      .iter()
      .map(|name| {
        let effect = if name.as_str() == "classify" {
          None
        } else {
          let mut effect = reporting.outcomes[&key("eligible")].clone().unwrap();
          effect.status = name.to_string();
          Some(effect)
        };
        (name.clone(), effect)
      })
      .collect();
    for dimension in &mut reporting.dimensions {
      dimension.outcomes = outcomes
        .iter()
        .filter(|name| name.as_str() != "escalation")
        .cloned()
        .collect();
    }
    let child = reporting
      .compile(
        &schema,
        &outcomes,
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
      )
      .unwrap();
    template = reporting
      .bind(&template, &key(source), &child, &key(&format!("report-{source}")))
      .unwrap();
    children.push(child);
  }
  let parent = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert_eq!(
    parent
      .definition
      .nodes()
      .iter()
      .filter(|node| node.kind() == FlowNodeKind::Reasoning)
      .count(),
    2
  );
  for (name, profile) in &profiles {
    assert_eq!(parent.definition.node(name).unwrap().build(), Some(profile));
  }
  let root = parent.definition.reference();
  let mut definitions = vec![parent.definition];
  let mut schemas = parent.schemas;
  for child in children {
    definitions.push(child.definition);
    for schema in child.schemas {
      if !schemas.iter().any(|s| s.reference() == schema.reference()) {
        schemas.push(schema);
      }
    }
  }
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(root, definitions).unwrap(),
    FlowAdmissionLimits::product_defaults(1, template.execution.budget(), template.execution.permissions().clone())
      .unwrap(),
    schemas,
    BTreeMap::new(),
  )
  .unwrap();
}

#[test]
fn triage_reproduction_reporting_requests_only_the_mapped_addition_after_acceptance() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("../../config/factory/presets/triage/reproduction-reporting-v1.json");
  let mut dimension: FactoryReportingDimension =
    serde_json::from_slice(&std::fs::read(path).expect("triage must supply an opt-in reproduction Reporter mapping"))
      .unwrap();
  let template = crate::factory_node_test_support::fixtures::supplied_triage_template().unwrap();
  let classification = template
    .nodes
    .iter()
    .find(|node| node.key == key("classification_gate"))
    .unwrap();
  let outcomes: BTreeSet<_> = classification
    .outcomes
    .iter()
    .map(|outcome| outcome.key.clone())
    .collect();
  let (result, summary) = classified_result();
  let mut reporting = draft();
  reporting.outcomes = outcomes.iter().map(|outcome| (outcome.clone(), None)).collect();
  reporting.target.policy.labels = vec!["need reproduce".into()];
  reporting.dimensions = vec![dimension.clone()];
  let compile = |reporting: &FactoryResultReportingDraft| {
    reporting.compile(
      &result,
      &outcomes,
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
  };
  assert!(compile(&reporting).is_ok());
  reporting.target.policy.labels = vec!["repro: needed".into()];
  assert!(
    compile(&reporting).is_err(),
    "a label absent from the writer policy must be rejected"
  );
  for effects in dimension.cases.iter_mut().filter_map(|case| case.effects.as_mut()) {
    effects.add = vec!["repro: needed".into()];
  }
  reporting.dimensions = vec![dimension];
  let published = compile(&reporting).unwrap();
  let input_schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == published.definition.input_schema())
    .unwrap();
  for (outcome, reproduction, expected) in [
    ("defect_research", "unknown", true),
    ("defect_research", "needs_research", true),
    ("requirements", "not_reproduced", true),
    ("requirements", "reproduced", false),
    ("requirements", "not_applicable", false),
    ("escalation", "unknown", false),
    ("escalation", "needs_input", false),
  ] {
    let mut summary = summary.clone();
    if reproduction != "not_applicable" {
      summary["assessment"]["work_kind"] = "defect".into();
      summary["assessment"]["severity"] = "high".into();
    }
    if outcome == "escalation" {
      summary["accepted"] = false.into();
      summary["decision"] = "review".into();
    }
    summary["assessment"]["reproducibility"] = reproduction.into();
    let input = FlowPayload::new(input_schema, serde_json::json!({"outcome": outcome, "result": summary})).unwrap();
    let mut node = published.definition.node(published.definition.entry()).unwrap();
    let mut effects = Vec::new();
    loop {
      let selected = if let Some(action) = node.action() {
        effects.push(action.parameters()["source_update"]["effects"].clone());
        node.outcomes()[0].key().clone()
      } else {
        FlowGateProgram::from_node(node).unwrap().select(&input).unwrap()
      };
      let edge = published
        .definition
        .transitions()
        .iter()
        .find(|edge| edge.predecessor() == node.key() && edge.outcome() == &selected)
        .unwrap();
      let FlowTransitionTarget::Node(next) = edge.target() else {
        break;
      };
      node = published.definition.node(next).unwrap();
    }
    if expected {
      assert_eq!(effects.len(), 1, "{outcome}/{reproduction}");
      assert_eq!(effects[0]["add"], serde_json::json!(["repro: needed"]));
      assert_eq!(effects[0]["remove"], serde_json::json!([]));
      assert!(effects[0]["state"].is_null());
    } else {
      assert!(effects.is_empty(), "{outcome}/{reproduction}");
    }
  }
}

#[test]
fn delivery_reproduction_routes_one_build_and_reports_only_conclusive_checks() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/factory");
    let mut template = FlowDefinitionTemplate::from_manifest(
      &std::fs::read(root.join("presets/ticket-delivery/reproduction-v1.json"))
        .expect("ticket delivery supplies an ordinary reproduction branch"),
      |part| {
        std::fs::read(root.join(part.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
          field: "reproduction branch part",
        })
      },
    )
    .unwrap();
    use crate::factory::commands::tests::factory_command_output_source_tests::{
      configure_profile, retained_command_executor,
    };
    let contract = crate::factory_node_test_support::probe_observation_contract();
    let report = contract.report_schema().unwrap();
    template.schemas.insert(
      key("reproduction_report"),
      FlowTemplateSchema {
        identity: report.reference().identity().clone(),
        version: report.reference().version().clone(),
        shape: report.shape().clone(),
      },
    );
    let mut reporting = draft();
    reporting.target.policy.labels = vec!["need reproduce".into(), "can't reproduce".into()];
    reporting.execution = FlowExecutionPolicy::new(
      BudgetLimit::new(6, 5000, 0, 0, 65536).unwrap(),
      reporting.execution.permissions().clone(),
      1,
    )
    .unwrap();
    reporting.outcomes = serde_json::from_slice(
      &std::fs::read(root.join("presets/ticket-delivery/reproduction-effects-v1.json")).unwrap(),
    )
    .unwrap();
    let work = store_testing::factory_node_journal_contract_fixture().admission.work;
    let (_, _, mut command) =
      crate::factory::commands::tests::factory_command_tests::command_fixture_with_observation(Some(contract.clone()));
    configure_profile(&mut command, &work);
    let mut grants = serde_json::to_value(&command.project_permissions).unwrap();
    grants["plugins"]
      .as_array_mut()
      .unwrap()
      .push(serde_json::json!(reference("reporter")));
    template.execution =
      FlowExecutionPolicy::new(template.execution.budget(), serde_json::from_value(grants).unwrap(), 1).unwrap();
    template
      .nodes
      .iter_mut()
      .find(|node| node.key == key("reproduction"))
      .unwrap()
      .permissions = command.project_permissions.clone();
    let child = reporting
      .compile(
        &report,
        &[key("reproduced"), key("cannot_reproduce"), key("unresolved")].into(),
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
      )
      .unwrap();
    let template = reporting
      .bind(
        &template,
        &key("reproduction-result"),
        &child,
        &key("report-reproduction"),
      )
      .unwrap();
    let mut profile = crate::factory_node_test_support::fixtures::binding(work.subject().project_id());
    profile.build_configuration = command.build_configuration.clone();
    profile.task_digest = command.task_digest();
    profile.result_outcome = key("observed");
    for (kind, preliminary, finding, status, terminal, expected_effect) in [
      (
        "defect",
        "unknown",
        "matched",
        "completed",
        "reproduced",
        Some(serde_json::json!({"status":"reproduced","add":[],"remove":["need reproduce"],"state":null})),
      ),
      (
        "defect",
        "not_reproduced",
        "not_matched",
        "completed",
        "cannot_reproduce",
        Some(
          serde_json::json!({"status":"cannot-reproduce","add":["can't reproduce"],"remove":["need reproduce"],"state":"open"}),
        ),
      ),
      (
        // The routing graph remains safe even if an installed profile accepts indeterminate reports.
        "defect",
        "needs_research",
        "indeterminate",
        "environment_unavailable",
        "unresolved",
        None,
      ),
      (
        "feature_request",
        "not_applicable",
        "indeterminate",
        "environment_unavailable",
        "bypass",
        None,
      ),
      (
        "defect",
        "reproduced",
        "indeterminate",
        "environment_unavailable",
        "bypass",
        None,
      ),
      (
        "defect",
        "needs_input",
        "indeterminate",
        "environment_unavailable",
        "needs_input",
        None,
      ),
    ] {
      let mut selected = template.clone();
      selected.nodes[0].input_binding = FlowInputBinding::new(
        BTreeMap::new(),
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!({"work_kind":kind,"reproducibility":preliminary}),
        })
        .unwrap(),
      )
      .unwrap();
      let published = selected
        .instantiate(
          FlowDefinitionId::generate(),
          FlowDefinitionVersion::INITIAL,
          &BTreeMap::from([(key("reproduction"), profile.clone())]),
        )
        .unwrap();
      let mut schemas = published.schemas;
      for schema in &child.schemas {
        if !schemas.iter().any(|current| current.reference() == schema.reference()) {
          schemas.push(schema.clone());
        }
      }
      let (store, run, claim) = super::stage_reporting::support::admitted_closure_with_work(
        vec![published.definition, child.definition.clone()],
        schemas,
        work.clone(),
      )
      .await;
      let mut value = serde_json::json!({
        "subject":work.subject().repository().unwrap(),
        "command":profile.model_or_tool, "environment":contract.environment, "criterion":contract.criterion,
        "status":status, "finding":finding,
      });
      if status == "completed" {
        value["observations"] = serde_json::json!({"probe":"GET /health","requests":1,
          "unexpected_statuses":u8::from(finding == "matched")});
      }
      let reporter = reporter();
      let action = Arc::new(FactoryWorkStatusAction::new(
        reference("reporter"),
        reporter.clone(),
        FlowOutcomeDefinition::new(
          key("queued"),
          FlowOutcomeKind::Success,
          reporting.receipt.reference().clone(),
        ),
      ));
      let runner_for = |executor: Arc<dyn FactoryNodeExecutor>| {
        let executors = FactoryNodeExecutorRegistry::new(vec![
          (
            FlowNodeKind::BuildCommand,
            NodeExecutionIdentity::External(profile.tool.clone()),
            executor,
          ),
          (
            FlowNodeKind::TrustedAction,
            NodeExecutionIdentity::External(reference("reporter")),
            Arc::new(FactoryFlowActionAdapter::new(action.clone())),
          ),
        ])
        .unwrap();
        FactoryFlowRunner::new(
          store.clone(),
          Arc::new(executors),
          Arc::new(crate::factory_flow_tests::Inputs),
          crate::factory_node_test_support::clock(),
        )
      };
      let mut runner = runner_for(Arc::new(crate::factory_flow_tests::FakePending));
      let mut build_ready_at = None;
      let mut resolved = None;
      let pauses = matches!(terminal, "unresolved" | "needs_input");
      for tick in 11..160 {
        if build_ready_at.is_some_and(|ready| tick < ready) {
          continue;
        }
        if build_ready_at.is_none() {
          let snapshot = store.factory_run_snapshot(run).await.unwrap();
          if let Some(intent) = snapshot.flow.data.build_intents.first() {
            value["input_digest"] = serde_json::json!(intent.input().digest().unwrap());
            runner = runner_for(
              retained_command_executor(
                command.clone(),
                store.clone(),
                snapshot.admitted_flow,
                intent.clone(),
                value.clone(),
              )
              .await,
            );
            // Observe after the ordinary job and report publication have finished.
            build_ready_at = Some(intent.node().started_at().unix_millis() + 30);
            continue;
          }
        }
        let step = crate::factory_node_test_support::reconcile_admission_then_observe(
          &runner,
          store.as_ref(),
          run,
          claim,
          at(tick),
        )
        .await;
        let step = match step {
          Ok(step) => step,
          Err(error) => {
            let state = store.factory_run_snapshot(run).await.unwrap();
            let attempted: Vec<_> = state.flow.attempts.iter().map(|attempt| attempt.node_key()).collect();
            panic!("{kind}/{preliminary}/{finding} at {tick} after {attempted:?}: {error:?}");
          }
        };
        if let FactoryFlowStep::Resolved(outcome) = step {
          resolved = Some(outcome);
          break;
        }
        if pauses
          && step == FactoryFlowStep::Waiting
          && store
            .factory_run_snapshot(run)
            .await
            .unwrap()
            .flow
            .attempts
            .iter()
            .any(|attempt| attempt.node_kind() == FlowNodeKind::HumanGate)
        {
          break;
        }
      }
      assert_eq!(
        resolved,
        (!pauses).then(|| key(terminal)),
        "{kind}/{preliminary}/{finding}"
      );
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      if pauses {
        assert!(!snapshot.run.state().is_terminal());
        assert!(
          snapshot
            .flow
            .attempts
            .iter()
            .any(|attempt| attempt.node_kind() == FlowNodeKind::HumanGate)
        );
      }
      if terminal == "unresolved" {
        assert!(
          snapshot
            .accepted_terminal_result(
              snapshot.admitted_flow.closure().root(),
              &key("unresolved"),
              &[key("reported")].into(),
            )
            .unwrap()
            .is_none(),
          "a no-effect reporting call must preserve the source result's failure category"
        );
      }
      let expected_builds = usize::from(matches!(terminal, "reproduced" | "cannot_reproduce" | "unresolved"));
      assert_eq!(snapshot.flow.data.build_intents.len(), expected_builds);
      if expected_builds > 0 {
        let intent = &snapshot.flow.data.build_intents[0];
        assert_eq!(
          intent.input().work_task(),
          Some(work.artifacts().task()),
          "the Tester must receive the explicit original task selection"
        );
        assert_eq!(
          intent.input().payload().value(),
          &serde_json::json!(work.artifacts().task())
        );
        assert_eq!(intent.input().subject(), work.subject().repository().unwrap());
        let result = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|record| record.input().node() == &key("reproduction-result"))
          .unwrap();
        assert_eq!(result.payload().value(), &value);
        assert_eq!(
          result.payload().value()["input_digest"],
          intent.input().digest().unwrap().to_string(),
          "the report must be verified against the actual frozen Build input"
        );
      }
      let effects: Vec<_> = snapshot
        .flow
        .data
        .records
        .iter()
        .filter_map(|record| {
          let node = snapshot
            .admitted_flow
            .closure()
            .definition(record.input().definition())
            .unwrap()
            .node(record.input().node())
            .unwrap();
          node
            .action()
            .map(|action| action.parameters()["source_update"]["effects"].clone())
        })
        .collect();
      assert_eq!(effects, expected_effect.into_iter().collect::<Vec<_>>());
      assert_eq!(
        snapshot
          .outbox
          .iter()
          .filter(|row| row.kind == key("work.status.projection"))
          .count(),
        effects.len()
      );
      assert!(
        reporter.requests.lock().unwrap().is_empty(),
        "Reporter delivery is separate"
      );
      assert_eq!(
        runner.reconcile_flow(run, claim, at(200)).await.unwrap(),
        if pauses {
          FactoryFlowStep::Waiting
        } else {
          FactoryFlowStep::Resolved(key(terminal))
        }
      );
      assert_eq!(store.factory_run_snapshot(run).await.unwrap(), snapshot);
    }
  });
}
