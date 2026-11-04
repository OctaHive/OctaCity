//! Ticket-delivery qualification against the common Factory owner.
use super::*;

fn delivery_requirements_template(extra_parts: &[&str]) -> FlowDefinitionTemplate {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/factory");
  let manifest = std::fs::read(root.join("presets/ticket-delivery/requirements-v1.json"))
    .expect("ticket delivery provides a configurable requirements Flow");
  let mut manifest: Value = serde_json::from_slice(&manifest).unwrap();
  manifest["parts"]
    .as_array_mut()
    .unwrap()
    .extend(extra_parts.iter().map(|part| json!(part)));
  FlowDefinitionTemplate::from_manifest(&serde_json::to_vec(&manifest).unwrap(), |part| {
    std::fs::read(root.join(part.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
      field: "requirements Flow part",
    })
  })
  .unwrap()
}

fn delivery_requirements_fixture(
  template: FlowDefinitionTemplate,
  triage: Value,
) -> (
  PublishedFactoryAdmission,
  Arc<Execution>,
  Arc<FactoryNodeExecutorRegistry>,
) {
  delivery_fixture_with_profiles(template, triage, |_, _| {})
}

fn delivery_fixture_with_profiles(
  mut template: FlowDefinitionTemplate,
  triage: Value,
  configure: impl FnMut(&FlowTemplateNode, &mut FlowBuildBinding),
) -> (
  PublishedFactoryAdmission,
  Arc<Execution>,
  Arc<FactoryNodeExecutorRegistry>,
) {
  // Supply the reviewed caller payload directly when qualifying the branch as a root.
  template
    .nodes
    .iter_mut()
    .find(|node| node.key == template.entry)
    .unwrap()
    .input_binding = FlowInputBinding::new(
    BTreeMap::new(),
    FlowDataMapping::new(FlowValueMapping::Constant { value: triage }).unwrap(),
  )
  .unwrap();
  let work = testing::factory_node_journal_contract_fixture().admission.work;
  let (admission, execution) = crate::factory_flow_tests::configured_fixture_with_profiles(template, work, configure);
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  execution.output_builder = Some(Arc::new(QualifiedOutputs {
    feedback: None,
    validation: None,
  }));
  let execution = Arc::new(execution);
  let registry = FactoryNodeExecutorRegistry::new(vec![(
    FlowNodeKind::Reasoning,
    NodeExecutionIdentity::External(fixtures_build_tool(&admission)),
    Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
  )])
  .unwrap();
  (admission, execution, Arc::new(registry))
}

#[test]
fn ticket_delivery_freezes_requirements_from_the_accepted_triage_size() {
  crate::factory_flow_tests::run_ready(async {
    let template = delivery_requirements_template(&[]);
    for (kind, size) in [
      ("feature_request", "small"),
      ("defect", "medium"),
      ("feature_request", "large"),
      ("defect", "large"),
    ] {
      let summary = json!({
        "work_kind": kind, "size": size, "risk": "high", "admission_risk": "high",
        "recommended_route": "requirements", "research_required": false, "research_accepted": false,
        "component": "api", "severity": "high", "dependencies": [],
      });
      let (admission, execution, registry) =
        delivery_requirements_fixture(template.clone(), json!({"summary": summary, "priority_assessment": 69}));
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      let audit = testing::management_mutation_with_request((), "delivery-requirements")
        .audit()
        .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
      let run = admission.run.id();
      let claim = claim_run(store.as_ref(), run, 10).await;
      let owner = FactoryFlowRunner::new(
        store.clone(),
        registry,
        Arc::new(Inputs),
        crate::factory_node_test_support::clock(),
      );
      let mut terminal = None;
      for tick in 31..70 {
        execution.clock.store(tick, Ordering::SeqCst);
        if let FactoryFlowStep::Resolved(outcome) = crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          store.as_ref(),
          run,
          claim.id,
          at(tick),
        )
        .await
        .unwrap()
        {
          terminal = Some(outcome);
          break;
        }
      }
      assert_eq!(terminal, Some(key("ready")), "{kind}/{size}");
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      let record = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|record| record.outcome() == &key("automatic"))
        .unwrap();
      let contract = AcceptedWorkContract::freeze(
        &snapshot.work,
        &snapshot.admitted_flow,
        &snapshot.flow.data,
        snapshot.flow.runtime(),
        record.node_attempt_id(),
      )
      .unwrap();
      assert_eq!(
        contract.work().artifacts().acceptance(),
        admission.work.artifacts().acceptance()
      );
      assert_eq!(
        snapshot.flow.data.build_intents.len(),
        usize::from(size == "large"),
        "{kind}/{size}"
      );
      let result = record.payload().value();
      assert_eq!(result["size"], summary["size"]);
      if size == "large" {
        let specification = &result["specification"];
        let paths = specification["manifest"]["changed_paths"].as_array().unwrap();
        for path in [
          "openspec/changes/api/proposal.md",
          "openspec/changes/api/design.md",
          "openspec/changes/api/tasks.md",
          "openspec/changes/api/specs/api/spec.md",
        ] {
          assert!(paths.iter().any(|item| item["path"] == path), "missing retained {path}");
        }
        let author = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|record| record.input().node() == &key("specification_author"))
          .unwrap();
        for (field, output) in [
          ("manifest", "change-set-manifest.json"),
          ("bundle", "change-set.bundle"),
        ] {
          assert_eq!(
            specification["artifacts"][field],
            author.metadata().unwrap()["build"]["outputs"][output]["artifact"]
          );
        }
      } else {
        assert!(result.get("specification").is_none());
      }
    }
  });
}

#[test]
fn ticket_delivery_requirements_approval_pins_the_review_before_protected_tests() {
  crate::factory_flow_tests::run_ready(async {
    // Qualify the reusable branch in the same graph as the ordinary Tester.
    let mut template = delivery_requirements_template(&[
      "flows/base-flow/schemas/test_correction_input.json",
      "flows/base-flow/schemas/protected_author_input.json",
      "flows/base-flow/schemas/protected_test_handoff.json",
      "flows/base-flow/nodes/protected_tests.json",
    ]);
    assert!(
      template.nodes.iter().any(|node| node.key == key("requirements_review")),
      "the editable requirements Flow must include the pinned human acceptance step"
    );
    template
      .nodes
      .iter_mut()
      .find(|node| node.key == key("protected_tests"))
      .unwrap()
      .pool = None;
    for transition in &mut template.transitions {
      if transition.target() == &FlowTransitionTarget::Terminal(key("ready")) {
        *transition = FlowTransition::new(
          transition.predecessor().clone(),
          transition.outcome().clone(),
          FlowTransitionTarget::Node(key("protected_tests")),
        );
      }
    }
    template.terminals.remove(&key("ready"));
    template.terminals.insert(key("tested"), key("protected_test_handoff"));
    template.transitions.push(FlowTransition::new(
      key("protected_tests"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("tested")),
    ));
    for (component, size, priority, decisions) in [
      (
        "api",
        "large",
        80,
        [Some(FlowHumanDisposition::Resume), Some(FlowHumanDisposition::Abort)],
      ),
      ("api", "medium", 70, [Some(FlowHumanDisposition::Resume), None]),
      ("api", "medium", 70, [Some(FlowHumanDisposition::Abort), None]),
      ("web", "large", 80, [None, None]),
      ("api", "small", 80, [None, None]),
      ("api", "large", 69, [None, None]),
    ] {
      let mut selected = template.clone();
      let correction = decisions[1].is_some();
      if correction {
        // Exercise a declared return to requirements with a new candidate in the same Run.
        *selected.transitions.last_mut().unwrap() = serde_json::from_value(json!({
          "predecessor": "protected_tests", "outcome": "observed",
          "target": {"kind":"node","key":"requirements_needed"},
          "max_repeats": 1, "exhausted_terminal": "tested", "starts_cycle": true
        }))
        .unwrap();
      }
      let summary = json!({
        "work_kind": "feature_request", "size": size, "risk": "high", "admission_risk": "high",
        "recommended_route": "requirements", "research_required": false, "research_accepted": false,
        "component": component, "severity": "high", "dependencies": [],
      });
      let (admission, execution, registry) =
        delivery_requirements_fixture(selected, json!({"summary": summary, "priority_assessment": priority}));
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      let audit = testing::management_mutation_with_request((), "delivery-requirements-approval")
        .audit()
        .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
      let run = admission.run.id();
      let mut claim = claim_run(store.as_ref(), run, 10).await;
      let make_owner = || {
        FactoryFlowRunner::new(
          store.clone(),
          registry.clone(),
          Arc::new(Inputs),
          crate::factory_node_test_support::clock(),
        )
      };
      let mut owner = make_owner();
      let mut reviews = Vec::new();
      let mut dispatched_tester = false;
      let mut rejected = false;
      for tick in 31..100 {
        execution.clock.store(tick, Ordering::SeqCst);
        let step = crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          store.as_ref(),
          run,
          claim.id,
          at(tick),
        )
        .await
        .unwrap();
        let snapshot = store.factory_run_snapshot(run).await.unwrap();
        if step == FactoryFlowStep::Waiting
          && let Some(attempt) = snapshot.flow.attempts.iter().find(|attempt| {
            attempt.node_key() == &key("requirements_review") && !reviews.iter().any(|(id, _, _)| *id == attempt.id())
          })
        {
          assert!(
            attempt.deadline() >= at(tick + 5 * 60 * 1000),
            "the preset must leave time for an operator to inspect the requirements"
          );
          let disposition = decisions[reviews.len()].expect("an unmatched ticket must not wait for review");
          assert_eq!(
            snapshot
              .flow
              .data
              .build_intents
              .iter()
              .filter(|intent| intent.input().node() == &key("protected_tests"))
              .count(),
            usize::from(dispatched_tester),
          );
          let input = snapshot
            .flow
            .data
            .inputs
            .iter()
            .find(|input| input.digest().unwrap() == attempt.input_digest())
            .unwrap();
          assert_eq!(input.payload().value()["size"], size);
          assert_eq!(input.payload().value()["component"], component);
          assert_eq!(input.payload().value().get("specification").is_some(), size == "large");
          owner = make_owner();
          assert_eq!(
            owner.reconcile_flow(run, claim.id, at(tick)).await.unwrap(),
            FactoryFlowStep::Waiting
          );
          assert_eq!(store.factory_run_snapshot(run).await.unwrap(), snapshot);
          let controls = FactoryControlHandlers::new(store.clone());
          let context = ManagementRequestContext::new(
            crate::ManagementActor::authenticated("requirements-reviewer").unwrap(),
            crate::ManagementSecurityScope::new("requirements-test").unwrap(),
            ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
            crate::ManagementRequestAttributes::new(crate::ManagementIngress::VerifiedIdentity, None),
          )
          .unwrap();
          let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
          let command = crate::ConfirmFactoryNodeOperationCommand {
            run_id: run,
            node_attempt_id: attempt.id(),
            operation: FlowHumanOperation::Escalation,
            disposition,
            reason: FactoryText::new("review the retained requirements").unwrap(),
            expected_version: snapshot.run.version(),
            idempotency_key: IdempotencyKey::new(format!("requirements-review-{}", attempt.id())).unwrap(),
            requested_at: at(tick),
          };
          let revision = &input.payload().value()["specification"]["manifest"]["candidate_revision"];
          if let Some((previous, digest, approved_revision)) = reviews.first() {
            assert_ne!(attempt.input_digest(), *digest);
            assert_ne!(revision, approved_revision);
            let mut stale = command.clone();
            stale.node_attempt_id = *previous;
            stale.disposition = FlowHumanDisposition::Resume;
            assert!(
              controls
                .execute_management_command(&context, &grant, stale)
                .await
                .is_err()
            );
            assert_eq!(store.factory_run_snapshot(run).await.unwrap(), snapshot);
          }
          assert!(
            controls
              .execute_management_command(
                &context,
                &grant,
                crate::SubmitFactoryNodeResponseCommand {
                  run_id: run,
                  node_attempt_id: attempt.id(),
                  outcome: key("approved"),
                  value: json!({"operation":"escalation","disposition":"resume","reason":"bypass"}),
                  expected_version: snapshot.run.version(),
                  idempotency_key: IdempotencyKey::new("generic-response").unwrap(),
                  requested_at: at(tick),
                }
              )
              .await
              .is_err()
          );
          controls
            .execute_management_command(&context, &grant, command)
            .await
            .unwrap();
          let audit = store
            .list_factory_run_diagnostics(
              ListFactoryRunDiagnostics::new(run, FactoryRunDiagnosticKind::Audit, None, 100).unwrap(),
            )
            .await
            .unwrap();
          assert!(audit.items.iter().any(|row| matches!(
            row,
            FactoryRunDiagnosticRecord::Audit(fact)
                if fact.actor_kind == AuditActorKind::AuthenticatedManagement
                  && fact.actor_identity_digest.is_some()
                  && fact.operation == key("resolve-factory-node")
          )));
          reviews.push((attempt.id(), attempt.input_digest(), revision.clone()));
          claim = claim_run(store.as_ref(), run, tick).await;
        }
        if !dispatched_tester
          && let Some(tester) = snapshot
            .flow
            .data
            .build_intents
            .iter()
            .find(|intent| intent.input().node() == &key("protected_tests"))
        {
          let accepted = tester.input().sources().first().unwrap().node_attempt_id();
          let contract = AcceptedWorkContract::freeze(
            &snapshot.work,
            &snapshot.admitted_flow,
            &snapshot.flow.data,
            snapshot.flow.runtime(),
            accepted,
          )
          .unwrap();
          assert_eq!(contract.work(), &snapshot.work);
          if let Some((review, _, _)) = reviews.first() {
            let receipt = contract
              .records()
              .iter()
              .find(|record| record.node_attempt_id() == *review)
              .unwrap();
            assert_eq!(receipt.outcome(), &key("approved"));
            assert_eq!(
              &tester.input().payload().value()["accepted"],
              receipt.input().payload().value()
            );
          }
          assert_eq!(tester.input().payload().value()["accepted"]["size"], size);
          dispatched_tester = true;
          if !correction {
            break;
          }
        }
        if let FactoryFlowStep::Resolved(outcome) = step {
          assert_eq!(outcome, key("needs_correction"));
          assert_eq!(snapshot.flow.cycles.len(), 1 + usize::from(correction));
          assert_eq!(
            snapshot
              .flow
              .data
              .build_intents
              .iter()
              .filter(|intent| intent.input().node() == &key("protected_tests"))
              .count(),
            usize::from(correction),
          );
          rejected = true;
          break;
        }
      }
      assert_eq!(
        reviews.len(),
        decisions.iter().flatten().count(),
        "{component}/{size}/{priority}"
      );
      assert_eq!(
        dispatched_tester,
        decisions[0] != Some(FlowHumanDisposition::Abort),
        "{component}/{size}/{priority}"
      );
      assert_eq!(
        rejected,
        decisions.contains(&Some(FlowHumanDisposition::Abort)),
        "{component}/{size}/{priority}"
      );
    }
  });
}

fn delivery_preparation_template(extra_parts: &[&str]) -> FlowDefinitionTemplate {
  let mut template = delivery_requirements_template(
    &[
      "flows/base-flow/schemas/test_correction_input.json",
      "flows/base-flow/schemas/protected_author_input.json",
      "flows/base-flow/schemas/protected_test_handoff.json",
      "flows/base-flow/schemas/implementation_entry_input.json",
      "flows/base-flow/schemas/implementation_input.json",
      "flows/base-flow/schemas/execution_facts.json",
      "flows/base-flow/schemas/candidate_capture_facts.json",
      "flows/base-flow/policies/public-deliverables.json",
      "flows/base-flow/nodes/protected_tests.json",
      "flows/base-flow/nodes/implementation_entry.json",
      "flows/base-flow/nodes/implementation.json",
      "presets/ticket-delivery/protected-preparation-v1.json",
    ]
    .into_iter()
    .chain(extra_parts.iter().copied())
    .collect::<Vec<_>>(),
  );
  for edge in &mut template.transitions {
    if edge.target() == &FlowTransitionTarget::Terminal(key("ready")) {
      *edge = FlowTransition::new(
        edge.predecessor().clone(),
        edge.outcome().clone(),
        FlowTransitionTarget::Node(key("protected_tests")),
      );
    }
  }
  template.terminals.remove(&key("ready"));
  let observation = crate::factory_node_test_support::probe_observation_contract();
  let schema = observation.report_schema().unwrap();
  template.schemas.insert(
    key("baseline_report"),
    FlowTemplateSchema {
      identity: schema.reference().identity().clone(),
      version: schema.reference().version().clone(),
      shape: schema.shape().clone(),
    },
  );
  template
}

// Execution cases here exercise the standalone variant without a queue scheduler.
fn without_protected_test_pool(mut template: FlowDefinitionTemplate) -> FlowDefinitionTemplate {
  template
    .nodes
    .iter_mut()
    .find(|node| node.key == key("protected_tests"))
    .unwrap()
    .pool = None;
  template
}

#[test]
fn ticket_delivery_protected_preparation_admission_requires_the_configured_pool() {
  let mut template = delivery_preparation_template(&[]);
  template.terminals.insert(key("implemented"), key("summary"));
  template.transitions.push(FlowTransition::new(
    key("implementation"),
    key("observed"),
    FlowTransitionTarget::Terminal(key("implemented")),
  ));
  let (admission, _, _) = delivery_requirements_fixture(template, json!({}));
  assert!(matches!(
    admission.flow.clone().with_pool_settings(BTreeMap::new()),
    Err(FactoryError::InvalidConfiguration {
      field: "configured Flow pools"
    })
  ));
  admission
    .flow
    .clone()
    .with_pool_settings(admission.flow.pool_settings().clone())
    .unwrap();
}

#[test]
fn ticket_delivery_protected_preparation_requires_observed_behavioral_failure() {
  crate::factory_flow_tests::run_ready(async {
    let mut template = without_protected_test_pool(delivery_preparation_template(&[]));
    template.terminals.insert(key("implemented"), key("summary"));
    template.transitions.push(FlowTransition::new(
      key("implementation"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("implemented")),
    ));
    for (status, finding, terminal) in [
      ("completed", "matched", None),
      ("completed", "not_matched", Some("already_satisfied")),
      (
        "environment_unavailable",
        "indeterminate",
        Some("test_preparation_blocked"),
      ),
    ] {
      let summary = json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low",
        "recommended_route":"requirements","research_required":false,"research_accepted":false,
        "component":"api","severity":"low","dependencies":[]});
      let (admission, execution, _) =
        delivery_requirements_fixture(template.clone(), json!({"summary": summary, "priority_assessment": 50}));
      let mut execution = Arc::try_unwrap(execution).ok().unwrap();
      let exact = admission.work.subject().repository().unwrap();
      execution
        .reports
        .insert(key("tests_on_base"), baseline_report(exact, status, finding));
      let execution = Arc::new(execution);
      let registry = delivery_execution_registry(&admission, &execution);
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      let audit = testing::management_mutation_with_request((), "ticket-delivery-baseline")
        .audit()
        .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
      let run = admission.run.id();
      let claim = claim_run(store.as_ref(), run, 10).await;
      let owner = FactoryFlowRunner::new(
        store.clone(),
        registry,
        Arc::new(Inputs),
        crate::factory_node_test_support::clock(),
      );
      for tick in 31..90 {
        execution.clock.store(tick, Ordering::SeqCst);
        let step = crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          store.as_ref(),
          run,
          claim.id,
          at(tick),
        )
        .await
        .unwrap();
        let snapshot = store.factory_run_snapshot(run).await.unwrap();
        let intents = &snapshot.flow.data.build_intents;
        if let Some(coder) = intents
          .iter()
          .find(|intent| intent.input().node() == &key("implementation"))
        {
          assert!(
            terminal.is_none(),
            "an inconclusive or passing baseline must never dispatch implementation"
          );
          let author = intents
            .iter()
            .find(|intent| intent.input().node() == &key("protected_tests"))
            .unwrap();
          let baseline = intents
            .iter()
            .find(|intent| intent.input().node() == &key("tests_on_base"))
            .unwrap();
          assert_ne!(author.node().id(), coder.node().id());
          assert_ne!(author.node().id(), baseline.node().id());
          assert_eq!(coder.input().payload().value()["test_scope"], json!(["project.custom"]));
          assert_eq!(coder.input().payload().value()["accepted"], summary);
          assert_eq!(coder.input().context().unwrap().entries().len(), 2);
          assert_eq!(
            baseline.input().exact_resource().unwrap().base_revision(),
            exact.base_revision()
          );
          assert!(
            baseline
              .input()
              .context()
              .unwrap()
              .entries()
              .iter()
              .any(|entry| { entry.logical_identity() == &key("protected_inventory") })
          );
          break;
        }
        if let FactoryFlowStep::Resolved(outcome) = step {
          assert_eq!(Some(outcome.as_str()), terminal);
          assert_eq!(
            intents.len(),
            2,
            "a stop after baseline dispatches no implementation or publication"
          );
          break;
        }
        assert!(
          tick < 89,
          "baseline did not select a visible route for {status}/{finding}"
        );
      }
    }
  });
}

fn delivery_development_template() -> FlowDefinitionTemplate {
  delivery_preparation_template(&[
    "flows/base-flow/schemas/candidate_subject.json",
    "flows/base-flow/schemas/code_review.json",
    "flows/base-flow/schemas/code_review_input.json",
    "flows/base-flow/schemas/spec_review_facts.json",
    "flows/base-flow/schemas/execution_gate_input.json",
    "flows/base-flow/schemas/protected_validation_input.json",
    "flows/base-flow/schemas/protected_validation_report.json",
    "flows/base-flow/schemas/protected_validation_facts.json",
    "flows/base-flow/schemas/protected_validation_gate_input.json",
    "flows/base-flow/schemas/validation_diagnostic_input.json",
    "flows/base-flow/schemas/validation_classification.json",
    "flows/base-flow/schemas/validation_classification_input.json",
    "presets/ticket-delivery/implementation-capture-gate-v1.json",
    "flows/base-flow/nodes/implementation_rework_entry.json",
    "flows/base-flow/nodes/review.json",
    "flows/base-flow/nodes/protected_validation.json",
    "flows/base-flow/nodes/protected_validation_gate.json",
    "flows/base-flow/nodes/validation_classification.json",
    "flows/base-flow/gate-programs.json",
    "presets/ticket-delivery/agent-result-gates-v1.json",
    "presets/ticket-delivery/development-v1.json",
  ])
}

#[test]
fn ticket_delivery_capture_gate_requires_fresh_pinned_evidence() {
  let template = delivery_development_template();
  let schema = &template.data_schemas().unwrap()[&key("implementation_capture_gate_input")];
  let gate = template
    .nodes
    .iter()
    .find(|node| node.key == key("implementation_result_gate"))
    .unwrap()
    .gate
    .as_ref()
    .unwrap();
  let accepted = json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low",
    "recommended_route":"requirements","research_required":false,"research_accepted":false,
    "component":"api","severity":"low","dependencies":[]});
  let input = json!({
    "result": accepted, "accepted": accepted, "primary_output":"codex-run-result", "primary_fresh_until": 100,
    "expected_input":"a".repeat(64),
    "capture":{"candidate_revision":"new-revision", "candidate_digest":"b".repeat(64), "input_digest":"a".repeat(64), "fresh_until": 100}
  });
  let select = |input| {
    gate
      .select_at(&FlowPayload::new(schema, input).unwrap(), at(50))
      .unwrap()
  };
  assert_eq!(select(input.clone()), key("continue"));
  for (path, value) in [
    ("/primary_output", json!("observations")),
    ("/primary_fresh_until", json!(50)),
    ("/capture/input_digest", json!("c".repeat(64))),
    ("/capture/fresh_until", json!(49)),
  ] {
    let mut forged = input.clone();
    *forged.pointer_mut(path).unwrap() = value;
    assert_eq!(select(forged), key("invalid"), "{path}");
  }
  let mut correction = input;
  correction["result"]["requirements_defect"] = json!({
    "reason":"Acceptance needs clarification", "candidate_revision":"new-revision",
    "evidence":[{"artifact_id":"00000000-0000-0000-0000-000000000012","content_digest":"d".repeat(64),"encoded_size":100}]
  });
  assert_eq!(select(correction), key("requirements_defect"));
}

#[test]
fn ticket_delivery_pins_public_deliverables_for_capture_and_validation() {
  let template = delivery_development_template();
  let implementation = template
    .nodes
    .iter()
    .find(|node| node.key == key("implementation"))
    .unwrap();
  let validation = template
    .nodes
    .iter()
    .find(|node| node.key == key("protected_validation"))
    .unwrap();
  let capture = implementation
    .capture_deliverables
    .as_ref()
    .expect("implementation must enforce public deliverables");
  let verify = validation
    .capture_deliverables
    .as_ref()
    .expect("validation must verify public deliverables");
  assert_eq!(
    serde_json::to_value(capture).unwrap(),
    serde_json::to_value(verify).unwrap()
  );
  assert!(template.deliverable_policies.contains_key(&key("public_changes")));
}

#[test]
fn ticket_delivery_reviews_each_candidate_before_protected_validation() {
  crate::factory_flow_tests::run_ready(async {
    let template = delivery_development_template();
    for (review, repository, expected, order) in [
      (["satisfied"; 2], ["passed"; 2], "verified_candidate", "IRV"),
      (
        ["violated", "satisfied"],
        ["passed", "failed"],
        "implementation_rework_exhausted",
        "IRIRV",
      ),
      (["satisfied"; 2], ["failed", "passed"], "verified_candidate", "IRVIRV"),
      (
        ["satisfied"; 2],
        ["indeterminate", "passed"],
        "validation_needs_human_input",
        "IRV",
      ),
      (
        ["indeterminate", "satisfied"],
        ["passed"; 2],
        "code_review_needs_human_input",
        "IR",
      ),
    ] {
      let outputs = TicketDeliveryChecks {
        review,
        repository,
        candidates: Mutex::default(),
      };
      let (snapshot, terminal) = ticket_delivery_development_snapshot(template.clone(), outputs).await;
      assert_eq!(terminal, key(expected), "{review:?}/{repository:?}");
      if expected == "verified_candidate" {
        assert_eq!(snapshot.run.state(), FactoryRunState::Completed);
      } else {
        assert_eq!(
          snapshot.run.state(),
          FactoryRunState::Waiting,
          "{expected} must hold the Run for an operator"
        );
        let gate = snapshot
          .flow
          .attempts
          .iter()
          .find(|attempt| attempt.node_key() == &key(expected))
          .unwrap();
        let input = snapshot
          .flow
          .data
          .inputs
          .iter()
          .find(|input| input.digest().unwrap() == gate.input_digest())
          .unwrap();
        let data = input.payload().value();
        match expected {
          "code_review_needs_human_input" => assert_eq!(
            data["findings"],
            json!(["Could not verify response status for a missing fixture"])
          ),
          "validation_needs_human_input" => assert_eq!(data["classes"][1]["outcome"], "indeterminate"),
          "implementation_rework_exhausted" => assert_eq!(data["remediation"], json!(["Preserve the public contract"])),
          _ => unreachable!(),
        }
      }
      let attempts = &snapshot.flow.data.build_intents;
      let stages = |name| {
        let mut selected = attempts
          .iter()
          .filter(|row| row.input().node() == &key(name))
          .collect::<Vec<_>>();
        selected.sort_unstable_by_key(|intent| intent.node().number());
        selected
      };
      let authors = stages("protected_tests");
      assert_eq!(authors.len(), 1, "rework must keep the original test bundle");
      let author = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|row| row.node_attempt_id() == authors[0].node().id())
        .unwrap();
      let inventory = &author
        .build_provenance()
        .unwrap()
        .outputs()
        .iter()
        .find(|output| output.name == key("protected_inventory"))
        .unwrap()
        .artifact;

      let mut observed = attempts
        .iter()
        .filter_map(|intent| {
          let stage = match intent.input().node().as_str() {
            "implementation" => 'I',
            "review" => 'R',
            "protected_validation" => 'V',
            _ => return None,
          };
          Some((intent.node().started_at(), stage))
        })
        .collect::<Vec<_>>();
      observed.sort_unstable();
      assert_eq!(observed.into_iter().map(|(_, stage)| stage).collect::<String>(), order);

      let implementations = stages("implementation");
      let reviews = stages("review");
      assert_ne!(
        implementations[0].profile().task_digest,
        reviews[0].profile().task_digest
      );
      let acceptance = snapshot.work.artifacts().acceptance();
      assert_eq!(
        serde_json::to_value(authors[0].input()).unwrap()["work_acceptance"],
        json!(acceptance)
      );
      for intent in authors.iter().chain(&implementations).chain(&reviews) {
        assert!(
          intent.input().context().unwrap().entries().iter().any(|entry| {
            matches!(
              entry.source_kind(),
              ContextSourceKind::Task | ContextSourceKind::Specification
            ) && entry.source() == &FactoryContextReference::Artifact(acceptance.clone())
          }),
          "the configured stage {} must receive the exact Work acceptance",
          intent.input().node()
        );
        let wire = serde_json::to_string(intent.input()).unwrap();
        assert!(!wire.contains("protected_inventory"));
        assert!(!wire.contains(&inventory.artifact_id().to_string()));
      }
      let public_contract = implementations[0].profile().capture_deliverables.as_ref().unwrap();
      for validation in stages("protected_validation") {
        assert_eq!(
          validation.profile().capture_deliverables.as_ref(),
          Some(public_contract)
        );
        let review = reviews
          .iter()
          .find(|review| {
            review.input().context().unwrap().subject() == validation.input().context().unwrap().subject()
              && review.node().started_at() < validation.node().started_at()
          })
          .expect("validation must follow review of its exact candidate");
        let payload = validation.input().payload().value();
        let reviewed = review.input().payload().value();
        assert_eq!(payload["candidate_manifest"], reviewed["manifest"]);
        assert_eq!(payload["candidate_bundle"], reviewed["bundle"]);
        assert_eq!(payload["protected_inventory"], json!(inventory));
        assert_eq!(payload["classes"], json!(["project.custom", "repository.checks"]));
      }
      if let [initial, replacement] = implementations.as_slice() {
        let (initial, replacement) = (initial.input().payload().value(), replacement.input().payload().value());
        let first = reviews[0].input().payload().value();
        let second = reviews[1].input().payload().value();
        assert_ne!(first["candidate_revision"], second["candidate_revision"]);
        assert_eq!(replacement["parent"]["candidate_revision"], first["candidate_revision"]);
        assert_eq!(replacement["accepted_contract"], initial["accepted_contract"]);
        assert_eq!(replacement["test_scope"], initial["test_scope"]);
        let findings = if review[0] == "violated" {
          json!(["Preserve the response status", "Handle the empty response body"])
        } else {
          json!(["Preserve the public contract"])
        };
        assert_eq!(replacement["remediation"], findings);
      }
      if repository[0] == "indeterminate" {
        assert!(stages("validation_classification").is_empty());
      }
    }
  });
}

struct TicketDeliveryChecks {
  review: [&'static str; 2],
  repository: [&'static str; 2],
  candidates: Mutex<Vec<Value>>,
}

impl FactoryTestOutputBuilder for TicketDeliveryChecks {
  fn outputs(
    &self,
    request: &CreateFactoryBuild,
    builds: &Builds,
    schema: &ImmutableReference,
    requirements: &[EvidenceRequirement],
  ) -> Option<FactoryNodeBuildOutputs> {
    let c = request.causality.as_ref();
    let wire: Value = serde_json::from_slice(&c.input).unwrap();
    let input = &wire["payload"]["value"];
    let report = match schema.identity().as_str() {
      "factory.base.protected_test_handoff" => Some(json!({
        "accepted_contract":c.input_digest,"classes":["project.custom","repository.checks"]
      })),
      "factory.base.code_review" => {
        let mut candidates = self.candidates.lock().unwrap();
        let revision = &input["candidate_revision"];
        let index = candidates
          .iter()
          .position(|candidate| candidate == revision)
          .unwrap_or_else(|| {
            candidates.push(revision.clone());
            candidates.len() - 1
          });
        let correctness = self.review[index];
        Some(
          json!({"candidate_revision":revision,"candidate_digest":input["candidate_digest"],"input_digest":c.input_digest,
          "quality":{"correctness":correctness,"maintainability":"satisfied","public_tests":"satisfied","security":"satisfied"},
          "findings":match correctness {
            "violated" => vec!["Preserve the response status", "Handle the empty response body"],
            "indeterminate" => vec!["Could not verify response status for a missing fixture"],
            _ => vec![],
          }}),
        )
      }
      "factory.base.protected_validation_report" => {
        let candidates = self.candidates.lock().unwrap();
        let index = candidates
          .iter()
          .position(|candidate| candidate == &input["candidate_revision"])
          .expect("a candidate must be reviewed before validation");
        Some(
          json!({"changeset_id":ChangeSetId::generate(),"candidate":c.context.subject().candidate().unwrap(),
          "tests":input["protected_inventory"],"input_digest":c.input_digest,"classes":[
            {"class":"project.custom","outcome":"passed"},
            {"class":"repository.checks","outcome":self.repository[index]}
          ]}),
        )
      }
      _ => None,
    };
    QualifiedOutputs {
      feedback: None,
      validation: None,
    }
    .outputs_with_report(request, builds, schema, requirements, report)
  }
}

async fn ticket_delivery_development_snapshot(
  template: FlowDefinitionTemplate,
  outputs: TicketDeliveryChecks,
) -> (FactoryRunSnapshot, FactoryKey) {
  let summary = json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low",
    "recommended_route":"requirements","research_required":false,"research_accepted":false,
    "component":"api","severity":"low","dependencies":[]});
  let (admission, execution, _) = delivery_fixture_with_profiles(
    without_protected_test_pool(template),
    json!({"summary":summary,"priority_assessment":50}),
    |node, profile| {
      if node.key == key("implementation") {
        profile.result_output = key("codex-run-result");
        profile
          .evidence
          .retain(|evidence| evidence.kind() != &key("execution_facts"));
      }
      if matches!(
        node.outcomes[0].schema.as_str(),
        "code_review" | "validation_classification"
      ) {
        profile.result_output = key("codex-run-result");
        profile.evidence.clear();
      }
    },
  );
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  execution.output_builder = Some(Arc::new(outputs));
  let exact = admission.work.subject().repository().unwrap();
  execution
    .reports
    .insert(key("tests_on_base"), baseline_report(exact, "completed", "matched"));
  let execution = Arc::new(execution);
  let registry = delivery_execution_registry(&admission, &execution);
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  let audit = testing::management_mutation_with_request((), "ticket-delivery-development")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  let run = admission.run.id();
  let claim = claim_run(store.as_ref(), run, 10).await;
  let owner = FactoryFlowRunner::new(
    store.clone(),
    registry,
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  for tick in 31..150 {
    execution.clock.store(tick, Ordering::SeqCst);
    match crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      store.as_ref(),
      run,
      claim.id,
      at(tick),
    )
    .await
    .unwrap()
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => return (store.factory_run_snapshot(run).await.unwrap(), outcome),
      FactoryFlowStep::Waiting => {
        let snapshot = store.factory_run_snapshot(run).await.unwrap();
        if snapshot.run.state() == FactoryRunState::Waiting {
          let gate = snapshot
            .flow
            .attempts
            .iter()
            .rev()
            .find(|attempt| attempt.node_kind() == FlowNodeKind::HumanGate)
            .unwrap();
          let gate = gate.node_key().clone();
          return (snapshot, gate);
        }
      }
      other => panic!("unexpected delivery development progress {other:?}"),
    }
  }
  panic!("ticket delivery development did not resolve");
}

fn baseline_report(exact: &ExactSubject, status: &str, finding: &str) -> Value {
  let observation = crate::factory_node_test_support::probe_observation_contract();
  let mut report = json!({
    "input_digest": FactoryDigest::from_bytes([1; 32]),
    "subject": {"project_id": exact.project_id(), "repository_id": exact.repository_id(), "base_revision": exact.base_revision()},
    "command": reference("baseline"),
    "criterion": observation.criterion,
    "environment": observation.environment,
    "status": status,
    "finding": finding
  });
  if status == "completed" {
    report["observations"] =
      json!({"probe": "GET /health", "requests": 1, "unexpected_statuses": u8::from(finding == "matched")});
  }
  report
}

fn delivery_execution_registry(
  admission: &PublishedFactoryAdmission,
  execution: &Arc<Execution>,
) -> Arc<FactoryNodeExecutorRegistry> {
  let adapter = Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()));
  let tool = NodeExecutionIdentity::External(fixtures_build_tool(admission));
  Arc::new(
    FactoryNodeExecutorRegistry::new(vec![
      (FlowNodeKind::Reasoning, tool.clone(), adapter.clone()),
      (FlowNodeKind::BuildCommand, tool, adapter),
    ])
    .unwrap(),
  )
}
