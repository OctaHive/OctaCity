//! Closed wire grammar and authoritative bounds shared with planning policy.
use super::super::*;
use octacity_server_application::{
  MAX_ISSUE_ASSIGNEES, MAX_ISSUE_DOCUMENT_BYTES, MAX_ISSUE_LABELS, MAX_ISSUE_PROPERTIES, MAX_ISSUE_PROPERTY_CHOICES,
  MAX_ISSUE_PROPERTY_KEY_BYTES, MAX_ISSUE_PROPERTY_NUMBER, MAX_ISSUE_PROPERTY_TEXT_BYTES, MAX_ISSUE_TITLE_BYTES,
};

fn key() -> Value {
  json!({"type":"string","minLength":1,"maxLength":MAX_ISSUE_PROPERTY_KEY_BYTES,"pattern":"^[a-z0-9_]+$","x-max-utf8-bytes":MAX_ISSUE_PROPERTY_KEY_BYTES})
}
fn label() -> Value {
  json!({"type":"string","minLength":1,"maxLength":MAX_ISSUE_TITLE_BYTES,"x-max-utf8-bytes":MAX_ISSUE_TITLE_BYTES})
}
fn date() -> Value {
  json!({"type":"string","format":"date","minLength":10,"maxLength":10,"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"Valid Gregorian date YYYY-MM-DD, years 0001–9999; no time or timezone."})
}
fn number() -> Value {
  json!({"type":"integer","minimum":-MAX_ISSUE_PROPERTY_NUMBER,"maximum":MAX_ISSUE_PROPERTY_NUMBER})
}
fn tagged(kind: &str, fields: impl IntoIterator<Item = (&'static str, Value)>, required: &[&str]) -> Value {
  let mut fields: Vec<_> = fields.into_iter().collect();
  fields.push(("type", json!({"type":"string","const":kind})));
  let mut required = required.to_vec();
  required.push("type");
  object(fields, &required)
}
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "IssuePropertyChoice".into(),
    object(
      [("id", key()), ("name", label()), ("retired", boolean())],
      &["id", "name", "retired"],
    ),
  );
  schemas.insert("IssuePropertyKind".into(),json!({"oneOf":[
    tagged("text",[("max_bytes",json!({"type":"integer","minimum":1,"maximum":MAX_ISSUE_PROPERTY_TEXT_BYTES}))],&["max_bytes"]),
    tagged("choice",[("choices",json!({"type":"array","minItems":1,"maxItems":MAX_ISSUE_PROPERTY_CHOICES,"items":schema_ref("IssuePropertyChoice")}))],&["choices"]),
    tagged("boolean",[],&[]),tagged("number",[("minimum",number()),("maximum",number())],&["minimum","maximum"]),tagged("date",[],&[])
  ]}));
  schemas.insert(
    "IssuePropertyDefinition".into(),
    object(
      [
        ("id", key()),
        ("name", label()),
        ("retired", boolean()),
        ("kind", schema_ref("IssuePropertyKind")),
      ],
      &["id", "name", "retired", "kind"],
    ),
  );
  let mut definition = object(
    [
      (
        "labels",
        json!({"type":"array","maxItems":MAX_ISSUE_PROPERTY_CHOICES,"items":schema_ref("IssuePropertyChoice")}),
      ),
      (
        "properties",
        json!({"type":"array","maxItems":MAX_ISSUE_PROPERTIES,"items":schema_ref("IssuePropertyDefinition")}),
      ),
    ],
    &["labels", "properties"],
  );
  definition["x-max-json-utf8-bytes"] = json!(MAX_ISSUE_DOCUMENT_BYTES);
  definition["description"] = json!(
    "Unique stable keys and kinds are retained; retire instead of removing. Reserved status/execution/authority setters are forbidden. Bounds and dates are validated by Project policy."
  );
  schemas.insert("IssuePropertyDefinitionSet".into(), definition);
  schemas.insert("IssuePropertyValue".into(),json!({"oneOf":[
    tagged("text",[("value",json!({"type":"string","maxLength":MAX_ISSUE_PROPERTY_TEXT_BYTES,"x-max-utf8-bytes":MAX_ISSUE_PROPERTY_TEXT_BYTES}))],&["value"]),
    tagged("choice",[("value",key())],&["value"]),tagged("boolean",[("value",boolean())],&["value"]),tagged("number",[("value",number())],&["value"]),tagged("date",[("value",date())],&["value"])
  ]}));
  schemas.insert("IssuePlanning".into(),object([
    ("definition_version",non_negative_integer()),("assignee",nullable(label())),
    ("labels",json!({"type":"array","maxItems":MAX_ISSUE_LABELS,"uniqueItems":true,"items":key()})),
    ("due_date",nullable(date())),("parent_id",nullable(json!({"type":"string","format":"uuid"}))),
    ("values",json!({"type":"object","maxProperties":MAX_ISSUE_PROPERTIES,"propertyNames":key(),"additionalProperties":schema_ref("IssuePropertyValue")}))
  ],&["definition_version","assignee","labels","due_date","parent_id","values"]));
  schemas.insert(
    "PublishedIssueProperties".into(),
    object(
      [
        ("project_id", json!({"type":"string","format":"uuid"})),
        ("version", non_negative_integer()),
        ("definition", schema_ref("IssuePropertyDefinitionSet")),
        ("published_at_unix_ms", non_negative_integer()),
      ],
      &["project_id", "version", "definition", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "IssueAssignee".into(),
    object([("id", label()), ("name", label())], &["id", "name"]),
  );
  schemas.insert(
    "IssueAssigneeChoices".into(),
    object(
      [
        ("available", boolean()),
        (
          "items",
          json!({"type":"array","maxItems":MAX_ISSUE_ASSIGNEES,"items":schema_ref("IssueAssignee")}),
        ),
      ],
      &["available", "items"],
    ),
  );
  schemas.insert(
    "IssuePropertyChoices".into(),
    object(
      [
        ("definitions", schema_ref("PublishedIssueProperties")),
        ("assignees", schema_ref("IssueAssigneeChoices")),
      ],
      &["definitions", "assignees"],
    ),
  );
  schemas.insert(
    "PublishIssuePropertiesRequest".into(),
    object(
      [
        ("expected_version", non_negative_integer()),
        ("definition", schema_ref("IssuePropertyDefinitionSet")),
      ],
      &["expected_version", "definition"],
    ),
  );
  schemas.insert(
    "IssuePropertiesMutationResponse".into(),
    mutation_response("PublishedIssueProperties"),
  );
  // Creation keeps the original flat body; editing requirements cannot set planning.
  let mut create = schemas["IssueContent"].clone();
  create["properties"]["planning"] = schema_ref("IssuePlanning");
  schemas.insert("CreateIssueRequest".into(), create);
  schemas.get_mut("IssueResource").unwrap()["properties"]["planning"] = schema_ref("IssuePlanning");
}
