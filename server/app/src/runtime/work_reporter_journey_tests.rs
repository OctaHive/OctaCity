//! Public ordinary admission/action/outbox journey through the content-verified process boundary.
use super::*;
use crate::runtime::*;
use octacity_server_factory::NetworkHost;
use octacity_work_source_protocol::{ConfigurationReference, CredentialClass, CredentialHandle, ResourceAccess};
#[path = "work_reporter_configuration_tests.rs"]
mod configuration;
#[path = "work_reporter_deadline_tests.rs"]
mod deadline;
#[path = "issue_reporter_tests.rs"]
mod issues;
#[path = "work_reporter_ordering_tests.rs"]
mod ordering;
#[path = "work_reporter_recovery_tests.rs"]
mod recovery;

// Call from a start_paused test. Tokio documents that a blocking task inhibits
// automatic clock advancement; block_on still runs real PostgreSQL/process I/O.
// Each test owns its runtime and blocking task, without a lock or a busy loop.
async fn controlled_io<F>(future: F) -> F::Output
where
  F: std::future::Future + Send + 'static,
  F::Output: Send + 'static,
{
  let runtime = tokio::runtime::Handle::current();
  tokio::task::spawn_blocking(move || runtime.block_on(future))
    .await
    .unwrap()
}
// External process barriers use a real watchdog, independent of the frozen clock.
async fn wait_for_provider(mut ready: impl FnMut() -> bool + Send + 'static) {
  tokio::task::spawn_blocking(move || {
    let started = std::time::Instant::now();
    while !ready() {
      assert!(started.elapsed() < TEST_WAIT, "provider did not reach the test barrier");
      std::thread::sleep(Duration::from_millis(5));
    }
  })
  .await
  .unwrap();
}

#[allow(dead_code)]
#[path = "../../tests/support/work_reporter.rs"]
mod reporter_support;
use reporter_support::*;

#[tokio::test]
async fn reporter_leaves_other_capabilities_unclaimed() {
  let (fixture, profile) = fixture("opaque-reporter").await;
  reporter_ownership_contract(fixture, profile).await;
}

async fn reporter_ownership_contract<
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + IssueSourceProjectionStore + 'static,
>(
  fixture: Fixture<S>,
  profile: FactoryReporterProfile,
) {
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let unrelated = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(
      exact("unrelated-capability", 95),
      projector.clone() as Arc<dyn FactoryWorkStatusProjector>,
    )]),
    FactoryReporterWorkerSettings::new(key("unrelated-owner"), 10000, 1).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(unrelated.run_once(time(120)).await.unwrap().claimed, 0);
  let untouched = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(untouched.outbox, before.outbox);
  assert!(captured_requests(&fixture.root.path().join("opaque-reporter")).is_empty());
  assert_eq!(
    reporting_worker(&fixture, projector, "rightful-owner", 10000)
      .run_once(time(120))
      .await
      .unwrap()
      .delivered,
    1
  );
}

#[tokio::test]
async fn accepted_source_status_reaches_only_its_original_verified_reporter_for_every_provider() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let (fixture, profile) = fixture(provider).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let operation = before
      .outbox
      .iter()
      .find(|row| row.kind.as_str() == "work.status.projection")
      .unwrap()
      .operation_id;
    let node = before
      .admitted_flow
      .closure()
      .definition(before.admitted_flow.closure().root())
      .unwrap()
      .node(&key("report-any-stage"))
      .unwrap();
    let pending = FactoryWorkStatusRequest {
      operation_id: operation,
      outbox_claim: None,
      work_id: before.work.id(),
      external_identity: before.work.external_identity().clone(),
      subject: before.work.subject().clone(),
      status: FactorySafeText::new("needs-human-eyes").unwrap(),
      permissions: node.permissions().clone(),
      budget: node.budget(),
      deadline: time(5000),
    };
    assert!(
      projector.set_status(pending).await.is_err(),
      "a pending outbox row alone cannot authorize a write"
    );
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("reporting"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    assert_eq!(worker.run_once(time(120)).await.unwrap().delivered, 1);
    let calls = std::fs::read_to_string(fixture.root.path().join(provider).join("reporter-requests.jsonl")).unwrap();
    let calls = calls
      .lines()
      .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
      .collect::<Vec<_>>();
    assert_eq!(calls.len(), 4);
    let operations: Vec<_> = calls
      .iter()
      .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
      .collect();
    assert_eq!(operations, ["inspect", "inspect", "apply", "inspect"]);
    for call in calls {
      let payload = original_intent(&call);
      assert_eq!(payload["projection_id"], operation.to_string());
      assert_eq!(payload["item_id"], "TASK-native");
      assert_eq!(payload["display_id"], "TASK-7");
      assert_eq!(payload["mapping"]["identity"], "original-effects");
      assert_eq!(payload["resource"]["credential"]["class"], "work_reporter_write");
      assert_eq!(payload["status"], "needs-human-eyes");
    }
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert_eq!(worker.run_once(time(121)).await.unwrap().claimed, 0);
  }
}

#[tokio::test]
async fn input_owned_reporting_keeps_original_effects_after_input_replacement_for_any_provider() {
  for provider in ["github", "custom-tracker"] {
    let (fixture, profile) = fixture_with_reporting(provider, None, true).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let original = fixture.input.definition();
    let target = fixture
      .store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: fixture.input.project_id(),
        configuration_id: original.target.id(),
        configuration_version: original.target.version(),
        repository_id: Some(original.repository.id),

        repository_version: Some(original.repository.version),
      })
      .await
      .unwrap();
    let source = WorkSourceCapability {
      project_id: fixture.input.project_id(),
      role: WorkSourceCapabilityRole::Source,
      binding: original.source.clone(),
    };
    let mut changed = original.clone();
    changed.reporting = None;
    changed.reporter = None;
    changed.enabled = false;
    let replacement = fixture
      .input
      .replace(
        WorkSourceInputVersion::new(3).unwrap(),
        changed,
        WorkSourceInputContext {
          target: &target.configuration.configuration,
          repository: &original.repository,
          source: Some(&source),
          reporter: None,
        },
      )
      .unwrap();
    fixture
      .store
      .publish_work_sources(PublishWorkSources {
        project_id: fixture.input.project_id(),
        inputs: vec![WorkSourceInputChange {
          expected_version: Some(fixture.input.version()),
          input: replacement,
        }],
        group: None,
      })
      .await
      .unwrap();
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("custom-reporting"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    assert_eq!(worker.run_once(time(120)).await.unwrap().delivered, 1);
    let calls = captured_requests(&fixture.root.path().join(provider));
    assert!(!calls.is_empty());
    for call in calls.lines() {
      let value: serde_json::Value = serde_json::from_str(call).unwrap();
      assert_eq!(
        original_intent(&value)["effects"],
        serde_json::json!({"add":["custom-ready"],"remove":[],"state":null})
      );
    }
    assert!(
      fixture
        .store
        .factory_run_snapshot(run)
        .await
        .unwrap()
        .outbox
        .iter()
        .any(|o| o.kind.as_str() == "work.status.projection")
    );
  }
}

#[tokio::test]
async fn reporter_retry_lower_bounds_and_exhaustion_survive_restart_without_reopening_the_flow() {
  let (fixture, profile) = fixture("jira").await;
  let projector = hosted(&fixture, vec![profile.clone()]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let dir = fixture.root.path().join("jira");
  std::fs::write(
    dir.join("reporter-mode.json"),
    br#"{"failure":{"class":"transient","retry_after_millis":2000}}"#,
  )
  .unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("first-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
  let next = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert!(
    next
      .outbox
      .iter()
      .any(|row| row.state == FactoryOutboxState::Pending && row.attempt == 1 && row.available_at == time(2120)),
    "the provider lower bound must exceed the configured 100ms delay"
  );
  let projector = hosted(&fixture, vec![profile]);
  let restarted = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("restarted-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  fixture.clock.0.store(2119, Ordering::SeqCst);
  assert_eq!(restarted.run_once(time(2119)).await.unwrap().claimed, 0);
  fixture.clock.0.store(2120, Ordering::SeqCst);
  assert_eq!(restarted.run_once(time(2120)).await.unwrap().failed, 1);
  assert_eq!(restarted.run_once(time(2200)).await.unwrap().claimed, 0);
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  let calls = std::fs::read_to_string(dir.join("reporter-requests.jsonl")).unwrap();
  let calls = calls
    .lines()
    .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
    .collect::<Vec<_>>();
  assert_eq!(calls.len(), 2);
  assert_eq!(
    calls[0]["message"]["command"]["payload"]["projection_id"],
    calls[1]["message"]["command"]["payload"]["projection_id"]
  );
  assert!(!dir.join("reporter-applied").exists());
}

#[tokio::test]
async fn missing_action_authority_is_denied_before_exposing_a_write_handle_to_the_process() {
  for omitted in ["grant", "network", "plugin"] {
    let (fixture, profile) = fixture_with_authority("gitlab", Some(omitted)).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1, "{omitted}");
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert!(!fixture.root.path().join("gitlab/reporter-requests.jsonl").exists());
  }
}

#[tokio::test]
async fn disabling_or_replacing_the_current_input_never_redirects_an_accepted_projection() {
  let (mut fixture, profile) = fixture("custom-tracker").await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let target = fixture
    .store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: fixture.input.project_id(),
      configuration_id: fixture.input.definition().target.id(),
      configuration_version: fixture.input.definition().target.version(),
      repository_id: Some(fixture.input.definition().repository.id),

      repository_version: Some(fixture.input.definition().repository.version),
    })
    .await
    .unwrap();
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.read.binding.clone(),
  };
  let mut draft = fixture.input.definition().clone();
  draft.enabled = false;
  draft.reporter = None;
  let replacement = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(3).unwrap(),
      draft,
      WorkSourceInputContext {
        target: &target.configuration.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&source),
        reporter: None,
      },
    )
    .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: Some(fixture.input.version()),
        input: replacement.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  fixture.input = replacement;
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().delivered, 1);
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  let retained = fixture
    .store
    .source_preparation_for_work(fixture.input.project_id(), after.work.id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained.input.version(), WorkSourceInputVersion::new(2).unwrap());
  let calls = std::fs::read_to_string(fixture.root.path().join("custom-tracker/reporter-requests.jsonl")).unwrap();
  assert!(calls.lines().all(
    |line| original_intent(&serde_json::from_str::<serde_json::Value>(line).unwrap())["mapping"]["identity"]
      == "original-effects"
  ));
}

#[tokio::test]
async fn unavailable_reporter_is_local_and_manual_factory_intake_requires_no_reporter() {
  let (fixture, profile) = fixture("github").await;
  let initial = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, initial).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let unavailable = hosted(&fixture, vec![]);
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), unavailable as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
  assert!(!fixture.root.path().join("github/reporter-requests.jsonl").exists());
  let source = fixture
    .store
    .source_preparation_for_work(fixture.input.project_id(), before.work.id())
    .await
    .unwrap()
    .unwrap();
  let mut command = source.intent.unwrap();
  command.external_identity = ExternalWorkIdentity::new("manually-entered-task").unwrap();
  command.idempotency_key = IdempotencyKey::new("manual-during-reporter-outage").unwrap();
  command.admitted_at = time(121);
  let manual = FactoryAdmissionHandlers::new(fixture.store.clone(), Arc::new(ExactRevisionResolver))
    .execute_management_command(
      &fixture.profile.read.context,
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      command,
    )
    .await
    .unwrap();
  assert_ne!(manual.factory_run_id, run);
  assert!(
    fixture
      .store
      .source_preparation_for_work(fixture.input.project_id(), manual.work_envelope_id)
      .await
      .unwrap()
      .is_none()
  );
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  assert!(!fixture.root.path().join("github/reporter-requests.jsonl").exists());
}

#[tokio::test]
async fn reporter_timeout_and_original_action_deadline_do_not_authorize_a_fresh_execution_window() {
  let (fixture, profile) = fixture("jira").await;
  let projector = Arc::new(
    HostedFactoryWorkStatusProjector::new(
      fixture.store.clone(),
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![profile],
      fixture.clock.clone(),
      Duration::from_millis(100),
      Duration::from_millis(20),
      CancellationToken::new(),
    )
    .unwrap(),
  );
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let dir = fixture.root.path().join("jira");
  std::fs::write(dir.join("reporter-mode.json"), br#"{"delay":5}"#).unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  let result = tokio::time::timeout(Duration::from_secs(2), worker.run_once(time(120)))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(result.retrying, 1);
  // A deadline may expire before a loaded host starts Python; zero requests
  // and one incomplete observation both forbid writes and a later restart.
  let captured = captured_requests(&dir);
  assert!(captured.lines().count() <= 1);
  fixture.clock.0.store(6000, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(6000)).await.unwrap().failed, 1);
  assert_eq!(
    captured_requests(&dir),
    captured,
    "expiry must be checked before another process starts"
  );
  assert!(!dir.join("reporter-applied").exists());
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
}

#[tokio::test]
async fn an_observation_that_exhausts_the_action_output_budget_cannot_start_a_write() {
  let (fixture, profile) = fixture_with_authority("jira", Some("output")).await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1);
  assert!(
    !fixture.root.path().join("jira/reporter-applied").exists(),
    "an exhausted observation budget cannot authorize projection"
  );
  assert_eq!(before.flow, fixture.store.factory_run_snapshot(run).await.unwrap().flow);
}

#[tokio::test]
async fn managed_reporter_revocation_blocks_retained_retry_without_rewriting_work_or_flow() {
  let (fixture, profile) = fixture("custom-tracker").await;
  let (access, id) = managed_access(&fixture, profile.binding(), IntegrationPurpose::TaskUpdate).await;
  let profile = profile.with_managed_access(access);
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let dir = fixture.root.path().join("custom-tracker");
  std::fs::write(
    dir.join("reporter-mode.json"),
    br#"{"failure":{"class":"transient","retry_after_millis":200}}"#,
  )
  .unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("managed-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
  let captured = std::fs::read(dir.join("reporter-requests.jsonl")).unwrap();
  fixture
    .store
    .publish_integration_project_access(octacity_server_store::testing::management_mutation(
      PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          connection_version: IntegrationVersion::INITIAL,
          project_id: fixture.input.project_id(),
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![],
        },
        idempotency_key: IdempotencyKey::new("revoke-managed-write").unwrap(),
        published_at: time(200),
      },
    ))
    .await
    .unwrap();
  std::fs::write(dir.join("reporter-mode.json"), b"{}").unwrap();
  fixture.clock.0.store(320, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(320)).await.unwrap().claimed, 1);
  assert_eq!(std::fs::read(dir.join("reporter-requests.jsonl")).unwrap(), captured);
  assert!(!dir.join("reporter-applied").exists());
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(after.work, before.work);
  assert_eq!(after.run, before.run);
  assert_eq!(after.flow, before.flow);
}

#[tokio::test]
async fn source_reporting_diagnostics_distinguishes_queued_applied_and_exhausted_without_dispatch() {
  for exhausted in [false, true] {
    let (fixture, profile) = fixture("custom-tracker").await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let queries = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(
        FactorySourceConfigurationHandlers::new(
          fixture.store.clone(),
          Arc::new(TrustedNetworkManagementPolicy),
          vec![],
        )
        .unwrap(),
      ),
    );
    let query = GetWorkSourceDiagnosticsQuery {
      project_id: fixture.input.project_id(),
      target: WorkSourceScanTarget::Input {
        id: fixture.input.id(),
        version: fixture.input.version(),
      },
      after: None,
      limit: 1,
    };
    let pending = queries
      .handle_query(&fixture.profile.read.context, query.clone())
      .await
      .unwrap();
    assert_eq!(pending.admissions[0].association.run_id, run);
    assert_eq!(pending.admissions[0].reporting.len(), 1);
    let progress = serde_json::to_value(&pending.admissions[0].reporting[0]).unwrap();
    assert_eq!(progress["origin"]["action"]["node"], "report-any-stage");
    assert_eq!(progress["origin"]["action"]["outcome"], "done");
    assert_eq!(progress["origin"]["stages"], serde_json::json!([]));
    assert_eq!(
      pending.admissions[0].reporting[0].state,
      WorkSourceReportingState::Pending
    );
    assert!(
      !fixture
        .root
        .path()
        .join("custom-tracker/reporter-requests.jsonl")
        .exists()
    );
    if exhausted {
      std::fs::write(
        fixture.root.path().join("custom-tracker/reporter-mode.json"),
        br#"{"failure":{"class":"permanent","retry_after_millis":null}}"#,
      )
      .unwrap();
    }
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("diagnostics-reporter"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    worker.run_once(time(120)).await.unwrap();
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let calls = std::fs::read(fixture.root.path().join("custom-tracker/reporter-requests.jsonl")).unwrap();
    let result = queries
      .handle_query(&fixture.profile.read.context, query)
      .await
      .unwrap();
    assert_eq!(result.admissions[0].reporting.len(), 1);
    assert_eq!(
      result.admissions[0].reporting[0].state,
      if exhausted {
        WorkSourceReportingState::Exhausted
      } else {
        WorkSourceReportingState::Applied
      }
    );
    assert_eq!(fixture.store.factory_run_snapshot(run).await.unwrap(), before);
    assert_eq!(
      std::fs::read(fixture.root.path().join("custom-tracker/reporter-requests.jsonl")).unwrap(),
      calls
    );
  }
}

#[tokio::test]
async fn stage_owned_effects_override_input_mapping_and_keep_original_target_after_replacement() {
  struct LaterCardAssociation;
  #[async_trait::async_trait]
  impl FactoryWorkSourceIssueLinks for LaterCardAssociation {
    async fn visible_issue(
      &self,
      _: &ManagementRequestContext,
      project: ProjectId,
      _: FactoryDigest,
    ) -> Result<Option<WorkSourceIssueLink>, ApplicationError> {
      Ok(Some(WorkSourceIssueLink {
        project_id: project,
        issue_id: ManagementResourceIdentity::new("card-linked-after-action").unwrap(),
      }))
    }
  }
  for provider in ["github", "custom-tracker"] {
    let (mut fixture, profile) = fixture_with_reporting(provider, None, true).await;
    let original = fixture.input.definition().clone();
    let update = serde_json::json!({
      "source":original.source,"reporter":profile.binding(),"connection":null,
      "effects":{"status":"needs-human-eyes","add":[],"remove":["custom-ready"],"state":null}
    });
    let configured = flow(provider);
    let root = configured.closure.definition(configured.closure.root()).unwrap();
    let mut nodes = root.nodes().to_vec();
    nodes[0] = nodes[0].clone().with_action(FlowActionBinding::new(capability(), key("set_status"),
      serde_json::json!({"status":"needs-human-eyes","outbox":{"max_attempts":2,"retry_delay_millis":100},"source_update":update})
    ).unwrap()).unwrap();
    let root = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes,
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    })
    .unwrap();
    let configured = FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap(),
      configured.limits,
      configured.data_schemas,
      configured.pools,
    )
    .unwrap();
    let config = configuration_with_flow(
      fixture.input.project_id(),
      FactoryConfigurationId::generate(),
      Some(configured),
    );
    fixture
      .store
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration: config.clone(),
        published_at: time(2),
      })
      .unwrap();
    let source = WorkSourceCapability {
      project_id: fixture.input.project_id(),
      role: WorkSourceCapabilityRole::Source,
      binding: original.source.clone(),
    };
    let reporter = WorkSourceCapability {
      project_id: fixture.input.project_id(),
      role: WorkSourceCapabilityRole::Reporter,
      binding: profile.binding().clone(),
    };
    let mut draft = original.clone();
    draft.target = config.reference().clone();
    let input = fixture
      .input
      .replace(
        WorkSourceInputVersion::new(3).unwrap(),
        draft,
        WorkSourceInputContext {
          target: &config,
          repository: &original.repository,
          source: Some(&source),
          reporter: Some(&reporter),
        },
      )
      .unwrap();
    fixture
      .store
      .publish_work_sources(PublishWorkSources {
        project_id: fixture.input.project_id(),
        inputs: vec![WorkSourceInputChange {
          expected_version: Some(fixture.input.version()),
          input: input.clone(),
        }],
        group: None,
      })
      .await
      .unwrap();
    fixture.input = input;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let history = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(
        FactorySourceConfigurationHandlers::new(
          fixture.store.clone(),
          Arc::new(TrustedNetworkManagementPolicy),
          vec![],
        )
        .unwrap()
        .with_issue_links(Arc::new(LaterCardAssociation)),
      ),
    );
    let diagnostic = history
      .handle_query(
        &fixture.profile.read.context,
        GetWorkSourceDiagnosticsQuery {
          project_id: fixture.input.project_id(),
          target: WorkSourceScanTarget::Input {
            id: fixture.input.id(),
            version: fixture.input.version(),
          },
          after: None,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(
      diagnostic.admissions[0].issue.as_ref().unwrap().issue_id.as_str(),
      "card-linked-after-action"
    );
    let mut changed = fixture.input.definition().clone();
    changed.enabled = false;
    changed.reporter = None;
    changed.reporting = None;
    let replaced = fixture
      .input
      .replace(
        WorkSourceInputVersion::new(4).unwrap(),
        changed,
        WorkSourceInputContext {
          target: &config,
          repository: &original.repository,
          source: Some(&source),
          reporter: None,
        },
      )
      .unwrap();
    fixture
      .store
      .publish_work_sources(PublishWorkSources {
        project_id: fixture.input.project_id(),
        inputs: vec![WorkSourceInputChange {
          expected_version: Some(fixture.input.version()),
          input: replaced,
        }],
        group: None,
      })
      .await
      .unwrap();
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("stage-effects"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    assert_eq!(worker.run_once(time(120)).await.unwrap().delivered, 1);
    assert_eq!(
      fixture
        .store
        .factory_run_snapshot(run)
        .await
        .unwrap()
        .work
        .subject()
        .project_id(),
      fixture.input.project_id()
    );
    for call in captured_requests(&fixture.root.path().join(provider)).lines() {
      let value: serde_json::Value = serde_json::from_str(call).unwrap();
      let intent = original_intent(&value);
      assert_eq!(
        intent["effects"],
        serde_json::json!({"add":[],"remove":["custom-ready"],"state":null})
      );
      assert_eq!(intent["item_id"], "TASK-native");
      assert_eq!(intent["mapping"]["identity"], "original-effects");
    }
  }
}
