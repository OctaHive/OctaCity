use std::{
  fs,
  path::{Path, PathBuf},
  process::Command,
};

use octacity_private_fs::create_private_directory;
use octacity_protocol::{
  ChangeSetBinaryPolicyV3, ChangeSetCapturePolicyV3, ChangeSetCaptureV3, ChangeSetSecretPatternV3,
  FactoryImmutableReferenceV3,
};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::{
  CaptureError, CapturePolicyViolation, CaptureRequest, ChangeSetCapturer as _, ChangeSetMaterializer as _,
  GitCaptureTool, GitChangeSetCapturer, GitChangeSetMaterializer, MaterializationRequest,
};

#[tokio::test]
async fn creates_reproducible_hook_free_bundle_manifest_patch_and_provenance() {
  let git = git_executable();
  let root = TempDir::new().unwrap();
  let origin = root.path().join("origin");
  fs::create_dir(&origin).unwrap();
  git_run(&git, &origin, &["init", "--quiet", "--initial-branch=main"]);
  fs::write(origin.join("existing.txt"), "base\n").unwrap();
  git_run(&git, &origin, &["add", "--all"]);
  git_run_with_identity(&git, &origin, &["commit", "--quiet", "-m", "base"]);
  let workspace = root.path().join("workspace");
  assert!(
    git_command(&git)
      .args(["clone", "--quiet", "--depth=1", "--no-local"])
      .arg(&origin)
      .arg(&workspace)
      .status()
      .unwrap()
      .success()
  );
  assert!(workspace.join(".git/shallow").is_file());
  let base = git_output(&git, &workspace, &["rev-parse", "HEAD"]);

  let hooks = workspace.join(".git/hooks");
  let hook = hooks.join("pre-commit");
  fs::write(&hook, "#!/bin/sh\nexit 93\n").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
  }
  fs::write(workspace.join("existing.txt"), "candidate\n").unwrap();
  fs::write(workspace.join("new.txt"), "new\n").unwrap();

  let instruction = ChangeSetCaptureV3 {
    author_name: "OctaCity Factory".to_owned(),
    author_email: "factory@octacity.invalid".to_owned(),
    committed_at: 1_767_225_600,
    patch_max_bytes: Some(64 * 1024),
    policy: policy(),
  };
  let capturer = GitChangeSetCapturer::new(GitCaptureTool {
    executable: git.clone(),
    identity: FactoryImmutableReferenceV3 {
      identity: "git".to_owned(),
      version: git_output(&git, &workspace, &["--version"]),
      sha256: digest(&git),
    },
  })
  .unwrap();

  let first_dir = root.path().join("first");
  fs::create_dir(&first_dir).unwrap();
  let first = capturer
    .capture(
      CaptureRequest {
        workspace: workspace.clone(),
        destination: first_dir.clone(),
        base_revision: base.clone(),
        bundle_base_revision: base.clone(),
        stage_attempt_id: "00000000-0000-0000-0000-000000000003".to_owned(),
        instruction: instruction.clone(),
      },
      CancellationToken::new(),
    )
    .await
    .unwrap();

  let second_dir = root.path().join("second");
  fs::create_dir(&second_dir).unwrap();
  let second = capturer
    .capture(
      CaptureRequest {
        workspace: workspace.clone(),
        destination: second_dir.clone(),
        base_revision: base.clone(),
        bundle_base_revision: base.clone(),
        stage_attempt_id: "00000000-0000-0000-0000-000000000003".to_owned(),
        instruction: instruction.clone(),
      },
      CancellationToken::new(),
    )
    .await
    .unwrap();

  assert_eq!(first.manifest, second.manifest);
  assert_eq!(first.manifest_file.sha256, second.manifest_file.sha256);
  assert_eq!(first.manifest.base_revision, base);
  assert_eq!(first.manifest.changed_paths.len(), 2);
  assert_eq!(first.manifest.changed_paths[0].path, "existing.txt");
  assert_eq!(first.manifest.changed_paths[0].new_mode, "100644");
  assert_eq!(first.manifest.changed_paths[1].path, "new.txt");
  assert_eq!(first.manifest.capture_tool.identity, "git");
  assert!(first.manifest.bundle.size_bytes > 0);
  assert!(first.manifest.patch.as_ref().unwrap().size_bytes > 0);
  assert_eq!(git_output(&git, &workspace, &["rev-parse", "HEAD"]), base);
  git_run(
    &git,
    &workspace,
    &[
      "bundle",
      "verify",
      first_dir.join("change-set.bundle").to_str().unwrap(),
    ],
  );
  let inspection = root.path().join("inspection");
  git_run(
    &git,
    root.path(),
    &[
      "clone",
      "--quiet",
      "--no-local",
      workspace.to_str().unwrap(),
      inspection.to_str().unwrap(),
    ],
  );
  git_run(
    &git,
    &inspection,
    &[
      "fetch",
      "--quiet",
      first_dir.join("change-set.bundle").to_str().unwrap(),
      "refs/octacity/capture/00000000-0000-0000-0000-000000000003",
    ],
  );
  assert_eq!(
    git_output(
      &git,
      &inspection,
      &["show", "-s", "--format=%an|%ae|%at|%P", "FETCH_HEAD"]
    ),
    format!("OctaCity Factory|factory@octacity.invalid|1767225600|{base}")
  );
  assert!(!first_dir.join("capture.git").exists());
}

#[tokio::test]
async fn rematerializes_exact_candidate_lineage_from_original_base_without_remote_authority() {
  let fixture = CaptureFixture::new();
  fs::remove_file(fixture.workspace.join("existing.txt")).unwrap();
  fs::create_dir_all(fixture.workspace.join("nested/deep")).unwrap();
  fs::write(fixture.workspace.join("rename-source.txt"), "rename me\n").unwrap();
  git_run(&fixture.git, &fixture.workspace, &["add", "rename-source.txt"]);
  git_run_with_identity(
    &fixture.git,
    &fixture.workspace,
    &["commit", "--quiet", "-m", "fixture predecessor"],
  );
  let predecessor = git_output(&fixture.git, &fixture.workspace, &["rev-parse", "HEAD"]);
  // Capture requires the signed base to be the checked-out commit. This
  // fixture models a rename in the candidate while retaining a single parent.
  fs::rename(
    fixture.workspace.join("rename-source.txt"),
    fixture.workspace.join("nested/deep/renamed.txt"),
  )
  .unwrap();
  fs::write(fixture.workspace.join("nested/deep/binary.bin"), [0, 1, 2, 255]).unwrap();
  let executable = fixture.workspace.join("nested/tool.sh");
  fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
  }
  let mut capture_policy = policy();
  capture_policy.binary_policy = ChangeSetBinaryPolicyV3::Allow;
  let captured = fixture
    .capture("round-trip", &predecessor, capture_policy)
    .await
    .unwrap();
  let bundle = captured.root.join("change-set.bundle");
  let manifest = captured.root.join("change-set-manifest.json");
  assert_eq!(
    git_output(&fixture.git, &fixture.workspace, &["rev-parse", "HEAD"]),
    predecessor
  );

  let materializer = GitChangeSetMaterializer::new(GitCaptureTool {
    executable: fixture.git.clone(),
    identity: captured.manifest.capture_tool.clone(),
  })
  .unwrap();
  for name in ["first-materialization", "second-materialization"] {
    let workspace = fixture._root.path().join(name);
    create_private_directory(&workspace).unwrap();
    git_run(
      &fixture.git,
      fixture._root.path(),
      &[
        "clone",
        "--quiet",
        "--no-local",
        fixture.workspace.to_str().unwrap(),
        workspace.to_str().unwrap(),
      ],
    );
    git_run(&fixture.git, &workspace, &["reset", "--hard", "--quiet", &fixture.base]);
    let hook = workspace.join(".git/hooks/post-checkout");
    fs::write(&hook, "#!/bin/sh\nexit 97\n").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let scratch = fixture._root.path().join(format!("{name}-scratch"));
    create_private_directory(&scratch).unwrap();
    let result = materializer
      .materialize(
        MaterializationRequest {
          workspace: workspace.clone(),
          bundle: bundle.clone(),
          manifest: manifest.clone(),
          expected_base_revision: fixture.base.clone(),
          expected_candidate_revision: captured.manifest.candidate_revision.clone(),
          scratch,
        },
        CancellationToken::new(),
      )
      .await
      .unwrap();
    assert_eq!(result.base_revision, fixture.base);
    assert_eq!(result.candidate_revision, captured.manifest.candidate_revision);
    assert_eq!(
      git_output(&fixture.git, &workspace, &["rev-parse", "HEAD"]),
      result.candidate_revision
    );
    assert!(!workspace.join("existing.txt").exists());
    assert!(!workspace.join("rename-source.txt").exists());
    assert_eq!(
      fs::read_to_string(workspace.join("nested/deep/renamed.txt")).unwrap(),
      "rename me\n"
    );
    assert_eq!(
      fs::read(workspace.join("nested/deep/binary.bin")).unwrap(),
      [0, 1, 2, 255]
    );
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      assert_eq!(
        fs::metadata(workspace.join("nested/tool.sh"))
          .unwrap()
          .permissions()
          .mode()
          & 0o111,
        0o111
      );
    }
  }
  assert_eq!(
    git_output(&fixture.git, &fixture.workspace, &["branch", "--list"]),
    "* main"
  );
}

fn policy() -> ChangeSetCapturePolicyV3 {
  ChangeSetCapturePolicyV3 {
    allowed_path_prefixes: vec![".".to_owned()],
    forbidden_control_paths: vec![".github/workflows".to_owned()],
    max_changed_paths: 64,
    max_file_bytes: 1024 * 1024,
    max_total_bytes: 4 * 1024 * 1024,
    allow_empty: false,
    binary_policy: ChangeSetBinaryPolicyV3::Reject,
    forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
  }
}

#[tokio::test]
async fn rejects_base_mismatch_empty_changes_and_disallowed_paths_without_capture_products() {
  let fixture = CaptureFixture::new();
  fixture
    .reject(
      "base-mismatch",
      "0000000000000000000000000000000000000000",
      policy(),
      CapturePolicyViolation::BaseMismatch,
    )
    .await;
  fixture
    .reject("empty", &fixture.base, policy(), CapturePolicyViolation::EmptyChange)
    .await;

  fs::write(fixture.workspace.join("outside.txt"), "candidate\n").unwrap();
  let mut restricted = policy();
  restricted.allowed_path_prefixes = vec!["src".to_owned()];
  fixture
    .reject("path", &fixture.base, restricted, CapturePolicyViolation::Path)
    .await;
  fixture.reset();

  let control = fixture.workspace.join(".github/workflows");
  fs::create_dir_all(&control).unwrap();
  fs::write(control.join("factory.yml"), "permissions: write-all\n").unwrap();
  fixture
    .reject("control", &fixture.base, policy(), CapturePolicyViolation::ControlFile)
    .await;
}

#[tokio::test]
async fn enforces_changed_file_count_size_aggregate_binary_and_secret_bounds() {
  let fixture = CaptureFixture::new();
  fs::write(fixture.workspace.join("one.txt"), "1").unwrap();
  fs::write(fixture.workspace.join("two.txt"), "2").unwrap();
  let mut bounded = policy();
  bounded.max_changed_paths = 1;
  fixture
    .reject(
      "count",
      &fixture.base,
      bounded,
      CapturePolicyViolation::ChangedPathCount,
    )
    .await;
  fixture.reset();

  fs::write(fixture.workspace.join("large.txt"), "12345").unwrap();
  let mut bounded = policy();
  bounded.max_file_bytes = 4;
  bounded.max_total_bytes = 8;
  fixture
    .reject("file-size", &fixture.base, bounded, CapturePolicyViolation::FileSize)
    .await;
  fixture.reset();

  fs::write(fixture.workspace.join("one.txt"), "1234").unwrap();
  fs::write(fixture.workspace.join("two.txt"), "5678").unwrap();
  let mut bounded = policy();
  bounded.max_file_bytes = 4;
  bounded.max_total_bytes = 6;
  fixture
    .reject("total-size", &fixture.base, bounded, CapturePolicyViolation::TotalBytes)
    .await;
  fixture.reset();

  fs::write(fixture.workspace.join("binary.bin"), [0, 1, 2, 3]).unwrap();
  fixture
    .reject("binary", &fixture.base, policy(), CapturePolicyViolation::Binary)
    .await;
  fixture.reset();

  fs::write(
    fixture.workspace.join("credential.txt"),
    "github_pat_0123456789abcdefghij\n",
  )
  .unwrap();
  fixture
    .reject("secret", &fixture.base, policy(), CapturePolicyViolation::Secret)
    .await;
}

#[tokio::test]
async fn rejects_a_patch_that_would_disclose_removed_secret_material() {
  let fixture = CaptureFixture::new();
  fs::write(
    fixture.workspace.join("credential.txt"),
    "github_pat_0123456789abcdefghij\n",
  )
  .unwrap();
  git_run(&fixture.git, &fixture.workspace, &["add", "credential.txt"]);
  git_run_with_identity(
    &fixture.git,
    &fixture.workspace,
    &["commit", "--quiet", "-m", "secret predecessor"],
  );
  let predecessor = git_output(&fixture.git, &fixture.workspace, &["rev-parse", "HEAD"]);
  fs::remove_file(fixture.workspace.join("credential.txt")).unwrap();

  fixture
    .reject("removed-secret", &predecessor, policy(), CapturePolicyViolation::Secret)
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn ignores_untrusted_local_git_filters_during_capture() {
  use std::os::unix::fs::PermissionsExt as _;

  let fixture = CaptureFixture::new();
  let marker = fixture._root.path().join("filter-executed");
  let filter = fixture._root.path().join("filter.sh");
  fs::write(&filter, format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display())).unwrap();
  fs::set_permissions(&filter, fs::Permissions::from_mode(0o755)).unwrap();
  git_run(
    &fixture.git,
    &fixture.workspace,
    &["config", "filter.octacity-host-escape.clean", filter.to_str().unwrap()],
  );
  fs::write(
    fixture.workspace.join(".gitattributes"),
    "*.txt filter=octacity-host-escape\n",
  )
  .unwrap();
  fs::write(fixture.workspace.join("existing.txt"), "candidate\n").unwrap();

  fixture
    .capture("untrusted-filter", &fixture.base, policy())
    .await
    .unwrap();
  assert!(
    !marker.exists(),
    "capture executed a repository-controlled clean filter"
  );
}

#[tokio::test]
async fn permits_bounded_binary_content_only_when_explicitly_authorized() {
  let fixture = CaptureFixture::new();
  fs::write(fixture.workspace.join("binary.bin"), [0, 1, 2, 3]).unwrap();
  let mut policy = policy();
  policy.binary_policy = ChangeSetBinaryPolicyV3::Allow;
  let captured = fixture.capture("binary-allowed", &fixture.base, policy).await.unwrap();

  assert_eq!(captured.manifest.changed_paths[0].path, "binary.bin");
  assert!(captured.manifest.patch.is_some());
}

#[tokio::test]
async fn permits_an_empty_candidate_only_when_explicitly_authorized() {
  let fixture = CaptureFixture::new();
  let mut policy = policy();
  policy.allow_empty = true;
  let captured = fixture.capture("empty-allowed", &fixture.base, policy).await.unwrap();

  assert!(captured.manifest.changed_paths.is_empty());
  assert_ne!(captured.manifest.candidate_revision, fixture.base);
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_escaping_symlinks() {
  use std::os::unix::fs::symlink;

  let fixture = CaptureFixture::new();
  symlink("../../outside", fixture.workspace.join("escape")).unwrap();
  fixture
    .reject("symlink", &fixture.base, policy(), CapturePolicyViolation::Symlink)
    .await;
}

#[tokio::test]
async fn rejects_nested_repositories() {
  let fixture = CaptureFixture::new();
  let nested = fixture.workspace.join("nested");
  fs::create_dir(&nested).unwrap();
  git_run(&fixture.git, &nested, &["init", "--quiet", "--initial-branch=main"]);
  fs::write(nested.join("nested.txt"), "nested\n").unwrap();
  git_run(&fixture.git, &nested, &["add", "--all"]);
  git_run_with_identity(&fixture.git, &nested, &["commit", "--quiet", "-m", "nested"]);
  fixture
    .reject("submodule", &fixture.base, policy(), CapturePolicyViolation::Submodule)
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn accepts_portable_executable_modes_and_repository_local_symlinks() {
  use std::os::unix::fs::{PermissionsExt as _, symlink};

  let fixture = CaptureFixture::new();
  let script = fixture.workspace.join("script.sh");
  fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
  fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
  symlink("existing.txt", fixture.workspace.join("existing-link")).unwrap();

  let captured = fixture
    .capture("portable-modes", &fixture.base, policy())
    .await
    .unwrap();
  assert_eq!(captured.manifest.changed_paths.len(), 2);
  assert_eq!(captured.manifest.changed_paths[0].new_mode, "120000");
  assert_eq!(captured.manifest.changed_paths[1].new_mode, "100755");
}

struct CaptureFixture {
  _root: TempDir,
  git: PathBuf,
  workspace: PathBuf,
  base: String,
  capturer: GitChangeSetCapturer,
}

impl CaptureFixture {
  fn new() -> Self {
    let git = git_executable();
    let root = TempDir::new().unwrap();
    let origin = root.path().join("origin");
    fs::create_dir(&origin).unwrap();
    git_run(&git, &origin, &["init", "--quiet", "--initial-branch=main"]);
    fs::write(origin.join("existing.txt"), "base\n").unwrap();
    git_run(&git, &origin, &["add", "--all"]);
    git_run_with_identity(&git, &origin, &["commit", "--quiet", "-m", "base"]);
    let workspace = root.path().join("workspace");
    git_run(
      &git,
      root.path(),
      &[
        "clone",
        "--quiet",
        "--no-local",
        origin.to_str().unwrap(),
        workspace.to_str().unwrap(),
      ],
    );
    let base = git_output(&git, &workspace, &["rev-parse", "HEAD"]);
    let capturer = GitChangeSetCapturer::new(GitCaptureTool {
      executable: git.clone(),
      identity: FactoryImmutableReferenceV3 {
        identity: "git".to_owned(),
        version: git_output(&git, &workspace, &["--version"]),
        sha256: digest(&git),
      },
    })
    .unwrap();
    Self {
      _root: root,
      git,
      workspace,
      base,
      capturer,
    }
  }

  async fn capture(
    &self,
    name: &str,
    base_revision: &str,
    policy: ChangeSetCapturePolicyV3,
  ) -> Result<super::CapturedChangeSet, CaptureError> {
    let destination = self._root.path().join(name);
    fs::create_dir(&destination).unwrap();
    self
      .capturer
      .capture(
        CaptureRequest {
          workspace: self.workspace.clone(),
          destination,
          base_revision: base_revision.to_owned(),
          bundle_base_revision: self.base.clone(),
          stage_attempt_id: "00000000-0000-0000-0000-000000000003".to_owned(),
          instruction: ChangeSetCaptureV3 {
            author_name: "OctaCity Factory".to_owned(),
            author_email: "factory@octacity.invalid".to_owned(),
            committed_at: 1_767_225_600,
            patch_max_bytes: Some(64 * 1024),
            policy,
          },
        },
        CancellationToken::new(),
      )
      .await
  }

  async fn reject(
    &self,
    name: &str,
    base_revision: &str,
    policy: ChangeSetCapturePolicyV3,
    expected: CapturePolicyViolation,
  ) {
    let destination = self._root.path().join(name);
    let error = self.capture(name, base_revision, policy).await.unwrap_err();
    assert!(matches!(error, CaptureError::Policy(actual) if actual == expected));
    assert!(!destination.join("change-set-manifest.json").exists());
    assert!(!destination.join("change-set.bundle").exists());
  }

  fn reset(&self) {
    git_run(&self.git, &self.workspace, &["reset", "--hard", "--quiet", "HEAD"]);
    git_run(&self.git, &self.workspace, &["clean", "-dffx", "--quiet"]);
  }
}

fn git_executable() -> PathBuf {
  let names: &[&str] = if cfg!(windows) { &["git.exe", "git"] } else { &["git"] };
  std::env::var_os("PATH")
    .into_iter()
    .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
    .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
    .find(|candidate| candidate.is_file())
    .and_then(|candidate| candidate.canonicalize().ok())
    .expect("Git must be installed for ChangeSet capture tests")
}

fn git_run(git: &Path, workspace: &Path, arguments: &[&str]) {
  assert!(
    git_command(git)
      .args(arguments)
      .current_dir(workspace)
      .status()
      .unwrap()
      .success()
  );
}

fn git_run_with_identity(git: &Path, workspace: &Path, arguments: &[&str]) {
  assert!(
    git_command(git)
      .args(arguments)
      .current_dir(workspace)
      .env("GIT_AUTHOR_NAME", "Fixture")
      .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
      .env("GIT_COMMITTER_NAME", "Fixture")
      .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
      .status()
      .unwrap()
      .success()
  );
}

fn git_output(git: &Path, workspace: &Path, arguments: &[&str]) -> String {
  let output = git_command(git)
    .args(arguments)
    .current_dir(workspace)
    .output()
    .unwrap();
  assert!(output.status.success());
  String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn git_command(git: &Path) -> Command {
  let mut command = Command::new(git);
  command
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", super::null_device())
    .env("GIT_TERMINAL_PROMPT", "0")
    .arg("-c")
    .arg("core.autocrlf=false");
  command
}

fn digest(path: &Path) -> String {
  hex::encode(Sha256::digest(fs::read(path).unwrap()))
}
