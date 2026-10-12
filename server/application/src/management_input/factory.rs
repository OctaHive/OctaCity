use super::*;
use crate::{FactoryConfigurationPagePosition, ListProjectFactoryConfigurationsQuery};

impl ManagementInputFactory {
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
