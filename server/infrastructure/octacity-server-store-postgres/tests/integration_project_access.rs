mod support;
use octacity_server_domain::{ProjectId, ProjectName, Timestamp};
use octacity_server_store::testing::{management_mutation, verify_project_grant_replay, verify_project_grant_subset};
use octacity_server_store::{CreateProject, IdempotencyKey, ProjectStore};
use octacity_server_store_postgres::PostgresStore;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn project_resource_grants_persist_through_the_public_store_contract() {
  let database = support::TestDatabase::migrated().await;
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let store = PostgresStore::new(pool.clone());
  let project_id = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Grant owner").unwrap(),
      idempotency_key: IdempotencyKey::new("project").unwrap(),
      created_at: Timestamp::from_unix_millis(0).unwrap(),
    }))
    .await
    .unwrap();
  // A successful publication cannot rely on a spare pool connection.
  verify_project_grant_subset(&store, project_id).await;
  verify_project_grant_replay(&PostgresStore::new(database.pool.clone()), project_id).await;
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn project_grants_commit_once_under_competing_publication_and_rollback_failed_events() {
  use octacity_server_domain::{IntegrationId, IntegrationVersion};
  use octacity_server_store::{testing::integration_connection_fixture, *};
  let database = support::TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project_id = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Access owner").unwrap(),
      idempotency_key: IdempotencyKey::new("owner").unwrap(),
      created_at: Timestamp::from_unix_millis(0).unwrap(),
    }))
    .await
    .unwrap();
  let connection_id = IntegrationId::generate();
  let (installed, draft) = integration_connection_fixture("jira");
  let resources = draft.resources.clone();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id: connection_id,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("connection").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  let mut request = PublishIntegrationProjectAccess {
    intent: IntegrationProjectAccessIntent {
      connection_id,
      project_id,
      connection_version: IntegrationVersion::INITIAL,
      expected_current_version: None,
      resources,
    },
    idempotency_key: IdempotencyKey::new("grant").unwrap(),
    published_at: Timestamp::from_unix_millis(2).unwrap(),
  };
  sqlx::raw_sql("CREATE FUNCTION reject_grant_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test event failure'; END $$; CREATE TRIGGER reject_grant_event BEFORE INSERT ON outbox_entries FOR EACH ROW WHEN (NEW.topic = 'integration.project-access.published') EXECUTE FUNCTION reject_grant_event()")
    .execute(&database.pool).await.unwrap();
  assert!(
    store
      .publish_integration_project_access(management_mutation(request.clone()))
      .await
      .is_err()
  );
  assert!(
    store
      .integration_project_access(connection_id, project_id)
      .await
      .unwrap()
      .is_none()
  );
  let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM integration_project_access_versions), (SELECT count(*) FROM audit_facts WHERE operation = 'publish-integration-project-access'), (SELECT count(*) FROM idempotency_records WHERE scope = 'publish-integration-project-access')").fetch_one(&database.pool).await.unwrap();
  assert_eq!(counts, (0, 0, 0));
  sqlx::query("DROP TRIGGER reject_grant_event ON outbox_entries")
    .execute(&database.pool)
    .await
    .unwrap();
  let (left, right) = tokio::join!(
    store.publish_integration_project_access(management_mutation(request.clone())),
    store.publish_integration_project_access(management_mutation(request.clone()))
  );
  let (left, right) = (left.unwrap(), right.unwrap());
  assert_eq!(left.access, right.access);
  assert_eq!(
    [left.disposition, right.disposition]
      .into_iter()
      .filter(|d| *d == MutationDisposition::Applied)
      .count(),
    1
  );
  request.intent.expected_current_version = Some(left.access.version);
  request.idempotency_key = IdempotencyKey::new("keep-read").unwrap();
  let mut revoke = request.clone();
  revoke.intent.resources.clear();
  revoke.idempotency_key = IdempotencyKey::new("revoke").unwrap();
  let (left, right) = tokio::join!(
    store.publish_integration_project_access(management_mutation(request)),
    store.publish_integration_project_access(management_mutation(revoke))
  );
  assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
  let winner = left.or(right).unwrap().access;
  assert_eq!(winner.version.get(), 2);
  assert_eq!(
    PostgresStore::new(database.pool.clone())
      .integration_project_access(connection_id, project_id)
      .await
      .unwrap(),
    Some(winner)
  );
  let counts: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM integration_project_access_versions), (SELECT count(*) FROM audit_facts WHERE operation = 'publish-integration-project-access'), (SELECT count(*) FROM idempotency_records WHERE scope = 'publish-integration-project-access'), (SELECT count(*) FROM outbox_entries WHERE topic = 'integration.project-access.published')").fetch_one(&database.pool).await.unwrap();
  assert_eq!(counts, (2, 2, 2, 2));
  database.cleanup().await;
}
