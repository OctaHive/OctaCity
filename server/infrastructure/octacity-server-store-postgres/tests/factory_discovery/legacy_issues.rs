use super::*;
use octacity_server_store::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn legacy_backfill_preserves_original_work_and_converges_after_restart() {
  let database = TestDatabase::before_issue_backfill().await;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let admission = &fixture.admission;
  seed_projection_admission(&database, admission).await;
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .acquire_timeout(std::time::Duration::from_millis(200))
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let store = PostgresStore::new(pool.clone());
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let project = admission.work.subject().project_id();
  let work = admission.work.id();
  // Ordinary reads cannot migrate or admit anything.
  assert!(store.issue_work_association(project, work).await.unwrap().is_none());
  let original = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let initial = store.default_issue_workflow(project).await.unwrap();
  let mut definition = initial.workflow.definition.clone();
  definition.name = "Explicit legacy intake policy".into();
  definition.initial_status_id = definition.statuses[2].id;
  definition.statuses[2].name = "Needs review".into();
  let custom = store
    .publish_issue_workflow(octacity_server_store::testing::management_mutation(
      PublishIssueWorkflow {
        intent: PublishIssueWorkflowIntent {
          project_id: project,
          id: octacity_server_domain::IssueWorkflowId::generate(),
          expected_current_version: None,
          definition,
        },
        idempotency_key: "legacy-policy".parse().unwrap(),
        published_at: time(40),
      },
    ))
    .await
    .unwrap()
    .workflow;
  store
    .select_default_issue_workflow(octacity_server_store::testing::management_mutation(
      SelectDefaultIssueWorkflow {
        intent: SelectDefaultIssueWorkflowIntent {
          project_id: project,
          expected_version: initial.version,
          workflow_id: custom.id,
          workflow_version: custom.version,
        },
        idempotency_key: "legacy-default".parse().unwrap(),
      },
    ))
    .await
    .unwrap();
  let batch = store.backfill_legacy_issue_work(1, time(50)).await.unwrap();
  assert_eq!(batch.len(), 1);
  let association = &batch[0];
  assert_eq!(association.work_id, work);
  assert_eq!(association.run_id, admission.run.id());
  assert_eq!(association.requirements_version, None);
  assert_eq!(association.request_id, None);
  let issue = store.issue(project, association.issue_id, None).await.unwrap();
  assert!(!issue.content_available);
  let page = store.legacy_issue_work(project, issue.id, None, None, 1).await.unwrap();
  assert_eq!(page.items, vec![association.clone()]);
  assert!(page.next.is_none());
  assert_eq!(issue.content.title, "Legacy task (content unavailable)");
  assert_eq!(issue.content.description, "");
  assert_eq!(issue.content.acceptance, None);
  let workflow = store.default_issue_workflow(project).await.unwrap();
  assert_eq!(issue.status_id, workflow.workflow.definition.initial_status_id);
  assert_eq!(issue.workflow_id, custom.id);
  assert_eq!(issue.status_id, custom.definition.initial_status_id);
  let restarted = PostgresStore::new(database.pool.clone());
  assert!(
    restarted
      .backfill_legacy_issue_work(1, time(60))
      .await
      .unwrap()
      .is_empty()
  );
  assert_eq!(
    restarted.issue_work_association(project, work).await.unwrap(),
    Some(association.clone())
  );
  let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM issue_legacy_backfill")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(
    pending, 0,
    "completed association must retire its pending migration record"
  );
  let after = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(original.work, after.work);
  assert_eq!(original.run, after.run);
  assert_eq!(original.current, after.current);
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn legacy_backfill_does_not_capture_work_admitted_after_the_upgrade() {
  let database = TestDatabase::migrated().await;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let (store, _) = seed_projection_admission(&database, &fixture.admission).await;
  assert!(store.backfill_legacy_issue_work(1, time(50)).await.unwrap().is_empty());
  assert!(
    store
      .issue_work_association(
        fixture.admission.work.subject().project_id(),
        fixture.admission.work.id()
      )
      .await
      .unwrap()
      .is_none()
  );
  database.cleanup().await;
}
