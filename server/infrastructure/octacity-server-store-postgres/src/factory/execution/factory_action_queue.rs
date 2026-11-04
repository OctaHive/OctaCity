//! Current source/visibility join; collection counts and pages share one observation.
use crate::{PostgresStore, database::unavailable, read_visibility::sql_read_visibility};
use octacity_server_domain::EntityKind;
use octacity_server_store::{
  FactoryActionProjectionCoverage, FactoryActionQueueItem, FactoryActionQueuePage, FactoryActionQueueStore,
  ReadFactoryActionQueue, StoreError,
};
use sqlx::{Postgres, QueryBuilder, Row, types::Json};

// Every query uses the same parameterized predicates, including coverage of
// currently unindexed Runs. A hidden Issue is never treated as unlinked Work.
fn observation(request: &ReadFactoryActionQueue) -> QueryBuilder<Postgres> {
  let runs = sql_read_visibility(request.runs.view(), |id| id.as_uuid());
  let issues = sql_read_visibility(request.issues.view(), |id| id.as_uuid());
  let projects = sql_read_visibility(request.projects.view(), |id| id.as_uuid());
  let mut sql = QueryBuilder::new(
    "WITH visible AS (SELECT r.id,r.version,w.project_id,a.document AS association,COALESCE(p.source_version=r.version AND p.observed_at_millis<=",
  );
  sql
    .push_bind(request.observed_at.unix_millis())
    .push(",FALSE) AS indexed FROM factory_runs r JOIN factory_work_envelopes w ON w.id=r.work_envelope_id LEFT JOIN issue_work_associations a ON a.run_id=r.id LEFT JOIN factory_action_projections p ON p.run_id=r.id WHERE r.visible AND (")
    .push_bind(request.project_id.map(|id| id.as_uuid()))
    .push("::uuid IS NULL OR w.project_id=")
    .push_bind(request.project_id.map(|id| id.as_uuid()))
    .push(") AND (")
    .push_bind(request.issue_id.map(|id| id.as_uuid()))
    .push("::uuid IS NULL OR a.issue_id=")
    .push_bind(request.issue_id.map(|id| id.as_uuid()))
    .push(") AND (")
    .push_bind(runs.all)
    .push(" OR r.id=ANY(")
    .push_bind(runs.identities)
    .push("::uuid[])) AND (")
    .push_bind(projects.all)
    .push(" OR w.project_id=ANY(")
    .push_bind(projects.identities)
    .push("::uuid[])) AND (a.issue_id IS NULL OR ")
    .push_bind(issues.all)
    .push(" OR a.issue_id=ANY(")
    .push_bind(issues.identities)
    .push("::uuid[]))), eligible AS (SELECT v.project_id,v.association,i.run_id,i.node_attempt_id,i.description FROM visible v JOIN factory_action_items i ON i.run_id=v.id WHERE v.indexed AND (i.deadline_millis IS NULL OR i.deadline_millis>=")
    .push_bind(request.observed_at.unix_millis())
    .push(")) ");
  sql
}
#[async_trait::async_trait]
impl FactoryActionQueueStore for PostgresStore {
  async fn read_factory_action_queue(
    &self,
    request: ReadFactoryActionQueue,
  ) -> Result<FactoryActionQueuePage, StoreError> {
    request.validate()?;
    if let Some(issue) = request.issue_id {
      if !request.issues.allows(&issue) {
        return Err(StoreError::NotFound {
          entity: EntityKind::Issue,
        });
      }
      let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM issues WHERE project_id=$1 AND id=$2)")
        .bind(request.project_id.map(|id| id.as_uuid()))
        .bind(issue.as_uuid())
        .fetch_one(self.pool())
        .await
        .map_err(unavailable)?;
      if !exists {
        return Err(StoreError::NotFound {
          entity: EntityKind::Issue,
        });
      }
    }
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let mut coverage_sql = observation(&request);
    coverage_sql.push("SELECT NOT EXISTS(SELECT 1 FROM visible WHERE NOT indexed)");
    let complete: bool = coverage_sql
      .build_query_scalar()
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
    let mut count_sql = observation(&request);
    count_sql.push("SELECT COUNT(*) FROM eligible");
    let total: i64 = count_sql
      .build_query_scalar()
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
    let mut rows_sql = observation(&request);
    rows_sql
      .push("SELECT * FROM eligible WHERE (")
      .push_bind(request.after.map(|p| p.run_id.as_uuid()))
      .push("::uuid IS NULL OR (run_id,node_attempt_id)>(")
      .push_bind(request.after.map(|p| p.run_id.as_uuid()))
      .push(",")
      .push_bind(request.after.map(|p| p.node_attempt_id.as_uuid()))
      .push(")) ORDER BY run_id,node_attempt_id LIMIT ")
      .push_bind(i64::from(request.limit) + 1);
    let rows = rows_sql.build().fetch_all(&mut *tx).await.map_err(unavailable)?;
    let mut items = Vec::new();
    for row in rows {
      let Json(action) = row.try_get("description").map_err(unavailable)?;
      let association: Option<Json<octacity_server_store::IssueWorkAssociation>> =
        row.try_get("association").map_err(unavailable)?;
      let item = FactoryActionQueueItem {
        project_id: octacity_server_domain::ProjectId::from_uuid(row.try_get("project_id").map_err(unavailable)?)
          .map_err(|_| StoreError::Unavailable)?,
        association: association.map(|a| a.0),
        action,
      };
      let run: uuid::Uuid = row.try_get("run_id").map_err(unavailable)?;
      let attempt: uuid::Uuid = row.try_get("node_attempt_id").map_err(unavailable)?;
      if item.action.run_id.as_uuid() != run || item.action.node_attempt_id.as_uuid() != attempt {
        return Err(StoreError::Unavailable);
      }
      items.push(item);
    }
    let has_more = items.len() > usize::from(request.limit);
    items.truncate(usize::from(request.limit));
    let next = has_more.then(|| items.last().expect("positive bounded page").position());
    tx.commit().await.map_err(unavailable)?;
    Ok(FactoryActionQueuePage {
      total: u64::try_from(total).map_err(|_| StoreError::Unavailable)?,
      coverage: if complete {
        FactoryActionProjectionCoverage::Complete
      } else {
        FactoryActionProjectionCoverage::Incomplete
      },
      items,
      next,
    })
  }
}
