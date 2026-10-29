//! Pure composition of finite result effects over the ordinary Flow/action owners.
//! The caller retains publication, current grants and execution limits; compilation
//! performs no provider I/O, credential access, mutation or automatic activation.
use crate::{ApplicationError, FactoryReporterRetryPolicy, FactoryStageSourceUpdate};
use octacity_server_factory::*;
use std::collections::{BTreeMap, BTreeSet};

/// Exact installed Source/Reporter selection and its advertised editing ceiling.
#[derive(Clone, Debug)]
pub struct FactoryReportingTarget {
  /// Original input scope; the admitted Work supplies the native item.
  pub source: WorkSourceBinding,
  /// Independently selected writer and mapping pins.
  pub reporter: WorkSourceBinding,
  /// Reviewed managed connection/access revision, without credential values.
  pub connection: WorkSourceConnectionRef,
  /// Installed literal labels/states; this metadata is not a writer grant.
  pub policy: WorkSourceReportingPolicy,
}

/// A scalar choice or a closed integer band, never a model-authored label.
#[derive(Clone, Debug)]
pub enum FactoryReportingSelection {
  /// One value from the selected finite result vocabulary.
  Literal {
    /// Exact schema-declared scalar.
    value: serde_json::Value,
  },
  /// Inclusive band in an integer result field.
  IntegerRange {
    /// Inclusive lower bound.
    minimum: i64,
    /// Inclusive upper bound.
    maximum: i64,
  },
}
/// Explicit effect or explicit preservation for one reviewed selector.
#[derive(Clone, Debug)]
pub struct FactoryReportingCase {
  /// Reviewed scalar/band selector.
  pub selection: FactoryReportingSelection,
  /// Literal installed effect; None deliberately preserves this dimension.
  pub effects: Option<WorkSourceStatusEffects>,
}
/// One independent classification dimension, compiled once for all selected outcomes.
#[derive(Clone, Debug)]
pub struct FactoryReportingDimension {
  /// Operator-owned stable dimension identity.
  pub key: FactoryKey,
  /// Object-field path in the retained result; encoded as a pointer only at compilation.
  pub path: Vec<FactoryKey>,
  /// Source outcomes allowed to report this dimension.
  pub outcomes: BTreeSet<FactoryKey>,
  /// Finite explicit choices; no Cartesian product with other dimensions.
  pub cases: Vec<FactoryReportingCase>,
}

/// Operator-reviewed finite effects compiled into an ordinary reusable subflow.
#[derive(Clone, Debug)]
pub struct FactoryResultReportingDraft {
  /// Exact target and installed vocabulary.
  pub target: FactoryReportingTarget,
  /// Every source outcome has an explicit effect selection.
  pub outcomes: BTreeMap<FactoryKey, Option<WorkSourceStatusEffects>>,
  /// Independently selected result fields and their finite effects.
  pub dimensions: Vec<FactoryReportingDimension>,
  /// Exact installed status-action receipt contract.
  pub receipt: FlowDataSchema,
  /// Exact installed status-action outcome.
  pub action_outcome: FactoryKey,
  /// Per-node technical ceiling.
  pub budget: BudgetLimit,
  /// Explicit child-flow budget and permissions, narrowed again at publication.
  pub execution: FlowExecutionPolicy,
  /// Existing ordered-outbox retry policy.
  pub retry: FactoryReporterRetryPolicy,
}

impl FactoryResultReportingDraft {
  /// Inserts exact outcome-bound calls to one shared ordinary reporting definition.
  /// Caller inputs select only the accepted result; private Context is not inherited.
  /// The returned draft still needs normal complete-closure publication/authorization.
  pub fn bind(
    &self,
    template: &FlowDefinitionTemplate,
    source: &FactoryKey,
    reporting: &PublishedFlowTemplate,
    prefix: &FactoryKey,
  ) -> Result<FlowDefinitionTemplate, ApplicationError> {
    let fail = ApplicationError::invalid;
    let selected = template
      .nodes
      .iter()
      .find(|node| &node.key == source)
      .ok_or_else(fail)?;
    let schemas = template.data_schemas().map_err(invalid)?;
    let result = schemas
      .get(&selected.outcomes.first().ok_or_else(fail)?.schema)
      .ok_or_else(fail)?;
    let outcomes = selected.outcomes.iter().map(|outcome| outcome.key.clone()).collect();
    let expected = self.compile(
      result,
      &outcomes,
      reporting.definition.reference().id(),
      reporting.definition.reference().version(),
    )?;
    if expected.definition.reference() != reporting.definition.reference() {
      return Err(fail());
    }
    let input = reporting
      .schemas
      .iter()
      .find(|schema| Some(schema.reference()) == reporting.definition.input_schema())
      .ok_or_else(fail)?;
    let input_key = key(&format!("{prefix}-input"))?;
    if template.schemas.contains_key(&input_key)
      || !self.execution.budget().fits_within(template.execution.budget())
      || !self
        .execution
        .permissions()
        .is_no_broader_than(template.execution.permissions())
    {
      return Err(fail());
    }
    let mut bound = template.clone();
    bound.schemas.insert(
      input_key.clone(),
      FlowTemplateSchema {
        identity: input.reference().identity().clone(),
        version: input.reference().version().clone(),
        shape: input.shape().clone(),
      },
    );
    bound.transitions.retain(|edge| edge.predecessor() != source);
    for (index, outcome) in selected.outcomes.iter().enumerate() {
      let mut edges = template
        .transitions
        .iter()
        .filter(|edge| edge.predecessor() == source && edge.outcome() == &outcome.key);
      let edge = edges.next().ok_or_else(fail)?;
      if edges.next().is_some() || edge.max_repeats() != 0 || edge.starts_cycle() {
        return Err(fail());
      }
      let call = key(&format!("{prefix}-{index}"))?;
      if bound.nodes.iter().any(|node| node.key == call) {
        return Err(fail());
      }
      let binding = FlowInputBinding::new(
        BTreeMap::from([(
          key("result")?,
          FlowInputSource::Record {
            node: source.clone(),
            outcome: outcome.key.clone(),
            view: FlowRecordView::Payload,
            path: String::new(),
          },
        )]),
        FlowDataMapping::new(FlowValueMapping::Object {
          fields: BTreeMap::from([
            (key("result")?, FlowValueMapping::Pointer { path: "/result".into() }),
            (
              key("outcome")?,
              FlowValueMapping::Constant {
                value: serde_json::json!(outcome.key),
              },
            ),
          ]),
        })
        .map_err(invalid)?,
      )
      .map_err(invalid)?;
      bound.nodes.push(FlowTemplateNode {
        key: call.clone(),
        kind: FlowNodeKind::SubflowCall,
        input_schema: input_key.clone(),
        outcomes: vec![FlowTemplateOutcome {
          key: key("reported")?,
          kind: FlowOutcomeKind::Success,
          schema: outcome.schema.clone(),
          public_result: false,
        }],
        budget: self.execution.budget(),
        max_execution_retries: 0,
        permissions: self.execution.permissions().clone(),
        required: true,
        input_binding: binding,
        context_binding: None,
        build_profile: None,
        profile_selection: None,
        capture_write_paths: None,
        capture_deliverables: None,
        gate: None,
        accepted_work_outcomes: vec![],
        action: None,
        pool: None,
        subflow: Some(reporting.definition.reference()),
      });
      bound.transitions.push(FlowTransition::new(
        source.clone(),
        outcome.key.clone(),
        FlowTransitionTarget::Node(call.clone()),
      ));
      bound
        .transitions
        .push(FlowTransition::new(call, key("reported")?, edge.target().clone()));
    }
    Ok(bound)
  }

  /// Compiles literal outcome effects; source results remain immutable caller data.
  /// Normal configuration publication still rechecks current Source/Reporter authority.
  pub fn compile(
    &self,
    result: &FlowDataSchema,
    outcomes: &BTreeSet<FactoryKey>,
    id: FlowDefinitionId,
    version: FlowDefinitionVersion,
  ) -> Result<PublishedFlowTemplate, ApplicationError> {
    let receipt = FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("receipt")?,
        FlowFieldSchema::required(FlowValueSchema::String {
          min_bytes: 64,
          max_bytes: 64,
        }),
      )]),
    };
    if !self.receipt.accepts_all(&receipt)
      || self.outcomes.len() > MAX_FLOW_SCHEMA_ENTRIES
      || self.dimensions.len() > MAX_FLOW_DEFINITION_NODES
    {
      return Err(ApplicationError::invalid());
    }
    if self.outcomes.keys().cloned().collect::<BTreeSet<_>>() != *outcomes {
      return Err(ApplicationError::invalid());
    }
    for dimension in &self.dimensions {
      dimension.validate(result, outcomes)?;
    }
    // Reject an excessive linear plan before allocating its nodes/edges.
    let actions = self.outcomes.values().filter(|effect| effect.is_some()).count()
      + self
        .dimensions
        .iter()
        .map(|dimension| dimension.cases.iter().filter(|case| case.effects.is_some()).count())
        .sum::<usize>();
    let cases = self.outcomes.len()
      + self
        .dimensions
        .iter()
        .map(|dimension| dimension.cases.len())
        .sum::<usize>();
    if 2 + self.dimensions.len() + actions > MAX_FLOW_DEFINITION_NODES
      || 2 + self.dimensions.len() + cases + actions > MAX_FLOW_DEFINITION_EDGES
    {
      return Err(ApplicationError::invalid());
    }
    self.validate_effect_ownership()?;
    if !self.budget.fits_within(self.execution.budget()) {
      return Err(ApplicationError::invalid());
    }
    let input = FlowDataSchema::new(
      key("factory.reporting.input")?,
      key("v1")?,
      FlowValueSchema::Object {
        fields: BTreeMap::from([
          (
            key("outcome")?,
            FlowFieldSchema::required(FlowValueSchema::Enum {
              values: outcomes.iter().map(|name| serde_json::json!(name)).collect(),
            }),
          ),
          (key("result")?, FlowFieldSchema::required(result.shape().clone())),
        ]),
      },
    )
    .map_err(invalid)?;
    let control =
      FlowDataSchema::new(key("factory.reporting.control")?, key("v1")?, FlowValueSchema::Boolean).map_err(invalid)?;
    let finish = key("finish")?;
    let continuation = self
      .dimensions
      .first()
      .map(|dimension| key(&format!("dimension-{}", dimension.key)))
      .transpose()?
      .unwrap_or_else(|| finish.clone());
    let mut nodes = vec![];
    let mut transitions = vec![];
    let mut rules = vec![];
    let select = key("select-outcome")?;
    for (index, (outcome, effects)) in self.outcomes.iter().enumerate() {
      let choice = key(&format!("choice-{index}"))?;
      let action_key = key(&format!("effect-{index}"))?;
      rules.push(FlowGateRule {
        outcome: choice.clone(),
        when: FlowPredicate::Compare {
          left: FlowOperand::Path {
            path: "/outcome".into(),
          },
          comparison: FlowComparison::Equal,
          right: FlowOperand::Literal {
            value: serde_json::json!(outcome),
          },
        },
      });
      let Some(effects) = effects else {
        transitions.push(FlowTransition::new(
          select.clone(),
          choice,
          FlowTransitionTarget::Node(continuation.clone()),
        ));
        continue;
      };
      nodes.push(self.effect_node(action_key.clone(), &input, effects)?);
      transitions.push(FlowTransition::new(
        select.clone(),
        choice,
        FlowTransitionTarget::Node(action_key.clone()),
      ));
      transitions.push(FlowTransition::new(
        action_key,
        self.action_outcome.clone(),
        FlowTransitionTarget::Node(continuation.clone()),
      ));
    }
    transitions.push(FlowTransition::new(
      select.clone(),
      key("skip")?,
      FlowTransitionTarget::Node(continuation.clone()),
    ));
    nodes.push(self.control_node(select.clone(), &input, &control, rules, None)?);
    for (index, dimension) in self.dimensions.iter().enumerate() {
      let name = key(&format!("dimension-{}", dimension.key))?;
      let next = self
        .dimensions
        .get(index + 1)
        .map(|d| key(&format!("dimension-{}", d.key)))
        .transpose()?
        .unwrap_or_else(|| finish.clone());
      let path = format!(
        "/result/{}",
        dimension
          .path
          .iter()
          .map(|p| p.as_str().replace('~', "~0").replace('/', "~1"))
          .collect::<Vec<_>>()
          .join("/")
      );
      let mut rules = vec![];
      for (case_index, case) in dimension.cases.iter().enumerate() {
        let choice = key(&format!("choice-{case_index}"))?;
        let compare = |left, comparison, right| FlowPredicate::Compare {
          left,
          comparison,
          right,
        };
        let selected = FlowOperand::Path { path: path.clone() };
        let when = match &case.selection {
          FactoryReportingSelection::Literal { value } => compare(
            selected,
            FlowComparison::Equal,
            FlowOperand::Literal { value: value.clone() },
          ),
          FactoryReportingSelection::IntegerRange { minimum, maximum } => FlowPredicate::All {
            predicates: vec![
              compare(
                FlowOperand::Literal {
                  value: (*minimum).into(),
                },
                FlowComparison::LessOrEqual,
                selected.clone(),
              ),
              compare(
                selected,
                FlowComparison::LessOrEqual,
                FlowOperand::Literal {
                  value: (*maximum).into(),
                },
              ),
            ],
          },
        };
        rules.push(FlowGateRule {
          outcome: choice.clone(),
          when,
        });
        let target = if let Some(effects) = &case.effects {
          let action_key = key(&format!("dimension-{}-effect-{case_index}", dimension.key))?;
          nodes.push(self.effect_node(action_key.clone(), &input, effects)?);
          transitions.push(FlowTransition::new(
            action_key.clone(),
            self.action_outcome.clone(),
            FlowTransitionTarget::Node(next.clone()),
          ));
          action_key
        } else {
          next.clone()
        };
        transitions.push(FlowTransition::new(
          name.clone(),
          choice,
          FlowTransitionTarget::Node(target),
        ));
      }
      let skip = key("skip")?;
      transitions.push(FlowTransition::new(
        name.clone(),
        skip.clone(),
        FlowTransitionTarget::Node(next),
      ));
      let precondition = FlowPredicate::Any {
        predicates: dimension
          .outcomes
          .iter()
          .map(|outcome| FlowPredicate::Compare {
            left: FlowOperand::Path {
              path: "/outcome".into(),
            },
            comparison: FlowComparison::Equal,
            right: FlowOperand::Literal {
              value: serde_json::json!(outcome),
            },
          })
          .collect(),
      };
      nodes.push(self.control_node(name, &input, &control, rules, Some(precondition))?);
    }
    let done = key("reported")?;
    nodes.push(
      node(
        finish.clone(),
        FlowNodeKind::DeterministicGate,
        &input,
        vec![FlowOutcomeDefinition::new(
          done.clone(),
          FlowOutcomeKind::Success,
          result.reference().clone(),
        )],
        self.budget,
        FactoryPermissionSet::deny_all(),
      )?
      .with_gate(
        FlowGateProgram::new(vec![], done.clone())
          .and_then(|gate| {
            gate.with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer {
              path: "/result".into(),
            })?)
          })
          .and_then(|gate| gate.binding())
          .map_err(invalid)?,
      )
      .map_err(invalid)?,
    );
    transitions.push(FlowTransition::new(
      finish,
      done.clone(),
      FlowTransitionTarget::Terminal(done.clone()),
    ));
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id,
      version,
      input_schema: Some(input.reference().clone()),
      entry: select,
      nodes,
      transitions,
      terminals: vec![FlowTerminalDefinition::new(done, result.reference().clone())],
      context_projections: vec![],
      data_projections: vec![],
      execution: self.execution.clone(),
    })
    .map_err(invalid)?;
    Ok(PublishedFlowTemplate {
      definition,
      schemas: vec![input, control, self.receipt.clone(), result.clone()],
    })
  }

  // All effect rows share the same installed action contract and target validation.
  fn effect_node(
    &self,
    name: FactoryKey,
    input: &FlowDataSchema,
    effects: &WorkSourceStatusEffects,
  ) -> Result<FlowNodeDefinition, ApplicationError> {
    self
      .target
      .policy
      .validate_mapping(&WorkSourceStatusMapping {
        statuses: vec![effects.clone()],
      })
      .map_err(invalid)?;
    let update = FactoryStageSourceUpdate {
      ownership_scopes: self.target.policy.ownership_for(effects).map_err(invalid)?,
      source: self.target.source.clone(),
      reporter: self.target.reporter.clone(),
      connection: Some(self.target.connection.clone()),
      effects: effects.clone(),
    };
    self.validate_update(&update)?;
    node(
      name,
      FlowNodeKind::TrustedAction,
      input,
      vec![FlowOutcomeDefinition::new(
        self.action_outcome.clone(),
        FlowOutcomeKind::Success,
        self.receipt.reference().clone(),
      )],
      self.budget,
      self.execution.permissions().clone(),
    )?
    .with_action(
      FlowActionBinding::new(
        self.target.reporter.capability.clone(),
        key("set_status")?,
        serde_json::json!({"status":effects.status,"source_update":update,"outbox":self.retry}),
      )
      .map_err(invalid)?,
    )
    .map_err(invalid)
  }

  fn control_node(
    &self,
    name: FactoryKey,
    input: &FlowDataSchema,
    control: &FlowDataSchema,
    rules: Vec<FlowGateRule>,
    precondition: Option<FlowPredicate>,
  ) -> Result<FlowNodeDefinition, ApplicationError> {
    let skip = key("skip")?;
    let choices = rules
      .iter()
      .map(|rule| rule.outcome.clone())
      .chain(std::iter::once(skip.clone()))
      .map(|outcome| FlowOutcomeDefinition::new(outcome, FlowOutcomeKind::Success, control.reference().clone()))
      .collect();
    let mut program = FlowGateProgram::new(rules, skip).map_err(invalid)?;
    if let Some(precondition) = precondition {
      program = program.with_precondition(precondition).map_err(invalid)?;
    }
    let gate = program
      .with_default_output_mapping(
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!(true),
        })
        .map_err(invalid)?,
      )
      .and_then(|program| program.binding())
      .map_err(invalid)?;
    node(
      name,
      FlowNodeKind::DeterministicGate,
      input,
      choices,
      self.budget,
      FactoryPermissionSet::deny_all(),
    )?
    .with_gate(gate)
    .map_err(invalid)
  }

  fn validate_update(&self, update: &FactoryStageSourceUpdate) -> Result<(), ApplicationError> {
    update.validate()?;
    self.retry.validate()?;
    if !self
      .execution
      .permissions()
      .plugins()
      .any(|pin| pin == &self.target.reporter.capability)
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }

  // Alternatives inside one dimension are exclusive. Only groups that can run
  // together for the same accepted outcome must own disjoint native fields.
  fn validate_effect_ownership(&self) -> Result<(), ApplicationError> {
    for (outcome, effect) in &self.outcomes {
      let groups: Vec<_> = std::iter::once(effect.iter().collect::<Vec<_>>())
        .chain(
          self
            .dimensions
            .iter()
            .filter(|dimension| dimension.outcomes.contains(outcome))
            .map(|dimension| {
              dimension
                .cases
                .iter()
                .filter_map(|case| case.effects.as_ref())
                .collect()
            }),
        )
        .filter(|group| {
          group
            .iter()
            .any(|effect| !effect.add.is_empty() || !effect.remove.is_empty() || effect.state.is_some())
        })
        .collect();
      // One producer needs no independence proof. Multiple coactive producers do.
      if groups.len() < 2 {
        continue;
      }
      let mut owned = BTreeSet::new();
      for group in groups {
        let scopes = group.iter().try_fold(BTreeSet::new(), |mut scopes, effect| {
          scopes.extend(
            self
              .target
              .policy
              .ownership_for(effect)
              .map_err(invalid)?
              .ok_or_else(ApplicationError::invalid)?,
          );
          Ok::<_, ApplicationError>(scopes)
        })?;
        if !owned.is_disjoint(&scopes) {
          return Err(ApplicationError::invalid());
        }
        owned.extend(scopes);
      }
    }
    Ok(())
  }
}

impl FactoryReportingDimension {
  fn validate(&self, result: &FlowDataSchema, outcomes: &BTreeSet<FactoryKey>) -> Result<(), ApplicationError> {
    let fail = ApplicationError::invalid;
    if self.path.is_empty()
      || self.path.len() > MAX_FLOW_SCHEMA_DEPTH
      || self.cases.is_empty()
      || self.cases.len() > MAX_FLOW_SCHEMA_ENTRIES
      || self.outcomes.is_empty()
      || !self.outcomes.is_subset(outcomes)
    {
      return Err(fail());
    }
    let mut shape = result.shape();
    for part in &self.path {
      let FlowValueSchema::Object { fields } = shape else {
        return Err(fail());
      };
      let field = fields.get(part).filter(|field| field.is_required()).ok_or_else(fail)?;
      shape = field.value();
    }
    match shape {
      FlowValueSchema::Enum { values } => {
        let mut selected = vec![];
        for case in &self.cases {
          let FactoryReportingSelection::Literal { value } = &case.selection else {
            return Err(fail());
          };
          if !values.contains(value) || selected.contains(&value) {
            return Err(fail());
          }
          selected.push(value);
        }
        if selected.len() != values.len() {
          return Err(fail());
        }
      }
      FlowValueSchema::Integer { minimum, maximum } => {
        let mut ranges = vec![];
        for case in &self.cases {
          let range = match &case.selection {
            FactoryReportingSelection::IntegerRange { minimum, maximum } => (*minimum, *maximum),
            FactoryReportingSelection::Literal { value } => {
              let value = value.as_i64().ok_or_else(fail)?;
              (value, value)
            }
          };
          if range.0 > range.1 || range.0 < *minimum || range.1 > *maximum {
            return Err(fail());
          }
          ranges.push(range);
        }
        ranges.sort_unstable();
        if ranges.first().map(|r| r.0) != Some(*minimum)
          || ranges.last().map(|r| r.1) != Some(*maximum)
          || ranges
            .windows(2)
            .any(|pair| pair[0].1.checked_add(1) != Some(pair[1].0))
        {
          return Err(fail());
        }
      }
      _ => return Err(fail()),
    }
    Ok(())
  }
}

fn node(
  name: FactoryKey,
  kind: FlowNodeKind,
  input: &FlowDataSchema,
  outcomes: Vec<FlowOutcomeDefinition>,
  budget: BudgetLimit,
  permissions: FactoryPermissionSet,
) -> Result<FlowNodeDefinition, ApplicationError> {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: name,
    kind,
    input_schema: Some(input.reference().clone()),
    outcomes,
    budget,
    permissions,
    required: true,
    subflow: None,
  })
  .and_then(|node| {
    node.with_input_binding(FlowInputBinding::new(
      BTreeMap::from([(
        FactoryKey::new("accepted")?,
        FlowInputSource::CallerInput { path: String::new() },
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/accepted".into(),
      })?,
    )?)
  })
  .map_err(invalid)
}
fn key(name: &str) -> Result<FactoryKey, ApplicationError> {
  FactoryKey::new(name).map_err(invalid)
}
fn invalid(_: FactoryError) -> ApplicationError {
  ApplicationError::invalid()
}
