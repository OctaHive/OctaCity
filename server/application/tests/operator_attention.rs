use std::{future::Future, pin::pin, sync::Arc, task::Context, task::Poll, task::Waker};

use octacity_server_application::{
  ListOperatorAttentionQuery, MAX_OPERATOR_ATTENTION_CURSOR_BYTES, MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE,
  MAX_OPERATOR_ATTENTION_SCOPE_TARGETS, ManagementAction, ManagementAuthorizationGrant, ManagementAuthorizationTarget,
  ManagementResource, ManagementResourceIdentity, ManagementResourceKind, ManagementVisibility,
  OperatorAttentionCategory, OperatorAttentionCursor, OperatorAttentionCursorError, OperatorAttentionHandlers,
  OperatorAttentionInputError, OperatorAttentionScopeInput, OperatorAttentionSeverity, OperatorAttentionTarget,
};
use octacity_server_domain::{AgentId, BuildId, PoolId, Timestamp};
use octacity_server_store::{
  OperatorAttentionEvent, OperatorAttentionEventKind, OperatorAttentionId, testing::InMemoryOperatorAttentionStore,
};

#[path = "support/management_query.rs"]
mod management_query_support;
use management_query_support::{management_query, management_query_with_grant};

#[test]
fn target_and_critical_scopes_return_only_server_classified_safe_items() {
  run_ready(async {
    let store = Arc::new(InMemoryOperatorAttentionStore::new());
    let build = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let agent = AgentId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
    let pool = PoolId::from_uuid(uuid::Uuid::from_u128(3)).unwrap();
    seed(&store, 1, OperatorAttentionEventKind::BuildFailed(build), 100, None);
    seed(
      &store,
      2,
      OperatorAttentionEventKind::AgentUnavailable(agent),
      200,
      None,
    );
    seed(
      &store,
      3,
      OperatorAttentionEventKind::AgentPoolUnavailable(pool),
      300,
      None,
    );
    seed(
      &store,
      4,
      OperatorAttentionEventKind::CriticalSystemCondition {
        code: "storage_unavailable".to_owned(),
      },
      400,
      Some(450),
    );
    let handlers = OperatorAttentionHandlers::new(store);

    let targets = scope(
      [
        OperatorAttentionTarget::Build(build),
        OperatorAttentionTarget::Agent(agent),
        OperatorAttentionTarget::AgentPool(pool),
      ],
      false,
      None,
      None,
    );
    let page = management_query(
      &handlers,
      ListOperatorAttentionQuery::try_new(targets, None, 10).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
      page.items.iter().map(|item| item.category).collect::<Vec<_>>(),
      vec![
        OperatorAttentionCategory::AgentPool,
        OperatorAttentionCategory::Agent,
        OperatorAttentionCategory::Build,
      ]
    );
    assert!(page.items.iter().all(|item| item.target.is_some()));

    let critical = management_query(
      &handlers,
      ListOperatorAttentionQuery::try_new(scope([], true, Some(350), Some(500)), None, 10).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(critical.items.len(), 1);
    assert_eq!(critical.items[0].category, OperatorAttentionCategory::CriticalSystem);
    assert_eq!(critical.items[0].severity, OperatorAttentionSeverity::Critical);
    assert_eq!(critical.items[0].code, "storage_unavailable");
    assert_eq!(critical.items[0].resolved_at_unix_ms, Some(450));
    assert_eq!(critical.items[0].target, None);
  });
}

#[test]
fn newest_first_cursor_is_stable_for_equal_times_and_bound_to_the_complete_scope() {
  run_ready(async {
    let store = Arc::new(InMemoryOperatorAttentionStore::new());
    let build = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    seed(&store, 10, OperatorAttentionEventKind::BuildFailed(build), 100, None);
    seed(&store, 20, OperatorAttentionEventKind::BuildFailed(build), 100, None);
    let handlers = OperatorAttentionHandlers::new(store);
    let selected = scope([OperatorAttentionTarget::Build(build)], false, Some(0), Some(200));

    let first = management_query(
      &handlers,
      ListOperatorAttentionQuery::try_new(selected.clone(), None, 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(first.items[0].id, attention_id(20));
    let cursor = first.next_cursor.unwrap();
    let encoded = cursor.encode();
    assert_eq!(OperatorAttentionCursor::decode(&encoded).unwrap(), cursor);

    let second = management_query(
      &handlers,
      ListOperatorAttentionQuery::try_new(selected.clone(), Some(cursor.clone()), 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(second.items[0].id, attention_id(10));
    assert_eq!(second.next_cursor, None);

    for changed in [
      scope([], true, Some(0), Some(200)),
      scope([OperatorAttentionTarget::Build(build)], true, Some(0), Some(200)),
      scope([OperatorAttentionTarget::Build(build)], false, Some(1), Some(200)),
      scope([OperatorAttentionTarget::Build(build)], false, Some(0), Some(199)),
    ] {
      assert_eq!(
        ListOperatorAttentionQuery::try_new(changed, Some(cursor.clone()), 1),
        Err(OperatorAttentionInputError::CursorScopeMismatch)
      );
    }
  });
}

#[test]
fn empty_duplicate_oversized_time_page_and_cursor_inputs_are_rejected() {
  assert_eq!(
    OperatorAttentionScopeInput::try_new([], false, None, None),
    Err(OperatorAttentionInputError::InvalidScope)
  );
  let build = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
  assert_eq!(
    OperatorAttentionScopeInput::try_new(
      [
        OperatorAttentionTarget::Build(build),
        OperatorAttentionTarget::Build(build)
      ],
      false,
      None,
      None,
    ),
    Err(OperatorAttentionInputError::InvalidScope)
  );
  let oversized = (1..=MAX_OPERATOR_ATTENTION_SCOPE_TARGETS + 1)
    .map(|value| OperatorAttentionTarget::Build(BuildId::from_uuid(uuid::Uuid::from_u128(value as u128)).unwrap()));
  assert_eq!(
    OperatorAttentionScopeInput::try_new(oversized, false, None, None),
    Err(OperatorAttentionInputError::InvalidScope)
  );
  assert_eq!(
    OperatorAttentionScopeInput::try_new([OperatorAttentionTarget::Build(build)], false, Some(2), Some(1)),
    Err(OperatorAttentionInputError::InvalidTimeRange)
  );
  let selected = scope([OperatorAttentionTarget::Build(build)], false, None, None);
  for limit in [0, MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE + 1] {
    assert_eq!(
      ListOperatorAttentionQuery::try_new(selected.clone(), None, limit),
      Err(OperatorAttentionInputError::InvalidLimit)
    );
  }
  for invalid in ["", "%%%", &"a".repeat(MAX_OPERATOR_ATTENTION_CURSOR_BYTES + 1)] {
    assert_eq!(
      OperatorAttentionCursor::decode(invalid),
      Err(OperatorAttentionCursorError)
    );
  }
}

#[test]
fn visibility_precedes_classification_and_page_derivation_and_ordinary_activity_is_excluded() {
  run_ready(async {
    let store = Arc::new(InMemoryOperatorAttentionStore::new());
    let visible = BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let hidden = BuildId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
    seed(&store, 1, OperatorAttentionEventKind::BuildFailed(hidden), 500, None);
    seed(
      &store,
      2,
      OperatorAttentionEventKind::BuildSucceeded(visible),
      450,
      None,
    );
    seed(&store, 3, OperatorAttentionEventKind::BuildFailed(visible), 400, None);
    seed(&store, 4, OperatorAttentionEventKind::AuditActivity, 350, None);
    seed(&store, 5, OperatorAttentionEventKind::BuildFailed(hidden), 300, None);
    seed(&store, 6, OperatorAttentionEventKind::JobActivity, 250, None);
    seed(&store, 7, OperatorAttentionEventKind::BuildFailed(visible), 200, None);
    let handlers = OperatorAttentionHandlers::new(store);
    let selected = scope(
      [
        OperatorAttentionTarget::Build(visible),
        OperatorAttentionTarget::Build(hidden),
      ],
      false,
      None,
      None,
    );
    let grant = ManagementAuthorizationGrant::new(
      ManagementVisibility::restricted([resource(ManagementResourceKind::Build, visible)]).unwrap(),
    );

    let first = management_query_with_grant(
      &handlers,
      ListOperatorAttentionQuery::try_new(selected.clone(), None, 1).unwrap(),
      grant.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
      first
        .items
        .iter()
        .map(|item| item.occurred_at_unix_ms)
        .collect::<Vec<_>>(),
      [400]
    );
    let second = management_query_with_grant(
      &handlers,
      ListOperatorAttentionQuery::try_new(selected, first.next_cursor, 1).unwrap(),
      grant,
    )
    .await
    .unwrap();
    assert_eq!(
      second
        .items
        .iter()
        .map(|item| item.occurred_at_unix_ms)
        .collect::<Vec<_>>(),
      [200]
    );
    assert_eq!(second.next_cursor, None);
  });
}

#[test]
fn authorization_mapping_preserves_resource_and_critical_visibility_without_widening() {
  run_ready(async {
    let store = Arc::new(InMemoryOperatorAttentionStore::new());
    let visible = AgentId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let hidden = PoolId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
    seed(
      &store,
      1,
      OperatorAttentionEventKind::AgentUnavailable(visible),
      100,
      None,
    );
    seed(
      &store,
      2,
      OperatorAttentionEventKind::AgentPoolUnavailable(hidden),
      200,
      None,
    );
    seed(
      &store,
      3,
      OperatorAttentionEventKind::CriticalSystemCondition {
        code: "database_unavailable".to_owned(),
      },
      300,
      None,
    );
    let handlers = OperatorAttentionHandlers::new(store);
    let query = ListOperatorAttentionQuery::try_new(
      scope(
        [
          OperatorAttentionTarget::Agent(visible),
          OperatorAttentionTarget::AgentPool(hidden),
        ],
        true,
        None,
        None,
      ),
      None,
      10,
    )
    .unwrap();
    assert_eq!(
      ListOperatorAttentionQuery::AUTHORIZATION.action(),
      ManagementAction::View
    );
    assert_eq!(
      ListOperatorAttentionQuery::AUTHORIZATION.resource_kind(),
      ManagementResourceKind::ControlPlane
    );
    assert_eq!(
      query.management_resource().unwrap(),
      ManagementResource::collection(ManagementResourceKind::ControlPlane).unwrap()
    );
    let grant = ManagementAuthorizationGrant::new(
      ManagementVisibility::restricted([
        resource(ManagementResourceKind::Agent, visible),
        ManagementResource::collection(ManagementResourceKind::ControlPlane).unwrap(),
      ])
      .unwrap(),
    );
    let page = management_query_with_grant(&handlers, query, grant).await.unwrap();

    assert_eq!(
      page.items.iter().map(|item| item.category).collect::<Vec<_>>(),
      vec![
        OperatorAttentionCategory::CriticalSystem,
        OperatorAttentionCategory::Agent
      ]
    );
  });
}

fn scope<const N: usize>(
  targets: [OperatorAttentionTarget; N],
  include_critical_conditions: bool,
  occurred_from_unix_ms: Option<i64>,
  occurred_through_unix_ms: Option<i64>,
) -> OperatorAttentionScopeInput {
  OperatorAttentionScopeInput::try_new(
    targets,
    include_critical_conditions,
    occurred_from_unix_ms,
    occurred_through_unix_ms,
  )
  .unwrap()
}

fn seed(
  store: &InMemoryOperatorAttentionStore,
  id: u128,
  kind: OperatorAttentionEventKind,
  occurred_at: i64,
  resolved_at: Option<i64>,
) {
  store
    .seed(
      OperatorAttentionEvent::new(
        attention_id(id),
        kind,
        "Safe operator summary",
        time(occurred_at),
        resolved_at.map(time),
      )
      .unwrap(),
    )
    .unwrap();
}

fn attention_id(value: u128) -> OperatorAttentionId {
  OperatorAttentionId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn resource(kind: ManagementResourceKind, identity: impl ToString) -> ManagementResource {
  ManagementResource::instance(kind, ManagementResourceIdentity::new(identity.to_string()).unwrap()).unwrap()
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = pin!(future);
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(output) => output,
    Poll::Pending => panic!("in-memory operator attention future must complete without yielding"),
  }
}
