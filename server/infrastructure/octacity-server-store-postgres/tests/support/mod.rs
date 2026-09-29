use std::{env, str::FromStr as _, sync::Arc};

use octacity_server_job::JobSpecSigner;

use sqlx::{Connection as _, PgConnection, PgPool, postgres::PgConnectOptions};

const DATABASE_URL_ENV: &str = "OCTACITY_POSTGRES_URL";

#[allow(dead_code)]
pub fn test_signer() -> Arc<JobSpecSigner> {
  Arc::new(JobSpecSigner::new("postgres-contract-key", [7; 32]).expect("static signing fixture is valid"))
}

pub struct TestDatabase {
  pub pool: PgPool,
  pub(crate) admin: PgConnection,
  pub(crate) name: String,
}

impl TestDatabase {
  pub async fn empty() -> Self {
    Self::create(None).await
  }

  pub async fn migrated() -> Self {
    let database = Self::empty().await;
    octacity_server_store_postgres::migrate(&database.pool)
      .await
      .expect("migrate empty PostgreSQL database");
    database
  }

  pub async fn cleanup(mut self) {
    self.pool.close().await;
    let drop_database = sqlx::AssertSqlSafe(format!("DROP DATABASE \"{}\" WITH (FORCE)", self.name));
    sqlx::query(drop_database)
      .execute(&mut self.admin)
      .await
      .expect("drop migration database");
  }

  pub(crate) async fn create(template: Option<&str>) -> Self {
    let base_options = base_options();
    let name = database_name("migration");
    let mut admin = PgConnection::connect_with(&base_options)
      .await
      .expect("connect to PostgreSQL administration database");
    // Every identifier is generated locally from a fixed ASCII prefix and a
    // simple UUID, so quoting cannot include attacker-controlled SQL.
    let statement = match template {
      Some(template) => format!("CREATE DATABASE \"{name}\" TEMPLATE \"{template}\""),
      None => format!("CREATE DATABASE \"{name}\""),
    };
    sqlx::query(sqlx::AssertSqlSafe(statement))
      .execute(&mut admin)
      .await
      .expect("create disposable PostgreSQL database");
    let pool = PgPool::connect_with(base_options.database(&name))
      .await
      .expect("connect to disposable PostgreSQL database");
    Self { pool, admin, name }
  }
}

fn base_options() -> PgConnectOptions {
  let base_url = env::var(DATABASE_URL_ENV).expect("OCTACITY_POSTGRES_URL must name a disposable PostgreSQL server");
  PgConnectOptions::from_str(&base_url).expect("invalid OCTACITY_POSTGRES_URL")
}

fn database_name(kind: &str) -> String {
  format!("octacity_{kind}_{}", uuid::Uuid::new_v4().simple())
}
