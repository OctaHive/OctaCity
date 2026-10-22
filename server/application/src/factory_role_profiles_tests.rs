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

fn installed_role_choices(
  project: ProjectId,
) -> (
  octacity_server_factory::FactoryConfigurationChoices,
  octacity_server_factory::FactoryRoleProfileDefinition,
) {
  use octacity_server_factory::*;
  let mut definition = octacity_server_store::testing::role_profile_contract_definition(project);
  definition.execution.integrations.clear();
  definition.execution.mcp_servers.clear();
  definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    executables: vec![definition.toolchain.executable.clone()],
    commands: vec![
      CommandPermission::try_new(
        definition.toolchain.executable.clone(),
        vec![CommandArgumentPattern::Exact(
          FactoryText::new("--approved-private-argument").unwrap(),
        )],
      )
      .unwrap(),
    ],
    ..Default::default()
  })
  .unwrap();
  let refs = [
    definition.toolchain.plugin.clone(),
    definition.toolchain.executable.clone(),
    definition.toolchain.model.clone(),
  ]
  .into_iter()
  .chain(definition.skills.values().cloned())
  .enumerate()
  .map(|(i, reference)| FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: FactoryKey::new(format!("dependency-{i}")).unwrap(),
    reference,
  })
  .collect();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: refs,
    build_configurations: vec![(
      FactoryKey::new("build").unwrap(),
      definition.build_configuration.clone(),
    )],
  })
  .unwrap();
  let schema = |name, fields| {
    FlowDataSchema::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new("v1").unwrap(),
      FlowValueSchema::Object { fields },
    )
    .unwrap()
  };
  let capability = FactoryRoleProfileCapability {
    ceiling: definition.clone(),
    parameters: schema(
      "parameters",
      [(
        FactoryKey::new("reasoning_effort").unwrap(),
        FlowFieldSchema::required(FlowValueSchema::Enum {
          values: vec![serde_json::json!("medium"), serde_json::json!("high")],
        }),
      )]
      .into(),
    ),
    public_environment: [(
      "LANGUAGE".into(),
      FlowDataSchema::new(
        FactoryKey::new("environment").unwrap(),
        FactoryKey::new("v1").unwrap(),
        FlowValueSchema::String {
          min_bytes: 0,
          max_bytes: 20,
        },
      )
      .unwrap(),
    )]
    .into(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  (
    choices
      .with_role_profile_capability(FactoryKey::new("dependency-1").unwrap(), capability)
      .unwrap(),
    definition,
  )
}
#[test]
fn factory_role_profile_catalog_and_validation_are_authorized_normalized_and_read_only() {
  use octacity_server_factory::*;
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let (choices, definition) = installed_role_choices(project);
    store.enable_project(project, choices).unwrap();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let published = commands
      .handle_command(
        &context(),
        PublishFactoryRoleProfilesCommand {
          intent: role_profile_contract_intent(project),
          idempotency_key: IdempotencyKey::new("role-validation").unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let before = store.mutation_evidence_counts().await;
    let page = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].alias.as_str(), "dependency-1");
    assert_eq!(page.next_cursor, None);
    assert_eq!(page.items[0].secret_environment_names, ["MODEL_AUTH"]);
    let safe = format!("{page:?}");
    assert!(!safe.contains("projects/model"));
    assert!(!safe.contains("--output"));
    assert!(!safe.contains("--approved-private-argument"));
    assert!(!safe.contains("Use selected skills"));
    let details = queries
      .handle_query(
        &context(),
        GetFactoryRoleProfileCapabilityQuery {
          project_id: project,
          capability: FactoryKey::new("dependency-1").unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(details.definition, definition);
    let query = ValidateFactoryRoleProfileQuery {
      project_id: project,
      id: FactoryRoleProfileId::generate(),
      role: published.role.unwrap().reference().clone(),
      capability: FactoryKey::new("dependency-1").unwrap(),
      definition: definition.clone(),
    };
    let result = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(result.profile.unwrap().definition(), &definition);
    assert_eq!(result.incompatibility, None);
    let mut invalid = query.clone();
    invalid.definition.model_parameters.insert(
      FactoryKey::new("reasoning_effort").unwrap(),
      serde_json::json!("unsupported"),
    );
    let result = queries.handle_query(&context(), invalid).await.unwrap();
    assert_eq!(
      result.incompatibility,
      Some(FactoryRoleProfileIncompatibility::ModelParameters)
    );
    assert_eq!(result.profile, None);
    let deny = AuthorizedQueryHandler::new(Arc::new(Scoped(ManagementVisibility::none())), handlers);
    assert!(deny.handle_query(&context(), query).await.is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn factory_role_owned_catalogs_and_template_details_work_without_a_provider_or_factory_mode() {
  run_ready(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let project = ProjectId::generate();
    store.seed_project(project).unwrap();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let saved = commands
      .handle_command(
        &context(),
        PublishFactoryRoleProfilesCommand {
          intent: role_profile_contract_intent(project),
          idempotency_key: IdempotencyKey::new("catalog-owner").unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let before = store.mutation_evidence_counts().await;
    let roles = queries
      .handle_query(
        &context(),
        ListFactoryRolesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(roles.total, 1);
    assert_eq!(roles.items[0].reference, *saved.role.as_ref().unwrap().reference());
    let profiles = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfilesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
          role_id: saved.role.unwrap().reference().id,
        },
      )
      .await
      .unwrap();
    assert_eq!(profiles.total, 2);
    assert!(profiles.next_cursor.is_some());
    let templates = queries
      .handle_query(
        &context(),
        ListFactoryRoleTemplatesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(templates.total, 3);
    assert_eq!(templates.items[0].reference.identity().as_str(), "architect");
    assert_eq!(templates.next_cursor.as_ref().unwrap().as_str(), "architect");
    let selected = templates.items[0].reference.clone();
    let detail = queries
      .handle_query(
        &context(),
        GetFactoryRoleTemplateQuery {
          project_id: project,
          reference: selected.clone(),
        },
      )
      .await
      .unwrap();
    assert!(!detail.role.instructions.entries()[0].is_empty());
    let denied = AuthorizedQueryHandler::new(Arc::new(Scoped(ManagementVisibility::none())), handlers);
    assert!(
      denied
        .handle_query(
          &context(),
          GetFactoryRoleTemplateQuery {
            project_id: project,
            reference: selected
          }
        )
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn factory_role_profile_optional_integrations_are_filtered_without_blocking_other_profiles() {
  use octacity_server_domain::{IntegrationId, IntegrationVersion};
  use octacity_server_factory::*;
  use octacity_server_store::testing::{integration_connection_fixture, management_mutation};
  use octacity_server_store::*;
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let (installed, draft) = integration_connection_fixture("custom-tracker");
    let resource = installed.resources[0].reference.clone();
    let capability = installed.resources[0].capabilities[0].clone();
    let connection = IntegrationId::generate();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id: connection,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("profile-connection").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let access = IntegrationProjectAccessIntent {
      connection_id: connection,
      project_id: project,
      connection_version: IntegrationVersion::INITIAL,
      expected_current_version: None,
      resources: vec![IntegrationResourceSelection {
        reference: resource.clone(),
        purposes: vec![IntegrationPurpose::TaskRead],
      }],
    };
    store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: access.clone(),
        idempotency_key: IdempotencyKey::new("profile-access").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      }))
      .await
      .unwrap();
    let (choices, mut definition) = installed_role_choices(project);
    definition.execution.integrations.insert(
      FactoryKey::new("tracker").unwrap(),
      FactoryRoleIntegrationBinding {
        project_id: project,
        connection_id: connection,
        connection_version: IntegrationVersion::INITIAL,
        access_version: IntegrationVersion::INITIAL,
        resource: resource.clone(),
        capability: capability.capability.clone(),
      },
    );
    definition.execution.mcp_servers.insert(
      FactoryKey::new("tracker-mcp").unwrap(),
      FactoryRoleMcpBinding {
        configuration: definition.skills.values().next().unwrap().clone(),
        integration: Some(FactoryKey::new("tracker").unwrap()),
      },
    );
    let alias = FactoryKey::new("dependency-1").unwrap();
    let mut declared = choices.role_profile_capabilities()[&alias].clone();
    declared.ceiling = definition.clone();
    // Managed resources remain in the existing connection/grant owner, not a second catalog.
    let entries = choices.entries();
    store
      .enable_project(
        project,
        FactoryConfigurationChoices::try_new(entries)
          .unwrap()
          .with_role_profile_capability(alias.clone(), declared)
          .unwrap(),
      )
      .unwrap();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let page = || ListFactoryRoleProfileCapabilitiesQuery {
      page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
    };
    let inactive = queries.handle_query(&context(), page()).await.unwrap();
    assert_eq!(
      inactive.total, 1,
      "an inactive optional connection must not hide the runtime"
    );
    assert!(inactive.items[0].integrations.is_empty());
    assert!(inactive.items[0].mcp_servers.is_empty());
    let preparation = store
      .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
        intent: IntegrationAdministrationIntent {
          id: connection,
          expected_metadata_version: IntegrationVersion::INITIAL,
          expected_revision: 0,
          action: IntegrationAdministrationAction::Credential {
            reference: capability.credential,
            kind: octacity_server_secrets::CredentialPublicationKind::Adopt,
            activate_connection: true,
          },
        },
        idempotency_key: IdempotencyKey::new("activate-profile-connection").unwrap(),
        requested_at: Timestamp::from_unix_millis(2).unwrap(),
      }))
      .await
      .unwrap();
    store
      .complete_integration_administration(management_mutation(preparation))
      .await
      .unwrap();
    let active = queries.handle_query(&context(), page()).await.unwrap();
    assert_eq!(active.total, 1);
    assert_eq!(active.items[0].integrations.len(), 1);
    assert_eq!(active.items[0].mcp_servers.len(), 1);
    let saved = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers)
      .handle_command(
        &context(),
        PublishFactoryRoleProfilesCommand {
          intent: role_profile_contract_intent(project),
          idempotency_key: IdempotencyKey::new("integration-role").unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    let query = ValidateFactoryRoleProfileQuery {
      project_id: project,
      id: FactoryRoleProfileId::generate(),
      role: saved.role.unwrap().reference().clone(),
      capability: alias,
      definition,
    };
    assert!(
      queries
        .handle_query(&context(), query.clone())
        .await
        .unwrap()
        .profile
        .is_some()
    );
    let mut revoked = access;
    revoked.expected_current_version = Some(IntegrationVersion::INITIAL);
    revoked.resources.clear();
    store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: revoked,
        idempotency_key: IdempotencyKey::new("revoke-profile-access").unwrap(),
        published_at: Timestamp::from_unix_millis(3).unwrap(),
      }))
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let available = queries.handle_query(&context(), page()).await.unwrap();
    assert_eq!(available.total, 1);
    assert!(available.items[0].integrations.is_empty());
    assert!(available.items[0].mcp_servers.is_empty());
    assert_eq!(available.next_cursor, None);
    let details = queries
      .handle_query(
        &context(),
        GetFactoryRoleProfileCapabilityQuery {
          project_id: project,
          capability: query.capability.clone(),
        },
      )
      .await
      .unwrap();
    assert!(details.definition.execution.integrations.is_empty());
    assert!(details.definition.execution.mcp_servers.is_empty());
    let mut independent = query.clone();
    independent.definition.execution.integrations.clear();
    independent.definition.execution.mcp_servers.clear();
    let result = queries.handle_query(&context(), independent.clone()).await.unwrap();
    assert_eq!(result.profile.unwrap().definition(), &independent.definition);
    assert_eq!(result.incompatibility, None);
    let rejected = queries.handle_query(&context(), query).await.unwrap();
    assert_eq!(
      rejected.incompatibility,
      Some(FactoryRoleProfileIncompatibility::Dependencies)
    );
    assert!(rejected.profile.is_none());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

struct RoleChoiceScope(Vec<FactoryChoiceIdentity>);
#[async_trait]
impl ManagementAuthorizationPolicy for RoleChoiceScope {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::FactoryChoice {
      Ok(ManagementAuthorizationGrant::new(
        ManagementVisibility::restricted(self.0.iter().map(|identity| {
          ManagementResource::instance(
            ManagementResourceKind::FactoryChoice,
            ManagementResourceIdentity::new(identity.to_string()).unwrap(),
          )
          .unwrap()
        }))
        .unwrap(),
      ))
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
#[test]
fn factory_role_profile_choices_filter_dependencies_before_counts_and_cursors() {
  use octacity_server_factory::*;
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let (choices, definition) = installed_role_choices(project);
    let alias = FactoryKey::new("dependency-1").unwrap();
    let second = FactoryKey::new("runtime-second").unwrap();
    let capability = choices.role_profile_capabilities()[&alias].clone();
    let mut entries = choices.entries();
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: second.clone(),
      reference: definition.toolchain.executable.clone(),
    });
    let choices = FactoryConfigurationChoices::try_new(entries.clone())
      .unwrap()
      .with_role_profile_capability(alias.clone(), capability.clone())
      .unwrap()
      .with_role_profile_capability(second.clone(), capability)
      .unwrap();
    store.enable_project(project, choices).unwrap();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let saved = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone())
      .handle_command(
        &context(),
        PublishFactoryRoleProfilesCommand {
          intent: role_profile_contract_intent(project),
          idempotency_key: IdempotencyKey::new("scoped-role").unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let page = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].alias, alias);
    let tail = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(project, page.next_cursor, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(tail.total, 2);
    assert_eq!(tail.items[0].alias, second);
    assert_eq!(tail.next_cursor, None);
    let visible = entries
      .references
      .iter()
      .filter(|entry| entry.reference != definition.toolchain.model)
      .map(|entry| FactoryChoiceIdentity {
        project_id: project,
        kind: entry.kind,
        alias: entry.alias.clone(),
      })
      .chain(
        entries
          .build_configurations
          .iter()
          .map(|(alias, _)| FactoryChoiceIdentity {
            project_id: project,
            kind: FactoryChoiceKind::BuildConfiguration,
            alias: alias.clone(),
          }),
      )
      .collect();
    let policy = Arc::new(RoleChoiceScope(visible));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy)),
    );
    let before = store.mutation_evidence_counts().await;
    let page = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(page.total, 0);
    assert!(page.items.is_empty());
    assert_eq!(page.next_cursor, None);
    let query = ValidateFactoryRoleProfileQuery {
      project_id: project,
      id: FactoryRoleProfileId::generate(),
      role: saved.role.unwrap().reference().clone(),
      capability: alias,
      definition,
    };
    let error = queries.handle_query(&context(), query).await.unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::CapabilityUnavailable
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn factory_role_profile_hidden_optional_selections_do_not_hide_the_runtime() {
  use octacity_server_factory::*;
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let (choices, mut definition) = installed_role_choices(project);
    let alias = FactoryKey::new("dependency-1").unwrap();
    let mcp = FactoryKey::new("optional-mcp").unwrap();
    let mcp_reference = definition.skills.values().next().unwrap().clone();
    definition.execution.mcp_servers.insert(
      mcp.clone(),
      FactoryRoleMcpBinding {
        configuration: mcp_reference.clone(),
        integration: None,
      },
    );
    let independent_permissions = definition.permissions.clone();
    definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      executables: independent_permissions.executables().cloned().collect(),
      commands: independent_permissions.commands().cloned().collect(),
      tools: vec![mcp_reference],
      ..Default::default()
    })
    .unwrap();
    let mut capability = choices.role_profile_capabilities()[&alias].clone();
    capability.ceiling = definition.clone();
    capability.local_permissions = LocalPermissionCeiling::fully_enforced(definition.permissions.clone());
    let entries = choices.entries();
    let visible = entries
      .references
      .iter()
      .filter(|entry| !definition.skills.values().any(|skill| skill == &entry.reference))
      .map(|entry| FactoryChoiceIdentity {
        project_id: project,
        kind: entry.kind,
        alias: entry.alias.clone(),
      })
      .chain(
        entries
          .build_configurations
          .iter()
          .map(|(alias, _)| FactoryChoiceIdentity {
            project_id: project,
            kind: FactoryChoiceKind::BuildConfiguration,
            alias: alias.clone(),
          }),
      )
      .collect();
    store
      .enable_project(
        project,
        FactoryConfigurationChoices::try_new(entries)
          .unwrap()
          .with_role_profile_capability(alias.clone(), capability)
          .unwrap(),
      )
      .unwrap();
    let saved = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    )
    .handle_command(
      &context(),
      PublishFactoryRoleProfilesCommand {
        intent: role_profile_contract_intent(project),
        idempotency_key: IdempotencyKey::new("optional-dependencies").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      },
    )
    .await
    .unwrap();
    let policy = Arc::new(RoleChoiceScope(visible));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy)),
    );
    let before = store.mutation_evidence_counts().await;
    let page = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      page.total, 1,
      "hidden optional selections must not hide a visible runtime"
    );
    assert_eq!(page.items[0].alias, alias);
    assert!(page.items[0].skills.is_empty());
    assert!(page.items[0].mcp_servers.is_empty());
    assert!(page.items[0].tools.is_empty());
    assert_eq!(page.next_cursor, None);
    let detail = queries
      .handle_query(
        &context(),
        GetFactoryRoleProfileCapabilityQuery {
          project_id: project,
          capability: alias.clone(),
        },
      )
      .await
      .unwrap();
    assert!(detail.definition.skills.is_empty());
    assert!(detail.definition.execution.mcp_servers.is_empty());
    assert_eq!(detail.definition.permissions, independent_permissions);
    let mut query = ValidateFactoryRoleProfileQuery {
      project_id: project,
      id: FactoryRoleProfileId::generate(),
      role: saved.role.unwrap().reference().clone(),
      capability: alias,
      definition: definition.clone(),
    };
    query.definition.skills.clear();
    query.definition.execution.mcp_servers.clear();
    query.definition.permissions = independent_permissions;
    let result = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(result.profile.unwrap().definition(), &query.definition);
    assert_eq!(result.incompatibility, None);
    query.definition.skills = definition.skills;
    let rejected = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(
      rejected.incompatibility,
      Some(FactoryRoleProfileIncompatibility::Dependencies)
    );
    assert!(rejected.profile.is_none());
    query.definition.skills.clear();
    query.definition.execution.mcp_servers = definition.execution.mcp_servers;
    let rejected = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(
      rejected.incompatibility,
      Some(FactoryRoleProfileIncompatibility::Dependencies)
    );
    assert!(rejected.profile.is_none());
    query.definition.execution.mcp_servers.clear();
    query.definition.permissions = definition.permissions;
    let rejected = queries.handle_query(&context(), query).await.unwrap();
    assert_eq!(
      rejected.incompatibility,
      Some(FactoryRoleProfileIncompatibility::Permissions)
    );
    assert!(rejected.profile.is_none());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

struct NoRoleChoices;
#[async_trait]
impl ManagementAuthorizationPolicy for NoRoleChoices {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(
      if resource.kind() == ManagementResourceKind::FactoryChoice {
        ManagementVisibility::none()
      } else {
        ManagementVisibility::all()
      },
    ))
  }
}
#[test]
fn factory_role_profile_empty_dependency_scope_is_an_empty_page() {
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let (choices, _) = installed_role_choices(project);
    store.enable_project(project, choices).unwrap();
    let policy = Arc::new(NoRoleChoices);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy)),
    );
    let page = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 1).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(page.total, 0);
    assert!(page.items.is_empty());
    assert_eq!(page.next_cursor, None);
  });
}
