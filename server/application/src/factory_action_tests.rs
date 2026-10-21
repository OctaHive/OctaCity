//! Public control/description contracts over real retained owner snapshots.
use super::*;
#[path = "factory_action_projection_tests.rs"]
mod projection;

async fn pending(
  admission: PublishedFactoryAdmission,
) -> (Arc<testing::InMemoryFactoryConfigurationStore>, FactoryRunSnapshot) {
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store
    .seed_factory_admission(
      admission.clone(),
      FactoryWorkSourceScope {
        source: key("manual"),
        security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      },
      FactoryDigest::from_bytes([1; 32]),
      testing::management_mutation(()).audit(),
    )
    .unwrap();
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let ownership = claim(store.as_ref(), &snapshot, 10).await;
  let owner = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(NoExternalEffects),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  for time in 11..20 {
    owner
      .reconcile_flow(admission.run.id(), ownership.id, at(time))
      .await
      .unwrap();
    if store
      .factory_run_snapshot(admission.run.id())
      .await
      .unwrap()
      .flow
      .attempts
      .iter()
      .any(|row| row.node_kind() == FlowNodeKind::HumanGate)
    {
      break;
    }
  }
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  (store, snapshot)
}

#[test]
fn human_control_eligibility_preserves_original_time_and_recorded_intent_boundaries() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let attempt = &snapshot.flow.attempts[0];
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(snapshot.flow.runs[0].definition())
      .unwrap();
    let schema = definition
      .node(attempt.node_key())
      .unwrap()
      .outcome(&key("approved"))
      .unwrap()
      .schema();
    let payload = FlowPayload::new(
      snapshot.admitted_flow.data_schema(schema).unwrap(),
      serde_json::json!(true),
    )
    .unwrap();
    assert!(validate_factory_node_response(&snapshot, attempt.id(), &key("approved"), &payload, at(9)).is_err());
    assert!(
      validate_factory_node_response(&snapshot, attempt.id(), &key("approved"), &payload, attempt.deadline()).is_ok(),
      "the admitted deadline is inclusive"
    );
    assert!(
      validate_factory_node_response(
        &snapshot,
        attempt.id(),
        &key("approved"),
        &payload,
        at(attempt.deadline().unix_millis() + 1)
      )
      .is_err()
    );
    let command = crate::SubmitFactoryNodeResponseCommand {
      run_id: snapshot.run.id(),
      node_attempt_id: attempt.id(),
      outcome: key("approved"),
      value: serde_json::json!(true),
      expected_version: snapshot.run.version(),
      idempotency_key: IdempotencyKey::new("characterize-response").unwrap(),
      requested_at: at(12),
    };
    FactoryControlHandlers::new(store.clone())
      .execute_management_command(
        &context(),
        &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
        command,
      )
      .await
      .unwrap();
    let recorded = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    assert!(
      recorded.flow.completions.is_empty(),
      "intent has not been consumed by the owner"
    );
    assert!(
      validate_factory_node_response(&recorded, attempt.id(), &key("approved"), &payload, at(13)).is_err(),
      "recorded intent is no longer unanswered"
    );
  });
}

#[test]
fn issue_actions_describe_the_original_gate_contract_without_reserving_control() {
  crate::factory_flow_tests::run_ready(async {
    let admission = human_fixture();
    let (store, snapshot) = pending(admission.clone()).await;
    let issues = crate::issue_test_support::manual_issue_store(snapshot.work.subject().project_id()).await;
    let policy = Arc::new(crate::TrustedNetworkManagementPolicy);
    let issue = crate::AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(crate::IssueHandlers::new(issues.clone(), policy)),
    )
    .handle_command(
      &context(),
      crate::CreateIssueCommand {
        intent: crate::CreateIssueIntent {
          project_id: snapshot.work.subject().project_id(),
          planning: Default::default(),
          content: crate::IssueContent {
            title: "Review this task".into(),
            description: String::new(),
            acceptance: None,
            priority: None,
          },
        },
        idempotency_key: "action-issue".parse().unwrap(),
        created_at: at(1),
      },
    )
    .await
    .unwrap()
    .issue;
    let association = issues
      .associate_issue_work(testing::management_mutation(AssociateIssueWork {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: None,
        admission,
      }))
      .await
      .unwrap()
      .association;
    let descriptions = crate::describe_issue_factory_actions(&association, &snapshot, at(12)).unwrap();
    assert_eq!(descriptions.len(), 1);
    let described = &descriptions[0];
    assert_eq!(described.association, association);
    let action = &described.action;
    let attempt = &snapshot.flow.attempts[0];
    assert_eq!(action.work_id, snapshot.work.id());
    assert_eq!(action.run_id, snapshot.run.id());
    assert_eq!(action.run_version, snapshot.run.version());
    assert_eq!(action.flow_run_id, attempt.flow_run_id());
    assert_eq!(action.cycle_id, attempt.workflow_cycle_id());
    assert_eq!(action.node_attempt_id, attempt.id());
    assert_eq!(action.input_digest, attempt.input_digest());
    assert_eq!(action.contract, snapshot.flow.runs[0].definition());
    assert_eq!(action.actionable_at, at(11));
    assert_eq!(action.deadline, Some(attempt.deadline()));
    assert_eq!(action.candidate, None, "a task association cannot invent a candidate");
    let crate::FactoryNodeActionKind::Respond { outcomes, binding, .. } = &action.kind else {
      panic!("declared response");
    };
    assert_eq!(binding, &None);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].key(), &key("approved"));
    assert_eq!(outcomes[0].schema().identity(), &key("operator.response"));
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
    assert_eq!(issues.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    let mut foreign = association;
    foreign.run_id = FactoryRunId::generate();
    assert!(crate::describe_issue_factory_actions(&foreign, &snapshot, at(12)).is_err());
  });
}

#[test]
fn action_descriptions_offer_only_original_policy_permitted_retries() {
  crate::factory_flow_tests::run_ready(async {
    for allowance in [0, 1] {
      let mut fixture = testing::factory_node_journal_contract_fixture_with_retries(allowance);
      let candidate = CandidateSubject::new(
        fixture.admission.work.subject().clone(),
        octacity_server_domain::ImmutableRevision::new("original-head").unwrap(),
        FactoryDigest::from_bytes([97; 32]),
      );
      fixture.input = fixture
        .input
        .with_context(
          ContextManifest::new(
            ContextManifestId::generate(),
            FactoryTaskSubject::Candidate(candidate.clone()),
            FactoryDigest::from_bytes([63; 32]),
            vec![],
          )
          .unwrap(),
          vec![],
        )
        .unwrap();
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      store
        .seed_factory_run(
          fixture.admission.clone(),
          FactoryDigest::from_bytes([1; 32]),
          testing::management_mutation(()).audit(),
        )
        .unwrap();
      crate::factory_node_failure_tests::verify_failed_node_owner(
        store.clone(),
        store.clone(),
        &fixture,
        (
          octacity_server_domain::BuildId::generate(),
          octacity_server_domain::AttemptId::generate(),
          octacity_server_domain::JobId::generate(),
        ),
      )
      .await;
      let stopped = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      let attempt = &stopped.flow.attempts[0];
      let description = crate::FactoryNodeActionDescription::from_snapshot(&stopped, attempt.id(), at(14));
      if allowance == 0 {
        assert!(description.is_none(), "failure alone cannot authorize a retry");
        continue;
      }
      let description = description.expect("the admitted policy permits one explicit retry");
      assert_eq!(serde_json::to_value(&description.kind).unwrap()["kind"], "retry");
      assert_eq!(description.node_attempt_id, attempt.id());
      assert_eq!(description.candidate, Some(candidate));
      assert_eq!(description.actionable_at, at(12));
      assert_eq!(
        description.deadline, None,
        "an expired attempt deadline is not a new retry policy"
      );
      let context = context();
      FactoryControlHandlers::new(store.clone())
        .execute_management_command(
          &context,
          &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
          crate::RetryFactoryNodeCommand {
            run_id: stopped.run.id(),
            node_attempt_id: attempt.id(),
            expected_version: stopped.run.version(),
            idempotency_key: "described-retry".parse().unwrap(),
            requested_at: at(14),
          },
        )
        .await
        .unwrap();
      let recorded = store.factory_run_snapshot(stopped.run.id()).await.unwrap();
      assert!(
        crate::FactoryNodeActionDescription::from_snapshot(&recorded, attempt.id(), at(15)).is_none(),
        "recorded retry is no longer unanswered"
      );
    }
  });
}

fn with_definitions(prior: PublishedFactoryAdmission, definitions: Vec<FlowDefinition>) -> PublishedFactoryAdmission {
  let root = definitions[0].reference();
  let run = FactoryRun::admitted(prior.run.id(), &prior.work);
  let flow = FlowRun::root(&run, root).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(root, definitions).unwrap(),
    prior.flow.limits().clone(),
    flow,
    cycle,
    prior.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(prior.flow.data_schemas().to_vec())
  .unwrap();
  PublishedFactoryAdmission {
    work: prior.work,
    run,
    flow: admitted,
    admitted_at: prior.admitted_at,
  }
}

#[test]
fn bound_action_descriptions_offer_only_the_pinned_operation_choices() {
  crate::factory_flow_tests::run_ready(async {
    for (operation, disposition) in [
      (FlowHumanOperation::Delivery, FlowHumanDisposition::Confirm),
      (FlowHumanOperation::Promotion, FlowHumanDisposition::Confirm),
      (FlowHumanOperation::Escalation, FlowHumanDisposition::Resume),
    ] {
      let binding = FlowHumanGateBinding::new(
        operation,
        std::collections::BTreeMap::from([(disposition, key("approved"))]),
      )
      .unwrap();
      let schema = FlowDataSchema::new(
        key("operator.response"),
        key("v1"),
        binding.response_shape(disposition).unwrap(),
      )
      .unwrap();
      let prior = human_fixture_with_binding(schema, Some(binding.clone()));
      let original = prior.flow.closure().definition(prior.flow.closure().root()).unwrap();
      let mut node = serde_json::to_value(&original.nodes()[0]).unwrap();
      let mut extra = node["outcomes"][0].clone();
      extra["key"] = serde_json::json!("not_a_response");
      node["outcomes"].as_array_mut().unwrap().push(extra);
      let mut transitions = original.transitions().to_vec();
      transitions.push(FlowTransition::new(
        original.entry().clone(),
        key("not_a_response"),
        FlowTransitionTarget::Terminal(key("reviewed")),
      ));
      let definition = FlowDefinition::new(FlowDefinitionInput {
        id: original.reference().id(),
        version: original.reference().version(),
        input_schema: original.input_schema().cloned(),
        entry: original.entry().clone(),
        nodes: vec![serde_json::from_value(node).unwrap()],
        transitions,
        terminals: original.terminals().to_vec(),
        context_projections: vec![],
        data_projections: vec![],
        execution: original.execution().clone(),
      })
      .unwrap();
      let (_, snapshot) = pending(with_definitions(prior, vec![definition])).await;
      let attempt = &snapshot.flow.attempts[0];
      let action = crate::FactoryNodeActionDescription::from_snapshot(&snapshot, attempt.id(), at(12)).unwrap();
      let crate::FactoryNodeActionKind::Respond {
        outcomes,
        binding: described,
        ..
      } = action.kind
      else {
        panic!("bound human operation");
      };
      assert_eq!(described, Some(binding));
      assert_eq!(
        outcomes.len(),
        1,
        "a declared outcome outside the operation binding is not an allowed operator response"
      );
      assert_eq!(outcomes[0].key(), &key("approved"));
      assert!(
        crate::FactoryNodeActionDescription::from_snapshot(&snapshot, attempt.id(), attempt.deadline()).is_some()
      );
      assert!(
        crate::FactoryNodeActionDescription::from_snapshot(
          &snapshot,
          attempt.id(),
          at(attempt.deadline().unix_millis() + 1)
        )
        .is_none()
      );
    }
  });
}

#[test]
fn parallel_issue_executions_keep_nested_and_root_targets_separate() {
  crate::factory_flow_tests::run_ready(async {
    let prior = human_fixture();
    let child = prior
      .flow
      .closure()
      .definition(prior.flow.closure().root())
      .unwrap()
      .clone();
    let gate = &child.nodes()[0];
    let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: gate.key().clone(),
      kind: FlowNodeKind::SubflowCall,
      input_schema: child.input_schema().cloned(),
      outcomes: vec![FlowOutcomeDefinition::new(
        key("reviewed"),
        FlowOutcomeKind::Success,
        gate.outcomes()[0].schema().clone(),
      )],
      budget: child.execution().budget(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: Some(child.reference()),
    })
    .unwrap()
    .with_input_binding(gate.input_binding().unwrap().clone())
    .unwrap();
    let root = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: child.input_schema().cloned(),
      entry: call.key().clone(),
      nodes: vec![call],
      transitions: vec![FlowTransition::new(
        gate.key().clone(),
        key("reviewed"),
        FlowTransitionTarget::Terminal(key("reviewed")),
      )],
      terminals: child.terminals().to_vec(),
      context_projections: vec![],
      data_projections: vec![],
      execution: child.execution().clone(),
    })
    .unwrap();
    let nested = with_definitions(prior.clone(), vec![root, child.clone()]);
    let mut other = prior;
    other.work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      other.work.configuration().clone(),
      ExternalWorkIdentity::new("another-execution").unwrap(),
      other.work.subject().clone(),
      other.work.artifacts().clone(),
      WorkClassification::new(other.work.priority(), other.work.risk(), other.work.metadata().clone()),
    )
    .unwrap();
    other.run = FactoryRun::admitted(FactoryRunId::generate(), &other.work);
    let other = with_definitions(other, vec![child.clone()]);
    let (nested_store, nested_snapshot) = pending(nested).await;
    let (other_store, other_snapshot) = pending(other).await;
    let issue_id = octacity_server_domain::IssueId::generate();
    let association = |snapshot: &FactoryRunSnapshot| IssueWorkAssociation {
      project_id: snapshot.work.subject().project_id(),
      issue_id,
      request_id: None,
      requirements_version: None,
      work_id: snapshot.work.id(),
      run_id: snapshot.run.id(),
    };
    let left = crate::describe_issue_factory_actions(&association(&nested_snapshot), &nested_snapshot, at(20)).unwrap();
    let right = crate::describe_issue_factory_actions(&association(&other_snapshot), &other_snapshot, at(20)).unwrap();
    assert_eq!((left.len(), right.len()), (1, 1));
    assert_eq!(left[0].association.issue_id, right[0].association.issue_id);
    assert_eq!(
      left[0].action.node_key, right[0].action.node_key,
      "identical names never collapse targets"
    );
    assert_ne!(left[0].action.work_id, right[0].action.work_id);
    assert_ne!(left[0].action.run_id, right[0].action.run_id);
    assert_ne!(left[0].action.node_attempt_id, right[0].action.node_attempt_id);
    assert_ne!(
      left[0].action.flow_run_id,
      nested_snapshot.admitted_flow.root_run().id()
    );
    assert_eq!(left[0].action.contract, child.reference());
    assert_eq!(
      right[0].action.flow_run_id,
      other_snapshot.admitted_flow.root_run().id()
    );
    assert!(crate::describe_issue_factory_actions(&association(&nested_snapshot), &other_snapshot, at(20)).is_err());
    let command = crate::SubmitFactoryNodeResponseCommand {
      run_id: left[0].action.run_id,
      node_attempt_id: right[0].action.node_attempt_id,
      outcome: key("approved"),
      value: serde_json::json!(true),
      expected_version: left[0].action.run_version,
      idempotency_key: "wrong-parallel-target".parse().unwrap(),
      requested_at: at(20),
    };
    assert!(
      FactoryControlHandlers::new(nested_store.clone())
        .execute_management_command(
          &context(),
          &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
          command
        )
        .await
        .is_err()
    );
    assert_eq!(
      nested_store
        .factory_run_snapshot(nested_snapshot.run.id())
        .await
        .unwrap(),
      nested_snapshot
    );
    assert_eq!(
      other_store.factory_run_snapshot(other_snapshot.run.id()).await.unwrap(),
      other_snapshot
    );
  });
}

#[test]
fn action_descriptions_retire_recorded_cancelled_and_superseded_gate_targets() {
  crate::factory_flow_tests::run_ready(async {
    let schema = FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap();
    let (store, snapshot) = pending(human_fixture_with_contract(schema, None, true)).await;
    let attempt = snapshot.flow.attempts[0].id();
    projection::assert_materialized_actions(store.as_ref(), &snapshot, 12, &[attempt]).await;
    let context = context();
    let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
    let controls = FactoryControlHandlers::new(store.clone());
    controls
      .execute_management_command(
        &context,
        &grant,
        crate::SubmitFactoryNodeResponseCommand {
          run_id: snapshot.run.id(),
          node_attempt_id: attempt,
          outcome: key("approved"),
          value: serde_json::json!(true),
          expected_version: snapshot.run.version(),
          idempotency_key: "described-gate".parse().unwrap(),
          requested_at: at(12),
        },
      )
      .await
      .unwrap();
    let recorded = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    assert!(crate::FactoryNodeActionDescription::from_snapshot(&recorded, attempt, at(12)).is_none());
    let missing = store
      .read_factory_action_projection(FactoryActionProjectionQuery::new(snapshot.run.id(), at(12), None, 1).unwrap())
      .await
      .unwrap();
    assert_eq!(missing.coverage, FactoryActionProjectionCoverage::Incomplete);
    assert!(missing.items.is_empty());
    projection::assert_materialized_actions(store.as_ref(), &recorded, 12, &[]).await;
    let ownership = claim(store.as_ref(), &recorded, 13).await;
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExternalEffects),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    for time in 14..=16 {
      owner
        .reconcile_flow(snapshot.run.id(), ownership.id, at(time))
        .await
        .unwrap();
    }
    let current = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    assert_eq!(current.flow.cycles.len(), 2);
    assert!(crate::FactoryNodeActionDescription::from_snapshot(&current, attempt, at(17)).is_none());
    let current_attempt = current.flow.attempts.iter().find(|row| row.id() != attempt).unwrap();
    assert!(crate::FactoryNodeActionDescription::from_snapshot(&current, current_attempt.id(), at(17)).is_some());
    projection::assert_materialized_actions(store.as_ref(), &current, 17, &[current_attempt.id()]).await;
    assert!(
      crate::FactoryNodeActionDescription::from_snapshot(&current, current_attempt.id(), at(15)).is_none(),
      "a later snapshot cannot describe an earlier observation"
    );
    controls
      .execute_management_command(
        &context,
        &grant,
        CancelFactoryRunCommand {
          run_id: current.run.id(),
          expected_version: current.run.version(),
          idempotency_key: "cancel-described-gate".parse().unwrap(),
          requested_at: at(17),
        },
      )
      .await
      .unwrap();
    let cancelled = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    assert!(crate::FactoryNodeActionDescription::from_snapshot(&cancelled, current_attempt.id(), at(18)).is_none());
    let missing = store
      .read_factory_action_projection(FactoryActionProjectionQuery::new(snapshot.run.id(), at(18), None, 1).unwrap())
      .await
      .unwrap();
    assert_eq!(missing.coverage, FactoryActionProjectionCoverage::Incomplete);
    assert!(missing.items.is_empty());
    projection::assert_materialized_actions(store.as_ref(), &cancelled, 18, &[]).await;
  });
}
