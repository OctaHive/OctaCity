use super::*;
use octacity_server_store::{
  IssueSourceRefreshStore, IssueTimelineKind, IssueTimelineStore, ReadIssueTimeline, SetIssueSourcePull,
};
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn timeline_source_scope_and_policy_fencing_match_memory_after_restart() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let create = draft(project.id, "source-timeline");
  let issue = postgres
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(create)).await.unwrap();
  let attach = AttachIssueSource {
    observation: observation(&selection(project.id)),
    intent: AttachIssueSourceIntent {
      selection: selection(project.id),
      issue_id: issue.id,
      expected_version: issue.version,
      expected_field_policy: IssueFieldPolicy::default(),
    },
    idempotency_key: key("timeline-attach"),
    attached_at: at(3),
  };
  postgres
    .attach_issue_source(management_mutation(attach.clone()))
    .await
    .unwrap();
  memory.attach_issue_source(management_mutation(attach)).await.unwrap();
  let request = ReadIssueTimeline {
    project_id: project.id,
    issue_id: issue.id,
    scope: ManagementSecurityScope::trusted_network(),
    kinds: [IssueTimelineKind::Source].into_iter().collect(),
    boundary: None,
    after: None,
    limit: 1,
  };
  let first = postgres.issue_timeline(request.clone()).await.unwrap();
  let expected = memory.issue_timeline(request.clone()).await.unwrap();
  assert_eq!(first.boundary, expected.boundary);
  assert_eq!(first.items, expected.items);
  assert_eq!(first.items.len(), 1);
  let foreign = postgres
    .issue_timeline(ReadIssueTimeline {
      scope: ManagementSecurityScope::new("other-reader").unwrap(),
      ..request.clone()
    })
    .await
    .unwrap();
  assert!(foreign.items.is_empty());
  assert_ne!(foreign.boundary.source, first.boundary.source);
  let pause = SetIssueSourcePull {
    project_id: project.id,
    issue_id: issue.id,
    expected_generation: 1,
    enabled: false,
    idempotency_key: key("pause-timeline"),
    changed_at: at(5),
  };
  postgres
    .set_issue_source_pull(management_mutation(pause.clone()))
    .await
    .unwrap();
  memory.set_issue_source_pull(management_mutation(pause)).await.unwrap();
  let continuation = ReadIssueTimeline {
    boundary: Some(first.boundary),
    ..request
  };
  let restarted = PostgresStore::new(database.pool.clone());
  let actual = restarted.issue_timeline(continuation.clone()).await.unwrap();
  let expected = memory.issue_timeline(continuation).await.unwrap();
  assert!(actual.source_stale);
  assert!(!actual.source_unavailable);
  assert!(actual.items.is_empty());
  assert_eq!(actual.boundary, expected.boundary);
  assert_eq!(actual.source_stale, expected.source_stale);
  database.cleanup().await;
}
