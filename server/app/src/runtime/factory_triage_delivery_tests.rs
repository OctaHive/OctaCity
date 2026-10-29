//! Accepted result composition through ordinary durable Source, Flow, action and Reporter owners.
use super::*;
use crate::runtime::postgres_test_support::TestDatabase;
use octacity_server_store_postgres::PostgresStore;
use std::collections::BTreeMap;

struct AcceptedInputs;
#[async_trait::async_trait]
impl FactoryFlowInputPreparer for AcceptedInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    prepare_factory_flow_input(snapshot, target)
  }
}

async fn accepted_reporting_fixture(
  with_effect: bool,
) -> (
  Fixture<PostgresStore>,
  FactoryReporterProfile,
  TestDatabase,
  serde_json::Value,
) {
  let database = TestDatabase::migrated().await;
  let policy = WorkSourceReportingPolicy {
    ownership: None,
    labels: vec!["triage:reviewed".into(), "triage:raw".into()],
    states: vec![],
  };
  let (memory, _) = fixture_with_reporting_policy("triage-delivery", None, None).await;
  let profile = FactoryReporterProfile::new(
    key("triage-writer"),
    key("v1"),
    reporter_draft(&memory, Some(policy.clone()), None),
  )
  .unwrap();
  let fixture = memory.into_postgres(database.pool.clone()).await;
  let (mut fixture, profile) = managed_fixture_with_native_parent(fixture, profile, None).await;
  let original = fixture.input.definition().clone();
  let base = flow("triage-delivery");
  let execution = base
    .closure
    .definition(base.closure.root())
    .unwrap()
    .execution()
    .clone();
  let summary = serde_json::json!({"accepted":true,"priority_assessment":75});
  let shape = FlowValueSchema::Object {
    fields: BTreeMap::from([
      (key("accepted"), FlowFieldSchema::required(FlowValueSchema::Boolean)),
      (
        key("priority_assessment"),
        FlowFieldSchema::required(FlowValueSchema::Integer {
          minimum: 0,
          maximum: 100,
        }),
      ),
    ]),
  };
  let result = FlowDataSchema::new(key("triage.accepted-summary"), key("v1"), shape.clone()).unwrap();
  let gate = FlowGateProgram::new(vec![], key("triaged"))
    .unwrap()
    .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant { value: summary.clone() }).unwrap())
    .unwrap();
  let template: FlowDefinitionTemplate = serde_json::from_value(serde_json::json!({
    "input_schema":"input","entry":"accepted-decision",
    "schemas": {
      "input":{"identity":"triage.input","version":"v1","shape":FlowValueSchema::Boolean},
      "decision":{"identity":result.reference().identity(),"version":"v1","shape":shape}
    },
    "nodes":[{"key":"accepted-decision","kind":"deterministic_gate","input_schema":"input",
      "outcomes":[{"key":"triaged","kind":"success","schema":"decision","public_result":true}],
      "budget":BudgetLimit::new(1,1000,0,0,1000).unwrap(),"required":true,"gate":gate,
      "input_binding":FlowInputBinding::new(BTreeMap::new(),FlowDataMapping::new(FlowValueMapping::Constant {value:serde_json::json!(true)}).unwrap()).unwrap()
    }],
    "transitions":[FlowTransition::new(key("accepted-decision"),key("triaged"),FlowTransitionTarget::Terminal(key("done")))],
    "terminals":{"done":"decision"},"execution":execution,
  })).unwrap();
  let reviewed = WorkSourceStatusEffects {
    status: "report-accepted".into(),
    add: vec!["triage:reviewed".into()],
    remove: vec!["triage:raw".into()],
    state: None,
  };
  let reporting = FactoryResultReportingDraft {
    target: FactoryReportingTarget {
      source: original.source.clone(),
      reporter: profile.binding().clone(),
      connection: original.connection.clone().unwrap(),
      policy: policy.clone(),
    },
    outcomes: BTreeMap::from([(key("triaged"), with_effect.then_some(reviewed.clone()))]),
    dimensions: vec![],
    receipt: receipt_schema(),
    action_outcome: key("done"),
    budget: BudgetLimit::new(1, 1000, 0, 0, 10000).unwrap(),
    execution: FlowExecutionPolicy::new(
      BudgetLimit::new(6, 5000, 0, 0, 10000).unwrap(),
      execution.permissions().clone(),
      1,
    )
    .unwrap(),
    retry: FactoryReporterRetryPolicy {
      max_attempts: 2,
      retry_delay_millis: 100,
    },
  };
  let child = reporting
    .compile(
      &result,
      &[key("triaged")].into(),
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
    )
    .unwrap();
  let template = reporting
    .bind(&template, &key("accepted-decision"), &child, &key("report-selection"))
    .unwrap();
  let parent = template
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::new(),
    )
    .unwrap();
  let mut schemas = parent.schemas;
  for schema in child.schemas {
    if !schemas
      .iter()
      .any(|existing| existing.reference() == schema.reference())
    {
      schemas.push(schema);
    }
  }
  let configured = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(parent.definition.reference(), vec![parent.definition, child.definition]).unwrap(),
    base.limits,
    schemas,
    BTreeMap::new(),
  )
  .unwrap();
  let target = configuration_with_flow(
    fixture.input.project_id(),
    FactoryConfigurationId::generate(),
    Some(configured),
  );
  let draft = FactoryConfigurationDraft {
    flow: Some(target.flow().clone()),
    admission_policy: key("admission"),
    wip_limits: target.wip_limits(),
    hard_budget: target.hard_budget(),
    permission_ceiling: key("permissions"),
    enabled: true,
  };
  fixture
    .store
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      intent: FactoryConfigurationMutationIntent::Create {
        id: target.reference().id(),
        project_id: fixture.input.project_id(),
        definition_digest: target.reference().definition_digest(),
        draft,
      },
      configuration: target.clone(),
      idempotency_key: IdempotencyKey::new("accepted-reporting-flow").unwrap(),
      published_at: time(8),
    }))
    .await
    .unwrap();
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: original.source.clone(),
  };
  let writer = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: profile.binding().clone(),
  };
  let handlers = FactorySourceConfigurationHandlers::new(
    fixture.store.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
    vec![source, writer.clone()],
  )
  .unwrap()
  .with_connection_configuration(
    writer.clone(),
    original
      .connection
      .as_ref()
      .unwrap()
      .reporter_configuration
      .clone()
      .unwrap(),
  )
  .unwrap()
  .with_reporting_mapping(
    writer,
    WorkSourceStatusMapping {
      statuses: vec![reviewed],
    },
    Some(policy),
  )
  .unwrap();
  let mut definition = original;
  definition.target = target.reference().clone();
  let published = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers))
    .handle_command(
      &fixture.profile.read.context,
      PublishWorkSourceConfigurationCommand {
        intent: WorkSourceConfigurationIntent {
          project_id: fixture.input.project_id(),
          inputs: vec![WorkSourceInputIntent {
            id: fixture.input.id(),
            expected_version: Some(fixture.input.version()),
            definition,
          }],
          group: None,
        },
        idempotency_key: IdempotencyKey::new("accepted-reporting-input").unwrap(),
        published_at: time(9),
      },
    )
    .await
    .unwrap();
  fixture.input = published.publication.inputs[0].input.clone();
  assert!(
    fixture.runs().await.is_empty(),
    "configuration publication cannot admit or enqueue a write"
  );
  assert!(captured_requests(&fixture.root.path().join("triage-delivery")).is_empty());
  (fixture, profile, database, summary)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn reviewed_result_delivery_keeps_original_decision_and_operation_through_recovery() {
  for ending in ["confirmed", "forbidden", "exhausted", "unconfigured"] {
    let (mut fixture, profile, database, summary) = accepted_reporting_fixture(ending != "unconfigured").await;
    let projector = Arc::new(
      Arc::try_unwrap(hosted(&fixture, vec![profile.clone()]))
        .ok()
        .unwrap()
        .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone())),
    );
    assert_eq!(
      reporting_worker(&fixture, projector.clone(), "publication-only", 10000)
        .run_once(time(100))
        .await
        .unwrap()
        .claimed,
      0
    );
    let run = admit_managed(&fixture).await;
    let admitted = fixture.store.factory_run_snapshot(run).await.unwrap();
    let claim = FactoryRunClaimRecord::new(
      run,
      key("triage-flow-owner"),
      FactoryClaim::new(FactoryClaimFence::new(digest(91)), time(110), time(9900)).unwrap(),
    );
    fixture
      .store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: admitted.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run,
          AuditActorKind::Worker,
          None,
          key("claimed"),
          digest(92),
          key("accepted"),
          time(110),
        ),
      })
      .await
      .unwrap();
    let runner = FactoryFlowRunner::new(
      fixture.store.clone(),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
        capability(),
        projector.clone(),
        outcome(),
      )))),
      Arc::new(AcceptedInputs),
      fixture.clock.clone(),
    );
    let mut finished = false;
    let mut intermediate = false;
    for tick in 111..190 {
      fixture.clock.0.store(tick, Ordering::SeqCst);
      let step = runner.reconcile_flow(run, claim.id, time(tick)).await.unwrap();
      let snapshot = fixture.store.factory_run_snapshot(run).await.unwrap();
      if !intermediate
        && snapshot
          .outbox
          .iter()
          .any(|row| row.kind.as_str() == "work.status.projection")
      {
        intermediate = true;
        assert!(
          !matches!(step, FactoryFlowStep::Resolved(_)),
          "configured effects can queue before the reporting subflow returns"
        );
      }
      if step == FactoryFlowStep::Resolved(key("done")) {
        finished = true;
        break;
      }
    }
    assert!(finished);
    assert_eq!(intermediate, ending != "unconfigured");
    let accepted = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(accepted.run.state(), FactoryRunState::Completed);
    let decision = accepted
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("accepted-decision"))
      .unwrap();
    assert_eq!(decision.payload().value(), &summary);
    let call = accepted
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.node() == &key("report-selection-0"))
      .unwrap();
    assert_eq!(call.sources()[0].digest(), decision.digest().unwrap());
    let directory = fixture.root.path().join("triage-delivery");
    if ending == "unconfigured" {
      assert!(
        accepted
          .outbox
          .iter()
          .all(|row| row.kind.as_str() != "work.status.projection")
      );
      assert_eq!(
        reporting_worker(&fixture, projector, "completed-without-effect", 10000)
          .run_once(time(300))
          .await
          .unwrap()
          .claimed,
        0
      );
      assert!(
        captured_requests(&directory).is_empty(),
        "a completed Run and installed writer cannot authorize an unconfigured effect"
      );
      database.cleanup().await;
      continue;
    }
    let effect = accepted
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("effect-0"))
      .unwrap();
    assert_eq!(effect.input().caller_input_digest(), Some(call.digest().unwrap()));
    assert_eq!(effect.input().payload().value()["result"], summary);
    let caller_result = accepted
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("report-selection-0"))
      .unwrap();
    assert_eq!(caller_result.payload().value(), &summary);
    assert_eq!(
      accepted
        .outbox
        .iter()
        .filter(|row| row.kind.as_str() == "work.status.projection")
        .count(),
      1,
      "Flow completion cannot add a second unconfigured write"
    );
    let original = accepted
      .outbox
      .iter()
      .find(|row| row.kind.as_str() == "work.status.projection")
      .unwrap();
    assert_eq!(original.input_digest, effect.digest().unwrap());
    assert!(captured_requests(&directory).is_empty());
    std::fs::write(
      directory.join("reporter-mode.json"),
      br#"{"lose_project_response":true}"#,
    )
    .unwrap();
    fixture.clock.0.store(300, Ordering::SeqCst);
    let first = reporting_worker(&fixture, projector, "triage-before-restart", 10000);
    assert_eq!(first.run_once(time(300)).await.unwrap().retrying, 1);
    assert!(directory.join("reporter-applied").exists());
    let pending = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(pending.flow, accepted.flow);
    assert!(
      pending
        .outbox
        .iter()
        .any(|row| row.operation_id == original.operation_id
          && row.state == FactoryOutboxState::Pending
          && row.attempt == 1)
    );
    let captured = captured_requests(&directory);
    let checkpoint = recovery::calls(&directory).len();
    drop(first);
    fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
    if ending == "forbidden" {
      let connection = fixture.input.definition().connection.as_ref().unwrap();
      fixture
        .store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: connection.id,
            connection_version: connection.version,
            project_id: fixture.input.project_id(),
            expected_current_version: Some(connection.access_version),
            resources: vec![IntegrationResourceSelection {
              reference: fixture.input.definition().source.resource.clone(),
              purposes: vec![IntegrationPurpose::TaskRead],
            }],
          },
          idempotency_key: IdempotencyKey::new("revoke-triage-writer").unwrap(),
          published_at: time(350),
        }))
        .await
        .unwrap();
    } else if ending == "exhausted" {
      std::fs::write(
        directory.join("reporter-mode.json"),
        br#"{"failure":{"class":"transient","retry_after_millis":null}}"#,
      )
      .unwrap();
    }
    let projector = Arc::new(
      Arc::try_unwrap(hosted(&fixture, vec![profile]))
        .ok()
        .unwrap()
        .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone())),
    );
    let restarted = reporting_worker(&fixture, projector, "triage-after-restart", 10000);
    fixture.clock.0.store(400, Ordering::SeqCst);
    let result = restarted.run_once(time(400)).await.unwrap();
    assert_eq!(result.delivered, u16::from(ending == "confirmed"), "{ending}");
    assert_eq!(result.failed, u16::from(ending != "confirmed"), "{ending}");
    assert_eq!(restarted.run_once(time(401)).await.unwrap().claimed, 0);
    if ending == "forbidden" {
      assert_eq!(captured_requests(&directory), captured);
    }
    let calls = recovery::calls(&directory);
    for call in &calls {
      assert_eq!(original_intent(call), original_intent(&calls[0]));
      assert_eq!(
        original_intent(call)["projection_id"],
        original.operation_id.to_string()
      );
      assert_eq!(original_intent(call)["item_id"], "TASK-native");
      assert_eq!(
        original_intent(call)["effects"],
        serde_json::json!({"add":["triage:reviewed"],"remove":["triage:raw"],"state":null})
      );
    }
    if ending != "forbidden" {
      assert!(
        !calls[checkpoint..].is_empty(),
        "recovery must inspect rather than infer confirmation from an old Apply"
      );
      assert!(
        calls[checkpoint..]
          .iter()
          .all(|call| call["message"]["command"]["operation"] == "inspect")
      );
    }
    assert_eq!(
      calls
        .iter()
        .filter(|call| call["message"]["command"]["operation"] == "apply")
        .count(),
      1,
      "recovery confirms a lost reply without repeating an applied effect"
    );
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(after.flow, accepted.flow);
    assert_eq!(after.work, admitted.work);
    assert_eq!(after.run, accepted.run);
    assert!(
      after.outbox.contains(original),
      "the original accepted outbox row must remain retained"
    );
    assert!(
      after
        .outbox
        .iter()
        .filter(|row| row.kind.as_str() == "work.status.projection")
        .all(|row| row.operation_id == original.operation_id && row.input_digest == original.input_digest)
    );
    assert_eq!(fixture.runs().await.len(), 1);
    if ending != "confirmed" {
      assert!(
        after.outbox.iter().any(|row| row.failure
          == Some(if ending == "forbidden" {
            FactoryOutboxFailure::Forbidden
          } else {
            FactoryOutboxFailure::Exhausted
          })),
        "{ending}: {:?}",
        after
          .outbox
          .iter()
          .filter(|row| row.state == FactoryOutboxState::Failed)
          .map(|row| (row.kind.clone(), row.failure))
          .collect::<Vec<_>>()
      );
    }
    database.cleanup().await;
  }
}
