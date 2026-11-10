//! The original priority/size-label receipts remain independent of canonical metadata.
use super::*;
use octacity_server_store_postgres::{MIGRATOR, migrate};
use serde_json::json;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_metadata_upgrade_keeps_committed_and_rejected_legacy_receipts() {
  let database = TestDatabase::empty().await;
  MIGRATOR.run_to(115, &database.pool).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let (original, association) = linked(&store, &database.pool).await;
  let catalog = store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: original.project_id,
        expected_version: 0,
        definition: serde_json::from_value(json!({"labels":[
        {"id":"size_small","name":"S","retired":false}
      ],"properties":[]}))
        .unwrap(),
      },
      idempotency_key: key("legacy-labels"),
      published_at: at(10),
    }))
    .await
    .unwrap()
    .resource;
  let work = store.factory_run_snapshot(association.run_id).await.unwrap().work;
  let accepted = ApplyIssueClassification {
    association: association.clone(),
    policy: "legacy-triage".parse().unwrap(),
    record: FactoryDigest::from_bytes([81; 32]),
    accepted_at: at(15),
    operation_id: FactoryDigest::from_bytes([82; 32]),
    priority: Some(80),
    size_label: Some("size_small".into()),
    managed_labels: ["size_small".into(), "size_missing".into()].into(),
    applied_at: at(20),
    authority: None,
  };
  let applied = store
    .apply_issue_classification(management_mutation(accepted.clone()))
    .await
    .unwrap();
  assert_eq!(applied.issue.content.priority, Some(80));
  assert_eq!(applied.issue.planning.labels, ["size_small".to_owned()].into());
  assert!(applied.issue.classification.is_empty());
  let rejected = ApplyIssueClassification {
    record: FactoryDigest::from_bytes([83; 32]),
    operation_id: FactoryDigest::from_bytes([84; 32]),
    priority: Some(40),
    size_label: Some("size_missing".into()),
    accepted_at: at(21),
    applied_at: at(22),
    ..accepted.clone()
  };
  assert!(matches!(
    store
      .apply_issue_classification(management_mutation(rejected.clone()))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(
    store.issue(original.project_id, original.id, None).await.unwrap(),
    applied.issue
  );
  migrate(&database.pool).await.unwrap();
  migrate(&database.pool).await.unwrap();
  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  // A rejected operation must stay rejected even after its missing label exists.
  let mut definition = catalog.definition;
  definition.labels.push(IssuePropertyChoice {
    id: "size_missing".into(),
    name: "Now available".into(),
    retired: false,
  });
  restarted
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: original.project_id,
        expected_version: 1,
        definition,
      },
      idempotency_key: key("available-label"),
      published_at: at(23),
    }))
    .await
    .unwrap();
  let mut plan = applied.issue.planning.clone();
  plan.definition_version = 2;
  plan.due_date = Some("2026-12-01".into());
  let planned = restarted
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      assignees: Default::default(),
      intent: EditIssuePlanningIntent {
        project_id: original.project_id,
        id: original.id,
        expected_version: applied.issue.version,
        planning: plan,
      },
      idempotency_key: key("later-planning"),
      edited_at: at(24),
    }))
    .await
    .unwrap()
    .issue;
  let mut canonical = planned.clone();
  canonical.version = planned.version.next().unwrap();
  canonical.updated_at = at(25);
  canonical.classification = serde_json::from_value(json!({"fields":[{
    "field":{"kind":"priority"},"origin":{"kind":"manual","actor":{"kind":"authenticated_management","identity":"operator"}},
    "assessed_version":planned.version,"requirements_version":planned.applicability_version,"assessed_at":at(24)
  }]})).unwrap();
  // Restore a separate later canonical revision; never rewrite an old receipt/revision.
  support::issue_revision::seed(&pool, &canonical).await;

  let replay = restarted
    .apply_issue_classification(management_mutation(ApplyIssueClassification {
      applied_at: at(100),
      ..accepted
    }))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, applied.issue);
  assert!(replay.issue.classification.is_empty());
  assert!(matches!(
    restarted
      .apply_issue_classification(management_mutation(ApplyIssueClassification {
        applied_at: at(101),
        ..rejected.clone()
      }))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert!(matches!(
    restarted
      .apply_issue_classification(management_mutation(ApplyIssueClassification {
        size_label: Some("size_small".into()),
        applied_at: at(102),
        ..rejected
      }))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(
    restarted
      .issue(original.project_id, original.id, Some(original.version))
      .await
      .unwrap(),
    original
  );
  assert_eq!(
    restarted
      .issue(original.project_id, original.id, Some(applied.issue.version))
      .await
      .unwrap(),
    applied.issue
  );
  assert_eq!(
    restarted.issue(original.project_id, original.id, None).await.unwrap(),
    canonical
  );
  assert_eq!(
    restarted.factory_run_snapshot(association.run_id).await.unwrap().work,
    work
  );
  pool.close().await;
  database.cleanup().await;
}
