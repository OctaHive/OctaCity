//! Exercise the file adapter's lock scope and the public publisher without scheduler races.
use crate::{
  GithubCredentialClass, GithubCredentialManager, GithubCredentialTarget, credential_files::CredentialDirectory,
};
use octacity_server_secrets::{
  CredentialPublication, LogicalSecretReference, ProviderCredentialError, ProviderCredentialPublisher, SecretProviderId,
};
use std::{fs, os::unix::fs::PermissionsExt as _, path::Path};

#[tokio::test]
async fn completed_credential_scope_releases_lock_while_a_duplicate_descriptor_stays_open() {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  fs::create_dir(root.path().join("credentials")).unwrap();
  fs::set_permissions(root.path().join("credentials"), fs::Permissions::from_mode(0o700)).unwrap();
  let reference = LogicalSecretReference::new(SecretProviderId::new("installed").unwrap(), "reader").unwrap();
  let publisher = GithubCredentialManager::new(
    root.path(),
    Path::new("credentials"),
    vec![GithubCredentialTarget {
      reference: reference.clone(),
      handle: "private-reader".into(),
      class: GithubCredentialClass::TaskRead,
      repositories: vec!["installed-tickets".into()],
    }],
  )
  .unwrap();
  let stamp = CredentialPublication::new(reference, [31; 32], 1).unwrap();
  let directory = CredentialDirectory::new(root.path(), Path::new("credentials")).unwrap();
  let scope = directory.lock().unwrap();
  // dup and fork retain the same open file description. Keep that OS reference
  // alive deliberately, rather than hoping another test spawns at the right time.
  let inherited = rustix::io::dup(&scope).unwrap();
  assert_eq!(publisher.discard(&stamp).await, Err(ProviderCredentialError::Conflict));
  drop(scope);
  assert_eq!(
    publisher.discard(&stamp).await,
    Ok(()),
    "a completed scope must release its lock before the duplicate closes"
  );
  assert_eq!(publisher.discard(&stamp).await, Ok(()), "cleanup remains idempotent");
  drop(inherited);
}
