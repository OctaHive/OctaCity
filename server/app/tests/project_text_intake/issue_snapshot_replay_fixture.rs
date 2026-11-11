//! Durable snapshot input survives unknown acknowledgements and process restart.
use super::*;

pub(super) async fn replay_fixture(store: &PostgresStore) -> (IssueRevision, ProjectTextSubmission) {
  let flow = snapshot_flow_with_fields([
    ("title", FlowIssueFieldSource::Title),
    ("priority", FlowIssueFieldSource::Priority),
    ("size", FlowIssueFieldSource::Size),
  ]);
  let (issue, submission) = setup_with_flow(store, Some(flow)).await;
  let issue = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          priority: Some(IssuePriority::Choice("high".into())),
          ..issue.content
        },
      },
      idempotency_key: key("reviewed-priority"),
      edited_at: at(3),
    }))
    .await
    .unwrap()
    .issue;
  let issue = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        planning: IssuePlanning {
          size: Some(IssueSize::Small),
          ..issue.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("reviewed-size"),
      edited_at: at(4),
    }))
    .await
    .unwrap()
    .issue;
  (issue, submission)
}

pub(super) async fn change_reviewed_issue(store: &PostgresStore, issue: &IssueRevision) -> IssueRevision {
  let mut definition = store.issue_properties(issue.project_id, None).await.unwrap().definition;
  definition
    .priorities
    .iter_mut()
    .find(|choice| choice.id == "high")
    .unwrap()
    .retired = true;
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition,
      },
      idempotency_key: key("later-catalog"),
      published_at: at(25),
    }))
    .await
    .unwrap();
  let issue = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          title: "Later title".into(),
          description: "Later requirements".into(),
          acceptance: Some("Later acceptance".into()),
          priority: Some(IssuePriority::Choice("low".into())),
        },
      },
      idempotency_key: key("later-requirements-and-priority"),
      edited_at: at(26),
    }))
    .await
    .unwrap()
    .issue;
  store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        planning: IssuePlanning {
          definition_version: 1,
          size: Some(IssueSize::Large),
          ..issue.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("later-size"),
      edited_at: at(27),
    }))
    .await
    .unwrap()
    .issue
}
