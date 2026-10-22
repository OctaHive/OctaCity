mod support;

use octacity_server_domain::ProjectId;
use octacity_server_store::testing::{
  verify_factory_role_profile_publication, verify_factory_role_profile_replacement,
};
use octacity_server_store_postgres::PostgresStore;
use support::{TestDatabase, evidence::PostgresEvidenceProbe};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn publication_matches_the_shared_memory_contract() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  verify_factory_role_profile_publication(
    &PostgresStore::new(database.pool.clone()),
    &PostgresEvidenceProbe(database.pool.clone()),
    project,
  )
  .await;
  database.cleanup().await;
}

async fn seed_project(pool: &sqlx::PgPool) -> ProjectId {
  let project = ProjectId::generate();
  sqlx::query("INSERT INTO projects (id, name, version, created_at, updated_at) VALUES ($1, $2, 1, to_timestamp(0), to_timestamp(0))")
    .bind(project.as_uuid()).bind(format!("role owner {project}")).execute(pool).await.unwrap();
  project
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn replacement_matches_the_shared_memory_contract() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  verify_factory_role_profile_replacement(
    &PostgresStore::new(database.pool.clone()),
    &PostgresEvidenceProbe(database.pool.clone()),
    project,
  )
  .await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_replacements_have_one_winner_and_recover_original_receipts() {
  use octacity_server_factory::*;
  use octacity_server_store::{
    FactoryRoleProfilePublicationError, FactoryRoleProfileStore as _, MutationDisposition, PublishFactoryRoleProfiles,
    testing::{MutationEvidenceProbe as _, management_mutation_with_request, role_profile_contract_intent},
  };
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let initial = request(role_profile_contract_intent(project), "initial");
  let store = PostgresStore::new(database.pool.clone());
  let original = store
    .publish_factory_role_profiles(management_mutation_with_request(initial, "initial"))
    .await
    .unwrap();
  for include_role in [true, false] {
    let role = store
      .factory_role(project, original.role.as_ref().unwrap().reference().id, None)
      .await
      .unwrap();
    let profile = store
      .factory_role_profile(project, original.profiles[0].reference().id, None)
      .await
      .unwrap();
    let replacement = |label: &str| {
      let mut role_definition = role.definition().clone();
      role_definition.instructions = label.into();
      let role_intent = include_role.then(|| octacity_server_store::FactoryRoleIntent {
        id: role.reference().id,
        expected_current_version: Some(role.reference().version),
        definition: role_definition.clone(),
      });
      let exact_role = if include_role {
        role
          .replace(
            FactoryRoleVersion::new(role.reference().version.get() + 1).unwrap(),
            role_definition,
          )
          .unwrap()
      } else {
        role.clone()
      };
      let mut definition = profile.definition().clone();
      definition.instructions = label.into();
      request(
        octacity_server_store::FactoryRoleProfilesIntent {
          project_id: project,
          role: role_intent,
          profiles: vec![octacity_server_store::FactoryRoleProfileIntent {
            id: profile.reference().id,
            expected_current_version: Some(profile.reference().version),
            role: exact_role.reference().clone(),
            definition,
          }],
        },
        label,
      )
    };
    let left = replacement(if include_role { "batch-left" } else { "profile-left" });
    let right = replacement(if include_role { "batch-right" } else { "profile-right" });
    let left_pool = independent_pool(&database.pool).await;
    let right_pool = independent_pool(&database.pool).await;
    let left_store = PostgresStore::new(left_pool.clone());
    let right_store = PostgresStore::new(right_pool.clone());
    let evidence = PostgresEvidenceProbe(database.pool.clone());
    let before = evidence.mutation_evidence_counts().await;
    let (l, r) = tokio::join!(
      left_store.publish_factory_role_profiles(management_mutation_with_request(left.clone(), "left")),
      right_store.publish_factory_role_profiles(management_mutation_with_request(right.clone(), "right")),
    );
    let (winner, accepted, loser) = match (l, r) {
      (Ok(outcome), Err(FactoryRoleProfilePublicationError::PreconditionFailed)) => (left, outcome, right),
      (Err(FactoryRoleProfilePublicationError::PreconditionFailed), Ok(outcome)) => (right, outcome, left),
      other => panic!("exactly one replacement must commit: {other:?}"),
    };
    assert_eq!(accepted.disposition, MutationDisposition::Applied);
    assert_eq!(
      store
        .factory_role_profile(project, profile.reference().id, None)
        .await
        .unwrap(),
      accepted.profiles[0]
    );
    assert_eq!(
      store
        .factory_role_profile(project, profile.reference().id, Some(profile.reference().version))
        .await
        .unwrap(),
      profile
    );
    if let Some(replaced) = &accepted.role {
      assert_eq!(
        &store.factory_role(project, role.reference().id, None).await.unwrap(),
        replaced
      );
      assert_eq!(
        store
          .factory_role(project, role.reference().id, Some(role.reference().version))
          .await
          .unwrap(),
        role
      );
    }
    left_pool.close().await;
    right_pool.close().await;
    let restarted_pool = independent_pool(&database.pool).await;
    let restarted = PostgresStore::new(restarted_pool.clone());
    let mut retry: PublishFactoryRoleProfiles = winner;
    retry.published_at = octacity_server_domain::Timestamp::from_unix_millis(999).unwrap();
    let replay = restarted
      .publish_factory_role_profiles(management_mutation_with_request(retry, "lost-response-after-restart"))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.role, accepted.role);
    assert_eq!(replay.profiles, accepted.profiles);
    assert_eq!(replay.published_at, accepted.published_at);
    assert!(matches!(
      restarted
        .publish_factory_role_profiles(management_mutation_with_request(loser, "retry-loser"))
        .await,
      Err(FactoryRoleProfilePublicationError::PreconditionFailed)
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
    restarted_pool.close().await;
  }
  database.cleanup().await;
}

fn request(
  intent: octacity_server_store::FactoryRoleProfilesIntent,
  key: &str,
) -> octacity_server_store::PublishFactoryRoleProfiles {
  octacity_server_store::PublishFactoryRoleProfiles {
    intent,
    idempotency_key: octacity_server_store::IdempotencyKey::new(key).unwrap(),
    published_at: octacity_server_domain::Timestamp::from_unix_millis(1).unwrap(),
  }
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
async fn atomicity_matches_the_shared_memory_contract() {
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let foreign = seed_project(&database.pool).await;
  octacity_server_store::testing::verify_factory_role_profile_atomicity(
    &PostgresStore::new(database.pool.clone()),
    &PostgresEvidenceProbe(database.pool.clone()),
    project,
    foreign,
  )
  .await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn failed_commit_leaves_no_publication_and_retry_after_restart_applies_once() {
  use octacity_server_store::{
    FactoryRoleProfileStore as _, MutationDisposition, ReplayFactoryRoleProfiles, StoreError,
    testing::{MutationEvidenceProbe as _, management_mutation, role_profile_contract_intent},
  };
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let publication = request(role_profile_contract_intent(project), "interrupted");
  let writer_pool = independent_pool(&database.pool).await;
  let writer = PostgresStore::new(writer_pool.clone());
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  // Fail the last evidence write, after definitions and audit were staged.
  sqlx::raw_sql("CREATE FUNCTION reject_role_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected publication failure'; END $$; CREATE TRIGGER reject_role_event BEFORE INSERT ON outbox_entries FOR EACH ROW EXECUTE FUNCTION reject_role_event()")
    .execute(&database.pool).await.unwrap();
  assert!(
    writer
      .publish_factory_role_profiles(management_mutation(publication.clone()))
      .await
      .is_err()
  );
  writer_pool.close().await;
  let restarted_pool = independent_pool(&database.pool).await;
  let restarted = PostgresStore::new(restarted_pool.clone());
  let probe = || {
    management_mutation(ReplayFactoryRoleProfiles {
      intent: publication.intent.clone(),
      idempotency_key: publication.idempotency_key.clone(),
    })
  };
  assert_eq!(restarted.replay_factory_role_profiles(probe()).await.unwrap(), None);
  assert!(matches!(
    restarted
      .factory_role(project, publication.intent.role.as_ref().unwrap().id, None)
      .await,
    Err(StoreError::NotFound { .. })
  ));
  for profile in &publication.intent.profiles {
    assert!(matches!(
      restarted.factory_role_profile(project, profile.id, None).await,
      Err(StoreError::NotFound { .. })
    ));
  }
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  sqlx::query("DROP TRIGGER reject_role_event ON outbox_entries")
    .execute(&database.pool)
    .await
    .unwrap();
  let accepted = restarted
    .publish_factory_role_profiles(management_mutation(publication.clone()))
    .await
    .unwrap();
  assert_eq!(accepted.disposition, MutationDisposition::Applied);
  assert_eq!(
    restarted
      .replay_factory_role_profiles(probe())
      .await
      .unwrap()
      .unwrap()
      .profiles,
    accepted.profiles
  );
  assert_eq!(
    restarted
      .publish_factory_role_profiles(management_mutation(publication))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (1, 1, 1)
  );
  restarted_pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn upgrade_preserves_static_configuration_and_exact_flow_without_role_catalog() {
  use octacity_server_factory::*;
  use octacity_server_store::{
    CreateFactoryConfiguration, FactoryConfigurationMutationIntent, FactoryConfigurationStore as _,
    testing::management_mutation,
  };
  let database = TestDatabase::empty().await;
  let before_roles = sqlx::migrate::Migrator::with_migrations(
    octacity_server_store_postgres::MIGRATOR
      .iter()
      .filter(|migration| migration.version < 98)
      .cloned()
      .collect(),
  );
  octacity_server_store_postgres::run_migrator(&database.pool, &before_roles)
    .await
    .unwrap();
  let project = seed_project(&database.pool).await;
  let fixture = support::factory_configuration::fixture(project);
  let definition = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    FactoryDigest::from_bytes([3; 32]),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  let reference = definition.reference();
  let store = PostgresStore::new(database.pool.clone());
  let original = store
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      configuration: definition.clone(),
      idempotency_key: octacity_server_store::IdempotencyKey::new("static-before-roles").unwrap(),
      published_at: octacity_server_domain::Timestamp::from_unix_millis(1).unwrap(),
      intent: FactoryConfigurationMutationIntent::Create {
        id: reference.id(),
        project_id: project,
        definition_digest: reference.definition_digest(),
        draft: fixture.draft,
      },
    }))
    .await
    .unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let restarted_pool = independent_pool(&database.pool).await;
  let restarted = PostgresStore::new(restarted_pool.clone());
  assert_eq!(
    restarted.current_factory_configuration(reference.id()).await.unwrap(),
    original.configuration
  );
  assert_eq!(
    restarted
      .factory_configuration_version(reference.id(), reference.version())
      .await
      .unwrap()
      .configuration
      .flow(),
    definition.flow()
  );
  let role_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM factory_roles")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(role_count, 0, "upgrading must not invent AI agents for static Flows");
  verify_factory_role_profile_publication(&restarted, &PostgresEvidenceProbe(database.pool.clone()), project).await;
  restarted_pool.close().await;
  database.cleanup().await;
}
