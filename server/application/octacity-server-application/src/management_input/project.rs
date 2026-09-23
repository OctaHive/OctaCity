use super::*;

impl ManagementInputFactory {
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

  /// Creates a typed immutable Project-policy publication command.
  pub fn publish_project_policy(
    &self,
    project_id: &str,
    expected_current_version: Option<u64>,
    policy: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<PublishProjectPolicyCommand, ManagementInputError> {
    Ok(PublishProjectPolicyCommand {
      project_id: parse(project_id, "project id")?,
      expected_current_version: expected_current_version
        .map(|version_value| version(version_value, "project policy version"))
        .transpose()?,
      policy: decode::<ProjectPolicyDefinition>(policy, "project policy")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
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
}
