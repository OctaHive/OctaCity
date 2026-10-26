use super::*;
#[path = "issue_source_mapping_tests.rs"]
mod mapping;
use octacity_server_factory::{ExternalWorkIdentity, FactoryDigest, ImmutableReference, WorkSourceBinding};
use octacity_server_store::{
  IssueTransitionOrigin, WorkSourceItem, source_native_correlation, source_native_issue, source_security_partition,
};

fn exact(name: &str) -> ImmutableReference {
  serde_json::from_value(serde_json::json!({"identity":name,"version":"v1","digest":"01".repeat(32)})).unwrap()
}
fn selection(project_id: ProjectId) -> IssueSourceSelection {
  IssueSourceSelection {
    project_id,
    source: WorkSourceBinding {
      installation: exact("forge-installation"),
      capability: exact("task-reader"),
      configuration: exact("read-settings"),
      resource: exact("visible-resource"),
    },
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("container-41").unwrap(),
      item: ExternalWorkIdentity::new("native-7").unwrap(),
    },
  }
}
fn read(selection: &IssueSourceSelection) -> ReadWorkSourceObservations {
  ReadWorkSourceObservations {
    connection: None,
    project_id: selection.project_id,
    source: selection.source.clone(),
    scan_id: FactoryDigest::content_sha256(b"explicit reviewed read"),
    window: None,
    continuation: None,
    limit: 1,
  }
}
struct Reader {
  state: Option<String>,
  title: Option<String>,
  item: Option<WorkSourceItem>,
  observed_at: i64,
  denied: bool,
  wrong_scope: bool,
  advance_clock: Option<Arc<Clock>>,
}
impl Default for Reader {
  fn default() -> Self {
    Self {
      state: None,
      title: None,
      item: None,
      observed_at: 20,
      denied: false,
      wrong_scope: false,
      advance_clock: None,
    }
  }
}
#[async_trait::async_trait]
impl WorkSourceObservations for Reader {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    if self.denied {
      Err(ApplicationError::capability_unavailable())
    } else {
      Ok(())
    }
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    if self.denied {
      return Err(ApplicationError::capability_unavailable());
    }
    if let Some(clock) = &self.advance_clock {
      clock.0.store(100, std::sync::atomic::Ordering::SeqCst);
    }
    let selection = selection(request.project_id);
    let snapshot = WorkSourceSnapshot {
      selection_matches: vec![],
      item: self.item.clone().unwrap_or(selection.item),
      display_id: "DISPLAY-7".into(),
      update_identity: "opaque-revision".into(),
      title: self.title.clone().unwrap_or_else(|| "Same human title".into()),
      body: Some("Native task body".into()),
      fields: Default::default(),
      labels: vec![],
      state: self.state.clone().unwrap_or_else(|| "opaque-state".into()),
    };
    Ok(WorkSourceObservationPage {
      observations: vec![WorkSourceObservation {
        project_id: request.project_id,
        security_scope: if self.wrong_scope {
          ManagementSecurityScope::new("other-partition").unwrap()
        } else {
          context.security_scope().clone()
        },
        native_correlation: source_native_correlation(
          source_native_issue(
            request.project_id,
            request.source.installation.identity(),
            &snapshot.item,
          ),
          source_security_partition(&context.security_scope().to_store()),
        ),
        source: request.source,
        plugin: exact("installed-reader"),
        provider_configuration: FactoryDigest::content_sha256(b"transport"),
        selection_digest: request.scan_id,
        snapshot_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&snapshot).unwrap()),
        snapshot,
        observed_at: at(self.observed_at),
        expires_at: at(100),
      }],
      next_cursor: None,
    })
  }
}

struct Clock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0.load(std::sync::atomic::Ordering::SeqCst)))
  }
}

#[test]
fn issue_source_observation_expiring_during_read_cannot_publish_a_link() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(30)));
    let reader = Reader {
      advance_clock: Some(clock.clone()),
      ..Default::default()
    };
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueSourceHandlers::new(store.clone(), Arc::new(reader), policy, clock)),
    );
    let before = store.mutation_evidence_counts().await;
    assert!(
      commands
        .handle_command(&context(), import(selection(project_id), "expired-in-flight"))
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
fn linked(outcome: IssueSourceOutcome) -> IssueSourceLink {
  match outcome {
    IssueSourceOutcome::Linked { link, .. } => link,
    other => panic!("expected link, got {other:?}"),
  }
}

#[test]
fn issue_source_explicit_attach_preserves_manual_content_and_exact_replay() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    );
    let issue = issues
      .handle_command(&context(), create(project_id, "manual"))
      .await
      .unwrap()
      .issue;
    let handlers = Arc::new(source_handlers(
      store.clone(),
      Arc::new(Reader::default()),
      policy.clone(),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let selection = selection(project_id);
    assert!(
      queries
        .handle_query(
          &context(),
          GetIssueSourceLinkQuery {
            project_id,
            issue_id: issue.id
          }
        )
        .await
        .unwrap()
        .is_none()
    );
    let command = AttachIssueSourceCommand {
      intent: AttachIssueSourceIntent {
        selection: selection.clone(),
        issue_id: issue.id,
        expected_version: issue.version,
        expected_field_policy: IssueFieldPolicy::default(),
      },
      read: read(&selection),
      idempotency_key: key("attach"),
      attached_at: at(30),
    };
    let link = linked(commands.handle_command(&context(), command.clone()).await.unwrap());
    assert_eq!(link.issue_id, issue.id);
    assert_eq!(link.selection, selection);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueSourceLinkQuery {
            project_id,
            issue_id: issue.id
          }
        )
        .await
        .unwrap(),
      Some(link.clone())
    );
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
    let before = store.mutation_evidence_counts().await;
    assert!(
      matches!(commands.handle_command(&context(), command).await.unwrap(), IssueSourceOutcome::Linked { disposition: MutationDisposition::Replayed, link: replay } if replay == link)
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_source_concurrent_imports_share_native_identity_without_merging_text() {
  let store = Arc::new(InMemoryProjectStore::new());
  let mut project_id = None;
  run_ready(async {
    project_id = Some(project(&store).await);
  });
  let project_id = project_id.unwrap();
  let barrier = Arc::new(std::sync::Barrier::new(4));
  let ids: Vec<_> = std::thread::scope(|threads| {
    let handles: Vec<_> = (0..4)
      .map(|i| {
        let store = store.clone();
        let barrier = barrier.clone();
        threads.spawn(move || {
          let mut result = None;
          barrier.wait();
          run_ready(async {
            let mut selection = selection(project_id);
            // Independent installed read profiles (e.g. two pipeline selectors) share native identity.
            selection.source.configuration = exact(&format!("read-settings-{i}"));
            let policy = Arc::new(TrustedNetworkManagementPolicy);
            let commands = AuthorizedCommandHandler::new(
              policy.clone(),
              Arc::new(source_handlers(store, Arc::new(Reader::default()), policy)),
            );
            result = Some(
              linked(
                commands
                  .handle_command(
                    &context(),
                    ImportIssueSourceCommand {
                      read: read(&selection),
                      selection,
                      idempotency_key: key(&format!("import-{i}")),
                      imported_at: at(30),
                    },
                  )
                  .await
                  .unwrap(),
              )
              .issue_id,
            );
          });
          result.unwrap()
        })
      })
      .collect();
    handles.into_iter().map(|h| h.join().unwrap()).collect()
  });
  assert!(ids.iter().all(|id| *id == ids[0]));
  run_ready(async {
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
    let queries = AuthorizedQueryHandler::new(policy.clone(), issues.clone());
    let page = queries
      .handle_query(
        &context(),
        ListProjectIssuesQuery::try_new(project_id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    let imported = store.issue(project_id, ids[0], None).await.unwrap();
    assert_eq!(imported.content.title, "Same human title");
    assert_eq!(imported.content.description, "Native task body");
    assert_eq!(imported.content.acceptance, None);
    assert_eq!(imported.version.get(), 1);
    let commands = AuthorizedCommandHandler::new(policy, issues);
    let mut manual = create(project_id, "same-title-manual");
    manual.intent.content = imported.content;
    let manual = commands.handle_command(&context(), manual).await.unwrap().issue;
    assert_ne!(manual.id, imported.id);
    assert_eq!(manual.display_key(), "ISS-2");
  });
}

#[test]
fn issue_source_existing_import_seals_its_own_replay_identity() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(store.clone(), Arc::new(Reader::default()), policy)),
    );
    let selection = selection(project_id);
    let command = ImportIssueSourceCommand {
      read: read(&selection),
      selection: selection.clone(),
      idempotency_key: key("first-input"),
      imported_at: at(30),
    };
    let first = linked(commands.handle_command(&context(), command.clone()).await.unwrap());
    let mut second = command;
    second.idempotency_key = key("second-input");
    assert_eq!(
      linked(commands.handle_command(&context(), second.clone()).await.unwrap()),
      first
    );
    let before = store.mutation_evidence_counts().await;
    second.selection.source.installation = exact("another-installation");
    second.read = read(&second.selection);
    assert_eq!(
      commands
        .handle_command(&context(), second)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

struct HideIssue(octacity_server_domain::IssueId);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for HideIssue {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let hidden = action == ManagementAction::View
      && resource.kind() == ManagementResourceKind::Issue
      && resource.identity().is_none_or(|id| id.as_str() == self.0.to_string());
    Ok(ManagementAuthorizationGrant::new(if hidden {
      ManagementVisibility::none()
    } else {
      ManagementVisibility::all()
    }))
  }
}

#[test]
fn issue_source_hidden_canonical_import_denies_before_replay_or_audit_publication() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let selection = selection(project_id);
    let command = ImportIssueSourceCommand {
      read: read(&selection),
      selection,
      idempotency_key: key("visible-import"),
      imported_at: at(30),
    };
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(store.clone(), Arc::new(Reader::default()), policy)),
    );
    let canonical = linked(commands.handle_command(&context(), command.clone()).await.unwrap());
    let policy = Arc::new(HideIssue(canonical.issue_id));
    let restricted = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(store.clone(), Arc::new(Reader::default()), policy)),
    );
    let before = store.mutation_evidence_counts().await;
    let mut hidden = command;
    hidden.idempotency_key = key("hidden-import");
    assert!(restricted.handle_command(&context(), hidden).await.is_err());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_source_read_can_finish_after_the_command_arrived() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueSourceHandlers::new(
        store.clone(),
        Arc::new(Reader {
          observed_at: 35,
          ..Default::default()
        }),
        policy,
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(35))),
      )),
    );
    let selection = selection(project_id);
    let link = linked(
      commands
        .handle_command(
          &context(),
          ImportIssueSourceCommand {
            read: read(&selection),
            selection,
            idempotency_key: key("later-read"),
            imported_at: at(30),
          },
        )
        .await
        .unwrap(),
    );
    assert_eq!(link.linked_at, at(35));
    assert_eq!(
      store.issue(project_id, link.issue_id, None).await.unwrap().created_at,
      at(35)
    );
  });
}

#[test]
fn issue_source_initial_history_retains_import_origin_without_a_native_status_move() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
      )),
    );
    let selection = selection(project_id);
    let link = linked(
      commands
        .handle_command(
          &context(),
          ImportIssueSourceCommand {
            read: read(&selection),
            selection,
            idempotency_key: key("source-history"),
            imported_at: at(30),
          },
        )
        .await
        .unwrap(),
    );
    let queries = AuthorizedQueryHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store, policy)));
    let history = queries
      .handle_query(
        &context(),
        ListIssueStatusHistoryQuery::try_new(project_id, link.issue_id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(history.items.len(), 1);
    let initial = &history.items[0];
    assert_eq!(initial.origin, IssueTransitionOrigin::Source);
    assert_eq!(initial.from, None);
    assert_eq!(initial.transition_id, None);
    assert_eq!(initial.occurred_at, at(30));
  });
}

fn import(selection: IssueSourceSelection, identity: &str) -> ImportIssueSourceCommand {
  ImportIssueSourceCommand {
    read: read(&selection),
    selection,
    idempotency_key: key(identity),
    imported_at: at(30),
  }
}
fn attach(issue: &IssueRevision, selection: IssueSourceSelection, identity: &str) -> AttachIssueSourceCommand {
  AttachIssueSourceCommand {
    read: read(&selection),
    intent: AttachIssueSourceIntent {
      selection,
      issue_id: issue.id,
      expected_version: issue.version,
      expected_field_policy: Default::default(),
    },
    idempotency_key: key(identity),
    attached_at: at(30),
  }
}

#[test]
fn issue_source_native_key_partitions_project_scope_installation_container_and_item() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let other_project = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::Create, ManagementAction::View],
      visibility: ManagementVisibility::all(),
    });
    let base = selection(project_id);
    let handlers = Arc::new(source_handlers(
      store.clone(),
      Arc::new(Reader::default()),
      policy.clone(),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers);
    let a = verified_context("source-a");
    let b = verified_context("source-b");
    let first = linked(
      commands
        .handle_command(&a, import(base.clone(), "native"))
        .await
        .unwrap(),
    );
    let mut rotated = base.clone();
    rotated.source.installation = ImmutableReference::new(
      base.source.installation.identity().clone(),
      "v2".parse().unwrap(),
      FactoryDigest::content_sha256(b"rotated installation"),
    );
    rotated.source.configuration = exact("renamed-read-profile");
    rotated.source.resource = exact("renamed-resource-reference");
    assert_eq!(
      linked(commands.handle_command(&a, import(rotated, "rotated")).await.unwrap()),
      first
    );
    let scoped = linked(
      commands
        .handle_command(&b, import(base.clone(), "native"))
        .await
        .unwrap(),
    );
    assert_ne!(scoped.issue_id, first.issue_id);
    assert!(
      queries
        .handle_query(
          &b,
          GetIssueSourceLinkQuery {
            project_id,
            issue_id: first.issue_id
          }
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      queries
        .handle_query(
          &a,
          GetIssueSourceLinkQuery {
            project_id: other_project,
            issue_id: first.issue_id
          }
        )
        .await
        .is_err()
    );
    let mut identities = std::collections::BTreeSet::from([first.issue_id, scoped.issue_id]);
    for (index, mut variant) in [base.clone(), base.clone(), base.clone(), selection(other_project)]
      .into_iter()
      .enumerate()
    {
      match index {
        0 => variant.source.installation = exact("another-installation"),
        1 => variant.item.resource = ExternalWorkIdentity::new("another-container").unwrap(),
        2 => variant.item.item = ExternalWorkIdentity::new("another-item").unwrap(),
        _ => {}
      }
      let commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(source_handlers(
          store.clone(),
          Arc::new(Reader {
            item: Some(variant.item.clone()),
            ..Default::default()
          }),
          policy.clone(),
        )),
      );
      let link = linked(
        commands
          .handle_command(&a, import(variant, &format!("distinct-{index}")))
          .await
          .unwrap(),
      );
      assert!(
        identities.insert(link.issue_id),
        "each distinct native dimension owns a separate Issue"
      );
    }
    assert_eq!(identities.len(), 6);
  });
}

#[test]
fn issue_source_primary_conflicts_preserve_both_histories_and_hide_canonical_targets() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    );
    let a = issues
      .handle_command(&context(), create(project_id, "a"))
      .await
      .unwrap()
      .issue;
    let b = issues
      .handle_command(&context(), create(project_id, "b"))
      .await
      .unwrap()
      .issue;
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
      )),
    );
    let original = attach(&a, selection(project_id), "attach-a");
    let link = linked(commands.handle_command(&context(), original.clone()).await.unwrap());
    let before = store.mutation_evidence_counts().await;
    for issue in [&a, &b] {
      assert_eq!(
        commands
          .handle_command(&context(), attach(issue, selection(project_id), "duplicate"))
          .await
          .unwrap(),
        IssueSourceOutcome::Conflict {
          canonical: Some(link.clone())
        }
      );
    }
    let hidden_policy = Arc::new(HideIssue(a.id));
    let hidden = AuthorizedCommandHandler::new(
      hidden_policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        hidden_policy,
      )),
    );
    assert_eq!(
      hidden
        .handle_command(&context(), attach(&b, selection(project_id), "hidden-duplicate"))
        .await
        .unwrap(),
      IssueSourceOutcome::Conflict { canonical: None }
    );
    let mut other = selection(project_id);
    other.item.item = ExternalWorkIdentity::new("different-native-task").unwrap();
    let other_commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader {
          item: Some(other.item.clone()),
          ..Default::default()
        }),
        policy.clone(),
      )),
    );
    assert_eq!(
      other_commands
        .handle_command(&context(), attach(&a, other, "second-primary"))
        .await
        .unwrap(),
      IssueSourceOutcome::Conflict { canonical: None }
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(store.issue(project_id, a.id, None).await.unwrap(), a);
    assert_eq!(store.issue(project_id, b.id, None).await.unwrap(), b);
    let mut edited = a.content.clone();
    edited.title = "Later manual edit".into();
    let revision = issues
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id,
            id: a.id,
            expected_version: a.version,
            content: edited,
          },
          idempotency_key: key("later-edit"),
          edited_at: at(40),
        },
      )
      .await
      .unwrap()
      .issue;
    assert_eq!(
      linked(commands.handle_command(&context(), original).await.unwrap()),
      link
    );
    assert_eq!(store.issue(project_id, a.id, None).await.unwrap(), revision);
    let mut stale = attach(&a, selection(project_id), "stale");
    stale.attached_at = at(50);
    assert_eq!(
      commands
        .handle_command(&context(), stale)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
  });
}

#[test]
fn issue_source_denied_foreign_expired_or_unmatched_reads_publish_nothing_and_manual_creation_survives() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issues = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
    );
    let issue = issues
      .handle_command(&context(), create(project_id, "manual-before"))
      .await
      .unwrap()
      .issue;
    let before = store.mutation_evidence_counts().await;
    for reader in [
      Reader {
        denied: true,
        ..Default::default()
      },
      Reader {
        wrong_scope: true,
        ..Default::default()
      },
    ] {
      let commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(source_handlers(store.clone(), Arc::new(reader), policy.clone())),
      );
      assert!(
        commands
          .handle_command(&context(), import(selection(project_id), "denied"))
          .await
          .is_err()
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    let handlers = Arc::new(source_handlers(
      store.clone(),
      Arc::new(Reader::default()),
      policy.clone(),
    ));
    let commands = AuthorizedCommandHandler::new(policy.clone(), handlers);
    let mut expired = import(selection(project_id), "expired");
    expired.imported_at = at(100);
    assert!(commands.handle_command(&context(), expired).await.is_err());
    let mut unmatched = selection(project_id);
    unmatched.item.item = ExternalWorkIdentity::new("not-in-page").unwrap();
    assert!(
      commands
        .handle_command(&context(), import(unmatched, "unmatched"))
        .await
        .is_err()
    );
    let limited = Arc::new(ScopedPolicy {
      actions: vec![ManagementAction::View, ManagementAction::Update],
      visibility: ManagementVisibility::all(),
    });
    let restricted = AuthorizedCommandHandler::new(
      limited.clone(),
      Arc::new(source_handlers(store.clone(), Arc::new(Reader::default()), limited)),
    );
    assert!(matches!(
      restricted
        .handle_command(&context(), attach(&issue, selection(project_id), "no-admin"))
        .await
        .unwrap_err(),
      ManagementHandlerError::Forbidden(_)
    ));
    let mut ownership = attach(&issue, selection(project_id), "wrong-ownership");
    ownership
      .intent
      .expected_field_policy
      .source_owned
      .insert(IssueField::Title);
    assert_eq!(
      commands
        .handle_command(&context(), ownership)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    let other_project = project(&store).await;
    assert_eq!(
      commands
        .handle_command(&context(), attach(&issue, selection(other_project), "foreign"))
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::NotFound
    );
    assert!(
      issues
        .handle_command(&context(), create(project_id, "manual-after"))
        .await
        .is_ok()
    );
  });
}

#[test]
fn issue_source_import_and_attachment_share_atomic_issue_publication() {
  use octacity_server_store::testing::MutationFailurePoint;
  for point in [
    MutationFailurePoint::DomainState,
    MutationFailurePoint::IdempotencyOutcome,
    MutationFailurePoint::AuditFact,
    MutationFailurePoint::OutboxEntry,
    MutationFailurePoint::Commit,
  ] {
    run_ready(async {
      let store = Arc::new(InMemoryProjectStore::new());
      let project_id = project(&store).await;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let handlers = Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
      ));
      let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
      let queries = AuthorizedQueryHandler::new(policy.clone(), handlers);
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(
        commands
          .handle_command(&context(), import(selection(project_id), "atomic-import"))
          .await
          .is_err()
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
      let link = linked(
        commands
          .handle_command(&context(), import(selection(project_id), "atomic-import"))
          .await
          .unwrap(),
      );
      assert_eq!(
        store
          .issue(project_id, link.issue_id, None)
          .await
          .unwrap()
          .display_key(),
        "ISS-1"
      );
      let issues = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)));
      let issue = issues
        .handle_command(&context(), create(project_id, "manual"))
        .await
        .unwrap()
        .issue;
      let mut other = selection(project_id);
      other.source.installation = exact("other-installed-source");
      let command = attach(&issue, other, "atomic-attach");
      let before = store.mutation_evidence_counts().await;
      store.fail_next_issue_mutation_at(point).unwrap();
      assert!(commands.handle_command(&context(), command.clone()).await.is_err());
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert!(
        queries
          .handle_query(
            &context(),
            GetIssueSourceLinkQuery {
              project_id,
              issue_id: issue.id
            }
          )
          .await
          .unwrap()
          .is_none()
      );
      assert_eq!(
        linked(commands.handle_command(&context(), command).await.unwrap()).issue_id,
        issue.id
      );
    });
  }
}

fn source_handlers(
  store: Arc<InMemoryProjectStore>,
  reader: Arc<dyn WorkSourceObservations>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> IssueSourceHandlers<InMemoryProjectStore> {
  IssueSourceHandlers::new(
    store,
    reader,
    policy,
    Arc::new(Clock(std::sync::atomic::AtomicI64::new(30))),
  )
}

#[test]
fn issue_source_refresh_reconciles_changed_opaque_versions_without_target_or_execution() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let initial = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
      )),
    );
    let selection = selection(project_id);
    let link = linked(
      initial
        .handle_command(&context(), import(selection.clone(), "original"))
        .await
        .unwrap(),
    );
    store
      .seed_issue_field_policy(
        project_id,
        link.issue_id,
        IssueFieldPolicy {
          source_owned: [IssueField::Title].into(),
          ..Default::default()
        },
      )
      .unwrap();
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    for (observed, key, expected) in [
      (21, "uncertain", IssueSourceRefreshDisposition::Uncertain),
      (22, "confirmed", IssueSourceRefreshDisposition::Fresh),
      (22, "duplicate", IssueSourceRefreshDisposition::Duplicate),
      (23, "echo", IssueSourceRefreshDisposition::Unchanged),
      (20, "old", IssueSourceRefreshDisposition::Stale),
    ] {
      let commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(source_handlers(
          store.clone(),
          Arc::new(Reader {
            title: Some("Changed native title".into()),
            observed_at: observed,
            ..Default::default()
          }),
          policy.clone(),
        )),
      );
      let result = commands
        .handle_command(
          &context(),
          RefreshIssueSourceCommand {
            project_id,
            issue_id: link.issue_id,
            read: read(&selection),
            idempotency_key: self::key(key),
            refreshed_at: at(30),
          },
        )
        .await
        .unwrap();
      assert_eq!(result.disposition, expected);
      let current = store.issue(project_id, link.issue_id, None).await.unwrap();
      assert_eq!(
        current.content.title,
        if observed == 21 {
          "Same human title"
        } else {
          "Changed native title"
        }
      );
      assert_eq!(current.version.get(), if observed == 21 { 1 } else { 2 });
      assert_eq!(current.status_id, before.status_id);
      assert_eq!(current.content.description, before.content.description);
    }
  });
}

struct ResumeReader;
#[async_trait::async_trait]
impl WorkSourceObservations for ResumeReader {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    _context: &ManagementRequestContext,
    _request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    Err(ApplicationError::capability_unavailable())
  }
}
struct TwoItemPage;
#[async_trait::async_trait]
impl WorkSourceObservations for TwoItemPage {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let mut observations = Vec::new();
    for item in ["native-a", "native-b"] {
      let mut observation = Reader {
        item: Some(WorkSourceItem {
          item: ExternalWorkIdentity::new(item).unwrap(),
          ..selection(request.project_id).item
        }),
        ..Default::default()
      }
      .read(context, request.clone())
      .await?
      .observations
      .remove(0);
      observation.snapshot.title = item.into();
      observation.snapshot_digest = FactoryDigest::content_sha256(&serde_json::to_vec(&observation.snapshot).unwrap());
      observations.push(observation);
    }
    Ok(WorkSourceObservationPage {
      observations,
      next_cursor: Some("original-private-continuation".into()),
    })
  }
}
#[test]
fn issue_source_bounded_import_resumes_original_page_without_factory_or_provider_reread() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let authorization = Arc::new(TrustedNetworkManagementPolicy);
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(30)));
    let settings = read_settings();
    let command = RunIssueSourceReadCommand {
      project_id,
      target: IssueSourceReadTarget {
        pull_generation: 0,
        source: selection(project_id).source,
        issue_id: None,
      },
      owner: "card-reader".parse().unwrap(),
      until: at(90),
    };
    let owner = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(TwoItemPage),
        authorization.clone(),
        clock.clone(),
        settings.clone(),
      )
      .unwrap(),
    );
    let commands = AuthorizedCommandHandler::new(authorization.clone(), owner);
    let first = commands
      .handle_command(&context(), command.clone())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(first.items_processed, 1);
    assert_eq!(first.pages_read, 1);
    clock.0.store(31, std::sync::atomic::Ordering::SeqCst);
    let restarted = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(ResumeReader),
        authorization.clone(),
        clock,
        settings,
      )
      .unwrap(),
    );
    let second = AuthorizedCommandHandler::new(authorization, restarted)
      .handle_command(&context(), command)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(second.items_processed, 1);
    assert_eq!(second.pages_read, 0);
    let manual = store
      .create_issue(octacity_server_store::testing::management_mutation(
        octacity_server_store::CreateIssue {
          id: octacity_server_domain::IssueId::generate(),
          intent: CreateIssueIntent {
            project_id,
            content: draft(),
            planning: Default::default(),
          },
          assignees: Default::default(),
          idempotency_key: key("manual-after-import"),
          created_at: at(32),
        },
      ))
      .await
      .unwrap();
    assert_eq!(manual.issue.display_key(), "ISS-3");
  });
}

struct LimitedReader;
#[async_trait::async_trait]
impl WorkSourceObservations for LimitedReader {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    _context: &ManagementRequestContext,
    _request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    Err(ApplicationError::SourceDependency {
      class: WorkSourceFailure::RateLimited,
      retry_after_millis: Some(200),
    })
  }
}
#[test]
fn issue_source_rate_limit_and_exhaustion_survive_owner_restart_without_blocking_manual_tasks() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let authorization = Arc::new(TrustedNetworkManagementPolicy);
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(30)));
    let settings = read_settings();
    let mut command = RunIssueSourceReadCommand {
      project_id,
      target: IssueSourceReadTarget {
        pull_generation: 0,
        source: selection(project_id).source,
        issue_id: None,
      },
      owner: "limited-reader".parse().unwrap(),
      until: at(90),
    };
    for (now, deferred) in [(30, false), (91, true), (230, false), (430, false), (1000, true)] {
      clock.0.store(now, std::sync::atomic::Ordering::SeqCst);
      command.until = at(now + 60);
      let owner = Arc::new(
        IssueSourceReadHandlers::new(
          store.clone(),
          Arc::new(LimitedReader),
          authorization.clone(),
          clock.clone(),
          settings.clone(),
        )
        .unwrap(),
      );
      let result = AuthorizedCommandHandler::new(authorization.clone(), owner)
        .handle_command(&context(), command.clone())
        .await;
      if deferred {
        assert!(
          result.unwrap().is_none(),
          "persisted retry/exhaustion blocks the read at {now}"
        );
      } else {
        assert!(result.is_err());
      }
    }
    assert!(
      store
        .create_issue(octacity_server_store::testing::management_mutation(
          octacity_server_store::CreateIssue {
            id: octacity_server_domain::IssueId::generate(),
            intent: CreateIssueIntent {
              project_id,
              content: draft(),
              planning: Default::default()
            },
            assignees: Default::default(),
            idempotency_key: key("manual-during-outage"),
            created_at: at(1001)
          }
        ))
        .await
        .is_ok()
    );
  });
}

#[test]
fn issue_source_retained_link_refresh_uses_its_read_profile_and_stops_when_pull_is_disabled() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let authorization = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(source_handlers(
      store.clone(),
      Arc::new(Reader::default()),
      authorization.clone(),
    ));
    let commands = AuthorizedCommandHandler::new(authorization.clone(), handlers);
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "retained-link"))
        .await
        .unwrap(),
    );
    let disabled = commands
      .handle_command(
        &context(),
        SetIssueSourcePullCommand {
          project_id,
          issue_id: link.issue_id,
          expected_generation: 1,
          enabled: false,
          idempotency_key: key("disable-pull"),
          changed_at: at(30),
        },
      )
      .await
      .unwrap();
    assert!(!disabled.enabled);
    let mut source = selection(project_id).source;
    source.configuration = exact("broader-link-read-profile");
    let command = RunIssueSourceReadCommand {
      project_id,
      target: IssueSourceReadTarget {
        pull_generation: 0,
        source,
        issue_id: Some(link.issue_id),
      },
      owner: "retained-link-reader".parse().unwrap(),
      until: at(90),
    };
    let settings = read_settings();
    let reads = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(Reader {
          denied: true,
          ..Default::default()
        }),
        authorization.clone(),
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(31))),
        settings.clone(),
      )
      .unwrap(),
    );
    assert!(
      AuthorizedCommandHandler::new(authorization.clone(), reads)
        .handle_command(&context(), command.clone())
        .await
        .unwrap()
        .is_none()
    );
    commands
      .handle_command(
        &context(),
        SetIssueSourcePullCommand {
          project_id,
          issue_id: link.issue_id,
          expected_generation: disabled.generation,
          enabled: true,
          idempotency_key: key("enable-pull"),
          changed_at: at(32),
        },
      )
      .await
      .unwrap();
    let reads = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(Reader {
          observed_at: 33,
          ..Default::default()
        }),
        authorization.clone(),
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(34))),
        settings,
      )
      .unwrap(),
    );
    let outcome = AuthorizedCommandHandler::new(authorization, reads)
      .handle_command(&context(), command)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(outcome.items_processed, 1);
    assert_eq!(
      store
        .issue(project_id, link.issue_id, None)
        .await
        .unwrap()
        .version
        .get(),
      1
    );
  });
}

struct ExpiringReadAuthority(Arc<Clock>);
#[async_trait::async_trait]
impl WorkSourceObservations for ExpiringReadAuthority {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    self.0.0.store(91, std::sync::atomic::Ordering::SeqCst);
    Ok(())
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    Reader::default().read(context, request).await
  }
}
#[test]
fn issue_source_owner_expiring_during_reauthorization_cannot_publish_an_import() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(30)));
    let settings = read_settings();
    let owner = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(ExpiringReadAuthority(clock.clone())),
        policy.clone(),
        clock,
        settings,
      )
      .unwrap(),
    );
    let result = AuthorizedCommandHandler::new(policy.clone(), owner)
      .handle_command(
        &context(),
        RunIssueSourceReadCommand {
          project_id,
          target: IssueSourceReadTarget {
            pull_generation: 0,
            source: selection(project_id).source,
            issue_id: None,
          },
          owner: "expiring-owner".parse().unwrap(),
          until: at(90),
        },
      )
      .await;
    assert!(result.is_err());
    let manual = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)))
      .handle_command(&context(), create(project_id, "manual-after-expiry"))
      .await
      .unwrap();
    assert_eq!(
      manual.issue.display_key(),
      "ISS-1",
      "expired source owner cannot publish before its checkpoint fails"
    );
  });
}

fn read_settings() -> IssueSourceReadPolicy {
  IssueSourceReadPolicy {
    polling: SourcePollingPolicy {
      page_items: 2,
      max_pages: 1,
      max_items: 1,
      max_bytes: 100_000,
      max_page_bytes: 100_000,
      overlap_millis: 1,
      reconciliation_interval_millis: 1000,
      retention_millis: 50,
    },
    retry: DurableRetryPolicy::new(3, 1, 100).unwrap(),
    poll_interval_millis: 1,
  }
}

struct LivePage(Arc<Clock>);
#[async_trait::async_trait]
impl WorkSourceObservations for LivePage {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let mut page = TwoItemPage.read(context, request).await?;
    let now = self.0.now()?.unix_millis();
    for observation in &mut page.observations {
      observation.observed_at = at(now);
      observation.expires_at = at(now + 100);
    }
    Ok(page)
  }
}
#[test]
fn issue_source_expired_original_observation_discards_the_retained_tail_before_reading_again() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(30)));
    let mut settings = read_settings();
    settings.polling.retention_millis = 1000;
    let owner = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(LivePage(clock.clone())),
        policy.clone(),
        clock.clone(),
        settings,
      )
      .unwrap(),
    );
    let commands = AuthorizedCommandHandler::new(policy.clone(), owner);
    let mut command = RunIssueSourceReadCommand {
      project_id,
      target: IssueSourceReadTarget {
        pull_generation: 0,
        source: selection(project_id).source,
        issue_id: None,
      },
      owner: "expiry-reader".parse().unwrap(),
      until: at(90),
    };
    assert_eq!(
      commands
        .handle_command(&context(), command.clone())
        .await
        .unwrap()
        .unwrap()
        .items_processed,
      1
    );
    clock.0.store(131, std::sync::atomic::Ordering::SeqCst);
    command.until = at(190);
    let next = commands.handle_command(&context(), command).await.unwrap().unwrap();
    assert_eq!(
      next.pages_read, 1,
      "expiry must start a fresh read, never renew a retained observation"
    );
    assert_eq!(next.items_processed, 1);
    let manual = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)))
      .handle_command(&context(), create(project_id, "manual-after-expired-tail"))
      .await
      .unwrap();
    assert_eq!(
      manual.issue.display_key(),
      "ISS-2",
      "expired unprocessed native-b was not imported"
    );
  });
}

struct EmptyReadProbe(Arc<std::sync::atomic::AtomicUsize>);
#[async_trait::async_trait]
impl WorkSourceObservations for EmptyReadProbe {
  async fn read(
    &self,
    _context: &ManagementRequestContext,
    _request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(WorkSourceObservationPage {
      observations: vec![],
      next_cursor: None,
    })
  }
}
#[test]
fn issue_source_hidden_project_is_rejected_before_provider_activity_even_for_an_empty_page() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::Administer,
        ManagementAction::Create,
        ManagementAction::View,
      ],
      visibility: ManagementVisibility::none(),
    });
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let owner = Arc::new(
      IssueSourceReadHandlers::new(
        store,
        Arc::new(EmptyReadProbe(calls.clone())),
        policy.clone(),
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(30))),
        read_settings(),
      )
      .unwrap(),
    );
    let result = AuthorizedCommandHandler::new(policy, owner)
      .handle_command(
        &context(),
        RunIssueSourceReadCommand {
          project_id,
          target: IssueSourceReadTarget {
            pull_generation: 0,
            source: selection(project_id).source,
            issue_id: None,
          },
          owner: "hidden-project".parse().unwrap(),
          until: at(90),
        },
      )
      .await;
    assert!(result.is_err());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
  });
}

#[test]
fn issue_source_discovery_can_pass_a_paused_link_without_refreshing_it() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
      )),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "paused-discovery-link"))
        .await
        .unwrap(),
    );
    commands
      .handle_command(
        &context(),
        SetIssueSourcePullCommand {
          project_id,
          issue_id: link.issue_id,
          expected_generation: 1,
          enabled: false,
          idempotency_key: key("pause-before-discovery"),
          changed_at: at(30),
        },
      )
      .await
      .unwrap();
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    let owner = Arc::new(
      IssueSourceReadHandlers::new(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(31))),
        read_settings(),
      )
      .unwrap(),
    );
    let result = AuthorizedCommandHandler::new(policy, owner)
      .handle_command(
        &context(),
        RunIssueSourceReadCommand {
          project_id,
          target: IssueSourceReadTarget {
            pull_generation: 0,
            source: selection(project_id).source,
            issue_id: None,
          },
          owner: "board-importer".parse().unwrap(),
          until: at(90),
        },
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(result.items_processed, 1);
    assert_eq!(store.issue(project_id, link.issue_id, None).await.unwrap(), before);
  });
}

struct PausePullDuringUpdate {
  store: Arc<InMemoryProjectStore>,
  project_id: ProjectId,
  issue_id: octacity_server_domain::IssueId,
  paused: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for PausePullDuringUpdate {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::Update
      && resource.kind() == ManagementResourceKind::Issue
      && resource
        .identity()
        .is_some_and(|id| id.as_str() == self.issue_id.to_string())
      && !self.paused.swap(true, std::sync::atomic::Ordering::SeqCst)
    {
      // Another operator pauses this link after discovery's preliminary enabled check.
      octacity_server_store::IssueSourceRefreshStore::set_issue_source_pull(
        self.store.as_ref(),
        octacity_server_store::testing::management_mutation(SetIssueSourcePullCommand {
          project_id: self.project_id,
          issue_id: self.issue_id,
          expected_generation: 1,
          enabled: false,
          idempotency_key: key("pause-during-discovery-update"),
          changed_at: at(30),
        }),
      )
      .await
      .unwrap();
    }
    TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
  }
}

#[test]
fn issue_source_discovery_pause_during_refresh_does_not_exhaust_other_tasks() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let trusted = Arc::new(TrustedNetworkManagementPolicy);
    let mut native = selection(project_id);
    native.item.item = ExternalWorkIdentity::new("native-a").unwrap();
    let initial = AuthorizedCommandHandler::new(
      trusted.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader {
          item: Some(native.item.clone()),
          ..Default::default()
        }),
        trusted,
      )),
    );
    let link = linked(
      initial
        .handle_command(&context(), import(native, "before-raced-pause"))
        .await
        .unwrap(),
    );
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    let policy = Arc::new(PausePullDuringUpdate {
      store: store.clone(),
      project_id,
      issue_id: link.issue_id,
      paused: std::sync::atomic::AtomicBool::new(false),
    });
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(30)));
    let command = RunIssueSourceReadCommand {
      project_id,
      target: IssueSourceReadTarget {
        pull_generation: 0,
        source: selection(project_id).source,
        issue_id: None,
      },
      owner: "discovery-owner".parse().unwrap(),
      until: at(90),
    };
    let owner = IssueSourceReadHandlers::new(
      store.clone(),
      Arc::new(TwoItemPage),
      policy.clone(),
      clock.clone(),
      read_settings(),
    )
    .unwrap();
    let first = AuthorizedCommandHandler::new(policy.clone(), Arc::new(owner))
      .handle_command(&context(), command.clone())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(first.items_processed, 1);
    assert_eq!(store.issue(project_id, link.issue_id, None).await.unwrap(), before);
    clock.0.store(31, std::sync::atomic::Ordering::SeqCst);
    let restarted = IssueSourceReadHandlers::new(
      store.clone(),
      Arc::new(ResumeReader),
      policy.clone(),
      clock,
      read_settings(),
    )
    .unwrap();
    let remaining = AuthorizedCommandHandler::new(policy.clone(), Arc::new(restarted))
      .handle_command(&context(), command)
      .await
      .unwrap()
      .expect("discovery must remain runnable");
    assert_eq!(remaining.items_processed, 1);
    assert_eq!(remaining.pages_read, 0);
    let manual = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store, policy)))
      .handle_command(&context(), create(project_id, "manual-after-raced-pause"))
      .await
      .unwrap();
    assert_eq!(manual.issue.display_key(), "ISS-3");
  });
}

struct PassIdentityProbe(std::sync::Mutex<Vec<FactoryDigest>>);
#[async_trait::async_trait]
impl WorkSourceObservations for PassIdentityProbe {
  async fn read(
    &self,
    _context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    self.0.lock().unwrap().push(request.scan_id);
    Ok(WorkSourceObservationPage {
      observations: vec![],
      next_cursor: None,
    })
  }
}
#[test]
fn issue_source_read_pass_identity_is_partitioned_before_provider_continuations() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(ScopedPolicy {
      actions: vec![
        ManagementAction::Administer,
        ManagementAction::View,
        ManagementAction::Create,
      ],
      visibility: ManagementVisibility::all(),
    });
    let reader = Arc::new(PassIdentityProbe(std::sync::Mutex::new(vec![])));
    let owner = Arc::new(
      IssueSourceReadHandlers::new(
        store,
        reader.clone(),
        policy.clone(),
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(30))),
        read_settings(),
      )
      .unwrap(),
    );
    let commands = AuthorizedCommandHandler::new(policy, owner);
    for scope in ["operator:first", "operator:second"] {
      let context = ManagementRequestContext::new(
        ManagementActor::authenticated("verified-owner").unwrap(),
        ManagementSecurityScope::new(scope).unwrap(),
        ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
        ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
      )
      .unwrap();
      commands
        .handle_command(
          &context,
          RunIssueSourceReadCommand {
            project_id,
            target: IssueSourceReadTarget {
              pull_generation: 0,
              source: selection(project_id).source,
              issue_id: None,
            },
            owner: "partitioned-reader".parse().unwrap(),
            until: at(90),
          },
        )
        .await
        .unwrap();
    }
    let ids = reader.0.lock().unwrap();
    assert_eq!(ids.len(), 2);
    assert_ne!(
      ids[0], ids[1],
      "opaque provider pass context must not cross accepted security partitions"
    );
  });
}

#[test]
fn issue_timeline_binds_source_watermark_and_marks_only_changed_source_section_stale() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let sources = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader::default()),
        policy.clone(),
      )),
    );
    let link = linked(
      sources
        .handle_command(&context(), import(selection(project_id), "timeline-import"))
        .await
        .unwrap(),
    );
    let reads = AuthorizedQueryHandler::new(
      policy,
      Arc::new(IssueActivityHandlers::new(
        store.clone(),
        Arc::new(Clock(std::sync::atomic::AtomicI64::new(40))),
      )),
    );
    let source_only = reads
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(
          project_id,
          link.issue_id,
          Some([IssueTimelineKind::Source].into()),
          None,
          10,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(source_only.items.len(), 1);
    assert_eq!(source_only.items[0].position().kind, IssueTimelineKind::Source);
    assert!(!source_only.source_stale);
    let value = serde_json::to_string(&source_only).unwrap();
    for private in [
      "Same human title",
      "opaque-revision",
      "provider_configuration",
      "selection_digest",
    ] {
      assert!(!value.contains(private));
    }
    let first = reads
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(project_id, link.issue_id, None, None, 1).unwrap(),
      )
      .await
      .unwrap();
    sources
      .handle_command(
        &context(),
        SetIssueSourcePullCommand {
          project_id,
          issue_id: link.issue_id,
          expected_generation: 1,
          enabled: false,
          idempotency_key: key("timeline-disable"),
          changed_at: at(40),
        },
      )
      .await
      .unwrap();
    let next = reads
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(project_id, link.issue_id, None, first.next_cursor, 10).unwrap(),
      )
      .await
      .unwrap();
    assert!(next.source_stale);
    assert_eq!(next.boundary, first.boundary);
    assert!(
      next
        .items
        .iter()
        .all(|r| r.position().kind != IssueTimelineKind::Source)
    );
    assert!(
      next
        .items
        .iter()
        .any(|r| r.position().kind == IssueTimelineKind::Status)
    );
    let fresh = reads
      .handle_query(
        &context(),
        ListIssueTimelineQuery::try_new(project_id, link.issue_id, None, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_ne!(fresh.boundary.source, next.boundary.source);
    assert!(!fresh.source_stale);
    assert!(
      fresh
        .items
        .iter()
        .any(|r| r.position().kind == IssueTimelineKind::Source)
    );
  });
}
