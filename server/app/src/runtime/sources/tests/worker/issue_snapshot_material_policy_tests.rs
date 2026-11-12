//! Published material bounds and independent content permissions apply to automatic intake.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};

#[path = "issue_snapshot_material_encoding_tests.rs"]
mod encoding;
#[path = "issue_snapshot_material_revocation_tests.rs"]
mod revocation;

#[derive(Default)]
struct MaterialAccess {
  denied: AtomicBool,
  reads: AtomicUsize,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for MaterialAccess {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if (action == ManagementAction::View && resource.kind() == ManagementResourceKind::IssueDiscussion)
      || (action == ManagementAction::Download && resource.kind() == ManagementResourceKind::Artifact)
    {
      self.reads.fetch_add(1, Ordering::SeqCst);
      if self.denied.load(Ordering::SeqCst) {
        return Err(ManagementAuthorizationDenial::forbidden());
      }
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_material_policy_bounds_wait_without_omitting_oversized_input() {
  for required in [true, false] {
    for aggregate_bytes in [false, true] {
      let database = TestDatabase::migrated().await;
      let flow = snapshot_flow_with_inputs(
        false,
        Some(FlowIssueMaterialSelection {
          source: FlowIssueMaterialSource::Selected { max_entries: 3 },
          required,
          max_bytes: 4096,
        }),
      );
      let f = Fixture::enabled_with_flow("github", Some(flow))
        .await
        .into_postgres(database.pool.clone())
        .await;
      let issue = linked_issue(&f, "TASK-native", true).await;
      let mut approved = Vec::new();
      for index in 0..if aggregate_bytes { 2 } else { 4 } {
        // Two texts contain 5,000 UTF-8 bytes, although fewer than 4,096 characters.
        let text = if aggregate_bytes {
          "ёж ".repeat(500)
        } else {
          format!("Approved note {index}")
        };
        let entry = comment(&f, &issue, &text, &format!("bound-{index}")).await;
        approved.push(IssueMaterialReference::Comment {
          project_id: issue.project_id,
          issue_id: issue.id,
          comment_id: entry.id,
          body_digest: entry.body_digest,
        });
      }
      select(&f, &issue, approved, "approve-bounded-text").await;
      let result = worker(&f).run_once(CancellationToken::new()).await.unwrap();
      assert_eq!(
        (result.completed, result.deferred),
        (1, 0),
        "oversized material blocks only its candidate"
      );
      let retained = pending(&f).await;
      assert_eq!(
        retained.waiting.as_ref().map(|wait| wait.reason),
        Some(WorkSourceWaitReason::Incompatible)
      );
      assert!(retained.issue_input.is_none());
      assert!(f.runs().await.is_empty());
      assert!(f.bytes.0.lock().unwrap().is_empty());
      database.cleanup().await;
    }
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_without_material_policy_never_reads_approved_discussion_or_documents() {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let access = Arc::new(MaterialAccess::default());
  access.denied.store(true, Ordering::SeqCst);
  assert_eq!(
    worker_with_policy(&f, access.clone())
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let waiting = pending(&f).await;
  let issue = linked_issue(&f, "TASK-native", true).await;
  let approved = comment(&f, &issue, "SELECTED BUT NOT DECLARED by this consumer", "unconsumed").await;
  let (owned, shared) = publish_documents(&f, &issue, 1).await;
  select(
    &f,
    &issue,
    references(&issue, &approved, &owned, &shared),
    "unconsumed-references",
  )
  .await;
  f.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    worker_with_policy(&f, access.clone())
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let run = f.store.factory_run_snapshot(f.runs().await[0].id).await.unwrap();
  assert_eq!(
    run.flow.data.incoming[0].payload().value(),
    &serde_json::json!({"title":"Canonical TASK-native", "size":"small", "priority":"high"})
  );
  let retained = f.store.source_preparation(issue.project_id, waiting.id).await.unwrap();
  let snapshot = retained.issue_input.as_ref().unwrap().issue_snapshot().unwrap();
  assert!(snapshot.materials.is_empty());
  assert!(snapshot.omissions.is_empty());
  assert_eq!(
    access.reads.load(Ordering::SeqCst),
    0,
    "without a published material policy even a body authorization lookup is unnecessary"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_material_policy_respects_independent_body_denial_and_optional_omissions() {
  for required in [true, false] {
    let database = TestDatabase::migrated().await;
    let flow = snapshot_flow_with_inputs(
      false,
      Some(FlowIssueMaterialSelection {
        source: FlowIssueMaterialSource::Selected { max_entries: 3 },
        required,
        max_bytes: 4096,
      }),
    );
    let f = Fixture::enabled_with_flow("github", Some(flow))
      .await
      .into_postgres(database.pool.clone())
      .await;
    let access = Arc::new(MaterialAccess::default());
    access.denied.store(true, Ordering::SeqCst);
    assert_eq!(
      worker_with_policy(&f, access.clone())
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .completed,
      1
    );
    let waiting = pending(&f).await;
    let issue = linked_issue(&f, "TASK-native", true).await;
    let approved = comment(
      &f,
      &issue,
      "PRIVATE COMMENT must remain inaccessible",
      "private-comment",
    )
    .await;
    select(
      &f,
      &issue,
      vec![IssueMaterialReference::Comment {
        project_id: issue.project_id,
        issue_id: issue.id,
        comment_id: approved.id,
        body_digest: approved.body_digest,
      }],
      "approve-private-comment",
    )
    .await;
    f.clock.0.store(110, Ordering::SeqCst);
    let result = worker_with_policy(&f, access.clone())
      .run_once(CancellationToken::new())
      .await
      .unwrap();
    assert_eq!((result.completed, result.deferred), (1, 0));
    let retained = f.store.source_preparation(issue.project_id, waiting.id).await.unwrap();
    if required {
      assert_eq!(
        retained.waiting.as_ref().unwrap().reason,
        WorkSourceWaitReason::Inaccessible
      );
      assert!(retained.admission.is_none());
      assert!(f.runs().await.is_empty());
      assert!(f.bytes.0.lock().unwrap().is_empty());
    } else {
      let snapshot = retained.issue_input.as_ref().unwrap().issue_snapshot().unwrap();
      assert_eq!(
        snapshot.omissions[&key("materials")],
        IssueSnapshotOmission::Unavailable
      );
      assert!(!snapshot.values.contains_key(&key("materials")));
      assert!(snapshot.materials.is_empty());
      let run = f.store.factory_run_snapshot(f.runs().await[0].id).await.unwrap();
      assert!(run.flow.data.incoming[0].payload().value().get("materials").is_none());
    }
    assert!(
      access.reads.load(Ordering::SeqCst) > 0,
      "the installed reader must independently consult content authority"
    );
    database.cleanup().await;
  }
}
