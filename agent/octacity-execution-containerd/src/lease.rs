//! Owns the short-lived containerd lease used while a snapshot becomes a container root.

use super::*;
use containerd_client::services::v1::{
  CreateRequest as CreateLeaseRequest, DeleteRequest as DeleteLeaseRequest, ListRequest as ListLeasesRequest,
};

pub(super) async fn create_startup_lease(
  client: &Client,
  namespace: &str,
  id: &str,
  labels: HashMap<String, String>,
  deadline: Instant,
  cancellation: &CancellationToken,
) -> Result<(), ExecutionError> {
  grpc_before(
    deadline,
    Some(cancellation),
    "create containerd startup lease",
    client.leases().create(namespaced(
      CreateLeaseRequest {
        id: id.to_owned(),
        labels,
      },
      namespace,
    )?),
  )
  .await?;
  Ok(())
}

/// Deletes a lease synchronously so its resources have reached a stable GC state on return.
pub(super) async fn delete_lease(
  client: &Client,
  namespace: &str,
  id: &str,
  deadline: Instant,
  cancellation: Option<&CancellationToken>,
) -> Result<(), ExecutionError> {
  let result = before_deadline(
    deadline,
    cancellation,
    "delete containerd startup lease",
    client.leases().delete(namespaced(
      DeleteLeaseRequest {
        id: id.to_owned(),
        sync: true,
      },
      namespace,
    )?),
  )
  .await?;
  match result {
    Ok(_) => Ok(()),
    Err(error) if error.code() == Code::NotFound => Ok(()),
    Err(error) => Err(grpc("delete containerd startup lease", error)),
  }
}

pub(super) async fn owned_lease_ids(
  client: &Client,
  namespace: &str,
  owner: &str,
  deadline: Instant,
) -> Result<Vec<String>, ExecutionError> {
  let response = grpc_before(
    deadline,
    None,
    "list containerd leases",
    client
      .leases()
      .list(namespaced(ListLeasesRequest { filters: Vec::new() }, namespace)?),
  )
  .await?
  .into_inner();
  Ok(
    response
      .leases
      .into_iter()
      .filter(|lease| lease.labels.get(OWNER_LABEL).is_some_and(|label| label == owner))
      .map(|lease| lease.id)
      .collect(),
  )
}
