//! Preparation races are fenced by the final admission transaction.
use super::*;
#[path = "issue_snapshot_fixture.rs"]
mod fixture;
use fixture::*;
#[path = "issue_snapshot_policy.rs"]
mod admission_policy;
#[path = "issue_snapshot_integrity.rs"]
mod integrity;
#[path = "issue_snapshot_replay.rs"]
mod replay;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admission_rejects_requirements_changed_after_byte_publication() {
  assert_pending_change(ConcurrentChange::Requirements).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admission_rejects_definitions_changed_after_byte_publication() {
  assert_pending_change(ConcurrentChange::Definitions).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admission_rejects_classification_changed_after_byte_publication() {
  assert_pending_change(ConcurrentChange::Classification).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admission_keeps_reviewed_inputs_when_unselected_discussion_is_appended() {
  assert_pending_change(ConcurrentChange::Discussion).await;
}

enum ConcurrentChange {
  Requirements,
  Definitions,
  Classification,
  Discussion,
}

async fn assert_pending_change(change: ConcurrentChange) {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup_with_flow(&store, Some(snapshot_flow())).await;
  let request = snapshot_request(store.clone(), &issue, &submission).await;
  let bytes = Arc::new(PausedBytes::new());
  let publisher = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueExecutionPublisher::new(
      store.clone(),
      store.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(0))),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock(AtomicI64::new(20))),
    )),
  );
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification,
  };
  let publication = tokio::spawn(async move { publisher.handle_command(&context(), command).await });
  tokio::time::timeout(std::time::Duration::from_secs(10), bytes.reached.notified())
    .await
    .expect("publication reaches the byte boundary");
  let allowed = matches!(change, ConcurrentChange::Discussion);
  match change {
    ConcurrentChange::Requirements => {
      store
        .edit_issue(management_mutation(EditIssue {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            content: IssueContent {
              acceptance: Some("Changed acceptance".into()),
              ..issue.content.clone()
            },
          },
          idempotency_key: key("concurrent-requirements"),
          edited_at: at(21),
        }))
        .await
        .unwrap();
    }
    ConcurrentChange::Classification => {
      store
        .edit_issue(management_mutation(EditIssue {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            content: IssueContent {
              priority: Some(IssuePriority::Choice("high".into())),
              ..issue.content.clone()
            },
          },
          idempotency_key: key("concurrent-classification"),
          edited_at: at(21),
        }))
        .await
        .unwrap();
    }
    ConcurrentChange::Discussion => {
      store
        .append_issue_comment(management_mutation(AppendIssueComment {
          limits: IssueDiscussionLimits::default(),
          id: IssueCommentId::generate(),
          intent: AppendIssueCommentIntent {
            project_id: issue.project_id,
            issue_id: issue.id,
            parent_id: None,
            body: IssueCommentBody::try_new("Unselected supplementary discussion").unwrap(),
          },
          idempotency_key: key("concurrent-comment"),
          created_at: at(21),
        }))
        .await
        .unwrap();
    }
    ConcurrentChange::Definitions => {
      let mut definition = store.issue_properties(issue.project_id, None).await.unwrap().definition;
      definition.priorities[0].retired = true;
      store
        .publish_issue_properties(management_mutation(PublishIssueProperties {
          intent: PublishIssuePropertiesIntent {
            project_id: issue.project_id,
            expected_version: 0,
            definition,
          },
          idempotency_key: key("concurrent-definitions"),
          published_at: at(21),
        }))
        .await
        .unwrap();
    }
  }
  bytes.resume.notify_one();
  let result = publication.await.unwrap();
  if allowed {
    let published = result.expect("unselected discussion does not change the reviewed Issue revision");
    let association = published.association.unwrap();
    let run = store.factory_run_snapshot(association.run_id).await.unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({"title":"Задача"})
    );
    assert_eq!(
      run.flow.data.incoming[0].issue_snapshot().unwrap().digest,
      request.snapshot_digest.unwrap()
    );
    database.cleanup().await;
    return;
  }
  assert!(
    result.is_err(),
    "a concurrent reviewed input change must prevent admission: {result:?}"
  );
  assert_eq!(
    result.unwrap_err().application().unwrap().classification(),
    ApplicationFailure::Conflict
  );
  let preparation = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(preparation.admission.is_none());
  assert!(
    store
      .issue_execution(issue.project_id, issue.id, request.id)
      .await
      .unwrap()
      .association
      .is_none()
  );
  let runs = store
    .list_factory_runs(
      ListFactoryRuns::new(
        Some(issue.project_id),
        FactoryRunFilter::default(),
        None,
        10,
        FactoryRunListVisibility::all(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(
    runs.items.is_empty(),
    "no implementation can dispatch without an admitted Run"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_review_requires_fresh_review_if_definitions_change_before_preparation() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup_with_flow(&store, Some(snapshot_flow())).await;
  let request = snapshot_request(store.clone(), &issue, &submission).await;
  let mut definition = store.issue_properties(issue.project_id, None).await.unwrap().definition;
  definition.priorities[0].retired = true;
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition,
      },
      idempotency_key: key("definitions-before-preparation"),
      published_at: at(21),
    }))
    .await
    .unwrap();
  let bytes = Arc::new(Bytes::new());
  let handler = publisher(
    store.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(0))),
    Arc::new(Clock(AtomicI64::new(22))),
  );
  let error = handler
    .handle_command(
      &context(),
      PublishIssueExecutionCommand {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: request.id,
        classification: submission.intent.classification,
      },
    )
    .await
    .expect_err("a changed reviewed catalog requires a new review, not silent substitution");
  assert_eq!(
    error.application().unwrap().classification(),
    ApplicationFailure::Conflict
  );
  assert_eq!(bytes.writes.load(Ordering::SeqCst), 0);
  assert!(
    store
      .project_text_preparation(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key()
      )
      .await
      .unwrap()
      .is_none()
  );
  database.cleanup().await;
}
