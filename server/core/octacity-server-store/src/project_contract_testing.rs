use std::sync::Arc;

use octacity_server_domain::{EntityKind, ProjectId, ProjectName, ProjectVersion};

use crate::test_support::{id, run_ready, time};
use crate::testing::{InMemoryProjectStore, MutationEvidenceProbe};
use crate::{
  CreateProject, DeleteProject, IdempotencyKey, ListProjects, MoveProject, MutationDisposition, ProjectStore,
  RenameProject, StoreError,
};

const DEEP_TREE_DEPTH: u64 = 96;

/// Runs the reusable hierarchical Project contract against one empty adapter.
pub async fn verify_project_store_contract<S, P>(store: Arc<S>, evidence: Arc<P>)
where
  S: ProjectStore + 'static,
  P: MutationEvidenceProbe + 'static,
{
  let root_id = id::<ProjectId>(1);
  let root = store
    .create_project(create(1, None, "root", "create-root", 10))
    .await
    .unwrap();
  assert_eq!(root.disposition, MutationDisposition::Applied);
  assert_eq!(root.project.id, root_id);
  assert_eq!(root.project.version, ProjectVersion::INITIAL);

  let replay = store
    .create_project(create(1, None, "root", "create-root", 99))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.project, root.project);
  assert_eq!(
    store
      .create_project(create(2, None, "different", "create-root", 99))
      .await
      .unwrap_err(),
    conflict()
  );

  let child = store
    .create_project(create(2, Some(root_id), "child", "create-child", 20))
    .await
    .unwrap()
    .project;
  let details = store.project(child.id).await.unwrap();
  assert_eq!(details.project.id, child.id);
  assert_eq!(details.ancestors.as_slice(), std::slice::from_ref(&root.project));

  let roots = store
    .list_projects(ListProjects::new(None, None, 1).unwrap())
    .await
    .unwrap();
  assert_eq!(roots.projects.as_slice(), std::slice::from_ref(&root.project));
  assert_eq!(roots.next_cursor, None);
  let children = store
    .list_projects(ListProjects::new(Some(root_id), None, 10).unwrap())
    .await
    .unwrap();
  assert_eq!(children.projects.as_slice(), std::slice::from_ref(&child));

  assert_eq!(
    store
      .create_project(create(3, Some(root_id), "child", "duplicate-sibling", 21))
      .await
      .unwrap_err(),
    conflict(),
    "names must be unique only among siblings"
  );
  store
    .create_project(create(3, None, "child", "same-name-other-parent", 22))
    .await
    .unwrap();

  let renamed = store
    .rename_project(RenameProject {
      id: child.id,
      expected_version: child.version,
      name: ProjectName::new("renamed").unwrap(),
      idempotency_key: key("rename-child"),
      renamed_at: time(30),
    })
    .await
    .unwrap();
  assert_eq!(renamed.project.id, child.id, "rename must preserve stable identity");
  assert_eq!(renamed.project.version.get(), 2);
  assert_eq!(
    store
      .rename_project(RenameProject {
        id: child.id,
        expected_version: child.version,
        name: ProjectName::new("stale").unwrap(),
        idempotency_key: key("stale-rename"),
        renamed_at: time(31),
      })
      .await
      .unwrap_err(),
    conflict()
  );

  let mut parent_id = child.id;
  let mut parent_version = renamed.project.version;
  for offset in 0..DEEP_TREE_DEPTH {
    let project_id = 100 + offset;
    let created = store
      .create_project(create(
        project_id,
        Some(parent_id),
        &format!("level-{offset}"),
        &format!("create-level-{offset}"),
        100 + i64::try_from(offset).unwrap(),
      ))
      .await
      .unwrap()
      .project;
    parent_id = created.id;
    parent_version = created.version;
  }
  let deep = store.project(parent_id).await.unwrap();
  assert_eq!(deep.ancestors.len(), usize::try_from(DEEP_TREE_DEPTH).unwrap() + 1);
  assert_eq!(deep.ancestors.first().map(|project| project.id), Some(root_id));

  assert_eq!(
    store
      .move_project(MoveProject {
        id: root_id,
        expected_version: root.project.version,
        parent_id: Some(parent_id),
        idempotency_key: key("cycle"),
        moved_at: time(300),
      })
      .await
      .unwrap_err(),
    conflict(),
    "a deep descendant must not become an ancestor"
  );

  let moved = store
    .move_project(MoveProject {
      id: parent_id,
      expected_version: parent_version,
      parent_id: None,
      idempotency_key: key("move-deep-leaf"),
      moved_at: time(301),
    })
    .await
    .unwrap();
  assert_eq!(moved.project.id, parent_id, "move must preserve stable identity");
  assert!(store.project(parent_id).await.unwrap().ancestors.is_empty());

  assert_eq!(
    store
      .delete_project(DeleteProject {
        id: root_id,
        expected_version: root.project.version,
        idempotency_key: key("delete-referenced-root"),
        deleted_at: time(400),
      })
      .await
      .unwrap_err(),
    conflict(),
    "a Project with an active child reference must be guarded"
  );

  let disposable = store
    .create_project(create(900, None, "disposable", "create-disposable", 500))
    .await
    .unwrap()
    .project;
  let deleted = store
    .delete_project(DeleteProject {
      id: disposable.id,
      expected_version: disposable.version,
      idempotency_key: key("delete-disposable"),
      deleted_at: time(501),
    })
    .await
    .unwrap();
  assert_eq!(deleted.disposition, MutationDisposition::Applied);
  let deleted_replay = store
    .delete_project(DeleteProject {
      id: disposable.id,
      expected_version: disposable.version,
      idempotency_key: key("delete-disposable"),
      deleted_at: time(999),
    })
    .await
    .unwrap();
  assert_eq!(deleted_replay.disposition, MutationDisposition::Replayed);
  assert_eq!(
    store.project(disposable.id).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Project
    }
  );

  let counts = evidence.mutation_evidence_counts().await;
  assert_eq!(counts.idempotency, counts.audit);
  assert_eq!(counts.audit, counts.outbox);
  assert_eq!(counts.idempotency, usize::try_from(DEEP_TREE_DEPTH).unwrap() + 7);
}

/// Runs the Project contract against the deterministic in-memory adapter.
pub fn verify_in_memory_project_store_contract() {
  let store = Arc::new(InMemoryProjectStore::new());
  run_ready(
    verify_project_store_contract(Arc::clone(&store), store),
    "in-memory Project store operations must complete without I/O",
  );
}

fn create(id_value: u64, parent_id: Option<ProjectId>, name: &str, key_value: &str, at: i64) -> CreateProject {
  CreateProject {
    id: id(id_value),
    parent_id,
    name: ProjectName::new(name).unwrap(),
    idempotency_key: key(key_value),
    created_at: time(at),
  }
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Project,
  }
}
