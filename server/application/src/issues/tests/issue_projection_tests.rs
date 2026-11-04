use super::*;

#[test]
fn issue_card_retains_planning_pinned_policy_and_status_history_after_workflow_replacement() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers);
    let mut command = create(project_id, "planned-card");
    command.intent.content.priority = Some(7);
    command.intent.planning.due_date = Some("2026-11-15".into());
    let issue = commands.handle_command(&context(), command).await.unwrap().issue;
    let original = store
      .issue_workflow(project_id, issue.workflow_id, Some(issue.workflow_version))
      .await
      .unwrap();
    let mut definition = original.definition.clone();
    definition.name = "Later workflow".into();
    AuthorizedCommandHandler::new(policy, Arc::new(IssueWorkflowHandlers::new(store.clone())))
      .handle_command(
        &context(),
        PublishIssueWorkflowCommand {
          intent: PublishIssueWorkflowIntent {
            project_id,
            id: issue.workflow_id,
            expected_current_version: Some(issue.workflow_version),
            definition,
          },
          idempotency_key: key("replacement"),
          published_at: at(20),
        },
      )
      .await
      .unwrap();
    let moved = commands
      .handle_command(&context(), move_from(&issue, "card-move"))
      .await
      .unwrap();
    let card = queries
      .handle_query(
        &context(),
        GetIssueCardQuery {
          project_id,
          id: issue.id,
        },
      )
      .await
      .unwrap();
    assert_eq!(card.issue.status_id, moved.issue.status_id);
    assert_eq!(card.priority, Some(7));
    assert_eq!(card.planning.due_date.as_deref(), Some("2026-11-15"));
    assert_eq!(card.workflow, original);
    let history = queries
      .handle_query(
        &context(),
        ListIssueStatusHistoryQuery::try_new(project_id, issue.id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    let receipt = history.items.iter().find(|row| row.transition_id.is_some()).unwrap();
    assert_eq!(receipt.workflow_version, original.version);
    assert_eq!(receipt.from, Some(issue.status_id));
    assert_eq!(receipt.to, moved.issue.status_id);
    assert_eq!(receipt.origin, octacity_server_store::IssueTransitionOrigin::Manual);
    assert!(receipt.pipeline.is_none());
    assert!(receipt.source.is_none());
  });
}

#[test]
fn issue_card_projects_pinned_business_policy_without_bodies_or_execution_dependencies() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let mut command = create(project_id, "card");
    command.intent.content.description = "private requirements".into();
    command.intent.content.acceptance = Some("private acceptance".into());
    let issue = commands.handle_command(&context(), command).await.unwrap().issue;
    let before = store.mutation_evidence_counts().await;
    let card = AuthorizedQueryHandler::new(policy, handlers)
      .handle_query(
        &context(),
        GetIssueCardQuery {
          project_id,
          id: issue.id,
        },
      )
      .await
      .unwrap();
    assert_eq!(card.issue.id, issue.id);
    assert_eq!(card.workflow.id, issue.workflow_id);
    assert_eq!(card.workflow.version, issue.workflow_version);
    assert_eq!(card.issue.status_id, card.workflow.definition.initial_status_id);
    let value = serde_json::to_value(card).unwrap();
    for private in ["description", "acceptance", "work", "run", "accepted", "transcript"] {
      assert!(value.get(private).is_none());
      assert!(value["issue"].get(private).is_none());
    }
    assert!(!value.to_string().contains("private requirements"));
    assert!(!value.to_string().contains("private acceptance"));
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
