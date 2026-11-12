//! Candidate-local readiness diagnostics never discover a substitute producing Run.
use super::*;
use std::sync::atomic::AtomicUsize;

#[derive(Default)]
struct PredecessorReads(AtomicUsize);
#[async_trait::async_trait]
impl FactorySourcePredecessors for PredecessorReads {
  async fn resolve(
    &self,
    _: &FactorySourceAuthorization,
    _: &FactorySourceItem,
    _: FactorySourcePredecessorTarget<'_>,
  ) -> Result<FactorySourceHandoff, ApplicationError> {
    self.0.fetch_add(1, Ordering::SeqCst);
    panic!("Issue snapshots must never search for a producing Run")
  }
}

fn worker(f: &Fixture<PostgresStore>, reads: Arc<PredecessorReads>) -> FactorySourceWorker<PostgresStore, Bytes> {
  f.worker()
    .with_issue_snapshots(Arc::new(IssueSourceSnapshotReader::new(
      f.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    )))
    .with_predecessors(reads)
}

async fn assert_wait(f: &Fixture<PostgresStore>, reason: WorkSourceWaitReason) -> WorkSourcePreparation {
  let target = WorkSourceScanTarget::Input {
    id: f.input.id(),
    version: f.input.version(),
  };
  let scan = f
    .store
    .source_scan_schedule(f.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert!(
    scan.failure.is_none(),
    "one unavailable Issue must not fail the Source: {:?}",
    scan.failure
  );
  let preparations = f
    .store
    .pending_source_preparations(f.input.project_id(), target, None, 10)
    .await
    .unwrap();
  assert_eq!(preparations.len(), 1);
  let preparation = preparations.into_iter().next().unwrap();
  assert_eq!(preparation.waiting.as_ref().map(|wait| wait.reason), Some(reason));
  assert!(preparation.predecessor_cursor.is_none());
  assert!(preparation.admission.is_none());
  let diagnostics = f
    .store
    .source_scan_diagnostics(f.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(diagnostics.last_wait.unwrap().reason, reason);
  let public_wait = serde_json::to_value(diagnostics.last_wait.unwrap()).unwrap();
  assert_eq!(
    public_wait.as_object().unwrap().len(),
    3,
    "wait diagnostics expose only identity, time and reason"
  );
  assert!(public_wait.get("preparation_id").is_some());
  assert!(public_wait.get("checked_at").is_some());
  assert!(public_wait.get("reason").is_some());
  preparation
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_required_evidence_waits_without_producer_fallback_or_writes() {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(snapshot_flow_with_evidence(true)))
    .await
    .into_postgres(database.pool.clone())
    .await;
  linked_issue(&f, "TASK-native", true).await;
  let reads = Arc::new(PredecessorReads::default());
  let outcome = worker(&f, reads.clone())
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  assert_eq!((outcome.completed, outcome.deferred), (1, 0));
  assert_wait(&f, WorkSourceWaitReason::Inaccessible).await;
  assert!(f.runs().await.is_empty());
  assert!(f.bytes.0.lock().unwrap().is_empty());
  assert_eq!(reads.0.load(Ordering::SeqCst), 0);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_missing_link_and_stale_requirements_wait_without_fallback() {
  for stale in [false, true] {
    let database = TestDatabase::migrated().await;
    let f = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
      .await
      .into_postgres(database.pool.clone())
      .await;
    if stale {
      let issue = linked_issue(&f, "TASK-native", true).await;
      f.store
        .edit_issue(management_mutation(EditIssue {
          intent: EditIssueIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            content: IssueContent {
              description: "Requirements changed after assessment".into(),
              ..issue.content
            },
          },
          idempotency_key: IdempotencyKey::new("stale-requirements").unwrap(),
          edited_at: time(40),
        }))
        .await
        .unwrap();
    } else {
      // A ready but differently correlated Issue must not be guessed from the Project.
      linked_issue(&f, "TASK-other", true).await;
    }
    let reads = Arc::new(PredecessorReads::default());
    let outcome = worker(&f, reads.clone())
      .run_once(CancellationToken::new())
      .await
      .unwrap();
    assert_eq!((outcome.completed, outcome.deferred), (1, 0));
    let waiting = assert_wait(
      &f,
      if stale {
        WorkSourceWaitReason::Stale
      } else {
        WorkSourceWaitReason::Missing
      },
    )
    .await;
    assert!(waiting.intent.is_none());
    assert!(f.runs().await.is_empty());
    assert!(f.bytes.0.lock().unwrap().is_empty());
    assert_eq!(reads.0.load(Ordering::SeqCst), 0);
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_incompatible_code_subject_waits_with_existing_run_without_fallback() {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let issue = linked_issue(&f, "TASK-native", true).await;
  let reads = Arc::new(PredecessorReads::default());
  // A real reviewed manual execution supplies trusted classification provenance.
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let requested = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueExecutionHandlers::new(
      f.store.clone(),
      f.store.clone(),
      policy.clone(),
    )),
  )
  .handle_command(
    &context,
    RequestIssueExecutionCommand {
      intent: RequestIssueExecutionIntent {
        project_id: issue.project_id,
        issue_id: issue.id,
        expected_version: issue.version,
        classification: None,
        target: IssueExecutionTarget {
          configuration_id: f.input.definition().target.id(),
          configuration_version: f.input.definition().target.version(),
          repository_id: f.input.definition().repository.id,
          repository_version: f.input.definition().repository.version,
          source: f.profile.preparation.source.clone(),
          mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
          predecessor_run: None,
        },
      },
      idempotency_key: IdempotencyKey::new("review-code-subject").unwrap(),
      requested_at: time(90),
    },
  )
  .await
  .unwrap()
  .request;
  AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueExecutionPublisher::new(
      f.store.clone(),
      f.store.clone(),
      f.bytes.clone(),
      Arc::new(ExactRevisionResolver),
      policy,
      f.clock.clone(),
    )),
  )
  .handle_command(
    &context,
    PublishIssueExecutionCommand {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: requested.id,
      classification: f.profile.preparation.classification.clone(),
    },
  )
  .await
  .unwrap();
  let original = f.store.factory_run_snapshot(f.runs().await[0].id).await.unwrap();
  let association = f
    .store
    .issue_work_association(issue.project_id, original.work.id())
    .await
    .unwrap()
    .unwrap();
  f.store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition: IssuePropertyDefinitionSet::default(),
      },
      idempotency_key: IdempotencyKey::new("subject-catalog").unwrap(),
      published_at: time(101),
    }))
    .await
    .unwrap();
  let mut classification = ApplyIssueClassification {
    association,
    policy: key("code-dependent-size"),
    record: digest(181),
    operation_id: digest(182),
    accepted_at: time(104),
    applied_at: time(105),
    priority: None,
    size_label: None,
    managed_labels: Default::default(),
    authority: None,
    fields: Some(IssueClassificationUpdate {
      selection: IssueClassificationFieldSelection {
        definition_version: 1,
        priority: false,
        component: false,
        kind: false,
        size: true,
        properties: Default::default(),
      },
      priority: None,
      component: None,
      issue_type: None,
      size: Some(IssueSize::Medium),
      properties: Default::default(),
      inputs: IssueClassificationAssessmentInputs {
        subject: Some(FactoryTaskSubject::Exact(ExactSubject::new(
          issue.project_id,
          f.input.definition().repository.id,
          ImmutableRevision::new("another-base").unwrap(),
        ))),
        materials: vec![],
      },
    }),
  };
  f.store
    .apply_issue_classification(management_mutation(classification.clone()))
    .await
    .unwrap();
  f.clock.0.store(110, Ordering::SeqCst);
  let outcome = worker(&f, reads.clone())
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  assert_eq!((outcome.completed, outcome.deferred), (1, 0));
  let waiting = assert_wait(&f, WorkSourceWaitReason::WrongSubject).await;
  assert_eq!(
    f.runs().await.len(),
    1,
    "the incompatible consumer must not admit another Run"
  );
  assert_eq!(reads.0.load(Ordering::SeqCst), 0);
  // Reassessment for the independently resolved base releases the same preparation.
  classification.record = digest(183);
  classification.operation_id = digest(184);
  classification.accepted_at = time(115);
  classification.applied_at = time(116);
  classification.fields.as_mut().unwrap().inputs.subject = Some(FactoryTaskSubject::Exact(
    original.work.subject().repository().unwrap().clone(),
  ));
  f.store
    .apply_issue_classification(management_mutation(classification))
    .await
    .unwrap();
  f.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    worker(&f, reads.clone())
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert_eq!(f.runs().await.len(), 2);
  let resumed = f.store.source_preparation(issue.project_id, waiting.id).await.unwrap();
  assert!(resumed.waiting.is_none());
  let admitted = resumed.admission.unwrap();
  let run = f.store.factory_run_snapshot(admitted.run_id).await.unwrap();
  assert!(run.flow.data.incoming[0].predecessor().is_none());
  assert_eq!(run.flow.data.incoming[0].payload().value()["size"], "medium");
  assert_eq!(reads.0.load(Ordering::SeqCst), 0);
  database.cleanup().await;
}
