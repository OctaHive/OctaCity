//! Bounded provider JSONL serving; protocol families never inherit another role's authority.
use crate::{GithubPlugin, failure};
use octacity_delivery_protocol::{
  Command, DELIVERY_PROTOCOL_VERSION, FailureClass, MAX_DELIVERY_MESSAGE_BYTES, Outcome, Request, Response,
  decode_request,
};
use std::io::{BufRead as _, Read as _, Write as _};
use tokio_util::sync::CancellationToken;

/// Serves one independently scoped protocol operation and same-operation cancellation.
/// Invalid initial frames terminate without uncorrelated output or diagnostics.
pub async fn serve() -> std::io::Result<()> {
  let (sender, mut input) = tokio::sync::mpsc::channel(2);
  std::thread::spawn(move || {
    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());
    let maximum = MAX_DELIVERY_MESSAGE_BYTES.max(octacity_work_source_protocol::MAX_REQUEST_BYTES);
    loop {
      let mut bytes = Vec::new();
      match reader.by_ref().take((maximum + 2) as u64).read_until(b'\n', &mut bytes) {
        Ok(0) => return,
        Ok(_) if bytes.last() == Some(&b'\n') && bytes.len() <= maximum + 1 => {
          bytes.pop();
          if sender.blocking_send(Ok(bytes)).is_err() {
            return;
          }
        }
        _ => {
          let _ = sender.blocking_send(Err(()));
          return;
        }
      }
    }
  });
  let first = input.recv().await.and_then(Result::ok).ok_or_else(invalid)?;
  if let Ok(request) = octacity_work_source_protocol::decode_request(&first) {
    let directory = std::env::current_dir()?;
    let token = CancellationToken::new();
    let response = run_operation(
      crate::work_protocol::execute(&request, &directory, token.clone()),
      &mut input,
      token,
      |bytes| crate::work_protocol::same_cancellation(&request, bytes),
      || crate::work_protocol::response(&request, octacity_work_source_protocol::FailureClass::ProtocolFault),
    )
    .await;
    response.validate_for(&request).map_err(|_| invalid())?;
    return write_response(
      serde_json::to_vec(&response).map_err(|_| invalid())?,
      octacity_work_source_protocol::MAX_RESPONSE_BYTES,
    );
  }
  let request: Request = decode_request(&first).map_err(|_| invalid())?;
  let outcome = if let Command::Cancel { target_delivery_id } = &request.command {
    Outcome::Acknowledged {
      delivery_id: target_delivery_id.clone(),
    }
  } else {
    let directory = std::env::current_dir()?;
    match GithubPlugin::load(&directory) {
      Err(_) => failure(FailureClass::Permanent),
      Ok(plugin) => {
        let token = CancellationToken::new();
        run_operation(
          plugin.execute(&request, token.clone()),
          &mut input,
          token,
          |bytes| decode_request(bytes).is_ok_and(|cancel| same_cancellation(&request, &cancel)),
          || failure(FailureClass::ProtocolFault),
        )
        .await
      }
    }
  };
  let mut response = Response {
    protocol_version: DELIVERY_PROTOCOL_VERSION,
    request_id: request.request_id.clone(),
    outcome,
  };
  if response.validate_for(&request).is_err() {
    response.outcome = failure(FailureClass::ProtocolFault);
  }
  write_response(
    serde_json::to_vec(&response).map_err(|_| invalid())?,
    MAX_DELIVERY_MESSAGE_BYTES,
  )
}
async fn run_operation<T>(
  operation: impl std::future::Future<Output = T>,
  input: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, ()>>,
  token: CancellationToken,
  same_cancellation: impl Fn(&[u8]) -> bool,
  protocol_fault: impl Fn() -> T,
) -> T {
  tokio::pin!(operation);
  let mut open = true;
  loop {
    tokio::select! {
      result=&mut operation=>return result,
      next=input.recv(),if open=>{
        match next {
          Some(Ok(bytes)) if same_cancellation(&bytes)=>token.cancel(),
          Some(_)=>{token.cancel();return protocol_fault();}
          None=>open=false,
        }
      }
    }
  }
}
fn write_response(mut bytes: Vec<u8>, maximum: usize) -> std::io::Result<()> {
  if bytes.len() > maximum {
    return Err(invalid());
  }
  bytes.push(b'\n');
  let stdout = std::io::stdout();
  let mut output = stdout.lock();
  output.write_all(&bytes)?;
  output.flush()
}
fn same_cancellation(original: &Request, cancel: &Request) -> bool {
  let id = match &original.command {
    Command::Observe(intent) | Command::Publish(intent) => &intent.delivery_id,
    Command::Cancel { target_delivery_id } => target_delivery_id,
  };
  matches!(&cancel.command,Command::Cancel {target_delivery_id} if target_delivery_id==id)
}
fn invalid() -> std::io::Error {
  std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid provider frame")
}
