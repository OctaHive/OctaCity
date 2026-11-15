//! Selected forms share the existing Issue transaction, history and replay owner.
use super::*;
use serde_json::json;

#[path = "selected_field_form_replay.rs"]
mod replay;

#[path = "selected_field_form_conflicts.rs"]
mod conflicts;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn selected_field_form_round_trip_retains_all_scalar_values_and_exact_form_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (baseline, catalog, command) = fixture(&store, &database.pool).await;
  let form = &command.fields.as_ref().unwrap().selection;
  // The reviewed selection, including requiredness and its Project/catalog pin,
  // defines the form's content identity; no copied value schema is retained.
  let form_digest = FactoryDigest::content_sha256(&serde_json::to_vec(form).unwrap());
  let applied = store
    .apply_issue_classification(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(
    applied.issue.content.priority,
    Some(IssuePriority::Choice("high".into()))
  );
  assert_eq!(applied.issue.planning.component.as_deref(), Some("engine"));
  assert_eq!(applied.issue.planning.issue_type.as_deref(), Some("feature_request"));
  assert_eq!(applied.issue.planning.size.as_deref(), Some("medium"));
  assert_eq!(
    applied.issue.planning.values,
    [
      ("summary".into(), IssuePropertyValue::Text("CI: ёж".into())),
      ("passed".into(), IssuePropertyValue::Boolean(false)),
      ("failures".into(), IssuePropertyValue::Number(0)),
      ("checked_on".into(), IssuePropertyValue::Date("2028-02-29".into())),
      ("environment".into(), IssuePropertyValue::Choice("linux".into())),
      ("optional_note".into(), IssuePropertyValue::Text("Keep optional".into())),
      (
        "unselected_note".into(),
        IssuePropertyValue::Text("Keep unselected".into())
      ),
    ]
    .into()
  );
  for field in &applied.issue.classification.fields {
    let wire = serde_json::to_value(field).unwrap();
    if wire["origin"]["kind"] == "pipeline" {
      assert_eq!(
        wire["origin"]["form"],
        json!(form_digest),
        "saved values must identify their exact reviewed form"
      );
      assert_eq!(wire["origin"]["record"], json!(command.record));
      assert_eq!(wire["origin"]["run_id"], json!(command.association.run_id));
      assert_eq!(wire["origin"]["work_id"], json!(command.association.work_id));
      assert_eq!(field.field.definition_version(), Some(catalog.version));
    }
  }
  assert_eq!(
    applied
      .issue
      .classification
      .fields
      .iter()
      .filter(|field| matches!(field.origin, IssueClassificationOrigin::Pipeline { .. }))
      .count(),
    9
  );
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  let pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(pool.clone());
  assert_eq!(
    restarted.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    applied.issue
  );
  assert_eq!(
    restarted
      .issue(baseline.project_id, baseline.id, Some(baseline.version))
      .await
      .unwrap(),
    baseline
  );
  assert_eq!(
    restarted
      .issue_properties(baseline.project_id, Some(catalog.version))
      .await
      .unwrap(),
    catalog
  );
  let replay = restarted
    .apply_issue_classification(management_mutation(command))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, applied.issue);
  pool.close().await;
  database.cleanup().await;
}

async fn fixture(
  store: &PostgresStore,
  pool: &sqlx::PgPool,
) -> (IssueRevision, PublishedIssueProperties, ApplyIssueClassification) {
  let (original, association) = linked(store, pool).await;
  let catalog = store.publish_issue_properties(management_mutation(PublishIssueProperties {
    intent:PublishIssuePropertiesIntent {project_id:original.project_id,expected_version:0,
      definition:serde_json::from_value(json!({"labels":[],"components":[{"id":"engine","name":"Engine","retired":false}],"properties":[
        {"id":"summary","name":"Summary","retired":false,"kind":{"type":"text","max_bytes":80}},
        {"id":"passed","name":"Passed","retired":false,"kind":{"type":"boolean"}},
        {"id":"failures","name":"Failures","retired":false,"kind":{"type":"number","minimum":0,"maximum":20}},
        {"id":"checked_on","name":"Checked on","retired":false,"kind":{"type":"date"}},
        {"id":"environment","name":"Environment","retired":false,"kind":{"type":"choice","choices":[{"id":"linux","name":"Linux","retired":false}]}},
        {"id":"optional_note","name":"Optional","retired":false,"kind":{"type":"text","max_bytes":80}},
        {"id":"unselected_note","name":"Unselected","retired":false,"kind":{"type":"text","max_bytes":80}}
      ]})).unwrap()},
    idempotency_key:key("selected-form-catalog"),published_at:at(10),
  })).await.unwrap().resource;
  let baseline = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: original.project_id,
        id: original.id,
        expected_version: original.version,
        planning: IssuePlanning {
          definition_version: catalog.version,
          values: [
            ("optional_note".into(), IssuePropertyValue::Text("Keep optional".into())),
            (
              "unselected_note".into(),
              IssuePropertyValue::Text("Keep unselected".into()),
            ),
          ]
          .into(),
          ..original.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("form-baseline-planning"),
      edited_at: at(11),
    }))
    .await
    .unwrap()
    .issue;
  let selection: IssueFieldForm = serde_json::from_value(
    json!({"project_id":original.project_id,"definition_version":catalog.version,
    "fields":[{"field":{"kind":"priority"}},{"field":{"kind":"component"}},
      {"field":{"kind":"type"}},{"field":{"kind":"size"}},
      {"field":{"kind":"property","id":"summary"}},{"field":{"kind":"property","id":"passed"}},
      {"field":{"kind":"property","id":"failures"}},{"field":{"kind":"property","id":"checked_on"}},
      {"field":{"kind":"property","id":"environment"}},
      {"field":{"kind":"property","id":"optional_note"},"required":false}]}),
  )
  .unwrap();
  let update = selection
    .complete(
      &catalog,
      &json!({"priority":"high","component":"engine","type":"feature_request","size":"medium",
    "property.summary":"CI: ёж","property.passed":false,"property.failures":0,
    "property.checked_on":"2028-02-29","property.environment":"linux"}),
      Default::default(),
    )
    .unwrap();
  let command = ApplyIssueClassification {
    association,
    policy: "ci-assessment".parse().unwrap(),
    record: FactoryDigest::from_bytes([211; 32]),
    operation_id: FactoryDigest::from_bytes([212; 32]),
    accepted_at: at(15),
    applied_at: at(20),
    priority: None,
    fields: Some(update),
    size_label: None,
    managed_labels: Default::default(),
    authority: None,
  };
  (baseline, catalog, command)
}
