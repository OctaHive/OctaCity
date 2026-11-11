//! Pending input stays fenced; admitted input can recover without private bodies.
use super::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_pending_restart_rejects_later_edits_without_refreshing_its_input() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = replay_fixture(&store).await;
  let request = snapshot_request(store.clone(), &issue, &submission).await;
  let bytes = Arc::new(Bytes::new());
  bytes.lose_ack.store(true, Ordering::SeqCst);
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification,
  };
  assert!(
    publisher(
      store.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(0))),
      Arc::new(Clock(AtomicI64::new(20)))
    )
    .handle_command(&context(), command.clone())
    .await
    .is_err()
  );
  let original = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let documents = store
    .project_text_documents(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let later = change_reviewed_issue(&store, &issue).await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let error = publisher(
    restarted.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    Arc::new(Clock(AtomicI64::new(40))),
  )
  .handle_command(&context(), command)
  .await
  .expect_err("restart cannot authorize an outdated unadmitted snapshot");
  assert_eq!(
    error.application().unwrap().classification(),
    ApplicationFailure::Conflict
  );
  let retained = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(retained.admission.is_none());
  assert_eq!(retained.objects, original.objects);
  assert_eq!(retained.frozen_revision, original.frozen_revision);
  assert_eq!(retained.intent, original.intent);
  assert_eq!(retained.expires_at, original.expires_at);
  assert_eq!(
    restarted
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key()
      )
      .await
      .unwrap()
      .unwrap(),
    documents
  );
  assert!(
    restarted
      .issue_execution(issue.project_id, issue.id, request.id)
      .await
      .unwrap()
      .association
      .is_none()
  );
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), later);
  let runs = restarted
    .list_factory_runs(
      ListFactoryRuns::new(
        Some(issue.project_id),
        FactoryRunFilter::default(),
        None,
        10,
        FactoryRunListVisibility::all(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(runs.items.is_empty());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admitted_restart_recovers_after_private_cleanup_without_resurrection() {
  for stage in ["admitted", "associated"] {
    let database = postgres::TestDatabase::migrated().await;
    let store = Arc::new(PostgresStore::new(database.pool.clone()));
    let (issue, submission) = replay_fixture(&store).await;
    let request = snapshot_request(store.clone(), &issue, &submission).await;
    let directory = tempfile::tempdir().unwrap();
    let retained = publication_crash::crash_at(
      &database,
      &store,
      &issue,
      &submission,
      &request,
      directory.path(),
      stage,
    )
    .await;
    let admitted = retained.admission.as_ref().unwrap();
    let before = store.factory_run_snapshot(admitted.run.id()).await.unwrap();
    let later = change_reviewed_issue(&store, &issue).await;
    let bytes = Arc::new(Bytes::shared(directory.path().into(), 0));
    assert_eq!(
      FactoryExecutionInputRetentionWorker::new(store.clone(), bytes.clone())
        .run_once(retained.expires_at, 10)
        .await
        .unwrap(),
      2
    );
    assert!(
      store
        .project_text_documents(
          issue.project_id,
          &ManagementSecurityScope::trusted_network(),
          &request.admission_key()
        )
        .await
        .unwrap()
        .is_none()
    );
    let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
    let handler = publisher(
      restarted.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(1))),
      Arc::new(Clock(AtomicI64::new(retained.expires_at.unix_millis()))),
    );
    let command = PublishIssueExecutionCommand {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: request.id,
      classification: submission.intent.classification.clone(),
    };
    let recovered = handler.handle_command(&context(), command.clone()).await.unwrap();
    let association = recovered.association.as_ref().unwrap();
    assert_eq!(association.work_id, admitted.work.id());
    assert_eq!(association.run_id, admitted.run.id());
    assert_eq!(association.requirements_version, Some(issue.version));
    assert_eq!(
      handler.handle_command(&context(), command.clone()).await.unwrap(),
      recovered
    );
    let mut changed = command;
    changed.classification = WorkClassification::new(WorkPriority::new(2).unwrap(), RiskClass::Low, Default::default());
    assert!(handler.handle_command(&context(), changed).await.is_err());
    let run = restarted.factory_run_snapshot(association.run_id).await.unwrap();
    assert_eq!(run, before);
    assert_eq!(
      run.flow.data.incoming[0].issue_snapshot().unwrap().digest,
      request.snapshot_digest.unwrap()
    );
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({"title":"Задача", "priority":"high", "size":"small"})
    );
    assert!(
      restarted
        .project_text_documents(
          issue.project_id,
          &ManagementSecurityScope::trusted_network(),
          &request.admission_key()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(bytes.writes.load(Ordering::SeqCst), 0);
    assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), later);
    database.cleanup().await;
  }
}
