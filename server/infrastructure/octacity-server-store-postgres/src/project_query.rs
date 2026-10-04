use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_store::{ListProjects, Project, ProjectDetails, ProjectPage, StoreError};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{database::unavailable, project_row::ProjectRow, read_visibility::sql_read_visibility};

#[derive(FromRow)]
struct ProjectNavigationRow {
  id: Uuid,
  parent_id: Option<Uuid>,
  name: String,
  version: i64,
  created_at_millis: i64,
  updated_at_millis: i64,
  has_visible_children: bool,
}

impl ProjectNavigationRow {
  fn into_project(self) -> Result<(Project, bool), StoreError> {
    let has_visible_children = self.has_visible_children;
    let project = Project::try_from(ProjectRow {
      id: self.id,
      parent_id: self.parent_id,
      name: self.name,
      version: self.version,
      created_at_millis: self.created_at_millis,
      updated_at_millis: self.updated_at_millis,
    })?;
    Ok((project, has_visible_children))
  }
}

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
  if request.visibility().kind() == octacity_server_store::ReadVisibilityKind::None {
    return Ok(ProjectPage {
      projects: Vec::new(),
      projects_with_visible_children: Default::default(),
      next_cursor: None,
    });
  }
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
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
  let mut rows = sqlx::query_as::<_, ProjectNavigationRow>(
    "SELECT project.id, project.parent_id, project.name, project.version, \
       FLOOR(EXTRACT(EPOCH FROM project.created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM project.updated_at) * 1000)::BIGINT AS updated_at_millis, \
       EXISTS (SELECT 1 FROM projects AS child \
         WHERE child.parent_id = project.id AND ($2 OR child.id = ANY($3::uuid[]))) AS has_visible_children \
     FROM projects AS project \
     WHERE project.parent_id IS NOT DISTINCT FROM $1 \
       AND ($2 OR project.id = ANY($3::uuid[])) \
       AND ($4::UUID IS NULL OR project.id > $4) \
     ORDER BY project.id LIMIT $5",
  )
  .bind(request.parent_id().map(ProjectId::as_uuid))
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(request.after().map(ProjectId::as_uuid))
  .bind(fetch_limit)
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(ProjectNavigationRow::into_project)
  .collect::<Result<Vec<_>, StoreError>>()?;

  transaction.commit().await.map_err(unavailable)?;
  let limit = usize::from(request.limit().get());
  let has_more = rows.len() > limit;
  rows.truncate(limit);
  let next_cursor = has_more.then(|| rows.last().expect("a non-zero full page has a last item").0.id);
  let projects_with_visible_children = rows
    .iter()
    .filter_map(|(project, has_children)| has_children.then_some(project.id))
    .collect();
  let projects = rows.into_iter().map(|(project, _)| project).collect();
  Ok(ProjectPage {
    projects,
    projects_with_visible_children,
    next_cursor,
  })
}
