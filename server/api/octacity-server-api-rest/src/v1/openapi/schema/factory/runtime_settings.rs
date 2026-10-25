//! Runtime authoring shares installed toolchain, integration and permission schemas.
use super::*;
use octacity_server_application::{
  MAX_FACTORY_MCP_ARGUMENT_BYTES, MAX_FACTORY_MCP_CONFIGURATION_BYTES, MAX_FACTORY_MCP_SELECTION_ENTRIES,
  MAX_FACTORY_MCP_SERVER_NAME_BYTES, MAX_FACTORY_MCP_TOOL_NAME_BYTES, MAX_PERMISSION_PATH_BYTES,
};

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  let environment =
    || json!({"type":"string","minLength":1,"maxLength":bounds.key_bytes,"pattern":"^[A-Za-z_][A-Za-z0-9_]*$"});
  let tools = || {
    nullable(
      json!({"type":"array","uniqueItems":true,"maxItems":MAX_FACTORY_MCP_SELECTION_ENTRIES,"items":{"type":"string","minLength":1,"maxLength":MAX_FACTORY_MCP_TOOL_NAME_BYTES}}),
    )
  };
  schemas.insert("FactoryMcpServer".into(), json!({"oneOf":[
    tagged("transport","http",[
      ("url",json!({"type":"string","minLength":1,"maxLength":MAX_FACTORY_MCP_CONFIGURATION_BYTES})),
      ("bearer_token_env_var",nullable(environment())),("enabled_tools",tools())
    ], &["url"]),
    tagged("transport","stdio",[
      ("command",json!({"type":"string","minLength":1,"maxLength":MAX_PERMISSION_PATH_BYTES,"pattern":"^/"})),
      ("args",bounded_array(json!({"type":"string","maxLength":MAX_FACTORY_MCP_ARGUMENT_BYTES}),bounds.command_arguments)),
      ("env_vars",json!({"type":"array","maxItems":MAX_FACTORY_MCP_SELECTION_ENTRIES,"uniqueItems":true,"items":environment()})),
      ("enabled_tools",tools())
    ], &["command"])
  ]}));
  let mut servers = bounded_map(
    object(
      [
        ("server", schema_ref("FactoryMcpServer")),
        ("integration", nullable(key())),
      ],
      &["server"],
    ),
    bounds.role_entries,
  );
  servers["propertyNames"] = json!({"type":"string","minLength":1,"maxLength":MAX_FACTORY_MCP_SERVER_NAME_BYTES,"pattern":"^[a-z0-9][a-z0-9_-]*$"});
  let mut definition = object(
    [
      ("toolchain", schema_ref("FactoryRoleToolchain")),
      ("mcp_servers", servers),
      (
        "integrations",
        bounded_map(schema_ref("FactoryRoleIntegrationBinding"), bounds.role_entries),
      ),
      ("permissions", schema_ref("FactoryPermissions")),
    ],
    &["toolchain", "mcp_servers", "integrations", "permissions"],
  );
  definition["x-max-encoded-bytes"] = json!(bounds.role_definition_bytes);
  schemas.insert("FactoryRuntimeSettingsDefinition".into(), definition);
  schemas.insert(
    "FactoryRuntimeSettingsRequest".into(),
    object(
      [
        ("expected_version", nullable(positive_integer())),
        ("definition", schema_ref("FactoryRuntimeSettingsDefinition")),
      ],
      &["definition"],
    ),
  );
  schemas.insert(
    "FactoryRuntimeSettings".into(),
    object(
      [
        ("project_id", json!({"type":"string","format":"uuid"})),
        ("runtime", key()),
        ("version", positive_integer()),
        ("definition", schema_ref("FactoryRuntimeSettingsDefinition")),
        ("digest", schema_ref("FactoryDigest")),
      ],
      &["project_id", "runtime", "version", "definition", "digest"],
    ),
  );
  schemas.insert(
    "FactoryRuntimeSettingsRead".into(),
    object(
      [("settings", nullable(schema_ref("FactoryRuntimeSettings")))],
      &["settings"],
    ),
  );
  schemas.insert(
    "FactoryRuntimeSettingsDetails".into(),
    object([("settings", schema_ref("FactoryRuntimeSettings"))], &["settings"]),
  );
  schemas.insert(
    "FactoryRuntimeSettingsPublication".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("settings", schema_ref("FactoryRuntimeSettings")),
        ("published_at", integer()),
      ],
      &["disposition", "settings", "published_at"],
    ),
  );
  schemas.insert("FactoryRuntimeSettingsSupport".into(),object([
    ("mcp_transports",json!({"type":"array","uniqueItems":true,"maxItems":2,"items":{"type":"string","enum":["http","stdio"]}})),
    ("mcp_executables",json!({"type":"object","maxProperties":bounds.role_entries,"propertyNames":{"type":"string","minLength":1,"maxLength":MAX_PERMISSION_PATH_BYTES,"pattern":"^/"},"additionalProperties":schema_ref("FactoryImmutableReferenceResource")})),
  ], &["mcp_transports","mcp_executables"]));
  schemas.insert("FactoryRuntimeChoice".into(),object([
    ("alias",key()),("toolchain",schema_ref("FactoryRoleToolchain")),("support",schema_ref("FactoryRuntimeSettingsSupport")),
    ("permissions",schema_ref("FactoryPermissions")),
    ("enforcement",bounded_array(json!({"type":"string","enum":octacity_server_application::factory_permission_categories().collect::<Vec<_>>()}),octacity_server_application::factory_permission_categories().count())),
    ("environment_names",bounded_array(environment(),2*bounds.role_entries)),
    ("secret_environment_names",bounded_array(environment(),bounds.role_entries)),
  ], &["alias","toolchain","support","permissions","enforcement","environment_names","secret_environment_names"]));
  schemas.insert(
    "FactoryRuntimeChoicePage".into(),
    object(
      [
        ("total", non_negative_integer()),
        (
          "items",
          bounded_array(schema_ref("FactoryRuntimeChoice"), bounds.choice_entries),
        ),
        ("next_cursor", nullable(key())),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
}
