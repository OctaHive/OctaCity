//! Qualification evidence is public metadata; transient credential material belongs only to private setup.
use super::*;

pub(super) const READER: &str = "packaged-fixture-reader-canary";
pub(super) const WRITER: &str = "packaged-fixture-writer-canary";

pub(super) fn verify(root: &Path) -> Result<(), &'static str> {
  let mut directories = vec![root.to_path_buf()];
  while let Some(directory) = directories.pop() {
    for entry in fs::read_dir(directory).map_err(|_| "qualification evidence unreadable")? {
      let entry = entry.map_err(|_| "qualification evidence unreadable")?;
      let kind = entry.file_type().map_err(|_| "qualification evidence unreadable")?;
      if kind.is_dir() {
        directories.push(entry.path());
      } else if kind.is_file() {
        let bytes = fs::read(entry.path()).map_err(|_| "qualification evidence unreadable")?;
        if [READER, WRITER]
          .iter()
          .any(|secret| bytes.windows(secret.len()).any(|part| part == secret.as_bytes()))
        {
          return Err("credential material found in qualification evidence");
        }
      }
    }
  }
  Ok(())
}

#[test]
fn source_evidence_rejects_credential_material_in_nested_public_results_and_logs() {
  let root = tempfile::tempdir().unwrap();
  fs::create_dir(root.path().join("browser")).unwrap();
  fs::write(
    root.path().join("browser/projection.json"),
    br#"{"error":"packaged-fixture-writer-canary"}"#,
  )
  .unwrap();
  assert!(
    verify(root.path()).is_err(),
    "qualification cannot retain private writer material in a public projection"
  );
  fs::remove_file(root.path().join("browser/projection.json")).unwrap();
  fs::write(root.path().join("server.log"), b"packaged-fixture-reader-canary").unwrap();
  assert!(
    verify(root.path()).is_err(),
    "credential material cannot reach process logs"
  );
  fs::write(root.path().join("server.log"), br#"{"operation":"delivered"}"#).unwrap();
  verify(root.path()).unwrap();
}
