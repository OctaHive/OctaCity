//! One-operation JSONL process serving with cooperative cancellation.

use std::io::{BufRead as _, Read as _, Write as _};

use octacity_vcs_protocol::{
  Command as VcsCommand, Failure, FailureClass, MAX_VCS_MESSAGE_BYTES, Outcome, Request, Response,
  VCS_PROTOCOL_VERSION, decode_request,
};
use tokio_util::sync::CancellationToken;

use crate::GitVcsAdapter;

/// Serves one VCS request from stdin and writes exactly one correlated response.
pub async fn serve() -> Result<(), Box<dyn std::error::Error>> {
  let mut input = request_stream();
  let request = input.recv().await.ok_or("VCS input reader stopped")??;
  let response = execute_request(request, &mut input).await;
  write_response(&response)?;
  Ok(())
}

async fn execute_request(
  request: Request,
  input: &mut tokio::sync::mpsc::Receiver<Result<Request, String>>,
) -> Response {
  if let VcsCommand::Cancel(value) = &request.command {
    return response(
      &request,
      Outcome::Acknowledged {
        operation_id: value.target_operation_id.clone(),
      },
    );
  }
  let operation_id = operation_id(&request).to_owned();
  let cancellation = CancellationToken::new();
  let adapter_directory = std::env::current_dir();
  let operation = async {
    let adapter = match adapter_directory
      .ok()
      .and_then(|directory| GitVcsAdapter::load(&directory).ok())
    {
      Some(adapter) => adapter,
      None => {
        return Outcome::Failure(Failure {
          class: FailureClass::Permanent,
          code: "adapter_configuration_invalid".to_owned(),
          diagnostic: "Git adapter configuration is unavailable".to_owned(),
          retry_after_ms: None,
        });
      }
    };
    adapter.execute(&request, cancellation.clone()).await
  };
  tokio::pin!(operation);
  let mut input_open = true;
  let outcome = loop {
    tokio::select! {
      outcome = &mut operation => break outcome,
      next = input.recv(), if input_open => {
        match next {
          Some(Ok(cancel)) if matches_cancel(&cancel, &operation_id) => cancellation.cancel(),
          Some(Ok(_)) => {
            cancellation.cancel();
            break Outcome::Failure(Failure {
              class: FailureClass::ProtocolFault,
              code: "unexpected_command".to_owned(),
              diagnostic: "adapter received an unexpected command while an operation was active".to_owned(),
              retry_after_ms: None,
            });
          }
          Some(Err(_)) => {
            cancellation.cancel();
            break Outcome::Failure(Failure {
              class: FailureClass::ProtocolFault,
              code: "malformed_command".to_owned(),
              diagnostic: "adapter received a malformed command while an operation was active".to_owned(),
              retry_after_ms: None,
            });
          }
          None => input_open = false,
        }
      }
    }
  };
  response(&request, outcome)
}

fn request_stream() -> tokio::sync::mpsc::Receiver<Result<Request, String>> {
  let (sender, receiver) = tokio::sync::mpsc::channel(2);
  std::thread::spawn(move || {
    let stdin = std::io::stdin();
    let mut input = std::io::BufReader::new(stdin.lock());
    loop {
      let mut frame = Vec::new();
      let read = match input
        .by_ref()
        .take((MAX_VCS_MESSAGE_BYTES + 2) as u64)
        .read_until(b'\n', &mut frame)
      {
        Ok(read) => read,
        Err(_) => {
          let _ = sender.blocking_send(Err("failed to read VCS request".to_owned()));
          return;
        }
      };
      if read == 0 {
        return;
      }
      if frame.last() != Some(&b'\n') || frame.len() > MAX_VCS_MESSAGE_BYTES + 1 {
        let _ = sender.blocking_send(Err("invalid VCS request frame".to_owned()));
        return;
      }
      frame.pop();
      let decoded = decode_request(&frame).map_err(|_| "invalid VCS request".to_owned());
      if sender.blocking_send(decoded).is_err() {
        return;
      }
    }
  });
  receiver
}

fn response(request: &Request, mut outcome: Outcome) -> Response {
  let mut response = Response {
    protocol_version: VCS_PROTOCOL_VERSION,
    request_id: request.request_id.clone(),
    outcome,
  };
  if response.validate_for(request).is_err() {
    outcome = Outcome::Failure(Failure {
      class: FailureClass::ProtocolFault,
      code: "adapter_result_invalid".to_owned(),
      diagnostic: "Git adapter produced an invalid normalized result".to_owned(),
      retry_after_ms: None,
    });
    response.outcome = outcome;
  }
  response
}

fn operation_id(request: &Request) -> &str {
  match &request.command {
    VcsCommand::ListReferences(value) => &value.repository.operation_id,
    VcsCommand::ReadCommit(value) => &value.repository.operation_id,
    VcsCommand::ListTree(value) => &value.repository.operation_id,
    VcsCommand::ReadFile(value) => &value.repository.operation_id,
    VcsCommand::ResolveRevision(value) => &value.repository.operation_id,
    VcsCommand::Cancel(value) => &value.target_operation_id,
  }
}

fn matches_cancel(request: &Request, operation_id: &str) -> bool {
  matches!(&request.command, VcsCommand::Cancel(value) if value.target_operation_id == operation_id)
}

fn write_response(response: &Response) -> std::io::Result<()> {
  let mut frame = serde_json::to_vec(response).map_err(std::io::Error::other)?;
  if frame.len() > MAX_VCS_MESSAGE_BYTES {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidData,
      "VCS response exceeds the message limit",
    ));
  }
  frame.push(b'\n');
  let stdout = std::io::stdout();
  let mut output = stdout.lock();
  output.write_all(&frame)?;
  output.flush()
}
