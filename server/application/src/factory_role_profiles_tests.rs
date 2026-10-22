use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::FactoryRoleVersion;
use octacity_server_store::{
  IdempotencyKey, MutationDisposition,
  testing::{InMemoryFactoryConfigurationStore, MutationEvidenceProbe, role_profile_contract_intent},
};
use std::sync::Arc;
fn run_ready(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(future.as_mut().poll(&mut context).is_ready());
}
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}
#[test]
fn factory_role_profiles_use_audited_management_publication_and_exact_reads() {
  run_ready(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let project = ProjectId::generate();
    store.seed_project(project).unwrap(); // Factory mode remains disabled, no provider/worker enrolled.
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let command = PublishFactoryRoleProfilesCommand {
      intent: role_profile_contract_intent(project),
      idempotency_key: IdempotencyKey::new("custom-ai-agent").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    };
    let original = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(original.disposition, MutationDisposition::Applied);
    let role = original.role.as_ref().unwrap();
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetFactoryRoleQuery {
            project_id: project,
            id: role.reference().id,
            version: Some(FactoryRoleVersion::INITIAL)
          }
        )
        .await
        .unwrap(),
      *role
    );
    let profile = &original.profiles[0];
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetFactoryRoleProfileQuery {
            project_id: project,
            id: profile.reference().id,
            version: None
          }
        )
        .await
        .unwrap(),
      *profile
    );
    let before = store.mutation_evidence_counts().await;
    let replay = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.role, original.role);
    assert_eq!(store.mutation_evidence_counts().await, before);
    let mut update = command.clone();
    update.idempotency_key = IdempotencyKey::new("update").unwrap();
    update.intent.profiles.clear();
    update.intent.role.as_mut().unwrap().expected_current_version = Some(FactoryRoleVersion::INITIAL);
    commands.handle_command(&context(), update.clone()).await.unwrap();
    update.idempotency_key = IdempotencyKey::new("stale").unwrap();
    assert_eq!(
      commands
        .handle_command(&context(), update)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::PreconditionFailed
    );
    let denied = AuthorizedCommandHandler::new(
      Arc::new(ReadOnly),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    );
    let before = store.mutation_evidence_counts().await;
    assert!(denied.handle_command(&context(), command).await.is_err()); // Replay cannot bypass revoked publication authority.
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
struct ReadOnly;
#[async_trait]
impl ManagementAuthorizationPolicy for ReadOnly {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::View {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}

#[test]
fn factory_role_profiles_apply_project_visibility_before_content_and_replay() {
  run_ready(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let project = ProjectId::generate();
    store.seed_project(project).unwrap();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let command = PublishFactoryRoleProfilesCommand {
      intent: role_profile_contract_intent(project),
      idempotency_key: IdempotencyKey::new("private-role").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    };
    let outcome = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone())
      .handle_command(&context(), command.clone())
      .await
      .unwrap();
    let resource =
      |kind, id: String| ManagementResource::instance(kind, ManagementResourceIdentity::new(id).unwrap()).unwrap();
    for visibility in [
      ManagementVisibility::none(),
      ManagementVisibility::restricted([resource(
        ManagementResourceKind::Project,
        ProjectId::generate().to_string(),
      )])
      .unwrap(),
      ManagementVisibility::restricted([resource(
        ManagementResourceKind::Agent,
        uuid::Uuid::new_v4().to_string(),
      )])
      .unwrap(),
    ] {
      let policy = Arc::new(Scoped(visibility));
      let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
      let commands = AuthorizedCommandHandler::new(policy, handlers.clone());
      let before = store.mutation_evidence_counts().await;
      assert!(
        queries
          .handle_query(
            &context(),
            GetFactoryRoleQuery {
              project_id: project,
              id: outcome.role.as_ref().unwrap().reference().id,
              version: None
            }
          )
          .await
          .is_err()
      );
      assert!(
        queries
          .handle_query(
            &context(),
            GetFactoryRoleProfileQuery {
              project_id: project,
              id: outcome.profiles[0].reference().id,
              version: None
            }
          )
          .await
          .is_err()
      );
      assert!(commands.handle_command(&context(), command.clone()).await.is_err());
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    let policy = Arc::new(Scoped(
      ManagementVisibility::restricted([resource(ManagementResourceKind::Project, project.to_string())]).unwrap(),
    ));
    assert_eq!(
      AuthorizedCommandHandler::new(policy, handlers)
        .handle_command(&context(), command)
        .await
        .unwrap()
        .disposition,
      MutationDisposition::Replayed
    );
  });
}
struct Scoped(ManagementVisibility);
#[async_trait]
impl ManagementAuthorizationPolicy for Scoped {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(self.0.clone()))
  }
}
