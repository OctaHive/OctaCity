//! Public persistence contracts for separately versioned native synchronization.
use super::*;
use octacity_server_store::*;

fn ownership(
  issue: &IssueRevision,
  imported: &ImportIssueSource,
  identity: &str,
  published_at: i64,
) -> PublishIssueSourceMapping {
  PublishIssueSourceMapping {
    intent: PublishIssueSourceMappingIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_issue_version: issue.version,
      expected_mapping: None,
      definition: IssueSourceMappingDefinition {
        source: imported.selection.source.clone(),
        reporter: None,
        push_enabled: false,
        fields: [(IssueField::Description, IssueSourceFieldAuthority::SourceOwnedPull)].into(),
        status: None,
      },
    },
    native_resource: imported.selection.item.resource.clone(),
    readable: octacity_server_factory::WorkSourceReportingPolicy {
      labels: vec![],
      states: vec![],
    },
    writable: None,
    idempotency_key: key(identity),
    published_at: at(published_at),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_mapping_replacement_retains_old_versions_and_invalidates_prior_read_authority() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let imported = import(project.id, "mapping-epoch-link");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("linked Issue")
  };
  let issue = store.issue(project.id, link.issue_id, None).await.unwrap();
  let first = ownership(&issue, &imported, "first-mapping-epoch", 4);
  let original = store
    .publish_issue_source_mapping(management_mutation(first.clone()))
    .await
    .unwrap()
    .mapping;
  let scope = ManagementSecurityScope::trusted_network();
  let target = IssueSourceReadTarget {
    source: imported.selection.source.clone(),
    issue_id: Some(issue.id),
    pull_generation: 0,
  };
  let scan = store
    .claim_issue_source_read(
      &scope,
      ClaimWorkSourceScan {
        project_id: project.id,
        target: target.clone(),
        owner: "old-mapping-owner".parse().unwrap(),
        now: at(6),
        until: at(20),
      },
    )
    .await
    .unwrap()
    .unwrap();
  let mut replacement = ownership(&issue, &imported, "second-mapping-epoch", 7);
  replacement.intent.expected_mapping = Some(original.reference());
  replacement
    .intent
    .definition
    .fields
    .insert(IssueField::Title, IssueSourceFieldAuthority::SourceOwnedPull);
  let current = store
    .publish_issue_source_mapping(management_mutation(replacement))
    .await
    .unwrap()
    .mapping;
  assert_eq!(current.version.get(), 2);
  assert!(
    store
      .checkpoint_issue_source_read(
        &scope,
        scan.claim,
        at(8),
        None,
        WorkSourceProgress {
          polling: None,
          window_start: at(0),
          window_end: at(6),
          continuation: None,
          reconciliation: false,
        }
      )
      .await
      .is_err(),
    "a changed ownership review must invalidate the original read owner"
  );
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .issue_source_mapping(project.id, issue.id, &scope, Some(original.version))
      .await
      .unwrap(),
    Some(original.clone())
  );
  assert_eq!(
    restarted
      .publish_issue_source_mapping(management_mutation(first))
      .await
      .unwrap()
      .mapping,
    original
  );
  assert_eq!(
    restarted
      .issue_source_mapping(project.id, issue.id, &scope, None)
      .await
      .unwrap(),
    Some(current)
  );
  assert_eq!(restarted.issue(project.id, issue.id, None).await.unwrap(), issue);
  let next = restarted
    .claim_issue_source_read(
      &scope,
      ClaimWorkSourceScan {
        project_id: project.id,
        target,
        owner: "current-mapping-owner".parse().unwrap(),
        now: at(21),
        until: at(40),
      },
    )
    .await
    .unwrap()
    .unwrap();
  assert!(next.progress.is_none());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_mapping_publication_and_transition_survive_restart_as_one_original_refresh() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let imported = import(project.id, "mapped-source");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("linked Issue")
  };
  let before = store.issue(project.id, link.issue_id, None).await.unwrap();
  let workflow = store.default_issue_workflow(project.id).await.unwrap().workflow;
  let target = workflow.definition.transitions[0].to;
  let publish = PublishIssueSourceMapping {
    intent: PublishIssueSourceMappingIntent {
      project_id: project.id,
      issue_id: before.id,
      expected_issue_version: before.version,
      expected_mapping: None,
      definition: IssueSourceMappingDefinition {
        source: imported.selection.source.clone(),
        reporter: None,
        push_enabled: false,
        fields: [
          (IssueField::Description, IssueSourceFieldAuthority::SourceOwnedPull),
          (IssueField::Status, IssueSourceFieldAuthority::SourceOwnedPull),
        ]
        .into(),
        status: Some(IssueSourceStatusMapping {
          workflow: IssueWorkflowPin {
            id: before.workflow_id,
            version: before.workflow_version,
          },
          pull: vec![IssueSourceStatusRule {
            native: octacity_server_factory::WorkSourcePredicate {
              state: Some("opaque-native".parse().unwrap()),
              all_of: vec![],
              any_of: vec![],
              none_of: vec![],
            },
            status_id: target,
          }],
          push: None,
        }),
      },
    },
    native_resource: imported.selection.item.resource.clone(),
    readable: octacity_server_factory::WorkSourceReportingPolicy {
      labels: vec![],
      states: vec!["opaque-native".into()],
    },
    writable: None,
    idempotency_key: key("publish-native-mapping"),
    published_at: at(4),
  };
  let original = store
    .publish_issue_source_mapping(management_mutation(publish.clone()))
    .await
    .unwrap();
  let scope = ManagementSecurityScope::trusted_network();
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .issue_source_mapping(project.id, before.id, &scope, Some(original.mapping.version))
      .await
      .unwrap(),
    Some(original.mapping.clone())
  );
  let mut refresh = RefreshIssueSource {
    explicit_read: None,
    status: Some(IssueSourceStatusObservation {
      state: "opaque-native".into(),
      labels: vec![],
    }),
    read_claim: None,
    selection: imported.selection.clone(),
    issue_id: before.id,
    observation: imported.observation,
    content: IssueContent {
      description: "Source-owned description".into(),
      ..before.content.clone()
    },
    idempotency_key: key("mapped-refresh"),
    refreshed_at: at(6),
  };
  refresh.observation.observed_at = at(5);
  let control = restarted
    .issue_source_refresh_state(project.id, before.id, &scope)
    .await
    .unwrap();
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_mapped_refresh CHECK (topic <> 'issue.source-refreshed') NOT VALID")
    .execute(&database.pool).await.unwrap();
  assert!(
    restarted
      .refresh_issue_source(management_mutation(refresh.clone()))
      .await
      .is_err()
  );
  assert_eq!(restarted.issue(project.id, before.id, None).await.unwrap(), before);
  assert_eq!(
    restarted
      .issue_source_refresh_state(project.id, before.id, &scope)
      .await
      .unwrap(),
    control
  );
  assert_eq!(
    restarted
      .issue_status_history(project.id, before.id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_mapped_refresh")
    .execute(&database.pool)
    .await
    .unwrap();
  let moved = restarted
    .refresh_issue_source(management_mutation(refresh.clone()))
    .await
    .unwrap();
  assert_eq!(
    moved.status.as_ref().unwrap().condition,
    IssueSourceStatusCondition::Applied
  );
  let current = restarted.issue(project.id, before.id, None).await.unwrap();
  assert_eq!(current.version.get(), 2, "text and status share one ordinary revision");
  assert_eq!(current.content.description, "Source-owned description");
  assert_eq!(current.status_id, target);
  let history = restarted
    .issue_status_history(project.id, before.id, None, 10)
    .await
    .unwrap();
  assert_eq!(history.items.len(), 2);
  assert_eq!(
    history.items[1].source.as_ref().unwrap().mapping,
    original.mapping.reference()
  );
  assert_eq!(
    PostgresStore::new(database.pool.clone())
      .refresh_issue_source(management_mutation(refresh))
      .await
      .unwrap(),
    moved
  );
  let replay = restarted
    .publish_issue_source_mapping(management_mutation(publish))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.mapping, original.mapping);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_mapping_failed_publication_and_competing_reviews_keep_one_atomic_version() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let imported = import(project.id, "atomic-mapping-link");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("linked Issue")
  };
  let issue = store.issue(project.id, link.issue_id, None).await.unwrap();
  let request = ownership(&issue, &imported, "atomic-mapping-publication", 4);
  let scope = ManagementSecurityScope::trusted_network();
  let control = store
    .issue_source_refresh_state(project.id, issue.id, &scope)
    .await
    .unwrap();
  sqlx::query("ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_mapping CHECK (topic <> 'issue.source-mapping-published') NOT VALID")
    .execute(&database.pool).await.unwrap();
  assert!(
    store
      .publish_issue_source_mapping(management_mutation(request.clone()))
      .await
      .is_err()
  );
  assert_eq!(
    store
      .issue_source_mapping(project.id, issue.id, &scope, None)
      .await
      .unwrap(),
    None
  );
  assert_eq!(
    store.issue_field_policy(project.id, issue.id).await.unwrap(),
    IssueFieldPolicy::default()
  );
  assert_eq!(
    store
      .issue_source_refresh_state(project.id, issue.id, &scope)
      .await
      .unwrap(),
    control
  );
  assert_eq!(
    store
      .issue_source_mapping_replay(&scope, &request.intent, &request.idempotency_key)
      .await
      .unwrap(),
    None
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_mapping")
    .execute(&database.pool)
    .await
    .unwrap();
  let mut other = request.clone();
  other.idempotency_key = key("competing-mapping-review");
  other
    .intent
    .definition
    .fields
    .insert(IssueField::Title, IssueSourceFieldAuthority::SourceOwnedPull);
  let restarted = PostgresStore::new(database.pool.clone());
  let (a, b) = tokio::join!(
    store.publish_issue_source_mapping(management_mutation(request)),
    restarted.publish_issue_source_mapping(management_mutation(other))
  );
  assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
  assert_eq!(
    store
      .issue_source_mapping(project.id, issue.id, &scope, None)
      .await
      .unwrap()
      .unwrap()
      .version
      .get(),
    1
  );
  assert!(
    store
      .issue_source_mapping(
        project.id,
        issue.id,
        &ManagementSecurityScope::new("another-partition").unwrap(),
        None
      )
      .await
      .is_err()
  );
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_push_freezes_original_desired_effects_in_the_existing_mutation_outbox() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let imported = import(project.id, "push-link");
  let IssueSourceOutcome::Linked { link, .. } = store
    .import_issue_source(management_mutation(imported.clone()))
    .await
    .unwrap()
  else {
    panic!("link");
  };
  let issue = store.issue(project.id, link.issue_id, None).await.unwrap();
  let edge = store
    .default_issue_workflow(project.id)
    .await
    .unwrap()
    .workflow
    .definition
    .transitions[0]
    .clone();
  let effects = octacity_server_factory::WorkSourceStatusEffects {
    status: edge.to.to_string(),
    add: vec!["native-ready".into()],
    remove: vec![],
    state: Some("native-state".into()),
  };
  let mut publication = ownership(&issue, &imported, "push-mapping", 4);
  publication.intent.definition.fields = [(IssueField::Status, IssueSourceFieldAuthority::OctacityOwnedPush)].into();
  publication.intent.definition.reporter = Some(octacity_server_factory::WorkSourceBinding {
    capability: exact("writer"),
    ..imported.selection.source.clone()
  });
  publication.intent.definition.push_enabled = true;
  publication.intent.definition.status = Some(IssueSourceStatusMapping {
    workflow: IssueWorkflowPin {
      id: issue.workflow_id,
      version: issue.workflow_version,
    },
    pull: vec![],
    push: Some(octacity_server_factory::WorkSourceStatusMapping {
      statuses: vec![effects.clone()],
    }),
  });
  publication.writable = Some(octacity_server_factory::WorkSourceReportingPolicy {
    labels: vec!["native-ready".into()],
    states: vec!["native-state".into()],
  });
  let mapping = store
    .publish_issue_source_mapping(management_mutation(publication))
    .await
    .unwrap()
    .mapping;
  let command = TransitionIssue::manual(
    TransitionIssueIntent {
      project_id: project.id,
      id: issue.id,
      expected_version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      expected_status_id: issue.status_id,
      transition_id: edge.id,
      reason: None,
    },
    key("push-transition"),
    at(5),
  );
  let moved = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  let records = outbox(&database, issue.id, "issue.transitioned").await;
  let push = &records[0]["push_intent"];
  assert_eq!(push["mapping"], serde_json::json!(mapping.reference()));
  assert_eq!(push["version"], serde_json::json!(moved.issue.version));
  assert_eq!(push["desired"]["link"], serde_json::json!(link));
  assert_eq!(push["desired"]["effects"], serde_json::json!(effects));
  assert_eq!(push["desired"]["command"]["idempotency_key"], "push-transition");
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted
      .transition_issue(management_mutation(command))
      .await
      .unwrap()
      .issue,
    moved.issue
  );
  assert_eq!(outbox(&database, issue.id, "issue.transitioned").await, records);
  database.cleanup().await;
}
