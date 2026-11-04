use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
use octacity_server_store::{
  testing::{
    InMemoryFactoryConfigurationStore, MutationEvidenceProbe, integration_connection_fixture, management_mutation,
  },
  *,
};
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

struct CheckPolicy(Vec<ManagementAction>);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for CheckPolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::IntegrationConnection && self.0.contains(&action) {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}
struct Peer(AtomicUsize);
#[async_trait::async_trait]
impl IntegrationConnectionChecker for Peer {
  async fn check(
    &self,
    _: &PublishedIntegrationConnection,
    _: &IntegrationResource,
    _: IntegrationPurpose,
  ) -> Result<IntegrationCheckReadiness, ApplicationError> {
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(IntegrationCheckReadiness::unverified())
  }
}

#[test]
fn connection_checks_require_separate_rights_and_missing_credentials_never_activate_or_grant_anything() {
  let mut future = std::pin::pin!(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let id = IntegrationId::generate();
    let (installed, draft) = integration_connection_fixture("custom-tracker");
    let resource = draft.resources[0].reference.clone();
    let connection = store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("check-connection").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap()
      .connection;
    let before = store.mutation_evidence_counts().await;
    let state = store.integration_administration(id).await.unwrap();
    let peer = Arc::new(Peer(AtomicUsize::new(0)));
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let request = CheckIntegrationConnectionQuery {
      id,
      version: IntegrationVersion::INITIAL,
      resource,
      purpose: IntegrationPurpose::TaskRead,
    };
    for omitted in [
      Some(ManagementAction::Administer),
      Some(ManagementAction::View),
      Some(ManagementAction::Execute),
      None,
    ] {
      let policy = Arc::new(CheckPolicy(
        [
          ManagementAction::Administer,
          ManagementAction::View,
          ManagementAction::Execute,
        ]
        .into_iter()
        .filter(|a| Some(*a) != omitted)
        .collect(),
      ));
      let query = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IntegrationCheckHandlers::new(store.clone(), peer.clone(), policy)),
      );
      let result = query.handle_query(&context, request.clone()).await;
      if omitted.is_some() {
        assert!(result.is_err());
      } else {
        let result = result.unwrap();
        assert_eq!(result.connection_id, id);
        assert_eq!(result.native_id.as_str(), "external-project-42");
        assert_eq!(result.readiness.authentication, IntegrationCheckState::Missing);
        assert_eq!(result.readiness.capability, IntegrationCheckState::Unverified);
        let safe = serde_json::to_string(&result).unwrap();
        for private in [
          "credential",
          "token",
          "existing-reader",
          "work_id",
          "input_id",
          "issue_id",
        ] {
          assert!(!safe.contains(private));
        }
      }
    }
    assert_eq!(
      peer.0.load(Ordering::SeqCst),
      0,
      "no credential/provider fallback when managed material is not configured"
    );
    assert_eq!(store.integration_administration(id).await.unwrap(), state);
    assert_eq!(store.integration_connection(id, None).await.unwrap(), connection);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

#[test]
fn checks_of_inactive_connections_remain_read_only_and_unsupported_or_stale_selections_never_dispatch() {
  let mut future = std::pin::pin!(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let id = IntegrationId::generate();
    let (installed, draft) = integration_connection_fixture("gitlab");
    let resource = draft.resources[0].reference.clone();
    let credential = installed.resources[0].capabilities[0].credential.clone();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft: draft.clone(),
        },
        installed: installed.clone(),
        idempotency_key: IdempotencyKey::new("inactive-check").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let pending = store
      .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
        intent: IntegrationAdministrationIntent {
          id,
          expected_metadata_version: IntegrationVersion::INITIAL,
          expected_revision: 0,
          action: IntegrationAdministrationAction::Credential {
            reference: credential,
            kind: octacity_server_secrets::CredentialPublicationKind::Adopt,
            activate_connection: false,
          },
        },
        idempotency_key: IdempotencyKey::new("adopt-for-check").unwrap(),
        requested_at: Timestamp::from_unix_millis(2).unwrap(),
      }))
      .await
      .unwrap();
    store
      .complete_integration_administration(management_mutation(pending))
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let state = store.integration_administration(id).await.unwrap();
    assert!(!state.enabled);
    let peer = Arc::new(Peer(AtomicUsize::new(0)));
    let policy = Arc::new(CheckPolicy(vec![
      ManagementAction::Administer,
      ManagementAction::View,
      ManagementAction::Execute,
    ]));
    let query = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IntegrationCheckHandlers::new(store.clone(), peer.clone(), policy)),
    );
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let request = CheckIntegrationConnectionQuery {
      id,
      version: IntegrationVersion::INITIAL,
      resource,
      purpose: IntegrationPurpose::TaskRead,
    };
    let result = query.handle_query(&context, request.clone()).await.unwrap();
    assert_eq!(result.readiness, IntegrationCheckReadiness::unverified());
    assert_eq!(peer.0.load(Ordering::SeqCst), 1);
    assert_eq!(store.integration_administration(id).await.unwrap(), state);
    assert_eq!(store.mutation_evidence_counts().await, before);
    let mut unsupported = request.clone();
    unsupported.purpose = IntegrationPurpose::Delivery;
    assert_eq!(
      query
        .handle_query(&context, unsupported)
        .await
        .unwrap()
        .readiness
        .capability,
      IntegrationCheckState::Unsupported
    );
    assert_eq!(peer.0.load(Ordering::SeqCst), 1);
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: Some(IntegrationVersion::INITIAL),
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("replace-after-check").unwrap(),
        published_at: Timestamp::from_unix_millis(3).unwrap(),
      }))
      .await
      .unwrap();
    assert!(query.handle_query(&context, request).await.is_err());
    assert_eq!(
      peer.0.load(Ordering::SeqCst),
      1,
      "old checks cannot be redirected to a replacement"
    );
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}
