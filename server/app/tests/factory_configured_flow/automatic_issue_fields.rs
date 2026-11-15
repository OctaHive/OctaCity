//! The ordinary agent path captures trusted Issue state and recovers local writes independently.
use super::*;
use octacity_server_store::testing::factory_issue_field_journal_fixture_for_work;

#[path = "automatic_issue_fields_context_lineage.rs"]
mod context_lineage;

struct AutomaticFixture {
  admission: PublishedFactoryAdmission,
  schema: FlowDataSchema,
  input: FlowNodeInput,
}

struct FactoryTestClock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for FactoryTestClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0.load(std::sync::atomic::Ordering::SeqCst)))
  }
}
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}
struct Inputs(FlowNodeInput);
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(&self, _: &FactoryRunSnapshot, _: &FactoryNodeTarget) -> Result<FlowNodeInput, ApplicationError> {
    Ok(self.0.clone())
  }
}
async fn fixture() -> (
  postgres_support::TestDatabase,
  Arc<PostgresStore>,
  AutomaticFixture,
  IssueRevision,
  FactoryRunClaimRecord,
) {
  fixture_options(|_| {}).await
}

async fn fixture_options(
  configure: impl FnOnce(&mut AutomaticFixture),
) -> (
  postgres_support::TestDatabase,
  Arc<PostgresStore>,
  AutomaticFixture,
  IssueRevision,
  FactoryRunClaimRecord,
) {
  let db = postgres_support::TestDatabase::migrated().await;
  let original = factory_node_test_support::fixtures::work();
  let request = IssueExecutionRequestId::generate();
  let content = requirements();
  let base = factory_issue_field_journal_fixture_for_work(work_for(&original, request, &content));
  let mut fixture = AutomaticFixture {
    admission: base.admission,
    schema: base.schema,
    input: base.input,
  };
  configure(&mut fixture);
  let store = admit(&db.pool, &fixture.admission).await;
  let issue = create_issue(
    &store,
    fixture.admission.work.subject().project_id(),
    IssueId::generate(),
    content,
  )
  .await;
  associate(&store, &issue, Some(request), &fixture.admission).await;
  let run = fixture.admission.run.id();
  let claim = FactoryRunClaimRecord::new(
    run,
    key("auto-fields"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([42; 32])),
      at(10),
      at(200),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: fixture.admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        claim.id,
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  (db, store, fixture, issue, claim)
}
fn writer(
  store: Arc<PostgresStore>,
  fixture: &AutomaticFixture,
  clock: Arc<FactoryTestClock>,
) -> FactoryIssueFieldWrites<PostgresStore, PostgresStore> {
  writer_with(
    store.clone(),
    store,
    fixture,
    clock,
    Arc::new(TrustedNetworkManagementPolicy),
  )
}
fn writer_with<S: IssueClassificationStore + 'static>(
  commands: Arc<S>,
  store: Arc<PostgresStore>,
  fixture: &AutomaticFixture,
  clock: Arc<FactoryTestClock>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> FactoryIssueFieldWrites<S, PostgresStore> {
  let selected = fixture
    .admission
    .flow
    .closure()
    .definitions()
    .iter()
    .flat_map(|definition| definition.nodes())
    .flat_map(|node| node.outcomes())
    .find_map(FlowOutcomeDefinition::issue_fields)
    .unwrap();
  let action = Arc::new(FactoryIssueClassificationAction::new(
    selected.writer.clone(),
    commands,
    store.clone(),
    store.clone(),
    context(),
    policy,
    clock,
  ));
  FactoryIssueFieldWrites::new(
    action,
    store,
    FactoryReporterWorkerSettings::new(key("fields-outbox"), 100, 8).unwrap(),
  )
}
fn input_owner(store: Arc<PostgresStore>, input: &FlowNodeInput) -> FactoryIssueInputs {
  FactoryIssueInputs::new(store.clone(), store, Arc::new(Inputs(input.clone())))
}
async fn analyze(
  db: &postgres_support::TestDatabase,
  store: &Arc<PostgresStore>,
  fixture: &AutomaticFixture,
  claim: &FactoryRunClaimRecord,
  clock: &Arc<FactoryTestClock>,
) -> Box<FlowNodeRecord> {
  let snapshot = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: fixture.input.flow_run_id(),
    cycle_id: fixture.input.cycle_id(),
    node: fixture.input.node().clone(),
  };
  let inputs = input_owner(store.clone(), &fixture.input);
  let frozen = inputs.prepare(&snapshot, &target).await.unwrap();
  let mut execution = flow_contract::Execution::for_reports(
    &[(
      target.node.clone(),
      (fixture.schema.reference().clone(), json!({"priority":"high"})),
    )]
    .into(),
  );
  let started = clock.now().unwrap().unix_millis();
  execution.clock.store(started, std::sync::atomic::Ordering::SeqCst);
  execution.seeder = Some(Arc::new(Seeder {
    pool: db.pool.clone(),
    seeded: Mutex::default(),
  }));
  let execution = Arc::new(execution);
  let runner = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution)),
    clock.clone(),
  );
  runner
    .run_once(snapshot.run.id(), claim.id, target.clone(), Some(frozen), at(started))
    .await
    .unwrap_or_else(|error| panic!("{} could not start at {started}: {error:?}", target.node));
  for now in started + 1..started + 5 {
    clock.0.store(now, std::sync::atomic::Ordering::SeqCst);
    if let FactoryNodeStep::Completed(record) = runner
      .run_once(snapshot.run.id(), claim.id, target.clone(), None, at(now))
      .await
      .unwrap()
    {
      return record;
    }
  }
  panic!("analysis must be accepted before Issue application");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_captures_invocation_baseline_and_applies_the_exact_accepted_result() {
  let (db, store, fixture, issue, claim) = fixture().await;
  // A manual edit before analysis becomes the invocation baseline, despite the older Work association.
  let mut content = issue.content.clone();
  content.priority = Some(IssuePriority::Choice("low".into()));
  let before = edit_issue(&store, &issue, content, 11).await;
  let clock = Arc::new(FactoryTestClock(std::sync::atomic::AtomicI64::new(12)));
  let writer = writer(store.clone(), &fixture, clock.clone());
  let record = analyze(&db, &store, &fixture, &claim, &clock).await;
  assert_eq!(record.input().issue_field_baseline().unwrap().version, before.version);
  assert_eq!(
    store.issue(issue.project_id, issue.id, None).await.unwrap(),
    before,
    "pending save cannot look applied"
  );
  clock.0.store(16, std::sync::atomic::Ordering::SeqCst);
  let flow = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(flow_contract::FakePending),
    Arc::new(Inputs(fixture.input.clone())),
    clock.clone(),
  );
  assert_eq!(
    flow
      .reconcile_flow(fixture.admission.run.id(), claim.id, at(16))
      .await
      .unwrap(),
    FactoryFlowStep::Resolved(key("done"))
  );
  assert_eq!(
    store
      .factory_run_snapshot(fixture.admission.run.id())
      .await
      .unwrap()
      .run
      .state(),
    FactoryRunState::Completed
  );
  assert_eq!(writer.dispatch_once(at(17)).await.unwrap().delivered, 1);
  let applied = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(applied.content.priority, Some(IssuePriority::Choice("high".into())));
  let origin = serde_json::to_value(&applied.classification).unwrap()["fields"][0]["origin"].clone();
  assert_eq!(origin["record"], json!(record.digest().unwrap()));
  assert_eq!(origin["invocation"]["node"], json!(record.input().node()));
  assert_eq!(origin["invocation"]["attempt_id"], json!(record.node_attempt_id()));
  assert_eq!(origin["invocation"]["baseline"], json!(before.version));
  assert_eq!(writer.dispatch_once(at(18)).await.unwrap().claimed, 0);
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), applied);
  db.cleanup().await;
}

struct DenyWrites;
#[async_trait]
impl ManagementAuthorizationPolicy for DenyWrites {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Err(ManagementAuthorizationDenial::forbidden())
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_save_conflict_and_denial_retain_the_accepted_analysis() {
  for denied in [false, true] {
    let (db, store, fixture, issue, claim) = fixture().await;
    let clock = Arc::new(FactoryTestClock(std::sync::atomic::AtomicI64::new(12)));
    let writer = writer(store.clone(), &fixture, clock.clone());
    let record = analyze(&db, &store, &fixture, &claim, &clock).await;
    let mut expected = issue.clone();
    if !denied {
      let mut content = issue.content.clone();
      content.priority = Some(IssuePriority::Choice("blocker".into()));
      expected = edit_issue(&store, &issue, content, 16).await;
    }
    let writer = if denied {
      writer_with(
        store.clone(),
        store.clone(),
        &fixture,
        clock.clone(),
        Arc::new(DenyWrites),
      )
    } else {
      writer
    };
    let result = writer.dispatch_once(at(17)).await.unwrap();
    assert_eq!((result.delivered, result.failed), (0, 1));
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), expected);
    let retained = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    assert_eq!(retained.flow.data.records, vec![*record]);
    let latest = retained.outbox.iter().max_by_key(|row| row.order_key()).unwrap();
    assert_eq!(
      latest.failure,
      Some(if denied {
        FactoryOutboxFailure::Forbidden
      } else {
        FactoryOutboxFailure::Conflict
      })
    );
    assert_eq!(writer.dispatch_once(at(18)).await.unwrap().claimed, 0);
    db.cleanup().await;
  }
}

struct LoseSaveAck {
  store: Arc<PostgresStore>,
  lose: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl IssueClassificationStore for LoseSaveAck {
  async fn apply_issue_classification(
    &self,
    request: ManagementMutation<ApplyIssueClassification>,
  ) -> Result<IssueMutationOutcome, StoreError> {
    let outcome = self.store.apply_issue_classification(request).await?;
    if self.lose.swap(false, std::sync::atomic::Ordering::SeqCst) {
      return Err(StoreError::Unavailable);
    }
    Ok(outcome)
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_lost_save_ack_replays_the_receipt_after_restart_without_another_build() {
  let (db, store, fixture, issue, claim) = fixture().await;
  let clock = Arc::new(FactoryTestClock(std::sync::atomic::AtomicI64::new(12)));
  let commands = Arc::new(LoseSaveAck {
    store: store.clone(),
    lose: std::sync::atomic::AtomicBool::new(true),
  });
  let writer = writer_with(
    commands.clone(),
    store.clone(),
    &fixture,
    clock.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
  );
  let record = analyze(&db, &store, &fixture, &claim, &clock).await;
  assert!(
    writer.dispatch_once(at(17)).await.is_err(),
    "lost acknowledgement leaves a recoverable claim"
  );
  let saved = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(saved.content.priority, Some(IssuePriority::Choice("high".into())));
  let mut content = saved.content.clone();
  content.priority = Some(IssuePriority::Choice("blocker".into()));
  let later = edit_issue(&store, &saved, content, 18).await;
  let mut catalog = store.issue_properties(issue.project_id, None).await.unwrap();
  catalog
    .definition
    .priorities
    .iter_mut()
    .find(|v| v.id == "high")
    .unwrap()
    .retired = true;
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: catalog.version,
        definition: catalog.definition,
      },
      idempotency_key: "after-accepted-save".parse().unwrap(),
      published_at: at(19),
    }))
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
  let prepared = input_owner(store.clone(), &fixture.input)
    .prepare(
      &snapshot,
      &FactoryNodeTarget {
        flow_run_id: record.input().flow_run_id(),
        cycle_id: record.input().cycle_id(),
        node: record.input().node().clone(),
      },
    )
    .await
    .unwrap();
  assert_eq!(
    &prepared,
    record.input(),
    "re-preparation keeps the captured invocation after Issue and catalog edits"
  );
  drop(writer);
  let restarted = Arc::new(PostgresStore::new(db.pool.clone()));
  let writer = writer_with(
    commands,
    restarted.clone(),
    &fixture,
    clock,
    Arc::new(TrustedNetworkManagementPolicy),
  );
  assert_eq!(writer.dispatch_once(at(118)).await.unwrap().delivered, 1);
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    later,
    "receipt replay cannot replace a subsequent human correction"
  );
  let history = restarted
    .factory_run_snapshot(fixture.admission.run.id())
    .await
    .unwrap();
  assert_eq!(history.flow.attempts.len(), 1);
  assert_eq!(history.flow.data.records, vec![*record]);
  assert_eq!(history.flow.data.build_intents.len(), 1);
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_form_retains_only_consumed_document_dependencies_and_rejects_replacement() {
  for remove_selected in [false, true] {
    let (db, store, mut fixture, issue, claim) = fixture().await;
    let clock = Arc::new(FactoryTestClock(std::sync::atomic::AtomicI64::new(11)));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let documents = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        ProjectTextIntake::new(
          store.clone(),
          Arc::new(DocumentBytes::default()),
          Arc::new(ExactRevisionResolver),
          policy,
          clock.clone(),
        )
        .with_issue_documents(store.clone()),
      ),
    );
    let mut references = Vec::new();
    for name in ["selected", "unselected"] {
      let doc = documents
        .handle_command(
          &context(),
          PublishIssueDocumentCommand {
            submission: IssueDocumentSubmission {
              project_id: issue.project_id,
              issue_id: issue.id,
              name: name.parse().unwrap(),
              content: format!("{name} requirements"),
              idempotency_key: name.parse().unwrap(),
            },
          },
        )
        .await
        .unwrap();
      references.push(IssueMaterialReference::IssueDocument {
        project_id: issue.project_id,
        issue_id: issue.id,
        artifact: doc.artifact,
      });
    }
    let issue = store
      .edit_issue_materials(management_mutation(EditIssueMaterials {
        intent: EditIssueMaterialsIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          references: references.clone(),
        },
        idempotency_key: "approved-docs".parse().unwrap(),
        edited_at: at(11),
      }))
      .await
      .unwrap()
      .issue;
    let IssueMaterialReference::IssueDocument { artifact, .. } = &references[0] else {
      unreachable!()
    };
    let mut entries = fixture.input.context().unwrap().entries().to_vec();
    let subject = fixture.input.task_subject().unwrap();
    entries.push(
      ContextManifestEntry::new(
        ContextSourceKind::Specification,
        key("selected-doc"),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        FactorySafeText::new("Explicit selected document").unwrap(),
        FactoryDigest::from_bytes([219; 32]),
      )
      .unwrap(),
    );
    fixture.input = fixture
      .input
      .with_context(
        ContextManifest::new(
          ContextManifestId::generate(),
          subject,
          FactoryDigest::from_bytes([219; 32]),
          entries,
        )
        .unwrap(),
        vec![],
      )
      .unwrap();
    let writer = writer(store.clone(), &fixture, clock.clone());
    let record = analyze(&db, &store, &fixture, &claim, &clock).await;
    let current = store
      .edit_issue_materials(management_mutation(EditIssueMaterials {
        intent: EditIssueMaterialsIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          references: vec![references[usize::from(remove_selected)].clone()],
        },
        idempotency_key: "remove-one-doc".parse().unwrap(),
        edited_at: at(16),
      }))
      .await
      .unwrap()
      .issue;
    let summary = writer.dispatch_once(at(17)).await.unwrap();
    let saved = store.issue(issue.project_id, issue.id, None).await.unwrap();
    if remove_selected {
      assert_eq!(
        (summary.delivered, summary.failed),
        (0, 1),
        "replaced consumed source must block saving accepted analysis"
      );
      assert_eq!(saved, current);
    } else {
      assert_eq!(summary.delivered, 1);
      let field = saved
        .classification
        .fields
        .iter()
        .find(|field| matches!(field.field, IssueClassificationFieldReference::Priority { .. }))
        .unwrap();
      assert_eq!(
        field.inputs.materials,
        vec![references[0].clone()],
        "unselected documents must not become dependencies"
      );
      let later = store
        .edit_issue_materials(management_mutation(EditIssueMaterials {
          intent: EditIssueMaterialsIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: saved.version,
            references: vec![],
          },
          idempotency_key: "remove-consumed-doc".parse().unwrap(),
          edited_at: at(18),
        }))
        .await
        .unwrap()
        .issue;
      assert!(later.assessment_problems(field, &Default::default(), None).iter().any(|problem|
        matches!(problem, IssueClassificationProblem::MaterialChanged { reference } if reference == &references[0])));
    }
    assert_eq!(
      store
        .factory_run_snapshot(fixture.admission.run.id())
        .await
        .unwrap()
        .flow
        .data
        .records,
      vec![*record]
    );
    db.cleanup().await;
  }
}

#[derive(Default)]
struct DocumentBytes(Mutex<std::collections::BTreeMap<ArtifactId, Vec<u8>>>);
#[async_trait]
impl octacity_artifact_store::ArtifactBytePublisher for DocumentBytes {
  async fn publish_verified(
    &self,
    object: &octacity_artifact_store::ArtifactObject,
    bytes: Vec<u8>,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    self.0.lock().unwrap().insert(object.artifact_id(), bytes);
    Ok(())
  }
}
#[async_trait]
impl octacity_artifact_store::ArtifactByteReader for DocumentBytes {
  async fn read_published(
    &self,
    object: &octacity_artifact_store::ArtifactObject,
    _: u64,
  ) -> Result<Vec<u8>, octacity_artifact_store::ArtifactStoreError> {
    self
      .0
      .lock()
      .unwrap()
      .get(&object.artifact_id())
      .cloned()
      .ok_or(octacity_artifact_store::ArtifactStoreError::NotFound)
  }
}
