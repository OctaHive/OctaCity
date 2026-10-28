//! The ordinary output owner verifies native evidence before accepting stage data.
use super::*;
use serde_json::{Value, json};

pub(super) fn conclusion_admission(
  work: &WorkEnvelope,
  admitted: AdmittedFlow,
  old_schema: FlowDataSchema,
  old_input: FlowNodeInput,
) -> (AdmittedFlow, FlowDataSchema, FlowNodeInput) {
  let schema = FlowDataSchema::new(
    fixtures::key("evidence-conclusion"),
    fixtures::key("v1"),
    factory_mcp_conclusion_shape(old_schema.shape().clone()),
  )
  .unwrap();
  let old = admitted.closure().definitions()[0].clone();
  let mut node = serde_json::to_value(&old.nodes()[0]).unwrap();
  for outcome in node["outcomes"].as_array_mut().unwrap() {
    outcome["schema"] = json!(schema.reference());
  }
  let terminals = old
    .terminals()
    .iter()
    .map(|terminal| {
      let mut wire = serde_json::to_value(terminal).unwrap();
      wire["schema"] = json!(schema.reference());
      serde_json::from_value(wire).unwrap()
    })
    .collect();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: old.reference().id(),
    version: old.reference().version(),
    input_schema: old.input_schema().cloned(),
    entry: old.entry().clone(),
    nodes: vec![serde_json::from_value(node).unwrap()],
    transitions: old.transitions().to_vec(),
    terminals,
    context_projections: old.context_projections().to_vec(),
    data_projections: old.data_projections().to_vec(),
    execution: old.execution().clone(),
  })
  .unwrap();
  let run = FactoryRun::admitted(old_input.factory_run_id(), work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    admitted.limits().clone(),
    flow,
    cycle,
    admitted.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(vec![old_schema, schema.clone()])
  .unwrap();
  let mut input = FlowNodeInput::new(
    work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    old_input.node().clone(),
    old_input.payload().clone(),
  )
  .unwrap();
  if let Some(context) = old_input.context() {
    input = input.with_context(context.clone(), vec![]).unwrap();
  }
  (admitted, schema, input)
}

pub(super) fn native_records(
  trace_id: ArtifactId,
  result: Vec<u8>,
  provenance: Vec<u8>,
  started: i64,
  case: Option<&str>,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
  let mut records = Vec::new();
  let mut last_result = Value::Null;
  for (index, arguments, content) in [
    (
      0,
      json!({"query":"auth"}),
      json!({"items":[{"id":"ISSUE-42"}],"coverage":{"has_more":true}}),
    ),
    (
      1,
      json!({"query":"auth cancellation"}),
      json!({"items":[{"id":"ISSUE-42"}],"coverage":{"has_more":false}}),
    ),
    (
      2,
      json!({"id":"ISSUE-42"}),
      json!({"id":"ISSUE-42","version":3,"scope":"project-alpha","observed_at":started+3,"content_digest":"a".repeat(64)}),
    ),
  ] {
    let mut item = json!({"id":format!("call-{index}"),"type":"mcp_tool_call","server":"reader","tool":"lookup","arguments":arguments,"status":"in_progress","result":null,"error":null});
    records.push(json!({"format_version":2,"sequence":records.len(),"observed_at_unix_millis":started+index,"event":{"type":"item.started","item":item}}));
    last_result = json!({"content":[{"type":"text","text":content.to_string()}],"structured_content":null});
    item["status"] = json!("completed");
    item["result"] = last_result.clone();
    records.push(json!({"format_version":2,"sequence":records.len(),"observed_at_unix_millis":started+index+1,"event":{"type":"item.completed","item":item}}));
  }
  records.push(
    json!({"format_version":2,"sequence":6,"observed_at_unix_millis":started+4,"event":{"type":"turn.completed"}}),
  );
  match case {
    Some("failed") => {
      records[5]["event"]["item"]["status"] = json!("failed");
      records[5]["event"]["item"]["error"] = json!("tool failed after returning partial content");
    }
    Some("unavailable") => {
      last_result = Value::Null;
      records[5]["event"]["item"]["status"] = json!("failed");
      records[5]["event"]["item"]["error"] = json!("HTTP 503: lookup unavailable");
      records[5]["event"]["item"]["result"] = Value::Null;
    }
    Some("malformed") => {
      last_result["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"text","text":false}));
      records[5]["event"]["item"]["result"] = last_result.clone();
    }
    Some("empty") => {
      last_result = json!({"content":[]});
      records[5]["event"]["item"]["result"] = last_result.clone();
    }
    Some("truncated") => {
      last_result = json!({"content":[{"type":"text","text":"{\"id\":\"ISSUE-42\",\"truncated\":true}"}],"_meta":{"truncated":true}});
      records[5]["event"]["item"]["result"] = last_result.clone();
    }
    Some("cancelled") => {
      records.truncate(5);
      last_result = Value::Null;
    }
    _ => {}
  }
  let trace = records
    .iter()
    .flat_map(|row| {
      let mut bytes = serde_json::to_vec(row).unwrap();
      bytes.push(b'\n');
      bytes
    })
    .collect::<Vec<_>>();
  let reference = FactoryMcpEvidenceReference {
    trace: FactoryArtifactReference::new(trace_id, FactoryDigest::content_sha256(&trace), trace.len() as u64).unwrap(),
    sequence: if case == Some("cancelled") { 4 } else { 5 },
    call_id: "call-2".into(),
    result_digest: (!last_result.is_null())
      .then(|| FactoryDigest::content_sha256(&serde_json::to_vec(&last_result).unwrap())),
  };
  let mut result: Value = serde_json::from_slice(&result).unwrap();
  let assessment = result["structured_result"].take();
  result["structured_result"] = json!({"assessment":assessment,"claims":[{"connection":"reader","tool":"lookup","arguments_json":"{\"id\":\"ISSUE-42\"}","pointer":"/id","json_text":0,"value_json":"\"ISSUE-42\""}]});
  match case {
    Some("forged_reference") => {
      result["structured_result"]["evidence"] = json!([reference]);
      result["structured_result"]["evidence"][0]["sequence"] = json!(999);
    }
    Some("forged_candidate") => result["structured_result"]["claims"][0]["value_json"] = json!("\"HIDDEN-123\""),
    Some("no_claims") => result["structured_result"]["claims"] = json!([]),
    _ => {}
  }
  let mut provenance: Value = serde_json::from_slice(&provenance).unwrap();
  provenance["format_version"] = json!(2);
  provenance["trace_format_version"] = json!(2);
  let cancelled = case == Some("cancelled");
  provenance["capture"] = json!({"status":if cancelled {"cancelled"} else {"complete"},"coverage":"emitted_native_events","record_count":records.len(),"terminal_observed":!cancelled,"trace_digest":{"algorithm":"blake3","value":FactoryDigest::content_blake3(&trace)}});
  if cancelled {
    provenance["outcome"] = json!("failed");
  }
  (
    serde_json::to_vec(&result).unwrap(),
    trace,
    serde_json::to_vec(&provenance).unwrap(),
  )
}

#[test]
fn ordinary_mcp_stage_never_accepts_forged_failed_cancelled_or_incomplete_lookup_as_proof() {
  crate::factory_admission_tests::run_ready(async {
    for case in [
      "forged_reference",
      "forged_candidate",
      "cancelled",
      "failed",
      "unavailable",
      "malformed",
      "empty",
      "truncated",
      "no_claims",
    ] {
      let f = prepared_role_source_with_mcp_evidence(
        None,
        vec![],
        Some(json!({"native_case":case})),
        true,
        Some(FactoryMcpServer::Http {
          url: "https://reader.example.test/mcp".into(),
          bearer_token_env_var: None,
          enabled_tools: Some(vec!["lookup".into()]),
        }),
        true,
        true,
      )
      .await;
      let observed = f
        .source(f.query.clone())
        .with_runtime_settings_store(f.runs.clone())
        .published_outputs(
          f.query.build.build.id,
          f.query.attempt.id,
          f.intent.budget().max_output_bytes(),
        )
        .await;
      if [
        "forged_reference",
        "forged_candidate",
        "cancelled",
        "unavailable",
        "empty",
      ]
      .contains(&case)
      {
        assert!(observed.is_err(), "{case} cannot produce accepted semantic data");
      } else {
        let FactoryNodeBuildOutputObservation::Verified(outputs) = observed.unwrap() else {
          panic!("inspectable review result: {case}");
        };
        let value = outputs.documents[0].data.as_ref().unwrap().payload().value();
        let expected = if case == "truncated" {
          "observed"
        } else {
          "needs_review"
        };
        assert_eq!(value["verification"]["status"], expected, "{case}");
        assert_eq!(
          value["claims"].as_array().unwrap().len(),
          usize::from(case != "no_claims"),
          "{case}"
        );
        if case != "no_claims" {
          assert_eq!(value["claims"][0]["value_json"], "\"ISSUE-42\"", "{case}");
        }
        let expected_status = match case {
          "failed" => "failed",
          "malformed" => "malformed",
          _ => "completed",
        };
        assert_eq!(
          value["verification"]["observations"][2]["status"], expected_status,
          "{case}"
        );
        assert_eq!(value["verification"]["coverage"], "unknown", "{case}");
        assert_eq!(value["verification"]["global_absence_proven"], false, "{case}");
        assert_eq!(value["verification"]["duplicate_proven"], false, "{case}");
        assert_eq!(
          value["verification"]["observations"].as_array().unwrap().len(),
          3,
          "{case}"
        );
      }
    }
  });
}

#[test]
fn ordinary_mcp_stage_output_retains_iterative_lookup_and_owner_verified_candidate_references() {
  crate::factory_admission_tests::run_ready(async {
    let f = prepared_role_source_with_mcp_evidence(
      None,
      vec![],
      None,
      true,
      Some(FactoryMcpServer::Http {
        url: "https://reader.example.test/mcp".into(),
        bearer_token_env_var: None,
        enabled_tools: Some(vec!["lookup".into()]),
      }),
      true,
      true,
    )
    .await;
    let source = f.source(f.query.clone()).with_runtime_settings_store(f.runs.clone());
    let FactoryNodeBuildOutputObservation::Verified(outputs) = source
      .published_outputs(
        f.query.build.build.id,
        f.query.attempt.id,
        f.intent.budget().max_output_bytes(),
      )
      .await
      .unwrap()
    else {
      panic!("verified stage conclusion");
    };
    let document = &outputs.documents[0];
    assert_eq!(document.tool, mcp_verifier());
    let data = document.data.as_ref().unwrap().payload().value();
    assert_eq!(data["assessment"], json!({"count":9}));
    assert_eq!(data["verification"]["status"], "observed");
    assert_eq!(data["verification"]["observations"].as_array().unwrap().len(), 3);
    assert_eq!(data["claims"][0]["value_json"], "\"ISSUE-42\"");
    assert_eq!(
      data["verification"]["producer"]["job_id"],
      json!(f.query.attempt.jobs[0].id())
    );
    assert_eq!(data["verification"]["global_absence_proven"], false);
  });
}
