//! Managed preparation reuses its frozen bytes and admitted receipts retain their original material.
use super::*;
use std::sync::atomic::Ordering;

#[test]
fn issue_material_snapshot_recovers_original_text_after_lost_ack_and_later_source_publications() {
  run_ready(async {
    let f = MaterialFixture::new(true).await;
    let readers = f.readers(Arc::new(TrustedNetworkManagementPolicy));
    let request = f.review(readers.clone()).await.unwrap().request;
    f.bytes.lose_ack.store(true, Ordering::SeqCst);
    assert!(f.publish(&request, readers.clone()).await.is_err());
    let original = f
      .factory
      .project_text_documents(
        f.issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let before_reads = f.bytes.reads.lock().unwrap().clone();
    let published = f.publish(&request, readers.clone()).await.unwrap();
    let reads = f.bytes.reads.lock().unwrap().clone();
    for artifact in [f.owned.artifact.artifact_id(), f.shared.artifact.artifact_id()] {
      assert_eq!(
        reads.iter().filter(|id| **id == artifact).count(),
        before_reads.iter().filter(|id| **id == artifact).count() + 1,
        "recovery rechecks the exact source once before admission and keeps the prepared snapshot"
      );
    }
    let association = published.association.as_ref().unwrap();
    let run = f.factory.factory_run_snapshot(association.run_id).await.unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy.clone()));
    let owned = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: IssueDocumentSubmission {
            project_id: f.issue.project_id,
            issue_id: f.issue.id,
            name: f.owned.name.clone(),
            content: "NEW_ISSUE_PUBLICATION".into(),
            idempotency_key: key("later-owned-publication"),
          },
        },
      )
      .await
      .unwrap();
    let shared = commands
      .handle_command(
        &context(),
        PublishProjectDocumentCommand {
          submission: ProjectDocumentSubmission {
            project_id: f.issue.project_id,
            name: f.shared.name.clone(),
            content: "NEW_PROJECT_PUBLICATION".into(),
            purpose: ProjectDocumentPurpose::Context,
            idempotency_key: key("later-shared-publication"),
          },
        },
      )
      .await
      .unwrap();
    let comments = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(
        f.issues.clone(),
        policy.clone(),
        Arc::new(MaterialClock),
      )),
    );
    let comment = comments
      .handle_command(
        &context(),
        AppendIssueCommentCommand {
          intent: AppendIssueCommentIntent {
            project_id: f.issue.project_id,
            issue_id: f.issue.id,
            parent_id: None,
            body: IssueCommentBody::try_new("NEW_COMMENT_BODY").unwrap(),
          },
          idempotency_key: key("later-comment"),
        },
      )
      .await
      .unwrap()
      .comment;
    let selections = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        f.issues.clone(),
        f.factory.clone(),
        policy,
        Arc::new(MaterialClock),
      )),
    );
    selections
      .handle_command(
        &context(),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            project_id: f.issue.project_id,
            id: f.issue.id,
            expected_version: f.issue.version,
            references: vec![
              IssueMaterialReference::Comment {
                project_id: f.issue.project_id,
                issue_id: f.issue.id,
                comment_id: comment.id,
                body_digest: comment.body_digest,
              },
              IssueMaterialReference::IssueDocument {
                project_id: f.issue.project_id,
                issue_id: f.issue.id,
                artifact: owned.artifact,
              },
              IssueMaterialReference::ProjectDocument {
                project_id: f.issue.project_id,
                artifact: shared.artifact,
              },
            ],
          },
          idempotency_key: key("replace-later-materials"),
        },
      )
      .await
      .unwrap();
    let before = f.bytes.reads.lock().unwrap().len();
    assert_eq!(f.review(readers.clone()).await.unwrap().request, request);
    assert_eq!(f.publish(&request, readers).await.unwrap(), published);
    assert_eq!(f.bytes.reads.lock().unwrap().len(), before);
    assert_eq!(f.factory.factory_run_snapshot(association.run_id).await.unwrap(), run);
    assert_eq!(
      f.factory
        .project_text_documents(
          f.issue.project_id,
          &context().security_scope().to_store(),
          &request.admission_key()
        )
        .await
        .unwrap()
        .unwrap(),
      original
    );
  });
}
