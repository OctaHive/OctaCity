//! Reviewed triage selections compile to ordinary closed Context bindings.
use octacity_server_application::{TriageContextProblem, TriageProjectContextDraft};
use octacity_server_domain::{ArtifactId, ProjectId};
use octacity_server_factory::{
  ContextSourceKind, FactoryArtifactReference, FactoryContextReference, FactoryDigest, FactoryKey, FlowDataSchema,
  FlowPayload, FlowValueSchema,
};
use serde_json::json;

fn artifact(text: &str) -> FactoryArtifactReference {
  FactoryArtifactReference::new(
    ArtifactId::generate(),
    FactoryDigest::content_sha256(text.as_bytes()),
    text.len() as u64,
  )
  .unwrap()
}

#[test]
fn missing_project_goals_identifies_the_required_selection_before_context_compilation() {
  let project = ProjectId::generate();
  let draft: TriageProjectContextDraft = serde_json::from_value(json!({
    "project_id": project,
    "goals": null,
    "rules": artifact("Project rules remain controlling."),
    "documents": {}
  }))
  .unwrap();
  assert_eq!(draft.bind(project), Err(TriageContextProblem::MissingProjectGoals));
}

fn input_schema() -> FlowDataSchema {
  let artifact = json!({"type":"object","fields":{
    "artifact_id":{"required":true,"value":{"type":"string","min_bytes":36,"max_bytes":36}},
    "content_digest":{"required":true,"value":{"type":"string","min_bytes":64,"max_bytes":64}},
    "encoded_size":{"required":true,"value":{"type":"integer","minimum":1,"maximum":16777216}}
  }});
  FlowDataSchema::new(
    FactoryKey::new("reviewed-context").unwrap(),
    FactoryKey::new("v1").unwrap(),
    serde_json::from_value::<FlowValueSchema>(json!({"type":"object","fields":{
      "task":{"required":true,"value":artifact},
      "completion":{"required":true,"value":artifact}
    }}))
    .unwrap(),
  )
  .unwrap()
}
fn payload(task: &FactoryArtifactReference, completion: &FactoryArtifactReference) -> FlowPayload {
  FlowPayload::new(&input_schema(), json!({"task":task,"completion":completion})).unwrap()
}

#[test]
fn reviewed_project_context_freezes_goals_rules_documents_and_the_original_ticket() {
  let project = ProjectId::generate();
  let goals = artifact("Coordinate reproducible execution.");
  let rules = artifact("Project rules remain controlling.");
  let document = artifact("Component ownership and supported platforms.");
  let ticket = artifact("Original ticket from a separate GitHub tracker repository.");
  let completion = artifact("Complete the reviewed triage process.");
  let mut draft: TriageProjectContextDraft = serde_json::from_value(json!({
    "project_id":project, "goals":goals, "rules":rules, "documents":{"components":document}
  }))
  .unwrap();
  let binding = draft.bind(project).unwrap();
  let input = payload(&ticket, &completion);
  let selected = binding.selected_sources(&input).unwrap();
  let expected = [
    ("task", ContextSourceKind::Task, ticket),
    ("completion", ContextSourceKind::Specification, completion),
    ("project-goals", ContextSourceKind::Specification, goals),
    ("project-rules", ContextSourceKind::Policy, rules),
    ("document.components", ContextSourceKind::Specification, document),
  ];
  assert_eq!(
    selected.len(),
    expected.len(),
    "only explicitly reviewed inputs are selected"
  );
  for (name, kind, reference) in expected {
    assert_eq!(
      selected[&FactoryKey::new(name).unwrap()],
      (kind, FactoryContextReference::Artifact(reference))
    );
  }
  draft.goals = Some(artifact(
    "Replacement goals must require another reviewed configuration.",
  ));
  assert_eq!(
    binding.selected_sources(&input).unwrap(),
    selected,
    "published selection retains the original references"
  );
  assert_ne!(draft.bind(project).unwrap(), binding);
}

#[test]
fn unresolved_project_context_reports_the_selection_to_review() {
  let project = ProjectId::generate();
  let draft: TriageProjectContextDraft = serde_json::from_value(json!({
    "project_id":project, "goals":artifact("Reviewed goals"),
    "rules":artifact("Reviewed rules"), "documents":{}
  }))
  .unwrap();
  assert_eq!(
    draft.bind(ProjectId::generate()).unwrap_err().to_string(),
    "select context from the same Project"
  );
}

#[test]
fn project_goals_without_rules_select_only_the_original_ticket_completion_and_goals() {
  let project = ProjectId::generate();
  let goals = artifact("Coordinate reproducible execution.");
  let ticket = artifact("Original ticket.");
  let completion = artifact("Complete the reviewed assessment.");
  for rules in [None, Some(serde_json::Value::Null)] {
    let mut wire = json!({"project_id":project,"goals":goals,"documents":{}});
    if let Some(rules) = rules {
      wire["rules"] = rules;
    }
    let draft: TriageProjectContextDraft = serde_json::from_value(wire).unwrap();
    let selected = draft
      .bind(project)
      .unwrap()
      .selected_sources(&payload(&ticket, &completion))
      .unwrap();
    let expected = [
      ("task", ContextSourceKind::Task, ticket.clone()),
      ("completion", ContextSourceKind::Specification, completion.clone()),
      ("project-goals", ContextSourceKind::Specification, goals.clone()),
    ];
    assert_eq!(
      selected.len(),
      3,
      "absent rules must not create a placeholder policy input"
    );
    for (name, kind, reference) in expected {
      assert_eq!(
        selected[&FactoryKey::new(name).unwrap()],
        (kind, FactoryContextReference::Artifact(reference))
      );
    }
  }
}

#[test]
fn triage_context_keeps_document_aliases_bounded_and_separate_from_authority() {
  use octacity_server_factory::MAX_CONTEXT_MANIFEST_ENTRIES;
  let project = ProjectId::generate();
  let reference = artifact("Reviewed document");
  let mut wire = json!({"project_id":project,"goals":reference,"rules":reference,"documents":{"task":reference}});
  let mut draft: TriageProjectContextDraft = serde_json::from_value(wire.clone()).unwrap();
  let ticket = artifact("Original ticket");
  let selected = draft
    .bind(project)
    .unwrap()
    .selected_sources(&payload(&ticket, &reference))
    .unwrap();
  assert_eq!(
    selected[&FactoryKey::new("task").unwrap()].1,
    FactoryContextReference::Artifact(ticket)
  );
  assert!(selected.contains_key(&FactoryKey::new("document.task").unwrap()));
  draft.documents = (0..MAX_CONTEXT_MANIFEST_ENTRIES)
    .map(|index| (FactoryKey::new(format!("document-{index}")).unwrap(), reference.clone()))
    .collect();
  assert_eq!(
    draft.bind(project),
    Err(TriageContextProblem::InvalidSelection),
    "oversized choices fail without truncation"
  );
  wire["permissions"] = json!({"tools":["write"]});
  assert!(
    serde_json::from_value::<TriageProjectContextDraft>(wire).is_err(),
    "a context selection cannot carry authority fields"
  );
}

#[test]
fn supporting_document_alias_reserves_its_published_context_prefix() {
  let project = ProjectId::generate();
  let reference = artifact("Reviewed rules");
  for (length, valid) in [(119, true), (120, false)] {
    let draft = TriageProjectContextDraft {
      project_id: project,
      goals: Some(reference.clone()),
      rules: Some(reference.clone()),
      documents: [(FactoryKey::new("a".repeat(length)).unwrap(), reference.clone())].into(),
    };
    assert_eq!(draft.bind(project).is_ok(), valid, "alias length {length}");
  }
}

#[test]
fn triage_ticket_projection_keeps_the_admitted_ticket_and_separately_selected_code_repository() {
  use octacity_server_factory::*;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let original = fixture
    .admission
    .flow
    .closure()
    .definition(fixture.admission.flow.closure().root())
    .unwrap();
  let schema = input_schema();
  let mut wire = serde_json::to_value(&original.nodes()[0]).unwrap();
  wire["input_schema"] = json!(schema.reference());
  wire["input_binding"] = json!(TriageProjectContextDraft::ticket_binding());
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: Some(schema.reference().clone()),
    entry: original.entry().clone(),
    nodes: vec![serde_json::from_value(wire).unwrap()],
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: original.execution().clone(),
  })
  .unwrap();
  let work = &fixture.admission.work;
  let flow = FlowRun::root(&fixture.admission.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    fixture.admission.flow.limits().clone(),
    flow,
    cycle,
    fixture.admission.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(vec![fixture.schema, schema.clone()])
  .unwrap();
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work,
    admitted: &admitted,
    flow: admitted.root_run(),
    cycle: admitted.initial_cycle(),
    node: original.entry().clone(),
    schema: &schema,
    history: FlowRuntimeHistory {
      stops: &[],
      flow_runs: &[admitted.root_run().clone()],
      cycles: &[admitted.initial_cycle().clone()],
      attempts: &[],
      completions: &[],
    },
    records: &[],
    inputs: &[],
    incoming: None,
  })
  .unwrap();
  assert_eq!(
    input.payload().value(),
    &json!({"task":work.artifacts().task(),"completion":work.artifacts().acceptance()})
  );
  assert_eq!(
    input.subject(),
    work.subject(),
    "code Repository is the separately admitted subject, never inferred from the ticket source"
  );
  assert!(
    input.sources().is_empty(),
    "no stage or model-authored result can replace the original ticket"
  );
}
