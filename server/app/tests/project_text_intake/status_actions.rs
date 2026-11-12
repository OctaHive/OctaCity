//! Native local Issue actions use ordinary admission, the node owner and PostgreSQL replay.
use super::*;

#[path = "classification_fields.rs"]
mod classification_fields;

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
  submission: ProjectTextSubmission,
) -> ProjectTextSubmission {
  configure_issue_flow(store, issue, submission, flow(issue)).await
}

async fn configure_issue_flow(
  store: &PostgresStore,
  issue: &IssueRevision,
  mut submission: ProjectTextSubmission,
  flow: FactoryFlowConfiguration,
) -> ProjectTextSubmission {
  let mut f = postgres::factory_configuration::fixture(issue.project_id);
  f.draft.flow = Some(flow);
  let selected = store.factory_admission_context(submission.intent.target).await.unwrap();
  let mut policies = selected.project_policies.unwrap();
  policies.access_profile = octacity_server_store::testing::factory_project_policies_with_access(
    issue.project_id,
    true,
    true,
    f.draft.flow.as_ref().unwrap().limits.permissions().clone(),
  )
  .access_profile;
  octacity_server_store::testing::publish_factory_project_policies_fixture(store, &policies).await;
  f.draft.admission_policy = FactoryPolicySelection::Project {
    project_policy: policies.launch_policy.reference().clone(),
  };
  f.draft.permission_ceiling = FactoryPolicySelection::Project {
    project_policy: policies.access_profile.reference().clone(),
  };
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

fn classification_flow(issue: &IssueRevision) -> FactoryFlowConfiguration {
  let base = flow(issue);
  let key = |value: &str| FactoryKey::new(value).unwrap();
  let root = base.closure.definition(base.closure.root()).unwrap();
  let initial = root.input_schema().unwrap().clone();
  let receipt = root.node(root.entry()).unwrap().outcomes()[0].clone();
  let permission = root.execution().permissions().clone();
  let assessment = FlowDataSchema::new(
    key("assessment"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [
        (
          key("priority"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        ),
        (
          key("size"),
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: vec![serde_json::json!("small")],
          }),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  let observed = serde_json::json!({"priority":80,"size":"small"});
  let outcome = key("classified");
  let gate = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("triage"),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(initial.clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      outcome.clone(),
      FlowOutcomeKind::Success,
      assessment.reference().clone(),
    )],
    budget: BudgetLimit::new(1, 100, 0, 0, 1000).unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_gate(
    FlowGateProgram::new(vec![], outcome.clone())
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant { value: observed }).unwrap())
      .unwrap()
      .binding()
      .unwrap(),
  )
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!({}),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let apply = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("apply"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(assessment.reference().clone()),
    outcomes: vec![receipt.clone()],
    budget: BudgetLimit::new(1, 100, 0, 0, 1000).unwrap(),
    permissions: permission,
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(
    FlowActionBinding::new(
      plugin_identity(),
      key("apply_issue_classification"),
      serde_json::json!({
        "policy":"triage", "source_node":"triage","source_outcome":"classified","outcome":"published",
        "size_labels":{"small":"size_small"}
      }),
    )
    .unwrap(),
  )
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      [(
        key("assessment"),
        FlowInputSource::Record {
          node: gate.key().clone(),
          outcome: outcome.clone(),
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      )]
      .into(),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/assessment".into(),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(initial),
    entry: gate.key().clone(),
    transitions: vec![
      FlowTransition::new(
        gate.key().clone(),
        outcome,
        FlowTransitionTarget::Node(apply.key().clone()),
      ),
      FlowTransition::new(
        apply.key().clone(),
        receipt.key().clone(),
        FlowTransitionTarget::Terminal(receipt.key().clone()),
      ),
    ],
    nodes: vec![gate, apply],
    terminals: vec![FlowTerminalDefinition::new(
      receipt.key().clone(),
      receipt.schema().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: root.execution().clone(),
  })
  .unwrap();
  let mut schemas = base.data_schemas;
  schemas.push(assessment);
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    base.limits,
    schemas,
    Default::default(),
  )
  .unwrap()
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn local_classification_consumes_the_accepted_triage_result_through_the_native_owner() {
  run_local_classification(false).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn local_classification_does_not_write_after_waiting_past_its_deadline() {
  run_local_classification(true).await;
}

async fn run_local_classification(lock_issue: bool) {
  let db = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(db.pool.clone()));
  let (issue, submission) = setup(&store).await;
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition: IssuePropertyDefinitionSet {
          labels: vec![IssuePropertyChoice {
            id: "size_small".into(),
            name: "Small".into(),
            retired: false,
          }],
          properties: vec![],
          ..Default::default()
        },
      },
      idempotency_key: key("local-classification-label"),
      published_at: at(3),
    }))
    .await
    .unwrap();
  let submission = configure_issue_flow(&store, &issue, submission, classification_flow(&issue)).await;
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
  let before = store.factory_run_snapshot(run).await.unwrap();
  let owner: FactoryKey = "local-owner".parse().unwrap();
  let claim = FactoryRunClaimRecord::new(
    run,
    owner,
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
      expected_version: before.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        "factory.claimed".parse().unwrap(),
        FactoryDigest::from_bytes([40; 32]),
        "accepted".parse().unwrap(),
        at(20),
      ),
    })
    .await
    .unwrap();
  let binding = resources(store.clone(), db.pool.clone()).issue_classification_executor(
    plugin_identity(),
    context(),
    Arc::new(TrustedNetworkManagementPolicy),
    clock.clone(),
  );
  let runner = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(FactoryNodeExecutorRegistry::new(vec![binding]).unwrap()),
    clock.clone(),
  );
  let root = before.admitted_flow.root_run();
  let cycle = before.admitted_flow.initial_cycle();
  let definition = before.admitted_flow.closure().definition(root.definition()).unwrap();
  for (offset, node) in ["triage", "apply"].into_iter().enumerate() {
    let time = 21 + offset as i64 * 2;
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    let node: FactoryKey = node.parse().unwrap();
    let schema = snapshot
      .admitted_flow
      .data_schema(definition.node(&node).unwrap().input_schema().unwrap())
      .unwrap();
    let input = FlowNodeInput::prepare(FlowInputPreparation {
      work: &snapshot.work,
      admitted: &snapshot.admitted_flow,
      flow: root,
      cycle,
      node: node.clone(),
      schema,
      history: snapshot.flow.runtime(),
      records: &snapshot.flow.data.records,
      inputs: &snapshot.flow.data.inputs,
      incoming: None,
    })
    .unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: root.id(),
      cycle_id: cycle.id(),
      node,
    };
    clock.0.store(time, Ordering::SeqCst);
    assert_eq!(
      runner
        .run_once(run, claim.id, target.clone(), Some(input), at(time))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    clock.0.store(time + 1, Ordering::SeqCst);
    if lock_issue && offset == 1 {
      let mut lock = db.pool.begin().await.unwrap();
      sqlx::query("SELECT id FROM issues WHERE id=$1 FOR UPDATE")
        .bind(issue.id.as_uuid())
        .execute(&mut *lock)
        .await
        .unwrap();
      let observe = runner.run_once(run, claim.id, target, None, at(time + 1));
      let release = async {
        use std::time::Duration;
        tokio::time::timeout(Duration::from_secs(5), async {
          loop {
            let waiting: bool = sqlx::query_scalar(
              "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() \
               AND wait_event_type='Lock' AND query LIKE 'SELECT id FROM issues%FOR NO KEY UPDATE')",
            )
            .fetch_one(&db.pool)
            .await
            .unwrap();
            if waiting {
              break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
          }
        })
        .await
        .expect("classification must be waiting for the Issue lock");
        tokio::time::sleep(Duration::from_millis(100)).await;
        clock.0.store(101, Ordering::SeqCst);
        lock.commit().await.unwrap();
      };
      let (observed, ()) = tokio::join!(observe, release);
      assert!(observed.is_err());
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
      db.cleanup().await;
      return;
    }
    assert!(matches!(
      runner
        .run_once(run, claim.id, target, None, at(time + 1))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
  }
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.content.priority, Some(80.into()));
  assert_eq!(current.planning.labels, ["size_small".to_owned()].into());
  assert_eq!(current.version, issue.version.next().unwrap());
  assert_eq!(current.status_id, issue.status_id);
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(snapshot.work, before.work);
  assert_eq!(snapshot.outbox, before.outbox);
  assert!(
    store
      .issue_push_projections(
        issue.project_id,
        issue.id,
        &ManagementSecurityScope::trusted_network(),
        None,
        20,
      )
      .await
      .unwrap()
      .is_empty()
  );
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_publication_reuses_project_mapping_and_rejects_substitution() {
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
  let publish_catalog = |expected_version, retired| PublishIssueProperties {
    intent: PublishIssuePropertiesIntent {
      project_id: issue.project_id,
      expected_version,
      definition: IssuePropertyDefinitionSet {
        labels: [("size_small", "Small work", retired), ("unrelated", "Unrelated", false)]
          .into_iter()
          .map(|(id, name, retired)| IssuePropertyChoice {
            id: id.into(),
            name: name.into(),
            retired,
          })
          .collect(),
        properties: vec![],
        ..Default::default()
      },
    },
    idempotency_key: key(&format!("classification-catalog-{expected_version}")),
    published_at: at(3 + expected_version as i64),
  };
  plain
    .publish_issue_properties(management_mutation(publish_catalog(0, false)))
    .await
    .unwrap();
  let mut entries = postgres::factory_configuration::fixture(issue.project_id)
    .choices
    .entries();
  let alias: FactoryKey = "local-issue-classification".parse().unwrap();
  entries.references.push(FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: alias.clone(),
    reference: plugin_identity(),
  });
  let choices = FactoryConfigurationChoices::try_new(entries)
    .unwrap()
    .with_choice_metadata(
      FactoryChoiceKind::NodeCapability,
      alias,
      serde_json::from_value(serde_json::json!({
        "node_kinds":["trusted_action"],"signal_purposes":[],"enforcement":[],"compatible_choices":[],
        "issue_classification":{"policy":"triage","size_labels":{"small":"size_small"}}
      }))
      .unwrap(),
    )
    .unwrap();
  let store = Arc::new(PostgresStore::with_factory_capabilities(
    db.pool.clone(),
    Arc::new(Installed(choices)),
  ));
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let roles = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(
      FactoryRoleProfileHandlers::with_policy(store.clone(), policy.clone()).with_issue_properties(store.clone()),
    ),
  );
  let flow_queries = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy).with_issue_properties(store.clone())),
  );
  let capabilities = GetFactoryAuthoringCapabilitiesQuery {
    project_id: issue.project_id,
  };
  let generic_choices = AuthorizedQueryHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(FactoryConfigurationHandlers::new(store.clone())),
  )
  .handle_query(
    &context(),
    ListFactoryConfigurationChoicesQuery::try_new(issue.project_id, Some(FactoryChoiceKind::NodeCapability), None, 100)
      .unwrap(),
  )
  .await
  .unwrap();
  let writer = generic_choices
    .items
    .iter()
    .find(|item| item.identity.alias.as_str() == "local-issue-classification")
    .unwrap();
  assert!(
    writer.metadata.as_ref().unwrap().issue_classification.is_none(),
    "the generic inventory cannot bypass the current Issue catalog read"
  );
  let offered = roles.handle_query(&context(), capabilities.clone()).await.unwrap();
  assert_eq!(
    serde_json::to_value(offered).unwrap()["issue_classification"]["size_labels"],
    serde_json::json!([{"size":"small","label":"size_small","name":"Small work"}])
  );
  let flow = classification_flow(&issue);
  let mut root = serde_json::to_value(&flow.closure.definitions()[0]).unwrap();
  root.as_object_mut().unwrap().remove("reference");
  let validate = |root: serde_json::Value| ValidateFactoryFlowDefinitionQuery {
    id: FlowDefinitionId::generate(),
    project_id: issue.project_id,
    draft: serde_json::from_value(
      serde_json::json!({"root":root,"limits":flow.limits,"data_schemas":flow.data_schemas}),
    )
    .unwrap(),
  };
  for _ in 0..2 {
    flow_queries
      .handle_query(&context(), validate(root.clone()))
      .await
      .unwrap();
  }
  let mut incompatible = root.clone();
  let nodes = incompatible["nodes"].as_array_mut().unwrap();
  let assessment = nodes.iter().find(|node| node["key"] == "triage").unwrap()["outcomes"][0]["schema"].clone();
  nodes.iter_mut().find(|node| node["key"] == "apply").unwrap()["outcomes"][0]["schema"] = assessment.clone();
  incompatible["terminals"][0]["schema"] = assessment;
  assert!(
    flow_queries
      .handle_query(&context(), validate(incompatible))
      .await
      .is_err(),
    "publication must reject a successful output that cannot carry the action receipt"
  );
  let mut fabricated = root.clone();
  let apply = fabricated["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "apply")
    .unwrap();
  apply["input_binding"]["mapping"]["value"] =
    serde_json::json!({"mapping":"constant","value":{"priority":1,"size":"small"}});
  assert!(
    flow_queries
      .handle_query(&context(), validate(fabricated))
      .await
      .is_err(),
    "a record binding does not authorize substituting a classification value"
  );
  let mut altered = root.clone();
  let action = altered["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "apply")
    .unwrap();
  action["action"]["parameters"]["size_labels"]["small"] = serde_json::json!("unrelated");
  assert!(flow_queries.handle_query(&context(), validate(altered)).await.is_err());
  plain
    .publish_issue_properties(management_mutation(publish_catalog(1, true)))
    .await
    .unwrap();
  let current = serde_json::to_value(roles.handle_query(&context(), capabilities).await.unwrap()).unwrap();
  assert_eq!(current["issue_classification"]["size_labels"], serde_json::json!([]));
  assert_eq!(
    current["issue_classification"]["field_contract"]["selection"]["definition_version"],
    2
  );
  assert!(flow_queries.handle_query(&context(), validate(root)).await.is_err());
}
