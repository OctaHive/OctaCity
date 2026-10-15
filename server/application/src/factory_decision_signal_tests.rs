use std::{
  collections::BTreeMap,
  future::Future,
  sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
  },
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalChoiceCriterion,
  DecisionSignalChoices, DecisionSignalConsumptionKind, DecisionSignalDigests, DecisionSignalDisposition,
  DecisionSignalFallback, DecisionSignalInputMedia, DecisionSignalMode, DecisionSignalProbability,
  DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProvider, DecisionSignalProviderCapability,
  DecisionSignalProviderFailure, DecisionSignalProviderFuture, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalProviderRequest, DecisionSignalProviderResult, DecisionSignalPurpose,
  DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind,
  DecisionSignalReceipt, DecisionSignalReceiptId, DecisionSignalRequest, DecisionSignalRequestId,
  DecisionSignalRouteSet, DecisionSignalState, DecisionSignalThreshold, ExactSubject, ExternalWorkIdentity,
  FactoryArtifactReference, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion,
  FactoryDigest, FactoryKey, FactoryMetadata, FactoryText, FlowNodeInput, ImmutableReference, NodeAttempt, RiskClass,
  ToolRiskChoiceMapping, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};
use octacity_server_store::FactoryDecisionSignalRequestDisposition;

#[path = "factory_decision_signal_recovery_tests.rs"]
mod recovery;

use super::factory_decision_signal::{
  DecisionSignalApplicationError, DecisionSignalClock, DecisionSignalPortError, DecisionSignalReceiptPublication,
  DecisionSignalRequestObservation, DecisionSignalService,
};

#[test]
fn installed_signal_node_rejects_ambiguous_disposition_mappings() {
  use crate::*;
  use octacity_server_factory::{FactoryPermissionSet, FlowDataMapping, FlowValueMapping, LocalPermissionCeiling};
  let plan = signal_plan_fixture(&provider_request(
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::Shadow,
  ));
  let (input, attempt) = node_fixture();
  let prepared = plan.prepare(&input, &attempt).unwrap();
  let provider = Arc::new(CountingProvider::successful(&prepared, "left"));
  let output = FactoryDecisionSignalNodeOutput {
    disposition: DecisionSignalDisposition::Route(key("right")),
    outcome: key("accepted"),
    projection: FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!({"count":9}),
    })
    .unwrap(),
  };
  let mut conflicting = output.clone();
  conflicting.outcome = key("rejected");
  let policy = FactoryDecisionSignalNodeBinding {
    action: octacity_server_factory::FlowActionBinding::new(
      plan.capability.adapter().clone(),
      key("installed-routing"),
      serde_json::json!({}),
    )
    .unwrap(),
    project_id: input.subject().project_id(),
    plan,
    baseline_route: key("right"),
    credential_profile: key("decision-credential"),
    network_host: octacity_server_factory::NetworkHost::new("jev.internal").unwrap(),
    project_permissions: FactoryPermissionSet::deny_all(),
    local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
    outputs: vec![output, conflicting],
  }
  .pin_policy()
  .unwrap();
  assert!(
    FactoryDecisionSignalNodeExecutor::new(
      Arc::new(()),
      vec![FactoryDecisionSignalNodeProfile {
        provider: provider.clone(),
        binding: policy.clone()
      }],
      Arc::new(SystemFactoryExecutionClock)
    )
    .is_err(),
    "installation must reject two different outcomes for the same consumed disposition"
  );
  let mut valid = policy;
  valid.outputs.pop();
  let pinned = valid.pin_policy().unwrap();
  let mut changed = pinned.clone();
  changed.plan.state_projection = FlowDataMapping::new(FlowValueMapping::Constant {
    value: serde_json::json!({"count":77}),
  })
  .unwrap();
  assert!(
    FactoryDecisionSignalNodeExecutor::new(
      Arc::new(()),
      vec![FactoryDecisionSignalNodeProfile {
        provider: provider.clone(),
        binding: changed
      }],
      Arc::new(SystemFactoryExecutionClock)
    )
    .is_err(),
    "changing an installed plan cannot preserve the action selected by an admitted node"
  );
  assert!(
    FactoryDecisionSignalNodeExecutor::new(
      Arc::new(()),
      vec![FactoryDecisionSignalNodeProfile {
        provider: provider.clone(),
        binding: pinned
      }],
      Arc::new(SystemFactoryExecutionClock)
    )
    .is_ok()
  );
  assert_eq!(provider.dispatches(), 0);
}

#[test]
fn installed_signal_node_uses_shadow_baseline_and_replays_without_another_provider_call() {
  use crate::*;
  use octacity_server_factory::*;
  use octacity_server_store::*;
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(Timestamp::from_unix_millis(15).unwrap())
    }
  }
  #[derive(Default)]
  struct Store {
    requests: Mutex<BTreeMap<DecisionSignalRequestId, DecisionSignalProviderRequest>>,
    receipts: Mutex<BTreeMap<DecisionSignalRequestId, DecisionSignalReceipt>>,
  }
  #[async_trait]
  impl FactoryDecisionSignalRequestStore for Store {
    async fn factory_decision_signal_request_reserved(
      &self,
      request: &DecisionSignalProviderRequest,
    ) -> Result<bool, StoreError> {
      Ok(self.requests.lock().unwrap().contains_key(&request.id()))
    }
    async fn reserve_factory_decision_signal_request(
      &self,
      request: ReserveFactoryDecisionSignalRequest,
    ) -> Result<FactoryDecisionSignalRequestDisposition, StoreError> {
      let old = self
        .requests
        .lock()
        .unwrap()
        .insert(request.request.id(), request.request);
      Ok(if old.is_some() {
        FactoryDecisionSignalRequestDisposition::Replayed
      } else {
        FactoryDecisionSignalRequestDisposition::Reserved
      })
    }
  }
  #[async_trait]
  impl FactoryDecisionSignalReceiptStore for Store {
    async fn factory_decision_signal_receipt(
      &self,
      request: &DecisionSignalProviderRequest,
    ) -> Result<Option<DecisionSignalReceipt>, StoreError> {
      Ok(self.receipts.lock().unwrap().get(&request.id()).cloned())
    }
    async fn publish_factory_decision_signal_receipt(
      &self,
      request: PublishFactoryDecisionSignalReceipt,
    ) -> Result<(), StoreError> {
      assert!(
        self
          .requests
          .lock()
          .unwrap()
          .contains_key(&request.receipt.request_id()),
        "external dispatch must be reserved first"
      );
      self
        .receipts
        .lock()
        .unwrap()
        .insert(request.receipt.request_id(), request.receipt);
      Ok(())
    }
  }
  run_ready(async {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
    let plan = signal_plan_fixture(&provider_request(
      DecisionSignalPurpose::Routing,
      DecisionSignalMode::Shadow,
    ));
    let plugin = plan.capability.adapter().clone();
    let binding = FlowActionBinding::new(
      plugin.clone(),
      key("operator-selected-operation"),
      serde_json::json!({"installed_contract": "routing-v1"}),
    )
    .unwrap();
    let mut grants = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
    grants["plugins"] = serde_json::json!([plugin]);
    grants["secret_profiles"] = serde_json::json!(["decision-credential"]);
    grants["network_hosts"] = serde_json::json!(["jev.internal"]);
    let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
    let observed = key("baseline-applied");
    let output = FlowOutcomeDefinition::new(
      observed.clone(),
      FlowOutcomeKind::Success,
      fixture.schema.reference().clone(),
    );
    let policy = FactoryDecisionSignalNodeBinding {
      action: binding,
      project_id: fixture.admission.work.subject().project_id(),
      plan,
      baseline_route: key("right"),
      credential_profile: key("decision-credential"),
      network_host: octacity_server_factory::NetworkHost::new("jev.internal").unwrap(),
      project_permissions: grants.clone(),
      local_permissions: LocalPermissionCeiling::fully_enforced(grants.clone()),
      outputs: vec![FactoryDecisionSignalNodeOutput {
        disposition: DecisionSignalDisposition::Route(key("right")),
        outcome: observed.clone(),
        projection: FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!({"count":9}),
        })
        .unwrap(),
      }],
    }
    .pin_policy()
    .unwrap();
    let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key("unfamiliar-observation-73"),
      kind: FlowNodeKind::DecisionSignal,
      input_schema: Some(fixture.schema.reference().clone()),
      outcomes: vec![output],
      budget: fixture.attempt.budget(),
      permissions: grants.clone(),
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_action(policy.action.clone())
    .unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: Some(fixture.schema.reference().clone()),
      entry: node.key().clone(),
      nodes: vec![node.clone()],
      transitions: vec![FlowTransition::new(
        node.key().clone(),
        observed.clone(),
        FlowTransitionTarget::Terminal(key("complete")),
      )],
      terminals: vec![FlowTerminalDefinition::new(
        key("complete"),
        fixture.schema.reference().clone(),
      )],
      context_projections: vec![],
      data_projections: vec![],
      execution: FlowExecutionPolicy::new(fixture.admission.flow.limits().budget(), grants.clone(), 1).unwrap(),
    })
    .unwrap();
    let flow = FlowRun::root(&fixture.admission.run, definition.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let mut limits = serde_json::to_value(fixture.admission.flow.limits()).unwrap();
    limits["permissions"] = serde_json::to_value(&grants).unwrap();
    let admitted = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      serde_json::from_value(limits).unwrap(),
      flow,
      cycle,
      fixture.admission.flow.wip_limits(),
    )
    .unwrap()
    .with_data_schemas(vec![fixture.schema.clone()])
    .unwrap();
    let input = FlowNodeInput::new(
      &fixture.admission.work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      node.key().clone(),
      FlowPayload::new(&fixture.schema, serde_json::json!({"count":3})).unwrap(),
    )
    .unwrap();
    let ownership = FactoryClaimOwnership::new(fixture.attempt.owner().clone(), fixture.attempt.claim());
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      admitted.closure().definition(input.definition()).unwrap(),
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: node.key().clone(),
        node_kind: FlowNodeKind::DecisionSignal,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: node.budget(),
        started_at: fixture.attempt.started_at(),
        deadline: fixture.attempt.deadline(),
        execution: NodeExecutionIdentity::External(policy.action.plugin().clone()),
        ownership: ownership.clone(),
      },
    )
    .unwrap();
    let request = policy.plan.prepare(&input, &attempt).unwrap();
    let provider = Arc::new(CountingProvider::successful(&request, "left"));
    let store = Arc::new(Store::default());
    let mut other = policy.clone();
    other.action = FlowActionBinding::new(
      other.action.plugin().clone(),
      key("another-installed-operation"),
      serde_json::json!({}),
    )
    .unwrap();
    other.outputs[0].projection = FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!({"count":77}),
    })
    .unwrap();
    let other = other.pin_policy().unwrap();
    let other_provider = Arc::new(CountingProvider::successful(&request, "left"));
    let executor = FactoryDecisionSignalNodeExecutor::new(
      store.clone(),
      vec![
        FactoryDecisionSignalNodeProfile {
          provider: other_provider.clone(),
          binding: other,
        },
        FactoryDecisionSignalNodeProfile {
          provider: provider.clone(),
          binding: policy,
        },
      ],
      Arc::new(Clock),
    )
    .unwrap();
    assert!(
      executor
        .cancel(
          FactoryNodeExecutionRequest {
            run_id: fixture.admission.run.id(),
            admitted: &admitted,
            work: &fixture.admission.work,
            input: &input,
            attempt: &attempt,
            build: None,
            ownership: ownership.clone(),
            observed_at: ownership.claim().expires_at(),
          },
          Timestamp::from_unix_millis(14).unwrap()
        )
        .await
        .is_err(),
      "an expired observer cannot declare even an undispatched operation stopped"
    );
    assert_eq!(provider.dispatches(), 0);
    for _ in 0..2 {
      let step = executor
        .observe(FactoryNodeExecutionRequest {
          run_id: fixture.admission.run.id(),
          admitted: &admitted,
          work: &fixture.admission.work,
          input: &input,
          attempt: &attempt,
          build: None,
          ownership: ownership.clone(),
          observed_at: Timestamp::from_unix_millis(15).unwrap(),
        })
        .await
        .unwrap();
      let FactoryNodeExecutionStep::Completed { record, usage, .. } = step else {
        panic!("a retained signal must complete the configured node");
      };
      assert_eq!(record.metadata().unwrap()["outcome"], serde_json::json!(observed));
      assert_eq!(record.payload().value(), &serde_json::json!({"count":9}));
      let facts = serde_json::to_value(record.discovery().unwrap()).unwrap();
      assert_eq!(
        facts["consumption"]["final_disposition"],
        serde_json::json!({"Route":"right"})
      );
      assert!(facts.get("request").is_none());
      assert_eq!(usage.tokens, 10);
    }
    assert_eq!(provider.dispatches(), 1);
    assert_eq!(
      other_provider.dispatches(),
      0,
      "a different installed action cannot select its provider for this node"
    );
    assert_eq!(store.requests.lock().unwrap().len(), 1);
    assert_eq!(store.receipts.lock().unwrap().len(), 1);
  });
}

fn signal_plan_fixture(template: &DecisionSignalProviderRequest) -> crate::FactoryDecisionSignalRequestPlan {
  use octacity_server_factory::{FlowDataMapping, FlowDataSchema, FlowFieldSchema, FlowValueMapping, FlowValueSchema};
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::Routing,
    provider: template.provider().clone(),
    adapter: template.adapter().clone(),
    model: template.model().clone(),
    question_set: template.question_set().clone(),
    policy: template.policy().clone(),
    mode: template.mode(),
    fallback: template.fallback(),
    budget: template.request().budget(),
    routes: Some(DecisionSignalRouteSet::new(
      template.input().routing_state().unwrap().clone(),
      template.input().choices().clone(),
    )),
  })
  .unwrap();
  let schema = FlowDataSchema::new(
    key("projected.signal-state"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("count"),
        FlowFieldSchema::required(FlowValueSchema::Integer {
          minimum: 0,
          maximum: 100,
        }),
      )]),
    },
  )
  .unwrap();
  crate::FactoryDecisionSignalRequestPlan {
    profile,
    capability: capability(template),
    questions: template.input().questions().to_vec(),
    consuming_question: key("route"),
    threshold: template.threshold().clone(),
    state_schema: schema,
    state_projection: FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!({"count":42}),
    })
    .unwrap(),
    call_budget: BudgetLimit::new(1, 100, 10, 100, 100).unwrap(),
  }
}

#[test]
fn signal_request_preparation_freezes_projection_identity_budget_and_deadline_without_owner_names() {
  let template = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::Shadow);
  let (input, node) = node_fixture();
  let plan = signal_plan_fixture(&template);
  let first = plan.prepare(&input, &node).unwrap();
  assert_eq!(first, plan.prepare(&input, &node).unwrap());
  assert_eq!(first.input().state_document().as_str(), "{\"count\":42}");
  assert_eq!(first.deadline(), node.deadline());
  assert_eq!(first.request().frozen_input_digest(), input.digest().unwrap());
  assert_eq!(first.request().budget(), plan.call_budget);
  assert_eq!(first.model(), template.model());
  let mut longer = serde_json::to_value(&node).unwrap();
  longer["deadline"] = serde_json::to_value(Timestamp::from_unix_millis(1000).unwrap()).unwrap();
  let longer: NodeAttempt = serde_json::from_value(longer).unwrap();
  assert_eq!(
    plan.prepare(&input, &longer).unwrap().deadline(),
    Timestamp::from_unix_millis(110).unwrap(),
    "the call ceiling cannot inherit a longer Node window"
  );

  let mut other = serde_json::to_value(&node).unwrap();
  other["id"] = serde_json::json!(octacity_server_factory::NodeAttemptId::generate());
  let other: NodeAttempt = serde_json::from_value(other).unwrap();
  assert_ne!(first.id(), plan.prepare(&input, &other).unwrap().id());
  let mut unbounded = plan;
  unbounded.call_budget = BudgetLimit::new(1, node.budget().max_elapsed_millis() + 1, 10, 100, 100).unwrap();
  assert!(
    unbounded.prepare(&input, &node).is_err(),
    "dispatch cannot exceed the common Node budget"
  );
}

#[test]
fn changed_capability_cannot_settle_a_reserved_operation_as_free_unavailability() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let mut provider = CountingProvider::successful(&request, "right");
    let mut changed = serde_json::to_value(provider.capability()).unwrap();
    changed["model"] = serde_json::json!(ImmutableReference::new(
      key("another-model"),
      key("v1"),
      FactoryDigest::from_bytes([99; 32])
    ));
    provider.capability = serde_json::from_value(changed).unwrap();
    let provider = Arc::new(provider);
    let receipts = Arc::new(MemoryReceipts::default());
    crate::DecisionSignalDispatchAuthorization::authorize_dispatch(receipts.as_ref(), &request)
      .await
      .unwrap();
    assert!(
      service(provider.clone(), receipts.clone())
        .route(DecisionSignalReceiptId::generate(), &request, key("left"))
        .await
        .is_err(),
      "a capability failure cannot erase a previously reserved call's unknown usage"
    );
    assert_eq!(provider.dispatches(), 0);
    assert_eq!(receipts.len(), 0);
  });
}

#[test]
fn retrying_an_unknown_signal_within_the_deadline_cannot_dispatch_the_provider_again() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::Shadow);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    *provider.result.lock().unwrap() = Some(Err(
      DecisionSignalProviderFailure::unobserved(
        request.id(),
        request.provider().clone(),
        request.adapter().clone(),
        request.model().clone(),
        DecisionSignalState::Unavailable,
      )
      .unwrap(),
    ));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts.clone(),
      Arc::new(FixedClock(Timestamp::from_unix_millis(100).unwrap())),
    );
    for _ in 0..2 {
      assert!(matches!(
        service
          .route(DecisionSignalReceiptId::generate(), &request, key("left"))
          .await,
        Err(DecisionSignalApplicationError::AwaitingProviderObservation)
      ));
    }
    assert_eq!(
      provider.dispatches(),
      1,
      "an unresolved dispatch reservation is not permission for a second external evaluation"
    );
    assert_eq!(
      receipts.len(),
      0,
      "unknown usage cannot become a synthetic free receipt"
    );
  });
}

#[test]
fn simultaneous_identical_signals_reserve_only_one_provider_dispatch() {
  struct PendingProvider(DecisionSignalProviderCapability, std::sync::atomic::AtomicUsize);
  impl DecisionSignalProvider for PendingProvider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.0
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      Box::pin(std::future::pending())
    }
  }
  let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::Shadow);
  let provider = Arc::new(PendingProvider(
    capability(&request),
    std::sync::atomic::AtomicUsize::new(0),
  ));
  let receipts = Arc::new(MemoryReceipts::default());
  let service = DecisionSignalService::for_provider_with_clock(
    provider.clone(),
    receipts.clone(),
    receipts.clone(),
    receipts.clone(),
    Arc::new(FixedClock(Timestamp::from_unix_millis(100).unwrap())),
  );
  let mut first = std::pin::pin!(service.route(DecisionSignalReceiptId::generate(), &request, key("left")));
  let mut second = std::pin::pin!(service.route(DecisionSignalReceiptId::generate(), &request, key("left")));
  let mut context = Context::from_waker(Waker::noop());
  assert!(first.as_mut().poll(&mut context).is_pending());
  assert!(matches!(
    second.as_mut().poll(&mut context),
    Poll::Ready(Err(DecisionSignalApplicationError::AwaitingProviderObservation))
  ));
  assert_eq!(provider.1.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn dispatch_authorization_that_finishes_at_deadline_cannot_start_an_external_call() {
  struct Expiring(Arc<MutableClock>, Timestamp);
  #[async_trait]
  impl crate::DecisionSignalDispatchAuthorization for Expiring {
    async fn was_reserved(&self, _: &DecisionSignalProviderRequest) -> Result<bool, DecisionSignalPortError> {
      Ok(true)
    }
    async fn authorize_dispatch(
      &self,
      _: &DecisionSignalProviderRequest,
    ) -> Result<FactoryDecisionSignalRequestDisposition, DecisionSignalPortError> {
      self.0.set(self.1.unix_millis());
      Ok(FactoryDecisionSignalRequestDisposition::Reserved)
    }
  }
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    let clock = Arc::new(MutableClock::new(100));
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      Arc::new(Expiring(clock.clone(), request.deadline())),
      clock,
    );
    assert!(
      service
        .route(DecisionSignalReceiptId::generate(), &request, key("left"))
        .await
        .is_err()
    );
    assert_eq!(
      provider.dispatches(),
      0,
      "time must be read again after asynchronous authorization"
    );
    assert_eq!(receipts.len(), 0);
  });
}

#[test]
fn an_expired_reserved_call_waits_for_measured_observation_without_inventing_zero_usage() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    crate::DecisionSignalDispatchAuthorization::authorize_dispatch(receipts.as_ref(), &request)
      .await
      .unwrap();
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts.clone(),
      Arc::new(FixedClock(request.deadline())),
    );
    assert!(
      service
        .route(DecisionSignalReceiptId::generate(), &request, key("left"))
        .await
        .is_err(),
      "a previously reserved operation may have spent resources; its missing observation is not zero usage"
    );
    assert_eq!(provider.dispatches(), 0);
    assert_eq!(receipts.len(), 0);
  });
}

#[test]
fn rejected_dispatch_authority_never_contacts_the_provider_or_publishes_a_receipt() {
  struct Denied;
  #[async_trait]
  impl crate::DecisionSignalDispatchAuthorization for Denied {
    async fn was_reserved(&self, _: &DecisionSignalProviderRequest) -> Result<bool, DecisionSignalPortError> {
      Ok(false)
    }
    async fn authorize_dispatch(
      &self,
      _: &DecisionSignalProviderRequest,
    ) -> Result<FactoryDecisionSignalRequestDisposition, DecisionSignalPortError> {
      Err(DecisionSignalPortError::Conflict)
    }
  }
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      Arc::new(Denied),
      Arc::new(FixedClock(Timestamp::from_unix_millis(100).unwrap())),
    );
    assert!(matches!(
      service
        .route(DecisionSignalReceiptId::generate(), &request, key("left"))
        .await,
      Err(DecisionSignalApplicationError::Authorization(
        DecisionSignalPortError::Conflict
      ))
    ));
    assert_eq!(provider.dispatches(), 0);
    assert_eq!(receipts.len(), 0);
  });
}

#[test]
fn exact_replay_observes_the_receipt_without_requerying_or_switching_models() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = service(provider.clone(), receipts.clone());
    let receipt_id = DecisionSignalReceiptId::generate();

    let first = service.route(receipt_id, &request, key("left")).await.unwrap();
    let replay = service.route(receipt_id, &request, key("left")).await.unwrap();

    assert_eq!(first, replay);
    assert_eq!(provider.dispatches(), 1);
    assert_eq!(replay.model(), request.model());
    assert_eq!(receipts.len(), 1);
  });
}

#[test]
fn provider_failure_is_purpose_scoped() {
  run_ready(async {
    let routing = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let routing_provider = Arc::new(CountingProvider::failing(&routing));
    let routing_receipts = Arc::new(MemoryReceipts::default());
    let failed = service(routing_provider, routing_receipts)
      .route(DecisionSignalReceiptId::generate(), &routing, key("left"))
      .await
      .unwrap();
    assert_eq!(failed.state(), DecisionSignalState::Unavailable);
    assert_eq!(failed.request().request().purpose(), DecisionSignalPurpose::Routing);

    let tool_risk = provider_request(DecisionSignalPurpose::ToolRisk, DecisionSignalMode::BoundedControl);
    let tool_provider = Arc::new(CountingProvider::successful(&tool_risk, "right"));
    let tool_receipts = Arc::new(MemoryReceipts::default());
    let mapping = ToolRiskChoiceMapping::try_new(
      tool_risk.choices(),
      vec![
        (key("left"), DecisionSignalDisposition::Allow),
        (key("right"), DecisionSignalDisposition::Deny),
      ],
    )
    .unwrap();
    let assessed = service(tool_provider, tool_receipts)
      .assess_tool_risk(DecisionSignalReceiptId::generate(), &tool_risk, &mapping)
      .await
      .unwrap();
    assert_eq!(assessed.state(), DecisionSignalState::Completed);
    assert_eq!(assessed.request().request().purpose(), DecisionSignalPurpose::ToolRisk);
    assert_eq!(
      assessed.consumption().final_disposition(),
      &DecisionSignalDisposition::Deny
    );
  });
}

#[test]
fn shadow_never_changes_execution() {
  run_ready(async {
    let shadow = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::Shadow);
    let shadow_provider = Arc::new(CountingProvider::successful(&shadow, "right"));
    let shadow_receipts = Arc::new(MemoryReceipts::default());
    let compared = service(shadow_provider, shadow_receipts)
      .route(DecisionSignalReceiptId::generate(), &shadow, key("left"))
      .await
      .unwrap();
    assert_eq!(compared.consumption().kind(), DecisionSignalConsumptionKind::Shadow);
    assert_eq!(
      compared.consumption().final_disposition(),
      &DecisionSignalDisposition::Route(key("left"))
    );
  });
}

#[test]
fn expired_requests_publish_a_timeout_without_contacting_the_provider() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let provider = Arc::new(CountingProvider::successful(&request, "right"));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts,
      Arc::new(FixedClock(request.deadline())),
    );

    let receipt = service
      .route(DecisionSignalReceiptId::generate(), &request, key("left"))
      .await
      .unwrap();

    assert_eq!(receipt.state(), DecisionSignalState::TimedOut);
    assert_eq!(provider.dispatches(), 0);
  });
}

#[test]
fn provider_results_completed_after_the_deadline_are_consumed_as_timeouts() {
  run_ready(async {
    let request = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
    let clock = Arc::new(MutableClock::new(100));
    let provider = Arc::new(CountingProvider::successful_with_completion_time(
      &request,
      "right",
      clock.clone(),
      request.deadline().unix_millis(),
    ));
    let receipts = Arc::new(MemoryReceipts::default());
    let service = DecisionSignalService::for_provider_with_clock(
      provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts,
      clock,
    );

    let receipt = service
      .route(DecisionSignalReceiptId::generate(), &request, key("left"))
      .await
      .unwrap();

    assert_eq!(receipt.state(), DecisionSignalState::TimedOut);
    assert_eq!(provider.dispatches(), 1);
    assert_eq!(receipt.usage().tokens, 10);
  });
}

fn service(provider: Arc<CountingProvider>, receipts: Arc<MemoryReceipts>) -> DecisionSignalService {
  DecisionSignalService::for_provider_with_clock(
    provider,
    receipts.clone(),
    receipts.clone(),
    receipts,
    Arc::new(FixedClock(Timestamp::from_unix_millis(100).unwrap())),
  )
}

struct FixedClock(Timestamp);

impl DecisionSignalClock for FixedClock {
  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError> {
    Ok(self.0)
  }
}

struct MutableClock(AtomicI64);

impl MutableClock {
  fn new(value: i64) -> Self {
    Self(AtomicI64::new(value))
  }

  fn set(&self, value: i64) {
    self.0.store(value, Ordering::Release);
  }
}

impl DecisionSignalClock for MutableClock {
  fn now(&self) -> Result<Timestamp, DecisionSignalApplicationError> {
    Ok(Timestamp::from_unix_millis(self.0.load(Ordering::Acquire)).unwrap())
  }
}

struct CountingProvider {
  capability: DecisionSignalProviderCapability,
  result: Mutex<Option<Result<DecisionSignalProviderResult, DecisionSignalProviderFailure>>>,
  dispatches: Mutex<u32>,
  completion_clock: Option<(Arc<MutableClock>, i64)>,
}

impl CountingProvider {
  fn successful(request: &DecisionSignalProviderRequest, selected: &str) -> Self {
    Self {
      capability: capability(request),
      result: Mutex::new(Some(Ok(result(request, selected)))),
      dispatches: Mutex::new(0),
      completion_clock: None,
    }
  }

  fn successful_with_completion_time(
    request: &DecisionSignalProviderRequest,
    selected: &str,
    clock: Arc<MutableClock>,
    completion_time: i64,
  ) -> Self {
    Self {
      capability: capability(request),
      result: Mutex::new(Some(Ok(result(request, selected)))),
      dispatches: Mutex::new(0),
      completion_clock: Some((clock, completion_time)),
    }
  }

  fn failing(request: &DecisionSignalProviderRequest) -> Self {
    Self {
      capability: capability(request),
      result: Mutex::new(Some(Err(
        DecisionSignalProviderFailure::new(
          request.id(),
          request.provider().clone(),
          request.adapter().clone(),
          request.model().clone(),
          DecisionSignalState::Unavailable,
          BudgetUsage::default(),
        )
        .unwrap(),
      ))),
      dispatches: Mutex::new(0),
      completion_clock: None,
    }
  }

  fn dispatches(&self) -> u32 {
    *self.dispatches.lock().unwrap()
  }
}

impl DecisionSignalProvider for CountingProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }

  fn evaluate<'a>(&'a self, _request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    *self.dispatches.lock().unwrap() += 1;
    if let Some((clock, completion_time)) = &self.completion_clock {
      clock.set(*completion_time);
    }
    let result = self.result.lock().unwrap().clone().expect("fixture result");
    Box::pin(async move { result })
  }
}

#[derive(Default)]
struct MemoryReceipts {
  values: Mutex<BTreeMap<DecisionSignalRequestId, DecisionSignalReceipt>>,
  reservations: Mutex<BTreeMap<DecisionSignalRequestId, DecisionSignalProviderRequest>>,
}

impl MemoryReceipts {
  fn len(&self) -> usize {
    self.values.lock().unwrap().len()
  }
}

#[async_trait]
impl DecisionSignalRequestObservation for MemoryReceipts {
  async fn observe_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalPortError> {
    Ok(self.values.lock().unwrap().get(&request.id()).cloned())
  }
}

#[async_trait]
impl DecisionSignalReceiptPublication for MemoryReceipts {
  async fn publish_receipt(
    &self,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalPortError> {
    let mut values = self.values.lock().unwrap();
    if let Some(existing) = values.get(&receipt.request_id()) {
      return (existing == &receipt)
        .then(|| existing.clone())
        .ok_or(DecisionSignalPortError::Conflict);
    }
    values.insert(receipt.request_id(), receipt.clone());
    Ok(receipt)
  }
}

fn provider_request(purpose: DecisionSignalPurpose, mode: DecisionSignalMode) -> DecisionSignalProviderRequest {
  let node = node_fixture();
  let choices = DecisionSignalChoices::try_new(vec![key("left"), key("right")]).unwrap();
  let routes = (purpose == DecisionSignalPurpose::Routing)
    .then(|| DecisionSignalRouteSet::new(key("current-state"), choices.clone()));
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose,
    provider: exact("provider", "v1", 3),
    adapter: exact("adapter", "v1", 4),
    model: exact("model", "v7", 5),
    question_set: exact("questions", "v2", 6),
    policy: exact("policy", "v4", 7),
    mode,
    fallback: DecisionSignalFallback::Escalate,
    budget: budget(),
    routes,
  })
  .unwrap();
  let input = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"state\":\"redacted\"}").unwrap(),
    vec![
      DecisionSignalQuestion::new(
        exact("route", "v1", 8),
        FactoryText::new("Choose the declared route that matches the redacted state.").unwrap(),
        DecisionSignalQuestionDomain::FiniteChoice(choices),
        DecisionSignalQuestionCriteria::FiniteChoice(vec![
          DecisionSignalChoiceCriterion::new(
            key("left"),
            FactoryText::new("Use the left route for the first declared case.").unwrap(),
          ),
          DecisionSignalChoiceCriterion::new(
            key("right"),
            FactoryText::new("Use the right route for the second declared case.").unwrap(),
          ),
        ]),
      )
      .unwrap(),
    ],
    key("route"),
    (purpose == DecisionSignalPurpose::Routing).then(|| key("current-state")),
  )
  .unwrap();
  let capability = capability_for_profile(&profile);
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &node.0,
    &node.1,
    purpose,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  )
  .expect("fixture signal request");
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      purpose,
      capability.probability_semantics().clone(),
      exact("calibration", "v1", 10),
      DecisionSignalProbability::new(600_000).unwrap(),
      DecisionSignalProbability::new(100_000).unwrap(),
    )
    .unwrap(),
    Timestamp::from_unix_millis(10_000).unwrap(),
  )
  .unwrap()
}

pub(super) fn capability(request: &DecisionSignalProviderRequest) -> DecisionSignalProviderCapability {
  DecisionSignalProviderCapability::new(
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    request.data_handling().clone(),
    vec![request.request().purpose()],
    vec![request.input().media()],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn capability_for_profile(profile: &DecisionSignalProfile) -> DecisionSignalProviderCapability {
  DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    exact("semantics", "v1", 9),
    exact("private-data", "v1", 11),
    vec![profile.purpose()],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 20, 24).unwrap(),
    budget(),
  )
  .unwrap()
}

fn result(request: &DecisionSignalProviderRequest, selected: &str) -> DecisionSignalProviderResult {
  DecisionSignalProviderResult::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    vec![DecisionSignalAnswer::new(
      request.input().questions()[0].reference().clone(),
      DecisionSignalAnswerValue::FiniteChoice {
        selected: key(selected),
        selected_probability: DecisionSignalProbability::new(800_000).unwrap(),
        runner_up_probability: DecisionSignalProbability::new(200_000).unwrap(),
      },
    )],
    BudgetUsage {
      attempts: 1,
      elapsed_millis: 5,
      tokens: 10,
      cost_micro_units: 0,
      output_bytes: 20,
    },
  )
  .unwrap()
}

fn node_fixture() -> (FlowNodeInput, NodeAttempt) {
  let subject = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("base").unwrap(),
  );
  let configuration = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    subject.project_id(),
    digest(1),
  );
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration,
    ExternalWorkIdentity::new("source/1").unwrap(),
    subject,
    WorkArtifacts::new(artifact(90), artifact(91), vec![]).unwrap(),
    WorkClassification::new(
      WorkPriority::new(0).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap();
  octacity_server_factory::testing::capability_node_fixture(&work, None)
}
fn budget() -> BudgetLimit {
  BudgetLimit::new(2, 10_000, 10_000, 2_000_000, 10_000).unwrap()
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}

fn exact(identity: &str, version: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(value))
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory Decision Signal future must be immediately ready"),
  }
}

#[async_trait]
impl crate::DecisionSignalDispatchAuthorization for MemoryReceipts {
  async fn was_reserved(&self, request: &DecisionSignalProviderRequest) -> Result<bool, DecisionSignalPortError> {
    Ok(self.reservations.lock().unwrap().get(&request.id()) == Some(request))
  }
  async fn authorize_dispatch(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<FactoryDecisionSignalRequestDisposition, DecisionSignalPortError> {
    let mut values = self.reservations.lock().unwrap();
    if let Some(existing) = values.get(&request.id()) {
      return if existing == request {
        Ok(FactoryDecisionSignalRequestDisposition::Replayed)
      } else {
        Err(DecisionSignalPortError::Conflict)
      };
    }
    values.insert(request.id(), request.clone());
    Ok(FactoryDecisionSignalRequestDisposition::Reserved)
  }
}
