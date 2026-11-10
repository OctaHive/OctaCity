//! Classification retries use durable original receipts, independently of later live Issue state.
use super::*;
use crate::support::evidence::PostgresEvidenceProbe;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn typed_classification_replay_after_later_edits_and_pool_restart_returns_the_original_outcome() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, association) = linked(&store, &database.pool).await;
  let original_run = store.factory_run_snapshot(association.run_id).await.unwrap();
  let (mut command, _) = apply_typed(&store, &association).await;
  command.operation_id = FactoryDigest::from_bytes([201; 32]);
  command.record = FactoryDigest::from_bytes([202; 32]);
  command.accepted_at = at(16);
  command.applied_at = at(21);
  command.fields.as_mut().unwrap().inputs.subject = Some(FactoryTaskSubject::Exact(
    original_run.work.subject().repository().unwrap().clone(),
  ));
  let applied = store
    .apply_issue_classification(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(
    applied.issue.content.priority,
    Some(IssuePriority::Choice("high".into()))
  );
  assert_eq!(applied.issue.planning.size, Some(IssueSize::Small));
  assert!(
    applied
      .issue
      .classification
      .fields
      .iter()
      .all(|field| field.inputs == command.fields.as_ref().unwrap().inputs)
  );

  let edited = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: applied.issue.version,
        content: IssueContent {
          priority: Some(IssuePriority::Choice("low".into())),
          acceptance: Some("New acceptance".into()),
          ..applied.issue.content.clone()
        },
      },
      idempotency_key: key("manual-after-classification"),
      edited_at: at(22),
    }))
    .await
    .unwrap()
    .issue;
  let mut definitions = store.issue_properties(issue.project_id, None).await.unwrap();
  definitions
    .definition
    .priorities
    .iter_mut()
    .find(|choice| choice.id == "high")
    .unwrap()
    .retired = true;
  definitions.definition.properties.push(IssuePropertyDefinition {
    id: "notes".into(),
    name: "Notes".into(),
    retired: false,
    kind: IssuePropertyKind::Text { max_bytes: 80 },
  });
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: definitions.version,
        definition: definitions.definition,
      },
      idempotency_key: key("later-catalog"),
      published_at: at(23),
    }))
    .await
    .unwrap();
  let later = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: edited.version,
        planning: IssuePlanning {
          definition_version: 2,
          size: Some(IssueSize::Large),
          issue_type: Some("feature_request".into()),
          due_date: Some("2026-12-01".into()),
          values: [("notes".into(), IssuePropertyValue::Text("Keep this manual note".into()))].into(),
          ..edited.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("later-planning"),
      edited_at: at(24),
    }))
    .await
    .unwrap()
    .issue;
  assert_ne!(later.applicability_version, applied.issue.applicability_version);
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  let pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let history = restarted
    .issue_revisions(issue.project_id, issue.id, None, 100)
    .await
    .unwrap();
  let probe = PostgresEvidenceProbe(pool.clone());
  let before = probe.mutation_evidence_counts().await;
  let replay = restarted
    .apply_issue_classification(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, applied.issue);
  let retries: Vec<_> = (0..4)
    .map(|_| {
      let store = PostgresStore::new(pool.clone());
      let command = command.clone();
      tokio::spawn(async move { store.apply_issue_classification(management_mutation(command)).await })
    })
    .collect();
  for retry in retries {
    let replay = retry.await.unwrap().unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, applied.issue);
  }
  let mut recovered = command.clone();
  recovered.applied_at = at(500);
  recovered.authority = Some(IssueClassificationAuthority {
    claim: FactoryDigest::from_bytes([203; 32]),
    deadline: std::time::Instant::now() - std::time::Duration::from_secs(1),
  });
  assert_eq!(
    restarted
      .apply_issue_classification(management_mutation(recovered))
      .await
      .unwrap()
      .issue,
    applied.issue
  );
  let mut changed_reuse = command.clone();
  changed_reuse.fields.as_mut().unwrap().size = Some(IssueSize::Large);
  assert!(matches!(
    restarted
      .apply_issue_classification(management_mutation(changed_reuse))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  let mut changed_pins = command;
  changed_pins.fields.as_mut().unwrap().inputs = Default::default();
  assert!(matches!(
    restarted
      .apply_issue_classification(management_mutation(changed_pins))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), later);
  assert_eq!(
    restarted
      .issue(issue.project_id, issue.id, Some(applied.issue.version))
      .await
      .unwrap(),
    applied.issue
  );
  assert_eq!(
    restarted
      .issue_revisions(issue.project_id, issue.id, None, 100)
      .await
      .unwrap(),
    history
  );
  assert_eq!(
    restarted.factory_run_snapshot(association.run_id).await.unwrap(),
    original_run
  );
  assert_eq!(probe.mutation_evidence_counts().await, before);
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn rejected_typed_classification_replays_its_original_conflict_after_restart_without_rechecking_runtime_authority()
 {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, association) = linked(&store, &database.pool).await;
  let (mut command, applied) = apply_typed(&store, &association).await;
  let current = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: applied.version,
        planning: IssuePlanning {
          size: Some(IssueSize::Large),
          ..applied.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("intervening-size"),
      edited_at: at(21),
    }))
    .await
    .unwrap()
    .issue;
  command.operation_id = FactoryDigest::from_bytes([204; 32]);
  command.record = FactoryDigest::from_bytes([205; 32]);
  command.accepted_at = at(16);
  command.applied_at = at(22);
  assert!(matches!(
    store
      .apply_issue_classification(management_mutation(command.clone()))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  let current = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: current.version,
        content: IssueContent {
          description: "Later requirements".into(),
          ..current.content
        },
      },
      idempotency_key: key("later-requirements"),
      edited_at: at(23),
    }))
    .await
    .unwrap()
    .issue;
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  let pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let history = restarted
    .issue_revisions(issue.project_id, issue.id, None, 100)
    .await
    .unwrap();
  let run = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  let probe = PostgresEvidenceProbe(pool.clone());
  let before = probe.mutation_evidence_counts().await;
  command.applied_at = at(500);
  command.authority = Some(IssueClassificationAuthority {
    claim: FactoryDigest::from_bytes([206; 32]),
    deadline: std::time::Instant::now() - std::time::Duration::from_secs(1),
  });
  for _ in 0..2 {
    assert!(
      matches!(
        restarted
          .apply_issue_classification(management_mutation(command.clone()))
          .await,
        Err(StoreError::Conflict {
          entity: EntityKind::Issue
        })
      ),
      "replay returns the stored conflict before current runtime authority is evaluated"
    );
  }
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    current
  );
  assert_eq!(
    restarted
      .issue_revisions(issue.project_id, issue.id, None, 100)
      .await
      .unwrap(),
    history
  );
  assert_eq!(restarted.factory_run_snapshot(association.run_id).await.unwrap(), run);
  assert_eq!(probe.mutation_evidence_counts().await, before);
  pool.close().await;
  database.cleanup().await;
}
