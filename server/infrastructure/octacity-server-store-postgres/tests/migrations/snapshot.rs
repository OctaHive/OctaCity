use std::{env, str::FromStr as _};

use sqlx::{Connection as _, PgConnection, PgPool, postgres::PgConnectOptions};

use crate::support::TestDatabase;

pub(super) struct DatabaseSnapshot {
  name: String,
  base_options: PgConnectOptions,
}

impl DatabaseSnapshot {
  pub(super) async fn capture(database: &mut TestDatabase) -> Self {
    database.pool.close().await;

    let name = format!("octacity_snapshot_{}", uuid::Uuid::new_v4().simple());
    // Both identifiers are generated locally from fixed ASCII prefixes and
    // simple UUIDs, so quoting cannot include attacker-controlled SQL.
    let create_snapshot = sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\" TEMPLATE \"{}\"", database.name));
    sqlx::query(create_snapshot)
      .execute(&mut database.admin)
      .await
      .expect("create PostgreSQL database snapshot");

    let base_options = test_database_options();
    database.pool = PgPool::connect_with(base_options.clone().database(&database.name))
      .await
      .expect("reconnect to snapshotted PostgreSQL database");
    Self { name, base_options }
  }

  pub(super) async fn restore(&self) -> TestDatabase {
    TestDatabase::create(Some(&self.name)).await
  }

  pub(super) async fn cleanup(self) {
    let mut admin = PgConnection::connect_with(&self.base_options)
      .await
      .expect("connect to PostgreSQL administration database");
    let drop_database = sqlx::AssertSqlSafe(format!("DROP DATABASE \"{}\" WITH (FORCE)", self.name));
    sqlx::query(drop_database)
      .execute(&mut admin)
      .await
      .expect("drop PostgreSQL database snapshot");
  }
}

fn test_database_options() -> PgConnectOptions {
  let base_url =
    env::var("OCTACITY_POSTGRES_URL").expect("OCTACITY_POSTGRES_URL must name a disposable PostgreSQL server");
  PgConnectOptions::from_str(&base_url).expect("invalid OCTACITY_POSTGRES_URL")
}
