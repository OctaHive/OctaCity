//! Actual GitHub HTTPS fixture output and an opaque second vocabulary cross the same installed
//! protocol normalization and task contracts. Neither fixture installs a Factory task target.
use super::*;
#[allow(dead_code)]
#[path = "../../../infrastructure/octacity-github/tests/support/issues.rs"]
mod github;

#[tokio::test]
async fn github_fixture_and_second_source_share_native_task_contracts_and_isolate_outages() {
  let forge = github::Forge::start().await;
  forge
    .replies
    .lock()
    .await
    .push_back(github::Reply::json(serde_json::json!([github::issue(9001, 17)])));
  let installation = github::Installation::new(&forge, github::selector());
  let request = installation.request();
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  let octacity_work_source_protocol::Response::Source(octacity_work_source_protocol::source::Response {
    outcome: octacity_work_source_protocol::source::Outcome::Page(page),
    ..
  }) = response
  else {
    panic!("normalized GitHub page")
  };
  assert_eq!(page.candidates[0].item_id, "9001");
  assert_eq!(page.candidates[0].display_id, "17");
  for provider in ["github", "fake-tracker"] {
    let mut fixture = Fixture::enabled(provider).await;
    if provider == "github" {
      fixture.profile.read.resource.resource_id = page.resource_id.clone();
      std::fs::write(
        fixture.root.path().join("github/mode.json"),
        serde_json::to_vec(&serde_json::json!({"candidates":page.candidates})).unwrap(),
      )
      .unwrap();
    }
    let context = &fixture.profile.read.context;
    let project = fixture.profile.read.project_id;
    let store = Arc::new(octacity_server_store::testing::InMemoryProjectStore::new());
    store
      .create_project(octacity_server_store::testing::management_mutation(CreateProject {
        id: project,
        parent_id: None,
        name: ProjectName::new("Manual tasks").unwrap(),
        idempotency_key: IdempotencyKey::new("project").unwrap(),
        created_at: time(1),
      }))
      .await
      .unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    );
    let draft = |key: &str| CreateIssueCommand {
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: "Local requirements".into(),
          description: "Independent description".into(),
          acceptance: None,
          priority: None,
        },
      },
      idempotency_key: IdempotencyKey::new(key).unwrap(),
      created_at: time(10),
    };
    let original = issues.handle_command(context, draft("manual")).await.unwrap().issue;
    let source = fixture.profile.read.binding.clone();
    let native = fixture.profile.read.resource.resource_id.clone();
    let installed = InstalledIssueSourceMapping {
      project_id: project,
      source: source.clone(),
      native_resource: ExternalWorkIdentity::new(&native).unwrap(),
      readable: WorkSourceReportingPolicy {
        states: vec![if provider == "github" { "open" } else { "native-state" }.into()],
        labels: vec!["arbitrary".into()],
      },
      writer: None,
    };
    let handlers = Arc::new(
      IssueSourceHandlers::new(
        store.clone(),
        Arc::new(reader(&fixture)),
        policy.clone(),
        fixture.clock.clone(),
      )
      .with_refresh_policy(IssueSourceRefreshPolicy {
        read: IssueSourceReadPolicy {
          polling: fixture.profile.read.polling.clone(),
          retry: DurableRetryPolicy::new(
            settings().max_attempts,
            settings().worker.poll_interval().as_millis() as u64,
            settings().worker.retry_policy().max_backoff().as_millis() as u64,
          )
          .unwrap(),
          poll_interval_millis: settings().worker.poll_interval().as_millis() as u64,
        },
        claim_lifetime: settings().worker.claim_lifetime(),
      })
      .unwrap()
      .with_source_mappings(vec![installed])
      .unwrap(),
    );
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let preview = commands
      .handle_command(
        context,
        PreviewIssueSourceCommand {
          project_id: project,
          issue_id: original.id,
          source: source.clone(),
          scan_id: None,
          continuation: None,
        },
      )
      .await
      .unwrap();
    let item = preview.items[0].clone();
    assert_eq!(item.resource.as_str(), native);
    assert_eq!(
      item.item.as_str(),
      if provider == "github" { "9001" } else { "TASK-native" }
    );
    let attach = AttachIssueSourceCommand {
      intent: AttachIssueSourceIntent {
        selection: IssueSourceSelection {
          project_id: project,
          source: source.clone(),
          item,
        },
        issue_id: original.id,
        expected_version: original.version,
        expected_field_policy: IssueFieldPolicy::default(),
      },
      read: super::request(&fixture),
      idempotency_key: IdempotencyKey::new("attach").unwrap(),
      attached_at: time(100),
    };
    let result = commands.handle_command(context, attach.clone()).await.unwrap();
    assert!(matches!(result, IssueSourceOutcome::Linked { .. }));
    assert_eq!(store.issue(project, original.id, None).await.unwrap(), original);
    let choices = queries
      .handle_query(
        context,
        GetIssueSourceChoicesQuery {
          project_id: project,
          issue_id: original.id,
        },
      )
      .await
      .unwrap();
    assert_eq!(choices.items.len(), 1);
    assert!(choices.items[0].writer.is_none());
    assert!(choices.items[0].push_fields.is_empty());
    // Outage affects an explicit read, never local requirements or retained provenance.
    std::fs::write(
      fixture.root.path().join(provider).join("mode.json"),
      br#"{"failure":{"class":"transient","retry_after_millis":null}}"#,
    )
    .unwrap();
    fixture.clock.0.store(200, Ordering::SeqCst);
    assert!(matches!(
      commands
        .handle_command(
          context,
          ObserveIssueSourceCommand {
            refresh: RefreshIssueSourceCommand {
              project_id: project,
              issue_id: original.id,
              read: super::request(&fixture),
              idempotency_key: IdempotencyKey::new("outage").unwrap(),
              refreshed_at: time(200)
            }
          }
        )
        .await,
      Err(ManagementHandlerError::Application(
        ApplicationError::SourceDependency {
          class: WorkSourceFailure::Transient,
          ..
        }
      ))
    ));
    let sync = queries
      .handle_query(
        context,
        GetIssueSynchronizationQuery {
          project_id: project,
          issue_id: original.id,
        },
      )
      .await
      .unwrap();
    assert_eq!(sync.link.unwrap().selection, attach.intent.selection);
    assert_eq!(sync.observation.unwrap().accepted.observed_at, time(100));
    let next = issues
      .handle_command(context, draft("no-gitlab-or-jira"))
      .await
      .unwrap()
      .issue;
    assert_ne!(next.id, original.id);
    assert_eq!(next.content.title, "Local requirements");
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
  }
}
