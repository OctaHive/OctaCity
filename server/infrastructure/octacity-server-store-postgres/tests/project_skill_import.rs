mod support;
use octacity_server_domain::ProjectId;
use octacity_server_store::{
  testing::{management_mutation, skill_import_contract_request, verify_project_skill_import_publication},
  *,
};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn project_skill_import_is_atomic_durable_and_uses_existing_transfer_and_cleanup() {
  let db = TestDatabase::migrated().await;
  let project = ProjectId::generate();
  sqlx::query("INSERT INTO projects (id,name,version,created_at,updated_at) VALUES ($1,'skill owner',1,to_timestamp(0),to_timestamp(0))").bind(project.as_uuid()).execute(&db.pool).await.unwrap();
  let store = PostgresStore::new(db.pool.clone());
  let original_contract = verify_project_skill_import_publication(
    &store,
    &support::evidence::PostgresEvidenceProbe(db.pool.clone()),
    project,
  )
  .await;
  let mut request = skill_import_contract_request(project);
  request.intent.idempotency_key = "transfer".parse().unwrap();
  let pending = store
    .reserve_project_skill_import(management_mutation(request.clone()))
    .await
    .unwrap();
  assert!(pending.published_at.is_none());
  assert!(
    store
      .project_skill_import(project, request.intent.bundle.reference())
      .await
      .unwrap()
      .is_none()
  );
  let artifact = *request.intent.uploads.keys().next().unwrap();
  assert!(store.published_execution_input(artifact).await.is_err());
  let original = store
    .publish_project_skill_import(management_mutation(request.clone()))
    .await
    .unwrap();
  let restarted = PostgresStore::new(db.pool.clone());
  assert_eq!(
    restarted
      .reserve_project_skill_import(management_mutation(request.clone()))
      .await
      .unwrap(),
    original
  );
  assert_eq!(
    restarted
      .project_skill_import(project, request.intent.bundle.reference())
      .await
      .unwrap(),
    Some(original.clone())
  );
  assert!(
    restarted
      .project_skill_import(ProjectId::generate(), request.intent.bundle.reference())
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(
    restarted.published_execution_input(artifact).await.unwrap(),
    original.inputs().unwrap()[0]
  );
  let mut conflict = request.clone();
  conflict.intent.bundle = skill_import_contract_request(project).intent.bundle;
  assert!(
    restarted
      .reserve_project_skill_import(management_mutation(conflict))
      .await
      .is_err()
  );
  let mut abandoned = skill_import_contract_request(project);
  abandoned.intent.idempotency_key = "abandoned".parse().unwrap();
  let staged = store
    .reserve_project_skill_import(management_mutation(abandoned.clone()))
    .await
    .unwrap();
  let expired = store
    .expired_factory_execution_inputs(staged.staging_until, 100)
    .await
    .unwrap();
  assert_eq!(expired, staged.inputs().unwrap());
  // A publisher can have sampled time before waiting on a transaction lock.
  // Cleanup claiming this import must permanently fence that stale publication.
  assert!(
    store
      .publish_project_skill_import(management_mutation(abandoned.clone()))
      .await
      .is_err()
  );
  abandoned.now = staged.staging_until;
  assert!(
    store
      .publish_project_skill_import(management_mutation(abandoned))
      .await
      .is_err()
  );
  store
    .complete_factory_execution_input_deletion(CompleteFactoryExecutionInputDeletion {
      expected: expired[0].clone(),
      deleted_at: staged.staging_until,
    })
    .await
    .unwrap();
  assert!(
    store
      .expired_factory_execution_inputs(staged.staging_until, 100)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(original_contract.published_at.is_some());
  // Published profile content must not be deleted as abandoned staging.
  assert!(
    store
      .project_skill_import(project, original.intent.bundle.reference())
      .await
      .unwrap()
      .is_some()
  );
  db.cleanup().await;
}
