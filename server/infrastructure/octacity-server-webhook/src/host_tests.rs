use octacity_webhook_provider_protocol::ManagedRegistrationOperation;
#[cfg(unix)]
use octacity_webhook_provider_protocol::{
  AuthenticatedRepositoryEvent, Capabilities, Command as WebhookCommand, Outcome, ProtocolRange, VerifyDelivery,
};
#[cfg(unix)]
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
use super::*;

#[test]
fn managed_credentials_are_redacted_from_debug_output() {
  let operation = ManagedRegistrationOperation {
    operation_id: "operation-01".to_owned(),
    integration_id: "integration-01".to_owned(),
    idempotency_key: "idempotency-01".to_owned(),
    callback_url: "https://hooks.example.test/callback".to_owned(),
    credential_handle: "provider-administration-secret".to_owned(),
  };
  let debug = format!("{operation:?}");
  assert!(!debug.contains("provider-administration-secret"));
  assert!(debug.contains("<redacted>"));
}

#[cfg(unix)]
mod process {
  use std::{fs, path::Path, time::Duration};

  use base64::{Engine as _, engine::general_purpose::STANDARD};
  use sha2::{Digest as _, Sha256};
  use tempfile::TempDir;

  use super::*;
  use crate::WebhookAdapterRegistry;

  const SUCCESS: &str = r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"authenticated_event","payload":{"integration_id":"integration-01","delivery_id":"delivery-01","event_kind":"push","repository_id":"repository-01","reference":"refs/heads/main","revision":"0123456789abcdef","provider_time_unix_ms":1700000000000,"actor_display_name":"Builder","metadata":{}}}}"#;

  #[tokio::test]
  async fn executes_a_verified_adapter_with_an_empty_environment() {
    let (root, digest) = install(
      &format!(
        "#!/bin/sh\nIFS= read -r request || exit 90\nif [ \"${{HOME+x}}\" = x ]; then exit 91; fi\nprintf '%s\\n' '{SUCCESS}'\n"
      ),
      default_capabilities(),
    );
    let registry = load_registry(&root);
    let adapter = registry.resolve("fixture", &digest).unwrap();
    let outcome = adapter
      .execute(
        verify_request(),
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap();
    assert!(matches!(
      outcome,
      Outcome::AuthenticatedEvent(AuthenticatedRepositoryEvent { delivery_id, .. }) if delivery_id == "delivery-01"
    ));
  }

  #[tokio::test]
  async fn rejects_malformed_and_spoofed_adapter_responses() {
    for response in [
      "{",
      r#"{"protocol_version":1,"request_id":"different-request","outcome":{"status":"authenticated_event","payload":{"integration_id":"integration-01","delivery_id":"delivery-01","event_kind":"push","repository_id":"repository-01","reference":null,"revision":null,"provider_time_unix_ms":null,"actor_display_name":null,"metadata":{}}}}"#,
      r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"authenticated_event","payload":{"integration_id":"different-integration","delivery_id":"delivery-01","event_kind":"push","repository_id":"repository-01","reference":null,"revision":null,"provider_time_unix_ms":null,"actor_display_name":null,"metadata":{}}}}"#,
    ] {
      let (root, digest) = install(
        &format!("#!/bin/sh\nIFS= read -r request || exit 90\nprintf '%s\\n' '{response}'\n"),
        default_capabilities(),
      );
      let registry = load_registry(&root);
      let error = registry
        .resolve("fixture", &digest)
        .unwrap()
        .execute(
          verify_request(),
          Duration::from_secs(2),
          Duration::from_millis(100),
          CancellationToken::new(),
        )
        .await
        .unwrap_err();
      assert_eq!(error.class(), HostFailureClass::ProtocolFault);
    }
  }

  #[tokio::test]
  async fn classifies_crashes_and_force_stops_hanging_adapters() {
    let (root, digest) = install(
      "#!/bin/sh\nIFS= read -r request || exit 90\nexit 17\n",
      default_capabilities(),
    );
    let registry = load_registry(&root);
    let error = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        verify_request(),
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(matches!(error, WebhookHostError::Crashed { .. }));

    let (root, digest) = install(
      "#!/bin/sh\nIFS= read -r request || exit 90\n/bin/sleep 30\n",
      default_capabilities(),
    );
    let started = std::time::Instant::now();
    let registry = load_registry(&root);
    let error = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        verify_request(),
        Duration::from_millis(40),
        Duration::from_millis(40),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(matches!(error, WebhookHostError::TimedOut { .. }));
    assert!(started.elapsed() < Duration::from_secs(2));
  }

  #[tokio::test]
  async fn cancellation_is_bounded_and_preserves_its_classification() {
    let (root, digest) = install(
      "#!/bin/sh\nIFS= read -r request || exit 90\n/bin/sleep 30\n",
      default_capabilities(),
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
        verify_request(),
        Duration::from_secs(2),
        Duration::from_millis(40),
        cancellation,
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::Cancelled);
  }

  #[tokio::test]
  async fn detects_replacement_before_sending_the_delivery() {
    let (root, digest) = install(
      &format!("#!/bin/sh\nIFS= read -r request || exit 90\nprintf '%s\\n' '{SUCCESS}'\n"),
      default_capabilities(),
    );
    let registry = WebhookAdapterRegistry::discover(root.path()).unwrap();
    let adapter = registry.resolve("fixture", &digest).unwrap();
    fs::write(root.path().join("fixture/adapter"), "#!/bin/sh\nexit 17\n").unwrap();
    make_executable(&root.path().join("fixture/adapter"));

    let error = adapter
      .execute(
        verify_request(),
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(matches!(error, WebhookHostError::ExecutableChanged { .. }));
    assert_eq!(error.class(), HostFailureClass::Permanent);
  }

  #[tokio::test]
  async fn rejects_unadvertised_managed_operations_before_spawn() {
    let (root, digest) = install("#!/bin/sh\nexit 99\n", default_capabilities());
    let request = Request {
      protocol_version: WEBHOOK_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: WebhookCommand::CreateRegistration(ManagedRegistrationOperation {
        operation_id: "operation-01".to_owned(),
        integration_id: "integration-01".to_owned(),
        idempotency_key: "idempotency-01".to_owned(),
        callback_url: "https://hooks.example.test/callback".to_owned(),
        credential_handle: "credential-handle".to_owned(),
      }),
    };
    let registry = load_registry(&root);
    let error = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        request,
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::Unsupported);
  }

  #[tokio::test]
  async fn supports_advertised_managed_operations_and_preserves_classified_failures() {
    let registration = r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"registration","payload":{"integration_id":"integration-01","registration_id":"remote-01","status":"active","callback_url":"https://hooks.example.test/callback"}}}"#;
    let capabilities = Capabilities {
      create_registration: true,
      ..default_capabilities()
    };
    let (root, digest) = install(
      &format!("#!/bin/sh\nIFS= read -r request || exit 90\nprintf '%s\\n' '{registration}'\n"),
      capabilities,
    );
    let registry = load_registry(&root);
    let outcome = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        managed_request(),
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap();
    assert!(matches!(outcome, Outcome::Registration(value) if value.registration_id == "remote-01"));

    let failure = r#"{"protocol_version":1,"request_id":"request-01","outcome":{"status":"failure","payload":{"class":"transient","code":"provider_unavailable","diagnostic":"provider is temporarily unavailable","retry_after_ms":250}}}"#;
    let (root, digest) = install(
      &format!("#!/bin/sh\nIFS= read -r request || exit 90\nprintf '%s\\n' '{failure}'\n"),
      default_capabilities(),
    );
    let registry = load_registry(&root);
    let outcome = registry
      .resolve("fixture", &digest)
      .unwrap()
      .execute(
        verify_request(),
        Duration::from_secs(2),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap();
    assert!(matches!(
      outcome,
      Outcome::Failure(failure)
        if failure.class == octacity_webhook_provider_protocol::FailureClass::Transient
          && failure.retry_after_ms == Some(250)
    ));
  }

  fn verify_request() -> Request {
    Request {
      protocol_version: WEBHOOK_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: WebhookCommand::VerifyDelivery(VerifyDelivery {
        operation_id: "operation-01".to_owned(),
        integration_id: "integration-01".to_owned(),
        verification_material_handle: "verification-handle".to_owned(),
        headers: [("x-signature".to_owned(), "signature".to_owned())]
          .into_iter()
          .collect(),
        body_base64: STANDARD.encode(b"payload"),
      }),
    }
  }

  fn managed_request() -> Request {
    Request {
      protocol_version: WEBHOOK_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: WebhookCommand::CreateRegistration(ManagedRegistrationOperation {
        operation_id: "operation-01".to_owned(),
        integration_id: "integration-01".to_owned(),
        idempotency_key: "idempotency-01".to_owned(),
        callback_url: "https://hooks.example.test/callback".to_owned(),
        credential_handle: "credential-handle".to_owned(),
      }),
    }
  }

  fn default_capabilities() -> Capabilities {
    Capabilities {
      verify_delivery: true,
      create_registration: false,
      observe_registration: false,
      rotate_registration: false,
      delete_registration: false,
    }
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
        "manifest_version = 1\nadapter_id = \"fixture\"\nexecutable = \"adapter\"\nexecutable_sha256 = \"{digest}\"\n\n[protocol]\nmin = {}\nmax = {}\n\n[capabilities]\nverify_delivery = {}\ncreate_registration = {}\nobserve_registration = {}\nrotate_registration = {}\ndelete_registration = {}\n",
        ProtocolRange { min: 1, max: 1 }.min,
        ProtocolRange { min: 1, max: 1 }.max,
        capabilities.verify_delivery,
        capabilities.create_registration,
        capabilities.observe_registration,
        capabilities.rotate_registration,
        capabilities.delete_registration,
      ),
    )
    .unwrap();
    (root, digest)
  }

  fn load_registry(root: &TempDir) -> WebhookAdapterRegistry {
    WebhookAdapterRegistry::discover(root.path()).unwrap()
  }

  fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
  }
}
