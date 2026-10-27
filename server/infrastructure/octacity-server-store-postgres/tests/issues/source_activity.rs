//! Task activity represents Source changes, independently of read freshness receipts.
use super::*;
use octacity_server_store::{
  IssueSourceRefreshStore, IssueTimelineKind, IssueTimelineStore, ReadIssueTimeline, RefreshIssueSource,
  SetIssueSourcePull,
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

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn unchanged_source_reads_keep_activity_stable_and_original_receipts_available() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let imported = import(project.id, "quiet-source");
  assert_unchanged_reads(&memory, imported.clone()).await;
  assert_unchanged_reads(&postgres, imported).await;
  database.cleanup().await;
}

async fn assert_unchanged_reads<S>(store: &S, imported: ImportIssueSource)
where
  S: IssueSourceStore + IssueSourceRefreshStore + IssueTimelineStore + IssueStore,
{
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("canonical imported task")
  };
  let read = ReadIssueTimeline {
    project_id: link.selection.project_id,
    issue_id: link.issue_id,
    scope: ManagementSecurityScope::trusted_network(),
    kinds: [IssueTimelineKind::Source].into_iter().collect(),
    boundary: None,
    after: None,
    limit: 1,
  };
  let initial = store.issue_timeline(read.clone()).await.unwrap();
  assert_eq!(initial.items.len(), 1);
  for now in [4, 5, 6] {
    let mut receipt = imported.observation.clone();
    receipt.observed_at = at(now);
    receipt.expires_at = at(now + 100);
    store
      .refresh_issue_source(management_mutation(RefreshIssueSource {
        explicit_read: None,
        status: None,
        read_claim: None,
        selection: imported.selection.clone(),
        issue_id: link.issue_id,
        observation: receipt.clone(),
        content: imported.content.clone(),
        idempotency_key: key(&format!("quiet-read-{now}")),
        refreshed_at: at(now),
      }))
      .await
      .unwrap();
    assert_eq!(
      store
        .issue_source_observation(
          read.project_id,
          read.issue_id,
          &read.scope,
          receipt.id(link.native_correlation).unwrap()
        )
        .await
        .unwrap(),
      Some(receipt),
      "freshness receipts remain exact and addressable"
    );
  }
  let current = store.issue_timeline(read.clone()).await.unwrap();
  assert_eq!(current.items, initial.items, "unchanged polling must not add activity");
  assert!(current.next.is_none(), "unchanged reads must not fill another page");
  assert_eq!(
    current.boundary, initial.boundary,
    "polling must not invalidate activity cursors"
  );
  let mut retained = read.clone();
  retained.boundary = Some(initial.boundary);
  assert!(!store.issue_timeline(retained).await.unwrap().source_stale);
  assert_eq!(
    store
      .issue_source_refresh_state(read.project_id, read.issue_id, &read.scope)
      .await
      .unwrap()
      .unwrap()
      .accepted
      .observed_at,
    at(6),
    "quiet activity still advances native freshness"
  );
  assert_eq!(
    store.issue(read.project_id, read.issue_id, None).await.unwrap().version,
    IssueVersion::INITIAL
  );
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_activity_preserves_changes_reversions_provenance_and_late_read_fencing() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let imported = import(project.id, "source-changes");
  assert_source_changes(&memory, imported.clone()).await;
  assert_source_changes(&postgres, imported).await;
  database.cleanup().await;
}

async fn assert_source_changes<S>(store: &S, imported: ImportIssueSource)
where
  S: IssueSourceStore + IssueSourceRefreshStore + IssueTimelineStore,
{
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("canonical imported task")
  };
  let original = imported.observation.snapshot_digest;
  let changed = FactoryDigest::content_sha256(b"changed native snapshot");
  let mut last = imported.observation.clone();
  // Equal consecutive reads fold, while A -> B -> A and changed provenance remain visible.
  for (now, snapshot, plugin) in [
    (4, original, "verified-reader"),
    (6, changed, "verified-reader"),
    (7, changed, "verified-reader"),
    (8, original, "verified-reader"),
    (9, original, "verified-reader"),
    (10, original, "updated-reader"),
    (11, original, "updated-reader"),
  ] {
    let mut receipt = imported.observation.clone();
    receipt.snapshot_digest = snapshot;
    receipt.plugin = exact(plugin);
    receipt.observed_at = at(now);
    receipt.expires_at = at(now + 100);
    store
      .refresh_issue_source(management_mutation(RefreshIssueSource {
        explicit_read: None,
        status: None,
        read_claim: None,
        selection: imported.selection.clone(),
        issue_id: link.issue_id,
        observation: receipt.clone(),
        content: imported.content.clone(),
        idempotency_key: key(&format!("changed-read-{now}")),
        refreshed_at: at(12),
      }))
      .await
      .unwrap();
    last = receipt;
  }
  let read = ReadIssueTimeline {
    project_id: imported.selection.project_id,
    issue_id: link.issue_id,
    scope: ManagementSecurityScope::trusted_network(),
    kinds: [IssueTimelineKind::Source].into_iter().collect(),
    boundary: None,
    after: None,
    limit: 1,
  };
  let initial = store.issue_timeline(read.clone()).await.unwrap();
  let mut page = initial.clone();
  let mut times = Vec::new();
  loop {
    assert_eq!(page.items.len(), 1);
    times.push(page.items[0].position().at.unix_millis());
    let Some(next) = page.next else { break };
    assert!(times.len() < 5, "pagination must finish after the four changes");
    page = store
      .issue_timeline(ReadIssueTimeline {
        after: Some(next),
        boundary: Some(initial.boundary),
        ..read.clone()
      })
      .await
      .unwrap();
    assert!(!page.source_stale);
  }
  assert_eq!(times, [2, 6, 8, 10]);

  // A delayed read replaces the first receipt of B without changing the number of groups.
  let mut late = imported.observation.clone();
  late.snapshot_digest = changed;
  late.observed_at = at(5);
  late.expires_at = at(105);
  store
    .refresh_issue_source(management_mutation(RefreshIssueSource {
      explicit_read: None,
      status: None,
      read_claim: None,
      selection: imported.selection.clone(),
      issue_id: link.issue_id,
      observation: late,
      content: imported.content,
      idempotency_key: key("late-read"),
      refreshed_at: at(13),
    }))
    .await
    .unwrap();
  let retained = store
    .issue_timeline(ReadIssueTimeline {
      boundary: Some(initial.boundary),
      ..read.clone()
    })
    .await
    .unwrap();
  assert!(
    retained.source_stale,
    "changed event membership must fence the original cursor"
  );
  let fresh = store
    .issue_timeline(ReadIssueTimeline {
      limit: 100,
      ..read.clone()
    })
    .await
    .unwrap();
  assert_eq!(
    fresh
      .items
      .iter()
      .map(|r| r.position().at.unix_millis())
      .collect::<Vec<_>>(),
    [2, 5, 8, 10]
  );
  assert_eq!(
    store
      .issue_source_refresh_state(read.project_id, read.issue_id, &read.scope)
      .await
      .unwrap()
      .unwrap()
      .accepted,
    last
  );
}
