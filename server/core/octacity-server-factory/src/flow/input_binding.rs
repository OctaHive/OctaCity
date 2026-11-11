use super::{mapping::copy_bounded, predicate::validate_pointer};
use crate::{
  FactoryClaimOwnership, FactoryError, FactoryKey, FlowDataMapping, FlowInputPreparation, FlowNodeRecord, FlowPayload,
  FlowRecordReference, MAX_FLOW_DATA_BYTES, MAX_FLOW_SCHEMA_ENTRIES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Hard encoded ceiling for one node's configured input selection.
pub const MAX_FLOW_INPUT_BINDING_BYTES: usize = 16 * 1024;

/// Explicit projection view of one accepted predecessor result.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowRecordView {
  /// Only the producing attempt's frozen input payload, explicitly selected.
  /// Neither its Context nor model-authored replacements are forwarded.
  InputPayload,
  /// Only the schema-validated output payload.
  #[default]
  Payload,
  /// Only owner metadata and independently verified output facts.
  Metadata,
  /// Only the independently verified primary Build output's retained artifact reference.
  PrimaryArtifact,
}

/// Explicit trusted source of projected input data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "source", deny_unknown_fields)]
pub enum FlowInputSource {
  /// Explicit field of this node's immutable admitted definition.
  /// Operator-owned settings are selected without copying other node data.
  NodeConfiguration {
    /// RFC 6901 selection in this exact published node. Fields under `/build/`
    /// can select a value shared by all declared profile candidates before one is chosen.
    path: String,
  },
  /// Explicit projection of the exact frozen input of this Flow's parent call.
  CallerInput {
    /// RFC 6901 selection in the calling node's payload; no implicit context transfer.
    path: String,
  },
  /// Latest producing attempt on an explicitly declared incoming control edge.
  /// The latest producer on a normal target must finish with the matching
  /// outcome. An exhaustion target only considers attempts that reached it.
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
    /// Explicit view; omitted declarations select the public payload.
    #[serde(default)]
    view: FlowRecordView,
    /// RFC 6901 selection in the producing result view.
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
  /// Qualifies this Incoming channel's paths, types and optional-presence guards.
  /// Other sources keep their existing contracts; no source or Context is widened.
  pub fn fits_incoming(
    &self,
    reference: &crate::ImmutableReference,
    incoming: &crate::FlowValueSchema,
    target: &crate::FlowDataSchema,
  ) -> bool {
    use super::mapping::{projection_segment, schema_pointer};
    use crate::FlowValueSchema;
    let mut known = BTreeMap::new();
    for (alias, source) in &self.sources {
      if let FlowInputSource::Incoming { schema, path } = source
        && schema == reference
      {
        let Some(shape) = schema_pointer(incoming, path, "", |_| false) else {
          return false;
        };
        known.insert(alias, shape);
      }
    }
    if known.is_empty() {
      return true;
    }
    self.mapping.fits_pointers(target.shape(), |path, target, present| {
      if path.is_empty() {
        let FlowValueSchema::Object { fields } = target else {
          return false;
        };
        return self.sources.keys().all(|alias| fields.contains_key(alias))
          && known
            .iter()
            .all(|(alias, shape)| fields.get(*alias).is_some_and(|field| field.value().covers(shape)))
          && fields
            .iter()
            .all(|(alias, field)| !field.is_required() || self.sources.contains_key(alias));
      }
      let Some((alias, tail)) = projection_segment(path) else {
        return false;
      };
      let Some((_, shape)) = known.iter().find(|(name, _)| name.as_str() == alias) else {
        return self.sources.keys().any(|name| name.as_str() == alias);
      };
      schema_pointer(shape, tail, &format!("/{alias}"), |optional| {
        present
          .iter()
          .any(|guard| *guard == optional || guard.starts_with(&format!("{optional}/")))
      })
      .is_some_and(|source| target.covers(source))
    })
  }

  /// Checks the configured sources of one projected input field without evaluating data.
  pub fn selects_only(&self, path: &str, mut check: impl FnMut(&FlowInputSource, &str) -> bool) -> bool {
    self.mapping.selects_only(path, &mut |pointer| {
      super::mapping::projection_segment(pointer).is_some_and(|(alias, tail)| {
        self
          .sources
          .iter()
          .find(|(name, _)| name.as_str() == alias)
          .is_some_and(|(_, source)| check(source, tail))
      })
    })
  }

  /// The exact external schemas required to prepare this node's input.
  pub fn incoming_schemas(&self) -> impl Iterator<Item = &crate::ImmutableReference> {
    self.sources.values().filter_map(|source| match source {
      FlowInputSource::Incoming { schema, .. } => Some(schema),
      _ => None,
    })
  }

  /// Returns the mapping over a single exact accepted output payload.
  pub fn record_payload_mapping(
    &self,
    node: &FactoryKey,
    outcome: &FactoryKey,
  ) -> Option<(&FactoryKey, &crate::FlowValueMapping)> {
    let (alias, source) = self.sources.iter().next()?;
    match source {
      FlowInputSource::Record {
        node: selected,
        outcome: selected_outcome,
        view: FlowRecordView::Payload,
        path,
      } if self.sources.len() == 1 && selected == node && selected_outcome == outcome && path.is_empty() => {
        Some((alias, self.mapping.value()))
      }
      _ => None,
    }
  }

  pub(crate) fn selects_record(&self, node: &FactoryKey) -> bool {
    self
      .sources
      .values()
      .any(|source| matches!(source,FlowInputSource::Record{node:selected,..} if selected==node))
  }

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
        FlowInputSource::NodeConfiguration { path }
        | FlowInputSource::Work { path }
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
          FlowInputSource::NodeConfiguration { path } => {
            let node = context
              .admitted
              .closure()
              .definition(context.flow.definition())
              .and_then(|definition| definition.node(&context.node))
              .ok_or_else(invalid)?;
            return Ok((
              alias.as_str().to_owned(),
              node_configuration(node, path, &mut remaining)?,
            ));
          }
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
            let attempt = predecessor(context)?;
            let record = record_for_attempt(context, attempt)?;
            let reference = FlowRecordReference::from_record(record)?;
            references.insert(reference.node_attempt_id(), reference);
            let selected = project_record(record, *view, path, &mut remaining)?;
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
          FlowInputSource::Record {
            node,
            outcome,
            view,
            path,
          } => {
            let record = select_record(context, node, outcome)?;
            let reference = FlowRecordReference::from_record(record)?;
            references.insert(reference.node_attempt_id(), reference);
            let selected = project_record(record, *view, path, &mut remaining)?;
            return Ok((alias.as_str().to_owned(), selected));
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

fn node_configuration(
  node: &crate::FlowNodeDefinition,
  path: &str,
  remaining: &mut usize,
) -> Result<Value, FactoryError> {
  if node.build().is_none()
    && let Some(build_path) = path.strip_prefix("/build")
    && build_path.starts_with('/')
  {
    // Input preparation precedes profile selection. Read only the field common to
    // every immutable candidate, never an arbitrary candidate or live profile.
    let mut common = None;
    for build in node.build_bindings() {
      let value = serde_json::to_value(build).map_err(|_| invalid())?;
      let field = value.pointer(build_path).ok_or_else(invalid)?;
      match &common {
        None => common = Some(copy_bounded(field, remaining)?),
        Some(previous) if previous == field => (),
        Some(_) => return Err(invalid()),
      }
    }
    return common.ok_or_else(invalid);
  }
  let configuration = serde_json::to_value(node).map_err(|_| invalid())?;
  copy_bounded(configuration.pointer(path).ok_or_else(invalid)?, remaining)
}

fn predecessor<'a>(context: &'a FlowInputPreparation<'_>) -> Result<&'a crate::NodeAttempt, FactoryError> {
  let definition = context
    .admitted
    .closure()
    .definition(context.flow.definition())
    .ok_or_else(invalid)?;
  let incoming = definition
    .transitions()
    .iter()
    .filter(|edge| edge.node_targets().any(|key| key == &context.node))
    .collect::<Vec<_>>();
  let numbers = incoming.iter().any(|edge| edge.max_repeats() > 0).then(|| {
    context
      .history
      .attempts
      .iter()
      .map(|attempt| (attempt.id(), attempt.number()))
      .collect::<BTreeMap<_, _>>()
  });
  let completion_for = |id| {
    context
      .history
      .completions
      .iter()
      .find(|row| row.node_attempt_id() == id)
  };
  let routes_here =
    |edge: &crate::FlowTransition, attempt: &crate::NodeAttempt, completion: &crate::NodeAttemptCompletion| {
      if edge.predecessor() != attempt.node_key() || edge.outcome() != completion.outcome() {
        return false;
      }
      let traversals = if edge.max_repeats() > 0 {
        super::history::traversals_through(context.history.completions, completion, attempt, |id| {
          numbers.as_ref()?.get(&id).copied()
        })
      } else {
        0
      };
      edge.selected_node(traversals) == Some(&context.node)
    };
  let attempt = context
    .history
    .attempts
    .iter()
    .filter(|attempt| {
      attempt.flow_run_id() == context.flow.id()
        && attempt.workflow_cycle_id() == context.cycle.id()
        && incoming.iter().any(|edge| {
          !edge.starts_cycle()
            && edge.predecessor() == attempt.node_key()
            && (edge.exhausted_node() != Some(&context.node)
              || completion_for(attempt.id()).is_some_and(|completion| routes_here(edge, attempt, completion)))
        })
    })
    .max_by_key(|attempt| attempt.number());
  let attempt = match attempt {
    Some(attempt) => attempt,
    None => {
      // Only the declared entry can consume the exact boundary trigger. Ordinary
      // Record selections remain restricted to the containing cycle.
      let validated = context.admitted.validated()?;
      if crate::FlowInterpreter::new(&validated).cycle_entry(context.flow, context.cycle, context.history)?
        != context.node
      {
        return Err(invalid());
      }
      context
        .history
        .attempts
        .iter()
        .find(|row| Some(row.id()) == context.cycle.source_attempt_id())
        .ok_or_else(invalid)?
    }
  };
  let completion = completion_for(attempt.id()).ok_or_else(invalid)?;
  if !incoming.iter().any(|edge| routes_here(edge, attempt, completion)) {
    return Err(invalid());
  }
  Ok(attempt)
}
pub(super) fn select_record<'a>(
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
  record_for_attempt(context, attempt)
}
fn record_for_attempt<'a>(
  context: &'a FlowInputPreparation<'_>,
  attempt: &crate::NodeAttempt,
) -> Result<&'a FlowNodeRecord, FactoryError> {
  let completion = context
    .history
    .completions
    .iter()
    .find(|row| row.node_attempt_id() == attempt.id())
    .ok_or_else(invalid)?;
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
fn project_record(
  record: &FlowNodeRecord,
  view: FlowRecordView,
  path: &str,
  remaining: &mut usize,
) -> Result<serde_json::Value, FactoryError> {
  match view {
    FlowRecordView::InputPayload => copy_bounded(
      record.input().payload().value().pointer(path).ok_or_else(invalid)?,
      remaining,
    ),
    FlowRecordView::Payload => copy_bounded(record.payload().value().pointer(path).ok_or_else(invalid)?, remaining),
    FlowRecordView::Metadata => copy_bounded(record.metadata()?.pointer(path).ok_or_else(invalid)?, remaining),
    FlowRecordView::PrimaryArtifact => {
      let output = record.primary_build_output().ok_or_else(invalid)?;
      let reference = serde_json::to_value(&output.artifact).map_err(|_| invalid())?;
      copy_bounded(reference.pointer(path).ok_or_else(invalid)?, remaining)
    }
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow input binding",
  }
}
