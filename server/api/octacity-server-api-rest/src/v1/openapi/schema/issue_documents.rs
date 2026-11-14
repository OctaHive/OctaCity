//! Closed Issue publication contracts; ownership and attribution come from the route and context.
use super::super::*;
use super::factory::bounded_array;
use octacity_server_application::{MAX_PROJECT_DOCUMENT_BYTES, MAX_PROJECT_DOCUMENT_NAME_BYTES};
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "IssueDocumentPage".into(),
    object(
      [
        (
          "items",
          bounded_array(
            schema_ref("IssueDocumentResource"),
            octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE as usize,
          ),
        ),
        ("next_cursor", nullable(json!({"type":"string","format":"uuid"}))),
      ],
      &["items", "next_cursor"],
    ),
  );
  let content = json!({
    "type":"string", "minLength":1, "maxLength":MAX_PROJECT_DOCUMENT_BYTES,
    "x-max-utf8-bytes":MAX_PROJECT_DOCUMENT_BYTES
  });
  schemas.insert(
    "IssueDocumentContent".into(),
    object(
      [
        ("document", schema_ref("IssueDocumentResource")),
        ("content", content.clone()),
      ],
      &["document", "content"],
    ),
  );
  schemas.insert(
    "PublishIssueDocumentRequest".into(),
    object(
      [
        (
          "name",
          json!({
            "type":"string", "minLength":1, "maxLength":MAX_PROJECT_DOCUMENT_NAME_BYTES,
            "x-max-utf8-bytes":MAX_PROJECT_DOCUMENT_NAME_BYTES
          }),
        ),
        ("content", content),
      ],
      &["name", "content"],
    ),
  );
  schemas.insert(
    "IssueDocumentResource".into(),
    object(
      [
        ("project_id", json!({"type":"string", "format":"uuid"})),
        ("issue_id", json!({"type":"string", "format":"uuid"})),
        (
          "name",
          json!({"type":"string", "minLength":1, "maxLength":MAX_PROJECT_DOCUMENT_NAME_BYTES}),
        ),
        ("artifact", schema_ref("FactoryArtifactReference")),
        ("actor", schema_ref("AuditActor")),
        ("published_at", non_negative_integer()),
        ("expires_at", non_negative_integer()),
      ],
      &[
        "project_id",
        "issue_id",
        "name",
        "artifact",
        "actor",
        "published_at",
        "expires_at",
      ],
    ),
  );
}
