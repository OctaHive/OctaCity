use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId, Timestamp};
use octacity_server_store::testing::{
  InMemoryFactoryConfigurationStore, integration_connection_fixture, management_mutation,
};
use octacity_server_store::*;

#[test]
fn runtime_use_requires_live_project_resource_and_independent_purpose_grants() {
  let mut future = std::pin::pin!(async {
    let store = InMemoryFactoryConfigurationStore::new();
    let id = IntegrationId::generate();
    let project_id = ProjectId::generate();
    store.seed_project(project_id).unwrap();
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
    let credential = installed.resources[0].capabilities[0].credential.clone();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("runtime-connection").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let prepared = store
      .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
        intent: IntegrationAdministrationIntent {
          id,
          expected_metadata_version: IntegrationVersion::INITIAL,
          expected_revision: 0,
          action: IntegrationAdministrationAction::Credential {
            reference: credential,
            kind: octacity_server_secrets::CredentialPublicationKind::Adopt,
            activate_connection: true,
          },
        },
        idempotency_key: IdempotencyKey::new("runtime-activate").unwrap(),
        requested_at: Timestamp::from_unix_millis(2).unwrap(),
      }))
      .await
      .unwrap();
    store
      .complete_integration_administration(management_mutation(prepared))
      .await
      .unwrap();
    assert!(store.integration_administration(id).await.unwrap().enabled);
    let request = IntegrationResourceAccess {
      connection_id: id,
      connection_version: IntegrationVersion::INITIAL,
      project_id,
      resource: resource.clone(),
      purpose: IntegrationPurpose::TaskRead,
    };
    assert!(
      authorize_integration_resource(&store, &request).await.is_err(),
      "token configuration is not a Project grant"
    );
    store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          connection_version: IntegrationVersion::INITIAL,
          project_id,
          expected_current_version: None,
          resources: vec![IntegrationResourceSelection {
            reference: resource,
            purposes: vec![IntegrationPurpose::TaskRead],
          }],
        },
        idempotency_key: IdempotencyKey::new("runtime-read-grant").unwrap(),
        published_at: Timestamp::from_unix_millis(3).unwrap(),
      }))
      .await
      .unwrap();
    assert_eq!(
      authorize_integration_resource(&store, &request)
        .await
        .unwrap()
        .capability
        .purpose,
      IntegrationPurpose::TaskRead
    );
    for purpose in [
      IntegrationPurpose::TaskUpdate,
      IntegrationPurpose::RepositoryRead,
      IntegrationPurpose::Delivery,
    ] {
      let mut denied = request.clone();
      denied.purpose = purpose;
      assert!(
        authorize_integration_resource(&store, &denied).await.is_err(),
        "{purpose:?} must remain independent"
      );
    }
    // Granting any other purpose cannot inherit its neighbours' authority.
    for (index, purpose) in [
      IntegrationPurpose::TaskUpdate,
      IntegrationPurpose::RepositoryRead,
      IntegrationPurpose::Delivery,
    ]
    .into_iter()
    .enumerate()
    {
      let current = store.integration_project_access(id, project_id).await.unwrap().unwrap();
      store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: id,
            project_id,
            connection_version: IntegrationVersion::INITIAL,
            expected_current_version: Some(current.version),
            resources: vec![IntegrationResourceSelection {
              reference: request.resource.clone(),
              purposes: vec![purpose],
            }],
          },
          idempotency_key: IdempotencyKey::new(format!("independent-purpose-{index}")).unwrap(),
          published_at: Timestamp::from_unix_millis(4).unwrap(),
        }))
        .await
        .unwrap();
      for required in [
        IntegrationPurpose::TaskRead,
        IntegrationPurpose::TaskUpdate,
        IntegrationPurpose::RepositoryRead,
        IntegrationPurpose::Delivery,
      ] {
        let mut selected = request.clone();
        selected.purpose = required;
        assert_eq!(
          authorize_integration_resource(&store, &selected).await.is_ok(),
          required == purpose
        );
      }
    }
    let mut other_project = request.clone();
    other_project.project_id = ProjectId::generate();
    assert!(authorize_integration_resource(&store, &other_project).await.is_err());
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

use std::sync::Arc;
struct AccessPolicy {
  connection_admin: bool,
  project_admin: bool,
  use_connection: bool,
  view_connection: bool,
  view_project: bool,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for AccessPolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let allowed = match (resource.kind(), action) {
      (ManagementResourceKind::IntegrationConnection, ManagementAction::Administer) => self.connection_admin,
      (ManagementResourceKind::Project, ManagementAction::Administer) => self.project_admin,
      (ManagementResourceKind::IntegrationConnection, ManagementAction::Execute) => self.use_connection,
      (ManagementResourceKind::Project, ManagementAction::View) => self.view_project,
      (ManagementResourceKind::IntegrationConnection, ManagementAction::View) => self.view_connection,
      _ => false,
    };
    if allowed {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}
#[test]
fn grant_publication_needs_both_administrators_but_enumeration_needs_only_visible_use() {
  let mut future = std::pin::pin!(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let id = IntegrationId::generate();
    let project_id = ProjectId::generate();
    store.seed_project(project_id).unwrap();
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
        idempotency_key: IdempotencyKey::new("managed-connection").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let command = PublishIntegrationProjectAccessCommand {
      request: PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          project_id,
          connection_version: IntegrationVersion::INITIAL,
          expected_current_version: None,
          resources,
        },
        idempotency_key: IdempotencyKey::new("managed-grant").unwrap(),
        published_at: Timestamp::from_unix_millis(2).unwrap(),
      },
    };
    for (connection_admin, project_admin) in [(true, false), (false, true)] {
      let policy = Arc::new(AccessPolicy {
        connection_admin,
        project_admin,
        use_connection: true,
        view_connection: true,
        view_project: true,
      });
      let handler = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IntegrationProjectAccessHandlers::new(store.clone(), policy)),
      );
      assert!(handler.handle_command(&context, command.clone()).await.is_err());
      assert!(
        store
          .integration_project_access(id, project_id)
          .await
          .unwrap()
          .is_none()
      );
    }
    let policy = Arc::new(AccessPolicy {
      connection_admin: true,
      project_admin: true,
      use_connection: true,
      view_connection: true,
      view_project: true,
    });
    let handler = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IntegrationProjectAccessHandlers::new(store.clone(), policy)),
    );
    handler.handle_command(&context, command.clone()).await.unwrap();
    let policy = Arc::new(AccessPolicy {
      connection_admin: false,
      project_admin: false,
      use_connection: true,
      view_connection: true,
      view_project: true,
    });
    let handlers = Arc::new(IntegrationProjectAccessHandlers::new(store.clone(), policy.clone()));
    let query = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
    let visible = query
      .handle_query(
        &context,
        ListIntegrationProjectResourcesQuery {
          connection_id: id,
          project_id,
        },
      )
      .await
      .unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].capabilities.len(), 1);
    assert_eq!(visible[0].capabilities[0].purpose, IntegrationPurpose::TaskRead);
    let read = query
      .handle_query(
        &context,
        GetIntegrationProjectAccessQuery {
          connection_id: id,
          project_id,
        },
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(read.resources, command.request.intent.resources);
    for (use_connection, view_connection) in [(true, false), (false, true)] {
      let denied_policy = Arc::new(AccessPolicy {
        connection_admin: true,
        project_admin: true,
        use_connection,
        view_connection,
        view_project: true,
      });
      let denied = AuthorizedQueryHandler::new(
        denied_policy.clone(),
        Arc::new(IntegrationProjectAccessHandlers::new(store.clone(), denied_policy)),
      );
      assert!(
        denied
          .handle_query(
            &context,
            ListIntegrationProjectResourcesQuery {
              connection_id: id,
              project_id
            }
          )
          .await
          .is_err(),
        "enumeration needs both visibility and use, even for an administrator"
      );
    }
    let command_handler = AuthorizedCommandHandler::new(policy, handlers);
    assert!(command_handler.handle_command(&context, command).await.is_err());
    let denied_policy = Arc::new(AccessPolicy {
      connection_admin: true,
      project_admin: true,
      use_connection: true,
      view_connection: true,
      view_project: false,
    });
    let denied = AuthorizedQueryHandler::new(
      denied_policy.clone(),
      Arc::new(IntegrationProjectAccessHandlers::new(store, denied_policy)),
    );
    assert!(
      denied
        .handle_query(
          &context,
          ListIntegrationProjectResourcesQuery {
            connection_id: id,
            project_id
          }
        )
        .await
        .is_err()
    );
  });
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}
