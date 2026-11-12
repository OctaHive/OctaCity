//! Ticket-delivery qualification against the common Factory owner.
use super::*;

fn delivery_context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}

fn delivery_requirements_template(extra_parts: &[&str]) -> FlowDefinitionTemplate {
  requirements_template(extra_parts, false)
}

fn requirements_template(extra_parts: &[&str], issue_input: bool) -> FlowDefinitionTemplate {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/factory");
  let manifest = std::fs::read(root.join("presets/ticket-delivery/requirements-v1.json"))
    .expect("ticket delivery provides a configurable requirements Flow");
  let mut manifest: Value = serde_json::from_slice(&manifest).unwrap();
  if issue_input {
    for part in manifest["parts"].as_array_mut().unwrap() {
      *part = match part.as_str().unwrap() {
        "flows/base-flow/schemas/summary.json" => json!("presets/ticket-delivery/issue-summary-v2.json"),
        "presets/ticket-delivery/requirements-branch-v1.json" => {
          json!("presets/ticket-delivery/requirements-branch-v2.json")
        }
        "presets/ticket-delivery/requirements-approval-v1.json" => {
          json!("presets/ticket-delivery/requirements-approval-v2.json")
        }
        _ => part.clone(),
      };
    }
  }
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
  template: FlowDefinitionTemplate,
  triage: Value,
  configure: impl FnMut(&mut FlowTemplateNode, &mut FlowBuildBinding),
) -> (
  PublishedFactoryAdmission,
  Arc<Execution>,
  Arc<FactoryNodeExecutorRegistry>,
) {
  let work = testing::factory_node_journal_contract_fixture().admission.work;
  requirements_fixture_with_work(template, triage, work, configure)
}

/// Qualifies v2 specification capture and human review with an already frozen Issue payload.
/// Admission and edits are exercised by the caller through the ordinary Issue owner.
pub(crate) fn issue_requirements_fixture(
  snapshot: &FactoryRunSnapshot,
) -> (
  PublishedFactoryAdmission,
  Arc<Execution>,
  Arc<FactoryNodeExecutorRegistry>,
) {
  requirements_fixture_with_work(
    requirements_template(&[], true),
    snapshot.flow.data.incoming[0].payload().value().clone(),
    snapshot.work.clone(),
    |_, _| {},
  )
}

fn requirements_fixture_with_work(
  mut template: FlowDefinitionTemplate,
  input: Value,
  work: WorkEnvelope,
  configure: impl FnMut(&mut FlowTemplateNode, &mut FlowBuildBinding),
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
    FlowDataMapping::new(FlowValueMapping::Constant { value: input }).unwrap(),
  )
  .unwrap();
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
      let (admission, execution, registry) = delivery_fixture_with_profiles(
        template.clone(),
        json!({"summary": summary, "priority_assessment": 69}),
        |node, build| {
          if kind == "feature_request" && node.key == key("specification_author") {
            pin_delivery_profile(node, build);
          }
        },
      );
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      let audit = testing::management_mutation_with_request((), "delivery-requirements")
        .audit()
        .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
      let project = admission.work.subject().project_id();
      store.seed_project(project).unwrap();
      store
        .enable_project(
          project,
          testing::factory_profile_selection_capabilities(&admission.flow),
        )
        .unwrap();
      let run = admission.run.id();
      let claim = claim_run(store.as_ref(), run, 10).await;
      let owner = FactoryFlowRunner::new(
        store.clone(),
        registry,
        Arc::new(Inputs),
        crate::factory_node_test_support::clock(),
      )
      .with_profile_catalog(store.clone());
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
        let accepted = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|record| record.input().node() == &key("specification_accepted"))
          .unwrap();
        assert_delivery_card(
          &snapshot,
          accepted,
          "Specifications",
          &[
            author.metadata().unwrap()["build"]["outputs"]["change-set-manifest.json"]["artifact"].clone(),
            author.metadata().unwrap()["build"]["outputs"]["change-set.bundle"]["artifact"].clone(),
          ],
        );
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
      store.seed_project(admission.run.configuration().project_id()).unwrap();
      let audit = testing::management_mutation_with_request((), "delivery-requirements-approval")
        .audit()
        .clone();
      store
        .seed_factory_admission(
          admission.clone(),
          octacity_server_store::FactoryWorkSourceScope {
            source: key("manual"),
            security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
          },
          FactoryDigest::from_bytes([1; 32]),
          &audit,
        )
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
          let page = crate::AuthorizedQueryHandler::new(
            Arc::new(crate::TrustedNetworkManagementPolicy),
            Arc::new(crate::FactoryDiscoveryHandlers::new(store.clone())),
          )
          .handle_query(
            &delivery_context(),
            crate::ListFactoryDiagnosticsQuery::try_new(
              run,
              crate::FactoryDiagnosticCollection::NodeAttempts,
              None,
              crate::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,
            )
            .unwrap(),
          )
          .await
          .unwrap();
          let wire = serde_json::to_value(page).unwrap();
          let gate = wire["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["record"]["id"] == json!(attempt.id()))
            .unwrap();
          let sources = gate["record"]["input_summary"]["accepted_results"]
            .as_array()
            .expect("pending requirements approval exposes a safe summary of its frozen input");
          assert_eq!(sources.len(), 1);
          assert_eq!(sources[0]["node"], "requirements_review_policy");
          assert_eq!(sources[0]["digest"], json!(input.sources()[0].digest()));
          assert!(sources[0].get("payload").is_none());
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
  delivery_development_template_with(&[])
}

fn delivery_development_template_with(extra_parts: &[&str]) -> FlowDefinitionTemplate {
  let parts = [
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
  ];
  delivery_preparation_template(&[parts.as_slice(), extra_parts].concat())
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
        review_ttl_millis: None,
        validation_ttl_millis: None,
      };
      let (snapshot, terminal, _) = ticket_delivery_development_snapshot(template.clone(), outputs, None).await;
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
      for accepted in snapshot
        .flow
        .data
        .records
        .iter()
        .filter(|record| record.input().node() == &key("code_review_gate") && record.outcome() == &key("accepted"))
      {
        let review = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|record| {
            record.input().node() == &key("review")
              && record.payload().value()["candidate_revision"]
                == accepted.input().payload().value()["review"]["candidate_revision"]
          })
          .unwrap();
        let build = review.build_provenance().unwrap();
        assert_eq!(
          review.metadata().unwrap()["build"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
          ["outputs", "primary_fresh_until", "profile"],
          "the entire Build metadata is a supported input projection for existing Runs"
        );
        let output = build
          .outputs()
          .iter()
          .find(|output| output.name == build.profile().result_output)
          .unwrap();
        assert_delivery_card(&snapshot, accepted, "Code review", &[json!(output.artifact)]);
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
  review_ttl_millis: Option<i64>,
  validation_ttl_millis: Option<i64>,
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
      "factory.ticket-delivery.pull-request-metadata" => Some(json!({
        "title":"Fix the public API", "summary":"Reviewed and verified the candidate."
      })),
      _ => None,
    };
    let mut outputs = QualifiedOutputs {
      feedback: None,
      validation: None,
    }
    .outputs_with_report(
      request,
      builds,
      schema,
      requirements,
      report,
      self
        .validation_ttl_millis
        .map(|ttl| at(builds.created_at.unix_millis() + ttl)),
    )?;
    if schema.identity().as_str() == "factory.base.code_review"
      && let Some(ttl) = self.review_ttl_millis
    {
      outputs.fresh_until = at(builds.created_at.unix_millis() + ttl);
    }
    Some(outputs)
  }
}

async fn ticket_delivery_development_snapshot(
  template: FlowDefinitionTemplate,
  outputs: TicketDeliveryChecks,
  publisher: Option<Arc<Publisher>>,
) -> (
  FactoryRunSnapshot,
  FactoryKey,
  Arc<testing::InMemoryFactoryConfigurationStore>,
) {
  let summary = json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low",
    "recommended_route":"requirements","research_required":false,"research_accepted":false,
    "component":"api","severity":"low","dependencies":[]});
  let (admission, execution, _) = delivery_fixture_with_profiles(
    without_protected_test_pool(template),
    json!({"summary":summary,"priority_assessment":50}),
    |node, profile| {
      if node.key == key("prepare_pull_request") {
        pin_delivery_profile(node, profile);
      }
      if node.key == key("replacement") {
        let schemas = crate::factory_node_test_support::fixtures::supplied_base_flow_template()
          .data_schemas()
          .unwrap();
        for name in ["change-set.bundle", "change-set-manifest.json", "candidate_facts"] {
          let (kind, schema) = if name == "candidate_facts" {
            (
              EvidenceOutputKind::Report,
              schemas[&key("candidate_capture_facts")].reference().clone(),
            )
          } else {
            (EvidenceOutputKind::Artifact, reference("capture.artifacts"))
          };
          profile.evidence.push(EvidenceRequirement::new(
            key(name),
            kind,
            schema,
            reference("git.capture"),
            profile.plugin.clone(),
          ));
        }
      }
      if matches!(node.key.as_str(), "implementation" | "replacement") {
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
  let qualifies_publication = publisher.is_some();
  execution
    .reports
    .insert(key("tests_on_base"), baseline_report(exact, "completed", "matched"));
  let execution = Arc::new(execution);
  let mut restart_after_write = publisher
    .as_ref()
    .filter(|publisher| publisher.behavior == PublicationCase::LostResponse)
    .cloned();
  let expects_delayed_observation = restart_after_write
    .as_ref()
    .is_some_and(|publisher| publisher.lose_first_recovery_probe.load(Ordering::SeqCst));
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  let registry = if let Some(publisher) = publisher {
    let definition = admission
      .flow
      .closure()
      .definition(admission.flow.root_run().definition())
      .unwrap();
    let node = definition.node(&key("publish_pull_request")).unwrap();
    let identity = node.action().unwrap().plugin().clone();
    let publication = FactoryFlowActionAdapter::new(Arc::new(
      FactoryCandidatePublicationAction::new(
        identity.clone(),
        publisher,
        node.outcome(&key("published")).unwrap().clone(),
      )
      .with_run_store(store.clone()),
    ));
    let adapter = Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()));
    let tool = NodeExecutionIdentity::External(fixtures_build_tool(&admission));
    Arc::new(
      FactoryNodeExecutorRegistry::new(vec![
        (
          FlowNodeKind::Reasoning,
          tool.clone(),
          adapter.clone() as Arc<dyn crate::FactoryNodeExecutor>,
        ),
        (FlowNodeKind::BuildCommand, tool, adapter),
        (
          FlowNodeKind::TrustedAction,
          NodeExecutionIdentity::External(identity),
          Arc::new(publication),
        ),
      ])
      .unwrap(),
    )
  } else {
    delivery_execution_registry(&admission, &execution)
  };
  let project = admission.work.subject().project_id();
  store.seed_project(project).unwrap();
  if qualifies_publication {
    publish_manual_delivery_configuration(&admission, &store).await;
  } else {
    store
      .enable_project(
        project,
        testing::factory_profile_selection_capabilities(&admission.flow),
      )
      .unwrap();
  }
  let audit = testing::management_mutation_with_request((), "ticket-delivery-development")
    .audit()
    .clone();
  store
    .seed_factory_admission(
      admission.clone(),
      octacity_server_store::FactoryWorkSourceScope {
        source: key("manual"),
        security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      },
      FactoryDigest::from_bytes([1; 32]),
      &audit,
    )
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
    .with_profile_catalog(store.clone())
  };
  let mut owner = make_owner();
  for tick in 31..150 {
    execution.clock.store(tick, Ordering::SeqCst);
    if restart_after_write
      .as_ref()
      .is_some_and(|publisher| publisher.lost.load(Ordering::SeqCst))
    {
      claim = claim_run(store.as_ref(), run, tick).await;
      owner = make_owner();
      restart_after_write = None;
    }
    let step = crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      store.as_ref(),
      run,
      claim.id,
      at(tick),
    )
    .await;
    if expects_delayed_observation
      && matches!(&step, Err(ApplicationError::Store(StoreError::Unavailable)))
      && restart_after_write
        .as_ref()
        .is_some_and(|publisher| publisher.lost.load(Ordering::SeqCst))
    {
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      assert!(
        snapshot
          .flow
          .data
          .records
          .iter()
          .all(|record| record.input().node() != &key("publish_pull_request"))
      );
      continue;
    }
    if let Err(error) = &step {
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      panic!(
        "delivery progress at {tick}: {error:?}; attempts: {:?}; inputs: {:?}",
        snapshot
          .flow
          .attempts
          .iter()
          .map(NodeAttempt::node_key)
          .collect::<Vec<_>>(),
        snapshot
          .flow
          .data
          .inputs
          .iter()
          .map(FlowNodeInput::node)
          .collect::<Vec<_>>()
      );
    }
    match step.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => return (store.factory_run_snapshot(run).await.unwrap(), outcome, store),
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
          return (snapshot, gate, store);
        }
      }
      other => panic!("unexpected delivery development progress {other:?}"),
    }
  }
  panic!("ticket delivery development did not resolve");
}

fn pin_delivery_profile(node: &mut FlowTemplateNode, build: &FlowBuildBinding) {
  let project = build.build_configuration.project_id();
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      name: FactoryText::new("DevOps").unwrap(),
      instructions: "Prepare public review notes.".into(),
      template: None,
      skills: Default::default(),
      icon: None,
      avatar: None,
      description: String::new(),
      concurrency_limit: None,
    },
  )
  .unwrap();
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    project,
    &role,
    FactoryRoleProfileDefinition {
      execution_binding: Default::default(),
      template: None,
      name: FactoryText::new("PR preparation").unwrap(),
      instructions: "Use only the accepted evidence.".into(),
      toolchain: FactoryTaskToolchain {
        executable: build.tool.clone(),
        plugin: build.plugin.clone(),
        model: build.model_or_tool.clone(),
      },
      model_identifier: None,
      execution: Default::default(),
      model_parameters: Default::default(),
      thinking_parameter: None,
      skills: Default::default(),
      context_requirements: Default::default(),
      build_configuration: build.build_configuration.clone(),
      credential_purpose: FactoryCredentialPurpose::Model,
      permissions: node.permissions.clone(),
      budget: node.budget,
    },
  )
  .unwrap();
  let candidate = key("devops");
  node.build_profile = None;
  node.profile_selection = Some(FlowProfileSelectionBinding {
    role: role.reference().clone(),
    candidates: BTreeMap::from([(
      candidate.clone(),
      FlowProfileCandidate {
        profile,
        build: build.clone(),
      },
    )]),
    policy: FlowProfileSelectionPolicy::Manual {
      candidate: Some(candidate),
    },
    failure: FlowProfileSelectionFailure::ManualSelection,
    selection_budget: BudgetUsage::default(),
  });
}

fn pull_request_template() -> (FlowDefinitionTemplate, FactoryKey, octacity_server_factory::NetworkHost) {
  let mut template = delivery_development_template_with(&["presets/ticket-delivery/pull-request-v1.json"]);
  template.transitions.retain(
    |edge| !matches!(edge.target(), FlowTransitionTarget::Terminal(terminal) if terminal == &key("verified_candidate")),
  );
  template.terminals.remove(&key("verified_candidate"));
  let publication = template
    .nodes
    .iter_mut()
    .find(|node| node.key == key("publish_pull_request"))
    .unwrap();
  let writer = key("delivery-writer");
  let host = octacity_server_factory::NetworkHost::new("forge.example").unwrap();
  publication.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![publication.action.as_ref().unwrap().plugin().clone()],
    secret_profiles: vec![writer.clone()],
    network_hosts: vec![host.clone()],
    ..Default::default()
  })
  .unwrap();
  template.execution = FlowExecutionPolicy::new(
    template.execution.budget(),
    publication.permissions.clone(),
    template.execution.max_active_nodes(),
  )
  .unwrap();
  (template, writer, host)
}

async fn publish_manual_delivery_configuration(
  admission: &PublishedFactoryAdmission,
  store: &Arc<testing::InMemoryFactoryConfigurationStore>,
) {
  use crate::factory_node_test_support::publication::PublicationChoices;
  use crate::{
    AuthorizedCommandHandler, CreateFactoryConfigurationCommand, CreateFactoryFlowDefinitionCommand,
    FactoryConfigurationHandlers, FactoryFlowDefinitionDraft, FactoryFlowDefinitionHandlers,
    TrustedNetworkManagementPolicy,
  };
  let admitted = &admission.flow;
  let configuration = admission.work.configuration();
  let project = configuration.project_id();
  let root = admitted.closure().definition(admitted.closure().root()).unwrap();
  let original = testing::factory_profile_selection_capabilities(admitted);
  let mut entries = original.entries();
  let mut register = |kind, reference: &ImmutableReference| {
    if !entries
      .references
      .iter()
      .any(|entry| entry.kind == kind && &entry.reference == reference)
    {
      entries.references.push(FactoryReferenceChoice {
        kind,
        alias: key(&format!("installed.{}", entries.references.len())),
        reference: reference.clone(),
      });
    }
  };
  let installed = FactoryChoiceKind::NodeCapability;
  for node in root.nodes() {
    for build in node.build_bindings() {
      for reference in [&build.tool, &build.plugin, &build.model_or_tool, build.result_tool()] {
        register(installed, reference);
      }
      for evidence in &build.evidence {
        register(installed, evidence.tool());
        register(installed, evidence.plugin());
      }
      if !entries
        .build_configurations
        .iter()
        .any(|(_, reference)| reference == &build.build_configuration)
      {
        entries.build_configurations.push((
          key(&format!("installed.build.{}", entries.build_configurations.len())),
          build.build_configuration.clone(),
        ));
      }
    }
    if let Some(action) = node.action() {
      register(installed, action.plugin());
    }
  }
  for name in root.execution().permissions().secret_profiles() {
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::SecretProfile,
      alias: name.clone(),
      reference: reference(name.as_str()),
    });
  }
  for (kind, name) in [
    (FactoryChoiceKind::AdmissionPolicy, "manual-launch"),
    (FactoryChoiceKind::PermissionCeiling, "delivery-access"),
  ] {
    entries.references.push(FactoryReferenceChoice {
      kind,
      alias: key(name),
      reference: reference(name),
    });
  }
  let publisher = root.node(&key("publish_pull_request")).unwrap();
  let action = publisher.action().unwrap();
  let outcome = publisher.outcome(&key("published")).unwrap().clone();
  let mut choices = FactoryConfigurationChoices::try_new(entries)
    .unwrap()
    .with_candidate_publication(FactoryCandidatePublicationChoice {
      plugin: action.plugin().clone(),
      target: ImmutableReference::new(
        key("production.review"),
        key("v1"),
        FactoryDigest::from_bytes([0xaa; 32]),
      ),
      receipt_schema: Some(admitted.data_schema(outcome.schema()).unwrap().clone()),
      repository_id: admission.work.subject().repository().unwrap().repository_id(),
      outcome,
      review_url: true,
      permissions: Default::default(),
    })
    .unwrap();
  for (alias, capability) in original.role_profile_capabilities() {
    choices = choices
      .with_role_profile_capability(alias.clone(), capability.clone())
      .unwrap();
  }
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let authoring = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(
      Arc::new(PublicationChoices::for_publication(choices.clone())),
      policy.clone(),
    )),
  );
  let mut wire = serde_json::to_value(root).unwrap();
  wire.as_object_mut().unwrap().remove("reference");
  let context = delivery_context();
  let published = authoring
    .handle_command(
      &context,
      CreateFactoryFlowDefinitionCommand {
        id: root.reference().id(),
        project_id: project,
        draft: FactoryFlowDefinitionDraft {
          accepted_predecessor: None,
          issue_snapshot: None,
          root: serde_json::from_value(wire).unwrap(),
          limits: admitted.limits().clone(),
          data_schemas: admitted.data_schemas().to_vec(),
          pools: admitted.pool_settings().clone(),
        },
        idempotency_key: "manual-delivery-flow".parse().unwrap(),
        published_at: at(1),
      },
    )
    .await
    .unwrap();
  assert_eq!(&published.publication.flow.closure, admitted.closure());
  store.enable_project(project, choices).unwrap();
  let configurations = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryConfigurationHandlers::with_policy(store.clone(), policy.clone())),
  );
  let saved = configurations
    .handle_command(
      &context,
      CreateFactoryConfigurationCommand {
        id: configuration.id(),
        project_id: project,
        definition_digest: configuration.definition_digest(),
        draft: FactoryConfigurationDraft {
          admission_policy: key("manual-launch").into(),
          permission_ceiling: key("delivery-access").into(),
          flow: Some(published.publication.flow),
          wip_limits: admitted.wip_limits(),
          hard_budget: admitted.limits().budget(),
          enabled: true,
        },
        idempotency_key: "manual-delivery-configuration".parse().unwrap(),
        published_at: at(1),
      },
    )
    .await
    .unwrap();
  let saved = saved.configuration.configuration;
  assert_eq!(saved.reference(), configuration);
  assert_eq!(&saved.flow().closure, admitted.closure());
  let readiness = crate::AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(
      crate::FactoryReadinessHandlers::new(
        store.clone(),
        policy,
        crate::factory_node_test_support::clock(),
        std::time::Duration::from_secs(30),
      )
      .unwrap(),
    ),
  );
  let candidate_repository = admission.work.subject().repository().unwrap().repository_id();
  for (id, expected) in [
    (candidate_repository, crate::FactoryReadinessState::Ready),
    (
      octacity_server_domain::RepositoryId::generate(),
      crate::FactoryReadinessState::Blocked,
    ),
  ] {
    let report = readiness
      .handle_query(
        &context,
        crate::GetFactoryReadinessQuery {
          id: configuration.id(),
          version: configuration.version(),
          selection: crate::FactoryReadinessSelection {
            repository: WorkSourceRepositoryRef {
              project_id: project,
              id,
              version: octacity_server_domain::RepositoryVersion::INITIAL,
              definition_digest: FactoryDigest::from_bytes([9; 32]),
            },
            input: None,
          },
        },
      )
      .await
      .unwrap();
    assert!(
      report
        .checks
        .iter()
        .any(|check| check.aspect == "publication_target" && check.state == expected),
      "readiness must evaluate the installed target against the selected repository"
    );
  }
}

#[test]
fn ticket_delivery_retains_publication_conflict_without_success() {
  crate::factory_flow_tests::run_ready(async {
    let (template, _, _) = pull_request_template();
    let publisher = Arc::new(Publisher {
      behavior: PublicationCase::ChangedAncestry,
      lost: AtomicBool::new(false),
      lose_first_recovery_probe: AtomicBool::new(false),
      writes: Mutex::default(),
      requests: Mutex::default(),
    });
    let checks = TicketDeliveryChecks {
      review: ["satisfied"; 2],
      repository: ["passed"; 2],
      candidates: Mutex::default(),
      review_ttl_millis: None,
      validation_ttl_millis: None,
    };
    let (snapshot, terminal, _) = ticket_delivery_development_snapshot(template, checks, Some(publisher.clone())).await;
    assert_eq!(terminal, key("publication_blocked"));
    let publication = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|row| row.input().node() == &key("publish_pull_request"))
      .unwrap();
    assert_eq!(publication.outcome(), &key("conflict"));
    assert_eq!(publication.payload().value(), &json!({"reason":"publication_conflict"}));
    assert!(
      snapshot
        .outbox
        .iter()
        .all(|row| row.kind.as_str() != "work.status.projection")
    );
    assert!(publisher.writes.lock().unwrap().is_empty());
    assert!(publisher.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn ticket_delivery_retains_final_rejections_without_a_forge_write() {
  crate::factory_flow_tests::run_ready(async {
    for (behavior, reason) in [
      (PublicationCase::ProviderRejected, "provider_rejected"),
      (PublicationCase::Normal, "validation_expired"),
    ] {
      let (template, _, _) = pull_request_template();
      let publisher = Arc::new(Publisher {
        behavior,
        lost: AtomicBool::new(false),
        lose_first_recovery_probe: AtomicBool::new(false),
        writes: Mutex::default(),
        requests: Mutex::default(),
      });
      let checks = TicketDeliveryChecks {
        review: ["satisfied"; 2],
        repository: ["passed"; 2],
        candidates: Mutex::default(),
        review_ttl_millis: None,
        validation_ttl_millis: (behavior == PublicationCase::Normal).then_some(4),
      };
      let (snapshot, terminal, _) =
        ticket_delivery_development_snapshot(template, checks, Some(publisher.clone())).await;
      assert_eq!(terminal, key("publication_blocked"));
      assert!(snapshot.flow.data.records.iter().any(|record| {
        record.input().node() == &key("protected_validation_gate") && record.outcome() == &key("passed")
      }));
      let publication = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|row| row.input().node() == &key("publish_pull_request"))
        .unwrap();
      assert_eq!(publication.payload().value(), &json!({"reason":reason}));
      assert!(
        publication
          .discovery()
          .and_then(FlowDiscoveryFacts::confirmed_candidate_review)
          .is_none()
      );
      assert!(publisher.requests.lock().unwrap().is_empty());
    }
  });
}

#[test]
fn ticket_delivery_blocks_new_pr_when_review_expires_after_its_gate() {
  crate::factory_flow_tests::run_ready(async {
    let (template, _, _) = pull_request_template();
    let publisher = Arc::new(Publisher {
      behavior: PublicationCase::Normal,
      lost: AtomicBool::new(false),
      lose_first_recovery_probe: AtomicBool::new(false),
      writes: Mutex::default(),
      requests: Mutex::default(),
    });
    let checks = TicketDeliveryChecks {
      review: ["satisfied"; 2],
      repository: ["passed"; 2],
      candidates: Mutex::default(),
      review_ttl_millis: Some(8),
      validation_ttl_millis: None,
    };
    let (snapshot, terminal, _) = ticket_delivery_development_snapshot(template, checks, Some(publisher.clone())).await;
    assert!(snapshot.flow.data.records.iter().any(|record| {
      record.input().node() == &key("protected_validation_gate") && record.outcome() == &key("passed")
    }));
    assert_eq!(terminal, key("publication_blocked"));
    assert!(publisher.writes.lock().unwrap().is_empty());
  });
}

#[test]
fn ticket_delivery_recovers_the_original_pr_after_the_publish_response_is_lost() {
  crate::factory_flow_tests::run_ready(async {
    for delay_observation in [false, true] {
      let (template, _, _) = pull_request_template();
      let publisher = Arc::new(Publisher {
        behavior: PublicationCase::LostResponse,
        lost: AtomicBool::new(false),
        lose_first_recovery_probe: AtomicBool::new(delay_observation),
        writes: Mutex::default(),
        requests: Mutex::default(),
      });
      let checks = TicketDeliveryChecks {
        review: ["satisfied"; 2],
        repository: ["passed"; 2],
        candidates: Mutex::default(),
        review_ttl_millis: None,
        validation_ttl_millis: None,
      };
      let (snapshot, terminal, _) =
        ticket_delivery_development_snapshot(template, checks, Some(publisher.clone())).await;
      assert!(publisher.lost.load(Ordering::SeqCst));
      assert_eq!(terminal, key("delivered_for_review"));
      let attempts = snapshot
        .flow
        .attempts
        .iter()
        .filter(|attempt| attempt.node_key() == &key("publish_pull_request"))
        .collect::<Vec<_>>();
      assert_eq!(attempts.len(), 1);
      assert_ne!(
        snapshot.current_claim.as_ref().unwrap().claim.fence(),
        attempts[0].claim().fence()
      );
      let receipts = snapshot
        .flow
        .data
        .records
        .iter()
        .filter(|record| record.node_attempt_id() == attempts[0].id())
        .collect::<Vec<_>>();
      assert_eq!(receipts.len(), 1);
      let requests = publisher.requests.lock().unwrap();
      assert_eq!(requests.len(), 1);
      assert_eq!(publisher.writes.lock().unwrap().len(), 1);
      let retained = receipts[0].payload().value();
      let verified_facts = serde_json::to_value(receipts[0].discovery()).unwrap();
      assert_eq!(
        verified_facts["review_url"], "https://forge.example/team/repository/pull/23",
        "Issue links need trusted facts independent of the outcome payload"
      );
      assert_eq!(retained["receipt"], json!(requests[0].operation_id));
      assert_eq!(retained["external"]["review_identity"], "23");
      assert_eq!(retained["external"]["review_state"], "open");
      assert_eq!(
        retained["external"]["review_url"],
        "https://forge.example/team/repository/pull/23"
      );
      assert_eq!(
        retained["external"]["remote_head"],
        json!(requests[0].candidate.candidate_revision())
      );
    }
  });
}

#[test]
fn ticket_delivery_blocks_a_replacement_candidate_without_its_own_checks() {
  crate::factory_flow_tests::run_ready(async {
    let (mut template, _, _) = pull_request_template();
    let mut replacement = template
      .nodes
      .iter()
      .find(|node| node.key == key("implementation"))
      .unwrap()
      .clone();
    replacement.key = key("replacement");
    replacement.build_profile = Some(key("replacement"));
    replacement.input_binding = serde_json::from_value(json!({
      "sources": {"original": {"source":"record", "node":"implementation", "outcome":"observed", "view":"input_payload", "path":""}},
      "mapping": {"value": {"mapping":"pointer", "path":"/original"}}
    })).unwrap();
    template.nodes.push(replacement);
    template.transitions.retain(
      |edge| !matches!(edge.target(), FlowTransitionTarget::Node(node) if node == &key("publish_pull_request")),
    );
    for (from, to) in [
      ("prepare_pull_request", "replacement"),
      ("replacement", "publish_pull_request"),
    ] {
      template.transitions.push(FlowTransition::new(
        key(from),
        key("observed"),
        FlowTransitionTarget::Node(key(to)),
      ));
    }
    let publisher_node = template
      .nodes
      .iter_mut()
      .find(|node| node.key == key("publish_pull_request"))
      .unwrap();
    let mut binding = serde_json::to_value(&publisher_node.input_binding).unwrap();
    binding["sources"]["metadata"] =
      json!({"source":"record", "node":"prepare_pull_request", "outcome":"observed", "view":"payload", "path":""});
    for alias in ["bundle", "manifest"] {
      binding["sources"][alias]["node"] = json!("replacement");
    }
    binding["sources"]["replacement"] = json!({"source":"record", "node":"replacement", "outcome":"observed", "view":"metadata", "path":"/build/outputs/candidate_facts/data/payload/value"});
    for field in ["candidate_revision", "candidate_digest"] {
      binding["mapping"]["value"]["fields"][field] =
        json!({"mapping":"pointer", "path":format!("/replacement/{field}")});
    }
    publisher_node.input_binding = serde_json::from_value(binding).unwrap();
    let publisher = Arc::new(Publisher {
      behavior: PublicationCase::Normal,
      lost: AtomicBool::new(false),
      lose_first_recovery_probe: AtomicBool::new(false),
      writes: Mutex::default(),
      requests: Mutex::default(),
    });
    let (snapshot, terminal, _) = ticket_delivery_development_snapshot(
      template,
      TicketDeliveryChecks {
        review: ["satisfied"; 2],
        repository: ["passed"; 2],
        candidates: Mutex::default(),
        review_ttl_millis: None,
        validation_ttl_millis: None,
      },
      Some(publisher.clone()),
    )
    .await;
    let records = &snapshot.flow.data.records;
    let reviewed = records
      .iter()
      .find(|record| record.input().node() == &key("review"))
      .unwrap();
    let attempted = records
      .iter()
      .find(|record| record.input().node() == &key("publish_pull_request"))
      .unwrap();
    assert_ne!(
      reviewed.payload().value()["candidate_revision"],
      attempted.input().payload().value()["candidate_revision"]
    );
    assert_eq!(terminal, key("publication_blocked"));
    assert!(publisher.writes.lock().unwrap().is_empty());
  });
}

#[test]
fn ticket_delivery_blocks_publication_with_a_negative_review_or_validation_report() {
  crate::factory_flow_tests::run_ready(async {
    for (review, validation) in [("violated", "passed"), ("satisfied", "failed")] {
      let (mut template, _, _) = pull_request_template();
      // A hand-authored gate can route a completed report regardless of its verdict.
      for (node, outcome) in [
        ("code_review_gate", "accepted"),
        ("protected_validation_gate", "passed"),
      ] {
        let gate = template
          .nodes
          .iter_mut()
          .find(|step| step.key == key(node))
          .unwrap()
          .gate
          .as_mut()
          .unwrap();
        let mut program = serde_json::to_value(&*gate).unwrap();
        let rule = program["rules"]
          .as_array_mut()
          .unwrap()
          .iter_mut()
          .find(|rule| rule["outcome"] == outcome)
          .unwrap();
        rule["when"] = json!({
          "operation": "compare",
          "left": {"source": "literal", "value": true},
          "comparison": "equal",
          "right": {"source": "literal", "value": true}
        });
        *gate = serde_json::from_value(program).unwrap();
      }
      let publisher = Arc::new(Publisher {
        behavior: PublicationCase::Normal,
        lost: AtomicBool::new(false),
        lose_first_recovery_probe: AtomicBool::new(false),
        writes: Mutex::default(),
        requests: Mutex::default(),
      });
      let (_, terminal, _) = ticket_delivery_development_snapshot(
        template,
        TicketDeliveryChecks {
          review: [review; 2],
          repository: [validation; 2],
          candidates: Mutex::default(),
          review_ttl_millis: None,
          validation_ttl_millis: None,
        },
        Some(publisher.clone()),
      )
      .await;
      assert_eq!(
        terminal,
        key("publication_blocked"),
        "review {review}, validation {validation}"
      );
      assert!(publisher.writes.lock().unwrap().is_empty());
    }
  });
}

#[test]
fn devops_prepares_an_exact_candidate_for_trusted_production_pr_publication() {
  crate::factory_flow_tests::run_ready(async {
    let (template, writer, host) = pull_request_template();
    let publisher = Arc::new(Publisher {
      behavior: PublicationCase::Normal,
      lost: AtomicBool::new(false),
      lose_first_recovery_probe: AtomicBool::new(false),
      writes: Mutex::new(BTreeMap::new()),
      requests: Mutex::new(Vec::new()),
    });
    let checks = TicketDeliveryChecks {
      review: ["violated", "satisfied"],
      repository: ["passed"; 2],
      candidates: Mutex::default(),
      review_ttl_millis: None,
      validation_ttl_millis: None,
    };
    let (snapshot, terminal, store) =
      ticket_delivery_development_snapshot(template, checks, Some(publisher.clone())).await;
    assert_eq!(terminal, key("delivered_for_review"));
    let attempts = &snapshot.flow.data.build_intents;
    let devops = attempts
      .iter()
      .find(|intent| intent.input().node() == &key("prepare_pull_request"))
      .unwrap();
    let implementation = attempts
      .iter()
      .find(|intent| intent.input().node() == &key("implementation"))
      .unwrap();
    assert_ne!(devops.node().id(), implementation.node().id());
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(devops.input().definition())
      .unwrap();
    let preparation = definition.node(devops.input().node()).unwrap();
    let selected = devops.selected_profile().unwrap();
    let profile = &preparation.profile_selection().unwrap().candidates[&key("devops")].profile;
    assert_eq!(selected, profile.reference());
    assert_eq!(profile.definition().credential_purpose, FactoryCredentialPurpose::Model);
    assert_eq!(devops.permissions().secret_profiles().count(), 0);
    assert_eq!(devops.permissions().tools().count(), 0);
    assert_eq!(devops.permissions().network_hosts().count(), 0);
    assert_eq!(devops.permissions().commands().count(), 0);
    assert!(devops.step_task().is_some());
    assert!(
      snapshot
        .flow
        .attempts
        .iter()
        .all(|attempt| attempt.node_kind() != FlowNodeKind::HumanGate)
    );

    let request = {
      let requests = publisher.requests.lock().unwrap();
      assert_eq!(requests.len(), 1);
      requests[0].clone()
    };
    assert_eq!(request.target.identity(), &key("production.review"));
    assert_eq!(request.permissions.secret_profiles().collect::<Vec<_>>(), [&writer]);
    assert_eq!(request.permissions.network_hosts().collect::<Vec<_>>(), [&host]);
    let review = request.review_metadata.as_ref().unwrap();
    assert_eq!(review.title(), "Fix the public API");
    assert_eq!(review.summary(), "Reviewed and verified the candidate.");
    let record = |name: &str| {
      snapshot
        .flow
        .data
        .records
        .iter()
        .filter(|row| row.input().node() == &key(name))
        .max_by_key(|row| row.observed_at())
        .unwrap()
    };
    let validated = record("protected_validation_gate");
    assert_eq!(validated.outcome(), &key("passed"));
    assert_eq!(
      validated.input().payload().value()["candidate_revision"],
      json!(request.candidate.candidate_revision())
    );
    assert_eq!(
      record("review").payload().value()["candidate_revision"],
      json!(request.candidate.candidate_revision())
    );
    assert_eq!(
      validated.input().payload().value()["candidate_digest"],
      json!(request.candidate.changeset_digest())
    );
    let superseded = snapshot
      .flow
      .data
      .records
      .iter()
      .filter(|row| row.input().node() == &key("review"))
      .min_by_key(|row| row.observed_at())
      .unwrap();
    assert_ne!(
      superseded.payload().value()["candidate_revision"],
      json!(request.candidate.candidate_revision())
    );
    let captured = record("implementation").build_provenance().unwrap();
    for (name, expected) in [
      ("change-set.bundle", &request.bundle),
      ("change-set-manifest.json", &request.manifest),
    ] {
      assert_eq!(
        &captured
          .outputs()
          .iter()
          .find(|output| output.name == key(name))
          .unwrap()
          .artifact,
        expected
      );
    }
    let publication = definition.node(&key("publish_pull_request")).unwrap();
    assert_eq!(publication.action().unwrap().action(), &key("publish_candidate"));
    let retained = snapshot
      .accepted_terminal_result(
        devops.input().definition(),
        &key("delivered_for_review"),
        &std::collections::BTreeSet::from([key("published")]),
      )
      .unwrap()
      .unwrap()
      .payload()
      .value();
    assert_eq!(retained["external"]["review_state"], "open");
    assert_eq!(
      retained["external"]["review_url"],
      "https://forge.example/team/repository/pull/23"
    );
    assert_eq!(
      retained["external"]["remote_head"],
      json!(request.candidate.candidate_revision())
    );
    assert_eq!(retained["receipt"], json!(request.operation_id));
    assert!(
      snapshot
        .outbox
        .iter()
        .all(|row| row.kind.as_str() != "work.status.projection")
    );
    let queries = crate::AuthorizedQueryHandler::new(
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(crate::FactoryDiscoveryHandlers::new(store)),
    );
    let page = queries
      .handle_query(
        &delivery_context(),
        crate::ListFactoryDiagnosticsQuery::try_new(
          snapshot.run.id(),
          crate::FactoryDiagnosticCollection::NodeCompletions,
          None,
          crate::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    let wire = serde_json::to_value(page).unwrap();
    for name in ["review", "protected_validation"] {
      let row = record(name);
      let result = wire["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["record"]["node_attempt_id"] == json!(row.node_attempt_id()))
        .unwrap();
      let build = row.build_provenance().unwrap();
      let report = build
        .outputs()
        .iter()
        .find(|output| output.name == build.profile().result_output)
        .unwrap();
      assert_eq!(
        result["record"]["verified_report"],
        json!({
          "content_digest": report.artifact.content_digest(),
          "verified_at": report.verified_at,
          "fresh_until": report.fresh_until,
        })
      );
      assert!(result["record"].get("public_result").is_none());
    }
    assert_eq!(snapshot.run.state(), FactoryRunState::Completed);
  });
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

fn assert_delivery_card(snapshot: &FactoryRunSnapshot, record: &FlowNodeRecord, title: &str, artifacts: &[Value]) {
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(snapshot.admitted_flow.root_run().definition())
    .unwrap();
  let presentation = definition
    .node(record.input().node())
    .unwrap()
    .outcome(record.outcome())
    .unwrap()
    .issue_presentation()
    .expect("the accepted result must offer a card on its Issue");
  let value = record.payload().value();
  let references = octacity_server_store::IssueOutcomeReference::from_accepted(presentation, value);
  let expected: Vec<_> = artifacts
    .iter()
    .map(|value| octacity_server_store::IssueOutcomeReference::Artifact(serde_json::from_value(value.clone()).unwrap()))
    .collect();
  assert_eq!(references, expected);
  let (conclusion, content) = octacity_server_store::IssueOutcomeContent::project(
    presentation.clone(),
    value,
    octacity_server_store::IssueOutcomeClassification::Recommended,
  )
  .unwrap();
  assert!(!conclusion.is_empty());
  assert!(
    matches!(content, octacity_server_store::IssueOutcomeContent::Custom { title: name } if name.as_str() == title)
  );
}
