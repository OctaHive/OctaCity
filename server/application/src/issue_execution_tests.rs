use super::*;
use octacity_server_domain::{RepositoryId, RepositoryVersion};
use octacity_server_factory::{FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest};
use octacity_server_store::testing;
use octacity_server_store::*;

#[test]
fn issue_execution_request_freezes_reviewed_revision_and_replays_before_current_text() {
  run_ready(async {
    let (store, factory, issue, command) = fixture_with_content(IssueContent {
      description: "task bytes\n".into(),
      acceptance: Some("acceptance bytes\n".into()),
      ..draft()
    })
    .await;
    let project_id = issue.project_id;
    let issues = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let handlers = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory,
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let first = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(first.disposition, MutationDisposition::Applied);
    assert_eq!(first.request.intent, command.intent);
    assert_eq!(
      first.request.description_digest,
      FactoryDigest::content_sha256(b"task bytes\n")
    );
    assert_eq!(
      first.request.acceptance_digest,
      FactoryDigest::content_sha256(b"acceptance bytes\n")
    );
    assert!(first.request.association.is_none());
    let edited = issues
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id,
            id: issue.id,
            expected_version: issue.version,
            content: draft(),
          },
          idempotency_key: key("edit"),
          edited_at: at(21),
        },
      )
      .await
      .unwrap()
      .issue;
    let replay = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.request, first.request);
    assert_eq!(
      queries
        .handle_query(
          &context(),
          GetIssueExecutionQuery {
            project_id,
            issue_id: issue.id,
            request_id: first.request.id
          }
        )
        .await
        .unwrap(),
      first.request
    );
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), edited);
    assert_eq!(
      store.issue(project_id, issue.id, Some(issue.version)).await.unwrap(),
      issue
    );
    let mut stale = command.clone();
    stale.idempotency_key = key("later-stale");
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
    let mut changed = command;
    changed.intent.expected_version = edited.version;
    assert_eq!(
      commands
        .handle_command(&context(), changed)
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
fn issue_execution_requires_real_acceptance_but_manual_drafts_remain_valid() {
  run_ready(async {
    for acceptance in [None, Some(String::new()), Some(" \n\t".into())] {
      let (store, factory, issue, command) = fixture_with_content(IssueContent { acceptance, ..draft() }).await;
      let project_id = issue.project_id;
      let commands = AuthorizedCommandHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(IssueExecutionHandlers::new(
          store.clone(),
          factory,
          Arc::new(TrustedNetworkManagementPolicy),
        )),
      );
      let before = store.mutation_evidence_counts().await;
      let error = commands.handle_command(&context(), command).await.unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::Invalid
      );
      assert!(matches!(
        error.application().unwrap(),
        ApplicationError::Store(StoreError::InvalidInput {
          operation: StoreOperation::RequestIssueExecution,
          source: StoreInputError::IssueAcceptanceRequired,
        })
      ));
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
    }
  });
}
fn request(
  issue: &IssueRevision,
  configuration_id: FactoryConfigurationId,
  repository_id: RepositoryId,
  identity: &str,
) -> RequestIssueExecutionCommand {
  RequestIssueExecutionCommand {
    intent: RequestIssueExecutionIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_version: issue.version,
      target: IssueExecutionTarget {
        configuration_id,
        configuration_version: FactoryConfigurationVersion::INITIAL,
        repository_id,
        repository_version: RepositoryVersion::INITIAL,
        source: RepositorySourceSelection::DefaultReference,
        mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
      },
    },
    idempotency_key: key(identity),
    requested_at: at(20),
  }
}

type Fixture = (
  Arc<InMemoryProjectStore>,
  Arc<testing::InMemoryFactoryConfigurationStore>,
  IssueRevision,
  RequestIssueExecutionCommand,
);
async fn fixture() -> Fixture {
  fixture_with_content(IssueContent {
    acceptance: Some("verified acceptance".into()),
    ..draft()
  })
  .await
}
async fn fixture_with_content(content: IssueContent) -> Fixture {
  let store = Arc::new(InMemoryProjectStore::new());
  let project_id = project(&store).await;
  let issues = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueHandlers::new(
      store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    )),
  );
  let mut draft = create(project_id, "draft");
  draft.intent.content = content;
  let issue = issues.handle_command(&context(), draft).await.unwrap().issue;
  let configuration_id = FactoryConfigurationId::generate();
  let repository_id = RepositoryId::generate();
  let factory = crate::factory_admission_tests::seeded_store(
    project_id,
    configuration_id,
    crate::factory_admission_tests::repository(project_id, repository_id, Some("refs/heads/main".parse().unwrap())),
  );
  let command = request(&issue, configuration_id, repository_id, "start");
  (store, factory, issue, command)
}
struct ResourcePolicy {
  blocked: ManagementResourceKind,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for ResourcePolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.blocked {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
#[test]
fn issue_execution_checks_admission_authority_visible_exact_targets_and_base_policy() {
  run_ready(async {
    let (store, factory, issue, command) = fixture().await;
    let before = store.mutation_evidence_counts().await;
    for blocked in [
      ManagementResourceKind::FactoryRun,
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Repository,
      ManagementResourceKind::Issue,
      ManagementResourceKind::Project,
    ] {
      let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(ResourcePolicy { blocked });
      let commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      assert!(
        commands.handle_command(&context(), command.clone()).await.is_err(),
        "missing {blocked:?} authority must reject reservation"
      );
      assert_eq!(store.mutation_evidence_counts().await, before);
    }
    let handlers = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    for index in 0..5 {
      let mut invalid = command.clone();
      invalid.idempotency_key = key(&format!("invalid-{index}"));
      match index {
        0 => invalid.intent.target.configuration_version = FactoryConfigurationVersion::new(2).unwrap(),
        1 => invalid.intent.target.repository_version = RepositoryVersion::new(2).unwrap(),
        2 => {
          invalid.intent.target.source = RepositorySourceSelection::Reference("refs/heads/forbidden".parse().unwrap())
        }
        3 => invalid.intent.project_id = project(&store).await,
        _ => invalid.intent.target.repository_id = RepositoryId::generate(),
      }
      assert!(commands.handle_command(&context(), invalid).await.is_err());
    }
    let original = commands.handle_command(&context(), command.clone()).await.unwrap();
    // An unavailable target store cannot turn exact replay into a new lookup/admission.
    let empty = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        empty,
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    assert_eq!(
      commands
        .handle_command(&context(), command.clone())
        .await
        .unwrap()
        .request,
      original.request
    );
    let mut fresh = command;
    fresh.idempotency_key = key("fresh");
    assert!(commands.handle_command(&context(), fresh).await.is_err());
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  });
}

async fn admit_request(
  factory: Arc<testing::InMemoryFactoryConfigurationStore>,
  request: &IssueExecutionRequest,
) -> PublishedFactoryAdmission {
  use octacity_server_domain::ArtifactId;
  use octacity_server_factory::FactoryArtifactReference;
  let resolver = Arc::new(crate::factory_admission_tests::RecordingResolver::default());
  let admissions = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(FactoryAdmissionHandlers::new(factory.clone(), resolver.clone())),
  );
  let target = &request.intent.target;
  let mut command =
    crate::factory_admission_tests::command(request.intent.project_id, target.configuration_id, target.repository_id);
  command.external_identity = request.external_identity();
  command.idempotency_key = request.admission_key();
  command.artifacts = octacity_server_factory::WorkArtifacts::new(
    FactoryArtifactReference::new(ArtifactId::generate(), request.task_digest, request.task_size).unwrap(),
    FactoryArtifactReference::new(
      ArtifactId::generate(),
      request.acceptance_digest,
      request.acceptance_size,
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  let first = admissions.handle_command(&context(), command.clone()).await.unwrap();
  assert_eq!(
    admissions
      .handle_command(&context(), command)
      .await
      .unwrap()
      .work_envelope_id,
    first.work_envelope_id
  );
  assert_eq!(resolver.call_count(), 1);
  factory
    .recover_factory_admission(&FactoryAdmissionIdentity {
      source_scope: FactoryWorkSourceScope {
        source: octacity_server_factory::FactoryKey::new("manual").unwrap(),
        security_scope: context().security_scope().to_store(),
      },
      external_identity: request.external_identity(),
    })
    .await
    .unwrap()
    .unwrap()
}
#[test]
fn issue_execution_associations_retain_multiple_exact_work_and_one_original_run() {
  run_ready(async {
    let (store, factory, issue, command) = fixture().await;
    let handlers = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
    let first = commands
      .handle_command(&context(), command.clone())
      .await
      .unwrap()
      .request;
    assert_eq!(
      first.task_digest,
      FactoryDigest::content_sha256("Разобраться с задачей\n\n".as_bytes())
    );
    let admission = admit_request(factory.clone(), &first).await;
    assert!(
      store
        .issue_work_association(issue.project_id, admission.work.id())
        .await
        .unwrap()
        .is_none()
    );
    let link = AssociateIssueWork {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: Some(first.id),
      admission: admission.clone(),
    };
    let associated = store
      .associate_issue_work(management_mutation(link.clone()))
      .await
      .unwrap();
    assert_eq!(associated.disposition, MutationDisposition::Applied);
    assert_eq!(associated.association.work_id, admission.work.id());
    assert_eq!(associated.association.run_id, admission.run.id());
    assert_eq!(associated.association.requirements_version, Some(issue.version));
    assert_eq!(
      store
        .associate_issue_work(management_mutation(link.clone()))
        .await
        .unwrap()
        .disposition,
      MutationDisposition::Replayed
    );
    let mut later = command.clone();
    later.idempotency_key = key("intentional-later-execution");
    let second = commands.handle_command(&context(), later).await.unwrap().request;
    assert_ne!(first.id, second.id);
    assert_ne!(first.external_identity(), second.external_identity());
    assert_ne!(first.admission_key(), second.admission_key());
    let next = admit_request(factory.clone(), &second).await;
    assert_ne!(admission.work.id(), next.work.id());
    assert_ne!(admission.run.id(), next.run.id());
    let mut wrong = link.clone();
    wrong.admission = next.clone();
    assert!(store.associate_issue_work(management_mutation(wrong)).await.is_err());
    store
      .associate_issue_work(management_mutation(AssociateIssueWork {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: Some(second.id),
        admission: next.clone(),
      }))
      .await
      .unwrap();
    for (request, work) in [(&first, &admission), (&second, &next)] {
      let read = queries
        .handle_query(
          &context(),
          GetIssueExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            request_id: request.id,
          },
        )
        .await
        .unwrap();
      assert_eq!(read.association.unwrap().run_id, work.run.id());
      assert_eq!(
        store
          .issue_work_association(issue.project_id, work.work.id())
          .await
          .unwrap()
          .unwrap()
          .issue_id,
        issue.id
      );
    }
    // Original transport replay remains the reservation receipt, with recovery in the exact read.
    assert_eq!(
      commands.handle_command(&context(), command).await.unwrap().request,
      first
    );
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    assert!(store.retained_issue_push_intents().unwrap().is_empty());
  });
}

#[test]
fn issue_execution_association_rejects_foreign_or_retargeted_work_without_partial_linkage() {
  use octacity_server_factory::{AdmittedFlow, FactoryRun, FactoryRunId, WorkEnvelope, WorkEnvelopeId};
  run_ready(async {
    let (store, factory, issue, command) = fixture().await;
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let request = commands.handle_command(&context(), command).await.unwrap().request;
    let admission = admit_request(factory.clone(), &request).await;
    let issues = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        store.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let foreign_project = project(&store).await;
    let foreign = issues
      .handle_command(&context(), create(foreign_project, "foreign"))
      .await
      .unwrap()
      .issue;
    let second = issues
      .handle_command(&context(), create(issue.project_id, "second"))
      .await
      .unwrap()
      .issue;
    let link = AssociateIssueWork {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: Some(request.id),
      admission: admission.clone(),
    };
    let before = store.mutation_evidence_counts().await;
    let foreign_link = AssociateIssueWork {
      project_id: foreign.project_id,
      issue_id: foreign.id,
      request_id: None,
      admission: admission.clone(),
    };
    assert!(
      store
        .associate_issue_work(management_mutation(foreign_link))
        .await
        .is_err(),
      "Work cannot cross Project authority"
    );
    let mut wrong_run = link.clone();
    wrong_run.admission.run = FactoryRun::admitted(FactoryRunId::generate(), &admission.work);
    assert!(
      store
        .associate_issue_work(management_mutation(wrong_run))
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert!(
      store
        .issue_work_association(issue.project_id, admission.work.id())
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      store
        .issue_execution(issue.project_id, issue.id, request.id)
        .await
        .unwrap()
        .association
        .is_none()
    );
    let original = store
      .associate_issue_work(management_mutation(link.clone()))
      .await
      .unwrap()
      .association;
    let before = store.mutation_evidence_counts().await;
    let other_issue = AssociateIssueWork {
      project_id: issue.project_id,
      issue_id: second.id,
      request_id: None,
      admission: admission.clone(),
    };
    assert!(
      store
        .associate_issue_work(management_mutation(other_issue))
        .await
        .is_err()
    );
    assert!(
      store
        .issue_work_association(foreign_project, admission.work.id())
        .await
        .unwrap()
        .is_none()
    );
    // A replacement Work with otherwise equal inputs cannot occupy a previously linked request.
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      admission.work.configuration().clone(),
      admission.work.external_identity().clone(),
      admission.work.subject().clone(),
      admission.work.artifacts().clone(),
      octacity_server_factory::WorkClassification::new(
        admission.work.priority(),
        admission.work.risk(),
        admission.work.metadata().clone(),
      ),
    )
    .unwrap();
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    let configuration =
      crate::factory_admission_tests::configuration(issue.project_id, request.intent.target.configuration_id);
    let flow = AdmittedFlow::from_configuration(&configuration, &run).unwrap();
    let mut replacement = link.clone();
    replacement.admission = PublishedFactoryAdmission {
      work,
      run,
      flow,
      admitted_at: at(30),
    };
    assert!(
      store
        .associate_issue_work(management_mutation(replacement))
        .await
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      store
        .associate_issue_work(management_mutation(link))
        .await
        .unwrap()
        .association,
      original
    );
  });
}
