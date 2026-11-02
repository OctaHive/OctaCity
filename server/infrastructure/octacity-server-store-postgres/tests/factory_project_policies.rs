mod support;

use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::NetworkHost;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};
use octacity_server_store_postgres::PostgresStore;
use support::{TestDatabase, evidence::PostgresEvidenceProbe};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn publications_match_the_backend_neutral_project_policy_contract() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  verify_factory_project_policy_publication(
    &PostgresStore::new(database.pool.clone()),
    &PostgresEvidenceProbe(database.pool.clone()),
    launch(project),
  )
  .await;
  database.cleanup().await;
}

fn launch(project: ProjectId) -> FactoryProjectPolicyRecord {
  FactoryProjectPolicyRecord::Launch(
    FactoryLaunchPolicy::publish(
      "10000000-0000-4000-8000-000000000001".parse().unwrap(),
      project,
      FactoryLaunchPolicyDefinition {
        name: FactoryText::new("Operator policy").unwrap(),
        allow_manual: true,
        allow_automatic_source: false,
      },
    )
    .unwrap(),
  )
}

async fn seed_project(pool: &sqlx::PgPool) -> ProjectId {
  let project = ProjectId::generate();
  sqlx::query(
    "INSERT INTO projects (id,name,version,created_at,updated_at) VALUES ($1,$2,1,to_timestamp(0),to_timestamp(0))",
  )
  .bind(project.as_uuid())
  .bind(format!("policy owner {project}"))
  .execute(pool)
  .await
  .unwrap();
  project
}

fn replacement(policy: &FactoryProjectPolicyRecord, name: &str) -> FactoryProjectPolicyRecord {
  match policy {
    FactoryProjectPolicyRecord::Launch(value) => {
      let mut definition = value.definition().clone();
      definition.name = FactoryText::new(name).unwrap();
      FactoryProjectPolicyRecord::Launch(
        value
          .replace(
            FactoryLaunchPolicyVersion::new(value.reference().version.get() + 1).unwrap(),
            definition,
          )
          .unwrap(),
      )
    }
    FactoryProjectPolicyRecord::Access(value) => {
      let mut definition = value.definition().clone();
      definition.name = FactoryText::new(name).unwrap();
      FactoryProjectPolicyRecord::Access(
        value
          .replace(
            FactoryAccessProfileVersion::new(value.reference().version.get() + 1).unwrap(),
            definition,
          )
          .unwrap()
          .into(),
      )
    }
  }
}

fn publication(
  policy: FactoryProjectPolicyRecord,
  expected_current: Option<FactoryProjectPolicyRef>,
  key: &str,
) -> ManagementMutation<PublishFactoryProjectPolicy> {
  management_mutation_with_request(
    PublishFactoryProjectPolicy {
      intent: FactoryProjectPolicyIntent {
        policy,
        expected_current,
      },
      idempotency_key: IdempotencyKey::new(key).unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    },
    key,
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn publication_rejects_skipped_versions_before_creating_history() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let initial = launch(seed_project(&database.pool).await);
  let second = replacement(&initial, "second");
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  assert!(matches!(
    store
      .publish_factory_project_policy(publication(second.clone(), None, "skip-initial"))
      .await,
    Err(FactoryProjectPolicyPublicationError::Store(
      StoreError::InvalidInput { .. }
    ))
  ));
  assert!(
    store
      .factory_project_policy(second.reference().project_id(), &second.reference())
      .await
      .unwrap()
      .is_none()
  );
  store
    .publish_factory_project_policy(publication(initial.clone(), None, "initial"))
    .await
    .unwrap();
  let third = replacement(&second, "third");
  assert!(matches!(
    store
      .publish_factory_project_policy(publication(third, Some(initial.reference()), "skip-replacement"))
      .await,
    Err(FactoryProjectPolicyPublicationError::Store(
      StoreError::InvalidInput { .. }
    ))
  ));
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (1, 1, 1)
  );
  database.cleanup().await;
}

fn access(project: ProjectId) -> FactoryProjectPolicyRecord {
  FactoryProjectPolicyRecord::Access(
    FactoryAccessProfile::publish(
      "30000000-0000-4000-8000-000000000001".parse().unwrap(),
      project,
      FactoryAccessProfileDefinition {
        name: FactoryText::new("Read selected service").unwrap(),
        permissions: FactoryPermissionSet::try_new(FactoryPermissionDraft {
          network_hosts: vec![NetworkHost::new("api.example.com").unwrap()],
          ..Default::default()
        })
        .unwrap(),
      },
    )
    .unwrap()
    .into(),
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn defaults_retain_exact_owned_versions_and_require_an_explicit_selection() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = seed_project(&database.pool).await;
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let original_counts = evidence.mutation_evidence_counts().await;
  assert!(store.factory_project_policy_defaults(project).await.unwrap().is_none());
  assert_eq!(
    evidence.mutation_evidence_counts().await,
    original_counts,
    "reads must not provision defaults"
  );
  let launch = launch(project);
  let access = access(project);
  let FactoryProjectPolicyRef::Launch(launch_ref) = launch.reference() else {
    unreachable!()
  };
  let FactoryProjectPolicyRef::Access(access_ref) = access.reference() else {
    unreachable!()
  };
  let select = SelectFactoryProjectPolicyDefaults {
    intent: FactoryProjectPolicyDefaultsIntent {
      project_id: project,
      expected_version: None,
      launch_policy: Some(launch_ref.clone()),
      access_profile: Some(access_ref.clone()),
    },
    idempotency_key: IdempotencyKey::new("select-defaults").unwrap(),
    published_at: Timestamp::from_unix_millis(2).unwrap(),
  };
  assert!(
    store
      .select_factory_project_policy_defaults(management_mutation(select.clone()))
      .await
      .is_err(),
    "unpublished pins cannot become defaults"
  );
  assert_eq!(evidence.mutation_evidence_counts().await, original_counts);
  store
    .publish_factory_project_policy(publication(launch.clone(), None, "launch"))
    .await
    .unwrap();
  store
    .publish_factory_project_policy(publication(access.clone(), None, "access"))
    .await
    .unwrap();
  let before = evidence.mutation_evidence_counts().await;
  let first = store
    .select_factory_project_policy_defaults(management_mutation(select.clone()))
    .await
    .unwrap();
  assert_eq!(first.defaults.version, 1);
  assert_eq!(first.defaults.launch_policy, Some(launch_ref));
  assert_eq!(first.defaults.access_profile, Some(access_ref));
  let next = replacement(&launch, "updated launch");
  store
    .publish_factory_project_policy(publication(next, Some(launch.reference()), "launch-update"))
    .await
    .unwrap();
  assert_eq!(
    store.factory_project_policy_defaults(project).await.unwrap(),
    Some(first.defaults.clone()),
    "policy updates must not repin defaults"
  );
  let mut clear = select.clone();
  clear.intent.expected_version = Some(1);
  clear.intent.launch_policy = None;
  clear.intent.access_profile = None;
  clear.idempotency_key = IdempotencyKey::new("clear-defaults").unwrap();
  let second = store
    .select_factory_project_policy_defaults(management_mutation(clear.clone()))
    .await
    .unwrap();
  assert_eq!(second.defaults.version, 2);
  assert!(second.defaults.launch_policy.is_none() && second.defaults.access_profile.is_none());
  clear.idempotency_key = IdempotencyKey::new("stale-defaults").unwrap();
  assert!(matches!(
    store
      .select_factory_project_policy_defaults(management_mutation(clear))
      .await,
    Err(FactoryProjectPolicyPublicationError::PreconditionFailed)
  ));
  let mut retry = select;
  retry.published_at = Timestamp::from_unix_millis(500).unwrap();
  let replay = store
    .select_factory_project_policy_defaults(management_mutation(retry.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.defaults, first.defaults);
  assert_eq!(replay.published_at, first.published_at);
  retry.intent.launch_policy = None;
  assert!(matches!(
    store
      .select_factory_project_policy_defaults(management_mutation(retry))
      .await,
    Err(FactoryProjectPolicyPublicationError::Store(StoreError::Conflict { .. }))
  ));
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (3, 3, 3)
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn access_publications_match_the_same_backend_neutral_contract() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  verify_factory_project_policy_publication(
    &PostgresStore::new(database.pool.clone()),
    &PostgresEvidenceProbe(database.pool.clone()),
    access(project),
  )
  .await;
  database.cleanup().await;
}

async fn independent_pool(pool: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*pool.connect_options()).clone())
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn competing_policy_replacements_and_response_loss_preserve_exact_history() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let foreign = seed_project(&database.pool).await;
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  for (label, initial) in [("launch", launch(project)), ("access", access(project))] {
    let store = PostgresStore::new(database.pool.clone());
    store
      .publish_factory_project_policy(publication(initial.clone(), None, &format!("{label}-initial")))
      .await
      .unwrap();
    let left = publication(
      replacement(&initial, "left"),
      Some(initial.reference()),
      &format!("{label}-left"),
    );
    let right = publication(
      replacement(&initial, "right"),
      Some(initial.reference()),
      &format!("{label}-right"),
    );
    let left_pool = independent_pool(&database.pool).await;
    let right_pool = independent_pool(&database.pool).await;
    let left_store = PostgresStore::new(left_pool.clone());
    let right_store = PostgresStore::new(right_pool.clone());
    let before = evidence.mutation_evidence_counts().await;
    let (l, r) = tokio::join!(
      left_store.publish_factory_project_policy(left.clone()),
      right_store.publish_factory_project_policy(right.clone())
    );
    let (winner, accepted) = match (l, r) {
      (Ok(value), Err(FactoryProjectPolicyPublicationError::PreconditionFailed)) => (left, value),
      (Err(FactoryProjectPolicyPublicationError::PreconditionFailed), Ok(value)) => (right, value),
      other => panic!("one exact head must win: {other:?}"),
    };
    let after = evidence.mutation_evidence_counts().await;
    assert_eq!(
      (
        after.idempotency - before.idempotency,
        after.audit - before.audit,
        after.outbox - before.outbox
      ),
      (1, 1, 1)
    );
    store
      .publish_factory_project_policy(publication(
        replacement(&accepted.policy, "third"),
        Some(accepted.policy.reference()),
        &format!("{label}-third"),
      ))
      .await
      .unwrap();
    left_pool.close().await;
    right_pool.close().await;
    let reopened_pool = independent_pool(&database.pool).await;
    let reopened = PostgresStore::new(reopened_pool.clone());
    let before_replay = evidence.mutation_evidence_counts().await;
    let (mut request, _) = winner.clone().into_parts();
    request.published_at = Timestamp::from_unix_millis(900).unwrap();
    let replay = reopened
      .publish_factory_project_policy(management_mutation_with_request(request.clone(), "response-loss"))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.policy, accepted.policy);
    assert_eq!(replay.published_at, accepted.published_at);
    request.intent.policy = replacement(&accepted.policy, "different intent");
    assert!(matches!(
      reopened
        .publish_factory_project_policy(management_mutation(request))
        .await,
      Err(FactoryProjectPolicyPublicationError::Store(StoreError::Conflict { .. }))
    ));
    let (request, _) = winner.into_parts();
    let other_scope = MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some("other-operator".into()),
      },
      ManagementSecurityScope::new("other-operator").unwrap(),
      "other-scope",
    )
    .unwrap();
    assert!(
      matches!(
        reopened
          .publish_factory_project_policy(ManagementMutation::new(request, other_scope))
          .await,
        Err(FactoryProjectPolicyPublicationError::PreconditionFailed)
      ),
      "another scope must not receive an accepted receipt"
    );
    assert_eq!(
      reopened
        .factory_project_policy(project, &initial.reference())
        .await
        .unwrap(),
      Some(initial.clone())
    );
    assert!(
      reopened
        .factory_project_policy(foreign, &initial.reference())
        .await
        .unwrap()
        .is_none()
    );
    let mut forged = initial.reference();
    match &mut forged {
      FactoryProjectPolicyRef::Launch(r) => {
        r.project_id = foreign;
      }
      FactoryProjectPolicyRef::Access(r) => {
        r.project_id = foreign;
      }
    }
    assert!(
      reopened
        .factory_project_policy(foreign, &forged)
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(evidence.mutation_evidence_counts().await, before_replay);
    reopened_pool.close().await;
  }
  database.cleanup().await;
}

fn defaults_selection(
  project: ProjectId,
  launch: Option<FactoryLaunchPolicyRef>,
  access: Option<FactoryAccessProfileRef>,
  expected: Option<u64>,
  key: &str,
) -> ManagementMutation<SelectFactoryProjectPolicyDefaults> {
  management_mutation_with_request(
    SelectFactoryProjectPolicyDefaults {
      intent: FactoryProjectPolicyDefaultsIntent {
        project_id: project,
        expected_version: expected,
        launch_policy: launch,
        access_profile: access,
      },
      idempotency_key: IdempotencyKey::new(key).unwrap(),
      published_at: Timestamp::from_unix_millis(3).unwrap(),
    },
    key,
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn outbox_interruption_rolls_back_policy_and_defaults_without_reserving_replay_keys() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let initial = [launch(project), access(project)];
  for (index, record) in initial.iter().enumerate() {
    store
      .publish_factory_project_policy(publication(record.clone(), None, &format!("initial-{index}")))
      .await
      .unwrap();
  }
  let FactoryProjectPolicyRef::Launch(pin) = initial[0].reference() else {
    unreachable!()
  };
  let selection = defaults_selection(project, Some(pin), None, None, "interrupted-selection");
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  // Fault injection at the common evidence boundary, after content has been appended in the transaction.
  sqlx::raw_sql("CREATE FUNCTION reject_policy_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test outbox interruption'; END $$; CREATE TRIGGER reject_policy_event BEFORE INSERT ON outbox_entries FOR EACH ROW WHEN (NEW.topic IN ('factory.project-policy-published', 'factory.project-policy-defaults-selected')) EXECUTE FUNCTION reject_policy_event()")
    .execute(&database.pool).await.unwrap();
  for (index, record) in initial.iter().enumerate() {
    let next = replacement(record, "interrupted");
    assert!(
      store
        .publish_factory_project_policy(publication(
          next.clone(),
          Some(record.reference()),
          &format!("interrupted-{index}")
        ))
        .await
        .is_err()
    );
    assert!(
      store
        .factory_project_policy(project, &next.reference())
        .await
        .unwrap()
        .is_none()
    );
  }
  assert!(
    store
      .select_factory_project_policy_defaults(selection.clone())
      .await
      .is_err()
  );
  assert!(store.factory_project_policy_defaults(project).await.unwrap().is_none());
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  sqlx::raw_sql("DROP TRIGGER reject_policy_event ON outbox_entries; DROP FUNCTION reject_policy_event()")
    .execute(&database.pool)
    .await
    .unwrap();
  for (index, record) in initial.iter().enumerate() {
    let outcome = store
      .publish_factory_project_policy(publication(
        replacement(record, "interrupted"),
        Some(record.reference()),
        &format!("interrupted-{index}"),
      ))
      .await
      .unwrap();
    assert_eq!(outcome.disposition, MutationDisposition::Applied);
  }
  let defaults = store.select_factory_project_policy_defaults(selection).await.unwrap();
  assert_eq!(defaults.disposition, MutationDisposition::Applied);
  assert_eq!(defaults.defaults.version, 1);
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (3, 3, 3)
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn default_selections_are_fenced_owned_and_replayed_after_reopening() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let foreign = seed_project(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let launch = launch(project);
  store
    .publish_factory_project_policy(publication(launch.clone(), None, "owned-launch"))
    .await
    .unwrap();
  let FactoryProjectPolicyRef::Launch(pin) = launch.reference() else {
    unreachable!()
  };
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  for expected in [Some(0), Some(u64::MAX), Some(i64::MAX as u64)] {
    assert!(matches!(
      store
        .select_factory_project_policy_defaults(defaults_selection(
          project,
          Some(pin.clone()),
          None,
          expected,
          &format!("invalid-{expected:?}")
        ))
        .await,
      Err(FactoryProjectPolicyPublicationError::Store(
        StoreError::InvalidInput { .. }
      ))
    ));
  }
  assert!(matches!(
    store
      .select_factory_project_policy_defaults(defaults_selection(
        foreign,
        Some(pin.clone()),
        None,
        None,
        "foreign-default"
      ))
      .await,
    Err(FactoryProjectPolicyPublicationError::Store(
      StoreError::InvalidInput { .. }
    ))
  ));
  let mut forged = pin.clone();
  forged.project_id = foreign;
  assert!(
    store
      .select_factory_project_policy_defaults(defaults_selection(foreign, Some(forged), None, None, "forged-default"))
      .await
      .is_err()
  );
  let mut forged = pin.clone();
  forged.digest = FactoryDigest::from_bytes([233; 32]);
  assert!(
    store
      .select_factory_project_policy_defaults(defaults_selection(project, Some(forged), None, None, "wrong-digest"))
      .await
      .is_err()
  );
  assert!(store.factory_project_policy_defaults(foreign).await.unwrap().is_none());
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  store
    .select_factory_project_policy_defaults(defaults_selection(
      project,
      Some(pin.clone()),
      None,
      None,
      "initial-default",
    ))
    .await
    .unwrap();
  let left = defaults_selection(project, Some(pin), None, Some(1), "left-default");
  let right = defaults_selection(project, None, None, Some(1), "right-default");
  let lp = independent_pool(&database.pool).await;
  let rp = independent_pool(&database.pool).await;
  let ls = PostgresStore::new(lp.clone());
  let rs = PostgresStore::new(rp.clone());
  let before_race = evidence.mutation_evidence_counts().await;
  let (l, r) = tokio::join!(
    ls.select_factory_project_policy_defaults(left.clone()),
    rs.select_factory_project_policy_defaults(right.clone())
  );
  let (winner, accepted) = match (l, r) {
    (Ok(value), Err(FactoryProjectPolicyPublicationError::PreconditionFailed)) => (left, value),
    (Err(FactoryProjectPolicyPublicationError::PreconditionFailed), Ok(value)) => (right, value),
    other => panic!("one default revision must win: {other:?}"),
  };
  let after_race = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after_race.idempotency - before_race.idempotency,
      after_race.audit - before_race.audit,
      after_race.outbox - before_race.outbox
    ),
    (1, 1, 1)
  );
  store
    .select_factory_project_policy_defaults(defaults_selection(project, None, None, Some(2), "third-default"))
    .await
    .unwrap();
  lp.close().await;
  rp.close().await;
  let reopened_pool = independent_pool(&database.pool).await;
  let reopened = PostgresStore::new(reopened_pool.clone());
  let before_replay = evidence.mutation_evidence_counts().await;
  let (mut retry, _) = winner.into_parts();
  retry.published_at = Timestamp::from_unix_millis(999).unwrap();
  let replay = reopened
    .select_factory_project_policy_defaults(management_mutation(retry.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.defaults, accepted.defaults);
  assert_eq!(replay.published_at, accepted.published_at);
  let scope = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("other-operator".into()),
    },
    ManagementSecurityScope::new("other-operator").unwrap(),
    "foreign-default-replay",
  )
  .unwrap();
  assert!(matches!(
    reopened
      .select_factory_project_policy_defaults(ManagementMutation::new(retry, scope))
      .await,
    Err(FactoryProjectPolicyPublicationError::PreconditionFailed)
  ));
  assert_eq!(
    reopened
      .factory_project_policy_defaults(project)
      .await
      .unwrap()
      .unwrap()
      .version,
    3
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before_replay);
  let defaults_audits: Vec<_> = evidence
    .management_audit_facts()
    .await
    .into_iter()
    .filter(|fact| fact.operation == StoreOperation::SelectFactoryProjectPolicyDefaults)
    .collect();
  assert_eq!(defaults_audits.len(), 3);
  assert!(
    defaults_audits
      .iter()
      .all(|fact| fact.target_identity == project.to_string()
        && fact.actor_kind == AuditActorKind::UnauthenticatedManagement)
  );
  reopened_pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn policy_operations_retain_the_accepted_authenticated_actor_and_scope() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let foreign = seed_project(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let audit = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("operator-allowed-project".into()),
    },
    ManagementSecurityScope::new("operator-project-grant").unwrap(),
    "authenticated-policy-command",
  )
  .unwrap();
  let policy = launch(project);
  let (request, _) = publication(policy.clone(), None, "authenticated-create").into_parts();
  store
    .publish_factory_project_policy(ManagementMutation::new(request, audit.clone()))
    .await
    .unwrap();
  let FactoryProjectPolicyRef::Launch(pin) = policy.reference() else {
    unreachable!()
  };
  let (request, _) = defaults_selection(project, Some(pin), None, None, "authenticated-default").into_parts();
  store
    .select_factory_project_policy_defaults(ManagementMutation::new(request, audit.clone()))
    .await
    .unwrap();
  let before = evidence.mutation_evidence_counts().await;
  let foreign_policy = launch(foreign);
  let second_foreign = replacement(&foreign_policy, "foreign replacement");
  assert!(matches!(
    store
      .publish_factory_project_policy(publication(
        second_foreign,
        Some(policy.reference()),
        "foreign-precondition"
      ))
      .await,
    Err(FactoryProjectPolicyPublicationError::Store(
      StoreError::InvalidInput { .. }
    ))
  ));
  assert!(
    store
      .factory_project_policy(foreign, &foreign_policy.reference())
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  let facts = evidence.management_audit_facts().await;
  assert_eq!(facts.len(), 2);
  for fact in facts {
    assert_eq!(fact.actor_kind, AuditActorKind::AuthenticatedManagement);
    assert_eq!(fact.actor_identity.as_deref(), Some("operator-allowed-project"));
    assert_eq!(fact.request_identity, "authenticated-policy-command");
    assert_eq!(fact.target_identity, project.to_string());
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn catalog_pages_contain_only_current_owned_policy_summaries() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let foreign = seed_project(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let launch = launch(project);
  let access = access(project);
  store
    .publish_factory_project_policy(publication(launch.clone(), None, "launch"))
    .await
    .unwrap();
  store
    .publish_factory_project_policy(publication(access.clone(), None, "access"))
    .await
    .unwrap();
  let next = replacement(&launch, "Updated operator policy");
  store
    .publish_factory_project_policy(publication(next.clone(), Some(launch.reference()), "replacement"))
    .await
    .unwrap();
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  let first = store.list_factory_project_policies(project, None, 1).await.unwrap();
  assert_eq!(first.items.len(), 1);
  assert_eq!(first.items[0].reference, access.reference());
  assert_eq!(first.items[0].name.as_str(), "Read selected service");
  assert_eq!(first.next_cursor, Some(access.reference()));
  let second = store
    .list_factory_project_policies(project, first.next_cursor.as_ref(), 1)
    .await
    .unwrap();
  assert_eq!(second.items.len(), 1);
  assert_eq!(second.items[0].reference, next.reference());
  assert_eq!(second.items[0].name.as_str(), "Updated operator policy");
  assert!(second.next_cursor.is_none());
  assert!(
    store
      .list_factory_project_policies(foreign, None, 1)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert!(
    store
      .list_factory_project_policies(foreign, Some(&launch.reference()), 1)
      .await
      .is_err()
  );
  for limit in [0, MAX_CURRENT_DEFINITION_PAGE_SIZE + 1] {
    assert!(store.list_factory_project_policies(project, None, limit).await.is_err());
  }
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  database.cleanup().await;
}
