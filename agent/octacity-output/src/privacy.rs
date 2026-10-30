//! Checks exact frozen bytes without interpreting or rewriting producer evidence.
use crate::{OutputError, prepare::PreparedOutput};
use memchr::memmem::Finder;
use tokio::io::AsyncReadExt as _;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub(super) async fn validate(
  outputs: &[PreparedOutput],
  values: &[&[u8]],
  cancellation: &CancellationToken,
) -> Result<(), OutputError> {
  for output in outputs {
    let patterns = values
      .iter()
      .filter(|value| !value.is_empty() && value.len() as u64 <= output.metadata().size_bytes)
      .map(Finder::new)
      .collect::<Vec<_>>();
    let Some(maximum) = patterns.iter().map(|pattern| pattern.needle().len()).max() else {
      continue;
    };
    let mut file = tokio::fs::File::open(output.path())
      .await
      .map_err(|source| OutputError::Io {
        operation: "open frozen privacy input",
        path: output.path().to_owned(),
        source,
      })?;
    let mut chunk = Zeroizing::new(vec![0_u8; 64 * 1024]);
    let mut tail = Zeroizing::new(Vec::new());
    loop {
      let count = tokio::select! {
        biased;
        () = cancellation.cancelled() => return Err(OutputError::Cancelled),
        result = file.read(&mut chunk) => result.map_err(|source| OutputError::Io {
          operation: "read frozen privacy input", path: output.path().to_owned(), source,
        })?,
      };
      if count == 0 {
        break;
      }
      tail.extend_from_slice(&chunk[..count]);
      if patterns.iter().any(|pattern| pattern.find(&tail).is_some()) {
        return Err(OutputError::Invalid("private credential value in job output".into()));
      }
      let remove = tail.len().saturating_sub(maximum - 1);
      tail.drain(..remove);
    }
  }
  Ok(())
}
