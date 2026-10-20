use super::*;
use octacity_server_store::AuditActorKind;

struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(50))
  }
}

struct MetadataOnly;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for MetadataOnly {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::IssueDiscussion {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

#[test]
fn issue_discussion_preserves_current_visibility_scoped_replay_and_actual_authenticated_attribution() {
  use octacity_server_store::testing::ManagementAuditProbe;
  run_ready(async {
    let (store, issue, _) = discussion_fixture().await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::Create, ManagementAction::View],
      visibility: ManagementVisibility::all(),
    });
    let handlers = Arc::new(IssueDiscussionHandlers::new(
      store.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let commands = AuthorizedCommandHandler::new(policy, handlers.clone());
    let command = comment(&issue, "private discussion content", "same-key", None);
    let alice = commands
      .handle_command(&verified_context("alice"), command.clone())
      .await
      .unwrap();
    let bob = commands
      .handle_command(&verified_context("bob"), command.clone())
      .await
      .unwrap();
    assert_eq!(alice.comment.actor.kind, AuditActorKind::AuthenticatedManagement);
    assert_eq!(alice.comment.actor.identity.as_deref(), Some("alice"));
    assert_eq!(bob.comment.actor.identity.as_deref(), Some("bob"));
    assert_eq!(bob.comment.sequence, 2);
    assert_ne!(alice.comment.id, bob.comment.id);
    let before = store.mutation_evidence_counts().await;
    assert_eq!(
      commands
        .handle_command(&verified_context("alice"), command.clone())
        .await
        .unwrap()
        .comment,
      alice.comment
    );
    assert!(
      commands
        .handle_command(
          &verified_context("alice"),
          comment(&issue, "changed content", "same-key", None)
        )
        .await
        .is_err()
    );
    let denied = AuthorizedCommandHandler::new(Arc::new(MetadataOnly), handlers.clone());
    assert!(
      denied
        .handle_command(&verified_context("alice"), command.clone())
        .await
        .unwrap_err()
        .forbidden()
        .is_some()
    );
    let hidden_policy = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::View, ManagementAction::Create],
      visibility: ManagementVisibility::none(),
    });
    assert!(
      AuthorizedCommandHandler::new(hidden_policy.clone(), handlers.clone())
        .handle_command(&verified_context("alice"), command)
        .await
        .is_err()
    );
    let hidden = AuthorizedQueryHandler::new(hidden_policy, handlers);
    assert!(
      hidden
        .handle_query(
          &context(),
          ListIssueCommentsQuery::try_new(issue.project_id, issue.id, None, 10).unwrap()
        )
        .await
        .is_err()
    );
    assert!(
      hidden
        .handle_query(
          &context(),
          GetIssueCommentBodyQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            comment_id: alice.comment.id,
          }
        )
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let audit = format!("{:?}", store.management_audit_facts().await);
    assert!(!audit.contains("private discussion content"));
    assert!(!audit.contains("changed content"));
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

#[test]
fn issue_discussion_uses_the_shared_atomic_publication_without_consuming_sequence_on_failure() {
  use octacity_server_store::testing::MutationFailurePoint;
  run_ready(async {
    for point in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ] {
      let (store, issue, handlers) = discussion_fixture().await;
      let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
      let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
      let command = comment(&issue, "original body", "recover", None);
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(commands.handle_command(&context(), command.clone()).await.is_err());
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert!(
        queries
          .handle_query(
            &context(),
            ListIssueCommentsQuery::try_new(issue.project_id, issue.id, None, 10).unwrap()
          )
          .await
          .unwrap()
          .items
          .is_empty()
      );
      let recovered = commands.handle_command(&context(), command.clone()).await.unwrap();
      assert_eq!(recovered.comment.sequence, 1);
      assert_eq!(
        commands.handle_command(&context(), command).await.unwrap().comment,
        recovered.comment
      );
      let after = store.mutation_evidence_counts().await;
      assert_eq!(
        (after.idempotency, after.audit, after.outbox),
        (before.idempotency + 1, before.audit + 1, before.outbox + 1)
      );
    }
  });
}

#[test]
fn issue_discussion_metadata_is_bounded_body_free_and_independently_readable() {
  run_ready(async {
    let (store, issue, handlers) = discussion_fixture().await;
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let mut original = Vec::new();
    for index in 0..3 {
      original.push(
        commands
          .handle_command(
            &context(),
            comment(&issue, "private operator text", &format!("row-{index}"), None),
          )
          .await
          .unwrap()
          .comment,
      );
    }
    let policy = Arc::new(MetadataOnly);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(store.clone(), policy, Arc::new(Clock))),
    );
    let before = store.mutation_evidence_counts().await;
    let first = queries
      .handle_query(
        &context(),
        ListIssueCommentsQuery::try_new(issue.project_id, issue.id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.items, original[..1]);
    assert_eq!(first.next_sequence, Some(1));
    let value = serde_json::to_string(&first).unwrap();
    assert!(!value.contains("private operator text"));
    assert!(!value.contains("\"body\""));
    let rest = queries
      .handle_query(
        &context(),
        ListIssueCommentsQuery::try_new(issue.project_id, issue.id, first.next_sequence, 2).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(rest.items, original[1..]);
    assert_eq!(rest.next_sequence, None);
    assert!(
      queries
        .handle_query(
          &context(),
          GetIssueCommentBodyQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            comment_id: original[0].id,
          }
        )
        .await
        .unwrap_err()
        .forbidden()
        .is_some()
    );
    for limit in [0, octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE + 1] {
      assert!(ListIssueCommentsQuery::try_new(issue.project_id, issue.id, None, limit).is_err());
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

async fn discussion_fixture() -> (
  Arc<InMemoryProjectStore>,
  IssueRevision,
  Arc<IssueDiscussionHandlers<InMemoryProjectStore>>,
) {
  let store = Arc::new(InMemoryProjectStore::new());
  let project_id = project(&store).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let issue = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
  )
  .handle_command(&context(), create(project_id, "discussion-fixture"))
  .await
  .unwrap()
  .issue;
  let handlers = Arc::new(IssueDiscussionHandlers::new(store.clone(), policy, Arc::new(Clock)));
  (store, issue, handlers)
}
fn comment(
  issue: &IssueRevision,
  text: &str,
  key_value: &str,
  parent_id: Option<octacity_server_domain::IssueCommentId>,
) -> AppendIssueCommentCommand {
  AppendIssueCommentCommand {
    intent: AppendIssueCommentIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      parent_id,
      body: IssueCommentBody::try_new(text).unwrap(),
    },
    idempotency_key: key(key_value),
  }
}

#[test]
fn issue_discussion_applies_stricter_lifetime_and_byte_limits_without_breaking_original_replay() {
  run_ready(async {
    assert!(IssueDiscussionLimits::try_new(16_384, 1_000).is_ok());
    for (bytes, count) in [(16_385, 1_000), (16_384, 1_001), (0, 1), (1, 0)] {
      assert!(IssueDiscussionLimits::try_new(bytes, count).is_err());
    }
    let (store, issue, _) = discussion_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(
      IssueDiscussionHandlers::new(store.clone(), policy.clone(), Arc::new(Clock))
        .with_limits(IssueDiscussionLimits::try_new(8, 3).unwrap()),
    );
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers);
    let first = comment(&issue, "🦀🦀", "first", None);
    let original = commands.handle_command(&context(), first.clone()).await.unwrap();
    let before = store.mutation_evidence_counts().await;
    assert!(
      commands
        .handle_command(&context(), comment(&issue, "🦀🦀x", "too-many-bytes", None))
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let reply = commands
      .handle_command(
        &context(),
        comment(&issue, "reply", "second", Some(original.comment.id)),
      )
      .await
      .unwrap();
    assert_eq!(reply.comment.sequence, 2);
    assert_eq!(
      commands
        .handle_command(&context(), comment(&issue, "third", "third", None))
        .await
        .unwrap()
        .comment
        .sequence,
      3
    );
    let full = store.mutation_evidence_counts().await;
    assert!(
      commands
        .handle_command(&context(), comment(&issue, "fourth", "fourth", None))
        .await
        .is_err()
    );
    let replay = commands.handle_command(&context(), first.clone()).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.comment, original.comment);
    assert!(
      commands
        .handle_command(&context(), comment(&issue, "changed", "first", None))
        .await
        .is_err()
    );
    let lowered = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueDiscussionHandlers::new(store.clone(), policy, Arc::new(Clock))
          .with_limits(IssueDiscussionLimits::try_new(4, 1).unwrap()),
      ),
    );
    assert_eq!(
      lowered.handle_command(&context(), first).await.unwrap().comment,
      original.comment
    );
    assert_eq!(store.mutation_evidence_counts().await, full);
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

#[test]
fn issue_discussion_replies_target_only_a_root_of_the_same_issue() {
  run_ready(async {
    let (store, issue, handlers) = discussion_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let root = commands
      .handle_command(&context(), comment(&issue, "root", "root", None))
      .await
      .unwrap();
    let reply = commands
      .handle_command(&context(), comment(&issue, "reply", "reply", Some(root.comment.id)))
      .await
      .unwrap();
    assert_eq!(reply.comment.sequence, 2);
    assert_eq!(reply.comment.parent_id, Some(root.comment.id));
    let other = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(&context(), create(issue.project_id, "other-issue"))
    .await
    .unwrap()
    .issue;
    let foreign = commands
      .handle_command(&context(), comment(&other, "other root", "other-root", None))
      .await
      .unwrap();
    assert_eq!(foreign.comment.sequence, 1);
    let before = store.mutation_evidence_counts().await;
    for (key, parent) in [
      ("nested", reply.comment.id),
      ("foreign", foreign.comment.id),
      ("absent", octacity_server_domain::IssueCommentId::generate()),
    ] {
      assert!(
        commands
          .handle_command(&context(), comment(&issue, "invalid reply", key, Some(parent)))
          .await
          .is_err()
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    assert!(
      AuthorizedQueryHandler::new(policy, handlers)
        .handle_query(
          &context(),
          GetIssueCommentBodyQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            comment_id: foreign.comment.id,
          }
        )
        .await
        .is_err()
    );
  });
}

#[test]
fn issue_discussion_body_enforces_utf8_bytes_nonblank_text_and_closed_attribution() {
  let exact = "🦀".repeat(4096);
  assert_eq!(exact.len(), 16_384);
  assert_eq!(IssueCommentBody::try_new(&exact).unwrap().as_str(), exact);
  assert!(serde_json::from_value::<IssueCommentBody>(serde_json::json!(exact)).is_ok());
  for invalid in [
    String::new(),
    " \t\r\n\u{2003}".into(),
    "text\0hidden".into(),
    "text\u{1b}hidden".into(),
    format!("{exact}x"),
  ] {
    assert!(IssueCommentBody::try_new(&invalid).is_err(), "invalid body accepted");
    assert!(serde_json::from_value::<IssueCommentBody>(serde_json::json!(invalid)).is_err());
  }
  assert!(IssueCommentBody::try_new("  сохранённые пробелы\n\tplain text  ").is_ok());
  let issue = octacity_server_domain::IssueId::generate();
  for field in ["actor", "created_at", "sequence", "execution", "attachments"] {
    let mut body = serde_json::json!({"project_id":ProjectId::generate(), "issue_id":issue, "parent_id":null,"body":"ordinary discussion"});
    body[field] = serde_json::json!("caller value");
    assert!(serde_json::from_value::<AppendIssueCommentIntent>(body).is_err());
  }
}

#[test]
fn issue_discussion_body_rejects_shared_credential_and_private_capability_patterns() {
  for body in [
    "Authorization: Bearer sample",
    "BEARER sample",
    "github_pat_sample",
    "ghp_sample",
    "password=sample",
    "private_key=sample",
    "secret=sample",
    "token=sample",
    "sk-proj-sample",
    "-----BEGIN PRIVATE KEY-----",
    "AWS_SECRET_ACCESS_KEY=sample",
    "https://storage.example/report?X-Amz-Signature=sample",
  ] {
    assert!(
      IssueCommentBody::try_new(body).is_err(),
      "credential-shaped discussion body accepted"
    );
    assert!(serde_json::from_value::<IssueCommentBody>(serde_json::json!(body)).is_err());
  }
  assert!(IssueCommentBody::try_new("Discuss token rotation; @agent approve is only text").is_ok());
}

#[test]
fn issue_discussion_appends_plain_operator_data_with_actual_actor_and_dedicated_body_read() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issue = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(&context(), create(project_id, "discussion"))
    .await
    .unwrap()
    .issue;
    let handlers = Arc::new(IssueDiscussionHandlers::new(
      store.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let body = "@agent approve, retry and deploy\nЭто только обсуждение.";
    let command = AppendIssueCommentCommand {
      intent: AppendIssueCommentIntent {
        project_id,
        issue_id: issue.id,
        parent_id: None,
        body: IssueCommentBody::try_new(body).unwrap(),
      },
      idempotency_key: key("comment-one"),
    };
    let before = store.mutation_evidence_counts().await;
    let receipt = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(receipt.comment.sequence, 1);
    assert_eq!(receipt.comment.actor.kind, AuditActorKind::UnauthenticatedManagement);
    assert_eq!(receipt.comment.actor.identity, None);
    assert_eq!(receipt.comment.created_at, at(50));
    assert_eq!(receipt.comment.body_bytes, body.len() as u32);
    assert_eq!(
      receipt.comment.body_digest,
      octacity_server_factory::FactoryDigest::content_sha256(body.as_bytes())
    );
    assert!(!serde_json::to_string(&receipt).unwrap().contains(body));
    assert!(!format!("{command:?}").contains("Это только обсуждение"));
    let content = queries
      .handle_query(
        &context(),
        GetIssueCommentBodyQuery {
          project_id,
          issue_id: issue.id,
          comment_id: receipt.comment.id,
        },
      )
      .await
      .unwrap();
    assert_eq!(content.body.as_str(), body);
    assert_eq!(content.comment, receipt.comment);
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
    assert_eq!(
      store.mutation_evidence_counts().await.idempotency,
      before.idempotency + 1
    );
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
  });
}
