//! Download authority comes from the consuming Job and the existing publication owner.

use octacity_server_domain::{ArtifactId, EntityKind, JobId};
use octacity_server_store::StoreError;
use sqlx::PgPool;

pub(crate) async fn require_for_job(pool: &PgPool, job: JobId, artifact: ArtifactId) -> Result<(), StoreError> {
  // Private invocation inputs take precedence over Project publications: knowing an
  // exact identifier, or sharing a Project, cannot expose another step's package.
  let allowed: bool = sqlx::query_scalar(
    "SELECT EXISTS (
       SELECT 1 FROM jobs job
       JOIN attempts attempt ON attempt.id=job.attempt_id
       JOIN builds build ON build.id=attempt.build_id
       WHERE job.id=$1 AND build.metadata_visible
       AND EXISTS (
         SELECT 1 FROM jsonb_array_elements(
           COALESCE(job.job_spec_template#>'{execution,protected_inputs,inputs}','[]'::jsonb)
         ) selected WHERE selected->>'artifact_id'=$2::uuid::text
       )
       AND CASE WHEN EXISTS (SELECT 1 FROM factory_execution_inputs WHERE artifact_id=$2)
       THEN EXISTS (
         SELECT 1 FROM factory_execution_inputs input
         JOIN factory_execution_input_manifests manifest USING(operation_id)
         WHERE input.artifact_id=$2
         AND encode(input.operation_id,'hex')=build.factory_origin->>'operation_id'
         AND manifest.run_id::text=build.factory_origin->>'run_id'
       ) ELSE
         EXISTS (SELECT 1 FROM project_text_input_objects input
           JOIN project_text_preparations owner ON owner.id=input.preparation_id
           WHERE input.artifact_id=$2 AND owner.project_id=build.project_id)
         OR EXISTS (SELECT 1 FROM project_skill_input_objects input
           JOIN project_skill_imports owner ON owner.id=input.import_id
           WHERE input.artifact_id=$2 AND owner.project_id=build.project_id)
         OR EXISTS (SELECT 1 FROM factory_source_input_objects input
           JOIN factory_source_materializations owner ON owner.preparation_id=input.preparation_id
           WHERE input.artifact_id=$2 AND owner.project_id=build.project_id)
         OR EXISTS (SELECT 1 FROM artifacts input
           JOIN builds owner ON owner.id=input.build_id
           WHERE input.id=$2 AND owner.project_id=build.project_id)
       END
     )",
  )
  .bind(job.as_uuid())
  .bind(artifact.as_uuid())
  .fetch_one(pool)
  .await
  .map_err(crate::database::unavailable)?;
  if allowed {
    Ok(())
  } else {
    Err(StoreError::NotFound {
      entity: EntityKind::Artifact,
    })
  }
}
