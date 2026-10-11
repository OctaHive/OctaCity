//! Bounded composition of editable documents before immutable publication.
use crate::{
  FactoryError, FactoryKey, FactoryRepositoryPath, FlowContextProjection, FlowDataProjection, FlowDefinitionTemplate,
  FlowExecutionPolicy, FlowTemplateNode, FlowTemplateSchema, FlowTransition, MAX_FLOW_DATA_BYTES,
  MAX_FLOW_DEFINITION_EDGES, MAX_FLOW_DEFINITION_NODES, MAX_FLOW_SCHEMA_ENTRIES,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
  input_schema: FactoryKey,
  entry: FactoryKey,
  execution: FlowExecutionPolicy,
  parts: Vec<FactoryRepositoryPath>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Part {
  schemas: BTreeMap<FactoryKey, FlowTemplateSchema>,
  deliverable_policies: BTreeMap<FactoryKey, crate::CandidateDeliverablePolicy>,
  nodes: Vec<FlowTemplateNode>,
  transitions: Vec<FlowTransition>,
  terminals: BTreeMap<FactoryKey, FactoryKey>,
  context_projections: Vec<FlowContextProjection>,
  data_projections: Vec<FlowDataProjection>,
}
impl FlowDefinitionTemplate {
  /// Assembles explicitly listed, flat configuration parts into the same editable
  /// template consumed by `instantiate`. File and node names carry no semantics.
  ///
  /// The caller resolves portable relative paths from its chosen configuration
  /// root; the domain performs no filesystem or network access. Resolve all parts
  /// before publication: admitted runs retain only the immutable compiled closure,
  /// never mutable file references. Nested imports and silent overrides are rejected.
  pub fn from_manifest(
    bytes: &[u8],
    mut resolve: impl FnMut(&FactoryRepositoryPath) -> Result<Vec<u8>, FactoryError>,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if manifest.parts.is_empty() || manifest.parts.len() > MAX_FLOW_DEFINITION_NODES {
      return Err(invalid());
    }
    let mut template = Self {
      input_schema: manifest.input_schema,
      entry: manifest.entry,
      execution: manifest.execution,
      schemas: BTreeMap::new(),
      deliverable_policies: BTreeMap::new(),
      nodes: vec![],
      transitions: vec![],
      terminals: BTreeMap::new(),
      context_projections: vec![],
      data_projections: vec![],
    };
    let mut total = bytes.len();
    let mut paths = std::collections::BTreeSet::new();
    for path in manifest.parts {
      if !paths.insert(path.clone()) {
        return Err(invalid());
      }
      let bytes = resolve(&path)?;
      total = total.checked_add(bytes.len()).ok_or_else(invalid)?;
      if total > MAX_FLOW_DATA_BYTES {
        return Err(invalid());
      }
      let part: Part = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
      for (key, schema) in part.schemas {
        if template.schemas.insert(key, schema).is_some() {
          return Err(invalid());
        }
      }
      for (key, policy) in part.deliverable_policies {
        if template.deliverable_policies.insert(key, policy).is_some() {
          return Err(invalid());
        }
      }
      for (key, schema) in part.terminals {
        if template.terminals.insert(key, schema).is_some() {
          return Err(invalid());
        }
      }
      for node in part.nodes {
        if template.nodes.iter().any(|selected| selected.key == node.key) {
          return Err(invalid());
        }
        template.nodes.push(node);
      }
      template.transitions.extend(part.transitions);
      template.context_projections.extend(part.context_projections);
      template.data_projections.extend(part.data_projections);
      if template.schemas.len() > MAX_FLOW_SCHEMA_ENTRIES
        || template.deliverable_policies.len() > MAX_FLOW_SCHEMA_ENTRIES
        || template.nodes.len() > MAX_FLOW_DEFINITION_NODES
        || template.transitions.len() > MAX_FLOW_DEFINITION_EDGES
      {
        return Err(invalid());
      }
    }
    Ok(template)
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow template manifest",
  }
}
