use crate::*;
use async_trait::async_trait;
use octacity_server_factory::*;
use std::sync::{Arc, Mutex};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([1; 32]))
}
fn outcome() -> FlowOutcomeDefinition {
  FlowOutcomeDefinition::new(key("done"), FlowOutcomeKind::Success, reference("action.result"))
}

struct Reporter(Mutex<Vec<FactoryWorkStatusRequest>>);
#[async_trait]
impl FactoryWorkStatusProjector for Reporter {
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    self.0.lock().unwrap().push(request);
    Ok((FactoryDigest::from_bytes([7; 32]), BudgetUsage::default()))
  }
}

struct NotificationPlugin;
#[async_trait]
impl FactoryFlowActionPlugin for NotificationPlugin {
  fn identity(&self) -> ImmutableReference {
    reference("notification")
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.action() != &key("notify")
      || binding.parameters() != &serde_json::json!({"channel":"audit"})
      || outcomes != [outcome()]
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    assert_eq!(request.binding.parameters(), &serde_json::json!({"channel":"audit"}));
    Ok(FactoryFlowActionObservation {
      outcome: key("done"),
      output_schema: reference("action.result"),
      output_digest: FactoryDigest::from_bytes([8; 32]),
      usage: BudgetUsage::default(),
    })
  }
}

fn action_node(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
  binding: FlowActionBinding,
) -> (FlowDefinition, FlowRun, NodeAttempt) {
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![binding.plugin().clone()],
    ..Default::default()
  })
  .unwrap();
  action_node_with_permissions(snapshot, binding, permissions)
}

fn action_node_with_permissions(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
  binding: FlowActionBinding,
  permissions: FactoryPermissionSet,
) -> (FlowDefinition, FlowRun, NodeAttempt) {
  let budget = BudgetLimit::new(1, 1000, 1, 1, 1024).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("finish"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: None,
    outcomes: vec![outcome()],
    budget,
    permissions: permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(binding.clone())
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: None,
    entry: key("finish"),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("finish"),
      key("done"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), reference("action.result"))],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions, 1).unwrap(),
  })
  .unwrap();
  let flow = FlowRun::root(&snapshot.run, definition.reference()).unwrap();
  let claim = snapshot.current_claim.as_ref().unwrap();
  let node = NodeAttempt::new(
    &flow,
    &WorkflowCycle::initial(&flow).unwrap(),
    &definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("finish"),
      node_kind: FlowNodeKind::TrustedAction,
      number: NodeAttemptNumber::new(1).unwrap(),
      input_digest: FactoryDigest::from_bytes([4; 32]),
      budget,
      deadline: octacity_server_domain::Timestamp::from_unix_millis(99).unwrap(),
      execution: NodeExecutionIdentity::External(binding.plugin().clone()),
      ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
    },
  )
  .unwrap();
  (definition, flow, node)
}

#[test]
fn normal_flow_action_supports_arbitrary_status_and_another_plugin() {
  crate::factory_admission_tests::run_ready(async {
    let snapshot = crate::factory_triage_tests::accepted_research_work().await;
    let reporter = Arc::new(Reporter(Mutex::new(vec![])));
    let plugin = FactoryWorkStatusAction::new(reference("work-reporter"), reporter.clone(), outcome());
    let adapter = FactoryFlowActionAdapter::new(Arc::new(plugin));
    let (definition, flow, node) = action_node(
      &snapshot,
      FlowActionBinding::new(
        reference("work-reporter"),
        key("set_status"),
        serde_json::json!({"status":"Ожидает уточнения"}),
      )
      .unwrap(),
    );
    let make_context = || FactoryFlowActionContext {
      run: &snapshot.run,
      work: &snapshot.work,
      definition: &definition,
      flow: &flow,
      node: &node,
      ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
      at: octacity_server_domain::Timestamp::from_unix_millis(30).unwrap(),
    };
    let completion = adapter.execute(make_context()).await.unwrap();
    adapter.execute(make_context()).await.unwrap();
    assert_eq!(completion.outcome(), &key("done"));
    {
      let requests = reporter.0.lock().unwrap();
      assert_eq!(requests[0], requests[1]);
      assert_eq!(requests[0].status.as_str(), "Ожидает уточнения");
      assert_eq!(requests[0].external_identity, *snapshot.work.external_identity());
    }

    let (definition, flow, node) = action_node(
      &snapshot,
      FlowActionBinding::new(
        reference("notification"),
        key("notify"),
        serde_json::json!({"channel":"audit"}),
      )
      .unwrap(),
    );
    let completion = FactoryFlowActionAdapter::new(Arc::new(NotificationPlugin))
      .execute(FactoryFlowActionContext {
        run: &snapshot.run,
        work: &snapshot.work,
        definition: &definition,
        flow: &flow,
        node: &node,
        ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
        at: octacity_server_domain::Timestamp::from_unix_millis(30).unwrap(),
      })
      .await
      .unwrap();
    assert_eq!(completion.output_digest(), FactoryDigest::from_bytes([8; 32]));
  });
}

#[test]
fn invalid_action_binding_or_ownership_is_rejected_before_connector_effects() {
  crate::factory_admission_tests::run_ready(async {
    let snapshot = crate::factory_triage_tests::accepted_research_work().await;
    let reporter = Arc::new(Reporter(Mutex::new(vec![])));
    let adapter = FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("work-reporter"),
      reporter.clone(),
      outcome(),
    )));
    for invalid in 0..5 {
      let parameters = if invalid == 0 {
        serde_json::json!({"status":"Ready", "route":"implementation"})
      } else {
        serde_json::json!({"status":"Ready"})
      };
      let plugin = if invalid == 1 {
        reference("replacement-reporter")
      } else {
        reference("work-reporter")
      };
      let action = if invalid == 2 {
        key("unknown_action")
      } else {
        key("set_status")
      };
      let (definition, flow, node) =
        action_node(&snapshot, FlowActionBinding::new(plugin, action, parameters).unwrap());
      let ownership = if invalid == 3 {
        FactoryClaimOwnership::new(
          key("foreign"),
          FactoryClaim::new(
            FactoryClaimFence::new(FactoryDigest::from_bytes([99; 32])),
            octacity_server_domain::Timestamp::from_unix_millis(25).unwrap(),
            octacity_server_domain::Timestamp::from_unix_millis(90).unwrap(),
          )
          .unwrap(),
        )
      } else {
        FactoryClaimOwnership::new(node.owner().clone(), node.claim())
      };
      assert!(
        adapter
          .execute(FactoryFlowActionContext {
            run: &snapshot.run,
            work: &snapshot.work,
            definition: &definition,
            flow: &flow,
            node: &node,
            ownership,
            at: octacity_server_domain::Timestamp::from_unix_millis(if invalid == 4 { 99 } else { 30 }).unwrap(),
          })
          .await
          .is_err(),
        "invalid binding {invalid}"
      );
      assert!(reporter.0.lock().unwrap().is_empty());
    }
  });
}

struct UnavailableReporter;

struct ScopedReporter(Mutex<Vec<FactoryWorkStatusRequest>>);
#[async_trait]
impl FactoryWorkStatusProjector for ScopedReporter {
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    if !request
      .permissions
      .network_hosts()
      .any(|host| host.as_str() == "tickets.example")
      || !request
        .permissions
        .secret_profiles()
        .any(|profile| profile == &key("ticket-writer"))
    {
      return Err(ApplicationError::invalid());
    }
    self.0.lock().unwrap().push(request);
    Ok((FactoryDigest::from_bytes([7; 32]), BudgetUsage::default()))
  }
}

#[test]
fn status_connector_receives_exact_permissions_and_denies_missing_host_or_secret() {
  crate::factory_admission_tests::run_ready(async {
    let snapshot = crate::factory_triage_tests::accepted_research_work().await;
    let reporter = Arc::new(ScopedReporter(Mutex::new(vec![])));
    let adapter = FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("work-reporter"),
      reporter.clone(),
      outcome(),
    )));
    for missing in [Some("host"), Some("secret"), None] {
      let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
        plugins: vec![reference("work-reporter")],
        network_hosts: if missing == Some("host") {
          vec![]
        } else {
          vec![NetworkHost::new("tickets.example").unwrap()]
        },
        secret_profiles: if missing == Some("secret") {
          vec![]
        } else {
          vec![key("ticket-writer")]
        },
        ..Default::default()
      })
      .unwrap();
      let (definition, flow, node) = action_node_with_permissions(
        &snapshot,
        FlowActionBinding::new(
          reference("work-reporter"),
          key("set_status"),
          serde_json::json!({"status":"Ready"}),
        )
        .unwrap(),
        permissions.clone(),
      );
      let result = adapter
        .execute(FactoryFlowActionContext {
          run: &snapshot.run,
          work: &snapshot.work,
          definition: &definition,
          flow: &flow,
          node: &node,
          ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
          at: octacity_server_domain::Timestamp::from_unix_millis(30).unwrap(),
        })
        .await;
      if missing.is_some() {
        assert!(result.is_err());
        assert!(reporter.0.lock().unwrap().is_empty());
      } else {
        result.unwrap();
        assert_eq!(reporter.0.lock().unwrap()[0].permissions, permissions);
      }
    }
  });
}

#[test]
fn denied_action_plugin_cannot_reach_the_connector() {
  crate::factory_admission_tests::run_ready(async {
    let snapshot = crate::factory_triage_tests::accepted_research_work().await;
    let reporter = Arc::new(Reporter(Mutex::new(vec![])));
    let adapter = FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("work-reporter"),
      reporter.clone(),
      outcome(),
    )));
    let (definition, flow, node) = action_node_with_permissions(
      &snapshot,
      FlowActionBinding::new(
        reference("work-reporter"),
        key("set_status"),
        serde_json::json!({"status":"Ready"}),
      )
      .unwrap(),
      FactoryPermissionSet::deny_all(),
    );
    assert!(
      adapter
        .execute(FactoryFlowActionContext {
          run: &snapshot.run,
          work: &snapshot.work,
          definition: &definition,
          flow: &flow,
          node: &node,
          ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
          at: octacity_server_domain::Timestamp::from_unix_millis(30).unwrap(),
        })
        .await
        .is_err()
    );
    assert!(reporter.0.lock().unwrap().is_empty());
  });
}

#[async_trait]
impl FactoryWorkStatusProjector for UnavailableReporter {
  async fn set_status(&self, _: FactoryWorkStatusRequest) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    Err(ApplicationError::unavailable())
  }
}

#[test]
fn connector_failure_does_not_create_an_action_completion() {
  crate::factory_admission_tests::run_ready(async {
    let snapshot = crate::factory_triage_tests::accepted_research_work().await;
    let (definition, flow, node) = action_node(
      &snapshot,
      FlowActionBinding::new(
        reference("work-reporter"),
        key("set_status"),
        serde_json::json!({"status":"cannot_reproduce"}),
      )
      .unwrap(),
    );
    let adapter = FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("work-reporter"),
      Arc::new(UnavailableReporter),
      outcome(),
    )));
    let error = adapter
      .execute(FactoryFlowActionContext {
        run: &snapshot.run,
        work: &snapshot.work,
        definition: &definition,
        flow: &flow,
        node: &node,
        ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
        at: octacity_server_domain::Timestamp::from_unix_millis(30).unwrap(),
      })
      .await
      .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::Unavailable);
  });
}
