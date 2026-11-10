//! Qualifies existing transaction/replay fences for the material mutation kind.
use super::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_materials_concurrent_reviews_and_scoped_keys_have_exact_winners() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let original = store
    .create_issue(management_mutation(draft(project.id, "issue")))
    .await
    .unwrap()
    .issue;
  let shared = document(&store, project.id, None, "shared").await;
  let note = comment(&store, &original, "note").await;
  let document_ref = IssueMaterialReference::ProjectDocument {
    project_id: project.id,
    artifact: shared,
  };
  let comment_ref = IssueMaterialReference::Comment {
    project_id: project.id,
    issue_id: original.id,
    comment_id: note.id,
    body_digest: note.body_digest,
  };
  let a = edit(&original, vec![document_ref.clone()], "review-a");
  let b = edit(&original, vec![comment_ref.clone()], "review-b");
  let (first, second) = tokio::join!(
    store.edit_issue_materials(authored(a.clone(), "alice")),
    store.edit_issue_materials(authored(b.clone(), "alice"))
  );
  let (winner, mut loser) = match (first, second) {
    (
      Ok(winner),
      Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      }),
    ) => (winner, b),
    (
      Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      }),
      Ok(winner),
    ) => (winner, a),
    other => panic!("expected one reviewed winner: {other:?}"),
  };
  assert_eq!(winner.issue.version.get(), 2);
  assert_eq!(outbox(&database, original.id, "issue.materials-edited").await.len(), 1);
  let alice = ManagementSecurityScope::new("operator:alice").unwrap();
  assert!(
    store
      .issue_material_edit(&alice, &loser.idempotency_key)
      .await
      .unwrap()
      .is_none()
  );
  loser.intent.expected_version = winner.issue.version;
  loser.edited_at = at(11);
  let current = store
    .edit_issue_materials(authored(loser, "alice"))
    .await
    .unwrap()
    .issue;
  assert_eq!(current.version.get(), 3);
  let mut identical = edit(&current, vec![document_ref.clone(), comment_ref.clone()], "same-intent");
  identical.edited_at = at(12);
  let (first, second) = tokio::join!(
    store.edit_issue_materials(authored(identical.clone(), "alice")),
    store.edit_issue_materials(authored(identical.clone(), "alice"))
  );
  let results = [first.unwrap(), second.unwrap()];
  assert_eq!(results[0].issue, results[1].issue);
  assert_eq!(results[0].issue.version.get(), 4);
  assert_eq!(
    results
      .iter()
      .filter(|o| o.disposition == MutationDisposition::Applied)
      .count(),
    1
  );
  assert_eq!(
    results
      .iter()
      .filter(|o| o.disposition == MutationDisposition::Replayed)
      .count(),
    1
  );
  let mut conflicting_a = edit(&results[0].issue, vec![], "contested");
  conflicting_a.edited_at = at(13);
  let mut conflicting_b = conflicting_a.clone();
  conflicting_b.intent.references = vec![comment_ref];
  let (first, second) = tokio::join!(
    store.edit_issue_materials(authored(conflicting_a, "alice")),
    store.edit_issue_materials(authored(conflicting_b, "alice"))
  );
  let winner = match (first, second) {
    (
      Ok(winner),
      Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      }),
    )
    | (
      Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      }),
      Ok(winner),
    ) => winner,
    other => panic!("expected one scoped key winner: {other:?}"),
  };
  assert_eq!(winner.issue.version.get(), 5);
  let mut bob = edit(&winner.issue, vec![document_ref], "contested");
  bob.edited_at = at(14);
  let bob = store.edit_issue_materials(authored(bob, "bob")).await.unwrap();
  assert_eq!(bob.disposition, MutationDisposition::Applied);
  assert_eq!(bob.issue.version.get(), 6);
  assert_eq!(
    bob.issue.materials.entries()[0].selected_by.identity.as_deref(),
    Some("bob")
  );
  assert_eq!(
    store
      .issue_material_edit(&alice, &key("contested"))
      .await
      .unwrap()
      .unwrap()
      .outcome
      .issue,
    winner.issue
  );
  let before = outbox(&database, original.id, "issue.materials-edited").await;
  let replay = store.edit_issue_materials(authored(identical, "alice")).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, results[0].issue);
  assert_eq!(outbox(&database, original.id, "issue.materials-edited").await, before);
  assert_eq!(before.len(), 5);
  assert_eq!(
    store
      .issue_revisions(project.id, original.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    6
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_materials_evidence_failure_rolls_back_history_and_receipt_without_source_effects() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let original = store
    .create_issue(management_mutation(draft(project.id, "issue")))
    .await
    .unwrap()
    .issue;
  let shared = document(&store, project.id, None, "shared").await;
  seed_policy(
    &database,
    &original,
    &IssueFieldPolicy {
      push: Some(IssuePushPolicy {
        mapping: mapping("external"),
        fields: [IssueField::Description, IssueField::Status].into_iter().collect(),
      }),
      ..Default::default()
    },
  )
  .await;
  let command = edit(
    &original,
    vec![IssueMaterialReference::ProjectDocument {
      project_id: project.id,
      artifact: shared.clone(),
    }],
    "atomic",
  );
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT reject_material_evidence CHECK (topic <> 'issue.materials-edited') NOT VALID")
    .execute(&database.pool).await.unwrap();
  assert_eq!(
    store
      .edit_issue_materials(authored(command.clone(), "alice"))
      .await
      .unwrap_err(),
    StoreError::Unavailable
  );
  assert_eq!(store.issue(project.id, original.id, None).await.unwrap(), original);
  assert_eq!(
    store
      .issue_revisions(project.id, original.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  assert!(
    store
      .issue_material_edit(
        &ManagementSecurityScope::new("operator:alice").unwrap(),
        &command.idempotency_key
      )
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    outbox(&database, original.id, "issue.materials-edited")
      .await
      .is_empty()
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT reject_material_evidence")
    .execute(&database.pool)
    .await
    .unwrap();
  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  let selected = restarted
    .edit_issue_materials(authored(command, "alice"))
    .await
    .unwrap();
  assert_eq!(selected.disposition, MutationDisposition::Applied);
  assert_eq!(selected.issue.version.get(), 2);
  assert_eq!(selected.issue.content, original.content);
  assert_eq!(selected.issue.status_id, original.status_id);
  assert_eq!(selected.issue.applicability_version, original.applicability_version);
  let records = outbox(&database, original.id, "issue.materials-edited").await;
  assert_eq!(records.len(), 1);
  assert!(records[0].get("push_intent").is_none());
  assert!(!records[0].to_string().contains("Private shared text"));
  assert_eq!(
    restarted
      .project_document_metadata(project.id, shared.artifact_id())
      .await
      .unwrap()
      .unwrap()
      .artifact,
    shared
  );
  assert_eq!(
    restarted
      .project_document_content(project.id, &ManagementSecurityScope::trusted_network(), &key("shared"))
      .await
      .unwrap()
      .as_deref(),
    Some("Private shared text")
  );
  pool.close().await;
  database.cleanup().await;
}
