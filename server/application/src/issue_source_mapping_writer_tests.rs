//! Push authoring uses existing independent integration grants and retained Issue intent.
use super::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion};
use octacity_server_store::{testing::*, *};

async fn writer(
  project_id: ProjectId,
) -> (
  InstalledIssueSourceWriter,
  Arc<InMemoryFactoryConfigurationStore>,
  IntegrationId,
) {
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project_id).unwrap();
  let (mut installed, mut draft) = integration_connection_fixture("second-native-vocabulary");
  let source = selection(project_id);
  installed.installation = source.source.installation.clone();
  draft.installation = installed.installation.clone();
  installed.resources[0].reference = source.source.resource.clone();
  installed.resources[0].native_id = source.item.resource;
  draft.resources[0].reference = source.source.resource.clone();
  let mut update = installed.resources[0].capabilities[0].clone();
  update.purpose = IntegrationPurpose::TaskUpdate;
  update.capability = exact("native-status-writer");
  update.configuration = exact("native-effect-settings");
  installed.resources[0].capabilities.push(update.clone());
  draft.resources[0].purposes.push(IntegrationPurpose::TaskUpdate);
  let credential = update.credential.clone();
  let plugin = installed.adapter.clone();
  let binding = WorkSourceBinding {
    installation: installed.installation.clone(),
    capability: update.capability,
    configuration: update.configuration,
    resource: source.source.resource,
  };
  let connection = IntegrationId::generate();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id: connection,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: key("mapping-writer-connection"),
      published_at: at(1),
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
          reference: credential,
          kind: octacity_server_secrets::CredentialPublicationKind::Adopt,
          activate_connection: true,
        },
      },
      idempotency_key: key("mapping-writer-activate"),
      requested_at: at(2),
    }))
    .await
    .unwrap();
  store
    .complete_integration_administration(management_mutation(preparation))
    .await
    .unwrap();
  store
    .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
      intent: IntegrationProjectAccessIntent {
        connection_id: connection,
        project_id,
        connection_version: IntegrationVersion::INITIAL,
        expected_current_version: None,
        resources: vec![IntegrationResourceSelection {
          reference: binding.resource.clone(),
          purposes: vec![IntegrationPurpose::TaskRead, IntegrationPurpose::TaskUpdate],
        }],
      },
      idempotency_key: key("mapping-writer-project-grants"),
      published_at: at(3),
    }))
    .await
    .unwrap();
  (
    InstalledIssueSourceWriter {
      binding,
      plugin,
      access: ManagedIntegrationAccess::new(store.clone(), connection, IntegrationVersion::INITIAL),
      effects: octacity_server_factory::WorkSourceReportingPolicy {
        labels: vec!["native-ready".into()],
        states: vec!["closed".into()],
      },
    },
    store,
    connection,
  )
}

struct DenyConnectionUse;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for DenyConnectionUse {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::Execute && resource.kind() == ManagementResourceKind::IntegrationConnection {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}

#[test]
fn issue_source_choices_recheck_independent_writer_authority_without_hiding_authorized_reads() {
  run_ready(async {
    for allow_connection in [true, false] {
      let store = Arc::new(InMemoryProjectStore::new());
      let project_id = project(&store).await;
      let policy: Arc<dyn ManagementAuthorizationPolicy> = if allow_connection {
        Arc::new(TrustedNetworkManagementPolicy)
      } else {
        Arc::new(DenyConnectionUse)
      };
      let (writer, connections, connection_id) = writer(project_id).await;
      let binding = writer.binding.clone();
      let mut capability = installed(project_id);
      capability.writer = Some(writer);
      let owner = Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![capability])
          .unwrap(),
      );
      let link = linked(
        AuthorizedCommandHandler::new(policy.clone(), owner.clone())
          .handle_command(&context(), import(selection(project_id), "choices-link"))
          .await
          .unwrap(),
      );
      let queries = AuthorizedQueryHandler::new(policy, owner);
      let query = GetIssueSourceChoicesQuery {
        project_id,
        issue_id: link.issue_id,
      };
      let choices = queries.handle_query(&context(), query.clone()).await.unwrap();
      assert_eq!(choices.items.len(), 1);
      assert_eq!(
        choices.items[0].writer.as_ref().map(|w| &w.binding),
        allow_connection.then_some(&binding)
      );
      assert_eq!(
        choices.items[0].push_fields,
        if allow_connection {
          vec![IssueField::Status]
        } else {
          vec![]
        }
      );
      let access = connections
        .integration_project_access(connection_id, project_id)
        .await
        .unwrap()
        .unwrap();
      connections
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id,
            project_id,
            connection_version: IntegrationVersion::INITIAL,
            expected_current_version: Some(access.version),
            resources: vec![IntegrationResourceSelection {
              reference: binding.resource.clone(),
              purposes: vec![IntegrationPurpose::TaskRead],
            }],
          },
          idempotency_key: key("choices-revoke-write-only"),
          published_at: at(31),
        }))
        .await
        .unwrap();
      let choices = queries.handle_query(&context(), query).await.unwrap();
      assert_eq!(choices.items.len(), 1);
      assert!(choices.items[0].writer.is_none());
      assert!(choices.items[0].push_fields.is_empty());
      assert_eq!(
        choices.items[0].pull_fields,
        vec![IssueField::Title, IssueField::Description, IssueField::Status]
      );
    }
  });
}

#[test]
fn issue_source_mapping_issue_administration_cannot_bypass_connection_use_permission() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(DenyConnectionUse);
    let (writer, _, _) = writer(project_id).await;
    let binding = writer.binding.clone();
    let mut capability = installed(project_id);
    capability.writer = Some(writer);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy)
          .with_source_mappings(vec![capability])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "connection-permission-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let mut command = mapping_command(&issue, "deny-connection-use");
    command.intent.definition.reporter = Some(binding);
    command.intent.definition.push_enabled = true;
    command
      .intent
      .definition
      .fields
      .insert(IssueField::Status, IssueSourceFieldAuthority::OctacityOwnedPush);
    command.intent.definition.status.as_mut().unwrap().push = Some(octacity_server_factory::WorkSourceStatusMapping {
      statuses: vec![octacity_server_factory::WorkSourceStatusEffects {
        status: issue.status_id.to_string(),
        add: vec!["native-ready".into()],
        remove: vec![],
        state: None,
      }],
    });
    let before = store.mutation_evidence_counts().await;
    assert!(
      commands.handle_command(&context(), command).await.is_err(),
      "Issue administration does not grant connection use"
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      store.issue_field_policy(project_id, issue.id).await.unwrap(),
      IssueFieldPolicy::default()
    );
  });
}

#[test]
fn issue_source_mapping_push_requires_current_write_grant_and_does_not_pull_native_divergence() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let (writer, connections, connection_id) = writer(project_id).await;
    let writer_binding = writer.binding.clone();
    let mut capability = installed(project_id);
    capability.writer = Some(writer);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![capability])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "push-link"))
        .await
        .unwrap(),
    );
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    let workflow = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let edge = &workflow.definition.transitions[0];
    let mut command = mapping_command(&before, "publish-local-owned-status");
    command.intent.definition.reporter = Some(writer_binding);
    command.intent.definition.push_enabled = true;
    command
      .intent
      .definition
      .fields
      .insert(IssueField::Status, IssueSourceFieldAuthority::OctacityOwnedPush);
    command.intent.definition.status.as_mut().unwrap().push = Some(octacity_server_factory::WorkSourceStatusMapping {
      statuses: vec![octacity_server_factory::WorkSourceStatusEffects {
        status: edge.to.to_string(),
        add: vec!["native-ready".into()],
        remove: vec![],
        state: Some("closed".into()),
      }],
    });
    let mapping = commands
      .handle_command(&context(), command.clone())
      .await
      .unwrap()
      .mapping;
    let local = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    )
    .handle_command(
      &context(),
      TransitionIssueCommand {
        intent: TransitionIssueIntent {
          project_id,
          id: before.id,
          expected_version: before.version,
          workflow_id: before.workflow_id,
          workflow_version: before.workflow_version,
          expected_status_id: before.status_id,
          transition_id: edge.id,
          reason: None,
        },
        idempotency_key: key("accepted-local-status"),
        moved_at: at(33),
      },
    )
    .await
    .unwrap()
    .issue;
    let pushes = store.retained_issue_push_intents().unwrap();
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].mapping, mapping.reference());
    assert_eq!(pushes[0].fields, [IssueField::Status].into());
    let reads = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader {
          observed_at: 34,
          ..Default::default()
        }),
        policy,
      )),
    );
    let observed = reads
      .handle_command(
        &context(),
        RefreshIssueSourceCommand {
          project_id,
          issue_id: before.id,
          read: read(&selection(project_id)),
          idempotency_key: key("native-divergence"),
          refreshed_at: at(35),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      observed.status.unwrap().condition,
      IssueSourceStatusCondition::Unmanaged
    );
    assert_eq!(store.issue(project_id, before.id, None).await.unwrap(), local);
    assert_eq!(store.retained_issue_push_intents().unwrap().len(), 1);
    connections
      .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id,
          project_id,
          connection_version: IntegrationVersion::INITIAL,
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![IntegrationResourceSelection {
            reference: selection(project_id).source.resource,
            purposes: vec![IntegrationPurpose::TaskRead],
          }],
        },
        idempotency_key: key("revoke-mapping-write"),
        published_at: at(36),
      }))
      .await
      .unwrap();
    let original = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(
      original.mapping, mapping,
      "original replay must not activate another mapping"
    );
    command.intent.expected_mapping = Some(mapping.reference());
    command.intent.expected_issue_version = local.version;
    command.idempotency_key = key("deny-new-write-after-revocation");
    command.published_at = at(37);
    assert!(
      commands.handle_command(&context(), command).await.is_err(),
      "task read access cannot activate a writer"
    );
  });
}
