//! Dedicated databases keep polling workers from consuming another pilot's inputs.

pub(super) struct PilotDatabase {
  admin: sqlx::PgPool,
  name: String,
  pub(super) url: String,
}

impl PilotDatabase {
  pub(super) async fn create(original: &str) -> Self {
    let admin = sqlx::PgPool::connect(original).await.unwrap();
    let name = format!("octacity_pilot_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
      .execute(&admin)
      .await
      .unwrap();
    let mut url = url::Url::parse(original).unwrap();
    url.set_path(&name);
    Self {
      admin,
      name,
      url: url.into(),
    }
  }

  pub(super) async fn cleanup(self) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "DROP DATABASE \"{}\" WITH (FORCE)",
      self.name
    )))
    .execute(&self.admin)
    .await
    .unwrap();
    self.admin.close().await;
  }
}
