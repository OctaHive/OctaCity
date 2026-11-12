//! Published bounded material input consumes only explicitly approved canonical Issue text.
use super::*;

#[path = "issue_snapshot_material_policy_tests.rs"]
mod policy;

fn material_flow() -> FactoryFlowConfiguration {
  snapshot_flow_with_inputs(
    false,
    Some(FlowIssueMaterialSelection {
      source: FlowIssueMaterialSource::Selected { max_entries: 3 },
      required: true,
      max_bytes: 4096,
    }),
  )
}

fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
}

fn worker(f: &Fixture<PostgresStore>) -> FactorySourceWorker<PostgresStore, Bytes> {
  worker_with_policy(f, Arc::new(TrustedNetworkManagementPolicy))
}

fn worker_with_policy(
  f: &Fixture<PostgresStore>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> FactorySourceWorker<PostgresStore, Bytes> {
  f.worker()
    .with_issue_snapshots(crate::runtime::sources::source_issue_snapshots::reader(
      f.store.clone(),
      f.bytes.clone(),
      Arc::new(ExactRevisionResolver),
      policy,
      f.clock.clone(),
    ))
}

async fn comment(f: &Fixture<PostgresStore>, issue: &IssueRevision, body: &str, replay: &str) -> IssueCommentMetadata {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueDiscussionHandlers::new(f.store.clone(), policy, f.clock.clone())),
  )
  .handle_command(
    &context(),
    AppendIssueCommentCommand {
      intent: AppendIssueCommentIntent {
        project_id: issue.project_id,
        issue_id: issue.id,
        parent_id: None,
        body: IssueCommentBody::try_new(body).unwrap(),
      },
      idempotency_key: IdempotencyKey::new(replay).unwrap(),
    },
  )
  .await
  .unwrap()
  .comment
}

async fn pending(f: &Fixture<PostgresStore>) -> WorkSourcePreparation {
  f.store
    .pending_source_preparations(
      f.input.project_id(),
      WorkSourceScanTarget::Input {
        id: f.input.id(),
        version: f.input.version(),
      },
      None,
      10,
    )
    .await
    .unwrap()
    .remove(0)
}

fn documents(f: &Fixture<PostgresStore>) -> Arc<ProjectTextIntake<PostgresStore, Bytes>> {
  Arc::new(
    ProjectTextIntake::new(
      f.store.clone(),
      f.bytes.clone(),
      Arc::new(ExactRevisionResolver),
      Arc::new(TrustedNetworkManagementPolicy),
      f.clock.clone(),
    )
    .with_issue_documents(f.store.clone()),
  )
}

async fn publish_documents(
  f: &Fixture<PostgresStore>,
  issue: &IssueRevision,
  version: u8,
) -> (IssueDocumentResource, ProjectDocumentResource) {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let commands = AuthorizedCommandHandler::new(policy, documents(f));
  let owned = commands
    .handle_command(
      &context(),
      PublishIssueDocumentCommand {
        submission: IssueDocumentSubmission {
          project_id: issue.project_id,
          issue_id: issue.id,
          name: "Issue delivery notes".parse().unwrap(),
          content: format!("ISSUE_DOCUMENT_{version}\r\nточный текст\n"),
          idempotency_key: IdempotencyKey::new(format!("owned-{version}")).unwrap(),
        },
      },
    )
    .await
    .unwrap();
  let shared = commands
    .handle_command(
      &context(),
      PublishProjectDocumentCommand {
        submission: ProjectDocumentSubmission {
          project_id: issue.project_id,
          name: "Shared delivery notes".parse().unwrap(),
          content: format!("PROJECT_DOCUMENT_{version}\r\nreference\n"),
          purpose: ProjectDocumentPurpose::Context,
          idempotency_key: IdempotencyKey::new(format!("shared-{version}")).unwrap(),
        },
      },
    )
    .await
    .unwrap();
  (owned, shared)
}

async fn select(
  f: &Fixture<PostgresStore>,
  issue: &IssueRevision,
  references: Vec<IssueMaterialReference>,
  replay: &str,
) -> IssueRevision {
  let current = f.store.issue(issue.project_id, issue.id, None).await.unwrap();
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueMaterialHandlers::new(
      f.store.clone(),
      f.store.clone(),
      policy,
      f.clock.clone(),
    )),
  )
  .handle_command(
    &context(),
    EditIssueMaterialsCommand {
      intent: EditIssueMaterialsIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: current.version,
        references,
      },
      idempotency_key: IdempotencyKey::new(replay).unwrap(),
    },
  )
  .await
  .unwrap()
  .issue
}

fn references(
  issue: &IssueRevision,
  comment: &IssueCommentMetadata,
  owned: &IssueDocumentResource,
  shared: &ProjectDocumentResource,
) -> Vec<IssueMaterialReference> {
  vec![
    IssueMaterialReference::Comment {
      project_id: issue.project_id,
      issue_id: issue.id,
      comment_id: comment.id,
      body_digest: comment.body_digest,
    },
    IssueMaterialReference::IssueDocument {
      project_id: issue.project_id,
      issue_id: issue.id,
      artifact: owned.artifact.clone(),
    },
    IssueMaterialReference::ProjectDocument {
      project_id: issue.project_id,
      artifact: shared.artifact.clone(),
    },
  ]
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_required_material_policy_waits_for_approval_without_ingesting_discussion() {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(material_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let issue = linked_issue(&f, "TASK-native", true).await;
  comment(
    &f,
    &issue,
    "UNAPPROVED discussion must not satisfy required input",
    "unapproved",
  )
  .await;
  assert!(f.runs().await.is_empty());
  let outcome = worker(&f).run_once(CancellationToken::new()).await.unwrap();
  assert_eq!(
    (outcome.completed, outcome.deferred),
    (1, 0),
    "required material is a candidate-local wait"
  );
  let waiting = pending(&f).await;
  assert_eq!(
    waiting.waiting.as_ref().map(|wait| wait.reason),
    Some(WorkSourceWaitReason::Inaccessible)
  );
  assert!(waiting.intent.is_none());
  assert!(waiting.issue_input.is_none());
  assert!(f.runs().await.is_empty());
  assert!(f.bytes.0.lock().unwrap().is_empty());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_approved_materials_resume_once_and_survive_new_publications_selection_and_restart() {
  let database = TestDatabase::migrated().await;
  let mut f = Fixture::enabled_with_flow("github", Some(material_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let issue = linked_issue(&f, "TASK-native", true).await;
  assert_eq!(
    worker(&f).run_once(CancellationToken::new()).await.unwrap().completed,
    1
  );
  let waiting = pending(&f).await;
  let approved = comment(&f, &issue, "COMMENT_SELECTED\r\nёж\n", "approved").await;
  comment(&f, &issue, "UNSELECTED_DISCUSSION", "unselected").await;
  let (owned, shared) = publish_documents(&f, &issue, 1).await;
  assert!(
    f.store
      .issue(issue.project_id, issue.id, None)
      .await
      .unwrap()
      .materials
      .is_empty(),
    "publication does not approve input"
  );
  let original_references = references(&issue, &approved, &owned, &shared);
  let selected = select(&f, &issue, original_references.clone(), "approve-materials").await;
  assert!(f.runs().await.is_empty(), "selection alone cannot start execution");
  f.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    worker(&f).run_once(CancellationToken::new()).await.unwrap().completed,
    1
  );
  let runs = f.runs().await;
  assert_eq!(
    runs.len(),
    1,
    "approved canonical text must release the pending Source input"
  );
  let original = f.store.factory_run_snapshot(runs[0].id).await.unwrap();
  assert_eq!(
    original.flow.data.incoming[0].payload().value(),
    &serde_json::json!({
      "title":"Canonical TASK-native", "size":"small", "priority":"high",
      "materials":["COMMENT_SELECTED\r\nёж\n", "ISSUE_DOCUMENT_1\r\nточный текст\n", "PROJECT_DOCUMENT_1\r\nreference\n"]
    })
  );
  assert!(original.flow.data.incoming[0].predecessor().is_none());
  let prepared = f.store.source_preparation(issue.project_id, waiting.id).await.unwrap();
  let frozen = prepared.issue_input.as_ref().unwrap().issue_snapshot().unwrap();
  assert_eq!(frozen.version, selected.version);
  assert_eq!(
    frozen.materials[&key("materials")][0].source,
    IssueSnapshotMaterialSource::Comment {
      comment: approved.clone()
    }
  );
  assert_eq!(
    frozen.materials[&key("materials")][1].source,
    IssueSnapshotMaterialSource::IssueDocument {
      document: owned.clone()
    }
  );
  assert_eq!(
    frozen.materials[&key("materials")][2].source,
    IssueSnapshotMaterialSource::ProjectDocument {
      document: shared.clone()
    }
  );
  assert_eq!(
    frozen.materials[&key("materials")]
      .iter()
      .map(|entry| entry.selection.reference.clone())
      .collect::<Vec<_>>(),
    original_references
  );
  assert_eq!(prepared.admission.as_ref().unwrap().run_id, original.run.id());
  let association = f
    .store
    .issue_work_association(issue.project_id, original.work.id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(association.issue_id, issue.id);
  assert_eq!(association.requirements_version, Some(selected.version));
  let manifest = f
    .store
    .source_materialization(issue.project_id, waiting.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(manifest.objects.len(), 2);
  let original_inputs = f.bytes.0.lock().unwrap().clone();

  f.clock.0.store(120, Ordering::SeqCst);
  let later = comment(
    &f,
    &issue,
    "NEW_COMMENT must not replace admitted input",
    "later-comment",
  )
  .await;
  let (new_owned, new_shared) = publish_documents(&f, &issue, 2).await;
  assert_ne!(new_owned.artifact, owned.artifact);
  assert_ne!(new_shared.artifact, shared.artifact);
  assert_eq!(
    f.store.issue(issue.project_id, issue.id, None).await.unwrap().materials,
    selected.materials,
    "new publications do not replace references"
  );
  select(
    &f,
    &issue,
    references(&issue, &later, &new_owned, &new_shared),
    "replace-materials",
  )
  .await;
  let writes_after_publications = f.bytes.0.lock().unwrap().clone();
  assert_eq!(writes_after_publications.len(), original_inputs.len() + 2);
  for now in [130, 140] {
    f.clock.0.store(now, Ordering::SeqCst);
    f.store = Arc::new(PostgresStore::new(database.pool.clone()));
    assert_eq!(
      worker(&f).run_once(CancellationToken::new()).await.unwrap().completed,
      1
    );
    assert_eq!(f.runs().await.len(), 1);
    assert_eq!(f.store.factory_run_snapshot(original.run.id()).await.unwrap(), original);
    let retained = f.store.source_preparation(issue.project_id, waiting.id).await.unwrap();
    assert_eq!(retained.issue_input, prepared.issue_input);
    assert_eq!(retained.admission, prepared.admission);
    assert_eq!(
      f.store
        .source_materialization(issue.project_id, waiting.id)
        .await
        .unwrap()
        .unwrap()
        .objects,
      manifest.objects
    );
    assert_eq!(
      *f.bytes.0.lock().unwrap(),
      writes_after_publications,
      "replay must publish no new input bytes"
    );
  }
  database.cleanup().await;
}
