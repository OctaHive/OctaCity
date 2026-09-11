//! End-to-end Git materialization and agent/plugin lifecycle checks.

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  process::Command,
  time::Duration,
};

use octacity_source::{InstalledSourcePlugin, SourceMaterializationRequest};
use octacity_source_git::{GitSourceError, materialize};
use octacity_source_plugin::{MaterializeRequest, SourcePluginManifest};
use tokio_util::sync::CancellationToken;

struct RepositoryFixture {
  _temp: tempfile::TempDir,
  repository: PathBuf,
  workspace: PathBuf,
  revision: String,
  git: PathBuf,
}

impl RepositoryFixture {
  fn new() -> Self {
    let git = find_git();
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repository");
    let workspace = temp.path().join("workspace");
    fs::create_dir(&repository).unwrap();
    fs::create_dir(&workspace).unwrap();
    git_command(&git, &repository, ["init", "--quiet", "--initial-branch=main"]);
    fs::write(repository.join("message.txt"), "from exact revision\n").unwrap();
    git_command(&git, &repository, ["add", "message.txt"]);
    git_command(
      &git,
      &repository,
      [
        "-c",
        "user.name=OctaCity Test",
        "-c",
        "user.email=octacity@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "fixture",
      ],
    );
    let revision = git_output(&git, &repository, ["rev-parse", "HEAD"]);
    Self {
      _temp: temp,
      repository,
      workspace,
      revision: revision.trim().to_owned(),
      git,
    }
  }

  fn request(&self) -> MaterializeRequest {
    MaterializeRequest {
      destination: self.workspace.to_string_lossy().into_owned(),
      revision: self.revision.clone(),
      reference: Some("refs/heads/main".to_owned()),
      parameters: BTreeMap::from([(
        "url".to_owned(),
        serde_json::Value::String(self.repository.to_string_lossy().into_owned()),
      )]),
      settings: BTreeMap::from([
        (
          "git_path".to_owned(),
          serde_json::Value::String(self.git.to_string_lossy().into_owned()),
        ),
        ("allow_file".to_owned(), serde_json::Value::Bool(true)),
      ]),
      credential_files: BTreeMap::new(),
      max_workspace_bytes: 16 * 1024 * 1024,
    }
  }

  fn settings(&self) -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([
      (
        "git_path".to_owned(),
        serde_json::Value::String(self.git.to_string_lossy().into_owned()),
      ),
      ("allow_file".to_owned(), serde_json::Value::Bool(true)),
    ])
  }

  fn installed_plugin(&self) -> InstalledSourcePlugin {
    InstalledSourcePlugin {
      manifest: SourcePluginManifest {
        manifest_version: 1,
        name: "git".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol_min: 1,
        protocol_max: 1,
        executable: "octacity-source-git".to_owned(),
        sha256: "0".repeat(64),
        platforms: vec![format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)],
        settings: self.settings(),
      },
      executable: PathBuf::from(env!("CARGO_BIN_EXE_octacity-source-git")),
    }
  }

  fn host_request(&self, request_id: &str) -> SourceMaterializationRequest {
    SourceMaterializationRequest {
      request_id: request_id.to_owned(),
      destination: self.workspace.clone(),
      revision: self.revision.clone(),
      reference: Some("refs/heads/main".to_owned()),
      parameters: BTreeMap::from([(
        "url".to_owned(),
        serde_json::Value::String(self.repository.to_string_lossy().into_owned()),
      )]),
      credential_files: BTreeMap::new(),
      max_workspace_bytes: 16 * 1024 * 1024,
    }
  }
}

#[tokio::test]
async fn materializes_the_exact_revision_detached() {
  let fixture = RepositoryFixture::new();
  let source = materialize(&fixture.request(), CancellationToken::new()).await.unwrap();

  assert_eq!(source.revision, fixture.revision);
  assert_eq!(
    fs::read_to_string(fixture.workspace.join("message.txt")).unwrap(),
    "from exact revision\n"
  );
  assert_eq!(
    git_output(&fixture.git, &fixture.workspace, ["rev-parse", "HEAD"]).trim(),
    fixture.revision
  );
  assert!(!git_success(
    &fixture.git,
    &fixture.workspace,
    ["symbolic-ref", "--quiet", "HEAD"]
  ));
}

#[tokio::test]
async fn rejects_local_transport_without_operator_permission() {
  let fixture = RepositoryFixture::new();
  let mut request = fixture.request();
  request
    .settings
    .insert("allow_file".to_owned(), serde_json::Value::Bool(false));

  assert!(
    materialize(&request, CancellationToken::new())
      .await
      .unwrap_err()
      .to_string()
      .contains("local Git transports are disabled")
  );
}

#[tokio::test]
async fn observes_cancellation_before_starting_git() {
  let fixture = RepositoryFixture::new();
  let cancellation = CancellationToken::new();
  cancellation.cancel();
  assert!(matches!(
    materialize(&fixture.request(), cancellation).await,
    Err(GitSourceError::Cancelled)
  ));
}

#[tokio::test]
async fn agent_host_materializes_through_the_process_protocol() {
  let fixture = RepositoryFixture::new();
  let plugin = fixture.installed_plugin();
  let source = plugin
    .materialize(
      fixture.host_request("source-1"),
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap();

  assert_eq!(source.revision, fixture.revision);
  assert_eq!(source.progress, ["materializing Git revision"]);
  assert_eq!(
    fs::read_to_string(fixture.workspace.join("message.txt")).unwrap(),
    "from exact revision\n"
  );
}

#[tokio::test]
async fn agent_preserves_a_cancellation_requested_before_materialization() {
  let fixture = RepositoryFixture::new();
  let cancellation = CancellationToken::new();
  cancellation.cancel();
  let error = fixture
    .installed_plugin()
    .materialize(
      fixture.host_request("pre-cancelled-source"),
      Duration::from_secs(10),
      Duration::from_secs(1),
      cancellation,
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("was cancelled"));
}

#[cfg(unix)]
#[tokio::test]
async fn agent_forces_the_whole_plugin_process_group_down_after_cancel() {
  use std::{os::unix::fs::PermissionsExt as _, time::Instant};

  let fixture = RepositoryFixture::new();
  let slow_git = fixture._temp.path().join("slow-git");
  fs::write(&slow_git, "#!/bin/sh\nsleep 30\n").unwrap();
  fs::set_permissions(&slow_git, fs::Permissions::from_mode(0o755)).unwrap();
  let mut plugin = fixture.installed_plugin();
  plugin.manifest.settings.insert(
    "git_path".to_owned(),
    serde_json::Value::String(slow_git.to_string_lossy().into_owned()),
  );
  let cancellation = CancellationToken::new();
  let trigger = cancellation.clone();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(100)).await;
    trigger.cancel();
  });
  let started = Instant::now();
  let error = plugin
    .materialize(
      fixture.host_request("cancel-source"),
      Duration::from_secs(20),
      Duration::from_millis(300),
      cancellation,
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("was cancelled"));
  assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn agent_surfaces_a_plugin_validation_error() {
  let fixture = RepositoryFixture::new();
  let plugin = fixture.installed_plugin();
  let mut request = fixture.host_request("invalid-source");
  request.revision = "not-an-object-id".to_owned();
  let error = plugin
    .materialize(
      request,
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("full lowercase SHA-1 or SHA-256"));
}

#[cfg(unix)]
#[tokio::test]
async fn agent_enforces_the_source_operation_timeout() {
  use std::os::unix::fs::PermissionsExt as _;

  let fixture = RepositoryFixture::new();
  let slow_git = fixture._temp.path().join("slow-timeout-git");
  fs::write(&slow_git, "#!/bin/sh\nsleep 30\n").unwrap();
  fs::set_permissions(&slow_git, fs::Permissions::from_mode(0o755)).unwrap();
  let mut plugin = fixture.installed_plugin();
  plugin.manifest.settings.insert(
    "git_path".to_owned(),
    serde_json::Value::String(slow_git.to_string_lossy().into_owned()),
  );
  let error = plugin
    .materialize(
      fixture.host_request("timeout-source"),
      Duration::from_millis(100),
      Duration::from_millis(300),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("execution timeout"));
}

#[cfg(unix)]
#[tokio::test]
async fn agent_rejects_a_handshake_that_differs_from_the_manifest() {
  use std::os::unix::fs::PermissionsExt as _;

  let fixture = RepositoryFixture::new();
  let wrong_plugin = fixture._temp.path().join("wrong-plugin");
  fs::write(
    &wrong_plugin,
    "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"hello\",\"protocol_version\":1,\"plugin_name\":\"other\",\"plugin_version\":\"0.1.0\"}'\n",
  )
  .unwrap();
  fs::set_permissions(&wrong_plugin, fs::Permissions::from_mode(0o755)).unwrap();
  let mut plugin = fixture.installed_plugin();
  plugin.executable = wrong_plugin;
  let error = plugin
    .materialize(
      fixture.host_request("wrong-hello"),
      Duration::from_secs(2),
      Duration::from_millis(300),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("hello does not match"));
}

#[cfg(unix)]
#[tokio::test]
async fn source_deadline_includes_the_plugin_handshake() {
  use std::{os::unix::fs::PermissionsExt as _, time::Instant};

  let fixture = RepositoryFixture::new();
  let silent_plugin = fixture._temp.path().join("silent-plugin");
  fs::write(&silent_plugin, "#!/bin/sh\nsleep 30\n").unwrap();
  fs::set_permissions(&silent_plugin, fs::Permissions::from_mode(0o755)).unwrap();
  let mut plugin = fixture.installed_plugin();
  plugin.executable = silent_plugin;
  let started = Instant::now();
  let error = plugin
    .materialize(
      fixture.host_request("handshake-timeout"),
      Duration::from_millis(80),
      Duration::from_millis(200),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("execution timeout"));
  assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[tokio::test]
async fn terminal_plugin_cannot_leave_a_descendant_holding_stderr_open() {
  use std::{os::unix::fs::PermissionsExt as _, time::Instant};

  let fixture = RepositoryFixture::new();
  let plugin_script = fixture._temp.path().join("background-plugin");
  let script = format!(
    "#!/bin/sh\n\
     printf '%s\\n' '{{\"type\":\"hello\",\"protocol_version\":1,\"plugin_name\":\"git\",\"plugin_version\":\"{}\"}}'\n\
     IFS= read -r request\n\
     printf '%s\\n' '{{\"type\":\"accepted\",\"request_id\":\"background-source\"}}'\n\
     (sleep 30) >&2 &\n\
     printf '%s\\n' '{{\"type\":\"finished\",\"request_id\":\"background-source\",\"revision\":\"{}\",\"provenance\":{{}}}}'\n",
    env!("CARGO_PKG_VERSION"),
    fixture.revision,
  );
  fs::write(&plugin_script, script).unwrap();
  fs::set_permissions(&plugin_script, fs::Permissions::from_mode(0o755)).unwrap();
  let mut plugin = fixture.installed_plugin();
  plugin.executable = plugin_script;
  let started = Instant::now();
  let result = plugin
    .materialize(
      fixture.host_request("background-source"),
      Duration::from_secs(2),
      Duration::from_millis(200),
      CancellationToken::new(),
    )
    .await;

  assert!(result.is_ok(), "terminal plugin should be reaped cleanly: {result:?}");
  // Coverage instrumentation and loaded CI hosts can make process-group
  // teardown noticeably slower, but it must still be bounded far below the
  // descendant's 30-second sleep.
  assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn agent_rejects_zero_lifecycle_timeouts_without_starting_a_plugin() {
  let fixture = RepositoryFixture::new();
  let plugin = fixture.installed_plugin();
  let error = plugin
    .materialize(
      fixture.host_request("zero-timeout"),
      Duration::ZERO,
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("must be greater than zero"));
}

#[tokio::test]
async fn agent_rejects_an_invalid_request_id_without_starting_a_plugin() {
  let fixture = RepositoryFixture::new();
  let mut plugin = fixture.installed_plugin();
  plugin.executable = fixture._temp.path().join("missing-plugin");
  let error = plugin
    .materialize(
      fixture.host_request(""),
      Duration::from_secs(1),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();

  assert!(error.to_string().contains("request_id must contain"));
}

fn find_git() -> PathBuf {
  let executable = if cfg!(windows) { "git.exe" } else { "git" };
  std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
    .map(|directory| directory.join(executable))
    .find(|candidate| candidate.is_file())
    .expect("Git must be installed for the source-plugin integration tests")
}

fn git_command<const N: usize>(git: &Path, directory: &Path, arguments: [&str; N]) {
  let status = Command::new(git)
    .args(arguments)
    .current_dir(directory)
    .status()
    .unwrap();
  assert!(status.success());
}

fn git_output<const N: usize>(git: &Path, directory: &Path, arguments: [&str; N]) -> String {
  let output = Command::new(git)
    .args(arguments)
    .current_dir(directory)
    .output()
    .unwrap();
  assert!(output.status.success());
  String::from_utf8(output.stdout).unwrap()
}

fn git_success<const N: usize>(git: &Path, directory: &Path, arguments: [&str; N]) -> bool {
  Command::new(git)
    .args(arguments)
    .current_dir(directory)
    .status()
    .unwrap()
    .success()
}
