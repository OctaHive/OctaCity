//! Only sources consumed by the selected input mode require content permissions.
use super::*;

struct HideArtifact(ArtifactId);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for HideArtifact {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::Artifact
      && resource.identity().is_some_and(|id| id.as_str() == self.0.to_string())
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

fn worker(f: &Fixture<PostgresStore>, denied: ArtifactId) -> FactorySourceWorker<PostgresStore, Bytes> {
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(HideArtifact(denied));
  FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: f.store.clone(),
      bytes: f.bytes.clone(),
      project_inputs: None,
      revisions: Arc::new(ExactRevisionResolver),
      authorization: policy.clone(),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(f.root.path()).unwrap()),
      clock: f.clock.clone(),
    },
    FactorySourceInstallation {
      projects: vec![f.input.project_id()],
      profiles: vec![f.profile.clone()],
      settings: settings(),
    },
  )
  .unwrap()
  .with_issue_snapshots(Arc::new(IssueSourceSnapshotReader::new(f.store.clone(), policy)))
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_ignores_unused_completion_artifact_permissions() {
  for snapshot_mode in [true, false] {
    let database = TestDatabase::migrated().await;
    let mut f = Fixture::enabled_with_flow("github", snapshot_mode.then(snapshot_flow))
      .await
      .into_postgres(database.pool.clone())
      .await;
    if snapshot_mode {
      linked_issue(&f, "TASK-native", true).await;
    }
    let unused = ArtifactId::generate();
    f.profile.preparation.mapping.acceptance =
      SourceAcceptanceMapping::ProcessCompletion(SourceCompletionDocument::ProjectInput {
        project_id: f.input.project_id(),
        artifact: FactoryArtifactReference::new(unused, digest(71), 100).unwrap(),
      });
    let result = worker(&f, unused).run_once(CancellationToken::new()).await.unwrap();
    if snapshot_mode {
      assert_eq!(
        (result.completed, result.deferred),
        (1, 0),
        "canonical Issue acceptance must not depend on an unused completion Artifact"
      );
      let run = f.store.factory_run_snapshot(f.runs().await[0].id).await.unwrap();
      f.clock.0.store(110, Ordering::SeqCst);
      assert_eq!(
        worker(&f, unused)
          .run_once(CancellationToken::new())
          .await
          .unwrap()
          .completed,
        1
      );
      f.clock.0.store(120, Ordering::SeqCst);
      assert_eq!(
        worker(&f, run.work.artifacts().task().artifact_id())
          .run_once(CancellationToken::new())
          .await
          .unwrap()
          .deferred,
        1,
        "the actual frozen Work input still requires permission on replay"
      );
      assert_eq!(f.runs().await.len(), 1);
    } else {
      assert_eq!(
        (result.completed, result.deferred),
        (0, 1),
        "provider completion still requires independent Artifact permission"
      );
      assert!(f.runs().await.is_empty());
      assert!(f.bytes.0.lock().unwrap().is_empty());
    }
    database.cleanup().await;
  }
}
