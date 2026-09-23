use super::*;

impl ManagementInputFactory {
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
