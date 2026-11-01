//! Frozen repository-backed row shapes for migration tests, before migration 0109.
//! Current store code deliberately requires its current schema; it must not be
//! used as the old writer while constructing a pre-upgrade database.
use octacity_server_factory::{BudgetUsage, FactoryDigest, FactoryLifecycleProgress, FactoryRunVersion};
use octacity_server_store::{
  AdmitFactoryWork, FactoryBudgetRecord, FactoryLifecycleCheckpoint, PublishedFactoryAdmission,
};
use sqlx::{PgPool, types::Json};

pub async fn seed(pool: &PgPool, request: AdmitFactoryWork) -> PublishedFactoryAdmission {
  let work = &request.work;
  let subject = work.subject().repository().expect("historical repository subject");
  assert!(request.flow.code_pins().is_empty());
  let security = FactoryDigest::sha256(
    "octacity.factory.security-scope.v1",
    &[request.probe.source_scope.security_scope.as_str().as_bytes()],
  );
  let envelope = serde_json::to_vec(work).unwrap();
  let envelope_digest = FactoryDigest::sha256("octacity.factory.work-envelope.v1", &[&envelope]);
  let mut transaction = pool.begin().await.unwrap();
  sqlx::query("INSERT INTO factory_work_envelopes \
    (id,project_id,factory_configuration_id,factory_configuration_version,source_kind,security_scope_digest,external_identity,repository_id,repository_version,exact_revision,intent_digest,envelope_digest,envelope,admitted_at) \
    VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,to_timestamp($14::double precision/1000.0))")
    .bind(work.id().as_uuid()).bind(subject.project_id().as_uuid()).bind(work.configuration().id().as_uuid())
    .bind(i64::try_from(work.configuration().version().get()).unwrap()).bind(request.probe.source_scope.source.as_str())
    .bind(security.as_bytes().as_slice()).bind(work.external_identity().as_str()).bind(subject.repository_id().as_uuid())
    .bind(i64::try_from(request.repository_version.unwrap().get()).unwrap()).bind(subject.base_revision().as_str())
    .bind(request.probe.intent_digest.as_bytes().as_slice()).bind(envelope_digest.as_bytes().as_slice()).bind(Json(work))
    .bind(request.admitted_at.unix_millis()).execute(&mut *transaction).await.unwrap();
  let run = &request.run;
  sqlx::query("INSERT INTO factory_runs \
    (id,project_id,work_envelope_id,factory_configuration_id,factory_configuration_version,state,version,subject_digest,flow_admission_limits,admitted_at,updated_at) \
    VALUES ($1,$2,$3,$4,$5,'admitted',1,$6,$7,to_timestamp($8::double precision/1000.0),to_timestamp($8::double precision/1000.0))")
    .bind(run.id().as_uuid()).bind(subject.project_id().as_uuid()).bind(work.id().as_uuid()).bind(work.configuration().id().as_uuid())
    .bind(i64::try_from(work.configuration().version().get()).unwrap()).bind(subject.digest().as_bytes().as_slice())
    .bind(Json(request.flow.limits())).bind(request.admitted_at.unix_millis()).execute(&mut *transaction).await.unwrap();
  let root = request.flow.root_run();
  sqlx::query("INSERT INTO factory_flow_runs (id,factory_run_id,flow_definition_id,flow_definition_version,parent_flow_run_id,parent_node_attempt_id,created_at) \
    VALUES ($1,$2,$3,$4,NULL,NULL,to_timestamp($5::double precision/1000.0))")
    .bind(root.id().as_uuid()).bind(run.id().as_uuid()).bind(root.definition().id().as_uuid())
    .bind(i64::try_from(root.definition().version().get()).unwrap()).bind(request.admitted_at.unix_millis())
    .execute(&mut *transaction).await.unwrap();
  let cycle = request.flow.initial_cycle();
  sqlx::query(
    "INSERT INTO factory_workflow_cycles (id,factory_run_id,flow_run_id,cycle_number,predecessor_id,created_at) \
    VALUES ($1,$2,$3,1,NULL,to_timestamp($4::double precision/1000.0))",
  )
  .bind(cycle.id().as_uuid())
  .bind(run.id().as_uuid())
  .bind(root.id().as_uuid())
  .bind(request.admitted_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .unwrap();
  let budget = FactoryBudgetRecord::new(
    run.id(),
    FactoryRunVersion::INITIAL,
    BudgetUsage::default(),
    request.admitted_at,
  );
  sqlx::query("INSERT INTO factory_run_budgets (id,run_id,run_version,usage,recorded_at) VALUES ($1,$2,1,$3,to_timestamp($4::double precision/1000.0))")
    .bind(budget.id.as_bytes().as_slice()).bind(run.id().as_uuid()).bind(Json(budget.usage)).bind(request.admitted_at.unix_millis())
    .execute(&mut *transaction).await.unwrap();
  let lifecycle = FactoryLifecycleCheckpoint::new(
    run.id(),
    FactoryRunVersion::INITIAL,
    FactoryLifecycleProgress::Admitted,
    false,
    request.admitted_at,
  );
  sqlx::query("INSERT INTO factory_lifecycle_checkpoints (id,run_id,run_version,lifecycle,cancellation_requested,recorded_at) VALUES ($1,$2,1,$3,false,to_timestamp($4::double precision/1000.0))")
    .bind(lifecycle.id.as_bytes().as_slice()).bind(run.id().as_uuid()).bind(Json(serde_json::json!({"progress":lifecycle.progress})))
    .bind(request.admitted_at.unix_millis()).execute(&mut *transaction).await.unwrap();
  sqlx::query(
    "INSERT INTO factory_run_current (run_id,run_version,budget_id,lifecycle_checkpoint_id) VALUES ($1,1,$2,$3)",
  )
  .bind(run.id().as_uuid())
  .bind(budget.id.as_bytes().as_slice())
  .bind(lifecycle.id.as_bytes().as_slice())
  .execute(&mut *transaction)
  .await
  .unwrap();
  transaction.commit().await.unwrap();
  PublishedFactoryAdmission {
    work: request.work,
    run: request.run,
    flow: request.flow,
    admitted_at: request.admitted_at,
  }
}
