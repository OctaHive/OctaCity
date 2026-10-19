//! PostgreSQL planning contracts share the same provider-free requests as memory.
use super::*;
use octacity_server_store::*;

fn publication(project_id: ProjectId, replay: &str) -> PublishIssueProperties {
  PublishIssueProperties {
    intent: PublishIssuePropertiesIntent {
      project_id,
      expected_version: 0,
      definition: serde_json::from_value(serde_json::json!({
        "labels":[{"id":"defect","name":"Дефект","retired":false}],
        "properties":[{"id":"size","name":"Размер","retired":false,
          "kind":{"type":"choice","choices":[{"id":"small","name":"S","retired":false}]}}]
      }))
      .unwrap(),
    },
    idempotency_key: key(replay),
    published_at: at(2),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_catalog_matches_memory_retains_versions_and_replays_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let initial = store.issue_properties(project.id, None).await.unwrap();
  assert_eq!(initial.version, 0);
  assert_eq!(initial, memory.issue_properties(project.id, None).await.unwrap());
  let request = publication(project.id, "catalog");
  let first = store
    .publish_issue_properties(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(
    first,
    memory
      .publish_issue_properties(management_mutation(request.clone()))
      .await
      .unwrap()
  );
  assert_eq!(first.resource.version, 1);
  let mut replacement = request.clone();
  replacement.intent.expected_version = 1;
  replacement.intent.definition.labels[0].retired = true;
  replacement.idempotency_key = key("retire");
  let second = store
    .publish_issue_properties(management_mutation(replacement))
    .await
    .unwrap();
  assert_eq!(second.resource.version, 2);
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(restarted.issue_properties(project.id, Some(0)).await.unwrap(), initial);
  assert_eq!(
    restarted.issue_properties(project.id, Some(1)).await.unwrap(),
    first.resource
  );
  assert_eq!(
    restarted.issue_properties(project.id, None).await.unwrap(),
    second.resource
  );
  let replay = restarted
    .publish_issue_properties(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.resource, first.resource);
  let mut stale = request;
  stale.idempotency_key = key("stale");
  assert_eq!(
    restarted
      .publish_issue_properties(management_mutation(stale))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  database.cleanup().await;
}

fn edit(issue: &IssueRevision, replay: &str) -> EditIssuePlanning {
  EditIssuePlanning {
    intent: EditIssuePlanningIntent {
      project_id: issue.project_id,
      id: issue.id,
      expected_version: issue.version,
      planning: issue.planning.clone(),
    },
    assignees: Default::default(),
    idempotency_key: key(replay),
    edited_at: at(5),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_edits_match_memory_keep_retired_values_and_original_history() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let catalog = publication(project.id, "catalog");
  store
    .publish_issue_properties(management_mutation(catalog.clone()))
    .await
    .unwrap();
  memory
    .publish_issue_properties(management_mutation(catalog.clone()))
    .await
    .unwrap();
  let mut request = draft(project.id, "planned");
  request.assignees.insert("person-1".into());
  request.intent.planning = IssuePlanning {
    definition_version: 1,
    assignee: Some("person-1".into()),
    labels: ["defect".into()].into_iter().collect(),
    values: [("size".into(), IssuePropertyValue::Choice("small".into()))]
      .into_iter()
      .collect(),
    due_date: Some("2028-02-29".into()),
    parent_id: None,
  };
  let first = store
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  assert_eq!(
    first,
    memory
      .create_issue(management_mutation(request.clone()))
      .await
      .unwrap()
      .issue
  );
  let original_history = store
    .issue_status_history(project.id, first.id, None, 10)
    .await
    .unwrap();
  let mut replacement = catalog;
  replacement.intent.expected_version = 1;
  replacement.intent.definition.labels[0].retired = true;
  replacement.intent.definition.properties[0].retired = true;
  replacement.idempotency_key = key("retired");
  store
    .publish_issue_properties(management_mutation(replacement.clone()))
    .await
    .unwrap();
  memory
    .publish_issue_properties(management_mutation(replacement))
    .await
    .unwrap();
  let mut change = edit(&first, "planning");
  change.intent.planning.definition_version = 2;
  change.intent.planning.due_date = None;
  // The person directory is unavailable now; unchanged assignments remain explainable.
  let second = store
    .edit_issue_planning(management_mutation(change.clone()))
    .await
    .unwrap();
  assert_eq!(
    second,
    memory
      .edit_issue_planning(management_mutation(change.clone()))
      .await
      .unwrap()
  );
  assert_eq!(second.issue.version.get(), 2);
  assert_eq!(second.issue.applicability_version, first.applicability_version);
  assert_eq!(second.issue.status_id, first.status_id);
  assert_eq!(second.issue.content, first.content);
  assert_eq!(second.issue.planning.values, first.planning.values);
  assert_eq!(second.issue.planning.assignee, first.planning.assignee);
  assert_eq!(
    store
      .issue_status_history(project.id, first.id, None, 10)
      .await
      .unwrap(),
    original_history
  );
  let mut later = edit(&second.issue, "clear-labels");
  later.intent.planning.labels.clear();
  store.edit_issue_planning(management_mutation(later)).await.unwrap();
  assert_eq!(
    store.issue(project.id, first.id, Some(first.version)).await.unwrap(),
    first
  );
  let replay = store
    .edit_issue_planning(management_mutation(change.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, second.issue);
  change.idempotency_key = key("stale-issue");
  assert_eq!(
    store
      .edit_issue_planning(management_mutation(change))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  let current = store.issue(project.id, first.id, None).await.unwrap();
  let mut stale_catalog = edit(&current, "stale-catalog");
  stale_catalog.intent.planning.definition_version = 1;
  assert_eq!(
    store
      .edit_issue_planning(management_mutation(stale_catalog))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  let mut fresh = request;
  fresh.id = IssueId::generate();
  fresh.idempotency_key = key("new-retired");
  fresh.intent.planning.definition_version = 2;
  assert!(store.create_issue(management_mutation(fresh)).await.is_err());
  let mut owned = edit(&current, "source-owned");
  owned.intent.planning.due_date = Some("2030-01-01".into());
  seed_policy(
    &database,
    &current,
    &IssueFieldPolicy {
      source_owned: [IssueField::DueDate].into_iter().collect(),
      ..Default::default()
    },
  )
  .await;
  assert_eq!(
    store.edit_issue_planning(management_mutation(owned)).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
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
async fn planning_parent_writes_reject_self_foreign_missing_and_concurrent_cycles() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let x = store
    .create_issue(management_mutation(draft(project.id, "x")))
    .await
    .unwrap()
    .issue;
  let y = store
    .create_issue(management_mutation(draft(project.id, "y")))
    .await
    .unwrap()
    .issue;
  let foreign_project = CreateProject {
    name: ProjectName::new("Foreign").unwrap(),
    id: ProjectId::generate(),
    idempotency_key: key("foreign"),
    ..project.clone()
  };
  store
    .create_project(management_mutation(foreign_project.clone()))
    .await
    .unwrap();
  let foreign = store
    .create_issue(management_mutation(draft(foreign_project.id, "foreign")))
    .await
    .unwrap()
    .issue;
  for (target, expected) in [
    (
      x.id,
      StoreError::Conflict {
        entity: EntityKind::Issue,
      },
    ),
    (
      foreign.id,
      StoreError::NotFound {
        entity: EntityKind::Issue,
      },
    ),
    (
      IssueId::generate(),
      StoreError::NotFound {
        entity: EntityKind::Issue,
      },
    ),
  ] {
    let mut change = edit(&x, "invalid-parent");
    change.intent.planning.parent_id = Some(target);
    assert_eq!(
      store
        .edit_issue_planning(management_mutation(change))
        .await
        .unwrap_err(),
      expected
    );
  }
  let mut invalid = draft(project.id, "create-parent");
  invalid.intent.planning.parent_id = Some(invalid.id);
  assert_eq!(
    store
      .create_issue(management_mutation(invalid.clone()))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  invalid.intent.planning.parent_id = Some(foreign.id);
  assert_eq!(
    store
      .create_issue(management_mutation(invalid.clone()))
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  invalid.intent.planning.parent_id = Some(x.id);
  let child = store.create_issue(management_mutation(invalid)).await.unwrap().issue;
  assert_eq!(child.number, 3, "rejected parents consume no display number");
  let mut left = edit(&x, "x-y");
  left.intent.planning.parent_id = Some(y.id);
  let mut right = edit(&y, "y-x");
  right.intent.planning.parent_id = Some(x.id);
  let (left, right) = tokio::join!(
    store.edit_issue_planning(management_mutation(left)),
    store.edit_issue_planning(management_mutation(right))
  );
  assert_eq!([&left, &right].iter().filter(|r| r.is_ok()).count(), 1);
  assert_eq!(
    [left, right].into_iter().find_map(Result::err).unwrap(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  let x = store.issue(project.id, x.id, None).await.unwrap();
  let y = store.issue(project.id, y.id, None).await.unwrap();
  assert_eq!(x.version.get() + y.version.get(), 3);
  assert!(!(x.planning.parent_id == Some(y.id) && y.planning.parent_id == Some(x.id)));
  assert_eq!(
    store
      .issue_status_history(project.id, x.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_children_filter_before_paging_and_bind_cursors_to_parent() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "parent");
  let parent = store
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(request)).await.unwrap();
  let mut children = Vec::new();
  for i in 0..4 {
    let mut request = draft(project.id, &format!("child-{i}"));
    request.intent.planning.parent_id = Some(parent.id);
    let child = store
      .create_issue(management_mutation(request.clone()))
      .await
      .unwrap()
      .issue;
    assert_eq!(
      child,
      memory.create_issue(management_mutation(request)).await.unwrap().issue
    );
    children.push(child.id);
  }
  children.sort();
  let visible = IssueListVisibility::restricted([children[1], children[3]]).unwrap();
  let page = ListProjectIssues::new(project.id, None, 1, visible.clone()).unwrap();
  let first = store.issue_children(page.clone(), parent.id).await.unwrap();
  assert_eq!(first, memory.issue_children(page, parent.id).await.unwrap());
  assert_eq!(first.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![children[1]]);
  assert_eq!(first.next_cursor, Some(children[1]));
  let page = ListProjectIssues::new(project.id, first.next_cursor, 1, visible).unwrap();
  let last = store.issue_children(page, parent.id).await.unwrap();
  assert_eq!(last.items[0].id, children[3]);
  assert_eq!(last.next_cursor, None);
  for cursor in [parent.id, IssueId::generate()] {
    assert!(
      store
        .issue_children(
          ListProjectIssues::new(project.id, Some(cursor), 1, IssueListVisibility::all()).unwrap(),
          parent.id
        )
        .await
        .is_err()
    );
  }
  assert_eq!(
    store
      .issue_children(
        ListProjectIssues::new(ProjectId::generate(), None, 1, IssueListVisibility::all()).unwrap(),
        parent.id
      )
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  assert!(
    store
      .issue_children(
        ListProjectIssues::new(project.id, None, 1, IssueListVisibility::none()).unwrap(),
        parent.id
      )
      .await
      .unwrap()
      .items
      .is_empty()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_upgrade_preserves_legacy_revisions_and_enforces_retained_references() {
  let database = TestDatabase::empty().await;
  octacity_server_store_postgres::MIGRATOR
    .run_to(68, &database.pool)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let memory = InMemoryProjectStore::new();
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let legacy = memory
    .create_issue(management_mutation(draft(project.id, "legacy")))
    .await
    .unwrap()
    .issue;
  let workflow = store.default_issue_workflow(project.id).await.unwrap().workflow;
  store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: workflow.id,
        expected_current_version: Some(workflow.version),
        definition: workflow.definition.clone(),
      },
      idempotency_key: key("retain-legacy-workflow"),
      published_at: at(2),
    }))
    .await
    .unwrap();
  // Seed the published 0068 contract, whose JSON has no planning field.
  let document = serde_json::to_value(&legacy).unwrap();
  assert!(document.get("planning").is_none());
  let mut tx = database.pool.begin().await.unwrap();
  sqlx::query("INSERT INTO issues(project_id,id,number,current_version) VALUES ($1,$2,1,1)")
    .bind(project.id.as_uuid())
    .bind(legacy.id.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
  sqlx::query("INSERT INTO issue_revisions(project_id,issue_id,version,workflow_id,workflow_version,document) VALUES ($1,$2,1,$3,1,$4)")
    .bind(project.id.as_uuid()).bind(legacy.id.as_uuid()).bind(workflow.id.as_uuid()).bind(sqlx::types::Json(document)).execute(&mut *tx).await.unwrap();
  tx.commit().await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  assert_eq!(store.issue(project.id, legacy.id, None).await.unwrap(), legacy);
  store
    .publish_issue_properties(management_mutation(publication(project.id, "catalog")))
    .await
    .unwrap();
  let mut change = edit(&legacy, "legacy-planning");
  change.intent.planning.definition_version = 1;
  change.intent.planning.due_date = Some("2028-02-29".into());
  store.edit_issue_planning(management_mutation(change)).await.unwrap();
  let retained = sqlx::query("DELETE FROM issue_property_versions WHERE project_id=$1 AND version=1")
    .bind(project.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap_err();
  assert_eq!(
    retained.as_database_error().and_then(|e| e.code()).as_deref(),
    Some("23503")
  );
  let invalid_parent=sqlx::query("UPDATE issue_revisions SET document=jsonb_set(document,'{planning,parent_id}',to_jsonb($1::text)) WHERE project_id=$2 AND issue_id=$3 AND version=2")
    .bind(IssueId::generate().to_string()).bind(project.id.as_uuid()).bind(legacy.id.as_uuid()).execute(&database.pool).await.unwrap_err();
  assert_eq!(
    invalid_parent.as_database_error().and_then(|e| e.code()).as_deref(),
    Some("23503")
  );
  assert_eq!(
    store.issue(project.id, legacy.id, Some(legacy.version)).await.unwrap(),
    legacy
  );
  database.cleanup().await;
}

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
async fn audit(store: &PostgresStore, operation: &str) -> Vec<AuditFact> {
  store
    .list_audit_facts(ListAuditFacts {
      query: AuditFactQuery {
        actor_kind: None,
        actor_identity: None,
        operation: Some(operation.into()),
        target_kind: None,
        target_identity: None,
        request_identity: None,
        occurred_from: None,
        occurred_through: None,
        after: None,
        limit: 100,
      },
      visibility: AuditFactListVisibility::all(),
    })
    .await
    .unwrap()
    .items
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_replay_is_scoped_after_pool_restart_and_competing_definitions_have_one_winner() {
  let mut database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let original = publication(project.id, "shared");
  let first = store
    .publish_issue_properties(scoped(original.clone(), "operator:left"))
    .await
    .unwrap();
  let mut right = original.clone();
  right.intent.expected_version = 1;
  let second = store
    .publish_issue_properties(scoped(right.clone(), "operator:right"))
    .await
    .unwrap();
  assert_eq!(second.disposition, MutationDisposition::Applied);
  assert_eq!(second.resource.version, 2);
  let created = store
    .create_issue(scoped(draft(project.id, "shared"), "operator:left"))
    .await
    .unwrap()
    .issue;
  let mut change = edit(&created, "shared");
  change.intent.planning.definition_version = 2;
  change.intent.planning.due_date = Some("2028-01-01".into());
  let edited = store
    .edit_issue_planning(scoped(change.clone(), "operator:left"))
    .await
    .unwrap();
  assert_eq!(
    store
      .edit_issue_planning(scoped(change.clone(), "operator:right"))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  let mut later = edit(&edited.issue, "shared");
  later.intent.planning.due_date = None;
  let latest = store
    .edit_issue_planning(scoped(later, "operator:right"))
    .await
    .unwrap()
    .issue;
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  database.pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .publish_issue_properties(scoped(original.clone(), "operator:left"))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.resource, first.resource);
  assert_eq!(
    restarted
      .publish_issue_properties(scoped(right, "operator:right"))
      .await
      .unwrap()
      .resource,
    second.resource
  );
  let replay = restarted
    .edit_issue_planning(scoped(change.clone(), "operator:left"))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, edited.issue);
  assert_eq!(restarted.issue(project.id, latest.id, None).await.unwrap(), latest);
  change.intent.planning.due_date = None;
  assert_eq!(
    restarted
      .edit_issue_planning(scoped(change, "operator:left"))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  let mut a = original.clone();
  a.intent.expected_version = 2;
  a.idempotency_key = key("competing-a");
  let mut b = a.clone();
  b.idempotency_key = key("competing-b");
  b.intent.definition.labels[0].name = "Other name".into();
  let (a, b) = tokio::join!(
    restarted.publish_issue_properties(management_mutation(a)),
    restarted.publish_issue_properties(management_mutation(b))
  );
  assert_eq!([&a, &b].iter().filter(|r| r.is_ok()).count(), 1);
  assert_eq!(
    [a, b].into_iter().find_map(Result::err).unwrap(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  assert_eq!(restarted.issue_properties(project.id, None).await.unwrap().version, 3);
  assert_eq!(audit(&restarted, "publish-issue-properties").await.len(), 3);
  assert_eq!(audit(&restarted, "edit-issue-planning").await.len(), 2);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_evidence_failure_publishes_neither_catalog_nor_revision_or_audit() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let request = publication(project.id, "recover-catalog");
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT reject_planning CHECK (topic NOT IN ('issue-properties.published','issue.planning-edited')) NOT VALID").execute(&database.pool).await.unwrap();
  assert_eq!(
    store
      .publish_issue_properties(management_mutation(request.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.issue_properties(project.id, None).await.unwrap().version, 0);
  assert_eq!(
    store.issue_properties(project.id, Some(1)).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );
  assert!(audit(&store, "publish-issue-properties").await.is_empty());
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_planning")
    .execute(&database.pool)
    .await
    .unwrap();
  assert_eq!(
    store
      .publish_issue_properties(management_mutation(request))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Applied
  );
  let created = store
    .create_issue(management_mutation(draft(project.id, "manual")))
    .await
    .unwrap()
    .issue;
  let mut request = edit(&created, "recover-planning");
  request.intent.planning.definition_version = 1;
  request
    .intent
    .planning
    .values
    .insert("size".into(), IssuePropertyValue::Choice("small".into()));
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT reject_planning CHECK (topic <> 'issue.planning-edited') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    store
      .edit_issue_planning(management_mutation(request.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.issue(project.id, created.id, None).await.unwrap(), created);
  assert!(audit(&store, "edit-issue-planning").await.is_empty());
  assert!(outbox(&database, created.id, "issue.planning-edited").await.is_empty());
  assert_eq!(
    store
      .issue_revisions(project.id, created.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_planning")
    .execute(&database.pool)
    .await
    .unwrap();
  let saved = store
    .edit_issue_planning(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(saved.disposition, MutationDisposition::Applied);
  assert_eq!(saved.issue.version.get(), 2);
  assert_eq!(
    store
      .edit_issue_planning(management_mutation(request))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  let facts = audit(&store, "edit-issue-planning").await;
  assert_eq!(facts.len(), 1);
  assert_eq!(facts[0].target_kind, "issue");
  let records = outbox(&database, created.id, "issue.planning-edited").await;
  assert_eq!(records.len(), 1);
  assert_eq!(
    records[0],
    serde_json::json!({"project_id":project.id,"issue_id":created.id,"version":2})
  );
  assert_eq!(
    store
      .issue_status_history(project.id, created.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn planning_catalog_reads_match_memory_without_initialization_or_extra_pool_slots() {
  let database = TestDatabase::migrated().await;
  let options = (*database.pool.connect_options()).clone();
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(options)
    .await
    .unwrap();
  let store = PostgresStore::new(pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  for version in [None, Some(0), Some(1), Some(u64::MAX)] {
    assert_eq!(
      store.issue_properties(project.id, version).await,
      memory.issue_properties(project.id, version).await
    );
  }
  assert_eq!(
    store.issue_properties(ProjectId::generate(), None).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );
  let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM issue_property_versions")
    .fetch_one(&pool)
    .await
    .unwrap();
  assert_eq!(rows, 0, "reads must not initialize a catalog");
  pool.close().await;
  database.cleanup().await;
}
