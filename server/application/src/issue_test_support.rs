//! In-memory application fixtures for transport conformance; absent from production builds.
use octacity_server_domain::{ProjectId, ProjectName, Timestamp};
/// Deterministic store shared by task and workflow handlers in transport tests.
pub use octacity_server_store::testing::InMemoryProjectStore;
use octacity_server_store::{CreateProject, ProjectStore, testing::management_mutation};
use std::sync::Arc;
/// Prepares a visible ordinary Project without providers, repositories or Factory state.
pub async fn manual_issue_store(project_id: ProjectId) -> Arc<InMemoryProjectStore> {
  let store = Arc::new(InMemoryProjectStore::new());
  store
    .create_project(management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Tasks").expect("valid test Project"),
      idempotency_key: "project".parse().expect("valid fixture key"),
      created_at: Timestamp::from_unix_millis(1).expect("valid fixture time"),
    }))
    .await
    .expect("fresh manual Issue fixture");
  store
}
