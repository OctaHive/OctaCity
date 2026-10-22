//! Hermetic composed qualification: management -> Source -> Factory action -> Reporter -> Source.
//! Provider names below identify contract peers, not native GitLab/Jira implementations.
#![cfg(unix)]
use octacity_server::*;
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::NetworkHost;
use octacity_server_factory::*;
use octacity_server_store::{testing::InMemoryFactoryConfigurationStore, *};
use octacity_work_source_protocol::{ConfigurationReference, CredentialClass, CredentialHandle, ResourceAccess};
use std::{
  collections::BTreeSet,
  sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
  },
  time::Duration,
};
use tokio_util::sync::CancellationToken;
const TEST_WAIT: Duration = Duration::from_secs(30);
#[allow(dead_code)]
mod admission_fixture {
  use super::*;
  include!("../../application/src/factory_admission_test_fixture.rs");
}

use admission_fixture::*;
#[allow(dead_code)]
#[path = "support/work_source.rs"]
mod source_support;
use source_support::*;
#[allow(dead_code)]
#[path = "support/work_reporter.rs"]
mod reporter_support;
use reporter_support::*;
#[path = "support/work_source_pilot.rs"]
mod setup;
use setup::*;
#[path = "factory_work_source_pilot/issues.rs"]
mod issues;
#[path = "support/issue_review_reader.rs"]
mod review_reader;

#[tokio::test]
async fn composed_pilot_projects_assessment_labels_then_admits_downstream_once() {
  for (provider, labels) in [
    ("github", ["approved", "rejected"]),
    ("gitlab", ["ready-for-review", "archived"]),
    ("jira", ["editorial-ready", "needs-context"]),
    ("custom-tracker", ["green", "red"]),
  ] {
    for with_consumer in [false, true] {
      let Pilot {
        fixture,
        profile,
        connection,
        downstream_id,
        publisher,
      } = pipeline(provider, &labels).await;
      let directory = fixture.root.path().join(provider);
      std::fs::write(
        directory.join("reporter-mode.json"),
        serde_json::to_vec(&serde_json::json!({"source_effects":true,"lose_project_response":with_consumer})).unwrap(),
      )
      .unwrap();
      let worker = fixture
        .worker()
        .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
      assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
      assert_eq!(fixture.runs().await.len(), 1, "downstream waits for configured effects");
      let run = fixture.runs().await[0].id;
      let projector = managed_projector(&fixture, &profile);
      accept_status_at(&fixture, projector.clone(), run, 100).await;
      let accepted = fixture.store.factory_run_snapshot(run).await.unwrap();
      let first = reporting_worker(&fixture, projector, "pilot-reporter", 10000)
        .run_once(time(120))
        .await
        .unwrap();
      if with_consumer {
        assert_eq!(first.retrying, 1, "applied labels with a lost response retain a retry");
        let metadata = fixture.store.integration_connection(connection.id, None).await.unwrap();
        let writer = metadata.definition.resources[0]
          .capabilities
          .iter()
          .find(|c| c.purpose == IntegrationPurpose::TaskUpdate)
          .unwrap();
        let rotate = BeginIntegrationAdministration {
          intent: IntegrationAdministrationIntent {
            id: connection.id,
            expected_metadata_version: connection.version,
            expected_revision: 2,
            action: IntegrationAdministrationAction::Credential {
              reference: writer.credential.clone(),
              kind: CredentialPublicationKind::Rotate,
              activate_connection: true,
            },
          },
          idempotency_key: IdempotencyKey::new("pilot-rotate-before-retry").unwrap(),
          requested_at: time(130),
        };
        let receipt = credential(&fixture, publisher.clone(), rotate.clone()).await.unwrap();
        assert_eq!(receipt.revision, 3);
        fixture.clock.0.store(220, Ordering::SeqCst);
        let restarted = managed_projector(&fixture, &profile);
        assert_eq!(
          reporting_worker(&fixture, restarted, "pilot-restarted", 10000)
            .run_once(time(220))
            .await
            .unwrap()
            .delivered,
          1
        );
        assert_eq!(credential(&fixture, publisher, rotate).await.unwrap(), receipt);
        let calls = captured_requests(&directory);
        let writes = calls
          .lines()
          .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
          .filter(|c| c["message"]["command"]["operation"] == "apply")
          .count();
        assert_eq!(writes, 1, "restart observes applied effects instead of reapplying");
      } else {
        assert_eq!(first.delivered, 1);
      }
      fixture.clock.0.store(2100, Ordering::SeqCst);
      assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
      assert_eq!(
        fixture.runs().await.len(),
        2,
        "projected labels must select the distinct downstream input"
      );
      let runs = fixture.runs().await;
      let mut frozen_runs = Vec::new();
      for retained in &runs {
        frozen_runs.push(fixture.store.factory_run_snapshot(retained.id).await.unwrap());
      }
      let after = fixture.store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(after.work, accepted.work);
      assert_eq!(after.flow, accepted.flow);
      let downstream_run = runs.iter().find(|r| r.id != run).unwrap();
      let preparation = fixture
        .store
        .source_preparation_for_work(fixture.input.project_id(), downstream_run.work_id)
        .await
        .unwrap()
        .unwrap();
      assert_eq!(preparation.input.id(), downstream_id);
      assert_ne!(preparation.input.definition().target, fixture.input.definition().target);
      assert_eq!(
        preparation.input.definition().connection,
        fixture.input.definition().connection
      );
      let source_calls = std::fs::read(directory.join("requests.jsonl")).unwrap();
      let reader = Arc::new(observation_reader(&fixture, &connection));
      assert_eq!(
        std::fs::read(directory.join("requests.jsonl")).unwrap(),
        source_calls,
        "installing optional observation support performs no read"
      );
      let card = if with_consumer {
        Some(issues::card(&fixture, reader.clone(), &runs).await)
      } else {
        None
      };
      if with_consumer {
        // Observe the same native task independently of either execution input.
        let mut task_consumer = reader
          .read(&fixture.profile.read.context, observation_request(&fixture))
          .await
          .unwrap()
          .observations;
        let correlations = fixture
          .store
          .source_diagnostic_correlations(ReadWorkSourceCorrelations {
            project_id: fixture.input.project_id(),
            filter: WorkSourceDiagnosticFilter::Inputs(vec![fixture.input.id(), downstream_id]),
            visibility: WorkSourceConfigurationVisibility {
              configurations: FactoryConfigurationListVisibility::all(),
              repositories: RepositoryListVisibility::all(),
              connections: IntegrationConnectionListVisibility::all(),
            },
            runs: FactoryRunListVisibility::all(),
            after: None,
            limit: 2,
          })
          .await
          .unwrap();
        assert_eq!(correlations.items.len(), 2);
        for association in &correlations.items {
          assert_eq!(association.native_correlation, task_consumer[0].native_correlation);
        }
        assert_eq!(
          task_consumer[0]
            .snapshot
            .labels
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
          vec!["arbitrary".to_owned(), labels[0].to_owned()]
            .into_iter()
            .collect::<BTreeSet<_>>()
        );
        let path = directory.join("mode.json");
        let mut native: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        native["candidates"][0]["state"] = serde_json::json!("closed");
        native["candidates"][0]["body"] = serde_json::json!("Fresh edited task after both admissions");
        native["candidates"][0]["labels"] = serde_json::json!([labels[1]]);
        std::fs::write(path, serde_json::to_vec(&native).unwrap()).unwrap();
        fixture.clock.0.store(2200, Ordering::SeqCst);
        let fresh = reader
          .read(&fixture.profile.read.context, observation_request(&fixture))
          .await
          .unwrap()
          .observations
          .remove(0);
        assert_eq!(
          fresh.freshness_at(time(2200), Some(&task_consumer[0])),
          WorkSourceObservationFreshness::Fresh
        );
        assert_eq!(fresh.native_correlation, task_consumer[0].native_correlation);
        assert_eq!(
          fresh.snapshot.body.as_deref(),
          Some("Fresh edited task after both admissions")
        );
        assert!(
          !fixture
            .input
            .definition()
            .predicate
            .as_ref()
            .unwrap()
            .mismatches(&fresh.snapshot.state, &fresh.snapshot.labels)
            .is_empty()
        );
        task_consumer.push(fresh);
        assert_eq!(task_consumer.len(), 2);
      } else {
        assert_eq!(
          std::fs::read(directory.join("requests.jsonl")).unwrap(),
          source_calls,
          "no consumer means no extra Source read"
        );
      }
      let artifacts = fixture.bytes.0.lock().unwrap().clone();
      fixture.clock.0.store(4100, Ordering::SeqCst);
      let restarted = fixture
        .worker()
        .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
      assert_eq!(restarted.run_once(CancellationToken::new()).await.unwrap().completed, 2);
      assert_eq!(
        fixture.runs().await,
        runs,
        "restart, own labels and edited tasks cannot create more Runs"
      );
      assert_eq!(fixture.bytes.0.lock().unwrap().as_slice(), artifacts.as_slice());
      for frozen in frozen_runs {
        assert_eq!(
          fixture.store.factory_run_snapshot(frozen.run.id()).await.unwrap(),
          frozen,
          "observation/replay cannot rewrite either original Work/Run"
        );
      }
      // A real process-boundary dependency failure stays local to these inputs.
      let native_path = directory.join("mode.json");
      let mut native: serde_json::Value = serde_json::from_slice(&std::fs::read(&native_path).unwrap()).unwrap();
      native["failure"] = serde_json::json!({"class":"transient","retry_after_millis":100});
      std::fs::write(native_path, serde_json::to_vec(&native).unwrap()).unwrap();
      fixture.clock.0.store(5100, Ordering::SeqCst);
      assert_eq!(restarted.run_once(CancellationToken::new()).await.unwrap().deferred, 2);
      assert_eq!(fixture.runs().await, runs);
      // Revocation blocks both the execution-neutral reader and the two execution inputs.
      grant(
        &fixture,
        connection.id,
        Some(connection.access_version),
        vec![],
        "pilot-revoke",
      )
      .await;
      let before_reads = std::fs::read(directory.join("requests.jsonl")).unwrap();
      assert!(
        reader
          .read(&fixture.profile.read.context, observation_request(&fixture))
          .await
          .is_err()
      );
      fixture.clock.0.store(6100, Ordering::SeqCst);
      assert_eq!(restarted.run_once(CancellationToken::new()).await.unwrap().deferred, 2);
      assert_eq!(std::fs::read(directory.join("requests.jsonl")).unwrap(), before_reads);
      assert_eq!(fixture.runs().await, runs);
      let original = fixture
        .store
        .source_preparation_for_work(fixture.input.project_id(), accepted.work.id())
        .await
        .unwrap()
        .unwrap();
      let mut manual = original.intent.unwrap();
      manual.external_identity = ExternalWorkIdentity::new("manual-pilot-during-outage").unwrap();
      manual.idempotency_key = IdempotencyKey::new("manual-pilot-during-outage").unwrap();
      manual.admitted_at = time(6101);
      let manual = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(FactoryAdmissionHandlers::new(
          fixture.store.clone(),
          Arc::new(ExactRevisionResolver),
        )),
      )
      .handle_command(&fixture.profile.read.context, manual)
      .await
      .unwrap();
      assert!(
        fixture
          .store
          .source_preparation_for_work(fixture.input.project_id(), manual.work_envelope_id)
          .await
          .unwrap()
          .is_none()
      );
      if let Some((store, issue)) = card {
        assert_eq!(
          store.issue(issue.project_id, issue.id, None).await.unwrap(),
          issue,
          "outage, Reporter recovery and native edits preserve the reviewed card"
        );
        assert_eq!(
          store
            .issue_comments(issue.project_id, issue.id, None, 10)
            .await
            .unwrap()
            .items
            .len(),
          1
        );
        store
          .create_issue(octacity_server_store::testing::management_mutation(CreateIssue {
            id: IssueId::generate(),
            assignees: Default::default(),
            intent: CreateIssueIntent {
              project_id: issue.project_id,
              planning: Default::default(),
              content: IssueContent {
                title: "Manual during outage".into(),
                description: String::new(),
                acceptance: None,
                priority: None,
              },
            },
            idempotency_key: "manual-outage".parse().unwrap(),
            created_at: time(6101),
          }))
          .await
          .unwrap();
      }
      assert_eq!(fixture.runs().await.len(), 3);
      assert_eq!(std::fs::read(directory.join("requests.jsonl")).unwrap(), before_reads);
    }
  }
}

#[tokio::test]
async fn composed_pilot_rejects_excluded_tasks_and_retains_ambiguity_without_a_run() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let labels = ["editorial-ready", "needs-context"];
    let Pilot {
      mut fixture,
      profile,
      connection,
      downstream_id,
      ..
    } = pipeline(provider, &labels).await;
    // Same logical action, different configured assessment result; no status spelling selects effects.
    let mut assessment = fixture.input.definition().clone();
    assessment.reporting.as_mut().unwrap().statuses[0].add = vec![labels[1].into()];
    assessment.reporting.as_mut().unwrap().statuses[0].remove = vec![labels[0].into()];
    fixture.input = publish(
      &fixture,
      &profile,
      &labels,
      vec![WorkSourceInputIntent {
        id: fixture.input.id(),
        expected_version: Some(fixture.input.version()),
        definition: assessment,
      }],
      None,
      "pilot-rejected-outcome",
    )
    .await
    .publication
    .inputs
    .remove(0)
    .input;
    let directory = fixture.root.path().join(provider);
    std::fs::write(directory.join("reporter-mode.json"), br#"{"source_effects":true}"#).unwrap();
    let worker = fixture
      .worker()
      .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    let run = fixture.runs().await[0].id;
    let projector = managed_projector(&fixture, &profile);
    accept_status_at(&fixture, projector.clone(), run, 100).await;
    assert_eq!(
      reporting_worker(&fixture, projector, "pilot-rejected-reporter", 10000)
        .run_once(time(120))
        .await
        .unwrap()
        .delivered,
      1
    );
    let observed = observation_reader(&fixture, &connection)
      .read(&fixture.profile.read.context, observation_request(&fixture))
      .await
      .unwrap()
      .observations
      .remove(0);
    assert!(observed.snapshot.labels.contains(&labels[1].to_owned()));
    assert!(!observed.snapshot.labels.contains(&labels[0].to_owned()));
    assert!(
      observed.snapshot.labels.contains(&"arbitrary".to_owned()),
      "unmanaged labels survive rejection"
    );
    fixture.clock.0.store(2100, Ordering::SeqCst);
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    assert_eq!(
      fixture.runs().await.len(),
      1,
      "rejected result is excluded from downstream"
    );

    let downstream = fixture
      .store
      .source_input(
        fixture.input.project_id(),
        downstream_id,
        WorkSourceInputVersion::INITIAL,
      )
      .await
      .unwrap();
    let (group, overlapping) = overlapping_group(&fixture, &profile, &labels, &downstream).await;
    let path = directory.join("mode.json");
    let mut native: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    native["candidates"][0]["labels"] = serde_json::json!([labels[0], "operator-label"]);
    std::fs::write(path, serde_json::to_vec(&native).unwrap()).unwrap();
    fixture.clock.0.store(4100, Ordering::SeqCst);
    let restarted = fixture
      .worker()
      .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
    assert_eq!(restarted.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    assert_eq!(
      fixture.runs().await.len(),
      1,
      "ambiguous alternatives cannot select a pipeline"
    );
    let diagnostics = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(handlers(&fixture, &profile, &labels)),
    )
    .handle_query(
      &fixture.profile.read.context,
      GetWorkSourceDiagnosticsQuery {
        project_id: fixture.input.project_id(),
        target: WorkSourceScanTarget::Group {
          id: group.id(),
          version: group.version(),
        },
        after: None,
        limit: 2,
      },
    )
    .await
    .unwrap();
    let selection = diagnostics.scan.unwrap().selection.unwrap();
    assert_eq!(selection.matching_inputs.len(), 2);
    assert!(selection.matching_inputs.contains(&downstream_id));
    assert!(selection.matching_inputs.contains(&overlapping));
    assert!(diagnostics.admissions.is_empty());
    fixture.clock.0.store(6100, Ordering::SeqCst);
    assert_eq!(restarted.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    assert_eq!(fixture.runs().await.len(), 1);
  }
}

#[path = "factory_work_source_pilot/postgres.rs"]
mod postgres;

#[path = "factory_work_source_pilot/conflict.rs"]
mod conflict;
#[path = "support/pilot_controls.rs"]
mod controls;
