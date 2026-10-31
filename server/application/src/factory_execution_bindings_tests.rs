//! Public authoring queries over installed metadata and the ordinary role store.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{
  IdempotencyKey,
  testing::{InMemoryFactoryConfigurationStore, MutationEvidenceProbe, role_profile_contract_intent},
};
use std::sync::Arc;

#[test]
fn a_static_build_compiler_is_not_a_role_profile_execution_choice() {
  crate::factory_admission_tests::run_ready(async {
    let BindingFixture {
      store, handlers, query, ..
    } = fixture(false).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let before = store.mutation_evidence_counts().await;
    let resolved = queries.handle_query(&context(), query).await.unwrap();
    assert!(
      resolved.build.is_none(),
      "a compiler that ignores Role/Profile instructions cannot execute the selected agent"
    );
    assert_eq!(
      resolved.incompatibility,
      Some(FactoryExecutionBindingProblem::BuildUnavailable)
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}

struct BindingFixture {
  store: Arc<InMemoryFactoryConfigurationStore>,
  handlers: Arc<FactoryRoleProfileHandlers<InMemoryFactoryConfigurationStore>>,
  query: ResolveFactoryExecutionBindingQuery,
  role: FactoryRoleRef,
}
async fn fixture(role_compiler: bool) -> BindingFixture {
  let project = ProjectId::generate();
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let (installed, definition) = crate::factory_test_support::installed_role_execution_choices(project);
  let mut compiler = installed.build_pipeline_contracts()[0].clone();
  compiler.compilation.as_mut().unwrap().role_profiles = role_compiler;
  let alias = FactoryKey::new("dependency-1").unwrap();
  let choices = FactoryConfigurationChoices::try_new(installed.entries())
    .unwrap()
    .with_role_profile_capability(alias.clone(), installed.role_profile_capabilities()[&alias].clone())
    .unwrap()
    .with_build_pipeline_contract(compiler)
    .unwrap();
  store.enable_project(project, choices).unwrap();
  let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
  let mut intent = role_profile_contract_intent(project);
  intent.profiles.truncate(1);
  intent.profiles[0].definition = definition;
  let publication = commands
    .handle_command(
      &context(),
      PublishFactoryRoleProfilesCommand {
        intent,
        idempotency_key: IdempotencyKey::new("first-agent").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      },
    )
    .await
    .unwrap();

  BindingFixture {
    store,
    handlers,
    role: publication.role.unwrap().reference().clone(),
    query: ResolveFactoryExecutionBindingQuery {
      project_id: project,
      profile: publication.profiles[0].reference().clone(),
      capability: alias,
      result_outcome: FactoryKey::new("observed").unwrap(),
    },
  }
}

#[test]
fn a_deleted_agent_cannot_supply_a_new_execution_binding_from_its_retained_profile() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = fixture(true).await;
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), fixture.handlers.clone());
    commands
      .handle_command(
        &context(),
        DeleteFactoryRoleCommand {
          request: octacity_server_store::DeleteFactoryRole {
            project_id: fixture.query.project_id,
            id: fixture.role.id,
            expected_current_version: fixture.role.version,
            idempotency_key: IdempotencyKey::new("delete-agent").unwrap(),
            deleted_at: Timestamp::from_unix_millis(2).unwrap(),
          },
        },
      )
      .await
      .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), fixture.handlers);
    let before = fixture.store.mutation_evidence_counts().await;
    let resolved = queries.handle_query(&context(), fixture.query).await.unwrap();
    assert!(
      resolved.build.is_none(),
      "retained historical content must not restore deleted agent eligibility"
    );
    assert_eq!(
      resolved.incompatibility,
      Some(FactoryExecutionBindingProblem::ProfileIncompatible)
    );
    assert_eq!(fixture.store.mutation_evidence_counts().await, before);
  });
}
