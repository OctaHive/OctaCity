//! Ordinary manual Issue ports retain classification atomically, without a producer.
use super::*;
use octacity_server_store::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn project_size_catalog_round_trips_custom_values_retirement_and_original_replay() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let mut definition = IssuePropertyDefinitionSet::default();
  definition.sizes.push(IssuePropertyChoice {
    id: "xl".into(),
    name: "Extra large".into(),
    retired: false,
  });
  let publication = PublishIssueProperties {
    intent: PublishIssuePropertiesIntent {
      project_id: project.id,
      expected_version: 0,
      definition,
    },
    idempotency_key: key("custom-size-catalog"),
    published_at: at(3),
  };
  let catalog = store
    .publish_issue_properties(management_mutation(publication.clone()))
    .await
    .unwrap()
    .resource;
  let mut creation = draft(project.id, "custom-size-issue");
  creation.intent.planning = IssuePlanning {
    definition_version: 1,
    size: Some("xl".into()),
    ..Default::default()
  };
  let original = store
    .create_issue(management_mutation(creation.clone()))
    .await
    .unwrap()
    .issue;
  drop(store);
  let restored = PostgresStore::new(database.pool.clone());
  assert_eq!(restored.issue(project.id, original.id, None).await.unwrap(), original);
  let mut definition = catalog.definition.clone();
  definition.sizes.last_mut().unwrap().retired = true;
  definition.sizes.reverse();
  restored
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: project.id,
        expected_version: 1,
        definition,
      },
      idempotency_key: key("retire-size"),
      published_at: at(4),
    }))
    .await
    .unwrap();
  let edit = EditIssuePlanning {
    intent: EditIssuePlanningIntent {
      project_id: project.id,
      id: original.id,
      expected_version: original.version,
      planning: IssuePlanning {
        definition_version: 2,
        ..original.planning.clone()
      },
    },
    assignees: Default::default(),
    idempotency_key: key("retain-retired-size"),
    edited_at: at(5),
  };
  let edited = restored
    .edit_issue_planning(management_mutation(edit))
    .await
    .unwrap()
    .issue;
  assert_eq!(edited.planning.size.as_deref(), Some("xl"));
  let mut rejected = draft(project.id, "new-retired-size");
  rejected.intent.planning = IssuePlanning {
    definition_version: 2,
    size: Some("xl".into()),
    ..Default::default()
  };
  assert!(restored.create_issue(management_mutation(rejected)).await.is_err());
  assert_eq!(restored.issue_properties(project.id, Some(1)).await.unwrap(), catalog);
  assert_eq!(
    restored
      .publish_issue_properties(management_mutation(publication))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    restored
      .create_issue(management_mutation(creation))
      .await
      .unwrap()
      .issue,
    original
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn manual_classification_persists_origins_stale_fields_and_original_receipts() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let mut creation = draft(project.id, "manually-prepared");
  creation.intent.content.priority = Some(IssuePriority::Choice("high".into()));
  creation.intent.planning.issue_type = Some("defect".into());
  creation.intent.planning.size = Some(String::from("small"));
  let created = store
    .create_issue(management_mutation(creation.clone()))
    .await
    .unwrap()
    .issue;
  assert_eq!(created.classification.fields.len(), 3);
  for field in &created.classification.fields {
    assert!(
      matches!(&field.origin, IssueClassificationOrigin::Manual {actor} if actor.kind == AuditActorKind::UnauthenticatedManagement && actor.identity.is_none())
    );
    assert_eq!(field.requirements_version, created.applicability_version);
    assert!(field.evidence.is_empty());
  }
  let changed = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: project.id,
        id: created.id,
        expected_version: created.version,
        content: IssueContent {
          acceptance: Some("New acceptance".into()),
          ..created.content.clone()
        },
      },
      idempotency_key: key("requirements"),
      edited_at: at(3),
    }))
    .await
    .unwrap()
    .issue;
  assert_eq!(changed.classification, created.classification);
  let edit = EditIssuePlanning {
    intent: EditIssuePlanningIntent {
      project_id: project.id,
      id: changed.id,
      expected_version: changed.version,
      planning: IssuePlanning {
        size: Some(String::from("large")),
        ..changed.planning.clone()
      },
    },
    assignees: Default::default(),
    idempotency_key: key("size-override"),
    edited_at: at(4),
  };
  let overridden = store
    .edit_issue_planning(management_mutation(edit.clone()))
    .await
    .unwrap()
    .issue;
  let size = overridden
    .classification
    .fields
    .iter()
    .find(|field| field.field.identity() == octacity_server_store::IssueClassificationFieldIdentity::Size)
    .unwrap();
  assert_eq!(size.requirements_version, changed.applicability_version);
  assert_eq!(size.assessed_version, changed.version);
  assert_eq!(size.assessed_at, at(4));
  for prior in created
    .classification
    .fields
    .iter()
    .filter(|field| field.field.identity() != octacity_server_store::IssueClassificationFieldIdentity::Size)
  {
    assert!(overridden.classification.fields.contains(prior));
    assert_ne!(prior.requirements_version, overridden.applicability_version);
  }
  let priority = EditIssue {
    intent: EditIssueIntent {
      project_id: project.id,
      id: overridden.id,
      expected_version: overridden.version,
      content: IssueContent {
        priority: Some(IssuePriority::Choice("low".into())),
        ..overridden.content.clone()
      },
    },
    idempotency_key: key("priority-override"),
    edited_at: at(5),
  };
  let current = store
    .edit_issue(management_mutation(priority.clone()))
    .await
    .unwrap()
    .issue;
  let priority_origin = current
    .classification
    .fields
    .iter()
    .find(|field| {
      matches!(
        field.field,
        IssueClassificationFieldReference::Priority {
          definition_version: Some(0)
        }
      )
    })
    .unwrap();
  assert_eq!(priority_origin.assessed_at, at(5));
  assert_eq!(priority_origin.requirements_version, changed.applicability_version);
  let restarted = PostgresStore::new(database.pool.clone());
  for (version, expected) in [
    (created.version, &created),
    (overridden.version, &overridden),
    (current.version, &current),
  ] {
    assert_eq!(
      &restarted.issue(project.id, created.id, Some(version)).await.unwrap(),
      expected
    );
  }
  let replay = restarted.edit_issue_planning(management_mutation(edit)).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, overridden);
  assert_eq!(
    restarted.edit_issue(management_mutation(priority)).await.unwrap().issue,
    current
  );
  assert_eq!(
    restarted
      .create_issue(management_mutation(creation))
      .await
      .unwrap()
      .issue,
    created
  );
  assert_eq!(restarted.issue(project.id, created.id, None).await.unwrap(), current);
  database.cleanup().await;
}
