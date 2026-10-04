use std::{future::Future, pin::pin, sync::Arc, task::Context, task::Poll, task::Waker};

use octacity_server_application::{
  MAX_RESOURCE_SEARCH_CURSOR_BYTES, MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE, ManagementAction,
  ManagementAuthorizationGrant, ManagementAuthorizationTarget, ManagementResource, ManagementResourceIdentity,
  ManagementResourceKind, ManagementVisibility, ResourceSearchCursor, ResourceSearchCursorError,
  ResourceSearchHandlers, ResourceSearchIdentityProjection, ResourceSearchInputError, ResourceSearchKind,
  ResourceSearchKinds, SearchResourcesQuery,
};
use octacity_server_domain::{AgentId, BuildId, PoolId, ProjectId};
use octacity_server_store::{
  MAX_RESOURCE_SEARCH_QUERY_BYTES, ResourceSearchResource, ResourceSearchSummary, testing::InMemoryResourceSearchStore,
};

#[path = "support/management_query.rs"]
mod management_query_support;
use management_query_support::{management_query, management_query_with_grant};

#[test]
fn search_ranks_exact_identifiers_then_names_and_applies_kind_filters() {
  run_ready(async {
    let store = Arc::new(InMemoryResourceSearchStore::new());
    let exact = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let project = ProjectId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
    let agent = AgentId::from_uuid(uuid::Uuid::from_u128(3)).unwrap();
    let pool = PoolId::from_uuid(uuid::Uuid::from_u128(4)).unwrap();
    seed(&store, ResourceSearchResource::Build(exact), "Unrelated Build");
    seed(&store, ResourceSearchResource::Project(project), &exact.to_string());
    seed(&store, ResourceSearchResource::Agent(agent), &format!("Agent {exact}"));
    seed(&store, ResourceSearchResource::AgentPool(pool), "Pool");
    let handlers = ResourceSearchHandlers::new(store);

    let page = management_query(
      &handlers,
      SearchResourcesQuery::try_new(&exact.to_string(), ResourceSearchKinds::all(), None, 10).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
      page.items.iter().map(|item| item.resource).collect::<Vec<_>>(),
      vec![
        ResourceSearchIdentityProjection::Build(exact),
        ResourceSearchIdentityProjection::Project(project),
        ResourceSearchIdentityProjection::Agent(agent),
      ]
    );

    let builds_only = management_query(
      &handlers,
      SearchResourcesQuery::try_new(
        &exact.to_string(),
        ResourceSearchKinds::try_only([ResourceSearchKind::Build]).unwrap(),
        None,
        10,
      )
      .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(builds_only.items.len(), 1);
    assert_eq!(
      builds_only.items[0].resource,
      ResourceSearchIdentityProjection::Build(exact)
    );
  });
}

#[test]
fn equal_rank_order_and_cursor_pages_are_stable_and_scope_bound() {
  run_ready(async {
    let store = Arc::new(InMemoryResourceSearchStore::new());
    let project_high = ProjectId::from_uuid(uuid::Uuid::from_u128(20)).unwrap();
    let project_low = ProjectId::from_uuid(uuid::Uuid::from_u128(10)).unwrap();
    let build = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    seed(&store, ResourceSearchResource::Project(project_high), "Alpha");
    seed(&store, ResourceSearchResource::Project(project_low), "Alpha");
    seed(&store, ResourceSearchResource::Build(build), "Alpha");
    let handlers = ResourceSearchHandlers::new(store);
    let kinds = ResourceSearchKinds::all();

    let first = management_query(
      &handlers,
      SearchResourcesQuery::try_new("  ALPHA ", kinds.clone(), None, 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
      first.items[0].resource,
      ResourceSearchIdentityProjection::Project(project_low)
    );
    let cursor = first.next_cursor.unwrap();
    let encoded = cursor.encode();
    let decoded = ResourceSearchCursor::decode(&encoded).unwrap();
    assert_eq!(decoded, cursor);
    assert_eq!(
      SearchResourcesQuery::try_new("beta", kinds.clone(), Some(cursor.clone()), 1),
      Err(ResourceSearchInputError::CursorScopeMismatch)
    );
    assert_eq!(
      SearchResourcesQuery::try_new(
        "alpha",
        ResourceSearchKinds::try_only([ResourceSearchKind::Project]).unwrap(),
        Some(cursor),
        1,
      ),
      Err(ResourceSearchInputError::CursorScopeMismatch)
    );

    let second = management_query(
      &handlers,
      SearchResourcesQuery::try_new("alpha", kinds.clone(), Some(decoded), 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
      second.items[0].resource,
      ResourceSearchIdentityProjection::Project(project_high)
    );
    let third = management_query(
      &handlers,
      SearchResourcesQuery::try_new("alpha", kinds, second.next_cursor, 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(third.items[0].resource, ResourceSearchIdentityProjection::Build(build));
    assert_eq!(third.next_cursor, None);
  });
}

#[test]
fn visibility_is_applied_before_ranking_and_pagination() {
  run_ready(async {
    let store = Arc::new(InMemoryResourceSearchStore::new());
    let hidden = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let visible = ProjectId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
    seed(&store, ResourceSearchResource::Build(hidden), "Hidden");
    seed(
      &store,
      ResourceSearchResource::Project(visible),
      &format!("{} visible", hidden),
    );
    let handlers = ResourceSearchHandlers::new(store);
    let visible_resource = ManagementResource::instance(
      ManagementResourceKind::Project,
      ManagementResourceIdentity::new(visible.to_string()).unwrap(),
    )
    .unwrap();
    let page = management_query_with_grant(
      &handlers,
      SearchResourcesQuery::try_new(&hidden.to_string(), ResourceSearchKinds::all(), None, 1).unwrap(),
      ManagementAuthorizationGrant::new(ManagementVisibility::restricted([visible_resource]).unwrap()),
    )
    .await
    .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(
      page.items[0].resource,
      ResourceSearchIdentityProjection::Project(visible)
    );
    assert_eq!(page.next_cursor, None);
  });
}

#[test]
fn search_inputs_cursors_and_authorization_mapping_are_bounded() {
  assert_eq!(
    SearchResourcesQuery::try_new(" \t ", ResourceSearchKinds::all(), None, 1),
    Err(ResourceSearchInputError::InvalidQuery)
  );
  assert_eq!(
    SearchResourcesQuery::try_new(
      &"a".repeat(MAX_RESOURCE_SEARCH_QUERY_BYTES + 1),
      ResourceSearchKinds::all(),
      None,
      1,
    ),
    Err(ResourceSearchInputError::InvalidQuery)
  );
  assert_eq!(
    ResourceSearchKinds::try_only([]),
    Err(ResourceSearchInputError::InvalidKinds)
  );
  assert_eq!(
    ResourceSearchKinds::try_only([ResourceSearchKind::Agent, ResourceSearchKind::Agent]),
    Err(ResourceSearchInputError::InvalidKinds)
  );
  for limit in [0, MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE + 1] {
    assert_eq!(
      SearchResourcesQuery::try_new("alpha", ResourceSearchKinds::all(), None, limit),
      Err(ResourceSearchInputError::InvalidLimit)
    );
  }
  for invalid in ["", "%%%", &"a".repeat(MAX_RESOURCE_SEARCH_CURSOR_BYTES + 1)] {
    assert_eq!(ResourceSearchCursor::decode(invalid), Err(ResourceSearchCursorError));
  }

  let query = SearchResourcesQuery::try_new("alpha", ResourceSearchKinds::all(), None, 1).unwrap();
  assert_eq!(SearchResourcesQuery::AUTHORIZATION.action(), ManagementAction::Search);
  assert_eq!(
    SearchResourcesQuery::AUTHORIZATION.resource_kind(),
    ManagementResourceKind::ControlPlane
  );
  assert_eq!(
    query.management_resource().unwrap(),
    ManagementResource::collection(ManagementResourceKind::ControlPlane).unwrap()
  );
}

fn seed(store: &InMemoryResourceSearchStore, resource: ResourceSearchResource, label: &str) {
  store
    .seed(ResourceSearchSummary::new(resource, label, Some("safe context".to_owned())).unwrap())
    .unwrap();
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = pin!(future);
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(output) => output,
    Poll::Pending => panic!("in-memory search future must complete without yielding"),
  }
}
