//! Retention and agent-wide quota contracts over ordinary Factory Build boundaries.
use super::*;

fn retained_role_fixture(intent: &FlowBuildIntent) -> RetainedFactoryRoleInvocation {
  let context = intent.input().context().unwrap().clone();
  let task = context
    .entries()
    .iter()
    .find_map(|entry| match (entry.source_kind(), entry.source()) {
      (ContextSourceKind::Task, FactoryContextReference::Artifact(file)) => Some(file.clone()),
      _ => None,
    })
    .unwrap();
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    intent.input().subject().project_id(),
    serde_json::from_value(
      serde_json::json!({"name":"Custom agent", "instructions":"Original private role", "concurrency_limit":1}),
    )
    .unwrap(),
  )
  .unwrap();
  let mut definition = octacity_server_store::testing::role_profile_contract_definition(role.reference().project_id);
  definition.skills.clear();
  definition.context_requirements.clear();
  definition.permissions = intent.permissions().clone();
  definition.budget = intent.budget();
  definition.toolchain = FactoryTaskToolchain {
    plugin: intent.profile().plugin.clone(),
    executable: intent.profile().tool.clone(),
    model: intent.profile().model_or_tool.clone(),
  };
  definition.build_configuration = intent.profile().build_configuration.clone();
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    role.reference().project_id,
    &role,
    definition,
  )
  .unwrap();
  let task = FactoryTaskEnvelope::new(
    FactoryTaskDeclaration {
      id: TaskEnvelopeId::generate(),
      node_attempt_id: intent.node().id(),
      subject: context.subject().clone(),
      mode: FactoryTaskMode::Observe,
    },
    FactoryTaskDefinition {
      task,
      specifications: vec![],
      prior_findings: vec![],
      permissions: intent.permissions().clone(),
      budget: intent.budget(),
      result_schema: FactoryTaskResultSchema::FlowV1,
      deliverables: vec![],
    },
    &context,
    profile.definition().toolchain.clone(),
    FactoryTaskControlDigests {
      policy: FactoryDigest::from_bytes([1; 32]),
      prompt: FactoryDigest::from_bytes([2; 32]),
      prompt_provenance: FactoryDigest::from_bytes([3; 32]),
      provenance: FactoryDigest::from_bytes([4; 32]),
    },
  )
  .unwrap();
  RetainedFactoryRoleInvocation {
    package: FactoryRoleExecutionPackage::new(FactoryRoleExecutionPackageInput {
      project_rules: vec![],
      role,
      profile,
      skills: Default::default(),
      task,
      context,
    })
    .unwrap(),
    skills: None,
    secret_variables: Default::default(),
  }
}

async fn publish_role_package_catalog<S: FactoryRoleProfileStore>(store: &S, retained: &RetainedFactoryRoleInvocation) {
  let inputs = retained.package.inputs();
  store
    .publish_factory_role_profiles(octacity_server_store::testing::management_mutation(
      PublishFactoryRoleProfiles {
        intent: FactoryRoleProfilesIntent {
          project_id: inputs.role.reference().project_id,
          role: Some(FactoryRoleIntent {
            id: inputs.role.reference().id,
            expected_current_version: None,
            definition: inputs.role.definition().clone(),
          }),
          profiles: vec![FactoryRoleProfileIntent {
            id: inputs.profile.reference().id,
            expected_current_version: None,
            role: inputs.role.reference().clone(),
            definition: inputs.profile.definition().clone(),
          }],
        },
        idempotency_key: "retained-role-create".parse().unwrap(),
        published_at: at(12),
      },
    ))
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn role_package_recovers_exact_private_content_after_restart_and_catalog_replacement() {
  let (database, store, build) = prepared_factory_build(true).await;
  let retained = retained_role_fixture(&build.intent);
  assert!(
    store
      .retain_factory_role_invocation(&build.intent, &build.ownership, retained.clone(), at(12))
      .await
      .is_err(),
    "package retention must pin existing owned definitions"
  );
  publish_role_package_catalog(store.as_ref(), &retained).await;
  store
    .retain_factory_role_invocation(&build.intent, &build.ownership, retained.clone(), at(12))
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let operation = build.intent.operation_id().unwrap();
  let recovered = restarted.factory_role_invocation(operation).await.unwrap().unwrap();
  assert!(recovered == retained);
  let mut substituted = retained.clone();
  let mut inputs = retained.package.inputs().clone();
  let mut role = inputs.role.definition().clone();
  role.instructions = FactoryInstructions::new(vec!["Replacement private role".into()]).unwrap();
  // A replacement is a new catalog version, never a mutation of an accepted invocation.
  let replacement = inputs.role.replace(FactoryRoleVersion::new(2).unwrap(), role).unwrap();
  store
    .publish_factory_role_profiles(octacity_server_store::testing::management_mutation(
      PublishFactoryRoleProfiles {
        intent: FactoryRoleProfilesIntent {
          project_id: replacement.reference().project_id,
          role: Some(FactoryRoleIntent {
            id: replacement.reference().id,
            expected_current_version: Some(retained.package.inputs().role.reference().version),
            definition: replacement.definition().clone(),
          }),
          profiles: vec![],
        },
        idempotency_key: "replace-retained-role".parse().unwrap(),
        published_at: at(13),
      },
    ))
    .await
    .unwrap();
  assert_eq!(
    store
      .factory_role(replacement.reference().project_id, replacement.reference().id, None)
      .await
      .unwrap()
      .reference(),
    replacement.reference()
  );
  let profile = inputs.profile.definition().clone();
  inputs.profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    replacement.reference().project_id,
    &replacement,
    profile,
  )
  .unwrap();
  inputs.role = replacement;
  substituted.package = FactoryRoleExecutionPackage::new(inputs).unwrap();
  assert!(
    restarted
      .retain_factory_role_invocation(&build.intent, &build.ownership, substituted, at(13))
      .await
      .is_err()
  );
  assert!(restarted.factory_role_invocation(operation).await.unwrap().unwrap() == retained);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn role_package_inputs_outlive_download_deadlines_until_run_retention_releases_them() {
  let (database, store, build) = prepared_factory_build(true).await;
  let retained = retained_role_fixture(&build.intent);
  publish_role_package_catalog(store.as_ref(), &retained).await;
  store
    .retain_factory_role_invocation(&build.intent, &build.ownership, retained.clone(), at(12))
    .await
    .unwrap();
  struct Bytes;
  #[async_trait]
  impl octacity_artifact_store::ArtifactBytePublisher for Bytes {
    async fn publish_verified(
      &self,
      _: &octacity_artifact_store::ArtifactObject,
      _: Vec<u8>,
    ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
      Ok(())
    }
  }
  let publisher = FactoryExecutionInputPublisher::new(store.clone(), Arc::new(Bytes), Arc::new(BuildClock));
  let manifest = publisher
    .publish(
      &build.intent,
      &build.ownership,
      vec![
        FactoryExecutionDocument::new(
          "/octacity/protected/role-execution-package.json",
          "application/json",
          retained.package.canonical_bytes().unwrap(),
        )
        .unwrap(),
      ],
    )
    .await
    .unwrap();
  let artifact = manifest.inputs[0].artifact_id.parse().unwrap();
  let published = store.published_execution_input(artifact).await.unwrap();
  assert_eq!(published.expires_at, Some(build.intent.node().deadline()));
  assert!(
    store
      .expired_factory_execution_inputs(build.intent.node().deadline(), 10)
      .await
      .unwrap()
      .is_empty(),
    "visible Run retains exact role package beyond its download window"
  );
  assert!(
    protected_input_transfer::service(store.clone())
      .authorize_protected_inputs(AuthorizeProtectedInputsInput {
        manifest,
        observed_at_unix_ms: build.intent.node().deadline().unix_millis(),
        lease_expires_at: at(1_000_000)
      })
      .await
      .is_err()
  );
  // Fixture represents the existing retention worker's irreversible hidden phase.
  sqlx::query("UPDATE factory_runs SET visible=false,hidden_at=to_timestamp(1) WHERE id=$1")
    .bind(build.intent.input().factory_run_id().as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  let expired = store
    .expired_factory_execution_inputs(build.intent.node().deadline(), 10)
    .await
    .unwrap();
  assert_eq!(expired, vec![published.clone()]);
  store
    .complete_factory_execution_input_deletion(CompleteFactoryExecutionInputDeletion {
      expected: published,
      deleted_at: build.intent.node().deadline(),
    })
    .await
    .unwrap();
  assert!(
    store
      .expired_factory_execution_inputs(build.intent.node().deadline(), 10)
      .await
      .unwrap()
      .is_empty()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn role_package_cleanup_fences_late_build_holds() {
  let (database, store, build) = prepared_factory_build(true).await;
  let retained = retained_role_fixture(&build.intent);
  publish_role_package_catalog(store.as_ref(), &retained).await;
  store
    .retain_factory_role_invocation(&build.intent, &build.ownership, retained, at(12))
    .await
    .unwrap();
  let authority = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let accepted = authority.accept_factory_build(build.clone()).await.unwrap();
  sqlx::query("UPDATE factory_runs SET visible=false,hidden_at=to_timestamp(1) WHERE id=$1")
    .bind(build.intent.input().factory_run_id().as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  // Bounded metadata cleanup has already released the private package snapshot.
  sqlx::query("DELETE FROM factory_role_execution_packages WHERE operation_id=$1")
    .bind(build.intent.operation_id().unwrap().as_bytes().as_slice())
    .execute(&database.pool)
    .await
    .unwrap();
  let hold = store
    .place_build_result_hold(octacity_server_store::testing::management_mutation(
      PlaceBuildResultHold {
        build_id: accepted.build_id,
        reason: RetentionHoldReason::new("investigation").unwrap(),
        expires_at: None,
        idempotency_key: "late-role-hold".parse().unwrap(),
        placed_at: at(13),
      },
    ))
    .await;
  assert!(
    hold.is_err(),
    "a hold cannot revive role input bytes already released by Run retention"
  );
  database.cleanup().await;
}

fn role_quota_admissions() -> Vec<PublishedFactoryAdmission> {
  let original = protected_build_admission(true);
  (0..3)
    .map(|index| {
      let work = WorkEnvelope::new(
        WorkEnvelopeId::generate(),
        original.work.configuration().clone(),
        ExternalWorkIdentity::new(format!("manual/role-quota-{index}")).unwrap(),
        original.work.subject().clone(),
        original.work.artifacts().clone(),
        WorkClassification::new(original.work.priority(), original.work.risk(), Default::default()),
      )
      .unwrap();
      let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
      let root = FlowRun::root(&run, original.flow.closure().root()).unwrap();
      let cycle = WorkflowCycle::initial(&root).unwrap();
      let flow = AdmittedFlow::new(
        original.flow.closure().clone(),
        FlowAdmissionLimits::product_defaults(
          3,
          original.flow.limits().budget(),
          original.flow.limits().permissions().clone(),
        )
        .unwrap(),
        root,
        cycle,
        FactoryWipLimits::new(3, 3).unwrap(),
      )
      .unwrap()
      .with_data_schemas(original.flow.data_schemas().to_vec())
      .unwrap();
      PublishedFactoryAdmission {
        work,
        run,
        flow,
        admitted_at: at(1),
      }
    })
    .collect()
}
async fn role_quota_requests<S: FactoryRunStore + 'static>(
  store: Arc<S>,
  admissions: &[PublishedFactoryAdmission],
) -> Vec<AcceptFactoryBuild> {
  let (claims, targets, inputs) = failure_contract::claim_configuration_nodes(store.as_ref(), admissions).await;
  let mut requests = Vec::new();
  for index in 0..admissions.len() {
    FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NeverDispatch),
      factory_node_test_support::clock(),
    )
    .run_once(
      admissions[index].run.id(),
      claims[index].id,
      targets[index].clone(),
      Some(inputs[index].clone()),
      at(11),
    )
    .await
    .unwrap();
    let snapshot = store.factory_run_snapshot(admissions[index].run.id()).await.unwrap();
    let request = factory_build_contract_request(
      &snapshot.flow.data.build_intents[0],
      FactoryClaimOwnership::new(claims[index].owner.clone(), claims[index].claim),
      true,
      None,
      None,
    );
    requests.push(request);
  }
  requests
}
async fn verify_role_quota<
  S: FactoryBuildAcceptanceStore + BuildControlStore,
  C: FactoryRoleProfileStore + FactoryRoleInvocationStore,
>(
  store: &S,
  restarted: &S,
  catalog: &C,
  requests: &[AcceptFactoryBuild],
) -> AcceptFactoryBuild {
  let mut packages: Vec<_> = requests
    .iter()
    .map(|request| retained_role_fixture(&request.intent))
    .collect();
  let mut role_definition = packages[0].package.inputs().role.definition().clone();
  role_definition.concurrency_limit = std::num::NonZeroU32::new(2);
  let role = FactoryRole::publish(
    packages[0].package.inputs().role.reference().id,
    requests[0].build.project_id,
    role_definition,
  )
  .unwrap();
  let first_profile_id = packages[0].package.inputs().profile.reference().id;
  let second_profile_id = packages[1].package.inputs().profile.reference().id;
  let first_definition = packages[0].package.inputs().profile.definition().clone();
  for (index, package) in packages.iter_mut().enumerate() {
    let mut inputs = package.package.inputs().clone();
    inputs.profile = FactoryRoleProfile::publish(
      if index == 1 {
        second_profile_id
      } else {
        first_profile_id
      },
      role.reference().project_id,
      &role,
      if index == 1 {
        inputs.profile.definition().clone()
      } else {
        first_definition.clone()
      },
    )
    .unwrap();
    inputs.role = role.clone();
    package.package = FactoryRoleExecutionPackage::new(inputs).unwrap();
  }
  let definitions = FactoryRoleProfilesIntent {
    project_id: role.reference().project_id,
    role: Some(FactoryRoleIntent {
      id: role.reference().id,
      expected_current_version: None,
      definition: role.definition().clone(),
    }),
    profiles: packages[..2]
      .iter()
      .map(|package| {
        let profile = &package.package.inputs().profile;
        FactoryRoleProfileIntent {
          id: profile.reference().id,
          expected_current_version: None,
          role: role.reference().clone(),
          definition: profile.definition().clone(),
        }
      })
      .collect(),
  };
  catalog
    .publish_factory_role_profiles(octacity_server_store::testing::management_mutation(
      PublishFactoryRoleProfiles {
        intent: definitions,
        idempotency_key: "role-quota-create".parse().unwrap(),
        published_at: at(12),
      },
    ))
    .await
    .unwrap();
  for (request, package) in requests.iter().zip(packages) {
    catalog
      .retain_factory_role_invocation(&request.intent, &request.ownership, package, at(12))
      .await
      .unwrap();
  }
  let first = store.accept_factory_build(requests[0].clone()).await.unwrap();
  let (left, right) = tokio::join!(
    store.accept_factory_build(requests[1].clone()),
    restarted.accept_factory_build(requests[2].clone())
  );
  assert_eq!(
    usize::from(left.is_ok()) + usize::from(right.is_ok()),
    1,
    "one remaining role slot has one winner across profiles"
  );
  let (winner, loser) = if left.is_ok() { (1, 2) } else { (2, 1) };
  let accepted = restarted
    .factory_build_for_operation(requests[winner].intent.operation_id().unwrap())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    restarted.accept_factory_build(requests[winner].clone()).await.unwrap(),
    accepted
  );
  let mut reduced = role.definition().clone();
  reduced.concurrency_limit = std::num::NonZeroU32::new(1);
  catalog
    .publish_factory_role_profiles(octacity_server_store::testing::management_mutation(
      PublishFactoryRoleProfiles {
        intent: FactoryRoleProfilesIntent {
          project_id: role.reference().project_id,
          role: Some(FactoryRoleIntent {
            id: role.reference().id,
            expected_current_version: Some(role.reference().version),
            definition: reduced,
          }),
          profiles: vec![],
        },
        idempotency_key: "role-quota-reduce".parse().unwrap(),
        published_at: at(13),
      },
    ))
    .await
    .unwrap();
  assert!(matches!(
    restarted.accept_factory_build(requests[loser].clone()).await,
    Err(StoreError::CapacityUnavailable)
  ));
  let cancel = |build_id, key: &str| {
    octacity_server_store::testing::management_mutation(CancelBuild {
      build_id,
      idempotency_key: key.parse().unwrap(),
      requested_at: at(14),
    })
  };
  let cancelled = store
    .cancel_build(cancel(first.build_id, "role-first-cancel"))
    .await
    .unwrap();
  assert_eq!(
    cancelled.build_state,
    octacity_server_orchestrator::BuildState::Cancelled
  );
  assert!(
    matches!(
      restarted.accept_factory_build(requests[loser].clone()).await,
      Err(StoreError::CapacityUnavailable)
    ),
    "reduced current ceiling still blocks a new start"
  );
  let cancelled_again = store
    .cancel_build(cancel(first.build_id, "role-first-cancel"))
    .await
    .unwrap();
  assert_eq!(cancelled_again.build_state, cancelled.build_state);
  assert!(
    matches!(
      restarted.accept_factory_build(requests[loser].clone()).await,
      Err(StoreError::CapacityUnavailable)
    ),
    "terminal replay cannot release another Build's slot"
  );
  store
    .cancel_build(cancel(accepted.build_id, "role-second-cancel"))
    .await
    .unwrap();
  let released = restarted.accept_factory_build(requests[loser].clone()).await.unwrap();
  assert_eq!(
    released.build_id, requests[loser].build.id,
    "capacity waiting preserves the original operation and profile"
  );
  requests[loser].clone()
}

async fn verify_factory_management_retry_is_rejected<S: FactoryBuildAcceptanceStore + BuildControlStore>(
  store: &S,
  request: AcceptFactoryBuild,
) {
  let operation = request.intent.operation_id().unwrap();
  let accepted = store.factory_build_for_operation(operation).await.unwrap().unwrap();
  let mut source = octacity_server_store::testing::authoritative_store_contract_fixture().request;
  source.build = request.build;
  source.jobs = request.jobs;
  let retry = octacity_server_store::testing::retry_request(&source, 99_001, "factory-management-retry", at(15));
  assert!(
    matches!(
      store
        .retry_build(octacity_server_store::testing::management_mutation(retry))
        .await,
      Err(StoreError::Conflict {
        entity: EntityKind::Build
      })
    ),
    "management Retry cannot replace Factory node authority, even after a Build fails"
  );
  assert_eq!(
    store.factory_build_for_operation(operation).await.unwrap(),
    Some(accepted)
  );
}
#[tokio::test]
async fn role_quota_memory_uses_ordinary_builds_across_profiles_and_current_versions() {
  let admissions = role_quota_admissions();
  let catalog = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
  catalog.seed_project(admissions[0].work.subject().project_id()).unwrap();
  for admission in &admissions {
    catalog
      .seed_factory_run(
        admission.clone(),
        FactoryDigest::from_bytes([1; 32]),
        octacity_server_store::testing::management_mutation(()).audit(),
      )
      .unwrap();
  }
  let requests = role_quota_requests(catalog.clone(), &admissions).await;
  let store = octacity_server_store::testing::InMemoryStore::new().with_factory_authority(catalog.clone());
  store
    .seed_agent_pool(requests[0].jobs[0].allowed_pools[0], PoolVersion::INITIAL)
    .unwrap();
  let request = verify_role_quota(&store, &store, catalog.as_ref(), &requests).await;
  store.seed_failed_factory_build(request.build.id).unwrap();
  verify_factory_management_retry_is_rejected(&store, request).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn role_quota_postgres_uses_the_same_ordinary_build_contract_after_restart() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admissions = role_quota_admissions();
  let catalog = admit(&database.pool, &admissions[0]).await;
  for (index, admission) in admissions.iter().enumerate().skip(1) {
    catalog
      .admit_factory_work(octacity_server_store::testing::management_mutation_with_request(
        AdmitFactoryWork {
          probe: FactoryAdmissionProbe {
            source_scope: FactoryWorkSourceScope {
              source: key("manual"),
              security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
            },
            external_identity: admission.work.external_identity().clone(),
            intent_digest: FactoryDigest::from_bytes([index as u8; 32]),
            idempotency_key: format!("role-quota-run-{index}").parse().unwrap(),
          },
          repository_version: RepositoryVersion::INITIAL,
          work: admission.work.clone(),
          run: admission.run.clone(),
          flow: admission.flow.clone(),
          admitted_at: at(1),
        },
        format!("role-quota-run-{index}"),
      ))
      .await
      .unwrap();
  }
  let requests = role_quota_requests(catalog.clone(), &admissions).await;
  seed_factory_build_prerequisites(&database.pool, &requests[0]).await;
  let store = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let restarted = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let request = verify_role_quota(&store, &restarted, catalog.as_ref(), &requests).await;
  sqlx::query("UPDATE builds SET state='failed' WHERE id=$1")
    .bind(request.build.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE attempts SET state='failed' WHERE id=$1")
    .bind(request.attempt_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE jobs SET state='failed' WHERE attempt_id=$1")
    .bind(request.attempt_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id IN (SELECT id FROM jobs WHERE attempt_id=$1)")
    .bind(request.attempt_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  verify_factory_management_retry_is_rejected(&restarted, request).await;
  database.cleanup().await;
}
