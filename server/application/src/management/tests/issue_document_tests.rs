//! Issue document behavior through authorized publication and independent read contracts.
use super::*;
use octacity_server_domain::IssueId;

async fn issue(store: &testing::InMemoryProjectStore, project_id: ProjectId) -> IssueId {
  store
    .create_issue(testing::management_mutation(CreateIssue {
      id: IssueId::generate(),
      intent: CreateIssueIntent {
        project_id,
        content: IssueContent {
          title: "Investigate defect".into(),
          description: "Task requirements".into(),
          acceptance: None,
          priority: None,
        },
        planning: Default::default(),
      },
      assignees: Default::default(),
      idempotency_key: uuid::Uuid::new_v4().to_string().parse().unwrap(),
      created_at: time(1),
    }))
    .await
    .unwrap()
    .issue
    .id
}

struct Fixture {
  project: ProjectId,
  issue_id: IssueId,
  issues: Arc<testing::InMemoryProjectStore>,
  store: Arc<InMemoryFactoryConfigurationStore>,
  bytes: Arc<DocumentBytes>,
}
impl Fixture {
  async fn new() -> Self {
    let project = ProjectId::generate();
    let issues = issue_fixture::manual_issue_store(project).await;
    let issue_id = issue(&issues, project).await;
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    Self {
      project,
      issue_id,
      issues,
      store,
      bytes: Arc::new(DocumentBytes::default()),
    }
  }
  fn owner(
    &self,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    now: i64,
  ) -> Arc<ProjectTextIntake<InMemoryFactoryConfigurationStore, DocumentBytes>> {
    Arc::new(
      ProjectTextIntake::new(
        self.store.clone(),
        self.bytes.clone(),
        Arc::new(RecordingResolver::default()),
        policy,
        Arc::new(DocumentClock(time(now))),
      )
      .with_issue_documents(self.issues.clone()),
    )
  }
  fn submission(&self, key: &str, content: &str) -> IssueDocumentSubmission {
    IssueDocumentSubmission {
      project_id: self.project,
      issue_id: self.issue_id,
      name: "Investigation notes".parse().unwrap(),
      content: content.into(),
      idempotency_key: key.parse().unwrap(),
    }
  }
}

#[test]
fn issue_documents_cannot_be_read_as_generic_project_inputs() {
  run(async {
    let f = Fixture::new().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy, 20));
    let document = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: f.submission("notes", "Private task content"),
        },
      )
      .await
      .unwrap();
    let reads = f.bytes.reads.load(Ordering::SeqCst);
    let generic = FactoryProjectInputReader::new(f.store.clone(), f.bytes.clone());
    assert!(
      generic
        .read(ReadFactoryArtifact {
          project_id: f.project,
          artifact: document.artifact,
          maximum_bytes: 4096,
          observed_at: time(21),
          required_until: time(30),
        })
        .await
        .is_err(),
      "Project input access cannot replace exact Issue ownership checks"
    );
    assert_eq!(f.bytes.reads.load(Ordering::SeqCst), reads, "deny before private I/O");
  });
}

#[test]
fn issue_documents_publish_only_under_their_issue_and_not_in_project_documents() {
  run(async {
    let f = Fixture::new().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let owner = f.owner(policy.clone(), 20);
    let commands = AuthorizedCommandHandler::new(policy.clone(), owner.clone());
    let queries = AuthorizedQueryHandler::new(policy, owner);
    let document = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: f.submission("notes", "Private task notes\r\nExact bytes.\n"),
        },
      )
      .await
      .unwrap();
    assert_eq!(document.project_id, f.project);
    assert_eq!(document.issue_id, f.issue_id);
    assert_eq!(document.actor.kind, AuditActorKind::UnauthenticatedManagement);
    let list = queries
      .handle_query(
        &context(),
        ListIssueDocumentsQuery::try_new(f.project, f.issue_id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(list.items, vec![document.clone()]);
    assert_eq!(list.next_cursor, None);
    let shared = queries
      .handle_query(
        &context(),
        ListProjectInputDocumentsQuery {
          page: CurrentDefinitionPageInput::try_new(f.project, None, 10).unwrap(),
        },
      )
      .await
      .unwrap();
    assert!(shared.items.is_empty());
    assert!(
      queries
        .handle_query(
          &context(),
          ReadProjectInputDocumentQuery {
            project_id: f.project,
            artifact_id: document.artifact.artifact_id(),
          }
        )
        .await
        .is_err()
    );
    let content = queries
      .handle_query(
        &context(),
        ReadIssueDocumentQuery {
          project_id: f.project,
          issue_id: f.issue_id,
          artifact_id: document.artifact.artifact_id(),
        },
      )
      .await
      .unwrap();
    assert_eq!(content.content, "Private task notes\r\nExact bytes.\n");
    assert!(!format!("{content:?}").contains("Private task notes"));
    assert_eq!(
      f.issues.issue(f.project, f.issue_id, None).await.unwrap().version.get(),
      1
    );
    assert!(
      f.issues
        .issue_executions(f.project, f.issue_id, None, None, 10)
        .await
        .unwrap()
        .items
        .is_empty()
    );
  });
}

struct IssueEditor(IssueId);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for IssueEditor {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if (resource.kind() == ManagementResourceKind::Project && action == ManagementAction::Publish)
      || (resource.kind() == ManagementResourceKind::Issue
        && resource.identity().is_some_and(|id| id.as_str() != self.0.to_string()))
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

#[test]
fn issue_documents_use_issue_update_rights_and_reject_other_parent_routes_before_io() {
  run(async {
    let f = Fixture::new().await;
    let other = issue(&f.issues, f.project).await;
    let policy = Arc::new(IssueEditor(f.issue_id));
    let owner = f.owner(policy.clone(), 20);
    let commands = AuthorizedCommandHandler::new(policy.clone(), owner.clone());
    let document = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: f.submission("notes", "Private notes"),
        },
      )
      .await
      .unwrap();
    assert!(
      owner
        .publish_document(
          &context(),
          ProjectDocumentSubmission {
            project_id: f.project,
            name: "Shared".parse().unwrap(),
            content: "Not permitted".into(),
            idempotency_key: "shared".parse().unwrap(),
            purpose: Default::default(),
          }
        )
        .await
        .is_err(),
      "an Issue editor need not have Project publication rights"
    );
    let reads = f.bytes.reads.load(Ordering::SeqCst);
    for policy in [
      policy as Arc<dyn ManagementAuthorizationPolicy>,
      Arc::new(TrustedNetworkManagementPolicy),
    ] {
      let queries = AuthorizedQueryHandler::new(policy.clone(), f.owner(policy, 20));
      assert!(
        queries
          .handle_query(
            &context(),
            ReadIssueDocumentQuery {
              project_id: f.project,
              issue_id: other,
              artifact_id: document.artifact.artifact_id(),
            }
          )
          .await
          .is_err(),
        "even access to both Issues cannot reparent an Artifact"
      );
    }
    let all = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(all.clone(), f.owner(all.clone(), 20));
    assert!(
      queries
        .handle_query(
          &context(),
          ListIssueDocumentsQuery::try_new(f.project, other, None, 10).unwrap()
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    let commands = AuthorizedCommandHandler::new(all.clone(), f.owner(all, 20));
    for (project_id, issue_id) in [(f.project, IssueId::generate()), (ProjectId::generate(), f.issue_id)] {
      let mut submission = f.submission("invalid-owner", "Private notes");
      submission.project_id = project_id;
      submission.issue_id = issue_id;
      assert!(
        commands
          .handle_command(&context(), PublishIssueDocumentCommand { submission })
          .await
          .is_err()
      );
    }
    assert_eq!(f.bytes.reads.load(Ordering::SeqCst), reads);
    assert_eq!(f.bytes.writes.load(Ordering::SeqCst), 1);
  });
}

#[test]
fn issue_documents_recover_and_replay_original_versions_actor_and_retention() {
  run(async {
    let f = Fixture::new().await;
    let author = ManagementRequestContext::new(
      ManagementActor::authenticated("alice").unwrap(),
      crate::ManagementSecurityScope::new("operator:alice").unwrap(),
      ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
      ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
    )
    .unwrap();
    let policy = Arc::new(IssueEditor(f.issue_id));
    let command = PublishIssueDocumentCommand {
      submission: f.submission("notes", "Original notes\r\n"),
    };
    f.bytes.lose_ack.store(true, Ordering::SeqCst);
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy.clone(), 20));
    assert!(commands.handle_command(&author, command.clone()).await.is_err());
    let pending = AuthorizedQueryHandler::new(policy.clone(), f.owner(policy.clone(), 20));
    assert!(
      pending
        .handle_query(
          &author,
          ListIssueDocumentsQuery::try_new(f.project, f.issue_id, None, 10).unwrap()
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy.clone(), 50));
    let first = commands.handle_command(&author, command.clone()).await.unwrap();
    assert_eq!(
      first.actor,
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some("alice".into())
      }
    );
    assert_eq!(first.published_at, time(50));
    assert_eq!(first.expires_at, time(20 + MAX_SOURCE_RETENTION_MILLIS as i64));
    let second = commands
      .handle_command(
        &author,
        PublishIssueDocumentCommand {
          submission: f.submission("notes-v2", "Revised notes\n"),
        },
      )
      .await
      .unwrap();
    assert_ne!(first.artifact, second.artifact);
    let restarted = f.owner(policy.clone(), 75);
    let commands = AuthorizedCommandHandler::new(policy.clone(), restarted.clone());
    assert_eq!(commands.handle_command(&author, command.clone()).await.unwrap(), first);
    let mut changed = command.clone();
    changed.submission.content = "Changed reuse".into();
    assert!(commands.handle_command(&author, changed).await.is_err());
    let queries = AuthorizedQueryHandler::new(policy.clone(), restarted);
    for (resource, expected) in [(&first, "Original notes\r\n"), (&second, "Revised notes\n")] {
      let read = queries
        .handle_query(
          &author,
          ReadIssueDocumentQuery {
            project_id: f.project,
            issue_id: f.issue_id,
            artifact_id: resource.artifact.artifact_id(),
          },
        )
        .await
        .unwrap();
      assert_eq!(read.content, expected);
      assert_eq!(&read.document, resource);
    }
    let history = queries
      .handle_query(
        &author,
        ListIssueDocumentsQuery::try_new(f.project, f.issue_id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(history.items.len(), 2);
    assert!(history.items.contains(&first) && history.items.contains(&second));
    let reads = f.bytes.reads.load(Ordering::SeqCst);
    let expired = f.owner(policy.clone(), first.expires_at.unix_millis());
    let queries = AuthorizedQueryHandler::new(policy.clone(), expired.clone());
    assert!(
      queries
        .handle_query(
          &author,
          ReadIssueDocumentQuery {
            project_id: f.project,
            issue_id: f.issue_id,
            artifact_id: first.artifact.artifact_id(),
          }
        )
        .await
        .is_err()
    );
    let commands = AuthorizedCommandHandler::new(policy, expired);
    assert_eq!(commands.handle_command(&author, command).await.unwrap(), first);
    assert_eq!(
      f.bytes.writes.load(Ordering::SeqCst),
      2,
      "recovery/replay reuse original byte identities"
    );
    assert_eq!(
      f.bytes.reads.load(Ordering::SeqCst),
      reads,
      "expiry never restores or reads old bodies"
    );
  });
}

#[test]
fn issue_documents_keep_download_permission_separate_from_metadata_visibility() {
  run(async {
    let f = Fixture::new().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy, 20));
    let document = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: f.submission("notes", "Private text"),
        },
      )
      .await
      .unwrap();
    let policy = Arc::new(MetadataWithoutDownload);
    let queries = AuthorizedQueryHandler::new(policy.clone(), f.owner(policy, 20));
    let list = queries
      .handle_query(
        &context(),
        ListIssueDocumentsQuery::try_new(f.project, f.issue_id, None, 10).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(list.items, vec![document.clone()]);
    assert!(!serde_json::to_string(&list).unwrap().contains("Private text"));
    let reads = f.bytes.reads.load(Ordering::SeqCst);
    assert!(
      queries
        .handle_query(
          &context(),
          ReadIssueDocumentQuery {
            project_id: f.project,
            issue_id: f.issue_id,
            artifact_id: document.artifact.artifact_id()
          }
        )
        .await
        .is_err()
    );
    assert_eq!(f.bytes.reads.load(Ordering::SeqCst), reads);
  });
}

#[test]
fn issue_documents_replay_keys_are_partitioned_by_issue_and_security_scope() {
  run(async {
    let f = Fixture::new().await;
    let other = issue(&f.issues, f.project).await;
    let policy = Arc::new(AnyScope);
    let owner = f.owner(policy.clone(), 20);
    let commands = AuthorizedCommandHandler::new(policy.clone(), owner.clone());
    let command = PublishIssueDocumentCommand {
      submission: f.submission("same-key", "Issue A notes"),
    };
    let first = commands.handle_command(&context(), command.clone()).await.unwrap();
    let mut other_command = command.clone();
    other_command.submission.issue_id = other;
    other_command.submission.content = "Issue B notes".into();
    let second = commands.handle_command(&context(), other_command).await.unwrap();
    assert_ne!(first.artifact, second.artifact);
    assert_eq!(second.issue_id, other);
    let other_scope = ManagementRequestContext::new(
      ManagementActor::authenticated("bob").unwrap(),
      crate::ManagementSecurityScope::new("operator:bob").unwrap(),
      ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
      ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
    )
    .unwrap();
    let mut scoped_command = command.clone();
    scoped_command.submission.content = "Bob's separate publication".into();
    let scoped = commands.handle_command(&other_scope, scoped_command).await.unwrap();
    assert_ne!(scoped.artifact, first.artifact);
    assert_eq!(scoped.actor.identity.as_deref(), Some("bob"));
    assert_eq!(commands.handle_command(&context(), command).await.unwrap(), first);
    assert_eq!(f.bytes.writes.load(Ordering::SeqCst), 3);
  });
}

struct ArtifactChoice(ArtifactId);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for ArtifactChoice {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let visibility = if resource.kind() == ManagementResourceKind::Artifact {
      ManagementVisibility::restricted([ManagementResource::instance(
        ManagementResourceKind::Artifact,
        ManagementResourceIdentity::new(self.0.to_string()).unwrap(),
      )
      .unwrap()])
      .unwrap()
    } else {
      ManagementVisibility::all()
    };
    Ok(ManagementAuthorizationGrant::new(visibility))
  }
}

#[test]
fn issue_documents_filter_artifact_visibility_before_bounded_pagination() {
  run(async {
    let f = Fixture::new().await;
    let all = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(all.clone(), f.owner(all.clone(), 20));
    let mut documents = vec![];
    for key in ["one", "two", "three"] {
      documents.push(
        commands
          .handle_command(
            &context(),
            PublishIssueDocumentCommand {
              submission: f.submission(key, key),
            },
          )
          .await
          .unwrap(),
      );
    }
    let visible = documents
      .into_iter()
      .max_by_key(|doc| doc.artifact.artifact_id())
      .unwrap();
    let policy = Arc::new(ArtifactChoice(visible.artifact.artifact_id()));
    let queries = AuthorizedQueryHandler::new(policy.clone(), f.owner(policy, 20));
    let page = queries
      .handle_query(
        &context(),
        ListIssueDocumentsQuery::try_new(f.project, f.issue_id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items, vec![visible.clone()]);
    assert_eq!(
      page.next_cursor, None,
      "hidden earlier documents cannot consume the page"
    );
    assert!(
      queries
        .handle_query(
          &context(),
          ListIssueDocumentsQuery::try_new(f.project, f.issue_id, Some(visible.artifact.artifact_id()), 1).unwrap()
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(ListIssueDocumentsQuery::try_new(f.project, f.issue_id, None, 0).is_err());
    assert!(
      ListIssueDocumentsQuery::try_new(
        f.project,
        f.issue_id,
        None,
        octacity_server_store::MAX_CURRENT_DEFINITION_PAGE_SIZE + 1
      )
      .is_err()
    );
  });
}

struct IssueViewer;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for IssueViewer {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::Update && resource.kind() == ManagementResourceKind::Issue {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

#[test]
fn issue_documents_reject_unbounded_text_forged_attribution_and_missing_issue_write_rights() {
  run(async {
    let f = Fixture::new().await;
    let policy = Arc::new(IssueViewer);
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy, 20));
    assert!(
      commands
        .handle_command(
          &context(),
          PublishIssueDocumentCommand {
            submission: f.submission("denied", "Private notes")
          }
        )
        .await
        .is_err()
    );
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy, 20));
    for content in [
      String::new(),
      "   ".into(),
      "invalid\0text".into(),
      "x".repeat(MAX_SOURCE_INPUT_BYTES as usize + 1),
    ] {
      assert!(
        commands
          .handle_command(
            &context(),
            PublishIssueDocumentCommand {
              submission: f.submission("invalid", &content)
            }
          )
          .await
          .is_err()
      );
    }
    for field in ["actor", "owner", "purpose", "artifact"] {
      let mut forged = serde_json::to_value(f.submission("forged", "Private notes")).unwrap();
      forged[field] = serde_json::json!("invented-authority");
      assert!(serde_json::from_value::<IssueDocumentSubmission>(forged).is_err());
    }
    assert_eq!(f.bytes.writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.bytes.reads.load(Ordering::SeqCst), 0);
  });
}
