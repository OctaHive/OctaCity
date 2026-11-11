//! Duplicate native observations and restarted workers retain the original admitted snapshot.
use super::*;

#[path = "issue_snapshot_publication_recovery_tests.rs"]
mod publication;

fn worker(f: &Fixture<PostgresStore>) -> FactorySourceWorker<PostgresStore, Bytes> {
  f.worker().with_issue_snapshots(Arc::new(IssueSourceSnapshotReader::new(
    f.store.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
  )))
}

async fn publish_input(f: &mut Fixture<PostgresStore>, target: &FactoryConfiguration) {
  let mut draft = f.input.definition().clone();
  draft.target = target.reference().clone();
  draft.predicate = Some(WorkSourcePredicate {
    state: None,
    label_presence: WorkSourceLabelPresence::Any,
    all_of: vec!["arbitrary".into()],
    any_of: vec![],
    none_of: vec![],
  });
  let capability = WorkSourceCapability {
    project_id: f.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: f.profile.read.binding.clone(),
  };
  let input = f
    .input
    .replace(
      WorkSourceInputVersion::new(f.input.version().get() + 1).unwrap(),
      draft,
      WorkSourceInputContext {
        target,
        repository: &f.input.definition().repository,
        source: Some(&capability),
        reporter: None,
      },
    )
    .unwrap();
  f.store
    .publish_work_sources(PublishWorkSources {
      project_id: f.input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: Some(f.input.version()),
        input: input.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  f.input = input;
}

fn labels(f: &Fixture<PostgresStore>, included: bool) {
  let path = f.root.path().join("github/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["candidates"][0]["labels"] = if included {
    serde_json::json!(["arbitrary"])
  } else {
    serde_json::json!([])
  };
  // A changed native update marker is still the same input/item identity.
  mode["candidates"][0]["update_identity"] = serde_json::json!(if included { "readded" } else { "removed" });
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
}

async fn replace_configuration(f: &mut Fixture<PostgresStore>) {
  let selected = f
    .store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: f.input.project_id(),
      configuration_id: f.input.definition().target.id(),
      configuration_version: f.input.definition().target.version(),
      repository_id: Some(f.input.definition().repository.id),
      repository_version: Some(f.input.definition().repository.version),
    })
    .await
    .unwrap();
  let original = selected.configuration.configuration;
  let policies = selected.project_policies.unwrap();
  let mut flow = snapshot_flow();
  // Relaxing a new consumer's prerequisite must not replace an admitted Incoming.
  flow
    .issue_snapshot
    .as_mut()
    .unwrap()
    .fields
    .get_mut(&key("priority"))
    .unwrap()
    .required = false;
  let draft = FactoryConfigurationDraft {
    flow: Some(flow),
    admission_policy: FactoryPolicySelection::Project {
      project_policy: policies.launch_policy.reference().clone(),
    },
    permission_ceiling: FactoryPolicySelection::Project {
      project_policy: policies.access_profile.reference().clone(),
    },
    wip_limits: original.wip_limits(),
    hard_budget: original.hard_budget(),
    enabled: true,
  };
  let replacement = original
    .replace(
      FactoryConfigurationVersion::new(original.reference().version().get() + 1).unwrap(),
      digest(89),
      draft.clone(),
      &FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
        references: vec![],
        build_configurations: vec![],
      })
      .unwrap(),
    )
    .unwrap();
  f.store
    .replace_factory_configuration(management_mutation(ReplaceFactoryConfiguration {
      expected_current_version: original.reference().version(),
      intent: FactoryConfigurationMutationIntent::Replace {
        id: original.reference().id(),
        expected_current_version: original.reference().version(),
        definition_digest: replacement.reference().definition_digest(),
        draft,
      },
      configuration: replacement.clone(),
      idempotency_key: IdempotencyKey::new("replace-snapshot-consumer").unwrap(),
      published_at: time(140),
    }))
    .await
    .unwrap();
  publish_input(f, &replacement).await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn automatic_issue_snapshot_duplicates_and_restart_keep_one_run_after_classification_label_and_target_edits() {
  let database = TestDatabase::migrated().await;
  let mut fixture = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let target = fixture
    .store
    .factory_configuration_version(
      fixture.input.definition().target.id(),
      fixture.input.definition().target.version(),
    )
    .await
    .unwrap()
    .configuration;
  publish_input(&mut fixture, &target).await;
  let issue = linked_issue(&fixture, "TASK-native", true).await;
  assert_eq!(
    worker(&fixture)
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let run = fixture.runs().await.remove(0);
  let original = fixture.store.factory_run_snapshot(run.id).await.unwrap();
  let association = fixture
    .store
    .issue_work_association(issue.project_id, original.work.id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(association.issue_id, issue.id);
  assert_eq!(association.requirements_version, Some(issue.version));
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let claim = fixture
    .store
    .source_scan_schedule(fixture.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert!(claim.failure.is_none());
  let writes = fixture.bytes.0.lock().unwrap().clone();

  let issue = fixture
    .store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        planning: IssuePlanning {
          size: Some(IssueSize::Large),
          ..issue.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: IdempotencyKey::new("change-admitted-size").unwrap(),
      edited_at: time(105),
    }))
    .await
    .unwrap()
    .issue;
  fixture
    .store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          priority: Some(IssuePriority::Choice("low".into())),
          ..issue.content
        },
      },
      idempotency_key: IdempotencyKey::new("change-admitted-priority").unwrap(),
      edited_at: time(106),
    }))
    .await
    .unwrap();

  for (at, included) in [(110, true), (120, false), (130, true)] {
    labels(&fixture, included);
    fixture.clock.0.store(at, Ordering::SeqCst);
    fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
    let outcome = worker(&fixture).run_once(CancellationToken::new()).await.unwrap();
    assert_eq!((outcome.completed, outcome.deferred), (1, 0));
    assert_eq!(
      fixture.runs().await.len(),
      1,
      "native echo and label churn must retain one Run"
    );
  }
  let identity = fixture.input.id();
  replace_configuration(&mut fixture).await;
  assert_eq!(fixture.input.id(), identity);
  fixture.clock.0.store(150, Ordering::SeqCst);
  fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
  assert_eq!(
    worker(&fixture)
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let runs = fixture.runs().await;
  assert_eq!(
    runs.len(),
    1,
    "Configuration replacement is not another automatic cycle"
  );
  assert_eq!(runs[0].id, run.id);
  let actual = fixture.store.factory_run_snapshot(run.id).await.unwrap();
  assert_eq!(actual.work, original.work);
  assert_eq!(actual.flow.data.incoming, original.flow.data.incoming);
  let current = fixture.store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.planning.size, Some(IssueSize::Large));
  assert_eq!(current.content.priority, Some(IssuePriority::Choice("low".into())));
  assert_eq!(
    fixture
      .store
      .native_issue_work(issue.project_id, issue.id, None, 10)
      .await
      .unwrap(),
    vec![association],
    "the canonical Issue keeps one association to its original revision"
  );
  assert_eq!(
    actual.flow.data.incoming[0].payload().value(),
    &serde_json::json!({
      "title":"Canonical TASK-native", "size":"small", "priority":"high"
    })
  );
  assert_eq!(
    *fixture.bytes.0.lock().unwrap(),
    writes,
    "replay publishes no new input bytes"
  );
  database.cleanup().await;
}
