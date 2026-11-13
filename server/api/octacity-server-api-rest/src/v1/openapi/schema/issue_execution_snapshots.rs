//! Closed exact-input review/detail; source bodies are only returned after independent authorization.
use super::super::*;
use super::factory::{bounded_array, bounded_map, tagged};
use octacity_server_application::{
  MAX_ISSUE_CLASSIFICATION_FIELDS, MAX_ISSUE_CLASSIFICATION_REFERENCES, MAX_ISSUE_MATERIALS, MAX_ISSUE_PROPERTIES,
  MAX_ISSUE_PROPERTY_CHOICES, MAX_ISSUE_PROPERTY_NUMBER, MAX_PROJECT_DOCUMENT_BYTES,
};

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  let text =
    || json!({"type":"string","maxLength":MAX_PROJECT_DOCUMENT_BYTES,"x-max-utf8-bytes":MAX_PROJECT_DOCUMENT_BYTES});
  schemas.insert(
    "IssueSnapshotValue".into(),
    // Empty material and dependency collections share the same JSON value.
    // Their meaning comes from the pinned alias contract, so this union is inclusive.
    json!({"anyOf":[text(),boolean(),
      json!({"type":"integer","minimum":-MAX_ISSUE_PROPERTY_NUMBER,"maximum":MAX_ISSUE_PROPERTY_NUMBER}),
      bounded_array(text(),MAX_ISSUE_MATERIALS),
      bounded_array(schema_ref("IssueClassificationDependencyReference"),MAX_ISSUE_CLASSIFICATION_REFERENCES)
    ]}),
  );
  schemas.insert(
    "IssueSnapshotPropertyDefinition".into(),
    object(
      [
        ("definition", schema_ref("IssuePropertyDefinition")),
        ("classification_slot", super::factory::key()),
      ],
      &["definition"],
    ),
  );
  let selected = object(
    [
      ("project_id", id()),
      ("version", non_negative_integer()),
      ("publication_digest", schema_ref("FactoryDigest")),
      ("published_at", non_negative_integer()),
      (
        "priorities",
        bounded_array(schema_ref("IssuePropertyChoice"), MAX_ISSUE_PROPERTY_CHOICES),
      ),
      (
        "components",
        bounded_array(schema_ref("IssuePropertyChoice"), MAX_ISSUE_PROPERTY_CHOICES),
      ),
      (
        "types",
        bounded_array(schema_ref("IssuePropertyChoice"), MAX_ISSUE_PROPERTY_CHOICES),
      ),
      (
        "properties",
        bounded_array(schema_ref("IssueSnapshotPropertyDefinition"), MAX_ISSUE_PROPERTIES),
      ),
    ],
    &["project_id", "version", "publication_digest", "published_at"],
  );
  let legacy = object(
    [
      ("project_id", id()),
      ("version", non_negative_integer()),
      ("definition", schema_ref("IssuePropertyDefinitionSet")),
      ("published_at", non_negative_integer()),
    ],
    &["project_id", "version", "definition", "published_at"],
  );
  schemas.insert(
    "IssueSnapshotDefinitions".into(),
    json!({"oneOf":[object([("selected_v1",selected)],&["selected_v1"]),legacy]}),
  );
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
  schemas.insert("IssueDocumentResource".into(),object([
    ("project_id",id()),("issue_id",id()),("name",json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_PROJECT_DOCUMENT_NAME_BYTES})),
    ("artifact",schema_ref("FactoryArtifactReference")),("actor",schema_ref("AuditActor")),("published_at",non_negative_integer()),("expires_at",non_negative_integer())
  ],&["project_id","issue_id","name","artifact","actor","published_at","expires_at"]));
  schemas.insert(
    "IssueSnapshotMaterial".into(),
    object(
      [
        ("selection", schema_ref("IssueMaterialSelection")),
        (
          "source",
          json!({"oneOf":[
            tagged("kind","comment",[("comment",schema_ref("IssueCommentMetadata"))],&["comment"]),
            tagged("kind","issue_document",[("document",schema_ref("IssueDocumentResource"))],&["document"]),
            tagged("kind","project_document",[("document",schema_ref("ProjectDocumentResource"))],&["document"])
          ]}),
        ),
      ],
      &["selection", "source"],
    ),
  );
  let mut definitions = bounded_map(
    schema_ref("IssueSnapshotDefinitions"),
    MAX_ISSUE_CLASSIFICATION_FIELDS + 1,
  );
  definitions["propertyNames"] = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$"});
  let mut snapshot = object(
    [
      ("project_id", id()),
      ("issue_id", id()),
      ("version", positive_integer()),
      ("applicability_version", positive_integer()),
      ("target", schema_ref("IssueExecutionTarget")),
      ("title_digest", schema_ref("FactoryDigest")),
      ("description_digest", schema_ref("FactoryDigest")),
      ("acceptance_digest", schema_ref("FactoryDigest")),
      ("input", schema_ref("FactoryFlowIssueSnapshotInput")),
      ("definition_head", non_negative_integer()),
      ("admission_classification", schema_ref("FactoryWorkClassification")),
      ("definitions", definitions),
      (
        "values",
        bounded_map(schema_ref("IssueSnapshotValue"), bounds.schema_entries),
      ),
      (
        "attribution",
        bounded_map(schema_ref("IssueClassificationAttribution"), bounds.schema_entries),
      ),
      (
        "omissions",
        bounded_map(
          string_enum(&["missing", "unavailable", "inapplicable"]),
          bounds.schema_entries,
        ),
      ),
      (
        "materials",
        bounded_map(
          bounded_array(schema_ref("IssueSnapshotMaterial"), MAX_ISSUE_MATERIALS),
          MAX_ISSUE_MATERIALS,
        ),
      ),
    ],
    &[
      "project_id",
      "issue_id",
      "version",
      "applicability_version",
      "target",
      "title_digest",
      "description_digest",
      "acceptance_digest",
      "input",
      "definitions",
      "values",
      "attribution",
      "omissions",
    ],
  );
  snapshot["x-max-json-utf8-bytes"] = json!(MAX_PROJECT_DOCUMENT_BYTES);
  snapshot["description"] = json!(
    "Exact server-resolved Issue selection under the pinned consumer contract. Returned values require current owner/body authorization; no input, evidence or policy authority is accepted from the client."
  );
  schemas.insert("IssueExecutionSnapshot".into(), snapshot);
  schemas.insert(
    "IssueExecutionReview".into(),
    object(
      [
        ("version", positive_integer()),
        ("target", schema_ref("IssueExecutionTarget")),
        ("snapshot_digest", nullable(schema_ref("FactoryDigest"))),
        ("snapshot", nullable(schema_ref("IssueExecutionSnapshot"))),
      ],
      &["version", "target", "snapshot_digest", "snapshot"],
    ),
  );
  schemas.insert("IssueExecutionSnapshotObservation".into(),json!({"oneOf":[
    tagged("availability","not_selected",[],&[]),
    tagged("availability","unavailable",[("digest",schema_ref("FactoryDigest"))],&["digest"]),
    tagged("availability","available",[("digest",schema_ref("FactoryDigest")),("snapshot",schema_ref("IssueExecutionSnapshot")),("subject",nullable(schema_ref("IssueClassificationSubject")))],&["digest","snapshot","subject"])
  ]}));
  schemas.get_mut("RequestIssueExecutionRequest").unwrap()["description"] = json!(
    "Exact reviewed target; policy-driven Issue snapshots omit classification and predecessor_run. Legacy/exact producer-result targets still require their explicit classification and eligible producer Run where declared. The server validates the selected target mode."
  );
}
