//! Accepted producer commands exercise the same authorized transition boundary as manual moves.
use super::*;
use octacity_server_factory::{FactoryKey, FlowRunId, NodeAttemptId, WorkflowCycleId};

async fn linked() -> (
  Arc<InMemoryProjectStore>,
  IssueRevision,
  IssueWorkAssociation,
  Arc<testing::InMemoryFactoryConfigurationStore>,
) {
  linked_with_evidence(vec![]).await
}
async fn linked_with_evidence(
  evidence: Vec<IssueTransitionEvidence>,
) -> (
  Arc<InMemoryProjectStore>,
  IssueRevision,
  IssueWorkAssociation,
  Arc<testing::InMemoryFactoryConfigurationStore>,
) {
  let (store, factory, mut issue, mut command) = fixture().await;
  if !evidence.is_empty() {
    let workflow = store
      .issue_workflow(issue.project_id, issue.workflow_id, Some(issue.workflow_version))
      .await
      .unwrap();
    let mut definition = workflow.definition.clone();
    definition.transitions[0].evidence = evidence;
    let target = store
      .publish_issue_workflow(testing::management_mutation(PublishIssueWorkflow {
        intent: PublishIssueWorkflowIntent {
          project_id: issue.project_id,
          id: issue.workflow_id,
          expected_current_version: Some(issue.workflow_version),
          definition,
        },
        idempotency_key: key("guarded-workflow"),
        published_at: at(11),
      }))
      .await
      .unwrap()
      .workflow;
    issue = store
      .adopt_issue_workflow(testing::management_mutation(AdoptIssueWorkflow {
        intent: AdoptIssueWorkflowIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          expected_workflow_id: issue.workflow_id,
          expected_workflow_version: issue.workflow_version,
          expected_status_id: issue.status_id,
          target_workflow_id: target.id,
          target_workflow_version: target.version,
          status_correspondence: target.definition.statuses.iter().map(|s| (s.id, s.id)).collect(),
          expected_field_policy: Default::default(),
        },
        idempotency_key: key("adopt-guarded"),
        adopted_at: at(12),
      }))
      .await
      .unwrap()
      .issue;
    command.intent.expected_version = issue.version;
  }
  let request = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    )),
  )
  .handle_command(&context(), command)
  .await
  .unwrap()
  .request;
  let admission = admit_request(factory.clone(), &request).await;
  let association = store
    .associate_issue_work(testing::management_mutation(AssociateIssueWork {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: Some(request.id),
      admission,
    }))
    .await
    .unwrap()
    .association;
  (store, issue, association, factory)
}
fn action(
  issue: &IssueRevision,
  association: &IssueWorkAssociation,
  edge: &str,
  operation: u8,
) -> ApplyIssuePipelineStatusCommand {
  ApplyIssuePipelineStatusCommand {
    intent: TransitionIssueIntent {
      project_id: issue.project_id,
      id: issue.id,
      expected_version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      expected_status_id: issue.status_id,
      transition_id: edge.parse().unwrap(),
      reason: None,
    },
    accepted: AcceptedIssueStatusAction::for_selection(
      IssuePipelineStatusSelection {
        workflow_id: issue.workflow_id,
        workflow_version: issue.workflow_version,
        transition_id: edge.parse().unwrap(),
        outcome: FactoryKey::new("configured-eligible").unwrap(),
      },
      IssuePipelineActionProvenance {
        issue_id: issue.id,
        requirements_version: association.requirements_version.unwrap(),
        work_id: association.work_id,
        run_id: association.run_id,
        flow_run_id: FlowRunId::generate(),
        cycle_id: WorkflowCycleId::generate(),
        node_attempt_id: NodeAttemptId::generate(),
        candidate: None,
        evidence: vec![],
        operation_id: FactoryDigest::from_bytes([operation; 32]),
        outcome: FactoryKey::new("configured-eligible").unwrap(),
      },
    )
    .unwrap(),
    moved_at: at(30 + i64::from(operation)),
  }
}
#[test]
fn issue_pipeline_action_moves_through_shared_transition_without_provider_or_execution_effects() {
  run_ready(async {
    let (store, issue, association, factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let frozen = factory.factory_run_snapshot(association.run_id).await.unwrap();
    let command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 1);
    let moved = handlers.handle_command(&context(), command).await.unwrap();
    assert_eq!(
      moved.issue.status_id.to_string(),
      "10000000-0000-4000-8000-000000000002"
    );
    assert_eq!(moved.transition.origin, IssueTransitionOrigin::Pipeline);
    assert_eq!(moved.transition.pipeline.as_ref().unwrap().work_id, association.work_id);
    assert_eq!(
      store
        .issue_work_association(issue.project_id, association.work_id)
        .await
        .unwrap(),
      Some(association)
    );
    assert_eq!(moved.issue.content, issue.content);
    let retained = factory.factory_run_snapshot(frozen.run.id()).await.unwrap();
    assert_eq!(retained.flow, frozen.flow);
    assert_eq!(retained.run, frozen.run);
  });
}

#[test]
fn issue_pipeline_action_cannot_retarget_its_configured_workflow_edge_or_outcome() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let mut command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 2);
    let selection = IssuePipelineStatusSelection {
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      transition_id: command.intent.transition_id,
      outcome: FactoryKey::new("configured-eligible").unwrap(),
    };
    command.accepted =
      AcceptedIssueStatusAction::for_selection(selection.clone(), command.accepted.provenance().clone()).unwrap();
    command.intent.transition_id = "20000000-0000-4000-8000-000000000003".parse().unwrap();
    let before = store.mutation_evidence_counts().await;
    assert!(handlers.handle_command(&context(), command.clone()).await.is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    let mut provenance = command.accepted.provenance().clone();
    provenance.outcome = FactoryKey::new("raw-model-ready").unwrap();
    assert!(AcceptedIssueStatusAction::for_selection(selection, provenance).is_err());
  });
}

#[test]
fn issue_pipeline_action_keeps_sequential_lineage_and_original_replay_after_edit() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let first = handlers
      .handle_command(
        &context(),
        action(&issue, &association, "20000000-0000-4000-8000-000000000001", 3),
      )
      .await
      .unwrap();
    let next = action(&first.issue, &association, "20000000-0000-4000-8000-000000000002", 4);
    let second = handlers.handle_command(&context(), next.clone()).await.unwrap();
    assert_eq!(
      second.issue.status_id.to_string(),
      "10000000-0000-4000-8000-000000000003"
    );
    let edited = handlers
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: second.issue.version,
            content: IssueContent {
              description: "Later requirements".into(),
              ..issue.content.clone()
            },
          },
          idempotency_key: key("after-pipeline"),
          edited_at: at(50),
        },
      )
      .await
      .unwrap();
    let replay = handlers.handle_command(&context(), next).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, second.issue);
    assert_eq!(
      store.issue(issue.project_id, issue.id, None).await.unwrap(),
      edited.issue
    );
    let history = store
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap();
    assert_eq!(history.items.len(), 3);
  });
}

#[test]
fn issue_pipeline_action_rejects_forged_issue_or_requirements_lineage() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    for wrong_issue in [false, true] {
      let mut command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 8);
      let selection = command.accepted.selection().clone();
      let mut provenance = command.accepted.provenance().clone();
      if wrong_issue {
        provenance.issue_id = IssueId::generate();
      } else {
        provenance.requirements_version = issue.version.next().unwrap();
      }
      command.accepted = AcceptedIssueStatusAction::for_selection(selection, provenance).unwrap();
      assert!(handlers.handle_command(&context(), command).await.is_err());
    }
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

#[test]
fn issue_pipeline_action_failed_acceptance_cannot_be_retargeted_to_a_new_head() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 9);
    let edited = handlers
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            content: IssueContent {
              priority: Some(7),
              ..issue.content.clone()
            },
          },
          idempotency_key: key("priority-before-acceptance"),
          edited_at: at(25),
        },
      )
      .await
      .unwrap();
    assert!(handlers.handle_command(&context(), command.clone()).await.is_err());
    let mut retarget = command.clone();
    retarget.intent.expected_version = edited.issue.version;
    assert!(handlers.handle_command(&context(), retarget).await.is_err());
    assert!(handlers.handle_command(&context(), command).await.is_err());
    assert_eq!(
      store.issue(issue.project_id, issue.id, None).await.unwrap(),
      edited.issue
    );
  });
}

#[test]
fn issue_pipeline_action_rejects_manual_and_content_changes_even_after_returning_to_original_values() {
  run_ready(async {
    for status in [false, true] {
      let (store, issue, association, _factory) = linked().await;
      let handlers = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(IssueHandlers::new(
          store.clone(),
          Arc::new(TrustedNetworkManagementPolicy),
        )),
      );
      let mut current = issue.clone();
      if status {
        for (edge, suffix) in [
          ("20000000-0000-4000-8000-000000000001", "triage"),
          ("20000000-0000-4000-8000-000000000003", "decline"),
          ("20000000-0000-4000-8000-000000000008", "reopen"),
        ] {
          let mut movement = move_from(&current, suffix);
          movement.intent.transition_id = edge.parse().unwrap();
          movement.moved_at = at(40 + current.version.get() as i64);
          current = handlers.handle_command(&context(), movement).await.unwrap().issue;
        }
      } else {
        for (description, identity) in [("Changed", "change"), ("", "restore")] {
          current = handlers
            .handle_command(
              &context(),
              EditIssueCommand {
                intent: EditIssueIntent {
                  project_id: issue.project_id,
                  id: issue.id,
                  expected_version: current.version,
                  content: IssueContent {
                    description: description.into(),
                    ..issue.content.clone()
                  },
                },
                idempotency_key: key(identity),
                edited_at: at(40 + current.version.get() as i64),
              },
            )
            .await
            .unwrap()
            .issue;
        }
      }
      assert_eq!(current.content, issue.content);
      assert_eq!(current.status_id, issue.status_id);
      let mut stale = action(&current, &association, "20000000-0000-4000-8000-000000000001", 10);
      stale.moved_at = at(60);
      assert!(handlers.handle_command(&context(), stale).await.is_err());
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), current);
    }
  });
}

#[test]
fn issue_pipeline_action_new_execution_and_unrelated_metadata_do_not_invalidate_but_competing_move_does() {
  run_ready(async {
    let (store, issue, association, factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let current = handlers
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            content: IssueContent {
              priority: Some(8),
              ..issue.content.clone()
            },
          },
          idempotency_key: key("priority"),
          edited_at: at(21),
        },
      )
      .await
      .unwrap()
      .issue;
    let work = factory.factory_run_snapshot(association.run_id).await.unwrap().work;
    let request = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    )
    .handle_command(
      &context(),
      request(
        &current,
        work.configuration().id(),
        work.subject().repository().unwrap().repository_id(),
        "parallel",
      ),
    )
    .await
    .unwrap()
    .request;
    let admission = admit_request(factory.clone(), &request).await;
    let parallel = store
      .associate_issue_work(testing::management_mutation(AssociateIssueWork {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: Some(request.id),
        admission,
      }))
      .await
      .unwrap()
      .association;
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), current);
    let moved = handlers
      .handle_command(
        &context(),
        action(&current, &association, "20000000-0000-4000-8000-000000000001", 11),
      )
      .await
      .unwrap();
    let stale = action(&moved.issue, &parallel, "20000000-0000-4000-8000-000000000002", 12);
    assert!(handlers.handle_command(&context(), stale).await.is_err());
    let history = store
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap();
    assert_eq!(history.items.len(), 2);
    assert_eq!(
      store
        .issue_work_association(issue.project_id, association.work_id)
        .await
        .unwrap(),
      Some(association)
    );
  });
}

#[test]
fn issue_pipeline_action_checks_current_rights_source_ownership_and_missing_association() {
  run_ready(async {
    for condition in ["rights", "hidden", "source", "missing", "run"] {
      let (store, issue, association, _factory) = linked().await;
      let policy: Arc<dyn ManagementAuthorizationPolicy> = match condition {
        "rights" => Arc::new(ScopedPolicy {
          actions: vec![ManagementAction::Update],
          visibility: ManagementVisibility::all(),
        }),
        "hidden" => Arc::new(ScopedPolicy {
          actions: vec![ManagementAction::Publish],
          visibility: ManagementVisibility::none(),
        }),
        _ => Arc::new(TrustedNetworkManagementPolicy),
      };
      let handlers = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)));
      let mut command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 13);
      if condition == "source" {
        store
          .seed_issue_field_policy(
            issue.project_id,
            issue.id,
            IssueFieldPolicy {
              source_owned: [IssueField::Status].into_iter().collect(),
              ..Default::default()
            },
          )
          .unwrap();
      }
      if condition == "missing" || condition == "run" {
        let mut p = command.accepted.provenance().clone();
        if condition == "missing" {
          p.work_id = octacity_server_factory::WorkEnvelopeId::generate();
        } else {
          p.run_id = octacity_server_factory::FactoryRunId::generate();
        }
        command.accepted = AcceptedIssueStatusAction::for_selection(command.accepted.selection().clone(), p).unwrap();
      }
      let before = store.mutation_evidence_counts().await;
      assert!(
        handlers.handle_command(&context(), command).await.is_err(),
        "{condition}"
      );
      assert_eq!(store.mutation_evidence_counts().await, before, "{condition}");
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    }
  });
}

#[test]
fn issue_pipeline_action_qualifies_exact_retained_evidence_without_inference_from_completion_or_payload() {
  run_ready(async {
    use octacity_server_factory::FlowDiscoveryKind;
    for (failed, qualified, allowed) in [(false, true, true), (true, true, false), (false, false, false)] {
      let (store, issue, association, factory) = linked_with_evidence(vec![
        IssueTransitionEvidence::AcceptedExecution,
        IssueTransitionEvidence::PassedRequiredChecks,
      ])
      .await;
      let work = factory.factory_run_snapshot(association.run_id).await.unwrap().work;
      let snapshot =
        crate::factory_evaluation_action_tests::issue_action_records(work, association.run_id, failed, qualified).await;
      // Qualified completion, including an accepted Decision, does not infer a business move.
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
      let refs = snapshot
        .flow
        .data
        .records
        .iter()
        .filter(|r| {
          r.discovery()
            .is_some_and(|f| matches!(f.kind(), FlowDiscoveryKind::Evidence | FlowDiscoveryKind::Decision))
        })
        .map(|r| r.digest().unwrap())
        .collect::<Vec<_>>();
      let candidate = snapshot
        .flow
        .data
        .records
        .iter()
        .find_map(|r| r.discovery().and_then(|f| f.candidate_subject()).cloned())
        .unwrap();
      let mut command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 14);
      command.moved_at = at(100);
      let mut provenance = command.accepted.provenance().clone();
      provenance.candidate = Some(candidate);
      command.accepted = AcceptedIssueStatusAction::for_selection(command.accepted.selection().clone(), provenance)
        .unwrap()
        .with_evidence(&snapshot, &refs)
        .unwrap();
      let handlers = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(IssueHandlers::new(
          store.clone(),
          Arc::new(TrustedNetworkManagementPolicy),
        )),
      );
      let result = handlers.handle_command(&context(), command).await;
      assert_eq!(result.is_ok(), allowed, "failed={failed}, qualified={qualified}");
      if !allowed {
        assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
      }
    }
  });
}

#[test]
fn issue_pipeline_action_workflow_adoption_away_and_back_invalidates_old_execution() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let old = store
      .issue_workflow(issue.project_id, issue.workflow_id, Some(issue.workflow_version))
      .await
      .unwrap();
    let mut definition = old.definition.clone();
    definition.name = "Renamed organizational policy".into();
    let target = store
      .publish_issue_workflow(testing::management_mutation(PublishIssueWorkflow {
        intent: PublishIssueWorkflowIntent {
          project_id: issue.project_id,
          id: issue.workflow_id,
          expected_current_version: Some(issue.workflow_version),
          definition,
        },
        idempotency_key: key("new-workflow"),
        published_at: at(25),
      }))
      .await
      .unwrap()
      .workflow;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let mut current = issue.clone();
    for workflow in [&target, &old] {
      current = handlers
        .handle_command(
          &context(),
          AdoptIssueWorkflowCommand {
            intent: AdoptIssueWorkflowIntent {
              project_id: issue.project_id,
              id: issue.id,
              expected_version: current.version,
              expected_workflow_id: current.workflow_id,
              expected_workflow_version: current.workflow_version,
              expected_status_id: current.status_id,
              target_workflow_id: workflow.id,
              target_workflow_version: workflow.version,
              status_correspondence: workflow.definition.statuses.iter().map(|s| (s.id, s.id)).collect(),
              expected_field_policy: Default::default(),
            },
            idempotency_key: key(&format!("adopt-{}", workflow.version.get())),
            adopted_at: at(30 + current.version.get() as i64),
          },
        )
        .await
        .unwrap()
        .issue;
    }
    assert_eq!(current.workflow_version, issue.workflow_version);
    assert_eq!(current.status_id, issue.status_id);
    let mut command = action(&current, &association, "20000000-0000-4000-8000-000000000001", 20);
    command.moved_at = at(60);
    assert!(handlers.handle_command(&context(), command).await.is_err());
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), current);
  });
}

#[test]
fn issue_pipeline_action_replay_checks_current_authority_and_exact_action_provenance() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 21);
    let allowed = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let moved = allowed.handle_command(&context(), command.clone()).await.unwrap();
    for field in ["flow", "cycle", "attempt"] {
      let mut changed = command.clone();
      let mut p = command.accepted.provenance().clone();
      match field {
        "flow" => p.flow_run_id = FlowRunId::generate(),
        "cycle" => p.cycle_id = WorkflowCycleId::generate(),
        _ => p.node_attempt_id = NodeAttemptId::generate(),
      }
      changed.accepted = AcceptedIssueStatusAction::for_selection(command.accepted.selection().clone(), p).unwrap();
      assert!(allowed.handle_command(&context(), changed).await.is_err());
    }
    let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::Update],
      visibility: ManagementVisibility::all(),
    });
    let revoked = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)));
    assert!(revoked.handle_command(&context(), command).await.is_err());
    assert_eq!(
      store.issue(issue.project_id, issue.id, None).await.unwrap(),
      moved.issue
    );
  });
}

#[test]
fn issue_pipeline_action_evidence_rejects_foreign_run_candidate_unselected_and_expired_records() {
  run_ready(async {
    use octacity_server_factory::{CandidateSubject, FlowDiscoveryKind};
    let (store, issue, association, factory) =
      linked_with_evidence(vec![IssueTransitionEvidence::PassedRequiredChecks]).await;
    let work = factory.factory_run_snapshot(association.run_id).await.unwrap().work;
    let snapshot =
      crate::factory_evaluation_action_tests::issue_action_records(work, association.run_id, false, true).await;
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.discovery().is_some_and(|f| f.kind() == FlowDiscoveryKind::Evidence))
      .unwrap();
    let candidate = record.discovery().unwrap().candidate_subject().unwrap().clone();
    let command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 22);
    for condition in ["foreign-run", "candidate", "unselected"] {
      let mut p = command.accepted.provenance().clone();
      p.candidate = Some(candidate.clone());
      if condition == "foreign-run" {
        p.run_id = octacity_server_factory::FactoryRunId::generate();
      }
      if condition == "candidate" {
        p.candidate = Some(CandidateSubject::new(
          candidate.exact().clone(),
          "different-head".parse().unwrap(),
          FactoryDigest::from_bytes([99; 32]),
        ));
      }
      let refs = if condition == "unselected" {
        vec![FactoryDigest::from_bytes([98; 32])]
      } else {
        vec![record.digest().unwrap()]
      };
      assert!(
        AcceptedIssueStatusAction::for_selection(command.accepted.selection().clone(), p)
          .unwrap()
          .with_evidence(&snapshot, &refs)
          .is_err(),
        "{condition}"
      );
    }
    let mut expired = command;
    let mut p = expired.accepted.provenance().clone();
    p.candidate = Some(candidate);
    expired.accepted = AcceptedIssueStatusAction::for_selection(expired.accepted.selection().clone(), p)
      .unwrap()
      .with_evidence(&snapshot, &[record.digest().unwrap()])
      .unwrap();
    expired.moved_at = at(5001);
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    assert!(handlers.handle_command(&context(), expired).await.is_err());
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

#[test]
fn issue_pipeline_action_atomic_failure_retains_original_command_and_recovers_once() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 23);
    let before = store.mutation_evidence_counts().await;
    store
      .fail_next_issue_mutation_at(testing::MutationFailurePoint::Commit)
      .unwrap();
    assert!(handlers.handle_command(&context(), command.clone()).await.is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    let mut changed = command.clone();
    changed.intent.reason = Some("changed after failure".into());
    assert!(handlers.handle_command(&context(), changed).await.is_err());
    let first = handlers.handle_command(&context(), command.clone()).await.unwrap();
    let replay = handlers.handle_command(&context(), command).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(first.transition, replay.transition);
    assert_eq!(
      store
        .issue_status_history(issue.project_id, issue.id, None, 20)
        .await
        .unwrap()
        .items
        .len(),
      2
    );
  });
}

#[test]
fn issue_pipeline_action_pending_retry_preserves_original_move_time() {
  run_ready(async {
    let (store, issue, association, _factory) = linked().await;
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let original = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 25);
    store
      .fail_next_issue_mutation_at(testing::MutationFailurePoint::Commit)
      .unwrap();
    assert!(handlers.handle_command(&context(), original.clone()).await.is_err());
    let mut retry = original.clone();
    retry.moved_at = at(100);
    let outcome = handlers.handle_command(&context(), retry).await.unwrap();
    assert_eq!(outcome.transition.occurred_at, original.moved_at);
    let retained = store
      .issue_status_command(
        issue.project_id,
        issue.id,
        &octacity_server_store::ManagementSecurityScope::trusted_network(),
        original.accepted.provenance().operation_id,
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(retained.moved_at, original.moved_at);
  });
}

#[test]
fn issue_pipeline_action_restore_rejects_candidate_metadata_without_selected_evidence() {
  run_ready(async {
    let (store, issue, association, factory) = linked().await;
    let snapshot = factory.factory_run_snapshot(association.run_id).await.unwrap();
    let command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 26);
    let mut retained = TransitionIssue::pipeline(command.intent, command.accepted, command.moved_at)
      .retained_pipeline_command()
      .unwrap();
    retained.provenance.candidate = Some(octacity_server_factory::CandidateSubject::new(
      snapshot.work.subject().repository().unwrap().clone(),
      octacity_server_domain::ImmutableRevision::new("unverified-candidate").unwrap(),
      FactoryDigest::from_bytes([77; 32]),
    ));
    assert!(
      retained.restore(&snapshot).is_err(),
      "unqualified candidate metadata is not accepted provenance"
    );
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

#[test]
fn issue_pipeline_action_legacy_artifact_association_has_no_invented_requirements_authority() {
  run_ready(async {
    let (store, issue, association, factory) = linked().await;
    let work = factory.factory_run_snapshot(association.run_id).await.unwrap().work;
    let command = crate::factory_admission_tests::command(
      issue.project_id,
      work.configuration().id(),
      work.subject().repository().unwrap().repository_id(),
    );
    let identity = command.external_identity.clone();
    AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryAdmissionHandlers::new(
        factory.clone(),
        Arc::new(crate::factory_admission_tests::RecordingResolver::default()),
      )),
    )
    .handle_command(&context(), command)
    .await
    .unwrap();
    let admission = factory
      .recover_factory_admission(&FactoryAdmissionIdentity {
        source_scope: FactoryWorkSourceScope {
          source: FactoryKey::new("manual").unwrap(),
          security_scope: context().security_scope().to_store(),
        },
        external_identity: identity,
      })
      .await
      .unwrap()
      .unwrap();
    let legacy = store
      .associate_issue_work(testing::management_mutation(AssociateIssueWork {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: None,
        admission,
      }))
      .await
      .unwrap()
      .association;
    assert!(legacy.requirements_version.is_none());
    let mut command = action(&issue, &association, "20000000-0000-4000-8000-000000000001", 24);
    let mut p = command.accepted.provenance().clone();
    p.work_id = legacy.work_id;
    p.run_id = legacy.run_id;
    command.accepted = AcceptedIssueStatusAction::for_selection(command.accepted.selection().clone(), p).unwrap();
    let handlers = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    assert!(handlers.handle_command(&context(), command).await.is_err());
    assert_eq!(
      store
        .issue_work_association(issue.project_id, legacy.work_id)
        .await
        .unwrap(),
      Some(legacy)
    );
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

#[test]
fn issue_pipeline_push_review_rejects_a_mapping_installed_after_the_original_review() {
  run_ready(async {
    let (store, issue, association, _) = linked().await;
    let edge = store
      .default_issue_workflow(issue.project_id)
      .await
      .unwrap()
      .workflow
      .definition
      .transitions[0]
      .id;
    let mut command = action(&issue, &association, &edge.to_string(), 91);
    command.accepted = command.accepted.with_push_mapping_review(None);
    store
      .seed_issue_field_policy(
        issue.project_id,
        issue.id,
        IssueFieldPolicy {
          push: Some(IssuePushPolicy {
            mapping: octacity_server_factory::ImmutableReference::new(
              "later-mapping".parse().unwrap(),
              "v1".parse().unwrap(),
              FactoryDigest::from_bytes([1; 32]),
            ),
            fields: [IssueField::Status].into(),
          }),
          ..Default::default()
        },
      )
      .unwrap();
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    assert!(commands.handle_command(&context(), command).await.is_err());
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
  });
}

#[test]
fn issue_legacy_pipeline_command_cannot_gain_push_from_a_later_source_mapping() {
  run_ready(async {
    for sealed in [false, true] {
      let (store, issue, association, _) = linked().await;
      let workflow = store.default_issue_workflow(issue.project_id).await.unwrap().workflow;
      let edge = &workflow.definition.transitions[0];
      let command = action(&issue, &association, &edge.id.to_string(), 9);
      assert!(command.accepted.push_mapping_review().is_none());
      let commands = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(IssueHandlers::new(
          store.clone(),
          Arc::new(TrustedNetworkManagementPolicy),
        )),
      );
      if sealed {
        store
          .fail_next_issue_mutation_at(testing::MutationFailurePoint::Commit)
          .unwrap();
        assert!(commands.handle_command(&context(), command.clone()).await.is_err());
      }
      crate::factory_flow_action_tests::issue_push::install_push_mapping(
        &store,
        &issue,
        edge.to,
        if sealed { 40 } else { 32 },
        None,
        true,
      )
      .await;
      let moved = commands.handle_command(&context(), command.clone()).await.unwrap();
      assert_eq!(moved.issue.status_id, edge.to);
      assert!(
        store.retained_issue_push_intents().unwrap().is_empty(),
        "a legacy command cannot acquire a newly attached push producer"
      );
      assert_eq!(
        commands.handle_command(&context(), command).await.unwrap().issue,
        moved.issue
      );
    }
  });
}
