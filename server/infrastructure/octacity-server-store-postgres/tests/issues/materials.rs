//! Material persistence contracts use public Issue and original-owner source ports.
use super::*;
use octacity_server_domain::IssueCommentId;
use octacity_server_factory::FactoryArtifactReference;
use octacity_server_store::*;
#[path = "material_sources.rs"]
mod sources;
#[path = "material_transactions.rs"]
mod transactions;

fn authored<T>(request: T, actor: &str) -> ManagementMutation<T> {
  ManagementMutation::new(
    request,
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some(actor.into()),
      },
      ManagementSecurityScope::new(format!("operator:{actor}")).unwrap(),
      "material-contract",
    )
    .unwrap(),
  )
}
async fn document(
  store: &PostgresStore,
  project_id: ProjectId,
  issue_id: Option<IssueId>,
  replay: &str,
) -> FactoryArtifactReference {
  let reserved = store
    .reserve_project_document(management_mutation(ReserveProjectDocument {
      issue_id,
      selection_digest: None,
      now: at(3),
      submission: ProjectDocumentSubmission {
        project_id,
        name: "Investigation".parse().unwrap(),
        content: format!("Private {replay} text"),
        purpose: ProjectDocumentPurpose::Context,
        idempotency_key: key(replay),
      },
    }))
    .await
    .unwrap();
  store
    .publish_project_document(management_mutation(reserved.guard(at(4))))
    .await
    .unwrap()
    .objects[0]
    .artifact
    .clone()
}
async fn comment(store: &PostgresStore, issue: &IssueRevision, replay: &str) -> IssueCommentMetadata {
  store
    .append_issue_comment(authored(
      AppendIssueComment {
        id: IssueCommentId::generate(),
        limits: IssueDiscussionLimits::default(),
        intent: AppendIssueCommentIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          parent_id: None,
          body: IssueCommentBody::try_new(format!("Private {replay} discussion")).unwrap(),
        },
        idempotency_key: key(replay),
        created_at: at(3),
      },
      "author",
    ))
    .await
    .unwrap()
    .comment
}
fn edit(issue: &IssueRevision, references: Vec<IssueMaterialReference>, replay: &str) -> EditIssueMaterials {
  EditIssueMaterials {
    intent: EditIssueMaterialsIntent {
      project_id: issue.project_id,
      id: issue.id,
      expected_version: issue.version,
      references,
    },
    idempotency_key: key(replay),
    edited_at: at(10),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_materials_upgrade_restore_round_trip_retains_exact_history_and_original_receipts() {
  let mut database = TestDatabase::empty().await;
  // Last schema before material mutations; pre-existing Issue JSON has no materials field.
  octacity_server_store_postgres::MIGRATOR
    .run_to(120, &database.pool)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let create = draft(project.id, "old-empty-issue");
  let original = store
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  let owned = document(&store, project.id, Some(original.id), "owned").await;
  let shared = document(&store, project.id, None, "shared").await;
  let note = comment(&store, &original, "note").await;
  let command = edit(
    &original,
    vec![
      IssueMaterialReference::Comment {
        project_id: project.id,
        issue_id: original.id,
        comment_id: note.id,
        body_digest: note.body_digest,
      },
      IssueMaterialReference::IssueDocument {
        project_id: project.id,
        issue_id: original.id,
        artifact: owned.clone(),
      },
      IssueMaterialReference::ProjectDocument {
        project_id: project.id,
        artifact: shared.clone(),
      },
    ],
    "select",
  );
  let snapshot = super::snapshot::DatabaseSnapshot::capture(&mut database).await;
  let restored = snapshot.restore().await;
  for copy in [&database, &restored] {
    octacity_server_store_postgres::migrate(&copy.pool).await.unwrap();
    let store = PostgresStore::new(copy.pool.clone());
    assert_eq!(store.issue(project.id, original.id, None).await.unwrap(), original);
    assert!(original.materials.is_empty());
    let selected = store
      .edit_issue_materials(authored(command.clone(), "alice"))
      .await
      .unwrap()
      .issue;
    assert_eq!(selected.version.get(), 2);
    assert_eq!(selected.applicability_version, original.applicability_version);
    assert_eq!(selected.materials.entries().len(), 3);
    for (selection, reference) in selected.materials.entries().iter().zip(&command.intent.references) {
      assert_eq!(&selection.reference, reference);
      assert_eq!(selection.selected_by.identity.as_deref(), Some("alice"));
      assert_eq!(selection.selected_at, at(10));
    }
    let pool = sqlx::PgPool::connect_with((*copy.pool.connect_options()).clone())
      .await
      .unwrap();
    let restarted = PostgresStore::new(pool.clone());
    let mut content = selected.content.clone();
    content.description = "Later requirements".into();
    let changed = restarted
      .edit_issue(management_mutation(EditIssue {
        intent: EditIssueIntent {
          project_id: project.id,
          id: original.id,
          expected_version: selected.version,
          content,
        },
        idempotency_key: key("later-edit"),
        edited_at: at(11),
      }))
      .await
      .unwrap()
      .issue;
    assert_eq!(changed.materials, selected.materials);
    let mut remove = edit(&changed, vec![], "remove");
    remove.edited_at = at(12);
    let removed = restarted
      .edit_issue_materials(authored(remove, "alice"))
      .await
      .unwrap()
      .issue;
    assert!(removed.materials.is_empty());
    let mut retry = command.clone();
    retry.edited_at = at(99);
    let replay = restarted.edit_issue_materials(authored(retry, "alice")).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, selected);
    let receipt = restarted
      .issue_material_edit(&ManagementSecurityScope::new("operator:alice").unwrap(), &key("select"))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(receipt.intent, command.intent);
    assert_eq!(receipt.outcome.issue, selected);
    assert_eq!(
      restarted
        .create_issue(management_mutation(create.clone()))
        .await
        .unwrap()
        .issue,
      original
    );
    for revision in [&original, &selected, &changed, &removed] {
      assert_eq!(
        restarted
          .issue(project.id, original.id, Some(revision.version))
          .await
          .unwrap(),
        *revision
      );
    }
    assert_eq!(restarted.issue(project.id, original.id, None).await.unwrap(), removed);
    assert_eq!(
      restarted
        .issue_comment_body(project.id, original.id, note.id)
        .await
        .unwrap()
        .body
        .as_str(),
      "Private note discussion"
    );
    assert_eq!(
      restarted
        .issue_document_metadata(project.id, original.id, owned.artifact_id())
        .await
        .unwrap()
        .unwrap()
        .artifact,
      owned
    );
    assert!(
      restarted
        .project_document_metadata(project.id, owned.artifact_id())
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
      restarted
        .project_document_metadata(project.id, shared.artifact_id())
        .await
        .unwrap()
        .unwrap()
        .artifact,
      shared
    );
    for name in ["owned", "shared"] {
      assert_eq!(
        restarted
          .project_document_content(project.id, &ManagementSecurityScope::trusted_network(), &key(name))
          .await
          .unwrap()
          .unwrap(),
        format!("Private {name} text")
      );
    }
    assert_eq!(outbox(copy, original.id, "issue.materials-edited").await.len(), 2);
    pool.close().await;
  }
  restored.cleanup().await;
  database.cleanup().await;
  snapshot.cleanup().await;
}
