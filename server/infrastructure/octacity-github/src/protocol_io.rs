//! Bounded Delivery JSONL serving; no inherited environment carries authority.
use crate::{GithubPlugin, failure};
use octacity_delivery_protocol::{
  Command, DELIVERY_PROTOCOL_VERSION, FailureClass, MAX_DELIVERY_MESSAGE_BYTES, Outcome, Request, Response,
  decode_request,
};
use std::io::{BufRead as _, Read as _, Write as _};
use tokio_util::sync::CancellationToken;

/// Serves exactly one scoped Delivery request and same-operation cancellation.
/// Invalid initial frames terminate without uncorrelated output or diagnostics.
pub async fn serve() -> std::io::Result<()> {
  let (sender, mut input) = tokio::sync::mpsc::channel(2);
  std::thread::spawn(move || {
    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());
    loop {
      let mut bytes = Vec::new();
      match reader
        .by_ref()
        .take((MAX_DELIVERY_MESSAGE_BYTES + 2) as u64)
        .read_until(b'\n', &mut bytes)
      {
        Ok(0) => return,
        Ok(_) if bytes.last() == Some(&b'\n') && bytes.len() <= MAX_DELIVERY_MESSAGE_BYTES + 1 => {
          bytes.pop();
          if sender.blocking_send(decode_request(&bytes).map_err(|_| ())).is_err() {
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
  let request: Request = input.recv().await.and_then(Result::ok).ok_or_else(invalid)?;
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
        let operation = plugin.execute(&request, token.clone());
        tokio::pin!(operation);
        let mut open = true;
        loop {
          tokio::select! {
            result=&mut operation=>break result,
            next=input.recv(),if open=>{
              match next {
                Some(Ok(cancel)) if same_cancellation(&request,&cancel)=>token.cancel(),
                Some(_)=>{token.cancel();break failure(FailureClass::ProtocolFault);}
                None=>open=false,
              }
            }
          }
        }
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
  let mut bytes = serde_json::to_vec(&response).map_err(|_| invalid())?;
  if bytes.len() > MAX_DELIVERY_MESSAGE_BYTES {
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
  std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid Delivery frame")
}
