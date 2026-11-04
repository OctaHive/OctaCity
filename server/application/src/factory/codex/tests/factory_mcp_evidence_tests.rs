//! Evidence consumers use the same verified Artifact/native capture interface.
use super::*;
use crate::{FactoryMcpCallStatus, verify_codex_native_capture};

#[test]
fn native_mcp_capture_exposes_the_actual_query_result_and_exact_artifact_observation() {
  let fixture = native_capture_fixture("candidate evidence");
  let (_, profile, _) = node_observation_fixture();
  let capture = verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).unwrap();
  let calls = capture.mcp_calls();
  assert_eq!(calls.len(), 1);
  let call = &calls[0];
  assert_eq!(call.id(), "call-1");
  assert_eq!(call.connection(), "issues");
  assert_eq!(call.tool(), "lookup");
  assert_eq!(call.arguments(), &json!({"query":"candidate"}));
  assert_eq!(call.status(), FactoryMcpCallStatus::Completed);
  assert_eq!(call.started_sequence(), 0);
  assert_eq!(call.completed_sequence(), Some(1));
  assert_eq!(call.observed_at(), Timestamp::from_unix_millis(1005).unwrap());
  assert_eq!(call.trace(), &fixture.documents[1].identity);
  assert_eq!(
    call.result(),
    Some(&json!({"content":[{"type":"text","text":"candidate evidence"}],"structured_content":null}))
  );
  assert_eq!(
    call.result_digest(),
    Some(FactoryDigest::content_sha256(
      br#"{"content":[{"text":"candidate evidence","type":"text"}],"structured_content":null}"#
    ))
  );
}

fn trace_records(fixture: &Fixture) -> Vec<Value> {
  std::str::from_utf8(&fixture.documents[1].bytes)
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect()
}
fn replace_trace(fixture: &mut Fixture, records: &[Value], status: &str) {
  let trace = records
    .iter()
    .flat_map(|record| {
      let mut line = serde_json::to_vec(record).unwrap();
      line.push(b'\n');
      line
    })
    .collect::<Vec<_>>();
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["capture"]["record_count"] = json!(records.len());
  provenance["capture"]["status"] = json!(status);
  provenance["capture"]["terminal_observed"] =
    json!(records.iter().any(|row| row["event"]["type"] == "turn.completed"));
  provenance["capture"]["trace_digest"]["value"] = json!(FactoryDigest::content_blake3(&trace).to_string());
  fixture.replace_document_bytes("codex-run-trace", trace);
  fixture.replace_document("codex-run-provenance", provenance);
}

#[test]
fn native_mcp_capture_distinguishes_a_malformed_consumed_result_from_a_successful_empty_response() {
  let (_, profile, _) = node_observation_fixture();
  let mut fixture = native_capture_fixture("candidate evidence");
  let mut records = trace_records(&fixture);
  records[1]["event"]["item"]["result"] = json!({"content":"malformed MCP content"});
  replace_trace(&mut fixture, &records, "complete");
  let capture = verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).unwrap();
  assert_eq!(capture.mcp_calls()[0].status(), FactoryMcpCallStatus::Malformed);
  assert_eq!(
    capture.mcp_calls()[0].result(),
    Some(&json!({"content":"malformed MCP content"})),
    "malformed observed bytes stay inspectable without becoming empty success"
  );
}

#[test]
fn native_mcp_capture_preserves_empty_failed_and_interrupted_observations_without_inventing_results() {
  let (_, profile, _) = node_observation_fixture();
  for (name, result, native_status, expected) in [
    (
      "empty",
      json!({"content":[]}),
      "completed",
      FactoryMcpCallStatus::Completed,
    ),
    (
      "refused",
      json!({"content":[{"type":"text","text":"not_available"}],"isError":true}),
      "completed",
      FactoryMcpCallStatus::Failed,
    ),
    ("unavailable", Value::Null, "failed", FactoryMcpCallStatus::Failed),
    ("missing", Value::Null, "completed", FactoryMcpCallStatus::Malformed),
    (
      "interrupted",
      Value::Null,
      "in_progress",
      FactoryMcpCallStatus::Interrupted,
    ),
  ] {
    let mut fixture = native_capture_fixture("original");
    let mut records = trace_records(&fixture);
    records[1]["event"]["item"]["result"] = result.clone();
    records[1]["event"]["item"]["status"] = json!(native_status);
    let capture_status = if name == "interrupted" {
      records.truncate(1);
      "cancelled"
    } else {
      "complete"
    };
    replace_trace(&mut fixture, &records, capture_status);
    let capture = verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).unwrap();
    assert_eq!(capture.mcp_calls()[0].status(), expected, "{name}");
    assert_eq!(
      capture.mcp_calls()[0].result(),
      (!result.is_null()).then_some(&result),
      "{name}"
    );
  }
}

#[test]
fn native_mcp_capture_rejects_mispaired_calls_and_never_silently_truncates_the_call_budget() {
  let (_, profile, _) = node_observation_fixture();
  for mutation in [
    "missing_start",
    "tool",
    "connection",
    "arguments",
    "duplicate_id",
    "duplicate_completion",
    "too_many",
  ] {
    let mut fixture = native_capture_fixture("facts");
    let mut records = trace_records(&fixture);
    match mutation {
      "missing_start" => {
        records.remove(0);
        records[0]["sequence"] = json!(0);
        records[1]["sequence"] = json!(1);
      }
      "tool" => records[1]["event"]["item"]["tool"] = json!("write"),
      "connection" => records[1]["event"]["item"]["server"] = json!("ambient"),
      "arguments" => records[1]["event"]["item"]["arguments"] = json!({"query":"changed"}),
      "duplicate_id" | "duplicate_completion" => {
        let extra = records[usize::from(mutation == "duplicate_completion")].clone();
        records.insert(2, extra);
        for (index, row) in records.iter_mut().enumerate() {
          row["sequence"] = json!(index);
          row["observed_at_unix_millis"] = json!(1005);
        }
      }
      _ => {
        let original = records.clone();
        records.clear();
        for index in 0..65 {
          for original in &original[..2] {
            let mut row = original.clone();
            row["sequence"] = json!(records.len());
            row["observed_at_unix_millis"] = json!(1005);
            row["event"]["item"]["id"] = json!(format!("call-{index}"));
            records.push(row);
          }
        }
        let mut terminal = original[2].clone();
        terminal["sequence"] = json!(130);
        records.push(terminal);
      }
    }
    replace_trace(&mut fixture, &records, "complete");
    assert!(
      verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).is_err(),
      "{mutation}"
    );
  }
}

#[test]
fn native_mcp_evidence_references_resolve_only_the_exact_retained_call_and_content() {
  let fixture = native_capture_fixture("candidate evidence");
  let (_, profile, _) = node_observation_fixture();
  let capture = verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).unwrap();
  let reference = capture.mcp_calls()[0].reference();
  assert_eq!(capture.resolve_evidence(&reference).unwrap().id(), "call-1");
  let wire = serde_json::to_value(&reference).unwrap();
  assert_eq!(wire["sequence"], 1);
  for mutation in ["artifact", "trace_digest", "sequence", "call", "result"] {
    let mut forged = wire.clone();
    match mutation {
      "artifact" => forged["trace"]["artifact_id"] = json!(ArtifactId::generate()),
      "trace_digest" => forged["trace"]["content_digest"] = json!("ff".repeat(32)),
      "sequence" => forged["sequence"] = json!(0),
      "call" => forged["call_id"] = json!("invented-call"),
      _ => forged["result_digest"] = json!("ee".repeat(32)),
    }
    let reference = serde_json::from_value(forged).unwrap();
    assert!(capture.resolve_evidence(&reference).is_err(), "{mutation}");
  }
}

#[test]
fn native_mcp_capture_requires_observed_arguments_and_preserves_explicit_null() {
  let (_, profile, _) = node_observation_fixture();
  let mut fixture = native_capture_fixture("facts");
  let mut records = trace_records(&fixture);
  for row in &mut records[..2] {
    row["event"]["item"]["arguments"] = Value::Null;
  }
  replace_trace(&mut fixture, &records, "complete");
  let capture = verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).unwrap();
  assert_eq!(capture.mcp_calls()[0].arguments(), &Value::Null);
  for omitted in [&[0, 1][..], &[0][..], &[1][..]] {
    let mut incomplete = records.clone();
    for index in omitted {
      incomplete[*index]["event"]["item"]
        .as_object_mut()
        .unwrap()
        .remove("arguments");
    }
    replace_trace(&mut fixture, &incomplete, "complete");
    assert!(
      verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).is_err(),
      "missing arguments at records {omitted:?} must not become observed null"
    );
  }
}
