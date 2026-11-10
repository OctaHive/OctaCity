use super::*;
use octacity_server_store::testing::MutationFailurePoint;

#[test]
fn issue_materials_preserve_scoped_receipts_attribution_and_atomic_revision_publication() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::View,
        ManagementAction::Create,
        ManagementAction::Update,
      ],
      visibility: ManagementVisibility::all(),
    });
    let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let created = AuthorizedCommandHandler::new(policy.clone(), issues.clone())
      .handle_command(&context(), create(project_id, "atomic-materials"))
      .await
      .unwrap()
      .issue;
    let comments = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(
        store.clone(),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let mut references = vec![];
    for name in ["first", "second"] {
      let comment = comments
        .handle_command(
          &verified_context("author"),
          AppendIssueCommentCommand {
            intent: AppendIssueCommentIntent {
              project_id,
              issue_id: created.id,
              parent_id: None,
              body: IssueCommentBody::try_new(format!("Private {name} context")).unwrap(),
            },
            idempotency_key: key(name),
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
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        documents.clone(),
        policy.clone(),
        Arc::new(Clock),
      )),
    );
    let command = EditIssueMaterialsCommand {
      intent: EditIssueMaterialsIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        references: vec![references[0].clone()],
      },
      idempotency_key: key("shared-key"),
    };
    let before = store.mutation_evidence_counts().await;
    for point in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ] {
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(
        commands
          .handle_command(&verified_context("alice"), command.clone())
          .await
          .is_err()
      );
      assert_eq!(store.issue(project_id, created.id, None).await.unwrap(), created);
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    let selected = commands
      .handle_command(&verified_context("alice"), command.clone())
      .await
      .unwrap()
      .issue;
    assert_eq!(selected.version.get(), 2);
    assert_eq!(
      selected.materials.entries()[0].selected_by.identity.as_deref(),
      Some("alice")
    );
    assert!(
      commands
        .handle_command(&verified_context("bob"), command.clone())
        .await
        .is_err()
    );
    let bob = commands
      .handle_command(
        &verified_context("bob"),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            expected_version: selected.version,
            references,
            ..command.intent.clone()
          },
          idempotency_key: command.idempotency_key.clone(),
        },
      )
      .await
      .unwrap();
    assert_eq!(bob.disposition, MutationDisposition::Applied);
    assert_eq!(bob.issue.version.get(), 3);
    assert_eq!(bob.issue.materials.entries()[0], selected.materials.entries()[0]);
    assert_eq!(
      bob.issue.materials.entries()[1].selected_by.identity.as_deref(),
      Some("bob")
    );
    let denied_source = Arc::new(Deny(ManagementResourceKind::IssueDiscussion, ManagementAction::View));
    let removals = AuthorizedCommandHandler::new(
      denied_source.clone(),
      Arc::new(IssueMaterialHandlers::new(
        store.clone(),
        documents.clone(),
        denied_source,
        Arc::new(Clock),
      )),
    );
    let removed = removals
      .handle_command(
        &verified_context("alice"),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            expected_version: bob.issue.version,
            references: vec![],
            ..command.intent.clone()
          },
          idempotency_key: key("remove-hidden-sources"),
        },
      )
      .await
      .unwrap()
      .issue;
    assert!(removed.materials.is_empty());
    let before = store.mutation_evidence_counts().await;
    let replay = removals
      .handle_command(&verified_context("alice"), command.clone())
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, selected);
    let mut conflicting = command.clone();
    conflicting.intent.references.clear();
    assert!(
      commands
        .handle_command(&verified_context("alice"), conflicting)
        .await
        .is_err()
    );
    let denied_issue = Arc::new(Deny(ManagementResourceKind::Issue, ManagementAction::Update));
    assert!(
      AuthorizedCommandHandler::new(
        denied_issue.clone(),
        Arc::new(IssueMaterialHandlers::new(
          store.clone(),
          documents,
          denied_issue,
          Arc::new(Clock)
        ))
      )
      .handle_command(&verified_context("alice"), command)
      .await
      .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let queries = AuthorizedQueryHandler::new(policy, issues);
    assert_eq!(
      queries
        .handle_query(
          &verified_context("alice"),
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
    assert_eq!(
      queries
        .handle_query(
          &verified_context("alice"),
          GetIssueQuery {
            project_id,
            id: created.id,
            version: None
          }
        )
        .await
        .unwrap(),
      removed
    );
  });
}
