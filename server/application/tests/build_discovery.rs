use std::{future::Future, pin::pin, sync::Arc, task::Context, task::Poll, task::Waker};

use octacity_server_application::{
  BuildHandlers, BuildListFilter, ListProjectBuildsQuery, ManagementAuthorizationGrant, ManagementResource,
  ManagementResourceIdentity, ManagementResourceKind, ManagementVisibility, TriggerCauseProjection,
};
use octacity_server_domain::{AttemptNumber, BuildConfigurationVersion, BuildId, ProjectId, Timestamp};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{ProjectBuildSummary, testing::InMemoryBuildDiscoveryStore};
use octacity_server_trigger::TriggerCause;

#[path = "support/management_query.rs"]
mod management_query_support;
use management_query_support::{management_query, management_query_with_grant};

#[test]
fn build_handlers_map_store_pages_and_authorization_visibility() {
  run_ready(async {
    let project_id = id(1);
    let configuration_id = id(2);
    let visible_id = id(3);
    let hidden_id = id(4);
    let failed_id = id(5);
    let store = Arc::new(InMemoryBuildDiscoveryStore::new());
    store.seed_project(project_id).unwrap();
    store.seed_configuration(project_id, configuration_id).unwrap();
    store
      .seed_build(build(visible_id, project_id, configuration_id, BuildState::Running, 20))
      .unwrap();
    store
      .seed_build(build(hidden_id, project_id, configuration_id, BuildState::Running, 10))
      .unwrap();
    store
      .seed_build(build(failed_id, project_id, configuration_id, BuildState::Failed, 5))
      .unwrap();
    let handlers = BuildHandlers::new(store);
    let query = ListProjectBuildsQuery::try_new(
      project_id,
      BuildListFilter {
        configuration_id: Some(configuration_id),
        state: Some(BuildState::Running),
      },
      None,
      1,
    )
    .unwrap();

    let visible = ManagementResource::instance(
      ManagementResourceKind::Build,
      ManagementResourceIdentity::new(visible_id.to_string()).unwrap(),
    )
    .unwrap();
    let page = management_query_with_grant(
      &handlers,
      query.clone(),
      ManagementAuthorizationGrant::new(ManagementVisibility::restricted([visible]).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    let summary = &page.items[0];
    assert_eq!(summary.id, visible_id);
    assert_eq!(summary.project_id, project_id);
    assert_eq!(summary.configuration_id, configuration_id);
    assert_eq!(summary.cause, TriggerCauseProjection::Manual);
    assert_eq!(summary.current_attempt_state, AttemptState::Running);
    assert_eq!(summary.terminal_at, None);
    assert_eq!(page.next_cursor, None);

    let unrestricted = management_query(&handlers, query).await.unwrap();
    assert_eq!(unrestricted.items[0].id, visible_id);
    assert_eq!(
      unrestricted.next_cursor,
      Some(octacity_server_application::BuildPageCursor::new(
        Timestamp::from_unix_millis(20).unwrap(),
        visible_id,
      ))
    );

    let terminal = management_query(
      &handlers,
      ListProjectBuildsQuery::try_new(
        project_id,
        BuildListFilter {
          configuration_id: Some(configuration_id),
          state: Some(BuildState::Failed),
        },
        None,
        10,
      )
      .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(terminal.items[0].id, failed_id);
    assert_eq!(terminal.items[0].current_attempt_state, AttemptState::Failed);
    assert_eq!(terminal.items[0].terminal_at, Timestamp::from_unix_millis(6).ok());
  });
}

fn build(
  build_id: BuildId,
  project_id: ProjectId,
  configuration_id: octacity_server_domain::BuildConfigurationId,
  state: BuildState,
  created_at: i64,
) -> ProjectBuildSummary {
  ProjectBuildSummary {
    id: build_id,
    project_id,
    configuration_id,
    configuration_version: BuildConfigurationVersion::INITIAL,
    cause: TriggerCause::Manual {},
    state,
    created_at: Timestamp::from_unix_millis(created_at).unwrap(),
    current_attempt_id: id(build_id.as_uuid().as_u128() + 1_000),
    current_attempt_number: AttemptNumber::FIRST,
    current_attempt_state: match state {
      BuildState::Queued => AttemptState::Created,
      BuildState::Running => AttemptState::Running,
      BuildState::Succeeded => AttemptState::Succeeded,
      BuildState::Failed => AttemptState::Failed,
      BuildState::Cancelled => AttemptState::Cancelled,
    },
    terminal_at: state
      .is_terminal()
      .then(|| Timestamp::from_unix_millis(created_at + 1).unwrap()),
  }
}

fn id<T>(value: u128) -> T
where
  T: FromUuid,
{
  T::from_uuid(uuid::Uuid::from_u128(value))
}

trait FromUuid {
  fn from_uuid(value: uuid::Uuid) -> Self;
}

macro_rules! impl_from_uuid {
  ($($type:ty),+ $(,)?) => {$(
    impl FromUuid for $type {
      fn from_uuid(value: uuid::Uuid) -> Self {
        <$type>::from_uuid(value).unwrap()
      }
    }
  )+};
}

impl_from_uuid!(
  ProjectId,
  BuildId,
  octacity_server_domain::BuildConfigurationId,
  octacity_server_domain::AttemptId
);

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory Build discovery unexpectedly waited for external I/O"),
  }
}
