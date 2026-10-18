mod support;

use octacity_server_domain::{EntityKind, IssueWorkflowId, IssueWorkflowVersion, ProjectId, ProjectName, Timestamp};
use octacity_server_store::{AuditFactListVisibility, AuditFactQuery, AuditFactStore, ListAuditFacts};
use octacity_server_store::{
  CreateProject, IdempotencyKey, IssueWorkflowStore, MutationDisposition, ProjectStore, PublishIssueWorkflow,
  PublishIssueWorkflowIntent, SelectDefaultIssueWorkflow, SelectDefaultIssueWorkflowIntent, StoreError,
  testing::{InMemoryProjectStore, management_mutation},
};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn default_workflow_matches_memory_without_factory_provider_or_administration() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = CreateProject {
    id: ProjectId::generate(),
    parent_id: None,
    name: ProjectName::new("Manual tasks").unwrap(),
    idempotency_key: key("manual-project"),
    created_at: time(1),
  };
  postgres
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let expected = memory.default_issue_workflow(project.id).await.unwrap();
  let actual = postgres.default_issue_workflow(project.id).await.unwrap();
  assert_eq!(actual, expected);
  assert_eq!(actual.version.get(), 1);
  assert_eq!(actual.workflow.published_at, time(1));
  assert_eq!(
    actual.workflow.definition.initial_status_id.to_string(),
    "10000000-0000-4000-8000-000000000001"
  );
  assert_eq!(
    postgres
      .issue_workflow(project.id, actual.workflow.id, None)
      .await
      .unwrap(),
    actual.workflow
  );
  assert_eq!(
    postgres
      .issue_workflow(
        project.id,
        actual.workflow.id,
        Some(IssueWorkflowVersion::new(2).unwrap())
      )
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::IssueWorkflow
    }
  );
  assert_eq!(
    postgres
      .default_issue_workflow(ProjectId::generate())
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn publication_retains_exact_versions_and_replay_after_schema_upgrade_and_restart() {
  let mut database = TestDatabase::empty().await;
  // The immediately preceding development schema has Projects but no Issue workflows.
  octacity_server_store_postgres::MIGRATOR
    .run_to(66, &database.pool)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = store
    .create_project(management_mutation(CreateProject {
      id: ProjectId::generate(),
      parent_id: None,
      name: ProjectName::new("Existing project").unwrap(),
      idempotency_key: key("existing-project"),
      created_at: time(1),
    }))
    .await
    .unwrap()
    .project;
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let original = store.default_issue_workflow(project.id).await.unwrap();
  let mut definition = original.workflow.definition.clone();
  definition.name = "Our local workflow".into();
  definition.statuses.reverse();
  definition.initial_status_id = definition.statuses[1].id;
  definition.terminal_status_ids = vec![];
  let request = PublishIssueWorkflow {
    intent: PublishIssueWorkflowIntent {
      project_id: project.id,
      id: original.workflow.id,
      expected_current_version: Some(original.workflow.version),
      definition: definition.clone(),
    },
    idempotency_key: key("replace-workflow"),
    published_at: time(2),
  };
  let applied = store
    .publish_issue_workflow(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(applied.workflow.version.get(), 2);
  assert_eq!(applied.workflow.definition, definition);
  let mut next = request.clone();
  next.intent.expected_current_version = Some(applied.workflow.version);
  next.intent.definition.name = "Later definition".into();
  next.idempotency_key = key("replace-again");
  next.published_at = time(3);
  let latest = store.publish_issue_workflow(management_mutation(next)).await.unwrap();
  assert_eq!(latest.workflow.version.get(), 3);
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  database.pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(restarted.project(project.id).await.unwrap().project, project);
  assert_eq!(restarted.default_issue_workflow(project.id).await.unwrap(), original);
  for expected in [&original.workflow, &applied.workflow, &latest.workflow] {
    assert_eq!(
      restarted
        .issue_workflow(project.id, expected.id, Some(expected.version))
        .await
        .unwrap(),
      *expected
    );
  }
  assert_eq!(
    restarted
      .issue_workflow(project.id, latest.workflow.id, None)
      .await
      .unwrap(),
    latest.workflow
  );
  let mut replay = request.clone();
  replay.published_at = time(99);
  let replayed = restarted
    .publish_issue_workflow(management_mutation(replay))
    .await
    .unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.workflow, applied.workflow);
  let mut stale = request.clone();
  stale.idempotency_key = key("stale-replacement");
  assert!(matches!(
    restarted.publish_issue_workflow(management_mutation(stale)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::IssueWorkflow
    })
  ));
  let mut changed = request;
  changed.intent.definition.name = "Changed intent".into();
  assert!(matches!(
    restarted.publish_issue_workflow(management_mutation(changed)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::IssueWorkflow
    })
  ));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn default_selection_pins_an_exact_version_and_replays_without_restoring_it() {
  let mut database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = store
    .create_project(management_mutation(CreateProject {
      id: ProjectId::generate(),
      parent_id: None,
      name: ProjectName::new("Custom workflow").unwrap(),
      idempotency_key: key("project"),
      created_at: time(1),
    }))
    .await
    .unwrap()
    .project;
  let original = store.default_issue_workflow(project.id).await.unwrap();
  let mut definition = original.workflow.definition.clone();
  definition.name = "Different workflow".into();
  let custom = store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: IssueWorkflowId::generate(),
        expected_current_version: None,
        definition,
      },
      idempotency_key: key("custom"),
      published_at: time(2),
    }))
    .await
    .unwrap()
    .workflow;
  let request = SelectDefaultIssueWorkflow {
    intent: SelectDefaultIssueWorkflowIntent {
      project_id: project.id,
      expected_version: original.version,
      workflow_id: custom.id,
      workflow_version: custom.version,
    },
    idempotency_key: key("select"),
  };
  let selected = store
    .select_default_issue_workflow(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(selected.disposition, MutationDisposition::Applied);
  assert_eq!(selected.default.version.get(), 2);
  assert_eq!(selected.default.workflow, custom);
  let updated = store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: custom.id,
        expected_current_version: Some(custom.version),
        definition: original.workflow.definition.clone(),
      },
      idempotency_key: key("custom-next"),
      published_at: time(3),
    }))
    .await
    .unwrap();
  assert_eq!(updated.workflow.version.get(), 2);
  assert_eq!(
    store.default_issue_workflow(project.id).await.unwrap(),
    selected.default
  );
  let restored = store
    .select_default_issue_workflow(management_mutation(SelectDefaultIssueWorkflow {
      intent: SelectDefaultIssueWorkflowIntent {
        expected_version: selected.default.version,
        workflow_id: original.workflow.id,
        workflow_version: original.workflow.version,
        ..request.intent.clone()
      },
      idempotency_key: key("restore-initial"),
    }))
    .await
    .unwrap();
  assert_eq!(restored.default.version.get(), 3);
  assert_eq!(restored.default.workflow, original.workflow);
  let options = (*database.pool.connect_options()).clone();
  drop(store);
  database.pool.close().await;
  database.pool = sqlx::PgPool::connect_with(options).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .select_default_issue_workflow(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.default, selected.default);
  assert_eq!(
    restarted.default_issue_workflow(project.id).await.unwrap(),
    restored.default
  );
  let mut stale = request.clone();
  stale.idempotency_key = key("stale-default");
  assert!(matches!(
    restarted
      .select_default_issue_workflow(management_mutation(stale))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::IssueWorkflow
    })
  ));
  let mut changed = request;
  changed.intent.workflow_version = updated.workflow.version;
  assert!(matches!(
    restarted
      .select_default_issue_workflow(management_mutation(changed))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::IssueWorkflow
    })
  ));
  let audit = workflow_audit(&restarted, "select-default-issue-workflow").await;
  assert_eq!(audit.len(), 2, "replay and conflicts append no extra audit facts");
  for fact in audit {
    assert_eq!(fact.target_kind, "project", "default selection belongs to its Project");
    assert_eq!(fact.target_identity, project.id.to_string());
  }
  database.cleanup().await;
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}
fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

async fn workflow_audit(store: &PostgresStore, operation: &str) -> Vec<octacity_server_store::AuditFact> {
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
async fn concurrent_first_publication_and_default_selection_have_one_winner_each() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store, "Concurrent policy").await;
  let original = store.default_issue_workflow(project.id).await.unwrap();
  let mut publications = Vec::new();
  for name in ["First policy", "Second policy"] {
    let mut definition = original.workflow.definition.clone();
    definition.name = name.into();
    publications.push(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project.id,
        id: original.workflow.id,
        expected_current_version: Some(original.workflow.version),
        definition,
      },
      idempotency_key: key(name),
      published_at: time(2),
    });
  }
  let (left, right) = tokio::join!(
    store.publish_issue_workflow(management_mutation(publications[0].clone())),
    store.publish_issue_workflow(management_mutation(publications[1].clone()))
  );
  let results = [left, right];
  assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
  assert_eq!(
    results
      .iter()
      .filter(|r| matches!(
        r,
        Err(StoreError::Conflict {
          entity: EntityKind::IssueWorkflow
        })
      ))
      .count(),
    1
  );
  let winner = results.into_iter().find_map(Result::ok).unwrap();
  assert_eq!(winner.workflow.version.get(), 2);
  assert_eq!(
    store
      .issue_workflow(project.id, original.workflow.id, None)
      .await
      .unwrap(),
    winner.workflow
  );
  assert_eq!(store.default_issue_workflow(project.id).await.unwrap(), original);
  assert_eq!(workflow_audit(&store, "publish-issue-workflow").await.len(), 1);
  let selections: Vec<_> = [original.workflow.version, winner.workflow.version]
    .into_iter()
    .enumerate()
    .map(|(i, workflow_version)| SelectDefaultIssueWorkflow {
      intent: SelectDefaultIssueWorkflowIntent {
        project_id: project.id,
        expected_version: original.version,
        workflow_id: original.workflow.id,
        workflow_version,
      },
      idempotency_key: key(&format!("select-{i}")),
    })
    .collect();
  let (left, right) = tokio::join!(
    store.select_default_issue_workflow(management_mutation(selections[0].clone())),
    store.select_default_issue_workflow(management_mutation(selections[1].clone()))
  );
  let results = [left, right];
  assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
  assert_eq!(
    results
      .iter()
      .filter(|r| matches!(
        r,
        Err(StoreError::Conflict {
          entity: EntityKind::IssueWorkflow
        })
      ))
      .count(),
    1
  );
  let selected = results.into_iter().find_map(Result::ok).unwrap();
  assert_eq!(selected.default.version.get(), 2);
  assert_eq!(
    store.default_issue_workflow(project.id).await.unwrap(),
    selected.default
  );
  assert_eq!(workflow_audit(&store, "select-default-issue-workflow").await.len(), 1);
  // Identical concurrent requests share the one retained receipt, even when
  // server publication timestamps differ and the target has not existed before.
  let mut request = publications.remove(0);
  request.intent.id = IssueWorkflowId::generate();
  request.intent.expected_current_version = None;
  request.idempotency_key = key("concurrent-replay");
  let mut retry = request.clone();
  retry.published_at = time(99);
  let (left, right) = tokio::join!(
    store.publish_issue_workflow(management_mutation(request)),
    store.publish_issue_workflow(management_mutation(retry))
  );
  let outcomes = [left.unwrap(), right.unwrap()];
  assert_eq!(outcomes[0].workflow, outcomes[1].workflow);
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
    1
  );
  assert_eq!(workflow_audit(&store, "publish-issue-workflow").await.len(), 2);
  database.cleanup().await;
}

async fn create_project(store: &PostgresStore, name: &str) -> octacity_server_store::Project {
  store
    .create_project(management_mutation(CreateProject {
      id: ProjectId::generate(),
      parent_id: None,
      name: ProjectName::new(name).unwrap(),
      idempotency_key: key(name),
      created_at: time(1),
    }))
    .await
    .unwrap()
    .project
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn invalid_and_foreign_intents_leave_policy_and_receipts_unchanged() {
  use octacity_server_store::{DeleteProject, StoreInputError, StoreOperation};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store, "Local policy").await;
  let foreign = create_project(&store, "Other project").await;
  let original = store.default_issue_workflow(project.id).await.unwrap();
  let other = store.default_issue_workflow(foreign.id).await.unwrap();
  let mut request = PublishIssueWorkflow {
    intent: PublishIssueWorkflowIntent {
      project_id: project.id,
      id: original.workflow.id,
      expected_current_version: Some(original.workflow.version),
      definition: original.workflow.definition.clone(),
    },
    idempotency_key: key("rejected-publication"),
    published_at: time(2),
  };
  request
    .intent
    .definition
    .terminal_status_ids
    .push(octacity_server_domain::IssueStatusId::generate());
  assert_eq!(
    store
      .publish_issue_workflow(management_mutation(request.clone()))
      .await
      .unwrap_err(),
    StoreError::InvalidInput {
      operation: StoreOperation::PublishIssueWorkflow,
      source: StoreInputError::InvalidIssueWorkflow
    }
  );
  assert_eq!(store.default_issue_workflow(project.id).await.unwrap(), original);
  assert_eq!(
    store
      .issue_workflow(project.id, original.workflow.id, None)
      .await
      .unwrap(),
    original.workflow
  );
  assert!(workflow_audit(&store, "publish-issue-workflow").await.is_empty());
  let selection = SelectDefaultIssueWorkflow {
    intent: SelectDefaultIssueWorkflowIntent {
      project_id: project.id,
      expected_version: original.version,
      workflow_id: other.workflow.id,
      workflow_version: other.workflow.version,
    },
    idempotency_key: key("foreign-default"),
  };
  assert_eq!(
    store
      .select_default_issue_workflow(management_mutation(selection))
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::IssueWorkflow
    }
  );
  assert_eq!(
    store
      .issue_workflow(project.id, other.workflow.id, None)
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::IssueWorkflow
    }
  );
  assert_eq!(store.default_issue_workflow(project.id).await.unwrap(), original);
  assert_eq!(store.default_issue_workflow(foreign.id).await.unwrap(), other);
  assert!(workflow_audit(&store, "select-default-issue-workflow").await.is_empty());
  // Reading a default does not create references that block ordinary Project deletion.
  store
    .delete_project(management_mutation(DeleteProject {
      id: foreign.id,
      expected_version: foreign.version,
      idempotency_key: key("delete-read-only-project"),
      deleted_at: time(2),
    }))
    .await
    .unwrap();
  assert_eq!(
    store.default_issue_workflow(foreign.id).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );
  request.intent.definition = original.workflow.definition;
  let applied = store
    .publish_issue_workflow(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(
    applied.disposition,
    MutationDisposition::Applied,
    "invalid commands reserve no replay receipt"
  );
  let mut absent = request;
  absent.intent.project_id = ProjectId::generate();
  absent.idempotency_key = key("missing-project");
  assert_eq!(
    store
      .publish_issue_workflow(management_mutation(absent))
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );
  assert!(matches!(
    store
      .delete_project(management_mutation(DeleteProject {
        id: project.id,
        expected_version: project.version,
        idempotency_key: key("delete-referenced-project"),
        deleted_at: time(3),
      }))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Project
    })
  ));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn security_scopes_and_operations_partition_replay_without_borrowing_a_receipt() {
  use octacity_server_store::{
    AuditActor, AuditActorKind, ManagementMutation, ManagementSecurityScope, MutationAuditContext,
  };
  fn scoped<T>(request: T, actor: &str) -> ManagementMutation<T> {
    ManagementMutation::new(
      request,
      MutationAuditContext::try_new(
        AuditActor {
          kind: AuditActorKind::AuthenticatedManagement,
          identity: Some(actor.into()),
        },
        ManagementSecurityScope::new(actor).unwrap(),
        format!("request-{actor}"),
      )
      .unwrap(),
    )
  }
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store, "Scoped workflow").await;
  let original = store.default_issue_workflow(project.id).await.unwrap();
  let first = PublishIssueWorkflow {
    intent: PublishIssueWorkflowIntent {
      project_id: project.id,
      id: IssueWorkflowId::generate(),
      expected_current_version: None,
      definition: original.workflow.definition.clone(),
    },
    idempotency_key: key("shared-key"),
    published_at: time(2),
  };
  let mut second = first.clone();
  second.intent.id = IssueWorkflowId::generate();
  let left = store
    .publish_issue_workflow(scoped(first.clone(), "operator:left"))
    .await
    .unwrap();
  let right = store
    .publish_issue_workflow(scoped(second.clone(), "operator:right"))
    .await
    .unwrap();
  assert_ne!(left.workflow.id, right.workflow.id);
  let selection = store
    .select_default_issue_workflow(scoped(
      SelectDefaultIssueWorkflow {
        intent: SelectDefaultIssueWorkflowIntent {
          project_id: project.id,
          expected_version: original.version,
          workflow_id: left.workflow.id,
          workflow_version: left.workflow.version,
        },
        idempotency_key: key("shared-key"),
      },
      "operator:left",
    ))
    .await
    .unwrap();
  assert_eq!(selection.disposition, MutationDisposition::Applied);
  let restarted = PostgresStore::new(database.pool.clone());
  for (request, actor, expected) in [
    (first.clone(), "operator:left", left.workflow),
    (second, "operator:right", right.workflow),
  ] {
    let replay = restarted.publish_issue_workflow(scoped(request, actor)).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.workflow, expected);
  }
  assert!(matches!(
    restarted.publish_issue_workflow(scoped(first, "operator:right")).await,
    Err(StoreError::Conflict {
      entity: EntityKind::IssueWorkflow
    })
  ));
  let facts = workflow_audit(&store, "publish-issue-workflow").await;
  assert_eq!(facts.len(), 2);
  assert_eq!(
    facts
      .iter()
      .map(|f| f.actor.identity.as_deref().unwrap())
      .collect::<std::collections::BTreeSet<_>>(),
    ["operator:left", "operator:right"].into_iter().collect()
  );
  assert_eq!(
    store.default_issue_workflow(project.id).await.unwrap(),
    selection.default
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn evidence_commit_failure_rolls_back_publication_and_selection_with_their_replay_keys() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store, "Atomic workflow").await;
  let original = store.default_issue_workflow(project.id).await.unwrap();
  let request = PublishIssueWorkflow {
    intent: PublishIssueWorkflowIntent {
      project_id: project.id,
      id: original.workflow.id,
      expected_current_version: Some(original.workflow.version),
      definition: original.workflow.definition.clone(),
    },
    idempotency_key: key("retry-after-failed-commit"),
    published_at: time(2),
  };
  // Fault injection is confined to this test's disposable database. Outcomes
  // and retry receipts are observed through public workflow/audit ports.
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT reject_workflow_outbox CHECK (topic <> 'issue-workflow.published')",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    store
      .publish_issue_workflow(management_mutation(request.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(
    store
      .issue_workflow(project.id, original.workflow.id, None)
      .await
      .unwrap(),
    original.workflow
  );
  assert!(workflow_audit(&store, "publish-issue-workflow").await.is_empty());
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_workflow_outbox")
    .execute(&database.pool)
    .await
    .unwrap();
  let applied = store
    .publish_issue_workflow(management_mutation(request))
    .await
    .unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(applied.workflow.version.get(), 2);
  let selection = SelectDefaultIssueWorkflow {
    intent: SelectDefaultIssueWorkflowIntent {
      project_id: project.id,
      expected_version: original.version,
      workflow_id: applied.workflow.id,
      workflow_version: applied.workflow.version,
    },
    idempotency_key: key("retry-after-failed-commit"),
  };
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT reject_workflow_outbox CHECK (topic <> 'issue-workflow.default-selected')")
    .execute(&database.pool).await.unwrap();
  assert_eq!(
    store
      .select_default_issue_workflow(management_mutation(selection.clone()))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.default_issue_workflow(project.id).await.unwrap(), original);
  assert!(workflow_audit(&store, "select-default-issue-workflow").await.is_empty());
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_workflow_outbox")
    .execute(&database.pool)
    .await
    .unwrap();
  let selected = store
    .select_default_issue_workflow(management_mutation(selection))
    .await
    .unwrap();
  assert_eq!(selected.disposition, MutationDisposition::Applied);
  assert_eq!(selected.default.version.get(), 2);
  assert_eq!(selected.default.workflow, applied.workflow);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discovery_applies_visibility_before_cursor_and_includes_the_unconfigured_default() {
  use octacity_server_store::{IssueWorkflowListVisibility, ListProjectIssueWorkflows};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store, "Discovery").await;
  let initial = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let initial_page = store
    .list_issue_workflows(
      ListProjectIssueWorkflows::new(project.id, None, 1, IssueWorkflowListVisibility::all()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(initial_page.items.len(), 1);
  assert_eq!(initial_page.items[0].id, initial.id);
  assert_eq!(initial_page.items[0].version, initial.version);
  assert!(initial_page.next_cursor.is_none());
  let ids: Vec<IssueWorkflowId> = [1_u128, 2, 3]
    .into_iter()
    .map(|id| IssueWorkflowId::from_uuid(uuid::Uuid::from_u128(id)).unwrap())
    .collect();
  for (i, id) in ids.iter().enumerate() {
    store
      .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
        intent: PublishIssueWorkflowIntent {
          project_id: project.id,
          id: *id,
          expected_current_version: None,
          definition: initial.definition.clone(),
        },
        idempotency_key: key(&format!("discover-{i}")),
        published_at: time(2),
      }))
      .await
      .unwrap();
  }
  let visibility = IssueWorkflowListVisibility::restricted([ids[0], ids[2]]).unwrap();
  let first = store
    .list_issue_workflows(ListProjectIssueWorkflows::new(project.id, None, 1, visibility.clone()).unwrap())
    .await
    .unwrap();
  assert_eq!(first.items.iter().map(|w| w.id).collect::<Vec<_>>(), [ids[0]]);
  assert_eq!(first.next_cursor, Some(ids[0]));
  let next = store
    .list_issue_workflows(ListProjectIssueWorkflows::new(project.id, first.next_cursor, 1, visibility).unwrap())
    .await
    .unwrap();
  assert_eq!(next.items.iter().map(|w| w.id).collect::<Vec<_>>(), [ids[2]]);
  assert!(next.next_cursor.is_none());
  let hidden_request =
    ListProjectIssueWorkflows::new(ProjectId::generate(), None, 1, IssueWorkflowListVisibility::none()).unwrap();
  let hidden = store.list_issue_workflows(hidden_request.clone()).await.unwrap();
  assert_eq!(
    hidden,
    InMemoryProjectStore::new()
      .list_issue_workflows(hidden_request)
      .await
      .unwrap()
  );
  assert!(hidden.items.is_empty());
  assert!(hidden.next_cursor.is_none());
  database.cleanup().await;
}
