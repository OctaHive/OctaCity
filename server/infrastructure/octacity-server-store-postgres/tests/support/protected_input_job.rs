//! A real ordinary consuming Job for publication/transfer contracts, without Factory causality.
use octacity_server_domain::*;
use octacity_server_job::JobSpecTemplate;
use octacity_server_store::TriggerAcceptanceStore;
use octacity_server_store_postgres::PostgresAuthoritativeStore;

#[path = "authoritative_fixture.rs"]
mod prerequisites;

pub async fn accept(
  pool: &sqlx::PgPool,
  project: ProjectId,
  inputs: &[octacity_protocol::ProtectedInputV3],
) -> (JobId, JobSpecTemplate) {
  let mut fixture = octacity_server_store::testing::source_free_store_contract_fixture();
  fixture.allowed_pool = PoolId::generate();
  fixture.other_pool = PoolId::generate();
  fixture.disabled_pool = PoolId::generate();
  fixture.draining_pool = PoolId::generate();
  fixture.request.build.project_id = project;
  fixture.request.build.configuration_id = BuildConfigurationId::generate();
  fixture.request.build.pipeline_id = PipelineId::generate();
  fixture.request.trigger.trigger.id = TriggerId::generate();
  fixture.request.trigger.target.configuration_id = fixture.request.build.configuration_id;
  for job in &mut fixture.request.jobs {
    job.allowed_pools = vec![fixture.allowed_pool];
    let mut template = serde_json::to_value(&job.job_spec_template).unwrap();
    let selected = template["execution"]["protected_inputs"]["inputs"]
      .as_array_mut()
      .unwrap();
    selected.extend(inputs.iter().map(|input| serde_json::to_value(input).unwrap()));
    selected.sort_by(|a, b| a["artifact_id"].as_str().cmp(&b["artifact_id"].as_str()));
    job.job_spec_template = serde_json::from_value(template).unwrap();
  }
  fixture.request.validate().unwrap();
  prerequisites::seed_authoritative_prerequisites(pool, &fixture)
    .await
    .unwrap();
  let job = &fixture.request.jobs[0];
  let result = (job.id, job.job_spec_template.clone());
  PostgresAuthoritativeStore::new(pool.clone(), super::test_signer())
    .accept_trigger(fixture.request)
    .await
    .unwrap();
  result
}
