#![cfg(unix)]
use octacity_github::GithubWorkProvider;
use octacity_work_source_protocol::{Request, decode_request};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{
  fs,
  net::TcpListener,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
};

struct Fixture {
  root: tempfile::TempDir,
  peer: TcpListener,
}
impl Fixture {
  fn new(source: bool, reporter: bool) -> Self {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(root.path().join("credentials")).unwrap();
    fs::set_permissions(root.path().join("credentials"), fs::Permissions::from_mode(0o700)).unwrap();
    let peer = TcpListener::bind("127.0.0.1:0").unwrap();
    peer.set_nonblocking(true).unwrap();
    let config = format!(
      "config_version = 1\napi_base = \"https://{}/api/\"\napi_version = \"fixture-v1\"\ncredential_directory = \"credentials\"\n\
      [capabilities]\nsource_repositories = {:?}\nreporter_repositories = {:?}\n\
      [repositories.\"installed-ticket-space\"]\nrepository_id = \"native-container\"\nowner = \"owner\"\nname = \"tickets\"\n",
      peer.local_addr().unwrap(),
      if source { vec!["installed-ticket-space"] } else { vec![] },
      if reporter {
        vec!["installed-ticket-space"]
      } else {
        vec![]
      }
    );
    private_file(&root.path().join("github-work.toml"), config.as_bytes());
    Self { root, peer }
  }
  fn grant(&self, handle: &str, class: &str) -> PathBuf {
    let path = self
      .root
      .path()
      .join("credentials")
      .join(format!("{}.json", hex::encode(Sha256::digest(handle))));
    private_file(
      &path,
      &serde_json::to_vec(&json!({"class":class,"token":"private-fixture-token",
      "repositories":["installed-ticket-space"]}))
      .unwrap(),
    );
    path
  }
  fn request(&self, reporter: bool) -> Request {
    let bytes = if reporter {
      include_bytes!("../../../protocols/octacity-work-source-protocol/fixtures/project-v1.json").as_slice()
    } else {
      include_bytes!("../../../protocols/octacity-work-source-protocol/fixtures/scan-v1.json").as_slice()
    };
    let mut wire: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    wire["message"]["command"]["payload"]["configuration_digest"] = json!(hex::encode(Sha256::digest(
      fs::read(self.root.path().join("github-work.toml")).unwrap()
    )));
    wire["message"]["command"]["payload"]["network_hosts"] = json!(["127.0.0.1"]);
    decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap()
  }
  fn no_provider_call(&self) {
    assert!(matches!(self.peer.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
  }
}
fn private_file(path: &Path, bytes: &[u8]) {
  fs::write(path, bytes).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn issue_read_grant_works_without_delivery_material() {
  let fixture = Fixture::new(true, false);
  fixture.grant("private-source-grant", "work_source_read");
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  let access = provider.prepare(&fixture.request(false)).unwrap();
  assert_eq!(access.resource_id(), "native-container");
  let rendered = format!("{access:?}");
  for private in ["private-fixture-token", "private-source-grant", "127.0.0.1"] {
    assert!(!rendered.contains(private));
  }
  assert!(!fixture.root.path().join("github-plugin.toml").exists());
  assert!(!fixture.root.path().join("inputs").exists());
  fixture.no_provider_call();
}

#[test]
fn reporter_grant_is_independent_and_cannot_borrow_read_or_delivery_authority() {
  use octacity_work_source_protocol::FailureClass;
  let fixture = Fixture::new(false, true);
  fixture.grant("private-reporter-grant", "work_reporter_write");
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  assert_eq!(
    provider.prepare(&fixture.request(true)).unwrap().resource_id(),
    "native-container"
  );
  assert_eq!(
    provider.prepare(&fixture.request(false)).unwrap_err(),
    FailureClass::Unsupported
  );
  for class in ["work_source_read", "delivery_write", "vcs_read", "repository_job"] {
    fixture.grant("private-reporter-grant", class);
    let error = provider.prepare(&fixture.request(true)).unwrap_err();
    assert_eq!(
      error,
      FailureClass::Permanent,
      "stored {class} cannot be reclassified by a request"
    );
    assert!(!format!("{error:?}").contains("private-fixture-token"));
  }
  fixture.no_provider_call();
}

#[test]
fn profile_drift_invalidates_loaded_access_while_token_rotation_keeps_the_pin() {
  use octacity_work_source_protocol::FailureClass;
  use std::io::Write as _;
  let fixture = Fixture::new(true, true);
  fixture.grant("private-source-grant", "work_source_read");
  fixture.grant("private-reporter-grant", "work_reporter_write");
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  let originals = [fixture.request(false), fixture.request(true)];
  fs::OpenOptions::new()
    .append(true)
    .open(fixture.root.path().join("github-work.toml"))
    .unwrap()
    .write_all(b"\n# exact profile replacement\n")
    .unwrap();
  for original in originals {
    assert_eq!(provider.prepare(&original).unwrap_err(), FailureClass::Conflict);
  }
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  let pin = provider.configuration_digest().to_owned();
  for (reporter, handle, class) in [
    (false, "private-source-grant", "work_source_read"),
    (true, "private-reporter-grant", "work_reporter_write"),
  ] {
    let request = fixture.request(reporter);
    let credential = fixture.grant(handle, class);
    let rotated = fs::read_to_string(&credential)
      .unwrap()
      .replace("private-fixture-token", "rotated-private-token");
    private_file(&credential, rotated.as_bytes());
    let access = provider.prepare(&request).unwrap();
    assert!(!format!("{access:?}").contains("rotated-private-token"));
    assert_eq!(provider.configuration_digest(), pin);
  }
  fixture.no_provider_call();
}

#[test]
fn private_grant_directory_is_rechecked_before_each_operation() {
  use octacity_work_source_protocol::FailureClass;
  let fixture = Fixture::new(true, true);
  fixture.grant("private-source-grant", "work_source_read");
  fixture.grant("private-reporter-grant", "work_reporter_write");
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  fs::set_permissions(
    fixture.root.path().join("credentials"),
    fs::Permissions::from_mode(0o755),
  )
  .unwrap();
  for reporter in [false, true] {
    assert_eq!(
      provider.prepare(&fixture.request(reporter)).unwrap_err(),
      FailureClass::Permanent
    );
  }
  fixture.no_provider_call();
}

#[test]
fn exact_resource_profile_and_network_ceilings_precede_missing_grants() {
  use octacity_work_source_protocol::FailureClass;
  let fixture = Fixture::new(true, true);
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  for reporter in [false, true] {
    for (field, replacement, expected) in [
      ("resource_id", "foreign-container", FailureClass::InvalidRequest),
      ("resource_locator", "uninstalled-alias", FailureClass::Unsupported),
      ("configuration_digest", &"f".repeat(64), FailureClass::Conflict),
      ("network_hosts", "other.example", FailureClass::InvalidRequest),
    ] {
      let mut wire = serde_json::to_value(fixture.request(reporter)).unwrap();
      let payload = &mut wire["message"]["command"]["payload"];
      match field {
        "resource_id" | "resource_locator" => payload["resource"][field] = json!(replacement),
        "network_hosts" => payload[field] = json!([replacement]),
        _ => payload[field] = json!(replacement),
      }
      let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
      assert_eq!(provider.prepare(&request).unwrap_err(), expected, "{field}");
    }
    assert_eq!(
      provider.prepare(&fixture.request(reporter)).unwrap_err(),
      FailureClass::Permanent
    );
  }
  fixture.no_provider_call();
}

#[test]
fn source_grants_cannot_borrow_other_classes_or_another_repository() {
  use octacity_work_source_protocol::FailureClass;
  let fixture = Fixture::new(true, true);
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  for class in ["work_reporter_write", "delivery_write", "vcs_read", "repository_job"] {
    fixture.grant("private-source-grant", class);
    assert_eq!(
      provider.prepare(&fixture.request(false)).unwrap_err(),
      FailureClass::Permanent
    );
  }
  for (reporter, handle, class) in [
    (false, "private-source-grant", "work_source_read"),
    (true, "private-reporter-grant", "work_reporter_write"),
  ] {
    let path = fixture.grant(handle, class);
    private_file(
      &path,
      &serde_json::to_vec(
        &json!({"class":class,"token":"private-fixture-token", "repositories":["different-ticket-space"]}),
      )
      .unwrap(),
    );
    assert_eq!(
      provider.prepare(&fixture.request(reporter)).unwrap_err(),
      FailureClass::Permanent
    );
  }
  fixture.no_provider_call();
}

fn exchange(fixture: &Fixture, request: &Request) -> std::process::Output {
  use std::{
    io::Write as _,
    process::{Command, Stdio},
  };
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(fixture.root.path())
    .env_clear()
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let mut bytes = serde_json::to_vec(request).unwrap();
  bytes.push(b'\n');
  child.stdin.take().unwrap().write_all(&bytes).unwrap();
  child.wait_with_output().unwrap()
}

#[test]
fn provider_process_checks_installed_work_scopes_before_issue_calls() {
  for reporter in [false, true] {
    let fixture = Fixture::new(true, true);
    let request = fixture.request(reporter);
    assert_process_failure(&fixture, &request, "permanent");
    let (handle, class) = if reporter {
      ("private-reporter-grant", "work_reporter_write")
    } else {
      ("private-source-grant", "work_source_read")
    };
    fixture.grant(handle, class);
    // Listing/projection belongs to subsequent tasks, even after authorization succeeds.
    assert_process_failure(&fixture, &request, "unsupported");
    let config = fixture.root.path().join("github-work.toml");
    let changed = fs::read_to_string(&config).unwrap() + "\n# replaced installed profile\n";
    private_file(&config, changed.as_bytes());
    assert_process_failure(&fixture, &request, "conflict");
    private_file(&config, b"private-profile-content is not TOML");
    assert_process_failure(&fixture, &request, "permanent");
    fixture.no_provider_call();
  }
}

fn assert_process_failure(fixture: &Fixture, request: &Request, expected: &str) {
  let output = exchange(fixture, request);
  assert!(output.status.success());
  let response = octacity_work_source_protocol::decode_response(&output.stdout, request).unwrap();
  let response = serde_json::to_value(response).unwrap();
  assert_eq!(response["message"]["outcome"]["payload"]["class"], expected);
  assert!(output.stderr.is_empty());
  for private in [
    "private-fixture-token",
    "private-source-grant",
    "private-reporter-grant",
    "private-profile-content",
    "127.0.0.1",
  ] {
    assert!(!String::from_utf8_lossy(&output.stdout).contains(private));
  }
}

#[test]
fn installed_profiles_reject_unbounded_or_unconfined_operator_configuration_safely() {
  for defect in [
    "traversal",
    "absolute-directory",
    "unknown-alias",
    "duplicate-alias",
    "unknown-field",
    "unknown-version",
    "http",
    "url-credentials",
    "invalid-certificate",
    "oversized-config",
  ] {
    let fixture = Fixture::new(true, true);
    let path = fixture.root.path().join("github-work.toml");
    let mut wire: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    match defect {
      "traversal" => wire["credential_directory"] = toml::Value::String("../private-path".into()),
      "absolute-directory" => wire["credential_directory"] = toml::Value::String("/private-path".into()),
      "unknown-alias" => {
        wire["capabilities"]["source_repositories"] = toml::Value::Array(vec![toml::Value::String("unknown".into())])
      }
      "duplicate-alias" => {
        wire["capabilities"]["reporter_repositories"] =
          toml::Value::Array(vec![toml::Value::String("installed-ticket-space".into()); 2])
      }
      "unknown-field" => {
        wire.as_table_mut().unwrap().insert(
          "private-unknown-field".into(),
          toml::Value::String("private-content".into()),
        );
      }
      "unknown-version" => wire["config_version"] = toml::Value::Integer(2),
      "http" => wire["api_base"] = toml::Value::String("http://127.0.0.1/".into()),
      "url-credentials" => wire["api_base"] = toml::Value::String("https://private-token@127.0.0.1/".into()),
      "invalid-certificate" => {
        wire.as_table_mut().unwrap().insert(
          "api_root_certificates_pem".into(),
          toml::Value::Array(vec![toml::Value::String("private-invalid-certificate".into())]),
        );
      }
      "oversized-config" => {
        wire["api_version"] = toml::Value::String("x".repeat(65537));
      }
      _ => unreachable!(),
    }
    private_file(&path, toml::to_string(&wire).unwrap().as_bytes());
    let error = GithubWorkProvider::load(fixture.root.path()).err().expect(defect);
    assert_eq!(error.to_string(), "GitHub plugin configuration is invalid");
    assert_eq!(format!("{error:?}"), "GithubConfigError");
    fixture.no_provider_call();
  }
}

#[test]
fn private_files_reject_symlinks_public_permissions_and_malformed_grants() {
  use octacity_work_source_protocol::FailureClass;
  use std::os::unix::fs::symlink;
  for reporter in [false, true] {
    for defect in [
      "public-token",
      "symlink-token",
      "public-config",
      "symlink-config",
      "public-root",
      "empty-token",
      "control-token",
      "oversized-token",
      "malformed",
      "extra-field",
    ] {
      let fixture = Fixture::new(true, true);
      let (handle, class) = if reporter {
        ("private-reporter-grant", "work_reporter_write")
      } else {
        ("private-source-grant", "work_source_read")
      };
      let path = fixture.grant(handle, class);
      let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
      let request = fixture.request(reporter);
      let config = fixture.root.path().join("github-work.toml");
      match defect {
        "public-token" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
        "public-config" => fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap(),
        "public-root" => fs::set_permissions(fixture.root.path(), fs::Permissions::from_mode(0o755)).unwrap(),
        "symlink-token" | "symlink-config" => {
          let selected = if defect == "symlink-token" { &path } else { &config };
          let moved = selected.with_extension("private");
          fs::rename(selected, &moved).unwrap();
          symlink(moved, selected).unwrap();
        }
        "malformed" => private_file(&path, b"private-fixture-token is not JSON"),
        _ => {
          let token = match defect {
            "empty-token" => String::new(),
            "control-token" => "private\ntoken".into(),
            "oversized-token" => "x".repeat(8193),
            _ => "private-fixture-token".into(),
          };
          let mut wire = json!({"class":class,"token":token,"repositories":["installed-ticket-space"]});
          if defect == "extra-field" {
            wire["caller_path"] = json!("/private/path");
          }
          private_file(&path, &serde_json::to_vec(&wire).unwrap());
        }
      }
      let error = provider.prepare(&request).unwrap_err();
      assert_eq!(error, FailureClass::Permanent, "{defect}");
      assert_eq!(format!("{error:?}"), "Permanent");
      fixture.no_provider_call();
    }
  }
}

#[test]
fn changed_nested_credential_directory_cannot_redirect_an_installed_grant() {
  use octacity_work_source_protocol::FailureClass;
  use std::os::unix::fs::symlink;
  let fixture = Fixture::new(true, false);
  fixture.grant("private-source-grant", "work_source_read");
  let nested = fixture.root.path().join("nested");
  fs::create_dir(&nested).unwrap();
  fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
  fs::rename(fixture.root.path().join("credentials"), nested.join("credentials")).unwrap();
  let config = fixture.root.path().join("github-work.toml");
  let bytes = fs::read_to_string(&config).unwrap().replace(
    "credential_directory = \"credentials\"",
    "credential_directory = \"nested/credentials\"",
  );
  private_file(&config, bytes.as_bytes());
  let provider = GithubWorkProvider::load(fixture.root.path()).unwrap();
  let request = fixture.request(false);
  let foreign = tempfile::tempdir().unwrap();
  fs::rename(&nested, foreign.path().join("moved")).unwrap();
  symlink(foreign.path().join("moved"), &nested).unwrap();
  assert_eq!(provider.prepare(&request).unwrap_err(), FailureClass::Permanent);
  fixture.no_provider_call();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "work_credentials/management.rs"]
mod management;
