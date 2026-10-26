use super::*;
use octacity_server_domain::IssueId;
use octacity_server_store::testing::management_mutation;
use octacity_server_store::{
  AssociateIssueWork, ImportIssueSource, IssueContent, IssueExecutionStore, IssueListVisibility,
  IssueSourceObservation, IssueSourceOutcome, IssueSourceSelection, IssueSourceStore, IssueStore,
  ManagementSecurityScope, MutationDisposition, StoreError,
};

async fn setup() -> (TestDatabase, PostgresStore, FactoryConfiguration, PublishedRepository) {
  setup_on(TestDatabase::migrated().await).await
}
async fn setup_on(database: TestDatabase) -> (TestDatabase, PostgresStore, FactoryConfiguration, PublishedRepository) {
  let store = PostgresStore::new(database.pool.clone());
  let project = ProjectId::generate();
  seed_projects(&database.pool, project, ProjectId::generate()).await;
  let fixture = fixture(project);
  let configuration = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    digest(3),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  let repository = repository(project, RepositoryId::generate());
  seed_repository(&database.pool, &repository).await;
  store
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      configuration: configuration.clone(),
      idempotency_key: idempotency("source-issue-target"),
      published_at: time(1),
      intent: FactoryConfigurationMutationIntent::Create {
        id: configuration.reference().id(),
        project_id: project,
        definition_digest: digest(3),
        draft: fixture.draft,
      },
    }))
    .await
    .unwrap();
  (database, store, configuration, repository)
}

async fn prepare(
  store: &PostgresStore,
  input: &WorkSourceInput,
  configuration: &FactoryConfiguration,
  repository: &PublishedRepository,
) -> (
  octacity_server_store::WorkSourcePreparation,
  octacity_server_store::PublishedFactoryAdmission,
) {
  prepare_scoped(
    store,
    input,
    configuration,
    repository,
    ManagementSecurityScope::trusted_network(),
  )
  .await
}
async fn prepare_scoped(
  store: &PostgresStore,
  input: &WorkSourceInput,
  configuration: &FactoryConfiguration,
  repository: &PublishedRepository,
  scope: ManagementSecurityScope,
) -> (
  octacity_server_store::WorkSourcePreparation,
  octacity_server_store::PublishedFactoryAdmission,
) {
  store
    .publish_work_sources(PublishWorkSources {
      project_id: input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: input.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  let scan = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target: WorkSourceScanTarget::Input {
        id: input.id(),
        version: input.version(),
      },
      owner: key("source-issue-owner"),
      now: time(31),
      until: time(100),
    })
    .await
    .unwrap()
    .unwrap();
  let pending = store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim.clone(),
      input_id: input.id(),
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("native-container").unwrap(),
        item: ExternalWorkIdentity::new("ticket-42").unwrap(),
      },
      snapshot: b"original private native snapshot".to_vec(),
      now: time(32),
    })
    .await
    .unwrap();
  let example = admission_from_configuration(900, 901, "source-issue-example", 34, configuration, repository);
  let intent = FactoryWorkAdmissionIntent {
    profile_overrides: Default::default(),
    project_id: input.project_id(),
    configuration_id: configuration.reference().id(),
    configuration_version: configuration.reference().version(),
    repository_id: repository.id,
    repository_version: repository.version,
    external_identity: pending.admission_identity(scope.clone()).external_identity,
    source: octacity_server_store::RepositorySourceSelection::DefaultReference,
    artifacts: example.work.artifacts().clone(),
    classification: WorkClassification::new(
      example.work.priority(),
      example.work.risk(),
      example.work.metadata().clone(),
    ),
    idempotency_key: idempotency(&format!("source.{}", pending.id)),
    admitted_at: time(34),
  };
  let prepared = store
    .publish_source_preparation_intent(PublishSourcePreparationIntent {
      claim: scan.claim.clone(),
      preparation_id: pending.id,
      intent,
      now: time(33),
    })
    .await
    .unwrap();
  let outcome = store
    .admit_prepared_source_work(
      AdmitPreparedWorkSource {
        claim: scan.claim,
        preparation_id: prepared.id,
        now: time(34),
      },
      {
        let mut command = prepared_admission(&prepared, configuration);
        command.probe.source_scope.security_scope = scope.clone();
        let audit = octacity_server_store::MutationAuditContext::try_new(
          management_mutation(()).audit().actor().clone(),
          scope,
          "source-admission",
        )
        .unwrap();
        octacity_server_store::ManagementMutation::new(command, audit)
      },
    )
    .await
    .unwrap();
  (
    store.source_preparation(input.project_id(), prepared.id).await.unwrap(),
    outcome.admission,
  )
}

async fn imported(store: &PostgresStore, input: &WorkSourceInput, item: WorkSourceItem, replay: &str) -> IssueId {
  let selection = IssueSourceSelection {
    connection: None,
    project_id: input.project_id(),
    source: input.definition().source.clone(),
    item,
  };
  let observation = IssueSourceObservation {
    display_id: None,
    source: selection.source.clone(),
    plugin: ImmutableReference::new(key("verified-reader"), key("v1"), digest(3)),
    provider_configuration: digest(4),
    selection_digest: digest(5),
    snapshot_digest: digest(6),
    observed_at: time(32),
    expires_at: time(100),
  };
  let outcome = store
    .import_issue_source(management_mutation(ImportIssueSource {
      read_claim: None,
      observation,
      visibility: IssueListVisibility::all(),
      selection,
      id: IssueId::generate(),
      content: IssueContent {
        title: "Native task".into(),
        description: "Requirements".into(),
        acceptance: None,
        priority: None,
      },
      idempotency_key: idempotency(replay),
      imported_at: time(35),
    }))
    .await
    .unwrap();
  let IssueSourceOutcome::Linked { link, .. } = outcome else {
    panic!("native import")
  };
  link.issue_id
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn new_source_admissions_recover_one_card_without_manual_association() {
  use octacity_server_store::IssueLegacyStore;
  let (database, store, configuration, repository) = setup().await;
  let first = input(&configuration, &repository, "installed-tracker");
  let second = input(&configuration, &repository, "installed-tracker");
  let (prepared, original) = prepare(&store, &first, &configuration, &repository).await;
  // The admission may lose its acknowledgement before maintenance observes it.
  let restarted = PostgresStore::new(database.pool.clone());
  let linked = restarted.backfill_legacy_issue_work(1, time(50)).await.unwrap();
  assert_eq!(
    linked.len(),
    1,
    "settled new Source admissions must enter durable association recovery"
  );
  let issue = imported(&restarted, &first, prepared.item, "explicit-import").await;
  assert_eq!(linked[0].issue_id, issue);
  assert_eq!(linked[0].work_id, original.work.id());
  assert_eq!(linked[0].run_id, original.run.id());
  let (_, later) = prepare(&restarted, &second, &configuration, &repository).await;
  let again = PostgresStore::new(database.pool.clone());
  let next = again.backfill_legacy_issue_work(1, time(60)).await.unwrap();
  assert_eq!(next.len(), 1);
  assert_eq!(next[0].issue_id, issue);
  assert_eq!(next[0].work_id, later.work.id());
  assert_ne!(next[0].work_id, linked[0].work_id);
  assert_eq!(next[0].request_id, None);
  assert_eq!(next[0].requirements_version, None);
  assert!(again.backfill_legacy_issue_work(1, time(70)).await.unwrap().is_empty());
  assert_eq!(
    again
      .legacy_issue_work(first.project_id(), issue, None, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_work_cannot_attach_to_another_native_issue_and_correct_replay_keeps_its_run() {
  let (database, store, configuration, repository) = setup().await;
  let input = input(&configuration, &repository, "installed-tracker");
  let (prepared, admission) = prepare(&store, &input, &configuration, &repository).await;
  let mut other_item = prepared.item.clone();
  other_item.item = ExternalWorkIdentity::new("ticket-other").unwrap();
  let wrong_issue = imported(&store, &input, other_item, "other-task").await;
  let wrong = AssociateIssueWork {
    project_id: input.project_id(),
    issue_id: wrong_issue,
    request_id: None,
    admission: admission.clone(),
  };
  assert!(matches!(
    store.associate_issue_work(management_mutation(wrong)).await,
    Err(StoreError::Conflict { .. })
  ));
  assert!(
    store
      .issue_work_association(input.project_id(), admission.work.id())
      .await
      .unwrap()
      .is_none()
  );
  let issue = imported(&store, &input, prepared.item.clone(), "correct-task").await;
  let command = AssociateIssueWork {
    project_id: input.project_id(),
    issue_id: issue,
    request_id: None,
    admission: admission.clone(),
  };
  let outcome = store
    .associate_issue_work(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(outcome.association.run_id, admission.run.id());
  assert_eq!(outcome.association.work_id, admission.work.id());
  assert_eq!(outcome.association.requirements_version, None);
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .associate_issue_work(management_mutation(command))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.association, outcome.association);
  assert_eq!(
    restarted
      .issue(input.project_id(), issue, None)
      .await
      .unwrap()
      .version
      .get(),
    1
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn simultaneous_source_inputs_keep_one_issue_and_independently_recover_their_original_runs() {
  // Current Source admissions enqueue recovery atomically. Upgrade-only Work
  // membership is separately rehearsed against the old schema in legacy_issues.
  let (database, store, configuration, repository) = setup().await;
  let left = input(&configuration, &repository, "installed-tracker");
  let right = input(&configuration, &repository, "installed-tracker");
  let (a, b) = tokio::join!(
    prepare(&store, &left, &configuration, &repository),
    prepare(&store, &right, &configuration, &repository)
  );
  assert_ne!(a.1.work.id(), b.1.work.id());
  assert_ne!(a.1.run.id(), b.1.run.id());
  let (left_issue, right_issue) = tokio::join!(
    imported(&store, &left, a.0.item.clone(), "input-a"),
    imported(&store, &right, b.0.item.clone(), "input-b")
  );
  assert_eq!(left_issue, right_issue);
  let command = |admission| AssociateIssueWork {
    project_id: left.project_id(),
    issue_id: left_issue,
    request_id: None,
    admission,
  };
  let (aa, bb) = tokio::join!(
    store.associate_issue_work(management_mutation(command(a.1.clone()))),
    store.associate_issue_work(management_mutation(command(b.1.clone())))
  );
  assert_eq!(aa.unwrap().association.issue_id, left_issue);
  assert_eq!(bb.unwrap().association.issue_id, left_issue);
  let restarted = PostgresStore::new(database.pool.clone());
  for (prepared, original) in [a, b] {
    let scan = restarted
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: left.project_id(),
        target: prepared.target,
        owner: key("restarted-source-owner"),
        now: time(101),
        until: time(200),
      })
      .await
      .unwrap()
      .unwrap();
    let recovered = restarted
      .admit_prepared_source_work(
        AdmitPreparedWorkSource {
          claim: scan.claim,
          preparation_id: prepared.id,
          now: time(102),
        },
        management_mutation(prepared_admission(&prepared, &configuration)),
      )
      .await
      .unwrap();
    assert_eq!(recovered.disposition, MutationDisposition::Replayed);
    assert_eq!(recovered.admission, original);
    let linked = restarted
      .associate_issue_work(management_mutation(command(recovered.admission)))
      .await
      .unwrap();
    assert_eq!(linked.disposition, MutationDisposition::Replayed);
    assert_eq!(linked.association.run_id, original.run.id());
    assert_eq!(
      restarted
        .source_preparation_for_work(left.project_id(), original.work.id())
        .await
        .unwrap()
        .unwrap(),
      prepared
    );
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_association_recovers_backend_termination_failed_publication_and_lost_acknowledgement() {
  let (database, store, configuration, repository) = setup().await;
  let input = input(&configuration, &repository, "installed-tracker");
  let (prepared, admission) = prepare(&store, &input, &configuration, &repository).await;
  let issue = imported(&store, &input, prepared.item.clone(), "source-task").await;
  let command = AssociateIssueWork {
    project_id: input.project_id(),
    issue_id: issue,
    request_id: None,
    admission: admission.clone(),
  };
  let mut blocker = database.pool.acquire().await.unwrap();
  sqlx::query("SELECT pg_advisory_lock(4202)")
    .execute(&mut *blocker)
    .await
    .unwrap();
  sqlx::query("CREATE FUNCTION pause_source_association() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM pg_advisory_xact_lock(4202); RETURN NEW; END$$").execute(&database.pool).await.unwrap();
  sqlx::query("CREATE TRIGGER pause_source_association BEFORE INSERT ON issue_work_associations FOR EACH ROW EXECUTE FUNCTION pause_source_association()").execute(&database.pool).await.unwrap();
  let options = database
    .pool
    .connect_options()
    .as_ref()
    .clone()
    .application_name("source-association-crash");
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(options)
    .await
    .unwrap();
  let interrupted = PostgresStore::new(pool.clone());
  let pending = command.clone();
  let task = tokio::spawn(async move { interrupted.associate_issue_work(management_mutation(pending)).await });
  let pid = tokio::time::timeout(std::time::Duration::from_secs(10),async {
    loop {
      let pid:Option<i32> = sqlx::query_scalar("SELECT a.pid FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.datname=current_database() AND a.application_name='source-association-crash' AND l.locktype='advisory' AND NOT l.granted").fetch_optional(&database.pool).await.unwrap();
      if let Some(pid) = pid { break pid; } tokio::task::yield_now().await;
    }
  }).await.unwrap();
  sqlx::query("SELECT pg_terminate_backend($1)")
    .bind(pid)
    .execute(&database.pool)
    .await
    .unwrap();
  assert!(matches!(task.await.unwrap(), Err(StoreError::Unavailable)));
  pool.close().await;
  sqlx::query("DROP TRIGGER pause_source_association ON issue_work_associations")
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("SELECT pg_advisory_unlock(4202)")
    .execute(&mut *blocker)
    .await
    .unwrap();
  drop(blocker);
  assert!(
    store
      .issue_work_association(input.project_id(), admission.work.id())
      .await
      .unwrap()
      .is_none()
  );
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_source_association CHECK (topic <> 'issue.work-associated') NOT VALID").execute(&database.pool).await.unwrap();
  assert!(
    store
      .associate_issue_work(management_mutation(command.clone()))
      .await
      .is_err()
  );
  assert!(
    store
      .issue_work_association(input.project_id(), admission.work.id())
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(
    store
      .source_preparation_for_work(input.project_id(), admission.work.id())
      .await
      .unwrap()
      .unwrap(),
    prepared
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_source_association")
    .execute(&database.pool)
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let original = restarted
    .associate_issue_work(management_mutation(command.clone()))
    .await
    .unwrap();
  let mut current = restarted.issue(input.project_id(), issue, None).await.unwrap();
  current.content.title = "Later manual requirements".into();
  restarted
    .edit_issue(management_mutation(octacity_server_store::EditIssue {
      intent: octacity_server_store::EditIssueIntent {
        project_id: input.project_id(),
        id: issue,
        expected_version: current.version,
        content: current.content,
      },
      idempotency_key: idempotency("later-edit"),
      edited_at: time(40),
    }))
    .await
    .unwrap();
  let replay = PostgresStore::new(database.pool.clone())
    .associate_issue_work(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.association, original.association);
  let other_scope = octacity_server_store::ManagementMutation::new(
    command,
    octacity_server_store::MutationAuditContext::try_new(
      octacity_server_store::AuditActor {
        kind: octacity_server_store::AuditActorKind::AuthenticatedManagement,
        identity: Some("other-operator".into()),
      },
      ManagementSecurityScope::new("operator:other").unwrap(),
      "different-scope",
    )
    .unwrap(),
  );
  assert!(matches!(
    restarted.associate_issue_work(other_scope).await,
    Err(StoreError::Conflict { .. })
  ));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_recovery_groups_only_verified_native_work_and_later_import_reuses_the_card() {
  use octacity_server_store::IssueLegacyStore;
  // Current Source admissions enqueue recovery atomically. Upgrade-only Work
  // membership is separately rehearsed against the old schema in legacy_issues.
  let (database, store, configuration, repository) = setup().await;
  let left = input(&configuration, &repository, "installed-tracker");
  let right = input(&configuration, &repository, "installed-tracker");
  let (a, b) = tokio::join!(
    prepare(&store, &left, &configuration, &repository),
    prepare(&store, &right, &configuration, &repository)
  );
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let first = store.backfill_legacy_issue_work(1, time(50)).await.unwrap();
  assert_eq!(first.len(), 1);
  // Committed membership survives an interruption between bounded passes.
  let restarted = PostgresStore::new(database.pool.clone());
  let next = restarted.backfill_legacy_issue_work(1, time(60)).await.unwrap();
  assert_eq!(next.len(), 1);
  assert_eq!(first[0].issue_id, next[0].issue_id);
  assert_ne!(first[0].work_id, next[0].work_id);
  assert_ne!(first[0].run_id, next[0].run_id);
  assert!(
    restarted
      .issue_source_link(
        left.project_id(),
        first[0].issue_id,
        &ManagementSecurityScope::trusted_network()
      )
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .backfill_legacy_issue_work(1, time(70))
      .await
      .unwrap()
      .is_empty()
  );
  assert_eq!(
    imported(&restarted, &left, a.0.item, "later-explicit-import").await,
    first[0].issue_id
  );
  assert_eq!(
    restarted
      .issue_work_association(left.project_id(), a.1.work.id())
      .await
      .unwrap()
      .unwrap()
      .run_id,
    a.1.run.id()
  );
  assert_eq!(
    restarted
      .issue_work_association(right.project_id(), b.1.work.id())
      .await
      .unwrap()
      .unwrap()
      .run_id,
    b.1.run.id()
  );
  let page = restarted
    .legacy_issue_work(left.project_id(), first[0].issue_id, None, None, 1)
    .await
    .unwrap();
  let continuation = page.next.unwrap();
  let tail = restarted
    .legacy_issue_work(
      left.project_id(),
      first[0].issue_id,
      Some(continuation),
      Some(page.observation),
      1,
    )
    .await
    .unwrap();
  assert_eq!(tail.items.len(), 1);
  assert_ne!(page.items[0].work_id, tail.items[0].work_id);
  assert!(tail.next.is_none());
  assert!(
    restarted
      .legacy_issue_work(
        left.project_id(),
        first[0].issue_id,
        Some(WorkEnvelopeId::generate()),
        Some(page.observation),
        1
      )
      .await
      .is_err()
  );
  let issue = restarted
    .issue(left.project_id(), first[0].issue_id, None)
    .await
    .unwrap();
  restarted
    .edit_issue(management_mutation(octacity_server_store::EditIssue {
      intent: octacity_server_store::EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          title: "Later explicit requirements".into(),
          ..issue.content.clone()
        },
      },
      idempotency_key: idempotency("edit-after-legacy-page"),
      edited_at: time(80),
    }))
    .await
    .unwrap();
  assert!(
    matches!(
      restarted
        .legacy_issue_work(
          left.project_id(),
          issue.id,
          Some(continuation),
          Some(page.observation),
          1
        )
        .await,
      Err(StoreError::Conflict { .. })
    ),
    "continuation cannot silently mix Issue revisions"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn backfill_does_not_merge_other_installations_or_security_partitions() {
  use octacity_server_store::{FactoryRunStore, IssueLegacyStore};
  let (database, store, configuration, repository) = setup().await;
  let first = input(&configuration, &repository, "tracker-a");
  let other_installation = input(&configuration, &repository, "tracker-b");
  let other_partition = input(&configuration, &repository, "tracker-a");
  let a = prepare(&store, &first, &configuration, &repository).await;
  let b = prepare(&store, &other_installation, &configuration, &repository).await;
  let c = prepare_scoped(
    &store,
    &other_partition,
    &configuration,
    &repository,
    "isolated-scope".parse().unwrap(),
  )
  .await;
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let (left, right) = tokio::join!(
    store.backfill_legacy_issue_work(2, time(50)),
    store.backfill_legacy_issue_work(2, time(50))
  );
  left.unwrap();
  right.unwrap();
  store.backfill_legacy_issue_work(2, time(60)).await.unwrap();
  let mut cards = std::collections::BTreeSet::new();
  for admission in [a.1, b.1, c.1] {
    let association = store
      .issue_work_association(first.project_id(), admission.work.id())
      .await
      .unwrap()
      .unwrap();
    assert!(cards.insert(association.issue_id));
    assert_eq!(
      store.factory_run_snapshot(admission.run.id()).await.unwrap().work,
      admission.work
    );
  }
  assert_eq!(cards.len(), 3);
  database.cleanup().await;
}
