#![cfg(unix)]
//! Private JSON credentials are scoped to one execution scratch tree, never a public input.
use octacity_runner::RunnerPrivateJsonFile;
use std::{fs, os::unix::fs::PermissionsExt as _};

#[test]
fn native_client_updates_are_retained_for_the_next_serialized_job_only_when_selected() {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let root = root.path().canonicalize().unwrap();
  let source = root.join("auth.json");
  fs::write(&source, br#"{"tokens":{"access_token":"seed-native-token"}}"#).unwrap();
  fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
  let scratch = root.join("scratch");
  fs::create_dir(&scratch).unwrap();
  fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
  let file = RunnerPrivateJsonFile::load_scoped(&source, "codex/auth.json", &["/tokens".into()]).unwrap();
  let guard = file.publish_persistent(&scratch).unwrap();
  let next_scratch = root.join("next");
  fs::create_dir(&next_scratch).unwrap();
  fs::set_permissions(&next_scratch, fs::Permissions::from_mode(0o700)).unwrap();
  assert!(
    file.publish_persistent(&next_scratch).is_err(),
    "one credential installation has one active native client"
  );
  let rotated = br#"{"tokens":{"access_token":"renewed-native-token"}}"#;
  fs::write(scratch.join(".credentials/codex/auth.json"), rotated).unwrap();
  guard.persist_updates().unwrap();
  assert_eq!(fs::read(&source).unwrap(), rotated);
  assert_eq!(fs::metadata(&source).unwrap().permissions().mode() & 0o777, 0o600);
  drop(guard);
  let next = RunnerPrivateJsonFile::load_scoped(&source, "codex/auth.json", &["/tokens".into()]).unwrap();
  let values = next.redaction_values().collect::<Vec<_>>();
  assert!(values.contains(&b"renewed-native-token".as_slice()));
  assert!(!values.contains(&b"seed-native-token".as_slice()));
  next.publish_persistent(&next_scratch).unwrap();
}

#[test]
fn execution_file_refresh_retains_old_and_new_secrets_and_fails_closed_if_unreadable() {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let source = root.path().canonicalize().unwrap().join("auth.json");
  fs::write(&source, br#"{"tokens":{"access_token":"original-private-token"}}"#).unwrap();
  fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
  let scratch = root.path().canonicalize().unwrap().join("scratch");
  fs::create_dir(&scratch).unwrap();
  fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
  let file = RunnerPrivateJsonFile::load_scoped(&source, "codex/auth.json", &["/tokens".into()]).unwrap();
  let guard = file.publish(&scratch).unwrap();
  let target = scratch.join(".credentials/codex/auth.json");
  fs::write(&target, br#"{"tokens":{"access_token":"rotated-private-token"}}"#).unwrap();
  let values = guard.snapshot().unwrap();
  let values = values.values().collect::<Vec<_>>();
  assert!(values.contains(&b"original-private-token".as_slice()));
  assert!(values.contains(&b"rotated-private-token".as_slice()));
  guard.persist_updates().unwrap();
  assert_eq!(
    fs::read(&source).unwrap(),
    br#"{"tokens":{"access_token":"original-private-token"}}"#,
    "ordinary execution copies have no write-back authority"
  );
  assert!(!format!("{guard:?}").contains("private-token"));
  fs::remove_file(target).unwrap();
  assert!(
    guard.snapshot().is_err(),
    "uninspectable credentials cannot authorize publication"
  );
}

#[test]
fn a_changed_installation_cannot_be_overwritten_by_an_older_job_copy() {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let root = root.path().canonicalize().unwrap();
  let source = root.join("auth.json");
  fs::write(&source, br#"{"tokens":{"access_token":"initial-token"}}"#).unwrap();
  fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
  let scratch = root.join("scratch");
  fs::create_dir(&scratch).unwrap();
  fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
  let file = RunnerPrivateJsonFile::load_scoped(&source, "codex/auth.json", &["/tokens".into()]).unwrap();
  let guard = file.publish_persistent(&scratch).unwrap();
  let replaced = br#"{"tokens":{"access_token":"operator-replacement"}}"#;
  fs::write(&source, replaced).unwrap();
  fs::write(
    scratch.join(".credentials/codex/auth.json"),
    br#"{"tokens":{"access_token":"older-job-refresh"}}"#,
  )
  .unwrap();
  assert!(guard.persist_updates().is_err());
  assert_eq!(fs::read(&source).unwrap(), replaced);
}

#[test]
fn selected_credential_subtree_protects_tokens_without_treating_authentication_metadata_as_secrets() {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let source = root.path().canonicalize().unwrap().join("auth.json");
  fs::write(&source, br#"{"auth_mode":"chatgpt","tokens":{"access_token":"selected-access","refresh_token":"selected-refresh","account_id":"selected-account"}}"#).unwrap();
  fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
  let file = RunnerPrivateJsonFile::load_scoped(&source, "codex/auth.json", &["/tokens".into()]).unwrap();
  let values = file.redaction_values().collect::<Vec<_>>();
  assert!(values.contains(&b"selected-access".as_slice()));
  assert!(values.contains(&b"selected-refresh".as_slice()));
  assert!(values.contains(&b"selected-account".as_slice()));
  assert!(!values.contains(&b"chatgpt".as_slice()));
  assert!(RunnerPrivateJsonFile::load_scoped(&source, "codex/auth.json", &["/missing".into()]).is_err());
}

#[test]
fn selected_json_login_is_copied_privately_without_other_home_files() {
  let operator = tempfile::tempdir().unwrap();
  fs::set_permissions(operator.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let operator_root = operator.path().canonicalize().unwrap();
  let source = operator_root.join("auth.json");
  let login = br#"{"auth_mode":"chatgpt","tokens":{"access_token":"private-access-canary","refresh_token":"private-refresh-canary"}}"#;
  fs::write(&source, login).unwrap();
  fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
  fs::write(operator.path().join("config.toml"), "unselected MCP configuration").unwrap();
  let scratch = tempfile::tempdir().unwrap();
  fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let selected = RunnerPrivateJsonFile::load(&source, "codex/auth.json").unwrap();
  let scratch_root = scratch.path().canonicalize().unwrap();
  selected.publish(&scratch_root).unwrap();
  let target = scratch.path().join(".credentials/codex/auth.json");
  assert_eq!(fs::read(&target).unwrap(), login);
  assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
  assert!(!target.parent().unwrap().join("config.toml").exists());
  assert!(!format!("{selected:?}").contains("private-access-canary"));
  assert!(
    selected.publish(&scratch_root).is_err(),
    "credentials cannot overwrite an existing execution file"
  );
}
