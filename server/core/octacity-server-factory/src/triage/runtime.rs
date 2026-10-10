use super::{TriageNode, TriageProvenance, TriageSchema, invalid};
use crate::{
  AdmittedFlow, FactoryDigest, FactoryError, FlowDirective, FlowInterpreter, FlowRun, FlowRuntimeHistory,
  NodeExecutionIdentity,
};

/// Verifies a triage producer independently from the exact completed phase export.
///
/// The supplied runtime history must also pass `validate_flow_runtime_history`.
/// A producer may precede deterministic gates or run inside nested child Flows;
/// every enclosing Flow must export the same result under its current cycle.
pub fn validate_triage_phase_observation(
  admitted: &AdmittedFlow,
  phase: &FlowRun,
  provenance: &TriageProvenance,
  digest: FactoryDigest,
  history: FlowRuntimeHistory<'_>,
) -> Result<(), FactoryError> {
  let settings = admitted.triage().ok_or_else(|| invalid("triage admission"))?;
  let parent = phase.parent().ok_or_else(|| invalid("triage phase parent"))?;
  let call = history
    .attempts
    .iter()
    .find(|row| row.id() == parent.node_attempt_id())
    .ok_or_else(|| invalid("triage phase call"))?;
  let composition = history
    .flow_runs
    .iter()
    .find(|row| row.id() == parent.flow_run_id())
    .ok_or_else(|| invalid("triage composition Run"))?;
  let (profile, schema) = if call.node_key() == &TriageNode::Eligibility.key() {
    (&settings.eligibility, TriageSchema::EligibilityResult)
  } else if call.node_key() == &TriageNode::Classification.key() {
    (&settings.classification, TriageSchema::TriageResult)
  } else {
    return Err(invalid("triage phase role"));
  };
  if composition.definition() != settings.definition || !profile.matches(provenance) {
    return Err(invalid("triage phase execution profile"));
  }
  let schema = schema.reference()?;
  let producer = history
    .attempts
    .iter()
    .find(|row| row.id() == provenance.node_attempt_id)
    .ok_or_else(|| invalid("triage producing attempt"))?;
  let mut flow = history
    .flow_runs
    .iter()
    .find(|row| row.id() == producer.flow_run_id())
    .ok_or_else(|| invalid("triage producer Flow"))?;
  if flow.definition() != provenance.definition
    || producer.input_digest() != provenance.input_digest
    || producer.execution() != &NodeExecutionIdentity::External(provenance.producer.clone())
    || !history.completions.iter().any(|row| {
      row.node_attempt_id() == producer.id() && row.output_digest() == digest && row.output_schema() == &schema
    })
  {
    return Err(invalid("triage producer binding"));
  }
  let validated = admitted.validated()?;
  let interpreter = FlowInterpreter::new(&validated);
  let mut producing_attempt = producer;
  for _ in 0..history.flow_runs.len() {
    let cycle = history
      .cycles
      .iter()
      .filter(|row| row.flow_run_id() == flow.id())
      .max_by_key(|row| row.number())
      .ok_or_else(|| invalid("triage current cycle"))?;
    if producing_attempt.workflow_cycle_id() != cycle.id() {
      return Err(invalid("triage superseded observation"));
    }
    let exported = history
      .completions
      .iter()
      .filter(|row| {
        row.workflow_cycle_id() == cycle.id() && row.output_digest() == digest && row.output_schema() == &schema
      })
      .any(|result| {
        history
          .attempts
          .iter()
          .find(|row| row.id() == result.node_attempt_id())
          .is_some_and(|attempt| {
            interpreter
              .advance(flow, cycle, attempt, result, history.attempts, history.completions)
              .is_ok_and(|directives| {
                directives.iter().any(|directive| {
                  matches!(directive,
              FlowDirective::Complete { terminal, .. } if flow.id() != phase.id() || terminal.as_str() == "observed")
                })
              })
          })
      });
    if !exported {
      return Err(invalid("triage incomplete phase export"));
    }
    if flow.id() == phase.id() {
      return Ok(());
    }
    let parent = flow.parent().ok_or_else(|| invalid("triage producer outside phase"))?;
    producing_attempt = history
      .attempts
      .iter()
      .find(|row| row.id() == parent.node_attempt_id())
      .ok_or_else(|| invalid("triage nested result call"))?;
    if !history
      .completions
      .iter()
      .any(|row| row.node_attempt_id() == producing_attempt.id() && row.output_digest() == digest)
    {
      return Err(invalid("triage nested result export"));
    }
    flow = history
      .flow_runs
      .iter()
      .find(|row| row.id() == parent.flow_run_id())
      .ok_or_else(|| invalid("triage nested result parent"))?;
  }
  Err(invalid("triage producer lineage"))
}
