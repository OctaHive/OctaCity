//! Triage authoring reuses one Project-owned classification contract without label effects.
use super::*;
use octacity_server_store_postgres::FactoryCapabilitySource;

#[path = "classification_authoring_regressions.rs"]
mod regressions;

struct Installed(FactoryConfigurationChoices);
#[async_trait::async_trait]
impl FactoryCapabilitySource for Installed {
  async fn availability(&self, _: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(FactoryConfigurationAvailability::Available(self.0.clone()))
  }
}

async fn fixture() -> (
  postgres::TestDatabase,
  Arc<PostgresStore>,
  IssueRevision,
  ProjectTextSubmission,
  FactoryIssueClassificationChoices,
) {
  let db = postgres::TestDatabase::migrated().await;
  let plain = PostgresStore::new(db.pool.clone());
  let (issue, submission) = setup(&plain).await;
  let mut definition: IssuePropertyDefinitionSet = serde_json::from_value(serde_json::json!({
    "labels":[],
    "components":[{"id":"compiler","name":"Compiler","retired":false}],
    "properties":[{"id":"repro","name":"Reproduction","retired":false,"kind":{"type":"choice","choices":[
      {"id":"unknown","name":"Unknown","retired":false},
      {"id":"needs_input","name":"Needs input","retired":false}
    ]}}],
    "classification":{"reproducibility":"repro"}
  }))
  .unwrap();
  definition.priorities.push(IssuePropertyChoice {
    id: "urgent_local".into(),
    name: "Urgent locally".into(),
    retired: false,
  });
  plain
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition,
      },
      idempotency_key: key("typed-classification-fields"),
      published_at: at(3),
    }))
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
        "issue_classification":{"policy":"triage","size_labels":{}}
      }))
      .unwrap(),
    )
    .expect("typed classification installation must not require legacy size labels");
  let store = Arc::new(PostgresStore::with_factory_capabilities(
    db.pool.clone(),
    Arc::new(Installed(choices)),
  ));
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let roles = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy).with_issue_properties(store.clone())),
  );
  let offered = roles
    .handle_query(
      &context(),
      GetFactoryAuthoringCapabilitiesQuery {
        project_id: issue.project_id,
      },
    )
    .await
    .unwrap()
    .issue_classification
    .unwrap();
  (db, store, issue, submission, offered)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn classification_authoring_offers_project_field_contract_without_size_labels() {
  let (db, store, issue, _, offered) = fixture().await;
  let offered = serde_json::to_value(offered).unwrap();
  let classification = &offered;
  assert_eq!(classification["size_labels"], serde_json::json!([]));
  let contract = &classification["field_contract"];
  assert_eq!(contract["selection"]["definition_version"], 1);
  assert_eq!(
    contract["selection"]["properties"],
    serde_json::json!({"reproducibility":"repro"})
  );
  let fields = &contract["schema"]["shape"]["fields"];
  for (name, expected) in [
    (
      "priority",
      serde_json::json!(["blocker", "high", "medium", "low", "urgent_local"]),
    ),
    ("component", serde_json::json!(["compiler"])),
    ("kind", serde_json::json!(["defect", "feature_request"])),
    ("size", serde_json::json!(["small", "medium", "large"])),
    ("reproducibility", serde_json::json!(["unknown", "needs_input"])),
  ] {
    assert_eq!(fields[name]["required"], false);
    assert_eq!(
      fields[name]["value"]["values"], expected,
      "{name} uses Project-owned vocabulary"
    );
  }
  assert_eq!(
    store.issue(issue.project_id, issue.id, None).await.unwrap(),
    issue,
    "authoring cannot apply classification or create execution"
  );
  db.cleanup().await;
}

fn typed_draft(issue: &IssueRevision, offered: &FactoryIssueClassificationChoices) -> FactoryFlowDefinitionDraft {
  let mut flow = classification_flow(issue);
  let contract = offered.field_contract.as_ref().unwrap();
  let mut root = serde_json::to_value(&flow.closure.definitions()[0]).unwrap();
  root.as_object_mut().unwrap().remove("reference");
  let nodes = root["nodes"].as_array_mut().unwrap();
  let triage = nodes.iter_mut().find(|node| node["key"] == "triage").unwrap();
  triage["outcomes"][0]["schema"] = serde_json::to_value(contract.schema.reference()).unwrap();
  triage["gate"] = serde_json::to_value(FlowGateProgram::new(vec![], "classified".parse().unwrap()).unwrap()
    .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!({"priority":"urgent_local","component":"compiler","kind":"feature_request","size":"medium","reproducibility":"needs_input"})
    }).unwrap()).unwrap().binding().unwrap()).unwrap();
  let apply = nodes.iter_mut().find(|node| node["key"] == "apply").unwrap();
  apply["input_schema"] = serde_json::to_value(contract.schema.reference()).unwrap();
  apply["action"]["parameters"] = serde_json::json!({
    "policy":offered.policy,"source_node":"triage","source_outcome":"classified","outcome":"published", "fields":contract.selection
  });
  flow.data_schemas.push(contract.schema.clone());
  serde_json::from_value(serde_json::json!({"root":root,"limits":flow.limits,"data_schemas":flow.data_schemas}))
    .unwrap()
}

async fn execute_triage(
  store: Arc<PostgresStore>,
  pool: sqlx::PgPool,
  issue: &IssueRevision,
  submission: ProjectTextSubmission,
  flow: FactoryFlowConfiguration,
) -> (FactoryRunSnapshot, FactoryRunSnapshot) {
  let submission = configure_issue_flow(&store, issue, submission, flow).await;
  let clock = Arc::new(Clock(AtomicI64::new(11)));
  let request = reviewed_request(&store, issue, &submission).await;
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
  let claim = FactoryRunClaimRecord::new(
    run,
    "local-owner".parse().unwrap(),
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
  let binding = resources(store.clone(), pool.clone()).issue_classification_executor(
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
  for (offset, node) in ["triage", "apply"].into_iter().enumerate() {
    let time = 21 + offset as i64 * 2;
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    let node: FactoryKey = node.parse().unwrap();
    let definition = snapshot.admitted_flow.closure().definition(root.definition()).unwrap();
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
    assert!(matches!(
      runner
        .run_once(run, claim.id, target, None, at(time + 1))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    if offset == 0 {
      assert_eq!(
        store.issue(issue.project_id, issue.id, None).await.unwrap(),
        *issue,
        "accepted assessment alone has no Issue effects"
      );
    }
  }
  let after = store.factory_run_snapshot(run).await.unwrap();
  (before, after)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn published_triage_explicitly_applies_shared_project_contract_without_external_effects() {
  let (db, store, issue, submission, offered) = fixture().await;
  let draft = typed_draft(&issue, &offered);
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let commands = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy).with_issue_properties(store.clone())),
  );
  let created = commands
    .handle_command(
      &context(),
      CreateFactoryFlowDefinitionCommand {
        id: FlowDefinitionId::generate(),
        project_id: issue.project_id,
        draft,
        idempotency_key: key("publish-typed-triage"),
        published_at: at(5),
      },
    )
    .await
    .expect("Project-owned typed classification must publish without per-step vocabularies or size labels");
  let (before, after) = execute_triage(
    store.clone(),
    db.pool.clone(),
    &issue,
    submission,
    created.publication.flow,
  )
  .await;
  let run = before.run.id();
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(
    serde_json::to_value(&current.content).unwrap()["priority"],
    serde_json::json!("urgent_local")
  );
  assert_eq!(current.planning.component.as_deref(), Some("compiler"));
  assert_eq!(current.planning.issue_type.as_deref(), Some("feature_request"));
  assert_eq!(current.planning.size, Some(IssueSize::Medium));
  assert_eq!(
    current.planning.values["repro"],
    IssuePropertyValue::Choice("needs_input".into())
  );
  assert!(current.planning.labels.is_empty());
  assert_eq!(current.version, issue.version.next().unwrap());
  assert_eq!(after.work, before.work);
  assert_eq!(after.outbox, before.outbox);
  assert_eq!(current.classification.fields.len(), 5);
  let accepted = after
    .flow
    .data
    .records
    .iter()
    .find(|record| record.input().node().as_str() == "triage")
    .unwrap();
  for field in &current.classification.fields {
    assert_eq!(field.assessed_version, issue.version);
    assert_eq!(field.requirements_version, issue.applicability_version);
    assert!(field.evidence.is_empty());
    let IssueClassificationOrigin::Pipeline {
      work_id,
      run_id,
      record,
      ..
    } = &field.origin
    else {
      panic!("accepted pipeline provenance")
    };
    assert_eq!(*work_id, before.work.id());
    assert_eq!(*run_id, run);
    assert_eq!(*record, accepted.digest().unwrap());
  }
  let runs = store
    .list_factory_runs(
      ListFactoryRuns::new(
        Some(issue.project_id),
        Default::default(),
        None,
        20,
        FactoryRunListVisibility::all(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(runs.items.len(), 1, "applying classification cannot activate delivery");

  assert!(
    store
      .issue_push_projections(
        issue.project_id,
        issue.id,
        &ManagementSecurityScope::trusted_network(),
        None,
        20
      )
      .await
      .unwrap()
      .is_empty()
  );
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn typed_classification_publication_rejects_stale_unbound_and_substituted_fields() {
  let (db, store, issue, _, offered) = fixture().await;
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let queries = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy).with_issue_properties(store.clone())),
  );
  let draft = typed_draft(&issue, &offered);
  let check = |draft| ValidateFactoryFlowDefinitionQuery {
    id: FlowDefinitionId::generate(),
    project_id: issue.project_id,
    draft,
  };
  queries.handle_query(&context(), check(draft.clone())).await.unwrap();
  for case in ["catalog", "binding", "substitution", "implicit"] {
    let mut altered = serde_json::to_value(&draft).unwrap();
    let action = altered["root"]["nodes"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|node| node["key"] == "apply")
      .unwrap();
    match case {
      "catalog" => action["action"]["parameters"]["fields"]["definition_version"] = serde_json::json!(2),
      "binding" => {
        action["action"]["parameters"]["fields"]["properties"]["reproducibility"] = serde_json::json!("foreign")
      }
      "substitution" => {
        action["input_binding"]["mapping"]["value"] =
          serde_json::json!({"mapping":"constant","value":{"priority":"high"}})
      }
      "implicit" => {
        action["action"]["parameters"].as_object_mut().unwrap().remove("fields");
      }
      _ => unreachable!(),
    }
    assert!(
      queries
        .handle_query(&context(), check(serde_json::from_value(altered).unwrap()))
        .await
        .is_err(),
      "{case} must fail closed"
    );
  }
  // A valid Flow enum is insufficient when its choices are not in this Project.
  let contract = offered.field_contract.unwrap();
  let FlowValueSchema::Object { mut fields } = contract.schema.shape().clone() else {
    panic!("classification object")
  };
  fields.insert(
    "component".parse().unwrap(),
    FlowFieldSchema::optional(FlowValueSchema::Enum {
      values: vec![serde_json::json!("foreign_component")],
    }),
  );
  let wrong = FlowDataSchema::new(
    "foreign.classification".parse().unwrap(),
    "1".parse().unwrap(),
    FlowValueSchema::Object { fields },
  )
  .unwrap();
  let mut altered = serde_json::to_value(&draft).unwrap();
  for node in altered["root"]["nodes"].as_array_mut().unwrap() {
    if node["key"] == "triage" {
      node["outcomes"][0]["schema"] = serde_json::to_value(wrong.reference()).unwrap();
      node["gate"] = serde_json::to_value(FlowGateProgram::new(vec![], "classified".parse().unwrap()).unwrap()
        .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!({"priority":"urgent_local","component":"foreign_component","kind":"feature_request","size":"medium","reproducibility":"needs_input"})
        }).unwrap()).unwrap().binding().unwrap()).unwrap();
    }
    if node["key"] == "apply" {
      node["input_schema"] = serde_json::to_value(wrong.reference()).unwrap();
    }
  }
  altered["data_schemas"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::to_value(wrong).unwrap());
  assert!(
    queries
      .handle_query(&context(), check(serde_json::from_value(altered).unwrap()))
      .await
      .is_err()
  );
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  db.cleanup().await;
}
