use std::sync::Arc;

use octacity_server_domain::{EntityKind, ProjectId, ProjectName, ProjectVersion};

use crate::test_support::{id, run_ready, time};
use crate::testing::{
  InMemoryProjectStore, ManagementAuditProbe, MutationEvidenceProbe, RecordedManagementAuditFact,
  assert_management_audit_facts, expected_management_audit, management_mutation,
};
use crate::{
  AuditActor, AuditActorKind, CreateProject, DeleteProject, IdempotencyKey, ListProjects, ManagementMutation,
  ManagementSecurityScope, MoveProject, MutationAuditContext, MutationDisposition, ProjectStore, RenameProject,
  StoreError, StoreOperation,
};

const DEEP_TREE_DEPTH: u64 = 96;

/// Runs the reusable hierarchical Project contract against one empty adapter.
pub async fn verify_project_store_contract<S, P>(store: Arc<S>, evidence: Arc<P>)
where
  S: ProjectStore + 'static,
  P: ManagementAuditProbe + MutationEvidenceProbe + 'static,
{
  let mut expected_audit = Vec::new();
  let root_id = id::<ProjectId>(1);
  let root = store
    .create_project(create(1, None, "root", "create-root", 10))
    .await
    .unwrap();
  assert_eq!(root.disposition, MutationDisposition::Applied);
  assert_eq!(root.project.id, root_id);
  assert_eq!(root.project.version, ProjectVersion::INITIAL);
  expected_audit.push(expected_management_audit(
    StoreOperation::CreateProject,
    EntityKind::Project,
    root_id,
  ));

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
  expected_audit.push(expected_management_audit(
    StoreOperation::CreateProject,
    EntityKind::Project,
    child.id,
  ));
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
  let sibling = store
    .create_project(create(3, None, "child", "same-name-other-parent", 22))
    .await
    .unwrap()
    .project;
  expected_audit.push(expected_management_audit(
    StoreOperation::CreateProject,
    EntityKind::Project,
    sibling.id,
  ));

  let renamed = store
    .rename_project(management_mutation(RenameProject {
      id: child.id,
      expected_version: child.version,
      name: ProjectName::new("renamed").unwrap(),
      idempotency_key: key("rename-child"),
      renamed_at: time(30),
    }))
    .await
    .unwrap();
  assert_eq!(renamed.project.id, child.id, "rename must preserve stable identity");
  assert_eq!(renamed.project.version.get(), 2);
  expected_audit.push(expected_management_audit(
    StoreOperation::RenameProject,
    EntityKind::Project,
    child.id,
  ));
  assert_eq!(
    store
      .rename_project(management_mutation(RenameProject {
        id: child.id,
        expected_version: child.version,
        name: ProjectName::new("stale").unwrap(),
        idempotency_key: key("stale-rename"),
        renamed_at: time(31),
      }))
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
    expected_audit.push(expected_management_audit(
      StoreOperation::CreateProject,
      EntityKind::Project,
      created.id,
    ));
  }
  let deep = store.project(parent_id).await.unwrap();
  assert_eq!(deep.ancestors.len(), usize::try_from(DEEP_TREE_DEPTH).unwrap() + 1);
  assert_eq!(deep.ancestors.first().map(|project| project.id), Some(root_id));

  assert_eq!(
    store
      .move_project(management_mutation(MoveProject {
        id: root_id,
        expected_version: root.project.version,
        parent_id: Some(parent_id),
        idempotency_key: key("cycle"),
        moved_at: time(300),
      }))
      .await
      .unwrap_err(),
    conflict(),
    "a deep descendant must not become an ancestor"
  );

  let moved = store
    .move_project(management_mutation(MoveProject {
      id: parent_id,
      expected_version: parent_version,
      parent_id: None,
      idempotency_key: key("move-deep-leaf"),
      moved_at: time(301),
    }))
    .await
    .unwrap();
  assert_eq!(moved.project.id, parent_id, "move must preserve stable identity");
  assert!(store.project(parent_id).await.unwrap().ancestors.is_empty());
  expected_audit.push(expected_management_audit(
    StoreOperation::MoveProject,
    EntityKind::Project,
    parent_id,
  ));

  assert_eq!(
    store
      .delete_project(management_mutation(DeleteProject {
        id: root_id,
        expected_version: root.project.version,
        idempotency_key: key("delete-referenced-root"),
        deleted_at: time(400),
      }))
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
  expected_audit.push(expected_management_audit(
    StoreOperation::CreateProject,
    EntityKind::Project,
    disposable.id,
  ));
  let deleted = store
    .delete_project(management_mutation(DeleteProject {
      id: disposable.id,
      expected_version: disposable.version,
      idempotency_key: key("delete-disposable"),
      deleted_at: time(501),
    }))
    .await
    .unwrap();
  assert_eq!(deleted.disposition, MutationDisposition::Applied);
  expected_audit.push(expected_management_audit(
    StoreOperation::DeleteProject,
    EntityKind::Project,
    disposable.id,
  ));
  let deleted_replay = store
    .delete_project(management_mutation(DeleteProject {
      id: disposable.id,
      expected_version: disposable.version,
      idempotency_key: key("delete-disposable"),
      deleted_at: time(999),
    }))
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
  assert_management_audit_facts(evidence.as_ref(), expected_audit).await;
}

/// Verifies that caller keys are replayed only inside their accepted security
/// scope while exact replays remain stable within each scope.
pub async fn verify_security_scoped_project_replay<S, P>(store: Arc<S>, evidence: Arc<P>)
where
  S: ProjectStore + 'static,
  P: ManagementAuditProbe + MutationEvidenceProbe + 'static,
{
  let left = scoped_create(991, "scope-left", "shared-key", "operator:left", "request-left");
  let right = scoped_create(992, "scope-right", "shared-key", "operator:right", "request-right");

  let left_outcome = store.create_project(left.clone()).await.unwrap();
  let right_outcome = store.create_project(right.clone()).await.unwrap();
  assert_eq!(left_outcome.disposition, MutationDisposition::Applied);
  assert_eq!(right_outcome.disposition, MutationDisposition::Applied);
  assert_ne!(left_outcome.project.id, right_outcome.project.id);
  assert_eq!(
    store.create_project(left).await.unwrap().disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    store.create_project(right).await.unwrap().disposition,
    MutationDisposition::Replayed
  );

  assert_eq!(
    evidence.mutation_evidence_counts().await,
    crate::testing::MutationEvidenceCounts {
      idempotency: 2,
      audit: 2,
      outbox: 2,
    }
  );
}

/// Runs the Project contract against the deterministic in-memory adapter.
pub fn verify_in_memory_project_store_contract() {
  let store = Arc::new(InMemoryProjectStore::new());
  run_ready(
    async move {
      verify_project_store_contract(Arc::clone(&store), store).await;
      verify_actor_faithful_replay().await;
      let scoped = Arc::new(InMemoryProjectStore::new());
      verify_security_scoped_project_replay(Arc::clone(&scoped), scoped).await;
    },
    "in-memory Project store operations must complete without I/O",
  );
}

fn scoped_create(
  id_value: u64,
  name: &str,
  key_value: &str,
  security_scope: &str,
  request_identity: &str,
) -> ManagementMutation<CreateProject> {
  ManagementMutation::new(
    CreateProject {
      id: id(id_value),
      parent_id: None,
      name: ProjectName::new(name).unwrap(),
      idempotency_key: key(key_value),
      created_at: time(i64::try_from(id_value).unwrap()),
    },
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some(security_scope.to_owned()),
      },
      ManagementSecurityScope::new(security_scope).unwrap(),
      request_identity,
    )
    .unwrap(),
  )
}

async fn verify_actor_faithful_replay() {
  let store = InMemoryProjectStore::new();
  let request = CreateProject {
    id: id::<ProjectId>(990),
    parent_id: None,
    name: ProjectName::new("actor-faithful").unwrap(),
    idempotency_key: key("actor-faithful"),
    created_at: time(990),
  };
  let audit = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("operator-42".to_owned()),
    },
    crate::ManagementSecurityScope::new("operator:42").unwrap(),
    "request-42",
  )
  .unwrap();
  assert_eq!(
    store
      .create_project(ManagementMutation::new(request.clone(), audit.clone()))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Applied
  );
  assert_eq!(
    store
      .create_project(ManagementMutation::new(request, audit))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    store.management_audit_facts().await,
    vec![RecordedManagementAuditFact {
      actor_kind: AuditActorKind::AuthenticatedManagement,
      actor_identity: Some("operator-42".to_owned()),
      operation: StoreOperation::CreateProject,
      target_kind: EntityKind::Project,
      target_identity: id::<ProjectId>(990).to_string(),
      request_identity: "request-42".to_owned(),
    }]
  );
}

fn create(
  id_value: u64,
  parent_id: Option<ProjectId>,
  name: &str,
  key_value: &str,
  at: i64,
) -> ManagementMutation<CreateProject> {
  management_mutation(CreateProject {
    id: id(id_value),
    parent_id,
    name: ProjectName::new(name).unwrap(),
    idempotency_key: key(key_value),
    created_at: time(at),
  })
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Project,
  }
}
