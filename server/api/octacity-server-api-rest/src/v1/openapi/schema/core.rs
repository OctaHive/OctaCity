use serde_json::{Map, Value, json};

use octacity_server_application::{MAX_AGENT_POOL_ADMISSION_PLATFORMS, MAX_AGENT_POOL_STATIC_CAPACITY};

use super::super::{
  MAX_CURSOR_BYTES, array, boolean, integer, mutation_response, non_empty_string, non_negative_integer, nullable,
  object, positive_integer, schema_ref, string_enum, string_map, unique_array, versioned_resource,
};
use super::{agent, artifact, audit, cache, execution, log_search, operational, retention, trigger};

mod agent_schema;
mod configuration_schema;
mod execution_schema;
mod management_schema;

use agent_schema::{insert_agent_pool_schemas, insert_agent_schemas};
use configuration_schema::{insert_configuration_schemas, insert_repository_schemas};
use execution_schema::insert_execution_schemas;
use management_schema::{insert_common_schemas, insert_pipeline_schemas, insert_project_schemas};

pub(crate) fn component_schemas() -> Value {
  let mut schemas = Map::new();
  insert_common_schemas(&mut schemas);
  operational::insert_operational_schemas(&mut schemas);
  audit::insert_audit_schemas(&mut schemas);
  insert_project_schemas(&mut schemas);
  insert_agent_pool_schemas(&mut schemas);
  agent::insert_agent_detail_schemas(&mut schemas);
  insert_agent_schemas(&mut schemas);
  insert_pipeline_schemas(&mut schemas);
  insert_repository_schemas(&mut schemas);
  insert_configuration_schemas(&mut schemas);
  trigger::insert_trigger_schemas(&mut schemas);
  execution::insert_execution_detail_schemas(&mut schemas);
  insert_execution_schemas(&mut schemas);
  artifact::insert_artifact_schemas(&mut schemas);
  cache::insert_cache_session_schemas(&mut schemas);
  operational::insert_job_event_schemas(&mut schemas);
  log_search::insert_log_search_schemas(&mut schemas);
  retention::insert_retention_schemas(&mut schemas);
  Value::Object(schemas)
}
