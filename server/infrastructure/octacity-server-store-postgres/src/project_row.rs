use octacity_server_domain::{ProjectId, ProjectName, ProjectVersion, Timestamp};
use octacity_server_store::{Project, StoreError};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct ProjectRow {
  pub(crate) id: Uuid,
  pub(crate) parent_id: Option<Uuid>,
  pub(crate) name: String,
  pub(crate) version: i64,
  pub(crate) created_at_millis: i64,
  pub(crate) updated_at_millis: i64,
}

impl TryFrom<ProjectRow> for Project {
  type Error = StoreError;

  fn try_from(row: ProjectRow) -> Result<Self, Self::Error> {
    Ok(Self {
      id: ProjectId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      parent_id: row
        .parent_id
        .map(ProjectId::from_uuid)
        .transpose()
        .map_err(|_| StoreError::Unavailable)?,
      name: ProjectName::new(row.name).map_err(|_| StoreError::Unavailable)?,
      version: ProjectVersion::new(u64::try_from(row.version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      created_at: Timestamp::from_unix_millis(row.created_at_millis).map_err(|_| StoreError::Unavailable)?,
      updated_at: Timestamp::from_unix_millis(row.updated_at_millis).map_err(|_| StoreError::Unavailable)?,
    })
  }
}
