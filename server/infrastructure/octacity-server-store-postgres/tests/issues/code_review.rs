use super::*;
use octacity_server_store::*;
fn attach(issue: &IssueRevision, replay: &str, review: &str) -> AttachIssueCodeReview {
  let selection: IssueCodeReviewSelection = serde_json::from_value(serde_json::json!({"source":{"installation":{"identity":"independent-code-forge","version":"v1","digest":"01".repeat(32)},"capability":{"identity":"read-reviews","version":"v1","digest":"02".repeat(32)},"configuration":{"identity":"review-config","version":"v1","digest":"03".repeat(32)},"resource":{"identity":"code-repository","version":"v1","digest":"04".repeat(32)}},"item":{"resource":"native-repository","item":review}})).unwrap();
  AttachIssueCodeReview {
    intent: AttachIssueCodeReviewIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_version: issue.version,
      selection: selection.clone(),
      binding: None,
    },
    observation: IssueCodeReviewObservation {
      selection,
      observed_at: at(3),
      title: Some("Code review".parse().unwrap()),
      number: Some("42".parse().unwrap()),
      url: None,
      author: None,
      state: Some(IssueCodeReviewState::Open),
      draft: Some(false),
      head: Some("old-head".parse().unwrap()),
      base: None,
      changed_files: None,
      additions: None,
      deletions: None,
      review: None,
      checks: None,
    },
    provenance: IssueCodeReviewProvenance::Explicit {
      actor: management_mutation(()).audit().actor().clone(),
    },
    idempotency_key: key(replay),
    attached_at: at(4),
  }
}
fn read(issue: &IssueRevision, limit: u16) -> ReadIssueCodeReviews {
  ReadIssueCodeReviews {
    project_id: issue.project_id,
    issue_id: issue.id,
    scope: management_mutation(()).audit().security_scope().clone(),
    boundary: None,
    after: None,
    limit,
    visibility: IssueCodeReviewRepositoryVisibility::all(),
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_reviews_match_memory_and_recover_original_receipts_and_head_history_after_restart() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let draft = draft(project.id, "manual-review");
  let issue = postgres
    .create_issue(management_mutation(draft.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(draft)).await.unwrap();
  let request = attach(&issue, "attach-review", "pr-42");
  let expected = memory
    .attach_issue_code_review(management_mutation(request.clone()))
    .await
    .unwrap();
  let original = postgres
    .attach_issue_code_review(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(original, expected);
  let mut observation = request.observation.clone();
  observation.head = Some("new-head".parse().unwrap());
  observation.observed_at = at(5);
  observation.state = Some(IssueCodeReviewState::Merged);
  let refresh = ObserveIssueCodeReview {
    intent: ObserveIssueCodeReviewIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      association_id: original.association.id,
    },
    observation,
    idempotency_key: key("refresh-review"),
    recorded_at: at(6),
  };
  let expected_refresh = memory
    .observe_issue_code_review(management_mutation(refresh.clone()))
    .await
    .unwrap();
  let observed = postgres
    .observe_issue_code_review(management_mutation(refresh.clone()))
    .await
    .unwrap();
  assert_eq!(observed, expected_refresh);
  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let mut retry = request.clone();
  retry.attached_at = at(99);
  retry.observation.head = Some("unrelated-later-head".parse().unwrap());
  assert_eq!(
    restarted
      .attach_issue_code_review(management_mutation(retry))
      .await
      .unwrap()
      .association,
    original.association
  );
  let mut refresh_retry = refresh.clone();
  refresh_retry.recorded_at = at(99);
  refresh_retry.observation.head = Some("unrelated-later-head".parse().unwrap());
  assert_eq!(
    restarted
      .observe_issue_code_review(management_mutation(refresh_retry))
      .await
      .unwrap()
      .observation,
    observed.observation
  );
  let first = restarted
    .issue_code_review_observations(read(&issue, 1), original.association.id)
    .await
    .unwrap();
  assert_eq!(first.items[0].observation.head.as_ref().unwrap().as_str(), "old-head");
  let mut continuation = read(&issue, 1);
  continuation.boundary = Some(first.boundary);
  continuation.after = first.next;
  let next = restarted
    .issue_code_review_observations(continuation, original.association.id)
    .await
    .unwrap();
  assert_eq!(next.items[0].observation.head.as_ref().unwrap().as_str(), "new-head");
  assert!(next.next.is_none());
  assert_eq!(
    restarted.issue_code_reviews(read(&issue, 10)).await.unwrap().items,
    memory.issue_code_reviews(read(&issue, 10)).await.unwrap().items
  );
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  let mut foreign = read(&issue, 10);
  foreign.scope = ManagementSecurityScope::new("foreign-reader").unwrap();
  assert!(
    restarted
      .issue_code_reviews(foreign.clone())
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert!(
    restarted
      .issue_code_review_observations(foreign, original.association.id)
      .await
      .is_err()
  );
  let mut changed = request;
  changed.intent.selection.item.item = "pr-999".parse().unwrap();
  assert!(
    restarted
      .attach_issue_code_review(management_mutation(changed))
      .await
      .is_err()
  );
  pool.close().await;
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_reviews_concurrent_canonical_attach_and_observation_membership_are_bounded_and_immutable() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "manual-review")))
    .await
    .unwrap()
    .issue;
  let a = attach(&issue, "a", "pr-42");
  let b = attach(&issue, "b", "pr-42");
  let (a, b) = tokio::join!(
    store.attach_issue_code_review(management_mutation(a)),
    store.attach_issue_code_review(management_mutation(b))
  );
  let a = a.unwrap();
  let b = b.unwrap();
  assert_eq!(a.association, b.association);
  assert_eq!(
    [a.disposition, b.disposition]
      .iter()
      .filter(|d| **d == MutationDisposition::Applied)
      .count(),
    1
  );
  assert_eq!(store.issue_code_reviews(read(&issue, 10)).await.unwrap().items.len(), 1);
  store
    .attach_issue_code_review(management_mutation(attach(&issue, "second", "mr-77")))
    .await
    .unwrap();
  let first = store.issue_code_reviews(read(&issue, 1)).await.unwrap();
  store
    .attach_issue_code_review(management_mutation(attach(&issue, "later", "review-88")))
    .await
    .unwrap();
  let mut continuation = read(&issue, 1);
  continuation.boundary = Some(first.boundary);
  continuation.after = first.next;
  let next = store.issue_code_reviews(continuation).await.unwrap();
  assert_eq!(next.items.len(), 1);
  assert_eq!(next.items[0].association.selection.item.item.as_str(), "mr-77");
  assert!(next.next.is_none());
  for limit in [0, 201] {
    assert!(store.issue_code_reviews(read(&issue, limit)).await.is_err());
  }
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_review_repository_visibility_precedes_watermarks_paging_and_observation_lookup_in_both_stores() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let draft = draft(project.id, "visibility-task");
  let issue = postgres
    .create_issue(management_mutation(draft.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(draft)).await.unwrap();
  let hidden = attach(&issue, "hidden", "hidden-pr");
  let mut visible = attach(&issue, "visible", "visible-mr");
  visible.intent.selection.item.resource = "another-native-code-repository".parse().unwrap();
  visible.observation.selection = visible.intent.selection.clone();
  for store in [
    &postgres as &dyn IssueCodeReviewStore,
    &memory as &dyn IssueCodeReviewStore,
  ] {
    let original = store
      .attach_issue_code_review(management_mutation(hidden.clone()))
      .await
      .unwrap();
    store
      .attach_issue_code_review(management_mutation(visible.clone()))
      .await
      .unwrap();
    let mut request = read(&issue, 1);
    request.visibility =
      IssueCodeReviewRepositoryVisibility::restricted([visible.intent.selection.repository_identity()]).unwrap();
    let page = store.issue_code_reviews(request.clone()).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].association.selection.item.item.as_str(), "visible-mr");
    assert!(page.next.is_none());
    assert!(
      store
        .issue_code_review_observations(request, original.association.id)
        .await
        .is_err()
    );
    let mut none = read(&issue, 1);
    none.visibility = IssueCodeReviewRepositoryVisibility::none();
    let empty = store.issue_code_reviews(none).await.unwrap();
    assert!(empty.items.is_empty());
    assert_eq!(empty.boundary, 0);
    assert!(empty.next.is_none());
  }
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_review_continuations_reject_future_positions_and_watermarks_in_memory_and_postgres() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let draft = draft(project.id, "cursor-task");
  let issue = postgres
    .create_issue(management_mutation(draft.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(draft)).await.unwrap();
  for store in [
    &postgres as &dyn IssueCodeReviewStore,
    &memory as &dyn IssueCodeReviewStore,
  ] {
    let original = store
      .attach_issue_code_review(management_mutation(attach(&issue, "attached", "pr-42")))
      .await
      .unwrap();
    for (boundary, after) in [(Some(999), None), (Some(1), Some(2)), (None, Some(u64::MAX))] {
      let mut request = read(&issue, 1);
      request.boundary = boundary;
      request.after = after;
      assert!(store.issue_code_reviews(request.clone()).await.is_err());
      assert!(
        store
          .issue_code_review_observations(request, original.association.id)
          .await
          .is_err()
      );
    }
  }
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_review_publication_recovers_each_common_transaction_failure_without_retargeting_candidate_lineage() {
  use octacity_server_factory::{CandidateSubject, ExactSubject, FactoryDigest, FactoryRunId, WorkEnvelopeId};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "failure-task")))
    .await
    .unwrap()
    .issue;
  let mut request = attach(&issue, "recover-attachment", "pr-42");
  request.observation.title = Some("never-in-generic-review-evidence".parse().unwrap());
  let binding = IssueCodeReviewBinding {
    association: IssueWorkAssociation {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: None,
      requirements_version: None,
      work_id: WorkEnvelopeId::generate(),
      run_id: FactoryRunId::generate(),
    },
    record: FactoryDigest::from_bytes([73; 32]),
    candidate: CandidateSubject::new(
      ExactSubject::new(
        issue.project_id,
        octacity_server_domain::RepositoryId::generate(),
        "base".parse().unwrap(),
      ),
      "accepted-head".parse().unwrap(),
      FactoryDigest::from_bytes([74; 32]),
    ),
  };
  request.intent.binding = Some(binding.clone());
  request.observation.head = Some("accepted-head".parse().unwrap());
  for (table, condition) in [
    ("audit_facts", "operation <> 'attach-issue-code-review'"),
    ("outbox_entries", "topic <> 'issue.code-review-attached'"),
    (
      "idempotency_records",
      "scope <> 'attach-issue-code-review' OR COALESCE(outcome->>'status','') = 'pending'",
    ),
  ] {
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "ALTER TABLE {table} ADD CONSTRAINT interrupt_review CHECK ({condition}) NOT VALID"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
    assert_eq!(
      store
        .attach_issue_code_review(management_mutation(request.clone()))
        .await
        .unwrap_err(),
      StoreError::Unavailable
    );
    assert!(
      store
        .issue_code_reviews(read(&issue, 10))
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(
      store
        .issue_code_review_replay(
          management_mutation(()).audit().security_scope(),
          &request.intent,
          &request.idempotency_key
        )
        .await
        .unwrap()
        .is_none()
    );
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "ALTER TABLE {table} DROP CONSTRAINT interrupt_review"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
  }
  let original = store
    .attach_issue_code_review(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(original.association.sequence, 1);
  let mut observation = request.observation.clone();
  observation.head = Some("new-remote-head".parse().unwrap());
  observation.observed_at = at(5);
  let refresh = ObserveIssueCodeReview {
    intent: ObserveIssueCodeReviewIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      association_id: original.association.id,
    },
    observation,
    idempotency_key: key("recover-observation"),
    recorded_at: at(6),
  };
  for (table, condition) in [
    ("audit_facts", "operation <> 'observe-issue-code-review'"),
    ("outbox_entries", "topic <> 'issue.code-review-observed'"),
    (
      "idempotency_records",
      "scope <> 'observe-issue-code-review' OR COALESCE(outcome->>'status','') = 'pending'",
    ),
  ] {
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "ALTER TABLE {table} ADD CONSTRAINT interrupt_review CHECK ({condition}) NOT VALID"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
    assert_eq!(
      store
        .observe_issue_code_review(management_mutation(refresh.clone()))
        .await
        .unwrap_err(),
      StoreError::Unavailable
    );
    let history = store
      .issue_code_review_observations(read(&issue, 10), original.association.id)
      .await
      .unwrap();
    assert_eq!(history.items.len(), 1);
    assert_eq!(
      history.items[0].observation.head.as_ref().unwrap().as_str(),
      "accepted-head"
    );
    assert!(
      store
        .issue_code_review_observation_replay(
          management_mutation(()).audit().security_scope(),
          &refresh.intent,
          &refresh.idempotency_key
        )
        .await
        .unwrap()
        .is_none()
    );
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "ALTER TABLE {table} DROP CONSTRAINT interrupt_review"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
  }
  let restarted = PostgresStore::new(database.pool.clone());
  let observed = restarted
    .observe_issue_code_review(management_mutation(refresh))
    .await
    .unwrap();
  assert_eq!(observed.observation.sequence, 2);
  let current = restarted.issue_code_reviews(read(&issue, 10)).await.unwrap();
  assert_eq!(current.items[0].association.binding, Some(binding.clone()));
  assert_eq!(
    current.items[0]
      .observation
      .as_ref()
      .unwrap()
      .head
      .as_ref()
      .unwrap()
      .as_str(),
    "new-remote-head"
  );
  let mut retarget = request;
  retarget.idempotency_key = key("retarget");
  retarget.intent.binding.as_mut().unwrap().record = FactoryDigest::from_bytes([75; 32]);
  assert!(matches!(
    restarted.attach_issue_code_review(management_mutation(retarget)).await,
    Err(StoreError::Conflict { .. })
  ));
  let leaked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM audit_facts a WHERE to_jsonb(a)::text LIKE $1) OR EXISTS(SELECT 1 FROM outbox_entries o WHERE to_jsonb(o)::text LIKE $1)").bind("%never-in-generic-review-evidence%").fetch_one(&database.pool).await.unwrap();
  assert!(!leaked);
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_reviews_additive_upgrade_preserves_manual_tasks_and_reenters_without_publication() {
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator::with_migrations(
    octacity_server_store_postgres::MIGRATOR
      .iter()
      .filter(|m| m.version <= 89)
      .cloned()
      .collect(),
  );
  octacity_server_store_postgres::run_migrator(&database.pool, &previous)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "legacy-review-task")))
    .await
    .unwrap()
    .issue;
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  assert!(
    store
      .issue_code_reviews(read(&issue, 10))
      .await
      .unwrap()
      .items
      .is_empty()
  );
  let association = store
    .attach_issue_code_review(management_mutation(attach(&issue, "after-upgrade", "mr-77")))
    .await
    .unwrap();
  assert_eq!(association.association.sequence, 1);
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_review_equal_time_observation_replay_cannot_replace_a_later_retained_head_in_either_store() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let draft = draft(project.id, "equal-time-task");
  let issue = postgres
    .create_issue(management_mutation(draft.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(draft)).await.unwrap();
  for store in [
    &postgres as &dyn IssueCodeReviewStore,
    &memory as &dyn IssueCodeReviewStore,
  ] {
    let request = attach(&issue, "old-head", "review-42");
    let original = store
      .attach_issue_code_review(management_mutation(request.clone()))
      .await
      .unwrap();
    let intent = ObserveIssueCodeReviewIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      association_id: original.association.id,
    };
    let mut updated = request.observation.clone();
    updated.head = Some("new-head-at-same-time".parse().unwrap());
    store
      .observe_issue_code_review(management_mutation(ObserveIssueCodeReview {
        intent: intent.clone(),
        observation: updated,
        idempotency_key: key("new-head"),
        recorded_at: at(5),
      }))
      .await
      .unwrap();
    let replay = store
      .observe_issue_code_review(management_mutation(ObserveIssueCodeReview {
        intent,
        observation: request.observation,
        idempotency_key: key("old-head-again"),
        recorded_at: at(6),
      }))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    let current = store.issue_code_reviews(read(&issue, 1)).await.unwrap();
    assert_eq!(
      current.items[0]
        .observation
        .as_ref()
        .unwrap()
        .head
        .as_ref()
        .unwrap()
        .as_str(),
      "new-head-at-same-time"
    );
    assert_eq!(
      store
        .issue_code_review_observations(read(&issue, 10), original.association.id)
        .await
        .unwrap()
        .items
        .len(),
      2
    );
  }
  database.cleanup().await;
}
