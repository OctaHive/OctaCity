//! Composition fixtures only. Native services and private storage remain explicit hermetic peers.
use super::*;
use octacity_server_secrets::{CredentialPublication, ProviderCredentialError, ProviderCredentialPublisher};
use std::sync::{Mutex, atomic::AtomicBool};

// Both pilots use identical public operations after installing their immutable target.
#[async_trait::async_trait]
pub(super) trait PilotStore:
  WorkSourceArtifactStore
  + FactorySourceConfigurationStore
  + FactoryDiscoveryStore
  + FactoryConfigurationStore
  + FactoryRunStore
  + IssueSourceProjectionStore
  + IntegrationConnectionStore
  + IntegrationAdministrationStore
  + IntegrationProjectAccessStore
  + 'static
{
  async fn install_target(&self, target: FactoryConfiguration);
}
#[async_trait::async_trait]
impl PilotStore for InMemoryFactoryConfigurationStore {
  async fn install_target(&self, target: FactoryConfiguration) {
    self
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration: target,
        published_at: time(2),
      })
      .unwrap();
  }
}
#[async_trait::async_trait]
impl PilotStore for octacity_server_store_postgres::PostgresStore {
  async fn install_target(&self, target: FactoryConfiguration) {
    self
      .create_factory_configuration(octacity_server_store::testing::management_mutation(
        CreateFactoryConfiguration {
          configuration: target.clone(),
          idempotency_key: "pilot-downstream".parse().unwrap(),
          published_at: time(2),
          intent: FactoryConfigurationMutationIntent::Create {
            id: target.reference().id(),
            project_id: target.reference().project_id(),
            definition_digest: target.reference().definition_digest(),
            draft: FactoryConfigurationDraft {
              flow: Some(target.flow().clone()),
              admission_policy: key("admission"),
              wip_limits: target.wip_limits(),
              hard_budget: target.hard_budget(),
              permission_ceiling: key("permissions"),
              enabled: true,
            },
          },
        },
      ))
      .await
      .unwrap();
  }
}

struct PilotPolicy;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for PilotPolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

/// Simulates the independently owned private publication boundary, including one lost activation.
#[derive(Default)]
pub(super) struct PrivatePublisher {
  prepared: Mutex<BTreeSet<[u8; 32]>>,
  pub(super) fail_activation: AtomicBool,
}
#[async_trait::async_trait]
impl ProviderCredentialPublisher for PrivatePublisher {
  async fn prepare(
    &self,
    stamp: &CredentialPublication,
    _: CredentialPublicationKind,
    _: Option<ProviderCredentialValue>,
  ) -> Result<(), ProviderCredentialError> {
    self.prepared.lock().unwrap().insert(stamp.operation);
    Ok(())
  }
  async fn activate(&self, stamp: &CredentialPublication) -> Result<(), ProviderCredentialError> {
    if !self.prepared.lock().unwrap().contains(&stamp.operation) || self.fail_activation.swap(false, Ordering::SeqCst) {
      return Err(ProviderCredentialError::Unavailable);
    }
    Ok(())
  }
  async fn discard(&self, _: &CredentialPublication) -> Result<(), ProviderCredentialError> {
    Ok(())
  }
}

pub(super) fn administrator() -> ManagementRequestContext {
  ManagementRequestContext::new(
    ManagementActor::authenticated("pilot-operator").unwrap(),
    octacity_server_application::ManagementSecurityScope::new("pilot-administration").unwrap(),
    ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .unwrap()
}

pub(super) async fn credential<S: PilotStore>(
  fixture: &Fixture<S>,
  publisher: Arc<PrivatePublisher>,
  request: BeginIntegrationAdministration,
) -> Result<IntegrationAdministrationReceipt, ManagementHandlerError<ApplicationError>> {
  AuthorizedCommandHandler::new(
    Arc::new(PilotPolicy),
    Arc::new(IntegrationAdministrationHandlers::new(fixture.store.clone(), publisher)),
  )
  .handle_command(
    &administrator(),
    ManageIntegrationCredentialCommand {
      value: matches!(
        request.intent.action,
        IntegrationAdministrationAction::Credential {
          kind: CredentialPublicationKind::Rotate,
          ..
        }
      )
      .then(|| ProviderCredentialValue::new(b"synthetic-pilot-rotation".to_vec()).unwrap()),
      request,
    },
  )
  .await
}

pub(super) fn managed_projector<S: PilotStore>(
  fixture: &Fixture<S>,
  profile: &FactoryReporterProfile,
) -> Arc<HostedFactoryWorkStatusProjector<S>> {
  Arc::new(
    Arc::try_unwrap(hosted(fixture, vec![profile.clone()]))
      .ok()
      .unwrap()
      .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone())),
  )
}
pub(super) fn observation_reader<S: PilotStore>(
  fixture: &Fixture<S>,
  connection: &WorkSourceConnectionRef,
) -> HostedWorkSourceObservations {
  let access = ManagedIntegrationConnections::new(fixture.store.clone())
    .for_work_binding(connection, &fixture.profile.read.binding, IntegrationPurpose::TaskRead)
    .unwrap();
  HostedWorkSourceObservations::new(
    Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
    vec![fixture.profile.read.clone().with_managed_access(access)],
    fixture.clock.clone(),
    settings().operation_timeout,
    settings().cancellation_grace,
    CancellationToken::new(),
  )
  .unwrap()
}
pub(super) fn observation_request<S: PilotStore>(fixture: &Fixture<S>) -> ReadWorkSourceObservations {
  ReadWorkSourceObservations {
    connection: None,
    project_id: fixture.input.project_id(),
    source: fixture.profile.read.binding.clone(),
    scan_id: digest(81),
    window: None,
    continuation: None,
    limit: 2,
  }
}

pub(super) async fn grant<S: PilotStore>(
  fixture: &Fixture<S>,
  id: IntegrationId,
  expected: Option<IntegrationVersion>,
  purposes: Vec<IntegrationPurpose>,
  operation: &str,
) -> PublishedIntegrationProjectAccess {
  AuthorizedCommandHandler::new(
    Arc::new(PilotPolicy),
    Arc::new(IntegrationProjectAccessHandlers::new(
      fixture.store.clone(),
      Arc::new(PilotPolicy),
    )),
  )
  .handle_command(
    &administrator(),
    PublishIntegrationProjectAccessCommand {
      request: PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          connection_version: IntegrationVersion::INITIAL,
          project_id: fixture.input.project_id(),
          expected_current_version: expected,
          resources: if purposes.is_empty() {
            vec![]
          } else {
            vec![IntegrationResourceSelection {
              reference: fixture.input.definition().source.resource.clone(),
              purposes,
            }]
          },
        },
        idempotency_key: IdempotencyKey::new(operation).unwrap(),
        published_at: time(10),
      },
    },
  )
  .await
  .unwrap()
  .access
}

pub(super) async fn connect<S: PilotStore>(
  fixture: &Fixture<S>,
  profile: &FactoryReporterProfile,
  publisher: Arc<PrivatePublisher>,
) -> WorkSourceConnectionRef {
  let read = &fixture.profile.read;
  let capabilities = [
    (IntegrationPurpose::TaskRead, &read.binding, "pilot-reader"),
    (IntegrationPurpose::TaskUpdate, profile.binding(), "pilot-writer"),
  ]
  .map(|(purpose, binding, handle)| IntegrationCapability {
    purpose,
    capability: binding.capability.clone(),
    configuration: binding.configuration.clone(),
    credential: LogicalSecretReference::new(
      octacity_server_secrets::SecretProviderId::new("installed").unwrap(),
      handle,
    )
    .unwrap(),
  });
  let selected = vec![IntegrationResourceSelection {
    reference: read.binding.resource.clone(),
    purposes: vec![IntegrationPurpose::TaskRead, IntegrationPurpose::TaskUpdate],
  }];
  let installed = InstalledIntegration {
    installation: read.binding.installation.clone(),
    provider: read.plugin.identity().clone(),
    adapter: read.plugin.clone(),
    endpoints: vec!["https://tracker.invalid/".into()],
    authentication: vec![key("token")],
    resources: vec![IntegrationResource {
      reference: read.binding.resource.clone(),
      native_id: ExternalWorkIdentity::new("native-container").unwrap(),
      locator: ExternalWorkIdentity::new("tickets/project").unwrap(),
      capabilities: capabilities.to_vec(),
    }],
  };
  let id = IntegrationId::generate();
  let command = PublishIntegrationConnectionCommand {
    intent: IntegrationConnectionIntent {
      id,
      expected_current_version: None,
      draft: IntegrationConnectionDraft {
        name: IntegrationName::new("Shared assessment tracker").unwrap(),
        installation: read.binding.installation.clone(),
        endpoint: "https://tracker.invalid/".into(),
        authentication: key("token"),
        resources: selected,
      },
    },
    idempotency_key: IdempotencyKey::new("pilot-named-connection").unwrap(),
    published_at: time(2),
  };
  let metadata = AuthorizedCommandHandler::new(
    Arc::new(PilotPolicy),
    Arc::new(IntegrationConnectionHandlers::new(fixture.store.clone(), vec![installed]).unwrap()),
  );
  let published = metadata
    .handle_command(&administrator(), command.clone())
    .await
    .unwrap();
  assert_eq!(
    published.connection.definition.name.as_str(),
    "Shared assessment tracker"
  );
  assert!(
    fixture
      .store
      .integration_project_access(id, fixture.input.project_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(!fixture.store.integration_administration(id).await.unwrap().enabled);
  for (index, capability) in capabilities.into_iter().enumerate() {
    let request = BeginIntegrationAdministration {
      intent: IntegrationAdministrationIntent {
        id,
        expected_metadata_version: IntegrationVersion::INITIAL,
        expected_revision: index as u64,
        action: IntegrationAdministrationAction::Credential {
          reference: capability.credential,
          kind: CredentialPublicationKind::Adopt,
          activate_connection: index == 1,
        },
      },
      idempotency_key: IdempotencyKey::new(format!("pilot-adopt-{index}")).unwrap(),
      requested_at: time(3 + index as i64),
    };
    if index == 1 {
      publisher.fail_activation.store(true, Ordering::SeqCst);
      assert!(credential(fixture, publisher.clone(), request.clone()).await.is_err());
      assert!(!fixture.store.integration_administration(id).await.unwrap().enabled);
    }
    credential(fixture, publisher.clone(), request).await.unwrap();
  }
  assert_eq!(
    metadata
      .handle_command(&administrator(), command)
      .await
      .unwrap()
      .connection,
    published.connection
  );
  let access = grant(
    fixture,
    id,
    None,
    vec![IntegrationPurpose::TaskRead, IntegrationPurpose::TaskUpdate],
    "pilot-project-grant",
  )
  .await;
  WorkSourceConnectionRef {
    id,
    version: published.connection.version,
    access_version: access.version,
    source_configuration: read.binding.configuration.clone(),
    reporter_configuration: Some(profile.binding().configuration.clone()),
  }
}

pub(super) fn handlers<S: PilotStore>(
  fixture: &Fixture<S>,
  profile: &FactoryReporterProfile,
  labels: &[&str],
) -> FactorySourceConfigurationHandlers<S> {
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.read.binding.clone(),
  };
  let reporter = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: profile.binding().clone(),
  };
  FactorySourceConfigurationHandlers::new(
    fixture.store.clone(),
    Arc::new(PilotPolicy),
    vec![source.clone(), reporter.clone()],
  )
  .unwrap()
  .with_selection_labels(
    source.clone(),
    WorkSourceSelectionLabels {
      label_presence: Default::default(),
      state: key("any"),
      all_of: vec![],
      any_of: vec![],
      none_of: vec![],
    },
  )
  .unwrap()
  .with_predicate_states(source, key("any"), vec![key("native-state"), key("closed")])
  .unwrap()
  .with_reporting_mapping(
    reporter,
    WorkSourceStatusMapping {
      statuses: vec![WorkSourceStatusEffects {
        status: "needs-human-eyes".into(),
        add: vec![labels[0].into()],
        remove: vec![labels[1].into()],
        state: None,
      }],
    },
    Some(WorkSourceReportingPolicy {
      labels: labels.iter().map(|s| (*s).into()).collect(),
      states: vec![],
    }),
  )
  .unwrap()
}

pub(super) async fn publish<S: PilotStore>(
  fixture: &Fixture<S>,
  profile: &FactoryReporterProfile,
  labels: &[&str],
  inputs: Vec<WorkSourceInputIntent>,
  group: Option<WorkSourceGroupIntent>,
  operation: &str,
) -> WorkSourceConfigurationOutcome {
  AuthorizedCommandHandler::new(Arc::new(PilotPolicy), Arc::new(handlers(fixture, profile, labels)))
    .handle_command(
      &fixture.profile.read.context,
      PublishWorkSourceConfigurationCommand {
        intent: WorkSourceConfigurationIntent {
          project_id: fixture.input.project_id(),
          inputs,
          group,
        },
        idempotency_key: IdempotencyKey::new(operation).unwrap(),
        published_at: time(11),
      },
    )
    .await
    .unwrap()
}

pub(super) struct Pilot<S = InMemoryFactoryConfigurationStore> {
  pub(super) fixture: Fixture<S>,
  pub(super) profile: FactoryReporterProfile,
  pub(super) connection: WorkSourceConnectionRef,
  pub(super) downstream_id: WorkSourceInputId,
  pub(super) publisher: Arc<PrivatePublisher>,
}
pub(super) async fn pipeline(provider: &str, labels: &[&str; 2]) -> Pilot {
  let (fixture, profile) = fixture_with_reporting_policy(
    provider,
    None,
    Some(WorkSourceReportingPolicy {
      labels: labels.iter().map(|s| (*s).into()).collect(),
      states: vec![],
    }),
  )
  .await;
  pipeline_in(fixture, profile, labels).await
}
pub(super) async fn pipeline_in<S: PilotStore>(
  fixture: Fixture<S>,
  profile: FactoryReporterProfile,
  labels: &[&str; 2],
) -> Pilot<S> {
  pipeline_with_target(fixture, profile, labels, None).await
}
pub(super) async fn pipeline_with_target<S: PilotStore>(
  mut fixture: Fixture<S>,
  profile: FactoryReporterProfile,
  labels: &[&str; 2],
  flow: Option<FactoryFlowConfiguration>,
) -> Pilot<S> {
  let publisher = Arc::new(PrivatePublisher::default());
  let connection = connect(&fixture, &profile, publisher.clone()).await;
  // The assessment is a deterministic accepted typed Factory action, with no model call.
  let mut assessment = fixture.input.definition().clone();
  assessment.reporter = Some(profile.binding().clone());
  let mut connection = connection;
  connection.reporter_configuration = Some(profile.binding().configuration.clone());
  assessment.connection = Some(connection.clone());
  assessment.reporting = Some(WorkSourceStatusMapping {
    statuses: vec![WorkSourceStatusEffects {
      status: "needs-human-eyes".into(),
      add: vec![labels[0].into()],
      remove: vec![labels[1].into()],
      state: None,
    }],
  });
  assessment.predicate = Some(WorkSourcePredicate {
    label_presence: Default::default(),
    state: None,
    all_of: vec![],
    any_of: vec![],
    none_of: vec![labels[0].into(), labels[1].into()],
  });
  let target = configuration_with_flow(fixture.input.project_id(), FactoryConfigurationId::generate(), flow);
  fixture.store.install_target(target.clone()).await;
  let mut downstream = assessment.clone();
  downstream.target = target.reference().clone();
  downstream.predicate = Some(WorkSourcePredicate {
    label_presence: Default::default(),
    state: None,
    all_of: vec![labels[0].into()],
    any_of: vec![],
    none_of: vec![labels[1].into()],
  });
  let downstream_id = WorkSourceInputId::generate();
  let inputs = vec![
    WorkSourceInputIntent {
      id: fixture.input.id(),
      expected_version: Some(fixture.input.version()),
      definition: assessment,
    },
    WorkSourceInputIntent {
      id: downstream_id,
      expected_version: None,
      definition: downstream,
    },
  ];
  let publication = publish(&fixture, &profile, labels, inputs.clone(), None, "pilot-publish-inputs").await;
  fixture.input = publication
    .publication
    .inputs
    .iter()
    .find(|i| i.input.id() == fixture.input.id())
    .unwrap()
    .input
    .clone();
  let replay = publish(&fixture, &profile, labels, inputs, None, "pilot-publish-inputs").await;
  assert_eq!(replay.publication, publication.publication);
  assert!(fixture.runs().await.is_empty());
  Pilot {
    fixture,
    profile,
    connection,
    downstream_id,
    publisher,
  }
}

pub(super) async fn overlapping_group<S: PilotStore>(
  fixture: &Fixture<S>,
  profile: &FactoryReporterProfile,
  labels: &[&str; 2],
  downstream: &WorkSourceInput,
) -> (WorkSourceGroup, WorkSourceInputId) {
  let overlapping = WorkSourceInputId::generate();
  let group_id = WorkSourceGroupId::generate();
  let grouped = publish(
    fixture,
    profile,
    labels,
    vec![WorkSourceInputIntent {
      id: overlapping,
      expected_version: None,
      definition: downstream.definition().clone(),
    }],
    Some(WorkSourceGroupIntent {
      id: group_id,
      expected_version: None,
      members: vec![
        WorkSourceMemberRef {
          id: downstream.id(),
          version: WorkSourceInputVersion::INITIAL,
        },
        WorkSourceMemberRef {
          id: overlapping,
          version: WorkSourceInputVersion::INITIAL,
        },
      ],
    }),
    "pilot-overlapping-group",
  )
  .await;
  let group = grouped.publication.group.unwrap().group;
  (group, overlapping)
}
