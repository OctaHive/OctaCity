use octacity_server_domain::{AgentId, BuildId, PoolId, ProjectId};
use octacity_server_store::{
  ResourceSearchKind, ResourceSearchPage, ResourceSearchPagePosition, ResourceSearchRank, ResourceSearchResource,
  ResourceSearchSummary, ResourceSearchVisibilityView, SearchResources, StoreError,
};
use sqlx::{FromRow, PgPool};

use crate::{database::unavailable, discovery::requested_row_limit};

const RESOURCE_SEARCH_SQL: &str = include_str!("resource_search.sql");

#[derive(FromRow)]
struct ResourceSearchRow {
  kind: String,
  id: uuid::Uuid,
  label: String,
  context: Option<String>,
  normalized_label: String,
  match_rank: i32,
}

struct SqlResourceSearchVisibility {
  all: bool,
  projects: Vec<uuid::Uuid>,
  builds: Vec<uuid::Uuid>,
  agents: Vec<uuid::Uuid>,
  pools: Vec<uuid::Uuid>,
}

pub(crate) async fn search(pool: &PgPool, request: SearchResources) -> Result<ResourceSearchPage, StoreError> {
  if matches!(request.visibility().view(), ResourceSearchVisibilityView::None) {
    return Ok(ResourceSearchPage {
      items: Vec::new(),
      next_cursor: None,
    });
  }

  let visibility = sql_visibility(request.visibility().view());
  let query = request.query().as_str();
  let exact_identity = uuid::Uuid::parse_str(query).ok();
  let after = request.after();
  let rows = sqlx::query_as::<_, ResourceSearchRow>(RESOURCE_SEARCH_SQL)
    .bind(query)
    .bind(request.kinds().contains(&ResourceSearchKind::Project))
    .bind(request.kinds().contains(&ResourceSearchKind::Build))
    .bind(request.kinds().contains(&ResourceSearchKind::Agent))
    .bind(request.kinds().contains(&ResourceSearchKind::AgentPool))
    .bind(visibility.all)
    .bind(exact_identity)
    .bind(visibility.projects)
    .bind(visibility.builds)
    .bind(visibility.agents)
    .bind(visibility.pools)
    .bind(after.map(|position| rank_value(position.rank)))
    .bind(after.map(|position| kind_value(position.kind)))
    .bind(after.map(|position| position.normalized_label.as_str()))
    .bind(after.map(|position| resource_uuid(position.resource)))
    .bind(requested_row_limit(request.limit().get()))
    .fetch_all(pool)
    .await
    .map_err(unavailable)?;

  let limit = usize::from(request.limit().get());
  let has_more = rows.len() > limit;
  let mut decoded = rows
    .into_iter()
    .take(limit)
    .map(decode)
    .collect::<Result<Vec<_>, StoreError>>()?;
  let next_cursor = has_more.then(|| {
    decoded
      .last()
      .expect("a positive full search page has a final item")
      .1
      .clone()
  });
  Ok(ResourceSearchPage {
    items: decoded.drain(..).map(|(summary, _)| summary).collect(),
    next_cursor,
  })
}

fn decode(row: ResourceSearchRow) -> Result<(ResourceSearchSummary, ResourceSearchPagePosition), StoreError> {
  let rank = match row.match_rank {
    0 => ResourceSearchRank::ExactIdentifier,
    1 => ResourceSearchRank::NamePrefix,
    2 => ResourceSearchRank::NameContains,
    _ => return Err(StoreError::Unavailable),
  };
  let resource = match row.kind.as_str() {
    "project" => ResourceSearchResource::Project(ProjectId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?),
    "build" => ResourceSearchResource::Build(BuildId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?),
    "agent" => ResourceSearchResource::Agent(AgentId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?),
    "agent_pool" => ResourceSearchResource::AgentPool(PoolId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?),
    _ => return Err(StoreError::Unavailable),
  };
  if row.normalized_label != octacity_server_store::normalize_resource_search_text(&row.label) {
    return Err(StoreError::Unavailable);
  }
  let summary = ResourceSearchSummary::new(resource, row.label, row.context).map_err(|_| StoreError::Unavailable)?;
  let position = ResourceSearchPagePosition {
    rank,
    kind: resource.kind(),
    normalized_label: row.normalized_label,
    resource,
  };
  Ok((summary, position))
}

fn sql_visibility(visibility: ResourceSearchVisibilityView<'_>) -> SqlResourceSearchVisibility {
  let mut sql = SqlResourceSearchVisibility {
    all: matches!(visibility, ResourceSearchVisibilityView::All),
    projects: Vec::new(),
    builds: Vec::new(),
    agents: Vec::new(),
    pools: Vec::new(),
  };
  if let ResourceSearchVisibilityView::Restricted(resources) = visibility {
    for resource in resources {
      match resource {
        ResourceSearchResource::Project(identity) => sql.projects.push(identity.as_uuid()),
        ResourceSearchResource::Build(identity) => sql.builds.push(identity.as_uuid()),
        ResourceSearchResource::Agent(identity) => sql.agents.push(identity.as_uuid()),
        ResourceSearchResource::AgentPool(identity) => sql.pools.push(identity.as_uuid()),
      }
    }
  }
  sql
}

const fn rank_value(rank: ResourceSearchRank) -> i32 {
  match rank {
    ResourceSearchRank::ExactIdentifier => 0,
    ResourceSearchRank::NamePrefix => 1,
    ResourceSearchRank::NameContains => 2,
  }
}

const fn kind_value(kind: ResourceSearchKind) -> i32 {
  match kind {
    ResourceSearchKind::Project => 0,
    ResourceSearchKind::Build => 1,
    ResourceSearchKind::Agent => 2,
    ResourceSearchKind::AgentPool => 3,
  }
}

const fn resource_uuid(resource: ResourceSearchResource) -> uuid::Uuid {
  match resource {
    ResourceSearchResource::Project(identity) => identity.as_uuid(),
    ResourceSearchResource::Build(identity) => identity.as_uuid(),
    ResourceSearchResource::Agent(identity) => identity.as_uuid(),
    ResourceSearchResource::AgentPool(identity) => identity.as_uuid(),
  }
}
