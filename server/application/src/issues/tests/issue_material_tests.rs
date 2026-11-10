use super::*;
#[path = "issue_material_guard_tests.rs"]
mod guards;
use octacity_server_store::{IssueDiscussionStore, testing::InMemoryFactoryConfigurationStore};
use octacity_server_store::{
  IssueDocumentInputStore, ProjectDocumentInputStore, ProjectDocumentPurpose, ProjectDocumentQueryStore,
  ProjectDocumentSubmission, ReserveProjectDocument,
};

struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(100))
  }
}

#[test]
fn issue_materials_reject_duplicate_and_over_limit_selections_atomically_across_source_kinds() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let created = AuthorizedCommandHandler::new(policy.clone(), issues)
      .handle_command(&context(), create(project_id, "bounded-issue"))
      .await
      .unwrap()
      .issue;
    let mut legacy = serde_json::to_value(&created).unwrap();
    legacy.as_object_mut().unwrap().remove("materials");
    assert!(
      serde_json::from_value::<IssueRevision>(legacy)
        .unwrap()
        .materials
        .is_empty()
    );
    let comments = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(
        store.clone(),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let mut references = vec![];
    for index in 0..31 {
      let comment = comments
        .handle_command(
          &context(),
          AppendIssueCommentCommand {
            intent: AppendIssueCommentIntent {
              project_id,
              issue_id: created.id,
              parent_id: None,
              body: IssueCommentBody::try_new(format!("Reproduction detail {index}")).unwrap(),
            },
            idempotency_key: key(&format!("bounded-comment-{index}")),
          },
        )
        .await
        .unwrap()
        .comment;
      references.push(IssueMaterialReference::Comment {
        project_id,
        issue_id: created.id,
        comment_id: comment.id,
        body_digest: comment.body_digest,
      });
    }
    let documents = Arc::new(InMemoryFactoryConfigurationStore::new());
    documents.seed_project(project_id).unwrap();
    let owned = document(
      &documents,
      project_id,
      Some(created.id),
      "bounded-owned",
      "Owned material",
    )
    .await;
    let shared = document(&documents, project_id, None, "bounded-shared", "Shared material").await;
    references.insert(
      0,
      IssueMaterialReference::IssueDocument {
        project_id,
        issue_id: created.id,
        artifact: owned,
      },
    );
    references.insert(
      0,
      IssueMaterialReference::ProjectDocument {
        project_id,
        artifact: shared,
      },
    );
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        documents,
        policy,
        Arc::new(Clock),
      )),
    );
    let before = store.mutation_evidence_counts().await;
    for (index, invalid) in [
      vec![references[0].clone(), references[0].clone()],
      vec![references[2].clone(), references[2].clone()],
      references.clone(),
    ]
    .into_iter()
    .enumerate()
    {
      assert!(
        commands
          .handle_command(
            &context(),
            EditIssueMaterialsCommand {
              intent: EditIssueMaterialsIntent {
                project_id,
                id: created.id,
                expected_version: created.version,
                references: invalid
              },
              idempotency_key: key(&format!("invalid-bound-{index}")),
            }
          )
          .await
          .is_err(),
        "invalid selection {index} was accepted"
      );
      assert_eq!(store.issue(project_id, created.id, None).await.unwrap(), created);
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    references.pop();
    let selected = commands
      .handle_command(
        &context(),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            project_id,
            id: created.id,
            expected_version: created.version,
            references,
          },
          idempotency_key: key("exactly-32"),
        },
      )
      .await
      .unwrap()
      .issue;
    assert_eq!(selected.materials.entries().len(), 32);
    let entry = serde_json::to_value(&selected.materials.entries()[0]).unwrap();
    assert!(serde_json::from_value::<IssueMaterials>(serde_json::json!([entry, entry])).is_err());
    assert!(serde_json::from_value::<IssueMaterials>(serde_json::Value::Array(vec![entry.clone(); 33])).is_err());
    let mut forged = serde_json::to_value(&selected.materials).unwrap();
    forged[0]["selected_by"]["kind"] = serde_json::json!("worker");
    assert!(serde_json::from_value::<IssueMaterials>(forged).is_err());
  });
}

async fn document(
  documents: &InMemoryFactoryConfigurationStore,
  project_id: ProjectId,
  issue_id: Option<octacity_server_domain::IssueId>,
  identity: &str,
  content: &str,
) -> octacity_server_factory::FactoryArtifactReference {
  let reserved = documents
    .reserve_project_document(management_mutation(ReserveProjectDocument {
      submission: ProjectDocumentSubmission {
        project_id,
        name: "Investigation".parse().unwrap(),
        content: content.into(),
        purpose: ProjectDocumentPurpose::Context,
        idempotency_key: key(identity),
      },
      issue_id,
      selection_digest: None,
      now: at(20),
    }))
    .await
    .unwrap();
  documents
    .publish_project_document(management_mutation(reserved.guard(at(21))))
    .await
    .unwrap()
    .objects[0]
    .artifact
    .clone()
}

#[test]
fn issue_materials_pin_both_document_owners_without_retargeting_or_transferring_content() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let created = AuthorizedCommandHandler::new(policy.clone(), issues.clone())
      .handle_command(&context(), create(project_id, "documents-issue"))
      .await
      .unwrap()
      .issue;
    let documents = Arc::new(InMemoryFactoryConfigurationStore::new());
    documents.seed_project(project_id).unwrap();
    let owned = document(
      &documents,
      project_id,
      Some(created.id),
      "issue-document",
      "Private issue instructions",
    )
    .await;
    let shared = document(
      &documents,
      project_id,
      None,
      "shared-document",
      "Private shared instructions",
    )
    .await;
    assert!(
      store
        .issue(project_id, created.id, None)
        .await
        .unwrap()
        .materials
        .is_empty()
    );
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        documents.clone(),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let selected = commands
      .handle_command(
        &context(),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            project_id,
            id: created.id,
            expected_version: created.version,
            references: vec![
              IssueMaterialReference::IssueDocument {
                project_id,
                issue_id: created.id,
                artifact: owned.clone(),
              },
              IssueMaterialReference::ProjectDocument {
                project_id,
                artifact: shared.clone(),
              },
            ],
          },
          idempotency_key: key("select-documents"),
        },
      )
      .await
      .unwrap()
      .issue;
    document(
      &documents,
      project_id,
      Some(created.id),
      "issue-document-v2",
      "Later issue text",
    )
    .await;
    document(&documents, project_id, None, "shared-document-v2", "Later shared text").await;
    let queried = AuthorizedQueryHandler::new(policy, issues)
      .handle_query(
        &context(),
        GetIssueQuery {
          project_id,
          id: created.id,
          version: None,
        },
      )
      .await
      .unwrap();
    assert_eq!(queried, selected);
    assert_eq!(queried.materials.entries().len(), 2);
    let wire = serde_json::to_string(&queried).unwrap();
    assert!(!wire.contains("Private issue instructions"));
    assert!(!wire.contains("Private shared instructions"));
    commands
      .handle_command(
        &context(),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            project_id,
            id: created.id,
            expected_version: selected.version,
            references: vec![],
          },
          idempotency_key: key("remove-documents"),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      documents
        .issue_document_metadata(project_id, created.id, owned.artifact_id())
        .await
        .unwrap()
        .unwrap()
        .artifact,
      owned
    );
    assert_eq!(
      documents
        .project_document_metadata(project_id, shared.artifact_id())
        .await
        .unwrap()
        .unwrap()
        .artifact,
      shared
    );
    assert!(
      documents
        .project_document_metadata(project_id, owned.artifact_id())
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      documents
        .issue_document_input(project_id, created.id, owned.artifact_id())
        .await
        .unwrap()
        .is_some()
    );
    assert!(
      documents
        .issue_document_input(project_id, created.id, shared.artifact_id())
        .await
        .unwrap()
        .is_none()
    );
  });
}

#[test]
fn issue_materials_select_remove_and_replay_exact_comment_without_copying_body() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let created = AuthorizedCommandHandler::new(policy.clone(), issues.clone())
      .handle_command(&context(), create(project_id, "create-materials"))
      .await
      .unwrap()
      .issue;
    let comments = Arc::new(IssueDiscussionHandlers::new(
      store.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let comment = AuthorizedCommandHandler::new(policy.clone(), comments)
      .handle_command(
        &context(),
        AppendIssueCommentCommand {
          intent: AppendIssueCommentIntent {
            project_id,
            issue_id: created.id,
            parent_id: None,
            body: IssueCommentBody::try_new("Private reproduction instructions").unwrap(),
          },
          idempotency_key: key("comment-material"),
        },
      )
      .await
      .unwrap()
      .comment;
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        Arc::new(InMemoryFactoryConfigurationStore::new()),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let select = EditIssueMaterialsCommand {
      intent: EditIssueMaterialsIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        references: vec![IssueMaterialReference::Comment {
          project_id,
          issue_id: created.id,
          comment_id: comment.id,
          body_digest: comment.body_digest,
        }],
      },
      idempotency_key: key("select-comment"),
    };
    assert!(created.materials.is_empty());
    let selected = commands.handle_command(&context(), select.clone()).await.unwrap().issue;
    assert_eq!(selected.version.get(), 2);
    assert_eq!(selected.applicability_version, created.applicability_version);
    assert_eq!(selected.materials.entries()[0].reference, select.intent.references[0]);
    assert_eq!(selected.materials.entries()[0].selected_at, at(100));
    assert!(
      !serde_json::to_string(&selected)
        .unwrap()
        .contains("Private reproduction instructions")
    );
    let removed = commands
      .handle_command(
        &context(),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            expected_version: selected.version,
            references: vec![],
            ..select.intent.clone()
          },
          idempotency_key: key("remove-comment"),
        },
      )
      .await
      .unwrap()
      .issue;
    assert!(removed.materials.is_empty());
    let queries = AuthorizedQueryHandler::new(policy, issues);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: created.id,
            version: Some(selected.version)
          }
        )
        .await
        .unwrap(),
      selected
    );
    let before = store.mutation_evidence_counts().await;
    let replay = commands.handle_command(&context(), select).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, selected);
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(store.issue(project_id, created.id, None).await.unwrap(), removed);
    assert_eq!(
      store
        .issue_comment_body(project_id, created.id, comment.id)
        .await
        .unwrap()
        .body
        .as_str(),
      "Private reproduction instructions"
    );
  });
}
