//! Native local Issue actions use ordinary admission, the node owner and PostgreSQL replay.
use super::*;

fn plugin_identity() -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new("issue-status").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([37; 32]),
  )
}
fn flow(issue: &IssueRevision) -> FactoryFlowConfiguration {
  let key = |s: &str| FactoryKey::new(s).unwrap();
  let receipt = FlowDataSchema::new(
    key("local.receipt"),
    key("v1"),
    FlowValueSchema::Object {
      fields: std::collections::BTreeMap::from([(
        key("receipt"),
        FlowFieldSchema::required(FlowValueSchema::String {
          min_bytes: 64,
          max_bytes: 64,
        }),
      )]),
    },
  )
  .unwrap();
  let input = FlowDataSchema::new(
    key("local.input"),
    key("v1"),
    FlowValueSchema::Object {
      fields: Default::default(),
    },
  )
  .unwrap();
  let outcome = FlowOutcomeDefinition::new(key("published"), FlowOutcomeKind::Success, receipt.reference().clone());
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin_identity()],
    ..Default::default()
  })
  .unwrap();
  let budget = BudgetLimit::new(4, 1000, 100, 100, 10000).unwrap();
  let selection = IssuePipelineStatusSelection {
    workflow_id: issue.workflow_id,
    workflow_version: issue.workflow_version,
    transition_id: "20000000-0000-4000-8000-000000000001".parse().unwrap(),
    outcome: key("published"),
  };
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("local_move"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(input.reference().clone()),
    outcomes: vec![outcome],
    budget: BudgetLimit::new(1, 500, 10, 10, 1000).unwrap(),
    permissions: permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(
    FlowActionBinding::new(
      plugin_identity(),
      key("transition_issue"),
      serde_json::to_value(selection).unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(input.reference().clone()),
    entry: key("local_move"),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("local_move"),
      key("published"),
      FlowTransitionTarget::Terminal(key("published")),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      key("published"),
      receipt.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, permissions).unwrap(),
    vec![input, receipt],
    Default::default(),
  )
  .unwrap()
}
async fn status_setup(store: &PostgresStore) -> (IssueRevision, ProjectTextSubmission) {
  let (issue, submission) = setup(store).await;
  let submission = configure_status(store, &issue, submission).await;
  (issue, submission)
}
pub(super) async fn configure_status(
  store: &PostgresStore,
  issue: &IssueRevision,
  mut submission: ProjectTextSubmission,
) -> ProjectTextSubmission {
  let mut f = postgres::factory_configuration::fixture(issue.project_id);
  f.draft.flow = Some(flow(issue));
  let target = &mut submission.intent.target;
  let digest = FactoryDigest::from_bytes([38; 32]);
  let version = FactoryConfigurationVersion::new(2).unwrap();
  let configuration = FactoryConfiguration::publish(
    target.configuration_id,
    version,
    issue.project_id,
    digest,
    f.draft.clone(),
    &f.choices,
  )
  .unwrap();
  store
    .replace_factory_configuration(management_mutation(ReplaceFactoryConfiguration {
      expected_current_version: target.configuration_version,
      configuration,
      idempotency_key: key("local-flow"),
      published_at: at(3),
      intent: FactoryConfigurationMutationIntent::Replace {
        id: target.configuration_id,
        expected_current_version: target.configuration_version,
        definition_digest: digest,
        draft: f.draft,
      },
    }))
    .await
    .unwrap();
  target.configuration_version = version;
  submission
}

struct LostActionAcknowledgement {
  inner: FactoryFlowActionAdapter,
}
#[async_trait::async_trait]
impl FactoryNodeExecutor for LostActionAcknowledgement {
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    self.inner.cancel(request, at).await
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    self.inner.observe(request).await?;
    Err(ApplicationError::unavailable())
  }
}
struct CurrentRights(AtomicBool);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for CurrentRights {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if self.0.load(Ordering::SeqCst)
      && action == ManagementAction::Publish
      && resource.kind() == ManagementResourceKind::Issue
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}

fn resources(store: Arc<PostgresStore>, pool: sqlx::PgPool) -> octacity_server::FactoryRuntimeResources {
  let objects = octacity_artifact_s3::S3ArtifactStore::new(octacity_artifact_s3::S3ArtifactStoreConfig {
    endpoint: "http://127.0.0.1:9".into(),
    region: "local".into(),
    bucket: "status-test".into(),
    prefix: "local".into(),
    access_key: zeroize::Zeroizing::new("fixture-only".into()),
    secret_key: zeroize::Zeroizing::new("fixture-only".into()),
    force_path_style: true,
    operation_timeout: std::time::Duration::from_secs(1),
    capability_recheck_interval: std::time::Duration::from_secs(1),
  })
  .unwrap();
  let spec: octacity_protocol::JobSpecV3 = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  ))
  .unwrap();
  octacity_server::FactoryRuntimeResources {
    runs: store.clone(),
    builds: Arc::new(PostgresAuthoritativeStore::new(pool, postgres::test_signer())),
    objects: Arc::new(objects),
    revisions: Arc::new(Resolver(AtomicUsize::new(0))),
    toolchain: JobSpecToolchainPolicy {
      source: octacity_server_job::SourcePluginPolicy::new("git", "0.1.0", spec.source.plugin_sha256, "url").unwrap(),
      octa: spec.octa,
      validity: octacity_server_job::JobSpecValidity::new(3600).unwrap(),
    },
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_native_owner_recovers_lost_ack_after_edits_and_rechecks_current_authority() {
  let db = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(db.pool.clone()));
  let (issue, submission) = status_setup(&store).await;
  let clock = Arc::new(Clock(AtomicI64::new(11)));
  let request = reviewed_request(&store, &issue, &submission).await;
  let linked = publisher(
    store.clone(),
    Arc::new(Bytes::new()),
    Arc::new(Resolver(AtomicUsize::new(0))),
    clock.clone(),
  )
  .handle_command(
    &context(),
    PublishIssueExecutionCommand {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: request.id,
      classification: submission.intent.classification,
    },
  )
  .await
  .unwrap();
  let run = linked.association.unwrap().run_id;
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let claim = FactoryRunClaimRecord::new(
    run,
    FactoryKey::new("local-owner").unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([39; 32])),
      at(20),
      at(100),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        FactoryDigest::from_bytes([40; 32]),
        FactoryKey::new("accepted").unwrap(),
        at(20),
      ),
    })
    .await
    .unwrap();
  let root = snapshot.admitted_flow.root_run();
  let cycle = snapshot.admitted_flow.initial_cycle();
  let definition = snapshot.admitted_flow.closure().definition(root.definition()).unwrap();
  let schema = snapshot
    .admitted_flow
    .data_schema(definition.node(definition.entry()).unwrap().input_schema().unwrap())
    .unwrap();
  let input = FlowNodeInput::new(
    &snapshot.work,
    &snapshot.admitted_flow,
    root,
    cycle,
    definition.entry().clone(),
    FlowPayload::new(schema, serde_json::json!({})).unwrap(),
  )
  .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  let rights = Arc::new(CurrentRights(AtomicBool::new(false)));
  let action = Arc::new(FactoryIssueStatusAction::new(
    plugin_identity(),
    store.clone(),
    store.clone(),
    context(),
    rights.clone(),
    clock.clone(),
  ));
  let runner = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(LostActionAcknowledgement {
      inner: FactoryFlowActionAdapter::new(action),
    }),
    clock.clone(),
  );
  clock.0.store(21, Ordering::SeqCst);
  assert!(matches!(
    runner
      .run_once(run, claim.id, target.clone(), Some(input), at(21))
      .await
      .unwrap(),
    FactoryNodeStep::Advanced
  ));
  clock.0.store(22, Ordering::SeqCst);
  assert!(
    runner
      .run_once(run, claim.id, target.clone(), None, at(22))
      .await
      .is_err()
  );
  let moved = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(moved.status_id.to_string(), "10000000-0000-4000-8000-000000000002");
  assert_eq!(
    store
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  assert!(
    store
      .factory_run_snapshot(run)
      .await
      .unwrap()
      .flow
      .completions
      .is_empty()
  );
  let edited = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: moved.version,
        content: IssueContent {
          description: "Later requirements".into(),
          ..moved.content
        },
      },
      idempotency_key: key("later-edit"),
      edited_at: at(23),
    }))
    .await
    .unwrap()
    .issue;
  let restarted = Arc::new(PostgresStore::new(db.pool.clone()));
  let binding = resources(restarted.clone(), db.pool.clone()).issue_status_executor(
    plugin_identity(),
    context(),
    rights.clone(),
    clock.clone(),
  );
  assert_eq!(binding.0, FlowNodeKind::TrustedAction);
  let runner = FactoryNodeRunner::new(
    restarted.clone(),
    Arc::new(FactoryNodeExecutorRegistry::new(vec![binding]).unwrap()),
    clock.clone(),
  );
  rights.0.store(true, Ordering::SeqCst);
  clock.0.store(24, Ordering::SeqCst);
  assert!(
    runner
      .run_once(run, claim.id, target.clone(), None, at(24))
      .await
      .is_err()
  );
  assert!(
    restarted
      .factory_run_snapshot(run)
      .await
      .unwrap()
      .flow
      .completions
      .is_empty()
  );
  rights.0.store(false, Ordering::SeqCst);
  clock.0.store(25, Ordering::SeqCst);
  let FactoryNodeStep::Completed(record) = runner
    .run_once(run, claim.id, target.clone(), None, at(25))
    .await
    .unwrap()
  else {
    panic!("original accepted action must resume")
  };
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), edited);
  let history = restarted
    .issue_status_history(issue.project_id, issue.id, None, 20)
    .await
    .unwrap();
  assert_eq!(history.items.len(), 2);
  let origin = history.items[1].pipeline.as_ref().unwrap();
  assert_eq!(origin.node_attempt_id, record.node_attempt_id());
  let sealed = restarted
    .issue_status_command(
      issue.project_id,
      issue.id,
      &ManagementSecurityScope::trusted_network(),
      origin.operation_id,
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(sealed.intent.expected_version, issue.version);
  assert_eq!(sealed.moved_at, at(22));
  assert_eq!(
    record.payload().value(),
    &serde_json::json!({"receipt":origin.operation_id})
  );
  clock.0.store(26, Ordering::SeqCst);
  assert_eq!(
    runner.run_once(run, claim.id, target, None, at(26)).await.unwrap(),
    FactoryNodeStep::Completed(record)
  );
  assert_eq!(
    restarted
      .factory_run_snapshot(run)
      .await
      .unwrap()
      .flow
      .completions
      .len(),
    1
  );
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_status_flow_authoring_preserves_exact_pins_and_stale_publication_preconditions() {
  use octacity_server_store_postgres::FactoryCapabilitySource;
  struct Installed(FactoryConfigurationChoices);
  #[async_trait::async_trait]
  impl FactoryCapabilitySource for Installed {
    async fn availability(&self, _: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
      Ok(FactoryConfigurationAvailability::Available(self.0.clone()))
    }
  }
  let db = postgres::TestDatabase::migrated().await;
  let plain = PostgresStore::new(db.pool.clone());
  let (issue, _) = setup(&plain).await;
  let flow = flow(&issue);
  let mut entries = postgres::factory_configuration::fixture(issue.project_id)
    .choices
    .entries();
  entries.references.push(FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: "local-issue-status".parse().unwrap(),
    reference: plugin_identity(),
  });
  let store = Arc::new(PostgresStore::with_factory_capabilities(
    db.pool.clone(),
    Arc::new(Installed(FactoryConfigurationChoices::try_new(entries).unwrap())),
  ));
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let workflows = Arc::new(AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(IssueWorkflowHandlers::new(store.clone())),
  ));
  let handlers =
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy.clone()).with_issue_status_actions(workflows));
  let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
  let commands = AuthorizedCommandHandler::new(policy, handlers);
  let mut root = serde_json::to_value(&flow.closure.definitions()[0]).unwrap();
  root.as_object_mut().unwrap().remove("reference");
  let draft: FactoryFlowDefinitionDraft =
    serde_json::from_value(serde_json::json!({"root":root,"limits":flow.limits,"data_schemas":flow.data_schemas}))
      .unwrap();
  let id = FlowDefinitionId::generate();
  let validation = ValidateFactoryFlowDefinitionQuery {
    id,
    project_id: issue.project_id,
    draft: draft.clone(),
  };
  let validated = queries.handle_query(&context(), validation.clone()).await.unwrap();
  assert_eq!(
    validated.closure.definitions()[0].nodes()[0]
      .action()
      .unwrap()
      .parameters()["workflow_version"],
    1
  );
  let created = commands
    .handle_command(
      &context(),
      CreateFactoryFlowDefinitionCommand {
        id,
        project_id: issue.project_id,
        draft: draft.clone(),
        idempotency_key: key("create-local-flow"),
        published_at: at(5),
      },
    )
    .await
    .unwrap();
  assert_eq!(created.disposition, MutationDisposition::Applied);
  let replace = ReplaceFactoryFlowDefinitionCommand {
    id,
    expected_current_version: FlowDefinitionVersion::INITIAL,
    draft: draft.clone(),
    idempotency_key: key("replace-local-flow"),
    published_at: at(6),
  };
  commands.handle_command(&context(), replace.clone()).await.unwrap();
  let mut stale = replace;
  stale.idempotency_key = key("stale-local-flow");
  assert_eq!(
    commands
      .handle_command(&context(), stale)
      .await
      .unwrap_err()
      .application()
      .unwrap()
      .classification(),
    ApplicationFailure::PreconditionFailed
  );
  let mut invalid = validation;
  invalid.draft.root.nodes[0] = invalid.draft.root.nodes[0].clone().with_action(FlowActionBinding::new(plugin_identity(), "transition_issue".parse().unwrap(), serde_json::json!({"workflow_id":issue.workflow_id,"workflow_version":1,"transition_id":"20000000-0000-4000-8000-000000000001","outcome":"model prose"})).unwrap()).unwrap();
  assert!(queries.handle_query(&context(), invalid).await.is_err());
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  db.cleanup().await;
}

pub(super) async fn qualify_status(store: Arc<PostgresStore>, run: FactoryRunId, issue: &IssueRevision) {
  let now = SystemFactoryExecutionClock.now().unwrap().unix_millis();
  let clock = Arc::new(Clock(AtomicI64::new(now)));
  let claim = pilot_controls::reclaim(&store, run, now).await;
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let admitted = &snapshot.admitted_flow;
  let definition = &admitted.closure().definitions()[0];
  let node = definition.node(definition.entry()).unwrap();
  let schema = admitted.data_schema(node.input_schema().unwrap()).unwrap();
  let input = FlowNodeInput::new(
    &snapshot.work,
    admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    node.key().clone(),
    FlowPayload::new(schema, serde_json::json!({})).unwrap(),
  )
  .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  let action = || {
    Arc::new(FactoryIssueStatusAction::new(
      plugin_identity(),
      store.clone(),
      store.clone(),
      context(),
      Arc::new(TrustedNetworkManagementPolicy),
      clock.clone(),
    ))
  };
  let runner = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(LostActionAcknowledgement {
      inner: FactoryFlowActionAdapter::new(action()),
    }),
    clock.clone(),
  );
  runner
    .run_once(run, claim.id, target.clone(), Some(input), at(now))
    .await
    .unwrap();
  assert!(
    runner
      .run_once(run, claim.id, target.clone(), None, at(now))
      .await
      .is_err()
  );
  let moved = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(moved.status_id.to_string(), "10000000-0000-4000-8000-000000000002");
  let recovered = FactoryNodeRunner::new(store.clone(), Arc::new(FactoryFlowActionAdapter::new(action())), clock);
  assert!(matches!(
    recovered.run_once(run, claim.id, target, None, at(now)).await.unwrap(),
    FactoryNodeStep::Completed(_)
  ));
  assert_eq!(
    store
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), moved);
}
