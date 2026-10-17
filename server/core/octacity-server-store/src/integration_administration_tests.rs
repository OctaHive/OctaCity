#[cfg(test)]
use crate::testing::InMemoryFactoryConfigurationStore;
use crate::testing::{integration_connection_fixture, management_mutation};
use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
use octacity_server_secrets::CredentialPublicationKind;

#[test]
fn credential_administration_retains_preparation_and_receipts_without_reactivating_old_rotations() {
  let future = async {
    let store = InMemoryFactoryConfigurationStore::new();
    verify_integration_administration(&store).await;
  };
  let mut future = std::pin::pin!(future);
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

/// Shared behavioral qualification for persistent and memory administrative journals.
pub async fn verify_integration_administration<S: IntegrationConnectionStore + IntegrationAdministrationStore>(
  store: &S,
) {
  let id = IntegrationId::generate();
  let (installed, draft) = integration_connection_fixture("jira");
  let reference = installed.resources[0].capabilities[0].credential.clone();
  let metadata = store
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
    .unwrap()
    .connection;
  let request = BeginIntegrationAdministration {
    intent: IntegrationAdministrationIntent {
      id,
      expected_metadata_version: IntegrationVersion::INITIAL,
      expected_revision: 0,
      action: IntegrationAdministrationAction::Credential {
        reference: reference.clone(),
        kind: CredentialPublicationKind::Provision,
        activate_connection: false,
      },
    },
    idempotency_key: IdempotencyKey::new("credential-create").unwrap(),
    requested_at: Timestamp::from_unix_millis(10).unwrap(),
  };
  let first = store
    .begin_integration_administration(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(first.status, IntegrationAdministrationStatus::Prepared);
  assert!(!store.integration_administration(id).await.unwrap().enabled);
  assert_eq!(
    store
      .begin_integration_administration(management_mutation(request.clone()))
      .await
      .unwrap(),
    first
  );
  let mut activate = request.clone();
  activate.intent.expected_revision = first.revision;
  activate.intent.action = IntegrationAdministrationAction::Activate;
  activate.idempotency_key = IdempotencyKey::new("activate-too-early").unwrap();
  assert!(
    store
      .begin_integration_administration(management_mutation(activate.clone()))
      .await
      .is_err()
  );
  let finished = store
    .complete_integration_administration(management_mutation(first.clone()))
    .await
    .unwrap();
  assert_eq!(finished.status, IntegrationAdministrationStatus::Completed);
  assert_eq!(
    store
      .begin_integration_administration(management_mutation(request.clone()))
      .await
      .unwrap(),
    finished
  );
  activate.idempotency_key = IdempotencyKey::new("activate").unwrap();
  let active = store
    .begin_integration_administration(management_mutation(activate))
    .await
    .unwrap();
  assert!(store.integration_administration(id).await.unwrap().enabled);
  let mut rotation = request.clone();
  rotation.intent.expected_revision = active.revision;
  rotation.intent.action = IntegrationAdministrationAction::Credential {
    reference,
    kind: CredentialPublicationKind::Rotate,
    activate_connection: false,
  };
  rotation.idempotency_key = IdempotencyKey::new("rotate").unwrap();
  let pending = store
    .begin_integration_administration(management_mutation(rotation))
    .await
    .unwrap();
  assert!(!store.integration_administration(id).await.unwrap().enabled);
  let mut disable = request.clone();
  disable.intent.expected_revision = pending.revision;
  disable.intent.action = IntegrationAdministrationAction::Disable;
  disable.idempotency_key = IdempotencyKey::new("disable").unwrap();
  let disabled = store
    .begin_integration_administration(management_mutation(disable))
    .await
    .unwrap();
  assert!(
    store
      .complete_integration_administration(management_mutation(pending))
      .await
      .is_err()
  );
  assert_eq!(
    store
      .begin_integration_administration(management_mutation(request))
      .await
      .unwrap(),
    finished
  );
  assert!(!store.integration_administration(id).await.unwrap().enabled);
  assert_eq!(
    store.integration_connection(id, Some(metadata.version)).await.unwrap(),
    metadata
  );
  let reactivate = BeginIntegrationAdministration {
    intent: IntegrationAdministrationIntent {
      id,
      expected_metadata_version: IntegrationVersion::INITIAL,
      expected_revision: disabled.revision,
      action: IntegrationAdministrationAction::Activate,
    },
    idempotency_key: IdempotencyKey::new("activate-cancelled-preparation").unwrap(),
    requested_at: Timestamp::from_unix_millis(30).unwrap(),
  };
  assert!(
    store
      .begin_integration_administration(management_mutation(reactivate))
      .await
      .is_err(),
    "cancelled rotation cannot be activated using the old configured flag"
  );
  let (installed, mut draft) = integration_connection_fixture("jira");
  let reference = installed.resources[0].capabilities[0].credential.clone();
  let pending = store
    .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
      intent: IntegrationAdministrationIntent {
        id,
        expected_metadata_version: IntegrationVersion::INITIAL,
        expected_revision: disabled.revision,
        action: IntegrationAdministrationAction::Credential {
          reference,
          kind: CredentialPublicationKind::Adopt,
          activate_connection: false,
        },
      },
      idempotency_key: IdempotencyKey::new("adopt-before-metadata-replacement").unwrap(),
      requested_at: Timestamp::from_unix_millis(40).unwrap(),
    }))
    .await
    .unwrap();
  draft.name = octacity_server_domain::IntegrationName::new("Replacement connection").unwrap();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id,
        expected_current_version: Some(IntegrationVersion::INITIAL),
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("replace-metadata").unwrap(),
      published_at: Timestamp::from_unix_millis(50).unwrap(),
    }))
    .await
    .unwrap();
  assert!(
    store
      .complete_integration_administration(management_mutation(pending))
      .await
      .is_err(),
    "metadata replacement cannot activate an earlier preparation"
  );
  assert_eq!(
    store.integration_connection(id, Some(metadata.version)).await.unwrap(),
    metadata
  );
}

#[test]
fn credential_preparation_cleanup_is_bounded_and_fences_expired_operations() {
  let store = InMemoryFactoryConfigurationStore::new();
  let future = async {
    use crate::testing::ManagementAuditProbe as _;
    verify_integration_preparation_cleanup(&store).await;
    let facts = store.management_audit_facts().await;
    let attributed: Vec<_> = facts
      .iter()
      .filter(|fact| fact.actor_identity.as_deref() == Some("cleanup-operator"))
      .collect();
    assert_eq!(
      attributed.len(),
      1,
      "only the first expiry transition records the cleanup actor"
    );
    assert_eq!(attributed[0].request_identity, "cleanup-first-pass");
    assert_eq!(
      attributed[0].target_kind,
      octacity_server_domain::EntityKind::Integration
    );
  };
  let mut future = std::pin::pin!(future);
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

/// Shared qualification for bounded cleanup, retry and late-completion fencing.
pub async fn verify_integration_preparation_cleanup<S: IntegrationConnectionStore + IntegrationAdministrationStore>(
  store: &S,
) {
  let id = IntegrationId::generate();
  let (mut installed, mut draft) = integration_connection_fixture("custom-provider");
  let reference = installed.resources[0].capabilities[0].credential.clone();
  let mut independent = installed.resources[0].capabilities[0].clone();
  independent.purpose = IntegrationPurpose::TaskUpdate;
  independent.credential = octacity_server_secrets::LogicalSecretReference::new(
    octacity_server_secrets::SecretProviderId::new("installed").unwrap(),
    "independent-writer",
  )
  .unwrap();
  let other_reference = independent.credential.clone();
  installed.resources[0].capabilities.push(independent);
  draft.resources[0].purposes.push(IntegrationPurpose::TaskUpdate);
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("cleanup-metadata").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  let request = BeginIntegrationAdministration {
    intent: IntegrationAdministrationIntent {
      id,
      expected_metadata_version: IntegrationVersion::INITIAL,
      expected_revision: 0,
      action: IntegrationAdministrationAction::Credential {
        reference: reference.clone(),
        kind: CredentialPublicationKind::Provision,
        activate_connection: false,
      },
    },
    idempotency_key: IdempotencyKey::new("expires").unwrap(),
    requested_at: Timestamp::from_unix_millis(10).unwrap(),
  };
  let mut not_ready = request.clone();
  if let IntegrationAdministrationAction::Credential {
    activate_connection, ..
  } = &mut not_ready.intent.action
  {
    *activate_connection = true;
  }
  assert!(
    store
      .begin_integration_administration(management_mutation(not_ready))
      .await
      .is_err(),
    "activation requires all independently selected purposes to be configured"
  );
  let pending = store
    .begin_integration_administration(management_mutation(request.clone()))
    .await
    .unwrap();
  let mut cleanup = IntegrationPreparationCleanup {
    id,
    requested_at: Timestamp::from_unix_millis(1000).unwrap(),
    older_than: Timestamp::from_unix_millis(9).unwrap(),
    limit: 1,
  };
  assert!(
    store
      .integration_preparation_cleanup(management_mutation(cleanup.clone()))
      .await
      .unwrap()
      .is_empty()
  );
  cleanup.older_than = Timestamp::from_unix_millis(10).unwrap();
  let cleanup_audit = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("cleanup-operator".into()),
    },
    ManagementSecurityScope::new("credential-maintenance").unwrap(),
    "cleanup-first-pass",
  )
  .unwrap();
  let batch = store
    .integration_preparation_cleanup(ManagementMutation::new(cleanup.clone(), cleanup_audit.clone()))
    .await
    .unwrap();
  assert_eq!(batch, vec![pending.publication.clone().unwrap()]);
  assert!(
    store
      .complete_integration_administration(management_mutation(pending))
      .await
      .is_err()
  );
  assert_eq!(
    store
      .begin_integration_administration(management_mutation(request))
      .await
      .unwrap()
      .status,
    IntegrationAdministrationStatus::Abandoned
  );
  assert_eq!(
    store
      .integration_preparation_cleanup(ManagementMutation::new(cleanup.clone(), cleanup_audit))
      .await
      .unwrap(),
    batch
  );
  store
    .acknowledge_integration_preparation_cleanup(management_mutation(batch[0].clone()))
    .await
    .unwrap();
  assert!(
    store
      .integration_preparation_cleanup(management_mutation(cleanup.clone()))
      .await
      .unwrap()
      .is_empty()
  );
  cleanup.limit = 101;
  assert!(
    store
      .integration_preparation_cleanup(management_mutation(cleanup))
      .await
      .is_err()
  );
  let state = store.integration_administration(id).await.unwrap();
  assert!(!state.enabled);
  assert!(state.pending.is_none());
  let mut other = BeginIntegrationAdministration {
    intent: IntegrationAdministrationIntent {
      id,
      expected_metadata_version: IntegrationVersion::INITIAL,
      expected_revision: state.revision,
      action: IntegrationAdministrationAction::Credential {
        reference: other_reference.clone(),
        kind: CredentialPublicationKind::Provision,
        activate_connection: false,
      },
    },
    idempotency_key: IdempotencyKey::new("independent-purpose").unwrap(),
    requested_at: Timestamp::from_unix_millis(20).unwrap(),
  };
  let published = store
    .begin_integration_administration(management_mutation(other.clone()))
    .await
    .unwrap();
  store
    .complete_integration_administration(management_mutation(published.clone()))
    .await
    .unwrap();
  other.intent.expected_revision = published.revision;
  other.intent.action = IntegrationAdministrationAction::Credential {
    reference,
    kind: CredentialPublicationKind::Provision,
    activate_connection: false,
  };
  other.idempotency_key = IdempotencyKey::new("another-expired-reader").unwrap();
  other.requested_at = Timestamp::from_unix_millis(30).unwrap();
  store
    .begin_integration_administration(management_mutation(other))
    .await
    .unwrap();
  store
    .integration_preparation_cleanup(management_mutation(IntegrationPreparationCleanup {
      id,
      requested_at: Timestamp::from_unix_millis(2000).unwrap(),
      older_than: Timestamp::from_unix_millis(30).unwrap(),
      limit: 100,
    }))
    .await
    .unwrap();
  let state = store.integration_administration(id).await.unwrap();
  assert!(
    state
      .credentials
      .iter()
      .any(|c| c.reference == other_reference && c.configured),
    "orphan cleanup cannot revoke an independent credential purpose"
  );
}
