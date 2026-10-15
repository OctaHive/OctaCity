//! Real trusted Git capture crosses the production bounded upload boundary.
use super::*;
use octacity_changeset::{
  CaptureObservationContext, CaptureRequest, ChangeSetCapturer, GitCaptureTool, GitChangeSetCapturer,
};
use octacity_coordinator::{CoordinatorError, OutputUploadCoordinator, Registration, RetryPolicy};
use octacity_output::{
  FreezeOutputs, OutputError, OutputPublisher, PresignedOutputPublisher, PresignedOutputPublisherConfig, PublishOutputs,
};
use octacity_protocol::*;
use sha2::{Digest as _, Sha256};
use std::{
  collections::BTreeMap,
  fs,
  path::Path,
  process::Command,
  sync::{Arc, Mutex},
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
};
use tokio_util::sync::CancellationToken;

struct Uploads {
  url: String,
  metadata: Mutex<Vec<OutputUploadMetadata>>,
}
#[async_trait::async_trait]
impl OutputUploadCoordinator for Uploads {
  async fn begin_output_upload(
    &self,
    _: &Registration,
    _: &LeaseAssignment,
    r: &BeginOutputUploadRequest,
    _: CancellationToken,
  ) -> Result<BeginOutputUploadResponse, CoordinatorError> {
    self.metadata.lock().unwrap().push(r.output.clone());
    Ok(BeginOutputUploadResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: r.request_id.clone(),
      upload_id: r.request_id.clone(),
      put_url: self.url.clone(),
      required_headers: BTreeMap::new(),
      expires_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 60,
    })
  }
  async fn complete_output_upload(
    &self,
    _: &Registration,
    _: &LeaseAssignment,
    _: &CompleteOutputUploadRequest,
    _: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    Ok(())
  }
}
fn hash(bytes: &[u8]) -> String {
  hex::encode(Sha256::digest(bytes))
}
fn git(git: &Path, root: &Path, args: &[&str]) -> String {
  let output = Command::new(git)
    .current_dir(root)
    .args(args)
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
    .env("GIT_AUTHOR_NAME", "Fixture")
    .env("GIT_COMMITTER_NAME", "Fixture")
    .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
    .env("GIT_COMMITTER_EMAIL", "fixture@example.test")
    .output()
    .unwrap();
  assert!(output.status.success());
  String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
#[tokio::test]
async fn trusted_capture_observation_is_a_schema_pinned_report_at_the_actual_freeze_boundary() {
  let temp = tempfile::tempdir().unwrap();
  let root = temp.path().canonicalize().unwrap();
  let workspace = root.join("source");
  let destination = root.join("captured");
  fs::create_dir(&workspace).unwrap();
  fs::create_dir(&destination).unwrap();
  let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap())
    .map(|p| p.join(if cfg!(windows) { "git.exe" } else { "git" }))
    .find(|p| p.is_file())
    .unwrap()
    .canonicalize()
    .unwrap();
  git(&executable, &workspace, &["init", "--quiet", "--initial-branch=main"]);
  fs::write(workspace.join("source.txt"), b"base").unwrap();
  git(&executable, &workspace, &["add", "--all"]);
  git(&executable, &workspace, &["commit", "--quiet", "-m", "base"]);
  let base = git(&executable, &workspace, &["rev-parse", "HEAD"]);
  fs::write(workspace.join("source.txt"), b"PRIVATE_CANDIDATE_BODY").unwrap();
  let captured = GitChangeSetCapturer::new(GitCaptureTool {
    identity: FactoryImmutableReferenceV3 {
      identity: "git".to_owned(),
      version: "installed".to_owned(),
      sha256: hash(&fs::read(&executable).unwrap()),
    },
    executable,
  })
  .unwrap()
  .capture(
    CaptureRequest {
      workspace: workspace.clone(),
      destination,
      base_revision: base.clone(),
      bundle_base_revision: base,
      node_attempt_id: "00000000-0000-0000-0000-000000000003".to_owned(),
      instruction: ChangeSetCaptureV3 {
        author_name: "Operator".to_owned(),
        author_email: "operator@example.test".to_owned(),
        committed_at: 1,
        patch_max_bytes: None,
        policy: ChangeSetCapturePolicyV3 {
          allowed_path_prefixes: vec![".".to_owned()],
          forbidden_control_paths: vec![],
          max_changed_paths: 4,
          max_file_bytes: 1024,
          max_total_bytes: 4096,
          allow_empty: false,
          binary_policy: ChangeSetBinaryPolicyV3::Reject,
          forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
        },
        observation: Some(ChangeSetObservationOutputV3 {
          name: "arbitrary.capture-observation".to_owned(),
          schema: FactoryImmutableReferenceV3 {
            identity: "project.capture-facts".to_owned(),
            version: "v7".to_owned(),
            sha256: "5b".repeat(32),
          },
        }),
      },
      observation: Some(CaptureObservationContext {
        input_digest: "2a".repeat(32),
        fresh_until_unix_ms: 5000,
      }),
    },
    CancellationToken::new(),
  )
  .await
  .unwrap();
  let trusted = trusted_change_set_outputs(&captured).unwrap();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin = format!("http://{}", listener.local_addr().unwrap());
  let uploads = Arc::new(Uploads {
    url: format!("{origin}/object"),
    metadata: Mutex::new(Vec::new()),
  });
  let publisher = PresignedOutputPublisher::new(
    uploads.clone(),
    PresignedOutputPublisherConfig {
      allowed_origins: vec![origin],
      ca_certificate_file: None,
      max_archive_entries: 8,
      upload_timeout: Duration::from_secs(10),
      retry: RetryPolicy {
        max_attempts: 1,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(1),
      },
    },
  )
  .unwrap();
  let mut limits = OutputLimits {
    artifact_count: 2,
    artifact_bytes: 1024 * 1024,
    report_count: 0,
    report_bytes: 0,
    single_output_bytes: 4096,
  };
  limits.validate().unwrap();
  let denied = publisher
    .freeze(
      FreezeOutputs {
        workspace: &workspace,
        workspace_prefix: None,
        results: &[],
        trusted_files: &trusted,
        limits: &limits,
        staging_root: &root.join("denied"),
      },
      CancellationToken::new(),
    )
    .await;
  assert!(
    matches!(denied, Err(OutputError::Invalid(_))),
    "the real publisher must count the additional trusted Report against its own signed quota"
  );
  limits.report_count = 1;
  limits.report_bytes = 4096;
  limits.validate().unwrap();
  let frozen = publisher
    .freeze(
      FreezeOutputs {
        workspace: &workspace,
        workspace_prefix: None,
        results: &[],
        trusted_files: &trusted,
        limits: &limits,
        staging_root: &root.join("staging"),
      },
      CancellationToken::new(),
    )
    .await
    .unwrap();
  let bodies = Arc::new(Mutex::new(Vec::new()));
  let received = bodies.clone();
  let task = tokio::spawn(async move {
    for _ in 0..3 {
      let (mut stream, _) = listener.accept().await.unwrap();
      let mut bytes = Vec::new();
      let mut buffer = [0u8; 4096];
      let header_end = loop {
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
          break i + 4;
        }
      };
      let headers = String::from_utf8_lossy(&bytes[..header_end]);
      let size: usize = headers
        .lines()
        .find_map(|line| {
          line
            .to_ascii_lowercase()
            .strip_prefix("content-length: ")
            .map(str::to_owned)
        })
        .unwrap()
        .parse()
        .unwrap();
      while bytes.len() - header_end < size {
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
      }
      received.lock().unwrap().push(bytes[header_end..].to_vec());
      stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    }
  });
  let registration = Registration {
    agent_id: "agent".to_owned(),
    registration_id: "registration".to_owned(),
    max_retry_delay: Duration::from_secs(1),
    execution_contract_version: 3,
  };
  let lease = LeaseAssignment {
    lease_id: "lease".to_owned(),
    job_id: "job".to_owned(),
    attempt: 1,
    fencing_token: "fence".to_owned(),
    issued_at: 1,
    expires_at: u64::MAX,
    signed_job_spec: SignedEnvelope {
      key_id: "unused".to_owned(),
      algorithm: "unused".to_owned(),
      payload: "unused".to_owned(),
      signature: "unused".to_owned(),
    },
    protected_inputs: vec![],
  };
  publisher
    .publish(
      PublishOutputs {
        registration: &registration,
        lease: &lease,
        frozen,
      },
      CancellationToken::new(),
    )
    .await
    .unwrap();
  task.await.unwrap();
  let metadata = uploads.metadata.lock().unwrap();
  assert_eq!(metadata.len(), 3);
  let index = metadata
    .iter()
    .position(|m| m.kind == OutputKind::Report)
    .expect("trusted capture observation is retained as Report");
  let report = &metadata[index];
  assert_eq!(report.name, "arbitrary.capture-observation");
  assert_eq!(report.report_format.as_deref(), Some("project.capture-facts"));
  assert_eq!(report.run_id, CHANGE_SET_CAPTURE_PRODUCER_ID);
  assert_eq!(report.task_id, CHANGE_SET_CAPTURE_PRODUCER_ID);
  let body = &bodies.lock().unwrap()[index];
  assert_eq!(hash(body), report.sha256);
  assert_eq!(body.len() as u64, report.size_bytes);
  let facts: CapturedChangeSetObservationV1 = serde_json::from_slice(body).unwrap();
  assert_eq!(facts.candidate_revision, captured.manifest.candidate_revision);
  assert_eq!(facts.input_digest, "2a".repeat(32));
  assert!(!String::from_utf8_lossy(body).contains("PRIVATE_CANDIDATE_BODY"));
}
