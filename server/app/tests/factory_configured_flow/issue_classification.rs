//! An accepted assessment reaches the real action, authorized command and PostgreSQL Issue owner.
use super::*;

fn plugin() -> ImmutableReference {
  ImmutableReference::new(
    key("test.classification"),
    key("v1"),
    FactoryDigest::from_bytes([173; 32]),
  )
}

fn assessment() -> serde_json::Value {
  json!({"priority":"high","component":"engine","size":"small","kind":"defect","reproducibility":"unknown"})
}

fn classification_flow(assessment: serde_json::Value) -> FlowDefinitionTemplate {
  let mut template = summary_flow(None);
  let mut shape = json!({"type":"object","fields":{
    "priority":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":64}},
    "component":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":64}},
    "size":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":64}},
    "kind":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":64}},
    "reproducibility":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":64}}
  }});
  if assessment.get("assessment_inputs").is_some() {
    shape["fields"]["assessment_inputs"] = json!({"required":true,"value":{"type":"object","fields":{
      "subject":{"required":true,"value":{"type":"object","fields":{
        "kind":{"required":true,"value":{"type":"enum","values":["exact"]}},
        "value":{"required":true,"value":{"type":"object","fields":{
          "project_id":{"required":true,"value":{"type":"string","min_bytes":36,"max_bytes":36}},
          "repository_id":{"required":true,"value":{"type":"string","min_bytes":36,"max_bytes":36}},
          "base_revision":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":256}}
        }}}
      }}}
    }}});
  }
  template.schemas.get_mut(&key("producer_result")).unwrap().shape = serde_json::from_value(shape).unwrap();
  template.schemas.get_mut(&key("custom")).unwrap().shape = serde_json::from_value(json!({
    "type":"object","fields":{"receipt":{"required":true,"value":{"type":"string","min_bytes":64,"max_bytes":64}}}
  }))
  .unwrap();
  let source = &mut template.nodes[0];
  source.outcomes[0].issue_presentation = None;
  source.gate = Some(
    FlowGateProgram::new(vec![], key("produced"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant { value: assessment }).unwrap())
      .unwrap(),
  );
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin()],
    ..Default::default()
  })
  .unwrap();
  let action = &mut template.nodes[1];
  action.kind = FlowNodeKind::TrustedAction;
  action.gate = None;
  action.permissions = permissions.clone();
  action.outcomes.truncate(1);
  action.outcomes[0].key = key("done");
  action.outcomes[0].schema = key("custom");
  action.outcomes[0].issue_presentation = None;
  action.action = Some(
    FlowActionBinding::new(
      plugin(),
      key("apply_issue_classification"),
      json!({
        "source_node":"produce_outcome","source_outcome":"produced","outcome":"done","policy":"project.triage",
        "size_labels":{},"fields":{"definition_version":1,"priority":true,"component":true,"kind":true,"size":true,
          "properties":{"reproducibility":"reproduction"}}
      }),
    )
    .unwrap(),
  );
  template.transitions = vec![
    FlowTransition::new(
      key("produce_outcome"),
      key("produced"),
      FlowTransitionTarget::Node(key("retain_outcome")),
    ),
    FlowTransition::new(
      key("retain_outcome"),
      key("done"),
      FlowTransitionTarget::Terminal(key("done")),
    ),
  ];
  template.terminals = [(key("done"), key("custom"))].into();
  template.execution = FlowExecutionPolicy::new(template.execution.budget(), permissions, 1).unwrap();
  template
}

async fn fixture() -> (
  postgres_support::TestDatabase,
  Arc<PostgresStore>,
  PublishedFactoryAdmission,
  IssueRevision,
) {
  fixture_with_assessment(assessment()).await
}

async fn fixture_with_assessment(
  assessment: serde_json::Value,
) -> (
  postgres_support::TestDatabase,
  Arc<PostgresStore>,
  PublishedFactoryAdmission,
  IssueRevision,
) {
  fixture_with_assessment_inputs(assessment, false).await
}

async fn fixture_with_assessment_inputs(
  assessment: serde_json::Value,
  declare_code: bool,
) -> (
  postgres_support::TestDatabase,
  Arc<PostgresStore>,
  PublishedFactoryAdmission,
  IssueRevision,
) {
  fixture_with_assessment_flow(assessment, declare_code, |flow| flow).await
}

async fn fixture_with_assessment_flow(
  mut assessment: serde_json::Value,
  declare_code: bool,
  adapt_flow: impl FnOnce(FlowDefinitionTemplate) -> FlowDefinitionTemplate,
) -> (
  postgres_support::TestDatabase,
  Arc<PostgresStore>,
  PublishedFactoryAdmission,
  IssueRevision,
) {
  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = factory_node_test_support::fixtures::work();
  if declare_code {
    assessment["assessment_inputs"] =
      json!({"subject":FactoryTaskSubject::Exact(fixture.subject().repository().unwrap().clone())});
  }
  let request = IssueExecutionRequestId::generate();
  let content = requirements();
  let (admission, _) = flow_contract::configured_fixture_with_work(
    adapt_flow(classification_flow(assessment)),
    work_for(&fixture, request, &content),
  );
  let store = admit(&database.pool, &admission).await;
  let project = admission.work.subject().project_id();
  let property = |id: &str, values: &[&str]| {
    json!({"id":id,"name":id,"retired":false,
    "kind":{"type":"choice","choices":values.iter().map(|v|json!({"id":v,"name":v,"retired":false})).collect::<Vec<_>>()}})
  };
  let mut definition: IssuePropertyDefinitionSet = serde_json::from_value(json!({"labels":[],
    "components":[{"id":"engine","name":"Engine","retired":false}],
    "properties":[property("reproduction",&["unknown","needs_input","not_applicable","reproduced"]),
      {"id":"customer_visible","name":"Customer visible","retired":false,"kind":{"type":"boolean"}}],
    "classification":{"reproducibility":"reproduction"}
  }))
  .unwrap();
  definition.priorities.push(IssuePropertyChoice {
    id: "unknown".into(),
    name: "Awaiting priority".into(),
    retired: false,
  });
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: project,
        expected_version: 0,
        definition,
      },
      idempotency_key: "classification-catalog".parse().unwrap(),
      published_at: at(1),
    }))
    .await
    .unwrap();
  let issue = create_issue(&store, project, IssueId::generate(), content).await;
  let issue = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: project,
        id: issue.id,
        expected_version: issue.version,
        planning: serde_json::from_value(
          json!({"definition_version":1,"assignee":null,"labels":[],"due_date":null,
        "parent_id":null,"values":{"customer_visible":{"type":"boolean","value":true}}}),
        )
        .unwrap(),
      },
      assignees: Default::default(),
      idempotency_key: "unrelated-property".parse().unwrap(),
      edited_at: at(2),
    }))
    .await
    .unwrap()
    .issue;
  associate(&store, &issue, Some(request), &admission).await;
  (database, store, admission, issue)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_action_retains_declared_code_inputs_and_reviews_changed_subject_after_restart() {
  let (database, store, admission, issue) = fixture_with_assessment_inputs(assessment(), true).await;
  finish_classification(&store, &admission, 30).await;
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  let subject = FactoryTaskSubject::Exact(admission.work.subject().repository().unwrap().clone());
  assert_eq!(current.classification.fields.len(), 5);
  assert!(
    current
      .classification
      .fields
      .iter()
      .all(|field| field.inputs.subject.as_ref() == Some(&subject))
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    current
  );
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = AuthorizedQueryHandler::new(policy.clone(), Arc::new(IssueHandlers::new(restarted.clone(), policy)));
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let query = ReviewIssueClassificationQuery {
    project_id: issue.project_id,
    id: issue.id,
    version: None,
    subject: Some(subject.clone()),
    checked_at: at(100),
  };
  let reviewed = queries.handle_query(&context, query.clone()).await.unwrap();
  assert!(reviewed.fields.iter().all(|field| field.problems.is_empty()));
  let mut changed = query;
  changed.subject = Some(FactoryTaskSubject::Exact(ExactSubject::new(
    issue.project_id,
    subject.exact().unwrap().repository_id(),
    "changed-base".parse().unwrap(),
  )));
  let stale = queries.handle_query(&context, changed).await.unwrap();
  assert!(
    stale
      .fields
      .iter()
      .all(|field| field.problems == vec![IssueClassificationProblem::CodeSubjectChanged])
  );
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    current
  );
  database.cleanup().await;
}

fn executor(store: &Arc<PostgresStore>) -> Arc<FactoryFlowActionAdapter> {
  let action = Arc::new(FactoryIssueClassificationAction::new(
    plugin(),
    store.clone(),
    store.clone(),
    store.clone(),
    ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
    Arc::new(TrustedNetworkManagementPolicy),
    factory_node_test_support::clock(),
  ));
  Arc::new(FactoryFlowActionAdapter::new(action))
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_action_atomically_applies_builtin_and_project_fields_with_provenance() {
  let (database, store, admission, issue) = fixture().await;
  let project = issue.project_id;
  finish_classification(&store, &admission, 30).await;
  let current = store.issue(project, issue.id, None).await.unwrap();
  assert_eq!(current.version, issue.version.next().unwrap());
  assert_eq!(current.applicability_version, issue.applicability_version);
  assert_eq!(serde_json::to_value(&current.content).unwrap()["priority"], "high");
  assert_eq!(
    serde_json::to_value(&current).unwrap()["planning"]["component"],
    "engine"
  );
  assert_eq!(serde_json::to_value(&current).unwrap()["planning"]["size"], "small");
  assert_eq!(serde_json::to_value(&current).unwrap()["planning"]["type"], "defect");
  assert_eq!(
    current.planning.values["reproduction"],
    IssuePropertyValue::Choice("unknown".into())
  );
  assert_eq!(
    current.planning.values["customer_visible"],
    IssuePropertyValue::Boolean(true)
  );
  assert!(
    current.planning.labels.is_empty(),
    "a typed size does not select a legacy label"
  );
  assert_eq!(current.classification.fields.len(), 5);
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let record = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|r| r.input().node() == &key("produce_outcome"))
    .unwrap();
  for field in &current.classification.fields {
    assert!(
      field.inputs.is_empty(),
      "earlier publications declare no assessment input pins"
    );
    assert!(serde_json::to_value(field).unwrap().get("inputs").is_none());
    assert_eq!(field.assessed_version, issue.version);
    assert_eq!(field.requirements_version, issue.applicability_version);
    assert!(field.evidence.is_empty());
    let IssueClassificationOrigin::Pipeline {
      work_id,
      run_id,
      record: source,
      ..
    } = &field.origin
    else {
      panic!("classification must retain its accepted producer");
    };
    assert_eq!(*work_id, admission.work.id());
    assert_eq!(*run_id, admission.run.id());
    assert_eq!(*source, record.digest().unwrap());
  }
  assert!(
    current.classification.evidence.is_empty(),
    "unresolved reproduction claims no verified evidence"
  );
  assert_eq!(
    store.issue(project, issue.id, Some(issue.version)).await.unwrap(),
    issue
  );
  assert_eq!(
    snapshot.work, admission.work,
    "classification must preserve admitted scheduling"
  );
  assert_eq!(
    store
      .issue_executions(project, issue.id, None, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_action_rejects_a_subject_not_supplied_to_its_accepted_producer() {
  let mut value = assessment();
  let other = factory_node_test_support::fixtures::work();
  value["assessment_inputs"] =
    json!({"subject":FactoryTaskSubject::Exact(other.subject().repository().unwrap().clone())});
  let (database, store, admission, issue) = fixture_with_assessment(value).await;
  let (runner, claim, accepted) = accepted_producer(&store, &admission, 30).await;
  let mut rejected = false;
  for time in 100..125 {
    match runner.reconcile_flow(admission.run.id(), claim, at(time)).await {
      Ok(FactoryFlowStep::Advanced) => {}
      Err(error) => {
        assert_eq!(error.classification(), ApplicationFailure::Invalid);
        rejected = true;
        break;
      }
      other => panic!("substituted producer input must be rejected: {other:?}"),
    }
  }
  assert!(rejected);
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  let after = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(after.work, accepted.work);
  assert_eq!(after.flow.data.records, accepted.flow.data.records);
  assert_eq!(after.outbox, accepted.outbox);
  database.cleanup().await;
}

type ClassificationRunner = FactoryFlowRunner<PostgresStore, FactoryFlowActionAdapter, flow_contract::Inputs>;

// Stop at the accepted producer boundary so ordinary Issue commands can race the action.
async fn accepted_producer(
  store: &Arc<PostgresStore>,
  admission: &PublishedFactoryAdmission,
  start: i64,
) -> (ClassificationRunner, FactoryDigest, FactoryRunSnapshot) {
  let run = admission.run.id();
  let claim = FactoryRunClaimRecord::new(
    run,
    key("classification-worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([43; 32])),
      at(start),
      at(start + 10_000),
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
    executor(store),
    Arc::new(flow_contract::Inputs),
    factory_node_test_support::clock(),
  );
  for time in start + 1..start + 25 {
    assert_eq!(
      runner.reconcile_flow(run, claim.id, at(time)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    if !snapshot.flow.data.records.is_empty() {
      assert_eq!(snapshot.flow.data.records.len(), 1);
      assert_eq!(snapshot.flow.data.records[0].outcome(), &key("produced"));
      return (runner, claim.id, snapshot);
    }
  }
  panic!("producer must be accepted before the action");
}

async fn resolves(runner: &ClassificationRunner, run: FactoryRunId, claim: FactoryDigest, start: i64) {
  for time in start..start + 25 {
    match runner.reconcile_flow(run, claim, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(terminal) => {
        assert_eq!(terminal, key("done"));
        return;
      }
      other => panic!("classification must resolve: {other:?}"),
    }
  }
  panic!("bounded classification must resolve");
}

async fn finish_classification(store: &Arc<PostgresStore>, admission: &PublishedFactoryAdmission, start: i64) {
  let (runner, claim, _) = accepted_producer(store, admission, start).await;
  resolves(&runner, admission.run.id(), claim, start + 25).await;
}

async fn rejects_without_effects(
  store: &PostgresStore,
  runner: &ClassificationRunner,
  claim: FactoryDigest,
  accepted: &FactoryRunSnapshot,
  current: &IssueRevision,
) {
  let history = store
    .issue_revisions(current.project_id, current.id, None, 100)
    .await
    .unwrap();
  let mut rejected = false;
  for time in 100..125 {
    match runner.reconcile_flow(accepted.run.id(), claim, at(time)).await {
      Ok(FactoryFlowStep::Advanced) => {}
      Err(error) => {
        assert_eq!(error.classification(), ApplicationFailure::Conflict, "{error:?}");
        rejected = true;
        break;
      }
      other => panic!("selected classification must conflict: {other:?}"),
    }
  }
  assert!(rejected, "classification must reject within the bounded action");
  assert_eq!(
    store.issue(current.project_id, current.id, None).await.unwrap(),
    *current
  );
  assert_eq!(
    store
      .issue_revisions(current.project_id, current.id, None, 100)
      .await
      .unwrap(),
    history
  );
  let retained = store.factory_run_snapshot(accepted.run.id()).await.unwrap();
  assert_eq!(retained.work, accepted.work);
  assert_eq!(
    retained.flow.data.records, accepted.flow.data.records,
    "retain the accepted assessment, with no action receipt"
  );
  assert_eq!(
    retained.flow.completions, accepted.flow.completions,
    "no successful action completion"
  );
  assert_eq!(retained.outbox, accepted.outbox, "no external effect is enqueued");
}

fn selected_fields() -> [IssueField; 5] {
  [
    IssueField::Priority,
    IssueField::Component,
    IssueField::Type,
    IssueField::Size,
    IssueField::Properties,
  ]
}

async fn planning_edit(
  store: &PostgresStore,
  issue: &IssueRevision,
  planning: IssuePlanning,
  time: i64,
) -> IssueRevision {
  store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        planning,
      },
      assignees: Default::default(),
      idempotency_key: format!("planning-{time}").parse().unwrap(),
      edited_at: at(time),
    }))
    .await
    .unwrap()
    .issue
}

async fn manual_edit(store: &PostgresStore, issue: &IssueRevision, field: IssueField, time: i64) -> IssueRevision {
  if field == IssueField::Priority {
    let mut content = issue.content.clone();
    content.priority = Some(IssuePriority::Choice("low".into()));
    return edit_issue(store, issue, content, time).await;
  }
  let mut planning = issue.planning.clone();
  match field {
    IssueField::Component => planning.component = Some("engine".into()),
    IssueField::Type => planning.issue_type = Some("feature_request".into()),
    IssueField::Size => planning.size = Some(IssueSize::Large),
    IssueField::Properties => {
      planning
        .values
        .insert("reproduction".into(), IssuePropertyValue::Choice("needs_input".into()));
    }
    _ => panic!("not a selected field"),
  }
  planning_edit(store, issue, planning, time).await
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_rejects_manual_edits_even_after_selected_values_are_restored() {
  for field in selected_fields() {
    let (database, store, admission, issue) = fixture().await;
    let (runner, claim, accepted) = accepted_producer(&store, &admission, 30).await;
    let edited = manual_edit(&store, &issue, field, 60).await;
    let restored = if field == IssueField::Priority {
      edit_issue(&store, &edited, issue.content.clone(), 61).await
    } else {
      planning_edit(&store, &edited, issue.planning.clone(), 61).await
    };
    assert_eq!(restored.applicability_version, issue.applicability_version);
    rejects_without_effects(&store, &runner, claim, &accepted, &restored).await;
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_rejects_intervening_manual_edits_to_each_selected_field() {
  for field in selected_fields() {
    let (database, store, admission, issue) = fixture().await;
    let (runner, claim, accepted) = accepted_producer(&store, &admission, 30).await;
    let edited = manual_edit(&store, &issue, field, 60).await;
    assert_eq!(edited.applicability_version, issue.applicability_version);
    rejects_without_effects(&store, &runner, claim, &accepted, &edited).await;
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_rejects_stale_requirements_even_if_the_original_text_is_restored() {
  for restore in [false, true] {
    let (database, store, admission, issue) = fixture().await;
    let (runner, claim, accepted) = accepted_producer(&store, &admission, 30).await;
    let mut content = issue.content.clone();
    content.acceptance = Some("Review the changed acceptance criteria".into());
    let edited = edit_issue(&store, &issue, content, 60).await;
    let current = if restore {
      edit_issue(&store, &edited, issue.content.clone(), 61).await
    } else {
      edited
    };
    assert_ne!(current.applicability_version, issue.applicability_version);
    rejects_without_effects(&store, &runner, claim, &accepted, &current).await;
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_rejects_source_owned_selected_fields() {
  for field in selected_fields() {
    let (database, store, admission, issue) = fixture().await;
    let (runner, claim, accepted) = accepted_producer(&store, &admission, 30).await;
    let policy = IssueFieldPolicy {
      source_owned: [field].into(),
      ..Default::default()
    };
    // Trusted source ownership is setup, never an ordinary Issue edit capability.
    sqlx::query("UPDATE issues SET field_policy=$1 WHERE project_id=$2 AND id=$3")
      .bind(sqlx::types::Json(&policy))
      .bind(issue.project_id.as_uuid())
      .bind(issue.id.as_uuid())
      .execute(&database.pool)
      .await
      .unwrap();
    assert_eq!(
      store.issue_field_policy(issue.project_id, issue.id).await.unwrap(),
      policy
    );
    rejects_without_effects(&store, &runner, claim, &accepted, &issue).await;
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_rejects_an_older_assessment_after_a_newer_applicable_one() {
  let (database, store, older, issue) = fixture().await;
  let (runner, claim, accepted) = accepted_producer(&store, &older, 30).await;
  let newer = admit_next(&store, &older, &issue, 40, true).await;
  finish_classification(&store, &newer, 60).await;
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.content.priority, Some(IssuePriority::Choice("high".into())));
  assert_eq!(current.planning.component.as_deref(), Some("engine"));
  rejects_without_effects(&store, &runner, claim, &accepted, &current).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_allows_a_newer_assessment_of_the_same_reviewed_issue() {
  let (database, store, older, issue) = fixture().await;
  let newer = admit_next(&store, &older, &issue, 20, true).await;
  finish_classification(&store, &older, 30).await;
  let first = store.issue(issue.project_id, issue.id, None).await.unwrap();
  finish_classification(&store, &newer, 70).await;
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.version, first.version.next().unwrap());
  assert_eq!(current.applicability_version, issue.applicability_version);
  for field in &current.classification.fields {
    assert_eq!(
      field.assessed_version, issue.version,
      "keep the original reviewed revision"
    );
    assert!(matches!(field.origin, IssueClassificationOrigin::Pipeline { run_id, .. } if run_id == newer.run.id()));
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_rejects_manual_changes_after_an_earlier_applied_assessment() {
  let (database, store, older, issue) = fixture().await;
  let newer = admit_next(&store, &older, &issue, 20, true).await;
  finish_classification(&store, &older, 30).await;
  let first = store.issue(issue.project_id, issue.id, None).await.unwrap();
  let edited = manual_edit(&store, &first, IssueField::Priority, 60).await;
  let restored = edit_issue(&store, &edited, first.content.clone(), 61).await;
  let (runner, claim, accepted) = accepted_producer(&store, &newer, 70).await;
  rejects_without_effects(&store, &runner, claim, &accepted, &restored).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_preserves_an_intervening_edit_to_an_unselected_property() {
  let (database, store, admission, issue) = fixture().await;
  let (runner, claim, accepted) = accepted_producer(&store, &admission, 30).await;
  let mut planning = issue.planning.clone();
  planning
    .values
    .insert("customer_visible".into(), IssuePropertyValue::Boolean(false));
  let edited = planning_edit(&store, &issue, planning, 60).await;
  resolves(&runner, admission.run.id(), claim, 100).await;
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.version, edited.version.next().unwrap());
  assert_eq!(current.content.priority, Some(IssuePriority::Choice("high".into())));
  assert_eq!(
    current.planning.values["customer_visible"],
    IssuePropertyValue::Boolean(false)
  );
  assert_eq!(
    current.planning.values["reproduction"],
    IssuePropertyValue::Choice("unknown".into())
  );
  assert!(
    current
      .classification
      .fields
      .iter()
      .all(|field| field.assessed_version == issue.version)
  );
  assert_eq!(
    store
      .issue(issue.project_id, issue.id, Some(edited.version))
      .await
      .unwrap(),
    edited
  );
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert!(snapshot.flow.data.records.contains(&accepted.flow.data.records[0]));
  assert_eq!(snapshot.work, accepted.work);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_applies_a_project_priority_named_unknown() {
  let mut result = assessment();
  result["priority"] = json!("unknown");
  let (database, store, admission, issue) = fixture_with_assessment(result).await;
  finish_classification(&store, &admission, 30).await;
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.content.priority, Some(IssuePriority::Choice("unknown".into())));
  assert_eq!(current.version, issue.version.next().unwrap());
  assert_eq!(current.classification.fields.len(), 5);
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let accepted = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|record| record.outcome() == &key("produced"))
    .unwrap();
  assert_eq!(accepted.payload().value()["priority"], "unknown");
  assert_eq!(snapshot.work, admission.work);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_cannot_drop_accepted_assessment_dependencies_at_execution() {
  let (database, store, admission, issue) = fixture_with_assessment_flow(assessment(), true, |mut flow| {
    let FlowValueSchema::Object { mut fields } = flow.schemas[&key("producer_result")].shape.clone() else {
      panic!("assessment object")
    };
    fields.remove(&key("assessment_inputs"));
    let mapping = FlowValueMapping::Object {
      fields: fields
        .keys()
        .map(|name| {
          (
            name.clone(),
            FlowValueMapping::Pointer {
              path: format!("/assessment/{name}"),
            },
          )
        })
        .collect(),
    };
    flow.schemas.insert(
      key("values_only"),
      FlowTemplateSchema {
        identity: key("assessment.values-only"),
        version: key("1"),
        shape: FlowValueSchema::Object { fields },
      },
    );
    let action = &mut flow.nodes[1];
    action.input_schema = key("values_only");
    action.input_binding = FlowInputBinding::new(
      [(
        key("assessment"),
        FlowInputSource::Record {
          node: key("produce_outcome"),
          outcome: key("produced"),
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      )]
      .into(),
      FlowDataMapping::new(mapping).unwrap(),
    )
    .unwrap();
    flow
  })
  .await;
  let (runner, claim, before) = accepted_producer(&store, &admission, 30).await;
  let mut rejected = false;
  for time in 100..125 {
    match runner.reconcile_flow(admission.run.id(), claim, at(time)).await {
      Ok(FactoryFlowStep::Advanced) => {}
      Err(_) => {
        rejected = true;
        break;
      }
      other => panic!("dependency omission must reject, got {other:?}"),
    }
  }
  assert!(rejected);
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  let after = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(after.flow.data.records, before.flow.data.records);
  assert_eq!(after.outbox, before.outbox);
  database.cleanup().await;
}
