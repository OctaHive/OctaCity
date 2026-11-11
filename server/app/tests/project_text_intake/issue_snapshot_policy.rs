//! The pinned policy result survives durable recovery and private input retirement.
use super::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_policy_replay_keeps_scheduling_after_issue_edits_and_body_retirement() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let mut flow = snapshot_flow_with_fields([
    ("title", FlowIssueFieldSource::Title),
    ("priority", FlowIssueFieldSource::Priority),
  ]);
  flow.issue_snapshot.as_mut().unwrap().admission = Some(FlowIssueAdmissionPolicy {
    priority: FlowIssueAdmissionMapping {
      field: "priority".parse().unwrap(),
      values: [
        ("high".into(), WorkPriority::new(80).unwrap()),
        ("low".into(), WorkPriority::new(10).unwrap()),
      ]
      .into(),
    },
    risk: FlowIssueAdmissionMapping {
      field: "priority".parse().unwrap(),
      values: [("high".into(), RiskClass::Low), ("low".into(), RiskClass::Medium)].into(),
    },
    allowed: Default::default(),
  });
  let (issue, submission) = setup_with_flow(&store, Some(flow)).await;
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
      idempotency_key: key("policy-high"),
      edited_at: at(3),
    }))
    .await
    .unwrap()
    .issue;
  let request = snapshot_request(store.clone(), &issue, &submission).await;
  assert_eq!(request.admission_classification.as_ref().unwrap().priority().get(), 80);
  let bytes = Arc::new(Bytes::new());
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: WorkClassification::new(
      WorkPriority::new(1).unwrap(),
      RiskClass::High,
      serde_json::from_value(serde_json::json!({"accepted":"true","admission_risk":"high"})).unwrap(),
    ),
  };
  bytes.lose_ack.store(true, Ordering::SeqCst);
  assert!(
    publisher(
      store.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(0))),
      Arc::new(Clock(AtomicI64::new(20)))
    )
    .handle_command(&context(), command.clone())
    .await
    .is_err()
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let published = publisher(
    restarted.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    Arc::new(Clock(AtomicI64::new(21))),
  )
  .handle_command(&context(), command.clone())
  .await
  .unwrap();
  let association = published.association.as_ref().unwrap();
  let original = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  assert_eq!(original.work.priority().get(), 80);
  assert_eq!(original.work.risk(), RiskClass::Low);
  assert!(original.work.metadata().is_empty());
  assert_eq!(original.flow.data.incoming[0].payload().value()["priority"], "high");
  restarted
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          priority: Some(IssuePriority::Choice("low".into())),
          ..issue.content
        },
      },
      idempotency_key: key("policy-low"),
      edited_at: at(30),
    }))
    .await
    .unwrap();
  let preparation = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    FactoryExecutionInputRetentionWorker::new(restarted.clone(), bytes.clone())
      .run_once(preparation.expires_at, 10)
      .await
      .unwrap(),
    2
  );
  assert!(
    restarted
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key()
      )
      .await
      .unwrap()
      .is_none()
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let mut changed = command;
  changed.classification =
    WorkClassification::new(WorkPriority::new(100).unwrap(), RiskClass::Medium, Default::default());
  let replay = publisher(
    restarted.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    Arc::new(Clock(AtomicI64::new(preparation.expires_at.unix_millis()))),
  )
  .handle_command(&context(), changed)
  .await
  .unwrap();
  assert_eq!(replay, published);
  assert_eq!(
    restarted.factory_run_snapshot(association.run_id).await.unwrap(),
    original
  );
  assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
  database.cleanup().await;
}
