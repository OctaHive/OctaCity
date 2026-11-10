//! Old and new classification actions share one canonical priority and ordering fence.
use super::*;

#[path = "classification_replay.rs"]
mod replay;

async fn apply_typed(
  store: &PostgresStore,
  association: &IssueWorkAssociation,
) -> (ApplyIssueClassification, IssueRevision) {
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: association.project_id,
        expected_version: 0,
        definition: IssuePropertyDefinitionSet::default(),
      },
      idempotency_key: key("mixed-catalog"),
      published_at: at(10),
    }))
    .await
    .unwrap();
  let request = ApplyIssueClassification {
    association: association.clone(),
    policy: FactoryKey::new("typed-priority-size").unwrap(),
    record: FactoryDigest::from_bytes([191; 32]),
    operation_id: FactoryDigest::from_bytes([192; 32]),
    accepted_at: at(15),
    applied_at: at(20),
    priority: None,
    fields: Some(IssueClassificationUpdate {
      inputs: Default::default(),
      selection: IssueClassificationFieldSelection {
        definition_version: 1,
        priority: true,
        component: false,
        kind: false,
        size: true,
        properties: Default::default(),
      },
      priority: Some("high".into()),
      component: None,
      issue_type: None,
      size: Some(IssueSize::Small),
      properties: Default::default(),
    }),
    size_label: None,
    managed_labels: Default::default(),
    authority: None,
  };
  let issue = store
    .apply_issue_classification(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  (request, issue)
}

fn legacy(typed: &ApplyIssueClassification, association: IssueWorkAssociation) -> ApplyIssueClassification {
  ApplyIssueClassification {
    association,
    record: FactoryDigest::from_bytes([193; 32]),
    operation_id: FactoryDigest::from_bytes([194; 32]),
    priority: Some(80),
    fields: None,
    accepted_at: at(25),
    applied_at: at(30),
    ..typed.clone()
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn newer_legacy_priority_supersedes_a_typed_assessment_of_the_same_reviewed_issue() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, older) = linked(&store, &database.pool).await;
  let newer = parallel_work(&store, &issue, &older).await;
  let (typed, first) = apply_typed(&store, &older).await;
  let applied = store
    .apply_issue_classification(management_mutation(legacy(&typed, newer)))
    .await
    .expect("a newer applicable legacy result can replace the same priority's typed assessment")
    .issue;
  assert_eq!(applied.content.priority, Some(IssuePriority::LegacyNumber(80)));
  assert_eq!(applied.version, first.version.next().unwrap());
  assert_eq!(applied.applicability_version, issue.applicability_version);
  assert_eq!(applied.planning, first.planning);
  assert_eq!(
    store
      .issue(issue.project_id, issue.id, Some(first.version))
      .await
      .unwrap(),
    first
  );
  let late = ApplyIssueClassification {
    record: FactoryDigest::from_bytes([195; 32]),
    operation_id: FactoryDigest::from_bytes([196; 32]),
    accepted_at: at(35),
    applied_at: at(40),
    ..typed
  };
  assert!(matches!(
    store.apply_issue_classification(management_mutation(late)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), applied);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn legacy_priority_still_rejects_a_manual_change_restored_after_the_typed_assessment() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, older) = linked(&store, &database.pool).await;
  let newer = parallel_work(&store, &issue, &older).await;
  let (typed, first) = apply_typed(&store, &older).await;
  let mut content = first.content.clone();
  content.priority = Some(IssuePriority::Choice("low".into()));
  let mut current = first.clone();
  for (value, time) in [(content, 22), (first.content.clone(), 23)] {
    current = store
      .edit_issue(management_mutation(EditIssue {
        intent: EditIssueIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: current.version,
          content: value,
        },
        idempotency_key: key(&format!("manual-priority-{time}")),
        edited_at: at(time),
      }))
      .await
      .unwrap()
      .issue;
  }
  assert_eq!(current.content.priority, first.content.priority);
  assert_eq!(current.applicability_version, first.applicability_version);
  let history = store
    .issue_revisions(issue.project_id, issue.id, None, 100)
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(newer.run_id).await.unwrap();
  assert!(matches!(
    store
      .apply_issue_classification(management_mutation(legacy(&typed, newer.clone())))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), current);
  assert_eq!(
    store
      .issue_revisions(issue.project_id, issue.id, None, 100)
      .await
      .unwrap(),
    history
  );
  let retained = store.factory_run_snapshot(newer.run_id).await.unwrap();
  assert_eq!(retained.work, snapshot.work);
  assert_eq!(retained.outbox, snapshot.outbox);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn legacy_priority_change_removes_stale_typed_attribution_without_rewriting_history() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, older) = linked(&store, &database.pool).await;
  let (typed, first) = apply_typed(&store, &older).await;
  assert_eq!(first.classification.fields.len(), 2);
  let newer = parallel_work(&store, &first, &older).await;
  let work = store.factory_run_snapshot(newer.run_id).await.unwrap().work;
  let command = legacy(&typed, newer.clone());
  let applied = store
    .apply_issue_classification(management_mutation(command.clone()))
    .await
    .unwrap()
    .issue;
  assert_eq!(applied.content.priority, Some(IssuePriority::LegacyNumber(80)));
  assert!(
    applied
      .classification
      .fields
      .iter()
      .all(|field| field.field.identity() != IssueClassificationFieldIdentity::Priority),
    "the old Project-choice assessment cannot describe the new numeric priority"
  );
  let size = first
    .classification
    .fields
    .iter()
    .find(|field| field.field.identity() == IssueClassificationFieldIdentity::Size)
    .unwrap()
    .clone();
  assert_eq!(applied.classification.fields, vec![size]);
  assert_eq!(applied.classification.evidence, first.classification.evidence);
  assert_eq!(applied.classification.dependencies, first.classification.dependencies);
  assert_eq!(applied.planning, first.planning);
  assert_eq!(applied.version, first.version.next().unwrap());
  assert_eq!(applied.applicability_version, first.applicability_version);
  assert_eq!(
    store
      .issue(issue.project_id, issue.id, Some(first.version))
      .await
      .unwrap(),
    first
  );
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .apply_issue_classification(management_mutation(typed))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, first);
  let replay = restarted
    .apply_issue_classification(management_mutation(command))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, applied);
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    applied
  );
  assert_eq!(restarted.factory_run_snapshot(newer.run_id).await.unwrap().work, work);
  database.cleanup().await;
}
