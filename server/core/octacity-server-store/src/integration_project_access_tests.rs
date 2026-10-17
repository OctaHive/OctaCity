#[cfg(test)]
use crate::testing::InMemoryFactoryConfigurationStore;
use crate::testing::{integration_connection_fixture, management_mutation};
use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId, Timestamp};

#[cfg(test)]
#[test]
fn project_grants_are_explicit_resource_and_purpose_subsets() {
  let mut future = std::pin::pin!(async {
    let store = InMemoryFactoryConfigurationStore::new();
    let project_id = ProjectId::generate();
    store.seed_project(project_id).unwrap();
    verify_project_grant_subset(&store, project_id).await;
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

/// Shared public store qualification for explicit Project/resource/purpose grants.
pub async fn verify_project_grant_subset<S: IntegrationConnectionStore + IntegrationProjectAccessStore>(
  store: &S,
  project_id: ProjectId,
) {
  let id = IntegrationId::generate();
  let (mut installed, mut draft) = integration_connection_fixture("custom-tracker");
  for purpose in [
    IntegrationPurpose::TaskUpdate,
    IntegrationPurpose::RepositoryRead,
    IntegrationPurpose::Delivery,
  ] {
    let mut capability = installed.resources[0].capabilities[0].clone();
    capability.purpose = purpose;
    installed.resources[0].capabilities.push(capability);
    draft.resources[0].purposes.push(purpose);
  }
  let resource = draft.resources[0].reference.clone();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("connection").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  assert!(
    store
      .integration_project_access(id, project_id)
      .await
      .unwrap()
      .is_none()
  );
  let granted = store
    .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
      intent: IntegrationProjectAccessIntent {
        connection_id: id,
        project_id,
        connection_version: IntegrationVersion::INITIAL,
        expected_current_version: None,
        resources: vec![IntegrationResourceSelection {
          reference: resource,
          purposes: vec![IntegrationPurpose::TaskRead],
        }],
      },
      idempotency_key: IdempotencyKey::new("grant-read").unwrap(),
      published_at: Timestamp::from_unix_millis(2).unwrap(),
    }))
    .await
    .unwrap();
  assert_eq!(granted.access.version, IntegrationVersion::INITIAL);
  assert_eq!(granted.access.resources.len(), 1);
  assert_eq!(granted.access.resources[0].purposes, vec![IntegrationPurpose::TaskRead]);
  assert_eq!(
    store.integration_project_access(id, project_id).await.unwrap(),
    Some(granted.access)
  );
}

#[cfg(test)]
#[test]
fn project_grants_reject_uninstalled_purposes_and_duplicate_resource_selections() {
  let mut future = std::pin::pin!(async {
    let store = InMemoryFactoryConfigurationStore::new();
    let id = IntegrationId::generate();
    let (installed, draft) = integration_connection_fixture("jira");
    let resource = draft.resources[0].reference.clone();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("restricted-connection").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    for (key, resources) in [
      (
        "write-not-installed",
        vec![IntegrationResourceSelection {
          reference: resource.clone(),
          purposes: vec![IntegrationPurpose::TaskUpdate],
        }],
      ),
      (
        "duplicate-resource",
        vec![
          IntegrationResourceSelection {
            reference: resource.clone(),
            purposes: vec![IntegrationPurpose::TaskRead]
          };
          2
        ],
      ),
      (
        "duplicate-purpose",
        vec![IntegrationResourceSelection {
          reference: resource.clone(),
          purposes: vec![IntegrationPurpose::TaskRead; 2],
        }],
      ),
      (
        "empty-purpose",
        vec![IntegrationResourceSelection {
          reference: resource.clone(),
          purposes: vec![],
        }],
      ),
    ] {
      let project_id = ProjectId::generate();
      store.seed_project(project_id).unwrap();
      let result = store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: id,
            project_id,
            connection_version: IntegrationVersion::INITIAL,
            expected_current_version: None,
            resources,
          },
          idempotency_key: IdempotencyKey::new(key).unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        }))
        .await;
      assert!(result.is_err(), "{key} must not grant use");
      assert!(
        store
          .integration_project_access(id, project_id)
          .await
          .unwrap()
          .is_none()
      );
    }
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

/// Shared recovery contract: historical grant replay cannot undo current revocation.
pub async fn verify_project_grant_replay<S: IntegrationConnectionStore + IntegrationProjectAccessStore>(
  store: &S,
  project_id: ProjectId,
) {
  let id = IntegrationId::generate();
  let (installed, draft) = integration_connection_fixture("gitlab");
  let resources = draft.resources.clone();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("replay-metadata").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  let mut request = PublishIntegrationProjectAccess {
    intent: IntegrationProjectAccessIntent {
      connection_id: id,
      connection_version: IntegrationVersion::INITIAL,
      project_id,
      expected_current_version: None,
      resources,
    },
    idempotency_key: IdempotencyKey::new("initial-project-grant").unwrap(),
    published_at: Timestamp::from_unix_millis(2).unwrap(),
  };
  let original_request = request.clone();
  let original = store
    .publish_integration_project_access(management_mutation(request.clone()))
    .await
    .unwrap();
  request.intent.expected_current_version = Some(original.access.version);
  request.intent.resources.clear();
  request.idempotency_key = IdempotencyKey::new("revoke-project-grant").unwrap();
  let revoked = store
    .publish_integration_project_access(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(revoked.access.version.get(), 2);
  assert!(revoked.access.resources.is_empty());
  let replay = store
    .publish_integration_project_access(management_mutation(original_request.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.access, original.access);
  assert_eq!(
    store.integration_project_access(id, project_id).await.unwrap(),
    Some(revoked.access.clone())
  );
  request.idempotency_key = IdempotencyKey::new("stale-project-grant").unwrap();
  request.intent.resources = original_request.intent.resources.clone();
  assert!(
    store
      .publish_integration_project_access(management_mutation(request))
      .await
      .is_err()
  );
  let mut conflicting = original_request;
  conflicting.intent.resources.clear();
  assert!(
    store
      .publish_integration_project_access(management_mutation(conflicting))
      .await
      .is_err()
  );
  assert_eq!(
    store.integration_project_access(id, project_id).await.unwrap(),
    Some(revoked.access)
  );
}
#[cfg(test)]
#[test]
fn project_grant_replay_does_not_restore_revoked_access_or_accept_stale_publication() {
  let mut future = std::pin::pin!(async {
    let store = InMemoryFactoryConfigurationStore::new();
    let project_id = ProjectId::generate();
    store.seed_project(project_id).unwrap();
    verify_project_grant_replay(&store, project_id).await;
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

#[cfg(test)]
#[test]
fn project_grants_require_an_existing_project() {
  let mut future = std::pin::pin!(async {
    let store = InMemoryFactoryConfigurationStore::new();
    let id = IntegrationId::generate();
    let (installed, draft) = integration_connection_fixture("custom-tracker");
    let resources = draft.resources.clone();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("missing-project-connection").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let project_id = ProjectId::generate();
    let result = store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          project_id,
          connection_version: IntegrationVersion::INITIAL,
          expected_current_version: None,
          resources,
        },
        idempotency_key: IdempotencyKey::new("missing-project-grant").unwrap(),
        published_at: Timestamp::from_unix_millis(2).unwrap(),
      }))
      .await;
    assert!(result.is_err(), "a grant must not create an implicit Project");
    assert!(
      store
        .integration_project_access(id, project_id)
        .await
        .unwrap()
        .is_none()
    );
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}
