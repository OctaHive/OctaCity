//! Shared process-wide tracing capture; assertions select their unique operation.
use std::sync::{Arc, Mutex, OnceLock};
pub fn capture() -> Arc<Mutex<Vec<u8>>> {
  static CAPTURE: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
  CAPTURE
    .get_or_init(|| {
      struct Writer(Arc<Mutex<Vec<u8>>>);
      impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
          self.0.lock().unwrap().extend_from_slice(bytes);
          Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
          Ok(())
        }
      }
      let captured = Arc::new(Mutex::new(Vec::new()));
      let writer = captured.clone();
      let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_writer(move || Writer(writer.clone()))
        .finish();
      // Thread-local dispatchers race callsite-interest caching in parallel Tokio tests.
      tracing::subscriber::set_global_default(subscriber).unwrap();
      captured
    })
    .clone()
}
