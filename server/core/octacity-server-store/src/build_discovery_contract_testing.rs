use octacity_server_domain::{AttemptNumber, BuildConfigurationVersion, EntityKind, ProjectId};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_trigger::TriggerCause;

use crate::test_support::{id, run_ready, time};
use crate::testing::InMemoryBuildDiscoveryStore;
use crate::{
  BuildDiscoveryStore, BuildListVisibility, ListProjectBuilds, ProjectBuildFilter, ProjectBuildSummary, StoreError,
};

/// Verifies Project ownership, filters, newest-first cursors, and visibility-first pagination.
pub fn verify_in_memory_build_discovery_contract() {
  run_ready(
    verify_build_discovery(),
    "in-memory Build discovery must complete without I/O",
  );
}

async fn verify_build_discovery() {
  let store = InMemoryBuildDiscoveryStore::new();
  let project_id = id::<ProjectId>(1);
  let other_project_id = id::<ProjectId>(2);
  let configuration_id = id(10);
  let other_configuration_id = id(11);
  let foreign_configuration_id = id(12);
  store.seed_project(project_id).unwrap();
  store.seed_project(other_project_id).unwrap();
  store.seed_configuration(project_id, configuration_id).unwrap();
  store.seed_configuration(project_id, other_configuration_id).unwrap();
  store
    .seed_configuration(other_project_id, foreign_configuration_id)
    .unwrap();

  for summary in [
    build(107, project_id, configuration_id, BuildState::Running, 30),
    build(106, project_id, configuration_id, BuildState::Running, 30),
    build(105, project_id, other_configuration_id, BuildState::Failed, 25),
    build(104, project_id, configuration_id, BuildState::Succeeded, 20),
    build(103, project_id, configuration_id, BuildState::Running, 15),
    build(102, project_id, configuration_id, BuildState::Running, 10),
    build(201, other_project_id, foreign_configuration_id, BuildState::Running, 40),
  ] {
    store.seed_build(summary).unwrap();
  }

  let visibility = BuildListVisibility::restricted([id(107), id(106), id(105), id(104), id(102), id(201)]).unwrap();
  let first = store
    .list_project_builds(
      ListProjectBuilds::new(project_id, ProjectBuildFilter::default(), None, 2, visibility.clone()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(ids(&first.items), vec![id(107), id(106)]);
  assert_eq!(
    first.next_cursor,
    first.items.last().map(ProjectBuildSummary::page_position)
  );

  // An equal-time insert that sorts before the cursor must not shift or repeat
  // the continuation page.
  store
    .seed_build(build(120, project_id, configuration_id, BuildState::Running, 30))
    .unwrap();
  let second = store
    .list_project_builds(
      ListProjectBuilds::new(
        project_id,
        ProjectBuildFilter::default(),
        first.next_cursor,
        2,
        BuildListVisibility::restricted([id(120), id(107), id(106), id(105), id(104), id(102)]).unwrap(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(ids(&second.items), vec![id(105), id(104)]);
  assert_eq!(second.next_cursor, Some(second.items[1].page_position()));
  let final_page = store
    .list_project_builds(
      ListProjectBuilds::new(
        project_id,
        ProjectBuildFilter::default(),
        second.next_cursor,
        2,
        visibility,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(ids(&final_page.items), vec![id(102)]);
  assert_eq!(final_page.next_cursor, None);

  let filtered = store
    .list_project_builds(
      ListProjectBuilds::new(
        project_id,
        ProjectBuildFilter {
          configuration_id: Some(configuration_id),
          state: Some(BuildState::Running),
        },
        None,
        10,
        BuildListVisibility::all(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(ids(&filtered.items), vec![id(120), id(107), id(106), id(103), id(102)]);
  assert_eq!(filtered.next_cursor, None);

  let hidden = store
    .list_project_builds(
      ListProjectBuilds::new(
        project_id,
        ProjectBuildFilter::default(),
        None,
        1,
        BuildListVisibility::none(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(hidden.items.is_empty());
  assert_eq!(hidden.next_cursor, None);

  for (selected_project, selected_configuration, entity) in [
    (id(999), None, EntityKind::Project),
    (project_id, Some(foreign_configuration_id), EntityKind::Configuration),
  ] {
    let error = store
      .list_project_builds(
        ListProjectBuilds::new(
          selected_project,
          ProjectBuildFilter {
            configuration_id: selected_configuration,
            state: None,
          },
          None,
          1,
          BuildListVisibility::none(),
        )
        .unwrap(),
      )
      .await
      .unwrap_err();
    assert_eq!(error, StoreError::NotFound { entity });
  }
}

fn build(
  value: u64,
  project_id: ProjectId,
  configuration_id: octacity_server_domain::BuildConfigurationId,
  state: BuildState,
  created_at: i64,
) -> ProjectBuildSummary {
  ProjectBuildSummary {
    id: id(value),
    project_id,
    configuration_id,
    configuration_version: BuildConfigurationVersion::INITIAL,
    cause: TriggerCause::Manual {},
    state,
    created_at: time(created_at),
    current_attempt_id: id(value + 1_000),
    current_attempt_number: AttemptNumber::FIRST,
    current_attempt_state: match state {
      BuildState::Queued => AttemptState::Created,
      BuildState::Running => AttemptState::Running,
      BuildState::Succeeded => AttemptState::Succeeded,
      BuildState::Failed => AttemptState::Failed,
      BuildState::Cancelled => AttemptState::Cancelled,
    },
    terminal_at: state.is_terminal().then(|| time(created_at + 1)),
  }
}

fn ids(items: &[ProjectBuildSummary]) -> Vec<octacity_server_domain::BuildId> {
  items.iter().map(|item| item.id).collect()
}
