use super::*;
use octacity_server_factory::{ExternalWorkIdentity, ImmutableReference, WorkSourceBinding};
use octacity_server_store::{IssueStore, WorkSourceItem};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
fn exact(value: &str) -> ImmutableReference {
  serde_json::from_value(serde_json::json!({"identity":value,"version":"v1","digest":"01".repeat(32)})).unwrap()
}
fn selection() -> IssueCodeReviewSelection {
  IssueCodeReviewSelection {
    source: WorkSourceBinding {
      installation: exact("independent-code-forge"),
      capability: exact("review-reader"),
      configuration: exact("review-settings"),
      resource: exact("code-repository"),
    },
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("native-code-repository").unwrap(),
      item: ExternalWorkIdentity::new("review-42").unwrap(),
    },
  }
}
struct Reader {
  hidden_repository: AtomicBool,
  revoked: AtomicBool,
  reads: AtomicUsize,
}
#[async_trait::async_trait]
impl IssueCodeReviewObservations for Reader {
  async fn visibility(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
  ) -> Result<octacity_server_store::IssueCodeReviewRepositoryVisibility, ApplicationError> {
    Ok(if self.hidden_repository.load(Ordering::SeqCst) {
      octacity_server_store::IssueCodeReviewRepositoryVisibility::none()
    } else {
      octacity_server_store::IssueCodeReviewRepositoryVisibility::all()
    })
  }

  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    if self.revoked.load(Ordering::SeqCst)
      || selection.source != self::selection().source
      || selection.item.resource != self::selection().item.resource
    {
      return Err(ApplicationError::capability_unavailable());
    }
    Ok(())
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    self.authorize(context, project, selection).await?;
    let read = self.reads.fetch_add(1, Ordering::SeqCst);
    Ok(IssueCodeReviewObservation {
      selection: selection.clone(),
      observed_at: at(20),
      title: Some("Visible code change".parse().unwrap()),
      number: Some("42".parse().unwrap()),
      url: None,
      author: None,
      state: Some(IssueCodeReviewState::Open),
      draft: Some(false),
      head: Some(
        if read == 0 { "original-head" } else { "updated-head" }
          .parse()
          .unwrap(),
      ),
      base: None,
      changed_files: None,
      additions: None,
      deletions: None,
      review: None,
      checks: None,
    })
  }
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(30))
  }
}
#[test]
fn issue_code_reviews_attach_multiple_canonical_links_with_exact_replay_without_task_or_execution_effects() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issue = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(&context(), create(project_id, "manual-review-task"))
    .await
    .unwrap()
    .issue;
    let reader = Arc::new(Reader {
      hidden_repository: AtomicBool::new(false),
      revoked: AtomicBool::new(false),
      reads: AtomicUsize::new(0),
    });
    let handlers = Arc::new(IssueCodeReviewHandlers::new(
      store.clone(),
      reader.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers);
    let command = AttachIssueCodeReviewCommand {
      intent: AttachIssueCodeReviewIntent {
        project_id,
        issue_id: issue.id,
        expected_version: issue.version,
        selection: selection(),
        binding: None,
      },
      idempotency_key: key("attach-review"),
    };
    let original = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(original.disposition, MutationDisposition::Applied);
    let replay = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(original.association, replay.association);
    assert_eq!(reader.reads.load(Ordering::SeqCst), 1);
    let mut other = command.clone();
    other.idempotency_key = key("attach-second");
    other.intent.selection.item.item = ExternalWorkIdentity::new("merge-request-77").unwrap();
    commands.handle_command(&context(), other).await.unwrap();
    let page = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewsQuery::try_new(project_id, issue.id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(
      reader.reads.load(Ordering::SeqCst),
      2,
      "GET must never dispatch a provider read"
    );
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
    assert!(
      store
        .issue_source_link(project_id, issue.id, &context().security_scope().to_store())
        .await
        .unwrap()
        .is_none()
    );
    assert!(page.items.iter().all(|item| item.observation.is_some()));
    let mut changed = command.clone();
    changed.intent.selection.item.item = ExternalWorkIdentity::new("review-999").unwrap();
    assert!(commands.handle_command(&context(), changed).await.is_err());
    reader.revoked.store(true, Ordering::SeqCst);
    let unavailable = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewsQuery::try_new(project_id, issue.id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert!(
      unavailable
        .items
        .iter()
        .all(|item| item.availability == IssueCodeReviewAvailability::Blocked && item.observation.is_some())
    );
    assert_eq!(
      commands.handle_command(&context(), command).await.unwrap().association,
      original.association,
      "original receipt remains exact after read revocation"
    );
  });
}

#[test]
fn issue_code_review_refresh_retains_original_heads_and_observation_replay_without_rewriting_binding() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issue = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(&context(), create(project_id, "review-history-task"))
    .await
    .unwrap()
    .issue;
    let reader = Arc::new(Reader {
      hidden_repository: AtomicBool::new(false),
      revoked: AtomicBool::new(false),
      reads: AtomicUsize::new(0),
    });
    let handlers = Arc::new(IssueCodeReviewHandlers::new(
      store.clone(),
      reader.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let original = commands
      .handle_command(
        &context(),
        AttachIssueCodeReviewCommand {
          intent: AttachIssueCodeReviewIntent {
            project_id,
            issue_id: issue.id,
            expected_version: issue.version,
            selection: selection(),
            binding: None,
          },
          idempotency_key: key("first-review"),
        },
      )
      .await
      .unwrap();
    let refresh = RefreshIssueCodeReviewCommand {
      project_id,
      issue_id: issue.id,
      association_id: original.association.id,
      idempotency_key: key("refresh-review"),
    };
    let refreshed = commands.handle_command(&context(), refresh.clone()).await.unwrap();
    assert_eq!(
      refreshed.observation.observation.head.as_ref().unwrap().as_str(),
      "updated-head"
    );
    reader.revoked.store(true, Ordering::SeqCst);
    assert_eq!(
      commands.handle_command(&context(), refresh).await.unwrap().observation,
      refreshed.observation
    );
    assert_eq!(reader.reads.load(Ordering::SeqCst), 2);
    let page = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewObservationsQuery::try_new(project_id, issue.id, original.association.id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(
      page.items[0].observation.head.as_ref().unwrap().as_str(),
      "original-head"
    );
    assert!(page.next_cursor.is_some());
    let next = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewObservationsQuery::try_new(
          project_id,
          issue.id,
          original.association.id,
          page.next_cursor,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(next.items[0], refreshed.observation);
    assert_eq!(next.availability, IssueCodeReviewAvailability::Blocked);
    assert!(next.next_cursor.is_none());
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
  });
}
#[test]
fn issue_code_review_pages_bind_issue_partition_visibility_and_original_membership_before_counts_or_replay() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::View,
        ManagementAction::Update,
        ManagementAction::Create,
      ],
      visibility: ManagementVisibility::all(),
    });
    let manual = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    );
    let issue = manual
      .handle_command(&context(), create(project_id, "visible-review-task"))
      .await
      .unwrap()
      .issue;
    let other = manual
      .handle_command(&context(), create(project_id, "other-review-task"))
      .await
      .unwrap()
      .issue;
    let reader = Arc::new(Reader {
      hidden_repository: AtomicBool::new(false),
      revoked: AtomicBool::new(false),
      reads: AtomicUsize::new(0),
    });
    let handlers = Arc::new(IssueCodeReviewHandlers::new(
      store.clone(),
      reader.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
    let command = AttachIssueCodeReviewCommand {
      intent: AttachIssueCodeReviewIntent {
        project_id,
        issue_id: issue.id,
        expected_version: issue.version,
        selection: selection(),
        binding: None,
      },
      idempotency_key: key("canonical-first"),
    };
    commands.handle_command(&context(), command.clone()).await.unwrap();
    let mut second = command.clone();
    second.idempotency_key = key("canonical-second");
    second.intent.selection.item.item = "mr-77".parse().unwrap();
    commands.handle_command(&context(), second).await.unwrap();
    let first = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewsQuery::try_new(project_id, issue.id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    let cursor = first.next_cursor.unwrap();
    let mut third = command.clone();
    third.idempotency_key = key("canonical-later");
    third.intent.selection.item.item = "review-88".parse().unwrap();
    commands.handle_command(&context(), third).await.unwrap();
    let next = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewsQuery::try_new(project_id, issue.id, Some(cursor.clone()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(next.items[0].association.selection.item.item.as_str(), "mr-77");
    assert!(next.next_cursor.is_none());
    assert!(
      queries
        .handle_query(
          &context(),
          ListIssueCodeReviewsQuery::try_new(project_id, other.id, Some(cursor.clone()), 1).unwrap()
        )
        .await
        .is_err()
    );
    assert!(
      queries
        .handle_query(
          &verified_context("other-reader"),
          ListIssueCodeReviewsQuery::try_new(project_id, issue.id, Some(cursor.clone()), 1).unwrap()
        )
        .await
        .is_err()
    );
    assert!(
      queries
        .handle_query(
          &verified_context("other-reader"),
          ListIssueCodeReviewsQuery::try_new(project_id, issue.id, None, 1).unwrap()
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    let hidden = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::View, ManagementAction::Update],
      visibility: ManagementVisibility::none(),
    });
    assert!(
      AuthorizedQueryHandler::new(hidden.clone(), handlers.clone())
        .handle_query(
          &context(),
          ListIssueCodeReviewsQuery::try_new(project_id, issue.id, Some(cursor), 1).unwrap()
        )
        .await
        .is_err()
    );
    let reads = reader.reads.load(Ordering::SeqCst);
    assert!(
      AuthorizedCommandHandler::new(hidden, handlers)
        .handle_command(&context(), command.clone())
        .await
        .is_err()
    );
    let mut foreign = command.clone();
    foreign.intent.project_id = ProjectId::generate();
    foreign.idempotency_key = key("foreign-project");
    assert!(commands.handle_command(&context(), foreign).await.is_err());
    let mut denied = command.clone();
    denied.idempotency_key = key("wrong-read-purpose");
    denied.intent.selection.source.capability = exact("delivery-writer");
    assert!(commands.handle_command(&context(), denied).await.is_err());
    assert_eq!(reader.reads.load(Ordering::SeqCst), reads);
    for limit in [0, 201] {
      assert!(ListIssueCodeReviewsQuery::try_new(project_id, issue.id, None, limit).is_err());
    }
    assert!(
      ListIssueCodeReviewsQuery::try_new(project_id, issue.id, Some("not-a-canonical-cursor".into()), 1).is_err()
    );
    reader.hidden_repository.store(true, Ordering::SeqCst);
    let hidden_page = queries
      .handle_query(
        &context(),
        ListIssueCodeReviewsQuery::try_new(project_id, issue.id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert!(hidden_page.items.is_empty());
    assert!(hidden_page.next_cursor.is_none());
    assert!(commands.handle_command(&context(), command).await.is_err());
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
  });
}
#[test]
fn issue_code_review_links_are_bounded_safe_navigation_facts_without_fetch_or_identity_authority() {
  run_ready(async {
    let reader = Reader {
      hidden_repository: AtomicBool::new(false),
      revoked: AtomicBool::new(false),
      reads: AtomicUsize::new(0),
    };
    let mut observation = reader
      .read(&context(), ProjectId::generate(), &selection())
      .await
      .unwrap();
    observation.url = Some("https://forge.invalid/reviews/42".parse().unwrap());
    assert!(observation.validate().is_ok());
    for link in [
      "http://forge.invalid/reviews/42",
      "https://operator@forge.invalid/reviews/42",
      "https://forge.invalid/reviews/42?page=2",
      "https://forge.invalid/reviews/42#private",
      "not-a-url",
    ] {
      observation.url = Some(link.parse().unwrap());
      assert!(observation.validate().is_err());
    }
  });
}
