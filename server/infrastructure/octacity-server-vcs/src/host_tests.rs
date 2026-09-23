use octacity_vcs_protocol::RepositoryAccess;
#[cfg(unix)]
use octacity_vcs_protocol::{Capabilities, Command as VcsCommand, Outcome, ProtocolRange, ResolveRevision};
#[cfg(unix)]
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
use super::*;

#[test]
fn credential_handles_are_redacted_from_debug_output() {
  let access = repository_access();
  let debug = format!("{access:?}");
  assert!(!debug.contains("repository-credential-secret"));
  assert!(debug.contains("<redacted>"));
}

fn repository_access() -> RepositoryAccess {
  RepositoryAccess {
    operation_id: "operation-01".to_owned(),
    repository_id: "repository-01".to_owned(),
    repository_locator: "https://example.invalid/repository.git".to_owned(),
    credential_handle: "repository-credential-secret".to_owned(),
  }
}

#[cfg(unix)]
mod process {
  use std::{fs, path::Path, time::Duration};

  use octacity_vcs_protocol::{FailureClass, ListReferences, ReadCommit, ReadFile};
  use sha2::{Digest as _, Sha256};
  use tempfile::TempDir;

  use super::*;
  use crate::VcsAdapterRegistry;

  const RESOLVED: &str = r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"resolved","payload":{"reference":"refs/heads/main","revision":"0123456789abcdef"}}}"#;

  #[tokio::test]
  async fn executes_a_verified_adapter_with_an_empty_environment() {
    let (root, digest) = install(
      &format!(
        "#!/bin/sh\nIFS= read -r request || exit 90\nif [ \"${{HOME+x}}\" = x ]; then exit 91; fi\nprintf '%s\\n' '{RESOLVED}'\n"
      ),
      all_capabilities(),
    );
    let outcome = execute(&root, &digest, resolve_request()).await.unwrap();
    assert!(matches!(outcome, Outcome::Resolved(value) if value.revision == "0123456789abcdef"));
  }

  #[tokio::test]
  async fn rejects_malformed_spoofed_and_operation_mismatched_responses() {
    for response in [
      "{",
      r#"{"protocol_version":1,"request_id":"different-request","outcome":{"status":"resolved","payload":{"reference":"refs/heads/main","revision":"0123456789abcdef"}}}"#,
      r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"resolved","payload":{"reference":"refs/heads/other","revision":"0123456789abcdef"}}}"#,
      r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"commit","payload":{"revision":"0123456789abcdef","parents":[],"message":"commit","author_display":null,"committed_at_unix_ms":null,"metadata":{}}}}"#,
    ] {
      let (root, digest) = install(&respond(response), all_capabilities());
      let error = execute(&root, &digest, resolve_request()).await.unwrap_err();
      assert_eq!(error.class(), HostFailureClass::ProtocolFault);
    }
  }

  #[tokio::test]
  async fn enforces_request_specific_page_and_file_limits() {
    let references = r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"references","payload":{"entries":[{"name":"refs/heads/main","kind":"branch","revision":"a"},{"name":"refs/heads/next","kind":"branch","revision":"b"}],"next_cursor":null}}}"#;
    let (root, digest) = install(&respond(references), all_capabilities());
    let request = Request {
      protocol_version: VCS_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: VcsCommand::ListReferences(ListReferences {
        repository: repository_access(),
        cursor: None,
        page_size: 1,
        prefix: None,
      }),
    };
    assert_eq!(
      execute(&root, &digest, request).await.unwrap_err().class(),
      HostFailureClass::ProtocolFault
    );

    let file = r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"file","payload":{"path":"README.md","offset":0,"content_base64":"dG9vIGxhcmdl","truncated":false}}}"#;
    let (root, digest) = install(&respond(file), all_capabilities());
    let request = Request {
      protocol_version: VCS_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: VcsCommand::ReadFile(ReadFile {
        repository: repository_access(),
        revision: "0123456789abcdef".to_owned(),
        path: "README.md".to_owned(),
        offset: 0,
        max_bytes: 4,
      }),
    };
    assert_eq!(
      execute(&root, &digest, request).await.unwrap_err().class(),
      HostFailureClass::ProtocolFault
    );
  }

  #[tokio::test]
  async fn accepts_every_advertised_vcs_result_shape() {
    let cases = [
      (
        Request {
          protocol_version: 1,
          request_id: "request-01".to_owned(),
          command: VcsCommand::ListReferences(ListReferences {
            repository: repository_access(),
            cursor: None,
            page_size: 10,
            prefix: None,
          }),
        },
        r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"references","payload":{"entries":[],"next_cursor":null}}}"#,
      ),
      (
        Request {
          protocol_version: 1,
          request_id: "request-01".to_owned(),
          command: VcsCommand::ReadCommit(ReadCommit {
            repository: repository_access(),
            revision: "abc".to_owned(),
          }),
        },
        r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"commit","payload":{"revision":"abc","parents":[],"message":"message","author_display":null,"committed_at_unix_ms":null,"metadata":{}}}}"#,
      ),
      (
        tree_request(),
        r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"tree","payload":{"entries":[{"path":"README.md","kind":"file","size_bytes":4}],"next_cursor":null}}}"#,
      ),
      (
        file_request(),
        r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"file","payload":{"path":"README.md","offset":0,"content_base64":"dGVzdA==","truncated":false}}}"#,
      ),
      (resolve_request(), RESOLVED),
    ];
    for (request, response) in cases {
      let (root, digest) = install(&respond(response), all_capabilities());
      execute(&root, &digest, request).await.unwrap();
    }
  }

  #[tokio::test]
  async fn preserves_all_adapter_failure_classes() {
    for class in [
      FailureClass::InvalidRequest,
      FailureClass::Unsupported,
      FailureClass::Permanent,
      FailureClass::Transient,
      FailureClass::Cancelled,
      FailureClass::ProtocolFault,
    ] {
      let response = serde_json::json!({
        "protocol_version": 1,
        "request_id": "request-01",
        "outcome": {
          "status": "failure",
          "payload": {
            "class": class,
            "code": "classified",
            "diagnostic": "safe diagnostic",
            "retry_after_ms": (class == FailureClass::Transient).then_some(250),
          }
        }
      })
      .to_string();
      let (root, digest) = install(&respond(&response), all_capabilities());
      let outcome = execute(&root, &digest, resolve_request()).await.unwrap();
      assert!(matches!(outcome, Outcome::Failure(failure) if failure.class == class));
    }
  }

  #[tokio::test]
  async fn classifies_crashes_timeouts_cancellation_and_replacement() {
    let (root, digest) = install(
      "#!/bin/sh\nIFS= read -r request || exit 90\nexit 17\n",
      all_capabilities(),
    );
    let error = execute(&root, &digest, resolve_request()).await.unwrap_err();
    assert!(matches!(error, VcsHostError::Crashed { .. }));
    assert_eq!(error.class(), HostFailureClass::Transient);

    let (root, digest) = install(
      "#!/bin/sh\nIFS= read -r request || exit 90\n/bin/sleep 30\n",
      all_capabilities(),
    );
    let registry = load_registry(&root);
    let error = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        resolve_request(),
        Duration::from_millis(40),
        Duration::from_millis(40),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(matches!(error, VcsHostError::TimedOut { .. }));

    let (root, digest) = install(
      "#!/bin/sh\nIFS= read -r request || exit 90\n/bin/sleep 30\n",
      all_capabilities(),
    );
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(30)).await;
      cancel.cancel();
    });
    let registry = load_registry(&root);
    let error = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        resolve_request(),
        Duration::from_secs(2),
        Duration::from_millis(40),
        cancellation,
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::Cancelled);

    let (root, digest) = install(&respond(RESOLVED), all_capabilities());
    let registry = load_registry(&root);
    let adapter = registry.resolve("fixture", &digest).unwrap();
    fs::write(root.path().join("fixture/adapter"), "#!/bin/sh\nexit 17\n").unwrap();
    make_executable(&root.path().join("fixture/adapter"));
    let error = adapter
      .execute(
        resolve_request(),
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::Permanent);
  }

  #[tokio::test]
  async fn rejects_unadvertised_operations_before_spawn() {
    let capabilities = Capabilities {
      resolve_revision: false,
      ..all_capabilities()
    };
    let (root, digest) = install("#!/bin/sh\nexit 99\n", capabilities);
    let error = execute(&root, &digest, resolve_request()).await.unwrap_err();
    assert_eq!(error.class(), HostFailureClass::Unsupported);
  }

  #[tokio::test]
  async fn cancellation_and_expired_deadlines_are_observed_before_spawn() {
    let root = TempDir::new().unwrap();
    let marker = root.path().join("spawned");
    let script = format!("#!/bin/sh\ntouch '{}'\nexit 99\n", marker.display());
    let (installation, digest) = install(&script, all_capabilities());
    let registry = load_registry(&installation);
    let adapter = registry.resolve("fixture", &digest).unwrap();

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = adapter
      .execute(
        resolve_request(),
        Duration::from_secs(2),
        Duration::from_millis(10),
        cancellation,
      )
      .await
      .unwrap_err();
    assert!(matches!(error, VcsHostError::Cancelled { .. }));
    assert!(!marker.exists());

    let error = adapter
      .execute(
        resolve_request(),
        Duration::from_nanos(1),
        Duration::from_millis(10),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(matches!(error, VcsHostError::TimedOut { .. }));
    assert!(!marker.exists());
  }

  async fn execute(root: &TempDir, digest: &str, request: Request) -> Result<Outcome, VcsHostError> {
    load_registry(root)
      .resolve("fixture", digest)
      .unwrap()
      .execute(
        request,
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
  }

  fn resolve_request() -> Request {
    Request {
      protocol_version: VCS_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: VcsCommand::ResolveRevision(ResolveRevision {
        repository: repository_access(),
        reference: "refs/heads/main".to_owned(),
      }),
    }
  }

  fn tree_request() -> Request {
    Request {
      protocol_version: 1,
      request_id: "request-01".to_owned(),
      command: VcsCommand::ListTree(octacity_vcs_protocol::ListTree {
        repository: repository_access(),
        revision: "abc".to_owned(),
        path: None,
        cursor: None,
        page_size: 10,
      }),
    }
  }

  fn file_request() -> Request {
    Request {
      protocol_version: 1,
      request_id: "request-01".to_owned(),
      command: VcsCommand::ReadFile(ReadFile {
        repository: repository_access(),
        revision: "abc".to_owned(),
        path: "README.md".to_owned(),
        offset: 0,
        max_bytes: 4,
      }),
    }
  }

  fn all_capabilities() -> Capabilities {
    Capabilities {
      list_references: true,
      read_commit: true,
      list_tree: true,
      read_file: true,
      resolve_revision: true,
    }
  }

  fn respond(response: &str) -> String {
    format!("#!/bin/sh\nIFS= read -r request || exit 90\nprintf '%s\\n' '{response}'\n")
  }

  fn install(script: &str, capabilities: Capabilities) -> (TempDir, String) {
    let root = TempDir::new().unwrap();
    let directory = root.path().join("fixture");
    fs::create_dir(&directory).unwrap();
    let executable = directory.join("adapter");
    fs::write(&executable, script).unwrap();
    make_executable(&executable);
    let digest = format!("{:x}", Sha256::digest(script.as_bytes()));
    fs::write(
      directory.join("adapter.toml"),
      format!(
        "manifest_version = 1\nadapter_id = \"fixture\"\nexecutable = \"adapter\"\nexecutable_sha256 = \"{digest}\"\n\n[protocol]\nmin = {}\nmax = {}\n\n[capabilities]\nlist_references = {}\nread_commit = {}\nlist_tree = {}\nread_file = {}\nresolve_revision = {}\n",
        ProtocolRange { min: 1, max: 1 }.min,
        ProtocolRange { min: 1, max: 1 }.max,
        capabilities.list_references,
        capabilities.read_commit,
        capabilities.list_tree,
        capabilities.read_file,
        capabilities.resolve_revision,
      ),
    )
    .unwrap();
    (root, digest)
  }

  fn load_registry(root: &TempDir) -> VcsAdapterRegistry {
    VcsAdapterRegistry::discover(root.path()).unwrap()
  }

  fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
  }
}
