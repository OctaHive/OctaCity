//! Reviewed completion documents through the installed Source worker and ordinary admission.
use super::*;
const CRITERIA: &str = "Assess fit and priority; retain evidence or explicitly request more input.\r\n";
#[tokio::test]
async fn installed_completion_document_prepares_and_admits_a_title_only_ticket() {
  completion_contract(Fixture::enabled("github").await).await;
}
use crate::runtime::postgres_test_support as database;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn installed_completion_document_has_postgres_publication_and_admission_parity() {
  let database = database::TestDatabase::migrated().await;
  let (fixture, artifacts) = completion_contract(
    Fixture::enabled("github")
      .await
      .into_postgres(database.pool.clone())
      .await,
  )
  .await;
  signed_input_transfer(&fixture, &artifacts).await;
  database.cleanup().await;
}
async fn install_completion<S>(fixture: &mut Fixture<S>) -> FactoryRunId
where
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + ProjectTextInputStore + 'static,
{
  let project = fixture.input.project_id();
  let document = ProjectTextIntake::new(
    fixture.store.clone(),
    fixture.bytes.clone(),
    Arc::new(ExactRevisionResolver),
    Arc::new(TrustedNetworkManagementPolicy),
    fixture.clock.clone(),
  )
  .submit(
    &fixture.profile.read.context,
    ProjectTextSubmission {
      intent: ProjectTextIntent {
        target: ReadFactoryAdmissionContext {
          project_id: project,
          configuration_id: fixture.input.definition().target.id(),
          configuration_version: fixture.input.definition().target.version(),
          repository_id: fixture.input.definition().repository.id,
          repository_version: fixture.input.definition().repository.version,
        },
        external_identity: ExternalWorkIdentity::new("reviewed-process").unwrap(),
        source: fixture.profile.preparation.source.clone(),
        classification: fixture.profile.preparation.classification.clone(),
        mapping: key("process-document.v1"),
        idempotency_key: IdempotencyKey::new("review-process").unwrap(),
      },
      documents: ProjectTextDocuments::try_new("Triage completion criteria", CRITERIA).unwrap(),
    },
  )
  .await
  .unwrap();
  fixture.profile.preparation.mapping.require_body = false;
  fixture.profile.preparation.mapping.acceptance =
    SourceAcceptanceMapping::ProcessCompletion(SourceCompletionDocument::ProjectInput {
      project_id: project,
      artifact: document.admission.work.artifacts().acceptance().clone(),
    });
  std::fs::write(fixture.root.path().join("github/mode.json"), br#"{"candidates":[{"container_id":"native-container","item_id":"TASK-native","display_id":"TASK-7","update_identity":"same-time","title":"Investigate unexpected response","body":null,"fields":{},"labels":[],"state":"native-state"}]}"#).unwrap();
  document.admission.run.id()
}
fn completion_worker<S, B>(
  fixture: &Fixture<S>,
  bytes: Arc<B>,
  revisions: Arc<dyn RevisionResolver>,
) -> FactorySourceWorker<S, B>
where
  S: WorkSourceArtifactStore + ProjectInputQueryStore + 'static,
  B: octacity_artifact_store::ArtifactBytePublisher + 'static,
{
  FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: fixture.store.clone(),
      bytes,
      revisions,
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      clock: fixture.clock.clone(),
      project_inputs: Some(Arc::new(FactoryProjectInputReader::new(
        fixture.store.clone(),
        fixture.bytes.clone(),
      ))),
    },
    FactorySourceInstallation {
      projects: vec![fixture.input.project_id()],
      profiles: vec![fixture.profile.clone()],
      settings: settings(),
    },
  )
  .unwrap()
}
async fn completion_contract<S>(mut fixture: Fixture<S>) -> (Fixture<S>, WorkArtifacts)
where
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + ProjectTextInputStore + 'static,
{
  let project = fixture.input.project_id();
  let document_run = install_completion(&mut fixture).await;
  let document = fixture.profile.preparation.mapping.acceptance.clone();
  let worker = completion_worker(&fixture, fixture.bytes.clone(), Arc::new(ExactRevisionResolver));
  let result = worker.run_once(CancellationToken::new()).await.unwrap();
  assert_eq!((result.completed, result.deferred), (1, 0));
  let runs = fixture.runs().await;
  assert_eq!(runs.len(), 2, "one document intake and exactly one source Work");
  let run = runs.iter().find(|r| r.id != document_run).unwrap();
  let retained = fixture
    .store
    .source_preparation_for_work(project, run.work_id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained.input.definition().target, fixture.input.definition().target);
  assert!(retained.policy.is_some());
  let objects = fixture.bytes.0.lock().unwrap();
  let acceptance: serde_json::Value = serde_json::from_slice(objects.last().unwrap()).unwrap();
  assert_eq!(acceptance["criteria"], CRITERIA);
  assert_eq!(acceptance["purpose"], "process_completion");
  assert_eq!(
    acceptance["completion_document"]["artifact"],
    match document {
      SourceAcceptanceMapping::ProcessCompletion(SourceCompletionDocument::ProjectInput { artifact, .. }) =>
        serde_json::to_value(artifact).unwrap(),
      _ => unreachable!(),
    }
  );
  let task: serde_json::Value = serde_json::from_slice(&objects[objects.len() - 2]).unwrap();
  assert!(task["body"].is_null());
  assert_eq!(task["fields"], serde_json::json!({}));
  assert!(task.get("acceptance").is_none());
  let artifacts = retained.intent.unwrap().artifacts;
  drop(objects);
  (fixture, artifacts)
}

use crate::runtime::factory_tests::managed as job_fixture;
#[allow(dead_code)]
#[path = "../../tests/support/protected_input_transfer.rs"]
mod transfer;

// Finite object-store peer verifies the exact bytes retained by the Source worker.
struct TransferBytes(Arc<Bytes>);
#[async_trait::async_trait]
impl octacity_artifact_store::ArtifactStore for TransferBytes {
  async fn authorize_upload(
    &self,
    _: &octacity_artifact_store::ArtifactObject,
    _: Duration,
  ) -> Result<octacity_artifact_store::UploadAuthorization, octacity_artifact_store::ArtifactStoreError> {
    unreachable!()
  }
  async fn complete_upload(
    &self,
    _: &octacity_artifact_store::ArtifactObject,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    unreachable!()
  }
  async fn delete(
    &self,
    object: &octacity_artifact_store::ArtifactObject,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    self
      .0
      .0
      .lock()
      .unwrap()
      .retain(|bytes| FactoryDigest::content_sha256(bytes).to_string() != object.sha256());
    Ok(())
  }
  async fn verify_published(
    &self,
    object: &octacity_artifact_store::ArtifactObject,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    use octacity_artifact_store::ArtifactByteReader as _;
    self.0.read_published(object, object.size_bytes()).await.map(|_| ())
  }
  async fn authorize_download(
    &self,
    _: &octacity_artifact_store::ArtifactObject,
    expires_in: Duration,
  ) -> Result<octacity_artifact_store::DownloadAuthorization, octacity_artifact_store::ArtifactStoreError> {
    Ok(octacity_artifact_store::DownloadAuthorization {
      url: "https://objects.invalid/private?grant=redacted".into(),
      expires_in,
    })
  }
}
async fn signed_input_transfer(
  fixture: &Fixture<octacity_server_store_postgres::PostgresStore>,
  artifacts: &WorkArtifacts,
) {
  use octacity_protocol::{JobBinding, ProtectedInputV3, VerifiedJobSpec, verify_compatible_job_spec};
  let mut canonical = job_fixture::canonical_v3_spec();
  let mut source_inputs = vec![];
  for (artifact, name) in [(artifacts.task(), "task"), (artifacts.acceptance(), "acceptance")] {
    let published = fixture
      .store
      .published_execution_input(artifact.artifact_id())
      .await
      .unwrap();
    assert_eq!(published.digest.to_string(), artifact.content_digest().to_string());
    assert!(fixture.store.published_artifact(artifact.artifact_id()).await.is_err());
    source_inputs.push(ProtectedInputV3 {
      artifact_id: artifact.artifact_id().to_string(),
      sha256: artifact.content_digest().to_string(),
      size_bytes: artifact.encoded_size(),
      media_type: "application/json".into(),
      destination: format!("/octacity/protected/source-{name}.json"),
    });
  }
  canonical.protected_inputs.inputs.extend(source_inputs.clone());
  canonical
    .protected_inputs
    .inputs
    .sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
  let permissions = job_fixture::permissions(false);
  let template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: job_fixture::build(&canonical),
    pipeline_node_id: PipelineNodeId::new("triage-input-contract").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: permissions.clone(),
      configuration: permissions.clone(),
      task: permissions.clone(),
      local: LocalPermissionCeiling::fully_enforced(permissions),
    },
    required_enforcement: canonical.required_enforcement.clone(),
    policy: job_fixture::policy(&canonical),
  })
  .unwrap();
  let signer = octacity_server_job::JobSpecSigner::new("triage-input-contract", [9; 32]).unwrap();
  let job = JobId::generate();
  let signed =
    octacity_server_job::sign_ready_job_spec(&template, AttemptNumber::FIRST, job, time(100_000), &signer).unwrap();
  let VerifiedJobSpec::V3(verified) = verify_compatible_job_spec(
    signed.envelope(),
    &std::collections::BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
    JobBinding {
      job_id: &job.to_string(),
      attempt: 1,
      now: 100,
    },
  )
  .unwrap() else {
    panic!("managed v3 expected")
  };
  for input in &source_inputs {
    assert!(verified.protected_inputs.inputs.contains(input));
  }
  let service = transfer::service_with_bytes(fixture.store.clone(), Arc::new(TransferBytes(fixture.bytes.clone())));
  let manifest = octacity_protocol::ProtectedInputManifestV3 {
    inputs: verified
      .protected_inputs
      .inputs
      .into_iter()
      .filter(|i| source_inputs.contains(i))
      .collect(),
  };
  let grants = service
    .authorize_protected_inputs(AuthorizeProtectedInputsInput {
      manifest: manifest.clone(),
      observed_at_unix_ms: 100_001,
      lease_expires_at: time(110_000),
    })
    .await
    .unwrap();
  assert_eq!(grants.len(), 2);
  assert!(grants.iter().all(|g| g.capability.expires_at_unix_ms == 110_000));
  for input in &source_inputs {
    assert!(grants.iter().any(|g| g.input == *input));
  }
  let mut substituted = manifest.clone();
  substituted.inputs[0].sha256 = digest(99).to_string();
  assert!(
    service
      .authorize_protected_inputs(AuthorizeProtectedInputsInput {
        manifest: substituted,
        observed_at_unix_ms: 100_001,
        lease_expires_at: time(110_000),
      })
      .await
      .is_err()
  );
  let expires = fixture
    .store
    .published_execution_input(artifacts.acceptance().artifact_id())
    .await
    .unwrap()
    .expires_at
    .unwrap();
  assert!(
    service
      .authorize_protected_inputs(AuthorizeProtectedInputsInput {
        manifest,
        observed_at_unix_ms: expires.unix_millis(),
        lease_expires_at: time(expires.unix_millis() + 1000),
      })
      .await
      .is_err()
  );
}

#[path = "work_source_completion_recovery_tests.rs"]
mod recovery;
