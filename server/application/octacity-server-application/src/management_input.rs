use std::time::Duration;
use std::{collections::BTreeMap, str::FromStr};

use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, Timestamp, TriggerId, TriggerIdentity, TriggerVersion,
};
use octacity_server_pipeline::{CapabilityCatalog, ExecutionCapability, PipelineDag};
use octacity_server_store::{TriggerDefinitionRef, TriggerTarget};
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
  AcceptManualTriggerCommand, CreateBuildConfigurationCommand, CreatePipelineCommand, CreateProjectCommand,
  CreateRepositoryCommand, DeleteProjectCommand, GetBuildConfigurationQuery, GetPipelineQuery, GetProjectQuery,
  GetRepositoryQuery, ListProjectsQuery, MAX_JOB_EVENT_WAIT, ManualSourceSelection, ManualTriggerCommand,
  MoveProjectCommand, PublishBuildConfigurationVersionCommand, PublishPipelineVersionCommand,
  PublishRepositoryVersionCommand, ReadJobEventsQuery, RenameProjectCommand,
};

/// Builds typed application commands and queries from transport-neutral wire primitives.
///
/// This keeps HTTP adapters from depending directly on server core crates while preserving
/// validation at the application boundary.
#[derive(Clone)]
pub struct ManagementInputFactory {
  pipeline_capabilities: CapabilityCatalog,
}

/// Transport-neutral primitive input for one manual Trigger command.
pub struct ManualTriggerInput {
  /// Exact Trigger definition identity.
  pub trigger_id: String,
  /// Exact Trigger definition version.
  pub trigger_version: u64,
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact target Build Configuration version.
  pub configuration_version: u64,
  /// Stable source-scoped occurrence identity.
  pub deduplication_identity: String,
  /// Tagged manual source-selection document.
  pub source: Value,
  /// Caller-supplied parameter values.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority.
  pub priority: i64,
}

impl ManagementInputFactory {
  /// Creates a factory using the execution capabilities accepted for new Pipeline versions.
  pub fn new(pipeline_capabilities: impl IntoIterator<Item = String>) -> Result<Self, ManagementInputError> {
    let pipeline_capabilities = pipeline_capabilities
      .into_iter()
      .map(|value| parse::<ExecutionCapability>(&value, "pipeline capability"))
      .collect::<Result<Vec<_>, _>>()?;
    Ok(Self {
      pipeline_capabilities: CapabilityCatalog::new(pipeline_capabilities),
    })
  }

  /// Creates a typed Project-create command.
  pub fn create_project(
    &self,
    id: Uuid,
    parent_id: Option<&str>,
    name: String,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateProjectCommand, ManagementInputError> {
    Ok(CreateProjectCommand {
      id: identifier(id, "project id")?,
      parent_id: optional_parse(parent_id, "parent project id")?,
      name: parse(&name, "project name")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      created_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Project-rename command.
  pub fn rename_project(
    &self,
    id: &str,
    expected_version: u64,
    name: String,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<RenameProjectCommand, ManagementInputError> {
    Ok(RenameProjectCommand {
      id: parse(id, "project id")?,
      expected_version: version(expected_version, "project version")?,
      name: parse(&name, "project name")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      renamed_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Project-move command.
  pub fn move_project(
    &self,
    id: &str,
    expected_version: u64,
    parent_id: Option<&str>,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<MoveProjectCommand, ManagementInputError> {
    Ok(MoveProjectCommand {
      id: parse(id, "project id")?,
      expected_version: version(expected_version, "project version")?,
      parent_id: optional_parse(parent_id, "parent project id")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      moved_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Project-delete command.
  pub fn delete_project(
    &self,
    id: &str,
    expected_version: u64,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<DeleteProjectCommand, ManagementInputError> {
    Ok(DeleteProjectCommand {
      id: parse(id, "project id")?,
      expected_version: version(expected_version, "project version")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      deleted_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Project read query.
  pub fn get_project(&self, id: &str) -> Result<GetProjectQuery, ManagementInputError> {
    Ok(GetProjectQuery {
      project_id: parse(id, "project id")?,
    })
  }

  /// Creates a typed bounded Project-list query.
  pub fn list_projects(
    &self,
    parent_id: Option<&str>,
    after: Option<&str>,
    limit: u16,
  ) -> Result<ListProjectsQuery, ManagementInputError> {
    Ok(ListProjectsQuery {
      parent_id: optional_parse(parent_id, "parent project id")?,
      after: optional_parse(after, "project cursor")?,
      limit,
    })
  }

  /// Creates a typed Pipeline-create command from a canonical DAG document.
  pub fn create_pipeline(
    &self,
    id: Uuid,
    project_id: &str,
    name: String,
    dag: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreatePipelineCommand, ManagementInputError> {
    Ok(CreatePipelineCommand {
      id: identifier(id, "pipeline id")?,
      project_id: parse(project_id, "project id")?,
      name: parse(&name, "pipeline name")?,
      dag: self.pipeline_dag(dag)?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Pipeline-publish command from a canonical DAG document.
  pub fn publish_pipeline(
    &self,
    id: &str,
    expected_version: u64,
    dag: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<PublishPipelineVersionCommand, ManagementInputError> {
    Ok(PublishPipelineVersionCommand {
      id: parse(id, "pipeline id")?,
      expected_current_version: version(expected_version, "pipeline version")?,
      dag: self.pipeline_dag(dag)?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed exact Pipeline-version query.
  pub fn get_pipeline(&self, id: &str, version_value: u64) -> Result<GetPipelineQuery, ManagementInputError> {
    Ok(GetPipelineQuery {
      pipeline_id: parse(id, "pipeline id")?,
      version: version(version_value, "pipeline version")?,
    })
  }

  /// Creates a typed Repository-create command.
  pub fn create_repository(
    &self,
    id: Uuid,
    project_id: &str,
    name: String,
    definition: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateRepositoryCommand, ManagementInputError> {
    Ok(CreateRepositoryCommand {
      id: identifier(id, "repository id")?,
      project_id: parse(project_id, "project id")?,
      name: parse(&name, "repository name")?,
      definition: decode(definition, "repository definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Repository-publish command.
  pub fn publish_repository(
    &self,
    id: &str,
    expected_version: u64,
    definition: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<PublishRepositoryVersionCommand, ManagementInputError> {
    Ok(PublishRepositoryVersionCommand {
      id: parse(id, "repository id")?,
      expected_current_version: version(expected_version, "repository version")?,
      definition: decode(definition, "repository definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed exact Repository-version query.
  pub fn get_repository(&self, id: &str, version_value: u64) -> Result<GetRepositoryQuery, ManagementInputError> {
    Ok(GetRepositoryQuery {
      repository_id: parse(id, "repository id")?,
      version: version(version_value, "repository version")?,
    })
  }

  /// Creates a typed Build Configuration-create command.
  pub fn create_build_configuration(
    &self,
    id: Uuid,
    project_id: &str,
    name: String,
    definition: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateBuildConfigurationCommand, ManagementInputError> {
    Ok(CreateBuildConfigurationCommand {
      id: identifier(id, "build configuration id")?,
      project_id: parse(project_id, "project id")?,
      name: parse(&name, "build configuration name")?,
      definition: decode(definition, "build configuration definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Build Configuration-publish command.
  pub fn publish_build_configuration(
    &self,
    id: &str,
    expected_version: u64,
    definition: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<PublishBuildConfigurationVersionCommand, ManagementInputError> {
    Ok(PublishBuildConfigurationVersionCommand {
      id: parse(id, "build configuration id")?,
      expected_current_version: version(expected_version, "build configuration version")?,
      definition: decode(definition, "build configuration definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed exact Build Configuration-version query.
  pub fn get_build_configuration(
    &self,
    id: &str,
    version_value: u64,
  ) -> Result<GetBuildConfigurationQuery, ManagementInputError> {
    Ok(GetBuildConfigurationQuery {
      configuration_id: parse(id, "build configuration id")?,
      version: version(version_value, "build configuration version")?,
    })
  }

  /// Creates a typed bounded Job-event long-poll query.
  pub fn read_job_events(
    &self,
    job_id: &str,
    after_sequence: u64,
    limit: u16,
    wait: Duration,
  ) -> Result<ReadJobEventsQuery, ManagementInputError> {
    if wait > MAX_JOB_EVENT_WAIT {
      return Err(ManagementInputError::Invalid("job event wait"));
    }
    let job_id = parse(job_id, "job id")?;
    octacity_server_store::ReadJobEvents::new(job_id, after_sequence, limit)
      .map_err(|_| ManagementInputError::Invalid("job event page"))?;
    Ok(ReadJobEventsQuery {
      job_id,
      after_sequence,
      limit,
      wait,
    })
  }

  /// Creates a typed manual-Trigger acceptance command.
  pub fn accept_manual_trigger(
    &self,
    input: ManualTriggerInput,
    now_unix_ms: i64,
  ) -> Result<AcceptManualTriggerCommand, ManagementInputError> {
    let now = timestamp(now_unix_ms)?;
    Ok(AcceptManualTriggerCommand {
      trigger: ManualTriggerCommand {
        trigger: TriggerDefinitionRef {
          id: parse::<TriggerId>(&input.trigger_id, "trigger id")?,
          version: version::<TriggerVersion>(input.trigger_version, "trigger version")?,
        },
        target: TriggerTarget {
          configuration_id: parse::<BuildConfigurationId>(&input.configuration_id, "build configuration id")?,
          configuration_version: version::<BuildConfigurationVersion>(
            input.configuration_version,
            "build configuration version",
          )?,
        },
        deduplication_identity: parse::<TriggerIdentity>(&input.deduplication_identity, "deduplication identity")?,
        source: decode::<ManualSourceSelection>(input.source, "manual source")?,
        parameters: input.parameters,
        priority: input.priority,
        observed_at: now,
      },
      accepted_at: now,
    })
  }

  fn pipeline_dag(
    &self,
    value: Value,
  ) -> Result<octacity_server_pipeline::PublishablePipelineDag, ManagementInputError> {
    let dag = decode::<PipelineDag>(value, "pipeline DAG")?;
    dag
      .for_publication(&self.pipeline_capabilities)
      .map_err(|_| ManagementInputError::Invalid("pipeline DAG"))
  }
}

/// Stable application-boundary classification for invalid command primitives.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ManagementInputError {
  /// One named input could not be parsed or validated.
  #[error("invalid {0}")]
  Invalid(&'static str),
}

fn parse<T>(value: &str, field: &'static str) -> Result<T, ManagementInputError>
where
  T: FromStr,
{
  value.parse().map_err(|_| ManagementInputError::Invalid(field))
}

fn optional_parse<T>(value: Option<&str>, field: &'static str) -> Result<Option<T>, ManagementInputError>
where
  T: FromStr,
{
  value.map(|value| parse(value, field)).transpose()
}

fn identifier<T>(value: Uuid, field: &'static str) -> Result<T, ManagementInputError>
where
  T: FromStr,
{
  parse(&value.hyphenated().to_string(), field)
}

fn version<T>(value: u64, field: &'static str) -> Result<T, ManagementInputError>
where
  T: DeserializeOwned,
{
  decode(Value::from(value), field)
}

fn timestamp(value: i64) -> Result<Timestamp, ManagementInputError> {
  Timestamp::from_unix_millis(value).map_err(|_| ManagementInputError::Invalid("timestamp"))
}

fn decode<T>(value: Value, field: &'static str) -> Result<T, ManagementInputError>
where
  T: DeserializeOwned,
{
  serde_json::from_value(value).map_err(|_| ManagementInputError::Invalid(field))
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn rejects_pipeline_capabilities_outside_the_configured_catalog() {
    let factory = ManagementInputFactory::new(["native".to_owned()]).unwrap();
    let dag = json!({
      "schema_version": 1,
      "nodes": [{
        "id": "build",
        "name": "Build",
        "dependency_policy": "all_succeeded",
        "required_capabilities": ["oci"],
        "template": {"commands": ["build"]}
      }],
      "edges": []
    });

    assert_eq!(
      factory.create_pipeline(
        Uuid::from_u128(1),
        "00000000-0000-0000-0000-000000000002",
        "Build".to_owned(),
        dag,
        "create-pipeline",
        1,
      ),
      Err(ManagementInputError::Invalid("pipeline DAG"))
    );
  }
}
