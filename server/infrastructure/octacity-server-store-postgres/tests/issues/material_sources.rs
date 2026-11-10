use super::*;
use octacity_server_factory::FactoryDigest;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_materials_validate_source_pins_and_replay_without_restoring_expired_bodies() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let mut create = draft(project.id, "issue");
  create.intent.content.description = "Private requirements body".into();
  let original = store.create_issue(management_mutation(create)).await.unwrap().issue;
  let other = store
    .create_issue(management_mutation(draft(project.id, "other")))
    .await
    .unwrap()
    .issue;
  let owned = document(&store, project.id, Some(original.id), "owned").await;
  let shared = document(&store, project.id, None, "shared").await;
  let other_owned = document(&store, project.id, Some(other.id), "other-owned").await;
  let note = comment(&store, &original, "note").await;
  let other_note = comment(&store, &other, "other-note").await;
  let comment_ref = IssueMaterialReference::Comment {
    project_id: project.id,
    issue_id: original.id,
    comment_id: note.id,
    body_digest: note.body_digest,
  };
  let document_ref = IssueMaterialReference::IssueDocument {
    project_id: project.id,
    issue_id: original.id,
    artifact: owned.clone(),
  };
  let shared_ref = IssueMaterialReference::ProjectDocument {
    project_id: project.id,
    artifact: shared.clone(),
  };
  let forged = FactoryArtifactReference::new(
    owned.artifact_id(),
    FactoryDigest::from_bytes([9; 32]),
    owned.encoded_size(),
  )
  .unwrap();
  let wrong_size =
    FactoryArtifactReference::new(shared.artifact_id(), shared.content_digest(), shared.encoded_size() + 1).unwrap();
  let reserved = store
    .reserve_project_document(management_mutation(ReserveProjectDocument {
      issue_id: Some(original.id),
      selection_digest: None,
      now: at(3),
      submission: ProjectDocumentSubmission {
        project_id: project.id,
        name: "Unpublished".parse().unwrap(),
        content: "Pending body".into(),
        purpose: Default::default(),
        idempotency_key: key("unpublished"),
      },
    }))
    .await
    .unwrap();
  let invalid = vec![
    IssueMaterialReference::Comment {
      project_id: project.id,
      issue_id: original.id,
      comment_id: note.id,
      body_digest: FactoryDigest::from_bytes([9; 32]),
    },
    IssueMaterialReference::Comment {
      project_id: project.id,
      issue_id: original.id,
      comment_id: other_note.id,
      body_digest: other_note.body_digest,
    },
    IssueMaterialReference::IssueDocument {
      project_id: project.id,
      issue_id: original.id,
      artifact: other_owned,
    },
    IssueMaterialReference::IssueDocument {
      project_id: project.id,
      issue_id: original.id,
      artifact: forged,
    },
    IssueMaterialReference::IssueDocument {
      project_id: project.id,
      issue_id: original.id,
      artifact: reserved.objects[0].artifact.clone(),
    },
    IssueMaterialReference::IssueDocument {
      project_id: project.id,
      issue_id: original.id,
      artifact: shared.clone(),
    },
    IssueMaterialReference::ProjectDocument {
      project_id: project.id,
      artifact: owned.clone(),
    },
    IssueMaterialReference::ProjectDocument {
      project_id: project.id,
      artifact: wrong_size,
    },
  ];
  let scope = ManagementSecurityScope::new("operator:alice").unwrap();
  for (index, invalid_ref) in invalid.into_iter().enumerate() {
    let command = edit(&original, vec![invalid_ref], &format!("invalid-{index}"));
    assert!(
      store
        .edit_issue_materials(authored(command.clone(), "alice"))
        .await
        .is_err(),
      "invalid source {index}"
    );
    assert_eq!(store.issue(project.id, original.id, None).await.unwrap(), original);
    assert!(
      store
        .issue_material_edit(&scope, &command.idempotency_key)
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      outbox(&database, original.id, "issue.materials-edited")
        .await
        .is_empty()
    );
  }
  let select = edit(
    &original,
    vec![comment_ref.clone(), document_ref.clone(), shared_ref.clone()],
    "select",
  );
  let selected = store
    .edit_issue_materials(authored(select.clone(), "alice"))
    .await
    .unwrap()
    .issue;
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      project_id: project.id,
      issue_id: original.id,
      expected_version: 0,
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Discussion { through: 1 },
        expires_at: at(20),
        hold: None,
      },
      idempotency_key: key("expire-discussion"),
      configured_at: at(19),
    }))
    .await
    .unwrap();
  assert_eq!(store.expire_issue_content(at(20), 10).await.unwrap(), 1);
  assert!(
    store
      .issue_comment_body(project.id, original.id, note.id)
      .await
      .is_err()
  );
  assert_eq!(
    store
      .issue_comments(project.id, original.id, None, 10)
      .await
      .unwrap()
      .items,
    vec![note.clone()]
  );
  let mut remove_comment = edit(
    &selected,
    vec![document_ref.clone(), shared_ref.clone()],
    "remove-comment",
  );
  remove_comment.edited_at = at(21);
  let removed_comment = store
    .edit_issue_materials(authored(remove_comment, "alice"))
    .await
    .unwrap()
    .issue;
  let mut expired_comment = edit(&removed_comment, vec![comment_ref], "expired-comment");
  expired_comment.edited_at = at(22);
  assert!(
    store
      .edit_issue_materials(authored(expired_comment, "alice"))
      .await
      .is_err()
  );
  store
    .set_issue_content_retention(management_mutation(SetIssueContentRetention {
      project_id: project.id,
      issue_id: original.id,
      expected_version: 0,
      plan: IssueContentRetentionPlan {
        scope: IssueContentRetentionScope::Requirements {
          through: removed_comment.version,
        },
        expires_at: at(30),
        hold: None,
      },
      idempotency_key: key("expire-requirements"),
      configured_at: at(29),
    }))
    .await
    .unwrap();
  assert_eq!(store.expire_issue_content(at(30), 10).await.unwrap(), 3);
  let deadline = store
    .issue_document_metadata(project.id, original.id, owned.artifact_id())
    .await
    .unwrap()
    .unwrap()
    .expires_at;
  // Check the deadline while both publications still exist; deletion below
  // separately qualifies replay/removal after the original bytes are gone.
  for (index, reference) in [document_ref, shared_ref].into_iter().enumerate() {
    let mut expired = edit(&removed_comment, vec![reference], &format!("expired-document-{index}"));
    expired.edited_at = deadline;
    assert!(store.edit_issue_materials(authored(expired, "alice")).await.is_err());
  }
  for source in store.expired_factory_execution_inputs(deadline, 10).await.unwrap() {
    store
      .complete_factory_execution_input_deletion(CompleteFactoryExecutionInputDeletion {
        expected: source,
        deleted_at: deadline,
      })
      .await
      .unwrap();
  }
  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let replay = restarted
    .edit_issue_materials(authored(select.clone(), "alice"))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue.version, selected.version);
  assert_eq!(replay.issue.materials, selected.materials);
  assert!(!replay.issue.content_available);
  assert!(
    !serde_json::to_string(&replay)
      .unwrap()
      .contains("Private requirements body")
  );
  assert!(
    !serde_json::to_string(&replay)
      .unwrap()
      .contains("Private note discussion")
  );
  let receipt = restarted
    .issue_material_edit(&scope, &select.idempotency_key)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(receipt.intent, select.intent);
  assert!(!receipt.outcome.issue.content_available);
  assert!(
    !serde_json::to_string(&receipt)
      .unwrap()
      .contains("Private requirements body")
  );
  assert!(
    restarted
      .issue_comment_body(project.id, original.id, note.id)
      .await
      .is_err()
  );
  assert!(
    restarted
      .issue_document_input(project.id, original.id, owned.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .project_input_artifact(project.id, shared.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  let current = restarted.issue(project.id, original.id, None).await.unwrap();
  let mut remove_all = edit(&current, vec![], "remove-expired-sources");
  remove_all.edited_at = deadline;
  let removed = restarted
    .edit_issue_materials(authored(remove_all, "alice"))
    .await
    .unwrap()
    .issue;
  assert!(removed.materials.is_empty());
  assert!(!removed.content_available);
  assert_eq!(
    restarted
      .issue(project.id, original.id, Some(selected.version))
      .await
      .unwrap()
      .materials,
    selected.materials
  );
  assert!(
    restarted
      .project_document_metadata(project.id, owned.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .issue_document_metadata(project.id, original.id, owned.artifact_id())
      .await
      .unwrap()
      .is_some()
  );
  assert!(
    restarted
      .project_document_metadata(project.id, shared.artifact_id())
      .await
      .unwrap()
      .is_some()
  );
  pool.close().await;
  database.cleanup().await;
}
