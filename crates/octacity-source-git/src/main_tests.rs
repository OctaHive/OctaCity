//! Bounded stdin and stdout tests for the source-plugin executable.

use super::*;
use std::collections::BTreeMap;

struct FailingReader;

impl std::io::Read for FailingReader {
  fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
    Err(std::io::Error::other("fixture read failure"))
  }
}

impl std::io::BufRead for FailingReader {
  fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
    Err(std::io::Error::other("fixture read failure"))
  }

  fn consume(&mut self, _amount: usize) {}
}

#[test]
fn exposes_the_source_protocol_version() {
  assert_eq!(SOURCE_PLUGIN_PROTOCOL_VERSION, 1);
}

#[test]
fn reads_commands_until_eof() {
  let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
  let mut input = &br#"{"type":"cancel","request_id":"request-1"}
"#[..];
  read_commands(&mut input, &sender);
  assert!(matches!(
    receiver.blocking_recv().unwrap().unwrap(),
    SourceCommand::Cancel { request_id } if request_id == "request-1"
  ));
  assert_eq!(
    receiver.blocking_recv().unwrap().unwrap_err(),
    "source-plugin input closed"
  );
}

#[test]
fn rejects_malformed_and_oversized_input_frames() {
  let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
  let mut malformed = &b"not-json\n"[..];
  read_commands(&mut malformed, &sender);
  assert!(
    receiver
      .blocking_recv()
      .unwrap()
      .unwrap_err()
      .contains("invalid source-plugin command")
  );

  let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
  let oversized = vec![b'a'; MAX_SOURCE_FRAME_BYTES + 1];
  read_commands(&mut oversized.as_slice(), &sender);
  assert!(receiver.blocking_recv().unwrap().unwrap_err().contains("frame limit"));
}

#[test]
fn reports_reader_failure_and_stops_when_the_consumer_is_gone() {
  let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
  read_commands(&mut FailingReader, &sender);
  assert!(
    receiver
      .blocking_recv()
      .unwrap()
      .unwrap_err()
      .contains("fixture read failure")
  );

  let (sender, receiver) = tokio::sync::mpsc::channel(1);
  drop(receiver);
  let mut input = &br#"{"type":"cancel","request_id":"request-1"}
"#[..];
  read_commands(&mut input, &sender);
}

#[test]
fn refuses_to_write_an_oversized_protocol_message() {
  let error = write_message(&SourceMessage::Diagnostic {
    request_id: "request-1".to_owned(),
    message: "x".repeat(MAX_SOURCE_FRAME_BYTES),
  })
  .unwrap_err();
  assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

fn materialize_command(protocol_version: u16) -> SourceCommand {
  SourceCommand::Materialize {
    protocol_version,
    request_id: "request-1".to_owned(),
    request: MaterializeRequest {
      destination: std::env::temp_dir().to_string_lossy().into_owned(),
      revision: "abcdef".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
      settings: BTreeMap::new(),
      credential_files: BTreeMap::new(),
      max_workspace_bytes: 1,
    },
  }
}

#[test]
fn validates_the_initial_materialize_command() {
  let (request_id, request) = validate_materialize_command(materialize_command(1)).unwrap();
  assert_eq!(request_id, "request-1");
  assert_eq!(request.revision, "abcdef");
  assert!(validate_materialize_command(materialize_command(2)).is_err());
  assert!(
    validate_materialize_command(SourceCommand::Cancel {
      request_id: "request-1".to_owned()
    })
    .is_err()
  );

  let SourceCommand::Materialize {
    protocol_version,
    mut request,
    ..
  } = materialize_command(1)
  else {
    unreachable!()
  };
  request.revision.clear();
  assert!(
    validate_materialize_command(SourceCommand::Materialize {
      protocol_version,
      request_id: "request-1".to_owned(),
      request,
    })
    .is_err()
  );
}

#[test]
fn maps_every_provider_outcome_to_one_terminal_message() {
  assert!(matches!(
    terminal_message(
      "request-1".to_owned(),
      Ok(MaterializedGitSource {
        revision: "abcdef".to_owned(),
        provenance: BTreeMap::from([("remote".to_owned(), "origin".to_owned())]),
      })
    ),
    SourceMessage::Finished { revision, .. } if revision == "abcdef"
  ));
  assert!(matches!(
    terminal_message("request-1".to_owned(), Err(GitSourceError::Cancelled)),
    SourceMessage::Cancelled { .. }
  ));
  assert!(matches!(
    terminal_message(
      "request-1".to_owned(),
      Err(GitSourceError::Invalid("broken".to_owned()))
    ),
    SourceMessage::Error { message, .. } if message.contains("broken")
  ));
}
