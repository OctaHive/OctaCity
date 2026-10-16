use super::*;
use octacity_server_factory::{
  WorkSourceBinding, WorkSourceCapability, WorkSourceCapabilityRole, WorkSourceGroup, WorkSourceGroupId,
  WorkSourceInput, WorkSourceInputContext, WorkSourceInputDraft, WorkSourceInputId, WorkSourceRepositoryRef,
};
use octacity_server_store::AdmitPreparedWorkSource;
use octacity_server_store::{ClaimWorkSourceScan, WorkSourceProgress, WorkSourceScanTarget};
use octacity_server_store::{FactorySourceStore, PublishWorkSources, WorkSourceGroupChange, WorkSourceInputChange};
use octacity_server_store::{
  FactoryWorkAdmissionIntent, PublishSourcePreparationIntent, ReserveWorkSourcePreparation, WorkSourceItem,
};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_publication_is_atomic_owned_and_retains_exact_versions_in_both_stores() {
  let database = TestDatabase::migrated().await;
  let source_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .acquire_timeout(std::time::Duration::from_millis(250))
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let postgres = PostgresStore::new(source_pool.clone());
  let memory = InMemoryFactoryConfigurationStore::new();
  let project = ProjectId::generate();
  let foreign = ProjectId::generate();
  seed_projects(&database.pool, project, foreign).await;
  memory.seed_project(project).unwrap();
  memory.seed_project(foreign).unwrap();
  let fixture = fixture(project);
  memory.enable_project(project, fixture.choices.clone()).unwrap();
  let configuration = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    digest(3),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  let repository = repository(project, RepositoryId::generate());
  seed_repository(&database.pool, &repository).await;
  memory.seed_repository_version(repository.clone()).unwrap();
  let create = CreateFactoryConfiguration {
    configuration: configuration.clone(),
    idempotency_key: idempotency("source-target"),
    published_at: time(1),
    intent: FactoryConfigurationMutationIntent::Create {
      id: configuration.reference().id(),
      project_id: project,
      definition_digest: digest(3),
      draft: fixture.draft,
    },
  };
  postgres
    .create_factory_configuration(management_mutation_with_request(create.clone(), "source-target"))
    .await
    .unwrap();
  memory
    .create_factory_configuration(management_mutation_with_request(create, "source-target"))
    .await
    .unwrap();
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let input = input(&configuration, &repository, provider);
    let group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, vec![input.clone()]).unwrap();
    for is_postgres in [false, true] {
      let store: &dyn FactorySourceStore = if is_postgres { &postgres } else { &memory };
      publication_contract(store, &input, &group, foreign).await;
      scan_contract(store, &input, &group, foreign).await;
      preparation_contract(store, &input, foreign).await;
      let restarted = PostgresStore::new(source_pool.clone());
      let store: &dyn FactorySourceStore = if is_postgres { &restarted } else { &memory };
      retained_intent_contract(store, &input, &configuration, &repository).await;
      let restarted = PostgresStore::new(source_pool.clone());
      let store: &dyn FactorySourceStore = if is_postgres { &restarted } else { &memory };
      let retained = fenced_admission_contract(store, &input, &configuration).await;
      replacement_contract(store, &input, &group, &configuration, &retained).await;
    }
    let restarted = PostgresStore::new(source_pool.clone());
    assert_eq!(
      restarted
        .source_input(project, input.id(), input.version())
        .await
        .unwrap(),
      input
    );
    assert_eq!(
      restarted
        .source_group(project, group.id(), group.version())
        .await
        .unwrap(),
      group
    );
    let extra = self::input(&configuration, &repository, provider);
    source_artifact_contract(&memory, &extra, &configuration, None).await;
    source_artifact_contract(&postgres, &extra, &configuration, Some(&postgres)).await;
  }
  use octacity_server_store::testing::MutationEvidenceProbe as _;
  let evidence = memory.mutation_evidence_counts().await;
  assert_eq!(evidence.idempotency, 17);
  assert_eq!(evidence.audit, 17);
  assert_eq!(evidence.outbox, 17);
  let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM factory_runs),(SELECT count(*) FROM audit_facts),(SELECT count(*) FROM idempotency_records)").fetch_one(&database.pool).await.unwrap();
  assert_eq!(counts, (16, 17, 17));
  source_pool.close().await;
  database.cleanup().await;
}

async fn source_artifact_contract(
  store: &dyn octacity_server_store::WorkSourceArtifactStore,
  input: &WorkSourceInput,
  configuration: &FactoryConfiguration,
  transfers: Option<&dyn octacity_server_store::ArtifactRecordStore>,
) {
  use octacity_server_store::{
    PublishSourceArtifacts, ReserveSourceArtifacts, SourceArtifactObject, SourceArtifactRole,
  };
  store
    .publish_work_sources(PublishWorkSources {
      project_id: input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: input.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  let scan = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target: WorkSourceScanTarget::Input {
        id: input.id(),
        version: input.version(),
      },
      owner: key("artifact-mapper"),
      now: time(1),
      until: time(1000),
    })
    .await
    .unwrap()
    .unwrap();
  let preparation = store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim.clone(),
      input_id: input.id(),
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("container").unwrap(),
        item: ExternalWorkIdentity::new("mapped-ticket").unwrap(),
      },
      snapshot: b"private original snapshot".to_vec(),
      now: time(2),
    })
    .await
    .unwrap();
  let objects = [SourceArtifactRole::Task, SourceArtifactRole::Acceptance]
    .into_iter()
    .map(|role| SourceArtifactObject {
      role,
      artifact: FactoryArtifactReference::new(ArtifactId::generate(), digest(90), 12).unwrap(),
      upload_id: octacity_server_domain::ArtifactUploadId::generate(),
      media_type: octacity_server_store::ArtifactMediaType::new("application/json").unwrap(),
    })
    .collect::<Vec<_>>();
  let request = ReserveSourceArtifacts {
    claim: scan.claim.clone(),
    preparation_id: preparation.id,
    mapping_digest: digest(88),
    objects: objects.clone(),
    retention_millis: 100,
    now: time(3),
  };
  let reserved = store.reserve_source_artifacts(request.clone()).await.unwrap();
  assert_eq!(reserved.objects, objects);
  assert_eq!(reserved.expires_at, time(103));
  assert!(reserved.published_at.is_none());
  assert!(
    store
      .source_artifact(input.project_id(), objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(
    store
      .reserve_source_artifacts(ReserveSourceArtifacts {
        now: time(4),
        ..request.clone()
      })
      .await
      .unwrap(),
    reserved
  );
  let mut conflicting = request.clone();
  conflicting.mapping_digest = digest(89);
  assert!(store.reserve_source_artifacts(conflicting).await.is_err());
  let mut stale = scan.claim.clone();
  stale.owner = key("stale-owner");
  assert!(
    store
      .publish_source_artifacts(PublishSourceArtifacts {
        claim: stale,
        preparation_id: preparation.id,
        mapping_digest: digest(88),
        now: time(4)
      })
      .await
      .is_err()
  );
  let publication = PublishSourceArtifacts {
    claim: scan.claim.clone(),
    preparation_id: preparation.id,
    mapping_digest: digest(88),
    now: time(4),
  };
  let published = store.publish_source_artifacts(publication.clone()).await.unwrap();
  assert_eq!(published.published_at, Some(time(4)));
  assert_eq!(store.publish_source_artifacts(publication).await.unwrap(), published);
  let visible = store
    .source_artifact(input.project_id(), objects[0].artifact.artifact_id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(visible.object, objects[0]);
  assert_eq!(visible.expires_at, time(103));
  let intent = FactoryWorkAdmissionIntent {
    project_id: input.project_id(),
    configuration_id: configuration.reference().id(),
    configuration_version: configuration.reference().version(),
    repository_id: input.definition().repository.id,
    repository_version: input.definition().repository.version,
    external_identity: preparation
      .admission_identity(octacity_server_store::ManagementSecurityScope::trusted_network())
      .external_identity,
    source: octacity_server_store::RepositorySourceSelection::ExactRevision("frozen-base".parse().unwrap()),
    artifacts: published.work_artifacts().unwrap(),
    classification: WorkClassification::new(WorkPriority::new(20).unwrap(), RiskClass::Medium, Default::default()),
    idempotency_key: idempotency(&format!("mapped-{}", preparation.id)),
    admitted_at: time(4),
  };
  let prepared = store
    .publish_source_preparation_intent(PublishSourcePreparationIntent {
      claim: scan.claim.clone(),
      preparation_id: preparation.id,
      intent,
      now: time(4),
    })
    .await
    .unwrap();
  let admission = prepared_admission(&prepared, configuration);
  let original = store
    .admit_prepared_source_work(
      AdmitPreparedWorkSource {
        claim: scan.claim.clone(),
        preparation_id: preparation.id,
        now: time(5),
      },
      management_mutation_with_request(admission.clone(), "mapped-original"),
    )
    .await
    .unwrap();
  let replay = store
    .admit_prepared_source_work(
      AdmitPreparedWorkSource {
        claim: scan.claim.clone(),
        preparation_id: preparation.id,
        now: time(103),
      },
      management_mutation_with_request(admission.clone(), "mapped-expired-replay"),
    )
    .await
    .unwrap();
  assert_eq!(replay.admission, original.admission);
  assert_eq!(replay.disposition, octacity_server_store::MutationDisposition::Replayed);

  if let Some(transfers) = transfers {
    let input = transfers
      .published_execution_input(objects[0].artifact.artifact_id())
      .await
      .unwrap();
    assert_eq!(input.artifact_id, objects[0].artifact.artifact_id());
    assert_eq!(input.upload_id, objects[0].upload_id);
    assert_eq!(input.size_bytes, 12);
    assert_eq!(input.digest.as_bytes(), digest(90).as_bytes());
    assert_eq!(input.expires_at, Some(time(103)));
    assert!(
      transfers.published_artifact(input.artifact_id).await.is_err(),
      "private source data has no fabricated Build output"
    );
  }
  assert!(
    store
      .source_artifact(ProjectId::generate(), objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .reserve_source_artifacts(ReserveSourceArtifacts {
        now: time(103),
        ..request
      })
      .await
      .is_err()
  );
  let expired = store
    .expired_source_preparations(time(103), None, 100)
    .await
    .unwrap()
    .into_iter()
    .find(|v| v.preparation_id == preparation.id)
    .unwrap();
  assert_eq!(expired.objects, objects);
  assert!(
    store
      .complete_source_preparation_cleanup(expired.clone(), time(102), time(113))
      .await
      .is_err()
  );
  store
    .complete_source_preparation_cleanup(expired.clone(), time(103), time(113))
    .await
    .unwrap();
  assert!(
    store
      .source_artifact(input.project_id(), objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .source_preparation(input.project_id(), preparation.id)
      .await
      .unwrap()
      .snapshot
      .is_empty()
  );
  if let Some(transfers) = transfers {
    assert!(
      transfers
        .published_execution_input(objects[0].artifact.artifact_id())
        .await
        .is_err()
    );
  }
  let replay = store
    .admit_prepared_source_work(
      AdmitPreparedWorkSource {
        claim: scan.claim.clone(),
        preparation_id: preparation.id,
        now: time(104),
      },
      management_mutation_with_request(admission, "mapped-cleaned-replay"),
    )
    .await
    .unwrap();
  assert_eq!(replay.admission, original.admission);
  assert_eq!(replay.disposition, octacity_server_store::MutationDisposition::Replayed);
  // Duplicate acknowledgements do not rewrite state or postpone the scheduled reconciliation.
  store
    .complete_source_preparation_cleanup(expired, time(104), time(500))
    .await
    .unwrap();
  assert!(
    !store
      .expired_source_preparations(time(112), None, 100)
      .await
      .unwrap()
      .iter()
      .any(|v| v.preparation_id == preparation.id)
  );
  let due = store
    .expired_source_preparations(time(113), None, 100)
    .await
    .unwrap()
    .into_iter()
    .find(|v| v.preparation_id == preparation.id)
    .unwrap();
  assert_eq!(due.cleaned_at, Some(time(103)));
  assert_eq!(due.next_cleanup_at, Some(time(113)));
  store
    .complete_source_preparation_cleanup(due, time(113), time(123))
    .await
    .unwrap();
  assert!(
    !store
      .expired_source_preparations(time(114), None, 100)
      .await
      .unwrap()
      .iter()
      .any(|v| v.preparation_id == preparation.id)
  );
  let rejected = store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim.clone(),
      input_id: input.id(),
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("container").unwrap(),
        item: ExternalWorkIdentity::new("missing-criteria").unwrap(),
      },
      snapshot: b"private rejected data".to_vec(),
      now: time(5),
    })
    .await
    .unwrap();
  let rejected = store
    .reject_source_preparation(octacity_server_store::RejectSourcePreparation {
      claim: scan.claim.clone(),
      preparation_id: rejected.id,
      mapping_digest: digest(88),
      reason: key("source.missing_acceptance"),
      retention_millis: 100,
      now: time(6),
    })
    .await
    .unwrap();
  assert_eq!(rejected.rejection, Some(key("source.missing_acceptance")));
  assert!(
    store
      .pending_source_preparations(input.project_id(), scan.claim.target, None, 64)
      .await
      .unwrap()
      .is_empty()
  );
  let raw = store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim.clone(),
      input_id: input.id(),
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("container").unwrap(),
        item: ExternalWorkIdentity::new("orphan-before-mapping").unwrap(),
      },
      snapshot: b"private raw orphan".to_vec(),
      now: time(7),
    })
    .await
    .unwrap();
  let raw = store
    .expired_source_preparations(time(2592000007), None, 100)
    .await
    .unwrap()
    .into_iter()
    .find(|v| v.preparation_id == raw.id)
    .unwrap();
  assert!(raw.objects.is_empty());
  store
    .complete_source_preparation_cleanup(raw.clone(), time(2592000007), time(2592000107))
    .await
    .unwrap();
  assert!(
    store
      .source_preparation(input.project_id(), raw.preparation_id)
      .await
      .unwrap()
      .snapshot
      .is_empty()
  );
  assert!(
    !store
      .expired_source_preparations(time(2592000008), None, 100)
      .await
      .unwrap()
      .iter()
      .any(|v| v.preparation_id == raw.preparation_id)
  );
}

async fn fenced_admission_contract(
  store: &dyn FactorySourceStore,
  input: &WorkSourceInput,
  configuration: &FactoryConfiguration,
) -> octacity_server_store::WorkSourcePreparation {
  let target = WorkSourceScanTarget::Input {
    id: input.id(),
    version: input.version(),
  };
  let first = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("admit-one"),
      now: time(60),
      until: time(70),
    })
    .await
    .unwrap()
    .unwrap();
  let preparation = store
    .pending_source_preparations(input.project_id(), target, None, 1)
    .await
    .unwrap()
    .remove(0);
  let admission = prepared_admission(&preparation, configuration);
  let second = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("admit-two"),
      now: time(70),
      until: time(80),
    })
    .await
    .unwrap()
    .unwrap();
  let stale = AdmitPreparedWorkSource {
    claim: first.claim,
    preparation_id: preparation.id,
    now: time(71),
  };
  assert!(
    store
      .admit_prepared_source_work(
        stale,
        management_mutation_with_request(admission.clone(), "stale-source")
      )
      .await
      .is_err()
  );
  assert!(
    store
      .recover_factory_admission(
        &preparation.admission_identity(octacity_server_store::ManagementSecurityScope::trusted_network())
      )
      .await
      .unwrap()
      .is_none()
  );
  let current = AdmitPreparedWorkSource {
    claim: second.claim.clone(),
    preparation_id: preparation.id,
    now: time(71),
  };
  let original = store
    .admit_prepared_source_work(
      current.clone(),
      management_mutation_with_request(admission.clone(), "fresh-source"),
    )
    .await
    .unwrap();
  assert_eq!(
    original.disposition,
    octacity_server_store::MutationDisposition::Applied
  );
  let correlated = store
    .source_preparation(input.project_id(), preparation.id)
    .await
    .unwrap();
  assert_eq!(
    correlated.admission.as_ref().unwrap().work_id,
    original.admission.work.id()
  );
  assert_eq!(
    correlated.admission.as_ref().unwrap().run_id,
    original.admission.run.id()
  );
  let associations = store
    .source_issue_correlations(input.project_id(), preparation.issue, None, 1)
    .await
    .unwrap();
  assert_eq!(associations.len(), 1);
  assert_eq!(associations[0].input_id, input.id());
  assert_eq!(associations[0].input_version, input.version());
  assert_eq!(associations[0].admission, correlated.admission.clone().unwrap());
  assert!(
    store
      .source_issue_correlations(ProjectId::generate(), preparation.issue, None, 1)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(
    store
      .source_issue_correlations(input.project_id(), preparation.issue, Some(input.id()), 1)
      .await
      .unwrap()
      .is_empty()
  );
  let replay = store
    .admit_prepared_source_work(
      current,
      management_mutation_with_request(admission, "source-response-lost"),
    )
    .await
    .unwrap();
  assert_eq!(replay.admission, original.admission);
  assert!(
    store
      .pending_source_preparations(input.project_id(), target, None, 64)
      .await
      .unwrap()
      .is_empty()
  );
  let progress = WorkSourceProgress {
    window_start: time(1),
    window_end: time(9),
    continuation: None,
    reconciliation: false,
  };
  store
    .checkpoint_source_scan(second.claim, time(72), second.progress, progress)
    .await
    .unwrap();
  correlated
}

async fn retained_intent_contract(
  store: &dyn FactorySourceStore,
  input: &WorkSourceInput,
  configuration: &FactoryConfiguration,
  repository: &PublishedRepository,
) {
  let scan = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target: WorkSourceScanTarget::Input {
        id: input.id(),
        version: input.version(),
      },
      owner: key("mapper"),
      now: time(50),
      until: time(60),
    })
    .await
    .unwrap()
    .unwrap();
  let pending = store
    .pending_source_preparations(input.project_id(), scan.claim.target, None, 1)
    .await
    .unwrap();
  assert_eq!(pending.len(), 1);
  assert_eq!(pending[0].snapshot, b"original immutable observation");
  let preparation = &pending[0];
  let admission = admission_from_configuration(900, 901, "prepared", 52, configuration, repository);
  let intent = FactoryWorkAdmissionIntent {
    project_id: input.project_id(),
    configuration_id: configuration.reference().id(),
    configuration_version: configuration.reference().version(),
    repository_id: repository.id,
    repository_version: repository.version,
    external_identity: preparation
      .admission_identity(octacity_server_store::ManagementSecurityScope::trusted_network())
      .external_identity,
    source: octacity_server_store::RepositorySourceSelection::DefaultReference,
    artifacts: admission.work.artifacts().clone(),
    classification: WorkClassification::new(
      admission.work.priority(),
      admission.work.risk(),
      admission.work.metadata().clone(),
    ),
    idempotency_key: idempotency("prepared-source"),
    admitted_at: time(52),
  };
  let request = PublishSourcePreparationIntent {
    claim: scan.claim.clone(),
    preparation_id: preparation.id,
    intent: intent.clone(),
    now: time(51),
  };
  let prepared = store.publish_source_preparation_intent(request.clone()).await.unwrap();
  assert_eq!(prepared.intent, Some(intent.clone()));
  assert_eq!(
    store.publish_source_preparation_intent(request.clone()).await.unwrap(),
    prepared
  );
  let mut changed = request;
  changed.intent.artifacts = WorkArtifacts::new(artifact(92), artifact(93), vec![]).unwrap();
  assert!(store.publish_source_preparation_intent(changed).await.is_err());
  let recovered = store
    .source_preparation(input.project_id(), preparation.id)
    .await
    .unwrap();
  assert_eq!(recovered.intent, Some(intent));
  assert!(
    store
      .pending_source_preparations(input.project_id(), scan.claim.target, Some(preparation.id), 1)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(
    store
      .pending_source_preparations(input.project_id(), scan.claim.target, None, 0)
      .await
      .is_err()
  );
}

async fn preparation_contract(store: &dyn FactorySourceStore, input: &WorkSourceInput, foreign: ProjectId) {
  let target = WorkSourceScanTarget::Input {
    id: input.id(),
    version: input.version(),
  };
  let first = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("prepare-one"),
      now: time(31),
      until: time(40),
    })
    .await
    .unwrap()
    .unwrap();
  let request = ReserveWorkSourcePreparation {
    claim: first.claim.clone(),
    input_id: input.id(),
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("Native/Container").unwrap(),
      item: ExternalWorkIdentity::new("Native/Ticket/42").unwrap(),
    },
    snapshot: b"original immutable observation".to_vec(),
    now: time(32),
  };
  let pending = store.reserve_source_preparation(request.clone()).await.unwrap();
  assert_eq!(pending.input, *input);
  assert_eq!(pending.snapshot, request.snapshot);
  let second = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("prepare-two"),
      now: time(40),
      until: time(50),
    })
    .await
    .unwrap()
    .unwrap();
  assert!(
    store
      .reserve_source_preparation(ReserveWorkSourcePreparation {
        now: time(41),
        ..request.clone()
      })
      .await
      .is_err()
  );
  let changed = ReserveWorkSourcePreparation {
    claim: second.claim.clone(),
    snapshot: b"edited content and labels".to_vec(),
    now: time(41),
    ..request.clone()
  };
  assert_eq!(store.reserve_source_preparation(changed).await.unwrap(), pending);
  assert_eq!(
    store.source_preparation(input.project_id(), pending.id).await.unwrap(),
    pending
  );
  assert!(store.source_preparation(foreign, pending.id).await.is_err());
  let invalid = ReserveWorkSourcePreparation {
    claim: second.claim.clone(),
    input_id: WorkSourceInputId::generate(),
    now: time(41),
    ..request.clone()
  };
  assert!(store.reserve_source_preparation(invalid).await.is_err());
  assert!(
    store
      .reserve_source_preparation(ReserveWorkSourcePreparation {
        claim: second.claim.clone(),
        snapshot: vec![0; 1_048_577],
        now: time(41),
        ..request
      })
      .await
      .is_err()
  );
  let progress = WorkSourceProgress {
    window_start: time(1),
    window_end: time(9),
    continuation: Some("next-page".into()),
    reconciliation: false,
  };
  // An unfinished candidate prevents publication of a cursor covering it.
  assert!(
    store
      .checkpoint_source_scan(second.claim, time(42), first.progress, progress)
      .await
      .is_err()
  );
}

async fn scan_contract(
  store: &dyn FactorySourceStore,
  input: &WorkSourceInput,
  group: &WorkSourceGroup,
  foreign: ProjectId,
) {
  for target in [
    WorkSourceScanTarget::Input {
      id: input.id(),
      version: input.version(),
    },
    WorkSourceScanTarget::Group {
      id: group.id(),
      version: group.version(),
    },
  ] {
    let racing = ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("race-one"),
      now: time(0),
      until: time(9),
    };
    let (left, right) = tokio::join!(
      store.claim_source_scan(racing.clone()),
      store.claim_source_scan(ClaimWorkSourceScan {
        owner: key("race-two"),
        ..racing
      })
    );
    assert_eq!(
      usize::from(left.unwrap().is_some()) + usize::from(right.unwrap().is_some()),
      1
    );
    let request = ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("worker-one"),
      now: time(10),
      until: time(20),
    };
    let original = store.claim_source_scan(request.clone()).await.unwrap().unwrap();
    assert_eq!(original.progress, None);
    let mut competing = request.clone();
    competing.owner = key("worker-two");
    assert!(store.claim_source_scan(competing.clone()).await.unwrap().is_none());
    let progress = WorkSourceProgress {
      window_start: time(1),
      window_end: time(9),
      continuation: Some("opaque-page:2".into()),
      reconciliation: false,
    };
    store
      .checkpoint_source_scan(original.claim.clone(), time(11), None, progress.clone())
      .await
      .unwrap();
    assert!(
      store
        .checkpoint_source_scan(original.claim.clone(), time(12), None, progress.clone())
        .await
        .is_err()
    );
    let mut forged = original.claim.clone();
    forged.owner = key("worker-two");
    assert!(
      store
        .checkpoint_source_scan(forged, time(12), Some(progress.clone()), progress.clone())
        .await
        .is_err()
    );
    competing.now = time(20);
    competing.until = time(30);
    let next = store.claim_source_scan(competing).await.unwrap().unwrap();
    assert!(next.claim.fence > original.claim.fence);
    assert_eq!(next.progress, Some(progress.clone()));
    assert!(
      store
        .checkpoint_source_scan(
          original.claim.clone(),
          time(21),
          Some(progress.clone()),
          progress.clone()
        )
        .await
        .is_err()
    );
    let mut forged = next.claim.clone();
    forged.project_id = foreign;
    assert!(
      store
        .checkpoint_source_scan(forged, time(21), Some(progress.clone()), progress.clone())
        .await
        .is_err()
    );
    store
      .checkpoint_source_scan(
        next.claim.clone(),
        time(21),
        Some(progress.clone()),
        WorkSourceProgress {
          continuation: None,
          ..progress.clone()
        },
      )
      .await
      .unwrap();
    assert!(
      store
        .checkpoint_source_scan(next.claim, time(30), Some(progress.clone()), progress)
        .await
        .is_err()
    );
  }
}

fn input(configuration: &FactoryConfiguration, repository: &PublishedRepository, provider: &str) -> WorkSourceInput {
  let reference = |name| ImmutableReference::new(key(name), key("v1"), digest(1));
  let source = WorkSourceCapability {
    project_id: repository.project_id,
    role: WorkSourceCapabilityRole::Source,
    binding: WorkSourceBinding {
      installation: reference(provider),
      capability: reference("ticket-source"),
      configuration: reference("selector"),
      resource: reference("tickets"),
    },
  };
  let repository = WorkSourceRepositoryRef {
    project_id: repository.project_id,
    id: repository.id,
    version: repository.version,
    definition_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&repository.definition).unwrap()),
  };
  WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    repository.project_id,
    WorkSourceInputDraft {
      target: configuration.reference().clone(),
      repository: repository.clone(),
      source: source.binding.clone(),
      reporter: None,
      enabled: true,
    },
    WorkSourceInputContext {
      target: configuration,
      repository: &repository,
      source: Some(&source),
      reporter: None,
    },
  )
  .unwrap()
}

async fn publication_contract(
  store: &dyn FactorySourceStore,
  input: &WorkSourceInput,
  group: &WorkSourceGroup,
  foreign: ProjectId,
) {
  let publication = PublishWorkSources {
    project_id: input.project_id(),
    inputs: vec![WorkSourceInputChange {
      expected_version: None,
      input: input.clone(),
    }],
    group: Some(WorkSourceGroupChange {
      expected_version: None,
      group: group.clone(),
    }),
  };
  store.publish_work_sources(publication.clone()).await.unwrap();
  assert_eq!(
    store
      .source_input(input.project_id(), input.id(), input.version())
      .await
      .unwrap(),
    *input
  );
  assert!(store.source_input(foreign, input.id(), input.version()).await.is_err());
  assert!(store.source_group(foreign, group.id(), group.version()).await.is_err());
  // Reusing an existing group must roll back the newly supplied input as well.
  let mut new = serde_json::to_value(input).unwrap();
  let new_id = WorkSourceInputId::generate();
  new["id"] = serde_json::to_value(new_id).unwrap();
  let fresh = serde_json::from_value::<WorkSourceInput>(new).unwrap();
  let mut conflicting = publication.clone();
  conflicting.inputs[0].input = fresh;
  assert!(store.publish_work_sources(conflicting).await.is_err());
  assert!(
    store
      .source_input(input.project_id(), new_id, input.version())
      .await
      .is_err()
  );
  let mut foreign_publication = publication;
  foreign_publication.project_id = foreign;
  assert!(store.publish_work_sources(foreign_publication).await.is_err());
}

async fn replacement_contract(
  store: &dyn FactorySourceStore,
  input: &WorkSourceInput,
  group: &WorkSourceGroup,
  configuration: &FactoryConfiguration,
  original: &octacity_server_store::WorkSourcePreparation,
) {
  use octacity_server_factory::{WorkSourceGroupVersion, WorkSourceInputVersion};
  let target = WorkSourceScanTarget::Input {
    id: input.id(),
    version: input.version(),
  };
  let first = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("before-replacement"),
      now: time(80),
      until: time(90),
    })
    .await
    .unwrap()
    .unwrap();
  let group_claim = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target: WorkSourceScanTarget::Group {
        id: group.id(),
        version: group.version(),
      },
      owner: key("old-group-owner"),
      now: time(80),
      until: time(90),
    })
    .await
    .unwrap()
    .unwrap();
  let pending = store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: first.claim.clone(),
      input_id: input.id(),
      item: WorkSourceItem {
        item: ExternalWorkIdentity::new("native-ticket-44").unwrap(),
        ..original.item.clone()
      },
      snapshot: b"retained across replacement".to_vec(),
      now: time(81),
    })
    .await
    .unwrap();
  let mut intent = original.intent.clone().unwrap();
  intent.external_identity = pending
    .admission_identity(octacity_server_store::ManagementSecurityScope::trusted_network())
    .external_identity;
  let pending = store
    .publish_source_preparation_intent(PublishSourcePreparationIntent {
      claim: first.claim.clone(),
      preparation_id: pending.id,
      intent: intent.clone(),
      now: time(81),
    })
    .await
    .unwrap();
  let mut definition = input.definition().clone();
  definition.source.configuration = ImmutableReference::new(key("edited-selector"), key("v2"), digest(22));
  let capability = WorkSourceCapability {
    project_id: input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: definition.source.clone(),
  };
  let changed = input
    .replace(
      WorkSourceInputVersion::new(2).unwrap(),
      definition,
      WorkSourceInputContext {
        target: configuration,
        repository: &input.definition().repository,
        source: Some(&capability),
        reporter: None,
      },
    )
    .unwrap();
  let changed_group = group
    .replace(WorkSourceGroupVersion::new(2).unwrap(), vec![changed.clone()])
    .unwrap();
  let replacement = PublishWorkSources {
    project_id: input.project_id(),
    inputs: vec![WorkSourceInputChange {
      expected_version: Some(input.version()),
      input: changed.clone(),
    }],
    group: Some(WorkSourceGroupChange {
      expected_version: Some(group.version()),
      group: changed_group.clone(),
    }),
  };
  store.publish_work_sources(replacement.clone()).await.unwrap();
  assert!(store.publish_work_sources(replacement).await.is_err());
  assert!(
    store
      .checkpoint_source_scan(
        group_claim.claim,
        time(82),
        group_claim.progress,
        WorkSourceProgress {
          window_start: time(1),
          window_end: time(9),
          continuation: None,
          reconciliation: false
        }
      )
      .await
      .is_err()
  );
  assert!(
    store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: input.project_id(),
        target: WorkSourceScanTarget::Group {
          id: group.id(),
          version: group.version()
        },
        owner: key("stale-group"),
        now: time(90),
        until: time(100)
      })
      .await
      .is_err()
  );
  let revised_group = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target: WorkSourceScanTarget::Group {
        id: changed_group.id(),
        version: changed_group.version(),
      },
      owner: key("new-group-owner"),
      now: time(90),
      until: time(100),
    })
    .await
    .unwrap()
    .unwrap();
  assert_eq!(revised_group.progress, None);

  assert!(
    store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: input.project_id(),
        target,
        owner: key("stale-definition"),
        now: time(90),
        until: time(100)
      })
      .await
      .is_err()
  );
  assert!(
    store
      .checkpoint_source_scan(
        first.claim,
        time(82),
        first.progress,
        WorkSourceProgress {
          window_start: time(1),
          window_end: time(9),
          continuation: None,
          reconciliation: false
        }
      )
      .await
      .is_err()
  );
  let target = WorkSourceScanTarget::Input {
    id: changed.id(),
    version: changed.version(),
  };
  let current = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: input.project_id(),
      target,
      owner: key("after-replacement"),
      now: time(90),
      until: time(100),
    })
    .await
    .unwrap()
    .unwrap();
  assert_eq!(current.progress, None);
  assert_eq!(
    store
      .pending_source_preparations(input.project_id(), target, None, 1)
      .await
      .unwrap(),
    vec![pending.clone()]
  );
  let admission = prepared_admission(&pending, configuration);
  let accepted = store
    .admit_prepared_source_work(
      AdmitPreparedWorkSource {
        claim: current.claim.clone(),
        preparation_id: pending.id,
        now: time(91),
      },
      management_mutation_with_request(admission, "resumed-original-intent"),
    )
    .await
    .unwrap();
  assert_eq!(accepted.admission.work.configuration(), &input.definition().target);
  assert_eq!(
    store
      .source_preparation(input.project_id(), pending.id)
      .await
      .unwrap()
      .input,
    *input
  );
  // Compatibility with a crash after ordinary admission but before source correlation.
  let lost = store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: current.claim.clone(),
      input_id: input.id(),
      item: WorkSourceItem {
        item: ExternalWorkIdentity::new("lost-correlation-ticket").unwrap(),
        ..original.item.clone()
      },
      snapshot: b"before response loss".to_vec(),
      now: time(92),
    })
    .await
    .unwrap();
  let mut lost_intent = intent;
  lost_intent.external_identity = lost
    .admission_identity(octacity_server_store::ManagementSecurityScope::trusted_network())
    .external_identity;
  let lost = store
    .publish_source_preparation_intent(PublishSourcePreparationIntent {
      claim: current.claim.clone(),
      preparation_id: lost.id,
      intent: lost_intent,
      now: time(92),
    })
    .await
    .unwrap();
  let admission = prepared_admission(&lost, configuration);
  let mut legacy_transport = admission.clone();
  legacy_transport.probe.idempotency_key = idempotency(&format!("legacy-source-{}", lost.id));
  let committed = store
    .admit_factory_work(management_mutation_with_request(
      legacy_transport,
      "before-correlation-crash",
    ))
    .await
    .unwrap();
  assert!(
    store
      .source_preparation(input.project_id(), lost.id)
      .await
      .unwrap()
      .admission
      .is_none()
  );
  let recovered = store
    .admit_prepared_source_work(
      AdmitPreparedWorkSource {
        claim: current.claim.clone(),
        preparation_id: lost.id,
        now: time(93),
      },
      management_mutation_with_request(admission, "after-correlation-crash"),
    )
    .await
    .unwrap();
  assert_eq!(recovered.admission, committed.admission);
  assert_eq!(
    recovered.disposition,
    octacity_server_store::MutationDisposition::Replayed
  );

  let mut disabled = changed.definition().clone();
  disabled.enabled = false;
  let disabled = changed
    .replace(
      WorkSourceInputVersion::new(3).unwrap(),
      disabled,
      WorkSourceInputContext {
        target: configuration,
        repository: &input.definition().repository,
        source: Some(&capability),
        reporter: None,
      },
    )
    .unwrap();
  let disabled_group = changed_group
    .replace(WorkSourceGroupVersion::new(3).unwrap(), vec![disabled.clone()])
    .unwrap();
  store
    .publish_work_sources(PublishWorkSources {
      project_id: input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: Some(changed.version()),
        input: disabled.clone(),
      }],
      group: Some(WorkSourceGroupChange {
        expected_version: Some(changed_group.version()),
        group: disabled_group.clone(),
      }),
    })
    .await
    .unwrap();
  for target in [
    WorkSourceScanTarget::Input {
      id: disabled.id(),
      version: disabled.version(),
    },
    WorkSourceScanTarget::Group {
      id: disabled_group.id(),
      version: disabled_group.version(),
    },
  ] {
    assert!(
      store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: input.project_id(),
          target,
          owner: key("disabled"),
          now: time(100),
          until: time(110)
        })
        .await
        .is_err()
    );
  }
  assert_eq!(
    store
      .source_input(input.project_id(), input.id(), input.version())
      .await
      .unwrap(),
    *input
  );
  assert_eq!(
    store
      .source_group(input.project_id(), group.id(), group.version())
      .await
      .unwrap(),
    *group
  );
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_migration_preserves_existing_manual_admission_and_its_replay() {
  let database = TestDatabase::empty().await;
  octacity_server_store_postgres::MIGRATOR
    .run_to(58, &database.pool)
    .await
    .unwrap();
  let project = ProjectId::generate();
  seed_projects(&database.pool, project, ProjectId::generate()).await;
  let fixture = fixture(project);
  let configuration = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    digest(3),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  let repository = repository(project, RepositoryId::generate());
  seed_repository(&database.pool, &repository).await;
  let store = PostgresStore::new(database.pool.clone());
  store
    .create_factory_configuration(management_mutation_with_request(
      CreateFactoryConfiguration {
        configuration: configuration.clone(),
        idempotency_key: idempotency("pre-source-target"),
        published_at: time(1),
        intent: FactoryConfigurationMutationIntent::Create {
          id: configuration.reference().id(),
          project_id: project,
          definition_digest: digest(3),
          draft: fixture.draft,
        },
      },
      "pre-source-target",
    ))
    .await
    .unwrap();
  let admission = admission_from_configuration(990, 991, "pre-source-manual-work", 10, &configuration, &repository);
  let original = store
    .admit_factory_work(management_mutation_with_request(admission.clone(), "pre-source-admit"))
    .await
    .unwrap();
  let before: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM factory_runs),(SELECT count(*) FROM audit_facts),(SELECT count(*) FROM idempotency_records)").fetch_one(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let after: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM factory_runs),(SELECT count(*) FROM audit_facts),(SELECT count(*) FROM idempotency_records)").fetch_one(&database.pool).await.unwrap();
  assert_eq!(after, before);
  let restarted = PostgresStore::new(database.pool.clone());
  let replay = restarted
    .admit_factory_work(management_mutation_with_request(admission, "post-source-replay"))
    .await
    .unwrap();
  assert_eq!(replay.admission, original.admission);
  assert_eq!(replay.disposition, octacity_server_store::MutationDisposition::Replayed);
  database.cleanup().await;
}

fn prepared_admission(
  preparation: &octacity_server_store::WorkSourcePreparation,
  configuration: &FactoryConfiguration,
) -> AdmitFactoryWork {
  let intent = preparation.intent.as_ref().unwrap();
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    intent.external_identity.clone(),
    ExactSubject::new(
      intent.project_id,
      intent.repository_id,
      ImmutableRevision::new("frozen-base").unwrap(),
    ),
    intent.artifacts.clone(),
    intent.classification.clone(),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  AdmitFactoryWork {
    probe: preparation
      .admission_probe(octacity_server_store::ManagementSecurityScope::trusted_network())
      .unwrap(),
    repository_version: intent.repository_version,
    flow: octacity_server_factory::AdmittedFlow::from_configuration(configuration, &run).unwrap(),
    work,
    run,
    admitted_at: intent.admitted_at,
  }
}
