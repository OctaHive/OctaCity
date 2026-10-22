//! Real Issue consumer of the same installed Source/Reporter process peers.
use super::*;
use octacity_server_store::testing::{InMemoryProjectStore, management_mutation};

pub(super) async fn card(
  fixture: &Fixture,
  reader: Arc<HostedWorkSourceObservations>,
  runs: &[FactoryRunSummary],
) -> (Arc<InMemoryProjectStore>, IssueRevision) {
  let project = fixture.input.project_id();
  let store = Arc::new(InMemoryProjectStore::new());
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new("Imported pilot").unwrap(),
      idempotency_key: "project".parse().unwrap(),
      created_at: time(1),
    }))
    .await
    .unwrap();
  let issue = card_on(fixture, reader, runs, store.clone(), false).await;
  (store, issue)
}

pub(super) async fn card_on<S, F>(
  fixture: &Fixture<F>,
  reader: Arc<HostedWorkSourceObservations>,
  runs: &[FactoryRunSummary],
  store: Arc<S>,
  existing: bool,
) -> IssueRevision
where
  S: IssueStore
    + IssueWorkflowStore
    + IssueSourceMappingStore
    + IssueDiscussionStore
    + IssueExecutionStore
    + IssueCodeReviewStore
    + 'static,
  F: PilotStore,
{
  let project = fixture.input.project_id();
  let context = &fixture.profile.read.context;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let default = store.default_issue_workflow(project).await.unwrap().workflow;
  let mut definition = default.definition.clone();
  definition.statuses[1].name = "Ready".into();
  let from = definition.initial_status_id;
  let ready = definition.statuses[1].id;
  let declined = definition.statuses[2].id;
  definition.transitions = vec![
    IssueWorkflowTransition {
      id: IssueTransitionId::generate(),
      name: "Review ready".into(),
      from,
      to: ready,
      origins: vec![IssueTransitionOrigin::Manual],
      evidence: vec![],
    },
    IssueWorkflowTransition {
      id: IssueTransitionId::generate(),
      name: "Review decline".into(),
      from: ready,
      to: declined,
      origins: vec![IssueTransitionOrigin::Manual],
      evidence: vec![],
    },
  ];
  definition.transitions.push(IssueWorkflowTransition {
    id: IssueTransitionId::generate(),
    name: "Review again".into(),
    from: declined,
    to: ready,
    origins: vec![IssueTransitionOrigin::Manual],
    evidence: vec![],
  });
  let published = store
    .publish_issue_workflow(management_mutation(PublishIssueWorkflow {
      intent: PublishIssueWorkflowIntent {
        project_id: project,
        id: default.id,
        expected_current_version: Some(default.version),
        definition: definition.clone(),
      },
      idempotency_key: "workflow".parse().unwrap(),
      published_at: time(2),
    }))
    .await
    .unwrap()
    .workflow;
  store
    .select_default_issue_workflow(management_mutation(SelectDefaultIssueWorkflow {
      intent: SelectDefaultIssueWorkflowIntent {
        project_id: project,
        expected_version: store.default_issue_workflow(project).await.unwrap().version,
        workflow_id: published.id,
        workflow_version: published.version,
      },
      idempotency_key: "default".parse().unwrap(),
    }))
    .await
    .unwrap();
  let observed = reader
    .read(context, observation_request(fixture))
    .await
    .unwrap()
    .observations
    .remove(0);
  let selection = IssueSourceSelection {
    project_id: project,
    source: observed.source.clone(),
    item: observed.snapshot.item.clone(),
  };
  let source = Arc::new(
    IssueSourceHandlers::new(store.clone(), reader, policy.clone(), fixture.clock.clone())
      .with_source_mappings(vec![InstalledIssueSourceMapping {
        project_id: project,
        source: selection.source.clone(),
        native_resource: selection.item.resource.clone(),
        readable: WorkSourceReportingPolicy {
          states: vec!["open".into(), "closed".into()],
          labels: vec![],
        },
        writer: None,
      }])
      .unwrap(),
  );
  let imports = AuthorizedCommandHandler::new(policy.clone(), source);
  let mut imported = None;
  // Pipeline identities never participate in canonical native task identity.
  for (index, run) in runs.iter().enumerate() {
    let outcome = imports
      .handle_command(
        context,
        ImportIssueSourceCommand {
          selection: selection.clone(),
          read: observation_request(fixture),
          idempotency_key: format!("import-{index}").parse().unwrap(),
          imported_at: time(2100),
        },
      )
      .await
      .unwrap();
    let IssueSourceOutcome::Linked { link, .. } = outcome else {
      panic!("verified native task");
    };
    if let Some(previous) = imported {
      assert_eq!(previous, link.issue_id);
    } else {
      imported = Some(link.issue_id);
    }
    if existing {
      let link = store
        .issue_work_association(project, run.work_id)
        .await
        .unwrap()
        .expect("Source admission owns durable association");
      assert_eq!(link.issue_id, imported.unwrap());
      assert_eq!(link.run_id, run.id);
    } else {
      let preparation = fixture
        .store
        .source_preparation_for_work(project, run.work_id)
        .await
        .unwrap()
        .unwrap();
      let admission = fixture
        .store
        .recover_factory_admission(&preparation.admission_identity(
          octacity_server_store::ManagementSecurityScope::new(context.security_scope().as_str()).unwrap(),
        ))
        .await
        .unwrap()
        .unwrap();
      // This isolated Issue-consumer fixture supplies an accepted association.
      // Production admission/queue/restart linkage is qualified without this seed
      // by new_source_admissions_recover_one_card_without_manual_association.
      store
        .associate_issue_work(management_mutation(AssociateIssueWork {
          project_id: project,
          issue_id: link.issue_id,
          request_id: None,
          admission,
        }))
        .await
        .unwrap();
    }
  }
  let id = imported.unwrap();
  let original = store.issue(project, id, None).await.unwrap();
  let original = if original.workflow_version != published.version {
    store
      .adopt_issue_workflow(management_mutation(AdoptIssueWorkflow {
        intent: AdoptIssueWorkflowIntent {
          project_id: project,
          id,
          expected_version: original.version,
          expected_workflow_id: original.workflow_id,
          expected_workflow_version: original.workflow_version,
          expected_status_id: original.status_id,
          target_workflow_id: published.id,
          target_workflow_version: published.version,
          status_correspondence: [(original.status_id, from)].into(),
          expected_field_policy: store.issue_field_policy(project, id).await.unwrap(),
        },
        idempotency_key: "pilot-adopt".parse().unwrap(),
        adopted_at: original.updated_at,
      }))
      .await
      .unwrap()
      .issue
  } else {
    original
  };
  assert_eq!(
    original.status_id, from,
    "native effects and completed Runs do not set business status"
  );
  let moved_at = time(original.updated_at.unix_millis().max(2100) + 1);
  let commands = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
  );
  let mut issue = original.clone();
  for edge in definition.transitions.into_iter().take(2) {
    issue = commands
      .handle_command(
        context,
        TransitionIssueCommand {
          intent: TransitionIssueIntent {
            project_id: project,
            id,
            expected_version: issue.version,
            workflow_id: issue.workflow_id,
            workflow_version: issue.workflow_version,
            expected_status_id: issue.status_id,
            transition_id: edge.id,
            reason: None,
          },
          idempotency_key: format!("move-{}", edge.id).parse().unwrap(),
          moved_at,
        },
      )
      .await
      .unwrap()
      .issue;
    assert_eq!(issue.status_id, edge.to);
  }
  imports
    .handle_command(
      context,
      PublishIssueSourceMappingCommand {
        intent: PublishIssueSourceMappingIntent {
          project_id: project,
          issue_id: id,
          expected_issue_version: issue.version,
          expected_mapping: None,
          definition: IssueSourceMappingDefinition {
            source: selection.source,
            reporter: None,
            push_enabled: false,
            fields: [
              (IssueField::Title, IssueSourceFieldAuthority::SourceOwnedPull),
              (IssueField::Description, IssueSourceFieldAuthority::Unsynchronized),
            ]
            .into(),
            status: None,
          },
        },
        idempotency_key: "field-ownership".parse().unwrap(),
        published_at: time(moved_at.unix_millis() + 1),
      },
    )
    .await
    .unwrap();
  assert!(
    commands
      .handle_command(
        context,
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id: project,
            id,
            expected_version: issue.version,
            content: IssueContent {
              title: "Unauthorized replacement".into(),
              ..issue.content.clone()
            }
          },
          idempotency_key: "owned-title-edit".parse().unwrap(),
          edited_at: time(moved_at.unix_millis() + 1),
        }
      )
      .await
      .is_err(),
    "separately reviewed source ownership is enforced without a native write"
  );
  // A plain operator comment cannot update native fields, request retry or change status.
  store
    .append_issue_comment(management_mutation(AppendIssueComment {
      id: IssueCommentId::generate(),
      intent: AppendIssueCommentIntent {
        project_id: project,
        issue_id: id,
        parent_id: None,
        body: IssueCommentBody::try_new("@agent retry: operator discussion only").unwrap(),
      },
      limits: IssueDiscussionLimits::default(),
      idempotency_key: "comment".parse().unwrap(),
      created_at: time(moved_at.unix_millis() + 1),
    }))
    .await
    .unwrap();
  assert_eq!(store.issue(project, id, None).await.unwrap(), issue);
  assert_eq!(
    store
      .issue_status_history(project, id, None, 10)
      .await
      .unwrap()
      .items
      .len(),
    if existing { 4 } else { 3 }
  );
  for run in runs {
    let association = store
      .issue_work_association(project, run.work_id)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(association.issue_id, id);
    assert_eq!(association.run_id, run.id);
  }
  let execution_reads = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      fixture.store.clone(),
      policy.clone(),
    )),
  );
  for run in runs {
    let projection = execution_reads
      .handle_query(
        context,
        GetIssueWorkExecutionQuery {
          project_id: project,
          issue_id: id,
          work_id: run.work_id,
        },
      )
      .await
      .unwrap();
    assert_eq!(projection.association.issue_id, id);
    let IssueRunObservation::Available(observed) = projection.run else {
      panic!("original imported Run");
    };
    assert_eq!(observed.id, run.id);
    assert_eq!(observed.work_id, run.work_id);
  }
  reviews(&store, &issue, fixture, policy).await;
  assert_eq!(
    store.issue(project, id, None).await.unwrap(),
    issue,
    "PR state and CI never complete the task"
  );
  issue
}
async fn reviews<S, F>(
  store: &Arc<S>,
  issue: &IssueRevision,
  fixture: &Fixture<F>,
  policy: Arc<TrustedNetworkManagementPolicy>,
) where
  S: IssueCodeReviewStore + 'static,
  F: PilotStore,
{
  let reader = Arc::new(review_reader::Reader {
    checks: true,
    ..Default::default()
  });
  let handlers = Arc::new(IssueCodeReviewHandlers::new(
    store.clone(),
    reader.clone(),
    policy.clone(),
    fixture.clock.clone(),
  ));
  let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
  let queries = AuthorizedQueryHandler::new(policy, handlers);
  let context = &fixture.profile.read.context;
  let mut source = fixture.profile.read.binding.clone();
  source.installation = ImmutableReference::new(key("independent-code-reader"), key("v1"), digest(91));
  let attachment = AttachIssueCodeReviewCommand {
    intent: AttachIssueCodeReviewIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_version: issue.version,
      selection: IssueCodeReviewSelection {
        source,
        item: WorkSourceItem {
          resource: ExternalWorkIdentity::new("code-repository").unwrap(),
          item: ExternalWorkIdentity::new("review-42").unwrap(),
        },
      },
      binding: None,
    },
    idempotency_key: "review".parse().unwrap(),
  };
  let attached = commands.handle_command(context, attachment).await.unwrap();
  commands
    .handle_command(
      context,
      RefreshIssueCodeReviewCommand {
        project_id: issue.project_id,
        issue_id: issue.id,
        association_id: attached.association.id,
        idempotency_key: "review-new-head".parse().unwrap(),
      },
    )
    .await
    .unwrap();
  reader.revoked.store(true, Ordering::SeqCst);
  let first = queries
    .handle_query(
      context,
      ListIssueExternalChecksQuery::try_new(issue.project_id, issue.id, attached.association.id, None).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(first.availability, IssueCodeReviewAvailability::Blocked);
  assert_eq!(first.source, IssueExternalCheckSource::ExternalCi);
  assert_eq!(first.coverage, IssueResultCoverage::Partial);
  assert_eq!(first.items[0].head.as_ref().unwrap().as_str(), "old-head");
  let old = first.items[0].checks.as_ref().unwrap();
  assert_eq!(
    old.items.iter().map(|item| item.state).collect::<Vec<_>>(),
    vec![
      IssueCheckState::Pass,
      IssueCheckState::Fail,
      IssueCheckState::Missing,
      IssueCheckState::Indeterminate
    ]
  );
  assert!(old.has_more);
  let second = queries
    .handle_query(
      context,
      ListIssueExternalChecksQuery::try_new(issue.project_id, issue.id, attached.association.id, first.next_cursor)
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(second.items[0].head.as_ref().unwrap().as_str(), "new-head");
  assert_eq!(second.items[0].checks.as_ref().unwrap().head.as_str(), "new-head");
  assert_eq!(
    reader.reads.load(Ordering::SeqCst),
    2,
    "history reads after revocation never dispatch a provider"
  );
}
