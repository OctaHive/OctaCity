use super::*;
#[path = "issue_material_replay_tests.rs"]
mod replay;
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest};
use std::sync::atomic::{AtomicBool, Ordering};

struct Deny(ManagementResourceKind, ManagementAction);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for Deny {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.0 && action == self.1 {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
// Models a document still live during resolution but expired by final acceptance.
struct ExpiringClock {
  deadline: Timestamp,
  first: AtomicBool,
}
impl FactoryExecutionClock for ExpiringClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(if self.first.swap(false, Ordering::SeqCst) {
      at(100)
    } else {
      self.deadline
    })
  }
}

#[test]
fn issue_materials_validate_sources_permissions_and_expiry_before_atomic_acceptance() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let issue_commands = AuthorizedCommandHandler::new(policy.clone(), issues);
    let created = issue_commands
      .handle_command(&context(), create(project_id, "guarded-issue"))
      .await
      .unwrap()
      .issue;
    let other = issue_commands
      .handle_command(&context(), create(project_id, "other-owner"))
      .await
      .unwrap()
      .issue;
    let foreign_project = project(&store).await;
    let documents = Arc::new(InMemoryFactoryConfigurationStore::new());
    documents.seed_project(project_id).unwrap();
    documents.seed_project(foreign_project).unwrap();
    let owned = document(&documents, project_id, Some(created.id), "guard-owned", "Owned body").await;
    let shared = document(&documents, project_id, None, "guard-shared", "Shared body").await;
    let foreign = document(&documents, foreign_project, None, "foreign-shared", "Foreign body").await;
    let comments = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(
        store.clone(),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let comment = comments
      .handle_command(
        &context(),
        AppendIssueCommentCommand {
          intent: AppendIssueCommentIntent {
            project_id,
            issue_id: created.id,
            parent_id: None,
            body: IssueCommentBody::try_new("Exact discussion body").unwrap(),
          },
          idempotency_key: key("guard-comment"),
        },
      )
      .await
      .unwrap()
      .comment;
    let valid = vec![
      IssueMaterialReference::Comment {
        project_id,
        issue_id: created.id,
        comment_id: comment.id,
        body_digest: comment.body_digest,
      },
      IssueMaterialReference::IssueDocument {
        project_id,
        issue_id: created.id,
        artifact: owned.clone(),
      },
      IssueMaterialReference::ProjectDocument {
        project_id,
        artifact: shared.clone(),
      },
    ];
    let command = EditIssueMaterialsCommand {
      intent: EditIssueMaterialsIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        references: valid.clone(),
      },
      idempotency_key: key("guard-select"),
    };
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        documents.clone(),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let forged = FactoryArtifactReference::new(
      shared.artifact_id(),
      FactoryDigest::from_bytes([9; 32]),
      shared.encoded_size(),
    )
    .unwrap();
    let wrong_size =
      FactoryArtifactReference::new(shared.artifact_id(), shared.content_digest(), shared.encoded_size() + 1).unwrap();
    let invalid = [
      IssueMaterialReference::Comment {
        project_id,
        issue_id: other.id,
        comment_id: comment.id,
        body_digest: comment.body_digest,
      },
      IssueMaterialReference::Comment {
        project_id,
        issue_id: created.id,
        comment_id: comment.id,
        body_digest: FactoryDigest::from_bytes([9; 32]),
      },
      IssueMaterialReference::Comment {
        project_id,
        issue_id: created.id,
        comment_id: octacity_server_domain::IssueCommentId::generate(),
        body_digest: comment.body_digest,
      },
      IssueMaterialReference::IssueDocument {
        project_id,
        issue_id: other.id,
        artifact: owned.clone(),
      },
      IssueMaterialReference::IssueDocument {
        project_id,
        issue_id: created.id,
        artifact: shared.clone(),
      },
      IssueMaterialReference::ProjectDocument {
        project_id,
        artifact: owned.clone(),
      },
      IssueMaterialReference::ProjectDocument {
        project_id: foreign_project,
        artifact: foreign.clone(),
      },
      IssueMaterialReference::ProjectDocument {
        project_id,
        artifact: foreign,
      },
      IssueMaterialReference::ProjectDocument {
        project_id,
        artifact: forged,
      },
      IssueMaterialReference::ProjectDocument {
        project_id,
        artifact: wrong_size,
      },
    ];
    let before = store.mutation_evidence_counts().await;
    for (index, reference) in invalid.into_iter().enumerate() {
      let mut bad = command.clone();
      bad.intent.references = vec![reference];
      bad.idempotency_key = key(&format!("invalid-source-{index}"));
      assert!(
        commands.handle_command(&context(), bad).await.is_err(),
        "source {index}"
      );
      assert_eq!(store.issue(project_id, created.id, None).await.unwrap(), created);
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    for (kind, action) in [
      (ManagementResourceKind::IssueDiscussion, ManagementAction::View),
      (ManagementResourceKind::Artifact, ManagementAction::Download),
      (ManagementResourceKind::Issue, ManagementAction::View),
      (ManagementResourceKind::Issue, ManagementAction::Update),
      (ManagementResourceKind::Project, ManagementAction::View),
    ] {
      let denied = Arc::new(Deny(kind, action));
      assert!(
        AuthorizedCommandHandler::new(
          denied.clone(),
          Arc::new(IssueMaterialHandlers::new(
            store.clone(),
            documents.clone(),
            denied,
            Arc::new(Clock)
          ))
        )
        .handle_command(&context(), command.clone())
        .await
        .is_err()
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    let mut wire = serde_json::to_value(&command.intent).unwrap();
    wire["selected_by"] = serde_json::json!("forged");
    assert!(serde_json::from_value::<EditIssueMaterialsIntent>(wire).is_err());
    let deadline = documents
      .project_document_metadata(project_id, shared.artifact_id())
      .await
      .unwrap()
      .unwrap()
      .expires_at;
    let expiring = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        documents.clone(),
        policy,
        Arc::new(ExpiringClock {
          deadline,
          first: AtomicBool::new(true),
        }),
      )),
    );
    let mut expiring_command = command;
    expiring_command.intent.references = vec![valid[2].clone()];
    assert!(
      expiring.handle_command(&context(), expiring_command).await.is_err(),
      "expired during review"
    );
    assert_eq!(store.issue(project_id, created.id, None).await.unwrap(), created);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
