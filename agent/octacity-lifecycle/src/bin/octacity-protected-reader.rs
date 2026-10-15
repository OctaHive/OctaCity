//! Single-operation worker; resource and mount confinement belong to its supervisor.
#![forbid(unsafe_code)]

use std::io::Write as _;

use octacity_private_fs::RootedReadDirectory;
use octacity_protocol::{MAX_PROTECTED_READ_REQUEST_BYTES, ProtectedReadWorkerRequestV1};

fn read_once(input: &mut impl std::io::Read) -> Option<Vec<u8>> {
  let mut length = [0; 4];
  input.read_exact(&mut length).ok()?;
  let length = u32::from_be_bytes(length) as usize;
  if length == 0 || length > MAX_PROTECTED_READ_REQUEST_BYTES {
    return None;
  }
  let mut bytes = vec![0; length];
  input.read_exact(&mut bytes).ok()?;
  let request: ProtectedReadWorkerRequestV1 = serde_json::from_slice(&bytes).ok()?;
  request.validate().ok()?;
  let root = RootedReadDirectory::open(&std::env::current_dir().ok()?).ok()?;
  root
    .read_bounded(std::path::Path::new(&request.relative_path), request.max_bytes)
    .ok()
}

fn main() {
  let response = read_once(&mut std::io::stdin().lock());
  let (status, bytes) = match response {
    Some(bytes) => (0, bytes),
    None => (1, Vec::new()),
  };
  let mut output = std::io::stdout().lock();
  // Errors intentionally contain neither paths nor input diagnostics.
  // The owner treats incomplete framing as refusal and tears down the worker.
  if output.write_all(&[status]).is_err()
    || output.write_all(&(bytes.len() as u32).to_be_bytes()).is_err()
    || output.write_all(&bytes).is_err()
  {
    std::process::exit(1);
  }
}
