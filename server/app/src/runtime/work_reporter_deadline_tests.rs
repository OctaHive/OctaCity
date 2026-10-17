//! Original Reporter deadlines include managed authorization at the public outbox boundary.
use super::*;
use std::{path::PathBuf, sync::atomic::AtomicBool, task::Poll};
use tokio::sync::Notify;

// Keep automatic clock advancement disabled while the test itself drives the
// runtime's timer driver. Each test owns its blocking task; no shared lock exists.
async fn paused_io<F: std::future::Future>(future: F) -> F::Output {
  let (release, held) = std::sync::mpsc::channel::<()>();
  let blocker = tokio::task::spawn_blocking(move || {
    let _ = held.recv();
  });
  let result = future.await;
  drop(release);
  blocker.await.unwrap();
  result
}

// Delay a store read after externally observed protocol progress, not an internal
// method-call count. All metadata and grants still come from the ordinary store.
struct DelayedGrants {
  store: Arc<InMemoryFactoryConfigurationStore>,
  directory: PathBuf,
  after_inspects: usize,
  pause: AtomicBool,
  entered: Notify,
  release: Notify,
}
#[async_trait::async_trait]
impl IntegrationProjectAccessStore for DelayedGrants {
  async fn publish_integration_project_access(
    &self,
    request: ManagementMutation<PublishIntegrationProjectAccess>,
  ) -> Result<IntegrationProjectAccessOutcome, StoreError> {
    self.store.publish_integration_project_access(request).await
  }
  async fn integration_project_access(
    &self,
    connection: IntegrationId,
    project: ProjectId,
  ) -> Result<Option<PublishedIntegrationProjectAccess>, StoreError> {
    let inspected = recovery::calls(&self.directory)
      .iter()
      .filter(|call| call["message"]["command"]["operation"] == "inspect")
      .count();
    if inspected >= self.after_inspects && self.pause.swap(false, Ordering::SeqCst) {
      self.entered.notify_one();
      self.release.notified().await;
    }
    self.store.integration_project_access(connection, project).await
  }
}
#[async_trait::async_trait]
impl IntegrationConnectionStore for DelayedGrants {
  async fn replay_integration_connection(
    &self,
    request: ManagementMutation<ReplayIntegrationConnection>,
  ) -> Result<Option<IntegrationConnectionMutationOutcome>, StoreError> {
    self.store.replay_integration_connection(request).await
  }
  async fn publish_integration_connection(
    &self,
    request: ManagementMutation<PublishIntegrationConnection>,
  ) -> Result<IntegrationConnectionMutationOutcome, StoreError> {
    self.store.publish_integration_connection(request).await
  }
  async fn integration_connection(
    &self,
    id: IntegrationId,
    version: Option<IntegrationVersion>,
  ) -> Result<PublishedIntegrationConnection, StoreError> {
    self.store.integration_connection(id, version).await
  }
}
#[async_trait::async_trait]
impl IntegrationAdministrationStore for DelayedGrants {
  async fn begin_integration_administration(
    &self,
    request: ManagementMutation<BeginIntegrationAdministration>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    self.store.begin_integration_administration(request).await
  }
  async fn complete_integration_administration(
    &self,
    request: ManagementMutation<IntegrationAdministrationReceipt>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    self.store.complete_integration_administration(request).await
  }
  async fn integration_administration(&self, id: IntegrationId) -> Result<IntegrationAdministrationState, StoreError> {
    self.store.integration_administration(id).await
  }
  async fn integration_preparation_cleanup(
    &self,
    request: ManagementMutation<IntegrationPreparationCleanup>,
  ) -> Result<Vec<CredentialPublication>, StoreError> {
    self.store.integration_preparation_cleanup(request).await
  }
  async fn acknowledge_integration_preparation_cleanup(
    &self,
    request: ManagementMutation<CredentialPublication>,
  ) -> Result<(), StoreError> {
    self.store.acknowledge_integration_preparation_cleanup(request).await
  }
}

async fn queued(
  provider: &str,
  after_inspects: usize,
) -> (
  Fixture,
  Arc<HostedFactoryWorkStatusProjector<InMemoryFactoryConfigurationStore>>,
  FactoryRunId,
  Arc<DelayedGrants>,
) {
  let (fixture, profile) = fixture(provider).await;
  let (_, id) = managed_access(&fixture, profile.binding(), IntegrationPurpose::TaskUpdate).await;
  let gate = Arc::new(DelayedGrants {
    store: fixture.store.clone(),
    directory: fixture.root.path().join(provider),
    after_inspects,
    pause: AtomicBool::new(true),
    entered: Notify::new(),
    release: Notify::new(),
  });
  let profile = profile.with_managed_access(ManagedIntegrationAccess::new(
    gate.clone(),
    id,
    IntegrationVersion::INITIAL,
  ));
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  (fixture, projector, run, gate)
}

#[tokio::test(start_paused = true)]
async fn managed_reporter_deadline_stops_stalled_write_authorization_without_provider_dispatch() {
  paused_io(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let (fixture, projector, run, gate) = queued(provider, 2).await;
      let before = fixture.store.factory_run_snapshot(run).await.unwrap();
      let worker = recovery::worker(&fixture, projector, "deadline", 10000);
      let mut dispatch = std::pin::pin!(worker.run_once(time(120)));
      tokio::select! {
        result = &mut dispatch => panic!("write authorization must reach its store barrier: {result:?}"),
        () = gate.entered.notified() => {},
      }
      let calls = captured_requests(&gate.directory);
      assert_eq!(
        recovery::calls(&gate.directory)
          .iter()
          .map(|c| c["message"]["command"]["operation"].as_str().unwrap())
          .collect::<Vec<_>>(),
        ["inspect", "inspect"]
      );
      // hosted() already bounds the whole projection to two seconds. No child
      // process is running at this store barrier; expiry must finish immediately.
      tokio::time::advance(Duration::from_millis(2001)).await;
      tokio::task::yield_now().await;
      let completed = std::future::poll_fn(|cx| Poll::Ready(std::future::Future::poll(dispatch.as_mut(), cx))).await;
      let Poll::Ready(result) = completed else {
        panic!("authorization remained pending beyond the original Reporter deadline");
      };
      assert_eq!(result.unwrap().retrying, 1);
      assert_eq!(captured_requests(&gate.directory), calls);
      assert!(!gate.directory.join("reporter-applied").exists());
      let after = fixture.store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(after.work, before.work);
      assert_eq!(after.run, before.run);
      assert_eq!(after.flow, before.flow);
    }
  })
  .await;
}

#[tokio::test(start_paused = true)]
async fn managed_reporter_deadline_passes_only_the_remaining_budget_to_the_provider() {
  paused_io(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let (fixture, projector, run, gate) = queued(provider, 1).await;
      let before = fixture.store.factory_run_snapshot(run).await.unwrap();
      // An already converged external state needs only the two safe observations.
      std::fs::write(
        gate.directory.join("reporter-state.json"),
        serde_json::to_vec(&serde_json::json!({"a".repeat(64):"original-effects"})).unwrap(),
      )
      .unwrap();
      let worker = recovery::worker(&fixture, projector, "remaining-budget", 10000);
      let mut dispatch = std::pin::pin!(worker.run_once(time(120)));
      tokio::select! {
        result = &mut dispatch => panic!("authorization must reach its store barrier: {result:?}"),
        () = gate.entered.notified() => {},
      }
      tokio::time::advance(Duration::from_millis(1500)).await;
      std::fs::write(gate.directory.join("reporter-mode.json"), br#"{"pause_observe":true}"#).unwrap();
      gate.release.notify_one();
      let directory = gate.directory.clone();
      tokio::select! {
        result = &mut dispatch => panic!("the second observation must reach the provider barrier: {result:?}"),
        () = wait_for_provider(move || directory.join("reporter-observing").exists()) => {},
      }
      // The second observation returns after the original two-second window,
      // but before a mistakenly renewed two-second provider window would end.
      tokio::time::advance(Duration::from_secs(1)).await;
      std::fs::write(gate.directory.join("reporter-release"), b"release").unwrap();
      let result = dispatch.await.unwrap();
      assert_eq!(
        (result.retrying, result.delivered),
        (1, 0),
        "late readiness cannot qualify delivery: {provider}"
      );
      assert!(!gate.directory.join("reporter-applied").exists());
      let after = fixture.store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(after.work, before.work);
      assert_eq!(after.run, before.run);
      assert_eq!(after.flow, before.flow);
    }
  })
  .await;
}
