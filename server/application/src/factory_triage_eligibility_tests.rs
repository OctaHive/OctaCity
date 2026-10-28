//! Native evidence crosses the released triage result contract and declarative policy.
use super::*;
use octacity_server_factory::{FlowDefinitionTemplate, FlowGateProgram, FlowPayload};

fn template() -> FlowDefinitionTemplate {
  fixtures::supplied_triage_template().unwrap()
}

fn assessment() -> Value {
  json!({"fit":"fits","uncertainty":"clear","reason":"Same documented symptom and affected component.",
    "duplicate": true})
}

fn conclusion() -> Value {
  json!({"assessment":assessment(),"claims":[
    {"connection":"issues","tool":"lookup","arguments_json":"{\"query\":\"candidate\"}","pointer":"/candidate","json_text":0,"value_json":"{\"id\":\"ISSUE-42\",\"version\":3}"},
    {"connection":"issues","tool":"lookup","arguments_json":"{\"query\":\"candidate\"}","pointer":"/coverage/has_more","json_text":0,"value_json":"false"}
  ]})
}

// A reviewed finite external server uses its native lookup contract. Only policy
// literals change; it needs no provider query adapter or different result verifier.
fn reviewed_gate(template: &FlowDefinitionTemplate) -> FlowGateProgram {
  let gate = template
    .nodes
    .iter()
    .find(|node| node.key.as_str() == "eligibility_gate")
    .unwrap()
    .gate
    .as_ref()
    .unwrap();
  let wire = serde_json::to_string(gate)
    .unwrap()
    .replace("read_issue", "lookup")
    .replace("search_issues", "lookup")
    .replace("/issue\"", "/candidate\"");
  serde_json::from_str(&wire).unwrap()
}

#[test]
fn triage_eligibility_accepts_only_a_reviewed_observed_duplicate_and_skips_the_analyst() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let retained = bound.verify_conclusion(conclusion()).unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  let result = FlowPayload::new(&schemas[&key("eligibility_observations")], retained)
    .expect("the triage result must accept independently retained MCP evidence and bounded assessment");
  let input = FlowPayload::new(
    &schemas[&key("eligibility_gate_input")],
    owner_input(&f, result.value().clone()),
  )
  .unwrap();
  let selected = reviewed_gate(&template)
    .select_at(&input, Timestamp::from_unix_millis(1006).unwrap())
    .unwrap();
  assert_eq!(selected.as_str(), "duplicate");
  let transition = template
    .transitions
    .iter()
    .find(|transition| transition.predecessor().as_str() == "eligibility_gate" && transition.outcome() == &selected)
    .unwrap();
  assert_eq!(
    serde_json::to_value(transition.target()).unwrap(),
    json!({"kind":"terminal","key":"rejected"})
  );
  assert_eq!(
    input.value()["conclusion"]["verification"]["duplicate_proven"],
    false,
    "the reviewed business policy owns acceptance, not the generic observation verifier"
  );
}

#[test]
fn triage_eligibility_candidate_formatting_does_not_change_the_observed_duplicate_decision() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  for candidate in [
    r#"{"id":"ISSUE-42","version":3}"#,
    "{\n  \"version\": 3,\n  \"id\": \"ISSUE-42\"\n}",
  ] {
    let mut model = conclusion();
    model["assessment"] =
      json!({"fit":"fits","uncertainty":"clear","reason":"Same documented symptom.","duplicate":true});
    model["claims"][0]["value_json"] = json!(candidate);
    let verified = bound.verify_conclusion(model).unwrap();
    let result = FlowPayload::new(&schemas[&key("eligibility_observations")], verified)
      .expect("the candidate is selected by its verified detail claim, without a second JSON representation");
    assert!(result.value()["assessment"].get("duplicate_candidate_json").is_none());
    assert_eq!(
      serde_json::from_str::<Value>(result.value()["claims"][0]["value_json"].as_str().unwrap()).unwrap(),
      json!({"id":"ISSUE-42","version":3})
    );
    let input = FlowPayload::new(
      &schemas[&key("eligibility_gate_input")],
      owner_input(&f, result.value().clone()),
    )
    .unwrap();
    assert_eq!(
      reviewed_gate(&template)
        .select_at(&input, Timestamp::from_unix_millis(1012).unwrap())
        .unwrap()
        .as_str(),
      "duplicate"
    );
  }
}

#[test]
fn triage_eligibility_keeps_one_candidate_claim_and_one_coverage_claim() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let schema = template()
    .data_schemas()
    .unwrap()
    .remove(&key("eligibility_observations"))
    .unwrap();
  let mut model = conclusion();
  let additional_candidate = model["claims"][0].clone();
  model["claims"].as_array_mut().unwrap().push(additional_candidate);
  let verified = bound.verify_conclusion(model).unwrap();
  assert!(
    FlowPayload::new(&schema, verified).is_err(),
    "the two-claim contract leaves only one candidate when the required coverage claim is present"
  );
}

#[test]
fn triage_eligibility_routes_clear_fit_and_out_of_scope_without_inventing_global_absence() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  for (fit, expected) in [("fits", "classify"), ("out_of_scope", "out_of_scope")] {
    let mut model = conclusion();
    model["assessment"]["duplicate"] = json!(false);
    model["assessment"]["fit"] = json!(fit);
    let retained = bound.verify_conclusion(model).unwrap();
    let result = FlowPayload::new(&schemas[&key("eligibility_observations")], retained).unwrap();
    let input = FlowPayload::new(
      &schemas[&key("eligibility_gate_input")],
      owner_input(&f, result.value().clone()),
    )
    .unwrap();
    let outcome = reviewed_gate(&template)
      .select_at(&input, Timestamp::from_unix_millis(1006).unwrap())
      .unwrap();
    assert_eq!(outcome.as_str(), expected, "{fit}");
    let transition = template
      .transitions
      .iter()
      .find(|t| t.predecessor().as_str() == "eligibility_gate" && t.outcome() == &outcome)
      .unwrap();
    assert_eq!(
      serde_json::to_value(transition.target()).unwrap(),
      if expected == "classify" {
        json!({"kind":"node","key":"classification"})
      } else {
        json!({"kind":"terminal","key":"rejected"})
      }
    );
    assert_eq!(result.value()["verification"]["global_absence_proven"], false);
    assert_eq!(result.value()["verification"]["coverage"], "unknown");
  }
}

#[test]
fn triage_eligibility_waits_for_declared_input_or_review_and_never_dispatches_the_analyst() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  for (fit, uncertainty, expected) in [
    ("fits", "needs_input", "needs_input"),
    ("fits", "review", "review"),
    ("inconclusive", "clear", "review"),
  ] {
    let mut model = conclusion();
    model["assessment"]["fit"] = json!(fit);
    model["assessment"]["uncertainty"] = json!(uncertainty);
    let result = FlowPayload::new(
      &schemas[&key("eligibility_observations")],
      bound.verify_conclusion(model).unwrap(),
    )
    .unwrap();
    let input = FlowPayload::new(
      &schemas[&key("eligibility_gate_input")],
      owner_input(&f, result.value().clone()),
    )
    .unwrap();
    let outcome = reviewed_gate(&template)
      .select_at(&input, Timestamp::from_unix_millis(1006).unwrap())
      .unwrap();
    assert_eq!(outcome.as_str(), expected, "{fit}/{uncertainty}");
    let transition = template
      .transitions
      .iter()
      .find(|t| t.predecessor().as_str() == "eligibility_gate" && t.outcome() == &outcome)
      .unwrap();
    assert_eq!(
      serde_json::to_value(transition.target()).unwrap(),
      json!({"kind":"terminal","key":"project_review"})
    );
  }
}

fn owner_input(f: &EvidenceFixture, conclusion: Value) -> Value {
  let identity = &f.native.documents[0].identity;
  json!({"conclusion":conclusion,"producer":{"build_id":identity.build_id,"attempt_id":identity.attempt_id,
    "job_id":identity.job_id,"tool":factory_mcp_evidence_verifier(),"plugin":f.role.package().inputs().task.plugin()},
    "fresh_until":1020})
}

#[test]
fn triage_eligibility_requires_the_actual_result_verifier_execution_and_freshness() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  let original = owner_input(&f, bound.verify_conclusion(conclusion()).unwrap());
  for case in [
    "valid",
    "unverified",
    "substituted_build",
    "substituted_attempt",
    "substituted_job",
    "stale",
  ] {
    let mut value = original.clone();
    match case {
      "unverified" => value["producer"]["tool"] = json!(reference("model-output", 44)),
      "substituted_build" => value["producer"]["build_id"] = json!(octacity_server_domain::BuildId::generate()),
      "substituted_attempt" => value["producer"]["attempt_id"] = json!(octacity_server_domain::AttemptId::generate()),
      "substituted_job" => value["producer"]["job_id"] = json!(octacity_server_domain::JobId::generate()),
      "stale" => value["fresh_until"] = json!(1005),
      _ => {}
    }
    let payload = FlowPayload::new(&schemas[&key("eligibility_gate_input")], value)
      .expect("the gate contract must carry independently owned output provenance and freshness");
    assert_eq!(
      reviewed_gate(&template)
        .select_at(&payload, Timestamp::from_unix_millis(1006).unwrap())
        .unwrap()
        .as_str(),
      if case == "valid" { "duplicate" } else { "review" },
      "{case}"
    );
  }
}

#[test]
fn triage_eligibility_rejects_forged_candidates_references_and_model_control_fields() {
  let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  for case in [
    "candidate",
    "reference",
    "write_tool",
    "ambient_connection",
    "route",
    "effect",
    "oversized_reason",
    "unknown_fit",
  ] {
    let mut model = conclusion();
    match case {
      "candidate" => model["claims"][0]["value_json"] = json!("{\"id\":\"HIDDEN-99\",\"version\":3}"),
      "reference" => {
        model["evidence"] = json!([capture.mcp_calls()[0].reference()]);
        model["evidence"][0]["sequence"] = json!(999);
      }
      "write_tool" => model["claims"][0]["tool"] = json!("close_issue"),
      "ambient_connection" => model["claims"][0]["connection"] = json!("reporter"),
      "route" => model["assessment"]["route"] = json!("classification"),
      "effect" => model["assessment"]["close_issue"] = json!(true),
      "oversized_reason" => model["assessment"]["reason"] = json!("x".repeat(2049)),
      _ => model["assessment"]["fit"] = json!("definitely"),
    }
    let observed = bound.verify_conclusion(model).and_then(|value| {
      FlowPayload::new(&schemas[&key("eligibility_observations")], value)
        .map_err(|_| crate::ApplicationError::invalid())
    });
    assert!(observed.is_err(), "{case} cannot reach an accepted triage result");
  }
}

#[test]
fn triage_eligibility_keeps_incomplete_unknown_and_unclaimed_coverage_in_review() {
  let template = template();
  let schemas = template.data_schemas().unwrap();
  for case in [
    "more_pages",
    "unknown",
    "truncated",
    "no_claims",
    "unresolved_candidate",
    "untrusted_verification",
  ] {
    let result = if case == "more_pages" {
      r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":true}}"#
    } else if case == "truncated" {
      r#"{"candidate":{"id":"ISSUE-42","version":3},"truncated":true}"#
    } else {
      r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#
    };
    let f = fixture(result);
    let capture = f.capture();
    let bound = capture
      .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
      .unwrap();
    let mut model = conclusion();
    match case {
      "more_pages" => model["claims"][1]["value_json"] = json!("true"),
      "unknown" | "truncated" => {
        model["claims"].as_array_mut().unwrap().truncate(1);
      }
      "no_claims" => model["claims"] = json!([]),
      "unresolved_candidate" => {
        model["claims"].as_array_mut().unwrap().remove(0);
      }
      _ => {
        model["claims"] = json!([]);
        model["verification"] = json!({"status":"observed","duplicate_proven":true,"coverage":"complete"});
      }
    }
    let value = bound.verify_conclusion(model).unwrap();
    let payload = FlowPayload::new(&schemas[&key("eligibility_observations")], value).unwrap();
    let input = FlowPayload::new(
      &schemas[&key("eligibility_gate_input")],
      owner_input(&f, payload.value().clone()),
    )
    .unwrap();
    assert_eq!(
      reviewed_gate(&template)
        .select_at(&input, Timestamp::from_unix_millis(1006).unwrap())
        .unwrap()
        .as_str(),
      "review",
      "{case}"
    );
    assert_eq!(payload.value()["verification"]["global_absence_proven"], false);
  }
}

#[test]
fn triage_eligibility_release_uses_the_shared_native_conclusion_contract() {
  let schemas = template().data_schemas().unwrap();
  let retained = schemas[&key("eligibility_observations")].shape();
  let octacity_server_factory::FlowValueSchema::Object { fields } = retained else {
    panic!("closed conclusion")
  };
  let mut shared = factory_mcp_conclusion_shape(fields[&key("assessment")].value().clone());
  let FlowValueSchema::Object { fields } = &mut shared else {
    panic!("closed shared conclusion")
  };
  let mut claims = fields[&key("claims")].value().clone();
  let FlowValueSchema::Array { max_items, .. } = &mut claims else {
    panic!("bounded claims")
  };
  *max_items = 2; // One selected entity and one required coverage value; the shared wire grammar is unchanged.
  fields.insert(key("claims"), FlowFieldSchema::required(claims));
  assert_eq!(*retained, shared);
}

fn native_issue_fixture() -> (EvidenceFixture, Value) {
  let mut f = fixture_with_tools("{}", vec!["read_issue".into(), "search_issues".into()]);
  let mut rows = std::str::from_utf8(&f.native.documents[1].bytes)
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str::<Value>(line).unwrap())
    .collect::<Vec<_>>();
  let id = "00000000-0000-0000-0000-000000000042";
  let entity = json!({"id":id,"project_id":"00000000-0000-0000-0000-000000000001","number":42,"version":3,"applicability_version":2,
    "content_available":true,"content":{"title":"Authentication cancellation","description":"Cancellation interrupts the request.","acceptance":"Reproduce and triage.","priority":3},
    "workflow_id":"00000000-0000-0000-0000-000000000010","workflow_version":1,"status_id":"00000000-0000-0000-0000-000000000011","created_at":1000,"updated_at":1005});
  for row in &mut rows[..2] {
    row["event"]["item"]["tool"] = json!("read_issue");
    row["event"]["item"]["arguments"] = json!({"issue_id":id});
  }
  rows[1]["event"]["item"]["result"] = json!({"content":[{"type":"text","text":json!({"issue":entity}).to_string()}]});
  let terminal = rows.pop().unwrap();
  for (sequence, event, status) in [(2, "item.started", "in_progress"), (3, "item.completed", "completed")] {
    rows.push(json!({"format_version":2,"sequence":sequence,"observed_at_unix_millis":1004+sequence,
      "event":{"type":event,"item":{"id":"call-2","type":"mcp_tool_call","server":"issues","tool":"search_issues","arguments":{"query":"candidate"},
        "status":status,"result":if sequence==3 {json!({"content":[{"type":"text","text":"{\"coverage\":{\"has_more\":false}}"}]})} else {Value::Null},"error":null}}}));
  }
  let mut terminal = terminal;
  terminal["sequence"] = json!(4);
  terminal["observed_at_unix_millis"] = json!(1008);
  rows.push(terminal);
  let trace = rows
    .iter()
    .flat_map(|row| {
      let mut bytes = serde_json::to_vec(row).unwrap();
      bytes.push(b'\n');
      bytes
    })
    .collect::<Vec<_>>();
  let mut provenance: Value = serde_json::from_slice(&f.native.documents[2].bytes).unwrap();
  provenance["capture"]["trace_digest"]["value"] = json!(FactoryDigest::content_blake3(&trace));
  provenance["capture"]["record_count"] = json!(rows.len());
  f.native.replace_document_bytes("codex-run-trace", trace);
  f.native.replace_document("codex-run-provenance", provenance);
  (f, entity)
}

#[test]
fn triage_eligibility_default_policy_consumes_native_issue_detail_and_search_contracts() {
  let (f, entity) = native_issue_fixture();
  let id = entity["id"].as_str().unwrap();
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let mut model = conclusion();
  model["claims"][0]["tool"] = json!("read_issue");
  model["claims"][0]["arguments_json"] = json!(json!({"issue_id":id}).to_string());
  model["claims"][0]["pointer"] = json!("/issue");
  model["claims"][0]["value_json"] = json!(entity.to_string());
  model["claims"][1]["tool"] = json!("search_issues");
  let result = bound.verify_conclusion(model).unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  let result = FlowPayload::new(&schemas[&key("eligibility_observations")], result).unwrap();
  assert_eq!(result.value()["evidence"].as_array().unwrap().len(), 2);
  let input = FlowPayload::new(
    &schemas[&key("eligibility_gate_input")],
    owner_input(&f, result.value().clone()),
  )
  .unwrap();
  let gate = template
    .nodes
    .iter()
    .find(|node| node.key.as_str() == "eligibility_gate")
    .unwrap()
    .gate
    .as_ref()
    .unwrap();
  assert_eq!(
    gate
      .select_at(&input, Timestamp::from_unix_millis(1012).unwrap())
      .unwrap()
      .as_str(),
    "duplicate"
  );
}

#[test]
fn triage_eligibility_compiled_project_prompt_supplies_the_evidence_contract_and_a_verified_example() {
  let seed: FactoryRoleTemplate = serde_json::from_slice(
    &std::fs::read(
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/factory/presets/triage/project-role-v1.json"),
    )
    .unwrap(),
  )
  .unwrap();
  let template = template();
  let schemas = template.data_schemas().unwrap();
  let schema = &schemas[&key("eligibility_observations")];
  let verifier = factory_mcp_evidence_verifier();
  let configuration = FactoryMcpConfiguration::new(
    key("issues"),
    key("v1"),
    FactoryMcpServer::Http {
      url: "https://octacity.example/mcp/issues".into(),
      bearer_token_env_var: None,
      enabled_tools: Some(vec!["read_issue".into(), "search_issues".into()]),
    },
    None,
  )
  .unwrap();
  let (intent, admitted, installed, package, context) = fixture_for_work_mode_with_context(
    fixtures::work(),
    |profile| {
      profile.instructions = seed.profiles[&key("default")].instructions.clone();
      profile.execution.mcp_servers.insert(
        key("issues"),
        FactoryRoleMcpBinding {
          configuration: configuration.reference().clone(),
          integration: None,
        },
      );
    },
    Default::default(),
    factory_role_task_digest,
    false,
    RoleFixtureOptions {
      role_definition: Some(&seed.role),
      result_contract: Some((schema, &verifier)),
      ..Default::default()
    },
  );
  let role = FactoryRoleInvocation::new(package, None, Default::default()).unwrap();
  let configurations = [FactoryCodexMcpConfiguration::try_from(configuration).unwrap()];
  let compiled = compile_codex_flow_task_with_options(
    &intent,
    &admitted,
    &installed.instructions,
    &installed.credentials,
    installed.consumer,
    &installed.usage_policy,
    FactoryCodexFlowCompilationOptions {
      role: Some(&role),
      context: Some(&context),
      mcp_configurations: Some(&configurations),
      ..Default::default()
    },
  )
  .unwrap();
  let prompt = std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).unwrap();
  for required in [
    "assessment.duplicate",
    "claims",
    "read_issue",
    "/issue",
    "search_issues",
    "/coverage/has_more",
    "json_text",
    "value_json",
  ] {
    assert!(
      prompt.contains(required),
      "the compiled Project invocation must explain {required}"
    );
  }
  let model_schema: Value =
    serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::ResultSchema).bytes()).unwrap();
  assert_eq!(model_schema["properties"]["claims"]["maxItems"], 2);
  assert_eq!(
    model_schema["properties"]["assessment"]["properties"]["duplicate"]["type"],
    "boolean"
  );
  assert!(model_schema["properties"].get("verification").is_none());
  let example: Value = serde_json::from_str(
    prompt
      .lines()
      .find_map(|line| line.strip_prefix("Example result: "))
      .expect("the executed seed must include a complete model-facing result example"),
  )
  .unwrap();
  assert!(example.get("evidence").is_none());
  assert!(example.get("verification").is_none());
  let (observed, entity) = native_issue_fixture();
  let capture = observed.capture();
  let bound = capture
    .bind_mcp_connections(&observed.role, &observed.compiled, &observed.configurations)
    .unwrap();
  let result = bound
    .verify_conclusion(example)
    .expect("the prompt example must cite independently specified native Issue observations");
  assert_eq!(
    serde_json::from_str::<Value>(result["claims"][0]["value_json"].as_str().unwrap()).unwrap(),
    entity
  );
  let result = FlowPayload::new(schema, result).unwrap();
  let input = FlowPayload::new(
    &schemas[&key("eligibility_gate_input")],
    owner_input(&observed, result.value().clone()),
  )
  .unwrap();
  let gate = template
    .nodes
    .iter()
    .find(|node| node.key.as_str() == "eligibility_gate")
    .unwrap()
    .gate
    .as_ref()
    .unwrap();
  assert_eq!(
    gate
      .select_at(&input, Timestamp::from_unix_millis(1012).unwrap())
      .unwrap()
      .as_str(),
    "duplicate"
  );
}

#[test]
fn triage_eligibility_failed_malformed_and_cancelled_calls_cannot_authorize_rejection() {
  let template = template();
  let schemas = template.data_schemas().unwrap();
  for case in ["failed", "malformed", "cancelled"] {
    let mut f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
    let mut rows = std::str::from_utf8(&f.native.documents[1].bytes)
      .unwrap()
      .lines()
      .map(|line| serde_json::from_str::<Value>(line).unwrap())
      .collect::<Vec<_>>();
    match case {
      "failed" => {
        rows[1]["event"]["item"]["status"] = json!("failed");
        rows[1]["event"]["item"]["error"] = json!("unavailable");
      }
      "malformed" => rows[1]["event"]["item"]["result"]["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"text","text":false})),
      _ => rows.truncate(1),
    }
    let trace = rows
      .iter()
      .flat_map(|row| {
        let mut bytes = serde_json::to_vec(row).unwrap();
        bytes.push(b'\n');
        bytes
      })
      .collect::<Vec<_>>();
    let mut provenance: Value = serde_json::from_slice(&f.native.documents[2].bytes).unwrap();
    provenance["capture"]["trace_digest"]["value"] = json!(FactoryDigest::content_blake3(&trace));
    provenance["capture"]["record_count"] = json!(rows.len());
    if case == "cancelled" {
      provenance["capture"]["status"] = json!("cancelled");
      provenance["capture"]["terminal_observed"] = json!(false);
      provenance["outcome"] = json!("failed");
    }
    f.native.replace_document_bytes("codex-run-trace", trace);
    f.native.replace_document("codex-run-provenance", provenance);
    let capture = f.capture();
    let bound = capture
      .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
      .unwrap();
    if case == "cancelled" {
      assert!(bound.verify_conclusion(conclusion()).is_err());
      continue;
    }
    let result = bound.verify_conclusion(conclusion()).unwrap();
    assert_eq!(result["verification"]["status"], "needs_review");
    let payload = FlowPayload::new(&schemas[&key("eligibility_gate_input")], owner_input(&f, result)).unwrap();
    assert_eq!(
      reviewed_gate(&template)
        .select_at(&payload, Timestamp::from_unix_millis(1012).unwrap())
        .unwrap()
        .as_str(),
      "review"
    );
  }
}

#[test]
fn triage_eligibility_committed_results_prepare_the_gate_and_only_the_declared_successor() {
  // The capture verifier is tested above; this owner fixture exercises retained
  // records, metadata-only provenance projections and actual interpreter progress.
  for (decision, fit, uncertainty, duplicate) in [
    ("duplicate", "fits", "clear", true),
    ("out_of_scope", "out_of_scope", "clear", false),
    ("needs_input", "fits", "needs_input", false),
    ("review", "inconclusive", "clear", false),
    ("eligible", "fits", "clear", false),
  ] {
    let f = fixture(r#"{"candidate":{"id":"ISSUE-42","version":3},"coverage":{"has_more":false}}"#);
    let capture = f.capture();
    let bound = capture
      .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
      .unwrap();
    let mut model = conclusion();
    model["assessment"]["fit"] = json!(fit);
    model["assessment"]["uncertainty"] = json!(uncertainty);
    if !duplicate {
      model["assessment"]["duplicate"] = json!(false);
    }
    let value = bound.verify_conclusion(model).unwrap();
    let mut template = template();
    let policy = reviewed_gate(&template);
    template
      .nodes
      .iter_mut()
      .find(|node| node.key.as_str() == "eligibility_gate")
      .unwrap()
      .gate = Some(policy);
    let work = fixtures::work();
    let profiles = template
      .nodes
      .iter()
      .filter_map(|node| node.build_profile.as_ref())
      .map(|name| {
        let mut build = fixtures::binding(work.subject().project_id());
        build.result_outcome = key("observed");
        build.result_output = key("codex-run-result");
        if name.as_str() == "eligibility" {
          build.result_tool = Some(factory_mcp_evidence_verifier());
        }
        (name.clone(), build)
      })
      .collect();
    let published = template
      .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
      .unwrap();
    let definition = published.definition;
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    let flow = FlowRun::root(&run, definition.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let admitted = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
      FlowAdmissionLimits::new(
        1,
        4,
        1,
        7,
        1,
        template.execution.budget(),
        FactoryPermissionSet::deny_all(),
      )
      .unwrap(),
      flow,
      cycle,
      FactoryWipLimits::new(1, 1).unwrap(),
    )
    .unwrap()
    .with_data_schemas(published.schemas)
    .unwrap();
    let at = |value| Timestamp::from_unix_millis(value).unwrap();
    let ownership = FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([72; 32])),
        at(1000),
        at(2000),
      )
      .unwrap(),
    );
    let mut attempts = vec![];
    let mut completions = vec![];
    let mut records = vec![];
    let mut inputs = vec![];
    for (index, name) in ["eligibility", "eligibility_gate"].into_iter().enumerate() {
      let history = FlowRuntimeHistory {
        stops: &[],
        flow_runs: std::slice::from_ref(admitted.root_run()),
        cycles: std::slice::from_ref(admitted.initial_cycle()),
        attempts: &attempts,
        completions: &completions,
      };
      let node = definition.node(&key(name)).unwrap();
      let mut input = FlowNodeInput::prepare(FlowInputPreparation {
        work: &work,
        admitted: &admitted,
        flow: admitted.root_run(),
        cycle: admitted.initial_cycle(),
        node: node.key().clone(),
        schema: admitted.data_schema(node.input_schema().unwrap()).unwrap(),
        history,
        records: &records,
        inputs: &inputs,
        incoming: None,
      })
      .unwrap();
      if index == 0 {
        let subject = FactoryTaskSubject::Exact(work.subject().clone());
        let artifact = FactoryArtifactReference::new(
          ArtifactId::generate(),
          FactoryDigest::content_sha256(b"private Project goals"),
          21,
        )
        .unwrap();
        let entry = ContextManifestEntry::new(
          ContextSourceKind::Specification,
          key("private-project-goals"),
          subject.clone(),
          FactoryContextReference::Artifact(artifact.clone()),
          artifact.content_digest(),
          artifact.encoded_size(),
          FactorySafeText::new("Reviewed private goals").unwrap(),
          FactoryDigest::from_bytes([73; 32]),
        )
        .unwrap();
        input = input
          .with_context(
            ContextManifest::new(
              ContextManifestId::generate(),
              subject,
              FactoryDigest::from_bytes([74; 32]),
              vec![entry],
            )
            .unwrap(),
            vec![],
          )
          .unwrap();
      }
      let attempt = NodeAttempt::new(
        admitted.root_run(),
        admitted.initial_cycle(),
        &definition,
        NodeAttemptInput {
          id: NodeAttemptId::generate(),
          node_key: node.key().clone(),
          node_kind: node.kind(),
          number: NodeAttemptNumber::new((index + 1) as u64).unwrap(),
          input_digest: input.digest().unwrap(),
          budget: node.budget(),
          started_at: at(1010 + index as i64 * 3),
          deadline: at(1100),
          execution: node.build().map_or(NodeExecutionIdentity::BuiltIn, |build| {
            NodeExecutionIdentity::External(build.tool.clone())
          }),
          ownership: ownership.clone(),
        },
      )
      .unwrap();
      let record = if index == 0 {
        let output = admitted
          .data_schema(node.outcome(&key("observed")).unwrap().schema())
          .unwrap();
        let intent =
          FlowBuildIntent::new(input.clone(), &admitted, attempt.clone(), BudgetUsage::default(), 1).unwrap();
        let identity = &f.native.documents[0].identity;
        FlowNodeRecord::new(
          &input,
          &attempt,
          &definition,
          FlowRecordObservation {
            outcome: key("observed"),
            payload: FlowPayload::new(output, value.clone()).unwrap(),
            producer: attempt.execution().clone(),
            observed_at: at(1011),
          },
        )
        .unwrap()
        .with_build_provenance(
          FlowBuildProvenance::new(
            intent.profile().clone(),
            vec![FlowVerifiedBuildOutput {
              name: key("codex-run-result"),
              output_kind: EvidenceOutputKind::Report,
              schema: output.reference().clone(),
              artifact: FactoryArtifactReference::new(
                identity.artifact_id,
                FactoryDigest::from_bytes(identity.digest.as_bytes()),
                identity.size_bytes,
              )
              .unwrap(),
              producer: EvidenceProducer::new(
                identity.build_id,
                identity.attempt_id,
                identity.job_id,
                factory_mcp_evidence_verifier(),
                intent.profile().plugin.clone(),
              ),
              verified_at: at(1011),
              fresh_until: at(1020),
              data: Some(FlowVerifiedBuildData::new(output.clone(), value.clone()).unwrap()),
            }],
          )
          .unwrap(),
          &definition,
        )
        .unwrap()
      } else {
        assert_eq!(input.payload().value()["conclusion"], value);
        assert_eq!(
          input.payload().value()["producer"]["tool"],
          json!(factory_mcp_evidence_verifier())
        );
        assert_eq!(
          input.sources().len(),
          1,
          "all three views retain one exact predecessor record"
        );
        FlowGateProgram::from_node(node)
          .unwrap()
          .record(
            &input,
            &attempt,
            &definition,
            admitted.data_schema(node.outcomes()[0].schema()).unwrap(),
            at(1014),
          )
          .unwrap()
      };
      completions.push(
        record
          .completion(
            &attempt,
            &definition,
            ownership.clone(),
            BudgetUsage::default(),
            at(1012 + index as i64 * 3),
          )
          .unwrap(),
      );
      inputs.push(input);
      attempts.push(attempt);
      records.push(record);
    }
    let history = FlowRuntimeHistory {
      stops: &[],
      flow_runs: std::slice::from_ref(admitted.root_run()),
      cycles: std::slice::from_ref(admitted.initial_cycle()),
      attempts: &attempts,
      completions: &completions,
    };
    let validated = admitted.validated().unwrap();
    let progress = FlowInterpreter::new(&validated)
      .progress(admitted.root_run(), admitted.initial_cycle(), history)
      .unwrap();
    let accepted = records.last().unwrap().payload().value();
    assert_eq!(accepted["decision"], decision);
    assert_eq!(accepted["accepted"], decision == "eligible");
    assert!(accepted.get("claims").is_none());
    assert!(accepted.get("verification").is_none());
    if decision == "eligible" {
      assert_eq!(progress, FlowProgress::Node(key("classification")));
      let analyst = FlowNodeInput::prepare(FlowInputPreparation {
        work: &work,
        admitted: &admitted,
        flow: admitted.root_run(),
        cycle: admitted.initial_cycle(),
        node: key("classification"),
        schema: admitted
          .data_schema(definition.node(&key("classification")).unwrap().input_schema().unwrap())
          .unwrap(),
        history,
        records: &records,
        inputs: &inputs,
        incoming: None,
      })
      .unwrap();
      assert_eq!(analyst.payload().value()["eligibility"], *accepted);
      assert_eq!(analyst.payload().value()["task"], json!(work.artifacts().task()));
      assert_eq!(
        analyst.payload().value()["completion"],
        json!(work.artifacts().acceptance())
      );
      assert!(analyst.context().is_none());
    } else {
      assert!(
        matches!(progress, FlowProgress::Terminal { .. }),
        "{decision} cannot dispatch the analyst"
      );
    }
  }
}
