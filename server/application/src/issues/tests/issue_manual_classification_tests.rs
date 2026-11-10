//! Manual preparation uses ordinary authorized Issue commands, without a Factory producer.
use super::*;
use octacity_server_store::{IssueClassificationFieldReference as Field, IssueClassificationOrigin};

#[path = "issue_classification_applicability_tests.rs"]
mod applicability;

fn manual_catalog(project_id: ProjectId) -> PublishIssuePropertiesCommand {
  let mut command = publication(project_id, "manual-classification-catalog");
  command.intent.definition = serde_json::from_value(serde_json::json!({
    "labels": [], "components": [{"id":"console","name":"Console","retired":false}],
    "properties": [
      {"id":"impact","name":"Impact","retired":false,"kind":{"type":"choice","choices":[
        {"id":"minor","name":"Minor","retired":false},{"id":"major","name":"Major","retired":false}]}},
      {"id":"risk_band","name":"Risk","retired":false,"kind":{"type":"choice","choices":[
        {"id":"low","name":"Low","retired":false}]}},
      {"id":"delivery_route","name":"Route","retired":false,"kind":{"type":"choice","choices":[
        {"id":"research","name":"Research","retired":false}]}},
      {"id":"reproduction","name":"Reproduction","retired":false,"kind":{"type":"choice","choices":[
        {"id":"unknown","name":"Unknown","retired":false},
        {"id":"needs_input","name":"Needs input","retired":false},
        {"id":"reproduced","name":"Reproduced","retired":false},
        {"id":"not_reproduced","name":"Not reproduced","retired":false}]}},
      {"id":"notes","name":"Notes","retired":false,"kind":{"type":"text","max_bytes":80}}
    ],
    "classification":{"severity":"impact","risk":"risk_band","recommended_route":"delivery_route","reproducibility":"reproduction"}
  })).unwrap();
  command
}

#[test]
fn manual_classification_planning_retains_human_origin_without_a_run() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::Administer,
        ManagementAction::Create,
        ManagementAction::Update,
        ManagementAction::View,
      ],
      visibility: ManagementVisibility::all(),
    });
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let operator = verified_context("alice");
    commands
      .handle_command(&operator, manual_catalog(project_id))
      .await
      .unwrap();
    let original = commands
      .handle_command(&operator, create(project_id, "manual-issue"))
      .await
      .unwrap()
      .issue;
    let mut edit = planning_edit(&original, "manual-classification");
    edit.intent.planning = serde_json::from_value(serde_json::json!({
      "definition_version":1,"labels":[],"component":"console","type":"defect","size":"medium",
      "values":{"impact":{"type":"choice","value":"major"},
        "risk_band":{"type":"choice","value":"low"},"delivery_route":{"type":"choice","value":"research"},
        "reproduction":{"type":"choice","value":"unknown"},"notes":{"type":"text","value":"reference"}}
    }))
    .unwrap();
    let saved = commands.handle_command(&operator, edit.clone()).await.unwrap().issue;
    assert_eq!(saved.planning, edit.intent.planning);
    assert_eq!(saved.applicability_version, original.applicability_version);
    let fields = &saved.classification.fields;
    assert_eq!(
      fields.len(),
      7,
      "only built-in and semantically bound fields receive classification origins"
    );
    for field in [
      Field::Component { definition_version: 1 },
      Field::Type { definition_version: 1 },
      Field::Size,
      Field::Property {
        id: "impact".into(),
        definition_version: 1,
      },
      Field::Property {
        id: "risk_band".into(),
        definition_version: 1,
      },
      Field::Property {
        id: "delivery_route".into(),
        definition_version: 1,
      },
      Field::Property {
        id: "reproduction".into(),
        definition_version: 1,
      },
    ] {
      let attribution = fields
        .iter()
        .find(|f| f.field == field)
        .expect("manual field attribution");
      let IssueClassificationOrigin::Manual { actor } = &attribution.origin else {
        panic!("manual preparation needs no Run")
      };
      assert_eq!(actor.identity.as_deref(), Some("alice"));
      assert_eq!(
        actor.kind,
        octacity_server_store::AuditActorKind::AuthenticatedManagement
      );
      assert_eq!(attribution.assessed_version, original.version);
      assert_eq!(attribution.requirements_version, original.applicability_version);
      assert_eq!(attribution.assessed_at, at(30));
      assert!(attribution.evidence.is_empty());
    }
    assert!(saved.classification.evidence.is_empty());
    assert!(saved.classification.dependencies.is_empty());
    assert_eq!(
      queries
        .handle_query(
          &operator,
          GetIssueQuery {
            project_id,
            id: saved.id,
            version: None
          }
        )
        .await
        .unwrap(),
      saved
    );
    assert_eq!(
      queries
        .handle_query(
          &operator,
          GetIssueQuery {
            project_id,
            id: saved.id,
            version: Some(original.version)
          }
        )
        .await
        .unwrap(),
      original
    );
    let mut remove = planning_edit(&saved, "remove-classification");
    remove.edited_at = at(31);
    remove.intent.planning = IssuePlanning {
      definition_version: 1,
      ..Default::default()
    };
    let cleared = commands.handle_command(&operator, remove).await.unwrap().issue;
    assert!(cleared.classification.is_empty());
    assert_eq!(commands.handle_command(&operator, edit).await.unwrap().issue, saved);
    assert_eq!(
      queries
        .handle_query(
          &operator,
          GetIssueQuery {
            project_id,
            id: cleared.id,
            version: None
          }
        )
        .await
        .unwrap(),
      cleared
    );
  });
}

#[test]
fn manual_classification_priority_override_does_not_refresh_other_stale_fields() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::Administer,
        ManagementAction::Create,
        ManagementAction::Update,
        ManagementAction::View,
      ],
      visibility: ManagementVisibility::all(),
    });
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let alice = verified_context("alice");
    let bob = verified_context("bob");
    commands
      .handle_command(&alice, manual_catalog(project_id))
      .await
      .unwrap();
    let original = commands
      .handle_command(&alice, create(project_id, "override-issue"))
      .await
      .unwrap()
      .issue;
    let mut edit = planning_edit(&original, "assess");
    edit.intent.planning.definition_version = 1;
    edit.intent.planning.size = Some(octacity_server_store::IssueSize::Medium);
    edit.intent.planning.values.insert(
      "impact".into(),
      octacity_server_store::IssuePropertyValue::Choice("major".into()),
    );
    let assessed = commands.handle_command(&alice, edit).await.unwrap().issue;
    let mut requirements = EditIssueCommand {
      intent: EditIssueIntent {
        project_id,
        id: assessed.id,
        expected_version: assessed.version,
        content: assessed.content.clone(),
      },
      idempotency_key: key("change-requirements"),
      edited_at: at(31),
    };
    requirements.intent.content.acceptance = Some("Different implementation behavior".into());
    let changed = commands.handle_command(&alice, requirements).await.unwrap().issue;
    assert_ne!(changed.applicability_version, assessed.applicability_version);
    let mut priority = EditIssueCommand {
      intent: EditIssueIntent {
        project_id,
        id: changed.id,
        expected_version: changed.version,
        content: changed.content.clone(),
      },
      idempotency_key: key("manual-priority"),
      edited_at: at(32),
    };
    priority.intent.content.priority = Some(IssuePriority::Choice("high".into()));
    let saved = commands.handle_command(&bob, priority.clone()).await.unwrap().issue;
    let attribution = saved
      .classification
      .fields
      .iter()
      .find(|f| {
        matches!(
          f.field,
          Field::Priority {
            definition_version: Some(1)
          }
        )
      })
      .expect("priority has its own manual origin");
    assert!(
      matches!(&attribution.origin, IssueClassificationOrigin::Manual {actor} if actor.identity.as_deref()==Some("bob"))
    );
    assert_eq!(attribution.assessed_version, changed.version);
    assert_eq!(attribution.requirements_version, changed.applicability_version);
    assert_eq!(attribution.assessed_at, at(32));
    for stale in &assessed.classification.fields {
      assert!(saved.classification.fields.contains(stale));
      assert_ne!(stale.requirements_version, saved.applicability_version);
    }
    let mut severity = planning_edit(&saved, "manual-severity");
    severity.edited_at = at(33);
    severity.intent.planning.values.insert(
      "impact".into(),
      octacity_server_store::IssuePropertyValue::Choice("minor".into()),
    );
    let overridden = commands.handle_command(&bob, severity).await.unwrap().issue;
    let impact = overridden
      .classification
      .fields
      .iter()
      .find(|f| matches!(&f.field, Field::Property {id,..} if id=="impact"))
      .unwrap();
    assert_eq!(impact.requirements_version, overridden.applicability_version);
    assert_eq!(impact.assessed_at, at(33));
    assert!(
      matches!(&impact.origin, IssueClassificationOrigin::Manual {actor} if actor.identity.as_deref()==Some("bob"))
    );
    assert!(
      overridden
        .classification
        .fields
        .iter()
        .any(|f| f.field == Field::Size && f.requirements_version == assessed.applicability_version)
    );
    let mut unrelated = planning_edit(&overridden, "unrelated-date");
    unrelated.edited_at = at(34);
    unrelated.intent.planning.due_date = Some("2026-12-01".into());
    let later = commands.handle_command(&bob, unrelated).await.unwrap().issue;
    assert_eq!(later.classification, overridden.classification);
    assert_eq!(commands.handle_command(&bob, priority).await.unwrap().issue, saved);
    assert_eq!(
      queries
        .handle_query(
          &bob,
          GetIssueQuery {
            project_id,
            id: later.id,
            version: None
          }
        )
        .await
        .unwrap(),
      later
    );
  });
}

#[test]
fn manual_classification_cannot_claim_verified_reproduction_on_edit_or_creation() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    commands
      .handle_command(&context(), manual_catalog(project_id))
      .await
      .unwrap();
    let original = commands
      .handle_command(&context(), create(project_id, "unverified"))
      .await
      .unwrap()
      .issue;
    let before = store.mutation_evidence_counts().await;
    for claim in ["reproduced", "not_reproduced"] {
      let mut edit = planning_edit(&original, claim);
      edit.intent.planning.definition_version = 1;
      edit.intent.planning.component = Some("console".into());
      edit.intent.planning.values.insert(
        "reproduction".into(),
        octacity_server_store::IssuePropertyValue::Choice(claim.into()),
      );
      assert!(
        commands.handle_command(&context(), edit.clone()).await.is_err(),
        "a manual value is not independent evidence: {claim}"
      );
      let mut creation = create(project_id, claim);
      creation.intent.planning = edit.intent.planning;
      assert!(
        commands.handle_command(&context(), creation).await.is_err(),
        "creation cannot bypass evidence checks: {claim}"
      );
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
    assert_eq!(store.mutation_evidence_counts().await, before);
    let mut unresolved = planning_edit(&original, "needs-input");
    unresolved.intent.planning.definition_version = 1;
    unresolved.intent.planning.values.insert(
      "reproduction".into(),
      octacity_server_store::IssuePropertyValue::Choice("needs_input".into()),
    );
    let saved = commands.handle_command(&context(), unresolved).await.unwrap().issue;
    assert_eq!(saved.classification.fields.len(), 1);
    assert!(saved.classification.fields[0].evidence.is_empty());
    assert!(saved.classification.evidence.is_empty());
  });
}

#[test]
fn manual_classification_uses_default_catalog_on_creation_and_edit_without_setup() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store, policy)));
    let mut creation = create(project_id, "default-classified");
    creation.intent.content.priority = Some(IssuePriority::Choice("high".into()));
    creation.intent.planning.issue_type = Some("defect".into());
    creation.intent.planning.size = Some(octacity_server_store::IssueSize::Small);
    let created = commands.handle_command(&context(), creation).await.unwrap().issue;
    assert_eq!(
      created.classification.fields.len(),
      3,
      "creation retains the same manual provenance as editing"
    );
    assert!(
      created
        .classification
        .fields
        .iter()
        .all(|field| matches!(&field.origin, IssueClassificationOrigin::Manual {actor} if actor.identity.is_none()))
    );
    for field in [
      Field::Priority {
        definition_version: Some(0),
      },
      Field::Type { definition_version: 0 },
      Field::Size,
    ] {
      let attribution = created.classification.fields.iter().find(|f| f.field == field).unwrap();
      assert_eq!(attribution.assessed_version, created.version);
      assert_eq!(attribution.requirements_version, created.applicability_version);
      assert_eq!(attribution.assessed_at, at(10));
    }
    let mut edit = planning_edit(&created, "default-type");
    edit.intent.planning.issue_type = Some("feature_request".into());
    let changed = commands.handle_command(&context(), edit).await.unwrap().issue;
    assert_eq!(changed.classification.fields.len(), 3);
    let changed = commands
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id,
            id: changed.id,
            expected_version: changed.version,
            content: IssueContent {
              priority: Some(IssuePriority::Choice("low".into())),
              description: "Changed requirements and priority together".into(),
              ..changed.content
            },
          },
          idempotency_key: key("default-priority"),
          edited_at: at(31),
        },
      )
      .await
      .unwrap()
      .issue;
    changed.classification.validate().unwrap();
    let retained: IssueRevision = serde_json::from_value(serde_json::to_value(&changed).unwrap()).unwrap();
    assert_eq!(retained, changed);
    assert_eq!(retained.planning.definition_version, 0);
    let priority = retained
      .classification
      .fields
      .iter()
      .find(|f| matches!(f.field, Field::Priority { .. }))
      .unwrap();
    assert_eq!(priority.assessed_version, retained.version);
    assert_eq!(priority.requirements_version, retained.applicability_version);
  });
}
