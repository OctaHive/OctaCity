//! Historical fixtures are seeded as persisted documents; assertions use public store ports.
use super::*;
use octacity_server_domain::RepositoryId;
use octacity_server_factory::{FactoryDigest, FactoryRunId, WorkEnvelopeId};
use octacity_server_store::*;
use serde_json::{Value, json};

fn pin() -> Value {
  json!({"identity":"reproduction","version":"v1","digest":"a".repeat(64)})
}
fn retained_metadata(project: ProjectId) -> IssueClassificationMetadata {
  let work = WorkEnvelopeId::generate();
  let run = FactoryRunId::generate();
  let records: Vec<_> = (1..=64).map(|i| FactoryDigest::from_bytes([i; 32])).collect();
  let evidence: Vec<_> = records.iter().map(|record| json!({
    "work_id":work,"run_id":run,"record":record,
    "subject":{"kind":"exact","value":{"project_id":project,"repository_id":RepositoryId::generate(),"base_revision":"abc123"}},
    "environment":pin(),"criterion":pin(),"observed_at":at(2),"fresh_until":at(10)
  })).collect();
  let dependencies: Vec<_> = (0..64).map(|_| json!({
    "work_id":WorkEnvelopeId::generate(),"run_id":FactoryRunId::generate(),"work_digest":"b".repeat(64),"terminal":"finished"
  })).collect();
  serde_json::from_value(json!({"fields":[
    {"field":{"kind":"priority"},"origin":{"kind":"manual","actor":{"kind":"authenticated_management","identity":"operator"}},
      "assessed_version":1,"requirements_version":1,"assessed_at":at(2)},
    {"field":{"kind":"property","id":"effort","definition_version":1},
      "origin":{"kind":"pipeline","actor":{"kind":"unauthenticated_management","identity":null},
        "work_id":work,"run_id":run,"record":"c".repeat(64),"policy":"triage","operation_id":"d".repeat(64)},
      "assessed_version":1,"requirements_version":1,"assessed_at":at(2),"evidence":records},
    {"field":{"kind":"dependencies"},"origin":{"kind":"source","actor":{"kind":"unauthenticated_management","identity":null},
      "mapping":pin(),"observation":"e".repeat(64),"native_correlation":"f".repeat(64)},
      "assessed_version":1,"requirements_version":1,"assessed_at":at(2)}
  ],"evidence":evidence,"dependencies":dependencies})).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_metadata_round_trip_preserves_exact_history_and_original_receipts() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  store.publish_issue_properties(management_mutation(PublishIssueProperties {
    intent: PublishIssuePropertiesIntent { project_id: project.id, expected_version: 0,
      definition: serde_json::from_value(json!({"labels":[],"properties":[
        {"id":"effort","name":"Size","retired":false,"kind":{"type":"choice","choices":[{"id":"small","name":"S","retired":false}]}}
      ],"classification":{"size":"effort"}})).unwrap() },
    idempotency_key: key("catalog"), published_at: at(2),
  })).await.unwrap();
  let mut create = draft(project.id, "create");
  create.intent.content.priority = Some(20.into());
  create.intent.planning = IssuePlanning {
    definition_version: 1,
    values: [("effort".into(), IssuePropertyValue::Choice("small".into()))].into(),
    ..Default::default()
  };
  let legacy = store
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  assert_eq!(legacy.classification.fields.len(), 2);
  assert!(
    legacy
      .classification
      .fields
      .iter()
      .all(|field| matches!(field.origin, IssueClassificationOrigin::Manual { .. }))
  );
  let mut retained = legacy.clone();
  retained.version = legacy.version.next().unwrap();
  retained.updated_at = at(3);
  retained.content.priority = Some(80.into());
  retained.classification = retained_metadata(project.id);
  support::issue_revision::seed(&database.pool, &retained).await;

  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  assert_eq!(restarted.issue(project.id, legacy.id, None).await.unwrap(), retained);
  let mut plan = retained.planning.clone();
  plan.due_date = Some("2026-12-01".into());
  let edit = EditIssuePlanning {
    assignees: Default::default(),
    intent: EditIssuePlanningIntent {
      project_id: project.id,
      id: retained.id,
      expected_version: retained.version,
      planning: plan,
    },
    idempotency_key: key("planning"),
    edited_at: at(4),
  };
  let planned = restarted
    .edit_issue_planning(management_mutation(edit.clone()))
    .await
    .unwrap()
    .issue;
  assert_eq!(planned.classification, retained.classification);
  assert_eq!(planned.planning.values, retained.planning.values);
  assert_eq!(planned.applicability_version, retained.applicability_version);
  let mut content = planned.content.clone();
  content.description = "New requirements".into();
  let changed = restarted
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: project.id,
        id: retained.id,
        expected_version: planned.version,
        content,
      },
      idempotency_key: key("requirements"),
      edited_at: at(5),
    }))
    .await
    .unwrap()
    .issue;
  assert_eq!(changed.applicability_version, changed.version);
  assert_eq!(changed.classification, retained.classification);
  for revision in [&legacy, &retained, &planned, &changed] {
    assert_eq!(
      restarted
        .issue(project.id, legacy.id, Some(revision.version))
        .await
        .unwrap(),
      *revision
    );
  }
  let replay = restarted.create_issue(management_mutation(create)).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, legacy);
  let replay = restarted.edit_issue_planning(management_mutation(edit)).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, planned);
  assert_eq!(restarted.issue(project.id, legacy.id, None).await.unwrap(), changed);
  assert_eq!(
    restarted
      .issue_status_history(project.id, legacy.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_metadata_upgrade_preserves_legacy_documents_catalogs_and_replay() {
  use octacity_server_store_postgres::{MIGRATOR, MigrationStatus, migrate, migration_status};
  let mut database = TestDatabase::empty().await;
  // Rehearse the schema immediately before the classification-receipt index.
  MIGRATOR.run_to(115, &database.pool).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let publication = PublishIssueProperties {
    intent: PublishIssuePropertiesIntent {
      project_id: project.id, expected_version: 0,
      definition: serde_json::from_value(json!({"labels":[],"properties":[
        {"id":"effort","name":"Legacy size","retired":false,"kind":{"type":"choice","choices":[{"id":"small","name":"S","retired":false}]}}
      ]})).unwrap(),
    },
    idempotency_key: key("legacy-catalog"), published_at: at(2),
  };
  let catalog = store
    .publish_issue_properties(management_mutation(publication.clone()))
    .await
    .unwrap()
    .resource;
  assert!(
    serde_json::to_value(&catalog.definition)
      .unwrap()
      .get("classification")
      .is_none()
  );
  let mut create = draft(project.id, "legacy-create");
  create.intent.content.priority = Some(20.into());
  create.intent.planning = IssuePlanning {
    definition_version: 1,
    values: [("effort".into(), IssuePropertyValue::Choice("small".into()))].into(),
    ..Default::default()
  };
  let mut original = store
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: original.id,
      expected_version: original.version,
      content: IssueContent {
        description: "Before upgrade".into(),
        ..original.content.clone()
      },
    },
    idempotency_key: key("legacy-edit"),
    edited_at: at(3),
  };
  let mut edited = store.edit_issue(management_mutation(edit.clone())).await.unwrap().issue;
  // The current manual writer emits attribution. Restore the historical wire
  // shape in both revisions and receipts before qualifying the old-schema upgrade.
  let mut legacy = database.pool.begin().await.unwrap();
  sqlx::query("UPDATE issue_revisions SET document=document-'classification' WHERE issue_id=$1")
    .bind(original.id.as_uuid())
    .execute(&mut *legacy)
    .await
    .unwrap();
  let receipts = sqlx::query("UPDATE idempotency_records SET outcome=jsonb_set(outcome,'{value,issue}',(outcome #> '{value,issue}')-'classification') WHERE outcome #>> '{value,issue,id}'=$1")
    .bind(original.id.to_string()).execute(&mut *legacy).await.unwrap();
  assert_eq!(receipts.rows_affected(), 2);
  legacy.commit().await.unwrap();
  original.classification = Default::default();
  edited.classification = Default::default();
  assert_eq!(store.issue(project.id, original.id, None).await.unwrap(), edited);
  assert_eq!(
    store
      .create_issue(management_mutation(create.clone()))
      .await
      .unwrap()
      .issue,
    original
  );
  let history = store
    .issue_status_history(project.id, original.id, None, 10)
    .await
    .unwrap();
  assert!(serde_json::to_value(&edited).unwrap().get("classification").is_none());
  let snapshot = super::snapshot::DatabaseSnapshot::capture(&mut database).await;
  let restored = snapshot.restore().await;

  for copy in [&database, &restored] {
    assert_eq!(migration_status(&copy.pool).await.unwrap(), MigrationStatus::Pending);
    migrate(&copy.pool).await.unwrap();
    migrate(&copy.pool).await.unwrap();
    assert_eq!(migration_status(&copy.pool).await.unwrap(), MigrationStatus::Current);
    let upgraded = PostgresStore::new(copy.pool.clone());
    assert_eq!(
      upgraded
        .issue(project.id, original.id, Some(original.version))
        .await
        .unwrap(),
      original
    );
    assert_eq!(upgraded.issue(project.id, original.id, None).await.unwrap(), edited);
    assert_eq!(
      upgraded
        .issue_status_history(project.id, original.id, None, 10)
        .await
        .unwrap(),
      history
    );
    assert_eq!(upgraded.issue_properties(project.id, Some(1)).await.unwrap(), catalog);

    let mut replacement = publication.clone();
    replacement.intent.expected_version = 1;
    replacement.idempotency_key = key("new-semantics");
    replacement.published_at = at(4);
    replacement.intent.definition.classification.size = Some("effort".into());
    replacement.intent.definition.properties[0].name = "Effort".into();
    upgraded
      .publish_issue_properties(management_mutation(replacement))
      .await
      .unwrap();
    let mut plan = edited.planning.clone();
    plan.definition_version = 2;
    plan.due_date = Some("2026-12-01".into());
    let current = upgraded
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        assignees: Default::default(),
        intent: EditIssuePlanningIntent {
          project_id: project.id,
          id: edited.id,
          expected_version: edited.version,
          planning: plan,
        },
        idempotency_key: key("after-upgrade"),
        edited_at: at(5),
      }))
      .await
      .unwrap()
      .issue;
    // Binding an existing value does not manufacture provenance or a producing Run.
    assert!(current.classification.is_empty());
    assert_eq!(current.content, edited.content);
    assert_eq!(current.planning.values, edited.planning.values);
    let replay = upgraded
      .create_issue(management_mutation(create.clone()))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, original);
    let replay = upgraded.edit_issue(management_mutation(edit.clone())).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, edited);
    let replay = upgraded
      .publish_issue_properties(management_mutation(publication.clone()))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.resource, catalog);
    assert_eq!(upgraded.issue(project.id, original.id, None).await.unwrap(), current);
  }
  restored.cleanup().await;
  database.cleanup().await;
  snapshot.cleanup().await;
}
