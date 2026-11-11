//! Durable snapshot input survives unknown acknowledgements and process restart.
use super::*;
#[path = "issue_snapshot_replay_fixture.rs"]
mod fixture;
use fixture::*;
#[path = "issue_snapshot_replay_retention.rs"]
mod retention;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_byte_ack_recovery_reuses_original_inputs_and_resolved_code_after_restart() {
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
    classification: submission.intent.classification.clone(),
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
  let retained = store
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
  assert!(retained.admission.is_none());
  assert_eq!(retained.frozen_revision.as_ref().unwrap().as_str(), "original-base");
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let recovered = publisher(
    restarted.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    Arc::new(Clock(AtomicI64::new(21))),
  )
  .handle_command(&context(), command.clone())
  .await
  .unwrap();
  let association = recovered.association.as_ref().unwrap();
  let run = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  assert_eq!(
    run.flow.data.incoming[0].payload().value(),
    &serde_json::json!({"title":"Задача", "priority":"high", "size":"small"})
  );
  assert_eq!(
    run.flow.data.incoming[0].issue_snapshot().unwrap().digest,
    request.snapshot_digest.unwrap()
  );
  assert_eq!(
    run.work.subject().repository().unwrap().base_revision(),
    retained.frozen_revision.as_ref().unwrap()
  );
  let later = change_reviewed_issue(&restarted, &issue).await;
  let after_edit = Arc::new(PostgresStore::new(database.pool.clone()));
  let replayed = publisher(
    after_edit.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    Arc::new(Clock(AtomicI64::new(40))),
  )
  .handle_command(&context(), command)
  .await
  .unwrap();
  assert_eq!(snapshot_request(after_edit.clone(), &issue, &submission).await, request);
  assert_eq!(replayed, recovered);
  assert_eq!(after_edit.factory_run_snapshot(association.run_id).await.unwrap(), run);
  assert_eq!(
    after_edit
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
  assert_eq!(after_edit.issue(issue.project_id, issue.id, None).await.unwrap(), later);
  assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_process_restart_keeps_original_input_at_each_publication_boundary() {
  for stage in ["first-byte", "both-bytes", "published", "admitted", "associated"] {
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
    let documents = store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let snapshot = documents.issue_snapshot().unwrap();
    assert_eq!(
      snapshot.values,
      serde_json::from_value(serde_json::json!({"title":"Задача", "priority":"high", "size":"small"})).unwrap()
    );
    assert_eq!(snapshot.definition_head, Some(0));
    assert_eq!(documents.snapshot_digest().unwrap(), request.snapshot_digest);
    let pending = store
      .issue_execution(issue.project_id, issue.id, request.id)
      .await
      .unwrap();
    assert_eq!(pending.association.is_some(), stage == "associated");
    // Edits before admission still require a fresh review (task 3.3). Once
    // admission committed, they must not prevent recovery of that same Run.
    let later = if retained.admission.is_some() {
      Some(change_reviewed_issue(&store, &issue).await)
    } else {
      None
    };
    let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
    let bytes = Arc::new(Bytes::shared(directory.path().into(), 0));
    let handler = publisher(
      restarted.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(1))),
      Arc::new(Clock(AtomicI64::new(30_020))),
    );
    let command = PublishIssueExecutionCommand {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: request.id,
      classification: submission.intent.classification.clone(),
    };
    let recovered = handler.handle_command(&context(), command.clone()).await.unwrap();
    let association = recovered.association.as_ref().unwrap();
    if let Some(admission) = retained.admission.as_ref() {
      assert_eq!(association.work_id, admission.work.id(), "{stage}: original Work");
      assert_eq!(association.run_id, admission.run.id(), "{stage}: original Run");
    }
    assert_eq!(association.requirements_version, Some(issue.version));
    let run = restarted.factory_run_snapshot(association.run_id).await.unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({"title":"Задача", "priority":"high", "size":"small"})
    );
    assert_eq!(
      run.flow.data.incoming[0].issue_snapshot().unwrap().digest,
      request.snapshot_digest.unwrap()
    );
    assert_eq!(
      run.work.subject().repository().unwrap().base_revision().as_str(),
      "original-base"
    );
    let original = restarted
      .project_text_preparation(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(original.objects, retained.objects);
    assert_eq!(original.frozen_revision, retained.frozen_revision);
    assert_eq!(original.expires_at, retained.expires_at);
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
    let later = match later {
      Some(later) => later,
      None => change_reviewed_issue(&restarted, &issue).await,
    };
    assert_eq!(handler.handle_command(&context(), command).await.unwrap(), recovered);
    assert_eq!(restarted.factory_run_snapshot(association.run_id).await.unwrap(), run);
    assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), later);
    assert_eq!(bytes.writes.load(Ordering::SeqCst), usize::from(stage == "first-byte"));
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
    assert_eq!(runs.items.len(), 1, "{stage}: no second processing cycle");
    database.cleanup().await;
  }
}
