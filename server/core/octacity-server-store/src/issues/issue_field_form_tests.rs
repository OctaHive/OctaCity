//! Public field-form and Issue mutation behavior.
use super::*;

#[test]
fn selected_issue_field_form_retains_configured_default_catalogs_explicitly() {
  let catalog = PublishedIssueProperties {
    project_id: ProjectId::generate(),
    version: 0,
    published_at: at(0),
    definition: IssuePropertyDefinitionSet::default(),
  };
  let wire = serde_json::to_value(&catalog.definition).unwrap();
  assert_eq!(
    wire["sizes"],
    json!([
      {"id":"small","name":"Small","retired":false},
      {"id":"medium","name":"Medium","retired":false},
      {"id":"large","name":"Large","retired":false}
    ])
  );
  assert_eq!(wire["priorities"].as_array().unwrap().len(), 4);
  assert_eq!(wire["types"].as_array().unwrap().len(), 2);
  let field = IssueClassificationFieldReference::Size {
    definition_version: Some(0),
  };
  let snapshot = IssueSnapshotDefinitions::selected(&catalog, [&field]).unwrap();
  assert_eq!(
    serde_json::to_value(snapshot).unwrap()["selected_v1"]["sizes"],
    wire["sizes"]
  );
}

#[test]
fn selected_issue_field_form_derives_types_choices_and_requiredness_from_project() {
  use octacity_server_factory::{FlowFieldSchema, FlowValueSchema};
  let catalog = form_catalog(ProjectId::generate());
  let selection: IssueFieldForm = serde_json::from_value(json!({
    "project_id":catalog.project_id,"definition_version":1,"fields":[
      {"field":{"kind":"size"}},
      {"field":{"kind":"property","id":"note"}},
      {"field":{"kind":"property","id":"ci_passed"}},
      {"field":{"kind":"property","id":"failures"}},
      {"field":{"kind":"property","id":"checked_on"},"required":false},
      {"field":{"kind":"property","id":"environment"}}
    ]
  }))
  .expect("fields need no classification roles or manually authored schema");
  assert_eq!(
    selection.form_schema(&catalog).unwrap(),
    FlowValueSchema::Object {
      fields: [
        (
          "size",
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: vec![json!("small"), json!("medium"), json!("large")]
          })
        ),
        (
          "property.note",
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 0,
            max_bytes: 80
          })
        ),
        (
          "property.ci_passed",
          FlowFieldSchema::required(FlowValueSchema::Boolean)
        ),
        (
          "property.failures",
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 20
          })
        ),
        (
          "property.checked_on",
          FlowFieldSchema::optional(FlowValueSchema::String {
            min_bytes: 10,
            max_bytes: 10
          })
        ),
        (
          "property.environment",
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: vec![json!("linux")]
          })
        ),
      ]
      .into_iter()
      .map(|(key, value)| (key.parse().unwrap(), value))
      .collect()
    }
  );
}

#[test]
fn selected_issue_field_form_applies_all_scalar_kinds_and_preserves_omitted_optional_values() {
  run_ready(async {
    let (baseline, catalog, request) = form_fixture().await;
    let actor = AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    };
    let applied = baseline
      .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
      .unwrap();
    assert_eq!(applied.content.priority, Some(IssuePriority::Choice("high".into())));
    assert_eq!(applied.planning.component.as_deref(), Some("engine"));
    assert_eq!(applied.planning.issue_type.as_deref(), Some("feature_request"));
    assert_eq!(applied.planning.size.as_deref(), Some("medium"));
    assert_eq!(
      applied.planning.values,
      [
        ("note".into(), IssuePropertyValue::Text("CI analysis".into())),
        ("ci_passed".into(), IssuePropertyValue::Boolean(false)),
        ("failures".into(), IssuePropertyValue::Number(2)),
        ("checked_on".into(), IssuePropertyValue::Date("2028-02-29".into())),
        ("environment".into(), IssuePropertyValue::Choice("linux".into())),
        ("optional_note".into(), IssuePropertyValue::Text("Keep this".into())),
      ]
      .into()
    );
    assert_eq!(
      applied.classification.fields.len(),
      9,
      "omission creates no fabricated attribution"
    );
    assert!(
      applied
        .classification
        .fields
        .iter()
        .all(|f| f.assessed_version == baseline.version
          && matches!(&f.origin,IssueClassificationOrigin::Pipeline{record,..} if *record==request.record))
    );
    assert_eq!(applied.applicability_version, baseline.applicability_version);
    assert_eq!(
      baseline.planning.values.len(),
      1,
      "the original revision stays immutable"
    );
  });
}

#[test]
fn selected_issue_field_form_debug_does_not_expose_agent_values() {
  run_ready(async {
    let (_, _, request) = form_fixture().await;
    assert!(!format!("{:?}", request.fields.unwrap()).contains("CI analysis"));
  });
}

#[test]
fn selected_issue_field_form_manual_override_retains_attribution_for_ordinary_properties() {
  run_ready(async {
    let (baseline, catalog, request) = form_fixture().await;
    let actor = AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    };
    let applied = baseline
      .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
      .unwrap();
    let mut planning = applied.planning.clone();
    planning
      .values
      .insert("note".into(), IssuePropertyValue::Text("Operator correction".into()));
    let edit = EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: applied.project_id,
        id: applied.id,
        expected_version: applied.version,
        planning,
      },
      assignees: Default::default(),
      idempotency_key: key("override-form-note"),
      edited_at: at(6),
    };
    let mut changed = applied.planned(&edit, &Default::default(), &catalog).unwrap();
    changed
      .attribute_manual_classification(Some(&applied), &catalog, &actor)
      .unwrap();
    let note = changed
      .classification
      .fields
      .iter()
      .find(|field| field.field.identity() == IssueClassificationFieldIdentity::Property { id: "note" })
      .expect("an ordinary property retains the actual human override");
    assert!(matches!(&note.origin,IssueClassificationOrigin::Manual{actor:actual} if actual==&actor));
    let unchanged = |issue: &IssueRevision| {
      issue
        .classification
        .fields
        .iter()
        .find(|field| field.field.identity() == IssueClassificationFieldIdentity::Property { id: "failures" })
        .cloned()
    };
    assert_eq!(
      unchanged(&changed),
      unchanged(&applied),
      "unrelated provenance is not renewed"
    );
  });
}

#[test]
fn selected_issue_field_form_priority_only_preserves_unselected_planning() {
  run_ready(async {
    let (mut baseline, catalog, mut request) = form_fixture().await;
    baseline.planning = Default::default();
    let form = IssueFieldForm {
      fields: vec![IssueFormField {
        field: IssueFieldTarget::Priority,
        required: true,
      }],
      ..request.fields.as_ref().unwrap().selection.clone()
    };
    request.fields = Some(
      form
        .complete(&catalog, &json!({"priority":"high"}), Default::default())
        .unwrap(),
    );
    let actor = AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    };
    let applied = baseline
      .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
      .unwrap();
    assert_eq!(
      applied.planning, baseline.planning,
      "a field form cannot update unselected planning metadata"
    );
    assert_eq!(applied.content.priority, Some(IssuePriority::Choice("high".into())));
  });
}

#[test]
fn selected_issue_field_form_rejects_invalid_missing_and_unselected_values_atomically() {
  run_ready(async {
    let (baseline, catalog, request) = form_fixture().await;
    let form = &request.fields.as_ref().unwrap().selection;
    for (key, value) in [
      ("priority", json!("foreign_priority")),
      ("component", json!("other_project_component")),
      ("type", json!("other_type")),
      ("size", json!("foreign_size")),
      ("property.note", json!("я".repeat(41))),
      ("property.note", json!("bad\u{0000}text")),
      ("property.ci_passed", json!("false")),
      ("property.failures", json!(21)),
      ("property.failures", json!(1.5)),
      ("property.checked_on", json!("2027-02-29")),
      ("property.environment", json!("foreign_environment")),
      ("property.environment", json!("old")),
      ("property.optional_note", Value::Null),
      ("property.unselected", json!("injected")),
      ("issue_id", json!(IssueId::generate())),
    ] {
      let mut valid = json!({"priority":"high","component":"engine","type":"feature_request","size":"medium",
        "property.note":"CI analysis","property.ci_passed":false,"property.failures":2,
        "property.checked_on":"2028-02-29","property.environment":"linux"});
      valid[key] = value;
      assert!(
        form.complete(&catalog, &valid, Default::default()).is_err(),
        "reject {key}"
      );
    }
    let mut missing = request.clone();
    missing
      .fields
      .as_mut()
      .unwrap()
      .values
      .remove(&"property.failures".parse().unwrap());
    let mut unselected = request.clone();
    unselected
      .fields
      .as_mut()
      .unwrap()
      .values
      .insert("property.unselected".parse().unwrap(), json!("injected"));
    let mut wrong_type = request.clone();
    wrong_type
      .fields
      .as_mut()
      .unwrap()
      .values
      .insert("property.checked_on".parse().unwrap(), json!(false));
    let actor = AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    };
    for invalid in [missing, unselected, wrong_type] {
      assert!(
        baseline
          .classified(&baseline, &invalid, None, &catalog, &Default::default(), &actor)
          .is_err()
      );
      assert_eq!(
        baseline.planning.values.len(),
        1,
        "rejection leaves every original value intact"
      );
    }
  });
}

#[test]
fn selected_issue_field_form_retains_existing_conflict_and_source_ownership_guards() {
  run_ready(async {
    let (baseline, catalog, request) = form_fixture().await;
    let actor = AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    };
    let mut current = baseline.clone();
    current
      .planning
      .values
      .insert("failures".into(), IssuePropertyValue::Number(5));
    assert!(
      current
        .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
        .is_err()
    );
    for field in [
      IssueField::Priority,
      IssueField::Component,
      IssueField::Type,
      IssueField::Size,
      IssueField::Properties,
    ] {
      let policy = IssueFieldPolicy {
        source_owned: [field].into(),
        ..Default::default()
      };
      assert!(
        baseline
          .classified(&baseline, &request, None, &catalog, &policy, &actor)
          .is_err(),
        "{field:?}"
      );
    }
    current.applicability_version = current.applicability_version.next().unwrap();
    assert!(
      current
        .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
        .is_err()
    );
    let mut foreign = request.clone();
    foreign.association.issue_id = IssueId::generate();
    assert!(
      baseline
        .classified(&baseline, &foreign, None, &catalog, &Default::default(), &actor)
        .is_err()
    );
  });
}

#[test]
fn selected_issue_field_form_rejects_unknown_duplicate_foreign_retired_and_obsolete_selections() {
  run_ready(async {
    let (_, catalog, request) = form_fixture().await;
    let original = request.fields.as_ref().unwrap().selection.clone();
    let mut duplicate = original.clone();
    duplicate.fields.push(duplicate.fields[0].clone());
    let mut unknown = original.clone();
    unknown.fields[0].field = IssueFieldTarget::Property { id: "missing".into() };
    let mut retired_catalog = catalog.clone();
    retired_catalog.definition.properties[0].retired = true;
    let mut stale = original.clone();
    stale.definition_version = 2;
    let mut foreign = original.clone();
    foreign.project_id = ProjectId::generate();
    let mut empty = original.clone();
    empty.fields.clear();
    let mut oversized = original.clone();
    oversized.fields = vec![oversized.fields[0].clone(); MAX_ISSUE_PROPERTIES + 5];
    for invalid in [duplicate, unknown, stale, foreign, empty, oversized] {
      assert!(invalid.form_schema(&catalog).is_err());
    }
    assert!(original.form_schema(&retired_catalog).is_err());
    let mut retired_value = catalog.clone();
    let IssuePropertyKind::Choice { choices } = &mut retired_value.definition.properties[4].kind else {
      panic!("choice fixture")
    };
    choices[0].retired = true;
    assert!(original.form_schema(&retired_value).is_err());
    for field in ["status", "execution", "dependencies"] {
      assert!(serde_json::from_value::<IssueFieldTarget>(json!({"kind":field})).is_err());
    }
    assert!(
      serde_json::from_value::<IssueFieldForm>(json!({"definition_version":1,"priority":true,"size":true})).is_err()
    );
  });
}

async fn form_fixture() -> (IssueRevision, PublishedIssueProperties, ApplyIssueClassification) {
  let store = testing::InMemoryProjectStore::new();
  let mut baseline = ordinary_issue(&store).await;
  let catalog = form_catalog(baseline.project_id);
  baseline.planning.definition_version = 1;
  baseline
    .planning
    .values
    .insert("optional_note".into(), IssuePropertyValue::Text("Keep this".into()));
  let selection: IssueFieldForm = serde_json::from_value(json!({
    "project_id":catalog.project_id,"definition_version":1,"fields":[
      {"field":{"kind":"priority"}},{"field":{"kind":"component"}},
      {"field":{"kind":"type"}},{"field":{"kind":"size"}},
      {"field":{"kind":"property","id":"note"}},
      {"field":{"kind":"property","id":"ci_passed"}},
      {"field":{"kind":"property","id":"failures"}},
      {"field":{"kind":"property","id":"checked_on"}},
      {"field":{"kind":"property","id":"environment"}},
      {"field":{"kind":"property","id":"optional_note"},"required":false}
    ]
  }))
  .unwrap();
  let update = selection
    .complete(
      &catalog,
      &json!({
        "priority":"high","component":"engine","type":"feature_request","size":"medium",
        "property.note":"CI analysis","property.ci_passed":false,"property.failures":2,
        "property.checked_on":"2028-02-29","property.environment":"linux"
      }),
      Default::default(),
    )
    .unwrap();
  let request = ApplyIssueClassification {
    association: IssueWorkAssociation {
      project_id: baseline.project_id,
      issue_id: baseline.id,
      request_id: None,
      requirements_version: Some(baseline.version),
      work_id: octacity_server_factory::WorkEnvelopeId::generate(),
      run_id: octacity_server_factory::FactoryRunId::generate(),
    },
    policy: "ci-analysis".parse().unwrap(),
    record: octacity_server_factory::FactoryDigest::from_bytes([1; 32]),
    operation_id: octacity_server_factory::FactoryDigest::from_bytes([2; 32]),
    accepted_at: at(4),
    applied_at: at(5),
    authority: None,
    priority: None,
    size_label: None,
    managed_labels: Default::default(),
    fields: Some(update),
  };
  (baseline, catalog, request)
}

fn form_catalog(project_id: ProjectId) -> PublishedIssueProperties {
  PublishedIssueProperties {
      project_id, version: 1, published_at: at(3),
      definition: serde_json::from_value(json!({"labels":[],"components":[{"id":"engine","name":"Engine","retired":false}],"properties":[
        {"id":"note","name":"Note","retired":false,"kind":{"type":"text","max_bytes":80}},
        {"id":"ci_passed","name":"CI passed","retired":false,"kind":{"type":"boolean"}},
        {"id":"failures","name":"Failures","retired":false,"kind":{"type":"number","minimum":0,"maximum":20}},
        {"id":"checked_on","name":"Checked on","retired":false,"kind":{"type":"date"}},
        {"id":"environment","name":"Environment","retired":false,"kind":{"type":"choice","choices":[{"id":"linux","name":"Linux","retired":false},{"id":"old","name":"Old","retired":true}]}},
        {"id":"optional_note","name":"Optional note","retired":false,"kind":{"type":"text","max_bytes":80}}
      ]})).unwrap(),
    }
}
