//! Owner-scoped document persistence through public Issue and input journal ports.
use super::*;
use octacity_server_domain::ArtifactId;
use octacity_server_store::*;

fn request(project: ProjectId, issue: IssueId, replay: &str, content: &str, now: i64) -> ReserveProjectDocument {
  IssueDocumentSubmission {
    project_id: project,
    issue_id: issue,
    name: "Investigation notes".parse().unwrap(),
    content: content.into(),
    idempotency_key: key(replay),
  }
  .into_reservation(at(now))
  .unwrap()
}
fn page(project: ProjectId, issue: IssueId, now: i64) -> ListIssueDocuments {
  ListIssueDocuments {
    issue_id: issue,
    page: ListProjectDocuments {
      project_id: project,
      after: None,
      limit: 10,
      visibility: ArtifactListVisibility::all(),
      observed_at: at(now),
    },
  }
}

async fn publish(store: &PostgresStore, command: ReserveProjectDocument, now: i64) -> IssueDocumentResource {
  let reserved = store
    .reserve_project_document(management_mutation(command))
    .await
    .unwrap();
  store
    .publish_project_document(management_mutation(reserved.guard(at(now))))
    .await
    .unwrap()
    .issue_resource()
    .unwrap()
}

fn authored<T>(command: T, actor: &str, scope: &str) -> ManagementMutation<T> {
  ManagementMutation::new(
    command,
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some(actor.into()),
      },
      ManagementSecurityScope::new(scope).unwrap(),
      "issue-document-contract",
    )
    .unwrap(),
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_cannot_be_substituted_as_generic_command_execution_inputs() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let private_issue = store
    .create_issue(management_mutation(draft(project.id, "private")))
    .await
    .unwrap()
    .issue;
  let other = store
    .create_issue(management_mutation(draft(project.id, "other")))
    .await
    .unwrap()
    .issue;
  let private = publish(
    &store,
    request(
      project.id,
      private_issue.id,
      "private-document",
      "Private task body",
      10,
    ),
    11,
  )
  .await;
  let shared = store
    .reserve_project_document(management_mutation(ReserveProjectDocument {
      issue_id: None,
      selection_digest: None,
      now: at(10),
      submission: ProjectDocumentSubmission {
        project_id: project.id,
        name: "Shared context".parse().unwrap(),
        content: "Shared task body".into(),
        purpose: Default::default(),
        idempotency_key: key("shared-document"),
      },
    }))
    .await
    .unwrap();
  let shared = store
    .publish_project_document(management_mutation(shared.guard(at(11))))
    .await
    .unwrap()
    .resource()
    .unwrap();
  // This is the exact reader used when a command mounts caller-selected Work task/acceptance Artifacts.
  assert!(
    matches!(
      store.published_execution_input(private.artifact.artifact_id()).await,
      Err(StoreError::NotFound {
        entity: EntityKind::Artifact
      })
    ),
    "Issue owner must be checked before generic command input resolution"
  );
  assert_eq!(
    store
      .published_execution_input(shared.artifact.artifact_id())
      .await
      .unwrap()
      .artifact_id,
    shared.artifact.artifact_id()
  );
  assert!(
    store
      .project_input_artifact(project.id, private.artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .issue_document_input(project.id, other.id, private.artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  let owned = store
    .issue_document_input(project.id, private_issue.id, private.artifact.artifact_id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(owned.object.artifact, private.artifact);
  assert_eq!(
    store.issue(project.id, private_issue.id, None).await.unwrap(),
    private_issue
  );
  assert_eq!(store.issue(project.id, other.id, None).await.unwrap(), other);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_round_trip_keeps_exact_owner_and_excludes_project_catalogs() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let original = store
    .create_issue(management_mutation(draft(project.id, "issue")))
    .await
    .unwrap()
    .issue;
  let other = store
    .create_issue(management_mutation(draft(project.id, "other-issue")))
    .await
    .unwrap()
    .issue;
  let command = request(project.id, original.id, "notes", "Exact task text\r\n", 10);
  let reserved = store
    .reserve_project_document(management_mutation(command.clone()))
    .await
    .unwrap();
  let artifact = reserved.objects[0].artifact.clone();
  assert!(
    store
      .issue_document_metadata(project.id, original.id, artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  let published = store
    .publish_project_document(management_mutation(reserved.guard(at(11))))
    .await
    .unwrap();
  let document = published.issue_resource().unwrap();
  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  assert_eq!(
    restarted
      .issue_document_metadata(project.id, original.id, artifact.artifact_id())
      .await
      .unwrap(),
    Some(document.clone())
  );
  assert_eq!(
    restarted
      .list_issue_documents(page(project.id, original.id, 12))
      .await
      .unwrap()
      .items,
    vec![document.clone()]
  );
  let input = restarted
    .issue_document_input(project.id, original.id, artifact.artifact_id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(input.object.artifact, artifact);
  assert_eq!(input.mapping.as_str(), ISSUE_DOCUMENT_MAPPING);
  assert_eq!(input.expires_at, reserved.expires_at);
  assert_eq!(document.actor.kind, AuditActorKind::UnauthenticatedManagement);
  assert_eq!(
    restarted
      .project_document_content(
        project.id,
        &ManagementSecurityScope::trusted_network(),
        &command.submission.idempotency_key
      )
      .await
      .unwrap()
      .as_deref(),
    Some("Exact task text\r\n")
  );
  assert!(
    restarted
      .issue_document_metadata(project.id, other.id, artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .issue_document_metadata(project.id, original.id, ArtifactId::generate())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .issue_document_input(project.id, other.id, artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .list_issue_documents(page(project.id, other.id, 12))
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert!(
    restarted
      .project_document_metadata(project.id, artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .project_input_artifact(project.id, artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .list_project_documents(page(project.id, original.id, 12).page)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_eq!(restarted.issue(project.id, original.id, None).await.unwrap(), original);
  assert_eq!(
    restarted
      .reserve_project_document(management_mutation(command))
      .await
      .unwrap()
      .issue_resource()
      .unwrap(),
    document
  );
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_reject_unknown_or_foreign_parent_without_partial_reservations() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let foreign = CreateProject {
    id: ProjectId::generate(),
    name: "Foreign documents".parse().unwrap(),
    idempotency_key: key("foreign-project"),
    ..project.clone()
  };
  store
    .create_project(management_mutation(foreign.clone()))
    .await
    .unwrap();
  let foreign_issue = store
    .create_issue(management_mutation(draft(foreign.id, "foreign-issue")))
    .await
    .unwrap()
    .issue;
  for issue in [IssueId::generate(), foreign_issue.id] {
    let command = request(project.id, issue, "invalid-owner", "Private text", 10);
    assert!(
      matches!(
        store
          .reserve_project_document(management_mutation(command.clone()))
          .await,
        Err(StoreError::NotFound {
          entity: EntityKind::Issue
        })
      ),
      "a Project cannot acquire an absent or foreign Issue owner"
    );
    assert!(
      store
        .project_document_preparation(
          project.id,
          &ManagementSecurityScope::trusted_network(),
          &command.submission.idempotency_key
        )
        .await
        .unwrap()
        .is_none(),
      "rejection must allocate no publication identity"
    );
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_upgrade_preserves_shared_publications_and_empty_legacy_issues() {
  use octacity_server_store_postgres::{MIGRATOR, migrate};
  let mut database = TestDatabase::empty().await;
  // Last schema before durable Issue document ownership; existing documents are Project-owned.
  MIGRATOR.run_to(119, &database.pool).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let create = draft(project.id, "legacy-issue");
  let issue = store
    .create_issue(management_mutation(create.clone()))
    .await
    .unwrap()
    .issue;
  let command = ReserveProjectDocument {
    issue_id: None,
    selection_digest: None,
    now: at(10),
    submission: ProjectDocumentSubmission {
      project_id: project.id,
      name: "Shared rules".parse().unwrap(),
      purpose: Default::default(),
      content: "Original Project rules\r\n".into(),
      idempotency_key: key("shared-rules"),
    },
  };
  let reserved = store
    .reserve_project_document(management_mutation(command.clone()))
    .await
    .unwrap();
  let published = store
    .publish_project_document(management_mutation(reserved.guard(at(11))))
    .await
    .unwrap();
  let shared = published.resource().unwrap();
  assert!(published.intent.issue.is_none());
  let snapshot = super::snapshot::DatabaseSnapshot::capture(&mut database).await;
  let restored = snapshot.restore().await;
  for copy in [&database, &restored] {
    migrate(&copy.pool).await.unwrap();
    migrate(&copy.pool).await.unwrap();
    let upgraded = PostgresStore::new(copy.pool.clone());
    assert_eq!(
      upgraded
        .project_document_metadata(project.id, shared.artifact.artifact_id())
        .await
        .unwrap(),
      Some(shared.clone())
    );
    assert_eq!(
      upgraded
        .project_document_content(
          project.id,
          &ManagementSecurityScope::trusted_network(),
          &command.submission.idempotency_key
        )
        .await
        .unwrap()
        .as_deref(),
      Some("Original Project rules\r\n")
    );
    assert_eq!(
      upgraded
        .list_project_documents(page(project.id, issue.id, 12).page)
        .await
        .unwrap()
        .items,
      vec![shared.clone()]
    );
    assert!(
      upgraded
        .list_issue_documents(page(project.id, issue.id, 12))
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(
      upgraded
        .issue_document_metadata(project.id, issue.id, shared.artifact.artifact_id())
        .await
        .unwrap()
        .is_none()
    );
    let replay = upgraded
      .reserve_project_document(management_mutation(command.clone()))
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(replay).unwrap(),
      serde_json::to_value(&published).unwrap()
    );
    assert_eq!(
      upgraded
        .create_issue(management_mutation(create.clone()))
        .await
        .unwrap()
        .issue,
      issue
    );
    let next = upgraded
      .reserve_project_document(management_mutation(request(
        project.id,
        issue.id,
        "task-notes",
        "Task-specific notes",
        20,
      )))
      .await
      .unwrap();
    let attached = upgraded
      .publish_project_document(management_mutation(next.guard(at(21))))
      .await
      .unwrap()
      .issue_resource()
      .unwrap();
    assert_eq!(attached.issue_id, issue.id);
    assert_eq!(
      upgraded
        .list_project_documents(page(project.id, issue.id, 22).page)
        .await
        .unwrap()
        .items,
      vec![shared.clone()]
    );
    assert_eq!(upgraded.issue(project.id, issue.id, None).await.unwrap(), issue);
  }
  restored.cleanup().await;
  database.cleanup().await;
  snapshot.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_recovery_retains_original_objects_actor_and_cleanup_tombstones() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "issue")))
    .await
    .unwrap()
    .issue;
  let scope = ManagementSecurityScope::new("operator:team").unwrap();
  let command = request(project.id, issue.id, "recover", "Original private notes\r\n", 10);
  let reserved = store
    .reserve_project_document(authored(command.clone(), "alice", scope.as_str()))
    .await
    .unwrap();
  store.release_project_document(reserved.guard(at(11))).await.unwrap();
  let pool = sqlx::PgPool::connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(pool.clone());
  assert_eq!(
    restarted
      .project_document_content(project.id, &scope, &command.submission.idempotency_key)
      .await
      .unwrap()
      .as_deref(),
    Some("Original private notes\r\n")
  );
  assert!(
    restarted
      .project_document_content(
        project.id,
        &ManagementSecurityScope::trusted_network(),
        &command.submission.idempotency_key
      )
      .await
      .unwrap()
      .is_none()
  );
  let mut retry = command.clone();
  retry.now = at(50);
  let reclaimed = restarted
    .reserve_project_document(authored(retry.clone(), "bob", scope.as_str()))
    .await
    .unwrap();
  assert_eq!(reclaimed.objects, reserved.objects);
  assert_eq!(reclaimed.id, reserved.id);
  assert_eq!(reclaimed.generation, reserved.generation + 1);
  assert_eq!(reclaimed.expires_at, reserved.expires_at);
  assert_eq!(
    reclaimed.intent.issue.as_ref().unwrap().actor.identity.as_deref(),
    Some("alice")
  );
  assert!(
    restarted
      .publish_project_document(authored(reserved.guard(at(51)), "alice", scope.as_str()))
      .await
      .is_err()
  );
  let published = restarted
    .publish_project_document(authored(reclaimed.guard(at(51)), "bob", scope.as_str()))
    .await
    .unwrap();
  let document = published.issue_resource().unwrap();
  let newer = publish(
    &restarted,
    request(project.id, issue.id, "new-version", "New private notes", 60),
    61,
  )
  .await;
  assert_ne!(newer.artifact, document.artifact);
  assert_eq!(
    restarted
      .reserve_project_document(authored(retry.clone(), "bob", scope.as_str()))
      .await
      .unwrap()
      .issue_resource()
      .unwrap(),
    document
  );
  let due = restarted
    .expired_factory_execution_inputs(document.expires_at, 10)
    .await
    .unwrap();
  let expired = due
    .into_iter()
    .find(|object| object.artifact_id == document.artifact.artifact_id())
    .unwrap();
  restarted
    .complete_factory_execution_input_deletion(CompleteFactoryExecutionInputDeletion {
      expected: expired,
      deleted_at: document.expires_at,
    })
    .await
    .unwrap();
  assert!(
    restarted
      .issue_document_input(project.id, issue.id, document.artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(
    restarted
      .issue_document_metadata(project.id, issue.id, document.artifact.artifact_id())
      .await
      .unwrap(),
    Some(document.clone())
  );
  assert!(
    restarted
      .project_document_content(project.id, &scope, &command.submission.idempotency_key)
      .await
      .unwrap()
      .is_none()
  );
  retry.now = document.expires_at;
  assert_eq!(
    restarted
      .reserve_project_document(authored(retry, "bob", scope.as_str()))
      .await
      .unwrap()
      .issue_resource()
      .unwrap(),
    document
  );
  assert!(
    restarted
      .project_document_content(project.id, &scope, &command.submission.idempotency_key)
      .await
      .unwrap()
      .is_none(),
    "replay cannot restore expired private bytes"
  );
  assert_eq!(
    restarted
      .list_issue_documents(page(project.id, issue.id, newer.published_at.unix_millis()))
      .await
      .unwrap()
      .items,
    vec![newer]
  );
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_competing_publications_keep_one_receipt_and_distinct_versions() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "issue")))
    .await
    .unwrap()
    .issue;
  let left = request(project.id, issue.id, "competing", "Left proposed text", 10);
  let right = request(project.id, issue.id, "competing", "Right proposed text", 10);
  let (a, b) = tokio::join!(
    store.reserve_project_document(management_mutation(left.clone())),
    store.reserve_project_document(management_mutation(right.clone()))
  );
  assert_ne!(a.is_ok(), b.is_ok());
  let (winner, loser, reserved) = match (a, b) {
    (Ok(record), Err(_)) => (left, right, record),
    (Err(_), Ok(record)) => (right, left, record),
    _ => unreachable!(),
  };
  let original = store
    .publish_project_document(management_mutation(reserved.guard(at(11))))
    .await
    .unwrap()
    .issue_resource()
    .unwrap();
  let (second, third) = tokio::join!(
    publish(
      &store,
      request(project.id, issue.id, "version-two", "Second immutable version", 20),
      21
    ),
    publish(
      &store,
      request(project.id, issue.id, "version-three", "Third immutable version", 20),
      21
    ),
  );
  let restarted = PostgresStore::new(database.pool.clone());
  let list = restarted
    .list_issue_documents(page(project.id, issue.id, 22))
    .await
    .unwrap();
  assert_eq!(list.items.len(), 3);
  for document in [&original, &second, &third] {
    assert!(list.items.contains(document));
  }
  assert_eq!(
    restarted
      .reserve_project_document(management_mutation(winner.clone()))
      .await
      .unwrap()
      .issue_resource()
      .unwrap(),
    original
  );
  assert!(
    restarted
      .reserve_project_document(management_mutation(loser))
      .await
      .is_err()
  );
  assert_eq!(
    restarted
      .project_document_content(
        project.id,
        &ManagementSecurityScope::trusted_network(),
        &winner.submission.idempotency_key
      )
      .await
      .unwrap(),
    Some(winner.submission.content.clone())
  );
  let separate = restarted
    .reserve_project_document(authored(winner.clone(), "charlie", "operator:other"))
    .await
    .unwrap();
  let separate = restarted
    .publish_project_document(authored(separate.guard(at(23)), "charlie", "operator:other"))
    .await
    .unwrap()
    .issue_resource()
    .unwrap();
  assert_ne!(
    separate.artifact, original.artifact,
    "another security partition cannot recover the first scope's operation"
  );
  assert_eq!(separate.actor.identity.as_deref(), Some("charlie"));
  assert_eq!(
    restarted
      .reserve_project_document(management_mutation(winner))
      .await
      .unwrap()
      .issue_resource()
      .unwrap(),
    original
  );
  assert!(
    restarted
      .list_project_documents(page(project.id, issue.id, 22).page)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_documents_apply_owner_visibility_and_expiry_before_page_boundaries() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "issue")))
    .await
    .unwrap()
    .issue;
  let other = store
    .create_issue(management_mutation(draft(project.id, "other-issue")))
    .await
    .unwrap()
    .issue;
  let mut documents = vec![];
  for replay in ["one", "two", "three"] {
    documents.push(publish(&store, request(project.id, issue.id, replay, replay, 10), 11).await);
  }
  publish(
    &store,
    request(project.id, other.id, "other", "Other Issue text", 10),
    11,
  )
  .await;
  let visible = documents
    .into_iter()
    .max_by_key(|doc| doc.artifact.artifact_id())
    .unwrap();
  let mut query = page(project.id, issue.id, 12);
  query.page.limit = 1;
  query.page.visibility = ArtifactListVisibility::restricted([visible.artifact.artifact_id()]).unwrap();
  let result = store.list_issue_documents(query.clone()).await.unwrap();
  assert_eq!(result.items, vec![visible.clone()]);
  assert_eq!(result.next_cursor, None, "hidden documents must not consume a page");
  query.page.after = Some(visible.artifact.artifact_id());
  assert!(
    store
      .list_issue_documents(query.clone())
      .await
      .unwrap()
      .items
      .is_empty()
  );
  query.page.after = None;
  query.page.visibility = ArtifactListVisibility::none();
  assert!(
    store
      .list_issue_documents(query.clone())
      .await
      .unwrap()
      .items
      .is_empty()
  );
  query.page.visibility = ArtifactListVisibility::all();
  query.page.observed_at = visible.expires_at;
  assert!(
    store
      .list_issue_documents(query.clone())
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_eq!(
    store
      .issue_document_metadata(project.id, issue.id, visible.artifact.artifact_id())
      .await
      .unwrap(),
    Some(visible)
  );
  query.page.limit = 0;
  assert!(store.list_issue_documents(query).await.is_err());
  database.cleanup().await;
}
