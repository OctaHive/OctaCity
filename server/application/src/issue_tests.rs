use crate::*;
use octacity_server_domain::{ProjectId, ProjectName, Timestamp};
use octacity_server_store::{
  CreateProject, IdempotencyKey, MutationDisposition, ProjectStore,
  testing::{InMemoryProjectStore, MutationEvidenceProbe, management_mutation},
};
use std::sync::Arc;

fn run_ready(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(future.as_mut().poll(&mut context).is_ready());
}
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}
async fn project(store: &InMemoryProjectStore) -> ProjectId {
  let id = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id,
      parent_id: None,
      name: ProjectName::new(format!("Tasks {id}")).unwrap(),
      idempotency_key: key(&id.to_string()),
      created_at: at(1),
    }))
    .await
    .unwrap();
  id
}
fn draft() -> IssueContent {
  IssueContent {
    title: "Разобраться с задачей".into(),
    description: String::new(),
    acceptance: None,
    priority: None,
  }
}
#[test]
fn issue_manual_draft_uses_default_without_execution_or_provider() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let workflow = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueWorkflowHandlers::new(store.clone())),
    )
    .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
    .await
    .unwrap()
    .workflow;
    let command = CreateIssueCommand {
      intent: CreateIssueIntent {
        project_id,
        content: draft(),
      },
      idempotency_key: key("first-issue"),
      created_at: at(10),
    };
    let created = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(created.disposition, MutationDisposition::Applied);
    assert_eq!(created.issue.display_key(), "ISS-1");
    assert_eq!(created.issue.content, draft());
    assert_eq!(created.issue.version.get(), 1);
    assert_eq!(created.issue.workflow_id, workflow.id);
    assert_eq!(created.issue.workflow_version, workflow.version);
    assert_eq!(created.issue.status_id, workflow.definition.initial_status_id);
    let before = store.mutation_evidence_counts().await;
    let read = queries
      .handle_query(
        &context(),
        GetIssueQuery {
          project_id,
          id: created.issue.id,
          version: None,
        },
      )
      .await
      .unwrap();
    assert_eq!(read, created.issue);
    let replay = commands.handle_command(&context(), command).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, created.issue);
    assert_eq!(store.mutation_evidence_counts().await, before);
    // This composition has no Factory, Work/Run store or provider port at all.
  });
}

fn create(project_id: ProjectId, identity: &str) -> CreateIssueCommand {
  CreateIssueCommand {
    intent: CreateIssueIntent {
      project_id,
      content: draft(),
    },
    idempotency_key: key(identity),
    created_at: at(10),
  }
}
#[test]
fn issue_edit_retains_revisions_and_original_replay_after_later_edits() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let created = commands
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    let mut content = draft();
    content.description = "Новые требования".into();
    content.acceptance = Some("Черновик критериев".into());
    content.priority = Some(5);
    let edit = EditIssueCommand {
      intent: EditIssueIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        content: content.clone(),
      },
      idempotency_key: key("edit"),
      edited_at: at(20),
    };
    let edited = commands.handle_command(&context(), edit.clone()).await.unwrap().issue;
    assert_eq!(edited.version.get(), 2);
    assert_eq!(edited.content, content);
    assert_eq!(
      (
        edited.number,
        edited.status_id,
        edited.workflow_id,
        edited.workflow_version
      ),
      (
        created.number,
        created.status_id,
        created.workflow_id,
        created.workflow_version
      )
    );
    let mut later = edit.clone();
    later.intent.expected_version = edited.version;
    later.intent.content.title = "Later title".into();
    later.idempotency_key = key("later");
    later.edited_at = at(30);
    let head = commands.handle_command(&context(), later).await.unwrap().issue;
    let before = store.mutation_evidence_counts().await;
    let replay = commands.handle_command(&context(), edit.clone()).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, edited);
    let mut stale = edit.clone();
    stale.idempotency_key = key("stale");
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
    let mut changed = edit;
    changed.intent.content.title = "Changed intent".into();
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
    for original in [created, edited, head] {
      assert_eq!(
        queries
          .handle_query(
            &context(),
            GetIssueQuery {
              project_id,
              id: original.id,
              version: Some(original.version)
            }
          )
          .await
          .unwrap(),
        original
      );
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_explicit_moves_share_exact_pinned_edges_and_replay_original_history() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let created = commands
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    let triage = TransitionIssueCommand {
      intent: TransitionIssueIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        workflow_id: created.workflow_id,
        workflow_version: created.workflow_version,
        expected_status_id: created.status_id,
        transition_id: "20000000-0000-4000-8000-000000000001".parse().unwrap(),
        reason: Some("Ready for assessment".into()),
      },
      idempotency_key: key("triage"),
      moved_at: at(20),
    };
    let moved = commands.handle_command(&context(), triage.clone()).await.unwrap();
    assert_eq!(moved.issue.version.get(), 2);
    assert_eq!(
      moved.issue.status_id.to_string(),
      "10000000-0000-4000-8000-000000000002"
    );
    assert_eq!(moved.issue.content, created.content);
    let mut decline = triage.clone();
    decline.intent.expected_version = moved.issue.version;
    decline.intent.expected_status_id = moved.issue.status_id;
    decline.intent.transition_id = "20000000-0000-4000-8000-000000000003".parse().unwrap();
    decline.intent.reason = None;
    decline.idempotency_key = key("decline");
    decline.moved_at = at(30);
    let declined = commands.handle_command(&context(), decline).await.unwrap();
    assert_eq!(
      declined.issue.status_id.to_string(),
      "10000000-0000-4000-8000-000000000007"
    );
    let before = store.mutation_evidence_counts().await;
    let replay = commands.handle_command(&context(), triage.clone()).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, moved.issue);
    assert_eq!(replay.transition, moved.transition);
    let mut changed = triage;
    changed.intent.reason = Some("Changed".into());
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
    let history = queries
      .handle_query(
        &context(),
        ListIssueStatusHistoryQuery::try_new(project_id, created.id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(history.items.len(), 3);
    assert_eq!(history.items[0].from, None);
    assert_eq!(history.items[1].from, Some(created.status_id));
    assert_eq!(history.items[1].to, moved.issue.status_id);
    assert_eq!(history.items[1].reason.as_deref(), Some("Ready for assessment"));
    assert_eq!(
      history.items[1].origin,
      octacity_server_store::IssueTransitionOrigin::Manual
    );
    assert_eq!(
      history.items[1].actor.kind,
      octacity_server_store::AuditActorKind::UnauthenticatedManagement
    );
    assert_eq!(history.items[2].to, declined.issue.status_id);
    let first = queries
      .handle_query(
        &context(),
        ListIssueStatusHistoryQuery::try_new(project_id, created.id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.items, history.items[..1]);
    assert_eq!(first.next_cursor, Some(created.version));
    let rest = queries
      .handle_query(
        &context(),
        ListIssueStatusHistoryQuery::try_new(project_id, created.id, first.next_cursor, 2).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(rest.items, history.items[1..]);
    assert_eq!(rest.next_cursor, None);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

struct ScopedPolicy {
  actions: Vec<ManagementAction>,
  visibility: ManagementVisibility,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for ScopedPolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if self.actions.contains(&action) {
      Ok(ManagementAuthorizationGrant::new(self.visibility.clone()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}
fn move_from(issue: &IssueRevision, identity: &str) -> TransitionIssueCommand {
  TransitionIssueCommand {
    intent: TransitionIssueIntent {
      project_id: issue.project_id,
      id: issue.id,
      expected_version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      expected_status_id: issue.status_id,
      transition_id: "20000000-0000-4000-8000-000000000001".parse().unwrap(),
      reason: None,
    },
    idempotency_key: key(identity),
    moved_at: at(40),
  }
}
#[test]
fn issue_hidden_reads_and_replays_are_rejected_before_content_or_mutation() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let created = commands
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    let moved = commands
      .handle_command(&context(), move_from(&created, "move"))
      .await
      .unwrap()
      .issue;
    let edit = EditIssueCommand {
      intent: EditIssueIntent {
        project_id,
        id: moved.id,
        expected_version: moved.version,
        content: IssueContent {
          title: "Private content".into(),
          ..draft()
        },
      },
      idempotency_key: key("edit"),
      edited_at: at(50),
    };
    commands.handle_command(&context(), edit.clone()).await.unwrap();
    let before = store.mutation_evidence_counts().await;
    for visibility in [
      ManagementVisibility::none(),
      ManagementVisibility::restricted([ManagementResource::instance(
        ManagementResourceKind::Issue,
        ManagementResourceIdentity::new(octacity_server_domain::IssueId::generate().to_string()).unwrap(),
      )
      .unwrap()])
      .unwrap(),
    ] {
      let policy = Arc::new(ScopedPolicy {
        actions: vec![
          ManagementAction::View,
          ManagementAction::Update,
          ManagementAction::Publish,
        ],
        visibility,
      });
      let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
      let scoped = AuthorizedCommandHandler::new(policy, handlers.clone());
      for version in [None, Some(created.version)] {
        let error = queries
          .handle_query(
            &context(),
            GetIssueQuery {
              project_id,
              id: created.id,
              version,
            },
          )
          .await
          .unwrap_err();
        assert_eq!(
          error.application().unwrap().classification(),
          ApplicationFailure::NotFound
        );
      }
      let error = scoped.handle_command(&context(), edit.clone()).await.unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::NotFound
      );
      let error = scoped
        .handle_command(&context(), move_from(&created, "move"))
        .await
        .unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::NotFound
      );
      let error = queries
        .handle_query(
          &context(),
          ListIssueStatusHistoryQuery::try_new(project_id, created.id, None, 10).unwrap(),
        )
        .await
        .unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::NotFound
      );
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_source_owned_fields_reject_local_replacement_but_allow_unrelated_edits() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let created = commands
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    store
      .seed_issue_field_policy(
        project_id,
        created.id,
        octacity_server_store::IssueFieldPolicy {
          status_mapping_workflow: None,
          source_owned: [
            octacity_server_store::IssueField::Description,
            octacity_server_store::IssueField::Status,
          ]
          .into_iter()
          .collect(),
          push: None,
        },
      )
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let edit = EditIssueCommand {
      intent: EditIssueIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        content: IssueContent {
          description: "Forbidden overwrite".into(),
          ..draft()
        },
      },
      idempotency_key: key("edit"),
      edited_at: at(20),
    };
    assert_eq!(
      commands
        .handle_command(&context(), edit.clone())
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(
      commands
        .handle_command(&context(), move_from(&created, "move"))
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let mut local = edit;
    local.intent.content = IssueContent {
      priority: Some(3),
      ..draft()
    };
    let edited = commands.handle_command(&context(), local).await.unwrap().issue;
    assert_eq!(edited.content.priority, Some(3));
    assert_eq!(edited.content.description, created.content.description);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: edited.id,
            version: None
          }
        )
        .await
        .unwrap(),
      edited
    );
  });
}

fn mapping(name: &str) -> octacity_server_factory::ImmutableReference {
  use octacity_server_factory::{FactoryDigest, FactoryKey, ImmutableReference};
  ImmutableReference::new(
    FactoryKey::new(name).unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([7; 32]),
  )
}
#[test]
fn issue_move_atomically_retains_exact_push_intent_without_provider_dispatch() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let created = commands
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
    let policy = |name| octacity_server_store::IssueFieldPolicy {
      status_mapping_workflow: None,
      source_owned: Default::default(),
      push: Some(octacity_server_store::IssuePushPolicy {
        mapping: mapping(name),
        fields: [octacity_server_store::IssueField::Status].into_iter().collect(),
      }),
    };
    store
      .seed_issue_field_policy(project_id, created.id, policy("opaque-mapping-one"))
      .unwrap();
    let command = move_from(&created, "move");
    let before = store.mutation_evidence_counts().await;
    let moved = commands.handle_command(&context(), command.clone()).await.unwrap();
    let intents = store.retained_issue_push_intents().unwrap();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].issue_id, created.id);
    assert_eq!(intents[0].project_id, project_id);
    assert_eq!(intents[0].version, moved.issue.version);
    assert_eq!(intents[0].mapping, mapping("opaque-mapping-one"));
    assert_eq!(
      intents[0].fields,
      [octacity_server_store::IssueField::Status].into_iter().collect()
    );
    let after = store.mutation_evidence_counts().await;
    assert_eq!(after.idempotency, before.idempotency + 1);
    assert_eq!(after.audit, before.audit + 1);
    assert_eq!(after.outbox, before.outbox + 1);
    store
      .seed_issue_field_policy(project_id, created.id, policy("opaque-mapping-two"))
      .unwrap();
    let replay = commands.handle_command(&context(), command).await.unwrap();
    assert_eq!(replay.transition, moved.transition);
    assert_eq!(store.retained_issue_push_intents().unwrap(), intents);
    assert_eq!(store.mutation_evidence_counts().await, after);
  });
}

#[test]
fn issue_mutations_rollback_revision_history_replay_audit_and_push_at_each_failure_boundary() {
  use octacity_server_store::testing::MutationFailurePoint;
  run_ready(async {
    for point in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ] {
      let store = Arc::new(InMemoryProjectStore::new());
      let project_id = project(&store).await;
      let handlers = Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      ));
      let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
      let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert_eq!(
        commands
          .handle_command(&context(), create(project_id, "create"))
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Unavailable
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
      let created = commands
        .handle_command(&context(), create(project_id, "create"))
        .await
        .unwrap()
        .issue;
      assert_eq!(created.display_key(), "ISS-1", "failed creation cannot consume a key");
      store
        .seed_issue_field_policy(
          project_id,
          created.id,
          octacity_server_store::IssueFieldPolicy {
            status_mapping_workflow: None,
            source_owned: Default::default(),
            push: Some(octacity_server_store::IssuePushPolicy {
              mapping: mapping("retained"),
              fields: [
                octacity_server_store::IssueField::Description,
                octacity_server_store::IssueField::Status,
              ]
              .into_iter()
              .collect(),
            }),
          },
        )
        .unwrap();
      let edit = EditIssueCommand {
        intent: EditIssueIntent {
          project_id,
          id: created.id,
          expected_version: created.version,
          content: IssueContent {
            description: "Changed requirements".into(),
            ..draft()
          },
        },
        idempotency_key: key("edit"),
        edited_at: at(20),
      };
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert_eq!(
        commands
          .handle_command(&context(), edit.clone())
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Unavailable
      );
      assert_eq!(
        queries
          .handle_query(
            &context(),
            GetIssueQuery {
              project_id,
              id: created.id,
              version: None
            }
          )
          .await
          .unwrap(),
        created
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert!(store.retained_issue_push_intents().unwrap().is_empty());
      let edited = commands.handle_command(&context(), edit).await.unwrap().issue;
      let retained = store.retained_issue_push_intents().unwrap();
      assert_eq!(retained.len(), 1);
      assert_eq!(retained[0].version, edited.version);
      let command = move_from(&edited, "move");
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert_eq!(
        commands
          .handle_command(&context(), command.clone())
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Unavailable
      );
      assert_eq!(
        queries
          .handle_query(
            &context(),
            GetIssueQuery {
              project_id,
              id: created.id,
              version: None
            }
          )
          .await
          .unwrap(),
        edited
      );
      let history = queries
        .handle_query(
          &context(),
          ListIssueStatusHistoryQuery::try_new(project_id, created.id, None, 10).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(history.items.len(), 1);
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert_eq!(store.retained_issue_push_intents().unwrap(), retained);
      let moved = commands.handle_command(&context(), command.clone()).await.unwrap();
      assert_eq!(moved.disposition, MutationDisposition::Applied);
      assert_eq!(store.retained_issue_push_intents().unwrap().len(), 2);
      assert_eq!(
        commands.handle_command(&context(), command).await.unwrap().disposition,
        MutationDisposition::Replayed
      );
    }
  });
}

#[test]
fn issue_debug_does_not_disclose_requirements_or_organizational_reason() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(store, Arc::new(TrustedNetworkManagementPolicy)));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let mut create = create(project_id, "private");
    create.intent.content.title = "private-title".into();
    create.intent.content.description = "private-description".into();
    create.intent.content.acceptance = Some("private-acceptance".into());
    let created = commands.handle_command(&context(), create.clone()).await.unwrap().issue;
    let mut command = move_from(&created, "move");
    command.intent.reason = Some("private-organizational-reason".into());
    let moved = commands.handle_command(&context(), command.clone()).await.unwrap();
    let debug = format!("{create:?} {created:?} {command:?} {moved:?}");
    for text in [
      "private-title",
      "private-description",
      "private-acceptance",
      "private-organizational-reason",
    ] {
      assert!(!debug.contains(text), "private body appeared in a generic diagnostic");
    }
  });
}

#[test]
fn issue_creation_requires_visible_project_even_for_original_replay() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let original = create(project_id, "create");
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone())
      .handle_command(&context(), original.clone())
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let commands = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Create],
        visibility: ManagementVisibility::none(),
      }),
      handlers.clone(),
    );
    for command in [original, create(project_id, "another")] {
      let error = commands.handle_command(&context(), command).await.unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::NotFound
      );
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    let commands = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Create],
        visibility: ManagementVisibility::restricted([ManagementResource::instance(
          ManagementResourceKind::Project,
          ManagementResourceIdentity::new(project_id.to_string()).unwrap(),
        )
        .unwrap()])
        .unwrap(),
      }),
      handlers,
    );
    let created = commands
      .handle_command(&context(), create(project_id, "visible-project"))
      .await
      .unwrap();
    assert_eq!(created.issue.display_key(), "ISS-2");
  });
}

#[test]
fn issue_edit_transition_and_execution_rights_are_independent() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let creator = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Create],
        visibility: ManagementVisibility::all(),
      }),
      handlers.clone(),
    );
    let created = creator
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    let edit = EditIssueCommand {
      intent: EditIssueIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        content: IssueContent {
          priority: Some(2),
          ..draft()
        },
      },
      idempotency_key: key("edit"),
      edited_at: at(20),
    };
    let editor = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Update],
        visibility: ManagementVisibility::all(),
      }),
      handlers.clone(),
    );
    let mover = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Publish],
        visibility: ManagementVisibility::all(),
      }),
      handlers.clone(),
    );
    let before = store.mutation_evidence_counts().await;
    assert!(matches!(
      mover.handle_command(&context(), edit.clone()).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    assert!(matches!(
      editor.handle_command(&context(), move_from(&created, "move")).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    assert_eq!(store.mutation_evidence_counts().await, before);
    let edited = editor.handle_command(&context(), edit.clone()).await.unwrap().issue;
    let moved = mover
      .handle_command(&context(), move_from(&edited, "move"))
      .await
      .unwrap();
    assert_eq!(moved.issue.version.get(), 3);
    let before = store.mutation_evidence_counts().await;
    // An editor cannot obtain the successful transition's original replay receipt either.
    assert!(matches!(
      editor.handle_command(&context(), move_from(&edited, "move")).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

async fn policy_definition(store: Arc<InMemoryProjectStore>, project_id: ProjectId) -> IssueWorkflowDefinition {
  AuthorizedQueryHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueWorkflowHandlers::new(store)),
  )
  .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
  .await
  .unwrap()
  .workflow
  .definition
}
async fn install_policy(
  store: Arc<InMemoryProjectStore>,
  project_id: ProjectId,
  definition: IssueWorkflowDefinition,
  identity: &str,
) -> PublishedIssueWorkflow {
  let handlers = Arc::new(IssueWorkflowHandlers::new(store));
  let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
  let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
  let selected = queries
    .handle_query(&context(), GetDefaultIssueWorkflowQuery { project_id })
    .await
    .unwrap();
  let workflow = commands
    .handle_command(
      &context(),
      PublishIssueWorkflowCommand {
        intent: PublishIssueWorkflowIntent {
          project_id,
          id: octacity_server_domain::IssueWorkflowId::generate(),
          expected_current_version: None,
          definition,
        },
        idempotency_key: key(identity),
        published_at: at(5),
      },
    )
    .await
    .unwrap()
    .workflow;
  commands
    .handle_command(
      &context(),
      SelectDefaultIssueWorkflowCommand {
        intent: SelectDefaultIssueWorkflowIntent {
          project_id,
          expected_version: selected.version,
          workflow_id: workflow.id,
          workflow_version: workflow.version,
        },
        idempotency_key: key(&format!("select-{identity}")),
      },
    )
    .await
    .unwrap();
  workflow
}
#[test]
fn issue_workflow_pins_and_guards_never_infer_names_paths_origins_or_evidence() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let original = commands
      .handle_command(&context(), create(project_id, "original"))
      .await
      .unwrap()
      .issue;
    let definition = policy_definition(store.clone(), project_id).await;
    let before = store.mutation_evidence_counts().await;
    for mutation in 0..5 {
      let mut command = move_from(&original, &format!("bad-{mutation}"));
      match mutation {
        0 => command.intent.expected_version = original.version.next().unwrap(),
        1 => command.intent.workflow_id = octacity_server_domain::IssueWorkflowId::generate(),
        2 => command.intent.workflow_version = original.workflow_version.next().unwrap(),
        3 => command.intent.expected_status_id = definition.statuses[1].id,
        _ => command.intent.transition_id = definition.transitions[1].id,
      }
      assert_eq!(
        commands
          .handle_command(&context(), command)
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Conflict
      );
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    for (index, evidence) in [
      vec![],
      vec![octacity_server_store::IssueTransitionEvidence::AcceptedExecution],
      vec![octacity_server_store::IssueTransitionEvidence::PassedRequiredChecks],
    ]
    .into_iter()
    .enumerate()
    {
      let mut custom = definition.clone();
      custom.statuses[0].name = "Done".into();
      custom.statuses.reverse();
      if evidence.is_empty() {
        custom.transitions[0].origins = vec![octacity_server_store::IssueTransitionOrigin::Source];
      }
      custom.transitions[0].evidence = evidence;
      let workflow = install_policy(store.clone(), project_id, custom, &format!("policy-{index}")).await;
      let created = commands
        .handle_command(&context(), create(project_id, &format!("draft-{index}")))
        .await
        .unwrap()
        .issue;
      assert_eq!(created.workflow_id, workflow.id);
      assert_eq!(created.status_id, definition.initial_status_id);
      let before = store.mutation_evidence_counts().await;
      assert_eq!(
        commands
          .handle_command(&context(), move_from(&created, "guarded-move"))
          .await
          .unwrap_err()
          .application()
          .unwrap()
          .classification(),
        ApplicationFailure::Conflict
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: original.id,
            version: None
          }
        )
        .await
        .unwrap(),
      original
    );
    // Even after a new default disables that edge, an old Issue follows its retained policy.
    assert!(
      commands
        .handle_command(&context(), move_from(&original, "retained-policy"))
        .await
        .is_ok()
    );
  });
}

#[test]
fn issue_text_bounds_reserved_setters_and_history_limits_fail_without_partial_writes() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let bounded = IssueContent {
      title: "я".repeat(64),
      description: "d".repeat(65_536),
      acceptance: Some("a".repeat(65_536)),
      priority: Some(u8::MAX),
    };
    let mut invalid = vec![];
    for title in [
      "".to_string(),
      " ".into(),
      " padded".into(),
      "line\nbreak".into(),
      "я".repeat(65),
    ] {
      invalid.push(IssueContent { title, ..draft() });
    }
    invalid.push(IssueContent {
      description: "d".repeat(65_537),
      ..draft()
    });
    invalid.push(IssueContent {
      acceptance: Some("a".repeat(65_537)),
      ..draft()
    });
    invalid.push(IssueContent {
      description: "bad\0text".into(),
      ..draft()
    });
    let before = store.mutation_evidence_counts().await;
    for (index, content) in invalid.into_iter().enumerate() {
      let mut command = create(project_id, &format!("invalid-{index}"));
      command.intent.content = content;
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
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    let mut command = create(project_id, "bounded");
    command.intent.content = bounded.clone();
    let created = commands.handle_command(&context(), command).await.unwrap().issue;
    assert_eq!(created.display_key(), "ISS-1");
    assert_eq!(created.content, bounded);
    for reserved in [
      "status_id",
      "workflow_id",
      "workflow_version",
      "actor",
      "source_owned",
      "run_id",
      "decision",
    ] {
      let mut json = serde_json::to_value(draft()).unwrap();
      json[reserved] = serde_json::json!("untrusted");
      assert!(serde_json::from_value::<IssueContent>(json).is_err());
      let mut json = serde_json::to_value(EditIssueIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        content: draft(),
      })
      .unwrap();
      json[reserved] = serde_json::json!("untrusted");
      assert!(serde_json::from_value::<EditIssueIntent>(json).is_err());
    }
    for reserved in [
      "origin",
      "evidence",
      "accepted_execution",
      "source_owned",
      "provider",
      "run_id",
    ] {
      let mut json = serde_json::to_value(move_from(&created, "move").intent).unwrap();
      json[reserved] = serde_json::json!(true);
      assert!(serde_json::from_value::<TransitionIssueIntent>(json).is_err());
    }
    for limit in [0, 201, u16::MAX] {
      assert!(ListIssueStatusHistoryQuery::try_new(project_id, created.id, None, limit).is_err());
    }
    for reason in ["".to_string(), " padded".into(), "bad\nreason".into(), "r".repeat(129)] {
      let mut command = move_from(&created, "invalid-reason");
      command.intent.reason = Some(reason);
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
    }
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: created.id,
            version: None
          }
        )
        .await
        .unwrap(),
      created
    );
  });
}

fn verified_context(actor: &str) -> ManagementRequestContext {
  ManagementRequestContext::new(
    ManagementActor::authenticated(actor).unwrap(),
    ManagementSecurityScope::new(format!("operator:{actor}")).unwrap(),
    ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .unwrap()
}
#[test]
fn issue_replay_is_security_scoped_and_history_retains_actual_actor_and_project() {
  use octacity_server_store::testing::ManagementAuditProbe;
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::Create,
        ManagementAction::View,
        ManagementAction::Update,
        ManagementAction::Publish,
      ],
      visibility: ManagementVisibility::all(),
    });
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let command = create(project_id, "same-key");
    let first = commands
      .handle_command(&verified_context("service-a"), command.clone())
      .await
      .unwrap()
      .issue;
    let second = commands
      .handle_command(&verified_context("service-b"), command.clone())
      .await
      .unwrap()
      .issue;
    assert_ne!(first.id, second.id);
    assert_eq!(
      (first.display_key(), second.display_key()),
      ("ISS-1".into(), "ISS-2".into())
    );
    assert_eq!(
      commands
        .handle_command(&verified_context("service-a"), command)
        .await
        .unwrap()
        .issue,
      first
    );
    let moved = commands
      .handle_command(&verified_context("service-b"), move_from(&first, "move"))
      .await
      .unwrap();
    assert_eq!(
      moved.transition.actor.kind,
      octacity_server_store::AuditActorKind::AuthenticatedManagement
    );
    assert_eq!(moved.transition.actor.identity.as_deref(), Some("service-b"));
    assert!(store.management_audit_facts().await.iter().any(|fact| fact.operation
      == octacity_server_store::StoreOperation::TransitionIssue
      && fact.actor_identity.as_deref() == Some("service-b")
      && fact.target_identity == first.id.to_string()));
    let foreign_project = project(&store).await;
    let independent = commands
      .handle_command(&verified_context("service-a"), create(foreign_project, "other-project"))
      .await
      .unwrap()
      .issue;
    assert_eq!(independent.display_key(), "ISS-1");
    let before = store.mutation_evidence_counts().await;
    let error = queries
      .handle_query(
        &verified_context("service-a"),
        GetIssueQuery {
          project_id: foreign_project,
          id: first.id,
          version: Some(first.version),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    let mut command = move_from(&moved.issue, "foreign");
    command.intent.project_id = foreign_project;
    assert_eq!(
      commands
        .handle_command(&verified_context("service-a"), command)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::NotFound
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_outbox_identity_cannot_collide_when_scope_and_key_contain_separators() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Create],
        visibility: ManagementVisibility::all(),
      }),
      handlers,
    );
    let before = store.mutation_evidence_counts().await;
    let one = commands
      .handle_command(&verified_context("a"), create(project_id, "b:c"))
      .await
      .unwrap();
    let two = commands
      .handle_command(&verified_context("a:b"), create(project_id, "c"))
      .await
      .unwrap();
    assert_ne!(one.issue.id, two.issue.id);
    let after = store.mutation_evidence_counts().await;
    assert_eq!(after.idempotency, before.idempotency + 2);
    assert_eq!(after.audit, before.audit + 2);
    assert_eq!(after.outbox, before.outbox + 2);
  });
}

#[test]
fn issue_workflow_adoption_is_reviewed_replayable_and_invalidates_old_applicability() {
  run_ready(async {
    use octacity_server_store::{
      AdoptIssueWorkflowIntent, IssueWorkflowStatus, IssueWorkflowStore, PublishIssueWorkflow,
      PublishIssueWorkflowIntent,
    };
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let created = commands
      .handle_command(&context(), create(project_id, "draft"))
      .await
      .unwrap()
      .issue;
    let initial = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let target_status = octacity_server_domain::IssueStatusId::generate();
    let mut definition = initial.definition.clone();
    definition.statuses = vec![IssueWorkflowStatus {
      id: target_status,
      name: "Наш статус".into(),
    }];
    definition.initial_status_id = target_status;
    definition.terminal_status_ids.clear();
    definition.transitions.clear();
    let target = store
      .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
        intent: PublishIssueWorkflowIntent {
          project_id,
          id: initial.id,
          expected_current_version: Some(initial.version),
          definition,
        },
        idempotency_key: key("replacement"),
        published_at: at(11),
      }))
      .await
      .unwrap()
      .workflow;
    assert_eq!(store.issue(project_id, created.id, None).await.unwrap(), created);
    let command = AdoptIssueWorkflowCommand {
      intent: AdoptIssueWorkflowIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        expected_workflow_id: created.workflow_id,
        expected_workflow_version: created.workflow_version,
        expected_status_id: created.status_id,
        target_workflow_id: target.id,
        target_workflow_version: target.version,
        status_correspondence: [(created.status_id, target_status)].into_iter().collect(),
        expected_field_policy: Default::default(),
      },
      idempotency_key: key("adopt"),
      adopted_at: at(12),
    };
    let adopted = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(adopted.issue.workflow_version, target.version);
    assert_eq!(adopted.issue.status_id, target_status);
    assert_eq!(adopted.issue.version.get(), 2);
    assert_eq!(adopted.issue.applicability_version, adopted.issue.version);
    assert_ne!(adopted.issue.applicability_version, created.applicability_version);
    assert_eq!(adopted.issue.content, created.content);
    assert_eq!(adopted.transition.from, Some(created.status_id));
    assert_eq!(
      adopted.transition.previous_workflow.as_ref().unwrap().version,
      initial.version
    );
    let edited = commands
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id,
            id: created.id,
            expected_version: adopted.issue.version,
            content: IssueContent {
              priority: Some(2),
              ..adopted.issue.content.clone()
            },
          },
          idempotency_key: key("priority"),
          edited_at: at(13),
        },
      )
      .await
      .unwrap();
    assert_eq!(edited.issue.applicability_version, adopted.issue.applicability_version);
    let replay = commands.handle_command(&context(), command).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, adopted.issue);
    assert_eq!(replay.transition, adopted.transition);
    assert_eq!(
      store
        .issue(project_id, created.id, Some(created.version))
        .await
        .unwrap(),
      created
    );
    assert_eq!(
      store
        .issue_workflow(project_id, initial.id, Some(initial.version))
        .await
        .unwrap(),
      initial
    );
  });
}

#[test]
fn issue_workflow_adoption_requires_separate_authority_and_current_visibility() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let handlers = Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let created = commands
      .handle_command(&context(), create(project_id, "create"))
      .await
      .unwrap()
      .issue;
    let command = AdoptIssueWorkflowCommand {
      intent: octacity_server_store::AdoptIssueWorkflowIntent {
        project_id,
        id: created.id,
        expected_version: created.version,
        expected_workflow_id: created.workflow_id,
        expected_workflow_version: created.workflow_version,
        expected_status_id: created.status_id,
        target_workflow_id: created.workflow_id,
        target_workflow_version: created.workflow_version,
        status_correspondence: [(created.status_id, created.status_id)].into_iter().collect(),
        expected_field_policy: Default::default(),
      },
      idempotency_key: key("adopt"),
      adopted_at: at(20),
    };
    let before = store.mutation_evidence_counts().await;
    let editor = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Update, ManagementAction::Publish],
        visibility: ManagementVisibility::all(),
      }),
      handlers.clone(),
    );
    assert!(
      editor
        .handle_command(&context(), command.clone())
        .await
        .unwrap_err()
        .application()
        .is_none()
    );
    let hidden = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Administer],
        visibility: ManagementVisibility::none(),
      }),
      handlers,
    );
    assert_eq!(
      hidden
        .handle_command(&context(), command)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::NotFound
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
