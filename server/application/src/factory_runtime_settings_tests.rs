use crate::factory_test_support::installed_role_choices;
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryConfigurationAvailability, FactoryConfigurationStore, FactoryRuntimeSettingsStore, IdempotencyKey,
  MutationDisposition,
  testing::{InMemoryFactoryConfigurationStore, MutationEvidenceProbe},
};
use std::sync::Arc;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}
fn run_ready(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(future.as_mut().poll(&mut context).is_ready());
}
fn fixture() -> (Arc<InMemoryFactoryConfigurationStore>, FactoryRuntimeSettingsIntent) {
  let project = ProjectId::generate();
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let (choices, definition) = installed_role_choices(project);
  let runtime = key("dependency-1");
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    network_hosts: vec![NetworkHost::new("mcp.example").unwrap()],
    ..Default::default()
  })
  .unwrap();
  let mut cap = choices.role_profile_capabilities()[&runtime].clone();
  cap.ceiling.permissions = permissions.clone();
  cap.local_permissions = LocalPermissionCeiling::fully_enforced(permissions.clone());
  let choices = choices
    .with_role_profile_capability(runtime.clone(), cap)
    .unwrap()
    .with_runtime_settings_support(
      runtime.clone(),
      FactoryRuntimeSettingsSupport {
        authenticated_mcp: Default::default(),
        mcp_transports: [FactoryMcpTransport::Http].into(),
        mcp_executables: Default::default(),
      },
    )
    .unwrap();
  store.enable_project(project, choices).unwrap();
  (
    store,
    FactoryRuntimeSettingsIntent {
      project_id: project,
      runtime,
      expected_version: None,
      definition: FactoryRuntimeSettingsDefinition {
        toolchain: definition.toolchain,
        mcp_servers: [(
          key("review"),
          FactoryRuntimeMcpEntry {
            authorization: None,
            server: FactoryMcpServer::Http {
              url: "https://mcp.example/tools".into(),
              bearer_token_env_var: None,
              enabled_tools: Some(vec!["read".into()]),
            },
            integration: None,
          },
        )]
        .into(),
        integrations: Default::default(),
        permissions,
      },
    },
  )
}
#[test]
fn authenticated_runtime_settings_require_exact_installed_peer_secret_and_resource_approval() {
  run_ready(async {
    let (store, mut intent) = fixture();
    let FactoryConfigurationAvailability::Available(installed) = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap()
    else {
      panic!("installed Runtime");
    };
    let secret = octacity_server_secrets::LogicalSecretReference::new(
      octacity_server_secrets::SecretProviderId::new("agent-private").unwrap(),
      "project/issue-read",
    )
    .unwrap();
    let resource = ImmutableReference::new(key("repository"), key("v1"), FactoryDigest::from_bytes([7; 32]));
    let authorization = FactoryMcpAuthorization {
      credential_profile: key("mcp-read"),
      secret: secret.clone(),
      environment: "ISSUE_TOKEN".into(),
      purpose: FactoryMcpCredentialPurpose::McpRead,
      resource,
    };
    let entry = intent.definition.mcp_servers.get_mut(&key("review")).unwrap();
    entry.server = FactoryMcpServer::Http {
      url: "https://mcp.example/read".into(),
      bearer_token_env_var: Some("ISSUE_TOKEN".into()),
      enabled_tools: Some(vec!["read".into()]),
    };
    entry.authorization = Some(authorization.clone());
    let approved = FactoryMcpConfiguration::new(key("review"), key("v1"), entry.server.clone(), None)
      .unwrap()
      .with_authorization(authorization)
      .unwrap();
    let mut cap = installed.role_profile_capabilities()[&intent.runtime].clone();
    cap.ceiling.execution.environment.insert(
      "ISSUE_TOKEN".into(),
      FactoryRoleEnvironmentValue::Secret { reference: secret },
    );
    let mut grants = serde_json::to_value(&cap.ceiling.permissions).unwrap();
    grants["secret_profiles"] = serde_json::json!(["mcp-read"]);
    cap.ceiling.permissions = serde_json::from_value(grants).unwrap();
    cap.local_permissions = LocalPermissionCeiling::fully_enforced(cap.ceiling.permissions.clone());
    intent.definition.permissions = cap.ceiling.permissions.clone();
    intent
      .settings()
      .expect("authenticated Runtime intent must be structurally valid");
    let mut entries = installed.entries();
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::SecretProfile,
      alias: key("mcp-read"),
      reference: ImmutableReference::new(key("mcp-read"), key("v1"), FactoryDigest::from_bytes([8; 32])),
    });
    let installed = FactoryConfigurationChoices::try_new(entries)
      .unwrap()
      .with_role_profile_capability(intent.runtime.clone(), cap)
      .unwrap()
      .with_runtime_settings_support(
        intent.runtime.clone(),
        FactoryRuntimeSettingsSupport {
          mcp_transports: [FactoryMcpTransport::Http].into(),
          ..Default::default()
        },
      )
      .unwrap();
    store.enable_project(intent.project_id, installed.clone()).unwrap();
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    );
    let command = |intent: FactoryRuntimeSettingsIntent, key_name: &str| PublishFactoryRuntimeSettingsCommand {
      intent,
      idempotency_key: IdempotencyKey::new(key_name).unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    };
    assert!(
      commands
        .handle_command(&context(), command(intent.clone(), "unqualified-auth"))
        .await
        .is_err(),
      "a secret name alone must not approve a peer/resource credential destination"
    );
    let support = serde_json::from_value(
      serde_json::json!({"mcp_transports":["http"],"mcp_executables":{},"authenticated_mcp":{"review":approved}}),
    )
    .unwrap();
    store
      .enable_project(
        intent.project_id,
        installed
          .with_runtime_settings_support(intent.runtime.clone(), support)
          .unwrap(),
      )
      .unwrap();
    let saved = commands
      .handle_command(&context(), command(intent.clone(), "qualified-auth"))
      .await
      .unwrap();
    assert_eq!(
      saved
        .settings
        .mcp_configuration(&key("review"))
        .unwrap()
        .unwrap()
        .authorization(),
      intent.definition.mcp_servers[&key("review")].authorization.as_ref()
    );
    let queries = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    );
    let FactoryConfigurationAvailability::Available(current) = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap()
    else {
      unreachable!()
    };
    let withdrawn = current
      .with_runtime_settings_support(
        intent.runtime.clone(),
        FactoryRuntimeSettingsSupport {
          mcp_transports: [FactoryMcpTransport::Http].into(),
          ..Default::default()
        },
      )
      .unwrap();
    store.enable_project(intent.project_id, withdrawn).unwrap();
    let details = queries
      .handle_query(
        &context(),
        GetFactoryRoleProfileCapabilityQuery {
          project_id: intent.project_id,
          capability: intent.runtime.clone(),
        },
      )
      .await
      .unwrap();
    assert!(
      !details.definition.execution.mcp_servers.contains_key(&key("review")),
      "withdrawing peer authority must remove it from new profile choices without erasing history"
    );
    assert!(
      store
        .factory_runtime_mcp_configuration(
          intent.project_id,
          saved
            .settings
            .mcp_configuration(&key("review"))
            .unwrap()
            .unwrap()
            .reference()
        )
        .await
        .unwrap()
        .is_some()
    );

    let FactoryMcpServer::Http { url, .. } = &mut intent.definition.mcp_servers.get_mut(&key("review")).unwrap().server
    else {
      unreachable!()
    };
    *url = "https://mcp.example/another-peer".into();
    intent.expected_version = Some(saved.settings.version());
    assert!(
      commands
        .handle_command(&context(), command(intent, "redirect-auth"))
        .await
        .is_err(),
      "saving settings must not redirect a credential even within an allowed host"
    );
  });
}

#[test]
fn runtime_settings_publish_and_read_through_project_authorization_without_changing_installation() {
  run_ready(async {
    let (store, intent) = fixture();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let before = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap();
    let command = PublishFactoryRuntimeSettingsCommand {
      intent: intent.clone(),
      idempotency_key: IdempotencyKey::new("configure-runtime").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    };
    let saved = commands.handle_command(&context(), command).await.unwrap();
    assert_eq!(saved.disposition, MutationDisposition::Applied);
    assert_eq!(saved.settings.version(), FactoryRuntimeSettingsVersion::INITIAL);
    assert_eq!(saved.settings.definition(), &intent.definition);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetFactoryRuntimeSettingsQuery {
            project_id: intent.project_id,
            runtime: intent.runtime.clone(),
            version: None
          }
        )
        .await
        .unwrap(),
      Some(saved.settings.clone())
    );
    let transport = saved.settings.mcp_configuration(&key("review")).unwrap().unwrap();
    assert_eq!(
      serde_json::to_value(&transport).unwrap()["server"]["url"],
      "https://mcp.example/tools"
    );
    assert_eq!(
      store
        .factory_configuration_installation(intent.project_id)
        .await
        .unwrap(),
      before
    );
    assert_eq!(store.mutation_evidence_counts().await.audit, 1);
  });
}

#[test]
fn runtime_settings_reject_uninstalled_authority_and_unsafe_mcp_without_publication() {
  run_ready(async {
    let (store, intent) = fixture();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let before = store.mutation_evidence_counts().await;
    let mut requests = Vec::new();
    let mut wider = intent.clone();
    wider.definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      network_hosts: vec![NetworkHost::new("outside.example").unwrap()],
      ..Default::default()
    })
    .unwrap();
    requests.push(wider);
    for url in [
      "https://outside.example/tools",
      "https://user:credential@mcp.example/tools",
    ] {
      let mut unsafe_url = intent.clone();
      unsafe_url
        .definition
        .mcp_servers
        .get_mut(&key("review"))
        .unwrap()
        .server = FactoryMcpServer::Http {
        url: url.into(),
        bearer_token_env_var: None,
        enabled_tools: None,
      };
      requests.push(unsafe_url);
    }
    let mut unsupported = intent.clone();
    unsupported
      .definition
      .mcp_servers
      .get_mut(&key("review"))
      .unwrap()
      .server = FactoryMcpServer::Stdio {
      command: "/usr/bin/true".into(),
      args: vec![],
      env_vars: vec![],
      enabled_tools: None,
    };
    requests.push(unsupported);
    let mut unmapped_auth = intent.clone();
    if let FactoryMcpServer::Http {
      bearer_token_env_var, ..
    } = &mut unmapped_auth
      .definition
      .mcp_servers
      .get_mut(&key("review"))
      .unwrap()
      .server
    {
      *bearer_token_env_var = Some("UNDECLARED_SECRET".into());
    }
    requests.push(unmapped_auth);
    let mut ungranted = intent.clone();
    let integration = octacity_server_store::testing::role_profile_contract_definition(intent.project_id)
      .execution
      .integrations
      .remove(&key("tracker"))
      .unwrap();
    ungranted.definition.integrations.insert(key("tracker"), integration);
    ungranted
      .definition
      .mcp_servers
      .get_mut(&key("review"))
      .unwrap()
      .integration = Some(key("tracker"));
    requests.push(ungranted);
    let mut too_many = intent.clone();
    let entry = too_many.definition.mcp_servers[&key("review")].clone();
    too_many.definition.mcp_servers = (0..=MAX_FACTORY_ROLE_PROFILE_ENTRIES)
      .map(|index| (key(&format!("server-{index}")), entry.clone()))
      .collect();
    requests.push(too_many);
    let mut too_large = intent.clone();
    let mut large_entry = entry;
    if let FactoryMcpServer::Http { url, .. } = &mut large_entry.server {
      *url = format!("https://mcp.example/{}", "x".repeat(60 * 1024));
    }
    too_large.definition.mcp_servers = (0..5)
      .map(|index| (key(&format!("server-{index}")), large_entry.clone()))
      .collect();
    requests.push(too_large);
    for (index, request) in requests.into_iter().enumerate() {
      let result = commands
        .handle_command(
          &context(),
          PublishFactoryRuntimeSettingsCommand {
            intent: request,
            idempotency_key: IdempotencyKey::new(format!("invalid-runtime-{index}")).unwrap(),
            published_at: Timestamp::from_unix_millis(1).unwrap(),
          },
        )
        .await;
      assert!(result.is_err(), "invalid Runtime request {index} was published");
    }
    assert!(
      store
        .factory_runtime_settings(intent.project_id, &intent.runtime, None)
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn runtime_settings_replay_precedes_mutable_inventory_and_requires_current_project_authority() {
  run_ready(async {
    let (store, intent) = fixture();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let original = PublishFactoryRuntimeSettingsCommand {
      intent: intent.clone(),
      idempotency_key: IdempotencyKey::new("original-settings").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    };
    let first = commands.handle_command(&context(), original.clone()).await.unwrap();
    let mut replacement = original.clone();
    replacement.intent.expected_version = Some(first.settings.version());
    replacement.intent.definition.mcp_servers.clear();
    replacement.idempotency_key = IdempotencyKey::new("replace-settings").unwrap();
    commands.handle_command(&context(), replacement).await.unwrap();
    store.disable_project(intent.project_id).unwrap();
    let before = store.mutation_evidence_counts().await;
    let mut retry = original.clone();
    retry.published_at = Timestamp::from_unix_millis(999).unwrap();
    let replay = commands.handle_command(&context(), retry).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.settings, first.settings);
    assert_eq!(replay.published_at, Timestamp::from_unix_millis(1).unwrap());
    let mut conflicting = original.clone();
    conflicting.intent.definition.mcp_servers.clear();
    assert!(commands.handle_command(&context(), conflicting).await.is_err());
    let denied = AuthorizedCommandHandler::new(Arc::new(NoRuntimeAccess), handlers.clone());
    assert!(denied.handle_command(&context(), original).await.is_err());
    let reads = AuthorizedQueryHandler::new(Arc::new(NoRuntimeAccess), handlers);
    assert!(
      reads
        .handle_query(
          &context(),
          GetFactoryRuntimeSettingsQuery {
            project_id: intent.project_id,
            runtime: intent.runtime,
            version: Some(first.settings.version())
          }
        )
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
struct NoRuntimeAccess;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for NoRuntimeAccess {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::none()))
  }
}

#[test]
fn runtime_settings_memory_adapter_matches_the_reusable_journal_contract() {
  run_ready(async {
    let (store, intent) = fixture();
    octacity_server_store::testing::verify_factory_runtime_settings_publication(store.as_ref(), store.as_ref(), intent)
      .await;
  });
}

#[test]
fn runtime_settings_mcp_names_match_the_existing_execution_contract() {
  run_ready(async {
    for name in ["review.invalid".to_owned(), "a".repeat(65)] {
      let (store, mut intent) = fixture();
      let entry = intent.definition.mcp_servers.remove(&key("review")).unwrap();
      intent.definition.mcp_servers.insert(key(&name), entry);
      let commands = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
      );
      let result = commands
        .handle_command(
          &context(),
          PublishFactoryRuntimeSettingsCommand {
            intent,
            idempotency_key: IdempotencyKey::new("invalid-mcp-name").unwrap(),
            published_at: Timestamp::from_unix_millis(1).unwrap(),
          },
        )
        .await;
      assert!(
        result.is_err(),
        "Runtime settings accepted an MCP name rejected by execution"
      );
      assert_eq!(store.mutation_evidence_counts().await.audit, 0);
    }
  });
}

#[test]
fn runtime_settings_reuse_managed_integration_grants_and_exact_installed_stdio_commands() {
  use octacity_server_domain::{IntegrationId, IntegrationVersion};
  use octacity_server_store::{
    testing::{integration_connection_fixture, management_mutation},
    *,
  };
  run_ready(async {
    let (store, mut intent) = fixture();
    let time = Timestamp::from_unix_millis(1).unwrap();
    let (installed, draft) = integration_connection_fixture("independent-tracker");
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
        idempotency_key: IdempotencyKey::new("runtime-connection").unwrap(),
        published_at: time,
      }))
      .await
      .unwrap();
    store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: connection,
          project_id: intent.project_id,
          connection_version: IntegrationVersion::INITIAL,
          expected_current_version: None,
          resources: vec![IntegrationResourceSelection {
            reference: resource.clone(),
            purposes: vec![IntegrationPurpose::TaskRead],
          }],
        },
        idempotency_key: IdempotencyKey::new("runtime-access").unwrap(),
        published_at: time,
      }))
      .await
      .unwrap();
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
        idempotency_key: IdempotencyKey::new("runtime-activate").unwrap(),
        requested_at: time,
      }))
      .await
      .unwrap();
    store
      .complete_integration_administration(management_mutation(preparation))
      .await
      .unwrap();
    let binding = FactoryRoleIntegrationBinding {
      project_id: intent.project_id,
      connection_id: connection,
      connection_version: IntegrationVersion::INITIAL,
      access_version: IntegrationVersion::INITIAL,
      resource,
      capability: capability.capability,
    };
    intent.definition.integrations.insert(key("tracker"), binding.clone());
    intent
      .definition
      .mcp_servers
      .get_mut(&key("review"))
      .unwrap()
      .integration = Some(key("tracker"));
    let executable = intent.definition.toolchain.executable.clone();
    intent.definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      network_hosts: vec![octacity_server_factory::NetworkHost::new("mcp.example").unwrap()],
      executables: vec![executable.clone()],
      commands: vec![
        CommandPermission::try_new(
          executable.clone(),
          vec![CommandArgumentPattern::Exact(FactoryText::new("--read").unwrap())],
        )
        .unwrap(),
      ],
      ..Default::default()
    })
    .unwrap();
    intent.definition.mcp_servers.insert(
      key("local"),
      FactoryRuntimeMcpEntry {
        authorization: None,
        server: FactoryMcpServer::Stdio {
          command: "/runtime/mcp-reader".into(),
          args: vec!["--read".into()],
          env_vars: vec!["LANGUAGE".into()],
          enabled_tools: None,
        },
        integration: None,
      },
    );
    let FactoryConfigurationAvailability::Available(mut choices) = store
      .factory_configuration_availability(intent.project_id)
      .await
      .unwrap()
    else {
      panic!("installed choices")
    };
    let mut cap = choices.role_profile_capabilities()[&intent.runtime].clone();
    cap.ceiling.permissions = intent.definition.permissions.clone();
    cap.local_permissions = LocalPermissionCeiling::fully_enforced(intent.definition.permissions.clone());
    choices = choices
      .with_role_profile_capability(intent.runtime.clone(), cap)
      .unwrap()
      .with_runtime_settings_support(
        intent.runtime.clone(),
        FactoryRuntimeSettingsSupport {
          authenticated_mcp: Default::default(),
          mcp_transports: [FactoryMcpTransport::Http, FactoryMcpTransport::Stdio].into(),
          mcp_executables: [("/runtime/mcp-reader".into(), executable)].into(),
        },
      )
      .unwrap();
    store.enable_project(intent.project_id, choices).unwrap();
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    );
    let original = commands
      .handle_command(
        &context(),
        PublishFactoryRuntimeSettingsCommand {
          intent: intent.clone(),
          idempotency_key: IdempotencyKey::new("managed-runtime").unwrap(),
          published_at: time,
        },
      )
      .await
      .unwrap();
    assert_eq!(
      original
        .settings
        .mcp_configuration(&key("review"))
        .unwrap()
        .unwrap()
        .integration(),
      Some(&binding)
    );
    assert_eq!(
      serde_json::to_value(
        original
          .settings
          .mcp_configuration(&key("local"))
          .unwrap()
          .unwrap()
          .server()
      )
      .unwrap(),
      serde_json::json!({"transport":"stdio","command":"/runtime/mcp-reader","args":["--read"],"env_vars":["LANGUAGE"]})
    );
    intent.expected_version = Some(original.settings.version());
    if let FactoryMcpServer::Stdio { args, .. } =
      &mut intent.definition.mcp_servers.get_mut(&key("local")).unwrap().server
    {
      args[0] = "--unapproved".into();
    }
    let before = store.mutation_evidence_counts().await;
    assert!(
      commands
        .handle_command(
          &context(),
          PublishFactoryRuntimeSettingsCommand {
            intent,
            idempotency_key: IdempotencyKey::new("unapproved-command").unwrap(),
            published_at: time
          }
        )
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let queries = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    );
    let query = GetFactoryRoleProfileCapabilityQuery {
      project_id: binding.project_id,
      capability: key("dependency-1"),
    };
    let selectable = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(selectable.definition.execution.mcp_servers.len(), 2);
    store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: binding.connection_id,
          project_id: binding.project_id,
          connection_version: IntegrationVersion::INITIAL,
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![],
        },
        idempotency_key: "revoke-runtime-integration".parse().unwrap(),
        published_at: Timestamp::from_unix_millis(3).unwrap(),
      }))
      .await
      .unwrap();
    let available = queries.handle_query(&context(), query).await.unwrap();
    assert!(available.definition.execution.integrations.is_empty());
    assert!(!available.definition.execution.mcp_servers.contains_key(&key("review")));
    assert!(available.definition.execution.mcp_servers.contains_key(&key("local")));
    assert_eq!(
      store
        .factory_runtime_settings(
          binding.project_id,
          original.settings.runtime(),
          Some(original.settings.version())
        )
        .await
        .unwrap(),
      Some(original.settings)
    );
  });
}

#[test]
fn runtime_settings_http_hosts_use_canonical_network_permission_identities() {
  run_ready(async {
    for (host, endpoint) in [
      ("127.0.0.1", "https://127.0.0.1/mcp"),
      ("2001:db8::1", "https://[2001:db8::1]/mcp"),
    ] {
      let (store, mut intent) = fixture();
      intent.definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
        network_hosts: vec![NetworkHost::new(host).unwrap()],
        ..Default::default()
      })
      .unwrap();
      intent.definition.mcp_servers.get_mut(&key("review")).unwrap().server = FactoryMcpServer::Http {
        url: endpoint.into(),
        bearer_token_env_var: None,
        enabled_tools: None,
      };
      let FactoryConfigurationAvailability::Available(choices) = store
        .factory_configuration_availability(intent.project_id)
        .await
        .unwrap()
      else {
        panic!("installed choices")
      };
      let mut cap = choices.role_profile_capabilities()[&intent.runtime].clone();
      cap.ceiling.permissions = intent.definition.permissions.clone();
      cap.local_permissions = LocalPermissionCeiling::fully_enforced(intent.definition.permissions.clone());
      store
        .enable_project(
          intent.project_id,
          choices
            .with_role_profile_capability(intent.runtime.clone(), cap)
            .unwrap(),
        )
        .unwrap();
      let commands = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(FactoryRoleProfileHandlers::new(store)),
      );
      assert!(
        commands
          .handle_command(
            &context(),
            PublishFactoryRuntimeSettingsCommand {
              intent,
              idempotency_key: IdempotencyKey::new("canonical-host").unwrap(),
              published_at: Timestamp::from_unix_millis(1).unwrap()
            }
          )
          .await
          .is_ok(),
        "explicitly allowed canonical host {host} was rejected"
      );
    }
  });
}

#[test]
fn runtime_integration_replacement_does_not_rebind_installed_mcp() {
  run_ready(async {
    let (store, mut intent) = fixture();
    let FactoryConfigurationAvailability::Available(installed) = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap()
    else {
      panic!("installed Runtime");
    };
    let mut cap = installed.role_profile_capabilities()[&intent.runtime].clone();
    let original = octacity_server_store::testing::role_profile_contract_definition(intent.project_id)
      .execution
      .integrations
      .remove(&key("tracker"))
      .unwrap();
    cap
      .ceiling
      .execution
      .integrations
      .insert(key("tracker"), original.clone());
    for name in ["bound-reader", "independent-reader"] {
      cap.ceiling.execution.mcp_servers.insert(
        key(name),
        FactoryRoleMcpBinding {
          configuration: intent.definition.toolchain.plugin.clone(),
          integration: (name == "bound-reader").then(|| key("tracker")),
        },
      );
    }
    let mut replacement = original.clone();
    replacement.access_version = octacity_server_domain::IntegrationVersion::new(2).unwrap();
    intent
      .definition
      .integrations
      .insert(key("tracker"), replacement.clone());
    intent
      .definition
      .mcp_servers
      .get_mut(&key("review"))
      .unwrap()
      .integration = Some(key("tracker"));
    let installed = installed
      .with_role_profile_capability(intent.runtime.clone(), cap)
      .unwrap();
    let settings = intent.settings().unwrap();
    let choices = installed.clone().with_runtime_settings(&settings).unwrap();
    let execution = &choices.role_profile_capabilities()[&intent.runtime].ceiling.execution;
    assert_eq!(execution.integrations[&key("tracker")], replacement);
    assert!(
      !execution.mcp_servers.contains_key(&key("bound-reader")),
      "an installed transport cannot silently adopt a different exact integration"
    );
    assert!(execution.mcp_servers.contains_key(&key("independent-reader")));
    assert_eq!(
      execution.mcp_servers[&key("review")].configuration,
      *settings.mcp_configuration(&key("review")).unwrap().unwrap().reference()
    );
    // The retained installation still carries its original dependency.
    assert_eq!(
      installed.role_profile_capabilities()[&intent.runtime]
        .ceiling
        .execution
        .integrations[&key("tracker")],
      original
    );
  });
}

#[test]
fn configured_runtime_choices_are_shared_by_profile_details_and_validation() {
  run_ready(async {
    let (store, mut intent) = fixture();
    let FactoryConfigurationAvailability::Available(installed) = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap()
    else {
      panic!("installed Runtime");
    };
    let executable = intent.definition.toolchain.executable.clone();
    let mut cap = installed.role_profile_capabilities()[&intent.runtime].clone();
    let secret = octacity_server_secrets::LogicalSecretReference::new(
      octacity_server_secrets::SecretProviderId::new("agent-private").unwrap(),
      "project/mcp-read",
    )
    .unwrap();
    let authorization = FactoryMcpAuthorization {
      credential_profile: key("mcp-read"),
      secret: secret.clone(),
      environment: "MCP_AUTH".into(),
      purpose: FactoryMcpCredentialPurpose::McpRead,
      resource: ImmutableReference::new(key("repository"), key("v1"), FactoryDigest::from_bytes([7; 32])),
    };
    let authenticated = FactoryMcpServer::Http {
      url: "https://mcp.example/authenticated".into(),
      bearer_token_env_var: Some("MCP_AUTH".into()),
      enabled_tools: None,
    };
    let approved = FactoryMcpConfiguration::new(key("authenticated"), key("v1"), authenticated.clone(), None)
      .unwrap()
      .with_authorization(authorization.clone())
      .unwrap();
    cap.ceiling.execution.environment.insert(
      "MCP_AUTH".into(),
      FactoryRoleEnvironmentValue::Secret { reference: secret },
    );
    let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      secret_profiles: vec![key("mcp-read")],
      network_hosts: vec![NetworkHost::new("mcp.example").unwrap()],
      executables: vec![executable.clone()],
      commands: vec![CommandPermission::try_new(executable.clone(), vec![]).unwrap()],
      ..Default::default()
    })
    .unwrap();
    cap.ceiling.permissions = permissions.clone();
    cap.local_permissions = LocalPermissionCeiling::fully_enforced(permissions.clone());
    let mut entries = installed.entries();
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::SecretProfile,
      alias: key("mcp-read"),
      reference: ImmutableReference::new(key("mcp-read"), key("v1"), FactoryDigest::from_bytes([8; 32])),
    });
    let installed = FactoryConfigurationChoices::try_new(entries).unwrap();
    store
      .enable_project(
        intent.project_id,
        installed
          .with_role_profile_capability(intent.runtime.clone(), cap)
          .unwrap()
          .with_runtime_settings_support(
            intent.runtime.clone(),
            FactoryRuntimeSettingsSupport {
              authenticated_mcp: [(key("authenticated"), approved)].into(),
              mcp_transports: [FactoryMcpTransport::Http, FactoryMcpTransport::Stdio].into(),
              mcp_executables: [("/runtime/mcp-reader".into(), executable)].into(),
            },
          )
          .unwrap(),
      )
      .unwrap();
    intent.definition.permissions = permissions;
    intent.definition.mcp_servers.insert(
      key("local"),
      FactoryRuntimeMcpEntry {
        authorization: None,
        server: FactoryMcpServer::Stdio {
          command: "/runtime/mcp-reader".into(),
          args: vec![],
          env_vars: vec!["LANGUAGE".into()],
          enabled_tools: None,
        },
        integration: None,
      },
    );
    intent.definition.mcp_servers.insert(
      key("authenticated"),
      FactoryRuntimeMcpEntry {
        authorization: Some(authorization),
        server: authenticated,
        integration: None,
      },
    );
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let saved = commands
      .handle_command(
        &context(),
        PublishFactoryRuntimeSettingsCommand {
          intent: intent.clone(),
          idempotency_key: IdempotencyKey::new("profile-runtime-choices").unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    let details = queries
      .handle_query(
        &context(),
        GetFactoryRoleProfileCapabilityQuery {
          project_id: intent.project_id,
          capability: intent.runtime.clone(),
        },
      )
      .await
      .unwrap();
    let selected = details.definition.execution.mcp_servers.get(&key("review"));
    assert!(
      selected.is_some(),
      "published Runtime MCP must be selectable in the ordinary profile catalog"
    );
    assert_eq!(
      selected.unwrap().configuration,
      *saved
        .settings
        .mcp_configuration(&key("review"))
        .unwrap()
        .unwrap()
        .reference()
    );
    assert_eq!(details.definition.permissions, intent.definition.permissions);
    let page = queries
      .handle_query(
        &context(),
        ListFactoryRoleProfileCapabilitiesQuery {
          page: CurrentDefinitionPageInput::try_new(intent.project_id, None, 10).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].mcp_servers, details.definition.execution.mcp_servers);
    let mut role = octacity_server_store::testing::role_profile_contract_intent(intent.project_id);
    role.profiles.clear();
    let published = commands
      .handle_command(
        &context(),
        PublishFactoryRoleProfilesCommand {
          intent: role,
          idempotency_key: "configured-profile-role".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      )
      .await
      .unwrap();
    let candidate = ValidateFactoryRoleProfileQuery {
      project_id: intent.project_id,
      id: FactoryRoleProfileId::generate(),
      role: published.role.unwrap().reference().clone(),
      capability: intent.runtime,
      definition: details.definition.clone(),
    };
    let validation = queries.handle_query(&context(), candidate.clone()).await.unwrap();
    assert_eq!(validation.incompatibility, None);
    assert_eq!(validation.profile.unwrap().definition(), &details.definition);
    for required in ["LANGUAGE", "MCP_AUTH"] {
      let mut incomplete = candidate.clone();
      incomplete.definition.execution.environment.remove(required);
      let rejected = queries.handle_query(&context(), incomplete.clone()).await.unwrap();
      assert_eq!(
        rejected.incompatibility,
        Some(FactoryRoleProfileIncompatibility::Environment),
        "selected MCP requires explicit mapping of {required}"
      );
      assert!(rejected.profile.is_none());
      // An unselected transport must not require or inherit its environment mapping.
      incomplete
        .definition
        .execution
        .mcp_servers
        .remove(&key(if required == "LANGUAGE" {
          "local"
        } else {
          "authenticated"
        }));
      assert_eq!(
        queries
          .handle_query(&context(), incomplete)
          .await
          .unwrap()
          .incompatibility,
        None
      );
    }
    let foreign = ProjectId::generate();
    store.seed_project(foreign).unwrap();
    assert!(
      store
        .factory_runtime_mcp_configuration(foreign, &selected.unwrap().configuration)
        .await
        .unwrap()
        .is_none()
    );
  });
}

#[test]
fn runtime_administration_can_restore_choices_within_the_independent_installed_ceiling() {
  run_ready(async {
    let (store, intent) = fixture();
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let mut narrowed = intent.clone();
    narrowed.definition.mcp_servers.clear();
    narrowed.definition.permissions = FactoryPermissionSet::deny_all();
    let first = commands
      .handle_command(
        &context(),
        PublishFactoryRuntimeSettingsCommand {
          intent: narrowed,
          idempotency_key: IdempotencyKey::new("narrow-runtime").unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    let mut restored = intent;
    restored.expected_version = Some(first.settings.version());
    let second = commands
      .handle_command(
        &context(),
        PublishFactoryRuntimeSettingsCommand {
          intent: restored.clone(),
          idempotency_key: IdempotencyKey::new("restore-runtime").unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      )
      .await
      .expect("Project settings must not overwrite the independent installed administration ceiling");
    assert_eq!(second.settings.definition(), &restored.definition);
    assert_eq!(second.settings.version().get(), 2);
  });
}

#[test]
fn configured_mcp_is_not_selectable_after_installation_withdraws_transport_support() {
  run_ready(async {
    let (store, intent) = fixture();
    let FactoryConfigurationAvailability::Available(installed) = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap()
    else {
      panic!("installed Runtime");
    };
    let handlers = Arc::new(FactoryRoleProfileHandlers::new(store.clone()));
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone())
      .handle_command(
        &context(),
        PublishFactoryRuntimeSettingsCommand {
          intent: intent.clone(),
          idempotency_key: "supported-mcp".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(1).unwrap(),
        },
      )
      .await
      .unwrap();
    store
      .enable_project(
        intent.project_id,
        installed
          .with_runtime_settings_support(intent.runtime.clone(), FactoryRuntimeSettingsSupport::default())
          .unwrap(),
      )
      .unwrap();
    let details = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers)
      .handle_query(
        &context(),
        GetFactoryRoleProfileCapabilityQuery {
          project_id: intent.project_id,
          capability: intent.runtime,
        },
      )
      .await
      .unwrap();
    assert!(
      details.definition.execution.mcp_servers.is_empty(),
      "retained authoring content cannot override withdrawn installation support"
    );
  });
}

#[test]
fn runtime_publication_rejects_choices_that_would_overflow_the_shared_catalog() {
  run_ready(async {
    let (store, intent) = fixture();
    let FactoryConfigurationAvailability::Available(original) = store
      .factory_configuration_installation(intent.project_id)
      .await
      .unwrap()
    else {
      panic!("installed Runtime");
    };
    let mut entries = original.entries();
    while entries.references.len() < MAX_CONFIGURATION_CHOICES_PER_KIND {
      let name = key(&format!("installed-{}", entries.references.len()));
      entries.references.push(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: name.clone(),
        reference: ImmutableReference::new(name, key("v1"), FactoryDigest::from_bytes([44; 32])),
      });
    }
    let installed = FactoryConfigurationChoices::try_new(entries)
      .unwrap()
      .with_role_profile_capability(
        intent.runtime.clone(),
        original.role_profile_capabilities()[&intent.runtime].clone(),
      )
      .unwrap()
      .with_runtime_settings_support(
        intent.runtime.clone(),
        original.runtime_settings_support(&intent.runtime).unwrap().clone(),
      )
      .unwrap();
    store.enable_project(intent.project_id, installed).unwrap();
    let before = store.mutation_evidence_counts().await;
    let result = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryRoleProfileHandlers::new(store.clone())),
    )
    .handle_command(
      &context(),
      PublishFactoryRuntimeSettingsCommand {
        intent: intent.clone(),
        idempotency_key: "catalog-overflow".parse().unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      },
    )
    .await;
    assert!(
      result.is_err(),
      "a valid installed catalog must not become unreadable after settings publication"
    );
    assert!(
      store
        .factory_runtime_settings(intent.project_id, &intent.runtime, None)
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let direct = store
      .publish_factory_runtime_settings(octacity_server_store::testing::management_mutation(
        octacity_server_store::PublishFactoryRuntimeSettings {
          intent,
          idempotency_key: "atomic-catalog-bound".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      ))
      .await;
    assert!(
      direct.is_err(),
      "the bound must be enforced by the atomic publication owner, including competing Runtime saves"
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
