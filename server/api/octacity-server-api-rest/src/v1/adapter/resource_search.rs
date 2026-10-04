use std::{fmt, str::FromStr};

use octacity_server_application::{
  ResourceSearchCursor as ApplicationResourceSearchCursor, ResourceSearchKind as ApplicationResourceSearchKind,
  ResourceSearchKinds, SearchResourcesQuery,
};
use serde::de::{Error as _, MapAccess, Visitor};

use super::*;

pub(super) struct ResourceSearchParameters {
  query: String,
  kinds: Option<ResourceSearchKindFilter>,
  after: Option<RestResourceSearchCursor>,
  limit: u16,
}

impl<'de> Deserialize<'de> for ResourceSearchParameters {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserializer.deserialize_map(ResourceSearchParametersVisitor)
  }
}

struct ResourceSearchParametersVisitor;

impl<'de> Visitor<'de> for ResourceSearchParametersVisitor {
  type Value = ResourceSearchParameters;

  fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("bounded global resource-search query parameters")
  }

  fn visit_map<M>(self, mut parameters: M) -> Result<Self::Value, M::Error>
  where
    M: MapAccess<'de>,
  {
    let mut query = None;
    let mut kinds = Vec::new();
    let mut after = None;
    let mut limit = None;
    while let Some(name) = parameters.next_key::<String>()? {
      match name.as_str() {
        "query" => set_once(&mut query, parameters.next_value()?, "query")?,
        "kinds" => {
          let next = parameters.next_value::<ResourceSearchKindFilter>()?.0;
          if kinds.len().saturating_add(next.len()) > ApplicationResourceSearchKind::ALL.len() {
            return Err(M::Error::custom("resource search kind filter is invalid"));
          }
          kinds.extend(next);
        }
        "after" => set_once(&mut after, parameters.next_value()?, "after")?,
        "limit" => set_once(&mut limit, parameters.next_value()?, "limit")?,
        _ => return Err(M::Error::unknown_field(&name, &["query", "kinds", "after", "limit"])),
      }
    }
    Ok(ResourceSearchParameters {
      query: query.ok_or_else(|| M::Error::missing_field("query"))?,
      kinds: (!kinds.is_empty()).then_some(ResourceSearchKindFilter(kinds)),
      after,
      limit: limit.unwrap_or_else(default_resource_search_limit),
    })
  }
}

#[derive(Debug)]
struct ResourceSearchKindFilter(Vec<ApplicationResourceSearchKind>);

impl<'de> Deserialize<'de> for ResourceSearchKindFilter {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let value = String::deserialize(deserializer)?;
    value.parse().map_err(D::Error::custom)
  }
}

impl FromStr for ResourceSearchKindFilter {
  type Err = ContractValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let mut kinds = Vec::new();
    for value in value.split(',') {
      if kinds.len() == ApplicationResourceSearchKind::ALL.len() {
        return Err(ContractValueError::InvalidResourceSearchKind);
      }
      kinds.push(value.parse::<ResourceSearchKind>().map(|kind| match kind {
        ResourceSearchKind::Project => ApplicationResourceSearchKind::Project,
        ResourceSearchKind::Build => ApplicationResourceSearchKind::Build,
        ResourceSearchKind::Agent => ApplicationResourceSearchKind::Agent,
        ResourceSearchKind::AgentPool => ApplicationResourceSearchKind::AgentPool,
      })?);
    }
    Ok(Self(kinds))
  }
}

const fn default_resource_search_limit() -> u16 {
  DEFAULT_PAGE_LIMIT
}

pub(super) async fn search_resources(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  parameters: Result<Query<ResourceSearchParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let kinds = parameters
    .kinds
    .map(|kinds| ResourceSearchKinds::try_only(kinds.0))
    .transpose()
    .map_err(|error| invalid_resource_search(&error.to_string(), &request_id))?
    .unwrap_or_default();
  let after = parameters
    .after
    .map(|cursor| ApplicationResourceSearchCursor::decode(cursor.as_str()))
    .transpose()
    .map_err(|error| invalid_resource_search(&error.to_string(), &request_id))?;
  let query = SearchResourcesQuery::try_new(&parameters.query, kinds, after, parameters.limit)
    .map_err(|error| invalid_resource_search(&error.to_string(), &request_id))?;
  let page = application
    .resource_search
    .0
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(resource_search_page(page, &request_id)?)))
}

fn invalid_resource_search(message: &str, request_id: &RequestId) -> ApiError {
  ApiError::new(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, message, request_id)
}
