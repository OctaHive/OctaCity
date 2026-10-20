use super::*;
use octacity_server_factory::{ExternalWorkIdentity, FactoryDigest, ImmutableReference, WorkSourceBinding};
use octacity_server_store::{
  AttachIssueSource, AttachIssueSourceIntent, IssueFieldPolicy, IssueSourceOutcome, IssueSourceSelection,
  IssueSourceStore, ManagementSecurityScope,
};
use octacity_server_store::{ImportIssueSource, IssueListVisibility};
#[path = "source_mapping.rs"]
mod mapping;
#[path = "source_refresh_command.rs"]
mod refresh_command;

fn exact(name: &str) -> ImmutableReference {
  ImmutableReference::new(
    name.parse().unwrap(),
    "v1".parse().unwrap(),
    FactoryDigest::content_sha256(name.as_bytes()),
  )
}
fn selection(project_id: ProjectId) -> IssueSourceSelection {
  IssueSourceSelection {
    project_id,
    source: WorkSourceBinding {
      installation: exact("installed-tracker"),
      capability: exact("task-reader"),
      configuration: exact("read-mapping"),
      resource: exact("resource-choice"),
    },
    item: octacity_server_store::WorkSourceItem {
      resource: ExternalWorkIdentity::new("native-container").unwrap(),
      item: ExternalWorkIdentity::new("native-item-7").unwrap(),
    },
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_attachment_matches_memory_and_keeps_original_replay_after_restart() {
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let memory = InMemoryProjectStore::new();
  let project = project(&postgres).await;
  memory
    .create_project(management_mutation(project.clone()))
    .await
    .unwrap();
  let manual = draft(project.id, "manual");
  let issue = postgres
    .create_issue(management_mutation(manual.clone()))
    .await
    .unwrap()
    .issue;
  memory.create_issue(management_mutation(manual)).await.unwrap();
  let request = AttachIssueSource {
    observation: observation(&selection(project.id)),
    intent: AttachIssueSourceIntent {
      selection: selection(project.id),
      issue_id: issue.id,
      expected_version: issue.version,
      expected_field_policy: IssueFieldPolicy::default(),
    },
    idempotency_key: key("attach"),
    attached_at: at(3),
  };
  let scope = ManagementSecurityScope::trusted_network();
  assert!(
    postgres
      .issue_source_attachment_replay(&scope, &request.intent, &request.idempotency_key)
      .await
      .unwrap()
      .is_none()
  );
  let expected = memory
    .attach_issue_source(management_mutation(request.clone()))
    .await
    .unwrap();
  let actual = postgres
    .attach_issue_source(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(actual, expected);
  let IssueSourceOutcome::Linked { link, .. } = actual else {
    panic!("new link required")
  };
  assert_eq!(link.issue_id, issue.id);
  assert_eq!(link.selection, selection(project.id));
  assert_eq!(postgres.issue(project.id, issue.id, None).await.unwrap(), issue);
  let scope = ManagementSecurityScope::trusted_network();
  let receipt = request.observation.id(link.native_correlation).unwrap();
  for store in [&postgres as &dyn IssueSourceStore, &memory] {
    assert_eq!(
      store
        .issue_source_observation(project.id, issue.id, &scope, receipt)
        .await
        .unwrap(),
      Some(request.observation.clone())
    );
  }
  let probe = postgres
    .issue_source_attachment_replay(&scope, &request.intent, &request.idempotency_key)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    probe,
    IssueSourceOutcome::Linked {
      link: link.clone(),
      disposition: MutationDisposition::Replayed
    }
  );
  assert_eq!(
    memory
      .issue_source_attachment_replay(&scope, &request.intent, &request.idempotency_key)
      .await
      .unwrap(),
    Some(probe.clone())
  );
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .issue_source_link(project.id, issue.id, &ManagementSecurityScope::trusted_network())
      .await
      .unwrap(),
    Some(link.clone())
  );
  assert_eq!(
    restarted
      .attach_issue_source(management_mutation(request))
      .await
      .unwrap(),
    IssueSourceOutcome::Linked {
      link,
      disposition: MutationDisposition::Replayed
    }
  );
  database.cleanup().await;
}

fn import(project: ProjectId, replay: &str) -> ImportIssueSource {
  ImportIssueSource {
    read_claim: None,
    observation: observation(&selection(project)),
    visibility: IssueListVisibility::all(),
    selection: selection(project),
    id: IssueId::generate(),
    content: draft(project, "unused").intent.content,
    idempotency_key: key(replay),
    imported_at: at(3),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_concurrent_imports_resolve_one_issue_and_keep_independent_receipts() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let first = import(project.id, "input-a");
  let mut second = import(project.id, "input-b");
  second.selection.source.configuration = exact("another-input-mapping");
  second.observation = observation(&second.selection);
  let (a, b) = tokio::join!(
    store.import_issue_source(management_mutation(first.clone())),
    store.import_issue_source(management_mutation(second.clone()))
  );
  let IssueSourceOutcome::Linked { link: a, .. } = a.unwrap() else {
    panic!("canonical import")
  };
  let IssueSourceOutcome::Linked { link: b, .. } = b.unwrap() else {
    panic!("canonical import")
  };
  assert_eq!(a, b);
  assert_eq!(
    store.issue(project.id, a.issue_id, None).await.unwrap().display_key(),
    "ISS-1"
  );
  let restarted = PostgresStore::new(database.pool.clone());
  for mut request in [first, second.clone()] {
    request.id = IssueId::generate();
    request.content.title = "Mutable later observation".into();
    request.imported_at = at(99);
    assert_eq!(
      restarted
        .import_issue_source(management_mutation(request))
        .await
        .unwrap(),
      IssueSourceOutcome::Linked {
        link: a.clone(),
        disposition: MutationDisposition::Replayed
      }
    );
  }
  second.selection.source.installation = exact("different-installation");
  assert_eq!(
    restarted
      .import_issue_source(management_mutation(second))
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Issue
    }
  );
  let manual = store
    .create_issue(management_mutation(draft(project.id, "manual")))
    .await
    .unwrap();
  assert_ne!(manual.issue.id, a.issue_id);
  assert_eq!(manual.issue.display_key(), "ISS-2");
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_observations_retain_each_original_mapping_and_provenance_across_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let scope = ManagementSecurityScope::trusted_network();
  let mut first = import(project.id, "observed-a");
  first.observation = observation(&first.selection);
  let first_id = first
    .observation
    .id(first.selection.native_correlation(&scope))
    .unwrap();
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(first.clone()))
    .await
    .unwrap()
  else {
    panic!("source link")
  };
  let mut second = import(project.id, "observed-b");
  second.selection.source.configuration = exact("other-mapping-version");
  second.observation = observation(&second.selection);
  second.observation.snapshot_digest = FactoryDigest::content_sha256(b"changed native snapshot");
  let second_id = second
    .observation
    .id(second.selection.native_correlation(&scope))
    .unwrap();
  store
    .import_issue_source(management_mutation(second.clone()))
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .issue_source_observation(project.id, link.issue_id, &scope, first_id)
      .await
      .unwrap(),
    Some(first.observation)
  );
  assert_eq!(
    restarted
      .issue_source_observation(project.id, link.issue_id, &scope, second_id)
      .await
      .unwrap(),
    Some(second.observation)
  );
  assert_eq!(
    restarted.issue(project.id, link.issue_id, None).await.unwrap().version,
    IssueVersion::INITIAL
  );
  assert_eq!(
    restarted
      .issue_source_link(project.id, link.issue_id, &scope)
      .await
      .unwrap(),
    Some(link)
  );
  database.cleanup().await;
}

fn observation(selection: &IssueSourceSelection) -> octacity_server_store::IssueSourceObservation {
  octacity_server_store::IssueSourceObservation {
    source: selection.source.clone(),
    plugin: exact("verified-reader"),
    provider_configuration: FactoryDigest::content_sha256(b"transport settings"),
    selection_digest: FactoryDigest::content_sha256(b"selector"),
    snapshot_digest: FactoryDigest::content_sha256(b"native snapshot"),
    observed_at: at(2),
    expires_at: at(100),
  }
}

fn scoped<T>(request: T, scope: &str) -> octacity_server_store::ManagementMutation<T> {
  use octacity_server_store::{AuditActor, AuditActorKind, ManagementMutation, MutationAuditContext};
  ManagementMutation::new(
    request,
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some(scope.into()),
      },
      ManagementSecurityScope::new(scope).unwrap(),
      format!("request-{scope}"),
    )
    .unwrap(),
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_rotation_preserves_links_and_partitions_visibility_and_primary_slots() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let original = import(project.id, "original");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(original.clone()))
    .await
    .unwrap()
  else {
    panic!("source link")
  };
  let mut rotated = import(project.id, "rotated-adopted");
  rotated.selection.source.installation = ImmutableReference::new(
    rotated.selection.source.installation.identity().clone(),
    "v2".parse().unwrap(),
    FactoryDigest::content_sha256(b"compatible adopted installation"),
  );
  rotated.selection.source.configuration = exact("new-credential-free-profile");
  rotated.selection.source.resource = exact("renamed-resource");
  rotated.observation = observation(&rotated.selection);
  rotated.observation.provider_configuration = FactoryDigest::content_sha256(b"changed transport");
  assert!(
    matches!(store.import_issue_source(management_mutation(rotated)).await.unwrap(),IssueSourceOutcome::Linked { link:ref current,.. } if current == &link)
  );
  let other_scope = ManagementSecurityScope::new("operator:other").unwrap();
  let other = store
    .import_issue_source(scoped(import(project.id, "original"), other_scope.as_str()))
    .await
    .unwrap();
  let IssueSourceOutcome::Linked { link: other, .. } = other else {
    panic!("partitioned link")
  };
  assert_ne!(other.issue_id, link.issue_id);
  assert!(
    store
      .issue_source_link(project.id, link.issue_id, &other_scope)
      .await
      .unwrap()
      .is_none()
  );
  let observation_id = original.observation.id(link.native_correlation).unwrap();
  assert!(
    store
      .issue_source_observation(project.id, link.issue_id, &other_scope, observation_id)
      .await
      .unwrap()
      .is_none()
  );
  let mut hidden = import(project.id, "hidden");
  hidden.visibility = IssueListVisibility::none();
  assert_eq!(
    store
      .import_issue_source(management_mutation(hidden))
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  let manual = store
    .create_issue(management_mutation(draft(project.id, "manual")))
    .await
    .unwrap()
    .issue;
  let request = AttachIssueSource {
    observation: original.observation.clone(),
    intent: AttachIssueSourceIntent {
      selection: original.selection.clone(),
      issue_id: manual.id,
      expected_version: manual.version,
      expected_field_policy: Default::default(),
    },
    idempotency_key: key("duplicate"),
    attached_at: at(4),
  };
  assert_eq!(
    store
      .attach_issue_source(management_mutation(request.clone()))
      .await
      .unwrap(),
    IssueSourceOutcome::Conflict {
      canonical: Some(link.clone())
    }
  );
  let mut foreign = project.clone();
  foreign.id = ProjectId::generate();
  foreign.idempotency_key = key("foreign-project");
  foreign.name = ProjectName::new("Foreign").unwrap();
  store
    .create_project(management_mutation(foreign.clone()))
    .await
    .unwrap();
  let foreign_link = store
    .import_issue_source(management_mutation(import(foreign.id, "foreign-import")))
    .await
    .unwrap();
  assert!(
    matches!(foreign_link,IssueSourceOutcome::Linked { link:ref current,.. } if current.issue_id != link.issue_id)
  );
  let mut wrong = request;
  wrong.intent.selection.project_id = foreign.id;
  wrong.observation = observation(&wrong.intent.selection);
  assert_eq!(
    store.attach_issue_source(management_mutation(wrong)).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  assert_eq!(store.issue(project.id, manual.id, None).await.unwrap(), manual);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_additive_migrations_preserve_manual_issues_and_can_run_again() {
  use octacity_server_store_postgres::{MIGRATOR, migrate, run_migrator};
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator::with_migrations(
    MIGRATOR
      .iter()
      .filter(|migration| migration.version <= 79)
      .cloned()
      .collect(),
  );
  run_migrator(&database.pool, &previous).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let manual = store
    .create_issue(management_mutation(draft(project.id, "manual-before-source")))
    .await
    .unwrap()
    .issue;
  migrate(&database.pool).await.unwrap();
  let scope = ManagementSecurityScope::trusted_network();
  assert_eq!(store.issue(project.id, manual.id, None).await.unwrap(), manual);
  assert!(
    store
      .issue_source_link(project.id, manual.id, &scope)
      .await
      .unwrap()
      .is_none()
  );
  let request = import(project.id, "after-upgrade");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(request.clone()))
    .await
    .unwrap()
  else {
    panic!("source link")
  };
  assert_eq!(
    store
      .issue(project.id, link.issue_id, None)
      .await
      .unwrap()
      .display_key(),
    "ISS-2"
  );
  migrate(&database.pool).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(restarted.issue(project.id, manual.id, None).await.unwrap(), manual);
  assert_eq!(
    restarted
      .issue_source_observation(
        project.id,
        link.issue_id,
        &scope,
        request.observation.id(link.native_correlation).unwrap()
      )
      .await
      .unwrap(),
    Some(request.observation.clone())
  );
  assert_eq!(
    restarted
      .import_issue_source(management_mutation(request))
      .await
      .unwrap(),
    IssueSourceOutcome::Linked {
      link,
      disposition: MutationDisposition::Replayed
    }
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_import_and_attachment_rollback_receipts_and_recover_original_links() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_source CHECK (topic NOT IN ('issue.source-imported','issue.source-attached')) NOT VALID").execute(&database.pool).await.unwrap();
  let command = import(project.id, "interrupted-import");
  assert!(
    store
      .import_issue_source(management_mutation(command.clone()))
      .await
      .is_err()
  );
  assert_eq!(
    store.issue(project.id, command.id, None).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Issue
    }
  );
  let manual = store
    .create_issue(management_mutation(draft(project.id, "manual")))
    .await
    .unwrap()
    .issue;
  assert_eq!(manual.display_key(), "ISS-1");
  let attach = AttachIssueSource {
    observation: observation(&selection(project.id)),
    intent: AttachIssueSourceIntent {
      selection: selection(project.id),
      issue_id: manual.id,
      expected_version: manual.version,
      expected_field_policy: Default::default(),
    },
    idempotency_key: key("interrupted-attach"),
    attached_at: at(4),
  };
  assert!(
    store
      .attach_issue_source(management_mutation(attach.clone()))
      .await
      .is_err()
  );
  let scope = ManagementSecurityScope::trusted_network();
  assert!(
    store
      .issue_source_link(project.id, manual.id, &scope)
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .issue_source_observation(
        project.id,
        manual.id,
        &scope,
        attach
          .observation
          .id(attach.intent.selection.native_correlation(&scope))
          .unwrap()
      )
      .await
      .unwrap()
      .is_none()
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_source")
    .execute(&database.pool)
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let applied = restarted
    .attach_issue_source(management_mutation(attach.clone()))
    .await
    .unwrap();
  let IssueSourceOutcome::Linked { link, .. } = applied else {
    panic!("recovered attachment")
  };
  assert_eq!(
    store.import_issue_source(management_mutation(command)).await.unwrap(),
    IssueSourceOutcome::Linked {
      link: link.clone(),
      disposition: MutationDisposition::Replayed
    }
  );
  assert_eq!(
    restarted
      .attach_issue_source(management_mutation(attach))
      .await
      .unwrap(),
    IssueSourceOutcome::Linked {
      link,
      disposition: MutationDisposition::Replayed
    }
  );
  assert_eq!(store.issue(project.id, manual.id, None).await.unwrap(), manual);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_refresh_reconciliation_and_receipts_survive_restart_and_publication_failure() {
  use octacity_server_store::{IssueField, IssueSourceRefreshDisposition, IssueSourceRefreshStore, RefreshIssueSource};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let request = import(project.id, "native-original");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(request.clone()))
    .await
    .unwrap()
  else {
    panic!("source link")
  };
  let issue = store.issue(project.id, link.issue_id, None).await.unwrap();
  seed_policy(
    &database,
    &issue,
    &IssueFieldPolicy {
      source_owned: [IssueField::Title].into(),
      ..Default::default()
    },
  )
  .await;
  let mut refresh = RefreshIssueSource {
    explicit_read: None,
    status: None,
    read_claim: None,
    selection: request.selection,
    issue_id: link.issue_id,
    observation: request.observation,
    content: IssueContent {
      title: "Changed requirements".into(),
      ..issue.content.clone()
    },
    idempotency_key: key("first-changed-read"),
    refreshed_at: at(6),
  };
  refresh.observation.snapshot_digest = FactoryDigest::content_sha256(b"changed complete snapshot");
  refresh.observation.observed_at = at(4);
  assert_eq!(
    store
      .refresh_issue_source(management_mutation(refresh.clone()))
      .await
      .unwrap()
      .disposition,
    IssueSourceRefreshDisposition::Uncertain
  );
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  refresh.observation.observed_at = at(5);
  refresh.idempotency_key = key("independent-confirmation");
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_refresh CHECK (topic <> 'issue.source-refreshed') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert!(
    store
      .refresh_issue_source(management_mutation(refresh.clone()))
      .await
      .is_err()
  );
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_refresh")
    .execute(&database.pool)
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let accepted = restarted
    .refresh_issue_source(management_mutation(refresh.clone()))
    .await
    .unwrap();
  assert_eq!(accepted.disposition, IssueSourceRefreshDisposition::Fresh);
  let current = restarted.issue(project.id, issue.id, None).await.unwrap();
  assert_eq!(current.content.title, "Changed requirements");
  assert_eq!(current.version.get(), 2);
  assert_eq!(current.status_id, issue.status_id);
  assert_eq!(
    PostgresStore::new(database.pool.clone())
      .refresh_issue_source(management_mutation(refresh))
      .await
      .unwrap(),
    accepted
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_refresh_paused_publication_is_a_durable_skip_without_advancing_freshness() {
  use octacity_server_store::{
    ClaimWorkSourceScan, IssueSourceReadStore, IssueSourceReadTarget, IssueSourceRefreshDisposition,
    IssueSourceRefreshStore, RefreshIssueSource, SetIssueSourcePull,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let request = import(project.id, "before-paused-publication");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(request.clone()))
    .await
    .unwrap()
  else {
    panic!("source link")
  };
  let scope = ManagementSecurityScope::trusted_network();
  let issue = store.issue(project.id, link.issue_id, None).await.unwrap();
  let scan = store
    .claim_issue_source_read(
      &scope,
      ClaimWorkSourceScan {
        project_id: project.id,
        target: IssueSourceReadTarget {
          pull_generation: 0,
          source: request.selection.source.clone(),
          issue_id: None,
        },
        owner: "discovery-owner".parse().unwrap(),
        now: at(4),
        until: at(50),
      },
    )
    .await
    .unwrap()
    .unwrap();
  let pause = SetIssueSourcePull {
    project_id: project.id,
    issue_id: issue.id,
    expected_generation: 1,
    enabled: false,
    idempotency_key: key("pause-before-refresh-publication"),
    changed_at: at(5),
  };
  let disabled = store
    .set_issue_source_pull(management_mutation(pause.clone()))
    .await
    .unwrap();
  let before = store
    .issue_source_refresh_state(project.id, issue.id, &scope)
    .await
    .unwrap()
    .unwrap();
  let mut refresh = RefreshIssueSource {
    explicit_read: None,
    status: None,
    read_claim: Some(scan.claim),
    selection: request.selection,
    issue_id: issue.id,
    observation: request.observation,
    content: IssueContent {
      title: "Changed while paused".into(),
      ..issue.content.clone()
    },
    idempotency_key: key("paused-refresh"),
    refreshed_at: at(6),
  };
  refresh.observation.snapshot_digest = FactoryDigest::content_sha256(b"changed while paused");
  refresh.observation.observed_at = at(4);
  let skipped = store
    .refresh_issue_source(management_mutation(refresh.clone()))
    .await
    .unwrap();
  assert_eq!(skipped.disposition, IssueSourceRefreshDisposition::PullDisabled);
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  assert_eq!(
    store
      .issue_source_refresh_state(project.id, issue.id, &scope)
      .await
      .unwrap()
      .unwrap(),
    before
  );
  assert_eq!(
    store
      .issue_source_observation(
        project.id,
        issue.id,
        &scope,
        refresh.observation.id(link.native_correlation).unwrap(),
      )
      .await
      .unwrap(),
    Some(refresh.observation.clone())
  );
  store
    .set_issue_source_pull(management_mutation(SetIssueSourcePull {
      expected_generation: disabled.generation,
      enabled: true,
      idempotency_key: key("resume-after-skipped-refresh"),
      changed_at: at(7),
      ..pause
    }))
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .refresh_issue_source(management_mutation(refresh.clone()))
      .await
      .unwrap(),
    skipped
  );
  refresh.idempotency_key = key("new-read-after-resume");
  refresh.observation.observed_at = at(8);
  refresh.refreshed_at = at(8);
  assert_eq!(
    restarted
      .refresh_issue_source(management_mutation(refresh))
      .await
      .unwrap()
      .disposition,
    IssueSourceRefreshDisposition::Uncertain,
    "a skipped read must not confirm a later changed snapshot"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_execution_neutral_progress_retains_pages_and_retry_without_reusing_expired_owner() {
  use octacity_server_store::{
    ClaimWorkSourceScan, IssueSourceReadStore, IssueSourceReadTarget, WorkSourceFailure, WorkSourceProgress,
    WorkSourceSchedule,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let scope = ManagementSecurityScope::trusted_network();
  let command = ClaimWorkSourceScan {
    project_id: project.id,
    target: IssueSourceReadTarget {
      pull_generation: 0,
      source: selection(project.id).source,
      issue_id: None,
    },
    owner: "reader".parse().unwrap(),
    now: at(10),
    until: at(20),
  };
  let original = store
    .claim_issue_source_read(&scope, command.clone())
    .await
    .unwrap()
    .unwrap();
  let progress = WorkSourceProgress {
    polling: None,
    window_start: at(1),
    window_end: at(10),
    continuation: Some("original-position".into()),
    reconciliation: false,
  };
  store
    .checkpoint_issue_source_read(&scope, original.claim.clone(), at(11), None, progress.clone())
    .await
    .unwrap();
  let schedule = WorkSourceSchedule {
    retry_at: at(30),
    attempts: 1,
    failure: Some(WorkSourceFailure::RateLimited),
    exhausted: false,
  };
  store
    .finish_issue_source_read(&scope, original.claim.clone(), at(12), schedule.clone())
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  assert!(
    restarted
      .claim_issue_source_read(
        &scope,
        ClaimWorkSourceScan {
          now: at(29),
          until: at(40),
          ..command.clone()
        }
      )
      .await
      .unwrap()
      .is_none()
  );
  let next = restarted
    .claim_issue_source_read(
      &scope,
      ClaimWorkSourceScan {
        now: at(30),
        until: at(40),
        ..command
      },
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(next.progress, Some(progress.clone()));
  assert_eq!(next.schedule, Some(schedule));
  assert!(next.claim.fence > original.claim.fence);
  assert!(
    restarted
      .checkpoint_issue_source_read(&scope, original.claim, at(31), Some(progress.clone()), progress)
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_read_takeover_fences_import_in_the_same_transaction_as_issue_publication() {
  use octacity_server_store::{ClaimWorkSourceScan, IssueSourceReadStore, IssueSourceReadTarget};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let scope = ManagementSecurityScope::trusted_network();
  let claim = ClaimWorkSourceScan {
    project_id: project.id,
    target: IssueSourceReadTarget {
      pull_generation: 0,
      source: selection(project.id).source,
      issue_id: None,
    },
    owner: "old-source-owner".parse().unwrap(),
    now: at(10),
    until: at(20),
  };
  let original = store
    .claim_issue_source_read(&scope, claim.clone())
    .await
    .unwrap()
    .unwrap();
  let replacement = store
    .claim_issue_source_read(
      &scope,
      ClaimWorkSourceScan {
        owner: "new-source-owner".parse().unwrap(),
        now: at(21),
        until: at(40),
        ..claim
      },
    )
    .await
    .unwrap()
    .unwrap();
  let mut request = import(project.id, "fenced-source-import");
  request.imported_at = at(22);
  request.read_claim = Some(original.claim);
  assert!(
    store
      .import_issue_source(management_mutation(request.clone()))
      .await
      .is_err()
  );
  request.read_claim = Some(replacement.claim);
  let IssueSourceOutcome::Linked { link, .. } = store.import_issue_source(management_mutation(request)).await.unwrap()
  else {
    panic!("current owner")
  };
  assert_eq!(
    store
      .issue(project.id, link.issue_id, None)
      .await
      .unwrap()
      .display_key(),
    "ISS-1"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_pull_disable_and_reenable_invalidates_original_read_owner_and_replays_control() {
  use octacity_server_store::{
    ClaimWorkSourceScan, IssueSourceReadStore, IssueSourceReadTarget, IssueSourceRefreshStore, SetIssueSourcePull,
    WorkSourceProgress,
  };
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(import(project.id, "original")))
    .await
    .unwrap()
  else {
    panic!("link")
  };
  let scope = ManagementSecurityScope::trusted_network();
  let command = ClaimWorkSourceScan {
    project_id: project.id,
    target: IssueSourceReadTarget {
      pull_generation: 0,
      source: selection(project.id).source,
      issue_id: Some(link.issue_id),
    },
    owner: "old-link-reader".parse().unwrap(),
    now: at(10),
    until: at(20),
  };
  let original = store
    .claim_issue_source_read(&scope, command.clone())
    .await
    .unwrap()
    .unwrap();
  let pause = SetIssueSourcePull {
    project_id: project.id,
    issue_id: link.issue_id,
    expected_generation: 1,
    enabled: false,
    idempotency_key: key("pause-link"),
    changed_at: at(11),
  };
  let disabled = store
    .set_issue_source_pull(management_mutation(pause.clone()))
    .await
    .unwrap();
  store
    .set_issue_source_pull(management_mutation(SetIssueSourcePull {
      expected_generation: disabled.generation,
      enabled: true,
      idempotency_key: key("resume-link"),
      changed_at: at(12),
      ..pause.clone()
    }))
    .await
    .unwrap();
  let progress = WorkSourceProgress {
    polling: None,
    window_start: at(0),
    window_end: at(10),
    continuation: None,
    reconciliation: false,
  };
  assert!(
    store
      .checkpoint_issue_source_read(&scope, original.claim, at(13), None, progress)
      .await
      .is_err(),
    "enable cannot revive the old owner"
  );
  assert_eq!(
    PostgresStore::new(database.pool.clone())
      .set_issue_source_pull(management_mutation(pause))
      .await
      .unwrap(),
    disabled
  );
  assert!(
    store
      .claim_issue_source_read(
        &scope,
        ClaimWorkSourceScan {
          now: at(21),
          until: at(30),
          ..command
        }
      )
      .await
      .unwrap()
      .is_some()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_refresh_upgrade_retains_the_original_schema_81_link_and_read_provenance() {
  use octacity_server_store::{IssueSourceLink, IssueSourceRefreshStore};
  use octacity_server_store_postgres::{MIGRATOR, migrate, run_migrator};
  use sqlx::types::Json;
  let database = TestDatabase::empty().await;
  let previous =
    sqlx::migrate::Migrator::with_migrations(MIGRATOR.iter().filter(|m| m.version <= 81).cloned().collect());
  run_migrator(&database.pool, &previous).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let manual = store
    .create_issue(management_mutation(draft(project.id, "previous-manual")))
    .await
    .unwrap()
    .issue;
  let scope = ManagementSecurityScope::trusted_network();
  let selected = selection(project.id);
  let link = IssueSourceLink {
    issue_id: manual.id,
    native_correlation: selected.native_correlation(&scope),
    selection: selected.clone(),
    linked_at: at(3),
  };
  let observed = observation(&selected);
  // Exact schema-81 fixture from its public receipt format: current import also writes the new watermark.
  sqlx::query("INSERT INTO issue_source_links(project_id,issue_id,security_scope,native_correlation,document) VALUES($1,$2,$3,$4,$5)").bind(project.id.as_uuid()).bind(manual.id.as_uuid()).bind(scope.as_str()).bind(link.native_correlation.as_bytes().as_slice()).bind(Json(&link)).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO issue_source_observations(project_id,security_scope,native_correlation,id,document) VALUES($1,$2,$3,$4,$5)").bind(project.id.as_uuid()).bind(scope.as_str()).bind(link.native_correlation.as_bytes().as_slice()).bind(observed.id(link.native_correlation).unwrap().as_bytes().as_slice()).bind(Json(&observed)).execute(&database.pool).await.unwrap();
  migrate(&database.pool).await.unwrap();
  migrate(&database.pool).await.unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let state = restarted
    .issue_source_refresh_state(project.id, manual.id, &scope)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(state.accepted, observed);
  assert_eq!(state.pending, None);
  assert!(state.enabled);
  assert_eq!(state.generation, 1);
  assert_eq!(restarted.issue(project.id, manual.id, None).await.unwrap(), manual);
  assert_eq!(
    restarted
      .issue_source_link(project.id, manual.id, &scope)
      .await
      .unwrap(),
    Some(link)
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_private_read_progress_does_not_anchor_an_otherwise_empty_project() {
  use octacity_server_store::{ClaimWorkSourceScan, DeleteProject, IssueSourceReadStore, IssueSourceReadTarget};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  store
    .claim_issue_source_read(
      &ManagementSecurityScope::trusted_network(),
      ClaimWorkSourceScan {
        project_id: project.id,
        target: IssueSourceReadTarget {
          pull_generation: 0,
          source: selection(project.id).source,
          issue_id: None,
        },
        owner: "empty-project-reader".parse().unwrap(),
        now: at(10),
        until: at(20),
      },
    )
    .await
    .unwrap()
    .unwrap();
  let current = store.project(project.id).await.unwrap().project;
  store
    .delete_project(management_mutation(DeleteProject {
      id: project.id,
      expected_version: current.version,
      idempotency_key: key("delete-empty-read-project"),
      deleted_at: at(11),
    }))
    .await
    .unwrap();
  assert!(matches!(
    store.project(project.id).await,
    Err(StoreError::NotFound {
      entity: EntityKind::Project
    })
  ));
  database.cleanup().await;
}
