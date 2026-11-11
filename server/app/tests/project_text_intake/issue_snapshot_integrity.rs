//! Complete snapshot identity is verified before publishing any Work/Run.
use super::*;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admission_rejects_substituted_payload_with_the_original_selection_digest() {
  assert_substitution(false).await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn snapshot_admission_rejects_a_snapshot_substituted_from_another_issue_request() {
  assert_substitution(true).await;
}

async fn assert_substitution(another_request: bool) {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup_with_flow(&store, Some(snapshot_flow())).await;
  let request = snapshot_request(store.clone(), &issue, &submission).await;
  let bytes = Arc::new(PausedBytes::new());
  let publisher = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueExecutionPublisher::new(
      store.clone(),
      store.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(0))),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock(AtomicI64::new(20))),
    )),
  );
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification.clone(),
  };
  let publication = tokio::spawn(async move { publisher.handle_command(&context(), command).await });
  tokio::time::timeout(std::time::Duration::from_secs(10), bytes.reached.notified())
    .await
    .unwrap();
  publication.abort();
  assert!(publication.await.unwrap_err().is_cancelled());
  let preparation = store
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
  let preparation = if another_request {
    let other = store
      .create_issue(management_mutation(CreateIssue {
        id: IssueId::generate(),
        assignees: Default::default(),
        intent: CreateIssueIntent {
          project_id: issue.project_id,
          content: issue.content.clone(),
          planning: Default::default(),
        },
        idempotency_key: key("other-issue"),
        created_at: at(21),
      }))
      .await
      .unwrap()
      .issue;
    let reviewed = snapshot_request(store.clone(), &other, &submission).await;
    let mut intent = preparation.intent.clone();
    intent.idempotency_key = reviewed.admission_key();
    intent.external_identity = reviewed.external_identity();
    // The selected documents still belong to the first Issue; the second request
    // has its own independently sealed snapshot digest despite identical prose.
    let reserved = store
      .reserve_project_text(management_mutation(ReserveProjectText {
        submission: ProjectTextSubmission {
          intent,
          documents: documents.clone(),
        },
        now: at(22),
      }))
      .await
      .unwrap();
    store
      .freeze_project_text(
        reserved.guard(at(22)),
        ImmutableRevision::new("original-base").unwrap(),
        Vec::new(),
      )
      .await
      .unwrap()
  } else {
    preparation
  };
  let now = preparation.claimed_at;
  let preparation = store.publish_project_text(preparation.guard(now)).await.unwrap();
  let mut admission = prepared_admission(&store, &preparation, &documents).await;
  if !another_request {
    let schema = admission
      .flow
      .data_schema(&documents.issue_snapshot().unwrap().input.schema)
      .unwrap();
    let header = admission.incoming.as_ref().unwrap().issue_snapshot().unwrap().clone();
    admission.incoming = Some(
      FlowIncomingData::from_issue_snapshot(
        &admission.work,
        &admission.flow,
        FlowPayload::new(schema, serde_json::json!({"title":"Substituted requirements"})).unwrap(),
        header,
      )
      .unwrap(),
    );
  }
  let result = store
    .admit_project_text(preparation.guard(now), management_mutation(admission))
    .await;
  assert!(
    matches!(result, Err(StoreError::Conflict { .. })),
    "matching digest must not authorize a different payload: {result:?}"
  );
  let runs = store
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
