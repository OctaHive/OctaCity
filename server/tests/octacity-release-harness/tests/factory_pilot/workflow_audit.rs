//! Paged public immutable audit and provenance continuity for a completed Run.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct AuditContinuity {
  audit: Vec<Value>,
  flow_runs: Vec<Value>,
}

impl AuditContinuity {
  pub(super) fn assert_retained_by(&self, after: &Self) {
    assert_eq!(self.flow_runs, after.flow_runs);
    // Input retention continues after completion. Its new audit facts must not
    // remove or change any previously committed fact or frozen Flow reference.
    let retained = after
      .audit
      .iter()
      .map(|fact| (fact["id"].as_str().unwrap(), fact))
      .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(retained.len(), after.audit.len(), "audit identities remain unique");
    for fact in &self.audit {
      let id = fact["id"].as_str().unwrap();
      assert_eq!(
        retained.get(id).copied(),
        Some(fact),
        "original audit fact {id} is retained exactly"
      );
    }
  }
}

#[test]
fn completed_run_audit_preserves_facts_when_retained_input_cleanup_appends() {
  let run = Uuid::from_u128(1);
  let fact = |id: u8, operation: &str| {
    let fact: octacity_server_application::FactoryAuditProjection = serde_json::from_value(json!({
      "id":hex::encode([id;32]),"run_id":run,"actor_kind":"worker","actor_identity_digest":null,
      "operation":operation,"request_identity_digest":hex::encode([id+1;32]),
      "outcome":"accepted","recorded_at":31,
    }))
    .unwrap();
    serde_json::to_value(fact).unwrap()
  };
  let flow: octacity_server_factory::FlowRun = serde_json::from_value(json!({
    "id":run,"factory_run_id":run,"parent":null,"definition":{
      "id":Uuid::from_u128(2),"version":1,"digest":hex::encode([3;32]),
    },
  }))
  .unwrap();
  let before = AuditContinuity {
    audit: vec![fact(1, "work.admitted"), fact(2, "factory.node.advance")],
    flow_runs: vec![serde_json::to_value(flow).unwrap()],
  };
  let mut after = before.clone();
  after.audit.insert(0, fact(3, "factory.execution_input_deleted"));
  before.assert_retained_by(&after);

  let mut missing = after.clone();
  missing.audit.remove(1);
  assert!(std::panic::catch_unwind(|| before.assert_retained_by(&missing)).is_err());
  let mut changed = after.clone();
  changed.audit[1]["recorded_at"] = json!(32);
  assert!(std::panic::catch_unwind(|| before.assert_retained_by(&changed)).is_err());
  let mut duplicated = after.clone();
  duplicated.audit.push(duplicated.audit[1].clone());
  assert!(std::panic::catch_unwind(|| before.assert_retained_by(&duplicated)).is_err());
  let mut replaced_flow = after;
  replaced_flow.flow_runs[0]["definition"]["version"] = json!(2);
  assert!(std::panic::catch_unwind(|| before.assert_retained_by(&replaced_flow)).is_err());
}

// Exact public DTOs; no SQL, private audit preimage reconstruction or new API.
pub(super) async fn capture(
  client: &Client,
  origin: &str,
  run: &str,
  published_flow: &Value,
  rows: &[(Vec<u8>, Vec<u8>)],
) -> AuditContinuity {
  let audit = collection(client, origin, run, "audit").await;
  let flow_runs = collection(client, origin, run, "flow-runs").await;
  let attempts = collection(client, origin, run, "node-attempts").await;
  let builds = collection(client, origin, run, "builds").await;
  let completions = collection(client, origin, run, "node-completions").await;
  assert_eq!(
    audit
      .iter()
      .filter(|row| row["operation"] == "work.admitted" && row["outcome"] == "accepted")
      .count(),
    1
  );
  assert!(
    audit
      .iter()
      .any(|row| row["operation"] == "factory.node.advance" && row["outcome"] == "accepted")
  );
  for fact in &audit {
    assert_eq!(fact["run_id"], run);
    assert!(
      fact["request_identity_digest"]
        .as_str()
        .is_some_and(|value| value.len() == 64)
    );
  }
  let roots = flow_runs
    .iter()
    .filter(|row| row["parent"].is_null())
    .collect::<Vec<_>>();
  assert_eq!(roots.len(), 1);
  assert_eq!(roots[0]["factory_run_id"], run);
  assert_eq!(roots[0]["definition"], published_flow["closure"]["root"]);
  for flow in &flow_runs {
    assert_eq!(flow["factory_run_id"], run);
    if !flow["parent"].is_null() {
      let parent = attempts
        .iter()
        .find(|attempt| attempt["id"] == flow["parent"]["node_attempt_id"])
        .unwrap();
      assert_eq!(parent["flow_run_id"], flow["parent"]["flow_run_id"]);
    }
  }
  for build in &builds {
    assert_eq!(
      audit
        .iter()
        .filter(|fact| fact["operation"] == "factory.build.accepted"
          && fact["outcome"] == "accepted"
          && fact["request_identity_digest"] == build["operation_id"])
        .count(),
      1
    );
    assert!(attempts.iter().any(|attempt| attempt["id"] == build["node_attempt_id"]));
  }
  for (digest, bytes) in rows {
    let record: Value = serde_json::from_slice(bytes).unwrap();
    let input = &record["input"];
    assert_eq!(input["factory_run_id"], run);
    assert_eq!(input["root_definition"], published_flow["closure"]["root"]);
    let flow = flow_runs
      .iter()
      .find(|flow| flow["id"] == input["flow_run_id"])
      .unwrap();
    assert_eq!(flow["definition"], input["definition"]);
    let attempt = attempts
      .iter()
      .find(|attempt| attempt["id"] == record["node_attempt_id"])
      .unwrap();
    assert_eq!(attempt["factory_run_id"], run);
    assert_eq!(attempt["flow_run_id"], input["flow_run_id"]);
    assert_eq!(attempt["cycle_id"], input["cycle_id"]);
    assert_eq!(attempt["node_key"], input["node"]);
    let completion = completions
      .iter()
      .find(|row| row["node_attempt_id"] == record["node_attempt_id"])
      .unwrap();
    assert_eq!(completion["flow_run_id"], input["flow_run_id"]);
    assert_eq!(completion["cycle_id"], input["cycle_id"]);
    assert_eq!(completion["node_key"], input["node"]);
    assert_eq!(completion["outcome"], record["outcome"]);
    assert_eq!(completion["output_schema"], record["payload"]["schema"]);
    assert_eq!(completion["output_digest"], hex::encode(digest));
    if record["discovery"]["kind"] == "candidate" {
      assert_eq!(record["discovery"]["subject"]["exact"], input["subject"]);
      assert!(record["discovery"]["subject"]["candidate_revision"].is_string());
      assert!(
        record["discovery"]["subject"]["changeset_digest"]
          .as_str()
          .is_some_and(|digest| digest.len() == 64)
      );
    }
  }
  AuditContinuity { audit, flow_runs }
}

async fn collection(client: &Client, origin: &str, run: &str, kind: &str) -> Vec<Value> {
  let mut after = None::<String>;
  let mut result = Vec::new();
  let mut identities = std::collections::BTreeSet::new();
  for _ in 0..64 {
    let mut request = client
      .get(format!("{origin}/api/v1/factory-runs/{run}/{kind}"))
      .query(&[("limit", "100")]);
    if let Some(cursor) = &after {
      request = request.query(&[("after", cursor)]);
    }
    let page: Value = request
      .send()
      .await
      .unwrap()
      .error_for_status()
      .unwrap()
      .json()
      .await
      .unwrap();
    for entry in page["items"].as_array().unwrap() {
      let row = &entry["record"];
      assert!(
        identities.insert(row["id"].as_str().unwrap().to_owned()),
        "paged immutable identities never repeat"
      );
      result.push(row.clone());
    }
    match page["next_cursor"].as_str() {
      None => return result,
      Some(next) => {
        assert_ne!(after.as_deref(), Some(next), "opaque continuation must advance");
        after = Some(next.to_owned());
      }
    }
  }
  panic!("completed pilot audit exceeds the explicit 6400-row fixture bound");
}
