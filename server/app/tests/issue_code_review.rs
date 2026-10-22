//! Application/real-PostgreSQL parity for optional read-only code reviews, without provider processes.
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_store::{testing::management_mutation, *};
use octacity_server_store_postgres::PostgresStore;
use std::sync::{
  Arc,
  atomic::{AtomicBool, AtomicUsize, Ordering},
};
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(50))
  }
}
#[path = "support/issue_review_reader.rs"]
mod review_reader;
use review_reader::Reader;
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn code_review_application_rechecks_current_reads_and_metadata_visibility_after_store_restart() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let project = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new("Manual task reviews").unwrap(),
      idempotency_key: "project".parse().unwrap(),
      created_at: at(1),
    }))
    .await
    .unwrap();
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let issue = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
  )
  .handle_command(
    &context,
    CreateIssueCommand {
      intent: CreateIssueIntent {
        project_id: project,
        content: IssueContent {
          title: "Manual task".into(),
          description: String::new(),
          acceptance: None,
          priority: None,
        },
        planning: Default::default(),
      },
      idempotency_key: "manual".parse().unwrap(),
      created_at: at(2),
    },
  )
  .await
  .unwrap()
  .issue;
  let reader = Arc::new(Reader {
    revoked: AtomicBool::new(false),
    hidden: AtomicBool::new(false),
    reads: AtomicUsize::new(0),
    checks: false,
  });
  let handlers = Arc::new(IssueCodeReviewHandlers::new(
    store.clone(),
    reader.clone(),
    policy.clone(),
    Arc::new(Clock),
  ));
  let commands = AuthorizedCommandHandler::new(policy.clone(), handlers);
  let selected:IssueCodeReviewSelection=serde_json::from_value(serde_json::json!({"source":{"installation":{"identity":"code-forge","version":"v1","digest":"01".repeat(32)},"capability":{"identity":"review-read","version":"v1","digest":"02".repeat(32)},"configuration":{"identity":"review-config","version":"v1","digest":"03".repeat(32)},"resource":{"identity":"native-code","version":"v1","digest":"04".repeat(32)}},"item":{"resource":"native-repository","item":"review-42"}})).unwrap();
  let attachment = AttachIssueCodeReviewCommand {
    intent: AttachIssueCodeReviewIntent {
      project_id: project,
      issue_id: issue.id,
      expected_version: issue.version,
      selection: selected,
      binding: None,
    },
    idempotency_key: "attach".parse().unwrap(),
  };
  let original = commands.handle_command(&context, attachment.clone()).await.unwrap();
  let refresh = RefreshIssueCodeReviewCommand {
    project_id: project,
    issue_id: issue.id,
    association_id: original.association.id,
    idempotency_key: "refresh".parse().unwrap(),
  };
  let observed = commands.handle_command(&context, refresh.clone()).await.unwrap();
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let handlers = Arc::new(IssueCodeReviewHandlers::new(
    restarted,
    reader.clone(),
    policy.clone(),
    Arc::new(Clock),
  ));
  let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
  let commands = AuthorizedCommandHandler::new(policy, handlers);
  reader.revoked.store(true, Ordering::SeqCst);
  let first = queries
    .handle_query(
      &context,
      ListIssueCodeReviewObservationsQuery::try_new(project, issue.id, original.association.id, None, 1).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(first.availability, IssueCodeReviewAvailability::Blocked);
  assert_eq!(first.items[0].observation.head.as_ref().unwrap().as_str(), "old-head");
  let cursor = first.next_cursor.unwrap();
  let next = queries
    .handle_query(
      &context,
      ListIssueCodeReviewObservationsQuery::try_new(
        project,
        issue.id,
        original.association.id,
        Some(cursor.clone()),
        1,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(next.items[0], observed.observation);
  assert!(next.next_cursor.is_none());
  assert_eq!(
    commands
      .handle_command(&context, attachment.clone())
      .await
      .unwrap()
      .association,
    original.association
  );
  assert_eq!(
    commands
      .handle_command(&context, refresh.clone())
      .await
      .unwrap()
      .observation,
    observed.observation
  );
  let mut denied = refresh;
  denied.idempotency_key = "new-refresh-after-revocation".parse().unwrap();
  assert!(commands.handle_command(&context, denied).await.is_err());
  assert_eq!(reader.reads.load(Ordering::SeqCst), 2);
  reader.hidden.store(true, Ordering::SeqCst);
  assert!(
    queries
      .handle_query(
        &context,
        ListIssueCodeReviewObservationsQuery::try_new(project, issue.id, original.association.id, Some(cursor), 1)
          .unwrap()
      )
      .await
      .is_err()
  );
  let hidden = queries
    .handle_query(
      &context,
      ListIssueCodeReviewsQuery::try_new(project, issue.id, None, 1).unwrap(),
    )
    .await
    .unwrap();
  assert!(hidden.items.is_empty());
  assert!(hidden.next_cursor.is_none());
  assert!(commands.handle_command(&context, attachment).await.is_err());
  assert_eq!(store.issue(project, issue.id, None).await.unwrap(), issue);
  database.cleanup().await;
}
