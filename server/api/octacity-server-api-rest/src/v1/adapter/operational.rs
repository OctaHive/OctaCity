use std::sync::Arc;

use axum::{
  Json,
  extract::{Extension, State},
};
use octacity_server_application::GetOperationalMetadataQuery;

use super::{ApiError, ManagementApplication, authorized_handler_error};
use crate::v1::OperationalMetadata;

pub(super) async fn get_operational_metadata(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
) -> Result<Json<OperationalMetadata>, ApiError> {
  let projection = application
    .operational
    .0
    .handle_authorized_query(&context, GetOperationalMetadataQuery)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(OperationalMetadata::trusted_network(
    projection.management_externally_reachable,
    projection.external_access_acknowledged,
    projection.agent_ingress_enabled,
    projection.webhook_ingress_enabled,
  )))
}
