use super::*;
use octacity_github::{GithubCredentialClass, GithubCredentialManager, GithubCredentialTarget};
use octacity_server_secrets::{
  LogicalSecretReference, ProviderCredentialError, ProviderCredentialManager, ProviderCredentialValue, SecretProviderId,
};

#[tokio::test]
async fn credential_publication_crash_writer() {
  use rustix::process::{Resource, Rlimit, setrlimit};
  let Ok(root) = std::env::var("OCTACITY_CREDENTIAL_CRASH_ROOT") else {
    return;
  };
  setrlimit(
    Resource::Core,
    Rlimit {
      current: Some(0),
      maximum: Some(0),
    },
  )
  .unwrap();
  // Recreate the filesystem cut point after the first chunk, before rename.
  // An explicit abrupt stop avoids OS-dependent ordering of EFBIG and SIGXFSZ.
  let operation = hex::encode([31; 32]);
  let staged = format!("octacity-prepared\0private-source-grant\0{operation}");
  let phase = std::env::var("OCTACITY_CREDENTIAL_CRASH_PHASE").unwrap();
  let handle = if phase == "prepare" {
    &staged
  } else {
    "private-source-grant"
  };
  let name = hex::encode(Sha256::digest(handle));
  let suffix = if phase == "legacy" {
    String::new()
  } else {
    format!("-{operation}")
  };
  let path = Path::new(&root)
    .join("credentials")
    .join(format!(".octacity-credential-{name}.json{suffix}.tmp"));
  private_file(
    &path,
    b"{\"class\":\"work_source_read\",\"token\":\"crash-private-token-",
  );
  std::process::abort();
}

#[tokio::test]
async fn cleanup_after_process_death_erases_partial_private_writes() {
  use octacity_server_secrets::{CredentialPublication, ProviderCredentialPublisher};
  use std::os::unix::process::ExitStatusExt as _;
  for phase in ["prepare", "activate", "legacy"] {
    let fixture = Fixture::new(true, false);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
      .args([
        "--exact",
        "management::credential_publication_crash_writer",
        "--nocapture",
      ])
      .env("OCTACITY_CREDENTIAL_CRASH_ROOT", fixture.root.path())
      .env("OCTACITY_CREDENTIAL_CRASH_PHASE", phase)
      .output()
      .unwrap();
    assert!(output.status.signal().is_some(), "child must stop abruptly: {output:?}");
    let directory = fixture.root.path().join("credentials");
    assert!(
      fs::read_dir(&directory).unwrap().any(
        |entry| String::from_utf8_lossy(&fs::read(entry.unwrap().path()).unwrap()).contains("crash-private-token")
      ),
      "the crash must leave actual partial secret bytes before recovery: {output:?}; files: {:?}",
      fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>()
    );
    let stamp = CredentialPublication::new(managed_target().reference, [31; 32], 1).unwrap();
    let publisher = manager(&fixture);
    publisher.discard(&stamp).await.unwrap();
    publisher.discard(&stamp).await.unwrap();
    for entry in fs::read_dir(directory).unwrap() {
      assert!(
        !String::from_utf8_lossy(&fs::read(entry.unwrap().path()).unwrap()).contains("crash-private-token"),
        "bounded cleanup must erase partial {phase} writes after process death"
      );
    }
    fixture.no_provider_call();
  }
}

#[tokio::test]
async fn recoverable_publication_keeps_preparation_private_and_replay_cannot_restore_old_authority() {
  use octacity_server_secrets::{CredentialPublication, CredentialPublicationKind, ProviderCredentialPublisher};
  let fixture = Fixture::new(true, false);
  let reference = managed_target().reference;
  let first = CredentialPublication::new(reference.clone(), [1; 32], 1).unwrap();
  let publisher = manager(&fixture);
  publisher
    .prepare(
      &first,
      CredentialPublicationKind::Provision,
      Some(ProviderCredentialValue::new(b"first-private-token".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  assert!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .prepare(&fixture.request(false))
      .is_err()
  );
  // Reloaded composition recovers the original prepared bytes without resubmitting a secret.
  let publisher = manager(&fixture);
  publisher
    .prepare(&first, CredentialPublicationKind::Provision, None)
    .await
    .unwrap();
  publisher.activate(&first).await.unwrap();
  assert!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .prepare(&fixture.request(false))
      .is_ok()
  );
  let second = CredentialPublication::new(reference.clone(), [2; 32], 2).unwrap();
  publisher
    .prepare(
      &second,
      CredentialPublicationKind::Rotate,
      Some(ProviderCredentialValue::new(b"second-private-token".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  publisher.activate(&second).await.unwrap();
  assert_eq!(
    publisher.activate(&first).await.unwrap_err(),
    ProviderCredentialError::Conflict
  );
  let file = fixture
    .root
    .path()
    .join("credentials")
    .join(format!("{}.json", hex::encode(Sha256::digest("private-source-grant"))));
  assert_eq!(
    serde_json::from_slice::<serde_json::Value>(&fs::read(&file).unwrap()).unwrap()["token"],
    "second-private-token"
  );
  let revoked = CredentialPublication::new(reference.clone(), [3; 32], 3).unwrap();
  publisher
    .prepare(&revoked, CredentialPublicationKind::Revoke, None)
    .await
    .unwrap();
  publisher.activate(&revoked).await.unwrap();
  assert_eq!(
    publisher.activate(&second).await.unwrap_err(),
    ProviderCredentialError::Conflict
  );
  assert!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .prepare(&fixture.request(false))
      .is_err()
  );
  let restored = CredentialPublication::new(reference, [4; 32], 4).unwrap();
  publisher
    .prepare(
      &restored,
      CredentialPublicationKind::Provision,
      Some(ProviderCredentialValue::new(b"explicit-new-authority".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  publisher.activate(&restored).await.unwrap();
  assert!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .prepare(&fixture.request(false))
      .is_ok()
  );
  assert_eq!(
    publisher.activate(&second).await.unwrap_err(),
    ProviderCredentialError::Conflict
  );
  fixture.no_provider_call();
}

#[tokio::test]
async fn abandoned_preparation_is_erased_and_legacy_rotation_cannot_bypass_managed_fencing() {
  use octacity_server_secrets::{
    CredentialPublication, CredentialPublicationKind as Kind, ProviderCredentialPublisher,
  };
  let fixture = Fixture::new(true, false);
  let publisher = manager(&fixture);
  let reference = managed_target().reference;
  let orphan = CredentialPublication::new(reference.clone(), [9; 32], 1).unwrap();
  publisher
    .prepare(
      &orphan,
      Kind::Provision,
      Some(ProviderCredentialValue::new(b"orphan-secret-token".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  publisher.discard(&orphan).await.unwrap();
  publisher.discard(&orphan).await.unwrap();
  assert_eq!(
    publisher
      .prepare(
        &orphan,
        Kind::Provision,
        Some(ProviderCredentialValue::new(b"orphan-secret-token".to_vec()).unwrap())
      )
      .await
      .unwrap_err(),
    ProviderCredentialError::Conflict
  );
  assert_eq!(
    publisher.activate(&orphan).await.unwrap_err(),
    ProviderCredentialError::Conflict
  );
  for entry in fs::read_dir(fixture.root.path().join("credentials")).unwrap() {
    assert!(!String::from_utf8_lossy(&fs::read(entry.unwrap().path()).unwrap()).contains("orphan-secret-token"));
  }
  let live = CredentialPublication::new(reference.clone(), [10; 32], 2).unwrap();
  publisher
    .prepare(
      &live,
      Kind::Provision,
      Some(ProviderCredentialValue::new(b"managed-live-token".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  publisher.activate(&live).await.unwrap();
  assert_eq!(
    publisher
      .rotate(
        &reference,
        ProviderCredentialValue::new(b"bypass-token".to_vec()).unwrap()
      )
      .await
      .unwrap_err(),
    ProviderCredentialError::Denied
  );
}

#[tokio::test]
async fn activation_rejects_corrupted_preparation_without_publishing_unusable_material() {
  use octacity_server_secrets::{CredentialPublication, CredentialPublicationKind, ProviderCredentialPublisher};
  let fixture = Fixture::new(true, false);
  let publisher = manager(&fixture);
  let stamp = CredentialPublication::new(managed_target().reference, [11; 32], 1).unwrap();
  publisher
    .prepare(
      &stamp,
      CredentialPublicationKind::Provision,
      Some(ProviderCredentialValue::new(b"before-corruption".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  let path = fs::read_dir(fixture.root.path().join("credentials"))
    .unwrap()
    .next()
    .unwrap()
    .unwrap()
    .path();
  let mut document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
  document["token"] = json!("invalid\nsecret");
  private_file(&path, serde_json::to_vec(&document).unwrap().as_slice());
  assert_eq!(
    publisher.activate(&stamp).await.unwrap_err(),
    ProviderCredentialError::Denied
  );
  assert!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .prepare(&fixture.request(false))
      .is_err()
  );
}

#[tokio::test]
async fn recovery_revalidates_current_material_before_returning_an_activation_receipt() {
  use octacity_server_secrets::{CredentialPublication, CredentialPublicationKind, ProviderCredentialPublisher};
  let fixture = Fixture::new(true, false);
  let publisher = manager(&fixture);
  let stamp = CredentialPublication::new(managed_target().reference, [12; 32], 1).unwrap();
  publisher
    .prepare(
      &stamp,
      CredentialPublicationKind::Provision,
      Some(ProviderCredentialValue::new(b"valid-before-recovery".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  publisher.activate(&stamp).await.unwrap();
  let path = fixture
    .root
    .path()
    .join("credentials")
    .join(format!("{}.json", hex::encode(Sha256::digest("private-source-grant"))));
  let mut document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
  document["token"] = json!("invalid\ncurrent-value");
  private_file(&path, serde_json::to_vec(&document).unwrap().as_slice());
  assert_eq!(
    publisher.activate(&stamp).await.unwrap_err(),
    ProviderCredentialError::Denied
  );
}

#[tokio::test]
async fn private_preparation_cannot_configure_another_permitted_opaque_handle() {
  use octacity_server_secrets::{CredentialPublication, CredentialPublicationKind, ProviderCredentialPublisher};
  let fixture = Fixture::new(true, false);
  let original = managed_target();
  let mut other = original.clone();
  other.reference = LogicalSecretReference::new(SecretProviderId::new("installed").unwrap(), "other-reader").unwrap();
  other.handle =
    "octacity-prepared:private-source-grant:0909090909090909090909090909090909090909090909090909090909090909".into();
  let other_reference = other.reference.clone();
  let publisher = GithubCredentialManager::new(
    fixture.root.path(),
    Path::new("credentials"),
    vec![original.clone(), other],
  )
  .unwrap();
  let stamp = CredentialPublication::new(original.reference, [9; 32], 1).unwrap();
  publisher
    .prepare(
      &stamp,
      CredentialPublicationKind::Provision,
      Some(ProviderCredentialValue::new(b"private-first-reader-token".to_vec()).unwrap()),
    )
    .await
    .unwrap();
  assert_eq!(
    publisher.adopt(&other_reference).await.unwrap_err(),
    ProviderCredentialError::NotConfigured
  );
}

#[tokio::test]
async fn managed_provisioning_is_write_only_and_preserves_the_installed_source_contract() {
  let fixture = Fixture::new(true, false);
  let reference = managed_target().reference;
  let manager = manager(&fixture);
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  let profile = provider.configuration_digest().to_owned();
  let port: &dyn ProviderCredentialManager = &manager;
  let receipt = port
    .provision(
      &reference,
      ProviderCredentialValue::new(b"managed-fixture-token".to_vec()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    serde_json::to_value(&receipt).unwrap(),
    json!({"reference":reference,"configured":true})
  );
  assert!(!format!("{receipt:?}").contains("managed-fixture-token"));
  assert_eq!(
    provider.prepare(&fixture.request(false)).unwrap().resource_id(),
    "native-container"
  );
  assert_eq!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .configuration_digest(),
    profile
  );
  fixture.no_provider_call();
}

#[tokio::test]
async fn managed_adoption_preserves_values_and_rejects_class_or_resource_substitution() {
  let fixture = Fixture::new(true, false);
  let path = fixture.grant("private-source-grant", "work_source_read");
  let original = fs::read(&path).unwrap();
  let target = managed_target();
  let reference = target.reference.clone();
  let manager =
    GithubCredentialManager::new(fixture.root.path(), Path::new("credentials"), vec![target.clone()]).unwrap();
  let receipt = manager.adopt(&reference).await.unwrap();
  assert_eq!(
    serde_json::to_value(receipt).unwrap(),
    json!({"reference":reference,"configured":true})
  );
  assert_eq!(fs::read(&path).unwrap(), original);
  for class in [
    GithubCredentialClass::TaskUpdate,
    GithubCredentialClass::RepositoryRead,
    GithubCredentialClass::Delivery,
  ] {
    let incompatible = GithubCredentialManager::new(
      fixture.root.path(),
      Path::new("credentials"),
      vec![GithubCredentialTarget {
        class,
        ..target.clone()
      }],
    )
    .unwrap();
    assert_eq!(
      incompatible.adopt(&reference).await.unwrap_err(),
      ProviderCredentialError::Denied
    );
  }
  let incompatible = GithubCredentialManager::new(
    fixture.root.path(),
    Path::new("credentials"),
    vec![GithubCredentialTarget {
      repositories: vec!["other-ticket-space".into()],
      ..target
    }],
  )
  .unwrap();
  assert_eq!(
    incompatible.adopt(&reference).await.unwrap_err(),
    ProviderCredentialError::Denied
  );
  let foreign = LogicalSecretReference::new(SecretProviderId::new("jira").unwrap(), "source-reader").unwrap();
  assert_eq!(
    manager.adopt(&foreign).await.unwrap_err(),
    ProviderCredentialError::NotConfigured
  );
  fixture.no_provider_call();
}

#[tokio::test]
async fn managed_rotation_atomically_replaces_only_the_same_purpose_handle() {
  use std::io::Read as _;
  let fixture = Fixture::new(true, false);
  let path = fixture.grant("private-source-grant", "work_source_read");
  let mut retained_reader = fs::File::open(&path).unwrap();
  let reference = managed_target().reference;
  let manager = manager(&fixture);
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  let profile = provider.configuration_digest().to_owned();
  assert_eq!(
    manager
      .provision(
        &reference,
        ProviderCredentialValue::new(b"never-overwrite-existing".to_vec()).unwrap()
      )
      .await
      .unwrap_err(),
    ProviderCredentialError::Conflict
  );
  let receipt = manager
    .rotate(
      &reference,
      ProviderCredentialValue::new(b"rotated-fixture-token".to_vec()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    serde_json::to_value(&receipt).unwrap(),
    json!({"reference":reference,"configured":true})
  );
  let current: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
  assert_eq!(
    current,
    json!({"class":"work_source_read","token":"rotated-fixture-token","repositories":["installed-ticket-space"]})
  );
  let mut previous = Vec::new();
  retained_reader.read_to_end(&mut previous).unwrap();
  assert_eq!(
    serde_json::from_slice::<serde_json::Value>(&previous).unwrap()["token"],
    "private-fixture-token"
  );
  assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
  assert_eq!(
    provider.prepare(&fixture.request(false)).unwrap().resource_id(),
    "native-container"
  );
  assert_eq!(
    GithubWorkProvider::load(fixture.root.path())
      .unwrap()
      .configuration_digest(),
    profile
  );
  assert_eq!(
    fs::read_dir(fixture.root.path().join("credentials")).unwrap().count(),
    1
  );
  fixture.no_provider_call();
}

#[tokio::test]
async fn managed_provisioning_bounds_the_complete_private_document_before_writing() {
  let fixture = Fixture::new(true, false);
  let reference = LogicalSecretReference::new(SecretProviderId::new("installed").unwrap(), "source-reader").unwrap();
  let manager = GithubCredentialManager::new(
    fixture.root.path(),
    Path::new("credentials"),
    vec![GithubCredentialTarget {
      reference: reference.clone(),
      handle: "private-source-grant".into(),
      class: GithubCredentialClass::TaskRead,
      repositories: (0..128).map(|i| format!("{i}-{}", "a".repeat(1000))).collect(),
    }],
  )
  .unwrap();
  let result = manager
    .provision(
      &reference,
      ProviderCredentialValue::new(b"private-fixture-token".to_vec()).unwrap(),
    )
    .await;
  assert_eq!(result.unwrap_err(), ProviderCredentialError::Denied);
  assert_eq!(
    fs::read_dir(fixture.root.path().join("credentials")).unwrap().count(),
    0
  );
}

fn managed_target() -> GithubCredentialTarget {
  GithubCredentialTarget {
    reference: LogicalSecretReference::new(SecretProviderId::new("installed").unwrap(), "source-reader").unwrap(),
    handle: "private-source-grant".into(),
    class: GithubCredentialClass::TaskRead,
    repositories: vec!["installed-ticket-space".into()],
  }
}
fn manager(fixture: &Fixture) -> GithubCredentialManager {
  GithubCredentialManager::new(fixture.root.path(), Path::new("credentials"), vec![managed_target()]).unwrap()
}

#[tokio::test]
async fn managed_credentials_refuse_unsafe_files_without_replacing_or_disclosing_them() {
  use std::os::unix::fs::symlink;
  for defect in [
    "public",
    "symlink",
    "hardlink",
    "directory",
    "fifo",
    "malformed",
    "oversized",
    "extra",
    "wrong-resource",
    "wrong-class",
  ] {
    let fixture = Fixture::new(true, false);
    let path = fixture.grant("private-source-grant", "work_source_read");
    let manager = manager(&fixture);
    let reference = managed_target().reference;
    let outside = tempfile::tempdir().unwrap();
    let retained = outside.path().join("never-change.json");
    fs::copy(&path, &retained).unwrap();
    let original = fs::read(&retained).unwrap();
    match defect {
      "public" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
      "symlink" | "hardlink" | "directory" | "fifo" => {
        fs::remove_file(&path).unwrap();
        match defect {
          "symlink" => symlink(&retained, &path).unwrap(),
          "hardlink" => fs::hard_link(&retained, &path).unwrap(),
          "directory" => fs::create_dir(&path).unwrap(),
          "fifo" => assert!(
            std::process::Command::new("mkfifo")
              .arg(&path)
              .status()
              .unwrap()
              .success()
          ),
          _ => unreachable!(),
        }
      }
      "malformed" => private_file(&path, b"malformed-private-fixture-token"),
      "oversized" => private_file(&path, &vec![b'x'; 64 * 1024 + 1]),
      _ => {
        let mut wire =
          json!({"class":"work_source_read","token":"private-fixture-token","repositories":["installed-ticket-space"]});
        match defect {
          "extra" => wire["path"] = json!("/private/credential-path"),
          "wrong-resource" => wire["repositories"] = json!(["another-ticket-space"]),
          "wrong-class" => wire["class"] = json!("delivery_write"),
          _ => unreachable!(),
        }
        private_file(&path, &serde_json::to_vec(&wire).unwrap());
      }
    }
    let adopted = manager.adopt(&reference).await.unwrap_err();
    let rotated = manager
      .rotate(
        &reference,
        ProviderCredentialValue::new(b"must-not-be-published".to_vec()).unwrap(),
      )
      .await
      .unwrap_err();
    assert_eq!(adopted, ProviderCredentialError::Denied, "{defect}");
    assert_eq!(rotated, ProviderCredentialError::Denied, "{defect}");
    for rendered in [format!("{adopted:?}"), rotated.to_string()] {
      for private in [
        "private-fixture-token",
        "must-not-be-published",
        fixture.root.path().to_str().unwrap(),
      ] {
        assert!(!rendered.contains(private), "{defect}");
      }
    }
    assert_eq!(fs::read(&retained).unwrap(), original, "{defect}");
    assert_eq!(
      fs::read_dir(fixture.root.path().join("credentials")).unwrap().count(),
      1,
      "{defect}"
    );
    fixture.no_provider_call();
  }
}

#[tokio::test]
async fn managed_credentials_recheck_private_ancestry_and_original_directory_identity() {
  use std::os::unix::fs::symlink;
  for defect in [
    "public-root",
    "public-directory",
    "symlink-root",
    "replaced-root",
    "symlink-directory",
    "replaced-directory",
  ] {
    let fixture = Fixture::new(true, false);
    fixture.grant("private-source-grant", "work_source_read");
    let manager = manager(&fixture);
    let reference = managed_target().reference;
    let outside = tempfile::tempdir().unwrap();
    let selected = if defect.ends_with("root") {
      fixture.root.path().to_path_buf()
    } else {
      fixture.root.path().join("credentials")
    };
    let moved = outside.path().join("original");
    if defect.starts_with("public") {
      fs::set_permissions(&selected, fs::Permissions::from_mode(0o755)).unwrap();
    } else {
      fs::rename(&selected, &moved).unwrap();
      if defect.starts_with("symlink") {
        symlink(&moved, &selected).unwrap();
      } else {
        fs::create_dir(&selected).unwrap();
        fs::set_permissions(&selected, fs::Permissions::from_mode(0o700)).unwrap();
      }
    }
    assert_eq!(
      manager.adopt(&reference).await.unwrap_err(),
      ProviderCredentialError::Denied,
      "{defect}"
    );
    assert_eq!(
      manager
        .rotate(
          &reference,
          ProviderCredentialValue::new(b"never-redirect-token".to_vec()).unwrap()
        )
        .await
        .unwrap_err(),
      ProviderCredentialError::Denied,
      "{defect}"
    );
    assert_eq!(
      manager
        .provision(
          &reference,
          ProviderCredentialValue::new(b"never-redirect-token".to_vec()).unwrap()
        )
        .await
        .unwrap_err(),
      ProviderCredentialError::Denied,
      "{defect}"
    );
    // Restore the fixture directory name so TempDir cleanup remains confined to its original tree.
    if !defect.starts_with("public") {
      if defect.starts_with("symlink") {
        fs::remove_file(&selected).unwrap();
      } else {
        fs::remove_dir(&selected).unwrap();
      }
      fs::rename(&moved, &selected).unwrap();
    }
    fixture.no_provider_call();
  }
}

#[tokio::test]
async fn managed_missing_material_has_safe_status_and_rotation_cannot_create_it() {
  let fixture = Fixture::new(true, false);
  let manager = manager(&fixture);
  let reference = managed_target().reference;
  assert_eq!(
    manager.adopt(&reference).await.unwrap_err(),
    ProviderCredentialError::NotConfigured
  );
  assert_eq!(
    manager
      .rotate(
        &reference,
        ProviderCredentialValue::new(b"not-a-provisioning-request".to_vec()).unwrap()
      )
      .await
      .unwrap_err(),
    ProviderCredentialError::NotConfigured
  );
  assert_eq!(
    fs::read_dir(fixture.root.path().join("credentials")).unwrap().count(),
    0
  );
}

#[tokio::test]
async fn managed_invalid_tokens_preserve_existing_material_and_safe_failures() {
  let fixture = Fixture::new(true, false);
  let path = fixture.grant("private-source-grant", "work_source_read");
  let original = fs::read(&path).unwrap();
  let manager = manager(&fixture);
  let reference = managed_target().reference;
  for bytes in [vec![0xff], b"private-fixture\ntoken".to_vec(), vec![b'x'; 8193]] {
    for rotate in [false, true] {
      let value = ProviderCredentialValue::new(bytes.clone()).unwrap();
      let error = if rotate {
        manager.rotate(&reference, value).await
      } else {
        manager.provision(&reference, value).await
      }
      .unwrap_err();
      assert_eq!(error, ProviderCredentialError::InvalidValue);
      assert_eq!(error.to_string(), "invalid provider credential value");
      assert_eq!(fs::read(&path).unwrap(), original);
    }
  }
  assert_eq!(
    fs::read_dir(fixture.root.path().join("credentials")).unwrap().count(),
    1
  );
  fixture.no_provider_call();
}

#[tokio::test]
async fn managed_provisioning_preserves_each_existing_private_credential_class() {
  for (class, wire) in [
    (GithubCredentialClass::TaskRead, "work_source_read"),
    (GithubCredentialClass::TaskUpdate, "work_reporter_write"),
    (GithubCredentialClass::RepositoryRead, "vcs_read"),
    (GithubCredentialClass::Delivery, "delivery_write"),
  ] {
    let fixture = Fixture::new(true, true);
    let target = GithubCredentialTarget {
      class,
      ..managed_target()
    };
    let manager =
      GithubCredentialManager::new(fixture.root.path(), Path::new("credentials"), vec![target.clone()]).unwrap();
    let receipt = manager
      .provision(
        &target.reference,
        ProviderCredentialValue::new(b"private-class-fixture-token".to_vec()).unwrap(),
      )
      .await
      .unwrap();
    let path = fs::read_dir(fixture.root.path().join("credentials"))
      .unwrap()
      .next()
      .unwrap()
      .unwrap()
      .path();
    let file: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
      file,
      json!({"class":wire,"token":"private-class-fixture-token","repositories":["installed-ticket-space"]})
    );
    assert_eq!(manager.adopt(&target.reference).await.unwrap(), receipt);
    fixture.no_provider_call();
  }
}

#[test]
fn managed_installer_rejects_escaping_paths_and_ambiguous_purpose_handles() {
  let fixture = Fixture::new(true, false);
  for relative in [
    "",
    "../credentials",
    "/tmp",
    "./credentials",
    "credentials/../credentials",
  ] {
    assert!(
      matches!(
        GithubCredentialManager::new(fixture.root.path(), Path::new(relative), vec![managed_target()]),
        Err(ProviderCredentialError::Denied)
      ),
      "{relative}"
    );
  }
  for defect in [
    "reference",
    "handle",
    "resources",
    "resource-duplicate",
    "resource-limit",
  ] {
    let mut second = managed_target();
    match defect {
      "reference" => second.handle = "another-private-handle".into(),
      "handle" => {
        second.reference =
          LogicalSecretReference::new(SecretProviderId::new("installed").unwrap(), "another-reader").unwrap()
      }
      "resources" => second.repositories.clear(),
      "resource-duplicate" => second.repositories.push("installed-ticket-space".into()),
      "resource-limit" => second.repositories = (0..129).map(|i| format!("scope-{i}")).collect(),
      _ => unreachable!(),
    }
    let targets = if matches!(defect, "reference" | "handle") {
      vec![managed_target(), second]
    } else {
      vec![second]
    };
    assert!(
      matches!(
        GithubCredentialManager::new(fixture.root.path(), Path::new("credentials"), targets),
        Err(ProviderCredentialError::Denied)
      ),
      "{defect}"
    );
  }
}

#[tokio::test]
async fn managed_concurrent_provisioning_publishes_one_complete_value_without_temporary_leaks() {
  let fixture = Fixture::new(true, false);
  let manager = manager(&fixture);
  let reference = managed_target().reference;
  let (left, right) = tokio::join!(
    manager.provision(
      &reference,
      ProviderCredentialValue::new(b"first-complete-token".to_vec()).unwrap()
    ),
    manager.provision(
      &reference,
      ProviderCredentialValue::new(b"second-complete-token".to_vec()).unwrap()
    ),
  );
  assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
  let failure = left.err().or(right.err()).unwrap();
  assert_eq!(failure, ProviderCredentialError::Conflict);
  let paths: Vec<_> = fs::read_dir(fixture.root.path().join("credentials"))
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect();
  assert_eq!(paths.len(), 1);
  let current: serde_json::Value = serde_json::from_slice(&fs::read(&paths[0]).unwrap()).unwrap();
  assert!(["first-complete-token", "second-complete-token"].contains(&current["token"].as_str().unwrap()));
  assert_eq!(fs::metadata(&paths[0]).unwrap().permissions().mode() & 0o777, 0o600);
  assert_eq!(manager.adopt(&reference).await.unwrap().reference, reference);
  fixture.no_provider_call();
}

#[tokio::test]
async fn managed_nested_installation_cannot_be_redirected_through_a_changed_parent() {
  use std::os::unix::fs::symlink;
  let fixture = Fixture::new(true, false);
  let original = fixture.grant("private-source-grant", "work_source_read");
  let nested = fixture.root.path().join("nested");
  fs::create_dir(&nested).unwrap();
  fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
  fs::rename(fixture.root.path().join("credentials"), nested.join("credentials")).unwrap();
  let manager = GithubCredentialManager::new(
    fixture.root.path(),
    Path::new("nested/credentials"),
    vec![managed_target()],
  )
  .unwrap();
  let reference = managed_target().reference;
  assert!(manager.adopt(&reference).await.unwrap().configured);
  let foreign = tempfile::tempdir().unwrap();
  let moved = foreign.path().join("moved");
  fs::rename(&nested, &moved).unwrap();
  symlink(&moved, &nested).unwrap();
  let target = moved.join("credentials").join(original.file_name().unwrap());
  let retained = fs::read(&target).unwrap();
  assert_eq!(
    manager
      .rotate(
        &reference,
        ProviderCredentialValue::new(b"never-redirect-this-value".to_vec()).unwrap()
      )
      .await
      .unwrap_err(),
    ProviderCredentialError::Denied
  );
  assert_eq!(fs::read(&target).unwrap(), retained);
  fs::remove_file(&nested).unwrap();
  fs::rename(moved, nested).unwrap();
}
