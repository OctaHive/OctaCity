//! Source lifetime is fenced in the transaction admitting already published input bytes.
use super::*;

#[derive(Clone, Copy)]
enum Source {
  Comment,
  IssueDocument,
  ProjectDocument,
}
#[derive(Clone, Copy)]
enum Change {
  RetireComment,
  ExpireDocument,
  RetireDocument,
  Selection,
  UnselectedComment,
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn material_admission_rejects_retired_comment_after_input_bytes_are_published() {
  assert_admission(Source::Comment, Change::RetireComment).await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn material_admission_rechecks_document_expiry_at_commit_instead_of_reservation_time() {
  for source in [Source::IssueDocument, Source::ProjectDocument] {
    assert_admission(source, Change::ExpireDocument).await;
    assert_admission(source, Change::RetireDocument).await;
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn material_admission_rejects_a_selection_removed_after_input_bytes_are_published() {
  assert_admission(Source::Comment, Change::Selection).await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn material_admission_preserves_selected_text_when_unselected_discussion_is_appended() {
  assert_admission(Source::Comment, Change::UnselectedComment).await;
}

async fn source(
  store: Arc<PostgresStore>,
  issue: &IssueRevision,
  bytes: Arc<Bytes>,
  kind: Source,
) -> (IssueMaterialReference, Option<u64>, Option<Timestamp>) {
  if matches!(kind, Source::Comment) {
    let comment = store
      .append_issue_comment(management_mutation(AppendIssueComment {
        limits: IssueDiscussionLimits::default(),
        id: IssueCommentId::generate(),
        intent: AppendIssueCommentIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          parent_id: None,
          body: IssueCommentBody::try_new("SELECTED\r\nёж\n").unwrap(),
        },
        idempotency_key: key("guard-selected-comment"),
        created_at: at(3),
      }))
      .await
      .unwrap()
      .comment;
    return (
      IssueMaterialReference::Comment {
        project_id: issue.project_id,
        issue_id: issue.id,
        comment_id: comment.id,
        body_digest: comment.body_digest,
      },
      Some(comment.sequence),
      None,
    );
  }
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let documents = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(
      ProjectTextIntake::new(
        store.clone(),
        bytes,
        Arc::new(Resolver(AtomicUsize::new(0))),
        policy,
        Arc::new(Clock(AtomicI64::new(3))),
      )
      .with_issue_documents(store),
    ),
  );
  if matches!(kind, Source::IssueDocument) {
    let doc = documents
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: IssueDocumentSubmission {
            project_id: issue.project_id,
            issue_id: issue.id,
            name: "Owned".parse().unwrap(),
            content: "SELECTED\r\nёж\n".into(),
            idempotency_key: key("guard-owned"),
          },
        },
      )
      .await
      .unwrap();
    (
      IssueMaterialReference::IssueDocument {
        project_id: issue.project_id,
        issue_id: issue.id,
        artifact: doc.artifact,
      },
      None,
      Some(doc.expires_at),
    )
  } else {
    let doc = documents
      .handle_command(
        &context(),
        PublishProjectDocumentCommand {
          submission: ProjectDocumentSubmission {
            project_id: issue.project_id,
            name: "Shared".parse().unwrap(),
            content: "SELECTED\r\nёж\n".into(),
            purpose: ProjectDocumentPurpose::Context,
            idempotency_key: key("guard-shared"),
          },
        },
      )
      .await
      .unwrap();
    (
      IssueMaterialReference::ProjectDocument {
        project_id: issue.project_id,
        artifact: doc.artifact,
      },
      None,
      Some(doc.expires_at),
    )
  }
}

async fn assert_admission(kind: Source, change: Change) {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup_with_flow(&store, Some(material_flow())).await;
  let source_bytes = Arc::new(Bytes::new());
  let (reference, sequence, expires) = source(store.clone(), &issue, source_bytes.clone(), kind).await;
  let issue = store
    .edit_issue_materials(management_mutation(EditIssueMaterials {
      intent: EditIssueMaterialsIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        references: vec![reference],
      },
      idempotency_key: key("guard-approved-source"),
      edited_at: at(4),
    }))
    .await
    .unwrap()
    .issue;
  let reader = readers(store.clone(), source_bytes.clone(), Arc::new(Clock(AtomicI64::new(10))));
  let request = snapshot_request_with_materials(store.clone(), &issue, &submission, Some(reader.clone())).await;
  let publication_time = expires.map_or(20, |expiry| expiry.unix_millis() - 1);
  let bytes = Arc::new(PausedBytes::new());
  let publisher = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(
      IssueExecutionPublisher::new(
        store.clone(),
        store.clone(),
        bytes.clone(),
        Arc::new(Resolver(AtomicUsize::new(0))),
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(Clock(AtomicI64::new(publication_time))),
      )
      .with_materials(reader),
    ),
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
    .unwrap();
  publication.abort();
  assert!(publication.await.unwrap_err().is_cancelled());
  match change {
    Change::RetireComment => {
      store
        .set_issue_content_retention(management_mutation(SetIssueContentRetention {
          project_id: issue.project_id,
          issue_id: issue.id,
          expected_version: 0,
          plan: IssueContentRetentionPlan {
            scope: IssueContentRetentionScope::Discussion {
              through: sequence.unwrap(),
            },
            expires_at: at(22),
            hold: None,
          },
          idempotency_key: key("expire-guard-comment"),
          configured_at: at(21),
        }))
        .await
        .unwrap();
      assert_eq!(store.expire_issue_content(at(22), 10).await.unwrap(), 1);
    }
    Change::RetireDocument => {
      assert_eq!(
        FactoryExecutionInputRetentionWorker::new(store.clone(), source_bytes)
          .run_once(expires.unwrap(), 10)
          .await
          .unwrap(),
        1
      );
    }
    Change::ExpireDocument => {}
    Change::Selection => {
      store
        .edit_issue_materials(management_mutation(EditIssueMaterials {
          intent: EditIssueMaterialsIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            references: vec![],
          },
          idempotency_key: key("remove-guard-selection"),
          edited_at: at(21),
        }))
        .await
        .unwrap();
    }
    Change::UnselectedComment => {
      store
        .append_issue_comment(management_mutation(AppendIssueComment {
          limits: IssueDiscussionLimits::default(),
          id: IssueCommentId::generate(),
          intent: AppendIssueCommentIntent {
            project_id: issue.project_id,
            issue_id: issue.id,
            parent_id: None,
            body: IssueCommentBody::try_new("UNSELECTED_NEW_TEXT").unwrap(),
          },
          idempotency_key: key("new-unselected-comment"),
          created_at: at(21),
        }))
        .await
        .unwrap();
    }
  }
  let preparation = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let documents = store
    .project_text_documents(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let now = expires.unwrap_or(at(23));
  let preparation = store.publish_project_text(preparation.guard(now)).await.unwrap();
  let admission = prepared_admission(&store, &preparation, &documents).await;
  let result = store
    .admit_project_text(preparation.guard(now), management_mutation(admission))
    .await;
  let allowed = matches!(change, Change::UnselectedComment);
  if allowed {
    let run = store
      .factory_run_snapshot(result.unwrap().admission.run.id())
      .await
      .unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value()["context"],
      serde_json::json!(["SELECTED\r\nёж\n"])
    );
  } else {
    assert!(
      matches!(result, Err(StoreError::Conflict { .. })),
      "changed/retired source must prevent admission"
    );
    assert!(
      store
        .list_factory_runs(
          ListFactoryRuns::new(
            Some(issue.project_id),
            Default::default(),
            None,
            10,
            FactoryRunListVisibility::all(),
          )
          .unwrap()
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(
      store
        .issue_execution(issue.project_id, issue.id, request.id)
        .await
        .unwrap()
        .association
        .is_none()
    );
  }
  database.cleanup().await;
}
