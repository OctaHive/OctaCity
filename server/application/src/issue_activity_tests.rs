use super::*;
use octacity_server_domain::IssueCommentId;
use octacity_server_store::{AppendIssueComment, IssueDiscussionStore};
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(100))
  }
}
#[test]
fn issue_timeline_merges_safe_references_in_timestamp_kind_identity_order_with_bounded_pages() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issue = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(&context(), create(project_id, "timeline"))
    .await
    .unwrap()
    .issue;
    for suffix in [2_u128, 1] {
      store
        .append_issue_comment(management_mutation(AppendIssueComment {
          limits: Default::default(),
          id: IssueCommentId::from_uuid(uuid::Uuid::from_u128(suffix)).unwrap(),
          intent: AppendIssueCommentIntent {
            project_id,
            issue_id: issue.id,
            parent_id: None,
            body: IssueCommentBody::try_new("private operator body").unwrap(),
          },
          idempotency_key: key(&format!("comment-{suffix}")),
          created_at: issue.created_at,
        }))
        .await
        .unwrap();
    }
    let before = store.mutation_evidence_counts().await;
    let queries = AuthorizedQueryHandler::new(
      policy,
      Arc::new(IssueActivityHandlers::new(store.clone(), Arc::new(Clock))),
    );
    let first = queries
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(project_id, issue.id, None, None, 2).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].position().kind, IssueTimelineKind::Revision);
    assert_eq!(first.items[1].position().kind, IssueTimelineKind::Status);
    assert_eq!(first.boundary.issue_version, issue.version);
    assert_eq!(first.observation, "issue-timeline-watermark-v1");
    let last = queries
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(project_id, issue.id, None, first.next_cursor.clone(), 2).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(last.items.len(), 2);
    assert_eq!(
      last.items[0].position().identity,
      IssueCommentId::from_uuid(uuid::Uuid::from_u128(1)).unwrap().to_string()
    );
    assert_eq!(
      last.items[1].position().identity,
      IssueCommentId::from_uuid(uuid::Uuid::from_u128(2)).unwrap().to_string()
    );
    assert!(last.next_cursor.is_none());
    assert_eq!(first.boundary, last.boundary);
    for page in [first, last] {
      assert!(!serde_json::to_string(&page).unwrap().contains("private operator body"));
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_timeline_freezes_discussion_and_rejects_cursor_reuse_across_current_visibility_scope_and_filters() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issue = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(&context(), create(project_id, "scoped-timeline"))
    .await
    .unwrap()
    .issue;
    let handlers = Arc::new(IssueActivityHandlers::new(store.clone(), Arc::new(Clock)));
    let queries = AuthorizedQueryHandler::new(policy, handlers.clone());
    let first = queries
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(project_id, issue.id, None, None, 1).unwrap(),
      )
      .await
      .unwrap();
    store
      .append_issue_comment(management_mutation(AppendIssueComment {
        limits: Default::default(),
        id: IssueCommentId::generate(),
        intent: AppendIssueCommentIntent {
          project_id,
          issue_id: issue.id,
          parent_id: None,
          body: IssueCommentBody::try_new("late comment").unwrap(),
        },
        idempotency_key: key("late-comment"),
        created_at: at(50),
      }))
      .await
      .unwrap();
    let continuation =
      ListIssueTimelineQuery::try_new(project_id, issue.id, None, first.next_cursor.clone(), 200).unwrap();
    let rest = queries.handle_query(&context(), continuation.clone()).await.unwrap();
    assert_eq!(rest.items.len(), 1);
    assert_eq!(rest.items[0].position().kind, IssueTimelineKind::Status);
    assert!(rest.next_cursor.is_none());
    assert_eq!(rest.boundary.discussion, 0);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          ListIssueTimelineQuery::try_new(project_id, issue.id, None, None, 200).unwrap()
        )
        .await
        .unwrap()
        .items
        .len(),
      3
    );
    for (project, selected, kinds) in [
      (ProjectId::generate(), issue.id, None),
      (project_id, IssueId::generate(), None),
      (
        project_id,
        issue.id,
        Some([IssueTimelineKind::Status].into_iter().collect()),
      ),
    ] {
      assert_eq!(
        queries
          .handle_query(
            &context(),
            ListIssueTimelineQuery::try_new(project, selected, kinds, first.next_cursor.clone(), 1).unwrap()
          )
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Invalid
      );
    }
    let restricted = AuthorizedQueryHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::View],
        visibility: ManagementVisibility::restricted(vec![
          ManagementResource::instance(
            ManagementResourceKind::Issue,
            ManagementResourceIdentity::new(issue.id.to_string()).unwrap(),
          )
          .unwrap(),
        ])
        .unwrap(),
      }),
      handlers.clone(),
    );
    assert_eq!(
      restricted
        .handle_query(&context(), continuation.clone())
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Invalid
    );
    let scoped = AuthorizedQueryHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::View],
        visibility: ManagementVisibility::all(),
      }),
      handlers.clone(),
    );
    // Authenticated identities have their own current security partition.
    assert_eq!(
      scoped
        .handle_query(&verified_context("other-reader"), continuation.clone())
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Invalid
    );
    let hidden = AuthorizedQueryHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::View],
        visibility: ManagementVisibility::none(),
      }),
      handlers,
    );
    assert_eq!(
      hidden
        .handle_query(&context(), continuation)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::NotFound
    );
  });
}
#[test]
fn issue_activity_queries_reject_noncanonical_and_unbounded_input_before_resource_reads() {
  let project = ProjectId::generate();
  let issue = IssueId::generate();
  for limit in [0, 201] {
    assert!(ListIssueTimelineQuery::try_new(project, issue, None, None, limit).is_err());
  }
  assert!(ListIssueTimelineQuery::try_new(project, issue, Some(Default::default()), None, 1).is_err());
  for cursor in ["bad".to_owned(), "x".repeat(MAX_ISSUE_OBSERVATION_CURSOR_BYTES + 1)] {
    assert!(ListIssueTimelineQuery::try_new(project, issue, None, Some(cursor), 1).is_err());
  }
}
