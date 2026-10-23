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
