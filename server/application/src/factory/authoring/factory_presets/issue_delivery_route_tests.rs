//! Released delivery routes consume the admitted Issue, even after its live fields change.
use super::*;
use crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration;
use octacity_server_store::testing;
use octacity_server_store::*;

#[path = "issue_delivery_requirements_routes.rs"]
mod requirements;

struct GateInputs;
#[async_trait::async_trait]
impl FactoryFlowInputPreparer for GateInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    assert!(matches!(
      snapshot
        .admitted_flow
        .closure()
        .definition(snapshot.admitted_flow.closure().root())
        .unwrap()
        .node(&target.node)
        .unwrap()
        .kind(),
      FlowNodeKind::DeterministicGate | FlowNodeKind::HumanGate
    ));
    prepare_factory_flow_input(snapshot, target)
  }
}

fn progress(snapshot: &FactoryRunSnapshot) -> FlowProgress {
  let validated = snapshot.admitted_flow.validated().unwrap();
  FlowInterpreter::new(&validated)
    .progress(
      snapshot.admitted_flow.root_run(),
      snapshot.flow.cycles.last().unwrap(),
      snapshot.flow.runtime(),
    )
    .unwrap()
}
fn next_node(snapshot: &FactoryRunSnapshot) -> FactoryKey {
  match progress(snapshot) {
    FlowProgress::Node(node) => node,
    other => panic!("expected next node, got {other:?}"),
  }
}

async fn admitted_issue(
  kind: &str,
  size: IssueSize,
  component: &str,
  priority: &str,
  reproducibility: &str,
) -> (
  DeliveryFixture,
  Arc<testing::InMemoryProjectStore>,
  IssueRevision,
  IssueExecutionRequest,
) {
  let f = delivery_fixture().await;
  let issues = properties(f.project).await;
  let mut selection = issue_selection(&f).await;
  selection.issue_snapshot = Some(issue_input());
  let resolved = AuthorizedQueryHandler::new(
    f.policy.clone(),
    Arc::new(
      FactoryPresetHandlers::new(f.store.clone(), f.policy.clone())
        .with_repositories(f.reader.clone())
        .with_issue_properties(issues.clone()),
    ),
  )
  .handle_query(
    &f.context,
    ResolveFactoryPresetQuery {
      project_id: f.project,
      selection: selection.clone(),
    },
  )
  .await
  .unwrap();
  assert!(resolved.diagnostics.is_empty(), "{:?}", resolved.diagnostics);
  let flow = AuthorizedCommandHandler::new(
    f.policy.clone(),
    Arc::new(
      FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(f.installed.clone())),
        f.policy.clone(),
      )
      .with_issue_properties(issues.clone()),
    ),
  )
  .handle_command(
    &f.context,
    CreateFactoryFlowDefinitionCommand {
      id: selection.flow_id.unwrap(),
      project_id: f.project,
      draft: resolved.flow.unwrap(),
      idempotency_key: "delivery-route-flow".parse().unwrap(),
      published_at: Timestamp::from_unix_millis(2).unwrap(),
    },
  )
  .await
  .unwrap()
  .publication
  .flow;
  let config_id = selection.configuration_id.unwrap();
  let mut configuration = resolved.configuration.unwrap();
  configuration.enabled = true;
  configuration.flow = Some(flow);
  seed_permitted_configuration(
    &f.store,
    PublishedFactoryConfiguration {
      configuration: FactoryConfiguration::publish(
        config_id,
        FactoryConfigurationVersion::INITIAL,
        f.project,
        FactoryDigest::from_bytes([53; 32]),
        configuration,
        &f.installed,
      )
      .unwrap(),
      published_at: Timestamp::from_unix_millis(3).unwrap(),
    },
  )
  .unwrap();
  let repository = selection.repository.unwrap();
  f.store.seed_repository_version(f.repository.clone()).unwrap();
  let issue = AuthorizedCommandHandler::new(
    f.policy.clone(),
    Arc::new(IssueHandlers::new(issues.clone(), f.policy.clone())),
  )
  .handle_command(
    &f.context,
    CreateIssueCommand {
      intent: CreateIssueIntent {
        project_id: f.project,
        content: IssueContent {
          title: "Deliver a feature".into(),
          description: "Original task".into(),
          acceptance: Some("Feature meets its acceptance checks".into()),
          priority: Some(IssuePriority::Choice(priority.into())),
        },
        planning: IssuePlanning {
          definition_version: 1,
          issue_type: Some(kind.into()),
          size: Some(size),
          component: Some(component.into()),
          values: [
            ("impact", "advisory"),
            ("risk", "low"),
            ("route", "requirements"),
            ("repro", reproducibility),
          ]
          .into_iter()
          .map(|(k, v)| (k.into(), IssuePropertyValue::Choice(v.into())))
          .collect(),
          ..Default::default()
        },
      },
      idempotency_key: "delivery-route-issue".parse().unwrap(),
      created_at: Timestamp::from_unix_millis(4).unwrap(),
    },
  )
  .await
  .unwrap()
  .issue;
  let request = AuthorizedCommandHandler::new(
    f.policy.clone(),
    Arc::new(IssueExecutionHandlers::new(
      issues.clone(),
      f.store.clone(),
      f.policy.clone(),
    )),
  )
  .handle_command(
    &f.context,
    RequestIssueExecutionCommand {
      intent: RequestIssueExecutionIntent {
        expected_snapshot_digest: None,
        project_id: f.project,
        issue_id: issue.id,
        expected_version: issue.version,
        target: IssueExecutionTarget {
          predecessor_run: None,
          configuration_id: config_id,
          configuration_version: FactoryConfigurationVersion::INITIAL,
          repository_id: repository.id,
          repository_version: repository.version,
          source: octacity_server_domain::RepositorySourceSelection::ExactRevision("0123456789abcdef".parse().unwrap()),
          mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
        },
        classification: None,
      },
      idempotency_key: "delivery-route-request".parse().unwrap(),
      requested_at: Timestamp::from_unix_millis(5).unwrap(),
    },
  )
  .await
  .unwrap()
  .request;
  let published = AuthorizedCommandHandler::new(
    f.policy.clone(),
    Arc::new(IssueExecutionPublisher::new(
      issues.clone(),
      f.store.clone(),
      Arc::new(crate::management::tests::project_text_intake_tests::DocumentBytes::default()),
      Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
      f.policy.clone(),
      crate::factory_node_test_support::clock(),
    )),
  )
  .handle_command(
    &f.context,
    PublishIssueExecutionCommand {
      project_id: f.project,
      issue_id: issue.id,
      request_id: request.id,
      classification: WorkClassification::new(WorkPriority::new(1).unwrap(), RiskClass::High, Default::default()),
    },
  )
  .await
  .unwrap();
  (f, issues, issue, published)
}

#[test]
fn issue_delivery_routes_the_original_small_feature_after_issue_edits() {
  run_ready(async {
    let (f, issues, issue, published) =
      admitted_issue("feature_request", IssueSize::Small, "compiler", "low", "not_applicable").await;
    let run = published.association.unwrap().run_id;
    let frozen = f.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(frozen.flow.data.incoming[0].payload().value()["size"], "small");
    change_issue(issues.as_ref(), &issue).await;
    let after = advance_gates(&f, run, 40).await;
    let record = after
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("requirements_review_policy"))
      .expect("frozen small feature reaches requirements policy");
    assert_eq!(record.outcome(), &key("automatic"));
    assert_eq!(record.payload().value()["size"], "small");
    assert!(
      !after
        .flow
        .data
        .inputs
        .iter()
        .any(|input| input.node() == &key("specification_author"))
    );
    assert_eq!(next_node(&after), key("protected_tests"));
    assert_eq!(after.work, frozen.work);
    assert_eq!(after.flow.data.incoming, frozen.flow.data.incoming);
  });
}

// Stop at the ordinary external-execution boundary; these tests qualify routing,
// while the shared delivery owner tests qualify Build proofs and publication.
async fn advance_gates(f: &DeliveryFixture, run: FactoryRunId, started_at: i64) -> FactoryRunSnapshot {
  let claim =
    crate::factory::execution::tests::factory_requirements_tests::claim_run(f.store.as_ref(), run, started_at).await;
  let owner = FactoryFlowRunner::new(
    f.store.clone(),
    Arc::new(crate::factory_flow_tests::FakePending),
    Arc::new(GateInputs),
    crate::factory_node_test_support::clock(),
  );
  for tick in started_at + 1..started_at + 40 {
    let state = f.store.factory_run_snapshot(run).await.unwrap();
    let FlowProgress::Node(next) = progress(&state) else {
      return state;
    };
    if !matches!(
      state
        .admitted_flow
        .closure()
        .definition(state.admitted_flow.closure().root())
        .unwrap()
        .node(&next)
        .unwrap()
        .kind(),
      FlowNodeKind::DeterministicGate | FlowNodeKind::HumanGate
    ) {
      return state;
    }
    let step = crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      f.store.as_ref(),
      run,
      claim.id,
      Timestamp::from_unix_millis(tick).unwrap(),
    )
    .await
    .unwrap();
    if step == FactoryFlowStep::Waiting {
      return f.store.factory_run_snapshot(run).await.unwrap();
    }
  }
  panic!("delivery must reach an external stage or human gate");
}

#[test]
fn issue_delivery_routes_frozen_kind_size_component_and_priority() {
  run_ready(async {
    for (kind, size, component, priority, repro, target) in [
      (
        "feature_request",
        IssueSize::Small,
        "api",
        "high",
        "not_applicable",
        "protected_tests",
      ),
      (
        "feature_request",
        IssueSize::Medium,
        "compiler",
        "high",
        "not_applicable",
        "protected_tests",
      ),
      (
        "feature_request",
        IssueSize::Medium,
        "api",
        "low",
        "not_applicable",
        "protected_tests",
      ),
      (
        "feature_request",
        IssueSize::Medium,
        "api",
        "high",
        "not_applicable",
        "requirements_review",
      ),
      (
        "feature_request",
        IssueSize::Medium,
        "api",
        "blocker",
        "not_applicable",
        "requirements_review",
      ),
      (
        "feature_request",
        IssueSize::Large,
        "compiler",
        "low",
        "not_applicable",
        "specification_author",
      ),
      ("defect", IssueSize::Small, "compiler", "low", "unknown", "reproduction"),
      (
        "defect",
        IssueSize::Large,
        "api",
        "high",
        "needs_input",
        "review-reproduction",
      ),
    ] {
      let (f, issues, issue, published) = admitted_issue(kind, size, component, priority, repro).await;
      let run = published.association.unwrap().run_id;
      let frozen = f.store.factory_run_snapshot(run).await.unwrap();
      change_issue(issues.as_ref(), &issue).await;
      let after = advance_gates(&f, run, 40).await;
      assert_eq!(
        next_node(&after),
        key(target),
        "{kind}/{size:?}/{component}/{priority}/{repro}"
      );
      assert_eq!(after.work, frozen.work);
      assert_eq!(after.flow.data.incoming, frozen.flow.data.incoming);
      assert!(
        after.flow.data.build_intents.is_empty(),
        "gates cannot start unselected stages"
      );
      if target == "protected_tests" || target == "requirements_review" {
        let policy = after
          .flow
          .data
          .records
          .iter()
          .find(|r| r.input().node() == &key("requirements_review_policy"))
          .unwrap();
        assert_eq!(
          policy.outcome(),
          &key(if target == "protected_tests" {
            "automatic"
          } else {
            "review"
          })
        );
        assert_eq!(policy.payload().value()["priority"], priority);
        assert_eq!(policy.payload().value()["component"], component);
        assert_eq!(policy.payload().value()["size"], serde_json::to_value(size).unwrap());
        assert!(
          !after
            .flow
            .data
            .inputs
            .iter()
            .any(|i| i.node() == &key("specification_author"))
        );
      }
    }
  });
}

#[test]
fn issue_delivery_human_acceptance_retains_the_reviewed_issue_after_edits() {
  run_ready(async {
    for disposition in [FlowHumanDisposition::Resume, FlowHumanDisposition::Abort] {
      let (f, issues, issue, published) =
        admitted_issue("feature_request", IssueSize::Medium, "api", "high", "not_applicable").await;
      let run = published.association.unwrap().run_id;
      let initial = f.store.factory_run_snapshot(run).await.unwrap();
      issues
        .edit_issue(testing::management_mutation(EditIssue {
          intent: EditIssueIntent {
            project_id: f.project,
            id: issue.id,
            expected_version: issue.version,
            content: IssueContent {
              priority: Some(IssuePriority::Choice("low".into())),
              ..issue.content
            },
          },
          idempotency_key: "lower-live-priority".parse().unwrap(),
          edited_at: Timestamp::from_unix_millis(31).unwrap(),
        }))
        .await
        .unwrap();
      let selected = advance_gates(&f, run, 40).await;
      assert_eq!(next_node(&selected), key("requirements_review"));
      let owner = FactoryFlowRunner::new(
        f.store.clone(),
        Arc::new(crate::factory_flow_tests::FakePending),
        Arc::new(GateInputs),
        crate::factory_node_test_support::clock(),
      );
      let claim = selected.current_claim.unwrap().id;
      for tick in 81..85 {
        crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          f.store.as_ref(),
          run,
          claim,
          Timestamp::from_unix_millis(tick).unwrap(),
        )
        .await
        .unwrap();
      }
      let waiting = f.store.factory_run_snapshot(run).await.unwrap();
      let attempt = waiting
        .flow
        .attempts
        .iter()
        .find(|a| a.node_key() == &key("requirements_review"))
        .unwrap();
      let input = waiting
        .flow
        .data
        .inputs
        .iter()
        .find(|i| i.digest().unwrap() == attempt.input_digest())
        .unwrap();
      assert_eq!(input.payload().value()["priority"], "high");
      assert_eq!(input.payload().value()["size"], "medium");
      assert!(
        waiting.flow.data.build_intents.is_empty(),
        "pending approval cannot dispatch tests"
      );
      assert_eq!(
        owner
          .reconcile_flow(run, claim, Timestamp::from_unix_millis(90).unwrap())
          .await
          .unwrap(),
        FactoryFlowStep::Waiting
      );
      assert_eq!(f.store.factory_run_snapshot(run).await.unwrap(), waiting);
      review_requirements(f.store.clone(), &waiting, disposition, 110).await;
      let after = advance_gates(&f, run, 120).await;
      if disposition == FlowHumanDisposition::Resume {
        assert_eq!(next_node(&after), key("protected_tests"));
        let accepted = after
          .flow
          .data
          .records
          .iter()
          .find(|r| r.input().node() == &key("requirements_approved"))
          .unwrap();
        let contract = AcceptedWorkContract::freeze(
          &after.work,
          &after.admitted_flow,
          &after.flow.data,
          after.flow.runtime(),
          accepted.node_attempt_id(),
        )
        .unwrap();
        assert_eq!(contract.work(), &initial.work);
        assert_eq!(accepted.payload().value(), input.payload().value());
      } else {
        assert!(
          matches!(progress(&after),FlowProgress::Terminal {key:terminal,..} if terminal==key("needs_correction"))
        );
        assert!(
          !after
            .flow
            .data
            .records
            .iter()
            .any(|r| r.input().node() == &key("requirements_approved"))
        );
      }
      assert!(after.flow.data.build_intents.is_empty());
      assert_eq!(after.work, initial.work);
      assert_eq!(after.flow.data.incoming, initial.flow.data.incoming);
    }
  });
}

async fn change_issue(issues: &testing::InMemoryProjectStore, issue: &IssueRevision) {
  let kind = issue.planning.issue_type.as_deref().unwrap();
  let size = issue.planning.size.unwrap();
  let component = issue.planning.component.as_deref().unwrap();
  let Some(IssuePriority::Choice(priority)) = &issue.content.priority else {
    panic!("Project priority")
  };
  let changed = issues
    .edit_issue(testing::management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          priority: Some(IssuePriority::Choice(
            if priority == "low" { "high" } else { "low" }.into(),
          )),
          ..issue.content.clone()
        },
      },
      idempotency_key: "change-priority".parse().unwrap(),
      edited_at: Timestamp::from_unix_millis(31).unwrap(),
    }))
    .await
    .unwrap()
    .issue;
  let mut planning = changed.planning.clone();
  planning.size = Some(if size == IssueSize::Large {
    IssueSize::Small
  } else {
    IssueSize::Large
  });
  planning.component = Some(if component == "api" { "compiler" } else { "api" }.into());
  planning.issue_type = Some(if kind == "defect" { "feature_request" } else { "defect" }.into());
  planning.values.insert(
    "repro".into(),
    IssuePropertyValue::Choice(if kind == "defect" { "not_applicable" } else { "unknown" }.into()),
  );
  issues
    .edit_issue_planning(testing::management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: changed.version,
        planning,
      },
      assignees: Default::default(),
      idempotency_key: "change-routing".parse().unwrap(),
      edited_at: Timestamp::from_unix_millis(32).unwrap(),
    }))
    .await
    .unwrap();
}

async fn review_requirements(
  store: Arc<testing::InMemoryFactoryConfigurationStore>,
  waiting: &FactoryRunSnapshot,
  disposition: FlowHumanDisposition,
  requested_at: i64,
) {
  let run = waiting.run.id();
  let attempt = waiting
    .flow
    .attempts
    .iter()
    .find(|a| a.node_key() == &key("requirements_review"))
    .unwrap();
  let operator = ManagementRequestContext::new(
    ManagementActor::authenticated("requirements-reviewer").unwrap(),
    crate::ManagementSecurityScope::new("review-tests").unwrap(),
    ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .unwrap();
  let controls = FactoryControlHandlers::new(store);
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  assert!(
    controls
      .execute_management_command(
        &operator,
        &grant,
        SubmitFactoryNodeResponseCommand {
          run_id: run,
          node_attempt_id: attempt.id(),
          outcome: key("approved"),
          value: serde_json::json!({"operation":"escalation","disposition":"resume","reason":"untyped bypass"}),
          expected_version: waiting.run.version(),
          idempotency_key: "generic-response".parse().unwrap(),
          requested_at: Timestamp::from_unix_millis(requested_at).unwrap()
        }
      )
      .await
      .is_err()
  );
  controls
    .execute_management_command(
      &operator,
      &grant,
      ConfirmFactoryNodeOperationCommand {
        run_id: run,
        node_attempt_id: attempt.id(),
        operation: FlowHumanOperation::Escalation,
        disposition,
        reason: FactoryText::new("Review the exact retained requirements").unwrap(),
        expected_version: waiting.run.version(),
        idempotency_key: "review-requirements".parse().unwrap(),
        requested_at: Timestamp::from_unix_millis(requested_at).unwrap(),
      },
    )
    .await
    .unwrap();
}

#[test]
fn issue_delivery_does_not_treat_unattributed_dependencies_as_an_empty_list() {
  run_ready(async {
    let (f, issues, issue, published) =
      admitted_issue("feature_request", IssueSize::Small, "compiler", "low", "not_applicable").await;
    let original = f
      .store
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    // Restore a legacy revision with dependency data but no assessment origin.
    // A real claim must still pass the ordinary classification applicability checks.
    let mut retained = issue.clone();
    retained.version = issue.version.next().unwrap();
    retained.updated_at = Timestamp::from_unix_millis(31).unwrap();
    retained
      .classification
      .dependencies
      .push(IssueClassificationDependencyReference {
        work_id: original.work.id(),
        run_id: original.run.id(),
        work_digest: phase_pool_work_digest(&original),
        terminal: key("done"),
      });
    issues.seed_retained_issue_revision(retained.clone()).unwrap();
    let before = f.store.mutation_evidence_counts().await;
    let error = AuthorizedCommandHandler::new(
      f.policy.clone(),
      Arc::new(IssueExecutionHandlers::new(
        issues.clone(),
        f.store.clone(),
        f.policy.clone(),
      )),
    )
    .handle_command(
      &f.context,
      RequestIssueExecutionCommand {
        intent: RequestIssueExecutionIntent {
          expected_snapshot_digest: None,
          project_id: f.project,
          issue_id: issue.id,
          expected_version: retained.version,
          target: published.intent.target,
          classification: None,
        },
        idempotency_key: "unattributed-dependencies".parse().unwrap(),
        requested_at: Timestamp::from_unix_millis(32).unwrap(),
      },
    )
    .await
    .expect_err("nonempty dependency claims cannot bypass their assessment");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected snapshot diagnostics: {error:?}")
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/fields/dependencies");
    assert_eq!(report.errors[0].code, "issue_snapshot_inapplicable");
    assert_eq!(f.store.mutation_evidence_counts().await, before);
  });
}
