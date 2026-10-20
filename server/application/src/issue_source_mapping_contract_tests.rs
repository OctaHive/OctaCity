//! Failure-condition characterization protects the shared planner and publication seam.
use super::*;
use octacity_server_store::testing::MutationFailurePoint;
use octacity_server_store::*;

#[test]
fn issue_source_mapping_conditions_preserve_cards_without_inference_or_guard_bypass() {
  run_ready(async {
    use IssueSourceStatusCondition::*;
    for (condition, reads) in [
      (Unmapped, vec![32, 33]),
      (Ambiguous, vec![32]),
      (ForbiddenTransition, vec![32]),
      (EvidenceRequired, vec![32]),
    ] {
      let store = Arc::new(InMemoryProjectStore::new());
      let project_id = project(&store).await;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let default = store.default_issue_workflow(project_id).await.unwrap();
      let mut definition = default.workflow.definition;
      for (index, status) in definition.statuses.iter_mut().enumerate() {
        status.name = format!("Business choice {index}");
      }
      if condition == EvidenceRequired {
        definition.transitions[0].evidence = vec![IssueTransitionEvidence::AcceptedExecution];
      }
      let workflow = store
        .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
          intent: PublishIssueWorkflowIntent {
            project_id,
            id: octacity_server_domain::IssueWorkflowId::generate(),
            expected_current_version: None,
            definition,
          },
          idempotency_key: key("custom-native-independent-workflow"),
          published_at: at(2),
        }))
        .await
        .unwrap()
        .workflow;
      store
        .select_default_issue_workflow(management_mutation(SelectDefaultIssueWorkflow {
          intent: SelectDefaultIssueWorkflowIntent {
            project_id,
            expected_version: default.version,
            workflow_id: workflow.id,
            workflow_version: workflow.version,
          },
          idempotency_key: key("select-custom-workflow"),
        }))
        .await
        .unwrap();
      let commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(
          source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
            .with_source_mappings(vec![installed(project_id)])
            .unwrap(),
        ),
      );
      let link = linked(
        commands
          .handle_command(&context(), import(selection(project_id), "condition-link"))
          .await
          .unwrap(),
      );
      let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
      let mut mapping = mapping_command(&issue, "explicit-native-condition");
      let targets = match condition {
        Unmapped => vec![],
        Ambiguous => vec![
          workflow.definition.transitions[0].to,
          workflow.definition.transitions[1].to,
        ],
        ForbiddenTransition => vec![workflow.definition.transitions[1].to],
        EvidenceRequired => vec![workflow.definition.transitions[0].to],
        _ => unreachable!(),
      };
      mapping.intent.definition.status.as_mut().unwrap().pull = targets
        .into_iter()
        .map(|status_id| IssueSourceStatusRule {
          native: octacity_server_factory::WorkSourcePredicate {
            state: Some("opaque-state".into()),
            all_of: vec![],
            any_of: vec![],
            none_of: vec![],
          },
          status_id,
        })
        .collect();
      commands.handle_command(&context(), mapping).await.unwrap();
      let manual = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
      );
      assert!(
        manual
          .handle_command(
            &context(),
            TransitionIssueCommand {
              intent: TransitionIssueIntent {
                project_id,
                id: issue.id,
                expected_version: issue.version,
                workflow_id: issue.workflow_id,
                workflow_version: issue.workflow_version,
                expected_status_id: issue.status_id,
                transition_id: workflow.definition.transitions[0].id,
                reason: None,
              },
              idempotency_key: key("deny-manual-source-owned-status"),
              moved_at: at(32),
            }
          )
          .await
          .is_err()
      );
      for observed_at in reads {
        let reader = Reader {
          state: (condition == Unmapped).then(|| "closed".into()),
          observed_at,
          ..Default::default()
        };
        let reads = AuthorizedCommandHandler::new(
          policy.clone(),
          Arc::new(source_handlers(store.clone(), Arc::new(reader), policy.clone())),
        );
        let result = reads
          .handle_command(
            &context(),
            RefreshIssueSourceCommand {
              project_id,
              issue_id: issue.id,
              read: read(&selection(project_id)),
              idempotency_key: key(&format!("condition-read-{observed_at}")),
              refreshed_at: at(34),
            },
          )
          .await
          .unwrap();
        assert_eq!(result.status.unwrap().condition, condition);
      }
      assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
      assert_eq!(
        store
          .issue_status_history(project_id, issue.id, None, 10)
          .await
          .unwrap()
          .items
          .len(),
        1
      );
      assert!(store.retained_issue_push_intents().unwrap().is_empty());
    }
  });
}

#[test]
fn issue_source_mapping_publication_rejects_unsupported_fields_values_workflows_and_scopes() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![installed(project_id)])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "invalid-mapping-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    for case in [
      "workflow",
      "status",
      "state",
      "label",
      "field",
      "authority",
      "writer",
      "partition",
    ] {
      let mut command = mapping_command(&issue, case);
      let status = command.intent.definition.status.as_mut().unwrap();
      match case {
        "workflow" => status.workflow.version = octacity_server_domain::IssueWorkflowVersion::new(99).unwrap(),
        "status" => status.pull.push(IssueSourceStatusRule {
          native: octacity_server_factory::WorkSourcePredicate {
            state: None,
            all_of: vec![],
            any_of: vec![],
            none_of: vec![],
          },
          status_id: octacity_server_domain::IssueStatusId::generate(),
        }),
        "state" | "label" => status.pull.push(IssueSourceStatusRule {
          native: octacity_server_factory::WorkSourcePredicate {
            state: (case == "state").then(|| "not-advertised".into()),
            all_of: if case == "label" {
              vec!["not-advertised".into()]
            } else {
              vec![]
            },
            any_of: vec![],
            none_of: vec![],
          },
          status_id: issue.status_id,
        }),
        "field" => {
          command
            .intent
            .definition
            .fields
            .insert(IssueField::Acceptance, IssueSourceFieldAuthority::SourceOwnedPull);
        }
        "authority" => {
          command
            .intent
            .definition
            .fields
            .insert(IssueField::Status, IssueSourceFieldAuthority::Unsynchronized);
        }
        "writer" => {
          command.intent.definition.reporter = Some(selection(project_id).source);
          command.intent.definition.push_enabled = true;
        }
        "partition" => {}
        _ => unreachable!(),
      }
      let before = store.mutation_evidence_counts().await;
      let ctx = if case == "partition" {
        verified_context("foreign-mapping-partition")
      } else {
        context()
      };
      assert!(commands.handle_command(&ctx, command).await.is_err(), "{case}");
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    let denied = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(
          store.clone(),
          Arc::new(Reader {
            denied: true,
            ..Default::default()
          }),
          policy,
        )
        .with_source_mappings(vec![installed(project_id)])
        .unwrap(),
      ),
    );
    assert!(
      denied
        .handle_command(&context(), mapping_command(&issue, "read-grant-revoked"))
        .await
        .is_err()
    );
    assert_eq!(
      store.issue_field_policy(project_id, issue.id).await.unwrap(),
      IssueFieldPolicy::default()
    );
  });
}

#[test]
fn issue_source_mapping_publication_uses_atomic_ownership_epoch_and_replay_publication() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy)
          .with_source_mappings(vec![installed(project_id)])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "atomic-mapping-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let command = mapping_command(&issue, "atomic-mapping-review");
    let scope = context().security_scope().to_store();
    let control = store
      .issue_source_refresh_state(project_id, issue.id, &scope)
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    for point in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ] {
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(commands.handle_command(&context(), command.clone()).await.is_err());
      assert_eq!(
        store
          .issue_source_mapping(project_id, issue.id, &scope, None)
          .await
          .unwrap(),
        None
      );
      assert_eq!(
        store.issue_field_policy(project_id, issue.id).await.unwrap(),
        IssueFieldPolicy::default()
      );
      assert_eq!(
        store
          .issue_source_refresh_state(project_id, issue.id, &scope)
          .await
          .unwrap(),
        control
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    assert_eq!(
      commands
        .handle_command(&context(), command)
        .await
        .unwrap()
        .mapping
        .version
        .get(),
      1
    );
  });
}
