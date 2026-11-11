//! Selected text and owner attribution survive the managed input's PostgreSQL recovery boundary.
use super::*;
#[path = "issue_snapshot_materials_fixture.rs"]
mod material_fixture;
use material_fixture::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn material_snapshot_recovers_frozen_sources_after_restart_and_content_retirement() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup_with_flow(&store, Some(material_flow())).await;
  let bytes = Arc::new(Bytes::new());
  let clock = Arc::new(Clock(AtomicI64::new(20)));
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let comments = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueDiscussionHandlers::new(
      store.clone(),
      policy.clone(),
      clock.clone(),
    )),
  );
  let comment = comments
    .handle_command(
      &context(),
      AppendIssueCommentCommand {
        intent: AppendIssueCommentIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          parent_id: None,
          body: IssueCommentBody::try_new("COMMENT\r\nёж\n").unwrap(),
        },
        idempotency_key: key("material-comment"),
      },
    )
    .await
    .unwrap()
    .comment;
  let docs = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(
      ProjectTextIntake::new(
        store.clone(),
        bytes.clone(),
        Arc::new(Resolver(AtomicUsize::new(0))),
        policy.clone(),
        clock.clone(),
      )
      .with_issue_documents(store.clone()),
    ),
  );
  let owned = docs
    .handle_command(
      &context(),
      PublishIssueDocumentCommand {
        submission: IssueDocumentSubmission {
          project_id: issue.project_id,
          issue_id: issue.id,
          name: "Issue source".parse().unwrap(),
          content: "ISSUE_DOC\r\nточные байты\n".into(),
          idempotency_key: key("material-owned"),
        },
      },
    )
    .await
    .unwrap();
  let shared = docs
    .handle_command(
      &context(),
      PublishProjectDocumentCommand {
        submission: ProjectDocumentSubmission {
          project_id: issue.project_id,
          name: "Project source".parse().unwrap(),
          content: "PROJECT_DOC\r\nreference\n".into(),
          purpose: ProjectDocumentPurpose::Context,
          idempotency_key: key("material-shared"),
        },
      },
    )
    .await
    .unwrap();
  assert!(
    store
      .issue(issue.project_id, issue.id, None)
      .await
      .unwrap()
      .materials
      .is_empty()
  );
  let runs = store
    .list_factory_runs(
      ListFactoryRuns::new(
        Some(issue.project_id),
        Default::default(),
        None,
        10,
        FactoryRunListVisibility::all(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(runs.items.is_empty(), "append/publication alone cannot admit execution");
  let materials = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueMaterialHandlers::new(
      store.clone(),
      store.clone(),
      policy.clone(),
      clock.clone(),
    )),
  );
  let issue = materials
    .handle_command(
      &context(),
      EditIssueMaterialsCommand {
        intent: EditIssueMaterialsIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          references: vec![
            IssueMaterialReference::Comment {
              project_id: issue.project_id,
              issue_id: issue.id,
              comment_id: comment.id,
              body_digest: comment.body_digest,
            },
            IssueMaterialReference::IssueDocument {
              project_id: issue.project_id,
              issue_id: issue.id,
              artifact: owned.artifact.clone(),
            },
            IssueMaterialReference::ProjectDocument {
              project_id: issue.project_id,
              artifact: shared.artifact.clone(),
            },
          ],
        },
        idempotency_key: key("approve-native-materials"),
      },
    )
    .await
    .unwrap()
    .issue;
  let readers = readers(store.clone(), bytes.clone(), clock.clone());
  let requests = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueExecutionHandlers::new(store.clone(), store.clone(), policy).with_materials(readers.clone())),
  );
  let request = requests
    .handle_command(
      &context(),
      RequestIssueExecutionCommand {
        intent: RequestIssueExecutionIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          expected_version: issue.version,
          target: IssueExecutionTarget {
            configuration_id: submission.intent.target.configuration_id,
            configuration_version: submission.intent.target.configuration_version,
            repository_id: submission.intent.target.repository_id.unwrap(),
            repository_version: submission.intent.target.repository_version.unwrap(),
            source: submission.intent.source.clone(),
            mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
            predecessor_run: None,
          },
          classification: Some(submission.intent.classification.clone()),
        },
        idempotency_key: key("native-material-review"),
        requested_at: at(30),
      },
    )
    .await
    .unwrap()
    .request;
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification,
  };
  bytes.lose_ack.store(true, Ordering::SeqCst);
  assert!(
    material_publisher(store.clone(), bytes.clone(), readers.clone(), 0, 40)
      .handle_command(&context(), command.clone())
      .await
      .is_err()
  );
  let documents = store
    .project_text_documents(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let snapshot = documents.issue_snapshot().unwrap();
  assert_eq!(snapshot.digest().unwrap(), request.snapshot_digest.unwrap());
  assert_eq!(snapshot.materials[&"context".parse().unwrap()].len(), 3);
  assert_eq!(
    snapshot.values[&"context".parse().unwrap()],
    serde_json::json!([
      "COMMENT\r\nёж\n",
      "ISSUE_DOC\r\nточные байты\n",
      "PROJECT_DOC\r\nreference\n"
    ])
  );
  assert_eq!(
    snapshot.materials[&"context".parse().unwrap()][0].selection,
    issue.materials.entries()[0]
  );
  let lineage = &snapshot.materials[&"context".parse().unwrap()];
  for (frozen, approved) in lineage.iter().zip(issue.materials.entries()) {
    assert_eq!(&frozen.selection, approved);
  }
  assert_eq!(
    lineage[0].source,
    IssueSnapshotMaterialSource::Comment {
      comment: comment.clone()
    }
  );
  assert_eq!(
    lineage[1].source,
    IssueSnapshotMaterialSource::IssueDocument { document: owned }
  );
  assert_eq!(
    lineage[2].source,
    IssueSnapshotMaterialSource::ProjectDocument { document: shared }
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let published = material_publisher(restarted.clone(), bytes.clone(), readers.clone(), 1, 60)
    .handle_command(&context(), command.clone())
    .await
    .unwrap();
  let association = published.association.as_ref().unwrap();
  let before = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  assert_eq!(
    before.flow.data.incoming[0].payload().value()["context"],
    snapshot.values[&"context".parse().unwrap()]
  );
  let preparation = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    bytes.writes.load(Ordering::SeqCst),
    4,
    "two sources and two Work inputs, without rewriting acknowledged bytes"
  );
  restarted
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_version: 0,
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Discussion {
          through: comment.sequence,
        },
        expires_at: at(100),
        hold: None,
      },
      idempotency_key: key("retire-source-comment"),
      configured_at: at(90),
    }))
    .await
    .unwrap();
  assert_eq!(restarted.expire_issue_content(at(100), 10).await.unwrap(), 1);
  assert!(
    restarted
      .issue_comment_body(issue.project_id, issue.id, comment.id)
      .await
      .is_err()
  );
  assert_eq!(
    FactoryExecutionInputRetentionWorker::new(restarted.clone(), bytes.clone())
      .run_once(preparation.expires_at, 10)
      .await
      .unwrap(),
    4
  );
  assert!(
    restarted
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key()
      )
      .await
      .unwrap()
      .is_none()
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let replay = material_publisher(
    restarted.clone(),
    bytes.clone(),
    readers,
    1,
    preparation.expires_at.unix_millis(),
  )
  .handle_command(&context(), command)
  .await
  .unwrap();
  assert_eq!(replay, published);
  assert_eq!(
    restarted.factory_run_snapshot(association.run_id).await.unwrap(),
    before
  );
  assert!(
    restarted
      .issue_comment_body(issue.project_id, issue.id, comment.id)
      .await
      .is_err()
  );
  assert!(
    restarted
      .project_text_documents(
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

#[path = "issue_snapshot_material_guards.rs"]
mod guards;
