//! Durable shared Project text intake, with controlled external bytes and VCS.
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{ManagementSecurityScope, MutationDisposition};
use octacity_server_store::{testing::management_mutation, *};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
#[allow(dead_code)]
#[path = "support/protected_input_transfer.rs"]
mod protected_input_transfer;
use octacity_artifact_store::{
  ArtifactBytePublisher, ArtifactByteReader, ArtifactObject, ArtifactStoreError, ArtifactStoreOperation,
};
use std::sync::{
  Arc,
  atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
};
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[path = "project_text_intake/status_actions.rs"]
mod status_actions;
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
fn key(s: &str) -> IdempotencyKey {
  s.parse().unwrap()
}
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::from_u128(17)).unwrap())
}
struct Clock(AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0.load(Ordering::SeqCst)))
  }
}
struct Resolver(AtomicUsize);
#[async_trait::async_trait]
impl RevisionResolver for Resolver {
  async fn resolve(&self, _: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    let n = self.0.fetch_add(1, Ordering::SeqCst);
    assert_eq!(n, 0, "recovery must reuse original base");
    Ok(ImmutableRevision::new("original-base").unwrap())
  }
}
struct Bytes {
  root: std::path::PathBuf,
  _directory: Option<tempfile::TempDir>,
  writes: AtomicUsize,
  lose_ack: AtomicBool,
  pause_after: usize,
  fail_delete: AtomicBool,
}
impl Bytes {
  fn new() -> Self {
    let directory = tempfile::tempdir().unwrap();
    Self {
      root: directory.path().to_path_buf(),
      _directory: Some(directory),
      writes: AtomicUsize::new(0),
      lose_ack: AtomicBool::new(false),
      pause_after: 0,
      fail_delete: AtomicBool::new(false),
    }
  }
  fn shared(root: std::path::PathBuf, pause_after: usize) -> Self {
    Self {
      root,
      _directory: None,
      writes: AtomicUsize::new(0),
      lose_ack: AtomicBool::new(false),
      pause_after,
      fail_delete: AtomicBool::new(false),
    }
  }
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    let count = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
    std::fs::write(self.root.join(object.upload_id().to_string()), bytes).unwrap();
    if count == self.pause_after {
      std::fs::write(self.root.join("paused"), b"ready").unwrap();
      std::future::pending::<()>().await;
    }
    if self.lose_ack.swap(false, Ordering::SeqCst) {
      return Err(ArtifactStoreError::Backend {
        operation: ArtifactStoreOperation::PublishVerified,
      });
    }
    Ok(())
  }
}
#[async_trait::async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, _: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    std::fs::read(self.root.join(object.upload_id().to_string())).map_err(|_| ArtifactStoreError::NotFound)
  }
}
async fn setup(store: &PostgresStore) -> (IssueRevision, ProjectTextSubmission) {
  setup_with_flow(store, None).await
}
async fn setup_with_flow(
  store: &PostgresStore,
  flow: Option<FactoryFlowConfiguration>,
) -> (IssueRevision, ProjectTextSubmission) {
  let project = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new("Manual text").unwrap(),
      idempotency_key: key("project"),
      created_at: at(1),
    }))
    .await
    .unwrap();
  let issue = store
    .create_issue(management_mutation(CreateIssue {
      id: IssueId::generate(),
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: "Задача".into(),
          description: "точные\r\nбайты".into(),
          acceptance: Some(" критерии\n".into()),
          priority: None,
        },
      },
      idempotency_key: key("issue"),
      created_at: at(2),
    }))
    .await
    .unwrap()
    .issue;
  let repo = RepositoryId::generate();
  store
    .create_repository(management_mutation(CreateRepository {
      id: repo,
      project_id: project,
      name: RepositoryName::new("Source").unwrap(),
      definition: RepositoryDefinition {
        vcs_integration_id: IntegrationId::generate(),
        repository_locator: RepositoryLocator::new("https://example.test/source.git").unwrap(),
        selection: RepositorySelectionPolicy {
          allowed_references: Default::default(),
          default_reference: None,
          allow_exact_revision: true,
        },
      },
      idempotency_key: key("repo"),
      published_at: at(2),
    }))
    .await
    .unwrap();
  let mut f = postgres::factory_configuration::fixture(project);
  if let Some(flow) = flow {
    f.draft.hard_budget = flow.limits.budget();
    f.draft.flow = Some(flow);
  }
  let id = FactoryConfigurationId::generate();
  let digest = FactoryDigest::from_bytes([7; 32]);
  let configuration = FactoryConfiguration::publish(
    id,
    FactoryConfigurationVersion::INITIAL,
    project,
    digest,
    f.draft.clone(),
    &f.choices,
  )
  .unwrap();
  store
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      configuration,
      intent: FactoryConfigurationMutationIntent::Create {
        id,
        project_id: project,
        definition_digest: digest,
        draft: f.draft,
      },
      idempotency_key: key("config"),
      published_at: at(2),
    }))
    .await
    .unwrap();
  let submission = ProjectTextSubmission {
    intent: ProjectTextIntent {
      target: ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: id,
        configuration_version: FactoryConfigurationVersion::INITIAL,
        repository_id: repo,
        repository_version: RepositoryVersion::INITIAL,
      },
      external_identity: ExternalWorkIdentity::new("manual-text/one").unwrap(),
      source: RepositorySourceSelection::ExactRevision(ImmutableRevision::new("original-base").unwrap()),
      classification: WorkClassification::new(WorkPriority::new(1).unwrap(), RiskClass::Low, Default::default()),
      mapping: FactoryKey::new("plain-text.v1").unwrap(),
      idempotency_key: key("text"),
    },
    documents: ProjectTextDocuments::try_new("Задача\r\n", " критерии\n").unwrap(),
  };
  (issue, submission)
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn durable_shared_text_recovers_unknown_bytes_and_original_admission_after_restart() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup(&store).await;
  let bytes = Arc::new(Bytes::new());
  let resolver = Arc::new(Resolver(AtomicUsize::new(0)));
  let clock = Arc::new(Clock(AtomicI64::new(20)));
  bytes.lose_ack.store(true, Ordering::SeqCst);
  let intake = ProjectTextIntake::new(
    store.clone(),
    bytes.clone(),
    resolver.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
    clock.clone(),
  );
  assert!(intake.submit(&context(), submission.clone()).await.is_err());
  let reserved = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &key("text"),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(reserved.admission.is_none());
  assert!(reserved.published_at.is_none());
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let intake = ProjectTextIntake::new(
    restarted.clone(),
    bytes.clone(),
    resolver.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
    clock,
  );
  let admitted = intake.submit(&context(), submission.clone()).await.unwrap();
  assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
  assert_eq!(resolver.0.load(Ordering::SeqCst), 1);
  assert_eq!(
    admitted.admission.work.subject().base_revision().as_str(),
    "original-base"
  );
  let original = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &key("text"),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(original.objects, reserved.objects);
  assert_eq!(original.admission, Some(admitted.admission.clone()));
  let replay = intake.submit(&context(), submission).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.admission, admitted.admission);
  database.cleanup().await;
}

async fn reviewed_request(
  store: &PostgresStore,
  issue: &IssueRevision,
  submission: &ProjectTextSubmission,
) -> IssueExecutionRequest {
  store
    .request_issue_execution(management_mutation(RequestIssueExecution {
      id: IssueExecutionRequestId::generate(),
      intent: RequestIssueExecutionIntent {
        classification: None,
        project_id: issue.project_id,
        issue_id: issue.id,
        expected_version: issue.version,
        target: IssueExecutionTarget {
          configuration_id: submission.intent.target.configuration_id,
          configuration_version: submission.intent.target.configuration_version,
          repository_id: submission.intent.target.repository_id,
          repository_version: submission.intent.target.repository_version,
          source: submission.intent.source.clone(),
          mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
        },
      },
      idempotency_key: key("execute"),
      requested_at: at(10),
    }))
    .await
    .unwrap()
    .request
}
fn publisher(
  store: Arc<PostgresStore>,
  bytes: Arc<Bytes>,
  resolver: Arc<Resolver>,
  clock: Arc<Clock>,
) -> AuthorizedCommandHandler<IssueExecutionPublisher<PostgresStore, PostgresStore, Bytes>> {
  AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueExecutionPublisher::new(
      store.clone(),
      store,
      bytes,
      resolver,
      Arc::new(TrustedNetworkManagementPolicy),
      clock,
    )),
  )
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_publication_associates_only_original_work_and_revision_after_restart() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup(&store).await;
  let request = reviewed_request(&store, &issue, &submission).await;
  store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          description: "new requirements".into(),
          acceptance: Some("new acceptance".into()),
          ..issue.content.clone()
        },
      },
      idempotency_key: key("edit"),
      edited_at: at(11),
    }))
    .await
    .unwrap();
  let bytes = Arc::new(Bytes::new());
  let resolver = Arc::new(Resolver(AtomicUsize::new(0)));
  let clock = Arc::new(Clock(AtomicI64::new(20)));
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification,
  };
  let first = publisher(store.clone(), bytes.clone(), resolver.clone(), clock.clone())
    .handle_command(&context(), command.clone())
    .await
    .unwrap();
  let association = first.association.clone().unwrap();
  assert_eq!(association.requirements_version, Some(issue.version));
  assert_eq!(
    store
      .issue_work_association(issue.project_id, association.work_id)
      .await
      .unwrap(),
    Some(association.clone())
  );
  assert!(
    store
      .issue_work_association(ProjectId::generate(), association.work_id)
      .await
      .unwrap()
      .is_none()
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let replay = publisher(restarted.clone(), bytes.clone(), resolver.clone(), clock)
    .handle_command(&context(), command)
    .await
    .unwrap();
  assert_eq!(replay.association, Some(association));
  assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
  assert_eq!(
    restarted
      .issue_execution(issue.project_id, issue.id, request.id)
      .await
      .unwrap(),
    first
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "subprocess fixture; invoked only by the crash recovery contract"]
async fn crash_child() {
  let Ok(path) = std::env::var("OCTACITY_WORK_CARD_CRASH") else {
    return;
  };
  let spec: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
  let pool = sqlx::PgPool::connect(spec["database"].as_str().unwrap()).await.unwrap();
  let store = Arc::new(PostgresStore::new(pool));
  let submission: ProjectTextSubmission = serde_json::from_value(spec["submission"].clone()).unwrap();
  let bytes = Arc::new(Bytes::shared(
    spec["root"].as_str().unwrap().into(),
    spec["pause_after"].as_u64().unwrap() as usize,
  ));
  let command = PublishIssueExecutionCommand {
    project_id: submission.intent.target.project_id,
    issue_id: serde_json::from_value(spec["issue_id"].clone()).unwrap(),
    request_id: serde_json::from_value(spec["request_id"].clone()).unwrap(),
    classification: submission.intent.classification,
  };
  publisher(
    store,
    bytes,
    Arc::new(Resolver(AtomicUsize::new(0))),
    Arc::new(Clock(AtomicI64::new(20))),
  )
  .handle_command(&context(), command)
  .await
  .unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn killed_publishers_resume_at_byte_publication_admission_and_association_boundaries() {
  use std::{
    process::{Command, Stdio},
    time::Duration,
  };
  struct Child(std::process::Child);
  impl Drop for Child {
    fn drop(&mut self) {
      let _ = self.0.kill();
      let _ = self.0.wait();
    }
  }
  for stage in ["first-byte", "both-bytes", "published", "admitted", "associated"] {
    let database = postgres::TestDatabase::migrated().await;
    let store = Arc::new(PostgresStore::new(database.pool.clone()));
    let (issue, submission) = setup(&store).await;
    let request = reviewed_request(&store, &issue, &submission).await;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let url = std::env::var("OCTACITY_POSTGRES_URL").unwrap();
    let (base, _) = url.rsplit_once('/').unwrap();
    let url = format!("{base}/{}", database.name);
    let spec = root.join("child.json");
    std::fs::write(&spec,serde_json::to_vec(&serde_json::json!({"database":url,"submission":submission,"issue_id":issue.id,"request_id":request.id,"root":root,"pause_after":match stage {"first-byte"=>1,"both-bytes"=>2,_=>0}})).unwrap()).unwrap();
    let mut barrier = database.pool.begin().await.unwrap();
    match stage {
      "published" => {
        sqlx::query("LOCK TABLE factory_work_envelopes IN SHARE MODE")
          .execute(&mut *barrier)
          .await
          .unwrap();
      }
      "admitted" => {
        sqlx::query("LOCK TABLE issue_work_associations IN SHARE MODE")
          .execute(&mut *barrier)
          .await
          .unwrap();
      }
      _ => {}
    }
    let mut child = Child(
      Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--ignored", "--nocapture"])
        .env("OCTACITY_WORK_CARD_CRASH", &spec)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap(),
    );
    let retained = tokio::time::timeout(Duration::from_secs(15), async {
      loop {
        if let Some(record) = store
          .project_text_preparation(
            issue.project_id,
            &ManagementSecurityScope::trusted_network(),
            &request.admission_key(),
          )
          .await
          .unwrap()
        {
          let ready = match stage {
            "first-byte" | "both-bytes" => root.join("paused").exists(),
            "published" => record.published_at.is_some(),
            "admitted" => record.admission.is_some(),
            "associated" => store
              .issue_execution(issue.project_id, issue.id, request.id)
              .await
              .unwrap()
              .association
              .is_some(),
            _ => false,
          };
          if ready {
            break record;
          }
        }
        assert!(
          child.0.try_wait().unwrap().is_none(),
          "child exited before crash boundary {stage}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
      }
    })
    .await
    .expect("child reaches retained crash boundary");
    if stage != "associated" {
      child.0.kill().unwrap();
    }
    child.0.wait().unwrap();
    barrier.rollback().await.unwrap();
    let pending = store
      .issue_execution(issue.project_id, issue.id, request.id)
      .await
      .unwrap();
    if stage != "associated" {
      assert!(
        pending.association.is_none(),
        "no admitted Issue receipt before association commits"
      );
    }
    store
      .edit_issue(management_mutation(EditIssue {
        intent: EditIssueIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          content: IssueContent {
            description: "edited after process death".into(),
            ..issue.content.clone()
          },
        },
        idempotency_key: key("edit-after-death"),
        edited_at: at(30_021),
      }))
      .await
      .unwrap();
    let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
    let bytes = Arc::new(Bytes::shared(root.into(), 0));
    let resumed = publisher(
      restarted.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(1))),
      Arc::new(Clock(AtomicI64::new(30_020))),
    )
    .handle_command(
      &context(),
      PublishIssueExecutionCommand {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: request.id,
        classification: submission.intent.classification,
      },
    )
    .await
    .unwrap();
    let associated = resumed.association.unwrap();
    assert_eq!(associated.requirements_version, Some(issue.version));
    assert_eq!(bytes.writes.load(Ordering::SeqCst), usize::from(stage == "first-byte"));
    let original = restarted
      .project_text_preparation(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(original.objects, retained.objects);
    if let Some(admission) = retained.admission {
      assert_eq!(associated.work_id, admission.work.id());
      assert_eq!(associated.run_id, admission.run.id());
    }
    let runs = restarted
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
    assert_eq!(runs.items.len(), 1, "{stage}: one original Run survives process death");
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn published_project_inputs_use_existing_protected_transfer_without_build_provenance() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, mut submission) = setup(&store).await;
  submission.documents = ProjectTextDocuments::try_new("hello", "acceptance").unwrap();
  let bytes = Arc::new(Bytes::new());
  let intake = ProjectTextIntake::new(
    store.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(0))),
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(Clock(AtomicI64::new(20))),
  );
  let admission = intake.submit(&context(), submission).await.unwrap().admission;
  let artifact = admission.work.artifacts().task();
  let published = store.published_execution_input(artifact.artifact_id()).await.unwrap();
  assert_eq!(published.size_bytes, 5);
  assert_eq!(
    published.digest.to_string(),
    "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
  );
  assert!(store.published_artifact(artifact.artifact_id()).await.is_err());
  let reader = FactoryArtifactReader::new(
    store.clone(),
    Arc::new(PostgresAuthoritativeStore::new(
      database.pool.clone(),
      postgres::test_signer(),
    )),
    bytes,
  )
  .with_project_inputs(store.clone());
  let selected = ReadFactoryArtifact {
    project_id: issue.project_id,
    artifact: artifact.clone(),
    maximum_bytes: 100,
    observed_at: at(21),
    required_until: at(50),
  };
  assert_eq!(reader.read_input(selected.clone()).await.unwrap().bytes(), b"hello");
  assert!(reader.read(selected).await.is_err());
  let input = octacity_protocol::ProtectedInputV3 {
    artifact_id: artifact.artifact_id().to_string(),
    sha256: published.digest.to_string(),
    size_bytes: 5,
    media_type: "text/plain".into(),
    destination: format!("{}/task.txt", octacity_protocol::PROTECTED_INPUT_ROOT),
  };
  let manifest = octacity_protocol::ProtectedInputManifestV3 {
    inputs: vec![input.clone()],
  };
  let expires = published.expires_at.unwrap();
  let transfers = protected_input_transfer::service(store.clone())
    .authorize_protected_inputs(AuthorizeProtectedInputsInput {
      manifest: manifest.clone(),
      observed_at_unix_ms: expires.unix_millis() - 10_000,
      lease_expires_at: at(expires.unix_millis() + 50_000),
    })
    .await
    .unwrap();
  assert_eq!(transfers[0].input, input);
  assert_eq!(transfers[0].capability.expires_at_unix_ms, expires.unix_millis() as u64);
  assert!(
    protected_input_transfer::service(store)
      .authorize_protected_inputs(AuthorizeProtectedInputsInput {
        manifest,
        observed_at_unix_ms: expires.unix_millis(),
        lease_expires_at: at(expires.unix_millis() + 50_000)
      })
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and object storage"]
async fn native_resources_publish_and_read_project_inputs_through_the_shared_mechanism() {
  use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
  use std::time::Duration;
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup(&store).await;
  let objects = Arc::new(
    S3ArtifactStore::new(S3ArtifactStoreConfig {
      endpoint: std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
      region: "us-east-1".into(),
      bucket: "octacity-artifacts".into(),
      prefix: format!("work-card-{}", issue.id),
      access_key: zeroize::Zeroizing::new(std::env::var("OCTACITY_MINIO_ACCESS_KEY").unwrap()),
      secret_key: zeroize::Zeroizing::new(std::env::var("OCTACITY_MINIO_SECRET_KEY").unwrap()),
      force_path_style: true,
      operation_timeout: Duration::from_secs(5),
      capability_recheck_interval: Duration::from_secs(1),
    })
    .unwrap(),
  );
  let spec: octacity_protocol::JobSpecV3 = serde_json::from_str(include_str!(
    "../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  ))
  .unwrap();
  let resources = octacity_server::FactoryRuntimeResources {
    runs: store.clone(),
    builds: Arc::new(PostgresAuthoritativeStore::new(
      database.pool.clone(),
      postgres::test_signer(),
    )),
    objects: objects.clone(),
    revisions: Arc::new(Resolver(AtomicUsize::new(0))),
    toolchain: JobSpecToolchainPolicy {
      source: octacity_server_job::SourcePluginPolicy::new("git", "0.1.0", spec.source.plugin_sha256, "url").unwrap(),
      octa: spec.octa,
      validity: octacity_server_job::JobSpecValidity::new(3600).unwrap(),
    },
  };
  let admission = resources
    .project_text_intake(Arc::new(TrustedNetworkManagementPolicy))
    .submit(&context(), submission.clone())
    .await
    .unwrap()
    .admission;
  let now = SystemFactoryExecutionClock.now().unwrap();
  for (artifact, expected) in [
    (admission.work.artifacts().task(), submission.documents.task()),
    (
      admission.work.artifacts().acceptance(),
      submission.documents.acceptance(),
    ),
  ] {
    let selected = ReadFactoryArtifact {
      project_id: issue.project_id,
      artifact: artifact.clone(),
      maximum_bytes: 100,
      observed_at: now,
      required_until: at(now.unix_millis() + 1000),
    };
    assert_eq!(
      resources
        .retained_artifact_reader()
        .read_input(selected.clone())
        .await
        .unwrap()
        .bytes(),
      expected
    );
    assert!(resources.retained_artifact_reader().read(selected).await.is_err());
  }
  // Exercise the real presigned transfer using the same immutable byte identity.
  use octacity_artifact_store::ArtifactStore as _;
  let task = store
    .published_execution_input(admission.work.artifacts().task().artifact_id())
    .await
    .unwrap();
  let object = ArtifactObject::new(
    task.artifact_id,
    task.upload_id,
    task.size_bytes,
    task.digest.to_string(),
    task.media_type.as_str(),
  )
  .unwrap();
  let grant = objects
    .authorize_download(&object, Duration::from_secs(10))
    .await
    .unwrap();
  let observed = reqwest::get(&grant.url)
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .bytes()
    .await
    .unwrap();
  assert_eq!(observed.as_ref(), submission.documents.task());
  database.cleanup().await;
}

#[async_trait::async_trait]
impl octacity_artifact_store::ArtifactStore for Bytes {
  async fn authorize_upload(
    &self,
    _: &ArtifactObject,
    _: std::time::Duration,
  ) -> Result<octacity_artifact_store::UploadAuthorization, ArtifactStoreError> {
    unreachable!("Project input publication does not use Agent uploads")
  }
  async fn complete_upload(&self, _: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    unreachable!()
  }
  async fn verify_published(&self, o: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self.read_published(o, o.size_bytes()).await.map(|_| ())
  }
  async fn authorize_download(
    &self,
    _: &ArtifactObject,
    expires_in: std::time::Duration,
  ) -> Result<octacity_artifact_store::DownloadAuthorization, ArtifactStoreError> {
    Ok(octacity_artifact_store::DownloadAuthorization {
      url: "https://objects.example.test/input".into(),
      expires_in,
    })
  }
  async fn delete(&self, o: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    if self.fail_delete.swap(false, Ordering::SeqCst) {
      return Err(ArtifactStoreError::Backend {
        operation: ArtifactStoreOperation::Delete,
      });
    }
    match std::fs::remove_file(self.root.join(o.upload_id().to_string())) {
      Ok(()) => Ok(()),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(_) => Err(ArtifactStoreError::Backend {
        operation: ArtifactStoreOperation::Delete,
      }),
    }
  }
}
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/authoritative_fixture.rs"]
mod authoritative_fixture;
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn shared_input_retention_cleans_abandoned_bytes_honors_build_holds_and_prevents_resurrection() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup(&store).await;
  let bytes = Arc::new(Bytes::new());
  let admitted = ProjectTextIntake::new(
    store.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(0))),
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(Clock(AtomicI64::new(20))),
  )
  .submit(&context(), submission.clone())
  .await
  .unwrap()
  .admission;
  let preparation = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &key("text"),
    )
    .await
    .unwrap()
    .unwrap();
  let expires = at(preparation.expires_at.unix_millis() + 1);
  let mut abandoned = submission.clone();
  abandoned.intent.idempotency_key = key("abandoned");
  abandoned.intent.external_identity = ExternalWorkIdentity::new("abandoned").unwrap();
  let abandoned = store
    .reserve_project_text(management_mutation(ReserveProjectText {
      submission: abandoned,
      now: at(21),
    }))
    .await
    .unwrap();
  assert!(
    store
      .expired_factory_execution_inputs(at(preparation.expires_at.unix_millis() - 1), 10)
      .await
      .unwrap()
      .is_empty()
  );
  // A genuinely accepted ordinary Build is linked by its immutable Factory origin,
  // including the lost-acknowledgement case before execution receipt publication.
  let fixture = octacity_server_store::testing::authoritative_store_contract_fixture();
  authoritative_fixture::seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), postgres::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();
  sqlx::query("UPDATE builds SET trigger_occurrence_id=NULL,factory_origin=$2,factory_materialization_digest=decode(repeat('41',32),'hex'),factory_permission_digest=decode(repeat('42',32),'hex') WHERE id=$1")
 .bind(fixture.request.build.id.as_uuid()).bind(sqlx::types::Json(serde_json::json!({"operation_id":"33".repeat(32),"run_id":admitted.run.id(),"node_attempt_id":uuid::Uuid::new_v4().to_string(),"input_digest":"32".repeat(32)}))).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO build_result_retention_holds(build_id,version,reason,created_at,actor_kind,request_identity) VALUES($1,1,'Investigation',to_timestamp(0),'system','hold')").bind(fixture.request.build.id.as_uuid()).execute(&database.pool).await.unwrap();
  assert_eq!(
    store.expired_factory_execution_inputs(expires, 1).await.unwrap().len(),
    1,
    "a held oldest preparation must not starve a bounded cleanup page"
  );
  let expired = store.expired_factory_execution_inputs(expires, 10).await.unwrap();
  assert_eq!(
    expired.len(),
    2,
    "only abandoned inputs can be cleaned while the Run has a held Build"
  );
  assert!(expired.iter().all(|e| {
    abandoned
      .objects
      .iter()
      .any(|o| o.artifact.artifact_id() == e.artifact_id)
  }));
  sqlx::query("UPDATE build_result_retention_holds SET released_at=to_timestamp($2::double precision/1000.0),release_actor_kind='system',release_request_identity='release' WHERE build_id=$1").bind(fixture.request.build.id.as_uuid()).bind(expires.unix_millis()).execute(&database.pool).await.unwrap();
  let expired = store.expired_factory_execution_inputs(expires, 10).await.unwrap();
  assert_eq!(expired.len(), 4);
  assert!(
    matches!(
      store
        .place_build_result_hold(management_mutation(PlaceBuildResultHold {
          build_id: fixture.request.build.id,
          reason: RetentionHoldReason::new("Late investigation").unwrap(),
          expires_at: None,
          idempotency_key: key("late-hold"),
          placed_at: at(expires.unix_millis() + 1)
        }))
        .await,
      Err(StoreError::Conflict {
        entity: EntityKind::RetentionHold
      })
    ),
    "a hold cannot claim inputs after irreversible deletion was reserved"
  );
  bytes.fail_delete.store(true, Ordering::SeqCst);
  let worker = FactoryExecutionInputRetentionWorker::new(store.clone(), bytes.clone());
  assert!(worker.run_once(expires, 10).await.is_err());
  assert_eq!(
    store.expired_factory_execution_inputs(expires, 10).await.unwrap().len(),
    4
  );
  assert_eq!(worker.run_once(expires, 10).await.unwrap(), 4);
  assert!(
    store
      .project_input_artifact(issue.project_id, admitted.work.artifacts().task().artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .published_execution_input(admitted.work.artifacts().task().artifact_id())
      .await
      .is_err()
  );
  let scrubbed = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &key("text"),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(
    store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &scrubbed.intent.idempotency_key
      )
      .await
      .unwrap()
      .is_none()
  );
  let resumed = ProjectTextIntake::new(
    store.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(Clock(AtomicI64::new(expires.unix_millis()))),
  );
  let replay = resumed.submit(&context(), submission).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(
    replay.admission, admitted,
    "cleanup preserves original metadata receipts"
  );
  assert!(
    store
      .project_input_artifact(issue.project_id, admitted.work.artifacts().task().artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  // Delayed I/O can leave an orphan after cleanup; retained identities schedule rechecks.
  let object = &preparation.objects[0];
  std::fs::write(bytes.root.join(object.upload_id.to_string()), b"late bytes").unwrap();
  assert_eq!(
    worker
      .run_once(at(expires.unix_millis() + 3_600_000), 10)
      .await
      .unwrap(),
    4
  );
  assert!(!bytes.root.join(object.upload_id.to_string()).exists());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn transaction_failures_never_expose_partial_publication_admission_or_issue_association() {
  for stage in ["reservation", "publication", "admission", "association"] {
    let database = postgres::TestDatabase::migrated().await;
    let store = Arc::new(PostgresStore::new(database.pool.clone()));
    let (issue, submission) = setup(&store).await;
    let request = reviewed_request(&store, &issue, &submission).await;
    let (table, event, predicate) = match stage {
      "reservation" => ("audit_facts", "INSERT", "NEW.operation='reserve-project-text-input'"),
      "publication" => (
        "project_text_preparations",
        "UPDATE",
        "OLD.document->>'published_at' IS NULL AND NEW.document->>'published_at' IS NOT NULL",
      ),
      "admission" => ("factory_work_envelopes", "INSERT", "true"),
      _ => ("audit_facts", "INSERT", "NEW.operation='associate-issue-work'"),
    };
    let sql = format!(
      "CREATE FUNCTION injected_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF {predicate} THEN RAISE EXCEPTION 'injected crash before commit'; END IF; RETURN NEW; END $$; CREATE TRIGGER injected_failure BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION injected_failure()"
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
      .execute(&database.pool)
      .await
      .unwrap();
    let bytes = Arc::new(Bytes::new());
    let resolver = Arc::new(Resolver(AtomicUsize::new(0)));
    let clock = Arc::new(Clock(AtomicI64::new(20)));
    let command = PublishIssueExecutionCommand {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: request.id,
      classification: submission.intent.classification,
    };
    assert!(
      publisher(store.clone(), bytes.clone(), resolver.clone(), clock.clone())
        .handle_command(&context(), command.clone())
        .await
        .is_err()
    );
    let original = store
      .project_text_preparation(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &request.admission_key(),
      )
      .await
      .unwrap();
    assert!(
      store
        .issue_execution(issue.project_id, issue.id, request.id)
        .await
        .unwrap()
        .association
        .is_none()
    );
    if stage == "reservation" {
      assert!(original.is_none());
      assert_eq!(bytes.writes.load(Ordering::SeqCst), 0);
    } else {
      let p = original.as_ref().unwrap();
      assert_eq!(p.published_at.is_some(), stage != "publication");
      assert_eq!(p.admission.is_some(), stage == "association");
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
      "DROP TRIGGER injected_failure ON {table}; DROP FUNCTION injected_failure()"
    )))
    .execute(&database.pool)
    .await
    .unwrap();
    let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
    let recovered = publisher(restarted.clone(), bytes.clone(), resolver.clone(), clock)
      .handle_command(&context(), command)
      .await
      .unwrap();
    let associated = recovered.association.unwrap();
    if let Some(p) = original
      && let Some(a) = p.admission
    {
      assert_eq!(associated.work_id, a.work.id());
      assert_eq!(associated.run_id, a.run.id());
    }
    assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
    assert_eq!(resolver.0.load(Ordering::SeqCst), 1);
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn completed_issue_replay_survives_private_cleanup_without_restoring_bodies() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup(&store).await;
  let request = reviewed_request(&store, &issue, &submission).await;
  let bytes = Arc::new(Bytes::new());
  let clock = Arc::new(Clock(AtomicI64::new(20)));
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification,
  };
  let first = publisher(
    store.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(0))),
    clock.clone(),
  )
  .handle_command(&context(), command.clone())
  .await
  .unwrap();
  let original = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  clock.0.store(original.expires_at.unix_millis(), Ordering::SeqCst);
  assert_eq!(
    FactoryExecutionInputRetentionWorker::new(store.clone(), bytes.clone())
      .run_once(original.expires_at, 10)
      .await
      .unwrap(),
    2
  );
  let writes = bytes.writes.load(Ordering::SeqCst);
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let handler = publisher(
    restarted.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    clock,
  );
  assert_eq!(
    handler.handle_command(&context(), command.clone()).await.unwrap(),
    first
  );
  let mut changed = command;
  changed.classification = WorkClassification::new(WorkPriority::new(2).unwrap(), RiskClass::Low, Default::default());
  assert!(handler.handle_command(&context(), changed).await.is_err());
  assert_eq!(bytes.writes.load(Ordering::SeqCst), writes);
  let retained = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(
    store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &retained.intent.idempotency_key
      )
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    restarted
      .project_input_artifact(issue.project_id, original.objects[0].artifact.artifact_id())
      .await
      .unwrap()
      .is_none()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn admitted_pending_issue_association_recovers_after_private_body_cleanup() {
  let database = postgres::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (issue, submission) = setup(&store).await;
  let request = reviewed_request(&store, &issue, &submission).await;
  sqlx::raw_sql("CREATE FUNCTION fail_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected association failure'; END $$; CREATE TRIGGER fail_link BEFORE INSERT ON issue_work_associations FOR EACH ROW EXECUTE FUNCTION fail_link()")
   .execute(&database.pool).await.unwrap();
  let bytes = Arc::new(Bytes::new());
  let clock = Arc::new(Clock(AtomicI64::new(20)));
  let command = PublishIssueExecutionCommand {
    project_id: issue.project_id,
    issue_id: issue.id,
    request_id: request.id,
    classification: submission.intent.classification,
  };
  assert!(
    publisher(
      store.clone(),
      bytes.clone(),
      Arc::new(Resolver(AtomicUsize::new(0))),
      clock.clone()
    )
    .handle_command(&context(), command.clone())
    .await
    .is_err()
  );
  let original = store
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let admission = original.admission.clone().unwrap();
  assert!(
    store
      .issue_execution(issue.project_id, issue.id, request.id)
      .await
      .unwrap()
      .association
      .is_none()
  );
  sqlx::raw_sql("DROP TRIGGER fail_link ON issue_work_associations; DROP FUNCTION fail_link()")
    .execute(&database.pool)
    .await
    .unwrap();
  clock.0.store(original.expires_at.unix_millis(), Ordering::SeqCst);
  assert_eq!(
    FactoryExecutionInputRetentionWorker::new(store.clone(), bytes.clone())
      .run_once(original.expires_at, 10)
      .await
      .unwrap(),
    2
  );
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let linked = publisher(
    restarted.clone(),
    bytes.clone(),
    Arc::new(Resolver(AtomicUsize::new(1))),
    clock,
  )
  .handle_command(&context(), command)
  .await
  .unwrap()
  .association
  .unwrap();
  assert_eq!(linked.work_id, admission.work.id());
  assert_eq!(linked.run_id, admission.run.id());
  assert_eq!(linked.requirements_version, Some(issue.version));
  assert_eq!(bytes.writes.load(Ordering::SeqCst), 2);
  let retained = restarted
    .project_text_preparation(
      issue.project_id,
      &ManagementSecurityScope::trusted_network(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(
    store
      .project_text_documents(
        issue.project_id,
        &ManagementSecurityScope::trusted_network(),
        &retained.intent.idempotency_key
      )
      .await
      .unwrap()
      .is_none()
  );
  database.cleanup().await;
}

#[path = "support/console_pilot.rs"]
mod console_pilot;
#[path = "support/runtime.rs"]
mod http_runtime;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and MinIO"]
async fn issue_execution_http_resumes_original_inputs_after_runtime_restart() {
  use octacity_server::ServerRuntime;
  use serde_json::{Value, json};
  let database = postgres::TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, submission) = setup_with_flow(&store, Some(pilot_controls::flow())).await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let runtime = ServerRuntime::start(http_runtime::runtime_config(
    root.path(),
    url.as_str(),
    "http://127.0.0.1:9",
  ))
  .await
  .unwrap();
  let client = reqwest::Client::new();
  let path = format!("/api/v1/projects/{}/issues/{}/executions", issue.project_id, issue.id);
  let origin = format!("http://{}", runtime.management_addr());
  let history = client.get(format!("{origin}{path}")).send().await.unwrap();
  assert_eq!(history.status(), reqwest::StatusCode::OK);
  assert_eq!(history.json::<Value>().await.unwrap()["items"], json!([]));
  let body = json!({"target":{
    "configuration_id":submission.intent.target.configuration_id,"configuration_version":1,
    "repository_id":submission.intent.target.repository_id,"repository_version":1,
    "source":submission.intent.source,"mapping":"title_description_acceptance_v1"},
    "classification":submission.intent.classification});
  let post = |origin: &str, replay: &str, version: &str| {
    client
      .post(format!("{origin}{path}"))
      .header("idempotency-key", replay)
      .header("if-match", version)
      .json(&body)
  };
  let response = post(&origin, "http-reviewed", "\"1\"").send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
  let location = response.headers()["location"].to_str().unwrap().to_owned();
  let pending: Value = response.json().await.unwrap();
  assert_eq!(pending["resource"]["state"], "pending");
  assert_eq!(pending["resource"]["association"], Value::Null);
  runtime.shutdown().await.unwrap();
  store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          description: "Later requirements".into(),
          ..issue.content.clone()
        },
      },
      idempotency_key: key("http-edit"),
      edited_at: at(11),
    }))
    .await
    .unwrap();
  let endpoint = std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap();
  let runtime = ServerRuntime::start(http_runtime::runtime_config(root.path(), url.as_str(), &endpoint))
    .await
    .unwrap();
  let origin = format!("http://{}", runtime.management_addr());
  let status = client.get(format!("{origin}{location}")).send().await.unwrap();
  assert_eq!(status.status(), reqwest::StatusCode::OK);
  assert_eq!(status.json::<Value>().await.unwrap()["state"], "pending");
  let response = post(&origin, "http-reviewed", "\"1\"").send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["location"], location);
  let admitted: Value = response.json().await.unwrap();
  assert_eq!(admitted["disposition"], "replayed");
  assert_eq!(admitted["resource"]["state"], "admitted");
  assert_eq!(admitted["resource"]["id"], pending["resource"]["id"]);
  assert_eq!(admitted["resource"]["intent"]["expected_version"], 1);
  assert_eq!(admitted["resource"]["task_digest"], pending["resource"]["task_digest"]);
  let response = post(&origin, "http-reviewed", "\"1\"").send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.json::<Value>().await.unwrap(), admitted);
  assert_eq!(
    post(&origin, "http-new", "\"1\"").send().await.unwrap().status(),
    reqwest::StatusCode::CONFLICT
  );
  let second = post(&origin, "http-new", "\"2\"").send().await.unwrap();
  assert_eq!(second.status(), reqwest::StatusCode::CREATED);
  let second: Value = second.json().await.unwrap();
  assert_ne!(
    second["resource"]["association"]["run_id"],
    admitted["resource"]["association"]["run_id"]
  );
  assert_ne!(second["resource"]["task_digest"], admitted["resource"]["task_digest"]);
  let run_url = admitted["resource"]["run_url"].as_str().unwrap();
  assert_eq!(
    client.get(format!("{origin}{run_url}")).send().await.unwrap().status(),
    reqwest::StatusCode::OK
  );
  let history = client.get(format!("{origin}{path}?limit=200")).send().await.unwrap();
  assert_eq!(history.status(), reqwest::StatusCode::OK);
  assert_eq!(
    history.json::<Value>().await.unwrap()["items"]
      .as_array()
      .unwrap()
      .len(),
    2
  );
  let count: i64 = sqlx::query_scalar("SELECT count(*) FROM factory_runs WHERE project_id=$1")
    .bind(issue.project_id.as_uuid())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(count, 2);
  let original_run = admitted["resource"]["association"]["run_id"]
    .as_str()
    .unwrap()
    .parse()
    .unwrap();
  let active_store = Arc::new(PostgresStore::new(database.pool.clone()));
  pilot_controls::begin(active_store.clone(), original_run).await;
  let edited = client.patch(format!("{origin}/api/v1/projects/{}/issues/{}", issue.project_id, issue.id)).header("idempotency-key", "http-running-edit").header("if-match", "\"2\"").json(&json!({"title":issue.content.title,"description":"Requirements edited during Run","acceptance":issue.content.acceptance})).send().await.unwrap();
  assert_eq!(edited.status(), reqwest::StatusCode::OK);
  pilot_controls::qualify(active_store, &context(), original_run).await;
  assert_eq!(
    store.issue(issue.project_id, issue.id, None).await.unwrap().status_id,
    issue.status_id
  );
  if std::env::var_os("OCTACITY_PILOT_BROWSER").is_some() {
    let result = console_pilot::run(
      &origin,
      Some(&format!("/projects/{}/issues/{}", issue.project_id, issue.id)),
    )
    .await;
    assert!(
      result.status.success(),
      "{}\n{}",
      String::from_utf8_lossy(&result.stdout),
      String::from_utf8_lossy(&result.stderr)
    );
  }
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  let status_submission = status_actions::configure_status(&store, &current, submission.clone()).await;
  let mut explicit = body.clone();
  explicit["target"]["configuration_version"] =
    serde_json::json!(status_submission.intent.target.configuration_version.get());
  let status = client
    .post(format!("{origin}{path}"))
    .header("idempotency-key", "explicit-status-pilot")
    .header("if-match", format!("\"{}\"", current.version.get()))
    .json(&explicit)
    .send()
    .await
    .unwrap();
  assert_eq!(status.status(), reqwest::StatusCode::CREATED);
  let status: Value = status.json().await.unwrap();
  status_actions::qualify_status(
    Arc::new(PostgresStore::new(database.pool.clone())),
    status["resource"]["association"]["run_id"]
      .as_str()
      .unwrap()
      .parse()
      .unwrap(),
    &current,
  )
  .await;
  // The common cleanup owner removes the very inputs just previewed, retaining the original association.
  let work: WorkEnvelopeId = admitted["resource"]["association"]["work_id"]
    .as_str()
    .unwrap()
    .parse()
    .unwrap();
  let task = store
    .factory_run_snapshot(
      admitted["resource"]["association"]["run_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap(),
    )
    .await
    .unwrap()
    .work
    .artifacts()
    .task()
    .artifact_id();
  let input = store.published_execution_input(task).await.unwrap();
  let objects = Arc::new(
    octacity_artifact_s3::S3ArtifactStore::new(octacity_artifact_s3::S3ArtifactStoreConfig {
      endpoint,
      region: "us-east-1".into(),
      bucket: "octacity-artifacts".into(),
      prefix: String::new(),
      access_key: zeroize::Zeroizing::new("octacity".into()),
      secret_key: zeroize::Zeroizing::new("octacity-secret".into()),
      force_path_style: true,
      operation_timeout: std::time::Duration::from_secs(5),
      capability_recheck_interval: std::time::Duration::from_secs(1),
    })
    .unwrap(),
  );
  let cleanup = FactoryExecutionInputRetentionWorker::new(Arc::new(PostgresStore::new(database.pool.clone())), objects);
  assert!(
    cleanup
      .run_once(at(input.expires_at.unwrap().unix_millis() + 1), 100)
      .await
      .unwrap()
      >= 2
  );
  let artifact_path = format!(
    "{origin}/api/v1/projects/{}/issues/{}/results/artifacts",
    issue.project_id, issue.id
  );
  let expired: Value = client
    .get(format!("{artifact_path}?work_id={work}"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert!(
    expired["items"]
      .as_array()
      .unwrap()
      .iter()
      .all(|item| item["availability"] != "available")
  );
  assert!(
    !client
      .get(format!("{artifact_path}/{task}/content?work_id={work}&mode=preview"))
      .send()
      .await
      .unwrap()
      .status()
      .is_success()
  );
  if std::env::var_os("OCTACITY_PILOT_BROWSER").is_some() {
    let result = console_pilot::run_named(
      &origin,
      Some(&format!("/projects/{}/issues/{}", issue.project_id, issue.id)),
      "expired execution",
    )
    .await;
    assert!(
      result.status.success(),
      "{}\n{}",
      String::from_utf8_lossy(&result.stdout),
      String::from_utf8_lossy(&result.stderr)
    );
  }
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn runtime_backfills_only_upgrade_work_without_factory_or_provider() {
  let database = postgres::TestDatabase::before_issue_backfill().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let (draft, submission) = setup(&store).await;
  let intake = ProjectTextIntake::new(
    store.clone(),
    Arc::new(Bytes::new()),
    Arc::new(Resolver(AtomicUsize::new(0))),
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(Clock(AtomicI64::new(20))),
  );
  let original = intake.submit(&context(), submission.clone()).await.unwrap().admission;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let runtime = octacity_server::ServerRuntime::start(http_runtime::runtime_config(
    root.path(),
    url.as_str(),
    "http://127.0.0.1:9",
  ))
  .await
  .unwrap();
  let association = tokio::time::timeout(std::time::Duration::from_secs(10), async {
    loop {
      if let Some(link) = store
        .issue_work_association(draft.project_id, original.work.id())
        .await
        .unwrap()
      {
        break link;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await
  .expect("legacy worker must run even when Factory is disabled");
  let response = reqwest::get(format!(
    "http://{}/api/v1/projects/{}/issues/{}",
    runtime.management_addr(),
    draft.project_id,
    association.issue_id
  ))
  .await
  .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let path = format!(
    "http://{}/api/v1/projects/{}/issues/{}",
    runtime.management_addr(),
    draft.project_id,
    association.issue_id
  );
  let legacy = reqwest::get(format!("{path}/legacy-work")).await.unwrap();
  assert_eq!(legacy.status(), reqwest::StatusCode::OK);
  let legacy: serde_json::Value = legacy.json().await.unwrap();
  assert_eq!(legacy["items"][0]["work_id"], original.work.id().to_string());
  assert!(legacy["items"][0]["request_id"].is_null());
  let exact = reqwest::get(format!("{path}/work/{}", original.work.id()))
    .await
    .unwrap();
  assert_eq!(exact.status(), reqwest::StatusCode::OK);
  assert_eq!(exact.headers()["cache-control"], "no-store");
  let exact: serde_json::Value = exact.json().await.unwrap();
  assert_eq!(exact["association"]["run_id"], original.run.id().to_string());
  let run = reqwest::get(format!(
    "http://{}/api/v1/factory-runs/{}",
    runtime.management_addr(),
    original.run.id()
  ))
  .await
  .unwrap();
  assert_eq!(
    run.headers().get("cache-control").and_then(|v| v.to_str().ok()),
    Some("no-store")
  );
  let run: serde_json::Value = run.json().await.unwrap();
  assert_eq!(run["issue_association"]["issue_id"], association.issue_id.to_string());

  assert_eq!(
    store.factory_run_snapshot(original.run.id()).await.unwrap().work,
    original.work
  );
  assert_eq!(association.run_id, original.run.id());
  assert_eq!(association.requirements_version, None);
  assert_eq!(intake.submit(&context(), submission).await.unwrap().admission, original);
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

#[path = "support/pilot_controls.rs"]
mod pilot_controls;
