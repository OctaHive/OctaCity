//! Bounded framing and lifecycle validation tests for the source host.

use super::*;
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
