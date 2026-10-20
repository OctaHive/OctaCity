//! Durable ordering at the existing source/action/store public boundary.
use super::*;
#[path = "work_reporter_postgres_tests.rs"]
pub(super) mod postgres;

#[tokio::test]
async fn accepted_projection_registers_opaque_fields_and_rejects_scope_or_fence_drift() {
  let (fixture, profile) = fixture("jira").await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let row = fixture
    .store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("ordering"), time(120), time(1120), 1)
        .unwrap()
        .only_kind(key("work.status.projection")),
    )
    .await
    .unwrap()
    .pop()
    .unwrap()
    .record;
  let mut request = SourceProjectionRequest {
    project_id: fixture.input.project_id(),
    work_id: Some(before.work.id()),
    operation_id: row.operation_id,
    claim: row.claim.unwrap(),
    observed_at: time(120),
    fields: vec![digest(1), digest(2)],
    action: SourceProjectionAction::Register,
  };
  assert_eq!(
    fixture
      .store
      .coordinate_source_projection(request.clone())
      .await
      .unwrap(),
    SourceProjectionState::Ready {
      fields: vec![digest(1), digest(2)],
      quiescent: true
    }
  );
  let mut changed = request.clone();
  changed.fields = vec![digest(3)];
  assert!(
    fixture.store.coordinate_source_projection(changed).await.is_err(),
    "an operation cannot replace its pinned managed fields"
  );
  let mut forged = request.clone();
  forged.claim = FactoryClaim::new(FactoryClaimFence::new(digest(90)), time(120), time(1120)).unwrap();
  assert!(fixture.store.coordinate_source_projection(forged).await.is_err());
  request.action = SourceProjectionAction::Begin {
    step_id: digest(77),
    fields: vec![digest(1)],
  };
  assert!(matches!(
    fixture
      .store
      .coordinate_source_projection(request.clone())
      .await
      .unwrap(),
    SourceProjectionState::Granted { .. }
  ));
  request.observed_at = time(1120);
  assert!(
    fixture
      .store
      .coordinate_source_projection(request.clone())
      .await
      .is_err(),
    "expired owners cannot dispatch more writes"
  );
  request.action = SourceProjectionAction::Finish {
    step_id: digest(77),
    uncertain: false,
  };
  assert_eq!(
    fixture.store.coordinate_source_projection(request).await.unwrap(),
    SourceProjectionState::Released,
    "a late known response can clear its own hazard without authorizing another write"
  );
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  assert_eq!(before.work, after.work);
}

async fn next_input<S: FactorySourceStore>(fixture: &mut Fixture<S>, reporter: &FactoryReporterProfile) {
  let target = fixture
    .store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: fixture.input.project_id(),
      configuration_id: fixture.input.definition().target.id(),
      configuration_version: fixture.input.definition().target.version(),
      repository_id: fixture.input.definition().repository.id,
      repository_version: fixture.input.definition().repository.version,
    })
    .await
    .unwrap();
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.read.binding.clone(),
  };
  let capability = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: reporter.binding().clone(),
  };
  let mut draft = fixture.input.definition().clone();
  draft.enabled = false;
  draft.reporter = None;
  let disabled = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(fixture.input.version().get() + 1).unwrap(),
      draft,
      WorkSourceInputContext {
        target: &target.configuration.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&source),
        reporter: None,
      },
    )
    .unwrap();
  let mut draft = fixture.input.definition().clone();
  draft.reporter = Some(reporter.binding().clone());
  draft.source.installation = reporter.binding().installation.clone();
  let next_source = WorkSourceCapability {
    binding: draft.source.clone(),
    ..source.clone()
  };
  let next = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    fixture.input.project_id(),
    draft,
    WorkSourceInputContext {
      target: &target.configuration.configuration,
      repository: &fixture.input.definition().repository,
      source: Some(&next_source),
      reporter: Some(&capability),
    },
  )
  .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![
        WorkSourceInputChange {
          expected_version: Some(fixture.input.version()),
          input: disabled,
        },
        WorkSourceInputChange {
          expected_version: None,
          input: next.clone(),
        },
      ],
      group: None,
    })
    .await
    .unwrap();
  fixture.profile.read.binding = next.definition().source.clone();
  fixture.profile.preparation.mapping.source = next.definition().source.clone();
  fixture.input = next;
}

#[tokio::test]
async fn newer_accepted_inputs_block_unknown_scopes_and_supersede_only_overlapping_fields() {
  for overlaps in [true, false] {
    let (mut fixture, profile) = fixture("gitlab").await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let old_run = queue(&fixture, projector.clone()).await;
    next_input(&mut fixture, &profile).await;
    let new_run = queue_at(&fixture, projector, 200).await;
    let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
    let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
    let rows = fixture
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(key("field-order"), time(220), time(1220), 4)
          .unwrap()
          .only_kind(key("work.status.projection")),
      )
      .await
      .unwrap();
    let original = rows
      .iter()
      .find(|r| r.record.subject.run_id() == Some(old_run))
      .unwrap();
    let newer = rows
      .iter()
      .find(|r| r.record.subject.run_id() == Some(new_run))
      .unwrap();
    let mut old_request = SourceProjectionRequest {
      project_id: fixture.input.project_id(),
      work_id: Some(old.work.id()),
      operation_id: original.record.operation_id,
      claim: original.record.claim.unwrap(),
      observed_at: time(220),
      fields: vec![digest(1)],
      action: SourceProjectionAction::Register,
    };
    assert_eq!(
      fixture
        .store
        .coordinate_source_projection(old_request.clone())
        .await
        .unwrap(),
      SourceProjectionState::Blocked,
      "an undispatched newer accepted action must already prevent older writes"
    );
    let field = if overlaps { digest(1) } else { digest(2) };
    let new_request = SourceProjectionRequest {
      project_id: fixture.input.project_id(),
      work_id: Some(new.work.id()),
      operation_id: newer.record.operation_id,
      claim: newer.record.claim.unwrap(),
      observed_at: time(220),
      fields: vec![field],
      action: SourceProjectionAction::Register,
    };
    assert_eq!(
      fixture.store.coordinate_source_projection(new_request).await.unwrap(),
      SourceProjectionState::Ready {
        fields: vec![field],
        quiescent: true
      }
    );
    let state = fixture
      .store
      .coordinate_source_projection(old_request.clone())
      .await
      .unwrap();
    if overlaps {
      assert_eq!(state, SourceProjectionState::Superseded);
    } else {
      assert_eq!(
        state,
        SourceProjectionState::Ready {
          fields: vec![digest(1)],
          quiescent: true
        }
      );
    }
    old_request.action = SourceProjectionAction::Begin {
      step_id: digest(50),
      fields: vec![digest(1)],
    };
    let state = fixture.store.coordinate_source_projection(old_request).await.unwrap();
    if overlaps {
      assert_eq!(state, SourceProjectionState::Superseded);
    } else {
      assert!(matches!(state, SourceProjectionState::Granted { .. }));
    }
    assert_eq!(
      old.flow,
      fixture.store.factory_run_snapshot(old_run).await.unwrap().flow
    );
    assert_eq!(
      new.flow,
      fixture.store.factory_run_snapshot(new_run).await.unwrap().flow
    );
    assert_eq!(fixture.runs().await.len(), 2);
  }
}

#[tokio::test]
async fn older_inflight_steps_require_known_completion_and_expired_uncertainty_is_conflict() {
  for uncertain in [false, true] {
    let (mut fixture, profile) = fixture("jira").await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let old_run = queue(&fixture, projector.clone()).await;
    let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
    let row = fixture
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(key("old-step"), time(120), time(1120), 1)
          .unwrap()
          .only_kind(key("work.status.projection")),
      )
      .await
      .unwrap()
      .pop()
      .unwrap()
      .record;
    let mut old_request = SourceProjectionRequest {
      project_id: fixture.input.project_id(),
      work_id: Some(old.work.id()),
      operation_id: row.operation_id,
      claim: row.claim.unwrap(),
      observed_at: time(120),
      fields: vec![digest(1)],
      action: SourceProjectionAction::Register,
    };
    fixture
      .store
      .coordinate_source_projection(old_request.clone())
      .await
      .unwrap();
    old_request.action = SourceProjectionAction::Begin {
      step_id: digest(70),
      fields: vec![digest(1)],
    };
    assert!(matches!(
      fixture
        .store
        .coordinate_source_projection(old_request.clone())
        .await
        .unwrap(),
      SourceProjectionState::Granted { .. }
    ));
    next_input(&mut fixture, &profile).await;
    let new_run = queue_at(&fixture, projector, 200).await;
    let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
    let row = fixture
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(key("new-step"), time(220), time(1220), 1)
          .unwrap()
          .only_kind(key("work.status.projection")),
      )
      .await
      .unwrap()
      .pop()
      .unwrap()
      .record;
    let mut new_request = SourceProjectionRequest {
      project_id: fixture.input.project_id(),
      work_id: Some(new.work.id()),
      operation_id: row.operation_id,
      claim: row.claim.unwrap(),
      observed_at: time(220),
      fields: vec![digest(1)],
      action: SourceProjectionAction::Register,
    };
    assert_eq!(
      fixture
        .store
        .coordinate_source_projection(new_request.clone())
        .await
        .unwrap(),
      SourceProjectionState::Ready {
        fields: vec![digest(1)],
        quiescent: false
      }
    );
    old_request.observed_at = time(220);
    assert_eq!(
      fixture
        .store
        .coordinate_source_projection(old_request.clone())
        .await
        .unwrap(),
      SourceProjectionState::Superseded,
      "old workers cannot start another step even before losing their own fence"
    );
    old_request.action = SourceProjectionAction::Finish {
      step_id: digest(70),
      uncertain,
    };
    fixture.store.coordinate_source_projection(old_request).await.unwrap();
    assert_eq!(
      fixture
        .store
        .coordinate_source_projection(new_request.clone())
        .await
        .unwrap(),
      SourceProjectionState::Ready {
        fields: vec![digest(1)],
        quiescent: !uncertain
      }
    );
    if uncertain {
      let rows = fixture
        .store
        .claim_factory_outbox(
          ClaimFactoryOutbox::new(key("restarted-ordering"), time(5112), time(6112), 4)
            .unwrap()
            .only_kind(key("work.status.projection")),
        )
        .await
        .unwrap();
      new_request.claim = rows
        .iter()
        .find(|r| r.record.subject.run_id() == Some(new_run))
        .unwrap()
        .record
        .claim
        .unwrap();
      new_request.observed_at = time(5112);
      assert!(
        matches!(
          fixture.store.coordinate_source_projection(new_request.clone()).await,
          Err(StoreError::Conflict { .. })
        ),
        "an expired unknown remote write is not proof of quiescence"
      );
      new_request.observed_at = time(5211);
      assert!(
        fixture.store.coordinate_source_projection(new_request).await.is_err(),
        "waiting for earlier hazards never renews the newer action deadline"
      );
    }
    assert_eq!(
      old.flow,
      fixture.store.factory_run_snapshot(old_run).await.unwrap().flow
    );
    assert_eq!(
      new.flow,
      fixture.store.factory_run_snapshot(new_run).await.unwrap().flow
    );
  }
}

#[tokio::test]
async fn completed_writes_invalidate_an_earlier_observation_even_after_the_lease_is_removed() {
  let (fixture, profile) = fixture("jira").await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector).await;
  let snapshot = fixture.store.factory_run_snapshot(run).await.unwrap();
  let row = fixture
    .store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("observation"), time(120), time(1120), 1)
        .unwrap()
        .only_kind(key("work.status.projection")),
    )
    .await
    .unwrap()
    .pop()
    .unwrap()
    .record;
  let mut request = SourceProjectionRequest {
    project_id: fixture.input.project_id(),
    work_id: Some(snapshot.work.id()),
    operation_id: row.operation_id,
    claim: row.claim.unwrap(),
    observed_at: time(120),
    fields: vec![digest(1)],
    action: SourceProjectionAction::Register,
  };
  fixture
    .store
    .coordinate_source_projection(request.clone())
    .await
    .unwrap();
  request.action = SourceProjectionAction::Observe { revision: None };
  let SourceProjectionState::Observation { revision, .. } = fixture
    .store
    .coordinate_source_projection(request.clone())
    .await
    .unwrap()
  else {
    panic!("expected observation fence")
  };
  request.action = SourceProjectionAction::Begin {
    step_id: digest(75),
    fields: vec![digest(1)],
  };
  fixture
    .store
    .coordinate_source_projection(request.clone())
    .await
    .unwrap();
  request.action = SourceProjectionAction::Finish {
    step_id: digest(75),
    uncertain: false,
  };
  fixture
    .store
    .coordinate_source_projection(request.clone())
    .await
    .unwrap();
  request.action = SourceProjectionAction::Observe {
    revision: Some(revision),
  };
  let SourceProjectionState::Observation {
    revision: next,
    stable,
    quiescent,
    ..
  } = fixture
    .store
    .coordinate_source_projection(request.clone())
    .await
    .unwrap()
  else {
    panic!("expected revised observation fence")
  };
  assert!(
    !stable,
    "removing an in-flight lease must not make an earlier read look current"
  );
  assert!(quiescent);
  assert_ne!(next, revision);
  request.action = SourceProjectionAction::Observe { revision: Some(next) };
  assert!(matches!(
    fixture.store.coordinate_source_projection(request).await.unwrap(),
    SourceProjectionState::Observation { stable: true, .. }
  ));
}

fn mapped_profile<S>(fixture: &Fixture<S>, mapping: &str) -> FactoryReporterProfile {
  mapped_installation_profile(
    fixture,
    mapping,
    fixture.profile.read.binding.installation.clone(),
    capability(),
  )
}
fn mapped_installation_profile<S>(
  fixture: &Fixture<S>,
  mapping: &str,
  installation: ImmutableReference,
  action_capability: ImmutableReference,
) -> FactoryReporterProfile {
  FactoryReporterProfile::new(
    key(mapping),
    key("v1"),
    FactoryReporterProfileDraft {
      issue_delivery: None,
      reporting_policy: None,
      project_id: fixture.input.project_id(),
      installation,
      capability: action_capability,
      resource: fixture.profile.read.binding.resource.clone(),
      plugin: fixture.profile.read.plugin.clone(),
      provider_configuration: FactoryDigest::from_bytes([0xdd; 32]),
      access: ResourceAccess {
        resource_id: "native-container".into(),
        resource_locator: "private-reporter-container".into(),
        credential: CredentialHandle {
          class: CredentialClass::WorkReporterWrite,
          handle: "private-reporter-grant".into(),
        },
      },
      mapping: ConfigurationReference {
        identity: mapping.into(),
        sha256: "c".repeat(64),
      },
      credential_profile: key("writer"),
      network_hosts: vec!["tracker.invalid".into()],
    },
  )
  .unwrap()
}
#[tokio::test]
async fn source_status_dispatch_skips_superseded_intents_across_provider_bindings() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let (mut fixture, original) = fixture(provider).await;
    let newer = mapped_profile(&fixture, "new-effects");
    let projector = hosted(&fixture, vec![original, newer.clone()]);
    let old_run = queue(&fixture, projector.clone()).await;
    next_input(&mut fixture, &newer).await;
    let new_run = queue_at(&fixture, projector.clone(), 200).await;
    let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
    let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
    let worker = super::recovery::worker(&fixture, projector, "ordered-dispatch", 10000);
    let first = worker.run_once(time(220)).await.unwrap();
    assert_eq!(
      (first.retrying, first.delivered),
      (1, 1),
      "the older action cannot write while newer accepted metadata is unresolved"
    );
    fixture.clock.0.store(320, Ordering::SeqCst);
    assert_eq!(
      worker.run_once(time(320)).await.unwrap().failed,
      1,
      "the older intent is explicitly superseded"
    );
    let calls = super::recovery::calls(&fixture.root.path().join(provider));
    let writes: Vec<_> = calls
      .iter()
      .filter(|c| c["message"]["command"]["operation"] == "apply" || c["message"]["command"]["operation"] == "project")
      .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(
      writes[0]["message"]["command"]["payload"]["intent"]["mapping"]["identity"],
      "new-effects"
    );
    assert_eq!(
      old.flow,
      fixture.store.factory_run_snapshot(old_run).await.unwrap().flow
    );
    assert_eq!(
      new.flow,
      fixture.store.factory_run_snapshot(new_run).await.unwrap().flow
    );
    assert_eq!(fixture.runs().await.len(), 2);
  }
}

async fn late_write_contract<
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + IssueSourceProjectionStore + 'static,
>(
  mut fixture: Fixture<S>,
  original: FactoryReporterProfile,
  lose_fence: bool,
  separate_installation: bool,
) {
  let newer = if separate_installation {
    mapped_installation_profile(&fixture, "new-effects", exact("another-installation", 99), capability())
  } else {
    mapped_profile(&fixture, "new-effects")
  };
  let projector = Arc::new(
    HostedFactoryWorkStatusProjector::new(
      fixture.store.clone(),
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![original, newer.clone()],
      fixture.clock.clone(),
      Duration::from_secs(10),
      Duration::from_millis(100),
      CancellationToken::new(),
    )
    .unwrap(),
  );
  let old_run = queue(&fixture, projector.clone()).await;
  let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
  // Keep source preparation/admission outside the in-flight Reporter window.
  // Only acceptance of the newer status action must follow the older Begin.
  next_input(&mut fixture, &newer).await;
  let new_run = admit_at(&fixture, 120).await;
  let directory = fixture
    .root
    .path()
    .join(fixture.profile.read.plugin.identity().as_str());
  std::fs::write(
    directory.join("reporter-state.json"),
    serde_json::to_vec(&serde_json::json!({"b".repeat(64):"human-owned"})).unwrap(),
  )
  .unwrap();
  std::fs::write(directory.join("reporter-mode.json"), br#"{"pause_before_apply":true}"#).unwrap();
  let old_worker = super::recovery::worker(
    &fixture,
    projector.clone(),
    "late-old",
    if lose_fence { 4000 } else { 10000 },
  );
  let old_task = tokio::spawn(async move { old_worker.run_once(time(120)).await });
  let ready = directory.join("reporter-before-apply");
  wait_for_provider(move || ready.exists()).await;
  std::fs::write(directory.join("reporter-mode.json"), b"{}").unwrap();
  accept_status_at(&fixture, projector.clone(), new_run, 4100).await;
  let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
  let new_worker = super::recovery::worker(&fixture, projector, "late-new", 10000);
  let new_task = tokio::spawn(async move { new_worker.run_once(time(4120)).await });
  let state_path = directory.join("reporter-state.json");
  wait_for_provider(move || {
    std::fs::read(&state_path)
      .ok()
      .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
      .is_some_and(|values| values["a".repeat(64)] == "new-effects")
  })
  .await;
  assert!(
    !new_task.is_finished(),
    "a matching read cannot confirm delivery while an older write is still in flight"
  );
  std::fs::write(directory.join("reporter-release-before"), b"apply-old-late").unwrap();
  let old_result = old_task.await.unwrap();
  if lose_fence {
    assert!(old_result.is_err());
  } else {
    assert_eq!(old_result.unwrap().failed, 1);
  }
  // The earlier Finish has committed. Wake the newer worker's 10ms hazard
  // poll exactly once; database/process scheduling consumes no virtual budget.
  tokio::time::advance(Duration::from_millis(10)).await;
  assert_eq!(
    new_task.await.unwrap().unwrap().delivered,
    1,
    "the newest accepted intent must be restored after a late old write"
  );
  let values: serde_json::Value =
    serde_json::from_slice(&std::fs::read(directory.join("reporter-state.json")).unwrap()).unwrap();
  assert_eq!(values["a".repeat(64)], "new-effects");
  assert_eq!(values["b".repeat(64)], "human-owned");
  let calls = super::recovery::calls(&directory);
  let writes: Vec<_> = calls
    .iter()
    .filter(|c| c["message"]["command"]["operation"] == "apply")
    .collect();
  assert_eq!(
    writes
      .iter()
      .filter(|c| c["message"]["command"]["payload"]["intent"]["mapping"]["identity"] == "original-effects")
      .count(),
    1
  );
  assert_eq!(
    writes
      .iter()
      .filter(|c| c["message"]["command"]["payload"]["intent"]["mapping"]["identity"] == "new-effects")
      .count(),
    2
  );
  assert_eq!(
    old.flow,
    fixture.store.factory_run_snapshot(old_run).await.unwrap().flow
  );
  assert_eq!(
    new.flow,
    fixture.store.factory_run_snapshot(new_run).await.unwrap().flow
  );
  assert_eq!(fixture.runs().await.len(), 2);
}
#[tokio::test(start_paused = true)]
async fn late_older_writes_are_reconciled_before_newest_delivery_with_or_without_fence_loss() {
  controlled_io(async move {
    for lose_fence in [false, true] {
      let (fixture, profile) = fixture("jira").await;
      late_write_contract(fixture, profile, lose_fence, false).await;
    }
  })
  .await;
}

#[tokio::test]
async fn independent_managed_fields_converge_without_superseding_another_inputs_effects() {
  let (mut fixture, original) = fixture("gitlab").await;
  let newer = mapped_profile(&fixture, "new-effects");
  let projector = hosted(&fixture, vec![original, newer.clone()]);
  let old_run = queue(&fixture, projector.clone()).await;
  next_input(&mut fixture, &newer).await;
  let new_run = queue_at(&fixture, projector.clone(), 200).await;
  let directory = fixture.root.path().join("gitlab");
  std::fs::write(
    directory.join("reporter-mode.json"),
    serde_json::to_vec(&serde_json::json!({"fieldsets":{"new-effects":["b".repeat(64)]}})).unwrap(),
  )
  .unwrap();
  let worker = super::recovery::worker(&fixture, projector, "independent-fields", 10000);
  let first = worker.run_once(time(220)).await.unwrap();
  assert_eq!((first.retrying, first.delivered), (1, 1));
  fixture.clock.0.store(320, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(320)).await.unwrap().delivered, 1);
  let values: serde_json::Value =
    serde_json::from_slice(&std::fs::read(directory.join("reporter-state.json")).unwrap()).unwrap();
  assert_eq!(values["a".repeat(64)], "original-effects");
  assert_eq!(values["b".repeat(64)], "new-effects");
  assert_eq!(
    fixture
      .store
      .factory_run_snapshot(old_run)
      .await
      .unwrap()
      .flow
      .data
      .records
      .len(),
    1
  );
  assert_eq!(
    fixture
      .store
      .factory_run_snapshot(new_run)
      .await
      .unwrap()
      .flow
      .data
      .records
      .len(),
    1
  );
}

#[tokio::test]
async fn human_conflict_and_continuous_edits_fail_bounded_reporting_without_changing_factory_history() {
  for (mode, paused_after_write) in [
    (br#"{"conflict_on_apply":true}"#.as_slice(), false),
    (br#"{"undo_after_apply":true}"#.as_slice(), false),
    (
      br#"{"undo_after_apply":true,"pause_after_apply":true}"#.as_slice(),
      true,
    ),
  ] {
    let (fixture, profile) = fixture("jira").await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let directory = fixture.root.path().join("jira");
    std::fs::write(directory.join("reporter-mode.json"), mode).unwrap();
    std::fs::write(
      directory.join("reporter-state.json"),
      serde_json::to_vec(&serde_json::json!({"b".repeat(64):"human-owned"})).unwrap(),
    )
    .unwrap();
    let worker = super::recovery::worker(&fixture, projector, "bounded-conflict", 10000);
    let mut summary = tokio::time::timeout(Duration::from_secs(5), worker.run_once(time(120)))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(summary.delivered, 0);
    if paused_after_write {
      assert_eq!(summary.retrying, 1, "lost confirmation uses the original finite policy");
    }
    let mut settled_at = 120;
    if summary.retrying == 1 {
      // A loaded host may exhaust the operation timeout before the output
      // budget. The published two-attempt policy still must end in failure.
      assert_eq!(worker.run_once(time(219)).await.unwrap().claimed, 0);
      fixture.clock.0.store(220, Ordering::SeqCst);
      summary = tokio::time::timeout(Duration::from_secs(5), worker.run_once(time(220)))
        .await
        .unwrap()
        .unwrap();
      settled_at = 220;
    }
    assert_eq!(summary.delivered, 0);
    assert_eq!(summary.failed, 1);
    let calls = super::recovery::calls(&directory);
    let writes = calls
      .iter()
      .filter(|c| c["message"]["command"]["operation"] == "apply")
      .count();
    assert!((1..=512).contains(&writes));
    if paused_after_write {
      assert_eq!(
        writes, 2,
        "each uncertain write consumes one of the two accepted attempts"
      );
    }
    assert_eq!(worker.run_once(time(settled_at + 1)).await.unwrap().claimed, 0);
    assert_eq!(
      super::recovery::calls(&directory),
      calls,
      "terminal conflict/exhaustion must not create another logical action"
    );
    let values: serde_json::Value =
      serde_json::from_slice(&std::fs::read(directory.join("reporter-state.json")).unwrap()).unwrap();
    assert_eq!(values["b".repeat(64)], "human-owned");
    assert_ne!(values["a".repeat(64)], "original-effects");
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert_eq!(before.work, after.work);
    assert_eq!(fixture.runs().await.len(), 1);
  }
}

async fn acceptance_order_contract<
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + IssueSourceProjectionStore + 'static,
>(
  mut fixture: Fixture<S>,
  profile: FactoryReporterProfile,
) {
  let projector = hosted(&fixture, vec![profile.clone()]);
  let old_run = queue_at(&fixture, projector.clone(), 200).await;
  next_input(&mut fixture, &profile).await;
  let new_run = queue_at(&fixture, projector, 100).await;
  let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
  let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
  let rows = fixture
    .store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("clock-skew"), time(220), time(1220), 4)
        .unwrap()
        .only_kind(key("work.status.projection")),
    )
    .await
    .unwrap();
  let old_row = rows
    .iter()
    .find(|r| r.record.subject.run_id() == Some(old_run))
    .unwrap();
  let new_row = rows
    .iter()
    .find(|r| r.record.subject.run_id() == Some(new_run))
    .unwrap();
  let old_request = SourceProjectionRequest {
    project_id: fixture.input.project_id(),
    work_id: Some(old.work.id()),
    operation_id: old_row.record.operation_id,
    claim: old_row.record.claim.unwrap(),
    observed_at: time(220),
    fields: vec![digest(1)],
    action: SourceProjectionAction::Register,
  };
  assert_eq!(
    fixture
      .store
      .coordinate_source_projection(old_request.clone())
      .await
      .unwrap(),
    SourceProjectionState::Blocked,
    "a later accepted action cannot lose authority because its clock moved backwards"
  );
  let newer = SourceProjectionRequest {
    project_id: fixture.input.project_id(),
    work_id: Some(new.work.id()),
    operation_id: new_row.record.operation_id,
    claim: new_row.record.claim.unwrap(),
    observed_at: time(220),
    fields: vec![digest(1)],
    action: SourceProjectionAction::Register,
  };
  fixture.store.coordinate_source_projection(newer).await.unwrap();
  assert_eq!(
    fixture.store.coordinate_source_projection(old_request).await.unwrap(),
    SourceProjectionState::Superseded
  );
}
#[tokio::test]
async fn source_projection_order_follows_atomic_acceptance_even_when_clocks_move_backwards() {
  let (fixture, profile) = fixture("jira").await;
  acceptance_order_contract(fixture, profile).await;
}

async fn disjoint_observation_contract<
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + IssueSourceProjectionStore + 'static,
>(
  mut fixture: Fixture<S>,
  profile: FactoryReporterProfile,
) {
  let newer = mapped_profile(&fixture, "new-effects");
  let projector = hosted(&fixture, vec![profile, newer.clone()]);
  let old_run = queue(&fixture, projector.clone()).await;
  next_input(&mut fixture, &newer).await;
  let new_run = queue_at(&fixture, projector, 200).await;
  let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
  let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
  let rows = fixture
    .store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("disjoint-observation"), time(220), time(1220), 4)
        .unwrap()
        .only_kind(key("work.status.projection")),
    )
    .await
    .unwrap();
  let old_row = rows
    .iter()
    .find(|r| r.record.subject.run_id() == Some(old_run))
    .unwrap();
  let new_row = rows
    .iter()
    .find(|r| r.record.subject.run_id() == Some(new_run))
    .unwrap();
  let mut old_request = SourceProjectionRequest {
    project_id: fixture.input.project_id(),
    work_id: Some(old.work.id()),
    operation_id: old_row.record.operation_id,
    claim: old_row.record.claim.unwrap(),
    observed_at: time(220),
    fields: vec![digest(1)],
    action: SourceProjectionAction::Register,
  };
  fixture
    .store
    .coordinate_source_projection(old_request.clone())
    .await
    .unwrap();
  let mut newer = SourceProjectionRequest {
    project_id: fixture.input.project_id(),
    work_id: Some(new.work.id()),
    operation_id: new_row.record.operation_id,
    claim: new_row.record.claim.unwrap(),
    observed_at: time(220),
    fields: vec![digest(2)],
    action: SourceProjectionAction::Register,
  };
  fixture.store.coordinate_source_projection(newer.clone()).await.unwrap();
  old_request.action = SourceProjectionAction::Observe { revision: None };
  let SourceProjectionState::Observation { revision, .. } = fixture
    .store
    .coordinate_source_projection(old_request.clone())
    .await
    .unwrap()
  else {
    panic!("expected read fence")
  };
  newer.action = SourceProjectionAction::Begin {
    step_id: digest(60),
    fields: vec![digest(2)],
  };
  fixture.store.coordinate_source_projection(newer.clone()).await.unwrap();
  newer.action = SourceProjectionAction::Finish {
    step_id: digest(60),
    uncertain: false,
  };
  fixture.store.coordinate_source_projection(newer).await.unwrap();
  old_request.action = SourceProjectionAction::Observe {
    revision: Some(revision),
  };
  assert!(
    matches!(
      fixture.store.coordinate_source_projection(old_request).await.unwrap(),
      SourceProjectionState::Observation {
        stable: true,
        quiescent: true,
        ..
      }
    ),
    "an unrelated managed field cannot invalidate this read fence"
  );
}
#[tokio::test]
async fn disjoint_field_writes_leave_another_inputs_observation_fence_current() {
  let (fixture, profile) = fixture("jira").await;
  disjoint_observation_contract(fixture, profile).await;
}

#[tokio::test]
async fn a_confirmed_prewrite_conflict_does_not_block_later_source_projections() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let (mut fixture, profile) = fixture(provider).await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let old_run = queue(&fixture, projector.clone()).await;
    let directory = fixture.root.path().join(provider);
    std::fs::write(directory.join("reporter-mode.json"), br#"{"conflict_on_apply":true}"#).unwrap();
    let worker = super::recovery::worker(&fixture, projector.clone(), "confirmed-conflict", 10000);
    assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1);
    assert!(
      !directory.join("reporter-applied").exists(),
      "Conflict precedes the native write"
    );
    let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
    std::fs::write(directory.join("reporter-mode.json"), b"{}").unwrap();
    next_input(&mut fixture, &profile).await;
    // Expiring the failed action must not convert a definite refusal into an unknown remote write.
    let next_run = queue_at(&fixture, projector, 6000).await;
    let next = fixture.store.factory_run_snapshot(next_run).await.unwrap();
    assert_eq!(worker.run_once(time(6020)).await.unwrap().delivered, 1, "{provider}");
    assert_eq!(
      old.flow,
      fixture.store.factory_run_snapshot(old_run).await.unwrap().flow
    );
    assert_eq!(
      next.flow,
      fixture.store.factory_run_snapshot(next_run).await.unwrap().flow
    );
    assert_eq!(fixture.runs().await.len(), 2);
  }
}

#[tokio::test(start_paused = true)]
async fn late_writes_from_separate_installations_share_the_native_item_order() {
  controlled_io(async move {
    let (fixture, profile) = fixture("github").await;
    late_write_contract(fixture, profile, false, true).await;
  })
  .await;
}
