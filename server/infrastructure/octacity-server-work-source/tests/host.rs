#![cfg(unix)]
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::{Request, decode_request};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{fs, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;

fn scan() -> Request {
  decode_request(include_bytes!(
    "../../../protocols/octacity-work-source-protocol/fixtures/scan-v1.json"
  ))
  .unwrap()
}
fn installed(provider: &str) -> (tempfile::TempDir, String) {
  let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/provider.sh");
  // Keep the executable inode immutable, even on Linux with concurrent process tests.
  let root = tempfile::tempdir_in(fixture.parent().unwrap()).unwrap();
  let dir = root.path().join(provider);
  fs::create_dir(&dir).unwrap();
  fs::hard_link(&fixture, dir.join("plugin")).unwrap();
  let digest = hex::encode(Sha256::digest(fs::read(&fixture).unwrap()));
  fs::write(dir.join("adapter.toml"), format!(
    "manifest_version = 1\nadapter_id = {provider:?}\nexecutable = \"plugin\"\nexecutable_sha256 = {digest:?}\n\
    [protocol.source]\nmin = 1\nmax = 1\n[protocol.reporter]\nmin = 1\nmax = 1\n\
    [capabilities.source]\ncancel = true\ncredential_class = \"work_source_read\"\nconfiguration_digest = {profile:?}\n\
    [capabilities.reporter]\nobserve = true\nproject = true\ncancel = true\nidempotency = \"observe_before_retry\"\ncredential_class = \"work_reporter_write\"\nconfiguration_digest = {profile:?}\n",
    profile="e".repeat(64)
  )).unwrap();
  let response = json!({"family":"source","message":{"protocol_version":1,"request_id":"scan-request-1",
    "outcome":{"status":"page","payload":{"scan_id":"a".repeat(64),"resource_id":"native-container",
      "configuration_digest":"e".repeat(64),"selection":{"identity":"arbitrary-installed-selector","sha256":"b".repeat(64)},
      "candidates":[],"next_cursor":null}}}});
  fs::write(dir.join("response.json"), serde_json::to_vec(&response).unwrap()).unwrap();
  (root, digest)
}

#[tokio::test]
async fn each_provider_uses_the_verified_host_and_preserves_the_wire_scope() {
  for provider in ["github", "gitlab", "jira", "custom-provider"] {
    let (root, digest) = installed(provider);
    let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
    let adapter = registry.resolve(provider, &digest).unwrap();
    let response = adapter
      .execute(
        scan(),
        Duration::from_secs(10),
        Duration::from_secs(2),
        CancellationToken::new(),
      )
      .await
      .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["message"]["outcome"]["status"], "page");
    assert_eq!(response["message"]["outcome"]["payload"]["candidates"], json!([]));
    let captured: Value =
      serde_json::from_slice(&fs::read(root.path().join(provider).join("captured-request.json")).unwrap()).unwrap();
    assert_eq!(captured, serde_json::to_value(scan()).unwrap());
  }
}

fn projection() -> Request {
  decode_request(include_bytes!(
    "../../../protocols/octacity-work-source-protocol/fixtures/project-v1.json"
  ))
  .unwrap()
}

#[tokio::test]
async fn custom_reporting_requires_explicit_installed_support_before_process_dispatch() {
  let (root, digest) = installed("custom-provider");
  let manifest = root.path().join("custom-provider/adapter.toml");
  fs::write(
    &manifest,
    fs::read_to_string(&manifest).unwrap().replace(
      "[protocol.reporter]\nmin = 1\nmax = 1",
      "[protocol.reporter]\nmin = 1\nmax = 2",
    ),
  )
  .unwrap();
  let mut wire = serde_json::to_value(projection()).unwrap();
  wire["message"]["protocol_version"] = json!(2);
  wire["message"]["command"]["operation"] = json!("inspect");
  wire["message"]["command"]["payload"]["effects"] = json!({"add":["custom"],"remove":[],"state":null});
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
  let error = registry
    .resolve("custom-provider", &digest)
    .unwrap()
    .execute(
      request,
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert_eq!(
    error.class(),
    octacity_server_work_source::HostFailureClass::Unsupported
  );
  assert!(!root.path().join("custom-provider/captured-request.json").exists());
}

#[tokio::test]
async fn incompatible_source_does_not_disable_reporter_and_rejections_expose_no_handles() {
  use octacity_server_work_source::HostFailureClass;
  let (root, digest) = installed("jira");
  let dir = root.path().join("jira");
  let manifest = fs::read_to_string(dir.join("adapter.toml")).unwrap();
  fs::write(
    dir.join("adapter.toml"),
    manifest.replacen("min = 1\nmax = 1", "min = 2\nmax = 2", 1),
  )
  .unwrap();
  let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
  let adapter = registry.resolve("jira", &digest).unwrap();
  let error = adapter
    .execute(
      scan(),
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert_eq!(error.class(), HostFailureClass::Unsupported);
  assert!(!dir.join("captured-request.json").exists());
  let mut wrong_profile = projection();
  let Request::Reporter(request) = &mut wrong_profile else {
    unreachable!()
  };
  let octacity_work_source_protocol::reporter::Command::Project(intent) = &mut request.command else {
    unreachable!()
  };
  intent.configuration_digest = "f".repeat(64);
  let error = adapter
    .execute(
      wrong_profile,
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert_eq!(error.class(), HostFailureClass::InvalidRequest);
  assert!(!dir.join("captured-request.json").exists());
  let response = json!({"family":"reporter","message":{"protocol_version":1,"request_id":"projection-request-1",
    "outcome":{"status":"applied","payload":{"projection_id":"c".repeat(64),"resource_id":"native-container",
      "item_id":"native-item","configuration_digest":"e".repeat(64),"mapping":{"identity":"arbitrary-installed-effects","sha256":"d".repeat(64)},
      "status":"needs-human-eyes","matches":true}}}});
  fs::write(dir.join("response.json"), serde_json::to_vec(&response).unwrap()).unwrap();
  let observed = adapter
    .execute(
      projection(),
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap();
  assert_eq!(serde_json::to_value(observed).unwrap(), response);
}

fn connection_check(family: &str) -> Request {
  let mut wire: Value = serde_json::from_slice(include_bytes!(
    "../../../protocols/octacity-work-source-protocol/fixtures/check-v1.json"
  ))
  .unwrap();
  wire["family"] = json!(family);
  if family == "reporter" {
    wire["message"]["command"]["payload"]["resource"]["credential"]["class"] = json!("work_reporter_write");
  }
  decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap()
}
fn advertise_checks(directory: &Path) {
  let manifest = fs::read_to_string(directory.join("adapter.toml")).unwrap();
  fs::write(
    directory.join("adapter.toml"),
    manifest.replace("cancel = true", "check = true\ncancel = true"),
  )
  .unwrap();
}

async fn ready(path: &Path) {
  tokio::time::timeout(Duration::from_secs(10), async {
    while !path.exists() {
      tokio::time::sleep(Duration::from_millis(1)).await;
    }
  })
  .await
  .expect("fixture process must reach its cancellation boundary");
}

#[tokio::test]
async fn cancellation_keeps_each_original_role_and_operation_and_reaps_descendants() {
  use octacity_server_work_source::HostFailureClass;
  for request in [
    scan(),
    projection(),
    connection_check("source"),
    connection_check("reporter"),
  ] {
    let (root, digest) = installed("provider");
    let dir = root.path().join("provider");
    advertise_checks(&dir);
    fs::write(dir.join("hold"), "").unwrap();
    let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
    let adapter = registry.resolve("provider", &digest).unwrap();
    let token = CancellationToken::new();
    let trigger = async {
      ready(&dir.join("ready")).await;
      token.cancel();
    };
    let operation = adapter.execute(
      request.clone(),
      Duration::from_secs(10),
      Duration::from_millis(300),
      token.clone(),
    );
    let (result, ()) = tokio::join!(operation, trigger);
    assert_eq!(result.unwrap_err().class(), HostFailureClass::Cancelled);
    let cancelled: Value = serde_json::from_slice(&fs::read(dir.join("cancellation.json")).unwrap()).unwrap();
    let original = serde_json::to_value(request).unwrap();
    assert_eq!(cancelled["family"], original["family"]);
    assert_eq!(cancelled["message"]["command"]["operation"], "cancel");
    assert_ne!(cancelled["message"]["request_id"], original["message"]["request_id"]);
    let (target, id) = if original["family"] == "source" {
      ("target_scan_id", "scan_id")
    } else {
      ("target_projection_id", "projection_id")
    };
    let id = if original["message"]["command"]["operation"] == "check" {
      "check_id"
    } else {
      id
    };
    assert_eq!(
      cancelled["message"]["command"]["payload"][target],
      original["message"]["command"]["payload"][id]
    );
    assert!(cancelled["message"]["command"]["payload"].get("credential").is_none());
  }
}

#[tokio::test]
async fn both_roles_keep_absolute_deadlines_and_precancellation_never_starts_a_process() {
  use octacity_server_work_source::HostFailureClass;
  for request in [
    scan(),
    projection(),
    connection_check("source"),
    connection_check("reporter"),
  ] {
    let (root, digest) = installed("provider");
    let dir = root.path().join("provider");
    advertise_checks(&dir);
    fs::write(dir.join("hold"), "").unwrap();
    let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
    let adapter = registry.resolve("provider", &digest).unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let result = adapter
      .execute(
        request.clone(),
        Duration::from_secs(10),
        Duration::from_millis(300),
        token,
      )
      .await;
    assert_eq!(result.unwrap_err().class(), HostFailureClass::Cancelled);
    assert!(!dir.join("captured-request.json").exists());
    let result = tokio::time::timeout(
      Duration::from_secs(5),
      adapter.execute(
        request,
        Duration::from_millis(150),
        Duration::from_millis(300),
        CancellationToken::new(),
      ),
    )
    .await
    .unwrap();
    assert!(
      matches!(result, Err(octacity_server_work_source::HostError::TimedOut { .. })),
      "expected the original deadline, got {result:?}"
    );
  }
}

#[tokio::test]
async fn wrong_classes_and_changed_executables_are_rejected_before_exposing_handles() {
  use octacity_server_work_source::HostFailureClass;
  use octacity_work_source_protocol::{CredentialClass, reporter, source};
  let (root, digest) = installed("provider");
  let dir = root.path().join("provider");
  let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
  let adapter = registry.resolve("provider", &digest).unwrap();
  for mut request in [scan(), projection()] {
    match &mut request {
      Request::Source(request) => {
        let source::Command::Scan(intent) = &mut request.command else {
          unreachable!()
        };
        intent.resource.credential.class = CredentialClass::WorkReporterWrite;
      }
      Request::Reporter(request) => {
        let reporter::Command::Project(intent) = &mut request.command else {
          unreachable!()
        };
        intent.resource.credential.class = CredentialClass::WorkSourceRead;
      }
    }
    let error = adapter
      .execute(
        request,
        Duration::from_secs(10),
        Duration::from_secs(2),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::InvalidRequest);
    assert!(!dir.join("captured-request.json").exists());
  }
  // Unlink first: the installed test executable is a hard link to an immutable fixture.
  fs::remove_file(dir.join("plugin")).unwrap();
  fs::copy(
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/provider.sh"),
    dir.join("plugin"),
  )
  .unwrap();
  use std::io::Write as _;
  fs::OpenOptions::new()
    .append(true)
    .open(dir.join("plugin"))
    .unwrap()
    .write_all(b"\n# drift\n")
    .unwrap();
  let error = adapter
    .execute(
      scan(),
      Duration::from_secs(10),
      Duration::from_secs(2),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert_eq!(error.class(), HostFailureClass::Permanent);
  assert!(!dir.join("captured-request.json").exists());
}

#[tokio::test]
async fn malformed_uncorrelated_oversized_and_extra_responses_cannot_become_success() {
  use octacity_server_work_source::HostFailureClass;
  for response in [
    b"not-json".to_vec(),
    serde_json::to_vec(
      &json!({"family":"source","message":{"protocol_version":1,"request_id":"foreign",
      "outcome":{"status":"failure","payload":{"class":"transient","retry_after_millis":null}}}}),
    )
    .unwrap(),
    vec![b'x'; octacity_work_source_protocol::MAX_RESPONSE_BYTES + 1],
    b"{}\n{}".to_vec(),
  ] {
    let (root, digest) = installed("provider");
    fs::write(root.path().join("provider/response.json"), response).unwrap();
    let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
    let adapter = registry.resolve("provider", &digest).unwrap();
    let error = adapter
      .execute(
        scan(),
        Duration::from_secs(10),
        Duration::from_secs(2),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::ProtocolFault);
    assert!(!error.to_string().contains("private-source-grant"));
  }
}

#[tokio::test]
async fn source_access_verification_requires_explicit_advertisement_before_exposing_read_handles() {
  use octacity_server_work_source::HostFailureClass;
  use octacity_work_source_protocol::source;
  let Request::Source(mut request) = scan() else {
    unreachable!()
  };
  let source::Command::Scan(intent) = request.command else {
    unreachable!()
  };
  request.command = source::Command::Authorize(source::AccessIntent {
    scan_id: intent.scan_id.clone(),
    configuration_digest: intent.configuration_digest.clone(),
    resource: intent.resource.clone(),
    network_hosts: intent.network_hosts.clone(),
  });
  let request = Request::Source(request);
  let (root, digest) = installed("jira");
  let dir = root.path().join("jira");
  let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
  let result = registry
    .resolve("jira", &digest)
    .unwrap()
    .execute(
      request.clone(),
      Duration::from_secs(2),
      Duration::from_millis(50),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert_eq!(result.class(), HostFailureClass::Unsupported);
  assert!(!dir.join("captured-request.json").exists());
  let manifest = fs::read_to_string(dir.join("adapter.toml")).unwrap();
  fs::write(
    dir.join("adapter.toml"),
    manifest.replace("[capabilities.source]", "[capabilities.source]\nauthorize = true"),
  )
  .unwrap();
  let response = json!({"family":"source","message":{"protocol_version":1,"request_id":"scan-request-1","outcome":{"status":"authorized","payload":{"scan_id":intent.scan_id,"resource_id":intent.resource.resource_id,"configuration_digest":intent.configuration_digest}}}});
  fs::write(dir.join("response.json"), serde_json::to_vec(&response).unwrap()).unwrap();
  let registry =
    WorkAdapterRegistry::discover(root.path()).expect("an independently advertised access check must be loadable");
  let result = registry
    .resolve("jira", &digest)
    .unwrap()
    .execute(
      request,
      Duration::from_secs(2),
      Duration::from_millis(50),
      CancellationToken::new(),
    )
    .await
    .unwrap();
  assert!(matches!(
    result,
    octacity_work_source_protocol::Response::Source(source::Response {
      outcome: source::Outcome::Authorized(_),
      ..
    })
  ));
}

#[tokio::test]
async fn ordered_steps_require_explicit_v2_before_exposing_any_handle() {
  use octacity_server_work_source::HostFailureClass;
  let (root, digest) = installed("jira");
  let directory = root.path().join("jira");
  let mut wire = serde_json::to_value(projection()).unwrap();
  wire["message"]["protocol_version"] = json!(2);
  wire["message"]["command"]["operation"] = json!("inspect");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let old_registry = WorkAdapterRegistry::discover(root.path()).unwrap();
  let old = old_registry.resolve("jira", &digest).unwrap();
  let error = old
    .execute(
      request.clone(),
      Duration::from_secs(3),
      Duration::from_millis(50),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert_eq!(error.class(), HostFailureClass::Unsupported);
  assert!(!directory.join("captured-request.json").exists());
  let manifest = fs::read_to_string(directory.join("adapter.toml")).unwrap();
  fs::write(
    directory.join("adapter.toml"),
    manifest.replace(
      "[protocol.reporter]\nmin = 1\nmax = 1",
      "[protocol.reporter]\nmin = 1\nmax = 2",
    ),
  )
  .unwrap();
  let receipt = json!({"projection_id":"c".repeat(64),"resource_id":"native-container","item_id":"native-item",
    "configuration_digest":"e".repeat(64),"mapping":{"identity":"arbitrary-installed-effects","sha256":"d".repeat(64)},
    "status":"needs-human-eyes","matches":true});
  let plan = json!({"receipt":receipt,"fields":["a".repeat(64)],"steps":[]});
  let response = json!({"family":"reporter","message":{"protocol_version":2,"request_id":"projection-request-1",
    "outcome":{"status":"planned","payload":plan}}});
  fs::write(directory.join("response.json"), serde_json::to_vec(&response).unwrap()).unwrap();
  let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
  let adapter = registry.resolve("jira", &digest).unwrap();
  let actual = adapter
    .execute(
      request,
      Duration::from_secs(3),
      Duration::from_millis(50),
      CancellationToken::new(),
    )
    .await
    .unwrap();
  assert_eq!(serde_json::to_value(actual).unwrap(), response);
}

#[tokio::test]
async fn read_only_connection_checks_require_explicit_advertisement_and_keep_each_providers_scope() {
  for provider in ["gitlab", "jira", "custom-provider"] {
    let (root, digest) = installed(provider);
    let dir = root.path().join(provider);
    for advertised in [false, true] {
      if advertised {
        let manifest = fs::read_to_string(dir.join("adapter.toml")).unwrap();
        fs::write(
          dir.join("adapter.toml"),
          manifest.replace("cancel = true", "check = true\ncancel = true"),
        )
        .unwrap();
      }
      let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
      let adapter = registry.resolve(provider, &digest).unwrap();
      for (family, class) in [("source", "work_source_read"), ("reporter", "work_reporter_write")] {
        let wire = json!({"family":family,"message":{"protocol_version":1,"request_id":"check-request",
          "command":{"operation":"check","payload":{"check_id":"a".repeat(64),"configuration_digest":"e".repeat(64),
            "endpoint":"https://tracker.invalid/api/","resource":{"resource_id":"native-container",
              "resource_locator":"installed-ticket-space","credential":{"class":class,"handle":"private-check-grant"}},"network_hosts":["tracker.invalid"]}}}});
        let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
        let response = json!({"family":family,"message":{"protocol_version":1,"request_id":"check-request",
          "outcome":{"status":"checked","payload":{"check_id":"a".repeat(64),"configuration_digest":"e".repeat(64),
            "resource_id":"native-container","configuration":"ready","authentication":"ready","resource_access":"ready",
            "capability":if family=="source" {"ready"} else {"unverified"},"retry_after_millis":null}}}});
        fs::write(dir.join("response.json"), serde_json::to_vec(&response).unwrap()).unwrap();
        let result = adapter
          .execute(
            request,
            Duration::from_secs(10),
            Duration::from_secs(1),
            CancellationToken::new(),
          )
          .await;
        if advertised {
          assert_eq!(serde_json::to_value(result.unwrap()).unwrap(), response);
          let captured: Value = serde_json::from_slice(&fs::read(dir.join("captured-request.json")).unwrap()).unwrap();
          assert_eq!(captured, wire);
        } else {
          assert_eq!(
            result.unwrap_err().class(),
            octacity_server_work_source::HostFailureClass::Unsupported
          );
          assert!(!dir.join("captured-request.json").exists());
        }
      }
    }
  }
}

#[tokio::test]
async fn connection_check_process_output_cannot_exceed_its_small_independent_ceiling() {
  use octacity_server_work_source::HostFailureClass;
  for family in ["source", "reporter"] {
    let (root, digest) = installed("provider");
    let directory = root.path().join("provider");
    advertise_checks(&directory);
    fs::write(directory.join("response.json"), vec![b' '; 8193]).unwrap();
    let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
    let error = registry
      .resolve("provider", &digest)
      .unwrap()
      .execute(
        connection_check(family),
        Duration::from_secs(10),
        Duration::from_secs(1),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::ProtocolFault);
    assert!(!error.to_string().contains("private-check-grant"));
  }
}

#[tokio::test]
async fn preview_requires_independent_advertisement_before_exposing_any_read_handle() {
  for provider in ["github", "gitlab", "jira", "custom-provider"] {
    let (root, digest) = installed(provider);
    let dir = root.path().join(provider);
    let mut wire = serde_json::to_value(scan()).unwrap();
    wire["message"]["command"]["operation"] = json!("preview");
    let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
    let registry = WorkAdapterRegistry::discover(root.path()).unwrap();
    let error = registry
      .resolve(provider, &digest)
      .unwrap()
      .execute(
        request.clone(),
        Duration::from_secs(10),
        Duration::from_secs(2),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.class(),
      octacity_server_work_source::HostFailureClass::Unsupported
    );
    assert!(!dir.join("captured-request.json").exists());
    let manifest = fs::read_to_string(dir.join("adapter.toml")).unwrap();
    fs::write(
      dir.join("adapter.toml"),
      manifest.replace("[capabilities.source]", "[capabilities.source]\npreview = true"),
    )
    .unwrap();
    let mut response: Value = serde_json::from_slice(&fs::read(dir.join("response.json")).unwrap()).unwrap();
    response["message"]["outcome"]["status"] = json!("previewed");
    response["message"]["outcome"]["payload"]
      .as_object_mut()
      .unwrap()
      .remove("selection");
    fs::write(dir.join("response.json"), serde_json::to_vec(&response).unwrap()).unwrap();
    let registry = WorkAdapterRegistry::discover(root.path()).expect("explicit read-only preview capability");
    let response = registry
      .resolve(provider, &digest)
      .unwrap()
      .execute(
        request,
        Duration::from_secs(10),
        Duration::from_secs(2),
        CancellationToken::new(),
      )
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(response).unwrap()["message"]["outcome"]["status"],
      "previewed"
    );
  }
}
