//! Additive producer-independent input wire shared by preset, Flow and Configuration review.
use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  let materials = octacity_server_application::MAX_ISSUE_MATERIALS;
  let references = octacity_server_application::MAX_ISSUE_CLASSIFICATION_REFERENCES;
  let classification = schemas["FactoryWorkClassification"]["properties"].clone();
  let mapping = |value| {
    object(
      [("field", key()), ("values", bounded_map(value, bounds.schema_entries))],
      &["field", "values"],
    )
  };
  schemas.insert(
    "FactoryFlowIssueAdmissionPolicy".into(),
    object(
      [
        ("priority", mapping(classification["priority"].clone())),
        ("risk", mapping(classification["risk"].clone())),
        (
          "allowed",
          bounded_map(
            bounded_array(non_empty_string(), bounds.schema_entries),
            bounds.schema_entries,
          ),
        ),
      ],
      &["priority", "risk"],
    ),
  );
  schemas.insert(
    "FactoryPresetIssueSnapshotSelection".into(),
    object(
      [
        ("definition_version", non_negative_integer()),
        ("admission", nullable(schema_ref("FactoryFlowIssueAdmissionPolicy"))),
      ],
      &["definition_version"],
    ),
  );
  let mut sources = [
    "title",
    "description",
    "acceptance",
    "priority",
    "component",
    "type",
    "size",
    "dependencies",
  ]
  .map(|kind| tagged("kind", kind, [], &[]))
  .to_vec();
  sources.push(tagged("kind", "property", [("id", non_empty_string())], &["id"]));
  sources.push(tagged("kind", "classification", [("slot", key())], &["slot"]));
  schemas.insert(
    "FactoryFlowIssueFieldSelection".into(),
    object(
      [("source", json!({"oneOf":sources})), ("required", boolean())],
      &["source", "required"],
    ),
  );
  schemas.insert("FactoryFlowIssueMaterialReference".into(),json!({"oneOf":[
    tagged("kind","comment",[("comment_id",json!({"type":"string","format":"uuid"})),("body_digest",schema_ref("FactoryDigest"))],&["comment_id","body_digest"]),
    tagged("kind","issue_document",[("artifact",schema_ref("FactoryArtifactReference"))],&["artifact"]),
    tagged("kind","project_document",[("artifact",schema_ref("FactoryArtifactReference"))],&["artifact"]),
  ]}));
  let mut exact = bounded_array(schema_ref("FactoryFlowIssueMaterialReference"), materials);
  exact["minItems"] = json!(1);
  schemas.insert(
    "FactoryFlowIssueMaterialSelection".into(),
    object(
      [
        (
          "source",
          json!({"oneOf":[
            tagged("kind","exact",[("references",exact)],&["references"]),
            tagged("kind","selected",[("max_entries",bounded_integer(1,materials as u64))],&["max_entries"])
          ]}),
        ),
        ("required", boolean()),
        ("max_bytes", bounded_integer(1, bounds.value_bytes as u64)),
      ],
      &["source", "required", "max_bytes"],
    ),
  );
  schemas.insert(
    "FactoryFlowIssueEvidenceSelection".into(),
    object(
      [
        ("field", key()),
        ("required", boolean()),
        ("maximum", bounded_integer(1, references as u64)),
      ],
      &["field", "required", "maximum"],
    ),
  );
  schemas.insert(
    "FactoryFlowIssueSnapshotInput".into(),
    object(
      [
        ("version", json!({"type":"integer","const":1})),
        ("schema", schema_ref("FactoryImmutableReferenceResource")),
        ("definition_version", non_negative_integer()),
        ("admission", schema_ref("FactoryFlowIssueAdmissionPolicy")),
        (
          "fields",
          bounded_map(schema_ref("FactoryFlowIssueFieldSelection"), bounds.schema_entries),
        ),
        (
          "materials",
          bounded_map(schema_ref("FactoryFlowIssueMaterialSelection"), materials),
        ),
        (
          "evidence",
          bounded_map(schema_ref("FactoryFlowIssueEvidenceSelection"), bounds.schema_entries),
        ),
      ],
      &["version", "schema", "definition_version", "fields"],
    ),
  );
}
