//! One-shot HTTP observation receipts reuse native refresh publication and retain legacy replay.
use super::*;
use octacity_server_store::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_explicit_refresh_replay_survives_rollback_restart_and_keeps_legacy_observation_identity() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let imported = import(project.id, "explicit-read-link");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("linked Issue")
  };
  let issue = store.issue(project.id, link.issue_id, None).await.unwrap();
  let scope = ManagementSecurityScope::trusted_network();
  let explicit_read = FactoryDigest::content_sha256(b"frozen public read command");
  let mut request = RefreshIssueSource {
    explicit_read: Some(explicit_read),
    status: None,
    read_claim: None,
    selection: imported.selection,
    issue_id: issue.id,
    content: issue.content.clone(),
    observation: imported.observation,
    idempotency_key: key("same-key-independent-producer"),
    refreshed_at: at(6),
  };
  request.observation.observed_at = at(5);
  let before = store
    .issue_source_refresh_state(project.id, issue.id, &scope)
    .await
    .unwrap();
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_explicit_refresh CHECK (topic <> 'issue.source-refreshed') NOT VALID")
    .execute(&database.pool).await.unwrap();
  assert!(
    store
      .refresh_issue_source(management_mutation(request.clone()))
      .await
      .is_err()
  );
  assert!(
    store
      .issue_source_refresh_replay(project.id, issue.id, &scope, explicit_read, &request.idempotency_key)
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(
    store
      .issue_source_refresh_state(project.id, issue.id, &scope)
      .await
      .unwrap(),
    before
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_explicit_refresh")
    .execute(&database.pool)
    .await
    .unwrap();
  let original = store
    .refresh_issue_source(management_mutation(request.clone()))
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .issue_source_refresh_replay(project.id, issue.id, &scope, explicit_read, &request.idempotency_key)
      .await
      .unwrap(),
    Some(original.clone())
  );
  assert!(
    restarted
      .issue_source_refresh_replay(
        project.id,
        issue.id,
        &scope,
        FactoryDigest::content_sha256(b"changed command"),
        &request.idempotency_key
      )
      .await
      .is_err()
  );
  request.observation.observed_at = at(7);
  request.refreshed_at = at(8);
  assert_eq!(
    restarted
      .refresh_issue_source(management_mutation(request.clone()))
      .await
      .unwrap(),
    original
  );
  assert_eq!(
    restarted
      .issue_source_refresh_state(project.id, issue.id, &scope)
      .await
      .unwrap()
      .unwrap()
      .accepted
      .observed_at,
    at(5)
  );
  request.explicit_read = None;
  restarted
    .refresh_issue_source(management_mutation(request.clone()))
    .await
    .unwrap();
  let accepted = restarted
    .issue_source_refresh_state(project.id, issue.id, &scope)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    accepted.accepted.observed_at,
    at(7),
    "the existing producer has an independent replay namespace"
  );
  request.observation.observed_at = at(9);
  request.refreshed_at = at(10);
  assert!(
    restarted
      .refresh_issue_source(management_mutation(request.clone()))
      .await
      .is_err(),
    "legacy producer still requires its exact original observation"
  );
  assert_eq!(restarted.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}
