use super::{mapping::copy_bounded, predicate::validate_pointer};
use crate::{
  FactoryClaimOwnership, FactoryError, FactoryKey, FlowDataMapping, FlowInputPreparation, FlowNodeRecord, FlowPayload,
  FlowRecordReference, MAX_FLOW_DATA_BYTES, MAX_FLOW_SCHEMA_ENTRIES,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Hard encoded ceiling for one node's configured input selection.
pub const MAX_FLOW_INPUT_BINDING_BYTES: usize = 16 * 1024;

/// Explicit projection view of one accepted predecessor result.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowRecordView {
  /// Only the schema-validated output payload.
  Payload,
  /// Only owner metadata and independently verified output facts.
  Metadata,
}

/// Explicit trusted source of projected input data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "source", deny_unknown_fields)]
pub enum FlowInputSource {
  /// Explicit projection of the exact frozen input of this Flow's parent call.
  CallerInput {
    /// RFC 6901 selection in the calling node's payload; no implicit context transfer.
    path: String,
  },
  /// Latest producing attempt on an explicitly declared incoming control edge.
  /// An unfinished or differently routed newer producer blocks an older result.
  Predecessor {
    /// Explicit selected result view.
    view: FlowRecordView,
    /// RFC 6901 projection within that view.
    path: String,
  },
  /// Arbitrary owner-retained incoming data under an exact configured schema.
  Incoming {
    /// Exact incoming data contract; a matching shape under another schema is insufficient.
    schema: crate::ImmutableReference,
    /// RFC 6901 selection in the incoming payload.
    path: String,
  },
  /// Structured immutable Work data, without provider-authored control metadata.
  Work {
    /// RFC 6901 selection in the owner-retained Work envelope.
    path: String,
  },
  /// Exact latest committed result in the owning Flow and Workflow Cycle.
  Record {
    /// Configured producing node key in the same definition.
    node: FactoryKey,
    /// Required finite producer outcome; newer differing outcomes block selection.
    outcome: FactoryKey,
    /// RFC 6901 selection in the producing result payload.
    path: String,
  },
}

/// Operator-owned source aliases and mapping into one node's declared input schema.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "BindingWire")]
pub struct FlowInputBinding {
  sources: BTreeMap<FactoryKey, FlowInputSource>,
  mapping: FlowDataMapping,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingWire {
  sources: BTreeMap<FactoryKey, FlowInputSource>,
  mapping: FlowDataMapping,
}
impl TryFrom<BindingWire> for FlowInputBinding {
  type Error = FactoryError;
  fn try_from(wire: BindingWire) -> Result<Self, Self::Error> {
    Self::new(wire.sources, wire.mapping)
  }
}
// Explicit retained provenance accompanies every schema-checked projection.
pub(super) struct ProjectedInput {
  pub payload: FlowPayload,
  pub sources: Vec<FlowRecordReference>,
  pub incoming_digest: Option<crate::FactoryDigest>,
  pub caller_input_digest: Option<crate::FactoryDigest>,
}
impl FlowInputBinding {
  pub(super) fn validate_sources(&self, nodes: &[crate::FlowNodeDefinition]) -> Result<(), FactoryError> {
    for source in self.sources.values() {
      if let FlowInputSource::Record { node, outcome, .. } = source
        && nodes
          .iter()
          .find(|declared| declared.key() == node)
          .and_then(|declared| declared.outcome(outcome))
          .is_none()
      {
        return Err(invalid());
      }
    }
    Ok(())
  }
  /// Validates explicit source selections before immutable definition publication.
  pub fn new(sources: BTreeMap<FactoryKey, FlowInputSource>, mapping: FlowDataMapping) -> Result<Self, FactoryError> {
    if sources.len() > MAX_FLOW_SCHEMA_ENTRIES {
      return Err(invalid());
    }
    for source in sources.values() {
      match source {
        FlowInputSource::Work { path }
        | FlowInputSource::Predecessor { path, .. }
        | FlowInputSource::Record { path, .. }
        | FlowInputSource::Incoming { path, .. }
        | FlowInputSource::CallerInput { path } => validate_pointer(path)?,
      }
    }
    let binding = Self { sources, mapping };
    if serde_json::to_vec(&binding).map_err(|_| invalid())?.len() > MAX_FLOW_INPUT_BINDING_BYTES {
      return Err(invalid());
    }
    Ok(binding)
  }
  pub(super) fn project(&self, context: &FlowInputPreparation<'_>) -> Result<ProjectedInput, FactoryError> {
    let work = serde_json::to_value(context.work).map_err(|_| invalid())?;
    let mut remaining = MAX_FLOW_DATA_BYTES;
    let mut references = BTreeMap::new();
    let mut incoming_digest = None;
    let mut caller_digest = None;
    let sources = self
      .sources
      .iter()
      .map(|(alias, source)| {
        let value = match source {
          FlowInputSource::CallerInput { path } => {
            let parent = context.flow.parent().ok_or_else(invalid)?;
            let attempt = context
              .history
              .attempts
              .iter()
              .find(|row| row.id() == parent.node_attempt_id() && row.flow_run_id() == parent.flow_run_id())
              .ok_or_else(invalid)?;
            let input = context
              .inputs
              .iter()
              .find(|row| row.digest().ok() == Some(attempt.input_digest()))
              .ok_or_else(invalid)?;
            input.verify_work(context.work, context.admitted)?;
            caller_digest = Some(attempt.input_digest());
            input.payload().value().pointer(path).ok_or_else(invalid)?
          }
          FlowInputSource::Predecessor { view, path } => {
            let (node, outcome) = predecessor(context)?;
            let record = select_record(context, node, outcome)?;
            let reference = FlowRecordReference::from_record(record)?;
            references.insert(reference.node_attempt_id(), reference);
            let selected = match view {
              FlowRecordView::Payload => copy_bounded(
                record.payload().value().pointer(path).ok_or_else(invalid)?,
                &mut remaining,
              )?,
              FlowRecordView::Metadata => {
                copy_bounded(record.metadata()?.pointer(path).ok_or_else(invalid)?, &mut remaining)?
              }
            };
            return Ok((alias.as_str().to_owned(), selected));
          }
          FlowInputSource::Incoming { schema, path } => {
            let incoming = context.incoming.ok_or_else(invalid)?;
            incoming.verify(context.work, context.admitted)?;
            if incoming.payload().schema() != schema {
              return Err(invalid());
            }
            incoming_digest = Some(incoming.digest()?);
            incoming.payload().value().pointer(path).ok_or_else(invalid)?
          }
          FlowInputSource::Work { path } => work.pointer(path).ok_or_else(invalid)?,
          FlowInputSource::Record { node, outcome, path } => {
            let record = select_record(context, node, outcome)?;
            let reference = FlowRecordReference::from_record(record)?;
            references.insert(reference.node_attempt_id(), reference);
            record.payload().value().pointer(path).ok_or_else(invalid)?
          }
        };
        Ok((alias.as_str().to_owned(), copy_bounded(value, &mut remaining)?))
      })
      .collect::<Result<serde_json::Map<_, _>, _>>()?;
    Ok(ProjectedInput {
      payload: self
        .mapping
        .project_value(&serde_json::Value::Object(sources), context.schema)?,
      sources: references.into_values().collect(),
      incoming_digest,
      caller_input_digest: caller_digest,
    })
  }
}
fn predecessor<'a>(context: &'a FlowInputPreparation<'_>) -> Result<(&'a FactoryKey, &'a FactoryKey), FactoryError> {
  let definition = context
    .admitted
    .closure()
    .definition(context.flow.definition())
    .ok_or_else(invalid)?;
  let incoming = definition
    .transitions()
    .iter()
    .filter(|edge| matches!(edge.target(),crate::FlowTransitionTarget::Node(key) if key==&context.node))
    .collect::<Vec<_>>();
  let attempt = context
    .history
    .attempts
    .iter()
    .filter(|attempt| {
      attempt.flow_run_id() == context.flow.id()
        && attempt.workflow_cycle_id() == context.cycle.id()
        && incoming.iter().any(|edge| edge.predecessor() == attempt.node_key())
    })
    .max_by_key(|attempt| attempt.number())
    .ok_or_else(invalid)?;
  let completion = context
    .history
    .completions
    .iter()
    .find(|row| row.node_attempt_id() == attempt.id())
    .ok_or_else(invalid)?;
  if !incoming
    .iter()
    .any(|edge| edge.predecessor() == attempt.node_key() && edge.outcome() == completion.outcome())
  {
    return Err(invalid());
  }
  Ok((attempt.node_key(), completion.outcome()))
}
fn select_record<'a>(
  context: &'a FlowInputPreparation<'_>,
  node: &FactoryKey,
  outcome: &FactoryKey,
) -> Result<&'a FlowNodeRecord, FactoryError> {
  let attempt = context
    .history
    .attempts
    .iter()
    .filter(|attempt| {
      attempt.flow_run_id() == context.flow.id()
        && attempt.workflow_cycle_id() == context.cycle.id()
        && attempt.node_key() == node
    })
    .max_by_key(|attempt| attempt.number())
    .ok_or_else(invalid)?;
  let completion = context
    .history
    .completions
    .iter()
    .find(|completion| completion.node_attempt_id() == attempt.id())
    .ok_or_else(invalid)?;
  if completion.outcome() != outcome {
    return Err(invalid());
  }
  let mut records = context
    .records
    .iter()
    .filter(|record| record.node_attempt_id() == attempt.id());
  let record = records.next().ok_or_else(invalid)?;
  if records.next().is_some() {
    return Err(invalid());
  }
  record.input().verify_work(context.work, context.admitted)?;
  let definition = context
    .admitted
    .closure()
    .definition(context.flow.definition())
    .ok_or_else(invalid)?;
  let expected = record.completion(
    attempt,
    definition,
    FactoryClaimOwnership::new(completion.owner().clone(), completion.claim()),
    completion.usage(),
    completion.observed_at(),
  )?;
  if &expected != completion {
    return Err(invalid());
  }
  Ok(record)
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow input binding",
  }
}
