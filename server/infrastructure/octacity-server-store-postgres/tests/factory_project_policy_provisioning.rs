mod support;

use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::NetworkHost;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};
use octacity_server_store_postgres::PostgresStore;
use std::sync::Arc;
use support::{TestDatabase, evidence::PostgresEvidenceProbe};

struct Installed(FactoryConfigurationChoices);
#[async_trait::async_trait]
impl FactoryCapabilitySource for Installed {
  async fn availability(&self, _: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(FactoryConfigurationAvailability::Available(self.0.clone()))
  }
}
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(name: &str, byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([byte; 32]))
}
fn permissions(host: &str) -> FactoryPermissionSet {
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    network_hosts: vec![NetworkHost::new(host).unwrap()],
    ..Default::default()
  })
  .unwrap()
}
fn inventory(
  entries: Vec<(
    FactoryChoiceKind,
    &str,
    ImmutableReference,
    Option<FactoryPermissionSet>,
  )>,
) -> FactoryConfigurationChoices {
  let mut choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: entries
      .iter()
      .map(|(kind, alias, reference, _)| FactoryReferenceChoice {
        kind: *kind,
        alias: key(alias),
        reference: reference.clone(),
      })
      .collect(),
    build_configurations: vec![],
  })
  .unwrap();
  for (kind, alias, _, declarations) in entries {
    choices = choices
      .with_choice_metadata(
        kind,
        key(alias),
        FactoryChoiceMetadata {
          permission_declarations: declarations,
          ..Default::default()
        },
      )
      .unwrap();
  }
  choices
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

async fn create_project(store: &PostgresStore) -> ProjectId {
  let project = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: format!("Policy owner {project}").parse().unwrap(),
      idempotency_key: IdempotencyKey::new(project.to_string()).unwrap(),
      created_at: time(1),
    }))
    .await
    .unwrap();
  project
}

fn provision(
  project: ProjectId,
  designated_access: Option<ImmutableReference>,
  key: &str,
) -> ManagementMutation<ProvisionFactoryProjectPolicies> {
  management_mutation_with_request(
    ProvisionFactoryProjectPolicies {
      intent: FactoryProjectProvisionIntent {
        project_id: project,
        designated_access,
        legacy_references: vec![],
      },
      idempotency_key: IdempotencyKey::new(key).unwrap(),
      provisioned_at: time(2),
    },
    key,
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn clean_project_requires_explicit_provisioning_and_has_no_implicit_execution_grants() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store).await;
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  assert!(
    store
      .factory_project_policy_provisioning(project)
      .await
      .unwrap()
      .is_none()
  );
  assert!(store.factory_project_policy_defaults(project).await.unwrap().is_none());
  assert!(
    store
      .list_factory_project_policies(project, None, 10)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  let request = provision(project, None, "initial-provision");
  let accepted = store.provision_factory_project_policies(request.clone()).await.unwrap();
  assert!(accepted.created);
  assert_eq!(accepted.disposition, MutationDisposition::Applied);
  assert_eq!(accepted.provisioning.project_id, project);
  assert_eq!(
    accepted.provisioning.access_prerequisite,
    Some(FactoryPolicyProvisionPrerequisite::NoDesignatedAccessDefault)
  );
  assert!(accepted.provisioning.access_origin.is_none());
  let FactoryProjectPolicyRecord::Launch(launch) = store
    .factory_project_policy(
      project,
      &FactoryProjectPolicyRef::Launch(accepted.provisioning.standard_launch.clone()),
    )
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("launch")
  };
  assert_eq!(launch.definition().name.as_str(), "Standard");
  assert!(launch.definition().allow_manual && launch.definition().allow_automatic_source);
  let FactoryProjectPolicyRecord::Access(access) = store
    .factory_project_policy(
      project,
      &FactoryProjectPolicyRef::Access(accepted.provisioning.standard_access.clone()),
    )
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("access")
  };
  assert_eq!(access.definition().name.as_str(), "Standard");
  assert_eq!(access.definition().permissions, FactoryPermissionSet::deny_all());
  assert_eq!(accepted.defaults.launch_policy, Some(launch.reference().clone()));
  assert_eq!(accepted.defaults.access_profile, Some(access.reference().clone()));
  assert_eq!(
    store.factory_project_policy_provisioning(project).await.unwrap(),
    Some(accepted.provisioning.clone())
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
  let replay = store.provision_factory_project_policies(request).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.provisioning, accepted.provisioning);
  assert_eq!(replay.defaults, accepted.defaults);
  assert_eq!(evidence.mutation_evidence_counts().await, after);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn designated_default_copies_only_exact_installed_declarations() {
  let database = TestDatabase::migrated().await;
  let selected = reference("reviewed-exact-ceiling", 21);
  let installed = inventory(vec![
    (
      FactoryChoiceKind::PermissionCeiling,
      "a-first",
      reference("other-ceiling", 20),
      Some(permissions("other.example.com")),
    ),
    (
      FactoryChoiceKind::PermissionCeiling,
      "z-selected",
      selected.clone(),
      Some(permissions("selected.example.com")),
    ),
  ]);
  let store = PostgresStore::with_factory_capabilities(database.pool.clone(), Arc::new(Installed(installed)));
  let project = create_project(&store).await;
  let accepted = store
    .provision_factory_project_policies(provision(project, Some(selected.clone()), "reviewed-default"))
    .await
    .unwrap();
  let FactoryProjectPolicyRecord::Access(access) = store
    .factory_project_policy(
      project,
      &FactoryProjectPolicyRef::Access(accepted.provisioning.standard_access),
    )
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("access")
  };
  assert_eq!(access.definition().permissions, permissions("selected.example.com"));
  assert_eq!(accepted.provisioning.access_origin, Some(selected));
  assert!(accepted.provisioning.access_prerequisite.is_none());
  let other = create_project(&store).await;
  let absent = store
    .provision_factory_project_policies(provision(other, None, "no-designation"))
    .await
    .unwrap();
  let FactoryProjectPolicyRecord::Access(access) = store
    .factory_project_policy(
      other,
      &FactoryProjectPolicyRef::Access(absent.provisioning.standard_access),
    )
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("access")
  };
  assert_eq!(
    access.definition().permissions,
    FactoryPermissionSet::deny_all(),
    "multiple installed ceilings do not designate a default"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn existing_configuration_backfill_maps_exact_references_without_rewriting_history() {
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator::with_migrations(
    octacity_server_store_postgres::MIGRATOR
      .iter()
      .filter(|migration| migration.version < 114)
      .cloned()
      .collect(),
  );
  octacity_server_store_postgres::run_migrator(&database.pool, &previous)
    .await
    .unwrap();
  let plain = PostgresStore::new(database.pool.clone());
  let project = create_project(&plain).await;
  let fixture = support::factory_configuration::fixture(project);
  let choices = fixture
    .choices
    .with_choice_metadata(
      FactoryChoiceKind::PermissionCeiling,
      key("permissions"),
      FactoryChoiceMetadata {
        permission_declarations: Some(permissions("legacy.example.com")),
        ..Default::default()
      },
    )
    .unwrap();
  let digest = FactoryDigest::from_bytes([77; 32]);
  let configuration = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    digest,
    fixture.draft.clone(),
    &choices,
  )
  .unwrap();
  plain
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      configuration: configuration.clone(),
      idempotency_key: IdempotencyKey::new("legacy-config").unwrap(),
      published_at: time(1),
      intent: FactoryConfigurationMutationIntent::Create {
        id: configuration.reference().id(),
        project_id: project,
        definition_digest: digest,
        draft: fixture.draft,
      },
    }))
    .await
    .unwrap();
  let bytes_before:(String,Vec<u8>)=sqlx::query_as("SELECT definition::text,definition_digest FROM factory_configuration_versions WHERE factory_configuration_id=$1 AND version=1")
    .bind(configuration.reference().id().as_uuid()).fetch_one(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let store = PostgresStore::with_factory_capabilities(database.pool.clone(), Arc::new(Installed(choices)));
  let launch = FactoryLegacyPolicyReference::Launch(configuration.admission_policy().legacy().unwrap().clone());
  let access = FactoryLegacyPolicyReference::Access(configuration.permission_ceiling().legacy().unwrap().clone());
  let unknown = FactoryLegacyPolicyReference::Access(ImmutableReference::new(
    configuration.permission_ceiling().legacy().unwrap().identity().clone(),
    configuration.permission_ceiling().legacy().unwrap().version().clone(),
    FactoryDigest::from_bytes([99; 32]),
  ));
  assert!(
    store
      .factory_legacy_policy_mapping(project, &access)
      .await
      .unwrap()
      .is_none()
  );
  let admission_selection = ReadFactoryAdmissionContext {
    project_id: project,
    configuration_id: configuration.reference().id(),
    configuration_version: FactoryConfigurationVersion::INITIAL,
    repository_id: None,
    repository_version: None,
  };
  assert_eq!(
    store
      .factory_admission_context(admission_selection)
      .await
      .unwrap()
      .project_policies,
    Err(FactoryPolicyPrerequisite::LaunchUnmapped),
    "a historical Configuration must expose its prerequisite instead of selecting Standard",
  );
  let (mut request, audit) = provision(project, None, "legacy-backfill").into_parts();
  request.intent.legacy_references = vec![launch.clone(), access.clone(), unknown.clone()];
  let accepted = store
    .provision_factory_project_policies(ManagementMutation::new(request.clone(), audit))
    .await
    .unwrap();
  assert_eq!(accepted.mappings.len(), 2);
  assert_eq!(accepted.unresolved.len(), 1);
  assert_eq!(accepted.unresolved[0].legacy, unknown);
  assert_eq!(
    accepted.unresolved[0].reason,
    FactoryPolicyProvisionPrerequisite::ReferenceNotInstalled
  );
  let mapped_launch = store
    .factory_legacy_policy_mapping(project, &launch)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(mapped_launch.legacy, launch);
  assert_eq!(
    mapped_launch.target,
    FactoryProjectPolicyRef::Launch(accepted.provisioning.standard_launch.clone()),
    "the verified legacy launch rules reuse the exact initial Standard version",
  );
  let FactoryProjectPolicyRecord::Launch(rules) = store
    .factory_project_policy(project, &mapped_launch.target)
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("launch")
  };
  assert!(rules.definition().allow_manual && rules.definition().allow_automatic_source);
  let mapped_access = store
    .factory_legacy_policy_mapping(project, &access)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(mapped_access.legacy, access);
  let FactoryProjectPolicyRecord::Access(profile) = store
    .factory_project_policy(project, &mapped_access.target)
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("access")
  };
  assert_eq!(profile.definition().permissions, permissions("legacy.example.com"));
  let resolved = store
    .factory_admission_context(admission_selection)
    .await
    .unwrap()
    .project_policies
    .unwrap();
  assert_eq!(
    FactoryProjectPolicyRef::Launch(resolved.launch_policy.reference().clone()),
    mapped_launch.target
  );
  assert_eq!(
    FactoryProjectPolicyRef::Access(resolved.access_profile.reference().clone()),
    mapped_access.target
  );
  assert!(
    store
      .factory_legacy_policy_mapping(project, &unknown)
      .await
      .unwrap()
      .is_none(),
    "unknown digest must not fall back to Standard or a similar alias"
  );
  let foreign = create_project(&store).await;
  assert!(
    store
      .factory_legacy_policy_mapping(foreign, &access)
      .await
      .unwrap()
      .is_none()
  );
  let restored = plain
    .current_factory_configuration(configuration.reference().id())
    .await
    .unwrap();
  assert_eq!(restored.configuration, configuration);
  let bytes_after:(String,Vec<u8>)=sqlx::query_as("SELECT definition::text,definition_digest FROM factory_configuration_versions WHERE factory_configuration_id=$1 AND version=1")
    .bind(configuration.reference().id().as_uuid()).fetch_one(&database.pool).await.unwrap();
  assert_eq!(bytes_after, bytes_before);
  let reopened = PostgresStore::new(database.pool.clone()); // No installed authority remains.
  request.idempotency_key = IdempotencyKey::new("repeated-backfill").unwrap();
  let repeated = reopened
    .provision_factory_project_policies(management_mutation(request))
    .await
    .unwrap();
  assert!(!repeated.created);
  assert_eq!(repeated.mappings, accepted.mappings);
  assert_eq!(repeated.provisioning, accepted.provisioning);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn provisioning_rejects_duplicate_and_oversized_backfill_batches_atomically() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store).await;
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  let entry = FactoryLegacyPolicyReference::Access(reference("unknown", 1));
  let (mut request, _) = provision(project, None, "duplicates").into_parts();
  request.intent.legacy_references = vec![entry.clone(), entry];
  assert!(matches!(
    store
      .provision_factory_project_policies(management_mutation(request))
      .await,
    Err(StoreError::InvalidInput { .. })
  ));
  let (mut request, _) = provision(project, None, "oversized").into_parts();
  request.intent.legacy_references = (0..257)
    .map(|index| FactoryLegacyPolicyReference::Access(reference(&format!("unknown-{index}"), 1)))
    .collect();
  assert!(matches!(
    store
      .provision_factory_project_policies(management_mutation(request))
      .await,
    Err(StoreError::InvalidInput { .. })
  ));
  assert!(
    store
      .factory_project_policy_provisioning(project)
      .await
      .unwrap()
      .is_none()
  );
  assert!(store.factory_project_policy_defaults(project).await.unwrap().is_none());
  assert!(
    store
      .list_factory_project_policies(project, None, 10)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  database.cleanup().await;
}

struct Unavailable;
#[async_trait::async_trait]
impl FactoryCapabilitySource for Unavailable {
  async fn availability(&self, _: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    Err(StoreError::Unavailable)
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn reprovisioning_and_response_loss_preserve_operator_edits_and_selected_defaults() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store).await;
  let original = provision(project, None, "original-provision");
  let first = store
    .provision_factory_project_policies(original.clone())
    .await
    .unwrap();
  let FactoryProjectPolicyRecord::Launch(launch) = store
    .factory_project_policy(
      project,
      &FactoryProjectPolicyRef::Launch(first.provisioning.standard_launch.clone()),
    )
    .await
    .unwrap()
    .unwrap()
  else {
    panic!("launch")
  };
  let edited = launch
    .replace(
      FactoryLaunchPolicyVersion::new(2).unwrap(),
      FactoryLaunchPolicyDefinition {
        name: FactoryText::new("Operator stop policy").unwrap(),
        allow_manual: false,
        allow_automatic_source: false,
      },
    )
    .unwrap();
  store
    .publish_factory_project_policy(management_mutation(PublishFactoryProjectPolicy {
      intent: FactoryProjectPolicyIntent {
        policy: FactoryProjectPolicyRecord::Launch(edited.clone()),
        expected_current: Some(FactoryProjectPolicyRef::Launch(launch.reference().clone())),
      },
      idempotency_key: IdempotencyKey::new("operator-edit").unwrap(),
      published_at: time(3),
    }))
    .await
    .unwrap();
  let selected = store
    .select_factory_project_policy_defaults(management_mutation(SelectFactoryProjectPolicyDefaults {
      intent: FactoryProjectPolicyDefaultsIntent {
        project_id: project,
        expected_version: Some(1),
        launch_policy: Some(edited.reference().clone()),
        access_profile: None,
      },
      idempotency_key: IdempotencyKey::new("operator-selection").unwrap(),
      published_at: time(4),
    }))
    .await
    .unwrap();
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let reopened = PostgresStore::with_factory_capabilities(pool.clone(), Arc::new(Unavailable));
  let repeated = reopened
    .provision_factory_project_policies(provision(
      project,
      Some(reference("new-installation", 88)),
      "independent-provision",
    ))
    .await
    .unwrap();
  assert!(!repeated.created);
  assert_eq!(repeated.provisioning, first.provisioning);
  assert_eq!(repeated.defaults, selected.defaults);
  let current = reopened.list_factory_project_policies(project, None, 10).await.unwrap();
  assert_eq!(current.items.len(), 2);
  assert!(current.items.iter().any(|item| item.reference
    == FactoryProjectPolicyRef::Launch(edited.reference().clone())
    && item.name.as_str() == "Operator stop policy"));
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  let (mut retry, audit) = original.into_parts();
  retry.provisioned_at = time(900);
  let replay = reopened
    .provision_factory_project_policies(ManagementMutation::new(retry.clone(), audit))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.defaults, first.defaults);
  assert_eq!(replay.provisioned_at, first.provisioned_at);
  retry.intent.designated_access = Some(reference("changed-intent", 89));
  assert!(matches!(
    reopened
      .provision_factory_project_policies(management_mutation(retry))
      .await,
    Err(StoreError::Conflict { .. })
  ));
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_provisioning_and_interrupted_backfill_use_one_atomic_policy_owner() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store).await;
  let installed = reference("legacy-installed-access", 31);
  let choices = inventory(vec![(
    FactoryChoiceKind::PermissionCeiling,
    "legacy",
    installed.clone(),
    Some(permissions("legacy.example.com")),
  )]);
  let configured =
    PostgresStore::with_factory_capabilities(database.pool.clone(), Arc::new(Installed(choices.clone())));
  let (mut request, _) = provision(project, Some(installed.clone()), "interrupted-provision").into_parts();
  let legacy = FactoryLegacyPolicyReference::Access(installed);
  request.intent.legacy_references = vec![legacy.clone()];
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  sqlx::raw_sql("CREATE FUNCTION reject_policy_provision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test interruption'; END $$; CREATE TRIGGER reject_policy_provision BEFORE INSERT ON outbox_entries FOR EACH ROW WHEN (NEW.topic='factory.project-policies-provisioned') EXECUTE FUNCTION reject_policy_provision()")
    .execute(&database.pool).await.unwrap();
  assert!(
    configured
      .provision_factory_project_policies(management_mutation(request.clone()))
      .await
      .is_err()
  );
  assert!(
    configured
      .factory_project_policy_provisioning(project)
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    configured
      .factory_project_policy_defaults(project)
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    configured
      .factory_legacy_policy_mapping(project, &legacy)
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    configured
      .list_factory_project_policies(project, None, 10)
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  sqlx::raw_sql("DROP TRIGGER reject_policy_provision ON outbox_entries; DROP FUNCTION reject_policy_provision()")
    .execute(&database.pool)
    .await
    .unwrap();
  let left_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let right_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let left = PostgresStore::with_factory_capabilities(left_pool.clone(), Arc::new(Installed(choices.clone())));
  let right = PostgresStore::with_factory_capabilities(right_pool.clone(), Arc::new(Installed(choices)));
  let mut competing = request.clone();
  competing.idempotency_key = IdempotencyKey::new("competing-provision").unwrap();
  let (l, r) = tokio::join!(
    left.provision_factory_project_policies(management_mutation(request.clone())),
    right.provision_factory_project_policies(management_mutation(competing))
  );
  let l = l.unwrap();
  let r = r.unwrap();
  assert_ne!(l.created, r.created);
  assert_eq!(l.provisioning, r.provisioning);
  assert_eq!(l.defaults, r.defaults);
  assert_eq!(l.mappings, r.mappings);
  assert_eq!(
    configured
      .list_factory_project_policies(project, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    3
  );
  let replay = configured
    .provision_factory_project_policies(management_mutation(request))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (2, 2, 2)
  );
  left_pool.close().await;
  right_pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn unresolved_exact_access_can_be_imported_later_without_rewriting_its_original_receipt() {
  let database = TestDatabase::migrated().await;
  let exact = reference("exact-access", 51);
  let cases = [
    (
      inventory(vec![(
        FactoryChoiceKind::PermissionCeiling,
        "missing",
        exact.clone(),
        None,
      )]),
      FactoryPolicyProvisionPrerequisite::MissingPermissionDeclarations,
    ),
    (
      inventory(vec![
        (
          FactoryChoiceKind::PermissionCeiling,
          "left",
          exact.clone(),
          Some(permissions("left.example.com")),
        ),
        (
          FactoryChoiceKind::PermissionCeiling,
          "right",
          exact.clone(),
          Some(permissions("right.example.com")),
        ),
      ]),
      FactoryPolicyProvisionPrerequisite::AmbiguousPermissionDeclarations,
    ),
  ];
  for (index, (choices, reason)) in cases.into_iter().enumerate() {
    let store = PostgresStore::with_factory_capabilities(database.pool.clone(), Arc::new(Installed(choices)));
    let project = create_project(&store).await;
    let legacy = FactoryLegacyPolicyReference::Access(exact.clone());
    let (mut request, _) = provision(project, Some(exact.clone()), &format!("unresolved-{index}")).into_parts();
    request.intent.legacy_references = vec![legacy.clone()];
    let original = store
      .provision_factory_project_policies(management_mutation(request.clone()))
      .await
      .unwrap();
    assert_eq!(original.provisioning.access_prerequisite, Some(reason));
    assert!(original.mappings.is_empty());
    assert_eq!(original.unresolved[0].reason, reason);
    assert!(
      store
        .factory_legacy_policy_mapping(project, &legacy)
        .await
        .unwrap()
        .is_none()
    );
    let qualified = PostgresStore::with_factory_capabilities(
      database.pool.clone(),
      Arc::new(Installed(inventory(vec![(
        FactoryChoiceKind::PermissionCeiling,
        "resolved",
        exact.clone(),
        Some(permissions("verified.example.com")),
      )]))),
    );
    let mut new_request = request.clone();
    new_request.idempotency_key = IdempotencyKey::new(format!("resolved-{index}")).unwrap();
    let resolved = qualified
      .provision_factory_project_policies(management_mutation(new_request))
      .await
      .unwrap();
    assert!(!resolved.created);
    assert_eq!(
      resolved.provisioning, original.provisioning,
      "initial Standard profile must not be widened by reprovisioning"
    );
    assert_eq!(resolved.defaults, original.defaults);
    assert_eq!(resolved.mappings.len(), 1);
    assert!(resolved.unresolved.is_empty());
    let FactoryProjectPolicyRecord::Access(access) = qualified
      .factory_project_policy(project, &resolved.mappings[0].target)
      .await
      .unwrap()
      .unwrap()
    else {
      panic!("access")
    };
    assert_eq!(access.definition().permissions, permissions("verified.example.com"));
    let replay = qualified
      .provision_factory_project_policies(management_mutation(request))
      .await
      .unwrap();
    assert_eq!(replay.unresolved, original.unresolved);
    assert!(replay.mappings.is_empty());
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn provisioning_preserves_preexisting_operator_defaults_even_when_names_match_standard() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = create_project(&store).await;
  let custom = FactoryLaunchPolicy::publish(
    FactoryLaunchPolicyId::generate(),
    project,
    FactoryLaunchPolicyDefinition {
      name: FactoryText::new("Standard").unwrap(),
      allow_manual: false,
      allow_automatic_source: false,
    },
  )
  .unwrap();
  store
    .publish_factory_project_policy(management_mutation(PublishFactoryProjectPolicy {
      intent: FactoryProjectPolicyIntent {
        policy: FactoryProjectPolicyRecord::Launch(custom.clone()),
        expected_current: None,
      },
      idempotency_key: IdempotencyKey::new("existing-custom-policy").unwrap(),
      published_at: time(1),
    }))
    .await
    .unwrap();
  let selected = store
    .select_factory_project_policy_defaults(management_mutation(SelectFactoryProjectPolicyDefaults {
      intent: FactoryProjectPolicyDefaultsIntent {
        project_id: project,
        expected_version: None,
        launch_policy: Some(custom.reference().clone()),
        access_profile: None,
      },
      idempotency_key: IdempotencyKey::new("existing-custom-default").unwrap(),
      published_at: time(1),
    }))
    .await
    .unwrap();
  let provisioned = store
    .provision_factory_project_policies(provision(project, None, "backfill-preserve-custom"))
    .await
    .unwrap();
  assert!(provisioned.created);
  assert_eq!(provisioned.defaults, selected.defaults);
  assert_ne!(
    provisioned.provisioning.standard_launch.id,
    custom.reference().id,
    "names must not identify managed initialization state"
  );
  assert_eq!(
    store
      .factory_project_policy(project, &FactoryProjectPolicyRef::Launch(custom.reference().clone()))
      .await
      .unwrap(),
    Some(FactoryProjectPolicyRecord::Launch(custom))
  );
  assert_eq!(
    store
      .list_factory_project_policies(project, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    3
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn catalog_omits_equivalent_imported_launch_but_keeps_its_exact_mapping() {
  let database = TestDatabase::migrated().await;
  let installed = reference("triage-admission", 12);
  let store = PostgresStore::with_factory_capabilities(
    database.pool.clone(),
    Arc::new(Installed(inventory(vec![(
      FactoryChoiceKind::AdmissionPolicy,
      "triage",
      installed.clone(),
      None,
    )]))),
  );
  let project = create_project(&store).await;
  let legacy = FactoryLegacyPolicyReference::Launch(installed);
  let (mut request, audit) = provision(project, None, "catalog-upgrade").into_parts();
  request.intent.legacy_references = vec![legacy.clone()];
  let provisioned = store
    .provision_factory_project_policies(ManagementMutation::new(request, audit))
    .await
    .unwrap();
  let imported = FactoryProjectPolicyRecord::Launch(
    FactoryLaunchPolicy::publish(
      FactoryLaunchPolicyId::generate(),
      project,
      FactoryLaunchPolicyDefinition {
        name: FactoryText::new("Imported triage-admission v1").unwrap(),
        allow_manual: true,
        allow_automatic_source: true,
      },
    )
    .unwrap(),
  );
  let target = imported.reference();
  store
    .publish_factory_project_policy(management_mutation(PublishFactoryProjectPolicy {
      intent: FactoryProjectPolicyIntent {
        policy: imported.clone(),
        expected_current: None,
      },
      idempotency_key: IdempotencyKey::new("previously-imported-launch").unwrap(),
      published_at: time(3),
    }))
    .await
    .unwrap();
  // Reproduce the persisted mapping written by the previous importer.
  let mapping = FactoryLegacyPolicyMapping {
    project_id: project,
    legacy: legacy.clone(),
    target: target.clone(),
  };
  sqlx::query("UPDATE factory_project_policy_mappings SET document=$2 WHERE project_id=$1")
    .bind(project.as_uuid())
    .bind(sqlx::types::Json(&mapping))
    .execute(&database.pool)
    .await
    .unwrap();

  let page = store.list_factory_project_policies(project, None, 2).await.unwrap();
  assert_eq!(page.items.len(), 2);
  assert!(
    page.next_cursor.is_none(),
    "the duplicate must not consume a catalog slot"
  );
  assert_eq!(
    page.items[1].reference,
    FactoryProjectPolicyRef::Launch(provisioned.provisioning.standard_launch)
  );
  assert_eq!(
    store.factory_legacy_policy_mapping(project, &legacy).await.unwrap(),
    Some(mapping)
  );
  assert_eq!(
    store.factory_project_policy(project, &target).await.unwrap(),
    Some(imported)
  );
  database.cleanup().await;
}
