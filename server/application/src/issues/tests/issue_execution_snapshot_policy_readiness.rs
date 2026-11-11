//! Caller envelope metadata cannot satisfy current data or policy prerequisites.
use super::*;

#[test]
fn issue_snapshot_policy_never_uses_an_accepted_flag_to_bypass_readiness() {
  run_ready(async {
    for case in ["missing", "unmapped", "disallowed", "stale"] {
      let (store, factory, mut issue, mut command) = policy_fixture().await;
      if case == "missing" || case == "disallowed" {
        issue = store
          .edit_issue_planning(management_mutation(EditIssuePlanning {
            intent: EditIssuePlanningIntent {
              project_id: issue.project_id,
              id: issue.id,
              expected_version: issue.version,
              planning: IssuePlanning {
                size: if case == "missing" {
                  None
                } else {
                  Some(IssueSize::Large)
                },
                ..issue.planning
              },
            },
            assignees: Default::default(),
            idempotency_key: key("policy-size"),
            edited_at: at(38),
          }))
          .await
          .unwrap()
          .issue;
      } else {
        issue = store
          .edit_issue(management_mutation(EditIssue {
            intent: EditIssueIntent {
              project_id: issue.project_id,
              id: issue.id,
              expected_version: issue.version,
              content: if case == "unmapped" {
                IssueContent {
                  priority: Some(IssuePriority::Choice("low".into())),
                  ..issue.content
                }
              } else {
                IssueContent {
                  description: "Changed requirements".into(),
                  ..issue.content
                }
              },
            },
            idempotency_key: key("policy-content"),
            edited_at: at(38),
          }))
          .await
          .unwrap()
          .issue;
      }
      command.intent.expected_version = issue.version;
      command.intent.classification = Some(WorkClassification::new(
        WorkPriority::new(100).unwrap(),
        RiskClass::Low,
        serde_json::from_value(serde_json::json!({"accepted":"true","admission_risk":"low","ready":"true"})).unwrap(),
      ));
      let before = store.mutation_evidence_counts().await;
      let factory_before = factory.mutation_evidence_counts().await;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let requests = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      let error = requests
        .handle_command(&context(), command)
        .await
        .expect_err("metadata cannot manufacture readiness");
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("expected {case} diagnostic: {error:?}")
      };
      let (field, code) = match case {
        "missing" => ("/issue_snapshot/fields/size", "issue_snapshot_missing_value"),
        "unmapped" => ("/issue_snapshot/admission/priority", "issue_snapshot_policy_ineligible"),
        "disallowed" => ("/issue_snapshot/admission/size", "issue_snapshot_policy_ineligible"),
        "stale" => ("/issue_snapshot/fields/priority", "issue_snapshot_inapplicable"),
        _ => unreachable!(),
      };
      assert!(
        report.errors.iter().any(|e| e.field == field && e.code == code),
        "{case}: {report:?}"
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert_eq!(factory.mutation_evidence_counts().await, factory_before);
    }
  });
}
