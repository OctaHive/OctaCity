//! Fault injection only: interrupt acknowledgement after a real effect commits in the disposable DB.
//! Business outcomes are observed through the public API, never manufactured by these triggers.
use super::*;

pub(super) struct Checkpoint {
  pool: sqlx::PgPool,
  table: &'static str,
  name: String,
  project: Uuid,
}
impl Checkpoint {
  pub(super) async fn import(project: &str) -> Self {
    Self::install(project,"issue_source_reads",
      "convert_from(NEW.document,'UTF8')::jsonb->'progress' IS DISTINCT FROM convert_from(OLD.document,'UTF8')::jsonb->'progress'",
      "EXISTS(SELECT 1 FROM issue_source_links WHERE project_id=NEW.project_id)").await
  }
  pub(super) async fn admission(project: &str) -> Self {
    Self::install(project,"factory_source_scans",
      "NEW.document->'progress' IS DISTINCT FROM OLD.document->'progress'",
      "NEW.kind='input' AND EXISTS(SELECT 1 FROM factory_source_preparations WHERE project_id=NEW.project_id AND input_id=NEW.id AND settled)").await
  }
  async fn install(project: &str, table: &'static str, progress: &str, committed: &str) -> Self {
    let pool = sqlx::PgPool::connect(&required_string("OCTACITY_POSTGRES_URL"))
      .await
      .unwrap();
    let project = Uuid::parse_str(project).unwrap();
    let name = format!("source_checkpoint_{}", Uuid::new_v4().simple());
    // Identifiers/SQL fragments are fixture literals or generated/parsed UUIDs, never provider text.
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
      IF NEW.project_id='{project}'::uuid AND ({progress}) AND ({committed}) THEN
        RAISE EXCEPTION 'controlled post-effect checkpoint interruption';
      END IF; RETURN NEW; END $$"
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "CREATE TRIGGER {name} BEFORE UPDATE ON {table} FOR EACH ROW EXECUTE FUNCTION {name}()"
    )))
    .execute(&pool)
    .await
    .unwrap();
    Self {
      pool,
      table,
      name,
      project,
    }
  }
  pub(super) async fn wait(&self) {
    let document = if self.table == "issue_source_reads" {
      "convert_from(document,'UTF8')::jsonb"
    } else {
      "document"
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
      // Coordination with the injected failure, not an assertion about business state.
      let failed: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE project_id=$1 AND {document}#>>'{{schedule,failure}}'='transient')",
        self.table
      )))
      .bind(self.project)
      .fetch_one(&self.pool)
      .await
      .unwrap();
      if failed {
        return;
      }
      assert!(
        Instant::now() < deadline,
        "the controlled post-effect interruption must be reached"
      );
      sleep(Duration::from_millis(20)).await;
    }
  }
  pub(super) async fn release(self) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "DROP TRIGGER {} ON {}",
      self.name, self.table
    )))
    .execute(&self.pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP FUNCTION {}()", self.name)))
      .execute(&self.pool)
      .await
      .unwrap();
    self.pool.close().await;
  }
}
