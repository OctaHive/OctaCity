use super::*;
use octacity_server_domain::IssueCommentId;
use octacity_server_store::{
  AppendIssueComment, AppendIssueCommentIntent, IssueCommentBody, IssueDiscussionLimits, IssueDiscussionStore,
};

fn comment(
  issue: &octacity_server_store::IssueRevision,
  body: &str,
  replay: &str,
  parent_id: Option<IssueCommentId>,
) -> AppendIssueComment {
  AppendIssueComment {
    limits: IssueDiscussionLimits::default(),
    id: IssueCommentId::generate(),
    intent: AppendIssueCommentIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      parent_id,
      body: IssueCommentBody::try_new(body).unwrap(),
    },
    idempotency_key: key(replay),
    created_at: at(3),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_matches_memory_and_recovers_original_receipt_after_lost_acknowledgement() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "draft");
  let issue = postgres
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(request)).await.unwrap();
  let request = comment(&issue, "@agent approve\nТолько обсуждение", "comment", None);
  let expected = memory
    .append_issue_comment(management_mutation(request.clone()))
    .await
    .unwrap();
  let applied = postgres
    .append_issue_comment(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(applied.comment, expected.comment);
  assert_eq!(applied.comment.sequence, 1);
  let restarted = PostgresStore::new(database.pool.clone());
  let mut retry = request.clone();
  retry.id = IssueCommentId::generate();
  retry.created_at = at(99);
  retry.limits = IssueDiscussionLimits::try_new(1, 1).unwrap();
  let replay = restarted
    .append_issue_comment(management_mutation(retry))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.comment, applied.comment);
  let content = restarted
    .issue_comment_body(project.id, issue.id, applied.comment.id)
    .await
    .unwrap();
  assert_eq!(content.body, request.intent.body);
  assert_eq!(content.comment, applied.comment);
  let page = restarted.issue_comments(project.id, issue.id, None, 10).await.unwrap();
  assert_eq!(page.items, vec![applied.comment]);
  assert_eq!(page.next_sequence, None);
  assert_eq!(restarted.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_concurrent_appends_share_lifetime_quota_and_allocate_unique_sequences() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let mut requests: Vec<_> = (0..3)
    .map(|n| {
      let mut request = comment(&issue, "🦀", &format!("concurrent-{n}"), None);
      request.limits = IssueDiscussionLimits::try_new(4, 2).unwrap();
      request
    })
    .collect();
  let (a, b, c) = tokio::join!(
    store.append_issue_comment(management_mutation(requests.remove(0))),
    store.append_issue_comment(management_mutation(requests.remove(0))),
    store.append_issue_comment(management_mutation(requests.remove(0))),
  );
  let outcomes = [a, b, c];
  assert_eq!(outcomes.iter().filter(|o| o.is_ok()).count(), 2);
  assert_eq!(
    outcomes
      .iter()
      .filter(|o| matches!(
        o,
        Err(StoreError::InvalidInput {
          source: octacity_server_store::StoreInputError::IssueDiscussionQuotaReached,
          ..
        })
      ))
      .count(),
    1
  );
  let page = store.issue_comments(project.id, issue.id, None, 1).await.unwrap();
  assert_eq!(page.items[0].sequence, 1);
  assert_eq!(page.next_sequence, Some(1));
  let page = store
    .issue_comments(project.id, issue.id, page.next_sequence, 1)
    .await
    .unwrap();
  assert_eq!(page.items[0].sequence, 2);
  assert_eq!(page.next_sequence, None);
  let other = store
    .create_issue(management_mutation(draft(project.id, "other")))
    .await
    .unwrap()
    .issue;
  let mut oversized = comment(&other, "🦀x", "bounded", None);
  oversized.limits = IssueDiscussionLimits::try_new(4, 2).unwrap();
  assert!(
    store
      .append_issue_comment(management_mutation(oversized))
      .await
      .is_err()
  );
  assert!(
    store
      .issue_comments(project.id, other.id, None, 10)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_replies_require_a_retained_same_issue_root_without_consuming_sequence() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let other = store
    .create_issue(management_mutation(draft(project.id, "other")))
    .await
    .unwrap()
    .issue;
  let root = store
    .append_issue_comment(management_mutation(comment(&issue, "root", "root", None)))
    .await
    .unwrap()
    .comment;
  let reply = store
    .append_issue_comment(management_mutation(comment(&issue, "reply", "reply", Some(root.id))))
    .await
    .unwrap()
    .comment;
  let foreign = store
    .append_issue_comment(management_mutation(comment(&other, "other root", "foreign", None)))
    .await
    .unwrap()
    .comment;
  for parent in [reply.id, foreign.id, IssueCommentId::generate()] {
    assert!(matches!(
      store
        .append_issue_comment(management_mutation(comment(
          &issue,
          "invalid",
          "reusable",
          Some(parent)
        )))
        .await,
      Err(StoreError::InvalidInput {
        source: octacity_server_store::StoreInputError::InvalidIssueDiscussion,
        ..
      })
    ));
  }
  let valid = store
    .append_issue_comment(management_mutation(comment(&issue, "valid", "reusable", Some(root.id))))
    .await
    .unwrap();
  assert_eq!(valid.comment.sequence, 3);
  assert!(store.issue_comment_body(project.id, other.id, root.id).await.is_err());
  assert!(
    store
      .issue_comment_body(ProjectId::generate(), issue.id, root.id)
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_common_publication_rolls_back_allocation_and_keeps_bodies_out_of_receipts_and_evidence() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let request = comment(&issue, "private-body-for-dedicated-reader", "recover", None);
  for table in ["audit_facts", "outbox_entries", "idempotency_records"] {
    let condition = match table {
      "audit_facts" => "operation <> 'append-issue-comment'",
      "outbox_entries" => "topic <> 'issue.comment-appended'",
      _ => "scope <> 'append-issue-comment' OR COALESCE(outcome->>'status','') = 'pending'",
    };
    // Locally selected fixture identifiers only; fail at each common transaction publication stage.
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "ALTER TABLE {table} ADD CONSTRAINT interrupt_discussion CHECK ({condition}) NOT VALID"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
    assert_eq!(
      store
        .append_issue_comment(management_mutation(request.clone()))
        .await
        .unwrap_err(),
      StoreError::Unavailable
    );
    assert!(
      store
        .issue_comments(project.id, issue.id, None, 10)
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(
      store
        .issue_comment_body(project.id, issue.id, request.id)
        .await
        .is_err()
    );
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM audit_facts WHERE operation='append-issue-comment'), (SELECT count(*) FROM outbox_entries WHERE topic='issue.comment-appended'), (SELECT count(*) FROM idempotency_records WHERE scope='append-issue-comment')").fetch_one(&database.pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "ALTER TABLE {table} DROP CONSTRAINT interrupt_discussion"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
  }
  let (first, concurrent) = tokio::join!(
    store.append_issue_comment(management_mutation(request.clone())),
    store.append_issue_comment(management_mutation(request.clone()))
  );
  let first = first.unwrap();
  let concurrent = concurrent.unwrap();
  assert_eq!(first.comment, concurrent.comment);
  assert_ne!(first.disposition, concurrent.disposition);
  assert_eq!(first.comment.sequence, 1);
  let mut changed = request.clone();
  changed.intent.body = IssueCommentBody::try_new("changed-body-forbidden-replay").unwrap();
  assert!(matches!(
    store.append_issue_comment(management_mutation(changed)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  let safe: Vec<(String,)> = sqlx::query_as("SELECT safe_metadata::text FROM audit_facts WHERE operation='append-issue-comment' UNION ALL SELECT payload::text FROM outbox_entries WHERE topic='issue.comment-appended' UNION ALL SELECT outcome::text FROM idempotency_records WHERE scope='append-issue-comment'").fetch_all(&database.pool).await.unwrap();
  assert_eq!(safe.len(), 3);
  for (value,) in safe {
    assert!(!value.contains("private-body-for-dedicated-reader"));
    assert!(!value.contains("changed-body-forbidden-replay"));
  }
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_scoped_replay_keeps_actual_actors_and_conflicts_on_changed_parent_or_target() {
  use octacity_server_store::{
    AuditActor, AuditActorKind, ManagementMutation, ManagementSecurityScope, MutationAuditContext,
  };
  fn scoped(request: AppendIssueComment, actor: &str) -> ManagementMutation<AppendIssueComment> {
    ManagementMutation::new(
      request,
      MutationAuditContext::try_new(
        AuditActor {
          kind: AuditActorKind::AuthenticatedManagement,
          identity: Some(actor.into()),
        },
        ManagementSecurityScope::new(format!("operator:{actor}")).unwrap(),
        format!("request-{actor}"),
      )
      .unwrap(),
    )
  }
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let other = store
    .create_issue(management_mutation(draft(project.id, "other")))
    .await
    .unwrap()
    .issue;
  let request = comment(&issue, "original body", "same-key", None);
  let alice = store
    .append_issue_comment(scoped(request.clone(), "alice"))
    .await
    .unwrap()
    .comment;
  let mut second = request.clone();
  second.id = IssueCommentId::generate();
  let bob = store.append_issue_comment(scoped(second, "bob")).await.unwrap().comment;
  assert_eq!(alice.actor.identity.as_deref(), Some("alice"));
  assert_eq!(bob.actor.identity.as_deref(), Some("bob"));
  assert_eq!(bob.sequence, 2);
  assert_ne!(alice.id, bob.id);
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .append_issue_comment(scoped(request.clone(), "alice"))
      .await
      .unwrap()
      .comment,
    alice
  );
  for (issue_id, parent_id) in [(issue.id, Some(bob.id)), (other.id, None)] {
    let mut changed = request.clone();
    changed.intent.issue_id = issue_id;
    changed.intent.parent_id = parent_id;
    assert!(matches!(
      restarted.append_issue_comment(scoped(changed, "alice")).await,
      Err(StoreError::Conflict {
        entity: EntityKind::Issue
      })
    ));
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_additive_upgrade_keeps_existing_manual_issue_and_is_reentrant() {
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator::with_migrations(
    octacity_server_store_postgres::MIGRATOR
      .iter()
      .filter(|m| m.version <= 88)
      .cloned()
      .collect(),
  );
  octacity_server_store_postgres::run_migrator(&database.pool, &previous)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "legacy")))
    .await
    .unwrap()
    .issue;
  for _ in 0..2 {
    octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  }
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  let first = store
    .append_issue_comment(management_mutation(comment(&issue, "after upgrade", "first", None)))
    .await
    .unwrap();
  assert_eq!(first.comment.sequence, 1);
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_metadata_continuations_beyond_lifetime_range_remain_empty_like_memory() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  store
    .append_issue_comment(management_mutation(comment(&issue, "private", "first", None)))
    .await
    .unwrap();
  let page = store
    .issue_comments(project.id, issue.id, Some(u64::MAX), 1)
    .await
    .unwrap();
  assert!(page.items.is_empty());
  assert_eq!(page.next_sequence, None);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn explicit_content_expiry_keeps_identity_replay_quota_and_hold() {
  use octacity_server_store::*;
  let mut database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let mut create = draft(project.id, "private-draft");
  create.intent.content.description = "Private retained requirements".into();
  create.intent.content.acceptance = Some("Private acceptance".into());
  let issue = store
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  let mut append = comment(&issue, "Private discussion", "private-comment", None);
  append.limits = IssueDiscussionLimits::try_new(100, 1).unwrap();
  let receipt = store
    .append_issue_comment(management_mutation(append.clone()))
    .await
    .unwrap();
  let pending = store
    .request_issue_execution(management_mutation(RequestIssueExecution {
      id: octacity_server_domain::IssueExecutionRequestId::generate(),
      intent: RequestIssueExecutionIntent {
        project_id: project.id,
        issue_id: issue.id,
        expected_version: issue.version,
        classification: None,
        target: IssueExecutionTarget {
          configuration_id: octacity_server_factory::FactoryConfigurationId::generate(),
          configuration_version: octacity_server_factory::FactoryConfigurationVersion::INITIAL,
          repository_id: octacity_server_domain::RepositoryId::generate(),
          repository_version: octacity_server_domain::RepositoryVersion::INITIAL,
          source: RepositorySourceSelection::DefaultReference,
          mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
        },
      },
      idempotency_key: key("pending-before-expiry"),
      requested_at: at(4),
    }))
    .await
    .unwrap()
    .request;
  let plan = IssueContentRetentionPlan {
    scope: IssueContentRetentionScope::Requirements { through: issue.version },
    expires_at: at(10),
    hold: Some(IssueContentHold {
      reason: "investigation".into(),
      expires_at: None,
    }),
  };
  let schedule = SetIssueContentRetention {
    project_id: project.id,
    issue_id: issue.id,
    expected_version: 0,
    plan: plan.clone(),
    idempotency_key: key("retention"),
    configured_at: at(5),
  };
  store
    .set_issue_content_retention(management_mutation(schedule.clone()))
    .await
    .unwrap();
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Discussion {
          through: receipt.comment.sequence,
        },
        ..plan.clone()
      },
      idempotency_key: key("discussion-retention"),
      ..schedule.clone()
    }))
    .await
    .unwrap();
  assert_eq!(store.expire_issue_content(at(20), 1).await.unwrap(), 0);
  assert_eq!(
    store
      .issue_comment_body(project.id, issue.id, receipt.comment.id)
      .await
      .unwrap()
      .body,
    append.intent.body
  );
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      project_id: project.id,
      issue_id: issue.id,
      expected_version: 1,
      plan: IssueContentRetentionPlan { hold: None, ..plan },
      idempotency_key: key("release-hold"),
      configured_at: at(21),
    }))
    .await
    .unwrap();
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Discussion {
          through: receipt.comment.sequence,
        },
        hold: None,
        ..schedule.plan.clone()
      },
      expected_version: 1,
      idempotency_key: key("release-discussion-hold"),
      ..schedule
    }))
    .await
    .unwrap();
  while store.expire_issue_content(at(22), 1).await.unwrap() > 0 {}
  let restarted = PostgresStore::new(database.pool.clone());
  let unavailable = restarted.issue(project.id, issue.id, None).await.unwrap();
  assert!(!unavailable.content_available);
  assert_eq!(unavailable.content.description, "");
  assert_eq!(unavailable.id, issue.id);
  assert_eq!(unavailable.workflow_id, issue.workflow_id);
  assert_eq!(unavailable.status_id, issue.status_id);
  assert_eq!(unavailable.applicability_version, issue.applicability_version);
  let replay = restarted
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap();
  assert_eq!(replay.issue, unavailable);
  assert!(matches!(
    restarted
      .issue_comment_body(project.id, issue.id, receipt.comment.id)
      .await,
    Err(StoreError::NotFound { .. })
  ));
  assert_eq!(
    restarted
      .append_issue_comment(management_mutation(append.clone()))
      .await
      .unwrap()
      .comment,
    receipt.comment
  );
  append.idempotency_key = key("another-comment");
  assert!(
    restarted
      .append_issue_comment(management_mutation(append))
      .await
      .is_err()
  );
  assert_eq!(
    restarted
      .issue_comments(project.id, issue.id, None, 10)
      .await
      .unwrap()
      .items,
    vec![receipt.comment.clone()]
  );
  let snapshot = super::snapshot::DatabaseSnapshot::capture(&mut database).await;
  let restored = snapshot.restore().await;
  let restored_store = PostgresStore::new(restored.pool.clone());
  assert_eq!(
    restored_store
      .issue_execution(project.id, issue.id, pending.id)
      .await
      .unwrap(),
    pending,
    "backup retains the original pending request and frozen digests after body expiry"
  );
  let recovered = restored_store
    .create_issue(management_mutation(create))
    .await
    .unwrap()
    .issue;
  assert_eq!(recovered, unavailable);
  assert_eq!(
    restored_store
      .default_issue_workflow(project.id)
      .await
      .unwrap()
      .workflow
      .id,
    issue.workflow_id
  );
  assert_eq!(
    restored_store
      .issue_status_history(project.id, issue.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  assert_eq!(
    restored_store
      .issue_comments(project.id, issue.id, None, 10)
      .await
      .unwrap()
      .items,
    vec![receipt.comment.clone()]
  );
  assert!(
    restored_store
      .issue_comment_body(project.id, issue.id, receipt.comment.id)
      .await
      .is_err()
  );
  restored.cleanup().await;
  snapshot.cleanup().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn requirements_expiry_does_not_scan_unrelated_replay_history() {
  use octacity_server_store::*;
  let database = TestDatabase::migrated().await;
  let reader = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let store = PostgresStore::new(reader.clone());
  let project = project(&store).await;
  let create = draft(project.id, "indexed-expiry");
  let issue = store
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      project_id: project.id,
      issue_id: issue.id,
      expected_version: 0,
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Requirements { through: issue.version },
        expires_at: at(10),
        hold: None,
      },
      idempotency_key: key("indexed-expiry-plan"),
      configured_at: at(5),
    }))
    .await
    .unwrap();
  // Observe the public operation's I/O, without copying its query or requiring
  // a particular index name. Small fixtures still require an indexed path.
  sqlx::query("SET enable_seqscan=off").execute(&reader).await.unwrap();
  async fn scans(pool: &sqlx::PgPool) -> i64 {
    sqlx::query("SELECT pg_stat_force_next_flush()")
      .execute(pool)
      .await
      .unwrap();
    sqlx::query("SELECT pg_stat_clear_snapshot()")
      .execute(pool)
      .await
      .unwrap();
    sqlx::query_scalar("SELECT seq_scan FROM pg_stat_user_tables WHERE relname='idempotency_records'")
      .fetch_one(pool)
      .await
      .unwrap()
  }
  let before = scans(&reader).await;
  assert_eq!(store.expire_issue_content(at(11), 1).await.unwrap(), 1);
  assert_eq!(
    scans(&reader).await,
    before,
    "bounded task expiry must not scan the shared replay history"
  );
  let replay = store.create_issue(management_mutation(create)).await.unwrap();
  assert!(!replay.issue.content_available);
  reader.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn later_requirements_are_available_while_original_expired_replay_stays_unavailable() {
  use octacity_server_store::*;
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let original = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      project_id: project.id,
      issue_id: original.id,
      expected_version: 0,
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Requirements {
          through: original.version,
        },
        expires_at: at(10),
        hold: None,
      },
      idempotency_key: key("expiry"),
      configured_at: at(3),
    }))
    .await
    .unwrap();
  assert_eq!(store.expire_issue_content(at(10), 1).await.unwrap(), 1);
  let edit = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: original.id,
      expected_version: original.version,
      content: IssueContent {
        title: "New requirements".into(),
        description: "Reviewed again".into(),
        acceptance: Some("New acceptance".into()),
        priority: None,
      },
    },
    idempotency_key: key("new-content"),
    edited_at: at(11),
  };
  let next = store.edit_issue(management_mutation(edit.clone())).await.unwrap().issue;
  assert!(next.content_available);
  assert_eq!(next.content, edit.intent.content);
  assert_eq!(next.version.get(), 2);
  store.expire_issue_content(at(20), 10).await.unwrap();
  assert_eq!(store.issue(project.id, original.id, None).await.unwrap(), next);
  let historical = store
    .issue(project.id, original.id, Some(original.version))
    .await
    .unwrap();
  assert!(!historical.content_available);
  assert_eq!(historical.content.description, "");
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn discussion_expiry_has_independent_deadline_hold_and_version_from_requirements() {
  use octacity_server_store::*;
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "draft")))
    .await
    .unwrap()
    .issue;
  let receipt = store
    .append_issue_comment(management_mutation(comment(
      &issue,
      "Independent discussion",
      "comment",
      None,
    )))
    .await
    .unwrap();
  for (scope, expires_at, hold, replay) in [
    (
      IssueContentRetentionScope::Requirements { through: issue.version },
      at(10),
      Some(IssueContentHold {
        reason: "investigation".into(),
        expires_at: None,
      }),
      "requirements",
    ),
    (
      IssueContentRetentionScope::Discussion {
        through: receipt.comment.sequence,
      },
      at(20),
      None,
      "discussion",
    ),
  ] {
    let state = store
      .set_issue_content_retention(management_mutation(SetIssueContentRetention {
        project_id: project.id,
        issue_id: issue.id,
        expected_version: 0,
        plan: IssueContentRetentionPlan {
          scope,
          expires_at,
          hold,
        },
        idempotency_key: key(replay),
        configured_at: at(5),
      }))
      .await
      .unwrap();
    assert_eq!(state.version, 1, "each lifetime has its own reviewed version");
  }
  assert_eq!(store.expire_issue_content(at(19), 1).await.unwrap(), 0);
  assert_eq!(store.expire_issue_content(at(20), 1).await.unwrap(), 1);
  assert!(
    store
      .issue_comment_body(project.id, issue.id, receipt.comment.id)
      .await
      .is_err()
  );
  assert_eq!(
    store.issue(project.id, issue.id, None).await.unwrap(),
    issue,
    "discussion cleanup preserves requirements under an independent hold"
  );
  assert_eq!(store.expire_issue_content(at(30), 1).await.unwrap(), 0);
  database.cleanup().await;
}
