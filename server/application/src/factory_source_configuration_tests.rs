//! Management of automatic intake remains separate from manual Work and future Issues.
use super::*;
use crate::*;
use octacity_server_domain::IntegrationVersion;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};

struct Fixture {
  store: Arc<InMemoryFactoryConfigurationStore>,
  intent: WorkSourceConfigurationIntent,
  capability: WorkSourceCapability,
  root: FlowDefinitionRef,
  reporter: Option<WorkSourceCapability>,
}
impl Fixture {
  async fn new(provider: &str) -> Self {
    Self::with_writer(provider, false).await
  }
  async fn with_writer(provider: &str, writer: bool) -> Self {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(project, configuration, repository(project, repository_id, None));
    let (mut installed, mut draft) = integration_connection_fixture(provider);
    let reporter = writer.then(|| {
      let mut capability = installed.resources[0].capabilities[0].clone();
      capability.purpose = IntegrationPurpose::TaskUpdate;
      capability.capability = exact("writer", 45);
      capability.configuration = exact("writer-settings", 46);
      capability.credential =
        serde_json::from_value(serde_json::json!({"provider":"installed","reference":"writer"})).unwrap();
      installed.resources[0].capabilities.push(capability.clone());
      draft.resources[0].purposes.push(IntegrationPurpose::TaskUpdate);
      WorkSourceCapability {
        project_id: project,
        role: WorkSourceCapabilityRole::Reporter,
        binding: WorkSourceBinding {
          installation: installed.installation.clone(),
          resource: installed.resources[0].reference.clone(),
          capability: capability.capability,
          configuration: capability.configuration,
        },
      }
    });
    let connection = IntegrationId::generate();
    let resource = installed.resources[0].clone();
    let source = &resource.capabilities[0];
    let capability = WorkSourceCapability {
      project_id: project,
      role: WorkSourceCapabilityRole::Source,
      binding: WorkSourceBinding {
        installation: installed.installation.clone(),
        capability: source.capability.clone(),
        configuration: source.configuration.clone(),
        resource: resource.reference.clone(),
      },
    };
    let resources = draft.resources.clone();
    store
      .publish_integration_connection(management_mutation(PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id: connection,
          expected_current_version: None,
          draft,
        },
        installed,
        idempotency_key: IdempotencyKey::new("configuration-connection").unwrap(),
        published_at: time(1),
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
            reference: source.credential.clone(),
            kind: octacity_server_secrets::CredentialPublicationKind::Adopt,
            activate_connection: !writer,
          },
        },
        idempotency_key: IdempotencyKey::new("configuration-activate").unwrap(),
        requested_at: time(2),
      }))
      .await
      .unwrap();
    store
      .complete_integration_administration(management_mutation(preparation))
      .await
      .unwrap();
    if writer {
      let current = store.integration_administration(connection).await.unwrap();
      let receipt = store
        .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
          intent: IntegrationAdministrationIntent {
            id: connection,
            expected_metadata_version: IntegrationVersion::INITIAL,
            expected_revision: current.revision,
            action: IntegrationAdministrationAction::Credential {
              reference: serde_json::from_value(serde_json::json!({"provider":"installed","reference":"writer"}))
                .unwrap(),
              kind: octacity_server_secrets::CredentialPublicationKind::Adopt,
              activate_connection: true,
            },
          },
          idempotency_key: IdempotencyKey::new("writer-adopt").unwrap(),
          requested_at: time(2),
        }))
        .await
        .unwrap();
      store
        .complete_integration_administration(management_mutation(receipt))
        .await
        .unwrap();
    }
    store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: connection,
          connection_version: IntegrationVersion::INITIAL,
          project_id: project,
          expected_current_version: None,
          resources,
        },
        idempotency_key: IdempotencyKey::new("configuration-grant").unwrap(),
        published_at: time(3),
      }))
      .await
      .unwrap();
    let selected = store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: configuration,
        configuration_version: FactoryConfigurationVersion::INITIAL,
        repository_id,
        repository_version: RepositoryVersion::INITIAL,
      })
      .await
      .unwrap();
    let intent = WorkSourceConfigurationIntent {
      project_id: project,
      inputs: vec![WorkSourceInputIntent {
        id: WorkSourceInputId::generate(),
        expected_version: None,
        definition: WorkSourceInputDraft {
          reporting: None,
          predicate: None,
          target: selected.configuration.configuration.reference().clone(),
          repository: WorkSourceRepositoryRef {
            project_id: project,
            id: repository_id,
            version: RepositoryVersion::INITIAL,
            definition_digest: FactoryDigest::content_sha256(
              &serde_json::to_vec(&selected.repository.definition).unwrap(),
            ),
          },
          source: capability.binding.clone(),
          reporter: None,
          enabled: true,
          connection: Some(WorkSourceConnectionRef {
            id: connection,
            version: IntegrationVersion::INITIAL,
            access_version: IntegrationVersion::INITIAL,
            source_configuration: capability.binding.configuration.clone(),
            reporter_configuration: None,
          }),
        },
      }],
      group: None,
    };
    Self {
      store,
      intent,
      capability,
      root: selected.configuration.configuration.flow().closure.root(),
      reporter,
    }
  }
  fn handlers(&self) -> FactorySourceConfigurationHandlers<InMemoryFactoryConfigurationStore> {
    FactorySourceConfigurationHandlers::new(
      self.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      std::iter::once(self.capability.clone())
        .chain(self.reporter.clone())
        .collect(),
    )
    .unwrap()
  }
}

#[test]
fn source_configuration_validates_one_connection_for_distinct_inputs_and_independent_code_repository() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let mut fixture = Fixture::new(provider).await;
      let mut second = fixture.intent.inputs[0].clone();
      second.id = WorkSourceInputId::generate();
      fixture.intent.inputs.push(second);
      let handler = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()));
      let validated = handler
        .handle_query(
          &context(),
          ValidateWorkSourceConfigurationQuery {
            intent: fixture.intent.clone(),
          },
        )
        .await
        .unwrap();
      assert_eq!(validated.inputs.len(), 2);
      assert_ne!(validated.inputs[0].input.id(), validated.inputs[1].input.id());
      assert_eq!(
        validated.inputs[0].input.definition(),
        &fixture.intent.inputs[0].definition
      );
      assert_eq!(validated.inputs[0].input.root_flow(), fixture.root);
      assert!(
        fixture
          .store
          .source_input(
            fixture.intent.project_id,
            fixture.intent.inputs[0].id,
            WorkSourceInputVersion::INITIAL
          )
          .await
          .is_err(),
        "validation cannot activate intake"
      );
    }
  });
}

#[test]
fn source_configuration_validation_freezes_exact_exclusive_group_members() {
  run_ready(async {
    let mut fixture = Fixture::new("jira").await;
    let mut second = fixture.intent.inputs[0].clone();
    second.id = WorkSourceInputId::generate();
    fixture.intent.inputs.push(second);
    let id = WorkSourceGroupId::generate();
    fixture.intent.group = Some(WorkSourceGroupIntent {
      id,
      expected_version: None,
      members: fixture
        .intent
        .inputs
        .iter()
        .map(|i| WorkSourceMemberRef {
          id: i.id,
          version: WorkSourceInputVersion::INITIAL,
        })
        .collect(),
    });
    let result = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
      .handle_query(
        &context(),
        ValidateWorkSourceConfigurationQuery {
          intent: fixture.intent.clone(),
        },
      )
      .await
      .unwrap();
    let group = result
      .group
      .expect("validation must retain the selected exclusive group")
      .group;
    assert_eq!(group.id(), id);
    assert_eq!(
      group.members().iter().map(WorkSourceInput::id).collect::<Vec<_>>(),
      fixture.intent.inputs.iter().map(|i| i.id).collect::<Vec<_>>()
    );
    assert!(
      fixture
        .store
        .source_group(fixture.intent.project_id, id, WorkSourceGroupVersion::INITIAL)
        .await
        .is_err()
    );
  });
}

#[test]
fn source_configuration_publication_replays_original_versions_after_replacement_and_rejects_key_reuse() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let handlers = Arc::new(fixture.handlers());
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let command = PublishWorkSourceConfigurationCommand {
      intent: fixture.intent.clone(),
      idempotency_key: IdempotencyKey::new("publish-input").unwrap(),
      published_at: time(4),
    };
    let published = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(
      published.disposition,
      octacity_server_store::MutationDisposition::Applied
    );
    let first = published.publication.inputs[0].input.clone();
    assert_eq!(
      fixture
        .store
        .source_input(fixture.intent.project_id, first.id(), first.version())
        .await
        .unwrap(),
      first
    );
    let mut replacement = command.clone();
    replacement.idempotency_key = IdempotencyKey::new("disable-input").unwrap();
    replacement.intent.inputs[0].expected_version = Some(first.version());
    replacement.intent.inputs[0].definition.enabled = false;
    let newer = commands.handle_command(&context(), replacement.clone()).await.unwrap();
    assert_eq!(newer.publication.inputs[0].input.version().get(), 2);
    let mut retry = command.clone();
    retry.published_at = time(99);
    let replayed = commands.handle_command(&context(), retry).await.unwrap();
    assert_eq!(
      replayed.disposition,
      octacity_server_store::MutationDisposition::Replayed
    );
    assert_eq!(replayed.publication, published.publication);
    assert_eq!(replayed.published_at, time(4));
    replacement.idempotency_key = IdempotencyKey::new("stale-input").unwrap();
    assert!(commands.handle_command(&context(), replacement).await.is_err());
    let mut conflict = command;
    conflict.intent.inputs[0].definition.enabled = false;
    assert!(commands.handle_command(&context(), conflict).await.is_err());
    assert!(
      !fixture
        .store
        .source_input(
          fixture.intent.project_id,
          first.id(),
          WorkSourceInputVersion::new(2).unwrap()
        )
        .await
        .unwrap()
        .definition()
        .enabled
    );
  });
}

#[test]
fn source_configuration_publication_rechecks_revocation_after_validation_atomically() {
  run_ready(async {
    let fixture = Fixture::new("gitlab").await;
    let publication =
      AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
        .handle_query(
          &context(),
          ValidateWorkSourceConfigurationQuery {
            intent: fixture.intent.clone(),
          },
        )
        .await
        .unwrap();
    let connection = fixture.intent.inputs[0].definition.connection.as_ref().unwrap();
    fixture
      .store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: connection.id,
          project_id: fixture.intent.project_id,
          connection_version: connection.version,
          expected_current_version: Some(connection.access_version),
          resources: vec![],
        },
        idempotency_key: IdempotencyKey::new("revoke-before-publication").unwrap(),
        published_at: time(4),
      }))
      .await
      .unwrap();
    let before = fixture.store.mutation_evidence_counts().await;
    assert!(
      fixture
        .store
        .publish_work_source_configuration(management_mutation(PublishWorkSourceConfiguration {
          intent: fixture.intent.clone(),
          publication,
          idempotency_key: IdempotencyKey::new("obsolete-validation").unwrap(),
          published_at: time(5),
        }))
        .await
        .is_err(),
      "validation cannot carry authority past revocation into publication"
    );
    assert!(
      fixture
        .store
        .source_input(
          fixture.intent.project_id,
          fixture.intent.inputs[0].id,
          WorkSourceInputVersion::INITIAL
        )
        .await
        .is_err()
    );
    assert_eq!(fixture.store.mutation_evidence_counts().await, before);
  });
}

struct DenySourceResource(ManagementResourceKind, ManagementAction);
#[async_trait]
impl ManagementAuthorizationPolicy for DenySourceResource {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.0 && action == self.1 {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
#[test]
fn source_configuration_replay_still_requires_current_dependency_visibility_and_use() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let command = PublishWorkSourceConfigurationCommand {
      intent: fixture.intent.clone(),
      idempotency_key: IdempotencyKey::new("visible-replay").unwrap(),
      published_at: time(4),
    };
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
      .handle_command(&context(), command.clone())
      .await
      .unwrap();
    for (kind, action) in [
      (ManagementResourceKind::Project, ManagementAction::View),
      (ManagementResourceKind::IntegrationConnection, ManagementAction::View),
      (ManagementResourceKind::IntegrationConnection, ManagementAction::Execute),
      (ManagementResourceKind::FactoryConfiguration, ManagementAction::View),
      (ManagementResourceKind::Repository, ManagementAction::View),
    ] {
      let policy = Arc::new(DenySourceResource(kind, action));
      let handlers = FactorySourceConfigurationHandlers::new(
        fixture.store.clone(),
        policy.clone(),
        vec![fixture.capability.clone()],
      )
      .unwrap();
      let denied = AuthorizedCommandHandler::new(policy, Arc::new(handlers));
      assert!(
        denied.handle_command(&context(), command.clone()).await.is_err(),
        "historical replay must not reveal a now-hidden {kind:?} or restore use"
      );
    }
  });
}

#[test]
fn source_configuration_reuses_one_connection_with_distinct_installed_input_settings() {
  run_ready(async {
    let mut fixture = Fixture::new("jira").await;
    let mut second = fixture.capability.clone();
    second.binding.configuration = exact("other-selector-and-mapping", 44);
    let mut input = fixture.intent.inputs[0].clone();
    input.id = WorkSourceInputId::generate();
    input.definition.source = second.binding.clone();
    fixture.intent.inputs.push(input);
    let handlers = FactorySourceConfigurationHandlers::new(
      fixture.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      vec![fixture.capability.clone(), second.clone()],
    )
    .unwrap()
    .with_connection_configuration(second, fixture.capability.binding.configuration.clone())
    .unwrap();
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers));
    let result = commands
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: fixture.intent.clone(),
          idempotency_key: IdempotencyKey::new("separate-input-settings").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    assert_ne!(
      result.publication.inputs[0].input.definition().source.configuration,
      result.publication.inputs[1].input.definition().source.configuration
    );
    assert_eq!(
      result.publication.inputs[0].input.definition().connection,
      result.publication.inputs[1].input.definition().connection
    );
    assert_eq!(
      fixture
        .store
        .source_input(
          fixture.intent.project_id,
          fixture.intent.inputs[1].id,
          WorkSourceInputVersion::INITIAL
        )
        .await
        .unwrap()
        .definition(),
      &fixture.intent.inputs[1].definition
    );
  });
}

#[test]
fn source_configuration_choices_are_current_granted_and_show_safe_literal_label_settings() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let selection = WorkSourceSelectionLabels {
      state: FactoryKey::new("custom-native-state").unwrap(),
      all_of: vec!["ready-for-review".into()],
      any_of: vec!["documentation".into(), "maintenance".into()],
      none_of: vec!["on-hold".into()],
    };
    let mut unsupported_writer = fixture.capability.clone();
    unsupported_writer.role = WorkSourceCapabilityRole::Reporter;
    let handlers = FactorySourceConfigurationHandlers::new(
      fixture.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      vec![fixture.capability.clone(), unsupported_writer],
    )
    .unwrap()
    .with_selection_labels(fixture.capability.clone(), selection.clone())
    .unwrap();
    let query = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers));
    let connection = fixture.intent.inputs[0].definition.connection.as_ref().unwrap();
    let choices = query
      .handle_query(
        &context(),
        ListWorkSourceInputChoicesQuery {
          project_id: fixture.intent.project_id,
          connection_id: connection.id,
          connection_version: connection.version,
        },
      )
      .await
      .unwrap();
    assert_eq!(choices.access_version, IntegrationVersion::INITIAL);
    assert_eq!(
      choices.items.len(),
      1,
      "a read grant offers no writer even if a declaration names one"
    );
    assert_eq!(choices.items[0].purpose, IntegrationPurpose::TaskRead);
    assert_eq!(choices.items[0].binding, fixture.capability.binding);
    assert_eq!(choices.items[0].selection.as_ref(), Some(&selection));
    let serialized = serde_json::to_string(&choices).unwrap();
    assert!(!serialized.contains("credential"));
    assert!(!serialized.contains("private-reader"));
    assert!(
      fixture
        .store
        .source_input(
          fixture.intent.project_id,
          fixture.intent.inputs[0].id,
          WorkSourceInputVersion::INITIAL
        )
        .await
        .is_err()
    );
  });
}

struct VisibleSourceTarget(FactoryConfigurationId);
#[async_trait]
impl ManagementAuthorizationPolicy for VisibleSourceTarget {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let visibility = if resource.kind() == ManagementResourceKind::FactoryConfiguration {
      ManagementVisibility::restricted([ManagementResource::instance(
        ManagementResourceKind::FactoryConfiguration,
        ManagementResourceIdentity::new(self.0.to_string()).unwrap(),
      )
      .unwrap()])
      .unwrap()
    } else {
      ManagementVisibility::all()
    };
    Ok(ManagementAuthorizationGrant::new(visibility))
  }
}
#[test]
fn source_configuration_discovery_filters_hidden_inputs_and_groups_before_pagination() {
  run_ready(async {
    let mut fixture = Fixture::new("gitlab").await;
    let visible_id = WorkSourceInputId::from_uuid(Uuid::from_u128(2)).unwrap();
    fixture.intent.inputs[0].id = visible_id;
    let hidden_target = configuration(fixture.intent.project_id, FactoryConfigurationId::generate());
    fixture
      .store
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration: hidden_target.clone(),
        published_at: time(1),
      })
      .unwrap();
    let mut hidden = fixture.intent.inputs[0].clone();
    hidden.id = WorkSourceInputId::from_uuid(Uuid::from_u128(1)).unwrap();
    hidden.definition.target = hidden_target.reference().clone();
    fixture.intent.inputs.push(hidden);
    fixture.intent.group = Some(WorkSourceGroupIntent {
      id: WorkSourceGroupId::generate(),
      expected_version: None,
      members: fixture
        .intent
        .inputs
        .iter()
        .map(|i| WorkSourceMemberRef {
          id: i.id,
          version: WorkSourceInputVersion::INITIAL,
        })
        .collect(),
    });
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: fixture.intent.clone(),
          idempotency_key: IdempotencyKey::new("visible-and-hidden-inputs").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    let policy = Arc::new(VisibleSourceTarget(fixture.intent.inputs[0].definition.target.id()));
    let handlers =
      FactorySourceConfigurationHandlers::new(fixture.store.clone(), policy.clone(), vec![fixture.capability.clone()])
        .unwrap();
    let queries = AuthorizedQueryHandler::new(policy, Arc::new(handlers));
    let page = queries
      .handle_query(
        &context(),
        ListWorkSourceConfigurationsQuery {
          project_id: fixture.intent.project_id,
          after: None,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(
      page.items[0].target(),
      WorkSourceScanTarget::Input {
        id: visible_id,
        version: WorkSourceInputVersion::INITIAL
      }
    );
    assert_eq!(
      page.next_cursor, None,
      "hidden rows and mixed-visibility groups cannot affect pagination"
    );
    let dependencies = queries
      .handle_query(
        &context(),
        ListIntegrationDependenciesQuery {
          connection_id: fixture.intent.inputs[0].definition.connection.as_ref().unwrap().id,
          project_id: fixture.intent.project_id,
          after: None,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(dependencies.items.len(), 1);
    assert_eq!(dependencies.items[0].inputs()[0].id(), visible_id);
    assert!(
      dependencies.next_cursor.is_none(),
      "hidden targets cannot affect dependency cursors"
    );
    let read = queries
      .handle_query(
        &context(),
        GetWorkSourceConfigurationQuery {
          project_id: fixture.intent.project_id,
          target: page.items[0].target(),
        },
      )
      .await
      .unwrap();
    assert_eq!(read, page.items[0]);
    assert!(
      queries
        .handle_query(
          &context(),
          GetWorkSourceConfigurationQuery {
            project_id: fixture.intent.project_id,
            target: WorkSourceScanTarget::Input {
              id: fixture.intent.inputs[1].id,
              version: WorkSourceInputVersion::INITIAL
            }
          }
        )
        .await
        .is_err()
    );
  });
}

#[test]
fn source_configuration_validation_identifies_invalid_input_fields_without_exposing_credentials() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let mut writer = fixture.capability.clone();
    writer.role = WorkSourceCapabilityRole::Reporter;
    let handlers = FactorySourceConfigurationHandlers::new(
      fixture.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      vec![fixture.capability.clone(), writer.clone()],
    )
    .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers));
    for field in ["reporter", "repository", "source"] {
      let mut intent = fixture.intent.clone();
      let definition = &mut intent.inputs[0].definition;
      match field {
        "reporter" => {
          definition.reporter = Some(writer.binding.clone());
          definition.connection.as_mut().unwrap().reporter_configuration = Some(writer.binding.configuration.clone());
        }
        "repository" => {
          definition.repository.definition_digest = FactoryDigest::content_sha256(b"wrong-code-reference")
        }
        _ => {
          definition.source.configuration = ImmutableReference::new(
            FactoryKey::new("unknown-selector").unwrap(),
            FactoryKey::new("v1").unwrap(),
            FactoryDigest::content_sha256(b"unknown"),
          )
        }
      }
      let error = queries
        .handle_query(&context(), ValidateWorkSourceConfigurationQuery { intent })
        .await
        .unwrap_err();
      let ManagementHandlerError::Application(ApplicationError::FactoryValidation(report)) = error else {
        panic!("expected normalized field diagnostics: {error:?}")
      };
      assert_eq!(report.errors[0].field, format!("/inputs/0/definition/{field}"));
      let safe = serde_json::to_string(&report).unwrap();
      assert!(!safe.contains("private-reader"));
      assert!(!safe.contains("unknown-selector"));
    }
  });
}

#[path = "factory_source_preview_tests.rs"]
mod preview;

#[test]
fn connection_dependencies_keep_retained_input_versions_after_replacement_and_revocation() {
  run_ready(async {
    let mut fixture = Fixture::new("custom-tracker").await;
    let connection = fixture.intent.inputs[0].definition.connection.as_ref().unwrap().id;
    let commands =
      AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()));
    commands
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: fixture.intent.clone(),
          idempotency_key: IdempotencyKey::new("dependency-v1").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    fixture.intent.inputs[0].expected_version = Some(WorkSourceInputVersion::INITIAL);
    fixture.intent.inputs[0].definition.enabled = false;
    commands
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: fixture.intent.clone(),
          idempotency_key: IdempotencyKey::new("dependency-v2").unwrap(),
          published_at: time(5),
        },
      )
      .await
      .unwrap();
    fixture
      .store
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: connection,
          project_id: fixture.intent.project_id,
          connection_version: IntegrationVersion::INITIAL,
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![],
        },
        idempotency_key: IdempotencyKey::new("dependency-revoke").unwrap(),
        published_at: time(6),
      }))
      .await
      .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()));
    let page = queries
      .handle_query(
        &context(),
        ListIntegrationDependenciesQuery {
          connection_id: connection,
          project_id: fixture.intent.project_id,
          after: None,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].inputs()[0].version().get(), 1);
    assert!(page.next_cursor.is_some());
    let next = queries
      .handle_query(
        &context(),
        ListIntegrationDependenciesQuery {
          connection_id: connection,
          project_id: fixture.intent.project_id,
          after: page.next_cursor,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(next.items[0].inputs()[0].version().get(), 2);
    assert!(!next.items[0].inputs()[0].definition().enabled);
    assert!(next.next_cursor.is_none());
  });
}

#[test]
fn custom_input_predicates_require_explicit_installed_editing_support() {
  run_ready(async {
    let mut fixture = Fixture::new("custom-tracker").await;
    fixture.intent.inputs[0].definition.predicate = Some(WorkSourcePredicate {
      state: Some(key("todo")),
      all_of: vec!["operator-label".into()],
      any_of: vec![],
      none_of: vec![],
    });
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()));
    assert!(
      queries
        .handle_query(
          &context(),
          ValidateWorkSourceConfigurationQuery { intent: fixture.intent }
        )
        .await
        .is_err(),
      "an installed profile must explicitly advertise normalized predicate editing"
    );
  });
}

#[test]
fn advertised_normalized_predicates_publish_immutable_inputs_for_open_provider_identities() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let mut fixture = Fixture::new(provider).await;
      let predicate = WorkSourcePredicate {
        state: Some(key("todo")),
        all_of: vec!["operator-label".into()],
        any_of: vec![],
        none_of: vec!["blocked".into()],
      };
      fixture.intent.inputs[0].definition.predicate = Some(predicate.clone());
      let handlers = fixture
        .handlers()
        .with_selection_labels(
          fixture.capability.clone(),
          WorkSourceSelectionLabels {
            state: key("any"),
            all_of: vec![],
            any_of: vec![],
            none_of: vec![],
          },
        )
        .unwrap()
        .with_predicate_states(
          fixture.capability.clone(),
          key("any"),
          vec![key("todo"), key("resolved")],
        )
        .unwrap();
      let handlers = Arc::new(handlers);
      let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
      let choice = queries
        .handle_query(
          &context(),
          ListWorkSourceInputChoicesQuery {
            project_id: fixture.intent.project_id,
            connection_id: fixture.intent.inputs[0].definition.connection.as_ref().unwrap().id,
            connection_version: IntegrationVersion::INITIAL,
          },
        )
        .await
        .unwrap();
      assert_eq!(
        choice.items[0].predicate_states.as_ref().unwrap(),
        &[key("todo"), key("resolved")]
      );
      let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
      let published = commands
        .handle_command(
          &context(),
          PublishWorkSourceConfigurationCommand {
            intent: fixture.intent.clone(),
            idempotency_key: IdempotencyKey::new("input-with-predicate").unwrap(),
            published_at: time(4),
          },
        )
        .await
        .unwrap();
      assert_eq!(
        published.publication.inputs[0].input.definition().predicate.as_ref(),
        Some(&predicate)
      );
      fixture.intent.inputs[0].definition.predicate.as_mut().unwrap().state = Some(key("unadvertised-state"));
      assert!(
        queries
          .handle_query(
            &context(),
            ValidateWorkSourceConfigurationQuery { intent: fixture.intent }
          )
          .await
          .is_err()
      );
    }
  });
}

#[path = "factory_source_example_tests.rs"]
mod examples;
#[path = "factory_source_reporting_tests.rs"]
mod reporting;

#[test]
fn source_input_cannot_enable_a_pipeline_requiring_an_issue_association_it_cannot_supply() {
  run_ready(async {
    let mut fixture = Fixture::new("custom-tracker").await;
    let project = fixture.intent.project_id;
    let input = &mut fixture.intent.inputs[0].definition;
    let selected = fixture
      .store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: input.target.id(),
        configuration_version: input.target.version(),
        repository_id: input.repository.id,
        repository_version: input.repository.version,
      })
      .await
      .unwrap();
    let flow = crate::factory_admission_tests::with_issue_status_action(
      selected.configuration.configuration.flow(),
      IssuePipelineStatusSelection {
        workflow_id: octacity_server_domain::IssueWorkflowId::generate(),
        workflow_version: octacity_server_domain::IssueWorkflowVersion::INITIAL,
        transition_id: octacity_server_domain::IssueTransitionId::generate(),
        outcome: "succeeded".parse().unwrap(),
      },
    );
    let configuration =
      crate::factory_admission_tests::configuration_with_flow(project, FactoryConfigurationId::generate(), Some(flow));
    input.target = configuration.reference().clone();
    fixture
      .store
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration,
        published_at: time(1),
      })
      .unwrap();
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()));
    let before = fixture.store.mutation_evidence_counts().await;
    assert!(
      queries
        .handle_query(
          &context(),
          ValidateWorkSourceConfigurationQuery {
            intent: fixture.intent.clone()
          }
        )
        .await
        .is_err(),
      "enabled intake cannot provide the required Issue association"
    );
    assert_eq!(fixture.store.mutation_evidence_counts().await, before);
    fixture.intent.inputs[0].definition.enabled = false;
    let disabled = queries
      .handle_query(
        &context(),
        ValidateWorkSourceConfigurationQuery { intent: fixture.intent },
      )
      .await
      .unwrap();
    assert!(!disabled.inputs[0].input.definition().enabled);
    assert_eq!(fixture.store.mutation_evidence_counts().await, before);
  });
}
