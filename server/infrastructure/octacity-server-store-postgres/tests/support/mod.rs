use std::{env, str::FromStr as _};

use sqlx::{Connection as _, PgConnection, PgPool, postgres::PgConnectOptions};

const DATABASE_URL_ENV: &str = "OCTACITY_POSTGRES_URL";

pub struct TestDatabase {
  pub pool: PgPool,
  admin: PgConnection,
  name: String,
}

impl TestDatabase {
  pub async fn migrated() -> Self {
    let base_url = env::var(DATABASE_URL_ENV).expect("OCTACITY_POSTGRES_URL must name a disposable PostgreSQL server");
    let base_options = PgConnectOptions::from_str(&base_url).expect("invalid OCTACITY_POSTGRES_URL");
    let name = format!("octacity_migration_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect_with(&base_options)
      .await
      .expect("connect to PostgreSQL administration database");
    // `name` is generated locally from a fixed ASCII prefix and a simple UUID,
    // so the quoted identifier cannot contain attacker-controlled SQL.
    let create_database = sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\""));
    sqlx::query(create_database)
      .execute(&mut admin)
      .await
      .expect("create empty migration database");
    let pool = PgPool::connect_with(base_options.database(&name))
      .await
      .expect("connect to empty migration database");
    octacity_server_store_postgres::migrate(&pool)
      .await
      .expect("migrate empty PostgreSQL database");
    Self { pool, admin, name }
  }

  pub async fn cleanup(mut self) {
    self.pool.close().await;
    let drop_database = sqlx::AssertSqlSafe(format!("DROP DATABASE \"{}\" WITH (FORCE)", self.name));
    sqlx::query(drop_database)
      .execute(&mut self.admin)
      .await
      .expect("drop migration database");
  }
}
