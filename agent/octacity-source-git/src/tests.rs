//! Unit tests for Git validation, process limits, and workspace accounting.

use std::io::Write as _;

use super::*;

#[test]
fn validates_remote_transport_policy() {
  assert!(validate_remote("https://example.com/repository.git", false).is_ok());
  assert!(validate_remote("http://example.com/repository.git", false).is_err());
  assert!(validate_remote("https://user@example.com/repository.git", false).is_err());
  assert!(validate_remote("https://example.com/repository.git?token=secret", false).is_err());

  let local = std::env::current_dir().unwrap();
  let local = local.to_str().unwrap();
  assert!(validate_remote(local, false).is_err());
  assert!(validate_remote(local, true).is_ok());
}

#[test]
fn recognizes_only_full_lowercase_object_ids() {
  assert!(is_object_id(&"a".repeat(40)));
  assert!(is_object_id(&"b".repeat(64)));
  assert!(!is_object_id(&"a".repeat(39)));
  assert!(!is_object_id(&"A".repeat(40)));
  assert!(!is_object_id(&"z".repeat(40)));
}

#[test]
fn enforces_workspace_size_without_following_special_entries() {
  let workspace = tempfile::tempdir().unwrap();
  fs::write(workspace.path().join("large"), [0_u8; 32]).unwrap();
  assert!(enforce_workspace_limit(workspace.path(), 32).is_ok());
  assert!(matches!(
    enforce_workspace_limit(workspace.path(), 31),
    Err(GitSourceError::WorkspaceLimit { limit: 31 })
  ));
}

#[test]
fn describes_process_outcomes_and_sanitizes_diagnostics() {
  assert_eq!(describe_outcome(processkit::Outcome::Exited(2)), "exit code 2");
  #[cfg(unix)]
  assert_eq!(describe_outcome(processkit::Outcome::Signalled(Some(9))), "signal 9");
  assert_eq!(sanitize_diagnostic(b"bad\0 message\n"), "bad message");
}

#[cfg(unix)]
#[tokio::test]
async fn bounds_git_output_through_the_process_runner() {
  use std::os::unix::fs::PermissionsExt as _;

  let temp = tempfile::tempdir().unwrap();
  let executable = temp.path().join("git");
  fs::write(&executable, "#!/bin/sh\nprintf 'too much output'\n").unwrap();
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
  let settings = GitSettings {
    git_path: executable,
    allow_file: false,
    max_diagnostic_bytes: 3,
  };

  let error = run_git(
    &settings,
    None,
    temp.path(),
    "test",
    ["--version"],
    &CancellationToken::new(),
  )
  .await
  .unwrap_err();

  assert!(matches!(error, GitSourceError::OutputLimit { step: "test" }));
}

#[test]
fn validates_operator_settings_and_credential_handles() {
  let temp = tempfile::tempdir().unwrap();
  let executable = temp.path().join(if cfg!(windows) { "git.exe" } else { "git" });
  create_executable(&executable);
  let mut settings = GitSettings {
    git_path: executable,
    allow_file: false,
    max_diagnostic_bytes: DEFAULT_DIAGNOSTIC_BYTES,
  };
  assert!(validate_settings(&mut settings).is_ok());
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&settings.git_path, fs::Permissions::from_mode(0o775)).unwrap();
    assert!(validate_settings(&mut settings).is_err());
    fs::set_permissions(&settings.git_path, fs::Permissions::from_mode(0o755)).unwrap();
  }
  settings.max_diagnostic_bytes = 0;
  assert!(validate_settings(&mut settings).is_err());

  assert_eq!(validate_credentials(&BTreeMap::new()).unwrap(), None);
  let unknown = BTreeMap::from([("token".to_owned(), "/credential".to_owned())]);
  assert!(validate_credentials(&unknown).is_err());
  let relative = BTreeMap::from([("git_config".to_owned(), "credential".to_owned())]);
  assert!(validate_credentials(&relative).is_err());

  let credential = temp.path().join("credential");
  fs::write(&credential, "[credential]\nhelper =\n").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
  }
  let configured = BTreeMap::from([("git_config".to_owned(), credential.to_string_lossy().into_owned())]);
  assert_eq!(
    validate_credentials(&configured).unwrap(),
    Some(credential.canonicalize().unwrap())
  );
}

fn create_executable(path: &Path) {
  fs::File::create(path).unwrap().write_all(b"executable").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
  }
}
