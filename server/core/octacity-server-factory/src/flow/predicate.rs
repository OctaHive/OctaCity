use crate::{
  FactoryDigest, FactoryError, FactoryKey, FlowDataMapping, FlowDataSchema, FlowDefinition, FlowGateBinding,
  FlowNodeDefinition, FlowNodeInput, FlowNodeKind, FlowNodeRecord, FlowPayload, FlowRecordObservation,
  ImmutableReference, MAX_FLOW_GATE_PARAMETER_BYTES, NodeAttempt, NodeExecutionIdentity,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const MAX_RULES: usize = 64;
const MAX_DEPTH: usize = 16;
const MAX_PREDICATES: usize = 128;
const MAX_POINTER_BYTES: usize = 256;
// Hard CPU ceiling across every rule, including nested rescans of root arrays.
const MAX_EVALUATION_STEPS: usize = 16_384;
const POLICY: &str = "factory.declarative-gate";

/// Bounded array or boolean collection aggregation; missing facts stay unknown.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowQuantifier {
  /// At least one known true observation, with no unknown operands.
  Any,
  /// Every observation is known true; empty collections remain unknown.
  Every,
}
impl FlowQuantifier {
  fn aggregate(self, values: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut seen = false;
    let mut result = self == Self::Every;
    for value in values {
      let value = value?;
      seen = true;
      match self {
        Self::Any => result |= value,
        Self::Every => result &= value,
      }
    }
    seen.then_some(result)
  }
}

/// One typed comparison, without string/number coercion.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowComparison {
  /// Exact scalar equality.
  Equal,
  /// Signed integer ordering.
  Greater,
  /// Signed integer ordering including equality.
  LessOrEqual,
}

/// An observation in the projected input or a frozen configuration literal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "source", deny_unknown_fields)]
pub enum FlowOperand {
  /// Authoritative owner observation time in Unix milliseconds; unavailable provider input cannot supply it.
  ObservationTime,
  /// RFC 6901 pointer into the schema-validated input.
  Path {
    /// Pointer into the root projected document.
    path: String,
  },
  /// RFC 6901 pointer into the item of the enclosing array quantifier.
  Item {
    /// Pointer relative to the current item, without parent access.
    path: String,
  },
  /// Operator-owned scalar constant.
  Literal {
    /// Exact configured value.
    value: Value,
  },
}

/// Bounded declarative condition; it cannot execute code or grant permissions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "operation", deny_unknown_fields)]
pub enum FlowPredicate {
  /// Typed comparison between two operands.
  Compare {
    /// Left observation or constant.
    left: FlowOperand,
    /// Operation without coercion.
    comparison: FlowComparison,
    /// Right observation or constant.
    right: FlowOperand,
  },
  /// Requires every configured child condition.
  All {
    /// Non-empty ordered child conditions.
    predicates: Vec<FlowPredicate>,
  },
  /// Requires at least one true child, with no missing operands.
  Any {
    /// Non-empty ordered child conditions.
    predicates: Vec<FlowPredicate>,
  },
  /// Aggregates one configured condition over a bounded projected array.
  ForEach {
    /// Array observation in the projected document or enclosing item.
    collection: FlowOperand,
    /// Finite aggregation rule.
    quantifier: FlowQuantifier,
    /// Condition with access to each item.
    predicate: Box<FlowPredicate>,
  },
  /// Negates a known condition; missing or mistyped operands remain unknown.
  Not {
    /// Condition whose known result is inverted.
    predicate: Box<FlowPredicate>,
  },
  /// Confirms an operand is present; absence remains unknown under negation.
  Exists {
    /// Projected observation or configured constant.
    operand: FlowOperand,
  },
}

/// Ordered condition and one finite outcome declared by the containing node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowGateRule {
  /// Condition evaluated on the explicitly projected input.
  pub when: FlowPredicate,
  /// Declared outcome selected when the condition holds.
  pub outcome: FactoryKey,
}

/// Frozen finite rules evaluated in order, with an explicit fallback.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ProgramWire")]
pub struct FlowGateProgram {
  rules: Vec<FlowGateRule>,
  fallback: FactoryKey,
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  outputs: BTreeMap<FactoryKey, FlowDataMapping>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  default_output: Option<FlowDataMapping>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramWire {
  rules: Vec<FlowGateRule>,
  fallback: FactoryKey,
  #[serde(default)]
  outputs: BTreeMap<FactoryKey, FlowDataMapping>,
  #[serde(default)]
  default_output: Option<FlowDataMapping>,
}
impl TryFrom<ProgramWire> for FlowGateProgram {
  type Error = FactoryError;
  fn try_from(wire: ProgramWire) -> Result<Self, Self::Error> {
    let mut program = Self::new(wire.rules, wire.fallback)?;
    if let Some(mapping) = wire.default_output {
      program = program.with_default_output_mapping(mapping)?;
    }
    for (outcome, mapping) in wire.outputs {
      program = program.with_output_mapping(outcome, mapping)?;
    }
    Ok(program)
  }
}
impl FlowGateProgram {
  /// Shares one bounded mapping across declared outcomes without an explicit override.
  /// Output shape is still checked against the selected outcome's exact schema.
  pub fn with_default_output_mapping(mut self, mapping: FlowDataMapping) -> Result<Self, FactoryError> {
    self.default_output = Some(mapping);
    self.validate_bytes()?;
    Ok(self)
  }
  /// Freezes explicit output fields for one rule or fallback outcome.
  pub fn with_output_mapping(mut self, outcome: FactoryKey, mapping: FlowDataMapping) -> Result<Self, FactoryError> {
    if outcome != self.fallback && !self.rules.iter().any(|rule| rule.outcome == outcome) {
      return Err(invalid());
    }
    self.outputs.insert(outcome, mapping);
    self.validate_bytes()?;
    Ok(self)
  }
  /// Executes the exact declared program and mapping into a common immutable record.
  ///
  /// The caller commits the returned record with its ordinary fenced completion;
  /// this pure operation grants no readiness and performs no external action.
  pub fn record(
    &self,
    input: &FlowNodeInput,
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    schema: &FlowDataSchema,
    at: Timestamp,
  ) -> Result<FlowNodeRecord, FactoryError> {
    let node = definition.node(input.node()).ok_or_else(invalid)?;
    if &Self::from_node(node)? != self {
      return Err(invalid());
    }
    let outcome = self.select_at(input.payload(), at)?;
    if node
      .outcome(&outcome)
      .is_none_or(|declared| declared.schema() != schema.reference())
    {
      return Err(invalid());
    }
    let payload = self
      .outputs
      .get(&outcome)
      .or(self.default_output.as_ref())
      .ok_or_else(invalid)?
      .project(input.payload(), schema)?;
    FlowNodeRecord::new(
      input,
      attempt,
      definition,
      FlowRecordObservation {
        outcome,
        payload,
        producer: NodeExecutionIdentity::BuiltIn,
        observed_at: at,
      },
    )
  }
  /// Binds the finite program to this exact built-in evaluator contract.
  pub fn binding(&self) -> Result<FlowGateBinding, FactoryError> {
    FlowGateBinding::new(policy_reference()?, serde_json::to_value(self).map_err(|_| invalid())?)
  }
  /// Validates the pinned evaluator and every declared rule/fallback outcome.
  pub fn from_node(node: &FlowNodeDefinition) -> Result<Self, FactoryError> {
    let binding = node.gate().ok_or_else(invalid)?;
    if node.kind() != FlowNodeKind::DeterministicGate || binding.policy() != &policy_reference()? {
      return Err(invalid());
    }
    let program: Self = serde_json::from_value(binding.parameters().clone()).map_err(|_| invalid())?;
    if node.outcome(&program.fallback).is_none()
      || program.rules.iter().any(|rule| node.outcome(&rule.outcome).is_none())
    {
      return Err(invalid());
    }
    Ok(program)
  }
  /// Validates and freezes rules; thresholds and logical names come from configuration.
  pub fn new(rules: Vec<FlowGateRule>, fallback: FactoryKey) -> Result<Self, FactoryError> {
    if rules.len() > MAX_RULES {
      return Err(invalid());
    }
    let mut count = 0;
    for rule in &rules {
      rule.when.validate(0, &mut count, false)?;
    }
    let program = Self {
      rules,
      fallback,
      outputs: BTreeMap::new(),
      default_output: None,
    };
    program.validate_bytes()?;
    Ok(program)
  }
  fn validate_bytes(&self) -> Result<(), FactoryError> {
    if serde_json::to_vec(self).map_err(|_| invalid())?.len() > MAX_FLOW_GATE_PARAMETER_BYTES {
      return Err(invalid());
    }
    Ok(())
  }
  /// Selects the first matching rule or the explicit fallback, without lifecycle authority.
  pub fn select(&self, input: &FlowPayload) -> Result<FactoryKey, FactoryError> {
    self.select_with_time(input, None)
  }
  /// Evaluates configured temporal rules against the authoritative owner observation time.
  pub fn select_at(&self, input: &FlowPayload, at: Timestamp) -> Result<FactoryKey, FactoryError> {
    self.select_with_time(input, Some(Value::from(at.unix_millis())))
  }
  fn select_with_time(&self, input: &FlowPayload, at: Option<Value>) -> Result<FactoryKey, FactoryError> {
    let mut remaining = MAX_EVALUATION_STEPS;
    for rule in &self.rules {
      let matched = rule.when.evaluate(input.value(), None, at.as_ref(), &mut remaining);
      if remaining == 0 {
        return Err(invalid());
      }
      if matched == Some(true) {
        return Ok(rule.outcome.clone());
      }
    }
    Ok(self.fallback.clone())
  }
}
pub(super) fn validate_binding(node: &FlowNodeDefinition) -> Result<(), FactoryError> {
  configured_program(node).map(|_| ())
}
pub(super) fn configured_program(node: &FlowNodeDefinition) -> Result<Option<FlowGateProgram>, FactoryError> {
  if node
    .gate()
    .is_some_and(|binding| binding.policy().identity().as_str() == POLICY)
  {
    return FlowGateProgram::from_node(node).map(Some);
  }
  Ok(None)
}
fn policy_reference() -> Result<ImmutableReference, FactoryError> {
  Ok(ImmutableReference::new(
    FactoryKey::new(POLICY)?,
    FactoryKey::new("v1")?,
    FactoryDigest::sha256(
      "octacity.factory.declarative-gate-contract.v1",
      &[b"ordered-rules;typed-scalars;missing-unknown;bounded-quantifiers;explicit-fallback;owner-observation-time;default-output-with-exact-overrides;16384-steps"],
    ),
  ))
}
impl FlowOperand {
  fn validate(&self, in_item: bool) -> Result<(), FactoryError> {
    if matches!(self, Self::Item { .. }) && !in_item {
      return Err(invalid());
    }
    match self {
      Self::Path { path } | Self::Item { path } => validate_pointer(path)?,
      Self::Literal { value }
        if value.is_object() || value.is_array() || value.is_number() && value.as_i64().is_none() =>
      {
        return Err(invalid());
      }
      Self::Literal { .. } | Self::ObservationTime => (),
    }
    Ok(())
  }
  fn resolve<'a>(&'a self, root: &'a Value, item: Option<&'a Value>, at: Option<&'a Value>) -> Option<&'a Value> {
    match self {
      Self::ObservationTime => at,
      Self::Path { path } => root.pointer(path),
      Self::Item { path } => item?.pointer(path),
      Self::Literal { value } => Some(value),
    }
  }
}
pub(super) fn validate_pointer(path: &str) -> Result<(), FactoryError> {
  if path.len() > MAX_POINTER_BYTES || (!path.is_empty() && !path.starts_with('/')) {
    return Err(invalid());
  }
  let mut bytes = path.bytes();
  while let Some(byte) = bytes.next() {
    if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
      return Err(invalid());
    }
  }
  Ok(())
}
impl FlowPredicate {
  fn validate(&self, depth: usize, count: &mut usize, in_item: bool) -> Result<(), FactoryError> {
    *count += 1;
    if depth > MAX_DEPTH || *count > MAX_PREDICATES {
      return Err(invalid());
    }
    match self {
      Self::All { predicates } | Self::Any { predicates } => {
        if predicates.is_empty() {
          return Err(invalid());
        }
        for predicate in predicates {
          predicate.validate(depth + 1, count, in_item)?;
        }
      }
      Self::ForEach {
        collection, predicate, ..
      } => {
        collection.validate(in_item)?;
        predicate.validate(depth + 1, count, true)?;
      }
      Self::Not { predicate } => predicate.validate(depth + 1, count, in_item)?,
      Self::Exists { operand } => operand.validate(in_item)?,
      Self::Compare { left, right, .. } => {
        left.validate(in_item)?;
        right.validate(in_item)?;
      }
    }
    Ok(())
  }
  fn evaluate(&self, root: &Value, item: Option<&Value>, at: Option<&Value>, remaining: &mut usize) -> Option<bool> {
    *remaining = remaining.checked_sub(1)?;
    match self {
      Self::All { predicates } => FlowQuantifier::Every.aggregate(
        predicates
          .iter()
          .map(|predicate| predicate.evaluate(root, item, at, remaining)),
      ),
      Self::Any { predicates } => FlowQuantifier::Any.aggregate(
        predicates
          .iter()
          .map(|predicate| predicate.evaluate(root, item, at, remaining)),
      ),
      Self::ForEach {
        collection,
        quantifier,
        predicate,
      } => {
        let values = collection.resolve(root, item, at)?.as_array()?;
        quantifier.aggregate(
          values
            .iter()
            .map(|value| predicate.evaluate(root, Some(value), at, remaining)),
        )
      }
      Self::Not { predicate } => predicate.evaluate(root, item, at, remaining).map(|known| !known),
      Self::Exists { operand } => operand.resolve(root, item, at).map(|_| true),
      Self::Compare {
        left,
        comparison,
        right,
      } => {
        let (left, right) = (left.resolve(root, item, at)?, right.resolve(root, item, at)?);
        match comparison {
          FlowComparison::Equal => {
            let same_type = matches!(
              (left, right),
              (Value::Null, Value::Null) | (Value::Bool(_), Value::Bool(_)) | (Value::String(_), Value::String(_))
            ) || (left.as_i64().is_some() && right.as_i64().is_some());
            same_type.then_some(left == right)
          }
          FlowComparison::Greater | FlowComparison::LessOrEqual => {
            let (left, right) = (left.as_i64()?, right.as_i64()?);
            Some(match comparison {
              FlowComparison::Greater => left > right,
              _ => left <= right,
            })
          }
        }
      }
    }
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow gate program",
  }
}
