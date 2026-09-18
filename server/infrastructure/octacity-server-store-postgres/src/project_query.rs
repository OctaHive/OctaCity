use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_store::{ListProjects, Project, ProjectDetails, ProjectPage, StoreError};
use sqlx::PgPool;

use crate::{database::unavailable, project_row::ProjectRow};

pub(crate) async fn read(pool: &PgPool, project_id: ProjectId) -> Result<ProjectDetails, StoreError> {
  let mut lineage = sqlx::query_as::<_, ProjectRow>(
    "WITH RECURSIVE lineage AS ( \
       SELECT id, parent_id, name, version, created_at, updated_at, 0 AS depth \
       FROM projects WHERE id = $1 \
       UNION ALL \
       SELECT parent.id, parent.parent_id, parent.name, parent.version, parent.created_at, parent.updated_at, \
              child.depth + 1 \
       FROM lineage AS child \
       JOIN projects AS parent ON parent.id = child.parent_id \
     ) \
     SELECT id, parent_id, name, version, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM lineage ORDER BY depth DESC",
  )
  .bind(project_id.as_uuid())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(Project::try_from)
  .collect::<Result<Vec<_>, StoreError>>()?;
  let project = lineage.pop().ok_or(StoreError::NotFound {
    entity: EntityKind::Project,
  })?;
  let ancestors = lineage;
  Ok(ProjectDetails { project, ancestors })
}

pub(crate) async fn list(pool: &PgPool, request: ListProjects) -> Result<ProjectPage, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  if let Some(parent_id) = request.parent_id() {
    let exists = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id = $1 FOR KEY SHARE")
      .bind(parent_id.as_uuid())
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?
      .is_some();
    if !exists {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
  }

  let fetch_limit = i64::from(request.limit().get()) + 1;
  let mut projects = sqlx::query_as::<_, ProjectRow>(
    "SELECT id, parent_id, name, version, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM projects \
     WHERE parent_id IS NOT DISTINCT FROM $1 AND ($2::UUID IS NULL OR id > $2) \
     ORDER BY id LIMIT $3",
  )
  .bind(request.parent_id().map(ProjectId::as_uuid))
  .bind(request.after().map(ProjectId::as_uuid))
  .bind(fetch_limit)
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(Project::try_from)
  .collect::<Result<Vec<_>, StoreError>>()?;

  transaction.commit().await.map_err(unavailable)?;
  let limit = usize::from(request.limit().get());
  let has_more = projects.len() > limit;
  projects.truncate(limit);
  let next_cursor = has_more.then(|| projects.last().expect("a non-zero full page has a last item").id);
  Ok(ProjectPage { projects, next_cursor })
}
