use crate::{
  AdmittedFlow, FactoryError, FlowIncomingData, FlowNodeInput, FlowNodeRecord, FlowRuntimeHistory, WorkEnvelope,
  validate_flow_runtime_history,
};
use std::collections::BTreeMap;

/// Common retained data for any configured Flow, independent of logical phase names.
///
/// Stores append these rows under the same fenced transaction as runtime history.
/// Inputs precede execution; a completed node must retain exactly one result whose
/// complete content digest is the one committed by its completion.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FlowDataHistory {
  /// Immutable externally supplied data; at most one envelope per Factory Run.
  pub incoming: Vec<FlowIncomingData>,
  /// Frozen projected inputs, deduplicated by their full content identity.
  pub inputs: Vec<FlowNodeInput>,
  /// Verified results, one per completed Node Attempt.
  pub records: Vec<FlowNodeRecord>,
  /// Frozen ordinary Build requests, retained before any external dispatch.
  pub build_intents: Vec<crate::FlowBuildIntent>,
  /// Actual ordinary execution identities, including pending Builds and retries.
  pub build_executions: Vec<crate::FlowBuildExecution>,
}
impl FlowDataHistory {
  /// Returns the exact terminal child result through its parent's declared outcome.
  /// A running child or an unconsumed successor cannot complete the calling node.
  pub fn subflow_result(
    &self,
    admitted: &AdmittedFlow,
    history: FlowRuntimeHistory<'_>,
    call: &crate::NodeAttempt,
  ) -> Result<Option<FlowNodeRecord>, FactoryError> {
    let child = history
      .flow_runs
      .iter()
      .find(|row| row.parent() == Some(crate::FlowRunParent::new(call.flow_run_id(), call.id())))
      .ok_or_else(invalid)?;
    let cycle = history
      .cycles
      .iter()
      .filter(|row| row.flow_run_id() == child.id())
      .max_by_key(|row| row.number())
      .ok_or_else(invalid)?;
    let validated = admitted.validated()?;
    let crate::FlowProgress::Terminal { key, source } =
      crate::FlowInterpreter::new(&validated).progress(child, cycle, history)?
    else {
      return Ok(None);
    };
    let result = self
      .records
      .iter()
      .find(|row| row.node_attempt_id() == source)
      .ok_or_else(invalid)?;
    let input = self
      .inputs
      .iter()
      .find(|row| row.digest().ok() == Some(call.input_digest()))
      .ok_or_else(invalid)?;
    let definition = admitted.closure().definition(input.definition()).ok_or_else(invalid)?;
    Ok(Some(FlowNodeRecord::for_subflow(
      input, call, definition, child, key, result,
    )?))
  }
  /// Revalidates exact Work, data contracts, source provenance and runtime bindings.
  pub fn validate(
    &self,
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<(), FactoryError> {
    validate_flow_runtime_history(admitted, history)?;
    if self.incoming.len() > 1 {
      return Err(invalid());
    }
    for incoming in &self.incoming {
      incoming.verify(work, admitted)?;
      if admitted.data_schema(incoming.payload().schema()).is_none() {
        return Err(invalid());
      }
    }
    let mut inputs = BTreeMap::new();
    for input in &self.inputs {
      if input.generation() as usize
        > history
          .attempts
          .iter()
          .filter(|row| {
            row.flow_run_id() == input.flow_run_id()
              && row.workflow_cycle_id() == input.cycle_id()
              && row.node_key() == input.node()
          })
          .count()
      {
        return Err(invalid());
      }
      input.verify_work(work, admitted)?;
      let flow = history
        .flow_runs
        .iter()
        .find(|flow| flow.id() == input.flow_run_id())
        .ok_or_else(invalid)?;
      let cycle = history
        .cycles
        .iter()
        .find(|cycle| cycle.id() == input.cycle_id())
        .ok_or_else(invalid)?;
      let schema = admitted.data_schema(input.payload().schema()).ok_or_else(invalid)?;
      let digest = input.digest()?;
      FlowNodeInput::restore(
        &serde_json::to_vec(input).map_err(|_| invalid())?,
        work,
        admitted,
        flow,
        cycle,
        schema,
        digest,
      )?;
      if inputs.insert(digest, input).is_some()
        || input.incoming_digest().is_some_and(|digest| {
          !self
            .incoming
            .iter()
            .any(|incoming| incoming.digest().ok() == Some(digest))
        })
      {
        return Err(invalid());
      }
      if admitted
        .closure()
        .definition(input.definition())
        .and_then(|definition| definition.node(input.node()))
        .and_then(crate::FlowNodeDefinition::input_binding)
        .is_some()
      {
        // Restore the immutable source snapshot, rather than interpreting old
        // inputs against results appended after they were frozen. New inputs
        // are separately checked against the current full history on append.
        let source_ceiling = history
          .attempts
          .iter()
          .filter(|attempt| {
            input
              .sources()
              .iter()
              .any(|source| source.node_attempt_id() == attempt.id())
          })
          .map(|attempt| attempt.number())
          .max();
        let attempts = history
          .attempts
          .iter()
          .filter(|attempt| {
            source_ceiling.is_none_or(|ceiling| {
              attempt.flow_run_id() != flow.id()
                || attempt.workflow_cycle_id() != cycle.id()
                || attempt.number() <= ceiling
            })
          })
          .cloned()
          .collect::<Vec<_>>();
        input.verify_preparation(crate::FlowInputPreparation {
          work,
          admitted,
          flow,
          cycle,
          node: input.node().clone(),
          schema,
          history: FlowRuntimeHistory {
            attempts: &attempts,
            ..history
          },
          records: &self.records,
          inputs: &self.inputs,
          incoming: self
            .incoming
            .iter()
            .find(|incoming| incoming.digest().ok() == input.incoming_digest()),
        })?;
      }
    }
    let mut intents = BTreeMap::new();
    for intent in &self.build_intents {
      let attempt = history
        .attempts
        .iter()
        .find(|attempt| attempt.id() == intent.node().id())
        .ok_or_else(invalid)?;
      let input = inputs.get(&attempt.input_digest()).ok_or_else(invalid)?;
      if intents.insert(attempt.id(), intent).is_some()
        || crate::FlowBuildIntent::restore(
          &serde_json::to_vec(intent).map_err(|_| invalid())?,
          input,
          admitted,
          attempt,
          intent.operation_id()?,
        )? != *intent
      {
        return Err(invalid());
      }
    }
    let mut executions = BTreeMap::new();
    let mut builds = BTreeMap::new();
    for execution in &self.build_executions {
      let intent = intents.get(&execution.node_attempt_id()).ok_or_else(invalid)?;
      if executions
        .insert((execution.node_attempt_id(), execution.attempt_id()), execution)
        .is_some()
        || builds
          .insert(execution.node_attempt_id(), execution.build_id())
          .is_some_and(|build| build != execution.build_id())
        || crate::FlowBuildExecution::restore(
          &serde_json::to_vec(execution).map_err(|_| invalid())?,
          intent,
          execution.digest()?,
        )? != *execution
      {
        return Err(invalid());
      }
    }
    let mut records = BTreeMap::new();
    for record in &self.records {
      if records.insert(record.node_attempt_id(), record).is_some() {
        return Err(invalid());
      }
      let attempt = history
        .attempts
        .iter()
        .find(|attempt| attempt.id() == record.node_attempt_id())
        .ok_or_else(invalid)?;
      let completion = history
        .completions
        .iter()
        .find(|completion| completion.node_attempt_id() == attempt.id())
        .ok_or_else(invalid)?;
      let input = inputs.get(&attempt.input_digest()).ok_or_else(invalid)?;
      if record.input() != *input {
        return Err(invalid());
      }
      let definition = admitted.closure().definition(input.definition()).ok_or_else(invalid)?;
      let schema = admitted.data_schema(record.payload().schema()).ok_or_else(invalid)?;
      let restored = FlowNodeRecord::restore(
        &serde_json::to_vec(record).map_err(|_| invalid())?,
        input,
        attempt,
        definition,
        schema,
        completion,
      )?;
      if &restored != record {
        return Err(invalid());
      }
      if let Some(provenance) = record.build_provenance() {
        for output in provenance.outputs() {
          let execution = executions
            .get(&(record.node_attempt_id(), output.producer.attempt_id()))
            .ok_or_else(invalid)?;
          if execution.build_id() != output.producer.build_id() || !execution.jobs().contains(&output.producer.job_id())
          {
            return Err(invalid());
          }
        }
      }
      let node = definition.node(input.node()).ok_or_else(invalid)?;
      if node.kind() == crate::FlowNodeKind::SubflowCall {
        if self.subflow_result(admitted, history, attempt)?.as_ref() != Some(record) {
          return Err(invalid());
        }
      } else if record.subflow_source().is_some() {
        return Err(invalid());
      }
      if let Some(program) = super::predicate::configured_program(node)?
        && &program.record(input, attempt, definition, schema, record.observed_at())? != record
      {
        return Err(invalid());
      }
    }
    for attempt in history
      .attempts
      .iter()
      .filter(|attempt| attempt.stage_projection_id().is_none())
    {
      let input = inputs.get(&attempt.input_digest()).ok_or_else(invalid)?;
      if input.flow_run_id() != attempt.flow_run_id()
        || input.cycle_id() != attempt.workflow_cycle_id()
        || input.node() != attempt.node_key()
        || input.generation() as usize
          != history
            .attempts
            .iter()
            .filter(|row| {
              row.flow_run_id() == attempt.flow_run_id()
                && row.workflow_cycle_id() == attempt.workflow_cycle_id()
                && row.node_key() == attempt.node_key()
                && row.number() < attempt.number()
            })
            .count()
      {
        return Err(invalid());
      }
      if matches!(
        attempt.node_kind(),
        crate::FlowNodeKind::BuildCommand | crate::FlowNodeKind::Reasoning
      ) && !intents.contains_key(&attempt.id())
      {
        return Err(invalid());
      }
    }
    for completion in history.completions.iter().filter(|completion| {
      history
        .attempts
        .iter()
        .any(|attempt| attempt.id() == completion.node_attempt_id() && attempt.stage_projection_id().is_none())
    }) {
      if !records.contains_key(&completion.node_attempt_id()) {
        return Err(invalid());
      }
    }
    for input in &self.inputs {
      for source in input.sources() {
        let record = records.get(&source.node_attempt_id()).ok_or_else(invalid)?;
        if record.digest()? != source.digest()
          || record.input().flow_run_id() != input.flow_run_id()
          || record.input().cycle_id() != input.cycle_id()
        {
          return Err(invalid());
        }
      }
    }
    for record in &self.records {
      let node = admitted
        .closure()
        .definition(record.input().definition())
        .and_then(|definition| definition.node(record.input().node()))
        .ok_or_else(invalid)?;
      let completion = history
        .completions
        .iter()
        .find(|completion| completion.node_attempt_id() == record.node_attempt_id())
        .ok_or_else(invalid)?;
      if node.accepted_work_outcomes().contains(completion.outcome()) {
        crate::AcceptedWorkContract::freeze_validated(work, admitted, self, history, record.node_attempt_id())?;
      }
    }
    Ok(())
  }
  /// Counts immutable rows for transition and snapshot ceilings.
  #[must_use]
  pub fn record_count(&self) -> usize {
    self.incoming.len()
      + self.inputs.len()
      + self.records.len()
      + self.build_intents.len()
      + self.build_executions.len()
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow data history",
  }
}
