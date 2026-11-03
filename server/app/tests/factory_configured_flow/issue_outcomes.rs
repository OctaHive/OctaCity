//! Current Issue summaries read ordinary, committed Flow results.
use super::*;
use octacity_server_store::testing::management_mutation;
use serde_json::json;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_outcomes_read_exact_current_accepted_records() {
  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = factory_node_test_support::fixtures::work();
  let project_id = fixture.subject().project_id();
  let issue_id = IssueId::generate();
  let content = requirements();
  let submission = ProjectDocumentSubmission {
    project_id,
    name: "Reviewed acceptance".parse().unwrap(),
    content: "A retained specification.".into(),
    idempotency_key: "outcome-document".parse().unwrap(),
  };
  let document = ProjectDocumentPreparation::reserve(
    &submission,
    octacity_server_store::ManagementSecurityScope::trusted_network(),
    at(1),
  )
  .unwrap()
  .objects[0]
    .artifact
    .clone();
  let original = IssueExecutionRequestId::generate();
  let (admission, execution) =
    flow_contract::configured_fixture_with_work(summary_flow(Some(&document)), work_for(&fixture, original, &content));
  let store = admit(&database.pool, &admission).await;
  let reserved = store
    .reserve_project_document(management_mutation(ReserveProjectDocument { submission, now: at(1) }))
    .await
    .unwrap();
  store
    .publish_project_document(management_mutation(reserved.guard(at(2))))
    .await
    .unwrap();
  let created = create_issue(&store, project_id, issue_id, content.clone()).await;
  let other = create_issue(&store, project_id, IssueId::generate(), content.clone()).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let read = || read_outcomes(&store, policy.clone(), project_id, issue_id);
  assert!(read().await.is_empty(), "ordinary Issues may have no result");
  associate(&store, &created, Some(original), &admission).await;
  assert!(read().await.is_empty(), "admission alone supplies no result");

  finish(&store, &execution, &admission, 30).await;
  let original_cards = read().await;
  assert_cards(&original_cards, &admission);
  assert_eq!(
    original_cards[0].links,
    vec![IssueOutcomeLink {
      title: Some("Reviewed acceptance".into()),
      target: IssueOutcomeLinkTarget::Document(document.artifact_id()),
    }]
  );
  assert!(original_cards[1].links.is_empty());

  let old_history = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let accepted: Vec<_> = old_history
    .flow
    .data
    .records
    .iter()
    .filter(|record| record.outcome().as_str() == "custom")
    .collect();
  assert_eq!(accepted.len(), 2, "the same slot was produced in two Flow occurrences");
  assert_eq!(
    original_cards[0].summary.node_attempt_id,
    accepted
      .iter()
      .max_by_key(|record| record.observed_at())
      .unwrap()
      .node_attempt_id(),
  );

  let late = admit_next(&store, &admission, &created, 2, true).await;
  assert_eq!(
    read().await,
    original_cards,
    "a newer pending Run cannot replace a card"
  );
  let newer = admit_next(&store, &admission, &created, 3, true).await;
  finish(&store, &execution, &newer, 80).await;
  let current = read().await;
  assert_cards(&current, &newer);
  finish(&store, &execution, &late, 130).await;
  assert_eq!(
    read().await,
    current,
    "a later completion cannot outrank a newer admission"
  );

  let legacy = admit_next(&store, &admission, &created, 4, false).await;
  finish(&store, &execution, &legacy, 180).await;
  assert_eq!(read().await, current, "legacy Work has no frozen requirements binding");
  assert_http_outcomes(&database, &store, &created, &other, &newer).await;
  assert!(
    read_outcomes(&store, policy.clone(), project_id, other.id)
      .await
      .is_empty()
  );

  let restricted = Arc::new(OutcomeVisibility {
    run: late.run.id(),
    hide_project: false,
  });
  assert_cards(&read_outcomes(&store, restricted, project_id, issue_id).await, &late);
  let project_hidden = Arc::new(OutcomeVisibility {
    run: late.run.id(),
    hide_project: true,
  });
  assert!(
    read_outcomes(&store, project_hidden, project_id, issue_id)
      .await
      .is_empty(),
    "the owning Project must remain visible independently of Issue and Run grants"
  );

  let instance_policy = Arc::new(DenyRunInstance(newer.run.id()));
  assert_cards(
    &read_outcomes(&store, instance_policy, project_id, issue_id).await,
    &late,
  );

  let planned = edit_issue(
    &store,
    &created,
    IssueContent {
      priority: Some(25),
      ..content.clone()
    },
    230,
  )
  .await;
  assert_eq!(
    read().await,
    current,
    "priority changes preserve requirements applicability"
  );
  for time in 231..243 {
    let hidden = admit_next(&store, &admission, &planned, time, true).await;
    finish(&store, &execution, &hidden, 300 + time * 50).await;
  }
  let instance_only = Arc::new(OnlyRunInstance(late.run.id()));
  assert_cards(&read_outcomes(&store, instance_only, project_id, issue_id).await, &late);
  edit_issue(
    &store,
    &planned,
    IssueContent {
      description: "New requirements.".into(),
      ..content
    },
    20_000,
  )
  .await;
  assert!(read().await.is_empty(), "superseded requirements exclude old summaries");
  assert_eq!(
    store
      .factory_run_snapshot(admission.run.id())
      .await
      .unwrap()
      .flow
      .data
      .records,
    old_history.flow.data.records
  );
  database.cleanup().await;
}

async fn assert_http_outcomes(
  database: &postgres_support::TestDatabase,
  store: &PostgresStore,
  issue: &IssueRevision,
  other: &IssueRevision,
  admission: &PublishedFactoryAdmission,
) {
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let server = octacity_server::ServerRuntime::start(runtime_fixture::runtime_config(
    root.path(),
    url.as_str(),
    "http://127.0.0.1:9",
  ))
  .await
  .unwrap();
  let client = reqwest::Client::new();
  let get = |project, issue| {
    client.get(format!(
      "http://{}/api/v1/projects/{project}/issues/{issue}/outcomes",
      server.management_addr()
    ))
  };
  let before = store
    .factory_run_snapshot(admission.run.id())
    .await
    .unwrap()
    .flow
    .data
    .records;
  let response = get(issue.project_id, issue.id).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let body: serde_json::Value = response.json().await.unwrap();
  let items = body["items"].as_array().unwrap();
  assert_eq!(items.len(), 2);
  for item in items {
    assert_eq!(item["run_id"], admission.run.id().to_string());
  }
  assert_eq!(
    items[0]["slot"],
    format!("custom:{}:retain_outcome:custom", admission.flow.closure().root().id())
  );
  assert_eq!(items[0]["conclusion"], "The requirements are ready.");
  assert_eq!(
    items[0]["presentation"],
    json!({"kind":"custom","title":"Acceptance checks"})
  );
  assert_eq!(
    items[0]["links"],
    json!([{"kind":"unavailable","title":"Reviewed acceptance"}])
  );
  assert_eq!(items[1]["slot"], "triage");
  assert_eq!(
    items[1]["presentation"],
    json!({"kind":"triage","decision":"eligible","recommended_priority":25,"recommended_size":"small","classification":{"status":"recommended"}})
  );
  assert_eq!(items[1]["links"], json!([]));
  let response = get(other.project_id, other.id).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.json::<serde_json::Value>().await.unwrap(), json!({"items":[]}));
  assert_eq!(
    get(ProjectId::generate(), issue.id).send().await.unwrap().status(),
    reqwest::StatusCode::NOT_FOUND
  );
  assert_eq!(
    store.issue(issue.project_id, issue.id, None).await.unwrap().version,
    issue.version
  );
  assert_eq!(
    store
      .factory_run_snapshot(admission.run.id())
      .await
      .unwrap()
      .flow
      .data
      .records,
    before
  );
  server.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_outcomes_ignore_legacy_flows_with_accepted_results_but_no_presentation() {
  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = factory_node_test_support::fixtures::work();
  let project = fixture.subject().project_id();
  let request = IssueExecutionRequestId::generate();
  let content = requirements();
  let mut flow = summary_flow(None);
  for outcome in flow.nodes.iter_mut().flat_map(|node| &mut node.outcomes) {
    outcome.issue_presentation = None;
  }
  let (admission, execution) = flow_contract::configured_fixture_with_work(flow, work_for(&fixture, request, &content));
  let store = admit(&database.pool, &admission).await;
  let issue = create_issue(&store, project, IssueId::generate(), content).await;
  associate(&store, &issue, Some(request), &admission).await;
  finish(&store, &execution, &admission, 30).await;
  assert!(
    read_outcomes(&store, Arc::new(TrustedNetworkManagementPolicy), project, issue.id)
      .await
      .is_empty()
  );
  database.cleanup().await;
}

fn requirements() -> IssueContent {
  IssueContent {
    title: "Review the project".into(),
    description: "Check the current requirements.".into(),
    acceptance: Some("Record a short conclusion.".into()),
    priority: None,
  }
}

fn summary_flow(document: Option<&FactoryArtifactReference>) -> FlowDefinitionTemplate {
  let mut template = factory_node_test_support::fixtures::supplied_flow_template("issue-outcomes-flow.json");
  template.schemas.get_mut(&key("producer_result")).unwrap().shape = serde_json::from_value(json!({
    "type":"object",
    "fields":{
      "conclusion":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":1024}},
      "decision":{"required":true,"value":{"type":"enum","values":["eligible"]}},
      "priority":{"required":true,"value":{"type":"integer","minimum":0,"maximum":100}},
      "size":{"required":true,"value":{"type":"enum","values":["small"]}},
      "custom":{"required":true,"value":{"type":"reference","schema":"custom"}},
      "review":{"required":false,"value":{"type":"reference","schema":"review"}},
      "specifications":{"required":false,"value":{"type":"reference","schema":"specifications"}}
    }
  }))
  .unwrap();
  if document.is_some() {
    let mut shape = serde_json::to_value(&template.schemas[&key("custom")].shape).unwrap();
    shape["fields"]["as_artifact"] = json!({"required":true,"value":{"type":"reference","schema":"document"}});
    shape["fields"]["document"] = json!({"required":true,"value":{"type":"reference","schema":"document"}});
    template.schemas.get_mut(&key("custom")).unwrap().shape = serde_json::from_value(shape).unwrap();
    let outcome = template
      .nodes
      .iter_mut()
      .flat_map(|node| &mut node.outcomes)
      .find(|outcome| outcome.key == key("custom"))
      .unwrap();
    let Some(FlowIssuePresentation::CustomV1 { references, .. }) = &mut outcome.issue_presentation else {
      panic!("expected custom presentation");
    };
    references.push(FlowIssueReferenceBinding {
      field: key("as_artifact"),
      kind: FlowIssueReferenceKind::Artifact,
    });
    references.push(FlowIssueReferenceBinding {
      field: key("document"),
      kind: FlowIssueReferenceKind::Document,
    });
  }
  let mut custom = json!({"conclusion":"The requirements are ready."});
  if let Some(document) = document {
    custom["document"] = json!(document);
    custom["as_artifact"] = json!(document);
  }
  let producer = template
    .nodes
    .iter_mut()
    .find(|node| node.key == template.entry)
    .unwrap();
  producer.outcomes[0].issue_presentation = Some(FlowIssuePresentation::TriageV1 { slot: None });
  producer.gate = Some(
    FlowGateProgram::new(vec![], producer.outcomes[0].key.clone())
      .unwrap()
      .with_default_output_mapping(
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: json!({
            "conclusion":"The project context is ready.",
            "decision":"eligible",
            "priority":25,
            "size":"small",
            "custom":custom
          }),
        })
        .unwrap(),
      )
      .unwrap(),
  );
  let route = template
    .transitions
    .iter_mut()
    .find(|edge| edge.outcome() == &key("custom"))
    .unwrap();
  *route = FlowTransition::repeated(
    key("retain_outcome"),
    key("custom"),
    FlowTransitionTarget::Node(template.entry.clone()),
    1,
    key("custom"),
  )
  .unwrap();
  template.execution = FlowExecutionPolicy::new(
    BudgetLimit::new(4, 4000, 0, 0, 32768).unwrap(),
    template.execution.permissions().clone(),
    1,
  )
  .unwrap();
  template
}

fn work_for(fixture: &WorkEnvelope, request: IssueExecutionRequestId, content: &IssueContent) -> WorkEnvelope {
  let artifact = |bytes: &[u8]| {
    FactoryArtifactReference::new(
      ArtifactId::generate(),
      FactoryDigest::content_sha256(bytes),
      bytes.len() as u64,
    )
    .unwrap()
  };
  WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    fixture.configuration().clone(),
    format!("issue-execution/{request}").parse().unwrap(),
    fixture.subject().clone(),
    WorkArtifacts::new(
      artifact(&IssueExecutionMapping::TitleDescriptionAcceptanceV1.task_bytes(content)),
      artifact(content.acceptance.as_ref().unwrap().as_bytes()),
      vec![],
    )
    .unwrap(),
    WorkClassification::new(fixture.priority(), fixture.risk(), Default::default()),
  )
  .unwrap()
}

async fn admit_next(
  store: &PostgresStore,
  original: &PublishedFactoryAdmission,
  issue: &IssueRevision,
  time: i64,
  reviewed: bool,
) -> PublishedFactoryAdmission {
  let request = IssueExecutionRequestId::generate();
  let work = work_for(&original.work, request, &issue.content);
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, original.flow.closure().root()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    original.flow.closure().clone(),
    original.flow.limits().clone(),
    root,
    cycle,
    original.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(original.flow.data_schemas().to_vec())
  .unwrap()
  .with_pool_settings(original.flow.pool_settings().clone())
  .unwrap();
  let admission = store
    .admit_factory_work(management_mutation(AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: key("manual"),
          security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
        },
        external_identity: work.external_identity().clone(),
        intent_digest: FactoryDigest::content_sha256(request.as_uuid().as_bytes()),
        idempotency_key: request.to_string().parse().unwrap(),
      },
      repository_version: Some(RepositoryVersion::INITIAL),
      work,
      run,
      flow,
      admitted_at: at(time),
    }))
    .await
    .unwrap()
    .admission;
  associate(store, issue, reviewed.then_some(request), &admission).await;
  admission
}

async fn associate(
  store: &PostgresStore,
  issue: &IssueRevision,
  request: Option<IssueExecutionRequestId>,
  admission: &PublishedFactoryAdmission,
) {
  if let Some(id) = request {
    store
      .request_issue_execution(management_mutation(RequestIssueExecution {
        id,
        intent: RequestIssueExecutionIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          expected_version: issue.version,
          target: IssueExecutionTarget {
            configuration_id: admission.work.configuration().id(),
            configuration_version: admission.work.configuration().version(),
            repository_id: admission.work.subject().repository().unwrap().repository_id(),
            repository_version: RepositoryVersion::INITIAL,
            source: RepositorySourceSelection::DefaultReference,
            mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
          },
          classification: None,
        },
        idempotency_key: id.to_string().parse().unwrap(),
        requested_at: admission.admitted_at,
      }))
      .await
      .unwrap();
  }
  store
    .associate_issue_work(management_mutation(AssociateIssueWork {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: request,
      admission: admission.clone(),
    }))
    .await
    .unwrap();
}

async fn finish(
  store: &Arc<PostgresStore>,
  execution: &Arc<flow_contract::Execution>,
  admission: &PublishedFactoryAdmission,
  start: i64,
) {
  let run = admission.run.id();
  let claim = FactoryRunClaimRecord::new(
    run,
    key("outcomes-worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([42; 32])),
      at(start),
      at(start + 100),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        claim.id,
        key("claimed"),
        at(start),
      ),
    })
    .await
    .unwrap();
  let runner = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
    Arc::new(flow_contract::Inputs),
    factory_node_test_support::clock(),
  );
  for time in start + 1..start + 25 {
    match runner.reconcile_flow(run, claim.id, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        assert_eq!(outcome, key("custom"));
        return;
      }
      other => panic!("unexpected result {other:?}"),
    }
  }
  panic!("bounded Flow must resolve");
}

async fn create_issue(
  store: &PostgresStore,
  project_id: ProjectId,
  id: IssueId,
  content: IssueContent,
) -> IssueRevision {
  store
    .create_issue(management_mutation(CreateIssue {
      id,
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id,
        planning: Default::default(),
        content,
      },
      idempotency_key: id.to_string().parse().unwrap(),
      created_at: at(1),
    }))
    .await
    .unwrap()
    .issue
}

async fn edit_issue(store: &PostgresStore, issue: &IssueRevision, content: IssueContent, time: i64) -> IssueRevision {
  store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content,
      },
      idempotency_key: format!("edit-{time}").parse().unwrap(),
      edited_at: at(time),
    }))
    .await
    .unwrap()
    .issue
}

async fn read_outcomes(
  store: &Arc<PostgresStore>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  project_id: ProjectId,
  issue_id: IssueId,
) -> Vec<octacity_server_application::IssueOutcomeCard> {
  let clock = Arc::new(OutcomeClock);
  let reviews = Arc::new(IssueCodeReviewHandlers::new(
    store.clone(),
    Arc::new(DocumentOnlyReaders),
    policy.clone(),
    clock.clone(),
  ));
  let readers = IssueOutcomeLinkReaders::new(store.clone(), Arc::new(DocumentOnlyReaders), reviews, clock);
  AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(IssueOutcomeHandlers::new(store.clone(), policy, readers)),
  )
  .handle_query(
    &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
    GetIssueOutcomesQuery { project_id, issue_id },
  )
  .await
  .unwrap()
}

fn assert_cards(cards: &[octacity_server_application::IssueOutcomeCard], admission: &PublishedFactoryAdmission) {
  let [result, preparation] = cards else {
    panic!("expected two coexisting slots, got {cards:?}")
  };
  assert_eq!(result.summary.conclusion, "The requirements are ready.");
  assert_eq!(preparation.summary.conclusion, "The project context is ready.");
  assert_eq!(
    result.summary.slot,
    FlowIssueSlot::Custom {
      flow: admission.flow.closure().root().id(),
      node: key("retain_outcome"),
      outcome: key("custom"),
    },
  );
  assert_eq!(preparation.summary.slot, FlowIssueSlot::Named(key("triage")));
  assert_eq!(
    preparation.summary.content,
    IssueOutcomeContent::Triage {
      decision: key("eligible"),
      duplicate_coverage: None,
      priority: Some(25),
      size: Some(key("small")),
      component: None,
      classification: octacity_server_application::IssueOutcomeClassification::Recommended,
    },
  );
  for card in cards {
    assert_eq!(card.summary.run_id, admission.run.id());
    assert_eq!(card.summary.work_id, admission.work.id());
  }
}

struct OutcomeVisibility {
  run: FactoryRunId,
  hide_project: bool,
}
#[async_trait]
impl ManagementAuthorizationPolicy for OutcomeVisibility {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    match resource.kind() {
      ManagementResourceKind::Project if self.hide_project => Err(ManagementAuthorizationDenial::forbidden()),
      ManagementResourceKind::FactoryRun => Ok(ManagementAuthorizationGrant::new(
        ManagementVisibility::restricted([ManagementResource::instance(
          ManagementResourceKind::FactoryRun,
          ManagementResourceIdentity::new(self.run.to_string()).unwrap(),
        )
        .unwrap()])
        .unwrap(),
      )),
      _ => Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all())),
    }
  }
}

struct DenyRunInstance(FactoryRunId);
#[async_trait]
impl ManagementAuthorizationPolicy for DenyRunInstance {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let denied = ManagementResource::instance(
      ManagementResourceKind::FactoryRun,
      ManagementResourceIdentity::new(self.0.to_string()).unwrap(),
    )
    .unwrap();
    if resource == &denied {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

struct OnlyRunInstance(FactoryRunId);
#[async_trait]
impl ManagementAuthorizationPolicy for OnlyRunInstance {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::FactoryRun
      && let Some(id) = resource.identity()
      && *id != ManagementResourceIdentity::new(self.0.to_string()).unwrap()
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

struct OutcomeClock;
impl FactoryExecutionClock for OutcomeClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(30))
  }
}

struct DocumentOnlyReaders;
#[async_trait]
impl IssueArtifactReader for DocumentOnlyReaders {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    panic!("document links must use the Project document owner")
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("outcome links must not read bytes")
  }
}
#[async_trait]
impl IssueCodeReviewObservations for DocumentOnlyReaders {
  async fn visibility(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
  ) -> Result<IssueCodeReviewRepositoryVisibility, ApplicationError> {
    panic!("no review selected")
  }
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    panic!("no review selected")
  }
  async fn read(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    panic!("outcome links must not read providers")
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_outcomes_http_distinguishes_advice_committed_classification_and_conflict() {
  use octacity_server_store::{
    ApplyIssueClassification, EditIssuePlanning, EditIssuePlanningIntent, IssueClassificationStore,
    IssueExecutionStore as _, IssuePropertyChoice, IssuePropertyDefinitionSet, IssuePropertyStore,
    PublishIssueProperties, PublishIssuePropertiesIntent,
  };

  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = factory_node_test_support::fixtures::work();
  let project = fixture.subject().project_id();
  let request = IssueExecutionRequestId::generate();
  let content = requirements();
  let (first, execution) =
    flow_contract::configured_fixture_with_work(summary_flow(None), work_for(&fixture, request, &content));
  let store = admit(&database.pool, &first).await;
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: project,
        expected_version: 0,
        definition: IssuePropertyDefinitionSet {
          labels: vec![IssuePropertyChoice {
            id: "size_small".into(),
            name: "Small work".into(),
            retired: false,
          }],
          properties: vec![],
        },
      },
      idempotency_key: "outcome-label-catalog".parse().unwrap(),
      published_at: at(2),
    }))
    .await
    .unwrap();
  let issue = create_issue(&store, project, IssueId::generate(), content).await;
  associate(&store, &issue, Some(request), &first).await;
  finish(&store, &execution, &first, 30).await;

  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let server = octacity_server::ServerRuntime::start(runtime_fixture::runtime_config(
    root.path(),
    url.as_str(),
    "http://127.0.0.1:9",
  ))
  .await
  .unwrap();
  let client = reqwest::Client::new();
  let url = format!(
    "http://{}/api/v1/projects/{project}/issues/{}",
    server.management_addr(),
    issue.id
  );
  let read = || async {
    let body: serde_json::Value = client
      .get(format!("{url}/outcomes"))
      .send()
      .await
      .unwrap()
      .error_for_status()
      .unwrap()
      .json()
      .await
      .unwrap();
    body["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|item| item["slot"] == "triage")
      .unwrap()
      .clone()
  };
  let apply = |admission: &PublishedFactoryAdmission, now| {
    let store = store.clone();
    let work = admission.work.id();
    let run = admission.run.id();
    async move {
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      let record = snapshot
        .flow
        .data
        .records
        .iter()
        .filter(|record| record.payload().value()["decision"] == "eligible")
        .max_by_key(|record| record.observed_at())
        .unwrap();
      let record_digest = record.digest().unwrap();
      let association = store.issue_work_association(project, work).await.unwrap().unwrap();
      store
        .apply_issue_classification(management_mutation(ApplyIssueClassification {
          association,
          policy: key("triage"),
          record: record_digest,
          accepted_at: record.observed_at(),
          operation_id: FactoryDigest::content_sha256(record_digest.as_bytes().as_slice()),
          priority: Some(25),
          size_label: Some("size_small".into()),
          managed_labels: ["size_small".into()].into(),
          applied_at: at(now),
          authority: None,
        }))
        .await
    }
  };

  let card = read().await;
  assert_eq!(card["presentation"]["classification"], json!({"status":"recommended"}));
  assert_eq!(card["presentation"]["recommended_priority"], 25);
  let applied = apply(&first, 70).await.unwrap().issue;
  assert_eq!(applied.content.priority, Some(25));
  assert_eq!(
    read().await["presentation"]["classification"],
    json!({"status":"applied","priority":{"value":25,"current":true},
      "size_label":{"name":"Small work","current":true}})
  );

  let next = admit_next(&store, &first, &applied, 80, true).await;
  let current = edit_issue(
    &store,
    &applied,
    IssueContent {
      priority: Some(77),
      ..applied.content.clone()
    },
    90,
  )
  .await;
  let mut planning = current.planning.clone();
  planning.labels.clear();
  let current = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      assignees: Default::default(),
      intent: EditIssuePlanningIntent {
        project_id: project,
        id: issue.id,
        expected_version: current.version,
        planning,
      },
      idempotency_key: "outcome-remove-size-label".parse().unwrap(),
      edited_at: at(95),
    }))
    .await
    .unwrap()
    .issue;
  assert_eq!(
    read().await["presentation"]["classification"],
    json!({"status":"applied","priority":{"value":25,"current":false},
      "size_label":{"name":"Small work","current":false}})
  );
  finish(&store, &execution, &next, 100).await;
  assert_eq!(
    read().await["presentation"]["classification"],
    json!({"status":"recommended"})
  );
  assert!(matches!(apply(&next, 160).await, Err(StoreError::Conflict { .. })));
  let card = read().await;
  assert_eq!(card["run_id"], next.run.id().to_string());
  assert_eq!(card["presentation"]["classification"], json!({"status":"conflict"}));
  let issue_body: serde_json::Value = client
    .get(&url)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(issue_body["content"]["priority"], 77);
  assert_eq!(issue_body["planning"]["labels"], json!([]));
  assert_eq!(store.issue(project, issue.id, None).await.unwrap(), current);
  server.shutdown().await.unwrap();
  database.cleanup().await;
}
