use octacity_server_domain::{EntityKind, ProjectId, ProjectPolicyVersion};
use octacity_server_store::{ProjectPolicyDocument, StoreError};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::database::unavailable;

pub(crate) async fn lineage(pool: &PgPool, project_id: ProjectId) -> Result<Vec<ProjectPolicyDocument>, StoreError> {
  let rows: Vec<PolicyRow> = sqlx::query_as(
    "WITH RECURSIVE lineage AS (\
       SELECT id, parent_id, 0::bigint AS depth FROM projects WHERE id = $1 \
       UNION ALL \
       SELECT parent.id, parent.parent_id, lineage.depth + 1 \
       FROM projects AS parent JOIN lineage ON parent.id = lineage.parent_id\
     ) \
     SELECT lineage.id AS project_id, lineage.parent_id, policy.version, policy.policy \
     FROM lineage \
     LEFT JOIN LATERAL (\
       SELECT version, policy FROM project_policy_versions \
       WHERE project_id = lineage.id ORDER BY version DESC LIMIT 1\
     ) AS policy ON TRUE \
     ORDER BY lineage.depth DESC",
  )
  .bind(project_id.as_uuid())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  if rows.is_empty() {
    return Err(StoreError::NotFound {
      entity: EntityKind::Project,
    });
  }
  rows.into_iter().map(PolicyRow::into_document).collect()
}

#[derive(FromRow)]
struct PolicyRow {
  project_id: Uuid,
  parent_id: Option<Uuid>,
  version: Option<i64>,
  policy: Option<Json<Value>>,
}

impl PolicyRow {
  fn into_document(self) -> Result<ProjectPolicyDocument, StoreError> {
    let version = self
      .version
      .and_then(|value| u64::try_from(value).ok())
      .and_then(|value| ProjectPolicyVersion::new(value).ok())
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Project,
      })?;
    Ok(ProjectPolicyDocument {
      project_id: ProjectId::from_uuid(self.project_id).map_err(|_| StoreError::Unavailable)?,
      parent_id: self
        .parent_id
        .map(ProjectId::from_uuid)
        .transpose()
        .map_err(|_| StoreError::Unavailable)?,
      version,
      policy: self.policy.ok_or(StoreError::Unavailable)?.0,
    })
  }
}
