use super::*;
use crate::{FactoryConfigurationPagePosition, ListProjectFactoryConfigurationsQuery};

/// Transport primitives shared by exact Node Attempt control commands.
pub struct FactoryNodeControlInput<'a> {
  /// Owning Run UUID.
  pub run_id: &'a str,
  /// Exact target attempt UUID.
  pub node_attempt_id: &'a str,
  /// Strong aggregate precondition.
  pub expected_version: u64,
  /// Stable scoped replay key.
  pub idempotency_key: &'a str,
  /// Trusted acceptance time in Unix milliseconds.
  pub requested_at: i64,
}

impl ManagementInputFactory {
  /// Uses the ordinary strong version precondition and scoped idempotency key.
  pub fn delete_factory_role(
    &self,
    project: &str,
    id: &str,
    expected: u64,
    key: &str,
    now: i64,
  ) -> Result<crate::DeleteFactoryRoleCommand, ManagementInputError> {
    Ok(crate::DeleteFactoryRoleCommand {
      request: octacity_server_store::DeleteFactoryRole {
        project_id: parse(project, "project id")?,
        id: parse(id, "role id")?,
        expected_current_version: version(expected, "role version")?,
        idempotency_key: parse(key, "idempotency key")?,
        deleted_at: timestamp(now)?,
      },
    })
  }

  /// Retains an exact pre-dispatch choice using ordinary Run preconditions and replay identity.
  pub fn select_factory_profile(
    &self,
    run: &str,
    input: &str,
    profile: Value,
    expected: u64,
    key: &str,
    now: i64,
  ) -> Result<crate::SelectFactoryProfileCommand, ManagementInputError> {
    Ok(crate::SelectFactoryProfileCommand {
      run_id: parse(run, "factory run id")?,
      input_digest: parse(input, "input digest")?,
      profile: factory_decode(profile)?,
      expected_version: version(expected, "factory run version")?,
      idempotency_key: parse(key, "idempotency key")?,
      requested_at: timestamp(now)?,
    })
  }

  /// Selects one authorized installed authoring capability by its stable Project alias.
  pub fn get_factory_role_profile_capability(
    &self,
    project: &str,
    alias: &str,
  ) -> Result<crate::GetFactoryRoleProfileCapabilityQuery, ManagementInputError> {
    Ok(crate::GetFactoryRoleProfileCapabilityQuery {
      project_id: parse(project, "project id")?,
      capability: parse(alias, "profile capability")?,
    })
  }
  /// Preserves complete submitted settings; the application qualifies them without side effects.
  pub fn validate_factory_role_profile(
    &self,
    project: &str,
    id: &str,
    role: Value,
    capability: &str,
    definition: Value,
  ) -> Result<crate::ValidateFactoryRoleProfileQuery, ManagementInputError> {
    Ok(crate::ValidateFactoryRoleProfileQuery {
      project_id: parse(project, "project id")?,
      id: parse(id, "profile id")?,
      role: factory_decode(role)?,
      capability: parse(capability, "profile capability")?,
      definition: factory_decode(definition)?,
    })
  }

  /// Decodes regular reviewed bytes; shared import owns all path/tree/digest checks.
  pub fn import_project_skill(
    &self,
    project: &str,
    identity: &str,
    files: Value,
    key: &str,
  ) -> Result<crate::ImportProjectSkill, ManagementInputError> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct File {
      path: String,
      sha256: octacity_server_factory::FactoryDigest,
      bytes: Vec<u8>,
    }
    let files: Vec<File> = factory_decode(files)?;
    Ok(crate::ImportProjectSkill {
      project_id: parse(project, "project id")?,
      identity: parse(identity, "skill identity")?,
      idempotency_key: parse(key, "idempotency key")?,
      entries: files
        .into_iter()
        .map(|f| crate::ReviewedSkillEntry::File {
          path: f.path,
          sha256: f.sha256,
          bytes: f.bytes,
        })
        .collect(),
    })
  }
  /// Parses a bounded Project-bound skill continuation; it conveys no content or authority.
  pub fn list_project_skills(
    &self,
    project: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::ListProjectSkillsQuery, ManagementInputError> {
    let page: crate::CurrentDefinitionPageInput<String> = self.factory_role_page(project, None, limit)?;
    let after = after
      .map(|cursor| {
        let mut parts = cursor.split(':');
        let invalid = || ManagementInputError::Invalid("skill cursor");
        if parts.next() != Some("1") || parts.next() != Some(project) {
          return Err(invalid());
        }
        let identity = parse(parts.next().ok_or_else(invalid)?, "skill identity")?;
        let version = parse(parts.next().ok_or_else(invalid)?, "skill version")?;
        let digest = parse(parts.next().ok_or_else(invalid)?, "skill digest")?;
        if parts.next().is_some() {
          return Err(invalid());
        }
        Ok(octacity_server_factory::ImmutableReference::new(
          identity, version, digest,
        ))
      })
      .transpose()?;
    Ok(crate::ListProjectSkillsQuery {
      project_id: page.project_id(),
      after,
      limit: page.limit(),
    })
  }

  /// Parses a complete role draft; normalization itself remains an authorized query.
  pub fn validate_factory_role(
    &self,
    project: &str,
    id: &str,
    definition: Value,
  ) -> Result<crate::ValidateFactoryRoleQuery, ManagementInputError> {
    Ok(crate::ValidateFactoryRoleQuery {
      project_id: parse(project, "project id")?,
      id: parse(id, "role id")?,
      definition: factory_decode(definition)?,
    })
  }
  /// Reads an exact retained skill and optional relative file under Project authorization.
  pub fn get_project_skill(
    &self,
    project: &str,
    identity: &str,
    version: &str,
    digest: &str,
    path: Option<&str>,
  ) -> Result<crate::GetProjectSkillQuery, ManagementInputError> {
    Ok(crate::GetProjectSkillQuery {
      project_id: parse(project, "project id")?,
      reference: octacity_server_factory::ImmutableReference::new(
        parse(identity, "skill identity")?,
        parse(version, "skill version")?,
        parse(digest, "skill digest")?,
      ),
      path: path
        .map(|path| {
          octacity_server_factory::FactoryRepositoryPath::new(path)
            .map_err(|_| ManagementInputError::Invalid("skill file path"))
        })
        .transpose()?,
    })
  }
  /// Reads one exact release template after Project authorization.
  pub fn get_factory_role_template(
    &self,
    project: &str,
    identity: &str,
    version: &str,
    digest: &str,
  ) -> Result<crate::GetFactoryRoleTemplateQuery, ManagementInputError> {
    Ok(crate::GetFactoryRoleTemplateQuery {
      project_id: parse(project, "project id")?,
      reference: octacity_server_factory::ImmutableReference::new(
        parse(identity, "template identity")?,
        parse(version, "template version")?,
        parse(digest, "template digest")?,
      ),
    })
  }
  /// Preserves reviewed copy settings for the common atomic publisher.
  pub fn copy_factory_role_template(
    &self,
    project: &str,
    value: Value,
    key: &str,
    now: i64,
  ) -> Result<crate::CopyFactoryRoleTemplateCommand, ManagementInputError> {
    use octacity_server_factory::*;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Binding {
      id: FactoryRoleProfileId,
      definition: FactoryRoleProfileDefinition,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Draft {
      template: ImmutableReference,
      role_id: FactoryRoleId,
      role: FactoryRoleDefinition,
      profiles: std::collections::BTreeMap<FactoryKey, Binding>,
    }
    let draft: Draft = factory_decode(value)?;
    Ok(crate::CopyFactoryRoleTemplateCommand {
      project_id: parse(project, "project id")?,
      template: draft.template,
      role_id: draft.role_id,
      role: draft.role,
      profiles: draft
        .profiles
        .into_iter()
        .map(|(key, b)| {
          (
            key,
            FactoryRoleTemplateBinding {
              id: b.id,
              definition: b.definition,
            },
          )
        })
        .collect(),
      idempotency_key: parse(key, "idempotency key")?,
      published_at: timestamp(now)?,
    })
  }

  /// Reuses Project-owned catalog bounds and typed exclusive continuation identities.
  pub fn factory_role_page<I: std::str::FromStr>(
    &self,
    project: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::CurrentDefinitionPageInput<I>, ManagementInputError> {
    crate::CurrentDefinitionPageInput::try_new(
      parse(project, "project id")?,
      optional_parse(after, "role catalog cursor")?,
      limit,
    )
    .map_err(|_| ManagementInputError::Invalid("role catalog page"))
  }
  /// Dedicated profile content read with an optional exact retained version.
  pub fn get_factory_role_profile(
    &self,
    project: &str,
    id: &str,
    exact: Option<u64>,
  ) -> Result<crate::GetFactoryRoleProfileQuery, ManagementInputError> {
    Ok(crate::GetFactoryRoleProfileQuery {
      project_id: parse(project, "project id")?,
      id: parse(id, "profile id")?,
      version: exact.map(|v| version(v, "profile version")).transpose()?,
    })
  }

  /// Parses one reviewed atomic role/profile publication; item versions fence independent heads.
  pub fn publish_factory_role_profiles(
    &self,
    project: &str,
    role: Option<Value>,
    profiles: Vec<Value>,
    key: &str,
    now: i64,
  ) -> Result<crate::PublishFactoryRoleProfilesCommand, ManagementInputError> {
    let intent: crate::FactoryRoleProfilesIntent = factory_decode(serde_json::json!({
      "project_id": project, "role": role, "profiles": profiles
    }))?;
    intent
      .validate()
      .map_err(|_| ManagementInputError::Invalid("role/profile publication"))?;
    Ok(crate::PublishFactoryRoleProfilesCommand {
      intent,
      idempotency_key: parse(key, "idempotency key")?,
      published_at: timestamp(now)?,
    })
  }
  /// Dedicated role content lookup, current or pinned; ownership is authorized by the query.
  pub fn get_factory_role(
    &self,
    project: &str,
    id: &str,
    exact: Option<u64>,
  ) -> Result<crate::GetFactoryRoleQuery, ManagementInputError> {
    Ok(crate::GetFactoryRoleQuery {
      project_id: parse(project, "project id")?,
      id: parse(id, "role id")?,
      version: exact.map(|v| version(v, "role version")).transpose()?,
    })
  }

  /// Decodes a Project-owned schema shape while leaving its content identity to the core.
  pub fn validate_factory_flow_data_schema(
    &self,
    project: &str,
    identity: &str,
    version: &str,
    shape: Value,
  ) -> Result<crate::ValidateFactoryFlowDataSchemaQuery, ManagementInputError> {
    Ok(crate::ValidateFactoryFlowDataSchemaQuery {
      project_id: parse(project, "project id")?,
      identity: factory_decode(Value::String(identity.to_owned()))?,
      version: factory_decode(Value::String(version.to_owned()))?,
      shape: factory_decode(shape)?,
    })
  }
  /// Parses closed operator choices without accepting caller lifecycle data.
  pub fn confirm_factory_node_operation(
    &self,
    input: FactoryNodeControlInput<'_>,
    operation: &str,
    disposition: &str,
    reason: &str,
  ) -> Result<crate::ConfirmFactoryNodeOperationCommand, ManagementInputError> {
    let operation: octacity_server_factory::FlowHumanOperation = factory_decode(Value::String(operation.to_owned()))?;
    let disposition = factory_decode(Value::String(disposition.to_owned()))?;
    if !operation.permits(disposition) {
      return Err(ManagementInputError::Invalid("operator disposition"));
    }
    Ok(crate::ConfirmFactoryNodeOperationCommand {
      run_id: parse(input.run_id, "factory run id")?,
      node_attempt_id: parse(input.node_attempt_id, "node attempt id")?,
      operation,
      disposition,
      reason: parse(reason, "operator reason")?,
      expected_version: version(input.expected_version, "factory run version")?,
      idempotency_key: parse(input.idempotency_key, "idempotency key")?,
      requested_at: timestamp(input.requested_at)?,
    })
  }
  /// Confirms a retry target; atomic store validation owns technical eligibility.
  pub fn retry_factory_node(
    &self,
    input: FactoryNodeControlInput<'_>,
  ) -> Result<crate::RetryFactoryNodeCommand, ManagementInputError> {
    Ok(crate::RetryFactoryNodeCommand {
      run_id: parse(input.run_id, "factory run id")?,
      node_attempt_id: parse(input.node_attempt_id, "node attempt id")?,
      expected_version: version(input.expected_version, "factory run version")?,
      idempotency_key: parse(input.idempotency_key, "idempotency key")?,
      requested_at: timestamp(input.requested_at)?,
    })
  }
  /// Decodes bounded observations without accepting a caller-provided schema.
  pub fn submit_factory_node_response(
    &self,
    input: FactoryNodeControlInput<'_>,
    outcome: &str,
    value: Value,
  ) -> Result<crate::SubmitFactoryNodeResponseCommand, ManagementInputError> {
    Ok(crate::SubmitFactoryNodeResponseCommand {
      run_id: parse(input.run_id, "factory run id")?,
      node_attempt_id: parse(input.node_attempt_id, "node attempt id")?,
      outcome: parse(outcome, "node outcome")?,
      value,
      expected_version: version(input.expected_version, "factory run version")?,
      idempotency_key: parse(input.idempotency_key, "idempotency key")?,
      requested_at: timestamp(input.requested_at)?,
    })
  }
  /// Decodes cancellation of the exact version observed by the operator.
  pub fn cancel_factory_run(
    &self,
    run: &str,
    expected: u64,
    key: &str,
    now: i64,
  ) -> Result<crate::CancelFactoryRunCommand, ManagementInputError> {
    Ok(crate::CancelFactoryRunCommand {
      run_id: parse(run, "factory run id")?,
      expected_version: version(expected, "factory run version")?,
      idempotency_key: parse(key, "idempotency key")?,
      requested_at: timestamp(now)?,
    })
  }
  /// Creates an immutable Flow publication intent.
  pub fn create_factory_flow_definition(
    &self,
    id: &str,
    project_id: &str,
    draft: Value,
    key: &str,
    now: i64,
  ) -> Result<crate::CreateFactoryFlowDefinitionCommand, ManagementInputError> {
    let query = self.validate_factory_flow_definition(id, project_id, draft)?;
    Ok(crate::CreateFactoryFlowDefinitionCommand {
      id: query.id,
      project_id: query.project_id,
      draft: query.draft,
      idempotency_key: parse(key, "idempotency key")?,
      published_at: timestamp(now)?,
    })
  }
  /// Creates the next immutable Flow version intent under a strong precondition.
  pub fn replace_factory_flow_definition(
    &self,
    id: &str,
    expected: u64,
    draft: Value,
    key: &str,
    now: i64,
  ) -> Result<crate::ReplaceFactoryFlowDefinitionCommand, ManagementInputError> {
    Ok(crate::ReplaceFactoryFlowDefinitionCommand {
      id: parse(id, "flow definition id")?,
      expected_current_version: version(expected, "flow definition version")?,
      draft: factory_decode(draft)?,
      idempotency_key: parse(key, "idempotency key")?,
      published_at: timestamp(now)?,
    })
  }
  /// Reads one exact or current immutable Flow.
  pub fn get_factory_flow_definition(
    &self,
    id: &str,
    selected: Option<u64>,
  ) -> Result<crate::GetFactoryFlowDefinitionQuery, ManagementInputError> {
    Ok(crate::GetFactoryFlowDefinitionQuery {
      id: parse(id, "flow definition id")?,
      version: selected.map(|v| version(v, "flow definition version")).transpose()?,
    })
  }
  /// Decodes a bounded visible Flow collection and exact composite cursor.
  pub fn list_project_factory_flow_definitions(
    &self,
    project: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::ListProjectFactoryFlowDefinitionsQuery, ManagementInputError> {
    if limit == 0 || limit > octacity_server_store::MAX_FACTORY_CONFIGURATION_PAGE_SIZE {
      return Err(ManagementInputError::Invalid("flow definition page"));
    }
    let after = after
      .map(|value| {
        let (time, id) = factory_composite_cursor(value)?;
        Ok(octacity_server_store::FlowDefinitionPagePosition {
          created_at: time,
          id: parse(id, "flow definition cursor")?,
        })
      })
      .transpose()?;
    Ok(crate::ListProjectFactoryFlowDefinitionsQuery {
      project_id: parse(project, "project id")?,
      after,
      limit,
    })
  }

  /// Decodes the closed root graph authoring grammar without accepting caller digests.
  pub fn validate_factory_flow_definition(
    &self,
    id: &str,
    project_id: &str,
    draft: Value,
  ) -> Result<crate::ValidateFactoryFlowDefinitionQuery, ManagementInputError> {
    Ok(crate::ValidateFactoryFlowDefinitionQuery {
      id: parse(id, "flow definition id")?,
      project_id: parse(project_id, "project id")?,
      draft: factory_decode(draft)?,
    })
  }

  /// Resolves and validates a complete candidate without publishing it.
  pub fn validate_factory_configuration(
    &self,
    id: &str,
    project_id: &str,
    draft: Value,
  ) -> Result<crate::ValidateFactoryConfigurationQuery, ManagementInputError> {
    let (draft, definition_digest) = configuration_draft(draft)?;
    Ok(crate::ValidateFactoryConfigurationQuery {
      id: parse(id, "factory configuration id")?,
      project_id: parse(project_id, "project id")?,
      definition_digest,
      draft,
    })
  }

  /// Constructs an immutable publication from an explicit caller-owned graph.
  /// The stable identity is retained by the client across an unknown response.
  pub fn create_factory_configuration(
    &self,
    id: Uuid,
    project_id: &str,
    draft: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<crate::CreateFactoryConfigurationCommand, ManagementInputError> {
    let (draft, definition_digest) = configuration_draft(draft)?;
    Ok(crate::CreateFactoryConfigurationCommand {
      id: identifier(id, "factory configuration id")?,
      project_id: parse(project_id, "project id")?,
      definition_digest,
      draft,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Constructs the next immutable version with an explicit current-version precondition.
  pub fn replace_factory_configuration(
    &self,
    id: &str,
    expected_current_version: u64,
    draft: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<crate::ReplaceFactoryConfigurationCommand, ManagementInputError> {
    let (draft, definition_digest) = configuration_draft(draft)?;
    Ok(crate::ReplaceFactoryConfigurationCommand {
      id: parse(id, "factory configuration id")?,
      expected_current_version: version(expected_current_version, "factory configuration version")?,
      definition_digest,
      draft,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }
  /// Decodes a bounded exact configuration-choice collection.
  pub fn list_factory_configuration_choices(
    &self,
    project_id: &str,
    kind: Option<&str>,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::ListFactoryConfigurationChoicesQuery, ManagementInputError> {
    let kind = kind
      .map(|kind| decode(Value::String(kind.to_owned()), "factory choice category"))
      .transpose()?;
    crate::ListFactoryConfigurationChoicesQuery::try_new(
      parse(project_id, "project id")?,
      kind,
      optional_parse(after, "factory choice cursor")?,
      limit,
    )
    .map_err(|_| ManagementInputError::Invalid("factory choice page"))
  }

  /// Reads one exact immutable Factory Configuration version.
  pub fn get_factory_configuration(
    &self,
    id: &str,
    version_value: u64,
  ) -> Result<crate::GetFactoryConfigurationQuery, ManagementInputError> {
    Ok(crate::GetFactoryConfigurationQuery {
      id: parse(id, "factory configuration id")?,
      version: version(version_value, "factory configuration version")?,
    })
  }

  /// Reads the current immutable Factory Configuration version.
  pub fn get_current_factory_configuration(
    &self,
    id: &str,
  ) -> Result<crate::GetCurrentFactoryConfigurationQuery, ManagementInputError> {
    Ok(crate::GetCurrentFactoryConfigurationQuery {
      id: parse(id, "factory configuration id")?,
    })
  }

  /// Validates a Project configuration collection and its exclusive composite cursor.
  pub fn list_project_factory_configurations(
    &self,
    project_id: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<ListProjectFactoryConfigurationsQuery, ManagementInputError> {
    let after = after
      .map(|encoded| {
        if encoded.len() > 128 {
          return Err(ManagementInputError::Invalid("factory configuration cursor"));
        }
        let mut parts = encoded.split(':');
        let (Some("1"), Some(time), Some(id), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
          return Err(ManagementInputError::Invalid("factory configuration cursor"));
        };
        let position = FactoryConfigurationPagePosition {
          created_at: timestamp(parse(time, "factory configuration cursor")?)?,
          configuration_id: parse(id, "factory configuration cursor")?,
        };
        if format!("1:{}:{}", position.created_at.unix_millis(), position.configuration_id) != encoded {
          return Err(ManagementInputError::Invalid("factory configuration cursor"));
        }
        Ok(position)
      })
      .transpose()?;
    ListProjectFactoryConfigurationsQuery::try_new(parse(project_id, "project id")?, after, limit)
      .map_err(|_| ManagementInputError::Invalid("factory configuration page"))
  }
}

fn configuration_draft(
  value: Value,
) -> Result<
  (
    octacity_server_factory::FactoryConfigurationDraft,
    octacity_server_factory::FactoryDigest,
  ),
  ManagementInputError,
> {
  let draft = factory_decode(value)?;
  let bytes =
    serde_json::to_vec(&draft).map_err(|_| ManagementInputError::Invalid("factory configuration definition"))?;
  let digest = octacity_server_factory::FactoryDigest::sha256("octacity.factory.configuration-draft.v1", &[&bytes]);
  Ok((draft, digest))
}

fn factory_composite_cursor(value: &str) -> Result<(Timestamp, &str), ManagementInputError> {
  if value.len() > 128 {
    return Err(ManagementInputError::Invalid("factory cursor"));
  }
  let mut parts = value.split(':');
  let (Some("1"), Some(time), Some(id), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
    return Err(ManagementInputError::Invalid("factory cursor"));
  };
  let time = timestamp(parse(time, "factory cursor")?)?;
  let uuid: Uuid = parse(id, "factory cursor")?;
  if uuid.is_nil() || format!("1:{}:{}", time.unix_millis(), uuid) != value {
    return Err(ManagementInputError::Invalid("factory cursor"));
  }
  Ok((time, id))
}

/// Provider-neutral manual Work fields decoded at the management boundary.
#[derive(Clone, Debug)]
pub struct ManualFactoryWorkInput {
  /// Owning Project.
  pub project_id: String,
  /// Exact configuration identity.
  pub configuration_id: String,
  /// Exact configuration version.
  pub configuration_version: u64,
  /// Exact repository identity.
  pub repository_id: String,
  /// Exact repository version.
  pub repository_version: u64,
  /// Stable external source identity.
  pub external_identity: String,
  /// Closed source-selection grammar.
  pub source: Value,
  /// Exact immutable artifact references.
  pub artifacts: Value,
  /// Bounded admission classification.
  pub classification: Value,
  /// Complete optional exact stage/profile overrides, normalized by the shared core contract.
  pub profile_overrides: Vec<Value>,
}
impl ManagementInputFactory {
  /// Builds a validated manual intent without resolving any mutable source.
  pub fn admit_manual_factory_work(
    &self,
    input: ManualFactoryWorkInput,
    key: &str,
    now: i64,
  ) -> Result<crate::AdmitManualFactoryWorkCommand, ManagementInputError> {
    use octacity_server_factory::{FactoryMetadata, WorkArtifacts, WorkClassification};
    let artifacts: WorkArtifacts = decode(input.artifacts, "work artifacts")?;
    let artifacts = WorkArtifacts::new(
      artifacts.task().clone(),
      artifacts.acceptance().clone(),
      artifacts.specifications().to_vec(),
    )
    .map_err(|_| ManagementInputError::Invalid("work artifacts"))?;
    let classification: WorkClassification = decode(input.classification, "work classification")?;
    let metadata = FactoryMetadata::try_new(
      classification
        .metadata()
        .iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned())),
    )
    .map_err(|_| ManagementInputError::Invalid("work metadata"))?;
    Ok(crate::AdmitManualFactoryWorkCommand {
      profile_overrides: factory_decode(Value::Array(input.profile_overrides))?,
      project_id: parse(&input.project_id, "project id")?,
      configuration_id: parse(&input.configuration_id, "factory configuration id")?,
      configuration_version: version(input.configuration_version, "factory configuration version")?,
      repository_id: parse(&input.repository_id, "repository id")?,
      repository_version: version(input.repository_version, "repository version")?,
      external_identity: parse(&input.external_identity, "external work identity")?,
      source: decode(input.source, "work source")?,
      artifacts,
      classification: WorkClassification::new(classification.priority(), classification.risk(), metadata),
      idempotency_key: parse(key, "idempotency key")?,
      admitted_at: timestamp(now)?,
    })
  }
}

/// Bounded Factory Run filter expressions accepted from management clients.
#[derive(Clone, Debug, Default)]
pub struct FactoryRunListInput {
  /// Optional visible Project; absent denotes the global collection.
  pub project_id: Option<String>,
  /// Exact Factory Configuration identity.
  pub configuration_id: Option<String>,
  /// Exact source kind.
  pub source: Option<String>,
  /// Closed technical Run state; logical phases live in the Flow.
  pub state: Option<String>,
  /// Authoritative current-candidate disposition; advisory signals never match.
  pub decision: Option<String>,
  /// Closed current-candidate delivery state.
  pub delivery_state: Option<String>,
  /// Inclusive lower admission time.
  pub admitted_from_unix_ms: Option<i64>,
  /// Exclusive upper admission time.
  pub admitted_before_unix_ms: Option<i64>,
  /// Exclusive canonical composite continuation.
  pub after: Option<String>,
  /// Positive bounded page size.
  pub limit: u16,
}
impl ManagementInputFactory {
  /// Decodes a Run page without resolving a configuration, source or cursor anchor.
  pub fn list_factory_runs(
    &self,
    input: FactoryRunListInput,
  ) -> Result<crate::ListFactoryRunsQuery, ManagementInputError> {
    let after = input
      .after
      .as_deref()
      .map(|value| {
        let (admitted_at, id) = factory_composite_cursor(value)?;
        Ok(octacity_server_store::FactoryRunPagePosition {
          admitted_at,
          run_id: parse(id, "factory run cursor")?,
        })
      })
      .transpose()?;
    crate::ListFactoryRunsQuery::try_new(
      optional_parse(input.project_id.as_deref(), "project id")?,
      octacity_server_store::FactoryRunFilter {
        configuration_id: optional_parse(input.configuration_id.as_deref(), "factory configuration id")?,
        source: optional_parse(input.source.as_deref(), "work source kind")?,
        state: optional_parse(input.state.as_deref(), "factory run state")?,
        decision: optional_parse(input.decision.as_deref(), "factory decision outcome")?,
        delivery_state: optional_parse(input.delivery_state.as_deref(), "factory delivery state")?,
        admitted_from: input.admitted_from_unix_ms.map(timestamp).transpose()?,
        admitted_before: input.admitted_before_unix_ms.map(timestamp).transpose()?,
      },
      after,
      input.limit,
    )
    .map_err(|_| ManagementInputError::Invalid("factory run page"))
  }
}

impl ManagementInputFactory {
  /// Decodes one exact phase policy and a bounded self-contained continuation.
  pub fn list_factory_phase_ready(
    &self,
    project: Option<&str>,
    policy: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::ListFactoryPhaseReadyQuery, ManagementInputError> {
    crate::ListFactoryPhaseReadyQuery::try_new(
      optional_parse(project, "project id")?,
      parse(policy, "phase policy digest")?,
      optional_parse(after, "phase ready cursor")?,
      limit,
    )
    .map_err(|_| ManagementInputError::Invalid("phase ready page"))
  }
}

fn factory_decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ManagementInputError> {
  serde_path_to_error::deserialize(value).map_err(|error| {
    let mut field = String::new();
    for segment in error.path() {
      match segment {
        serde_path_to_error::Segment::Map { key } => {
          if key.len() <= 128 {
            field.push('/');
            field.push_str(&key.replace('~', "~0").replace('/', "~1"));
          }
        }
        serde_path_to_error::Segment::Seq { index } => {
          field.push('/');
          field.push_str(&index.to_string());
        }
        _ => {}
      }
    }
    // Constructor errors can occur at the enclosing wire object. Only known
    // static constructor field names refine that path; error prose is never exposed.
    if let Some(name) = error
      .inner()
      .to_string()
      .strip_prefix("invalid factory configuration field: ")
      && matches!(name, "max_active_runs" | "max_active_nodes")
    {
      field.push('/');
      field.push_str(name);
    }
    if field.is_empty() || field.len() > 2048 {
      field = "/".to_owned();
    }
    let crate::ApplicationError::FactoryValidation(report) = crate::FactoryValidationDiagnostics::invalid(field) else {
      unreachable!()
    };
    ManagementInputError::FactoryValidation(report)
  })
}

impl ManagementInputFactory {
  /// Decodes a current Factory Run detail request.
  pub fn get_factory_run(&self, id: &str) -> Result<crate::GetFactoryRunQuery, ManagementInputError> {
    Ok(crate::GetFactoryRunQuery {
      run_id: parse(id, "factory run id")?,
    })
  }
}

impl ManagementInputFactory {
  /// Decodes a closed diagnostic collection and its self-contained continuation.
  pub fn list_factory_diagnostics(
    &self,
    run: &str,
    collection: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::ListFactoryDiagnosticsQuery, ManagementInputError> {
    crate::ListFactoryDiagnosticsQuery::try_new(
      parse(run, "factory run id")?,
      decode(Value::String(collection.to_owned()), "factory diagnostic collection")?,
      after,
      limit,
    )
    .map_err(|_| ManagementInputError::Invalid("factory diagnostic page"))
  }
}
