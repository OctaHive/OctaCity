//! Actual published native client/plugin records cross the public capture verifier.
use octacity_server_application::{
  FactoryCodexNativeCaptureContract, FactoryCodexOutputDocument, FactoryCodexProvenanceContract, FactoryMcpCallStatus,
  codex_prompt_provenance_digest, verify_codex_native_capture,
};
use octacity_server_artifacts::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactRetentionPolicy, ArtifactType,
};
use octacity_server_domain::{ArtifactId, ArtifactName, AttemptId, BuildId, JobId, LeaseId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey, ImmutableReference};
use std::{
  fs,
  path::PathBuf,
  process::Command,
  time::{SystemTime, UNIX_EPOCH},
};

fn now() -> Timestamp {
  Timestamp::from_unix_millis(
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_millis()
      .try_into()
      .unwrap(),
  )
  .unwrap()
}
struct OwnedDirectory(PathBuf, String);
impl Drop for OwnedDirectory {
  fn drop(&mut self) {
    if self.0.parent() == Some(std::env::temp_dir().as_path())
      && fs::read_to_string(self.0.join(".owner")).ok().as_deref() == Some(&self.1)
    {
      let _ = fs::remove_dir_all(&self.0);
    }
  }
}
#[test]
#[ignore = "select pinned Codex 0.161.0 and published Octa 0.8.0 plugin binaries"]
fn actual_native_mcp_results_resolve_by_exact_retained_artifact_record_and_digest() {
  let plugin = std::env::var("OCTACITY_TEST_CODEX_PLUGIN_BIN").expect("select the published plugin");
  let native = std::env::var("OCTACITY_TEST_CODEX_BIN").expect("select the native client");
  let owner = uuid::Uuid::new_v4().to_string();
  let root = std::env::temp_dir().join(format!("octacity-native-evidence-{owner}"));
  fs::create_dir(&root).unwrap();
  fs::write(root.join(".owner"), &owner).unwrap();
  let directory = OwnedDirectory(root, owner);
  let started = now();
  let output = Command::new("python3").current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap())
    .args(["-c", "from tools.tests.test_factory_mcp_credentials import NativeMcpCredentialTests; import os; t=NativeMcpCredentialTests(); rows=t.invoke(export_directory=os.environ['OCTACITY_CAPTURE_DIRECTORY']); t.assertEqual(rows[0][-1]['payload']['code'],0)"])
    .env("OCTACITY_CAPTURE_DIRECTORY", &directory.0).output().unwrap();
  let finished = now();
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let build = BuildId::generate();
  let attempt = AttemptId::generate();
  let job = JobId::generate();
  let mut documents = Vec::new();
  for (name, file, media) in [
    ("codex-run-trace", "trace.jsonl", "application/x-ndjson"),
    ("codex-run-provenance", "provenance.json", "application/json"),
  ] {
    let bytes = fs::read(directory.0.join(file)).unwrap();
    documents.push(FactoryCodexOutputDocument {
      identity: ArtifactIdentity {
        artifact_id: ArtifactId::generate(),
        build_id: build,
        attempt_id: attempt,
        job_id: job,
        lease_id: LeaseId::generate(),
        logical_name: ArtifactName::new(name).unwrap(),
        artifact_type: ArtifactType::Artifact,
        media_type: ArtifactMediaType::new(media).unwrap(),
        size_bytes: bytes.len() as u64,
        digest: ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(&bytes).as_bytes()),
        retention: ArtifactRetentionPolicy::Keep,
      },
      bytes,
    });
  }
  let pin = |name, version, bytes: &[u8]| {
    ImmutableReference::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new(version).unwrap(),
      FactoryDigest::content_sha256(bytes),
    )
  };
  let plugin = pin("codex", "0.8.0", &fs::read(plugin).unwrap());
  let executable = pin("codex-cli", "0.161.0", &fs::read(native).unwrap());
  let model = pin("finite-model", "v1", b"finite-model");
  let capture = verify_codex_native_capture(
    FactoryCodexNativeCaptureContract {
      build_id: build,
      attempt_id: attempt,
      job_id: job,
      maximum_bytes: 16 * 1024 * 1024,
      provenance: FactoryCodexProvenanceContract {
        plugin: &plugin,
        executable: &executable,
        model: &model,
        model_identifier: None,
        prompt_digest: codex_prompt_provenance_digest("Use the declared finite tools, then return true."),
        source_revision: &"a".repeat(40),
      },
      execution_window: started..=finished,
    },
    &documents,
  )
  .unwrap();
  assert_eq!(
    capture
      .mcp_calls()
      .iter()
      .map(|call| (call.connection(), call.tool(), call.status()))
      .collect::<Vec<_>>(),
    [
      ("remote", "facts", FactoryMcpCallStatus::Completed),
      ("local", "facts", FactoryMcpCallStatus::Completed),
      ("remote", "facts", FactoryMcpCallStatus::Failed),
      ("local", "write", FactoryMcpCallStatus::Failed)
    ]
  );
  for call in capture.mcp_calls() {
    assert_eq!(capture.resolve_evidence(&call.reference()).unwrap().id(), call.id());
  }
  for document in documents {
    let text = std::str::from_utf8(&document.bytes).unwrap();
    for token in [
      "fixture-model-only",
      "fixture-http-read-alpha",
      "fixture-stdio-read-alpha",
      "fixture-source-only",
      "fixture-reporter-only",
    ] {
      assert!(!text.contains(token));
    }
  }
}
