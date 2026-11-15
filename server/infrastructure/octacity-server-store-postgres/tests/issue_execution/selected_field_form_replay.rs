//! Fresh concurrent saves and recovery retain one original operation and exact form.
use super::*;
use crate::support::evidence::PostgresEvidenceProbe;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn selected_field_form_concurrent_save_and_restart_replay_preserve_the_original_receipt() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (baseline, mut catalog, command) = fixture(&store, &database.pool).await;
  let probe = PostgresEvidenceProbe(database.pool.clone());
  let before = probe.mutation_evidence_counts().await;
  let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(6));
  let mut saves = Vec::new();
  for _ in 0..6 {
    let store = PostgresStore::new(database.pool.clone());
    let command = command.clone();
    let barrier = barrier.clone();
    saves.push(tokio::spawn(async move {
      barrier.wait().await;
      store.apply_issue_classification(management_mutation(command)).await
    }));
  }
  let mut outcomes = Vec::new();
  for save in saves {
    outcomes.push(save.await.unwrap().unwrap());
  }
  assert_eq!(
    outcomes
      .iter()
      .filter(|o| o.disposition == MutationDisposition::Applied)
      .count(),
    1
  );
  assert_eq!(
    outcomes
      .iter()
      .filter(|o| o.disposition == MutationDisposition::Replayed)
      .count(),
    5
  );
  let original = &outcomes[0].issue;
  assert!(outcomes.iter().all(|outcome| outcome.issue == *original));
  assert_eq!(original.version, baseline.version.next().unwrap());
  let after = probe.mutation_evidence_counts().await;
  assert_eq!(after.idempotency, before.idempotency + 1);
  assert_eq!(after.audit, before.audit + 1);
  assert_eq!(after.outbox, before.outbox + 1);

  let mut planning = original.planning.clone();
  planning
    .values
    .insert("summary".into(), IssuePropertyValue::Text("Human correction".into()));
  let later = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: original.project_id,
        id: original.id,
        expected_version: original.version,
        planning,
      },
      assignees: Default::default(),
      idempotency_key: key("after-form-manual-edit"),
      edited_at: at(21),
    }))
    .await
    .unwrap()
    .issue;
  catalog.definition.properties[0].retired = true;
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: catalog.project_id,
        expected_version: catalog.version,
        definition: catalog.definition,
      },
      idempotency_key: key("after-form-catalog-edit"),
      published_at: at(22),
    }))
    .await
    .unwrap();
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  let pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let probe = PostgresEvidenceProbe(pool.clone());
  let before = probe.mutation_evidence_counts().await;
  let history = restarted
    .issue_revisions(baseline.project_id, baseline.id, None, 100)
    .await
    .unwrap();
  let mut recovery = command.clone();
  recovery.applied_at = at(500);
  recovery.authority = Some(IssueClassificationAuthority {
    claim: FactoryDigest::from_bytes([213; 32]),
    deadline: std::time::Instant::now() - std::time::Duration::from_secs(1),
  });
  let replay = restarted
    .apply_issue_classification(management_mutation(recovery))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(
    replay.issue, *original,
    "recovery returns the original receipt before current definitions/authority"
  );
  let mut different_form = command;
  different_form.fields.as_mut().unwrap().selection.fields[0].required = false;
  assert!(matches!(
    restarted
      .apply_issue_classification(management_mutation(different_form))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(
    restarted.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    later
  );
  assert_eq!(
    restarted
      .issue_revisions(baseline.project_id, baseline.id, None, 100)
      .await
      .unwrap(),
    history
  );
  assert_eq!(probe.mutation_evidence_counts().await, before);
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn selected_field_form_atomic_rejection_and_original_conflict_survive_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (baseline, _, mut invalid) = fixture(&store, &database.pool).await;
  invalid
    .fields
    .as_mut()
    .unwrap()
    .values
    .remove(&"property.failures".parse().unwrap());
  let probe = PostgresEvidenceProbe(database.pool.clone());
  let before = probe.mutation_evidence_counts().await;
  let history = store
    .issue_revisions(baseline.project_id, baseline.id, None, 100)
    .await
    .unwrap();
  assert!(matches!(
    store
      .apply_issue_classification(management_mutation(invalid.clone()))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(
    store.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    baseline
  );
  assert_eq!(
    store
      .issue_revisions(baseline.project_id, baseline.id, None, 100)
      .await
      .unwrap(),
    history
  );
  let rejected = probe.mutation_evidence_counts().await;
  assert_eq!(
    rejected.idempotency,
    before.idempotency + 1,
    "only the rejected operation's receipt is retained"
  );
  assert_eq!(rejected.audit, before.audit);
  assert_eq!(rejected.outbox, before.outbox);

  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  let pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let probe = PostgresEvidenceProbe(pool.clone());
  let mut retry = invalid.clone();
  retry.applied_at = at(500);
  retry.authority = Some(IssueClassificationAuthority {
    claim: FactoryDigest::from_bytes([214; 32]),
    deadline: std::time::Instant::now() - std::time::Duration::from_secs(1),
  });
  assert!(
    matches!(
      restarted.apply_issue_classification(management_mutation(retry)).await,
      Err(StoreError::Conflict {
        entity: EntityKind::Issue
      })
    ),
    "the stored rejection precedes current runtime authority"
  );
  let mut corrected = invalid;
  corrected
    .fields
    .as_mut()
    .unwrap()
    .values
    .insert("property.failures".parse().unwrap(), json!(0));
  assert!(
    matches!(
      restarted
        .apply_issue_classification(management_mutation(corrected.clone()))
        .await,
      Err(StoreError::Conflict {
        entity: EntityKind::Issue
      })
    ),
    "a changed result cannot reuse an operation identity"
  );
  assert_eq!(
    restarted.issue(baseline.project_id, baseline.id, None).await.unwrap(),
    baseline
  );
  assert_eq!(
    restarted
      .issue_revisions(baseline.project_id, baseline.id, None, 100)
      .await
      .unwrap(),
    history
  );
  assert_eq!(probe.mutation_evidence_counts().await, rejected);
  corrected.operation_id = FactoryDigest::from_bytes([215; 32]);
  corrected.record = FactoryDigest::from_bytes([216; 32]);
  let saved = restarted
    .apply_issue_classification(management_mutation(corrected))
    .await
    .unwrap();
  assert_eq!(saved.disposition, MutationDisposition::Applied);
  assert_eq!(saved.issue.planning.values["failures"], IssuePropertyValue::Number(0));
  assert_eq!(saved.issue.version, baseline.version.next().unwrap());
  pool.close().await;
  database.cleanup().await;
}
