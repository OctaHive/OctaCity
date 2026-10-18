use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{IntegrationId, Timestamp};
use octacity_server_factory::FactoryKey;
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

#[test]
fn connection_list_applies_visibility_before_deriving_its_cursor() {
  run_ready(async {
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let (installed, draft) = integration_connection_fixture("custom-tracker");
    let handlers = Arc::new(IntegrationConnectionHandlers::new(store, vec![installed]).unwrap());
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let mut ids = vec![];
    for index in 1..=3 {
      let id: IntegrationId = format!("00000000-0000-7000-8000-{index:012}").parse().unwrap();
      commands
        .handle_command(
          &context,
          PublishIntegrationConnectionCommand {
            intent: IntegrationConnectionIntent {
              id,
              expected_current_version: None,
              draft: draft.clone(),
            },
            idempotency_key: IdempotencyKey::new(format!("connection-{index}")).unwrap(),
            published_at: Timestamp::from_unix_millis(10).unwrap(),
          },
        )
        .await
        .unwrap();
      ids.push(id);
    }
    struct Visible(Vec<IntegrationId>);
    #[async_trait]
    impl ManagementAuthorizationPolicy for Visible {
      async fn authorize(
        &self,
        _: &ManagementRequestContext,
        _: ManagementAction,
        _: &ManagementResource,
      ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
        Ok(ManagementAuthorizationGrant::new(
          ManagementVisibility::restricted(self.0.iter().map(|id| {
            ManagementResource::instance(
              ManagementResourceKind::IntegrationConnection,
              ManagementResourceIdentity::new(id.to_string()).unwrap(),
            )
            .unwrap()
          }))
          .unwrap(),
        ))
      }
    }
    let queries = AuthorizedQueryHandler::new(Arc::new(Visible(vec![ids[0], ids[2]])), handlers.clone());
    let first = queries
      .handle_query(&context, ListIntegrationConnectionsQuery { after: None, limit: 1 })
      .await
      .unwrap();
    assert_eq!(first.items.iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[0]]);
    assert_eq!(first.next_cursor, Some(ids[0]));
    let last = queries
      .handle_query(
        &context,
        ListIntegrationConnectionsQuery {
          after: first.next_cursor,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(last.items.iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[2]]);
    assert_eq!(last.next_cursor, None);
    let denied = AuthorizedQueryHandler::new(Arc::new(Deny), handlers);
    assert!(matches!(
      denied
        .handle_query(&context, ListIntegrationConnectionsQuery { after: None, limit: 1 })
        .await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
  });
}

#[test]
fn installed_connection_choices_are_authorized_bounded_and_provider_neutral() {
  run_ready(async {
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let store = Arc::new(InMemoryFactoryConfigurationStore::new());
      let (installed, _) = integration_connection_fixture(provider);
      let handlers = Arc::new(IntegrationConnectionHandlers::new(store.clone(), vec![installed.clone()]).unwrap());
      let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
      let page = queries
        .handle_query(&context, ListIntegrationInstallationsQuery { after: None, limit: 1 })
        .await
        .unwrap();
      assert_eq!(page.items, vec![installed]);
      assert_eq!(page.next_cursor, None);
      assert!(
        queries
          .handle_query(
            &context,
            ListIntegrationInstallationsQuery {
              after: None,
              limit: 101
            }
          )
          .await
          .is_err()
      );
      let denied = AuthorizedQueryHandler::new(Arc::new(Deny), handlers);
      assert!(matches!(
        denied
          .handle_query(&context, ListIntegrationInstallationsQuery { after: None, limit: 1 })
          .await,
        Err(ManagementHandlerError::Forbidden(_))
      ));
      assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    }
  });
}

#[test]
fn connection_draft_validation_resolves_only_installed_choices_without_publication() {
  run_ready(async {
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let (installed, draft) = integration_connection_fixture("jira");
    let handlers = Arc::new(IntegrationConnectionHandlers::new(store.clone(), vec![installed.clone()]).unwrap());
    let query = ValidateIntegrationConnectionQuery {
      intent: IntegrationConnectionIntent {
        id: IntegrationId::generate(),
        expected_current_version: None,
        draft,
      },
    };
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let result = queries.handle_query(&context, query.clone()).await.unwrap();
    assert_eq!(result.provider.as_str(), "jira");
    assert_eq!(result.resources, installed.resources);
    for field in ["endpoint", "authentication", "resource", "purpose"] {
      let mut invalid = query.clone();
      match field {
        "endpoint" => invalid.intent.draft.endpoint = "https://outside.invalid/".into(),
        "authentication" => invalid.intent.draft.authentication = FactoryKey::new("unsupported").unwrap(),
        "resource" => invalid.intent.draft.resources.clear(),
        _ => invalid.intent.draft.resources[0].purposes = vec![octacity_server_store::IntegrationPurpose::TaskUpdate],
      }
      assert!(queries.handle_query(&context, invalid).await.is_err(), "{field}");
    }
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    assert!(store.integration_connection(query.intent.id, None).await.is_err());
  });
}
