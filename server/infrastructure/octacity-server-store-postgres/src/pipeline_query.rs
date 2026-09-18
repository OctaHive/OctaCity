use octacity_server_domain::{EntityKind, PipelineId, PipelineVersion};
use octacity_server_store::{PublishedPipeline, StoreError, StoreOperation};

use crate::{
  database::{number, unavailable},
  pipeline_row::PipelineRow,
};

pub(crate) async fn read(
  pool: &sqlx::PgPool,
  pipeline_id: PipelineId,
  version: PipelineVersion,
) -> Result<PublishedPipeline, StoreError> {
  let version = number(version.get(), StoreOperation::ReadPipelineVersion)?;
  sqlx::query_as::<_, PipelineRow>(
    "SELECT pipeline.id, pipeline.project_id, pipeline.name, version.version, version.dag_snapshot, \
       FLOOR(EXTRACT(EPOCH FROM version.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM pipelines AS pipeline \
     JOIN pipeline_versions AS version ON version.pipeline_id = pipeline.id \
     WHERE pipeline.id = $1 AND version.version = $2",
  )
  .bind(pipeline_id.as_uuid())
  .bind(version)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Pipeline,
  })?
  .try_into()
}
