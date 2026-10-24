//! Plugin metadata uses the common verified process host and Flow parameter grammar.
use octacity_server_adapter_host::{ProcessRequest, VerifiedExecutable, execute_process, verify_installed_program};
use octacity_server_factory::*;
use octacity_server_store::StoreError;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;

/// Trusted attachment to an existing installed profile, never constructed from Role content.
#[derive(Clone)]
pub(super) struct PluginModelDiscovery {
  executable: VerifiedExecutable,
  plugin: ImmutableReference,
  environment: BTreeMap<String, String>,
  /// Held through snapshot replacement; overlapping refreshes fail without queuing.
  pub(super) refresh: std::sync::Arc<tokio::sync::Mutex<()>>,
}
impl PluginModelDiscovery {
  pub(super) fn load(
    path: &Path,
    plugin: &ImmutableReference,
    executable_sha256: &str,
    environment: BTreeMap<String, String>,
  ) -> Result<Self, StoreError> {
    if environment.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || environment.iter().any(|(key, value)| {
        key.is_empty() || key.contains(['=', '\0']) || value.len() > MAX_FACTORY_TEXT_BYTES || value.contains('\0')
      })
    {
      return Err(StoreError::Unavailable);
    }
    let executable = verify_installed_program(path, plugin.identity().as_str(), executable_sha256)
      .map_err(|_| StoreError::Unavailable)?;
    Ok(Self {
      executable,
      plugin: plugin.clone(),
      environment,
      refresh: Default::default(),
    })
  }
  pub(super) async fn discover(&self) -> Result<(FactoryRoleModelCatalog, FlowDataSchema), StoreError> {
    let arguments = vec!["--describe-models".to_owned()];
    let request = ProcessRequest::new(
      &self.executable,
      "model-catalog",
      b"{}\n".to_vec(),
      None,
      octacity_protocol::MAX_MODEL_CATALOG_BYTES,
    )
    .with_launch(&arguments, &self.environment);
    let catalog: octacity_protocol::ModelCatalog = execute_process(
      request,
      Duration::from_secs(15),
      Duration::from_secs(2),
      CancellationToken::new(),
      |frame| serde_json::from_slice(frame).map_err(|_| "invalid plugin model metadata".to_owned()),
    )
    .await
    .map_err(|_| StoreError::Unavailable)?;
    let parameters = self.schema(&catalog.parameters)?;
    let models = catalog
      .models
      .into_iter()
      .map(|model| {
        Ok(FactoryRoleModelChoice {
          identifier: model.id,
          label: model.label,
          parameters: self.schema(&model.parameters)?,
        })
      })
      .collect::<Result<Vec<_>, StoreError>>()?;
    Ok((
      FactoryRoleModelCatalog {
        available: catalog.available,
        manual_model: catalog.manual_model,
        models,
      },
      parameters,
    ))
  }
  fn schema(&self, json: &Map<String, Value>) -> Result<FlowDataSchema, StoreError> {
    if json.get("type").and_then(Value::as_str) != Some("object")
      || json.get("additionalProperties") != Some(&Value::Bool(false))
    {
      return Err(StoreError::Unavailable);
    }
    // The existing profile contract accepts scalar parameters only. Unsupported
    // JSON Schema constructs fail explicitly rather than broadening validation.
    if json.keys().any(|key| {
      ![
        "type",
        "additionalProperties",
        "properties",
        "required",
        "title",
        "description",
        "$schema",
      ]
      .contains(&key.as_str())
    }) {
      return Err(StoreError::Unavailable);
    }
    let properties = json
      .get("properties")
      .and_then(Value::as_object)
      .ok_or(StoreError::Unavailable)?;
    let required = match json.get("required") {
      Some(value) => value.as_array().cloned().ok_or(StoreError::Unavailable)?,
      None => vec![],
    };
    if required
      .iter()
      .any(|key| key.as_str().is_none_or(|key| !properties.contains_key(key)))
    {
      return Err(StoreError::Unavailable);
    }
    let fields = properties
      .iter()
      .map(|(name, property)| {
        let name = FactoryKey::new(name).map_err(|_| StoreError::Unavailable)?;
        let shape = scalar(property)?;
        let field = if required.iter().any(|key| key.as_str() == Some(name.as_str())) {
          FlowFieldSchema::required(shape)
        } else {
          FlowFieldSchema::optional(shape)
        };
        Ok((name, field))
      })
      .collect::<Result<_, StoreError>>()?;
    FlowDataSchema::new(
      self.plugin.identity().clone(),
      self.plugin.version().clone(),
      FlowValueSchema::Object { fields },
    )
    .map_err(|_| StoreError::Unavailable)
  }
}
fn scalar(value: &Value) -> Result<FlowValueSchema, StoreError> {
  let object = value.as_object().ok_or(StoreError::Unavailable)?;
  if object.keys().any(|key| {
    ![
      "type",
      "enum",
      "minimum",
      "maximum",
      "minLength",
      "maxLength",
      "default",
      "title",
      "description",
    ]
    .contains(&key.as_str())
  }) {
    return Err(StoreError::Unavailable);
  }
  let kind = value["type"].as_str();
  let constraints = ["minimum", "maximum", "minLength", "maxLength"];
  if let Some(values) = value.get("enum") {
    let values = values.as_array().ok_or(StoreError::Unavailable)?;
    if constraints.iter().any(|key| value.get(key).is_some())
      || values.iter().any(|item| match kind {
        Some("string") => !item.is_string(),
        Some("integer") => item.as_i64().is_none(),
        Some("number") => !item.is_number(),
        Some("boolean") => !item.is_boolean(),
        None => false,
        _ => true,
      })
    {
      return Err(StoreError::Unavailable);
    }
    return Ok(FlowValueSchema::Enum { values: values.clone() });
  }
  if constraints.iter().any(|key| {
    value.get(key).is_some()
      && match kind {
        Some("string") => !matches!(*key, "minLength" | "maxLength"),
        Some("integer") => !matches!(*key, "minimum" | "maximum"),
        _ => true,
      }
  }) {
    return Err(StoreError::Unavailable);
  }
  // Flow measures UTF-8 bytes. A positive character minimum cannot be
  // represented without admitting strings shorter than the plugin permits.
  if value
    .get("minLength")
    .is_some_and(|minimum| minimum.as_u64() != Some(0))
  {
    return Err(StoreError::Unavailable);
  }
  match kind {
    Some("string") => Ok(FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: value
        .get("maxLength")
        .map_or(Some(MAX_FACTORY_TEXT_BYTES as u64), Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(StoreError::Unavailable)?,
    }),
    Some("integer") => Ok(FlowValueSchema::Integer {
      minimum: value["minimum"].as_i64().ok_or(StoreError::Unavailable)?,
      maximum: value["maximum"].as_i64().ok_or(StoreError::Unavailable)?,
    }),
    Some("boolean") => Ok(FlowValueSchema::Boolean),
    _ => Err(StoreError::Unavailable),
  }
}
