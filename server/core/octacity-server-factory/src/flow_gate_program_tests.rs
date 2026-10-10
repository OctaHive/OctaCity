use crate::*;

#[test]
fn configured_time_comparisons_use_the_owner_observation_time_and_fail_closed_without_it() {
  let schema = FlowDataSchema::new(
    FactoryKey::new("proof.deadline").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FlowValueSchema::Integer {
      minimum: 0,
      maximum: 1000,
    },
  )
  .unwrap();
  let input = FlowPayload::new(&schema, serde_json::json!(100)).unwrap();
  let program = FlowGateProgram::new(
    vec![FlowGateRule {
      outcome: FactoryKey::new("accepted").unwrap(),
      when: FlowPredicate::Compare {
        left: FlowOperand::ObservationTime,
        comparison: FlowComparison::LessOrEqual,
        right: FlowOperand::Path { path: "".into() },
      },
    }],
    FactoryKey::new("stale").unwrap(),
  )
  .unwrap();
  assert_eq!(program.select(&input).unwrap().as_str(), "stale");
  assert_eq!(
    program
      .select_at(&input, octacity_server_domain::Timestamp::from_unix_millis(99).unwrap())
      .unwrap()
      .as_str(),
    "accepted"
  );
  assert_eq!(
    program
      .select_at(
        &input,
        octacity_server_domain::Timestamp::from_unix_millis(101).unwrap()
      )
      .unwrap()
      .as_str(),
    "stale"
  );
}
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn compare(path: &str, comparison: FlowComparison, value: serde_json::Value) -> FlowPredicate {
  FlowPredicate::Compare {
    left: FlowOperand::Path { path: path.into() },
    comparison,
    right: FlowOperand::Literal { value },
  }
}
#[test]
fn configured_gate_selects_ordered_finite_outcomes_from_projected_data() {
  let schema = FlowDataSchema::new(
    key("team.audit_input"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("count"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        ),
        (
          key("risk"),
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: vec![serde_json::json!("low"), serde_json::json!("high")],
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let program = FlowGateProgram::new(
    vec![
      FlowGateRule {
        when: FlowPredicate::All {
          predicates: vec![
            compare("/count", FlowComparison::LessOrEqual, serde_json::json!(3)),
            compare("/risk", FlowComparison::Equal, serde_json::json!("low")),
          ],
        },
        outcome: key("continue"),
      },
      FlowGateRule {
        when: compare("/count", FlowComparison::Greater, serde_json::json!(3)),
        outcome: key("needs_review"),
      },
    ],
    key("escalate"),
  )
  .unwrap();
  for (value, expected) in [
    (serde_json::json!({"count": 2, "risk": "low"}), "continue"),
    (serde_json::json!({"count": 4, "risk": "high"}), "needs_review"),
    (serde_json::json!({"count": 0, "risk": "high"}), "escalate"),
  ] {
    assert_eq!(
      program.select(&FlowPayload::new(&schema, value).unwrap()).unwrap(),
      key(expected)
    );
  }
  let restored: FlowGateProgram = serde_json::from_slice(&serde_json::to_vec(&program).unwrap()).unwrap();
  assert_eq!(restored, program);
}

#[test]
fn configured_gate_missing_or_mistyped_facts_cannot_pass_negated_conditions() {
  let schema = FlowDataSchema::new(
    key("team.optional"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("count"),
          FlowFieldSchema::optional(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        ),
        (
          key("text"),
          FlowFieldSchema::optional(FlowValueSchema::String {
            min_bytes: 0,
            max_bytes: 10,
          }),
        ),
      ]),
    },
  )
  .unwrap();
  for predicate in [
    compare("/count", FlowComparison::Equal, serde_json::json!(3)),
    compare("/count", FlowComparison::Equal, serde_json::json!("3")),
    compare("/text", FlowComparison::Greater, serde_json::json!(0)),
    FlowPredicate::Exists {
      operand: FlowOperand::Path {
        path: "/missing".into(),
      },
    },
  ] {
    let program = FlowGateProgram::new(
      vec![FlowGateRule {
        when: FlowPredicate::Not {
          predicate: Box::new(predicate),
        },
        outcome: key("continue"),
      }],
      key("needs_review"),
    )
    .unwrap();
    for value in [serde_json::json!({}), serde_json::json!({"count": 3, "text": "4"})] {
      assert_eq!(
        program.select(&FlowPayload::new(&schema, value).unwrap()).unwrap(),
        key("needs_review")
      );
    }
  }
  let program = FlowGateProgram::new(
    vec![FlowGateRule {
      when: FlowPredicate::Not {
        predicate: Box::new(compare("/count", FlowComparison::Equal, serde_json::json!(3))),
      },
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  assert_eq!(
    program
      .select(&FlowPayload::new(&schema, serde_json::json!({"count": 4})).unwrap())
      .unwrap(),
    key("continue")
  );
}

#[test]
fn configured_gate_rejects_unbounded_and_ambiguous_programs_before_use() {
  let rule = |when| FlowGateRule {
    when,
    outcome: key("continue"),
  };
  for when in [
    FlowPredicate::All { predicates: vec![] },
    compare("count", FlowComparison::Equal, serde_json::json!(3)),
    compare("/bad~2path", FlowComparison::Equal, serde_json::json!(3)),
    compare("/count", FlowComparison::Equal, serde_json::json!({"number": 3})),
  ] {
    assert!(FlowGateProgram::new(vec![rule(when)], key("needs_review")).is_err());
  }
  let mut deep = compare("/count", FlowComparison::Equal, serde_json::json!(3));
  for _ in 0..17 {
    deep = FlowPredicate::Not {
      predicate: Box::new(deep),
    };
  }
  assert!(FlowGateProgram::new(vec![rule(deep)], key("needs_review")).is_err());
  let too_many = vec![rule(compare("/count", FlowComparison::Equal, serde_json::json!(3))); 65];
  assert!(FlowGateProgram::new(too_many, key("needs_review")).is_err());
  let broad = FlowPredicate::All {
    predicates: vec![compare("/a", FlowComparison::Equal, serde_json::json!(true)); 129],
  };
  assert!(FlowGateProgram::new(vec![rule(broad)], key("needs_review")).is_err());
  let invalid_wire = serde_json::json!({"rules": [{"when": {"operation": "all", "predicates": []}, "outcome": "continue"}], "fallback": "needs_review"});
  assert!(serde_json::from_value::<FlowGateProgram>(invalid_wire).is_err());
}

#[test]
fn configured_gate_quantifies_observations_and_keeps_missing_items_fail_closed() {
  let schema = FlowDataSchema::new(
    key("team.checks"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("checks"),
        FlowFieldSchema::optional(FlowValueSchema::Array {
          item: Box::new(FlowValueSchema::Object {
            fields: BTreeMap::from([(key("passed"), FlowFieldSchema::optional(FlowValueSchema::Boolean))]),
          }),
          min_items: 0,
          max_items: 4,
        }),
      )]),
    },
  )
  .unwrap();
  let every = FlowPredicate::ForEach {
    collection: FlowOperand::Path { path: "/checks".into() },
    quantifier: FlowQuantifier::Every,
    predicate: Box::new(FlowPredicate::Compare {
      left: FlowOperand::Item { path: "/passed".into() },
      comparison: FlowComparison::Equal,
      right: FlowOperand::Literal {
        value: serde_json::json!(true),
      },
    }),
  };
  let program = FlowGateProgram::new(
    vec![FlowGateRule {
      when: every.clone(),
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  for (value, expected) in [
    (
      serde_json::json!({"checks": [{"passed": true}, {"passed": true}]}),
      "continue",
    ),
    (
      serde_json::json!({"checks": [{"passed": true}, {"passed": false}]}),
      "needs_review",
    ),
    (serde_json::json!({"checks": [{"passed": true}, {}]}), "needs_review"),
    (serde_json::json!({"checks": []}), "needs_review"),
    (serde_json::json!({}), "needs_review"),
  ] {
    assert_eq!(
      program.select(&FlowPayload::new(&schema, value).unwrap()).unwrap(),
      key(expected)
    );
  }
  let negated = FlowGateProgram::new(
    vec![FlowGateRule {
      when: FlowPredicate::Not {
        predicate: Box::new(every),
      },
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  assert_eq!(
    negated
      .select(&FlowPayload::new(&schema, serde_json::json!({"checks": [{"passed": false}, {}]})).unwrap())
      .unwrap(),
    key("needs_review")
  );
  let any = FlowGateProgram::new(
    vec![FlowGateRule {
      when: FlowPredicate::Any {
        predicates: vec![
          FlowPredicate::Exists {
            operand: FlowOperand::Path {
              path: "/missing".into(),
            },
          },
          FlowPredicate::Exists {
            operand: FlowOperand::Path { path: "/checks".into() },
          },
        ],
      },
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  assert_eq!(
    any
      .select(&FlowPayload::new(&schema, serde_json::json!({"checks": []})).unwrap())
      .unwrap(),
    key("needs_review")
  );
  assert!(
    FlowGateProgram::new(
      vec![FlowGateRule {
        when: FlowPredicate::Exists {
          operand: FlowOperand::Item { path: "/passed".into() }
        },
        outcome: key("continue"),
      }],
      key("needs_review")
    )
    .is_err()
  );
}

#[test]
fn configured_gate_caps_evaluation_even_when_nested_rules_rescan_root_arrays() {
  let schema = FlowDataSchema::new(
    key("team.observations"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("checks"),
        FlowFieldSchema::required(FlowValueSchema::Array {
          item: Box::new(FlowValueSchema::Boolean),
          min_items: 4,
          max_items: 4,
        }),
      )]),
    },
  )
  .unwrap();
  let mut predicate = FlowPredicate::Exists {
    operand: FlowOperand::Item { path: String::new() },
  };
  for _ in 0..9 {
    predicate = FlowPredicate::ForEach {
      collection: FlowOperand::Path { path: "/checks".into() },
      quantifier: FlowQuantifier::Every,
      predicate: Box::new(predicate),
    };
  }
  let program = FlowGateProgram::new(
    vec![FlowGateRule {
      when: predicate,
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  assert!(
    program
      .select(&FlowPayload::new(&schema, serde_json::json!({"checks": [true, true, true, true]})).unwrap())
      .is_err()
  );
}

#[test]
fn configured_gate_pins_rules_and_rejects_undeclared_outcomes_or_policy_replacement() {
  let node = || {
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key("team.release_check"),
      kind: FlowNodeKind::DeterministicGate,
      input_schema: None,
      outcomes: ["continue", "needs_review"]
        .into_iter()
        .map(|name| {
          FlowOutcomeDefinition::new(
            key(name),
            FlowOutcomeKind::Success,
            ImmutableReference::new(key("observation"), key("v1"), FactoryDigest::from_bytes([1; 32])),
          )
        })
        .collect(),
      budget: BudgetLimit::new(1, 100, 10, 10, 100).unwrap(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: None,
    })
    .unwrap()
  };
  let program = FlowGateProgram::new(
    vec![FlowGateRule {
      when: compare("/count", FlowComparison::LessOrEqual, serde_json::json!(3)),
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  let binding = program.binding().unwrap();
  let configured = node().with_gate(binding.clone()).unwrap();
  assert_eq!(FlowGateProgram::from_node(&configured).unwrap(), program);
  let wrong = FlowGateProgram::new(vec![], key("invented_outcome")).unwrap();
  assert!(node().with_gate(wrong.binding().unwrap()).is_err());
  let reference = binding.policy();
  let replaced = FlowGateBinding::new(
    ImmutableReference::new(
      reference.identity().clone(),
      reference.version().clone(),
      FactoryDigest::from_bytes([99; 32]),
    ),
    binding.parameters().clone(),
  )
  .unwrap();
  assert!(node().with_gate(replaced).is_err());
  let different = FlowGateProgram::new(
    vec![FlowGateRule {
      when: compare("/count", FlowComparison::LessOrEqual, serde_json::json!(8)),
      outcome: key("continue"),
    }],
    key("needs_review"),
  )
  .unwrap();
  assert_ne!(binding, different.binding().unwrap());
  assert_ne!(
    serde_json::to_vec(&configured).unwrap(),
    serde_json::to_vec(&node().with_gate(different.binding().unwrap()).unwrap()).unwrap()
  );
}

#[test]
fn declared_outcomes_can_share_one_bounded_output_mapping_with_explicit_overrides() {
  let make = |count| {
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!({"count":count}),
    })
    .unwrap()
  };
  let program = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_default_output_mapping(make(4))
    .unwrap();
  let verify = |program: FlowGateProgram, expected| {
    let (work, admitted, schema) =
      crate::flow_data_tests::node_fixture_with_bindings(Some(program.binding().unwrap()), None);
    let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
    let input = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      key("accessibility_audit"),
      FlowPayload::new(&schema, serde_json::json!({"count":3})).unwrap(),
    )
    .unwrap();
    let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
    let expiry = octacity_server_domain::Timestamp::from_unix_millis(100).unwrap();
    let owner = FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(FactoryClaimFence::new(FactoryDigest::from_bytes([7; 32])), at, expiry).unwrap(),
    );
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: input.node().clone(),
        node_kind: FlowNodeKind::DeterministicGate,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: definition.nodes()[0].budget(),
        deadline: expiry,
        execution: NodeExecutionIdentity::BuiltIn,
        ownership: owner,
      },
    )
    .unwrap();
    let restored: FlowGateProgram = serde_json::from_slice(&serde_json::to_vec(&program).unwrap()).unwrap();
    let record = restored.record(&input, &attempt, definition, &schema, at).unwrap();
    assert_eq!(record.payload().value(), &serde_json::json!({"count":expected}));
  };
  verify(program.clone(), 4);
  verify(program.with_output_mapping(key("observed"), make(9)).unwrap(), 9);
}
