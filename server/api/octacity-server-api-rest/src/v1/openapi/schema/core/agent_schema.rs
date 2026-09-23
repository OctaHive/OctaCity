use super::*;

pub(super) fn insert_agent_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "IssueAgentEnrollmentRequest".to_owned(),
    object(
      [
        ("pool_id", non_empty_string()),
        ("pool_version", positive_integer()),
        ("expected_platform", nullable(schema_ref("AgentPlatform"))),
      ],
      &["pool_id", "pool_version", "expected_platform"],
    ),
  );
  schemas.insert(
    "IssueAgentEnrollmentResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("credential", non_empty_string()),
        ("pool_id", non_empty_string()),
        ("pool_version", positive_integer()),
        ("expires_at_unix_ms", integer()),
      ],
      &[
        "disposition",
        "credential",
        "pool_id",
        "pool_version",
        "expires_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "AgentStatus".to_owned(),
    string_enum(&["online", "offline", "draining"]),
  );
  schemas.insert(
    "AgentCapacity".to_owned(),
    object(
      [
        ("logical_cpu_count", positive_integer()),
        ("total_memory_bytes", positive_integer()),
        ("work_disk_total_bytes", positive_integer()),
        ("state_disk_total_bytes", positive_integer()),
        ("virtualization_available", boolean()),
      ],
      &[
        "logical_cpu_count",
        "total_memory_bytes",
        "work_disk_total_bytes",
        "state_disk_total_bytes",
        "virtualization_available",
      ],
    ),
  );
  schemas.insert(
    "AgentInventory".to_owned(),
    object(
      [
        ("agent_version", non_empty_string()),
        ("coordinator_protocols", unique_array(positive_integer())),
        ("labels", string_map()),
        ("host_platform", schema_ref("ProtocolPlatform")),
        ("runtimes", array(schema_ref("AgentRuntimeCapability"))),
        ("octa", schema_ref("AgentOctaInventory")),
        ("source_plugins", array(schema_ref("AgentSourcePluginInventory"))),
        ("cache", nullable(schema_ref("AgentCacheCapability"))),
      ],
      &[
        "agent_version",
        "coordinator_protocols",
        "labels",
        "host_platform",
        "runtimes",
        "octa",
        "source_plugins",
        "cache",
      ],
    ),
  );
  schemas.insert(
    "AgentResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("name", non_empty_string()),
        ("version", positive_integer()),
        ("pool_id", non_empty_string()),
        ("pool_version", positive_integer()),
        ("inventory", schema_ref("AgentInventory")),
        ("capacity", schema_ref("AgentCapacity")),
        ("status", schema_ref("AgentStatus")),
        ("last_seen_at_unix_ms", integer()),
      ],
      &[
        "id",
        "name",
        "version",
        "pool_id",
        "pool_version",
        "inventory",
        "capacity",
        "status",
        "last_seen_at_unix_ms",
      ],
    ),
  );
  schemas.insert("AgentMutationResponse".to_owned(), mutation_response("AgentResource"));
  schemas.insert(
    "AgentPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("AgentResource"))),
        ("next_cursor", nullable(non_empty_string())),
      ],
      &["items", "next_cursor"],
    ),
  );
  schemas.insert(
    "ReassignAgentPoolRequest".to_owned(),
    object([("pool_id", non_empty_string())], &["pool_id"]),
  );
  schemas.insert(
    "DrainAgentRequest".to_owned(),
    object([("mode", string_enum(&["graceful", "forced"]))], &["mode"]),
  );
}

pub(super) fn insert_agent_pool_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "AgentPlatform".to_owned(),
    object(
      [
        ("operating_system", non_empty_string()),
        ("architecture", non_empty_string()),
      ],
      &["operating_system", "architecture"],
    ),
  );
  schemas.insert(
    "AgentPoolDrainState".to_owned(),
    string_enum(&["accepting", "graceful_drain", "forced_drain", "drained"]),
  );
  schemas.insert(
    "AgentPoolAdmissionPolicy".to_owned(),
    json!({
      "oneOf": [
        object([("mode", json!({"const": "any"}))], &["mode"]),
        object(
          [
            ("mode", json!({"const": "allowlist"})),
            (
              "platforms",
              json!({
                "type": "array",
                "items": schema_ref("AgentPlatform"),
                "uniqueItems": true,
                "minItems": 1,
                "maxItems": MAX_AGENT_POOL_ADMISSION_PLATFORMS
              })
            )
          ],
          &["mode", "platforms"]
        )
      ],
      "discriminator": {"propertyName": "mode"}
    }),
  );
  schemas.insert(
    "AgentPoolDefinition".to_owned(),
    object(
      [
        ("enabled", boolean()),
        ("drain_state", schema_ref("AgentPoolDrainState")),
        ("admission_policy", schema_ref("AgentPoolAdmissionPolicy")),
        (
          "concurrency_limit",
          json!({"type": "integer", "minimum": 1, "maximum": MAX_AGENT_POOL_STATIC_CAPACITY}),
        ),
        (
          "fairness_policy",
          json!({"type": "string", "enum": ["priority_fifo", "configuration_fair"]}),
        ),
        (
          "static_capacity_limit",
          json!({"type": "integer", "minimum": 1, "maximum": MAX_AGENT_POOL_STATIC_CAPACITY}),
        ),
      ],
      &[
        "enabled",
        "drain_state",
        "admission_policy",
        "concurrency_limit",
        "fairness_policy",
        "static_capacity_limit",
      ],
    ),
  );
  schemas.insert(
    "CreateAgentPoolRequest".to_owned(),
    object(
      [
        ("name", non_empty_string()),
        ("definition", schema_ref("AgentPoolDefinition")),
      ],
      &["name", "definition"],
    ),
  );
  schemas.insert(
    "PublishAgentPoolVersionRequest".to_owned(),
    object([("definition", schema_ref("AgentPoolDefinition"))], &["definition"]),
  );
  schemas.insert(
    "AgentPoolResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("name", non_empty_string()),
        ("version", positive_integer()),
        ("definition", schema_ref("AgentPoolDefinition")),
        ("published_at_unix_ms", integer()),
      ],
      &["id", "name", "version", "definition", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "AgentPoolMutationResponse".to_owned(),
    mutation_response("AgentPoolResource"),
  );
  schemas.insert(
    "AgentPoolPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("AgentPoolResource"))),
        (
          "next_cursor",
          nullable(json!({"type": "string", "minLength": 1, "maxLength": MAX_CURSOR_BYTES})),
        ),
      ],
      &["items", "next_cursor"],
    ),
  );
  schemas.insert(
    "DeleteAgentPoolResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("pool_id", non_empty_string()),
      ],
      &["disposition", "pool_id"],
    ),
  );
}
