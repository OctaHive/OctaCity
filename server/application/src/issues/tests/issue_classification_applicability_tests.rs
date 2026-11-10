//! Applicability is reviewed through the authorized Issue API, independently of execution admission.
use super::*;
use octacity_server_store::IssueClassificationProblem as Problem;

fn review(issue: &IssueRevision) -> ReviewIssueClassificationQuery {
  ReviewIssueClassificationQuery {
    project_id: issue.project_id,
    id: issue.id,
    version: None,
    subject: None,
    checked_at: at(100),
  }
}

#[test]
fn classification_review_marks_changed_acceptance_stale_without_mutating_the_assessment() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let created = commands
      .handle_command(&context(), create(project_id, "applicability"))
      .await
      .unwrap()
      .issue;
    let mut edit = planning_edit(&created, "size");
    edit.intent.planning.size = Some(octacity_server_store::IssueSize::Small);
    let assessed = commands.handle_command(&context(), edit).await.unwrap().issue;
    let initial = queries.handle_query(&context(), review(&assessed)).await.unwrap();
    let size = initial.fields.iter().find(|field| field.field == Field::Size).unwrap();
    assert!(size.problems.is_empty());
    assert_eq!(initial.version, assessed.version);
    let changed = commands
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id,
            id: assessed.id,
            expected_version: assessed.version,
            content: IssueContent {
              acceptance: Some("A different expected behavior".into()),
              ..assessed.content.clone()
            },
          },
          idempotency_key: key("acceptance"),
          edited_at: at(31),
        },
      )
      .await
      .unwrap()
      .issue;
    let before = store.mutation_evidence_counts().await;
    let stale = queries.handle_query(&context(), review(&changed)).await.unwrap();
    let size = stale.fields.iter().find(|field| field.field == Field::Size).unwrap();
    assert_eq!(size.problems, vec![Problem::RequirementsChanged]);
    assert_eq!(changed.classification, assessed.classification);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: changed.id,
            version: None
          }
        )
        .await
        .unwrap(),
      changed
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

struct CommentClock;
impl FactoryExecutionClock for CommentClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(32))
  }
}

#[test]
fn classification_review_preserves_assessment_for_assignee_date_and_unselected_comment_changes() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let people = Arc::new(People(std::sync::Mutex::new(IssueAssigneeChoices {
      available: true,
      items: vec![IssueAssignee {
        id: "person".into(),
        name: "Person".into(),
      }],
    })));
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()).with_assignee_directory(people));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers);
    let original = commands
      .handle_command(&context(), create(project_id, "organizational-edits"))
      .await
      .unwrap()
      .issue;
    let mut classify = planning_edit(&original, "size");
    classify.intent.planning.size = Some(octacity_server_store::IssueSize::Small);
    let assessed = commands.handle_command(&context(), classify).await.unwrap().issue;
    let mut organizational = planning_edit(&assessed, "organization");
    organizational.edited_at = at(31);
    organizational.intent.planning.assignee = Some("person".into());
    organizational.intent.planning.due_date = Some("2026-12-01".into());
    let changed = commands.handle_command(&context(), organizational).await.unwrap().issue;
    assert_eq!(changed.classification, assessed.classification);
    assert_eq!(changed.applicability_version, assessed.applicability_version);
    let comments = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(
        store.clone(),
        policy,
        Arc::new(CommentClock),
      )),
    );
    comments
      .handle_command(
        &context(),
        AppendIssueCommentCommand {
          intent: AppendIssueCommentIntent {
            project_id,
            issue_id: changed.id,
            parent_id: None,
            body: IssueCommentBody::try_new("An unselected discussion note").unwrap(),
          },
          idempotency_key: key("unselected-comment"),
        },
      )
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let current = queries
      .handle_query(
        &context(),
        GetIssueQuery {
          project_id,
          id: changed.id,
          version: None,
        },
      )
      .await
      .unwrap();
    assert_eq!(current, changed);
    let reviewed = queries.handle_query(&context(), review(&current)).await.unwrap();
    assert!(
      reviewed
        .fields
        .iter()
        .find(|f| f.field == Field::Size)
        .unwrap()
        .problems
        .is_empty()
    );
    assert!(current.materials.is_empty());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
