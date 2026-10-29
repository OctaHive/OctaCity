use crate::factory_admission_tests::{RecordingResolver, context, repository, seeded_store, time};
use crate::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::{ArtifactId, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{FactoryConfigurationId, FactoryDigest, FactoryKey};
use octacity_server_store::testing;
use octacity_server_store::{testing::InMemoryFactoryConfigurationStore, *};
use std::{
  collections::BTreeMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
};
fn run(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(time(20))
  }
}
struct Bytes {
  store: Arc<InMemoryFactoryConfigurationStore>,
  project: ProjectId,
  objects: Mutex<BTreeMap<ArtifactId, (ArtifactObject, Vec<u8>)>>,
  key: Mutex<IdempotencyKey>,
  writes: AtomicUsize,
  lose_ack: AtomicBool,
  corrupt: AtomicBool,
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    let replay_key = self.key.lock().unwrap().clone();
    let prepared = self
      .store
      .project_text_preparation(self.project, &context().security_scope().to_store(), &replay_key)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(prepared.frozen_revision.as_ref().unwrap().as_str(), "0123456789abcdef");
    assert!(
      prepared
        .objects
        .iter()
        .any(|o| o.artifact.artifact_id() == object.artifact_id())
    );
    assert!(prepared.published_at.is_none());
    self.writes.fetch_add(1, Ordering::SeqCst);
    let mut bytes = bytes;
    if self.corrupt.load(Ordering::SeqCst) {
      bytes[0] ^= 1;
    }
    self
      .objects
      .lock()
      .unwrap()
      .insert(object.artifact_id(), (object.clone(), bytes));
    if self.lose_ack.swap(false, Ordering::SeqCst) {
      return Err(ArtifactStoreError::Backend {
        operation: octacity_artifact_store::ArtifactStoreOperation::PublishVerified,
      });
    }
    Ok(())
  }
}
#[async_trait::async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    let objects = self.objects.lock().unwrap();
    let (original, bytes) = objects.get(&object.artifact_id()).ok_or(ArtifactStoreError::NotFound)?;
    assert_eq!(original, object);
    assert!(bytes.len() as u64 <= maximum);
    Ok(bytes.clone())
  }
}
fn submission(
  project: ProjectId,
  configuration: FactoryConfigurationId,
  repository: RepositoryId,
) -> ProjectTextSubmission {
  let ordinary = crate::factory_admission_tests::command(project, configuration, repository);
  ProjectTextSubmission {
    intent: ProjectTextIntent {
      target: ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: configuration,
        configuration_version: ordinary.configuration_version,
        repository_id: repository,
        repository_version: ordinary.repository_version,
      },
      external_identity: ordinary.external_identity,
      source: ordinary.source,
      classification: ordinary.classification,
      mapping: FactoryKey::new("plain-text.v1").unwrap(),
      idempotency_key: "submit".parse().unwrap(),
    },
    documents: ProjectTextDocuments::try_new("Задача\r\nс точными байтами\n", "  критерии\n").unwrap(),
  }
}
#[test]
fn project_text_intake_publishes_exact_owned_inputs_and_uses_ordinary_admission() {
  run(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repo = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repo, Some("refs/heads/main".parse().unwrap())),
    );
    let bytes = Arc::new(Bytes {
      store: store.clone(),
      project,
      objects: Default::default(),
      key: Mutex::new("submit".parse().unwrap()),
      writes: AtomicUsize::new(0),
      lose_ack: AtomicBool::new(false),
      corrupt: AtomicBool::new(false),
    });
    let resolver = Arc::new(RecordingResolver::default());
    let intake = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let command = submission(project, configuration, repo);
    let admitted = intake.submit(&context(), command.clone()).await.unwrap();
    let replay = intake.submit(&context(), command).await.unwrap();
    assert_eq!(admitted.admission, replay.admission);
    assert_eq!(resolver.call_count(), 1);
    let pending = store
      .project_text_preparation(
        project,
        &context().security_scope().to_store(),
        &"submit".parse().unwrap(),
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(pending.admission, Some(admitted.admission.clone()));
    let work = &admitted.admission.work;
    assert_eq!(work.subject().base_revision().as_str(), "0123456789abcdef");
    assert_eq!(
      work.artifacts().task().content_digest(),
      FactoryDigest::content_sha256("Задача\r\nс точными байтами\n".as_bytes())
    );
    for (reference, expected) in [
      (work.artifacts().task(), "Задача\r\nс точными байтами\n"),
      (work.artifacts().acceptance(), "  критерии\n"),
    ] {
      let metadata = store
        .project_input_artifact(project, reference.artifact_id())
        .await
        .unwrap()
        .unwrap();
      assert_eq!(metadata.object.artifact, *reference);
      assert_eq!(metadata.preparation_id, pending.id);
      assert_eq!(metadata.object.media_type.as_str(), "text/plain");
      assert_eq!(
        bytes.objects.lock().unwrap()[&reference.artifact_id()].1,
        expected.as_bytes()
      );
    }
  });
}

#[test]
fn project_text_intake_rejects_invalid_documents_before_reservation_or_resolution() {
  run(async {
    for (task, acceptance) in [
      ("".into(), "a".to_string()),
      (" \n\t".into(), "a".into()),
      ("task".into(), "".into()),
      ("bad\0text".into(), "a".into()),
      ("bad\u{1b}".into(), "a".into()),
      ("t".repeat(MAX_SOURCE_INPUT_BYTES as usize + 1), "a".into()),
    ] {
      assert!(ProjectTextDocuments::try_new(task, acceptance).is_err());
    }
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repo = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repo, Some("refs/heads/main".parse().unwrap())),
    );
    let bytes = Arc::new(Bytes {
      store: store.clone(),
      project,
      objects: Default::default(),
      key: Mutex::new("submit".parse().unwrap()),
      writes: AtomicUsize::new(0),
      lose_ack: AtomicBool::new(false),
      corrupt: AtomicBool::new(false),
    });
    let resolver = Arc::new(RecordingResolver::default());
    let intake = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let mut json = serde_json::to_value(submission(project, configuration, repo)).unwrap();
    json["documents"]["task"] = serde_json::json!("bad\0raw");
    if let Ok(invalid) = serde_json::from_value::<ProjectTextSubmission>(json) {
      assert!(intake.submit(&context(), invalid).await.is_err());
    }
    assert_eq!(resolver.call_count(), 0);
    assert!(bytes.objects.lock().unwrap().is_empty());
    assert!(
      store
        .project_text_preparation(
          project,
          &context().security_scope().to_store(),
          &"submit".parse().unwrap()
        )
        .await
        .unwrap()
        .is_none()
    );
    let mut json = serde_json::to_value(submission(project, configuration, repo)).unwrap();
    json["resolved_revision"] = serde_json::json!("client-proof");
    assert!(serde_json::from_value::<ProjectTextSubmission>(json).is_err());
  });
}

fn fixture() -> (
  Arc<InMemoryFactoryConfigurationStore>,
  Arc<Bytes>,
  Arc<RecordingResolver>,
  ProjectTextSubmission,
) {
  let project = ProjectId::generate();
  let configuration = FactoryConfigurationId::generate();
  let repo = RepositoryId::generate();
  let store = seeded_store(
    project,
    configuration,
    repository(project, repo, Some("refs/heads/main".parse().unwrap())),
  );
  let bytes = Arc::new(Bytes {
    store: store.clone(),
    project,
    objects: Default::default(),
    key: Mutex::new("submit".parse().unwrap()),
    writes: AtomicUsize::new(0),
    lose_ack: AtomicBool::new(false),
    corrupt: AtomicBool::new(false),
  });
  (
    store,
    bytes,
    Arc::new(RecordingResolver::default()),
    submission(project, configuration, repo),
  )
}
struct Deny(ManagementResourceKind);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for Deny {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.0 {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
#[test]
fn project_text_intake_requires_reviewed_authority_and_exact_replay_before_effects() {
  run(async {
    let (store, bytes, resolver, command) = fixture();
    for kind in [
      ManagementResourceKind::Project,
      ManagementResourceKind::FactoryRun,
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Repository,
    ] {
      let intake = ProjectTextIntake::new(
        store.clone(),
        bytes.clone(),
        resolver.clone(),
        Arc::new(Deny(kind)),
        Arc::new(Clock),
      );
      assert!(
        intake.submit(&context(), command.clone()).await.is_err(),
        "{kind:?} must be authorized"
      );
      assert!(
        store
          .project_text_preparation(
            command.intent.target.project_id,
            &context().security_scope().to_store(),
            &command.intent.idempotency_key
          )
          .await
          .unwrap()
          .is_none()
      );
    }
    let intake = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    for change in 0..4 {
      let mut invalid = command.clone();
      match change {
        0 => {
          invalid.intent.target.configuration_version =
            octacity_server_factory::FactoryConfigurationVersion::new(2).unwrap()
        }
        1 => invalid.intent.target.repository_version = octacity_server_domain::RepositoryVersion::new(2).unwrap(),
        2 => invalid.intent.source = RepositorySourceSelection::Reference("refs/heads/not-reviewed".parse().unwrap()),
        _ => invalid.intent.target.project_id = ProjectId::generate(),
      }
      assert!(intake.submit(&context(), invalid.clone()).await.is_err());
      assert!(
        store
          .project_text_preparation(
            invalid.intent.target.project_id,
            &context().security_scope().to_store(),
            &invalid.intent.idempotency_key
          )
          .await
          .unwrap()
          .is_none()
      );
    }
    assert_eq!(resolver.call_count(), 0);
    assert!(bytes.objects.lock().unwrap().is_empty());
    let original = intake.submit(&context(), command.clone()).await.unwrap();
    let mut changed = command.clone();
    changed.documents = ProjectTextDocuments::try_new("different private task", "acceptance").unwrap();
    assert_eq!(
      intake
        .submit(&context(), changed.clone())
        .await
        .unwrap_err()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert!(
      store
        .reserve_project_text(testing::management_mutation(ReserveProjectText {
          submission: changed,
          now: time(21)
        }))
        .await
        .is_err()
    );
    assert_eq!(
      intake.submit(&context(), command.clone()).await.unwrap().admission,
      original.admission
    );
    let denied = ProjectTextIntake::new(
      store,
      bytes,
      resolver,
      Arc::new(Deny(ManagementResourceKind::FactoryRun)),
      Arc::new(Clock),
    );
    assert!(denied.submit(&context(), command).await.is_err());
  });
}

struct NoResolution;
#[async_trait::async_trait]
impl RevisionResolver for NoResolution {
  async fn resolve(
    &self,
    _: RevisionResolutionRequest,
  ) -> Result<octacity_server_domain::ImmutableRevision, RevisionResolutionError> {
    panic!("recovery must retain the original base, even after a branch moves");
  }
}
#[test]
fn project_text_intake_recovers_unknown_writes_without_republishing_or_resolving_again() {
  run(async {
    let (store, bytes, resolver, command) = fixture();
    bytes.lose_ack.store(true, Ordering::SeqCst);
    let intake = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    assert!(intake.submit(&context(), command.clone()).await.is_err());
    let pending = store
      .project_text_preparation(
        command.intent.target.project_id,
        &context().security_scope().to_store(),
        &command.intent.idempotency_key,
      )
      .await
      .unwrap()
      .unwrap();
    assert!(pending.published_at.is_none());
    assert!(pending.admission.is_none());
    assert_eq!(resolver.call_count(), 1);
    for object in &pending.objects {
      assert!(
        store
          .project_input_artifact(command.intent.target.project_id, object.artifact.artifact_id())
          .await
          .unwrap()
          .is_none()
      );
    }
    let restarted = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      Arc::new(NoResolution),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let completed = restarted.submit(&context(), command.clone()).await.unwrap();
    assert_eq!(
      bytes.writes.load(Ordering::SeqCst),
      2,
      "the acknowledged-lost immutable object is observed, not written twice"
    );
    assert_eq!(completed.admission.work.artifacts(), &pending.work_artifacts().unwrap());
    assert_eq!(
      completed.admission.work.subject().base_revision().as_str(),
      "0123456789abcdef"
    );
    assert_eq!(
      restarted.submit(&context(), command).await.unwrap().admission,
      completed.admission
    );
    let (store, bytes, resolver, command) = fixture();
    bytes.corrupt.store(true, Ordering::SeqCst);
    let intake = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver,
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    assert!(intake.submit(&context(), command.clone()).await.is_err());
    let pending = store
      .project_text_preparation(
        command.intent.target.project_id,
        &context().security_scope().to_store(),
        &command.intent.idempotency_key,
      )
      .await
      .unwrap()
      .unwrap();
    assert!(pending.published_at.is_none());
    assert!(pending.admission.is_none());
  });
}

#[test]
fn project_text_intake_fences_resolvers_and_never_renews_preparation_bounds() {
  run(async {
    let (store, _, _, command) = fixture();
    let reserve = |now| {
      testing::management_mutation(ReserveProjectText {
        submission: command.clone(),
        now,
      })
    };
    let first = store.reserve_project_text(reserve(time(20))).await.unwrap();
    assert!(
      store.reserve_project_text(reserve(time(21))).await.is_err(),
      "an active owner cannot be replaced"
    );
    let frozen = store
      .freeze_project_text(first.guard(time(22)), "0123456789abcdef".parse().unwrap())
      .await
      .unwrap();
    let next = store.reserve_project_text(reserve(first.lease_until)).await.unwrap();
    assert_ne!(first.generation, next.generation);
    assert_eq!(first.objects, next.objects);
    assert_eq!(first.expires_at, next.expires_at);
    assert_eq!(first.preparation_deadline, next.preparation_deadline);
    assert_eq!(next.frozen_revision, frozen.frozen_revision);
    assert!(
      store
        .freeze_project_text(first.guard(first.lease_until), "different".parse().unwrap())
        .await
        .is_err()
    );
    assert!(
      store
        .freeze_project_text(next.guard(next.claimed_at), "different".parse().unwrap())
        .await
        .is_err(),
      "the first committed base cannot be overwritten"
    );
    assert!(
      store
        .publish_project_text(first.guard(first.lease_until))
        .await
        .is_err()
    );
    store.release_project_text(next.guard(next.claimed_at)).await.unwrap();
    for _ in 0..32 {
      match store.reserve_project_text(reserve(next.claimed_at)).await {
        Ok(owner) => {
          store.release_project_text(owner.guard(next.claimed_at)).await.unwrap();
        }
        Err(_) => break,
      }
    }
    assert!(
      store.reserve_project_text(reserve(next.claimed_at)).await.is_err(),
      "attempt limits cannot be reset by replay"
    );
    assert!(
      store
        .reserve_project_text(reserve(first.preparation_deadline))
        .await
        .is_err()
    );
    let retained = store
      .project_text_preparation(
        command.intent.target.project_id,
        &context().security_scope().to_store(),
        &command.intent.idempotency_key,
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(retained.reserved_at, first.reserved_at);
    assert_eq!(retained.expires_at, first.expires_at);
    assert!(retained.admission.is_none());
  });
}

struct NoBuilds;
#[async_trait::async_trait]
impl BuildQueryStore for NoBuilds {
  async fn build(&self, _: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    panic!("Project text has no producing Build");
  }
  async fn attempt(&self, _: octacity_server_domain::AttemptId) -> Result<AttemptRecord, StoreError> {
    unreachable!()
  }
  async fn latest_attempt(&self, _: octacity_server_domain::BuildId) -> Result<AttemptRecord, StoreError> {
    unreachable!()
  }
  async fn job(&self, _: octacity_server_domain::JobId) -> Result<JobRecord, StoreError> {
    unreachable!()
  }
}
struct ProjectMetadata(PublishedProjectInput);
#[async_trait::async_trait]
impl ProjectInputQueryStore for ProjectMetadata {
  async fn project_input_artifact(
    &self,
    _: ProjectId,
    _: ArtifactId,
  ) -> Result<Option<PublishedProjectInput>, StoreError> {
    Ok(Some(self.0.clone()))
  }
}
#[test]
fn project_text_intake_inputs_are_readable_without_build_provenance_and_never_as_outputs() {
  run(async {
    let (store, bytes, resolver, command) = fixture();
    let admitted = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver,
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    )
    .submit(&context(), command.clone())
    .await
    .unwrap();
    let fixture = testing::authoritative_store_contract_fixture();
    let artifacts = Arc::new(testing::InMemoryArtifactRecordStore::new(
      testing::ArtifactLeaseFixture {
        access: LeaseAccess {
          lease_id: octacity_server_domain::LeaseId::generate(),
          fence: LeaseFence::from_bytes([7; 32]),
          agent_id: fixture.agent_id,
          registration_epoch: fixture.registration_epoch,
        },
        build_id: fixture.request.build.id,
        attempt_id: fixture.request.attempt_id,
        attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
        job_id: fixture.request.jobs[0].id,
        expires_at: time(1000),
      },
    ));
    let reader = FactoryArtifactReader::new(artifacts.clone(), Arc::new(NoBuilds), bytes.clone())
      .with_project_inputs(store.clone());
    let request = ReadFactoryArtifact {
      project_id: command.intent.target.project_id,
      artifact: admitted.admission.work.artifacts().task().clone(),
      maximum_bytes: 1024,
      observed_at: time(21),
      required_until: time(30),
    };
    let retained = reader.read_input(request.clone()).await.unwrap();
    assert_eq!(retained.bytes(), "Задача\r\nс точными байтами\n".as_bytes());
    assert!(
      reader.read(request.clone()).await.is_err(),
      "manual input must never gain Build output/evidence provenance"
    );
    let metadata = store
      .project_input_artifact(request.project_id, request.artifact.artifact_id())
      .await
      .unwrap()
      .unwrap();
    for change in 0..5 {
      let mut wrong = metadata.clone();
      match change {
        0 => wrong.project_id = ProjectId::generate(),
        1 => wrong.object.media_type = ArtifactMediaType::new("text/html").unwrap(),
        2 => {
          wrong.object.artifact = octacity_server_factory::FactoryArtifactReference::new(
            request.artifact.artifact_id(),
            FactoryDigest::from_bytes([2; 32]),
            request.artifact.encoded_size(),
          )
          .unwrap()
        }
        3 => wrong.expires_at = time(25),
        _ => wrong.published_at = time(22),
      }
      let wrong_reader = FactoryArtifactReader::new(artifacts.clone(), Arc::new(NoBuilds), bytes.clone())
        .with_project_inputs(Arc::new(ProjectMetadata(wrong)));
      assert!(wrong_reader.read_input(request.clone()).await.is_err());
    }
    let mut foreign = request.clone();
    foreign.project_id = ProjectId::generate();
    assert!(reader.read_input(foreign).await.is_err());
    let mut expired = request;
    expired.observed_at = metadata.expires_at;
    expired.required_until = metadata.expires_at;
    assert!(reader.read_input(expired).await.is_err());
  });
}

use crate::issue_test_support as issue_fixture;
struct AnyScope;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for AnyScope {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
#[test]
fn project_text_intake_issue_publication_recovers_original_revision_and_association() {
  run(async {
    let (factory, bytes, resolver, generic) = fixture();
    let project = generic.intent.target.project_id;
    let issues = issue_fixture::manual_issue_store(project).await;
    let edits = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueHandlers::new(
        issues.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let original = edits
      .handle_command(
        &context(),
        CreateIssueCommand {
          intent: CreateIssueIntent {
            project_id: project,
            planning: Default::default(),
            content: IssueContent {
              title: "Issue title".into(),
              description: "Original task".into(),
              acceptance: Some("Original acceptance".into()),
              priority: None,
            },
          },
          idempotency_key: "issue".parse().unwrap(),
          created_at: time(1),
        },
      )
      .await
      .unwrap()
      .issue;
    let requests = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(IssueExecutionHandlers::new(
        issues.clone(),
        factory.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let request = requests
      .handle_command(
        &context(),
        RequestIssueExecutionCommand {
          intent: RequestIssueExecutionIntent {
            classification: Some(generic.intent.classification.clone()),
            project_id: project,
            issue_id: original.id,
            expected_version: original.version,
            target: IssueExecutionTarget {
              configuration_id: generic.intent.target.configuration_id,
              configuration_version: generic.intent.target.configuration_version,
              repository_id: generic.intent.target.repository_id,
              repository_version: generic.intent.target.repository_version,
              source: generic.intent.source.clone(),
              mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
            },
          },
          idempotency_key: "request".parse().unwrap(),
          requested_at: time(10),
        },
      )
      .await
      .unwrap()
      .request;
    *bytes.key.lock().unwrap() = request.admission_key();
    let publisher = Arc::new(IssueExecutionPublisher::new(
      issues.clone(),
      factory.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(AnyScope),
      Arc::new(Clock),
    ));
    let commands = AuthorizedCommandHandler::new(Arc::new(AnyScope), publisher);
    let command = PublishIssueExecutionCommand {
      project_id: project,
      issue_id: original.id,
      request_id: request.id,
      classification: generic.intent.classification,
    };
    let mut changed_review = command.clone();
    changed_review.classification = octacity_server_factory::WorkClassification::new(
      octacity_server_factory::WorkPriority::new(99).unwrap(),
      octacity_server_factory::RiskClass::High,
      Default::default(),
    );
    assert_eq!(
      commands
        .handle_command(&context(), changed_review)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(bytes.writes.load(Ordering::SeqCst), 0);
    let other = ManagementRequestContext::new(
      ManagementActor::authenticated("other").unwrap(),
      crate::ManagementSecurityScope::new("operator:other").unwrap(),
      ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
      ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
    )
    .unwrap();
    assert!(commands.handle_command(&other, command.clone()).await.is_err());
    assert_eq!(bytes.writes.load(Ordering::SeqCst), 0);
    let edited = edits
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id: project,
            id: original.id,
            expected_version: original.version,
            content: IssueContent {
              description: "Changed while queued".into(),
              ..original.content.clone()
            },
          },
          idempotency_key: "edit".parse().unwrap(),
          edited_at: time(12),
        },
      )
      .await
      .unwrap()
      .issue;
    issues
      .fail_next_issue_mutation_at(testing::MutationFailurePoint::Commit)
      .unwrap();
    assert!(commands.handle_command(&context(), command.clone()).await.is_err());
    assert!(
      issues
        .issue_execution(project, original.id, request.id)
        .await
        .unwrap()
        .association
        .is_none()
    );
    let retained = factory
      .project_text_preparation(
        project,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let admitted = retained.admission.unwrap();
    assert_eq!(
      admitted.work.artifacts().task().content_digest(),
      FactoryDigest::content_sha256(b"Issue title\n\nOriginal task")
    );
    assert_eq!(resolver.call_count(), 1);
    let restarted = AuthorizedCommandHandler::new(
      Arc::new(AnyScope),
      Arc::new(IssueExecutionPublisher::new(
        issues.clone(),
        factory,
        bytes.clone(),
        Arc::new(NoResolution),
        Arc::new(AnyScope),
        Arc::new(Clock),
      )),
    );
    let linked = restarted.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(linked.association.as_ref().unwrap().work_id, admitted.work.id());
    assert_eq!(linked.association.as_ref().unwrap().run_id, admitted.run.id());
    assert_eq!(linked.intent.expected_version, original.version);
    assert_eq!(issues.issue(project, original.id, None).await.unwrap(), edited);
    assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
    let mut changed = command.clone();
    changed.classification = octacity_server_factory::WorkClassification::new(
      octacity_server_factory::WorkPriority::new(99).unwrap(),
      octacity_server_factory::RiskClass::Medium,
      Default::default(),
    );
    assert!(restarted.handle_command(&context(), changed).await.is_err());
    assert_eq!(restarted.handle_command(&context(), command).await.unwrap(), linked);
  });
}

struct NoMetadataBytes;
#[async_trait::async_trait]
impl ArtifactByteReader for NoMetadataBytes {
  async fn read_published(&self, _: &ArtifactObject, _: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    panic!("metadata must not read bytes")
  }
}
#[test]
fn issue_artifact_reader_describes_project_input_origin_and_expiry_without_byte_or_build_reads() {
  run(async {
    let (store, bytes, resolver, command) = fixture();
    let admitted = ProjectTextIntake::new(
      store.clone(),
      bytes,
      resolver,
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    )
    .submit(&context(), command.clone())
    .await
    .unwrap();
    let authority = testing::authoritative_store_contract_fixture();
    let artifacts = Arc::new(testing::InMemoryArtifactRecordStore::new(
      testing::ArtifactLeaseFixture {
        access: LeaseAccess {
          lease_id: octacity_server_domain::LeaseId::generate(),
          fence: LeaseFence::from_bytes([7; 32]),
          agent_id: authority.agent_id,
          registration_epoch: authority.registration_epoch,
        },
        build_id: authority.request.build.id,
        attempt_id: authority.request.attempt_id,
        attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
        job_id: authority.request.jobs[0].id,
        expires_at: time(1000),
      },
    ));
    let reader = FactoryArtifactReader::new(artifacts, Arc::new(NoBuilds), Arc::new(NoMetadataBytes))
      .with_project_inputs(store.clone());
    let request = ReadFactoryArtifact {
      project_id: command.intent.target.project_id,
      artifact: admitted.admission.work.artifacts().task().clone(),
      maximum_bytes: 1024,
      observed_at: time(21),
      required_until: time(22),
    };
    let metadata = reader.metadata(request.clone()).await.unwrap();
    assert_eq!(metadata.origin, IssueArtifactOrigin::ProjectInput);
    assert_eq!(metadata.media_type, "text/plain");
    assert_eq!(metadata.availability, IssueArtifactAvailability::Available);
    let expired = ReadFactoryArtifact {
      observed_at: metadata.expires_at.unwrap(),
      required_until: metadata.expires_at.unwrap(),
      ..request
    };
    assert_eq!(
      reader.metadata(expired.clone()).await.unwrap().availability,
      IssueArtifactAvailability::Expired
    );
    assert!(
      reader.content(expired).await.is_err(),
      "expired bytes must not be read or reconstructed"
    );
  });
}

#[test]
fn project_input_owner_publishes_a_named_context_document_without_build_or_admission() {
  use testing::MutationEvidenceProbe;
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let resolver = Arc::new(RecordingResolver::default());
    let bytes = Arc::new(DocumentBytes::default());
    let owner = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let submission = ProjectDocumentSubmission {
      project_id: project,
      name: "Reviewed Project goals".parse().unwrap(),
      content: "Independent Project goals\r\nKeep this exact document.\n".into(),
      idempotency_key: "project-goals".parse().unwrap(),
    };
    let before = store.mutation_evidence_counts().await;
    let resource = owner.publish_document(&context(), submission.clone()).await.unwrap();
    assert_eq!(resource.project_id, submission.project_id);
    assert_eq!(resource.name.as_str(), "Reviewed Project goals");
    assert_eq!(resource.published_at, time(20));
    assert!(resource.expires_at > resource.published_at);
    let reader = FactoryProjectInputReader::new(store.clone(), bytes);
    let retained = reader
      .read(ReadFactoryArtifact {
        project_id: submission.project_id,
        artifact: resource.artifact.clone(),
        maximum_bytes: 4096,
        observed_at: time(20),
        required_until: time(30),
      })
      .await
      .unwrap();
    assert_eq!(retained.bytes(), submission.content.as_bytes());
    assert_eq!(resolver.call_count(), 0);
    let after = store.mutation_evidence_counts().await;
    assert_eq!(
      after.audit,
      before.audit + 1,
      "one accepted document publication has one management audit fact"
    );
    assert_eq!(after.idempotency, before.idempotency + 1);
    assert_eq!(after.outbox, before.outbox + 1);
  });
}

#[derive(Default)]
struct DocumentBytes {
  objects: Mutex<BTreeMap<ArtifactId, (ArtifactObject, Vec<u8>)>>,
  writes: AtomicUsize,
  reads: AtomicUsize,
  lose_ack: AtomicBool,
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for DocumentBytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self
      .objects
      .lock()
      .unwrap()
      .insert(object.artifact_id(), (object.clone(), bytes));
    self.writes.fetch_add(1, Ordering::SeqCst);
    if self.lose_ack.swap(false, Ordering::SeqCst) {
      return Err(ArtifactStoreError::Backend {
        operation: octacity_artifact_store::ArtifactStoreOperation::PublishVerified,
      });
    }
    Ok(())
  }
}
#[async_trait::async_trait]
impl ArtifactByteReader for DocumentBytes {
  async fn read_published(&self, object: &ArtifactObject, _: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    self.reads.fetch_add(1, Ordering::SeqCst);
    let objects = self.objects.lock().unwrap();
    let (retained, bytes) = objects.get(&object.artifact_id()).ok_or(ArtifactStoreError::NotFound)?;
    assert_eq!(object, retained);
    Ok(bytes.clone())
  }
}

struct DocumentClock(Timestamp);
impl FactoryExecutionClock for DocumentClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(self.0)
  }
}
#[test]
fn named_project_document_recovers_an_unknown_write_and_replays_without_renewing_its_identity_or_retention() {
  use testing::ManagementAuditProbe;
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let bytes = Arc::new(DocumentBytes::default());
    bytes.lose_ack.store(true, Ordering::SeqCst);
    let resolver = Arc::new(RecordingResolver::default());
    let owner = |at| {
      ProjectTextIntake::new(
        store.clone(),
        bytes.clone(),
        resolver.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(DocumentClock(time(at))),
      )
    };
    let request = ProjectDocumentSubmission {
      project_id: project,
      name: "Reviewed rules".parse().unwrap(),
      content: "Private Project rules\n".into(),
      idempotency_key: "context-rules".parse().unwrap(),
    };
    assert!(owner(20).publish_document(&context(), request.clone()).await.is_err());
    let recovered = owner(50).publish_document(&context(), request.clone()).await.unwrap();
    let replayed = owner(75).publish_document(&context(), request).await.unwrap();
    assert_eq!(recovered, replayed);
    assert_eq!(recovered.published_at, time(50));
    assert_eq!(recovered.expires_at, time(20 + MAX_SOURCE_RETENTION_MILLIS as i64));
    assert_eq!(
      bytes.writes.load(Ordering::SeqCst),
      1,
      "recovery verifies retained bytes instead of repeating the accepted write"
    );
    assert_eq!(resolver.call_count(), 0);
    let facts = store.management_audit_facts().await;
    assert_eq!(
      facts.len(),
      1,
      "accepted publication is audited once across recovery and replay"
    );
    assert_eq!(facts[0].operation, StoreOperation::PublishProjectDocument);
  });
}

#[test]
fn project_document_management_lists_only_metadata_without_execution_or_private_byte_reads() {
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let bytes = Arc::new(DocumentBytes::default());
    let resolver = Arc::new(RecordingResolver::default());
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let owner = Arc::new(ProjectTextIntake::new(
      store,
      bytes.clone(),
      resolver.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let resource = AuthorizedCommandHandler::new(policy.clone(), owner.clone())
      .handle_command(
        &context(),
        PublishProjectDocumentCommand {
          submission: ProjectDocumentSubmission {
            project_id: project,
            name: "First Project goals".parse().unwrap(),
            content: "Private goals not in discovery".into(),
            idempotency_key: "first-document".parse().unwrap(),
          },
        },
      )
      .await
      .unwrap();
    let reads = bytes.reads.load(Ordering::SeqCst);
    let page = AuthorizedQueryHandler::new(policy, owner)
      .handle_query(
        &context(),
        ListProjectInputDocumentsQuery {
          page: CurrentDefinitionPageInput::try_new(project, None, 25).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      bytes.reads.load(Ordering::SeqCst),
      reads,
      "metadata discovery cannot read private object bytes"
    );
    assert_eq!(page.items, vec![resource]);
    assert!(page.next_cursor.is_none());
    assert!(
      !serde_json::to_string(&page)
        .unwrap()
        .contains("Private goals not in discovery")
    );
    assert_eq!(resolver.call_count(), 0);
  });
}

#[test]
fn named_project_document_rejects_an_absent_project_before_private_byte_effects() {
  run(async {
    let bytes = Arc::new(DocumentBytes::default());
    let owner = ProjectTextIntake::new(
      Arc::new(InMemoryFactoryConfigurationStore::new()),
      bytes.clone(),
      Arc::new(RecordingResolver::default()),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let result = owner
      .publish_document(
        &context(),
        ProjectDocumentSubmission {
          project_id: ProjectId::generate(),
          name: "Goals".parse().unwrap(),
          content: "Reviewed goals".into(),
          idempotency_key: "absent-project".parse().unwrap(),
        },
      )
      .await;
    assert!(matches!(
      result,
      Err(ApplicationError::Store(StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Project
      }))
    ));
    assert_eq!(bytes.writes.load(Ordering::SeqCst), 0);
    assert_eq!(bytes.reads.load(Ordering::SeqCst), 0);
  });
}
