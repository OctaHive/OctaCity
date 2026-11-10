//! Restores retained Issue data without invoking a live classification writer.
use octacity_server_store::IssueRevision;

pub async fn seed(pool: &sqlx::PgPool, revision: &IssueRevision) {
  // A restored historical revision is data, not a new classification command.
  // Guarded producer/manual writers are qualified separately in tasks 2.1–2.3.
  let mut tx = pool.begin().await.unwrap();
  sqlx::query("INSERT INTO issue_revisions(project_id,issue_id,version,workflow_id,workflow_version,document) VALUES($1,$2,$3,$4,$5,$6)")
    .bind(revision.project_id.as_uuid()).bind(revision.id.as_uuid()).bind(i64::try_from(revision.version.get()).unwrap())
    .bind(revision.workflow_id.as_uuid()).bind(i64::try_from(revision.workflow_version.get()).unwrap())
    .bind(sqlx::types::Json(revision)).execute(&mut *tx).await.unwrap();
  sqlx::query("UPDATE issues SET current_version=$3 WHERE project_id=$1 AND id=$2")
    .bind(revision.project_id.as_uuid())
    .bind(revision.id.as_uuid())
    .bind(i64::try_from(revision.version.get()).unwrap())
    .execute(&mut *tx)
    .await
    .unwrap();
  tx.commit().await.unwrap();
}
