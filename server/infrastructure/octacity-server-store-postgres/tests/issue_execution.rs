mod support;

#[path = "issue_execution/status_actions.rs"]
mod status_actions;

use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
fn key(s: &str) -> IdempotencyKey {
  s.parse().unwrap()
}

async fn issue(store: &impl ProjectStoreIssue) -> IssueRevision {
  let project = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new(format!("Manual {project}")).unwrap(),
      idempotency_key: key(&project.to_string()),
      created_at: at(1),
    }))
    .await
    .unwrap();
  store
    .create_issue(management_mutation(CreateIssue {
      id: IssueId::generate(),
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: "Задача".into(),
          description: "точные\r\nбайты".into(),
          acceptance: Some(" критерии\n".into()),
          priority: None,
        },
      },
      idempotency_key: key(&format!("issue.{project}")),
      created_at: at(2),
    }))
    .await
    .unwrap()
    .issue
}
trait ProjectStoreIssue: ProjectStore + IssueStore {}
impl<T: ProjectStore + IssueStore> ProjectStoreIssue for T {}

fn request(issue: &IssueRevision) -> RequestIssueExecution {
  RequestIssueExecution {
    id: IssueExecutionRequestId::generate(),
    intent: RequestIssueExecutionIntent {
      classification: None,
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_version: issue.version,
      target: IssueExecutionTarget {
        configuration_id: FactoryConfigurationId::generate(),
        configuration_version: FactoryConfigurationVersion::INITIAL,
        repository_id: RepositoryId::generate(),
        repository_version: RepositoryVersion::INITIAL,
        source: RepositorySourceSelection::DefaultReference,
        mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
      },
    },
    idempotency_key: key("execute"),
    requested_at: at(3),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn execution_reservation_is_atomic_scoped_and_replays_original_revision_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let original = issue(&store).await;
  let request = request(&original);
  let created = store
    .request_issue_execution(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(
    created.request.task_digest,
    FactoryDigest::content_sha256("Задача\n\nточные\r\nбайты".as_bytes())
  );
  assert_eq!(
    created.request.acceptance_digest,
    FactoryDigest::content_sha256(" критерии\n".as_bytes())
  );
  assert!(created.request.association.is_none());
  store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: original.project_id,
        id: original.id,
        expected_version: original.version,
        content: IssueContent {
          description: "changed".into(),
          ..original.content.clone()
        },
      },
      idempotency_key: key("edit"),
      edited_at: at(4),
    }))
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let mut retry = request.clone();
  retry.id = IssueExecutionRequestId::generate();
  retry.requested_at = at(10);
  let replay = restarted
    .request_issue_execution(management_mutation(retry))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.request, created.request);
  assert_eq!(
    restarted
      .issue_execution(original.project_id, original.id, request.id)
      .await
      .unwrap(),
    created.request
  );
  assert!(matches!(
    restarted
      .issue_execution(ProjectId::generate(), original.id, request.id)
      .await,
    Err(StoreError::NotFound { .. })
  ));
  assert!(matches!(
    restarted
      .issue_execution_for_admission(
        original.project_id,
        original.id,
        request.id,
        &ManagementSecurityScope::new("another-partition").unwrap()
      )
      .await,
    Err(StoreError::NotFound { .. })
  ));
  let mut stale = request.clone();
  stale.idempotency_key = key("new-stale");
  assert!(matches!(
    restarted.request_issue_execution(management_mutation(stale)).await,
    Err(StoreError::Conflict { .. })
  ));
  let mut changed = request;
  changed.intent.expected_version = IssueVersion::new(2).unwrap();
  assert!(matches!(
    restarted.request_issue_execution(management_mutation(changed)).await,
    Err(StoreError::Conflict { .. })
  ));
  database.cleanup().await;
}

fn submission(issue: &IssueRevision, replay: &str) -> ProjectTextSubmission {
  let r = request(issue);
  ProjectTextSubmission {
    intent: ProjectTextIntent {
      target: ReadFactoryAdmissionContext {
        project_id: issue.project_id,
        configuration_id: r.intent.target.configuration_id,
        configuration_version: r.intent.target.configuration_version,
        repository_id: r.intent.target.repository_id,
        repository_version: r.intent.target.repository_version,
      },
      external_identity: ExternalWorkIdentity::new("manual-project-text").unwrap(),
      source: RepositorySourceSelection::DefaultReference,
      classification: WorkClassification::new(WorkPriority::new(1).unwrap(), RiskClass::Low, Default::default()),
      mapping: FactoryKey::new("plain-text.v1").unwrap(),
      idempotency_key: key(replay),
    },
    documents: ProjectTextDocuments::try_new("Задача\r\n", " критерии\n").unwrap(),
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn project_text_reservation_survives_restart_and_fences_stale_owners_without_renewing_bounds() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let submission = submission(&issue, "text");
  let first = store
    .reserve_project_text(management_mutation(ReserveProjectText {
      submission: submission.clone(),
      now: at(10),
    }))
    .await
    .unwrap();
  assert_eq!(first.objects.len(), 2);
  assert_eq!(
    store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &key("text")
      )
      .await
      .unwrap()
      .unwrap()
      .task(),
    "Задача\r\n".as_bytes()
  );
  assert_eq!(first.preparation_deadline, at(300_010));
  assert_eq!(first.lease_until, at(30_010));
  assert!(
    store
      .reserve_project_text(management_mutation(ReserveProjectText {
        submission: submission.clone(),
        now: at(11)
      }))
      .await
      .is_err()
  );
  store
    .freeze_project_text(first.guard(at(12)), ImmutableRevision::new("original-base").unwrap())
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let retained = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &key("text"),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained.objects, first.objects);
  let next = restarted
    .reserve_project_text(management_mutation(ReserveProjectText {
      submission: submission.clone(),
      now: at(30_010),
    }))
    .await
    .unwrap();
  assert_eq!(next.generation, 2);
  assert_eq!(next.frozen_revision.as_ref().unwrap().as_str(), "original-base");
  assert_eq!(next.preparation_deadline, first.preparation_deadline);
  assert_eq!(next.expires_at, first.expires_at);
  assert!(restarted.publish_project_text(first.guard(at(30_011))).await.is_err());
  assert!(
    restarted
      .freeze_project_text(next.guard(at(30_011)), ImmutableRevision::new("changed-base").unwrap())
      .await
      .is_err()
  );
  assert!(
    restarted
      .project_input_artifact(issue.project_id, first.objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  let published = restarted.publish_project_text(next.guard(at(30_012))).await.unwrap();
  assert_eq!(published.published_at, Some(at(30_012)));
  assert!(
    restarted
      .project_input_artifact(issue.project_id, first.objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_some()
  );
  assert!(
    restarted
      .project_input_artifact(ProjectId::generate(), first.objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  let mut changed = submission.clone();
  changed.documents = ProjectTextDocuments::try_new("changed", "criteria").unwrap();
  assert!(
    restarted
      .reserve_project_text(management_mutation(ReserveProjectText {
        submission: changed,
        now: at(30_013)
      }))
      .await
      .is_err()
  );
  assert!(
    restarted
      .reserve_project_text(management_mutation(ReserveProjectText {
        submission,
        now: first.preparation_deadline
      }))
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn project_text_reservation_commits_one_safe_audit_and_replay_under_concurrency() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let submission = submission(&issue, "concurrent");
  let r = ReserveProjectText {
    submission: submission.clone(),
    now: at(10),
  };
  let (first, second) = tokio::join!(
    store.reserve_project_text(management_mutation(r.clone())),
    store.reserve_project_text(management_mutation(r))
  );
  assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
  let page = store
    .list_audit_facts(ListAuditFacts {
      visibility: AuditFactListVisibility::all(),
      query: AuditFactQuery {
        actor_kind: None,
        actor_identity: None,
        operation: Some("reserve-project-text-input".into()),
        target_kind: None,
        target_identity: None,
        request_identity: None,
        occurred_from: None,
        occurred_through: None,
        after: None,
        limit: 10,
      },
    })
    .await
    .unwrap();
  assert_eq!(page.items.len(), 1);
  let safe = format!("{:?}", page.items);
  assert!(!safe.contains("Задача"));
  assert!(!safe.contains("критерии"));
  let mut request = request(&issue);
  request.idempotency_key = key("concurrent-request");
  let mut other = request.clone();
  other.id = IssueExecutionRequestId::generate();
  let (a, b) = tokio::join!(
    store.request_issue_execution(management_mutation(request)),
    store.request_issue_execution(management_mutation(other))
  );
  let (a, b) = (a.unwrap(), b.unwrap());
  assert_eq!(a.request, b.request);
  assert_ne!(a.disposition, b.disposition);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn preparation_metadata_does_not_expose_private_documents() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let reserved = store
    .reserve_project_text(management_mutation(ReserveProjectText {
      submission: submission(&issue, "private-metadata"),
      now: at(10),
    }))
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let retained = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &key("private-metadata"),
    )
    .await
    .unwrap()
    .unwrap();
  for metadata in [reserved, retained] {
    let document = serde_json::to_value(metadata).unwrap();
    assert!(!document.to_string().contains("Задача"));
    assert!(!document.to_string().contains("критерии"));
    assert!(
      document.pointer("/submission/documents").is_none(),
      "preparation reads must expose metadata without private bodies"
    );
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn separated_documents_upgrade_preserves_pending_bytes_and_retired_receipts() {
  use octacity_server_store_postgres::{MIGRATOR, migrate, run_migrator};
  use std::borrow::Cow;
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator {
    migrations: Cow::Owned(MIGRATOR.iter().filter(|m| m.version <= 73).cloned().collect()),
    ..sqlx::migrate::Migrator::DEFAULT
  };
  run_migrator(&database.pool, &previous).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let mut intents = std::collections::BTreeMap::new();
  for replay in ["pending", "partial", "retired"] {
    let retired = replay == "retired";
    let submitted = submission(&issue, replay);
    intents.insert(replay, submitted.intent.clone());
    let prepared =
      ProjectTextPreparation::reserve(submitted.clone(), ManagementSecurityScope::trusted_network(), at(10)).unwrap();
    // Schema 73 stored bodies under submission, including empty-string retirement.
    let mut document = serde_json::to_value(&prepared).unwrap();
    let mut legacy = document.as_object_mut().unwrap().remove("intent").unwrap();
    legacy["documents"] = if retired {
      serde_json::json!({"task":"","acceptance":""})
    } else {
      serde_json::to_value(&submitted.documents).unwrap()
    };
    document["submission"] = legacy;
    sqlx::query("INSERT INTO project_text_preparations(id,project_id,security_scope,replay_key,document) VALUES($1,$2,'trusted-network',$3,$4)")
      .bind(prepared.id.as_bytes().as_slice()).bind(issue.project_id.as_uuid()).bind(replay)
      .bind(sqlx::types::Json(document)).execute(&database.pool).await.unwrap();
    for (object, role) in prepared.objects.iter().zip(["task", "acceptance"]) {
      let deleted_at =
        (retired || (replay == "partial" && role == "task")).then_some(prepared.expires_at.unix_millis());
      sqlx::query("INSERT INTO project_text_input_objects(artifact_id,preparation_id,upload_id,role,deleted_at_millis,deletion_started_at_millis,recheck_at_millis) VALUES($1,$2,$3,$4,$5,$5,$6)")
        .bind(object.artifact.artifact_id().as_uuid()).bind(prepared.id.as_bytes().as_slice())
        .bind(object.upload_id.as_uuid()).bind(role).bind(deleted_at)
        .bind(deleted_at.map(|at| at + 3_600_000))
        .execute(&database.pool).await.unwrap();
    }
  }
  migrate(&database.pool).await.unwrap();
  migrate(&database.pool).await.unwrap();
  let scope = ManagementSecurityScope::trusted_network();
  for replay in ["pending", "partial"] {
    assert_eq!(
      store
        .project_text_documents(issue.project_id, &scope, &key(replay))
        .await
        .unwrap()
        .unwrap(),
      ProjectTextDocuments::try_new("Задача\r\n", " критерии\n").unwrap()
    );
    let retained = store
      .project_text_preparation(issue.project_id, &scope, &key(replay))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(retained.intent, intents[replay]);
    let due: i64 = sqlx::query_scalar("SELECT cleanup_at_millis FROM project_text_preparations WHERE id=$1")
      .bind(retained.id.as_bytes().as_slice())
      .fetch_one(&database.pool)
      .await
      .unwrap();
    assert_eq!(due, retained.expires_at.unix_millis());
  }
  assert!(
    store
      .project_text_documents(issue.project_id, &scope, &key("retired"))
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .project_text_documents(ProjectId::generate(), &scope, &key("pending"))
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::new("other").unwrap(),
        &key("pending")
      )
      .await
      .unwrap()
      .is_none()
  );
  let retired = store
    .project_text_preparation(issue.project_id, &scope, &key("retired"))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retired.intent.idempotency_key, key("retired"));
  assert_eq!(retired.objects.len(), 2);
  let due: i64 = sqlx::query_scalar("SELECT cleanup_at_millis FROM project_text_preparations WHERE id=$1")
    .bind(retired.id.as_bytes().as_slice())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(due, retired.expires_at.unix_millis() + 3_600_000);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn cleanup_schedule_is_indexed_and_tracks_original_expiry_and_rechecks() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let reserved = store
    .reserve_project_text(management_mutation(ReserveProjectText {
      submission: submission(&issue, "cleanup-schedule"),
      now: at(10),
    }))
    .await
    .unwrap();
  let due: i64 = sqlx::query_scalar("SELECT cleanup_at_millis FROM project_text_preparations WHERE id=$1")
    .bind(reserved.id.as_bytes().as_slice())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(due, reserved.expires_at.unix_millis());
  let mut tx = database.pool.begin().await.unwrap();
  // Check index eligibility independently of this tiny fixture's cost estimates.
  sqlx::query("SET LOCAL enable_seqscan=off")
    .execute(&mut *tx)
    .await
    .unwrap();
  let plan: serde_json::Value = sqlx::query_scalar("EXPLAIN (FORMAT JSON) SELECT id FROM project_text_preparations WHERE cleanup_at_millis <= 1 ORDER BY cleanup_at_millis,id LIMIT 1")
    .fetch_one(&mut *tx).await.unwrap();
  assert!(
    plan.to_string().contains("project_text_cleanup_due"),
    "cleanup must have an ordered due-time index: {plan}"
  );
  tx.rollback().await.unwrap();
  for _ in 0..2 {
    let expired = store
      .expired_factory_execution_inputs(reserved.expires_at, 1)
      .await
      .unwrap();
    assert_eq!(expired.len(), 1);
    store
      .complete_factory_execution_input_deletion(CompleteFactoryExecutionInputDeletion {
        expected: expired.into_iter().next().unwrap(),
        deleted_at: reserved.expires_at,
      })
      .await
      .unwrap();
  }
  let next: i64 = sqlx::query_scalar("SELECT cleanup_at_millis FROM project_text_preparations WHERE id=$1")
    .bind(reserved.id.as_bytes().as_slice())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(next, reserved.expires_at.unix_millis() + 3_600_000);
  assert!(
    store
      .expired_factory_execution_inputs(at(next - 1), 10)
      .await
      .unwrap()
      .is_empty()
  );
  assert_eq!(
    store
      .expired_factory_execution_inputs(at(next), 10)
      .await
      .unwrap()
      .len(),
    2
  );
  assert!(
    store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &key("cleanup-schedule")
      )
      .await
      .unwrap()
      .is_none()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn execution_migrations_upgrade_existing_manual_issues_and_are_repeatable() {
  use octacity_server_store_postgres::{MIGRATOR, migrate, run_migrator};
  use std::borrow::Cow;
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator {
    migrations: Cow::Owned(MIGRATOR.iter().filter(|m| m.version <= 69).cloned().collect()),
    ..sqlx::migrate::Migrator::DEFAULT
  };
  run_migrator(&database.pool, &previous).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let original = issue(&store).await;
  migrate(&database.pool).await.unwrap();
  migrate(&database.pool).await.unwrap();
  assert_eq!(
    store.issue(original.project_id, original.id, None).await.unwrap(),
    original
  );
  assert!(
    store
      .issue_execution(original.project_id, original.id, IssueExecutionRequestId::generate())
      .await
      .is_err()
  );
  let created = store
    .request_issue_execution(management_mutation(request(&original)))
    .await
    .unwrap();
  assert!(created.request.association.is_none());
  let manual = issue(&store).await;
  assert!(manual.content.acceptance.is_some());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn execution_history_preserves_bounded_membership_and_order_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let mut requests = Vec::new();
  for n in 1..=3 {
    let mut reviewed = request(&issue);
    reviewed.idempotency_key = key(&format!("history-{n}"));
    reviewed.requested_at = at(n * 10);
    requests.push(
      store
        .request_issue_execution(management_mutation(reviewed))
        .await
        .unwrap()
        .request,
    );
  }
  let first = store
    .issue_executions(issue.project_id, issue.id, None, None, 2)
    .await
    .unwrap();
  assert_eq!(first.items, requests[..2]);
  assert_eq!(first.observation.issue_version, issue.version);
  let mut later = request(&issue);
  later.idempotency_key = key("history-later");
  later.requested_at = at(40);
  store.request_issue_execution(management_mutation(later)).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let last = restarted
    .issue_executions(issue.project_id, issue.id, first.next, Some(first.observation), 2)
    .await
    .unwrap();
  assert_eq!(last.items, requests[2..]);
  assert_eq!(last.next, None);
  assert_eq!(last.observation, first.observation);
  assert!(
    restarted
      .issue_executions(ProjectId::generate(), issue.id, None, None, 2)
      .await
      .is_err()
  );
  assert!(
    restarted
      .issue_executions(issue.project_id, issue.id, None, None, 201)
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn execution_review_accepts_full_valid_classification_without_storage_size_failures() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let mut reviewed = request(&issue);
  reviewed.intent.classification = Some(WorkClassification::new(
    WorkPriority::new(10).unwrap(),
    RiskClass::Medium,
    FactoryMetadata::try_new((0..16).map(|n| (format!("field{n}"), "\\".repeat(500)))).unwrap(),
  ));
  let first = store
    .request_issue_execution(management_mutation(reviewed.clone()))
    .await
    .unwrap();
  assert_eq!(first.request.intent, reviewed.intent);
  assert_eq!(
    store
      .request_issue_execution(management_mutation(reviewed))
      .await
      .unwrap()
      .request,
    first.request
  );
  database.cleanup().await;
}

async fn assert_issue_local_execution_watermark(store: &(impl ProjectStoreIssue + IssueExecutionStore)) {
  let visible = issue(store).await;
  let other = issue(store).await;
  for (index, target) in [&visible, &other, &other, &visible].into_iter().enumerate() {
    let mut reviewed = request(target);
    reviewed.idempotency_key = key(&format!("local-watermark-{index}"));
    reviewed.requested_at = at(index as i64 + 10);
    store
      .request_issue_execution(management_mutation(reviewed))
      .await
      .unwrap();
  }
  let first = store
    .issue_executions(visible.project_id, visible.id, None, None, 1)
    .await
    .unwrap();
  assert_eq!(
    first.observation.watermark, 2,
    "other Issues must not affect visible continuation metadata"
  );
  let last = store
    .issue_executions(visible.project_id, visible.id, first.next, Some(first.observation), 1)
    .await
    .unwrap();
  assert_eq!(last.items.len(), 1);
  assert_eq!(last.observation, first.observation);
  assert_eq!(last.next, None);
}
#[tokio::test]
async fn execution_watermark_is_issue_local_in_memory() {
  assert_issue_local_execution_watermark(&InMemoryProjectStore::new()).await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn execution_watermark_is_issue_local_in_postgres() {
  let database = TestDatabase::migrated().await;
  assert_issue_local_execution_watermark(&PostgresStore::new(database.pool.clone())).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn timeline_execution_membership_retains_original_request_references_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let issue = issue(&store).await;
  let mut expected = Vec::new();
  for n in 0..2 {
    let mut next = request(&issue);
    next.idempotency_key = key(&format!("timeline-request-{n}"));
    next.requested_at = at(10);
    expected.push(
      store
        .request_issue_execution(management_mutation(next))
        .await
        .unwrap()
        .request
        .id,
    );
  }
  expected.sort();
  let query = ReadIssueTimeline {
    project_id: issue.project_id,
    issue_id: issue.id,
    scope: ManagementSecurityScope::trusted_network(),
    kinds: [IssueTimelineKind::Execution].into_iter().collect(),
    boundary: None,
    after: None,
    limit: 1,
  };
  let first = store.issue_timeline(query.clone()).await.unwrap();
  assert!(
    matches!(&first.items[0],IssueTimelineReference::Execution {request_id,requirements_version,..} if *request_id==expected[0] && *requirements_version==issue.version)
  );
  let mut late = request(&issue);
  late.idempotency_key = key("late-timeline-request");
  late.requested_at = at(5);
  store.request_issue_execution(management_mutation(late)).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let last = restarted
    .issue_timeline(ReadIssueTimeline {
      boundary: Some(first.boundary),
      after: first.next,
      ..query
    })
    .await
    .unwrap();
  assert_eq!(last.items.len(), 1);
  assert!(matches!(&last.items[0],IssueTimelineReference::Execution {request_id,..} if *request_id==expected[1]));
  assert!(last.next.is_none());
  assert_eq!(last.boundary, first.boundary);
  database.cleanup().await;
}
