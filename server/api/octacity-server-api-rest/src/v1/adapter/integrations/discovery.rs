//! Bounded protected connection discovery and validation; no provider or credential reads.
use super::*;
use octacity_server_application::{
  ListIntegrationConnectionsQuery, ListIntegrationInstallationsQuery, ValidateIntegrationConnectionQuery,
};

pub(in crate::v1::adapter) struct DiscoveryEndpoints {
  connections: Arc<dyn AuthorizedManagementQueryHandler<ListIntegrationConnectionsQuery, Error = ApplicationError>>,
  installations: Arc<dyn AuthorizedManagementQueryHandler<ListIntegrationInstallationsQuery, Error = ApplicationError>>,
  validate: Arc<dyn AuthorizedManagementQueryHandler<ValidateIntegrationConnectionQuery, Error = ApplicationError>>,
}
impl IntegrationManagementApplication {
  /// Adds separately authorized safe discovery, without starting workers or granting Project use.
  pub fn with_discovery<Q>(mut self, queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<ListIntegrationConnectionsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIntegrationInstallationsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ValidateIntegrationConnectionQuery, Error = ApplicationError>
      + 'static,
  {
    self.discovery = Some(DiscoveryEndpoints {
      connections: queries.clone(),
      installations: queries.clone(),
      validate: queries,
    });
    self
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::v1::adapter) struct DiscoveryPageQuery {
  after: Option<String>,
  limit: Option<u16>,
}
fn page_limit(query: &DiscoveryPageQuery, id: &RequestId) -> Result<u16, ApiError> {
  let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
  if !(1..=octacity_server_application::MAX_INTEGRATION_DISCOVERY_PAGE_SIZE).contains(&limit) {
    return Err(application_error(ApplicationFailure::Invalid, id));
  }
  Ok(limit)
}
pub(in crate::v1::adapter) async fn list_connections(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  query: Result<Query<DiscoveryPageQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let Query(query) = query.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let limit = page_limit(&query, &id)?;
  let page = app
    .discovery
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .connections
    .handle_authorized_query(
      &context,
      ListIntegrationConnectionsQuery {
        after: query
          .after
          .map(|s| s.parse())
          .transpose()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        limit,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let items = page
    .items
    .into_iter()
    .map(|c| connection_representation(c, &id))
    .collect::<Result<Vec<_>, _>>()?;
  Ok(Json(
    json!({"items":items,"next_cursor":page.next_cursor.map(|id|id.to_string())}),
  ))
}
pub(in crate::v1::adapter) async fn list_installations(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  query: Result<Query<DiscoveryPageQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let Query(query) = query.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let limit = page_limit(&query, &id)?;
  let page = app
    .discovery
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .installations
    .handle_authorized_query(
      &context,
      ListIntegrationInstallationsQuery {
        after: query
          .after
          .map(|s| s.parse())
          .transpose()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        limit,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(page, &id)?))
}
pub(in crate::v1::adapter) async fn validate_connection(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(connection): Path<String>,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let intent: octacity_server_application::IntegrationConnectionIntent =
    serde_json::from_value(body(payload, &id)?.intent)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  if intent.id.to_string() != connection {
    return Err(application_error(ApplicationFailure::Invalid, &id));
  }
  let result = app
    .discovery
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .validate
    .handle_authorized_query(&context, ValidateIntegrationConnectionQuery { intent })
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
