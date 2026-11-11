//! Pending snapshots reconcile under one automatic identity; admitted input remains immutable.
use super::*;

#[path = "issue_snapshot_publication_fence_tests.rs"]
mod publication_fence;

struct RevisionPause(Notify);
#[async_trait::async_trait]
impl RevisionResolver for RevisionPause {
  async fn resolve(&self, _: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    self.0.notify_one();
    std::future::pending().await
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_pending_restart_refreshes_changed_requirements_and_classification() {
  for after_intent in [false, true] {
    let database = TestDatabase::migrated().await;
    let mut f = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
      .await
      .into_postgres(database.pool.clone())
      .await;
    let issue = linked_issue(&f, "TASK-native", true).await;
    let bytes = Arc::new(ByteAcknowledgementBarrier {
      bytes: f.bytes.clone(),
      calls: AtomicUsize::new(0),
      written: Notify::new(),
    });
    let vcs = Arc::new(RevisionPause(Notify::new()));
    let task = if after_intent {
      let paused = interrupted_worker(&f, f.bytes.clone(), vcs.clone());
      tokio::spawn(async move { paused.run_once(CancellationToken::new()).await })
    } else {
      let paused = interrupted_worker(&f, bytes.clone(), Arc::new(ExactRevisionResolver));
      tokio::spawn(async move { paused.run_once(CancellationToken::new()).await })
    };
    tokio::time::timeout(TEST_WAIT, async {
      if after_intent {
        vcs.0.notified().await
      } else {
        bytes.written.notified().await
      }
    })
    .await
    .unwrap();
    let target = WorkSourceScanTarget::Input {
      id: f.input.id(),
      version: f.input.version(),
    };
    let original = f
      .store
      .pending_source_preparations(f.input.project_id(), target, None, 2)
      .await
      .unwrap()
      .remove(0);
    assert_eq!(original.intent.is_some(), after_intent);
    let manifest = f
      .store
      .source_materialization(f.input.project_id(), original.id)
      .await
      .unwrap()
      .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let issue = revise_issue(&f, issue).await;
    let current = current_documents(&f, &original).await;
    assert_eq!(current.issue_snapshot().unwrap().version, issue.version);
    f.clock.0.store(60_101, Ordering::SeqCst);
    f.store = Arc::new(PostgresStore::new(database.pool.clone()));
    assert_eq!(worker(&f).run_once(CancellationToken::new()).await.unwrap().deferred, 0);
    let runs = f.runs().await;
    assert_eq!(
      runs.len(),
      1,
      "a changed unadmitted Issue must reconcile instead of waiting forever"
    );
    let run = f.store.factory_run_snapshot(runs[0].id).await.unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({
        "title":"Revised canonical task", "size":"large", "priority":"low"
      })
    );
    let retained = f
      .store
      .source_preparation(f.input.project_id(), original.id)
      .await
      .unwrap();
    assert_eq!(retained.input, original.input);
    assert_eq!(
      retained.issue_input.as_ref().unwrap().issue_snapshot().unwrap().version,
      issue.version
    );
    let current = f
      .store
      .source_materialization(f.input.project_id(), original.id)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(
      current.expires_at, manifest.expires_at,
      "refresh must not renew input retention"
    );
    let cleanup = f
      .store
      .expired_source_preparations(current.expires_at, None, 100)
      .await
      .unwrap();
    let cleanup = cleanup
      .iter()
      .find(|entry| entry.preparation_id == original.id)
      .unwrap();
    for object in manifest.objects.iter().chain(&current.objects) {
      assert!(
        cleanup.objects.contains(object),
        "replaced bytes must remain owned by cleanup"
      );
    }
    f.clock.0.store(60_201, Ordering::SeqCst);
    assert_eq!(
      worker(&f).run_once(CancellationToken::new()).await.unwrap().completed,
      1
    );
    assert_eq!(f.runs().await.len(), 1);
    database.cleanup().await;
  }
}

async fn revise_issue(f: &Fixture<PostgresStore>, issue: IssueRevision) -> IssueRevision {
  let issue = f
    .store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          title: "Revised canonical task".into(),
          priority: Some(IssuePriority::Choice("low".into())),
          ..issue.content
        },
      },
      idempotency_key: IdempotencyKey::new("change-pending-requirements").unwrap(),
      edited_at: time(105),
    }))
    .await
    .unwrap()
    .issue;
  f.store
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
      idempotency_key: IdempotencyKey::new("change-pending-size").unwrap(),
      edited_at: time(106),
    }))
    .await
    .unwrap()
    .issue
}

async fn current_documents(f: &Fixture<PostgresStore>, preparation: &WorkSourcePreparation) -> ProjectTextDocuments {
  let configuration = f
    .store
    .factory_configuration_version(f.input.definition().target.id(), f.input.definition().target.version())
    .await
    .unwrap();
  IssueSourceSnapshotReader::new(f.store.clone(), Arc::new(TrustedNetworkManagementPolicy))
    .resolve(
      &FactorySourceAuthorization {
        input: f.input.clone(),
        context: f.profile.read.context.clone(),
        capability: WorkSourceCapability {
          project_id: f.input.project_id(),
          role: WorkSourceCapabilityRole::Source,
          binding: f.profile.read.binding.clone(),
        },
        resource_identity: preparation.item.resource.clone(),
      },
      &preparation.input,
      &preparation.item,
      configuration.configuration.flow(),
      &f.profile.preparation.source,
    )
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_refresh_rejects_late_reservation_of_superseded_bytes() {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let PreparedInput {
    issue,
    claim,
    preparation: original,
    manifest,
  } = prepared_input(&f).await;
  let docs = original.issue_input.as_ref().unwrap();
  revise_issue(&f, issue).await;
  let change = RefreshSourceIssueInput {
    expected_snapshot: docs.snapshot_digest().unwrap().unwrap(),
    security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
    replacement: FreezeSourcePreparationPolicy {
      claim: claim.clone(),
      preparation_id: original.id,
      policy: original.policy.clone().unwrap(),
      issue_input: Some(current_documents(&f, &original).await),
      now: time(110),
    },
  };
  let refreshed = f.store.refresh_source_issue_input(change.clone()).await.unwrap();
  assert!(
    f.store
      .reserve_source_artifacts(ReserveSourceArtifacts {
        claim: claim.clone(),
        preparation_id: original.id,
        mapping_digest: manifest.mapping_digest,
        objects: manifest.objects.clone(),
        retention_millis: manifest.retention_millis,
        now: time(111),
      })
      .await
      .is_err(),
    "a stale writer must not reserve the old task under the fresh snapshot"
  );
  assert!(
    f.store.refresh_source_issue_input(change).await.is_err(),
    "the original seal is no longer current"
  );
  assert_eq!(
    f.store
      .source_preparation(f.input.project_id(), original.id)
      .await
      .unwrap(),
    refreshed
  );
  let cleanup = f
    .store
    .expired_source_preparations(manifest.expires_at, None, 100)
    .await
    .unwrap();
  assert!(
    manifest
      .objects
      .iter()
      .all(|object| cleanup[0].objects.contains(object))
  );
  database.cleanup().await;
}

struct PreparedInput {
  issue: IssueRevision,
  claim: WorkSourceScanClaim,
  preparation: WorkSourcePreparation,
  manifest: SourceMaterialization,
}

async fn prepared_input(f: &Fixture<PostgresStore>) -> PreparedInput {
  let issue = linked_issue(f, "TASK-native", true).await;
  let claim = f
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: f.input.project_id(),
      target: WorkSourceScanTarget::Input {
        id: f.input.id(),
        version: f.input.version(),
      },
      owner: key("refresh-contract"),
      now: time(100),
      until: time(1000),
    })
    .await
    .unwrap()
    .unwrap()
    .claim;
  let pending = f
    .store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: claim.clone(),
      input_id: f.input.id(),
      item: FactorySourceItem {
        resource: ExternalWorkIdentity::new("native-container").unwrap(),
        item: ExternalWorkIdentity::new("TASK-native").unwrap(),
      },
      snapshot: b"{}".to_vec(),
      now: time(100),
    })
    .await
    .unwrap();
  let docs = current_documents(f, &pending).await;
  FactoryWorkSourcePreparer::new(f.store.clone(), f.bytes.clone(), f.clock.clone())
    .prepare_issue_input(claim.clone(), pending.id, f.profile.preparation.clone(), &docs)
    .await
    .unwrap();
  let original = f
    .store
    .source_preparation(f.input.project_id(), pending.id)
    .await
    .unwrap();
  let manifest = f
    .store
    .source_materialization(f.input.project_id(), pending.id)
    .await
    .unwrap()
    .unwrap();
  PreparedInput {
    issue,
    claim,
    preparation: original,
    manifest,
  }
}
