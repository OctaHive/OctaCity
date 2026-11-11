//! Persisted input bodies keep their original bytes and independently pinned digest.
use super::*;

#[test]
fn legacy_full_catalog_snapshot_preserves_its_wire_and_digest() {
  let bytes = include_str!("fixtures/issue_snapshot_full_catalog_v1.json")
    .trim_end()
    .as_bytes();
  let snapshot: IssueExecutionSnapshot = serde_json::from_slice(bytes).unwrap();
  assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
  assert_eq!(
    serde_json::to_value(snapshot.digest().unwrap()).unwrap(),
    "3d540183dabec64d1b93b3067898623b05b6063fe6c2a3bbe018a5d450ec6723"
  );
}
