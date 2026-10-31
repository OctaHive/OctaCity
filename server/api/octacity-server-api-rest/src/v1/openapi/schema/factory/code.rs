//! The same explicit source inventory and per-step narrowing used by headless authoring.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert(
    "FactoryRepositoryReference".into(),
    object(
      [
        ("project_id", non_empty_string()),
        ("id", non_empty_string()),
        ("version", positive_integer()),
        ("definition_digest", schema_ref("FactoryDigest")),
      ],
      &["project_id", "id", "version", "definition_digest"],
    ),
  );
  let path = json!({"type":"string","minLength":1,"maxLength":bounds.repository_path_bytes,
    "description":"Normalized relative file or directory path; no absolute path, empty, dot or parent segments."});
  schemas.insert(
    "FactoryRepositoryLineRange".into(),
    object(
      [
        ("first_line", bounded_integer(1, u32::MAX.into())),
        ("last_line", bounded_integer(1, u32::MAX.into())),
      ],
      &["first_line", "last_line"],
    ),
  );
  schemas.insert(
    "FactoryFlowCodeSelection".into(),
    json!({"oneOf":[
      tagged("kind","path",[("path",path.clone())],&["path"]),
      tagged("kind","range",[("path",path),("lines",schema_ref("FactoryRepositoryLineRange"))],&["path","lines"]),
    ]}),
  );
  let mut entries = bounded_array(schema_ref("FactoryFlowCodeSelection"), bounds.permission_entries);
  entries["minItems"] = json!(1);
  schemas.insert(
    "FactoryFlowCodeScope".into(),
    json!({"oneOf":[
      tagged("kind","repository",[],&[]),
      tagged("kind","selected",[("entries",entries)],&["entries"]),
    ]}),
  );
  let access = json!({"type":"string","enum":["read_only","read_write"]});
  schemas.insert(
    "FactoryFlowCodeSource".into(),
    object(
      [
        ("repository", schema_ref("FactoryRepositoryReference")),
        ("selection", schema_ref("ManualSource")),
        ("access", access.clone()),
        ("scope", schema_ref("FactoryFlowCodeScope")),
      ],
      &["repository", "selection", "access", "scope"],
    ),
  );
  schemas.insert(
    "FactoryFlowCodeRevision".into(),
    json!({"oneOf":[
      tagged("kind", "admitted_base", [], &[]),
      tagged("kind", "exact_revision", [("revision",non_empty_string())], &["revision"]),
      tagged("kind", "accepted_candidate", [("node",key()),("outcome",key())], &["node","outcome"]),
    ]}),
  );
  schemas.insert(
    "FactoryFlowCodeBinding".into(),
    json!({"oneOf":[
      tagged("kind","none",[],&[]),
      tagged("kind","source",[
        ("source",key()),("access",access),("scope",schema_ref("FactoryFlowCodeScope")),
        ("revision",schema_ref("FactoryFlowCodeRevision")),
      ],&["source","access","scope","revision"]),
    ]}),
  );
}
