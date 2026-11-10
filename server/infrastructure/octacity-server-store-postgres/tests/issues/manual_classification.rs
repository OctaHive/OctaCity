//! Ordinary manual Issue ports retain classification atomically, without a producer.
use super::*;
use octacity_server_store::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn manual_classification_persists_origins_stale_fields_and_original_receipts() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let mut creation = draft(project.id, "manually-prepared");
  creation.intent.content.priority = Some(IssuePriority::Choice("high".into()));
  creation.intent.planning.issue_type = Some("defect".into());
  creation.intent.planning.size = Some(IssueSize::Small);
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
        size: Some(IssueSize::Large),
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
    .find(|field| field.field == IssueClassificationFieldReference::Size)
    .unwrap();
  assert_eq!(size.requirements_version, changed.applicability_version);
  assert_eq!(size.assessed_version, changed.version);
  assert_eq!(size.assessed_at, at(4));
  for prior in created
    .classification
    .fields
    .iter()
    .filter(|field| field.field != IssueClassificationFieldReference::Size)
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
