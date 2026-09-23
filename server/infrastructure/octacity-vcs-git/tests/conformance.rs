use std::{
  ffi::OsString,
  fs,
  io::Write as _,
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_vcs_git::GitVcsAdapter;
use octacity_vcs_protocol::{
  Command as VcsCommand, ListReferences, ListTree, Outcome, ReadCommit, ReadFile, ReferenceKind, RepositoryAccess,
  Request, ResolveRevision, TreeEntryKind, VCS_PROTOCOL_VERSION, decode_response,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

struct Fixture {
  _root: TempDir,
  adapter_directory: PathBuf,
  source: PathBuf,
  marker: PathBuf,
  revision: String,
}

#[tokio::test]
async fn lists_bounded_branch_and_tag_pages_and_resolves_immutable_commits() {
  let fixture = fixture();
  let adapter = GitVcsAdapter::load(&fixture.adapter_directory).unwrap();
  let first = request(
    "refs-1",
    VcsCommand::ListReferences(ListReferences {
      repository: access(&fixture, "refs-1"),
      cursor: None,
      page_size: 2,
      prefix: None,
    }),
  );
  let Outcome::References(first_page) = adapter.execute(&first, CancellationToken::new()).await else {
    panic!("reference listing failed")
  };
  assert_eq!(first_page.entries.len(), 2);
  let cursor = first_page.next_cursor.clone().expect("another reference page");

  let second = request(
    "refs-2",
    VcsCommand::ListReferences(ListReferences {
      repository: access(&fixture, "refs-2"),
      cursor: Some(cursor.clone()),
      page_size: 2,
      prefix: None,
    }),
  );
  let Outcome::References(second_page) = adapter.execute(&second, CancellationToken::new()).await else {
    panic!("second reference page failed")
  };
  assert!(second_page.entries.iter().all(|entry| entry.name > cursor));
  assert!(
    first_page
      .entries
      .iter()
      .chain(&second_page.entries)
      .any(|entry| entry.kind == ReferenceKind::Branch)
  );
  assert!(
    first_page
      .entries
      .iter()
      .chain(&second_page.entries)
      .any(|entry| entry.kind == ReferenceKind::Tag)
  );

  for reference in ["refs/heads/main", "refs/tags/v1"] {
    let request = request(
      reference,
      VcsCommand::ResolveRevision(ResolveRevision {
        repository: access(&fixture, reference),
        reference: reference.to_owned(),
      }),
    );
    let Outcome::Resolved(resolved) = adapter.execute(&request, CancellationToken::new()).await else {
      panic!("revision resolution failed")
    };
    assert_eq!(resolved.revision, fixture.revision);
  }

  fs::write(fixture.source.join("README.md"), "branch moved\n").unwrap();
  run_git(&fixture.source, ["add", "README.md"]);
  run_git(&fixture.source, ["commit", "-m", "move branch"]);
  let moved_revision = git_output(&fixture.source, ["rev-parse", "HEAD"]);
  let moved = request(
    "moved-main",
    VcsCommand::ResolveRevision(ResolveRevision {
      repository: access(&fixture, "moved-main"),
      reference: "refs/heads/main".to_owned(),
    }),
  );
  let Outcome::Resolved(moved) = adapter.execute(&moved, CancellationToken::new()).await else {
    panic!("moved revision resolution failed")
  };
  assert_eq!(moved.revision, moved_revision);
  assert_ne!(moved.revision, fixture.revision);
}

#[tokio::test]
async fn reads_commits_trees_and_bounded_binary_file_fragments_without_checkout() {
  let fixture = fixture();
  let adapter = GitVcsAdapter::load(&fixture.adapter_directory).unwrap();
  let commit_request = request(
    "commit",
    VcsCommand::ReadCommit(ReadCommit {
      repository: access(&fixture, "commit"),
      revision: fixture.revision.clone(),
    }),
  );
  let Outcome::Commit(commit) = adapter.execute(&commit_request, CancellationToken::new()).await else {
    panic!("commit read failed")
  };
  assert_eq!(commit.revision, fixture.revision);
  assert!(commit.message.contains("initial commit"));
  assert_eq!(
    commit.author_display.as_deref(),
    Some("OctaCity Test <test@example.invalid>")
  );

  let tree_request = request(
    "tree-1",
    VcsCommand::ListTree(ListTree {
      repository: access(&fixture, "tree-1"),
      revision: fixture.revision.clone(),
      path: None,
      cursor: None,
      page_size: 1,
    }),
  );
  let Outcome::Tree(first_tree_page) = adapter.execute(&tree_request, CancellationToken::new()).await else {
    panic!("tree read failed")
  };
  assert_eq!(first_tree_page.entries.len(), 1);
  assert!(first_tree_page.next_cursor.is_some());

  let nested_request = request(
    "tree-src",
    VcsCommand::ListTree(ListTree {
      repository: access(&fixture, "tree-src"),
      revision: fixture.revision.clone(),
      path: Some("src".to_owned()),
      cursor: None,
      page_size: 20,
    }),
  );
  let Outcome::Tree(nested) = adapter.execute(&nested_request, CancellationToken::new()).await else {
    panic!("nested tree read failed")
  };
  assert!(nested.entries.iter().any(|entry| {
    entry.path == "src/data.bin" && entry.kind == TreeEntryKind::File && entry.size_bytes == Some(10)
  }));

  #[cfg(unix)]
  {
    let root_request = request(
      "tree-root",
      VcsCommand::ListTree(ListTree {
        repository: access(&fixture, "tree-root"),
        revision: fixture.revision.clone(),
        path: None,
        cursor: None,
        page_size: 20,
      }),
    );
    let Outcome::Tree(root) = adapter.execute(&root_request, CancellationToken::new()).await else {
      panic!("root tree read failed")
    };
    assert!(
      root
        .entries
        .iter()
        .any(|entry| entry.path == "readme-link" && entry.kind == TreeEntryKind::Symlink)
    );
  }

  let file_request = request(
    "file",
    VcsCommand::ReadFile(ReadFile {
      repository: access(&fixture, "file"),
      revision: fixture.revision.clone(),
      path: "src/data.bin".to_owned(),
      offset: 3,
      max_bytes: 4,
    }),
  );
  let Outcome::File(file) = adapter.execute(&file_request, CancellationToken::new()).await else {
    panic!("file read failed")
  };
  assert_eq!(STANDARD.decode(file.content_base64).unwrap(), [3, 4, 5, 6]);
  assert!(file.truncated);
  assert!(!fixture.marker.exists(), "repository hook was executed");
  assert_eq!(
    directory_names(&fixture.adapter_directory),
    ["credentials", "git-adapter.toml"]
  );
}

#[test]
fn process_contract_returns_one_correlated_bounded_response() {
  let fixture = fixture();
  let request = request(
    "process-resolve",
    VcsCommand::ResolveRevision(ResolveRevision {
      repository: access(&fixture, "process-resolve"),
      reference: "refs/heads/main".to_owned(),
    }),
  );
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-vcs-git"))
    .current_dir(&fixture.adapter_directory)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let mut frame = serde_json::to_vec(&request).unwrap();
  frame.push(b'\n');
  child.stdin.take().unwrap().write_all(&frame).unwrap();
  let output = child.wait_with_output().unwrap();
  assert!(
    output.status.success(),
    "adapter stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(output.stderr.is_empty());
  assert_eq!(output.stdout.iter().filter(|byte| **byte == b'\n').count(), 1);
  let response = decode_response(output.stdout.strip_suffix(b"\n").unwrap(), &request).unwrap();
  assert!(matches!(response.outcome, Outcome::Resolved(value) if value.revision == fixture.revision));
}

#[tokio::test]
async fn rejects_credential_bearing_and_remote_helper_locators_as_data() {
  let fixture = fixture();
  let adapter = GitVcsAdapter::load(&fixture.adapter_directory).unwrap();
  for locator in ["https://user:secret@example.invalid/repo.git", "ext::sh -c malicious"] {
    let mut access = access(&fixture, "invalid");
    access.repository_locator = locator.to_owned();
    let request = request(
      "invalid",
      VcsCommand::ResolveRevision(ResolveRevision {
        repository: access,
        reference: "refs/heads/main".to_owned(),
      }),
    );
    assert!(matches!(
      adapter.execute(&request, CancellationToken::new()).await,
      Outcome::Failure(failure) if failure.class == octacity_vcs_protocol::FailureClass::InvalidRequest
    ));
  }
  let mut missing_credential = access(&fixture, "missing-credential");
  missing_credential.credential_handle = "private-repository-handle".to_owned();
  let request = request(
    "missing-credential",
    VcsCommand::ResolveRevision(ResolveRevision {
      repository: missing_credential,
      reference: "refs/heads/main".to_owned(),
    }),
  );
  let Outcome::Failure(failure) = adapter.execute(&request, CancellationToken::new()).await else {
    panic!("missing credential handle was accepted")
  };
  assert_eq!(failure.class, octacity_vcs_protocol::FailureClass::Permanent);
  assert_eq!(failure.code, "credential_unavailable");
  assert!(!failure.diagnostic.contains("private-repository-handle"));
  assert!(!fixture.marker.exists());
}

fn fixture() -> Fixture {
  let root = tempfile::tempdir().unwrap();
  let git_path = find_git();
  let adapter_directory = root.path().join("adapter");
  let credentials = adapter_directory.join("credentials");
  fs::create_dir_all(&credentials).unwrap();
  private_directory(&adapter_directory);
  private_directory(&credentials);
  let config = adapter_directory.join("git-adapter.toml");
  fs::write(
    &config,
    format!(
      "config_version = 1\ngit_path = {}\ncredential_directory = \"credentials\"\nallow_file = true\nmax_git_output_bytes = 4194304\nmax_repository_bytes = 67108864\nmax_blob_bytes = 8388608\n",
      toml_string(&git_path)
    ),
  )
  .unwrap();
  private_file(&config);

  let source = root.path().join("source");
  run_git(root.path(), [OsString::from("init"), source.as_os_str().to_owned()]);
  run_git(&source, ["config", "user.name", "OctaCity Test"]);
  run_git(&source, ["config", "user.email", "test@example.invalid"]);
  run_git(&source, ["branch", "-M", "main"]);
  fs::create_dir(source.join("src")).unwrap();
  fs::write(source.join("README.md"), "repository content is data\n").unwrap();
  fs::write(source.join("src/data.bin"), (0_u8..10).collect::<Vec<_>>()).unwrap();
  fs::write(source.join("run-me.sh"), "#!/bin/sh\nexit 99\n").unwrap();
  install_repository_symlink(&source);
  let marker = root.path().join("repository-hook-ran");
  install_malicious_hook(&source, &marker);
  run_git(&source, ["add", "."]);
  run_git(&source, ["commit", "-m", "initial commit\n\nwith details"]);
  let revision = git_output(&source, ["rev-parse", "HEAD"]);
  run_git(&source, ["tag", "-a", "v1", "-m", "version one"]);
  run_git(&source, ["branch", "feature"]);
  Fixture {
    _root: root,
    adapter_directory,
    source,
    marker,
    revision,
  }
}

fn access(fixture: &Fixture, operation_id: &str) -> RepositoryAccess {
  RepositoryAccess {
    operation_id: operation_id.to_owned(),
    repository_id: "repository-01".to_owned(),
    repository_locator: fixture.source.display().to_string(),
    credential_handle: "anonymous".to_owned(),
  }
}

fn request(request_id: &str, command: VcsCommand) -> Request {
  Request {
    protocol_version: VCS_PROTOCOL_VERSION,
    request_id: request_id.to_owned(),
    command,
  }
}

fn find_git() -> PathBuf {
  let path = std::env::var_os("PATH").expect("PATH is available to tests");
  let names = if cfg!(windows) {
    vec!["git.exe", "git.cmd", "git.bat"]
  } else {
    vec!["git"]
  };
  for directory in std::env::split_paths(&path) {
    for name in &names {
      let candidate = directory.join(name);
      if candidate.is_file() {
        return candidate.canonicalize().unwrap();
      }
    }
  }
  panic!("Git executable was not found in PATH")
}

fn run_git<I, S>(directory: &Path, arguments: I)
where
  I: IntoIterator<Item = S>,
  S: AsRef<std::ffi::OsStr>,
{
  let output = Command::new(find_git())
    .current_dir(directory)
    .args(arguments)
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "Git failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
}

fn git_output<const N: usize>(directory: &Path, arguments: [&str; N]) -> String {
  let output = Command::new(find_git())
    .current_dir(directory)
    .args(arguments)
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "Git failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn toml_string(path: &Path) -> String {
  serde_json::to_string(&path.display().to_string()).unwrap()
}

fn directory_names(directory: &Path) -> Vec<String> {
  let mut names = fs::read_dir(directory)
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .collect::<Vec<_>>();
  names.sort();
  names
}

#[cfg(unix)]
fn private_directory(path: &Path) {
  use std::os::unix::fs::PermissionsExt as _;
  fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(not(unix))]
fn private_directory(_path: &Path) {}

#[cfg(unix)]
fn private_file(path: &Path) {
  use std::os::unix::fs::PermissionsExt as _;
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn private_file(_path: &Path) {}

#[cfg(unix)]
fn install_malicious_hook(repository: &Path, marker: &Path) {
  use std::os::unix::fs::PermissionsExt as _;
  let hook = repository.join(".git/hooks/post-checkout");
  fs::write(&hook, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
  fs::set_permissions(hook, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
fn install_repository_symlink(repository: &Path) {
  std::os::unix::fs::symlink("README.md", repository.join("readme-link")).unwrap();
}

#[cfg(not(unix))]
fn install_repository_symlink(_repository: &Path) {}

#[cfg(not(unix))]
fn install_malicious_hook(repository: &Path, _marker: &Path) {
  fs::write(
    repository.join(".git/hooks/post-checkout"),
    "repository-controlled hook",
  )
  .unwrap();
}
