use super::*;
use octacity_server_domain::IssueCommentId;
use octacity_server_store::{
  AppendIssueComment, AppendIssueCommentIntent, IssueCommentBody, IssueDiscussionStore, IssueTimelineKind,
  IssueTimelineStore, ReadIssueTimeline,
};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn timeline_metadata_matches_memory_under_ties_and_survives_reader_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&store).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let request = draft(project.id, "timeline");
  let issue = store
    .create_issue(management_mutation(request.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(request)).await.unwrap();
  for n in [2_u128, 1] {
    let request = AppendIssueComment {
      limits: Default::default(),
      id: IssueCommentId::from_uuid(uuid::Uuid::from_u128(n)).unwrap(),
      intent: AppendIssueCommentIntent {
        project_id: project.id,
        issue_id: issue.id,
        parent_id: None,
        body: IssueCommentBody::try_new("private comment body").unwrap(),
      },
      idempotency_key: key(&format!("comment-{n}")),
      created_at: issue.created_at,
    };
    store
      .append_issue_comment(management_mutation(request.clone()))
      .await
      .unwrap();
    memory.append_issue_comment(management_mutation(request)).await.unwrap();
  }
  let mut request = ReadIssueTimeline {
    project_id: project.id,
    issue_id: issue.id,
    scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
    kinds: IssueTimelineKind::ALL.into_iter().collect(),
    boundary: None,
    after: None,
    limit: 2,
  };
  let expected = memory.issue_timeline(request.clone()).await.unwrap();
  let first = store.issue_timeline(request.clone()).await.unwrap();
  assert_eq!(first.items, expected.items);
  assert_eq!(first.boundary, expected.boundary);
  assert_eq!(first.next, expected.next);
  request.boundary = Some(first.boundary);
  request.after = first.next;
  let restarted = PostgresStore::new(database.pool.clone());
  let last = restarted.issue_timeline(request.clone()).await.unwrap();
  let expected = memory.issue_timeline(request).await.unwrap();
  assert_eq!(last.items, expected.items);
  assert_eq!(last.next, None);
  assert_eq!(last.items.len(), 2);
  assert!(!format!("{:?}", last.items).contains("private comment body"));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn timeline_keeps_local_metadata_when_source_storage_is_unavailable() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "partial-timeline")))
    .await
    .unwrap()
    .issue;
  sqlx::query("ALTER TABLE issue_source_observations RENAME TO unavailable_observations")
    .execute(&database.pool)
    .await
    .unwrap();
  let request = ReadIssueTimeline {
    project_id: project.id,
    issue_id: issue.id,
    scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
    kinds: IssueTimelineKind::ALL.into_iter().collect(),
    boundary: None,
    after: None,
    limit: 1,
  };
  let first = store.issue_timeline(request.clone()).await.unwrap();
  assert!(first.source_unavailable);
  assert_eq!(first.boundary.source, None);
  assert_eq!(first.items[0].position().kind, IssueTimelineKind::Revision);
  sqlx::query("ALTER TABLE unavailable_observations RENAME TO issue_source_observations")
    .execute(&database.pool)
    .await
    .unwrap();
  let continuation = store
    .issue_timeline(ReadIssueTimeline {
      boundary: Some(first.boundary),
      after: first.next,
      ..request.clone()
    })
    .await
    .unwrap();
  assert!(continuation.source_unavailable);
  assert_eq!(continuation.items[0].position().kind, IssueTimelineKind::Status);
  assert!(!store.issue_timeline(request).await.unwrap().source_unavailable);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn timeline_rejects_fabricated_future_membership_and_unbounded_pages_in_both_stores() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let create = draft(project.id, "bounds");
  let issue = postgres
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(create)).await.unwrap();
  for store in [&memory as &dyn IssueTimelineStore, &postgres as &dyn IssueTimelineStore] {
    let request = ReadIssueTimeline {
      project_id: project.id,
      issue_id: issue.id,
      scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      kinds: IssueTimelineKind::ALL.into_iter().collect(),
      boundary: None,
      after: None,
      limit: 1,
    };
    let first = store.issue_timeline(request.clone()).await.unwrap();
    for boundary in [
      octacity_server_store::IssueTimelineBoundary {
        issue_version: octacity_server_domain::IssueVersion::new(2).unwrap(),
        ..first.boundary
      },
      octacity_server_store::IssueTimelineBoundary {
        executions: 1,
        ..first.boundary
      },
      octacity_server_store::IssueTimelineBoundary {
        discussion: 1,
        ..first.boundary
      },
    ] {
      assert!(matches!(
        store
          .issue_timeline(ReadIssueTimeline {
            boundary: Some(boundary),
            ..request.clone()
          })
          .await,
        Err(StoreError::InvalidInput { .. })
      ));
    }
    let unavailable = store
      .issue_timeline(ReadIssueTimeline {
        boundary: Some(octacity_server_store::IssueTimelineBoundary {
          source: None,
          ..first.boundary
        }),
        ..request.clone()
      })
      .await
      .unwrap();
    assert!(unavailable.source_unavailable);
    assert!(
      !unavailable.source_stale,
      "unavailable original coverage cannot also claim a changed observation"
    );
    for limit in [0, 201] {
      assert!(matches!(
        store
          .issue_timeline(ReadIssueTimeline {
            limit,
            ..request.clone()
          })
          .await,
        Err(StoreError::InvalidInput { .. })
      ));
    }
  }
  database.cleanup().await;
}
