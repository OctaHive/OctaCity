use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{IntegrationId, Timestamp};
use octacity_server_store::{
  IdempotencyKey, IntegrationConnectionIntent, IntegrationConnectionStore, MutationDisposition,
  testing::{InMemoryFactoryConfigurationStore, MutationEvidenceProbe, integration_connection_fixture},
};

use crate::*;

struct Deny;
#[async_trait]
impl ManagementAuthorizationPolicy for Deny {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Err(ManagementAuthorizationDenial::forbidden())
  }
}

#[test]
fn connection_application_authorizes_before_publication_and_historical_replay() {
  run_ready(async {
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let store = Arc::new(InMemoryFactoryConfigurationStore::new());
      let (installed, draft) = integration_connection_fixture(provider);
      let handlers = Arc::new(IntegrationConnectionHandlers::new(store.clone(), vec![installed.clone()]).unwrap());
      let command = PublishIntegrationConnectionCommand {
        intent: IntegrationConnectionIntent {
          id: IntegrationId::generate(),
          expected_current_version: None,
          draft,
        },
        idempotency_key: IdempotencyKey::new("connection-publication").unwrap(),
        published_at: Timestamp::from_unix_millis(10).unwrap(),
      };
      let denied = AuthorizedCommandHandler::new(Arc::new(Deny), handlers.clone());
      assert!(matches!(
        denied.handle_command(&context, command.clone()).await,
        Err(ManagementHandlerError::Forbidden(_))
      ));
      assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
      let accepted = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
      let original = accepted.handle_command(&context, command.clone()).await.unwrap();
      assert_eq!(original.connection.definition.provider.as_str(), provider);
      assert_eq!(original.connection.definition.adapter, installed.adapter);
      assert_eq!(original.connection.definition.resources, installed.resources);
      let reloaded = Arc::new(IntegrationConnectionHandlers::new(store.clone(), vec![]).unwrap());
      let denied = AuthorizedCommandHandler::new(Arc::new(Deny), reloaded.clone());
      assert!(matches!(
        denied.handle_command(&context, command.clone()).await,
        Err(ManagementHandlerError::Forbidden(_))
      ));
      let accepted = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), reloaded.clone());
      let replay = accepted.handle_command(&context, command.clone()).await.unwrap();
      assert_eq!(replay.disposition, MutationDisposition::Replayed);
      assert_eq!(replay.connection, original.connection);
      let denied_query = AuthorizedQueryHandler::new(Arc::new(Deny), reloaded.clone());
      assert!(matches!(
        denied_query
          .handle_query(
            &context,
            GetIntegrationConnectionQuery {
              id: command.intent.id,
              version: None,
            }
          )
          .await,
        Err(ManagementHandlerError::Forbidden(_))
      ));
      let query = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), reloaded);
      assert_eq!(
        query
          .handle_query(
            &context,
            GetIntegrationConnectionQuery {
              id: command.intent.id,
              version: None
            }
          )
          .await
          .unwrap(),
        original.connection
      );
      let mut fresh = command;
      fresh.intent.id = IntegrationId::generate();
      fresh.idempotency_key = IdempotencyKey::new("missing-installation").unwrap();
      assert!(accepted.handle_command(&context, fresh).await.is_err());
      assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
      assert_eq!(
        store
          .integration_connection(original.connection.id, None)
          .await
          .unwrap(),
        original.connection
      );
    }
  });
}

fn run_ready(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(
    future.as_mut().poll(&mut context).is_ready(),
    "memory operations complete immediately"
  );
}
