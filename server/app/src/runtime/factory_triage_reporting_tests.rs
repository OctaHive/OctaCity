//! Independent Source/Issue reporting uses one native producer through the real outbox.
use super::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn work_reporting_rejects_overlapping_canonical_issue_push_without_local_transition() {
  for finite in [true, false] {
    let database = TestDatabase::migrated().await;
    let (fixture, profile) = fixture_with_reporting("triage-producer", None, finite).await;
    let fixture = fixture.into_postgres(database.pool.clone()).await;
    let (issue, _, _) = manual_issue(&fixture.store, &fixture).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let outcome = reporting_worker(&fixture, projector, "triage-producer-review", 10000)
      .run_once(time(120))
      .await
      .unwrap();
    assert_eq!(
      outcome.delivered, 0,
      "overlapping Issue push and Work reporting cannot both own the label"
    );
    assert_eq!(outcome.failed, 1);
    assert!(
      captured_requests(&fixture.root.path().join("triage-producer")).is_empty(),
      "producer review precedes any provider inspection or mutation"
    );
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(after.work, before.work);
    assert_eq!(after.flow, before.flow);
    assert_eq!(
      fixture.store.issue(issue.project_id, issue.id, None).await.unwrap(),
      issue
    );
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn work_reporting_preserves_source_target_without_adopting_issue_authority() {
  for condition in ["disabled", "disjoint", "same_group", "foreign_partition"] {
    let database = TestDatabase::migrated().await;
    let policy = serde_json::from_value(serde_json::json!({"labels":["custom-ready"],"states":[],
      "ownership":{"labels":{"custom-ready":["work-group"]},"state":[]}}))
    .unwrap();
    let (fixture, profile) = fixture_with_reporting_policy("triage-isolation", None, Some(policy)).await;
    let fixture = fixture.into_postgres(database.pool.clone()).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    // Explicitly link after action acceptance; neither import nor a card is required by the Work.
    let scope = if condition == "foreign_partition" {
      octacity_server_store::ManagementSecurityScope::new("other-management-partition").unwrap()
    } else {
      octacity_server_store::ManagementSecurityScope::trusted_network()
    };
    let (issue, _, _) = manual_issue_in_scope(&fixture.store, &fixture, &scope).await;
    if condition == "disabled" {
      disable_issue_push(&fixture.store, &issue).await;
    } else if condition == "disjoint" || condition == "same_group" {
      let previous = fixture
        .store
        .issue_source_mapping(issue.project_id, issue.id, &scope, None)
        .await
        .unwrap()
        .unwrap();
      let mut definition = previous.definition.clone();
      definition.status.as_mut().unwrap().push.as_mut().unwrap().statuses[0].add = vec!["issue-owned".into()];
      fixture
        .store
        .publish_issue_source_mapping(management_mutation(PublishIssueSourceMapping {
          writer_access: None,
          intent: PublishIssueSourceMappingIntent {
            project_id: issue.project_id,
            issue_id: issue.id,
            expected_issue_version: issue.version,
            expected_mapping: Some(previous.reference()),
            definition,
          },
          native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
          readable: WorkSourceReportingPolicy { ownership: None,
            labels: vec![],
            states: vec![],
          },
          writable: Some(WorkSourceReportingPolicy { ownership: Some(serde_json::from_value(serde_json::json!({
            "labels":{"custom-ready":["work-group"],"issue-owned":[if condition == "same_group" {"work-group"} else {"issue-group"}]},"state":[]
          })).unwrap()),
            labels: vec!["custom-ready".into(), "issue-owned".into()],
            states: vec![],
          }),
          idempotency_key: IdempotencyKey::new("reviewed-nonoverlap").unwrap(),
          published_at: time(100),
        }))
        .await
        .unwrap();
    }
    assert!(
      fixture
        .store
        .issue_work_association(issue.project_id, before.work.id())
        .await
        .unwrap()
        .is_none(),
      "a canonical link cannot invent reviewed execution requirements or applicability"
    );
    let delivery = reporting_worker(&fixture, projector, "triage-original-source", 10000)
      .run_once(time(120))
      .await
      .unwrap();
    assert_eq!(delivery.delivered, u16::from(condition != "same_group"), "{condition}");
    let calls = captured_requests(&fixture.root.path().join("triage-isolation"));
    if condition == "same_group" {
      assert_eq!(delivery.failed, 1);
      assert!(
        calls.is_empty(),
        "native group conflict is rejected before provider I/O"
      );
    } else {
      assert!(!calls.is_empty());
    }
    for line in calls.lines() {
      let call: serde_json::Value = serde_json::from_str(line).unwrap();
      let original = original_intent(&call);
      assert_eq!(original["item_id"], "TASK-native");
      assert_eq!(original["effects"]["add"], serde_json::json!(["custom-ready"]));
      assert_eq!(original["effects"]["remove"], serde_json::json!([]));
    }
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(after.work, before.work);
    assert_eq!(after.flow, before.flow);
    assert_eq!(
      fixture.store.issue(issue.project_id, issue.id, None).await.unwrap(),
      issue
    );
    database.cleanup().await;
  }
}
