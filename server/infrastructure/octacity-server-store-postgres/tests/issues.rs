#[path = "migrations/snapshot.rs"]
mod snapshot;
mod support;

#[path = "issues/source.rs"]
mod source;

use octacity_server_domain::{EntityKind, IssueId, IssueVersion, ProjectId, ProjectName, Timestamp};
use octacity_server_store::{
  CreateIssue, CreateIssueIntent, CreateProject, IdempotencyKey, IssueContent, IssueStore, IssueWorkflowStore,
  MutationDisposition, ProjectStore, StoreError,
  testing::{InMemoryProjectStore, management_mutation},
};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}
fn draft(project_id: ProjectId, replay: &str) -> CreateIssue {
  CreateIssue {
    assignees: Default::default(),
    id: IssueId::generate(),
    intent: CreateIssueIntent {
      planning: Default::default(),
      project_id,
      content: IssueContent {
        title: "Ручная задача".into(),
        description: String::new(),
        acceptance: None,
        priority: None,
      },
    },
    idempotency_key: key(replay),
    created_at: at(2),
  }
}
async fn project(store: &impl ProjectStore) -> CreateProject {
  let request = CreateProject {
    id: ProjectId::generate(),
    parent_id: None,
    name: ProjectName::new("Manual Issues").unwrap(),
    idempotency_key: key("project"),
    created_at: at(1),
  };
  store
    .create_project(management_mutation(request.clone()))
    .await
    .unwrap();
  request
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn manual_creation_matches_memory_and_replays_after_restart_without_factory() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "first");
  let expected = memory.create_issue(management_mutation(request.clone())).await.unwrap();
  let created = postgres
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(created, expected);
  assert_eq!(created.issue.display_key(), "ISS-1");
  assert_eq!(
    created.issue.status_id,
    postgres
      .default_issue_workflow(project.id)
      .await
      .unwrap()
      .workflow
      .definition
      .initial_status_id
  );
  let restarted = PostgresStore::new(database.pool.clone());
  let mut retry = request.clone();
  retry.id = IssueId::generate();
  retry.created_at = at(99);
  let replay = restarted.create_issue(management_mutation(retry)).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, created.issue);
  assert_eq!(
    restarted.issue(project.id, created.issue.id, None).await.unwrap(),
    created.issue
  );
  assert_eq!(
    restarted
      .issue(project.id, created.issue.id, Some(IssueVersion::INITIAL))
      .await
      .unwrap(),
    created.issue
  );
  let history = restarted
    .issue_status_history(project.id, created.issue.id, None, 1)
    .await
    .unwrap();
  assert_eq!(
    history,
    memory
      .issue_status_history(project.id, created.issue.id, None, 1)
      .await
      .unwrap()
  );
  assert_eq!(history.items.len(), 1);
  let second = postgres
    .create_issue(management_mutation(draft(project.id, "second")))
    .await
    .unwrap();
  assert_eq!(second.issue.display_key(), "ISS-2");
  assert_eq!(
    restarted
      .issue(ProjectId::generate(), created.issue.id, None)
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn edits_and_moves_match_memory_and_keep_original_replay_after_progress() {
  use octacity_server_store::{EditIssue, EditIssueIntent, TransitionIssue, TransitionIssueIntent};
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "draft");
  let created = postgres
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(request)).await.unwrap();
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      content: IssueContent {
        description: "Уточнение требований".into(),
        ..created.content.clone()
      },
    },
    idempotency_key: key("edit"),
    edited_at: at(3),
  };
  let expected = memory.edit_issue(management_mutation(edit.clone())).await.unwrap();
  let edited = postgres.edit_issue(management_mutation(edit.clone())).await.unwrap();
  assert_eq!(edited, expected);
  let policy = postgres
    .issue_workflow(project.id, created.workflow_id, Some(created.workflow_version))
    .await
    .unwrap();
  let edge = policy
    .definition
    .transitions
    .iter()
    .find(|edge| {
      edge.from == created.status_id
        && edge.evidence.is_empty()
        && edge
          .origins
          .contains(&octacity_server_store::IssueTransitionOrigin::Manual)
    })
    .unwrap();
  let movement = TransitionIssue::manual(
    TransitionIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: edited.issue.version,
      workflow_id: created.workflow_id,
      workflow_version: created.workflow_version,
      expected_status_id: created.status_id,
      transition_id: edge.id,
      reason: Some("Reviewed".into()),
    },
    key("move"),
    at(4),
  );
  let expected = memory
    .transition_issue(management_mutation(movement.clone()))
    .await
    .unwrap();
  let moved = postgres
    .transition_issue(management_mutation(movement.clone()))
    .await
    .unwrap();
  assert_eq!(moved, expected);
  assert_eq!(moved.issue.version.get(), 3);
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted.edit_issue(management_mutation(edit)).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, edited.issue);
  let replay = restarted
    .transition_issue(management_mutation(movement.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, moved.issue);
  assert_eq!(replay.transition, moved.transition);
  assert_eq!(
    restarted
      .issue(project.id, created.id, Some(created.version))
      .await
      .unwrap(),
    created
  );
  assert_eq!(
    restarted.issue(project.id, created.id, None).await.unwrap(),
    moved.issue
  );
  assert_eq!(
    restarted
      .issue_status_history(project.id, created.id, None, 1)
      .await
      .unwrap()
      .next_cursor,
    Some(IssueVersion::INITIAL)
  );
  let next = restarted
    .issue_status_history(project.id, created.id, Some(IssueVersion::INITIAL), 1)
    .await
    .unwrap();
  assert_eq!(next.items, [moved.transition]);
  assert!(next.next_cursor.is_none());
  let mut changed = movement;
  changed.intent.reason = Some("Changed replay".into());
  assert!(matches!(
    restarted.transition_issue(management_mutation(changed)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn reviewed_adoption_matches_memory_and_keeps_old_policy_after_restart() {
  use octacity_server_store::{
    AdoptIssueWorkflow, AdoptIssueWorkflowIntent, PublishIssueWorkflow, PublishIssueWorkflowIntent,
  };
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "create");
  let created = postgres
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(request)).await.unwrap();
  let initial = postgres.default_issue_workflow(project.id).await.unwrap().workflow;
  let mut definition = initial.definition.clone();
  definition.name = "Reviewed replacement".into();
  let publication = PublishIssueWorkflow {
    intent: PublishIssueWorkflowIntent {
      project_id: project.id,
      id: initial.id,
      expected_current_version: Some(initial.version),
      definition,
    },
    idempotency_key: key("publish"),
    published_at: at(3),
  };
  let target = postgres
    .publish_issue_workflow(management_mutation(publication.clone()))
    .await
    .unwrap()
    .workflow;
  memory
    .publish_issue_workflow(management_mutation(publication))
    .await
    .unwrap();
  let adoption = AdoptIssueWorkflow {
    intent: AdoptIssueWorkflowIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      expected_workflow_id: created.workflow_id,
      expected_workflow_version: created.workflow_version,
      expected_status_id: created.status_id,
      target_workflow_id: target.id,
      target_workflow_version: target.version,
      status_correspondence: [(created.status_id, created.status_id)].into_iter().collect(),
      expected_field_policy: Default::default(),
    },
    idempotency_key: key("adopt"),
    adopted_at: at(4),
  };
  assert_eq!(postgres.issue(project.id, created.id, None).await.unwrap(), created);
  let expected = memory
    .adopt_issue_workflow(management_mutation(adoption.clone()))
    .await
    .unwrap();
  let actual = postgres
    .adopt_issue_workflow(management_mutation(adoption.clone()))
    .await
    .unwrap();
  assert_eq!(actual, expected);
  assert_eq!(actual.issue.applicability_version.get(), 2);
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .adopt_issue_workflow(management_mutation(adoption))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, actual.issue);
  assert_eq!(replay.transition, actual.transition);
  assert_eq!(
    restarted
      .issue(project.id, created.id, Some(created.version))
      .await
      .unwrap(),
    created
  );
  assert_eq!(
    restarted
      .issue_workflow(project.id, initial.id, Some(initial.version))
      .await
      .unwrap(),
    initial
  );
  assert_eq!(
    restarted
      .issue_status_history(project.id, created.id, None, 200)
      .await
      .unwrap(),
    memory
      .issue_status_history(project.id, created.id, None, 200)
      .await
      .unwrap()
  );
  database.cleanup().await;
}

async fn seed_policy(
  database: &TestDatabase,
  issue: &octacity_server_store::IssueRevision,
  policy: &octacity_server_store::IssueFieldPolicy,
) {
  // Trusted association fixture only. Ordinary Issue commands cannot set ownership.
  sqlx::query("UPDATE issues SET field_policy=$3 WHERE project_id=$1 AND id=$2")
    .bind(issue.project_id.as_uuid())
    .bind(issue.id.as_uuid())
    .bind(sqlx::types::Json(policy))
    .execute(&database.pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn retained_source_ownership_blocks_edits_and_moves_without_partial_history() {
  use octacity_server_store::{
    EditIssue, EditIssueIntent, IssueField, IssueFieldPolicy, TransitionIssue, TransitionIssueIntent,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let created = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let policy = IssueFieldPolicy {
    source_owned: [IssueField::Description, IssueField::Status].into_iter().collect(),
    ..Default::default()
  };
  seed_policy(&database, &created, &policy).await;
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      content: IssueContent {
        description: "Must not replace source text".into(),
        ..created.content.clone()
      },
    },
    idempotency_key: key("blocked-edit"),
    edited_at: at(3),
  };
  assert!(matches!(
    store.edit_issue(management_mutation(edit.clone())).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  let workflow = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let movement = TransitionIssue::manual(
    TransitionIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      workflow_id: workflow.id,
      workflow_version: workflow.version,
      expected_status_id: created.status_id,
      transition_id: workflow.definition.transitions[0].id,
      reason: None,
    },
    key("blocked-move"),
    at(3),
  );
  assert!(matches!(
    store.transition_issue(management_mutation(movement)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(store.issue(project.id, created.id, None).await.unwrap(), created);
  assert_eq!(
    store
      .issue_status_history(project.id, created.id, None, 200)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  // Failed attempts reserve neither a revision nor the replay key.
  seed_policy(&database, &created, &Default::default()).await;
  assert_eq!(
    store.edit_issue(management_mutation(edit)).await.unwrap().disposition,
    MutationDisposition::Applied
  );
  database.cleanup().await;
}

fn mapping(name: &str) -> octacity_server_factory::ImmutableReference {
  use octacity_server_factory::{FactoryDigest, FactoryKey, ImmutableReference};
  ImmutableReference::new(
    FactoryKey::new(name).unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([7; 32]),
  )
}
async fn outbox(database: &TestDatabase, id: IssueId, topic: &str) -> Vec<serde_json::Value> {
  sqlx::query_scalar::<_, sqlx::types::Json<serde_json::Value>>(
    "SELECT payload FROM outbox_entries WHERE aggregate_identity=$1 AND topic=$2 ORDER BY created_at,id",
  )
  .bind(id.to_string())
  .bind(topic)
  .fetch_all(&database.pool)
  .await
  .unwrap()
  .into_iter()
  .map(|v| v.0)
  .collect()
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn managed_push_intent_is_retained_once_with_original_mapping_and_no_body() {
  use octacity_server_store::{
    EditIssue, EditIssueIntent, IssueField, IssueFieldPolicy, IssuePushIntent, IssuePushPolicy,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let created = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let policy = IssueFieldPolicy {
    push: Some(IssuePushPolicy {
      mapping: mapping("original"),
      fields: [IssueField::Description].into_iter().collect(),
    }),
    ..Default::default()
  };
  seed_policy(&database, &created, &policy).await;
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      content: IssueContent {
        description: "private-requirements-body".into(),
        ..created.content.clone()
      },
    },
    idempotency_key: key("edit"),
    edited_at: at(3),
  };
  let edited = store.edit_issue(management_mutation(edit.clone())).await.unwrap();
  let records = outbox(&database, created.id, "issue.edited").await;
  assert_eq!(records.len(), 1);
  let expected = IssuePushIntent {
    desired: None,
    project_id: project.id,
    issue_id: created.id,
    version: edited.issue.version,
    mapping: mapping("original"),
    fields: [IssueField::Description].into_iter().collect(),
  };
  assert_eq!(records[0]["push_intent"], serde_json::to_value(expected).unwrap());
  assert!(!records[0].to_string().contains("private-requirements-body"));
  let mut replacement = policy;
  replacement.push.as_mut().unwrap().mapping = mapping("replacement");
  seed_policy(&database, &created, &replacement).await;
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .edit_issue(management_mutation(edit))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(outbox(&database, created.id, "issue.edited").await, records);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_keys_edits_and_status_moves_have_exact_winners() {
  use octacity_server_store::{EditIssue, EditIssueIntent, TransitionIssue, TransitionIssueIntent};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let (left, right) = tokio::join!(
    store.create_issue(management_mutation(draft(project.id, "left"))),
    store.create_issue(management_mutation(draft(project.id, "right")))
  );
  let left = left.unwrap().issue;
  let right = right.unwrap().issue;
  assert_eq!(
    [left.number, right.number]
      .into_iter()
      .collect::<std::collections::BTreeSet<_>>(),
    [1, 2].into_iter().collect()
  );
  let request = draft(project.id, "same-request");
  let mut retry = request.clone();
  retry.id = IssueId::generate();
  retry.created_at = at(99);
  let (first, second) = tokio::join!(
    store.create_issue(management_mutation(request)),
    store.create_issue(management_mutation(retry))
  );
  let outcomes = [first.unwrap(), second.unwrap()];
  assert_eq!(outcomes[0].issue, outcomes[1].issue);
  assert_eq!(outcomes[0].issue.number, 3);
  assert_eq!(
    outcomes
      .iter()
      .filter(|o| o.disposition == MutationDisposition::Applied)
      .count(),
    1
  );
  let edit = |name: &str| EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: left.id,
      expected_version: left.version,
      content: IssueContent {
        title: name.into(),
        ..left.content.clone()
      },
    },
    idempotency_key: key(name),
    edited_at: at(3),
  };
  let (first, second) = tokio::join!(
    store.edit_issue(management_mutation(edit("one"))),
    store.edit_issue(management_mutation(edit("two")))
  );
  let results = [first, second];
  assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
  assert_eq!(
    results
      .iter()
      .filter(|r| matches!(
        r,
        Err(StoreError::Conflict {
          entity: EntityKind::Issue
        })
      ))
      .count(),
    1
  );
  let edited = results.into_iter().find_map(Result::ok).unwrap().issue;
  let workflow = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let movement = |name: &str| {
    TransitionIssue::manual(
      TransitionIssueIntent {
        project_id: project.id,
        id: left.id,
        expected_version: edited.version,
        workflow_id: edited.workflow_id,
        workflow_version: edited.workflow_version,
        expected_status_id: edited.status_id,
        transition_id: workflow.definition.transitions[0].id,
        reason: None,
      },
      key(name),
      at(4),
    )
  };
  let (first, second) = tokio::join!(
    store.transition_issue(management_mutation(movement("move-one"))),
    store.transition_issue(management_mutation(movement("move-two")))
  );
  let results = [first, second];
  assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
  assert_eq!(
    results
      .iter()
      .filter(|r| matches!(
        r,
        Err(StoreError::Conflict {
          entity: EntityKind::Issue
        })
      ))
      .count(),
    1
  );
  assert_eq!(
    store
      .issue_status_history(project.id, left.id, None, 200)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  assert_eq!(outbox(&database, left.id, "issue.edited").await.len(), 1);
  assert_eq!(outbox(&database, left.id, "issue.transitioned").await.len(), 1);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn evidence_failure_rolls_back_revision_key_history_replay_and_push() {
  use octacity_server_store::{
    EditIssue, EditIssueIntent, IssueField, IssueFieldPolicy, IssuePushPolicy, TransitionIssue, TransitionIssueIntent,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let request = draft(project.id, "recover-create");
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT reject_issue_evidence CHECK (topic NOT LIKE 'issue.%') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    store
      .create_issue(management_mutation(request.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(
    store.issue(project.id, request.id, None).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_issue_evidence")
    .execute(&database.pool)
    .await
    .unwrap();
  let created = store.create_issue(management_mutation(request)).await.unwrap().issue;
  assert_eq!(created.number, 1, "a failed publication consumes no key");
  let policy = IssueFieldPolicy {
    push: Some(IssuePushPolicy {
      mapping: mapping("original"),
      fields: [IssueField::Description, IssueField::Status].into_iter().collect(),
    }),
    ..Default::default()
  };
  seed_policy(&database, &created, &policy).await;
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      content: IssueContent {
        description: "atomic document".into(),
        ..created.content.clone()
      },
    },
    idempotency_key: key("recover-edit"),
    edited_at: at(3),
  };
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT reject_issue_evidence CHECK (topic NOT LIKE 'issue.%') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    store.edit_issue(management_mutation(edit.clone())).await.unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.issue(project.id, created.id, None).await.unwrap(), created);
  assert!(outbox(&database, created.id, "issue.edited").await.is_empty());
  assert_eq!(
    store
      .issue(project.id, created.id, Some(IssueVersion::new(2).unwrap()))
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_issue_evidence")
    .execute(&database.pool)
    .await
    .unwrap();
  let edited = store.edit_issue(management_mutation(edit)).await.unwrap();
  assert_eq!(edited.disposition, MutationDisposition::Applied);
  assert_eq!(outbox(&database, created.id, "issue.edited").await.len(), 1);
  let workflow = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let movement = TransitionIssue::manual(
    TransitionIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: edited.issue.version,
      workflow_id: workflow.id,
      workflow_version: workflow.version,
      expected_status_id: created.status_id,
      transition_id: workflow.definition.transitions[0].id,
      reason: None,
    },
    key("recover-move"),
    at(4),
  );
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT reject_issue_evidence CHECK (topic NOT LIKE 'issue.%') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    store
      .transition_issue(management_mutation(movement.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.issue(project.id, created.id, None).await.unwrap(), edited.issue);
  assert_eq!(
    store
      .issue_status_history(project.id, created.id, None, 200)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_issue_evidence")
    .execute(&database.pool)
    .await
    .unwrap();
  let moved = store.transition_issue(management_mutation(movement)).await.unwrap();
  assert_eq!(moved.disposition, MutationDisposition::Applied);
  assert_eq!(
    store
      .issue_status_history(project.id, created.id, None, 200)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  assert_eq!(outbox(&database, created.id, "issue.transitioned").await.len(), 1);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn upgrade_from_workflow_schema_can_adopt_an_unmaterialized_default() {
  use octacity_server_store::{
    AdoptIssueWorkflow, AdoptIssueWorkflowIntent, PublishIssueWorkflow, PublishIssueWorkflowIntent,
    SelectDefaultIssueWorkflow, SelectDefaultIssueWorkflowIntent,
  };
  let database = TestDatabase::empty().await;
  octacity_server_store_postgres::MIGRATOR
    .run_to(67, &database.pool)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let initial = store.default_issue_workflow(project.id).await.unwrap();
  let custom = store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: octacity_server_domain::IssueWorkflowId::generate(),
        expected_current_version: None,
        definition: initial.workflow.definition.clone(),
      },
      idempotency_key: key("custom"),
      published_at: at(2),
    }))
    .await
    .unwrap()
    .workflow;
  store
    .select_default_issue_workflow(management_mutation(SelectDefaultIssueWorkflow {
      intent: SelectDefaultIssueWorkflowIntent {
        project_id: project.id,
        expected_version: initial.version,
        workflow_id: custom.id,
        workflow_version: custom.version,
      },
      idempotency_key: key("select"),
    }))
    .await
    .unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let mut request = draft(project.id, "create-after-upgrade");
  request.created_at = at(3);
  let created = store.create_issue(management_mutation(request)).await.unwrap().issue;
  assert_eq!(created.workflow_id, custom.id);
  let adoption = AdoptIssueWorkflow {
    intent: AdoptIssueWorkflowIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      expected_workflow_id: created.workflow_id,
      expected_workflow_version: created.workflow_version,
      expected_status_id: created.status_id,
      target_workflow_id: initial.workflow.id,
      target_workflow_version: initial.workflow.version,
      status_correspondence: [(created.status_id, created.status_id)].into_iter().collect(),
      expected_field_policy: Default::default(),
    },
    idempotency_key: key("adopt-template"),
    adopted_at: at(4),
  };
  let adopted = store
    .adopt_issue_workflow(management_mutation(adoption.clone()))
    .await
    .unwrap();
  assert_eq!(adopted.issue.workflow_id, initial.workflow.id);
  assert_eq!(adopted.issue.content, created.content);
  assert_eq!(store.default_issue_workflow(project.id).await.unwrap().workflow, custom);
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .adopt_issue_workflow(management_mutation(adoption))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn adoption_rechecks_correspondence_ownership_mapping_and_atomic_publication() {
  use octacity_server_store::{
    AdoptIssueWorkflow, AdoptIssueWorkflowIntent, IssueField, IssueFieldPolicy, IssuePushPolicy, IssueWorkflowPin,
    PublishIssueWorkflow, PublishIssueWorkflowIntent,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let created = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let initial = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let target = store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: initial.id,
        expected_current_version: Some(initial.version),
        definition: initial.definition.clone(),
      },
      idempotency_key: key("replace"),
      published_at: at(3),
    }))
    .await
    .unwrap()
    .workflow;
  let command = AdoptIssueWorkflow {
    intent: AdoptIssueWorkflowIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      expected_workflow_id: created.workflow_id,
      expected_workflow_version: created.workflow_version,
      expected_status_id: created.status_id,
      target_workflow_id: target.id,
      target_workflow_version: target.version,
      status_correspondence: [(created.status_id, target.definition.statuses[1].id)]
        .into_iter()
        .collect(),
      expected_field_policy: Default::default(),
    },
    idempotency_key: key("adopt"),
    adopted_at: at(4),
  };
  for case in 0..5 {
    let mut rejected = command.clone();
    match case {
      0 => rejected.intent.status_correspondence.clear(),
      1 => {
        rejected
          .intent
          .status_correspondence
          .insert(octacity_server_domain::IssueStatusId::generate(), created.status_id);
      }
      2 => {
        rejected
          .intent
          .status_correspondence
          .insert(created.status_id, octacity_server_domain::IssueStatusId::generate());
      }
      3 => rejected.intent.expected_version = IssueVersion::new(2).unwrap(),
      _ => rejected.intent.expected_status_id = target.definition.statuses[1].id,
    }
    assert!(
      matches!(
        store.adopt_issue_workflow(management_mutation(rejected)).await,
        Err(
          StoreError::InvalidInput { .. }
            | StoreError::Conflict {
              entity: EntityKind::Issue
            }
        )
      ),
      "case {case}"
    );
  }
  let owned = IssueFieldPolicy {
    source_owned: [IssueField::Status].into_iter().collect(),
    ..Default::default()
  };
  seed_policy(&database, &created, &owned).await;
  // Both stale ownership review and a correctly reviewed source-owned status are rejected.
  assert!(
    store
      .adopt_issue_workflow(management_mutation(command.clone()))
      .await
      .is_err()
  );
  let mut reviewed = command.clone();
  reviewed.intent.expected_field_policy = owned;
  assert!(matches!(
    store.adopt_issue_workflow(management_mutation(reviewed)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  let mut mapped = IssueFieldPolicy {
    status_mapping_workflow: Some(IssueWorkflowPin {
      id: initial.id,
      version: initial.version,
    }),
    push: Some(IssuePushPolicy {
      mapping: mapping("status-effects"),
      fields: [IssueField::Status].into_iter().collect(),
    }),
    ..Default::default()
  };
  seed_policy(&database, &created, &mapped).await;
  let mut reviewed = command;
  reviewed.intent.expected_field_policy = mapped.clone();
  assert!(matches!(
    store.adopt_issue_workflow(management_mutation(reviewed.clone())).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  mapped.status_mapping_workflow = Some(IssueWorkflowPin {
    id: target.id,
    version: target.version,
  });
  seed_policy(&database, &created, &mapped).await;
  reviewed.intent.expected_field_policy = mapped;
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT reject_adoption CHECK (topic <> 'issue.workflow-adopted') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    store
      .adopt_issue_workflow(management_mutation(reviewed.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.issue(project.id, created.id, None).await.unwrap(), created);
  assert_eq!(
    store
      .issue_status_history(project.id, created.id, None, 200)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  assert!(outbox(&database, created.id, "issue.workflow-adopted").await.is_empty());
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_adoption")
    .execute(&database.pool)
    .await
    .unwrap();
  let adopted = store
    .adopt_issue_workflow(management_mutation(reviewed.clone()))
    .await
    .unwrap();
  assert_eq!(adopted.disposition, MutationDisposition::Applied);
  assert_eq!(adopted.issue.status_id, target.definition.statuses[1].id);
  let records = outbox(&database, created.id, "issue.workflow-adopted").await;
  assert_eq!(records.len(), 1);
  assert!(
    records[0].get("push_intent").is_none(),
    "adoption never dispatches or retains a native effect"
  );
  assert_eq!(
    store
      .adopt_issue_workflow(management_mutation(reviewed))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  let works: i64 = sqlx::query_scalar("SELECT count(*) FROM factory_work_envelopes")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(works, 0);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_replay_is_partitioned_by_security_scope_and_operation() {
  use octacity_server_store::{
    AuditActor, AuditActorKind, EditIssue, EditIssueIntent, ManagementMutation, ManagementSecurityScope,
    MutationAuditContext,
  };
  fn scoped<T>(request: T, scope: &str) -> ManagementMutation<T> {
    ManagementMutation::new(
      request,
      MutationAuditContext::try_new(
        AuditActor {
          kind: AuditActorKind::AuthenticatedManagement,
          identity: Some(scope.into()),
        },
        ManagementSecurityScope::new(scope).unwrap(),
        format!("request-{scope}"),
      )
      .unwrap(),
    )
  }
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let request = draft(project.id, "shared");
  let left = store
    .create_issue(scoped(request.clone(), "operator:left"))
    .await
    .unwrap()
    .issue;
  let mut second = request.clone();
  second.id = IssueId::generate();
  let right = store
    .create_issue(scoped(second.clone(), "operator:right"))
    .await
    .unwrap()
    .issue;
  assert_ne!(left.id, right.id);
  assert_eq!((left.number, right.number), (1, 2));
  let edited = store
    .edit_issue(scoped(
      EditIssue {
        intent: EditIssueIntent {
          project_id: project.id,
          id: left.id,
          expected_version: left.version,
          content: IssueContent {
            title: "Later requirements".into(),
            ..left.content.clone()
          },
        },
        idempotency_key: key("shared"),
        edited_at: at(3),
      },
      "operator:left",
    ))
    .await
    .unwrap();
  assert_eq!(edited.disposition, MutationDisposition::Applied);
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .create_issue(scoped(request.clone(), "operator:left"))
    .await
    .unwrap();
  assert_eq!(replay.issue, left);
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(
    restarted
      .create_issue(scoped(second, "operator:right"))
      .await
      .unwrap()
      .issue,
    right
  );
  let mut changed = request;
  changed.intent.content.title = "Changed intent".into();
  assert!(matches!(
    restarted.create_issue(scoped(changed, "operator:right")).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  let history = restarted
    .issue_status_history(project.id, left.id, None, 200)
    .await
    .unwrap();
  assert_eq!(history.items[0].actor.identity.as_deref(), Some("operator:left"));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_identity_is_globally_unique_and_foreign_reads_never_cross_projects() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  for store in [&memory as &dyn CombinedIssueStore, &postgres as &dyn CombinedIssueStore] {
    let first = CreateProject {
      id: ProjectId::generate(),
      parent_id: None,
      name: ProjectName::new("First").unwrap(),
      idempotency_key: key("first-project"),
      created_at: at(1),
    };
    let second = CreateProject {
      id: ProjectId::generate(),
      parent_id: None,
      name: ProjectName::new("Second").unwrap(),
      idempotency_key: key("second-project"),
      created_at: at(1),
    };
    store.create_project(management_mutation(first.clone())).await.unwrap();
    store.create_project(management_mutation(second.clone())).await.unwrap();
    let request = draft(first.id, "create");
    let created = store
      .create_issue(management_mutation(request.clone()))
      .await
      .unwrap()
      .issue;
    let mut duplicate = request;
    duplicate.intent.project_id = second.id;
    duplicate.idempotency_key = key("duplicate-identity");
    assert_eq!(
      store.create_issue(management_mutation(duplicate)).await.unwrap_err(),
      StoreError::Duplicate {
        entity: EntityKind::Issue
      }
    );
    assert_eq!(
      store.issue(first.id, created.id, Some(created.version)).await.unwrap(),
      created
    );
    assert_eq!(
      store.issue(second.id, created.id, None).await.unwrap_err(),
      StoreError::NotFound {
        entity: EntityKind::Issue
      }
    );
    assert_eq!(
      store
        .issue_status_history(second.id, created.id, None, 1)
        .await
        .unwrap_err(),
      StoreError::NotFound {
        entity: EntityKind::Issue
      }
    );
  }
  database.cleanup().await;
}
trait CombinedIssueStore: ProjectStore + IssueStore {}
impl<T: ProjectStore + IssueStore> CombinedIssueStore for T {}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn moves_and_adoption_complete_with_a_single_database_connection() {
  use octacity_server_store::{
    AdoptIssueWorkflow, AdoptIssueWorkflowIntent, PublishIssueWorkflow, PublishIssueWorkflowIntent, TransitionIssue,
    TransitionIssueIntent,
  };
  let database = TestDatabase::migrated().await;
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(database.pool.connect_options().as_ref().clone())
    .await
    .unwrap();
  let store = PostgresStore::new(pool.clone());
  let project = project(&store).await;
  let created = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let initial = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let target = store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: initial.id,
        expected_current_version: Some(initial.version),
        definition: initial.definition.clone(),
      },
      idempotency_key: key("replace"),
      published_at: at(3),
    }))
    .await
    .unwrap()
    .workflow;
  let moved = store
    .transition_issue(management_mutation(TransitionIssue::manual(
      TransitionIssueIntent {
        project_id: project.id,
        id: created.id,
        expected_version: created.version,
        workflow_id: created.workflow_id,
        workflow_version: created.workflow_version,
        expected_status_id: created.status_id,
        transition_id: initial.definition.transitions[0].id,
        reason: None,
      },
      key("move"),
      at(4),
    )))
    .await
    .unwrap()
    .issue;
  let adopted = store
    .adopt_issue_workflow(management_mutation(AdoptIssueWorkflow {
      intent: AdoptIssueWorkflowIntent {
        project_id: project.id,
        id: moved.id,
        expected_version: moved.version,
        expected_workflow_id: moved.workflow_id,
        expected_workflow_version: moved.workflow_version,
        expected_status_id: moved.status_id,
        target_workflow_id: target.id,
        target_workflow_version: target.version,
        status_correspondence: [(moved.status_id, moved.status_id)].into_iter().collect(),
        expected_field_policy: Default::default(),
      },
      idempotency_key: key("adopt"),
      adopted_at: at(5),
    }))
    .await
    .unwrap();
  assert_eq!(adopted.issue.version.get(), 3);
  assert_eq!(adopted.issue.workflow_version, target.version);
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_discovery_filters_before_pagination_and_rejects_hidden_or_foreign_cursors() {
  use octacity_server_store::{IssueListVisibility, ListProjectIssues};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let mut ids = Vec::new();
  for (index, id) in [
    "00000000-0000-4000-8000-000000000001",
    "00000000-0000-4000-8000-000000000002",
    "00000000-0000-4000-8000-000000000003",
  ]
  .into_iter()
  .enumerate()
  {
    let mut request = draft(project.id, &format!("draft-{index}"));
    request.id = id.parse().unwrap();
    ids.push(request.id);
    store.create_issue(management_mutation(request.clone())).await.unwrap();
    memory.create_issue(management_mutation(request)).await.unwrap();
  }
  let visibility = IssueListVisibility::restricted([ids[1], ids[2]]).unwrap();
  let query = ListProjectIssues::new(project.id, None, 1, visibility.clone()).unwrap();
  let page = store.list_issues(query.clone()).await.unwrap();
  assert_eq!(page, memory.list_issues(query).await.unwrap());
  assert_eq!(page.items.len(), 1);
  assert_eq!(page.items[0].id, ids[1]);
  assert_eq!(page.next_cursor, Some(ids[1]));
  let page = store
    .list_issues(ListProjectIssues::new(project.id, Some(ids[1]), 1, visibility.clone()).unwrap())
    .await
    .unwrap();
  assert_eq!(page.items[0].id, ids[2]);
  assert_eq!(page.next_cursor, None);
  assert!(matches!(
    ListProjectIssues::new(project.id, Some(ids[0]), 1, visibility),
    Err(StoreError::InvalidInput {
      operation: octacity_server_store::StoreOperation::ListProjectIssues,
      ..
    })
  ));
  let foreign = ProjectId::generate();
  assert!(
    store
      .list_issues(ListProjectIssues::new(foreign, Some(ids[1]), 1, IssueListVisibility::all()).unwrap())
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_revision_discovery_matches_memory_without_requirements_bodies() {
  use octacity_server_store::{EditIssue, EditIssueIntent};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "draft");
  let created = store
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(request)).await.unwrap();
  let mut content = created.content.clone();
  content.description = "Private requirements".into();
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: created.id,
      expected_version: created.version,
      content,
    },
    idempotency_key: key("edit"),
    edited_at: at(3),
  };
  store.edit_issue(management_mutation(edit.clone())).await.unwrap();
  memory.edit_issue(management_mutation(edit)).await.unwrap();
  let page = store.issue_revisions(project.id, created.id, None, 1).await.unwrap();
  assert_eq!(
    page,
    memory.issue_revisions(project.id, created.id, None, 1).await.unwrap()
  );
  assert_eq!(page.items.len(), 1);
  assert_eq!(page.items[0].version.get(), 1);
  assert_eq!(page.next_cursor, Some(IssueVersion::INITIAL));
  let page = store
    .issue_revisions(project.id, created.id, page.next_cursor, 1)
    .await
    .unwrap();
  assert_eq!(page.items[0].version.get(), 2);
  assert_eq!(page.next_cursor, None);
  assert!(matches!(
    store.issue_revisions(project.id, created.id, None, 0).await,
    Err(StoreError::InvalidInput {
      operation: octacity_server_store::StoreOperation::ListIssueRevisions,
      ..
    })
  ));
  assert!(
    store
      .issue_revisions(ProjectId::generate(), created.id, None, 1)
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_review_reads_retained_field_policy_without_granting_or_changing_it() {
  use octacity_server_store::{IssueField, IssueFieldPolicy};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let created = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  assert_eq!(
    store.issue_field_policy(project.id, created.id).await.unwrap(),
    IssueFieldPolicy::default()
  );
  let policy = IssueFieldPolicy {
    source_owned: [IssueField::Status].into(),
    ..Default::default()
  };
  sqlx::query("UPDATE issues SET field_policy=$3 WHERE project_id=$1 AND id=$2")
    .bind(project.id.as_uuid())
    .bind(created.id.as_uuid())
    .bind(sqlx::types::Json(&policy))
    .execute(&database.pool)
    .await
    .unwrap();
  assert_eq!(store.issue_field_policy(project.id, created.id).await.unwrap(), policy);
  assert!(
    store
      .issue_field_policy(ProjectId::generate(), created.id)
      .await
      .is_err()
  );
  assert_eq!(store.issue(project.id, created.id, None).await.unwrap(), created);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn manual_planning_is_retained_without_a_directory_or_factory() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let mut request = draft(project.id, "manual-planning");
  request.intent.planning.due_date = Some("2028-02-29".into());
  request.intent.planning.assignee = Some("untrusted-person".into());
  assert!(store.create_issue(management_mutation(request.clone())).await.is_err());
  request.intent.planning.assignee = None;
  let created = store.create_issue(management_mutation(request.clone())).await.unwrap();
  assert_eq!(
    created,
    memory.create_issue(management_mutation(request.clone())).await.unwrap()
  );
  assert_eq!(created.issue.display_key(), "ISS-1");
  assert_eq!(created.issue.planning.due_date.as_deref(), Some("2028-02-29"));
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted.issue(project.id, created.issue.id, None).await.unwrap(),
    created.issue
  );
  assert_eq!(
    restarted
      .create_issue(management_mutation(request))
      .await
      .unwrap()
      .issue,
    created.issue
  );
  database.cleanup().await;
}

#[path = "issues/planning.rs"]
mod planning;

#[path = "issues/discussion.rs"]
mod discussion;

#[path = "issues/activity.rs"]
mod activity;

#[path = "issues/code_review.rs"]
mod code_review;
