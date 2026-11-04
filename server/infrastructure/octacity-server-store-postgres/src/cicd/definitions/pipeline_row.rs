use octacity_server_domain::{PipelineId, PipelineName, PipelineVersion, ProjectId, Timestamp};
use octacity_server_store::{PublishedPipeline, StoreError};
use serde_json::Value;
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct PipelineRow {
  pub(crate) id: Uuid,
  pub(crate) project_id: Uuid,
  pub(crate) name: String,
  pub(crate) version: i64,
  pub(crate) dag_snapshot: Json<Value>,
  pub(crate) published_at_millis: i64,
}

impl TryFrom<PipelineRow> for PublishedPipeline {
  type Error = StoreError;

  fn try_from(row: PipelineRow) -> Result<Self, Self::Error> {
    Ok(Self {
      id: PipelineId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
      name: PipelineName::new(row.name).map_err(|_| StoreError::Unavailable)?,
      version: PipelineVersion::new(u64::try_from(row.version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      dag: serde_json::from_value(row.dag_snapshot.0).map_err(|_| StoreError::Unavailable)?,
      published_at: Timestamp::from_unix_millis(row.published_at_millis).map_err(|_| StoreError::Unavailable)?,
    })
  }
}
