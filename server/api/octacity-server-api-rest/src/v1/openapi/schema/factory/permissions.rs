use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert(
    "FactoryCommandArgumentPattern".to_owned(),
    json!({"oneOf":[
      tagged("kind","any",[],&[]),tagged("kind","exact",[("value",non_empty_string())],&["value"]),
    ]}),
  );
  schemas.insert(
    "FactoryCommandPermission".to_owned(),
    object(
      [
        ("executable", schema_ref("FactoryImmutableReferenceResource")),
        (
          "arguments",
          bounded_array(schema_ref("FactoryCommandArgumentPattern"), bounds.command_arguments),
        ),
      ],
      &["executable", "arguments"],
    ),
  );
  schemas.insert(
    "FactoryMountPermission".to_owned(),
    object(
      [
        ("root", non_empty_string()),
        ("mode", json!({"type":"string","enum":["read_only","read_write"]})),
      ],
      &["root", "mode"],
    ),
  );
  schemas.insert(
    "FactoryResourceLimits".to_owned(),
    object(
      [
        ("cpu_millis", bounded_integer(0, bounds.cpu_millis.into())),
        ("memory_bytes", bounded_integer(0, bounds.memory_bytes)),
        ("disk_bytes", bounded_integer(0, bounds.disk_bytes)),
        ("process_count", bounded_integer(0, bounds.processes.into())),
        ("elapsed_millis", bounded_integer(0, bounds.elapsed_millis)),
      ],
      &[
        "cpu_millis",
        "memory_bytes",
        "disk_bytes",
        "process_count",
        "elapsed_millis",
      ],
    ),
  );
  schemas.insert(
    "FactoryOutputPermissions".to_owned(),
    object(
      [
        ("kinds", bounded_array(key(), bounds.permission_entries)),
        ("max_artifact_count", bounded_integer(0, bounds.output_count.into())),
        ("max_artifact_bytes", bounded_integer(0, bounds.output_bytes)),
        ("max_report_count", bounded_integer(0, bounds.output_count.into())),
        ("max_report_bytes", bounded_integer(0, bounds.output_bytes)),
      ],
      &[
        "kinds",
        "max_artifact_count",
        "max_artifact_bytes",
        "max_report_count",
        "max_report_bytes",
      ],
    ),
  );
  schemas.insert(
    "FactoryPermissions".to_owned(),
    object(
      [
        (
          "plugins",
          bounded_array(
            schema_ref("FactoryImmutableReferenceResource"),
            bounds.permission_entries,
          ),
        ),
        (
          "executables",
          bounded_array(
            schema_ref("FactoryImmutableReferenceResource"),
            bounds.permission_entries,
          ),
        ),
        (
          "tools",
          bounded_array(
            schema_ref("FactoryImmutableReferenceResource"),
            bounds.permission_entries,
          ),
        ),
        (
          "commands",
          bounded_array(schema_ref("FactoryCommandPermission"), bounds.permission_entries),
        ),
        ("max_descendants", bounded_integer(0, bounds.processes.into())),
        (
          "mounts",
          bounded_array(schema_ref("FactoryMountPermission"), bounds.permission_entries),
        ),
        (
          "network_hosts",
          bounded_array(non_empty_string(), bounds.permission_entries),
        ),
        ("secret_profiles", bounded_array(key(), bounds.permission_entries)),
        (
          "workload_identity_profiles",
          bounded_array(key(), bounds.permission_entries),
        ),
        ("resources", schema_ref("FactoryResourceLimits")),
        ("outputs", schema_ref("FactoryOutputPermissions")),
      ],
      &[
        "plugins",
        "executables",
        "tools",
        "commands",
        "max_descendants",
        "mounts",
        "network_hosts",
        "secret_profiles",
        "workload_identity_profiles",
        "resources",
        "outputs",
      ],
    ),
  );
}
