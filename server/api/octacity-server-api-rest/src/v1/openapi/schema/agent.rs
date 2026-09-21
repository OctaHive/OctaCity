use serde_json::{Map, Value};

use super::super::{array, boolean, non_empty_string, nullable, object, positive_integer, schema_ref, string_enum};

pub(super) fn insert_agent_detail_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ProtocolPlatformOs".to_owned(),
    string_enum(&["linux", "windows", "macos"]),
  );
  schemas.insert(
    "ProtocolPlatformArchitecture".to_owned(),
    string_enum(&["amd64", "arm64"]),
  );
  schemas.insert(
    "ProtocolPlatform".to_owned(),
    object(
      [
        ("os", schema_ref("ProtocolPlatformOs")),
        ("architecture", schema_ref("ProtocolPlatformArchitecture")),
      ],
      &["os", "architecture"],
    ),
  );
  schemas.insert(
    "AgentRuntimeCapability".to_owned(),
    object(
      [
        ("backend", non_empty_string()),
        ("mode", string_enum(&["native", "oci"])),
        ("platform", schema_ref("ProtocolPlatform")),
        ("isolation", nullable(string_enum(&["process", "hypervisor"]))),
      ],
      &["backend", "mode", "platform", "isolation"],
    ),
  );
  schemas.insert(
    "AgentTaskPluginInventory".to_owned(),
    object(
      [
        ("name", non_empty_string()),
        ("version", non_empty_string()),
        ("protocol", positive_integer()),
        ("platforms", array(non_empty_string())),
        ("sha256", non_empty_string()),
        ("capabilities", array(non_empty_string())),
      ],
      &["name", "version", "protocol", "platforms", "sha256", "capabilities"],
    ),
  );
  schemas.insert(
    "AgentOctaInventory".to_owned(),
    object(
      [
        ("version", non_empty_string()),
        ("runner_sha256", non_empty_string()),
        ("build_commit", nullable(non_empty_string())),
        ("runner_protocols", array(positive_integer())),
        ("event_schemas", array(positive_integer())),
        ("plugin_protocols", array(positive_integer())),
        ("octafile_versions", array(positive_integer())),
        ("features", array(non_empty_string())),
        ("plugins", array(schema_ref("AgentTaskPluginInventory"))),
      ],
      &[
        "version",
        "runner_sha256",
        "build_commit",
        "runner_protocols",
        "event_schemas",
        "plugin_protocols",
        "octafile_versions",
        "features",
        "plugins",
      ],
    ),
  );
  schemas.insert(
    "AgentSourcePluginInventory".to_owned(),
    object(
      [
        ("name", non_empty_string()),
        ("version", non_empty_string()),
        ("protocol_min", positive_integer()),
        ("protocol_max", positive_integer()),
        ("platforms", array(non_empty_string())),
        ("sha256", non_empty_string()),
      ],
      &["name", "version", "protocol_min", "protocol_max", "platforms", "sha256"],
    ),
  );
  schemas.insert(
    "AgentCacheCapability".to_owned(),
    object(
      [
        ("runner_protocol", positive_integer()),
        ("action_key_format", positive_integer()),
        ("remote_http", boolean()),
      ],
      &["runner_protocol", "action_key_format", "remote_http"],
    ),
  );
}
