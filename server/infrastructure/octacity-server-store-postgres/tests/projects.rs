mod support;

use std::{fmt::Debug, str::FromStr, sync::Arc};

use octacity_server_domain::{EntityKind, ProjectId, ProjectName, Timestamp};
use octacity_server_store::testing::management_mutation;
use octacity_server_store::{
  CreateProject, DeleteProject, IdempotencyKey, ListProjects, MoveProject, ProjectListVisibility, ProjectStore as _,
  StoreError,
};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;
use tokio::sync::Barrier;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn visibility_is_applied_before_project_page_boundaries() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let mut projects = Vec::new();
  for value in 101..=107 {
    projects.push(
      create(
        &store,
        value,
        None,
        &format!("visibility-{value}"),
        &format!("visibility-{value}"),
        i64::try_from(value).unwrap(),
      )
      .await,
    );
  }
  let child = create(&store, 108, Some(projects[1].id), "visible-child", "visible-child", 108).await;

  let all = store
    .list_projects(ListProjects::new(None, None, 2, ProjectListVisibility::all()).unwrap())
    .await
    .unwrap();
  assert_eq!(all.projects, projects[..2]);
  assert_eq!(
    all.projects_with_visible_children,
    [projects[1].id].into_iter().collect()
  );
  assert_eq!(all.next_cursor, Some(projects[1].id));

  let restricted = ProjectListVisibility::restricted([projects[1].id, projects[3].id, projects[5].id]).unwrap();
  let first = store
    .list_projects(ListProjects::new(None, None, 2, restricted.clone()).unwrap())
    .await
    .unwrap();
  assert_eq!(first.projects, vec![projects[1].clone(), projects[3].clone()]);
  assert!(first.projects_with_visible_children.is_empty());
  assert_eq!(first.next_cursor, Some(projects[3].id));
  let second = store
    .list_projects(ListProjects::new(None, first.next_cursor, 2, restricted).unwrap())
    .await
    .unwrap();
  assert_eq!(second.projects, vec![projects[5].clone()]);
  assert!(second.projects_with_visible_children.is_empty());
  assert_eq!(second.next_cursor, None);

  let none = store
    .list_projects(ListProjects::new(Some(id(999)), None, 2, ProjectListVisibility::none()).unwrap())
    .await
    .unwrap();
  assert!(none.projects.is_empty());
  assert!(none.projects_with_visible_children.is_empty());
  assert_eq!(none.next_cursor, None);

  let children = store
    .list_projects(ListProjects::new(Some(projects[1].id), None, 2, ProjectListVisibility::all()).unwrap())
    .await
    .unwrap();
  assert_eq!(children.projects, vec![child]);

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_opposite_moves_cannot_create_a_cycle() {
  let database = TestDatabase::migrated().await;
  let result = async {
    let store = Arc::new(PostgresStore::new(database.pool.clone()));
    let left = create(&store, 1, None, "left", "create-left", 1).await;
    let right = create(&store, 2, None, "right", "create-right", 1).await;
    let barrier = Arc::new(Barrier::new(2));

    let left_move = {
      let store = Arc::clone(&store);
      let barrier = Arc::clone(&barrier);
      tokio::spawn(async move {
        barrier.wait().await;
        store
          .move_project(management_mutation(MoveProject {
            id: left.id,
            expected_version: left.version,
            parent_id: Some(right.id),
            idempotency_key: key("move-left-under-right"),
            moved_at: time(2),
          }))
          .await
      })
    };
    let right_move = {
      let store = Arc::clone(&store);
      let barrier = Arc::clone(&barrier);
      tokio::spawn(async move {
        barrier.wait().await;
        store
          .move_project(management_mutation(MoveProject {
            id: right.id,
            expected_version: right.version,
            parent_id: Some(left.id),
            idempotency_key: key("move-right-under-left"),
            moved_at: time(2),
          }))
          .await
      })
    };

    let outcomes = [left_move.await.unwrap(), right_move.await.unwrap()];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
      outcomes
        .iter()
        .filter(|outcome| matches!(
          outcome,
          Err(StoreError::Conflict {
            entity: EntityKind::Project
          })
        ))
        .count(),
      1
    );
    let left_details = store.project(left.id).await?;
    let right_details = store.project(right.id).await?;
    assert!(left_details.ancestors.len() <= 1);
    assert!(right_details.ancestors.len() <= 1);
    Ok::<(), StoreError>(())
  }
  .await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn sibling_name_race_has_one_winner() {
  let database = TestDatabase::migrated().await;
  let result = async {
    let store = Arc::new(PostgresStore::new(database.pool.clone()));
    let parent = create(&store, 10, None, "parent", "create-parent", 1).await;
    let barrier = Arc::new(Barrier::new(2));
    let mut tasks = Vec::new();
    for (id_value, key_value) in [(11, "sibling-a"), (12, "sibling-b")] {
      let store = Arc::clone(&store);
      let barrier = Arc::clone(&barrier);
      tasks.push(tokio::spawn(async move {
        barrier.wait().await;
        store
          .create_project(management_mutation(CreateProject {
            id: id(id_value),
            parent_id: Some(parent.id),
            name: ProjectName::new("same-name").unwrap(),
            idempotency_key: key(key_value),
            created_at: time(2),
          }))
          .await
      }));
    }
    let first = tasks.remove(0).await.unwrap();
    let second = tasks.remove(0).await.unwrap();
    assert_eq!(
      [&first, &second].into_iter().filter(|outcome| outcome.is_ok()).count(),
      1
    );
    assert_eq!(
      [&first, &second]
        .into_iter()
        .filter(|outcome| matches!(
          outcome,
          Err(StoreError::Conflict {
            entity: EntityKind::Project
          })
        ))
        .count(),
      1
    );
    Ok::<(), StoreError>(())
  }
  .await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn non_hierarchy_reference_guards_deletion() {
  let database = TestDatabase::migrated().await;
  let result = async {
    let store = PostgresStore::new(database.pool.clone());
    let project = create(&store, 20, None, "referenced", "create-referenced", 1).await;
    sqlx::query(
      "INSERT INTO project_policy_versions (project_id, version, policy, created_at) VALUES ($1, 1, '{}', now())",
    )
    .bind(project.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();

    assert_eq!(
      store
        .delete_project(management_mutation(DeleteProject {
          id: project.id,
          expected_version: project.version,
          idempotency_key: key("delete-referenced"),
          deleted_at: time(2),
        }))
        .await
        .unwrap_err(),
      StoreError::Conflict {
        entity: EntityKind::Project
      }
    );
    assert_eq!(store.project(project.id).await?.project, project);
    Ok::<(), StoreError>(())
  }
  .await;
  database.cleanup().await;
  result.unwrap();
}

async fn create(
  store: &PostgresStore,
  id_value: u64,
  parent_id: Option<ProjectId>,
  name: &str,
  key_value: &str,
  at: i64,
) -> octacity_server_store::Project {
  store
    .create_project(management_mutation(CreateProject {
      id: id(id_value),
      parent_id,
      name: ProjectName::new(name).unwrap(),
      idempotency_key: key(key_value),
      created_at: time(at),
    }))
    .await
    .unwrap()
    .project
}

fn id<T>(value: u64) -> T
where
  T: FromStr,
  T::Err: Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
