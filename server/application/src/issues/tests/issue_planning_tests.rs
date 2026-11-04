//! Planning is exercised through authorized application commands, with no executor.
use super::*;

#[test]
fn issue_planning_manual_creation_is_unassigned_without_a_directory() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
    let commands = AuthorizedCommandHandler::new(policy, handlers);
    let before = store.mutation_evidence_counts().await;
    let choices = queries
      .handle_query(&context(), GetIssuePropertyChoicesQuery { project_id })
      .await
      .unwrap();
    assert!(!choices.assignees.available);
    assert!(choices.assignees.items.is_empty());
    assert_eq!(choices.definitions.version, 0);
    assert!(choices.definitions.definition.properties.is_empty());
    assert_eq!(store.mutation_evidence_counts().await, before);
    let created = commands
      .handle_command(&context(), create(project_id, "unassigned"))
      .await
      .unwrap()
      .issue;
    assert_eq!(created.planning, IssuePlanning::default());
    assert_eq!(created.version.get(), 1);
    assert_eq!(created.applicability_version.get(), 1);
  });
}

fn catalog() -> octacity_server_store::IssuePropertyDefinitionSet {
  serde_json::from_value(serde_json::json!({"labels":[{"id":"defect","name":"Дефект","retired":false}],"properties":[
    {"id":"size","name":"Размер","retired":false,"kind":{"type":"choice","choices":[{"id":"small","name":"S","retired":false},{"id":"large","name":"XL","retired":false}]}},
    {"id":"component","name":"Компонент","retired":false,"kind":{"type":"choice","choices":[{"id":"ui","name":"Интерфейс","retired":false}]}},
    {"id":"type","name":"Тип","retired":false,"kind":{"type":"choice","choices":[{"id":"bug","name":"Дефект","retired":false}]}},
    {"id":"notes","name":"Заметка","retired":false,"kind":{"type":"text","max_bytes":8}},
    {"id":"verified","name":"Уточнено","retired":false,"kind":{"type":"boolean"}},
    {"id":"estimate","name":"Оценка","retired":false,"kind":{"type":"number","minimum":0,"maximum":10}},
    {"id":"review_date","name":"Дата","retired":false,"kind":{"type":"date"}}
  ]})).unwrap()
}
fn publication(project_id: ProjectId, identity: &str) -> PublishIssuePropertiesCommand {
  PublishIssuePropertiesCommand {
    intent: PublishIssuePropertiesIntent {
      project_id,
      expected_version: 0,
      definition: catalog(),
    },
    idempotency_key: key(identity),
    published_at: at(20),
  }
}
#[test]
fn issue_planning_definitions_are_bounded_versioned_and_replayable() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let original = publication(project_id, "catalog");
    let first = commands.handle_command(&context(), original.clone()).await.unwrap();
    assert_eq!(first.resource.version, 1);
    assert_eq!(first.resource.definition, catalog());
    let mut replacement = publication(project_id, "retire");
    replacement.intent.expected_version = 1;
    replacement.published_at = at(21);
    replacement.intent.definition.labels[0].retired = true;
    let second = commands.handle_command(&context(), replacement.clone()).await.unwrap();
    assert_eq!(second.resource.version, 2);
    let before = store.mutation_evidence_counts().await;
    let retained = queries
      .handle_query(
        &context(),
        GetIssuePropertiesQuery {
          project_id,
          version: Some(1),
        },
      )
      .await
      .unwrap();
    assert_eq!(retained, first.resource);
    let replay = commands.handle_command(&context(), original).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.resource, first.resource);
    assert_eq!(store.mutation_evidence_counts().await, before);
    replacement.idempotency_key = key("stale-catalog");
    assert!(commands.handle_command(&context(), replacement).await.is_err());
    for invalid in [
      serde_json::json!({"id":"status","name":"Статус","retired":false,"kind":{"type":"boolean"}}),
      serde_json::json!({"id":"empty","name":" ","retired":false,"kind":{"type":"boolean"}}),
      serde_json::json!({"id":"overflow","name":"Число","retired":false,"kind":{"type":"number","minimum":10,"maximum":0}}),
      serde_json::json!({"id":"text","name":"Текст","retired":false,"kind":{"type":"text","max_bytes":65535}}),
      serde_json::json!({"id":"choice","name":"Выбор","retired":false,"kind":{"type":"choice","choices":[]}}),
    ] {
      let mut bad = publication(project_id, "invalid");
      bad.intent.expected_version = 2;
      bad
        .intent
        .definition
        .properties
        .push(serde_json::from_value(invalid).unwrap());
      assert!(commands.handle_command(&context(), bad).await.is_err());
    }
    assert!(serde_json::from_value::<octacity_server_store::IssuePropertyDefinition>(serde_json::json!({"id":"formula","name":"Formula","retired":false,"kind":{"type":"formula","expression":"start()"}})).is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

fn planned() -> IssuePlanning {
  serde_json::from_value(serde_json::json!({"definition_version":1,"assignee":null,"labels":["defect"],"due_date":"2028-02-29","parent_id":null,"values":{
    "size":{"type":"choice","value":"small"},"component":{"type":"choice","value":"ui"},"type":{"type":"choice","value":"bug"},
    "notes":{"type":"text","value":"тест"},"verified":{"type":"boolean","value":false},"estimate":{"type":"number","value":3},"review_date":{"type":"date","value":"2026-10-19"}
  }})).unwrap()
}
fn planning_edit(issue: &IssueRevision, identity: &str) -> EditIssuePlanningCommand {
  EditIssuePlanningCommand {
    intent: EditIssuePlanningIntent {
      project_id: issue.project_id,
      id: issue.id,
      expected_version: issue.version,
      planning: issue.planning.clone(),
    },
    idempotency_key: key(identity),
    edited_at: at(30),
  }
}
#[test]
fn issue_planning_create_edit_retains_original_revisions_without_status_or_execution() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    commands
      .handle_command(&context(), publication(project_id, "definitions"))
      .await
      .unwrap();
    let mut original = create(project_id, "planned");
    original.intent.planning = planned();
    let first = commands
      .handle_command(&context(), original.clone())
      .await
      .unwrap()
      .issue;
    assert_eq!(first.planning, planned());
    for (field, value) in [
      ("due_date", serde_json::json!("2027-02-29")),
      ("due_date", serde_json::json!("2028-02-29T00:00:00Z")),
      ("labels", serde_json::json!(["unknown"])),
      (
        "values",
        serde_json::json!({"size":{"type":"choice","value":"unknown"}}),
      ),
      ("values", serde_json::json!({"estimate":{"type":"number","value":11}})),
      (
        "values",
        serde_json::json!({"notes":{"type":"text","value":"123456789"}}),
      ),
      ("values", serde_json::json!({"verified":{"type":"text","value":"true"}})),
      ("values", serde_json::json!({"status":{"type":"text","value":"done"}})),
    ] {
      let mut json = serde_json::to_value(planned()).unwrap();
      json[field] = value;
      let mut rejected = planning_edit(&first, "invalid-planning");
      rejected.intent.planning = serde_json::from_value(json).unwrap();
      assert!(commands.handle_command(&context(), rejected).await.is_err());
    }
    let mut reserved = serde_json::to_value(planned()).unwrap();
    reserved["status"] = serde_json::json!("done");
    assert!(serde_json::from_value::<IssuePlanning>(reserved).is_err());
    let mut edit = planning_edit(&first, "planning-edit");
    edit.intent.planning.due_date = Some("2029-01-01".into());
    let second = commands.handle_command(&context(), edit.clone()).await.unwrap().issue;
    assert_eq!(second.version.get(), 2);
    assert_eq!(second.applicability_version, first.applicability_version);
    assert_eq!(second.content, first.content);
    assert_eq!(second.status_id, first.status_id);
    assert_eq!(second.workflow_version, first.workflow_version);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: first.id,
            version: Some(first.version)
          }
        )
        .await
        .unwrap(),
      first
    );
    let before = store.mutation_evidence_counts().await;
    assert_eq!(
      commands.handle_command(&context(), original).await.unwrap().issue,
      first
    );
    assert_eq!(
      commands.handle_command(&context(), edit.clone()).await.unwrap().issue,
      second
    );
    edit.idempotency_key = key("stale");
    assert!(commands.handle_command(&context(), edit).await.is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
    let history = queries
      .handle_query(
        &context(),
        ListIssueStatusHistoryQuery::try_new(project_id, first.id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(history.items.len(), 1);
    let legacy = commands
      .handle_command(&context(), create(project_id, "ordinary-after-catalog"))
      .await
      .unwrap()
      .issue;
    assert_eq!(legacy.planning, IssuePlanning::default());
  });
}

struct People(std::sync::Mutex<IssueAssigneeChoices>);
#[async_trait::async_trait]
impl IssueAssigneeDirectory for People {
  async fn choices(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
  ) -> Result<IssueAssigneeChoices, ApplicationError> {
    Ok(self.0.lock().unwrap().clone())
  }
}
#[test]
fn issue_planning_assignment_uses_visible_directory_without_granting_rights() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let people = Arc::new(People(std::sync::Mutex::new(IssueAssigneeChoices {
      available: true,
      items: vec![IssueAssignee {
        id: "person-1".into(),
        name: "Оператор".into(),
      }],
    })));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()).with_assignee_directory(people.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers.clone());
    let choices = queries
      .handle_query(&context(), GetIssuePropertyChoicesQuery { project_id })
      .await
      .unwrap();
    assert!(choices.assignees.available);
    assert_eq!(choices.assignees.items[0].id, "person-1");
    let mut original = create(project_id, "assigned");
    original.intent.planning.assignee = Some("person-1".into());
    let first = commands
      .handle_command(&context(), original.clone())
      .await
      .unwrap()
      .issue;
    assert_eq!(first.planning.assignee.as_deref(), Some("person-1"));
    let before = store.mutation_evidence_counts().await;
    for person in ["hidden-person", "gateway-service"] {
      let mut rejected = planning_edit(&first, "unknown-person");
      rejected.intent.planning.assignee = Some(person.into());
      assert!(commands.handle_command(&context(), rejected).await.is_err());
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    *people.0.lock().unwrap() = IssueAssigneeChoices::default();
    let mut edit = planning_edit(&first, "date-while-directory-down");
    edit.intent.planning.due_date = Some("2028-01-01".into());
    let edited = commands.handle_command(&context(), edit).await.unwrap().issue;
    assert_eq!(edited.planning.assignee, first.planning.assignee);
    assert_eq!(
      commands.handle_command(&context(), original).await.unwrap().issue,
      first
    );
    let restricted = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::View],
        visibility: ManagementVisibility::all(),
      }),
      handlers,
    );
    assert!(
      restricted
        .handle_command(&context(), planning_edit(&edited, "no-edit-right"))
        .await
        .is_err()
    );
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
  });
}

struct HideIssue(octacity_server_domain::IssueId);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for HideIssue {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(
      if action == ManagementAction::View
        && resource.kind() == ManagementResourceKind::Issue
        && resource.identity().is_some_and(|id| id.as_str() == self.0.to_string())
      {
        ManagementVisibility::none()
      } else {
        ManagementVisibility::all()
      },
    ))
  }
}
#[test]
fn issue_planning_parent_links_reject_cycles_foreign_self_and_hidden_targets() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let foreign_project = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let parent = commands
      .handle_command(&context(), create(project_id, "parent"))
      .await
      .unwrap()
      .issue;
    let child = commands
      .handle_command(&context(), create(project_id, "child"))
      .await
      .unwrap()
      .issue;
    let foreign = commands
      .handle_command(&context(), create(foreign_project, "foreign"))
      .await
      .unwrap()
      .issue;
    let before = store.mutation_evidence_counts().await;
    for target in [child.id, foreign.id, octacity_server_domain::IssueId::generate()] {
      let mut rejected = planning_edit(&child, "invalid-parent");
      rejected.intent.planning.parent_id = Some(target);
      assert!(commands.handle_command(&context(), rejected).await.is_err());
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    let mut link = planning_edit(&child, "link");
    link.intent.planning.parent_id = Some(parent.id);
    let linked = commands.handle_command(&context(), link.clone()).await.unwrap().issue;
    assert_eq!(linked.planning.parent_id, Some(parent.id));
    let mut cycle = planning_edit(&parent, "cycle");
    cycle.intent.planning.parent_id = Some(child.id);
    assert!(commands.handle_command(&context(), cycle).await.is_err());
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: parent.id,
            version: None
          }
        )
        .await
        .unwrap(),
      parent
    );
    let policy = Arc::new(HideIssue(parent.id));
    let hidden = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)));
    let before = store.mutation_evidence_counts().await;
    assert!(
      hidden.handle_command(&context(), link).await.is_err(),
      "current parent visibility precedes replay"
    );
    let mut hidden_create = create(project_id, "hidden-parent-create");
    hidden_create.intent.planning.parent_id = Some(parent.id);
    assert!(hidden.handle_command(&context(), hidden_create).await.is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
    let moved = commands
      .handle_command(&context(), move_from(&linked, "child-move"))
      .await
      .unwrap()
      .issue;
    assert_ne!(moved.status_id, parent.status_id);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueQuery {
            project_id,
            id: parent.id,
            version: None
          }
        )
        .await
        .unwrap(),
      parent
    );
  });
}

#[test]
fn issue_planning_children_filter_visibility_before_paging_and_competing_links_stay_acyclic() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), handlers.clone()));
    let parent = commands
      .handle_command(&context(), create(project_id, "parent"))
      .await
      .unwrap()
      .issue;
    let a = commands
      .handle_command(&context(), create(project_id, "a"))
      .await
      .unwrap()
      .issue;
    let b = commands
      .handle_command(&context(), create(project_id, "b"))
      .await
      .unwrap()
      .issue;
    for issue in [&a, &b] {
      let mut edit = planning_edit(issue, &format!("link-{}", issue.id));
      edit.intent.planning.parent_id = Some(parent.id);
      commands.handle_command(&context(), edit).await.unwrap();
    }
    let policy = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::View],
      visibility: ManagementVisibility::restricted([parent.id, b.id].into_iter().map(|id| {
        ManagementResource::instance(
          ManagementResourceKind::Issue,
          ManagementResourceIdentity::new(id.to_string()).unwrap(),
        )
        .unwrap()
      }))
      .unwrap(),
    });
    let queries = AuthorizedQueryHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)));
    let page = queries
      .handle_query(
        &context(),
        ListIssueChildrenQuery::try_new(project_id, parent.id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, b.id);
    assert_eq!(page.next_cursor, None);
    assert!(
      queries
        .handle_query(
          &context(),
          ListIssueChildrenQuery::try_new(project_id, parent.id, Some(a.id), 1).unwrap()
        )
        .await
        .is_err()
    );
    let x = commands
      .handle_command(&context(), create(project_id, "x"))
      .await
      .unwrap()
      .issue;
    let y = commands
      .handle_command(&context(), create(project_id, "y"))
      .await
      .unwrap()
      .issue;
    let mut left = planning_edit(&x, "x-y");
    left.intent.planning.parent_id = Some(y.id);
    let mut right = planning_edit(&y, "y-x");
    right.intent.planning.parent_id = Some(x.id);
    let results = std::sync::Mutex::new(Vec::new());
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
      for command in [left, right] {
        let commands = &commands;
        let results = &results;
        let barrier = &barrier;
        scope.spawn(move || {
          barrier.wait();
          run_ready(async {
            let success = commands.handle_command(&context(), command).await.is_ok();
            results.lock().unwrap().push(success);
          });
        });
      }
    });
    assert_eq!(
      results.into_inner().unwrap().iter().filter(|success| **success).count(),
      1
    );
  });
}

#[test]
fn issue_planning_retirement_ownership_and_failure_recovery_preserve_history() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    commands
      .handle_command(&context(), publication(project_id, "catalog"))
      .await
      .unwrap();
    let mut original = create(project_id, "retained");
    original.intent.planning = planned();
    let first = commands
      .handle_command(&context(), original.clone())
      .await
      .unwrap()
      .issue;
    let mut replacement = publication(project_id, "retire");
    replacement.intent.expected_version = 1;
    replacement.published_at = at(21);
    replacement.intent.definition.labels[0].retired = true;
    if let octacity_server_store::IssuePropertyKind::Choice { choices } =
      &mut replacement.intent.definition.properties[0].kind
    {
      choices[0].retired = true;
      choices[0].name = "Маленький".into();
    }
    let mut published = commands.handle_command(&context(), replacement).await.unwrap().resource;
    store
      .seed_issue_field_policy(
        project_id,
        first.id,
        IssueFieldPolicy {
          source_owned: [IssueField::DueDate, IssueField::Properties].into_iter().collect(),
          ..Default::default()
        },
      )
      .unwrap();
    let mut rejected = planning_edit(&first, "owned-date");
    rejected.intent.planning.definition_version = 2;
    rejected.intent.planning.due_date = None;
    assert!(commands.handle_command(&context(), rejected).await.is_err());
    let mut edit = planning_edit(&first, "retain-retired");
    edit.intent.planning.definition_version = 2;
    edit.intent.planning.labels.clear();
    let edited = commands.handle_command(&context(), edit).await.unwrap().issue;
    assert_eq!(edited.planning.values, first.planning.values);
    assert_eq!(edited.applicability_version, first.applicability_version);
    let mut fresh = create(project_id, "retired-fresh");
    fresh.intent.planning = planned();
    fresh.intent.planning.definition_version = 2;
    assert!(commands.handle_command(&context(), fresh.clone()).await.is_err());
    fresh.intent.planning.labels.clear();
    assert!(commands.handle_command(&context(), fresh).await.is_err());
    assert_eq!(
      commands.handle_command(&context(), original).await.unwrap().issue,
      first
    );
    let old = queries
      .handle_query(
        &context(),
        GetIssuePropertiesQuery {
          project_id,
          version: Some(1),
        },
      )
      .await
      .unwrap();
    assert_eq!(old.definition, catalog());
    use octacity_server_store::testing::MutationFailurePoint;
    for point in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ] {
      let before = store.mutation_evidence_counts().await;
      let command = PublishIssuePropertiesCommand {
        intent: PublishIssuePropertiesIntent {
          project_id,
          expected_version: published.version,
          definition: published.definition.clone(),
        },
        idempotency_key: key(&format!("publication-{point:?}")),
        published_at: at(50),
      };
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(
        commands.handle_command(&context(), command.clone()).await.is_err(),
        "{point:?}"
      );
      assert_eq!(
        queries
          .handle_query(
            &context(),
            GetIssuePropertiesQuery {
              project_id,
              version: None
            }
          )
          .await
          .unwrap(),
        published
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
      published = commands.handle_command(&context(), command).await.unwrap().resource;
    }
    let mut current = commands
      .handle_command(&context(), create(project_id, "failure-edit"))
      .await
      .unwrap()
      .issue;
    for (index, point) in [
      MutationFailurePoint::DomainState,
      MutationFailurePoint::IdempotencyOutcome,
      MutationFailurePoint::AuditFact,
      MutationFailurePoint::OutboxEntry,
      MutationFailurePoint::Commit,
    ]
    .into_iter()
    .enumerate()
    {
      let mut command = planning_edit(&current, &format!("planning-{point:?}"));
      command.intent.planning.definition_version = published.version;
      command.intent.planning.due_date = Some(format!("2028-01-{:02}", index + 1));
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(commands.handle_command(&context(), command.clone()).await.is_err());
      assert_eq!(
        queries
          .handle_query(
            &context(),
            GetIssueQuery {
              project_id,
              id: current.id,
              version: None
            }
          )
          .await
          .unwrap(),
        current
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
      current = commands
        .handle_command(&context(), command.clone())
        .await
        .unwrap()
        .issue;
      assert_eq!(
        commands.handle_command(&context(), command).await.unwrap().issue,
        current
      );
    }
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
  });
}

#[test]
fn issue_planning_catalog_and_directory_bounds_keep_authority_separate() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let current = commands
      .handle_command(&context(), publication(project_id, "catalog"))
      .await
      .unwrap()
      .resource;
    let before = store.mutation_evidence_counts().await;
    let edit_only = AuthorizedCommandHandler::new(
      Arc::new(ScopedPolicy {
        actions: vec![ManagementAction::Update],
        visibility: ManagementVisibility::all(),
      }),
      handlers.clone(),
    );
    let mut publication = publication(project_id, "separate-admin");
    publication.intent.expected_version = 1;
    assert!(edit_only.handle_command(&context(), publication.clone()).await.is_err());
    let hidden = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::Administer, ManagementAction::View],
      visibility: ManagementVisibility::none(),
    });
    let hidden_commands = AuthorizedCommandHandler::new(hidden.clone(), handlers.clone());
    let hidden_queries = AuthorizedQueryHandler::new(hidden, handlers);
    assert!(
      hidden_commands
        .handle_command(&context(), publication.clone())
        .await
        .is_err()
    );
    assert!(
      hidden_queries
        .handle_query(&context(), GetIssuePropertyChoicesQuery { project_id })
        .await
        .is_err()
    );
    let mut invalid = Vec::new();
    let mut changed = current.definition.clone();
    changed.properties[0].kind = octacity_server_store::IssuePropertyKind::Boolean;
    invalid.push(changed);
    let mut removed = current.definition.clone();
    removed.properties.remove(0);
    invalid.push(removed);
    let mut duplicate = current.definition.clone();
    duplicate.labels.push(duplicate.labels[0].clone());
    invalid.push(duplicate);
    let mut duplicate = current.definition.clone();
    duplicate.properties.push(duplicate.properties[0].clone());
    invalid.push(duplicate);
    let mut oversized = current.definition.clone();
    oversized.properties = (0..33)
      .map(|n| octacity_server_store::IssuePropertyDefinition {
        id: format!("field_{n}"),
        name: format!("Field {n}"),
        kind: octacity_server_store::IssuePropertyKind::Boolean,
        retired: false,
      })
      .collect();
    invalid.push(oversized);
    for definition in invalid {
      let mut rejected = publication.clone();
      rejected.intent.definition = definition;
      assert!(commands.handle_command(&context(), rejected).await.is_err());
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
    let people = Arc::new(People(std::sync::Mutex::new(IssueAssigneeChoices {
      available: true,
      items: (0..201)
        .map(|n| IssueAssignee {
          id: format!("person-{n}"),
          name: format!("Person {n}"),
        })
        .collect(),
    })));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy).with_assignee_directory(people)),
    );
    assert!(
      queries
        .handle_query(&context(), GetIssuePropertyChoicesQuery { project_id })
        .await
        .is_err()
    );
    assert!(
      serde_json::from_value::<octacity_server_store::IssuePropertyValue>(
        serde_json::json!({"type":"number","value":1.5})
      )
      .is_err()
    );
    assert!(
      serde_json::from_value::<octacity_server_store::IssuePropertyValue>(
        serde_json::json!({"type":"text","value":{"execute":"start"}})
      )
      .is_err()
    );
    assert!(
      ListIssueChildrenQuery::try_new(project_id, octacity_server_domain::IssueId::generate(), None, 201).is_err()
    );
  });
}

#[test]
fn issue_planning_invalid_values_report_the_actual_mutation_operation() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store, policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy, handlers);
    commands
      .handle_command(&context(), publication(project_id, "definitions"))
      .await
      .unwrap();
    let mut original = create(project_id, "invalid-date");
    original.intent.planning = planned();
    original.intent.planning.due_date = Some("2027-02-29".into());
    let error = commands.handle_command(&context(), original).await.unwrap_err();
    assert!(matches!(
      error.application(),
      Some(ApplicationError::Store(
        octacity_server_store::StoreError::InvalidInput {
          operation: octacity_server_store::StoreOperation::CreateIssue,
          ..
        }
      ))
    ));
    let task = commands
      .handle_command(&context(), create(project_id, "ordinary"))
      .await
      .unwrap()
      .issue;
    let mut edit = planning_edit(&task, "invalid-edit-date");
    edit.intent.planning = planned();
    edit.intent.planning.due_date = Some("2027-02-29".into());
    let error = commands.handle_command(&context(), edit).await.unwrap_err();
    assert!(matches!(
      error.application(),
      Some(ApplicationError::Store(
        octacity_server_store::StoreError::InvalidInput {
          operation: octacity_server_store::StoreOperation::EditIssuePlanning,
          ..
        }
      ))
    ));
  });
}

#[test]
fn issue_planning_empty_snapshots_preserve_legacy_json_and_private_value_debug() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store, policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy, handlers);
    let command = create(project_id, "legacy-json");
    assert!(serde_json::to_value(&command.intent).unwrap().get("planning").is_none());
    let created = commands.handle_command(&context(), command).await.unwrap().issue;
    let json = serde_json::to_value(&created).unwrap();
    assert!(
      json.get("planning").is_none(),
      "empty planning must preserve the pre-planning stored revision shape"
    );
    assert_eq!(serde_json::from_value::<IssueRevision>(json).unwrap(), created);
    commands
      .handle_command(&context(), publication(project_id, "definitions"))
      .await
      .unwrap();
    let mut edit = planning_edit(&created, "private-value");
    edit.intent.planning.definition_version = 1;
    edit.intent.planning.values.insert(
      "notes".into(),
      octacity_server_store::IssuePropertyValue::Text("PRIVATE".into()),
    );
    let edited = commands.handle_command(&context(), edit).await.unwrap().issue;
    assert!(!format!("{edited:?}").contains("PRIVATE"));
    assert!(!format!("{:?}", edited.planning.values).contains("PRIVATE"));
  });
}
