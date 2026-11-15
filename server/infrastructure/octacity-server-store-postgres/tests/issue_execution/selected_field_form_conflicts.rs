//! Project/Issue fences serialize field saves with ordinary operator edits.
use super::*;
use crate::support::evidence::PostgresEvidenceProbe;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn independent_field_forms_can_finish_out_of_run_order_without_overwriting_newer_fields() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (baseline, _, mut earlier) = fixture(&store, &database.pool).await;
  let newer_association = parallel_work(&store, &baseline, &earlier.association).await;
  let update = earlier.fields.as_mut().unwrap();
  update.selection.fields.retain(|selected| {
    matches!(&selected.field,
    IssueFieldTarget::Property {id} if id=="summary" || id=="passed")
  });
  for selected in &mut update.selection.fields {
    selected.required = matches!(&selected.field,IssueFieldTarget::Property{id} if id=="summary");
  }
  update.values = [("property.summary".parse().unwrap(), json!("Earlier summary"))].into();
  earlier.applied_at = at(21);
  let mut newer = earlier.clone();
  newer.association = newer_association;
  newer.operation_id = FactoryDigest::from_bytes([220; 32]);
  newer.record = FactoryDigest::from_bytes([221; 32]);
  newer.accepted_at = at(16);
  newer.applied_at = at(20);
  let update = newer.fields.as_mut().unwrap();
  update
    .selection
    .fields
    .retain(|selected| matches!(&selected.field,IssueFieldTarget::Property{id} if id=="passed"));
  update.selection.fields[0].required = true;
  update.values = [("property.passed".parse().unwrap(), json!(false))].into();
  let passed = store
    .apply_issue_classification(management_mutation(newer.clone()))
    .await
    .unwrap()
    .issue;
  let saved = store
    .apply_issue_classification(management_mutation(earlier.clone()))
    .await
    .expect("an older Run may fill an untouched field independently of a newer Run")
    .issue;
  assert_eq!(
    saved.planning.values["summary"],
    IssuePropertyValue::Text("Earlier summary".into())
  );
  assert_eq!(saved.planning.values["passed"], IssuePropertyValue::Boolean(false));
  assert_eq!(
    saved.planning.values["optional_note"],
    baseline.planning.values["optional_note"]
  );
  assert_eq!(
    saved.planning.values["unselected_note"],
    baseline.planning.values["unselected_note"]
  );
  let passed_origin = |issue: &IssueRevision| {
    issue
      .classification
      .fields
      .iter()
      .find(|field| field.field.identity() == IssueClassificationFieldIdentity::Property { id: "passed" })
      .cloned()
  };
  assert_eq!(
    passed_origin(&saved),
    passed_origin(&passed),
    "omitted optional fields keep the newer provenance"
  );

  let mut overlapping = earlier;
  overlapping.operation_id = FactoryDigest::from_bytes([222; 32]);
  overlapping.record = FactoryDigest::from_bytes([223; 32]);
  overlapping.accepted_at = at(17);
  overlapping.applied_at = at(22);
  overlapping.fields = newer.fields;
  overlapping
    .fields
    .as_mut()
    .unwrap()
    .values
    .insert("property.passed".parse().unwrap(), json!(true));
  assert!(
    matches!(
      store.apply_issue_classification(management_mutation(overlapping)).await,
      Err(StoreError::Conflict {
        entity: EntityKind::Issue
      })
    ),
    "an older Run cannot replace a newer assessment of the same field"
  );
  assert_eq!(
    store.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    saved
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn selected_field_form_races_a_manual_property_edit_without_partial_values_or_origins() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (baseline, _, command) = fixture(&store, &database.pool).await;
  let probe = PostgresEvidenceProbe(database.pool.clone());
  let before = probe.mutation_evidence_counts().await;
  let mut planning = baseline.planning.clone();
  planning.values.insert("failures".into(), IssuePropertyValue::Number(5));
  let edit = EditIssuePlanning {
    intent: EditIssuePlanningIntent {
      project_id: baseline.project_id,
      id: baseline.id,
      expected_version: baseline.version,
      planning,
    },
    assignees: Default::default(),
    idempotency_key: key("racing-property-edit"),
    edited_at: at(20),
  };
  let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
  let save = {
    let store = PostgresStore::new(database.pool.clone());
    let barrier = barrier.clone();
    tokio::spawn(async move {
      barrier.wait().await;
      store.apply_issue_classification(management_mutation(command)).await
    })
  };
  let manual = {
    let store = PostgresStore::new(database.pool.clone());
    tokio::spawn(async move {
      barrier.wait().await;
      store.edit_issue_planning(management_mutation(edit)).await
    })
  };
  let (saved, edited) = (save.await.unwrap(), manual.await.unwrap());
  let (winner, loser) = match (saved, edited) {
    (Ok(saved), Err(error)) => {
      assert_eq!(saved.issue.planning.values["failures"], IssuePropertyValue::Number(0));
      assert_eq!(
        saved.issue.planning.values["summary"],
        IssuePropertyValue::Text("CI: ёж".into())
      );
      assert_eq!(saved.issue.content.priority, Some(IssuePriority::Choice("high".into())));
      (saved.issue, error)
    }
    (Err(error), Ok(edited)) => {
      assert_eq!(edited.issue.planning.values["failures"], IssuePropertyValue::Number(5));
      assert!(!edited.issue.planning.values.contains_key("summary"));
      assert_eq!(edited.issue.content.priority, baseline.content.priority);
      assert!(
        edited
          .issue
          .classification
          .fields
          .iter()
          .all(|field| matches!(field.origin, IssueClassificationOrigin::Manual { .. }))
      );
      (edited.issue, error)
    }
    unexpected => panic!("exactly one conflicting operation must commit: {unexpected:?}"),
  };
  assert!(matches!(
    loser,
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  ));
  assert_eq!(winner.version, baseline.version.next().unwrap());
  assert_eq!(
    store.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    winner
  );
  let after = probe.mutation_evidence_counts().await;
  assert_eq!(after.audit, before.audit + 1);
  assert_eq!(after.outbox, before.outbox + 1);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn selected_field_form_rejects_an_intermediate_property_edit_even_after_its_value_is_restored() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (baseline, _, command) = fixture(&store, &database.pool).await;
  let mut current = baseline.clone();
  for (id, value, time) in [
    ("intermediate-property", Some(IssuePropertyValue::Boolean(true)), 12),
    ("restore-property", None, 13),
  ] {
    let mut planning = current.planning.clone();
    match value {
      Some(value) => {
        planning.values.insert("passed".into(), value);
      }
      None => {
        planning.values.remove("passed");
      }
    }
    current = store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: current.project_id,
          id: current.id,
          expected_version: current.version,
          planning,
        },
        assignees: Default::default(),
        idempotency_key: key(id),
        edited_at: at(time),
      }))
      .await
      .unwrap()
      .issue;
  }
  assert_eq!(current.planning.values, baseline.planning.values);
  let history = store
    .issue_revisions(baseline.project_id, baseline.id, None, 100)
    .await
    .unwrap();
  let probe = PostgresEvidenceProbe(database.pool.clone());
  let before = probe.mutation_evidence_counts().await;
  assert!(matches!(
    store.apply_issue_classification(management_mutation(command)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(
    store.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    current
  );
  assert_eq!(
    store
      .issue_revisions(baseline.project_id, baseline.id, None, 100)
      .await
      .unwrap(),
    history
  );
  let after = probe.mutation_evidence_counts().await;
  assert_eq!(after.idempotency, before.idempotency + 1);
  assert_eq!(after.audit, before.audit);
  assert_eq!(after.outbox, before.outbox);
  database.cleanup().await;
}
