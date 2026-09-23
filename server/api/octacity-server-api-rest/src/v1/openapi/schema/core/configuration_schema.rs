use super::*;

pub(super) fn insert_repository_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "RepositorySelectionPolicy".to_owned(),
    object(
      [
        ("allowed_references", unique_array(non_empty_string())),
        ("default_reference", nullable(non_empty_string())),
        ("allow_exact_revision", boolean()),
      ],
      &["allowed_references", "default_reference", "allow_exact_revision"],
    ),
  );
  schemas.insert(
    "RepositoryDefinition".to_owned(),
    object(
      [
        ("vcs_integration_id", non_empty_string()),
        ("repository_locator", non_empty_string()),
        ("selection", schema_ref("RepositorySelectionPolicy")),
      ],
      &["vcs_integration_id", "repository_locator", "selection"],
    ),
  );
  schemas.insert(
    "CreateRepositoryRequest".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("name", non_empty_string()),
        ("definition", schema_ref("RepositoryDefinition")),
      ],
      &["project_id", "name", "definition"],
    ),
  );
  schemas.insert(
    "PublishRepositoryVersionRequest".to_owned(),
    object([("definition", schema_ref("RepositoryDefinition"))], &["definition"]),
  );
  schemas.insert(
    "RepositoryResource".to_owned(),
    versioned_resource("RepositoryDefinition"),
  );
  schemas.insert(
    "RepositoryMutationResponse".to_owned(),
    mutation_response("RepositoryResource"),
  );
}

pub(super) fn insert_configuration_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ParameterType".to_owned(),
    string_enum(&["string", "integer", "boolean"]),
  );
  schemas.insert(
    "ParameterDefinition".to_owned(),
    object(
      [
        ("value_type", schema_ref("ParameterType")),
        ("required", boolean()),
        ("default", nullable(json!({}))),
      ],
      &["value_type", "required", "default"],
    ),
  );
  schemas.insert(
    "ParameterSchema".to_owned(),
    object(
      [
        (
          "parameters",
          json!({"type": "object", "additionalProperties": schema_ref("ParameterDefinition")}),
        ),
        ("deny_unknown", boolean()),
      ],
      &["parameters", "deny_unknown"],
    ),
  );
  schemas.insert(
    "TriggerKind".to_owned(),
    string_enum(&["manual", "scheduled", "external", "internal"]),
  );
  schemas.insert(
    "AgentRequirements".to_owned(),
    object(
      [
        ("capabilities", unique_array(non_empty_string())),
        ("labels", string_map()),
        ("minimum_cpu_millis", non_negative_integer()),
        ("minimum_memory_bytes", non_negative_integer()),
        ("minimum_disk_bytes", non_negative_integer()),
      ],
      &[
        "capabilities",
        "labels",
        "minimum_cpu_millis",
        "minimum_memory_bytes",
        "minimum_disk_bytes",
      ],
    ),
  );
  schemas.insert(
    "RuntimeClass".to_owned(),
    string_enum(&["native", "oci_process", "oci_hypervisor"]),
  );
  schemas.insert("PlatformOs".to_owned(), string_enum(&["linux", "windows", "macos"]));
  schemas.insert("PlatformArchitecture".to_owned(), string_enum(&["amd64", "arm64"]));
  schemas.insert(
    "NetworkPolicy".to_owned(),
    json!({
      "oneOf": [
        object([("mode", json!({"const": "disabled"}))], &["mode"]),
        object([("mode", json!({"const": "unrestricted"}))], &["mode"]),
        object(
          [
            ("mode", json!({"const": "restricted"})),
            ("allowed_hosts", unique_array(non_empty_string()))
          ],
          &["mode", "allowed_hosts"]
        )
      ],
      "discriminator": {"propertyName": "mode"}
    }),
  );
  schemas.insert(
    "RuntimePolicy".to_owned(),
    object(
      [
        ("class", schema_ref("RuntimeClass")),
        ("operating_system", schema_ref("PlatformOs")),
        ("architecture", schema_ref("PlatformArchitecture")),
        ("immutable_image", nullable(non_empty_string())),
        ("cpu_millis", non_negative_integer()),
        ("memory_bytes", non_negative_integer()),
        ("writable_disk_bytes", non_negative_integer()),
        ("timeout_seconds", non_negative_integer()),
        ("network", schema_ref("NetworkPolicy")),
        ("workload_identity_profile", nullable(non_empty_string())),
      ],
      &[
        "class",
        "operating_system",
        "architecture",
        "immutable_image",
        "cpu_millis",
        "memory_bytes",
        "writable_disk_bytes",
        "timeout_seconds",
        "network",
        "workload_identity_profile",
      ],
    ),
  );
  schemas.insert(
    "CachePolicy".to_owned(),
    object(
      [
        ("namespace", nullable(non_empty_string())),
        ("read", boolean()),
        ("write", boolean()),
      ],
      &["namespace", "read", "write"],
    ),
  );
  schemas.insert(
    "ArtifactPolicy".to_owned(),
    object(
      [
        ("artifact_count", non_negative_integer()),
        ("artifact_bytes", non_negative_integer()),
        ("report_count", non_negative_integer()),
        ("report_bytes", non_negative_integer()),
        ("single_output_bytes", non_negative_integer()),
      ],
      &[
        "artifact_count",
        "artifact_bytes",
        "report_count",
        "report_bytes",
        "single_output_bytes",
      ],
    ),
  );
  schemas.insert(
    "RetryClass".to_owned(),
    string_enum(&["execution_failure", "infrastructure_failure"]),
  );
  schemas.insert(
    "RetryPolicy".to_owned(),
    object(
      [
        ("max_attempts", positive_integer()),
        ("retry_on", unique_array(schema_ref("RetryClass"))),
      ],
      &["max_attempts", "retry_on"],
    ),
  );
  schemas.insert(
    "BuildConfigurationDefinition".to_owned(),
    object(
      [
        ("enabled", boolean()),
        (
          "job_concurrency_limit",
          json!({"type": "integer", "minimum": 1, "maximum": 10000}),
        ),
        ("repository_id", non_empty_string()),
        ("repository_version", positive_integer()),
        ("pipeline_id", non_empty_string()),
        ("pipeline_version", positive_integer()),
        ("parameters", schema_ref("ParameterSchema")),
        ("triggers", unique_array(schema_ref("TriggerKind"))),
        ("agent_requirements", schema_ref("AgentRequirements")),
        ("allowed_pools", unique_array(non_empty_string())),
        ("runtime", schema_ref("RuntimePolicy")),
        ("cache", schema_ref("CachePolicy")),
        ("artifacts", schema_ref("ArtifactPolicy")),
        ("retry", schema_ref("RetryPolicy")),
      ],
      &[
        "enabled",
        "job_concurrency_limit",
        "repository_id",
        "repository_version",
        "pipeline_id",
        "pipeline_version",
        "parameters",
        "triggers",
        "agent_requirements",
        "allowed_pools",
        "runtime",
        "cache",
        "artifacts",
        "retry",
      ],
    ),
  );
  schemas.insert(
    "CreateBuildConfigurationRequest".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("name", non_empty_string()),
        ("definition", schema_ref("BuildConfigurationDefinition")),
      ],
      &["project_id", "name", "definition"],
    ),
  );
  schemas.insert(
    "PublishBuildConfigurationVersionRequest".to_owned(),
    object(
      [("definition", schema_ref("BuildConfigurationDefinition"))],
      &["definition"],
    ),
  );
  schemas.insert(
    "BuildConfigurationResource".to_owned(),
    versioned_resource("BuildConfigurationDefinition"),
  );
  schemas.insert(
    "BuildConfigurationMutationResponse".to_owned(),
    mutation_response("BuildConfigurationResource"),
  );
}
