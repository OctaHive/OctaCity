use super::*;

#[path = "issue_execution_job_tests.rs"]
mod jobs;
#[path = "issue_execution_journal_tests.rs"]
mod journal;

#[test]
fn issue_execution_result_collections_retain_original_candidate_evidence_and_decision_subjects() {
  run_ready(async {
    for (collection, kind, extension) in [
      (
        FactoryDiagnosticCollection::ChangeSets,
        "changeset",
        serde_json::json!({
          "id":"88888888-8888-4888-8888-888888888888"
        }),
      ),
      (
        FactoryDiagnosticCollection::Evidence,
        "evidence",
        serde_json::json!({
          "id":"88888888-8888-4888-8888-888888888888", "changeset_id":"99999999-9999-4999-8999-999999999999",
          "digest":FactoryDigest::from_bytes([93;32]), "item_count":1
        }),
      ),
      (
        FactoryDiagnosticCollection::Decisions,
        "decision",
        serde_json::json!({
          "id":"88888888-8888-4888-8888-888888888888", "outcome":"accept"
        }),
      ),
    ] {
      let (store, factory, issue, mut fixture) = associated_journal().await;
      let subject = octacity_server_factory::CandidateSubject::new(
        fixture.admission.work.subject().clone(),
        octacity_server_domain::ImmutableRevision::new("original-candidate").unwrap(),
        FactoryDigest::from_bytes([91; 32]),
      );
      let mut facts = extension;
      facts["kind"] = serde_json::json!(kind);
      facts["subject"] = serde_json::json!(subject);
      if kind == "changeset" {
        facts["node_attempt_id"] = serde_json::json!(fixture.attempt.id());
      }
      fixture.discovery = Some(serde_json::from_value(facts).unwrap());
      testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory, policy)),
      );
      let page = queries
        .handle_query(
          &context(),
          ListIssueExecutionRecordsQuery::try_new(
            GetIssueWorkExecutionQuery {
              project_id: issue.project_id,
              issue_id: issue.id,
              work_id: fixture.admission.work.id(),
            },
            collection,
            None,
            1,
          )
          .unwrap(),
        )
        .await
        .unwrap();
      let FactoryDiagnosticRecord::DomainFact(record) = &page.page.items[0] else {
        panic!("domain result")
      };
      let facts = serde_json::to_value(&record.facts).unwrap();
      assert_eq!(facts["kind"], kind);
      assert_eq!(facts["subject"], serde_json::to_value(subject).unwrap());
      assert_eq!(record.input_digest, fixture.input.digest().unwrap());
      assert_eq!(record.node_attempt_id, fixture.attempt.id());
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    }
  });
}

#[test]
fn issue_execution_reads_preserve_independent_visibility_and_common_collection_cursor_checks() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let selection = GetIssueWorkExecutionQuery {
      project_id: issue.project_id,
      issue_id: issue.id,
      work_id: fixture.admission.work.id(),
    };
    for blocked in [ManagementResourceKind::FactoryRun, ManagementResourceKind::Project] {
      let policy = Arc::new(ResourcePolicy { blocked });
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      let observation = queries.handle_query(&context(), selection.clone()).await.unwrap();
      assert!(matches!(observation.run, IssueRunObservation::Unavailable));
      assert!(
        queries
          .handle_query(
            &context(),
            ListIssueExecutionRecordsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Calls, None, 1,)
              .unwrap()
          )
          .await
          .is_err()
      );
    }
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueExecutionHandlers::new(store.clone(), factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(policy, handlers.clone());
    let first = queries
      .handle_query(
        &context(),
        ListIssueExecutionRecordsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Budgets, None, 1)
          .unwrap(),
      )
      .await
      .unwrap();
    let after = first.page.next_cursor.expect("several original budget records");
    assert!(
      queries
        .handle_query(
          &context(),
          ListIssueExecutionRecordsQuery::try_new(
            selection.clone(),
            FactoryDiagnosticCollection::Calls,
            Some(after),
            1,
          )
          .unwrap()
        )
        .await
        .is_err()
    );
    for limit in [0, MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE + 1] {
      assert!(
        ListIssueExecutionRecordsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Calls, None, limit)
          .is_err()
      );
    }
    let hidden = AuthorizedQueryHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::View],
        visibility: ManagementVisibility::none(),
      }),
      handlers,
    );
    assert_eq!(
      hidden
        .handle_query(&context(), selection)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::NotFound
    );
  });
}

#[test]
fn issue_execution_records_keep_exact_indeterminate_subjects_missing_evidence_and_safe_call_profiles() {
  run_ready(async {
    let (store, factory, issue, mut fixture) = associated_journal().await;
    let subject = octacity_server_factory::CandidateSubject::new(
      fixture.admission.work.subject().clone(),
      octacity_server_domain::ImmutableRevision::new("historical-head").unwrap(),
      FactoryDigest::from_bytes([91; 32]),
    );
    fixture.discovery = Some(
      serde_json::from_value(serde_json::json!({
        "kind":"assessment", "subject":subject,
        "id":"88888888-8888-4888-8888-888888888888", "plan_id":"99999999-9999-4999-8999-999999999999",
        "digest":FactoryDigest::from_bytes([90;32]), "evaluator":fixture.schema.reference(),
        "model":fixture.schema.reference(), "prompt_digest":FactoryDigest::from_bytes([92;32]),
        "outcome":"indeterminate"
      }))
      .unwrap(),
    );
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory, policy)),
    );
    let execution = GetIssueWorkExecutionQuery {
      project_id: issue.project_id,
      issue_id: issue.id,
      work_id: fixture.admission.work.id(),
    };
    let before = store.mutation_evidence_counts().await;
    let assessments = queries
      .handle_query(
        &context(),
        ListIssueExecutionRecordsQuery::try_new(execution.clone(), FactoryDiagnosticCollection::Assessments, None, 1)
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(assessments.association.work_id, fixture.admission.work.id());
    assert_eq!(assessments.association.requirements_version, None);
    let FactoryDiagnosticRecord::DomainFact(fact) = &assessments.page.items[0] else {
      panic!("assessment")
    };
    assert_eq!(fact.node_attempt_id, fixture.attempt.id());
    assert_eq!(fact.flow_run_id, fixture.input.flow_run_id());
    assert_eq!(fact.cycle_id, fixture.input.cycle_id());
    let facts = serde_json::to_value(&fact.facts).unwrap();
    assert_eq!(facts["subject"], serde_json::to_value(subject).unwrap());
    assert_eq!(facts["outcome"], "indeterminate");
    for collection in [
      FactoryDiagnosticCollection::Evidence,
      FactoryDiagnosticCollection::Decisions,
    ] {
      let missing = queries
        .handle_query(
          &context(),
          ListIssueExecutionRecordsQuery::try_new(execution.clone(), collection, None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert!(missing.page.items.is_empty());
      assert_eq!(missing.observation, "live-factory-collection-v1");
      assert!(serde_json::to_value(missing).unwrap().get("accepted").is_none());
    }
    let calls = queries
      .handle_query(
        &context(),
        ListIssueExecutionRecordsQuery::try_new(execution, FactoryDiagnosticCollection::Calls, None, 1).unwrap(),
      )
      .await
      .unwrap();
    let FactoryDiagnosticRecord::Call(call) = &calls.page.items[0] else {
      panic!("call profile")
    };
    assert_eq!(call.node.id, fixture.attempt.id());
    assert!(call.profile.is_some());
    let value = serde_json::to_value(calls).unwrap().to_string();
    for private in [
      "rendered_prompt",
      "transcript",
      "claim",
      "owner",
      "payload",
      "permission_set",
    ] {
      assert!(!value.contains(&format!("\"{private}\"")), "{private}");
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

async fn associated_journal() -> (
  Arc<InMemoryProjectStore>,
  Arc<testing::InMemoryFactoryConfigurationStore>,
  IssueRevision,
  testing::FactoryNodeJournalContractFixture,
) {
  let fixture = testing::factory_node_journal_contract_fixture();
  let project_id = fixture.admission.work.subject().project_id();
  let store = crate::issue_test_support::manual_issue_store(project_id).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let issue = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)))
    .handle_command(&context(), create(project_id, "journal-issue"))
    .await
    .unwrap()
    .issue;
  let factory = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  factory
    .seed_factory_admission(
      fixture.admission.clone(),
      FactoryWorkSourceScope {
        source: "manual".parse().unwrap(),
        security_scope: context().security_scope().to_store(),
      },
      FactoryDigest::from_bytes([1; 32]),
      testing::management_mutation_with_request((), "journal-admission").audit(),
    )
    .unwrap();
  store
    .associate_issue_work(management_mutation(AssociateIssueWork {
      project_id,
      issue_id: issue.id,
      request_id: None,
      admission: fixture.admission.clone(),
    }))
    .await
    .unwrap();
  (store, factory, issue, fixture)
}

#[test]
fn issue_execution_projection_reads_artifact_lineage_without_inventing_a_requirements_revision() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store, factory, policy)),
    );
    let projection = queries
      .handle_query(
        &context(),
        GetIssueWorkExecutionQuery {
          project_id: issue.project_id,
          issue_id: issue.id,
          work_id: fixture.admission.work.id(),
        },
      )
      .await
      .unwrap();
    assert_eq!(projection.association.requirements_version, None);
    assert_eq!(projection.association.request_id, None);
    let IssueRunObservation::Available(run) = projection.run else {
      panic!("linked artifact run")
    };
    assert_eq!(run.id, fixture.admission.run.id());
    assert!(run.diagnostics.latest_decision.is_none());
    let foreign = queries
      .handle_query(
        &context(),
        GetIssueWorkExecutionQuery {
          project_id: issue.project_id,
          issue_id: IssueId::generate(),
          work_id: fixture.admission.work.id(),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      foreign.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
  });
}

#[test]
fn issue_execution_projection_keeps_preparation_parallel_runs_and_old_requirements_separate() {
  run_ready(async {
    let (store, factory, issue, command) = fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      policy.clone(),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers);
    let mut runs = Vec::new();
    for ordinal in 0..2 {
      let mut next = command.clone();
      next.idempotency_key = key(&format!("parallel-{ordinal}"));
      let request = commands.handle_command(&context(), next).await.unwrap().request;
      let query = GetIssueExecutionProjectionQuery {
        execution: GetIssueExecutionQuery {
          project_id: issue.project_id,
          issue_id: issue.id,
          request_id: request.id,
        },
      };
      let preparing = queries.handle_query(&context(), query.clone()).await.unwrap();
      assert!(matches!(preparing.run, IssueRunObservation::Preparing));
      let admission = admit_request(factory.clone(), &request).await;
      store
        .associate_issue_work(management_mutation(AssociateIssueWork {
          project_id: issue.project_id,
          issue_id: issue.id,
          request_id: Some(request.id),
          admission: admission.clone(),
        }))
        .await
        .unwrap();
      let current = queries.handle_query(&context(), query.clone()).await.unwrap();
      assert_eq!(current.requirements, IssueRequirementsRelation::Current);
      let IssueRunObservation::Available(run) = current.run else {
        panic!("associated run must be readable")
      };
      assert_eq!(run.id, admission.run.id());
      assert_eq!(run.state, admission.run.state());
      assert!(run.diagnostics.current_nodes.is_empty());
      assert!(run.diagnostics.latest_decision.is_none());
      runs.push((query, run.id));
    }
    assert_ne!(runs[0].1, runs[1].1);
    let mut edit = draft();
    edit.description = "new requirements".into();
    AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)))
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            content: edit,
          },
          idempotency_key: key("requirements-changed"),
          edited_at: at(30),
        },
      )
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    for (query, original) in runs {
      let value = queries.handle_query(&context(), query).await.unwrap();
      assert_eq!(value.requirements, IssueRequirementsRelation::Historical);
      assert_eq!(value.request.intent.expected_version, issue.version);
      let IssueRunObservation::Available(run) = value.run else {
        panic!("retained run")
      };
      assert_eq!(run.id, original);
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_results_bind_pages_to_original_run_observation_and_keep_incomplete_coverage_explicit() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      policy.clone(),
    ));
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let selection = GetIssueWorkExecutionQuery {
      project_id: issue.project_id,
      issue_id: issue.id,
      work_id: fixture.admission.work.id(),
    };
    let first = queries
      .handle_query(
        &context(),
        ListIssueResultsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Budgets, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.coverage, IssueResultCoverage::Partial);
    assert_eq!(first.items.len(), 1);
    let cursor = first.next_cursor.clone().expect("more budgets");
    let mut content = draft();
    content.description = "changed current requirements".into();
    AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    )
    .handle_command(
      &context(),
      EditIssueCommand {
        intent: EditIssueIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          content,
        },
        idempotency_key: key("edit-observed-card"),
        edited_at: at(40),
      },
    )
    .await
    .unwrap();
    let last = queries
      .handle_query(
        &context(),
        ListIssueResultsQuery::try_new(
          selection.clone(),
          FactoryDiagnosticCollection::Budgets,
          Some(cursor.clone()),
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.boundary, last.boundary);
    assert!(last.current_version > first.boundary.issue_version);
    assert_eq!(first.run_version, last.run_version);
    assert_eq!(last.coverage, IssueResultCoverage::Partial);
    assert_ne!(
      serde_json::to_value(&first.items).unwrap(),
      serde_json::to_value(&last.items).unwrap()
    );
    AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryControlHandlers::new(factory.clone())),
    )
    .handle_command(
      &context(),
      CancelFactoryRunCommand {
        run_id: fixture.admission.run.id(),
        expected_version: first.run_version.unwrap(),
        idempotency_key: key("change-observed-run"),
        requested_at: at(50),
      },
    )
    .await
    .unwrap();
    let stale = queries
      .handle_query(
        &context(),
        ListIssueResultsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Budgets, Some(cursor), 1)
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(stale.coverage, IssueResultCoverage::Stale);
    assert!(stale.items.is_empty());
    assert!(stale.next_cursor.is_none());
    let missing = queries
      .handle_query(
        &context(),
        ListIssueResultsQuery::try_new(selection, FactoryDiagnosticCollection::Evidence, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(missing.coverage, IssueResultCoverage::Partial);
    assert!(missing.items.is_empty());
    assert!(serde_json::to_value(missing).unwrap().get("accepted").is_none());
  });
}

#[test]
fn issue_results_keep_retained_references_unavailable_without_factory_rights_and_bind_cursor_filters() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let selection = GetIssueWorkExecutionQuery {
      project_id: issue.project_id,
      issue_id: issue.id,
      work_id: fixture.admission.work.id(),
    };
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let first = queries
      .handle_query(
        &context(),
        ListIssueResultsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Budgets, None, 1).unwrap(),
      )
      .await
      .unwrap();
    let cursor = first.next_cursor.unwrap();
    for (execution, collection) in [
      (selection.clone(), FactoryDiagnosticCollection::Evidence),
      (
        GetIssueWorkExecutionQuery {
          work_id: octacity_server_factory::WorkEnvelopeId::generate(),
          ..selection.clone()
        },
        FactoryDiagnosticCollection::Budgets,
      ),
    ] {
      assert_eq!(
        queries
          .handle_query(
            &context(),
            ListIssueResultsQuery::try_new(execution, collection, Some(cursor.clone()), 1).unwrap()
          )
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Invalid
      );
    }
    for blocked in [ManagementResourceKind::FactoryRun, ManagementResourceKind::Project] {
      let policy = Arc::new(ResourcePolicy { blocked });
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      let page = queries
        .handle_query(
          &context(),
          ListIssueResultsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Evidence, None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(page.coverage, IssueResultCoverage::Unavailable);
      assert!(page.items.is_empty());
      assert!(page.next_cursor.is_none());
      assert!(page.run_version.is_none());
      assert_eq!(page.association.work_id, selection.work_id);
    }
    for limit in [0, 101] {
      assert!(
        ListIssueResultsQuery::try_new(selection.clone(), FactoryDiagnosticCollection::Evidence, None, limit).is_err()
      );
    }
    for cursor in ["bad".to_owned(), "x".repeat(MAX_ISSUE_OBSERVATION_CURSOR_BYTES + 1)] {
      assert!(
        ListIssueResultsQuery::try_new(
          selection.clone(),
          FactoryDiagnosticCollection::Evidence,
          Some(cursor),
          1
        )
        .is_err()
      );
    }
  });
}
