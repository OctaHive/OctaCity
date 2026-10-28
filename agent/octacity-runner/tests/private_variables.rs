//! Private operator overrides must fit the exact immutable task declaration.
use std::collections::BTreeMap;

use octacity_protocol::ProtectedInputV3;
use octacity_runner::RunnerPrivateVariables;
use sha2::{Digest as _, Sha256};

#[test]
fn independent_credential_selection_cannot_reuse_values_or_exceed_the_private_budget() {
  let model = RunnerPrivateVariables::new(BTreeMap::from([("MODEL".into(), "model-only-canary".into())])).unwrap();
  let reused = RunnerPrivateVariables::new(BTreeMap::from([("TOKEN".into(), "model-only-canary".into())])).unwrap();
  assert!(
    model.with_selected_credential("OCTACITY_MCP_ONE", &reused).is_err(),
    "a renamed model token is not an independent MCP credential"
  );
  let full = RunnerPrivateVariables::new(BTreeMap::from([("MODEL".into(), "x".repeat(1024 * 1024 - 6))])).unwrap();
  let extra = RunnerPrivateVariables::new(BTreeMap::from([("TOKEN".into(), "other-credential".into())])).unwrap();
  assert!(
    full.with_selected_credential("OCTACITY_MCP_ONE", &extra).is_err(),
    "merging profiles cannot multiply the existing private-byte limit"
  );
  let ambiguous = RunnerPrivateVariables::new(BTreeMap::from([
    ("ONE".into(), "one".into()),
    ("TWO".into(), "two".into()),
  ]))
  .unwrap();
  assert!(model.with_selected_credential("OCTACITY_MCP_ONE", &ambiguous).is_err());
}

#[test]
fn private_variables_authorize_only_declared_secret_required_inputs() {
  let octafile = br#"{"version":1,"vars":{"PRIVATE_AUTH":{"required":true,"secret":true},"PUBLIC_PROMPT":"immutable public instruction"},"tasks":{"execute":{"cmds":[{"id":"fixture","fixture":{"operation":"bounded"}}]}}}"#;
  let input = ProtectedInputV3 {
    artifact_id: "immutable-task".to_owned(),
    destination: "/octacity/protected/Octafile.yml".to_owned(),
    media_type: "application/json".to_owned(),
    size_bytes: octafile.len() as u64,
    sha256: hex::encode(Sha256::digest(octafile)),
  };
  let selected = vec!["execute".to_owned()];
  let private = RunnerPrivateVariables::new(BTreeMap::from([(
    "PRIVATE_AUTH".to_owned(),
    "private-provider-credential-canary".to_owned(),
  )]))
  .unwrap();
  let authorized = private.authorize(&input, octafile, &selected).unwrap();
  assert!(!format!("{private:?} {authorized:?}").contains("private-provider-credential-canary"));
  let public_override = RunnerPrivateVariables::new(BTreeMap::from([(
    "PUBLIC_PROMPT".to_owned(),
    "operator-must-not-rewrite-model-input".to_owned(),
  )]))
  .unwrap();
  assert!(
    public_override.authorize(&input, octafile, &selected).is_err(),
    "installed private values cannot replace the signed task's public instruction"
  );
}

#[test]
fn private_inputs_cannot_be_rebound_by_task_scopes_dependencies_or_includes() {
  let private = RunnerPrivateVariables::new(BTreeMap::from([(
    "PRIVATE_AUTH".to_owned(),
    "private-boundary-canary".to_owned(),
  )]))
  .unwrap();
  let base = serde_json::json!({
    "version":1,
    "vars":{"PRIVATE_AUTH":{"required":true,"secret":true}},
    "tasks":{"execute":{"cmds":[{"id":"fixture","fixture":{"operation":"bounded"}}]}}
  });
  let mut scoped = base.clone();
  scoped["tasks"]["execute"]["vars"] = serde_json::json!({"PRIVATE_AUTH":"public scope override"});
  let mut dependent = base.clone();
  dependent["tasks"]["execute"]["deps"] = serde_json::json!(["other"]);
  dependent["tasks"]["other"] = serde_json::json!({"cmds":[{"id":"fixture","fixture":{"operation":"other"}}]});
  let mut included = base.clone();
  included["includes"] = serde_json::json!({"outside":{"octafile":"repository-controlled.yml"}});
  let refusals = [scoped, dependent, included]
    .into_iter()
    .map(|document| {
      let bytes = serde_json::to_vec(&document).unwrap();
      let input = ProtectedInputV3 {
        artifact_id: "exact-immutable-task".to_owned(),
        destination: "/octacity/protected/Octafile.yml".to_owned(),
        media_type: "application/json".to_owned(),
        size_bytes: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(&bytes)),
      };
      private.authorize(&input, &bytes, &["execute".to_owned()]).is_err()
    })
    .collect::<Vec<_>>();
  assert_eq!(
    refusals,
    vec![true, true, true],
    "credential authority requires a closed immutable declaration scope"
  );
}

#[test]
fn private_inputs_cannot_be_rebound_by_a_nested_task_invocation() {
  let private = RunnerPrivateVariables::new(BTreeMap::from([(
    "PRIVATE_AUTH".to_owned(),
    "private-invocation-canary".to_owned(),
  )]))
  .unwrap();
  let bytes = serde_json::to_vec(&serde_json::json!({
    "version":1,
    "vars":{"PRIVATE_AUTH":{"required":true,"secret":true}},
    "tasks":{
      "execute":{"cmds":[{"task":"other","vars":{"PRIVATE_AUTH":"public invocation override"}}]},
      "other":{"cmds":[{"id":"fixture","fixture":{"operation":"bounded"}}]}
    }
  }))
  .unwrap();
  let input = ProtectedInputV3 {
    artifact_id: "immutable-invocation".to_owned(),
    destination: "/octacity/protected/Octafile.yml".to_owned(),
    media_type: "application/json".to_owned(),
    size_bytes: bytes.len() as u64,
    sha256: hex::encode(Sha256::digest(&bytes)),
  };
  assert!(
    private.authorize(&input, &bytes, &["execute".to_owned()]).is_err(),
    "nested Octa task invocation creates another variable scope and is outside the compiled private-input contract"
  );
}

#[test]
fn private_inputs_require_the_exact_generated_required_secret_declaration() {
  let private = RunnerPrivateVariables::new(BTreeMap::from([(
    "PRIVATE_AUTH".to_owned(),
    "private-declaration-canary".to_owned(),
  )]))
  .unwrap();
  let extras = [
    ("value", serde_json::json!("public default")),
    ("sh", serde_json::json!("read repository data")),
    ("from", serde_json::json!({"task":"other","output":"value"})),
    ("enum", serde_json::json!(["public constraint"])),
    ("question", serde_json::json!("public interactive prompt")),
  ];
  let refusals = extras
    .into_iter()
    .map(|(field, value)| {
      let mut document = serde_json::json!({
        "version":1,"vars":{"PRIVATE_AUTH":{"required":true,"secret":true}},
        "tasks":{"execute":{"cmds":[{"id":"fixture","fixture":{"operation":"bounded"}}]}}
      });
      document["vars"]["PRIVATE_AUTH"][field] = value;
      let bytes = serde_json::to_vec(&document).unwrap();
      let input = ProtectedInputV3 {
        artifact_id: "immutable-declaration".to_owned(),
        destination: "/octacity/protected/Octafile.yml".to_owned(),
        media_type: "application/json".to_owned(),
        size_bytes: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(&bytes)),
      };
      private.authorize(&input, &bytes, &["execute".to_owned()]).is_err()
    })
    .collect::<Vec<_>>();
  assert_eq!(
    refusals,
    vec![true; 5],
    "private authorization supports the compiler's exact declaration, rather than later runtime ambiguity or unsupported constraints"
  );
}

#[test]
fn private_declarations_allow_the_fixed_virtual_workspace_without_task_overrides() {
  let bytes = br#"{"version":1,"vars":{"PRIVATE_AUTH":{"required":true,"secret":true}},"tasks":{"execute":{"dir":"/workspace","cmds":[{"id":"fixture","fixture":{"operation":"bounded"}}]}}}"#;
  let input = ProtectedInputV3 {
    artifact_id: "anchored-task".to_owned(),
    destination: "/octacity/protected/Octafile.yml".to_owned(),
    media_type: "application/json".to_owned(),
    size_bytes: bytes.len() as u64,
    sha256: hex::encode(Sha256::digest(bytes)),
  };
  let private = RunnerPrivateVariables::new(BTreeMap::from([(
    "PRIVATE_AUTH".to_owned(),
    "private-value".to_owned(),
  )]))
  .unwrap();
  assert!(
    private.authorize(&input, bytes, &["execute".to_owned()]).is_ok(),
    "the immutable generated cwd selects only the fixed virtual workspace, without rebinding private variables"
  );
}

#[test]
fn declaration_byte_limit_is_independent_of_the_private_value_budget() {
  // A bounded public instruction fits the compiler prompt contract, while
  // structural metadata makes its protected document slightly larger.
  let instruction = "x".repeat(1024 * 1024);
  let bytes = serde_json::to_vec(&serde_json::json!({
    "version":1,
    "vars":{"PRIVATE_AUTH":{"required":true,"secret":true}},
    "tasks":{"execute":{"cmds":[{"id":"fixture","fixture":{"instruction":instruction}}]}}
  }))
  .unwrap();
  let input = ProtectedInputV3 {
    artifact_id: "bounded-large-task".to_owned(),
    destination: "/octacity/protected/Octafile.yml".to_owned(),
    media_type: "application/json".to_owned(),
    size_bytes: bytes.len() as u64,
    sha256: hex::encode(Sha256::digest(&bytes)),
  };
  let private = RunnerPrivateVariables::new(BTreeMap::from([(
    "PRIVATE_AUTH".to_owned(),
    "small-private-value".to_owned(),
  )]))
  .unwrap();
  assert!(
    private.authorize(&input, &bytes, &["execute".to_owned()]).is_ok(),
    "the finite private value budget must not reject a separately bounded public declaration"
  );
}

#[cfg(unix)]
mod start_boundary {
  use super::*;
  use async_trait::async_trait;
  use octacity_execution::*;
  use octacity_protocol::{ExecutionSpec, OctaSpec};
  use octacity_runner::{
    AuthorizedRunnerPrivateVariables, RunnerCapabilities, RunnerInstallation, RunnerJobRequest, RunnerRedactions,
    RunnerSupervisionPolicy, supervise_with_private_variables,
  };
  use std::{
    os::unix::fs::PermissionsExt as _,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
  };
  use tokio_util::sync::CancellationToken;

  struct StartCounter(AtomicUsize);
  #[async_trait]
  impl ExecutionBackend for StartCounter {
    async fn start(
      &self,
      _: &RunnerProgram,
      _: StartExecution,
      _: CancellationToken,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      self.0.fetch_add(1, Ordering::SeqCst);
      Err(ExecutionError::Unavailable("portable start-boundary probe".to_owned()))
    }
    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      Ok(())
    }
  }

  struct Fixture {
    _root: tempfile::TempDir,
    protected: std::path::PathBuf,
    file: std::path::PathBuf,
    runner: RunnerInstallation,
    job: Option<RunnerJobRequest>,
    authorized: AuthorizedRunnerPrivateVariables,
  }
  fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let job_root = root_path.join("job");
    octacity_private_fs::create_private_directory(&job_root).unwrap();
    let protected = job_root.join("protected");
    let source = job_root.join("source");
    let scratch = job_root.join("scratch");
    let output = job_root.join("output");
    for path in [&protected, &source, &scratch, &output] {
      octacity_private_fs::create_private_directory(path).unwrap();
    }
    let data = scratch.join("data");
    octacity_private_fs::create_private_directory(&data).unwrap();
    let bytes = br#"{"version":1,"vars":{"PRIVATE_AUTH":{"required":true,"secret":true}},"tasks":{"execute":{"shell":"bounded public fixture"}}}"#;
    let file = protected.join("Octafile.yml");
    std::fs::write(&file, bytes).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
    std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o500)).unwrap();
    let input = ProtectedInputV3 {
      artifact_id: "exact-task".to_owned(),
      destination: "/octacity/protected/Octafile.yml".to_owned(),
      media_type: "application/json".to_owned(),
      size_bytes: bytes.len() as u64,
      sha256: hex::encode(Sha256::digest(bytes)),
    };
    let tasks = vec!["execute".to_owned()];
    let authorized = RunnerPrivateVariables::new(BTreeMap::from([(
      "PRIVATE_AUTH".to_owned(),
      "private-canary".to_owned(),
    )]))
    .unwrap()
    .authorize(&input, bytes, &tasks)
    .unwrap();
    let digest = hex::encode(Sha256::digest(b"portable installed runner"));
    let runner = RunnerInstallation {
      root: root_path.clone(),
      executable: root_path.join("runner"),
      plugins_dir: root_path.join("plugins"),
      default_plugin_lock: root_path.join("Octa.lock"),
      sha256: digest.clone(),
      capabilities: RunnerCapabilities {
        octa_version: "fixture-version".to_owned(),
        runner_protocols: vec![1],
        event_schemas: vec![3],
        plugin_protocols: vec![1],
        octafile_versions: vec![1],
        platform: "any".to_owned(),
        features: vec![],
        build_commit: None,
      },
      plugins: BTreeMap::new(),
    };
    let job = RunnerJobRequest {
      request_id: "private-start-boundary".to_owned(),
      octa: OctaSpec {
        version: "fixture-version".to_owned(),
        runner_sha256: digest,
        runner_protocol: 1,
        event_schema: 3,
        plugin_protocol: 1,
        plugin_digests: BTreeMap::new(),
      },
      execution: StartExecution {
        execution_id: "private-start-boundary".to_owned(),
        workspace_root: root_path,
        workspace: source.clone(),
        data_dir: data,
        workload_identity: None,
        cache: None,
        factory: Some(FactoryExecutionLayout {
          workspace_anchor: false,
          disk_accounting: ExecutionDiskAccounting::WholeJob,
          protected_inputs: protected.clone(),
          source,
          source_visible: true,
          source_read_only: true,
          source_writable_paths: vec![],
          tool_channel: None,
          scratch,
          output,
          process_limit: 2,
        }),
        cpu_millis: 1000,
        memory_bytes: 1024,
        writable_disk_bytes: 1024,
        max_duration: Duration::from_secs(5),
        root: ExecutionTarget::Native {
          platform: ExecutionPlatform {
            os: ExecutionOs::Linux,
            architecture: ExecutionArchitecture::Amd64,
          },
        },
        network: NetworkAccess::Disabled,
      },
      spec: ExecutionSpec {
        octafile: Some(input.destination),
        commands: tasks,
        variables: BTreeMap::new(),
        arguments: vec![],
        concurrency: None,
        parallel: false,
        failfast: true,
        secrets_profile: None,
      },
      cache: None,
      redactions: RunnerRedactions::default(),
      cancellation_grace: Duration::from_secs(1),
      external_executables: BTreeMap::new(),
    };
    Fixture {
      _root: root,
      protected,
      file,
      runner,
      job: Some(job),
      authorized,
    }
  }
  impl Drop for Fixture {
    fn drop(&mut self) {
      // Restore the owned temporary directory for TempDir's ordinary cleanup.
      let _ = std::fs::set_permissions(&self.protected, std::fs::Permissions::from_mode(0o700));
    }
  }
  async fn starts(fixture: &mut Fixture) -> usize {
    let backend = StartCounter(AtomicUsize::new(0));
    let (events, _) = tokio::sync::mpsc::channel(1);
    let job = fixture.job.take().unwrap();
    let result = supervise_with_private_variables(
      &fixture.runner,
      &backend,
      job,
      CancellationToken::new(),
      &events,
      &RunnerSupervisionPolicy::default(),
      Some(fixture.authorized.clone()),
    )
    .await;
    assert!(result.is_err(), "this probe deliberately never starts a real execution");
    backend.0.load(Ordering::SeqCst)
  }
  #[tokio::test]
  async fn exact_private_binding_is_verified_again_before_backend_start() {
    let mut valid = fixture();
    assert_eq!(starts(&mut valid).await, 1);
    let mut changed = fixture();
    std::fs::set_permissions(&changed.file, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&changed.file, b"changed after authorization").unwrap();
    std::fs::set_permissions(&changed.file, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(
      starts(&mut changed).await,
      0,
      "a private proof cannot outlive the exact frozen bytes"
    );
  }

  #[tokio::test]
  async fn public_execution_variables_cannot_shadow_private_bindings_before_backend_start() {
    let mut fixture = fixture();
    fixture
      .job
      .as_mut()
      .unwrap()
      .spec
      .variables
      .insert("PRIVATE_AUTH".to_owned(), "signed public replacement".to_owned());
    assert_eq!(
      starts(&mut fixture).await,
      0,
      "public/private ambiguity must fail before backend mutation"
    );
  }
}
