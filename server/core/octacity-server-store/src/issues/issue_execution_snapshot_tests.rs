//! Persisted input bodies keep their original bytes and independently pinned digest.
use super::*;

#[test]
fn full_catalog_snapshot_retains_explicit_configuration_and_digest() {
  let bytes = include_str!("fixtures/issue_snapshot_full_catalog_v1.json")
    .trim_end()
    .as_bytes();
  let snapshot: IssueExecutionSnapshot = serde_json::from_slice(bytes).unwrap();
  assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
  assert_eq!(
    serde_json::to_value(snapshot.digest().unwrap()).unwrap(),
    "edfe5d9ce9f2c966c4b5c64e07b294ee88b1d1f2099f566eaea620551ff49ef7"
  );
}
