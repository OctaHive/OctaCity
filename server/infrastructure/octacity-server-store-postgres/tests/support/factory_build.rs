use octacity_server_domain::{AttemptId, BuildConfigurationId, BuildId, JobId, RepositoryVersion};
pub async fn seed_factory_build(
  pool: &sqlx::PgPool,
  subject: &octacity_server_factory::ExactSubject,
  repository_version: RepositoryVersion,
  build_configuration_id: BuildConfigurationId,
  build_id: BuildId,
  attempt_id: AttemptId,
  job_id: JobId,
) {
  let pipeline_id = uuid::Uuid::new_v4();
  let trigger_id = uuid::Uuid::new_v4();
  let occurrence_id = uuid::Uuid::new_v4();
  let pool_id = uuid::Uuid::new_v4();
  let mut transaction = pool.begin().await.unwrap();

  sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, $3, now())")
    .bind(pipeline_id)
    .bind(subject.project_id().as_uuid())
    .bind(format!("factory-linked-{build_id}"))
    .execute(&mut *transaction)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
         VALUES ($1, 1, '{}', now())",
  )
  .bind(pipeline_id)
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query("INSERT INTO build_configurations (id, project_id, name, created_at) VALUES ($1, $2, $3, now())")
    .bind(build_configuration_id.as_uuid())
    .bind(subject.project_id().as_uuid())
    .bind(format!("factory-linked-{build_id}"))
    .execute(&mut *transaction)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO build_configuration_versions \
           (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, \
            pipeline_version, configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, \
            retries_infrastructure, published_at) \
         VALUES ($1, 1, true, $2, $3, $4, 1, '{}', 1, ARRAY[$5::uuid], 1, false, now())",
  )
  .bind(build_configuration_id.as_uuid())
  .bind(subject.repository_id().as_uuid())
  .bind(i64::try_from(repository_version.get()).unwrap())
  .bind(pipeline_id)
  .bind(pool_id)
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO triggers \
           (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
         VALUES ($1, 1, $2, 1, 'manual', true, '{}', now())",
  )
  .bind(trigger_id)
  .bind(build_configuration_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO trigger_occurrences \
           (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
            deduplication_identity, cause, causality, provider_metadata, source_time, state, request_digest, \
            created_at, updated_at) \
         VALUES ($1, $2, 1, $3, 1, 'manual', $4, '{\"kind\":\"manual\"}', \
                 jsonb_build_object('root_occurrence_id', $1::text, 'parent_occurrence_id', NULL, 'depth', 0), '{}', \
                 now(), 'accepted', decode(repeat('02', 32), 'hex'), now(), now())",
  )
  .bind(occurrence_id)
  .bind(trigger_id)
  .bind(build_configuration_id.as_uuid())
  .bind(format!("factory-linked-{build_id}"))
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
        "INSERT INTO builds \
           (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
            repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
            effective_policy_snapshot, project_job_concurrency_limit, priority, state, version, created_at, updated_at, \
            metadata_retention_until, log_retention_until, artifact_retention_until, report_retention_until) \
         VALUES ($1, $2, $3, 1, $4, 1, $5, $6, $7, $8, '{}', '{}', 1, 0, 'running', 1, now(), now(), \
                 now() + interval '100 years', now() + interval '100 years', now() + interval '100 years', \
                 now() + interval '100 years')",
      )
      .bind(build_id.as_uuid())
      .bind(subject.project_id().as_uuid())
      .bind(build_configuration_id.as_uuid())
      .bind(pipeline_id)
      .bind(subject.repository_id().as_uuid())
      .bind(i64::try_from(repository_version.get()).unwrap())
      .bind(occurrence_id)
      .bind(subject.base_revision().as_str())
      .execute(&mut *transaction)
      .await
      .unwrap();
  sqlx::query(
    "INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at) \
         VALUES ($1, $2, 1, 'running', 1, now(), now())",
  )
  .bind(attempt_id.as_uuid())
  .bind(build_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO jobs \
           (id, attempt_id, pipeline_node_id, state, allowed_pool_ids, requirements, job_spec_template, \
            dependency_policy, signed_job_spec, version, created_at, updated_at) \
         VALUES ($1, $2, 'factory-linked', 'ready', ARRAY[$3]::uuid[], '{}', '{}', \
                 '\"all_succeeded\"', '{}', 1, now(), now())",
  )
  .bind(job_id.as_uuid())
  .bind(attempt_id.as_uuid())
  .bind(pool_id)
  .execute(&mut *transaction)
  .await
  .unwrap();
  transaction.commit().await.unwrap();
}
