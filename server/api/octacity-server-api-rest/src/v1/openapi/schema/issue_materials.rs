//! Shared closed source pins and metadata; selection never contains bodies or claimed attribution.
use super::super::*;
use super::factory::{bounded_array, tagged};
use octacity_server_application::MAX_ISSUE_MATERIALS;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  schemas.insert(
    "EditIssueMaterialsRequest".into(),
    object(
      [(
        "references",
        bounded_array(schema_ref("IssueMaterialReference"), MAX_ISSUE_MATERIALS),
      )],
      &["references"],
    ),
  );
  schemas.get_mut("IssueResource").unwrap()["properties"]["materials"] =
    bounded_array(schema_ref("IssueMaterialSelection"), MAX_ISSUE_MATERIALS);
  schemas.insert(
    "IssueMaterialSelection".into(),
    object(
      [
        ("reference", schema_ref("IssueMaterialReference")),
        ("selected_by", schema_ref("AuditActor")),
        ("selected_at", non_negative_integer()),
      ],
      &["reference", "selected_by", "selected_at"],
    ),
  );
  // Only retained source pins are exposed here; source bodies keep independent readers.
  schemas.insert("IssueMaterialReference".into(),json!({"oneOf":[
    tagged("kind","comment",[("project_id",id()),("issue_id",id()),("comment_id",id()),("body_digest",schema_ref("FactoryDigest"))],&["project_id","issue_id","comment_id","body_digest"]),
    tagged("kind","issue_document",[("project_id",id()),("issue_id",id()),("artifact",schema_ref("FactoryArtifactReference"))],&["project_id","issue_id","artifact"]),
    tagged("kind","project_document",[("project_id",id()),("artifact",schema_ref("FactoryArtifactReference"))],&["project_id","artifact"])
  ]}));
}
