use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
use octacity_server_secrets::{
  CredentialPublication, CredentialPublicationKind, ProviderCredentialError, ProviderCredentialPublisher,
  ProviderCredentialValue,
};
use octacity_server_store::{
  BeginIntegrationAdministration, IdempotencyKey, IntegrationAdministrationAction, IntegrationAdministrationIntent,
  IntegrationAdministrationStatus, IntegrationAdministrationStore, IntegrationConnectionIntent,
  IntegrationConnectionStore, PublishIntegrationConnection,
  testing::{InMemoryFactoryConfigurationStore, integration_connection_fixture, management_mutation},
};
use std::sync::{Arc, Mutex};

struct Policy(bool);
#[async_trait]
impl ManagementAuthorizationPolicy for Policy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if self.0 {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}
#[derive(Default)]
struct PrivatePeer {
  state: Mutex<(Vec<[u8; 32]>, usize, bool)>,
}
#[async_trait]
impl ProviderCredentialPublisher for PrivatePeer {
  async fn prepare(
    &self,
    stamp: &CredentialPublication,
    _: CredentialPublicationKind,
    value: Option<ProviderCredentialValue>,
  ) -> Result<(), ProviderCredentialError> {
    let mut state = self.state.lock().unwrap();
    state.1 += 1;
    if !state.0.contains(&stamp.operation) {
      if value.is_none() {
        return Err(ProviderCredentialError::NotConfigured);
      }
      state.0.push(stamp.operation);
    }
    Ok(())
  }
  async fn activate(&self, _: &CredentialPublication) -> Result<(), ProviderCredentialError> {
    let mut state = self.state.lock().unwrap();
    state.1 += 1;
    if std::mem::take(&mut state.2) {
      return Err(ProviderCredentialError::Unavailable);
    }
    Ok(())
  }
  async fn discard(&self, _: &CredentialPublication) -> Result<(), ProviderCredentialError> {
    Ok(())
  }
}
fn verified_context() -> ManagementRequestContext {
  ManagementRequestContext::new(
    ManagementActor::authenticated("integration-admin").unwrap(),
    ManagementSecurityScope::new("integration-admin-scope").unwrap(),
    ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .unwrap()
}

#[test]
fn credential_commands_authorize_and_resume_private_preparation_without_secret_replay() {
  let mut future = std::pin::pin!(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let (installed, draft) = integration_connection_fixture("gitlab");
    let reference = installed.resources[0].capabilities[0].credential.clone();
    let id = IntegrationId::generate();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("metadata").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let peer = Arc::new(PrivatePeer::default());
    peer.state.lock().unwrap().2 = true;
    let handlers = Arc::new(IntegrationAdministrationHandlers::new(store.clone(), peer.clone()));
    let request = BeginIntegrationAdministration {
      intent: IntegrationAdministrationIntent {
        id,
        expected_metadata_version: IntegrationVersion::INITIAL,
        expected_revision: 0,
        action: IntegrationAdministrationAction::Credential {
          reference,
          kind: CredentialPublicationKind::Provision,
          activate_connection: true,
        },
      },
      idempotency_key: IdempotencyKey::new("credential-operation").unwrap(),
      requested_at: Timestamp::from_unix_millis(10).unwrap(),
    };
    let command = || ManageIntegrationCredentialCommand {
      request: request.clone(),
      value: Some(ProviderCredentialValue::new(b"never-in-safe-receipts".to_vec()).unwrap()),
    };
    assert!(!format!("{:?}", command()).contains("never-in-safe-receipts"));
    let denied = AuthorizedCommandHandler::new(Arc::new(Policy(false)), handlers.clone());
    assert!(matches!(
      denied.handle_command(&verified_context(), command()).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    assert_eq!(peer.state.lock().unwrap().1, 0);
    let allowed = AuthorizedCommandHandler::new(Arc::new(Policy(true)), handlers);
    let anonymous = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    assert!(allowed.handle_command(&anonymous, command()).await.is_err());
    assert_eq!(store.integration_administration(id).await.unwrap().revision, 0);
    assert!(allowed.handle_command(&verified_context(), command()).await.is_err());
    assert!(!store.integration_administration(id).await.unwrap().enabled);
    let reloaded = AuthorizedCommandHandler::new(
      Arc::new(Policy(true)),
      Arc::new(IntegrationAdministrationHandlers::new(store.clone(), peer.clone())),
    );
    let receipt = reloaded
      .handle_command(
        &verified_context(),
        ManageIntegrationCredentialCommand {
          request: request.clone(),
          value: None,
        },
      )
      .await
      .unwrap();
    assert_eq!(receipt.status, IntegrationAdministrationStatus::Completed);
    assert!(
      store.integration_administration(id).await.unwrap().enabled,
      "reviewed activation commits under the original credential operation after private recovery"
    );
    let calls = peer.state.lock().unwrap().1;
    let replay = reloaded.handle_command(&verified_context(), command()).await.unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(peer.state.lock().unwrap().1, calls);
    assert!(
      !serde_json::to_string(&receipt)
        .unwrap()
        .contains("never-in-safe-receipts")
    );
    let denied_lifecycle = AuthorizedCommandHandler::new(
      Arc::new(Policy(false)),
      Arc::new(IntegrationAdministrationHandlers::new(store.clone(), peer.clone())),
    );
    let mut lifecycle = request.clone();
    lifecycle.intent.expected_revision = receipt.revision;
    lifecycle.intent.action = IntegrationAdministrationAction::Activate;
    lifecycle.idempotency_key = IdempotencyKey::new("enable").unwrap();
    assert!(matches!(
      denied_lifecycle
        .handle_command(
          &verified_context(),
          SetIntegrationConnectionStateCommand {
            request: lifecycle.clone()
          }
        )
        .await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    let enabled = reloaded
      .handle_command(
        &verified_context(),
        SetIntegrationConnectionStateCommand {
          request: lifecycle.clone(),
        },
      )
      .await
      .unwrap();
    assert!(store.integration_administration(id).await.unwrap().enabled);
    lifecycle.intent.expected_revision = enabled.revision;
    lifecycle.intent.action = IntegrationAdministrationAction::Disable;
    lifecycle.idempotency_key = IdempotencyKey::new("disable").unwrap();
    reloaded
      .handle_command(
        &verified_context(),
        SetIntegrationConnectionStateCommand { request: lifecycle },
      )
      .await
      .unwrap();
    assert!(!store.integration_administration(id).await.unwrap().enabled);
    assert_eq!(
      reloaded.handle_command(&verified_context(), command()).await.unwrap(),
      receipt
    );
    assert!(!store.integration_administration(id).await.unwrap().enabled);
    let query = AuthorizedQueryHandler::new(
      Arc::new(Policy(true)),
      Arc::new(IntegrationAdministrationHandlers::new(store.clone(), peer.clone())),
    );
    assert!(
      !query
        .handle_query(&verified_context(), GetIntegrationAdministrationQuery { id })
        .await
        .unwrap()
        .enabled
    );
    let cleanup = CleanupIntegrationPreparationsCommand {
      id,
      requested_at: Timestamp::from_unix_millis(1000).unwrap(),
      older_than: Timestamp::from_unix_millis(100).unwrap(),
      limit: 1,
    };
    assert_eq!(reloaded.handle_command(&verified_context(), cleanup).await.unwrap(), 0);
    use octacity_server_store::testing::{ManagementAuditProbe, MutationEvidenceProbe};
    let evidence = store.mutation_evidence_counts().await;
    assert_eq!((evidence.idempotency, evidence.audit, evidence.outbox), (5, 5, 5));
    assert!(!format!("{:?}", store.management_audit_facts().await).contains("never-in-safe-receipts"));
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

struct CorruptJournal;
#[async_trait]
impl IntegrationAdministrationStore for CorruptJournal {
  async fn begin_integration_administration(
    &self,
    request: octacity_server_store::ManagementMutation<BeginIntegrationAdministration>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    let (request, _) = request.into_parts();
    let foreign = octacity_server_secrets::LogicalSecretReference::new(
      octacity_server_secrets::SecretProviderId::new("installed").unwrap(),
      "another-connection-writer",
    )
    .unwrap();
    Ok(IntegrationAdministrationReceipt {
      intent: request.intent,
      operation: [7; 32],
      revision: 1,
      requested_at: request.requested_at,
      publication: Some(CredentialPublication::new(foreign, [7; 32], 1).unwrap()),
      status: IntegrationAdministrationStatus::Prepared,
    })
  }
  async fn complete_integration_administration(
    &self,
    _: octacity_server_store::ManagementMutation<IntegrationAdministrationReceipt>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    Err(StoreError::Unavailable)
  }
  async fn integration_administration(&self, _: IntegrationId) -> Result<IntegrationAdministrationState, StoreError> {
    Err(StoreError::Unavailable)
  }
  async fn integration_preparation_cleanup(
    &self,
    _: octacity_server_store::ManagementMutation<octacity_server_store::IntegrationPreparationCleanup>,
  ) -> Result<Vec<CredentialPublication>, StoreError> {
    Err(StoreError::Unavailable)
  }
  async fn acknowledge_integration_preparation_cleanup(
    &self,
    _: octacity_server_store::ManagementMutation<CredentialPublication>,
  ) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }
}

#[test]
fn retained_publication_cannot_redirect_a_credential_command_to_another_installed_handle() {
  let mut future = std::pin::pin!(async {
    let peer = Arc::new(PrivatePeer::default());
    let service = AuthorizedCommandHandler::new(
      Arc::new(Policy(true)),
      Arc::new(IntegrationAdministrationHandlers::new(
        Arc::new(CorruptJournal),
        peer.clone(),
      )),
    );
    let reference = octacity_server_secrets::LogicalSecretReference::new(
      octacity_server_secrets::SecretProviderId::new("installed").unwrap(),
      "source-reader",
    )
    .unwrap();
    let command = ManageIntegrationCredentialCommand {
      request: BeginIntegrationAdministration {
        intent: IntegrationAdministrationIntent {
          id: IntegrationId::generate(),
          expected_metadata_version: IntegrationVersion::INITIAL,
          expected_revision: 0,
          action: IntegrationAdministrationAction::Credential {
            reference,
            kind: CredentialPublicationKind::Provision,
            activate_connection: false,
          },
        },
        idempotency_key: IdempotencyKey::new("original-scope").unwrap(),
        requested_at: Timestamp::from_unix_millis(10).unwrap(),
      },
      value: Some(ProviderCredentialValue::new(b"do-not-redirect-me".to_vec()).unwrap()),
    };
    assert!(service.handle_command(&verified_context(), command).await.is_err());
    assert_eq!(
      peer.state.lock().unwrap().1,
      0,
      "corrupt retained scope must fail before private I/O"
    );
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}
