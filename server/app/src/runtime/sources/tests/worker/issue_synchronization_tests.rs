//! Retained synchronization reads compose the real Issue publication and Reporter journal.
use super::*;
use octacity_server_application::{IssueSynchronizationHandlers, ListIssueSourceProjectionsQuery};
use octacity_server_store::{IssueSourceResolutionReview, IssueSourceResolutionStore, ResolveIssueSourceConflict};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_synchronization_reads_original_pending_effect_without_dispatch_or_execution() {
  let (fixture, database, store) = standalone("sync-read").await;
  let (_, _, command) = manual_issue(&store, &fixture).await;
  let accepted = store.transition_issue(management_mutation(command)).await.unwrap();
  let queries = AuthorizedQueryHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueSynchronizationHandlers::new(store.clone(), fixture.clock.clone())),
  );
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let page = queries
    .handle_query(
      &context,
      ListIssueSourceProjectionsQuery::try_new(accepted.issue.project_id, accepted.issue.id, None, 1).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(page.items.len(), 1);
  assert!(page.next_cursor.is_none());
  let row = serde_json::to_value(&page.items[0]).unwrap();
  assert_eq!(row["state"], "pending");
  assert_eq!(row["issue_version"], serde_json::json!(accepted.issue.version));
  assert_eq!(row["effects"]["add"], serde_json::json!(["custom-ready"]));
  assert!(row["pipeline"].is_null());
  assert!(row["resolves"].is_null());
  let wire = serde_json::to_string(&page).unwrap();
  for private in [
    "private-reporter",
    "idempotency_key",
    "security_scope",
    "writer_access",
    "No Factory target",
  ] {
    assert!(!wire.contains(private), "private publication material: {private}");
  }
  assert_eq!(
    store
      .issue(accepted.issue.project_id, accepted.issue.id, None)
      .await
      .unwrap(),
    accepted.issue
  );
  assert!(!fixture.root.path().join("sync-read/reporter-requests.jsonl").exists());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_synchronization_pages_exact_original_receipts_under_current_scope() {
  let (fixture, database, store) = standalone("sync-history").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command))
    .await
    .unwrap()
    .issue;
  let owner = Arc::new(IssueSynchronizationHandlers::new(store.clone(), fixture.clock.clone()));
  let queries = AuthorizedQueryHandler::new(
    Arc::new(SynchronizationVisibility(ManagementVisibility::all())),
    owner.clone(),
  );
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let read = |after| ListIssueSourceProjectionsQuery::try_new(accepted.project_id, accepted.id, after, 1).unwrap();
  let pending = queries
    .handle_query(&context, read(None))
    .await
    .unwrap()
    .items
    .remove(0);
  let root = fixture.root.path().join("sync-history");
  std::fs::write(root.join("reporter-mode.json"), r#"{"conflict_on_apply":true}"#).unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    issue_worker(&fixture, store.clone(), vec![profile])
      .run_once(time(120))
      .await
      .unwrap()
      .failed,
    1
  );
  let original = queries
    .handle_query(&context, read(None))
    .await
    .unwrap()
    .items
    .remove(0);
  assert_eq!(original.operation_id, pending.operation_id);
  assert_ne!(original.record_id, pending.record_id);
  assert_eq!(
    original.failure,
    Some(octacity_server_store::FactoryOutboxFailure::Conflict)
  );
  let scope = octacity_server_store::ManagementSecurityScope::trusted_network();
  let mapping = store
    .issue_source_mapping(accepted.project_id, accepted.id, &scope, None)
    .await
    .unwrap()
    .unwrap();
  let resolved = store
    .resolve_issue_source_conflict(management_mutation(ResolveIssueSourceConflict {
      review: IssueSourceResolutionReview {
        operation_id: original.operation_id,
        expected_record: original.record_id,
      },
      publication: PublishIssueSourceMapping {
        writer_access: None,
        intent: PublishIssueSourceMappingIntent {
          project_id: accepted.project_id,
          issue_id: accepted.id,
          expected_issue_version: accepted.version,
          expected_mapping: Some(mapping.reference()),
          definition: mapping.definition.clone(),
        },
        native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
        readable: WorkSourceReportingPolicy {
          ownership: None,
          labels: vec![],
          states: vec![],
        },
        writable: Some(WorkSourceReportingPolicy {
          ownership: None,
          labels: vec!["custom-ready".into()],
          states: vec![],
        }),
        idempotency_key: IdempotencyKey::new("history-resolution").unwrap(),
        published_at: time(200),
      },
    }))
    .await
    .unwrap();
  let page = queries.handle_query(&context, read(None)).await.unwrap();
  let cursor = page.next_cursor.clone().expect("bounded history continuation");
  let last = queries
    .handle_query(&context, read(Some(cursor.clone())))
    .await
    .unwrap();
  assert!(last.next_cursor.is_none());
  assert_eq!(last.items.len(), 1);
  assert_ne!(page.items[0].operation_id, last.items[0].operation_id);
  let all = [page.items[0].clone(), last.items[0].clone()];
  let old = all.iter().find(|r| r.operation_id == original.operation_id).unwrap();
  assert_eq!(old.mapping, mapping.reference());
  assert_eq!(old.record_id, original.record_id);
  let new = all
    .iter()
    .find(|r| Some(r.operation_id) == resolved.projection)
    .unwrap();
  assert_eq!(new.mapping, resolved.mapping.reference());
  assert_eq!(new.resolves, Some(old.operation_id));
  assert_eq!(new.issue_version, old.issue_version);
  assert!(all.iter().all(|r| r.effects.as_ref().unwrap().add == ["custom-ready"]));
  let foreign = ManagementRequestContext::new(
    ManagementActor::authenticated("other").unwrap(),
    octacity_server_application::ManagementSecurityScope::new("operator:other").unwrap(),
    ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .unwrap();
  assert!(
    queries
      .handle_query(&foreign, read(None))
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert!(
    queries
      .handle_query(&foreign, read(Some(cursor.clone())))
      .await
      .is_err()
  );
  let restricted =
    AuthorizedQueryHandler::new(Arc::new(SynchronizationVisibility(ManagementVisibility::none())), owner);
  assert!(restricted.handle_query(&context, read(Some(cursor))).await.is_err());
  assert_eq!(
    store.issue(accepted.project_id, accepted.id, None).await.unwrap(),
    accepted
  );
  database.cleanup().await;
}
struct SynchronizationVisibility(ManagementVisibility);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for SynchronizationVisibility {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(self.0.clone()))
  }
}
