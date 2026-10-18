//! Public runtime preview uses only a verified read process and never the worker journal.
use super::*;
use octacity_server_store::testing::MutationEvidenceProbe;
#[tokio::test]
async fn hosted_preview_reads_explicit_capability_without_claiming_or_admitting() {
  for provider in ["github", "gitlab", "jira"] {
    let fixture = Fixture::enabled(provider).await;
    let dir = fixture.root.path().join(provider);
    let manifest = std::fs::read_to_string(dir.join("adapter.toml")).unwrap();
    std::fs::write(
      dir.join("adapter.toml"),
      manifest.replace("authorize=true", "authorize=true\npreview=true"),
    )
    .unwrap();
    let reader = HostedFactoryWorkSourcePreview::new(
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![fixture.profile.clone()],
      settings().operation_timeout,
      settings().cancellation_grace,
      CancellationToken::new(),
    )
    .unwrap();
    let request = ReadWorkSourcePreview {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputIntent {
        id: fixture.input.id(),
        expected_version: Some(fixture.input.version()),
        definition: fixture.input.definition().clone(),
      }],
      limit: 2,
    };
    let page = reader.preview(request.clone()).await.unwrap();
    assert_eq!(page.candidates.len(), 1);
    assert_eq!(page.candidates[0].title, "Original task");
    assert!(page.candidates[0].selections[0].reasons.is_empty());
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
    assert!(
      fixture
        .store
        .source_scan_schedule(
          fixture.input.project_id(),
          WorkSourceScanTarget::Input {
            id: fixture.input.id(),
            version: fixture.input.version()
          }
        )
        .await
        .unwrap()
        .is_none()
    );
    let calls = std::fs::read_to_string(dir.join("requests.jsonl")).unwrap();
    assert_eq!(calls.lines().count(), 1);
    assert!(calls.contains("\"operation\": \"preview\""));
    let mut managed = request;
    managed.inputs[0].definition.connection = Some(WorkSourceConnectionRef {
      id: IntegrationId::generate(),
      version: IntegrationVersion::INITIAL,
      access_version: IntegrationVersion::INITIAL,
      source_configuration: fixture.profile.binding.configuration.clone(),
      reporter_configuration: None,
    });
    assert!(
      reader.preview(managed).await.is_err(),
      "managed input cannot borrow legacy credentials"
    );
    assert_eq!(std::fs::read_to_string(dir.join("requests.jsonl")).unwrap(), calls);
  }
}
#[tokio::test]
async fn source_diagnostics_reads_existing_progress_and_admissions_without_changing_them() {
  let fixture = Fixture::enabled("custom-tracker").await;
  let handlers =
    FactorySourceConfigurationHandlers::new(fixture.store.clone(), Arc::new(TrustedNetworkManagementPolicy), vec![])
      .unwrap();
  let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers));
  let query = GetWorkSourceDiagnosticsQuery {
    project_id: fixture.input.project_id(),
    target: WorkSourceScanTarget::Input {
      id: fixture.input.id(),
      version: fixture.input.version(),
    },
    after: None,
    limit: 2,
  };
  let initial = queries
    .handle_query(&fixture.profile.context, query.clone())
    .await
    .unwrap();
  assert!(initial.scan.is_none());
  assert!(initial.admissions.is_empty());
  fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  let before = fixture.store.mutation_evidence_counts().await;
  let result = queries
    .handle_query(&fixture.profile.context, query.clone())
    .await
    .unwrap();
  assert!(result.scan.is_some());
  assert_eq!(result.admissions.len(), 1);
  assert_eq!(result.admissions[0].association.run_id, fixture.runs().await[0].id);
  assert_eq!(result.admissions[0].association.input_id, fixture.input.id());
  assert_eq!(result.admissions[0].association.input_version, fixture.input.version());
  assert!(result.admissions[0].reporting.is_empty());
  assert!(result.admissions[0].issue.is_none());
  assert_eq!(
    queries.handle_query(&fixture.profile.context, query).await.unwrap(),
    result
  );
  assert_eq!(fixture.store.mutation_evidence_counts().await, before);
}

/// An enabled card/observation owner exposes only its read capability to Factory preview.
struct Cards {
  project: ProjectId,
  visible: std::sync::atomic::AtomicBool,
  state: std::sync::Mutex<(u32, u32, u32, u32)>,
}
impl Cards {
  fn observe(&self) {
    let mut state = self.state.lock().unwrap();
    state.0 += 1;
    state.1 += 1;
    state.2 += 1;
    state.3 += 1;
  }
}
#[async_trait::async_trait]
impl FactoryWorkSourceIssueLinks for Cards {
  async fn visible_issue(
    &self,
    _: &ManagementRequestContext,
    project: ProjectId,
    _: FactoryDigest,
  ) -> Result<Option<WorkSourceIssueLink>, ApplicationError> {
    assert_eq!(project, self.project);
    Ok(self.visible.load(Ordering::SeqCst).then(|| WorkSourceIssueLink {
      project_id: project,
      issue_id: ManagementResourceIdentity::new("internal-task-19").unwrap(),
    }))
  }
}
#[tokio::test]
async fn source_preview_correlates_existing_work_and_optional_cards_without_observation_mutations() {
  let mut fixture = Fixture::enabled("custom-tracker").await;
  let (_, connection) = managed_access(&fixture, &fixture.profile.binding, IntegrationPurpose::TaskRead).await;
  let mut definition = fixture.input.definition().clone();
  definition.connection = Some(WorkSourceConnectionRef {
    id: connection,
    version: IntegrationVersion::INITIAL,
    access_version: IntegrationVersion::INITIAL,
    source_configuration: fixture.profile.binding.configuration.clone(),
    reporter_configuration: None,
  });
  let intent = WorkSourceConfigurationIntent {
    project_id: fixture.input.project_id(),
    inputs: vec![WorkSourceInputIntent {
      id: fixture.input.id(),
      expected_version: Some(fixture.input.version()),
      definition,
    }],
    group: None,
  };
  let handlers = || {
    FactorySourceConfigurationHandlers::new(
      fixture.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      vec![WorkSourceCapability {
        project_id: intent.project_id,
        role: WorkSourceCapabilityRole::Source,
        binding: fixture.profile.binding.clone(),
      }],
    )
    .unwrap()
  };
  let published = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers()))
    .handle_command(
      &fixture.profile.context,
      PublishWorkSourceConfigurationCommand {
        intent: intent.clone(),
        idempotency_key: IdempotencyKey::new("preview-managed").unwrap(),
        published_at: time(4),
      },
    )
    .await
    .unwrap();
  let dir = fixture.root.path().join("custom-tracker");
  let manifest = std::fs::read_to_string(dir.join("adapter.toml")).unwrap();
  std::fs::write(
    dir.join("adapter.toml"),
    manifest.replace("authorize=true", "authorize=true\npreview=true"),
  )
  .unwrap();
  let reader = HostedFactoryWorkSourcePreview::new(
    Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
    vec![fixture.profile.clone()],
    settings().operation_timeout,
    settings().cancellation_grace,
    CancellationToken::new(),
  )
  .unwrap()
  .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
  let cards = Arc::new(Cards {
    project: intent.project_id,
    visible: std::sync::atomic::AtomicBool::new(true),
    state: std::sync::Mutex::new((0, 0, 0, 0)),
  });
  cards.observe(); // The observation owner is enabled and has already imported its task.
  let before = *cards.state.lock().unwrap();
  let queries = AuthorizedQueryHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(
      handlers()
        .with_preview(Arc::new(reader))
        .with_issue_links(cards.clone()),
    ),
  );
  let query = PreviewWorkSourcesQuery {
    intent: intent.clone(),
    limit: 2,
    observed_at: time(5),
  };
  let first = queries
    .handle_query(&fixture.profile.context, query.clone())
    .await
    .unwrap();
  assert!(first.candidates[0].admissions.is_empty());
  assert!(first.candidates[0].issue.is_some());
  fixture.input = published.publication.inputs[0].input.clone();
  fixture
    .worker()
    .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()))
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  let second = queries
    .handle_query(&fixture.profile.context, query.clone())
    .await
    .unwrap();
  assert_eq!(second.candidates[0].admissions.len(), 1);
  assert_eq!(
    second.candidates[0].admissions[0].association.run_id,
    fixture.runs().await[0].id
  );
  assert_eq!(
    second.candidates[0].admissions[0].association.native_correlation,
    first.candidates[0].native_correlation
  );
  assert_eq!(
    second.candidates[0].disposition,
    WorkSourcePreviewDisposition::Selected,
    "a match remains separate from the existing admission"
  );
  cards.visible.store(false, Ordering::SeqCst);
  let third = queries.handle_query(&fixture.profile.context, query).await.unwrap();
  assert!(third.candidates[0].issue.is_none());
  assert!(third.candidates[0].admissions[0].issue.is_none());
  assert_eq!(third.candidates[0].admissions.len(), 1);
  assert_eq!(
    *cards.state.lock().unwrap(),
    before,
    "preview never creates/revises/associates/schedules task synchronization"
  );
  let calls = std::fs::read(dir.join("requests.jsonl")).unwrap();
  fixture
    .store
    .publish_integration_project_access(octacity_server_store::testing::management_mutation(
      PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: connection,
          connection_version: IntegrationVersion::INITIAL,
          project_id: fixture.input.project_id(),
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![],
        },
        idempotency_key: IdempotencyKey::new("revoke-preview-read").unwrap(),
        published_at: time(2100),
      },
    ))
    .await
    .unwrap();
  assert!(
    queries
      .handle_query(
        &fixture.profile.context,
        PreviewWorkSourcesQuery {
          intent,
          limit: 2,
          observed_at: time(2200)
        }
      )
      .await
      .is_err()
  );
  assert_eq!(std::fs::read(dir.join("requests.jsonl")).unwrap(), calls);
  assert_eq!(*cards.state.lock().unwrap(), before);
}
struct HideRuns;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for HideRuns {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(
      if resource.kind() == ManagementResourceKind::FactoryRun {
        ManagementVisibility::none()
      } else {
        ManagementVisibility::all()
      },
    ))
  }
}
#[tokio::test]
async fn source_diagnostics_hidden_runs_produce_neither_rows_nor_pagination_hints() {
  let fixture = Fixture::enabled("jira").await;
  fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  assert_eq!(fixture.runs().await.len(), 1);
  let queries = AuthorizedQueryHandler::new(
    Arc::new(HideRuns),
    Arc::new(FactorySourceConfigurationHandlers::new(fixture.store.clone(), Arc::new(HideRuns), vec![]).unwrap()),
  );
  let result = queries
    .handle_query(
      &fixture.profile.context,
      GetWorkSourceDiagnosticsQuery {
        project_id: fixture.input.project_id(),
        target: WorkSourceScanTarget::Input {
          id: fixture.input.id(),
          version: fixture.input.version(),
        },
        after: None,
        limit: 1,
      },
    )
    .await
    .unwrap();
  assert!(result.admissions.is_empty());
  assert!(result.next_cursor.is_none());
}
#[tokio::test]
async fn hosted_preview_can_only_narrow_installed_read_bounds() {
  let mut fixture = Fixture::enabled("jira").await;
  fixture.profile.polling.page_items = 1;
  fixture.profile.polling.max_page_bytes = 4096;
  let dir = fixture.root.path().join("jira");
  let manifest = std::fs::read_to_string(dir.join("adapter.toml")).unwrap();
  std::fs::write(
    dir.join("adapter.toml"),
    manifest.replace("authorize=true", "authorize=true\npreview=true"),
  )
  .unwrap();
  let reader = HostedFactoryWorkSourcePreview::new(
    Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
    vec![fixture.profile.clone()],
    settings().operation_timeout,
    settings().cancellation_grace,
    CancellationToken::new(),
  )
  .unwrap();
  reader
    .preview(ReadWorkSourcePreview {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputIntent {
        id: fixture.input.id(),
        expected_version: None,
        definition: fixture.input.definition().clone(),
      }],
      limit: 100,
    })
    .await
    .unwrap();
  let wire: serde_json::Value = serde_json::from_str(
    std::fs::read_to_string(dir.join("requests.jsonl"))
      .unwrap()
      .lines()
      .next()
      .unwrap(),
  )
  .unwrap();
  assert_eq!(wire["message"]["command"]["payload"]["max_items"], 1);
  assert_eq!(wire["message"]["command"]["payload"]["max_page_bytes"], 4096);
  assert!(wire["message"]["command"]["payload"]["cursor"].is_null());
}
