//! One bounded selected-field contract shared by schema generation and guarded saves.
use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let property = json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_PROPERTY_KEY_BYTES,"pattern":"^[a-z0-9_]+$"});
  let mut targets = ["priority", "component", "type", "size"]
    .map(|kind| tagged("kind", kind, [], &[]))
    .to_vec();
  targets.push(tagged("kind", "property", [("id", property)], &["id"]));
  schemas.insert("FactoryIssueFieldTarget".into(), json!({"oneOf":targets}));
  let mut fields = bounded_array(
    object(
      [
        ("field", schema_ref("FactoryIssueFieldTarget")),
        ("required", json!({"type":"boolean","default":true})),
      ],
      &["field"],
    ),
    octacity_server_application::MAX_ISSUE_PROPERTIES + 4,
  );
  fields["minItems"] = json!(1);
  schemas.insert(
    "FactoryIssueFieldForm".into(),
    object(
      [
        ("project_id", json!({"type":"string","format":"uuid"})),
        ("definition_version", non_negative_integer()),
        ("fields", fields),
      ],
      &["project_id", "definition_version", "fields"],
    ),
  );
  schemas.insert(
    "FactoryFlowIssueFieldWrite".into(),
    object(
      [
        ("writer", schema_ref("FactoryImmutableReferenceResource")),
        ("policy", key()),
        ("form", schema_ref("FactoryIssueFieldForm")),
      ],
      &["writer", "policy", "form"],
    ),
  );
  schemas.insert(
    "FactoryIssueFieldApplication".into(),
    object(
      [
        ("form", schema_ref("FactoryIssueFieldForm")),
        ("operation_id", schema_ref("FactoryDigest")),
        (
          "status",
          json!({"type":"string","enum":["pending","applied","conflict","denied","failed"]}),
        ),
        ("delivery_record", schema_ref("FactoryDigest")),
      ],
      &["form", "operation_id", "status"],
    ),
  );
}
