use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{IssueStatusId, IssueWorkflowId, ProjectId, ProjectName, Timestamp};
use octacity_server_store::{
  CreateProject, IdempotencyKey, IssueTransitionEvidence, IssueTransitionOrigin, IssueWorkflowDefinition,
  MutationDisposition, ProjectStore,
  testing::{InMemoryProjectStore, MutationEvidenceProbe, management_mutation},
};

use crate::*;

fn run_ready(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(future.as_mut().poll(&mut context).is_ready());
}

#[test]
fn issue_workflow_initial_and_terminal_choices_are_workflow_settings() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let before = store.mutation_evidence_counts().await;
    let queries = AuthorizedQueryHandler::new(Arc::new(ReadOnly), Arc::new(IssueWorkflowHandlers::new(store.clone())));
    let workflow = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap()
      .workflow;
    let definition = serde_json::to_value(workflow.definition).unwrap();
    assert_eq!(
      definition.get("initial_status_id"),
      Some(&serde_json::json!("10000000-0000-4000-8000-000000000001"))
    );
    assert_eq!(
      definition.get("terminal_status_ids"),
      Some(&serde_json::json!([
        "10000000-0000-4000-8000-000000000006",
        "10000000-0000-4000-8000-000000000007"
      ]))
    );
    for status in definition["statuses"].as_array().unwrap() {
      assert_eq!(
        status
          .as_object()
          .unwrap()
          .keys()
          .map(String::as_str)
          .collect::<Vec<_>>(),
        ["id", "name"]
      );
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_workflow_settings_validate_references_sets_and_reject_legacy_status_flags() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueWorkflowHandlers::new(store.clone()));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let original = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let mut definition = original.workflow.definition.clone();
    for terminal_status_ids in [vec![], vec![definition.initial_status_id]] {
      definition.terminal_status_ids = terminal_status_ids;
      assert_eq!(
        queries
          .handle_query(
            &context(),
            ValidateIssueWorkflowQuery {
              project_id,
              definition: definition.clone(),
            }
          )
          .await
          .unwrap(),
        definition
      );
    }
    for (index, terminal_status_ids) in [
      vec![IssueStatusId::generate()],
      vec![definition.initial_status_id; 2],
      vec![definition.initial_status_id; 65],
    ]
    .into_iter()
    .enumerate()
    {
      definition.terminal_status_ids = terminal_status_ids;
      let validation = queries
        .handle_query(
          &context(),
          ValidateIssueWorkflowQuery {
            project_id,
            definition: definition.clone(),
          },
        )
        .await;
      assert!(validation.is_err(), "invalid terminal selection was accepted");
      assert_eq!(
        validation.unwrap_err().application().unwrap().classification(),
        ApplicationFailure::Invalid
      );
      let error = commands
        .handle_command(
          &context(),
          PublishIssueWorkflowCommand {
            intent: PublishIssueWorkflowIntent {
              project_id,
              id: original.workflow.id,
              expected_current_version: Some(original.workflow.version),
              definition: definition.clone(),
            },
            idempotency_key: IdempotencyKey::new(format!("invalid-workflow-settings-{index}")).unwrap(),
            published_at: Timestamp::from_unix_millis(20).unwrap(),
          },
        )
        .await
        .unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::Invalid
      );
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      queries
        .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
        .await
        .unwrap(),
      original
    );
    for field in ["initial_status_id", "terminal_status_ids"] {
      let mut missing = serde_json::to_value(&original.workflow.definition).unwrap();
      missing.as_object_mut().unwrap().remove(field);
      assert!(serde_json::from_value::<IssueWorkflowDefinition>(missing).is_err());
    }
    for flag in ["initial", "terminal"] {
      let mut legacy = serde_json::to_value(&original.workflow.definition).unwrap();
      legacy["statuses"][0][flag] = serde_json::json!(true);
      assert!(serde_json::from_value::<IssueWorkflowDefinition>(legacy).is_err());
    }
  });
}

#[test]
fn issue_workflow_validation_bounds_business_policy_without_interpreting_names_or_scripts() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueWorkflowHandlers::new(store));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let mut definition = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap()
      .workflow
      .definition;
    let initial_id = definition.initial_status_id;
    definition.name = "Наши правила".into();
    definition.statuses[0].name = "Anything, including Done".into();
    definition.statuses.reverse();
    definition.transitions[0].origins = vec![IssueTransitionOrigin::Manual];
    definition.transitions[0].evidence = vec![
      IssueTransitionEvidence::AcceptedExecution,
      IssueTransitionEvidence::PassedRequiredChecks,
    ];
    let validate = |definition| ValidateIssueWorkflowQuery { project_id, definition };
    let accepted = queries
      .handle_query(&context(), validate(definition.clone()))
      .await
      .unwrap();
    assert_eq!(accepted, definition);
    assert_eq!(accepted.initial_status_id, initial_id);

    let mut invalid = Vec::new();
    let mut candidate = definition.clone();
    candidate.statuses.clear();
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.statuses = (0..65)
      .map(|_| {
        let mut status = definition.statuses[0].clone();
        status.id = IssueStatusId::generate();
        status
      })
      .collect();
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions = vec![candidate.transitions[0].clone(); 257];
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.initial_status_id = IssueStatusId::generate();
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.statuses[0].id = candidate.statuses[1].id;
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions[0].to = IssueStatusId::generate();
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions[0].from = candidate.transitions[0].to;
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions.push(candidate.transitions[0].clone());
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions[0].origins.clear();
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions[0].origins = vec![IssueTransitionOrigin::Manual; 2];
    invalid.push(candidate);
    let mut candidate = definition.clone();
    candidate.transitions[0].evidence = vec![IssueTransitionEvidence::AcceptedExecution; 2];
    invalid.push(candidate);
    for name in ["".to_owned(), " padded ".into(), "bad\nname".into(), "я".repeat(65)] {
      let mut candidate = definition.clone();
      candidate.statuses[0].name = name;
      invalid.push(candidate);
    }
    for candidate in invalid {
      let error = queries.handle_query(&context(), validate(candidate)).await.unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::Invalid
      );
    }
    let mut unsupported = serde_json::to_value(&definition).unwrap();
    unsupported["transitions"][0]["evidence"] = serde_json::json!(["run_script"]);
    assert!(serde_json::from_value::<IssueWorkflowDefinition>(unsupported).is_err());
    let mut script = serde_json::to_value(&definition).unwrap();
    script["transitions"][0]["guard"] = serde_json::json!("always approve");
    assert!(serde_json::from_value::<IssueWorkflowDefinition>(script).is_err());
  });
}

fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}

async fn project(store: &InMemoryProjectStore) -> ProjectId {
  let id = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id,
      parent_id: None,
      name: ProjectName::new(format!("Manual tasks {id}")).unwrap(),
      idempotency_key: IdempotencyKey::new(id.to_string()).unwrap(),
      created_at: Timestamp::from_unix_millis(10).unwrap(),
    }))
    .await
    .unwrap();
  id
}

struct ReadOnly;
#[async_trait]
impl ManagementAuthorizationPolicy for ReadOnly {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::View {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}

#[test]
fn issue_workflow_publication_retains_versions_replays_original_intent_and_requires_administration() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueWorkflowHandlers::new(store));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let first = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap();
    let mut definition = first.workflow.definition.clone();
    definition.statuses.reverse();
    definition.statuses[0].name = "Отложено".into();
    definition.initial_status_id = definition.statuses[1].id;
    definition.terminal_status_ids = vec![definition.statuses[0].id];
    let command = PublishIssueWorkflowCommand {
      intent: PublishIssueWorkflowIntent {
        project_id,
        id: first.workflow.id,
        expected_current_version: Some(first.workflow.version),
        definition: definition.clone(),
      },
      idempotency_key: IdempotencyKey::new("publish-policy").unwrap(),
      published_at: Timestamp::from_unix_millis(20).unwrap(),
    };
    let read_only = AuthorizedCommandHandler::new(Arc::new(ReadOnly), handlers);
    assert!(matches!(
      read_only.handle_command(&context(), command.clone()).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    let published = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(published.disposition, MutationDisposition::Applied);
    assert_eq!(published.workflow.version.get(), 2);
    assert_eq!(published.workflow.definition, definition);
    assert_eq!(published.workflow.id, first.workflow.id);
    assert_eq!(
      queries
        .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
        .await
        .unwrap(),
      first
    );
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueWorkflowQuery {
            project_id,
            id: first.workflow.id,
            version: Some(first.workflow.version),
          }
        )
        .await
        .unwrap(),
      first.workflow
    );
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueWorkflowQuery {
            project_id,
            id: first.workflow.id,
            version: None,
          }
        )
        .await
        .unwrap(),
      published.workflow
    );
    let mut replay = command.clone();
    replay.published_at = Timestamp::from_unix_millis(30).unwrap();
    let receipt = commands.handle_command(&context(), replay).await.unwrap();
    assert_eq!(receipt.disposition, MutationDisposition::Replayed);
    assert_eq!(receipt.workflow, published.workflow);
    let mut stale = command.clone();
    stale.idempotency_key = IdempotencyKey::new("stale-publication").unwrap();
    assert_eq!(
      commands
        .handle_command(&context(), stale)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    let mut changed = command.clone();
    changed.intent.definition.name = "Different intent".into();
    assert_eq!(
      commands
        .handle_command(&context(), changed)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert!(matches!(
      read_only.handle_command(&context(), command).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
  });
}

#[test]
fn issue_workflow_default_selection_is_explicit_versioned_and_replays_its_original_target() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueWorkflowHandlers::new(store));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let original = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap();
    let custom = commands
      .handle_command(
        &context(),
        PublishIssueWorkflowCommand {
          intent: PublishIssueWorkflowIntent {
            project_id,
            id: IssueWorkflowId::generate(),
            expected_current_version: None,
            definition: {
              let mut d = original.workflow.definition.clone();
              d.name = "Our project policy".into();
              d
            },
          },
          idempotency_key: IdempotencyKey::new("custom-workflow").unwrap(),
          published_at: Timestamp::from_unix_millis(20).unwrap(),
        },
      )
      .await
      .unwrap()
      .workflow;
    assert_eq!(custom.version.get(), 1);
    let select = SelectDefaultIssueWorkflowCommand {
      intent: SelectDefaultIssueWorkflowIntent {
        project_id,
        expected_version: original.version,
        workflow_id: custom.id,
        workflow_version: custom.version,
      },
      idempotency_key: IdempotencyKey::new("select-default").unwrap(),
    };
    let read_only = AuthorizedCommandHandler::new(Arc::new(ReadOnly), handlers);
    assert!(matches!(
      read_only.handle_command(&context(), select.clone()).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    let selected = commands.handle_command(&context(), select.clone()).await.unwrap();
    assert_eq!(selected.disposition, MutationDisposition::Applied);
    assert_eq!(selected.default.version.get(), 2);
    assert_eq!(selected.default.workflow, custom);
    assert_eq!(
      queries
        .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
        .await
        .unwrap(),
      selected.default
    );
    let restore = SelectDefaultIssueWorkflowCommand {
      intent: SelectDefaultIssueWorkflowIntent {
        project_id,
        expected_version: selected.default.version,
        workflow_id: original.workflow.id,
        workflow_version: original.workflow.version,
      },
      idempotency_key: IdempotencyKey::new("restore-default").unwrap(),
    };
    commands.handle_command(&context(), restore).await.unwrap();
    let replay = commands.handle_command(&context(), select.clone()).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.default, selected.default);
    let mut stale = select.clone();
    stale.idempotency_key = IdempotencyKey::new("stale-default").unwrap();
    assert_eq!(
      commands
        .handle_command(&context(), stale)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    let mut changed = select;
    changed.intent.workflow_id = original.workflow.id;
    assert_eq!(
      commands
        .handle_command(&context(), changed)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(
      queries
        .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
        .await
        .unwrap()
        .workflow,
      original.workflow
    );
  });
}

#[test]
fn issue_workflow_rejects_invalid_publication_and_foreign_default_without_partial_writes() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let other_project = project(&store).await;
    let handlers = Arc::new(IssueWorkflowHandlers::new(store.clone()));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let before = store.mutation_evidence_counts().await;
    let original = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap();
    let foreign = queries
      .handle_query(
        &context(),
        GetDefaultIssueWorkflowQuery {
          project_id: other_project,
        },
      )
      .await
      .unwrap();
    assert_eq!(
      store.mutation_evidence_counts().await,
      before,
      "default reads perform no mutation"
    );
    let mut invalid = original.workflow.definition.clone();
    invalid.initial_status_id = IssueStatusId::generate();
    let command = PublishIssueWorkflowCommand {
      intent: PublishIssueWorkflowIntent {
        project_id,
        id: original.workflow.id,
        expected_current_version: Some(original.workflow.version),
        definition: invalid,
      },
      idempotency_key: IdempotencyKey::new("invalid-policy").unwrap(),
      published_at: Timestamp::from_unix_millis(20).unwrap(),
    };
    assert_eq!(
      commands
        .handle_command(&context(), command)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Invalid
    );
    let result = commands
      .handle_command(
        &context(),
        SelectDefaultIssueWorkflowCommand {
          intent: SelectDefaultIssueWorkflowIntent {
            project_id,
            expected_version: original.version,
            workflow_id: foreign.workflow.id,
            workflow_version: foreign.workflow.version,
          },
          idempotency_key: IdempotencyKey::new("foreign-default").unwrap(),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      result.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      queries
        .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
        .await
        .unwrap(),
      original
    );
    assert!(
      queries
        .handle_query(
          &context(),
          GetIssueWorkflowQuery {
            project_id,
            id: foreign.workflow.id,
            version: None,
          }
        )
        .await
        .is_err()
    );
  });
}

#[test]
fn issue_workflow_accepts_declared_limits_and_rejects_one_extra_status_or_transition() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let queries = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueWorkflowHandlers::new(store)),
    );
    let mut definition = queries
      .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
      .await
      .unwrap()
      .workflow
      .definition;
    let prototype_status = definition.statuses[0].clone();
    definition.statuses = (0..64)
      .map(|i| {
        let mut status = prototype_status.clone();
        status.id = IssueStatusId::generate();
        status.name = format!("Status {i}");
        status
      })
      .collect();
    definition.initial_status_id = definition.statuses[0].id;
    definition.terminal_status_ids = definition.statuses.iter().map(|status| status.id).collect();
    let prototype_edge = definition.transitions[0].clone();
    definition.transitions = (0..64)
      .flat_map(|from| (0..64).map(move |to| (from, to)))
      .filter(|(from, to)| from != to)
      .take(256)
      .map(|(from, to)| {
        let mut edge = prototype_edge.clone();
        edge.id = octacity_server_domain::IssueTransitionId::generate();
        edge.from = definition.statuses[from].id;
        edge.to = definition.statuses[to].id;
        edge
      })
      .collect();
    assert_eq!(
      queries
        .handle_query(
          &context(),
          ValidateIssueWorkflowQuery {
            project_id,
            definition: definition.clone()
          }
        )
        .await
        .unwrap(),
      definition
    );
    let mut extra_status = definition.clone();
    let mut status = prototype_status;
    status.id = IssueStatusId::generate();
    extra_status.statuses.push(status);
    let mut extra_transition = definition.clone();
    let mut edge = prototype_edge;
    edge.id = octacity_server_domain::IssueTransitionId::generate();
    edge.from = definition.statuses[63].id;
    edge.to = definition.statuses[0].id;
    extra_transition.transitions.push(edge);
    for definition in [extra_status, extra_transition] {
      assert_eq!(
        queries
          .handle_query(&context(), ValidateIssueWorkflowQuery { project_id, definition })
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Invalid
      );
    }
  });
}

#[test]
fn issue_workflow_default_is_declarative_stable_and_needs_no_administration_or_factory() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let queries = AuthorizedQueryHandler::new(Arc::new(ReadOnly), Arc::new(IssueWorkflowHandlers::new(store)));
    let query = GetDefaultIssueWorkflowQuery { project_id };
    let first = queries.handle_query(&context(), query.clone()).await.unwrap();
    let again = queries.handle_query(&context(), query).await.unwrap();
    assert_eq!(first, again);
    assert_eq!(first.workflow.project_id, project_id);
    assert_eq!(first.workflow.version.get(), 1);
    assert_eq!(
      first
        .workflow
        .definition
        .statuses
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>(),
      ["New", "Triage", "Ready", "In progress", "Review", "Done", "Declined"]
    );
    assert_eq!(
      first.workflow.definition.initial_status_id.to_string(),
      "10000000-0000-4000-8000-000000000001"
    );
    assert_eq!(
      first
        .workflow
        .definition
        .terminal_status_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>(),
      [
        "10000000-0000-4000-8000-000000000006",
        "10000000-0000-4000-8000-000000000007"
      ]
    );
    assert!(!first.workflow.definition.transitions.is_empty());
  });
}
