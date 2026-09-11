//! Bounded framing and lifecycle validation tests for the source host.

use super::*;
#[cfg(unix)]
use octacity_source_plugin::SourcePluginManifest;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use tokio::io::AsyncReadExt as _;

#[tokio::test]
async fn reads_one_bounded_protocol_message() {
  let input = br#"{"type":"accepted","request_id":"request-1"}
"#;
  let mut reader = BufReader::new(&input[..]);
  assert_eq!(
    read_message("fixture", &mut reader).await.unwrap(),
    Some(SourceMessage::Accepted {
      request_id: "request-1".to_owned()
    })
  );
  assert_eq!(read_message("fixture", &mut reader).await.unwrap(), None);
}

#[tokio::test]
async fn rejects_malformed_protocol_messages() {
  let input = b"not-json\n";
  let mut reader = BufReader::new(&input[..]);
  assert!(matches!(
    read_message("fixture", &mut reader).await,
    Err(SourceHostError::Json { .. })
  ));
}

#[tokio::test]
async fn distinguishes_eof_and_unterminated_protocol_messages() {
  let mut empty = BufReader::new(&b""[..]);
  assert_eq!(read_message("fixture", &mut empty).await.unwrap(), None);

  let mut unterminated = BufReader::new(&br#"{"type":"accepted","request_id":"request-1"}"#[..]);
  assert!(matches!(
    read_message("fixture", &mut unterminated).await,
    Err(SourceHostError::Protocol { .. })
  ));
}

#[tokio::test]
async fn serializes_a_complete_command_frame() {
  let (mut writer, mut reader) = tokio::io::duplex(1024);
  write_command(
    "fixture",
    &mut writer,
    &SourceCommand::Cancel {
      request_id: "request-1".to_owned(),
    },
  )
  .await
  .unwrap();
  drop(writer);
  let mut bytes = Vec::new();
  reader.read_to_end(&mut bytes).await.unwrap();
  assert_eq!(
    serde_json::from_slice::<SourceCommand>(&bytes).unwrap(),
    SourceCommand::Cancel {
      request_id: "request-1".to_owned()
    }
  );
}

#[tokio::test]
async fn rejects_an_oversized_outgoing_command() {
  let mut output = Vec::new();
  let error = write_command(
    "fixture",
    &mut output,
    &SourceCommand::Cancel {
      request_id: "x".repeat(MAX_SOURCE_FRAME_BYTES),
    },
  )
  .await
  .unwrap_err();

  assert!(error.to_string().contains("outgoing command exceeds"));
  assert!(output.is_empty());
}

#[tokio::test]
async fn bounds_diagnostics_while_draining_the_reader() {
  let output = read_bounded(&b"abcdef"[..], 3).await.unwrap();
  assert_eq!(output.bytes, b"abc");
  assert!(output.truncated);
  assert_eq!(sanitize(b"error\0 text\n"), "error text");
}

#[test]
fn preserves_stop_reason_in_errors() {
  assert!(matches!(
    stop_error("fixture", StopReason::Cancelled),
    SourceHostError::Cancelled { .. }
  ));
  assert!(matches!(
    stop_error("fixture", StopReason::TimedOut),
    SourceHostError::TimedOut { .. }
  ));
}

#[test]
fn bounds_accumulated_lifecycle_messages() {
  let mut count = 0;
  let mut bytes = 0;
  record_lifecycle_message("fixture", "progress", &mut count, &mut bytes).unwrap();
  assert_eq!((count, bytes), (1, 8));

  let mut count = MAX_LIFECYCLE_MESSAGES;
  let mut bytes = 0;
  assert!(record_lifecycle_message("fixture", "one more", &mut count, &mut bytes).is_err());

  let mut count = 0;
  let mut bytes = MAX_LIFECYCLE_BYTES;
  assert!(record_lifecycle_message("fixture", "x", &mut count, &mut bytes).is_err());
}

#[cfg(unix)]
fn scripted_plugin(messages_after_request: &[&str]) -> (tempfile::TempDir, InstalledSourcePlugin) {
  let mut body = String::new();
  for message in messages_after_request {
    body.push_str("printf '%s\\n' '");
    body.push_str(message);
    body.push_str("'\n");
  }
  source_plugin_with_body(&body)
}

#[cfg(unix)]
fn source_plugin_with_body(body: &str) -> (tempfile::TempDir, InstalledSourcePlugin) {
  let directory = tempfile::tempdir().unwrap();
  let executable = directory.path().join("fixture-source");
  let mut script = String::from(
    "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"hello\",\"protocol_version\":1,\"plugin_name\":\"fixture\",\"plugin_version\":\"1.0.0\"}'\nIFS= read -r request\n",
  );
  script.push_str(body);
  std::fs::write(&executable, script).unwrap();
  let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
  permissions.set_mode(0o700);
  std::fs::set_permissions(&executable, permissions).unwrap();

  let plugin = InstalledSourcePlugin {
    manifest: SourcePluginManifest {
      manifest_version: 1,
      name: "fixture".to_owned(),
      version: "1.0.0".to_owned(),
      protocol_min: 1,
      protocol_max: 1,
      executable: "fixture-source".to_owned(),
      sha256: "0".repeat(64),
      platforms: vec![format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)],
      settings: BTreeMap::new(),
    },
    executable,
  };
  (directory, plugin)
}

#[cfg(unix)]
fn materialization_request(destination: PathBuf) -> SourceMaterializationRequest {
  SourceMaterializationRequest {
    request_id: "request-1".to_owned(),
    destination,
    revision: "abcdef".to_owned(),
    reference: Some("main".to_owned()),
    parameters: BTreeMap::new(),
    credential_files: BTreeMap::new(),
    max_workspace_bytes: 1024,
  }
}

#[cfg(unix)]
#[tokio::test]
async fn supervises_a_complete_source_plugin_lifecycle() {
  let (directory, plugin) = scripted_plugin(&[
    r#"{"type":"accepted","request_id":"request-1"}"#,
    r#"{"type":"progress","request_id":"request-1","message":"fetching"}"#,
    r#"{"type":"diagnostic","request_id":"request-1","message":"using mirror"}"#,
    r#"{"type":"finished","request_id":"request-1","revision":"abcdef","provenance":{"remote":"origin"}}"#,
  ]);
  let result = plugin
    .materialize(
      materialization_request(directory.path().to_owned()),
      Duration::from_secs(2),
      Duration::from_millis(200),
      CancellationToken::new(),
    )
    .await
    .unwrap();

  assert_eq!(result.revision, "abcdef");
  assert_eq!(result.progress, ["fetching"]);
  assert_eq!(result.diagnostics, ["using mirror"]);
  assert_eq!(result.provenance.get("remote").map(String::as_str), Some("origin"));
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_invalid_terminal_source_plugin_lifecycles() {
  let cases: &[(&[&str], &str)] = &[
    (&[r#"{"type":"accepted","request_id":"request-1"}"#], "stdout closed"),
    (
      &[
        r#"{"type":"accepted","request_id":"request-1"}"#,
        r#"{"type":"finished","request_id":"request-1","revision":"different","provenance":{}}"#,
      ],
      "terminal revision",
    ),
    (
      &[
        r#"{"type":"accepted","request_id":"request-1"}"#,
        r#"{"type":"cancelled","request_id":"request-1"}"#,
      ],
      "was cancelled",
    ),
    (
      &[
        r#"{"type":"accepted","request_id":"request-1"}"#,
        r#"{"type":"error","request_id":"request-1","message":"provider failed"}"#,
      ],
      "provider failed",
    ),
  ];

  for (messages, expected) in cases {
    let (directory, plugin) = scripted_plugin(messages);
    let error = plugin
      .materialize(
        materialization_request(directory.path().to_owned()),
        Duration::from_secs(2),
        Duration::from_millis(200),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(error.to_string().contains(expected), "unexpected error: {error}");
  }
}

#[cfg(unix)]
#[tokio::test]
async fn cooperatively_cancels_a_running_source_plugin() {
  let (directory, plugin) = source_plugin_with_body(
    "printf '%s\\n' '{\"type\":\"accepted\",\"request_id\":\"request-1\"}'\nIFS= read -r cancel\nprintf '%s\\n' '{\"type\":\"cancelled\",\"request_id\":\"request-1\"}'\n",
  );
  let cancellation = CancellationToken::new();
  let trigger = cancellation.clone();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(20)).await;
    trigger.cancel();
  });

  let error = plugin
    .materialize(
      materialization_request(directory.path().to_owned()),
      Duration::from_secs(2),
      Duration::from_millis(200),
      cancellation,
    )
    .await
    .unwrap_err();
  assert!(matches!(error, SourceHostError::Cancelled { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn times_out_and_stops_a_running_source_plugin() {
  let (directory, plugin) = source_plugin_with_body(
    "printf '%s\\n' '{\"type\":\"accepted\",\"request_id\":\"request-1\"}'\nIFS= read -r cancel\nprintf '%s\\n' '{\"type\":\"cancelled\",\"request_id\":\"request-1\"}'\n",
  );
  let error = plugin
    .materialize(
      materialization_request(directory.path().to_owned()),
      Duration::from_millis(20),
      Duration::from_millis(200),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert!(matches!(error, SourceHostError::TimedOut { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn preserves_cancellation_across_late_terminal_plugin_messages() {
  let terminal_messages = [
    r#"{"type":"finished","request_id":"request-1","revision":"abcdef","provenance":{}}"#,
    r#"{"type":"error","request_id":"request-1","message":"late provider failure"}"#,
    "not-json",
  ];
  for terminal in terminal_messages {
    let body = format!(
      "printf '%s\\n' '{{\"type\":\"accepted\",\"request_id\":\"request-1\"}}'\nIFS= read -r cancel\nprintf '%s\\n' '{terminal}'\n"
    );
    let (directory, plugin) = source_plugin_with_body(&body);
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(20)).await;
      trigger.cancel();
    });

    let error = plugin
      .materialize(
        materialization_request(directory.path().to_owned()),
        Duration::from_secs(2),
        Duration::from_millis(200),
        cancellation,
      )
      .await
      .unwrap_err();
    assert!(matches!(error, SourceHostError::Cancelled { .. }));
  }
}

#[cfg(unix)]
#[tokio::test]
async fn force_stops_a_plugin_that_ignores_the_timeout() {
  let (directory, plugin) =
    source_plugin_with_body("printf '%s\\n' '{\"type\":\"accepted\",\"request_id\":\"request-1\"}'\n/bin/sleep 5\n");
  let error = plugin
    .materialize(
      materialization_request(directory.path().to_owned()),
      Duration::from_millis(20),
      Duration::from_millis(20),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert!(matches!(error, SourceHostError::TimedOut { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn reports_spawn_and_post_handshake_protocol_failures() {
  let (directory, mut plugin) = scripted_plugin(&[]);
  plugin.executable = directory.path().join("missing-plugin");
  let error = plugin
    .materialize(
      materialization_request(directory.path().to_owned()),
      Duration::from_secs(1),
      Duration::from_millis(100),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert!(matches!(error, SourceHostError::Spawn { .. }));

  let cases: &[(&[&str], &str)] = &[
    (
      &[r#"{"type":"progress","request_id":"request-1","message":"too early"}"#],
      "unexpected",
    ),
    (&["not-json"], "invalid JSON"),
  ];
  for (messages, expected) in cases {
    let (directory, plugin) = scripted_plugin(messages);
    let error = plugin
      .materialize(
        materialization_request(directory.path().to_owned()),
        Duration::from_secs(5),
        Duration::from_millis(100),
        CancellationToken::new(),
      )
      .await
      .unwrap_err();
    assert!(error.to_string().contains(expected), "unexpected error: {error}");
  }
}
