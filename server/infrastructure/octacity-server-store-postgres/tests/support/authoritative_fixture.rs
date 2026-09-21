use octacity_server_store::testing::StoreContractFixture;
use sqlx::PgPool;

pub async fn seed_authoritative_prerequisites(
  pool: &PgPool,
  fixture: &StoreContractFixture,
) -> Result<(), sqlx::Error> {
  let trigger = &fixture.request.trigger;
  let build = &fixture.request.build;
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'contract-project', 1, now(), now())",
  )
  .bind(build.project_id.as_uuid())
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO repositories (id, project_id, name, created_at) VALUES ($1, $2, 'contract-repository', now())",
  )
  .bind(build.repository_id.as_uuid())
  .bind(build.project_id.as_uuid())
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, $2, $3, 'contract/repository', '{}', now())",
  )
  .bind(build.repository_id.as_uuid())
  .bind(i64::try_from(build.repository_version.get()).unwrap())
  .bind(uuid::Uuid::from_u128(600))
  .execute(pool)
  .await?;
  sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, 'contract-pipeline', now())")
    .bind(build.pipeline_id.as_uuid())
    .bind(build.project_id.as_uuid())
    .execute(pool)
    .await?;
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) VALUES ($1, $2, '{}', now())",
  )
  .bind(build.pipeline_id.as_uuid())
  .bind(i64::try_from(build.pipeline_version.get()).unwrap())
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO build_configurations (id, project_id, name, created_at) \
     VALUES ($1, $2, 'contract-configuration', now())",
  )
  .bind(build.configuration_id.as_uuid())
  .bind(build.project_id.as_uuid())
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO build_configuration_versions \
       (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, \
        pipeline_version, configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, \
        retries_infrastructure, published_at) \
     VALUES ($1, $2, true, $3, $4, $5, $6, $7, 4, $8, 1, false, now())",
  )
  .bind(build.configuration_id.as_uuid())
  .bind(i64::try_from(build.configuration_version.get()).unwrap())
  .bind(build.repository_id.as_uuid())
  .bind(i64::try_from(build.repository_version.get()).unwrap())
  .bind(build.pipeline_id.as_uuid())
  .bind(i64::try_from(build.pipeline_version.get()).unwrap())
  .bind(sqlx::types::Json(serde_json::json!({"job_concurrency_limit": 4})))
  .bind(vec![fixture.allowed_pool.as_uuid()])
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, $4, 'manual', true, '{}', now())",
  )
  .bind(trigger.trigger.id.as_uuid())
  .bind(i64::try_from(trigger.trigger.version.get()).unwrap())
  .bind(build.configuration_id.as_uuid())
  .bind(i64::try_from(build.configuration_version.get()).unwrap())
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO pools \
       (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
     VALUES \
       ($1, 1, 'contract-allowed', true, 'accepting', '{\"mode\":\"any\"}', 4, now()), \
       ($2, 1, 'contract-other', true, 'accepting', '{\"mode\":\"any\"}', 4, now()), \
       ($3, 1, 'contract-disabled', false, 'accepting', '{\"mode\":\"any\"}', 4, now()), \
       ($4, 1, 'contract-draining', true, 'graceful_drain', '{\"mode\":\"any\"}', 4, now())",
  )
  .bind(fixture.allowed_pool.as_uuid())
  .bind(fixture.other_pool.as_uuid())
  .bind(fixture.disabled_pool.as_uuid())
  .bind(fixture.draining_pool.as_uuid())
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO agents \
       (id, name, pool_id, pool_version, state, inventory, version, created_at, updated_at) \
     VALUES \
       ($1, 'contract-agent', $2, 1, 'online', $11, 1, now(), now()), \
       ($3, 'contract-other-agent', $4, 1, 'online', $11, 1, now(), now()), \
       ($5, 'contract-disabled-agent', $6, 1, 'online', $11, 1, now(), now()), \
       ($7, 'contract-draining-agent', $8, 1, 'online', $11, 1, now(), now()), \
       ($9, 'contract-expired-agent', $2, 1, 'online', $11, 1, now(), now()), \
       ($10, 'contract-revoked-agent', $2, 1, 'online', $11, 1, now(), now())",
  )
  .bind(fixture.agent_id.as_uuid())
  .bind(fixture.allowed_pool.as_uuid())
  .bind(fixture.other_agent_id.as_uuid())
  .bind(fixture.other_pool.as_uuid())
  .bind(fixture.disabled_agent_id.as_uuid())
  .bind(fixture.disabled_pool.as_uuid())
  .bind(fixture.draining_agent_id.as_uuid())
  .bind(fixture.draining_pool.as_uuid())
  .bind(fixture.expired_agent_id.as_uuid())
  .bind(fixture.revoked_agent_id.as_uuid())
  .bind(sqlx::types::Json(octacity_server_store::testing::compatible_inventory(
    fixture.agent_id,
  )))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO agent_registrations \
       (id, agent_id, epoch, credential_hash, inventory, registered_at, expires_at, revoked_at) \
     VALUES \
       ($1, $2, $3, decode(repeat('01', 32), 'hex'), $14, to_timestamp(0), '9999-12-31 23:59:59+00', NULL), \
       ($4, $5, $3, decode(repeat('02', 32), 'hex'), $14, to_timestamp(0), '9999-12-31 23:59:59+00', NULL), \
       ($6, $7, $3, decode(repeat('03', 32), 'hex'), $14, to_timestamp(0), '9999-12-31 23:59:59+00', NULL), \
       ($8, $9, $3, decode(repeat('04', 32), 'hex'), $14, to_timestamp(0), '9999-12-31 23:59:59+00', NULL), \
       ($10, $11, $3, decode(repeat('05', 32), 'hex'), $14, to_timestamp(0), to_timestamp(1), NULL), \
       ($12, $13, $3, decode(repeat('06', 32), 'hex'), $14, to_timestamp(0), '9999-12-31 23:59:59+00', to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(601))
  .bind(fixture.agent_id.as_uuid())
  .bind(i64::try_from(fixture.registration_epoch.get()).unwrap())
  .bind(uuid::Uuid::from_u128(602))
  .bind(fixture.other_agent_id.as_uuid())
  .bind(uuid::Uuid::from_u128(603))
  .bind(fixture.disabled_agent_id.as_uuid())
  .bind(uuid::Uuid::from_u128(604))
  .bind(fixture.draining_agent_id.as_uuid())
  .bind(uuid::Uuid::from_u128(605))
  .bind(fixture.expired_agent_id.as_uuid())
  .bind(uuid::Uuid::from_u128(606))
  .bind(fixture.revoked_agent_id.as_uuid())
  .bind(sqlx::types::Json(octacity_server_store::testing::compatible_inventory(
    fixture.agent_id,
  )))
  .execute(pool)
  .await?;
  Ok(())
}
