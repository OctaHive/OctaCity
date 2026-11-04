//! Independent discovery must not consume another mapping's refresh authority or stop its page.
use super::*;

#[test]
fn issue_source_mapping_a_different_refresh_profile_does_not_exhaust_discovery() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let mut native = selection(project_id);
    native.item.item = ExternalWorkIdentity::new("native-a").unwrap();
    let mut capability = installed(project_id);
    capability.source.configuration = exact("broader-link-refresh");
    let source = capability.source.clone();
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(
          store.clone(),
          Arc::new(Reader {
            item: Some(native.item.clone()),
            ..Default::default()
          }),
          policy.clone(),
        )
        .with_source_mappings(vec![capability])
        .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(native.clone(), "independent-profile-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let mut mapping = mapping_command(&issue, "broader-profile-mapping");
    mapping.intent.definition.source = source.clone();
    mapping.intent.definition.status = None;
    mapping.intent.definition.fields = [(IssueField::Description, IssueSourceFieldAuthority::SourceOwnedPull)].into();
    commands.handle_command(&context(), mapping).await.unwrap();
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(31)));
    let command = RunIssueSourceReadCommand {
      project_id,
      target: IssueSourceReadTarget {
        source: native.source.clone(),
        issue_id: None,
        connection: None,
        import: None,
        pull_generation: 0,
      },
      owner: "independent-discovery".parse().unwrap(),
      until: at(90),
    };
    let first = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueSourceReadHandlers::new(
          store.clone(),
          Arc::new(TwoItemPage),
          policy.clone(),
          clock.clone(),
          read_settings(),
        )
        .unwrap(),
      ),
    )
    .handle_command(&context(), command.clone())
    .await
    .unwrap()
    .unwrap();
    assert_eq!(first.items_processed, 1);
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
    clock.0.store(32, std::sync::atomic::Ordering::SeqCst);
    let tail = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueSourceReadHandlers::new(
          store.clone(),
          Arc::new(ResumeReader),
          policy.clone(),
          clock,
          read_settings(),
        )
        .unwrap(),
      ),
    )
    .handle_command(&context(), command)
    .await
    .unwrap()
    .expect("one mapped link must not exhaust discovery");
    assert_eq!(tail.items_processed, 1);
    assert_eq!(tail.pages_read, 0);
    let mut selected = native;
    selected.source = source;
    let refresh = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader {
          item: Some(selected.item.clone()),
          observed_at: 33,
          ..Default::default()
        }),
        policy.clone(),
      )),
    )
    .handle_command(
      &context(),
      RefreshIssueSourceCommand {
        project_id,
        issue_id: issue.id,
        read: read(&selected),
        idempotency_key: key("selected-profile-still-refreshes"),
        refreshed_at: at(34),
      },
    )
    .await
    .unwrap();
    assert_eq!(refresh.disposition, IssueSourceRefreshDisposition::Unchanged);
    let manual = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store, policy)))
      .handle_command(&context(), create(project_id, "manual-after-profile-mismatch"))
      .await
      .unwrap();
    assert_eq!(manual.issue.display_key(), "ISS-3");
  });
}
