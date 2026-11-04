use async_trait::async_trait;
use octacity_server_domain::ProjectId;

use crate::{ProjectPolicyDocument, StoreError};

/// Backend-neutral read port for an immutable root-to-leaf Project policy lineage.
#[async_trait]
pub trait ProjectPolicyStore: Send + Sync {
  /// Loads the current policy version for every Project from the root through
  /// `project_id`, preserving ancestry order.
  async fn project_policy_lineage(&self, project_id: ProjectId) -> Result<Vec<ProjectPolicyDocument>, StoreError>;
}
