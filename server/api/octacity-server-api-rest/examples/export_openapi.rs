//! Development-only exporter for the management API's live OpenAPI document.

use std::io::{self, Write as _};

use octacity_server_api_rest::v1::openapi_document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut stdout = io::BufWriter::new(io::stdout().lock());
  stdout.write_all(&render_openapi()?)?;
  Ok(())
}

fn render_openapi() -> serde_json::Result<Vec<u8>> {
  let mut document = serde_json::to_vec_pretty(&openapi_document())?;
  document.push(b'\n');
  Ok(document)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn export_is_deterministic_and_matches_the_live_document() {
    let first = render_openapi().unwrap();
    let second = render_openapi().unwrap();

    assert_eq!(first, second);
    assert_eq!(
      serde_json::from_slice::<serde_json::Value>(&first).unwrap(),
      openapi_document()
    );
  }
}
