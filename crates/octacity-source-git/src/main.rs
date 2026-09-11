//! Process entry point for the Git source plugin.
//!
//! stdout is reserved for bounded JSONL protocol frames. A dedicated blocking
//! stdin reader feeds commands into the async materialization loop so Cancel
//! can be observed while Git is running.

use std::{
  io::{BufRead as _, Read as _, Write as _},
  process::ExitCode,
};

use octacity_source_git::{GitSourceError, materialize};
use octacity_source_plugin::validate_request_id;
use octacity_source_plugin::{MAX_SOURCE_FRAME_BYTES, SOURCE_PLUGIN_PROTOCOL_VERSION, SourceCommand, SourceMessage};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> ExitCode {
  match run().await {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      let _ = write_message(&SourceMessage::Error {
        request_id: None,
        message: error.to_string(),
      });
      ExitCode::FAILURE
    }
  }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args();
  let _program = args.next();
  match (args.next().as_deref(), args.next()) {
    (Some("capabilities"), None) => write_hello().map_err(Into::into),
    (None, None) => serve().await,
    _ => Err("usage: octacity-source-git [capabilities]".into()),
  }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
  write_hello()?;
  let mut input = command_stream();
  let command = input.recv().await.ok_or("source-plugin input reader stopped")??;
  let SourceCommand::Materialize {
    protocol_version,
    request_id,
    request,
  } = command
  else {
    return Err("the first source-plugin command must be materialize".into());
  };
  if protocol_version != SOURCE_PLUGIN_PROTOCOL_VERSION {
    return Err(format!("unsupported source protocol version {protocol_version}").into());
  }
  validate_request_id(&request_id)?;
  request.validate()?;
  write_message(&SourceMessage::Accepted {
    request_id: request_id.clone(),
  })?;
  write_message(&SourceMessage::Progress {
    request_id: request_id.clone(),
    message: "materializing Git revision".to_owned(),
  })?;

  let cancellation = CancellationToken::new();
  let operation = materialize(&request, cancellation.clone());
  tokio::pin!(operation);
  loop {
    tokio::select! {
      result = &mut operation => {
        match result {
          Ok(source) => write_message(&SourceMessage::Finished {
            request_id,
            revision: source.revision,
            provenance: source.provenance,
          })?,
          Err(GitSourceError::Cancelled) => write_message(&SourceMessage::Cancelled { request_id })?,
          Err(error) => write_message(&SourceMessage::Error {
            request_id: Some(request_id),
            message: error.to_string(),
          })?,
        }
        return Ok(());
      }
      command = input.recv() => {
        match command.ok_or("source-plugin input reader stopped")?? {
          SourceCommand::Cancel { request_id: cancelled } if cancelled == request_id => cancellation.cancel(),
          _ => return Err("unexpected source-plugin command during materialization".into()),
        }
      }
    }
  }
}

fn command_stream() -> tokio::sync::mpsc::Receiver<Result<SourceCommand, String>> {
  // The protocol permits one active materialization and one cancellation, so
  // two buffered commands are sufficient and prevent a malformed controller
  // from growing plugin memory without bound.
  let (sender, receiver) = tokio::sync::mpsc::channel(2);
  // Standard input has a blocking API here; isolating it prevents a stalled
  // controller from occupying the async runtime thread.
  std::thread::spawn(move || {
    let stdin = std::io::stdin();
    read_commands(std::io::BufReader::new(stdin.lock()), &sender);
  });
  receiver
}

fn read_commands<R: std::io::BufRead>(mut input: R, sender: &tokio::sync::mpsc::Sender<Result<SourceCommand, String>>) {
  loop {
    let mut frame = Vec::new();
    let read = match (&mut input)
      .take((MAX_SOURCE_FRAME_BYTES + 1) as u64)
      .read_until(b'\n', &mut frame)
    {
      Ok(read) => read,
      Err(error) => {
        let _ = sender.blocking_send(Err(format!("failed to read source-plugin command: {error}")));
        return;
      }
    };
    if read == 0 {
      let _ = sender.blocking_send(Err("source-plugin input closed".to_owned()));
      return;
    }
    if frame.len() > MAX_SOURCE_FRAME_BYTES || !frame.ends_with(b"\n") {
      let _ = sender.blocking_send(Err(format!(
        "source-plugin command exceeds the {MAX_SOURCE_FRAME_BYTES}-byte frame limit"
      )));
      return;
    }
    match serde_json::from_slice(&frame) {
      Ok(command) => {
        if sender.blocking_send(Ok(command)).is_err() {
          return;
        }
      }
      Err(error) => {
        let _ = sender.blocking_send(Err(format!("invalid source-plugin command: {error}")));
        return;
      }
    }
  }
}

fn write_hello() -> std::io::Result<()> {
  write_message(&SourceMessage::Hello {
    protocol_version: SOURCE_PLUGIN_PROTOCOL_VERSION,
    plugin_name: "git".to_owned(),
    plugin_version: env!("CARGO_PKG_VERSION").to_owned(),
  })
}

fn write_message(message: &SourceMessage) -> std::io::Result<()> {
  let mut frame = serde_json::to_vec(message).map_err(std::io::Error::other)?;
  frame.push(b'\n');
  if frame.len() > MAX_SOURCE_FRAME_BYTES {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidData,
      "source-plugin output exceeds the bounded frame limit",
    ));
  }
  let stdout = std::io::stdout();
  let mut output = stdout.lock();
  output.write_all(&frame)?;
  output.flush()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exposes_the_source_protocol_version() {
    assert_eq!(SOURCE_PLUGIN_PROTOCOL_VERSION, 1);
  }

  #[test]
  fn reads_commands_until_eof() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    read_commands(
      &br#"{"type":"cancel","request_id":"request-1"}
"#[..],
      &sender,
    );
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
    read_commands(&b"not-json\n"[..], &sender);
    assert!(
      receiver
        .blocking_recv()
        .unwrap()
        .unwrap_err()
        .contains("invalid source-plugin command")
    );

    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    read_commands(vec![b'a'; MAX_SOURCE_FRAME_BYTES + 1].as_slice(), &sender);
    assert!(receiver.blocking_recv().unwrap().unwrap_err().contains("frame limit"));
  }
}
